use crate::flight::{Flight, Mode};
use crate::indexed::{palette_rgb6, BitmapFont, IndexedImage, LOGICAL_HEIGHT, LOGICAL_WIDTH};
use crate::model_lab::{LabView, ModelLab};
use ffe_content::{AssetKind, VerifiedCache};
use std::io::Cursor;

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
const VIEW_HEIGHT: usize = 158;

pub struct Screen {
    pub pixels: Vec<u32>,
    frame: Vec<u32>,
    cabin: Vec<u32>,
    panel: IndexedImage,
    font: BitmapFont,
    palette: [u32; 128],
    indexed: Box<[u8; 64000]>,
    stars: Vec<[f64; 3]>,
}

impl Screen {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let asset = |id| {
            cache.asset(id).ok_or_else(|| format!(
            "Cache lacks {id}. This build needs the v2 cache; build-data to a NEW path, verify it, then replace your generated data.bin."))
        };
        let panel_id = "base:bitmap/primary/043";
        let record = cache
            .manifest()
            .files
            .iter()
            .find(|record| record.id == panel_id)
            .ok_or("Cache lacks the original console panel.")?;
        let panel = match record.asset {
            AssetKind::IndexedBitmap {
                width: 320,
                height: 43,
                offset_x,
                offset_y,
                masked_zero,
                ..
            } => IndexedImage::new(320, 43, [offset_x, offset_y], masked_zero, asset(panel_id)?)?,
            _ => return Err("Invalid original console panel descriptor.".to_owned()),
        };
        Self::from_assets(
            asset("base:cockpit/cabin0")?,
            asset("base:font/main")?,
            panel,
            palette_rgb6(asset("base:palette/low")?)?,
        )
    }

    pub fn from_assets(
        cabin_png: &[u8],
        font: &[u8],
        panel: IndexedImage,
        palette: [u32; 128],
    ) -> Result<Self, String> {
        let font = BitmapFont::new(font)?;
        let mut decoder = png::Decoder::new(Cursor::new(cabin_png));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| format!("Cabin PNG: {e}"))?;
        if reader.info().width != 320
            || reader.info().height != 158
            || reader.output_buffer_size() > 320 * 158 * 4
        {
            return Err("Expected a bounded 320x158 cabin PNG.".to_owned());
        }
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut output)
            .map_err(|e| format!("Cabin pixels: {e}"))?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => return Err("Cabin PNG did not expand to RGB/RGBA.".to_owned()),
        };
        let cabin = output[..info.buffer_size()]
            .chunks_exact(channels)
            .map(|p| (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]))
            .collect();
        let mut seed = 7u32;
        let mut number = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
        };
        let stars = (0..800)
            .map(|_| [number() * 10000.0, number() * 10000.0, number() * 10000.0])
            .collect();
        Ok(Self {
            pixels: vec![0; WIDTH * HEIGHT],
            frame: vec![0; LOGICAL_WIDTH * LOGICAL_HEIGHT],
            cabin,
            panel,
            font,
            palette,
            indexed: Box::new([0; 64000]),
            stars,
        })
    }

    fn pixel(&mut self, x: i32, y: i32, colour: u8) {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            self.frame[y as usize * 320 + x as usize] = self.palette[usize::from(colour)];
        }
    }

    fn rect(&mut self, x: usize, y: usize, width: usize, height: usize, colour: u8) {
        for row in y..y.saturating_add(height).min(200) {
            for col in x..x.saturating_add(width).min(320) {
                self.frame[row * 320 + col] = self.palette[usize::from(colour)];
            }
        }
    }

    fn line(&mut self, start: (i32, i32), end: (i32, i32), colour: u8) {
        let (mut x, mut y) = start;
        let (end_x, end_y) = end;
        let dx = (end_x - x).abs();
        let dy = -(end_y - y).abs();
        let sx = if x < end_x { 1 } else { -1 };
        let sy = if y < end_y { 1 } else { -1 };
        let mut error = dx + dy;
        for _ in 0..4096 {
            if y < VIEW_HEIGHT as i32 {
                self.pixel(x, y, colour);
            }
            if x == end_x && y == end_y {
                break;
            }
            let twice = 2 * error;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    fn text(&mut self, x: usize, y: usize, message: &str, colour: u8) {
        let mut cursor = x;
        for character in message.chars() {
            // The original font is not Unicode. Full diagnostics also go to stderr.
            let code = if character.is_ascii() && !character.is_ascii_control() {
                character as u8
            } else {
                b'?'
            };
            if let Some(glyph) = self.font.glyph(code) {
                for (col, row) in glyph.pixels() {
                    if cursor + col < 320 && y + row < 200 {
                        self.frame[(y + row) * 320 + cursor + col] =
                            self.palette[usize::from(colour)];
                    }
                }
                cursor += glyph.advance;
            }
            if cursor >= 320 {
                break;
            }
        }
    }

    fn present(&mut self) {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                self.pixels[y * WIDTH + x] = self.frame[(y * 200 / HEIGHT) * 320 + x * 320 / WIDTH];
            }
        }
    }

    pub fn logical_pixels(&self) -> &[u32] {
        &self.frame
    }

    pub fn draw(&mut self, state: &Flight, help: bool, modern: bool, message: &str) {
        self.indexed.fill(0);
        self.panel.blit(&mut self.indexed, 0, 158);
        for (pixel, index) in self.frame.iter_mut().zip(self.indexed.iter()) {
            *pixel = self.palette[usize::from(*index)];
        }
        if state.mode == Mode::Docked {
            self.frame[..320 * 158].copy_from_slice(&self.cabin);
            self.rect(24, 24, 272, 92, 1);
            self.text(36, 30, "RUST DEVELOPMENT STATION", 18);
            self.text(36, 44, "DOCKED - EXPERIMENTAL MARKET", 15);
            self.text(
                36,
                59,
                &format!("FOOD: BUY 10  SELL 9  STOCK {}", state.stock),
                17,
            );
            self.text(36, 73, "B BUY ONE     N SELL ONE", 15);
            self.text(36, 87, "L LAUNCH   F5 SAVE   F9 LOAD", 15);
            self.text(36, 101, "F2 SCENARIOS / RESET", 12);
        } else {
            for index in 0..self.stars.len() {
                if let Some((x, y)) = project(self.stars[index], state, false) {
                    if y < 158 {
                        self.pixel(x, y, 12);
                    }
                }
            }
            let vertices = [
                [-40.0, -40.0, -40.0],
                [40.0, -40.0, -40.0],
                [40.0, 40.0, -40.0],
                [-40.0, 40.0, -40.0],
                [-40.0, -40.0, 40.0],
                [40.0, -40.0, 40.0],
                [40.0, 40.0, 40.0],
                [-40.0, 40.0, 40.0],
            ];
            for (a, b) in [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ] {
                if let (Some(a), Some(b)) = (
                    project(vertices[a], state, true),
                    project(vertices[b], state, true),
                ) {
                    self.line(a, b, 12);
                }
            }
            for (a, b) in [
                ([-14.0, -12.0, -41.0], [14.0, -12.0, -41.0]),
                ([14.0, -12.0, -41.0], [14.0, 12.0, -41.0]),
                ([14.0, 12.0, -41.0], [-14.0, 12.0, -41.0]),
                ([-14.0, 12.0, -41.0], [-14.0, -12.0, -41.0]),
            ] {
                if let (Some(a), Some(b)) = (project(a, state, true), project(b, state, true)) {
                    self.line(a, b, if state.docking_requested { 17 } else { 18 });
                }
            }
            if let Some((x, y)) = project([0.0; 3], state, true) {
                if x > 8 && x < 260 && y > 12 && y < 115 {
                    self.text(x as usize + 5, y as usize, "STATION", 17);
                }
            } else {
                self.text(8, 16, "STATION BEHIND / OFF VIEW", 18);
            }
            self.line((155, 79), (158, 79), 18);
            self.line((162, 79), (165, 79), 18);
            self.line((160, 74), (160, 77), 18);
            self.line((160, 81), (160, 84), 18);
            // Prototype contact on the original scanner artwork, not original scanner math.
            let relative = station_relative(state);
            let scale = state.distance().max(200.0);
            let sx = (153.0 + relative[0] / scale * 38.0) as i32;
            let sy = (188.0 - relative[2] / scale * 6.0) as i32;
            self.pixel(sx, sy, 17);
            self.pixel(sx - 1, sy, 17);
            self.pixel(sx + 1, sy, 17);
        }
        self.rect(0, 0, 320, 11, 0);
        self.text(3, 1, "FFE-RS 0.9 EXPERIMENTAL - SYNTHETIC FLIGHT", 18);
        self.rect(0, 119, 320, 39, 0);
        self.text(
            3,
            120,
            &format!(
                "SPD {:5.1}  RNG {:7.1}  CR {}  CARGO {}/10",
                state.speed(),
                state.distance(),
                state.credits,
                state.cargo
            ),
            15,
        );
        self.text(
            3,
            130,
            &format!(
                "DOCK {}  {}  F1 HELP  F2 RESET  F3 INPUT",
                if state.docking_requested { "ON" } else { "OFF" },
                if modern { "MODERN" } else { "CLASSIC-LIKE" }
            ),
            12,
        );
        let mut lines = Vec::new();
        let mut line = String::new();
        for ch in message.chars() {
            let ch = if ch.is_ascii() && !ch.is_ascii_control() {
                ch
            } else {
                '?'
            };
            line.push(ch);
            if self.font.width(&line).is_ok_and(|width| width >= 308) {
                lines.push(std::mem::take(&mut line));
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        for (index, line) in lines.iter().take(2).enumerate() {
            self.text(3, 139 + index * 9, line, 18);
        }
        if help {
            self.rect(0, 13, 320, 145, 1);
            for (index, line) in [
                "NATIVE RUST PLAYABLE SLICE - STILL SYNTHETIC FLIGHT",
                "Original font + console; not original world/models.",
                "DEFAULT MODERN: W/S thrust; arrows yaw/pitch",
                "CLASSIC-LIKE: Enter/Shift thrust; ,/. yaw; A/Z pitch",
                "X brake   D request dock   L launch   B buy   N sell",
                "Dock: range <80, speed <20, aim at station.",
                "F3 input. F4 station model lab. P pause. Esc quit.",
                "F5 new save snapshot; F9 load newest snapshot.",
                "F2 scenarios: 1 Docked / 2 Approach / 3 Free Flight",
                "Scenarios RESET progress, but keep saved snapshots.",
                "Help/menus/focus loss pause without time catch-up.",
                "No audio/combat/hyperspace/old saves/HD packs yet.",
                "F1 closes help. F2 opens playtest scenarios.",
            ]
            .iter()
            .enumerate()
            {
                self.text(14, 19 + index * 10, line, if index == 0 { 18 } else { 15 });
            }
        }
        self.present();
    }

    pub fn overlay(&mut self, title: &str, lines: &[&str]) {
        self.rect(12, 24, 296, 92, 1);
        self.text(20, 31, title, 18);
        for (index, line) in lines.iter().enumerate() {
            self.text(20, 47 + index * 13, line, 15);
        }
        self.present();
    }

    pub fn draw_model_lab(&mut self, lab: &ModelLab) -> Result<(), String> {
        self.rect(0, 0, 320, 158, 0);
        self.text(3, 1, "STATION MODEL LAB - GAME PAUSED", 18);
        let model = lab.model();
        self.text(
            3,
            12,
            &format!(
                "MODEL {} (0x{:02X})  {} VERTICES  SCALE {}",
                model.index, model.index, model.count, model.scale
            ),
            15,
        );
        let points = lab.points()?;
        let normals = lab.normals()?;
        match (lab.view(), model.program()) {
            (LabView::Points, _) => {
                self.text(
                    3,
                    23,
                    if model.program().is_some() {
                        "POINTS - GREEN = COMPUTED"
                    } else {
                        "POINTS ONLY - CACHE V4 ADDS FACE RECORDS"
                    },
                    12,
                );
                for (x, y, computed) in points {
                    let colour = if computed { 17 } else { 15 };
                    self.pixel(x, y, colour);
                    self.pixel(x + 1, y, colour);
                }
            }
            (view, Some(program)) => {
                self.text(
                    3,
                    23,
                    if view == LabView::Normals {
                        "NORMAL FILTER - NO BRANCH EXECUTION"
                    } else {
                        "DECLARED FACES - NOT A MODEL EXECUTION"
                    },
                    12,
                );
                for (index, face) in program.faces.iter().enumerate() {
                    if view == LabView::Face && lab.selected_face().is_none_or(|(i, _)| i != index)
                    {
                        continue;
                    }
                    if view == LabView::Normals
                        && normals
                            .as_ref()
                            .ok_or("Normal view requires cache v5.")?
                            .get(usize::from(face.normal))
                            .ok_or("Face normal outside imported table.")?
                            .culled
                    {
                        continue;
                    }
                    let colour = if face.reflected { 17 } else { 15 };
                    for edge in 0..face.vertices.len() {
                        let (x1, y1, _) = points[usize::from(face.vertices[edge])];
                        let (x2, y2, _) =
                            points[usize::from(face.vertices[(edge + 1) % face.vertices.len()])];
                        self.line((x1, y1), (x2, y2), colour);
                    }
                }
            }
            _ => return Err("Surface view has no validated program.".to_owned()),
        }
        let detail = if lab.view() == LabView::Face {
            let (index, face) = lab.selected_face().ok_or("No selected surface.")?;
            format!(
                "FACE {} AT {:03X} MAT {:04X} N {}",
                index + 1,
                face.offset,
                face.material,
                face.normal
            )
        } else if lab.view() == LabView::Normals {
            let count = normals
                .as_ref()
                .ok_or("Missing normal evaluation.")?
                .iter()
                .filter(|n| !n.culled)
                .count();
            format!(
                "{} NORMALS FRONT  TAB VIEW  INPUT {}",
                count,
                lab.animation()
            )
        } else {
            format!("INPUT {}   TAB VIEW   ,/. FACE", lab.animation())
        };
        self.text(3, 115, "NO FULL COMMANDS / SUBMODELS / SHADING", 12);
        self.text(3, 126, &detail, 17);
        self.text(3, 137, "1/2 MODEL  ARROWS ROTATE  0/5/9 TEST INPUT", 15);
        self.text(3, 148, "F4 RETURN - NO FLIGHT OR SAVE CHANGES", 18);
        self.present();
        Ok(())
    }
}

fn station_relative(state: &Flight) -> [f64; 3] {
    let p = state.position.map(|v| -v);
    [
        p[0] * state.yaw.cos() - p[2] * state.yaw.sin(),
        p[1],
        p[0] * state.yaw.sin() + p[2] * state.yaw.cos(),
    ]
}

fn project(point: [f64; 3], state: &Flight, translated: bool) -> Option<(i32, i32)> {
    let p = std::array::from_fn::<_, 3, _>(|i| {
        point[i] - if translated { state.position[i] } else { 0.0 }
    });
    let x = p[0] * state.yaw.cos() - p[2] * state.yaw.sin();
    let z = p[0] * state.yaw.sin() + p[2] * state.yaw.cos();
    let y = p[1] * state.pitch.cos() - z * state.pitch.sin();
    let z = p[1] * state.pitch.sin() + z * state.pitch.cos();
    if z <= 2.0 {
        return None;
    }
    let sx = 160.0 + 150.0 * x / z;
    let sy = 79.0 - 150.0 * y / z;
    if sx.abs() > 2000.0 || sy.abs() > 2000.0 {
        return None;
    }
    Some((sx as i32, sy as i32))
}
