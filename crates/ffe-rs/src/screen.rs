use crate::camera::{ship_axes, Camera, ViewMode, ViewState};
use crate::flight::{Flight, Mode};
use crate::indexed::{palette_rgb6, BitmapFont, IndexedImage, LOGICAL_HEIGHT, LOGICAL_WIDTH};
use crate::model_lab::{LabView, ModelLab};
use crate::model_viewer::{AnimationInputs, ModelViewer, Shape, THRUST_GLOBAL};
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::JITTER_SEED;
use ffe_core::model_fill::{fill_polygon, fill_quad, fill_triangle};
use ffe_core::model_line::{draw_curve, draw_line};
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
    /// The original console view icons (secondary bitmaps 63..81), when cached.
    icons: Vec<Option<IndexedImage>>,
    jitter: Jitter,
}

/// The F1476 stream the flight scene's jittered vertices draw from (the
/// flickering ends of thruster plumes). The original runs one stream across all
/// drawn objects and frames; here it advances once per flight step, so a paused
/// picture holds still, and it never feeds the simulation (KNOWN_ISSUES).
#[derive(Debug, Clone, Copy)]
struct Jitter {
    /// Flight step the current frame's start state belongs to.
    tick: Option<u64>,
    /// State the current flight step's first object draws from.
    start: [u32; 2],
    /// State after the last drawn object, in draw order.
    cursor: [u32; 2],
}

impl Default for Jitter {
    fn default() -> Self {
        Self {
            tick: None,
            start: JITTER_SEED,
            cursor: JITTER_SEED,
        }
    }
}

/// Console view buttons (`DATA_005100`): position, off and on bitmap ids, and the
/// button. The turret button (0x16) shows only for ships with more than two gun
/// mounts (as the original shows and hides it); missile (0x14) and combat (0x13)
/// need equipment that is not modelled, so they stay hidden.
const VIEW_BUTTONS: [(i32, i32, u8, u8, ViewButton); 4] = [
    (142, 163, 0xa2, 0xb1, ViewButton::Select(ViewMode::Front)),
    (162, 163, 0xa1, 0xb0, ViewButton::Select(ViewMode::Rear)),
    (181, 163, 0xa0, 0xaf, ViewButton::Turret),
    (198, 165, 0x9f, 0xae, ViewButton::Select(ViewMode::External)),
];

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
        let mut screen = Self::from_assets(
            asset("base:cockpit/cabin0")?,
            asset("base:font/main")?,
            panel,
            palette_rgb6(asset("base:palette/low")?)?,
        )?;
        // Console bitmap ids 96 and up are the secondary table.
        screen.icons = (0..=0xb1u32)
            .map(|id| {
                let index = id.checked_sub(96)?;
                let asset_id = format!("base:bitmap/secondary/{index:03}");
                let record = cache.manifest().files.iter().find(|r| r.id == asset_id)?;
                match record.asset {
                    AssetKind::IndexedBitmap {
                        width,
                        height,
                        offset_x,
                        offset_y,
                        masked_zero,
                        ..
                    } => IndexedImage::new(
                        width,
                        height,
                        [offset_x, offset_y],
                        masked_zero,
                        cache.asset(&asset_id)?,
                    )
                    .ok(),
                    _ => None,
                }
            })
            .collect();
        Ok(screen)
    }

    /// The console view button under logical pixel (x, y), if any (`DATA_007684`
    /// hot areas), or the F1 cycle area.
    pub fn view_button(x: i32, y: i32) -> Option<ViewButton> {
        let inside = |x0, y0, x1, y1| (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
        if inside(146, 163, 161, 182) {
            Some(ViewButton::Select(ViewMode::Front))
        } else if inside(166, 163, 181, 182) {
            Some(ViewButton::Select(ViewMode::Rear))
        } else if inside(185, 163, 200, 182) {
            Some(ViewButton::Turret)
        } else if inside(202, 165, 217, 184) {
            Some(ViewButton::Select(ViewMode::External))
        } else if inside(64, 172, 79, 191) {
            Some(ViewButton::Cycle)
        } else {
            None
        }
    }

    /// The logical 320x200 pixel under a mouse position in window client pixels
    /// (`Window::get_unscaled_mouse_pos`), for a client area of `size` showing the
    /// WIDTH x HEIGHT buffer with minifb's aspect-ratio stretch. `None` on the
    /// letterbox bars or outside the window.
    pub fn logical_point(x: f32, y: f32, size: (usize, usize)) -> Option<(i32, i32)> {
        let (window_width, window_height) = (size.0 as i32, size.1 as i32);
        if window_width <= 0 || window_height <= 0 {
            return None;
        }
        // As minifb's WM_PAINT places the picture.
        let aspect = WIDTH as f32 / HEIGHT as f32;
        let (mut width, mut height, mut left, mut top) = (window_width, window_height, 0, 0);
        if aspect > window_width as f32 / window_height as f32 {
            height = (window_width as f32 / aspect) as i32;
            top = (window_height - height) / 2;
        } else {
            width = (window_height as f32 * aspect) as i32;
            left = (window_width - width) / 2;
        }
        let (px, py) = (x - left as f32, y - top as f32);
        if width <= 0 || height <= 0 || px < 0.0 || py < 0.0 {
            return None;
        }
        let (lx, ly) = (
            (px * LOGICAL_WIDTH as f32 / width as f32) as i32,
            (py * LOGICAL_HEIGHT as f32 / height as f32) as i32,
        );
        (lx < LOGICAL_WIDTH as i32 && ly < LOGICAL_HEIGHT as i32).then_some((lx, ly))
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
            icons: Vec::new(),
            jitter: Jitter::default(),
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
        let value = self.palette[usize::from(colour)];
        self.line_value(start, end, value);
    }

    /// Bresenham in the 320x158 view, bounded to 4096 steps.
    fn line_value(&mut self, start: (i32, i32), end: (i32, i32), value: u32) {
        let (mut x, mut y) = start;
        let (end_x, end_y) = end;
        let dx = (end_x - x).abs();
        let dy = -(end_y - y).abs();
        let sx = if x < end_x { 1 } else { -1 };
        let sy = if y < end_y { 1 } else { -1 };
        let mut error = dx + dy;
        for _ in 0..4096 {
            if (0..VIEW_HEIGHT as i32).contains(&y) && (0..320).contains(&x) {
                self.frame[y as usize * 320 + x as usize] = value;
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

    /// Even-odd scanline fill at pixel centres, clipped to the 320x158 view.
    fn fill_value(&mut self, points: &[(i32, i32)], value: u32) {
        let Some(top) = points.iter().map(|p| p.1).min() else {
            return;
        };
        let bottom = points.iter().map(|p| p.1).max().unwrap_or(top);
        let mut crossings = Vec::with_capacity(points.len());
        for y in top.max(0)..=bottom.min(VIEW_HEIGHT as i32 - 1) {
            let centre = f64::from(y) + 0.5;
            crossings.clear();
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                let (ay, by) = (f64::from(a.1), f64::from(b.1));
                if (ay <= centre) != (by <= centre) {
                    let t = (centre - ay) / (by - ay);
                    crossings.push(f64::from(a.0) + t * f64::from(b.0 - a.0));
                }
            }
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.chunks_exact(2) {
                let start = (pair[0] - 0.5).ceil().max(0.0) as i32;
                let end = ((pair[1] - 0.5).floor() as i32).min(319);
                for x in start..=end {
                    self.frame[y as usize * 320 + x as usize] = value;
                }
            }
        }
        // Edges as well, so thin and edge-on faces stay visible.
        for (i, a) in points.iter().enumerate() {
            self.line_value(*a, points[(i + 1) % points.len()], value);
        }
    }

    fn text(&mut self, x: usize, y: usize, message: &str, colour: u8) {
        let bytes: Vec<u8> = message
            .chars()
            .map(|character| {
                if character.is_ascii() && !character.is_ascii_control() {
                    character as u8
                } else {
                    b'?'
                }
            })
            .collect();
        self.text_bytes(x, y, &bytes, colour);
    }

    fn text_bytes(&mut self, x: usize, y: usize, message: &[u8], colour: u8) {
        let mut cursor = x;
        for &code in message {
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

    /// `scene`, when present, draws the station as the original model 74.
    pub fn draw(
        &mut self,
        state: &Flight,
        views: ViewState,
        scene: Option<&ModelViewer>,
        help: bool,
        modern: bool,
        message: &str,
    ) {
        self.indexed.fill(0);
        self.panel.blit(&mut self.indexed, 0, 158);
        let view = views.mode;
        let stats = scene.and_then(|scene| scene.ship_stats(state.ship));
        let turrets = stats.is_some_and(|stats| stats.gun_mounts > 2);
        if state.mode != Mode::Docked {
            // Only the current view's icon is lit (radio group 1).
            for (x, y, off, on, button) in VIEW_BUTTONS {
                let lit = match button {
                    ViewButton::Select(mode) => mode == view,
                    ViewButton::Turret if !turrets => continue,
                    ViewButton::Turret => view.is_turret(),
                    ViewButton::Cycle => false,
                };
                let id = if lit { on } else { off };
                if let Some(Some(icon)) = self.icons.get(usize::from(id)) {
                    icon.blit(&mut self.indexed, x, y);
                }
            }
        }
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
            self.text(36, 87, "L LAUNCH  F5 SAVE  SHIFT+F5 LOAD", 15);
            self.text(36, 101, "F2 SCENARIOS / RESET", 12);
        } else {
            let scale = scene.and_then(model_scale).unwrap_or(DEFAULT_MODEL_SCALE);
            let camera = Camera::new(state, &views, stats.as_ref(), scale);
            // A new flight step continues the stream where the last frame left it;
            // redrawing the same step replays it.
            if self.jitter.tick != Some(state.ticks) {
                self.jitter.tick = Some(state.ticks);
                self.jitter.start = self.jitter.cursor;
            }
            self.jitter.cursor = self.jitter.start;
            for index in 0..self.stars.len() {
                if let Some((x, y)) = camera.project(self.stars[index], false) {
                    if y < 158 {
                        self.pixel(x, y, 12);
                    }
                }
            }
            // FUNC_001054 hides the player's own ship only in the front view.
            // Painter's order between the two objects: the farther one first.
            let own_ship = view != ViewMode::Front;
            let ship_first = own_ship
                && distance(camera.position, state.position) > distance(camera.position, [0.0; 3]);
            if ship_first {
                if let Some(scene) = scene {
                    self.draw_ship(scene, state, &camera);
                }
            }
            let drawn = scene.is_some_and(|scene| self.draw_station(scene, state, &camera));
            if !ship_first && own_ship {
                if let Some(scene) = scene {
                    self.draw_ship(scene, state, &camera);
                }
            }
            // Placeholder wireframe when the cache has no drawable model 74.
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
                if drawn {
                    break;
                }
                if let (Some(a), Some(b)) = (
                    camera.project(vertices[a], true),
                    camera.project(vertices[b], true),
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
                if drawn {
                    break;
                }
                if let (Some(a), Some(b)) = (camera.project(a, true), camera.project(b, true)) {
                    self.line(a, b, if state.docking_requested { 17 } else { 18 });
                }
            }
            if let Some((x, y)) = camera.project([0.0; 3], true) {
                if x > 8 && x < 260 && y > 12 && y < 115 {
                    self.text(x as usize + 5, y as usize, "STATION", 17);
                }
            } else {
                self.text(8, 16, "STATION BEHIND / OFF VIEW", 18);
            }
            if view == ViewMode::Front {
                self.line((155, 79), (158, 79), 18);
                self.line((162, 79), (165, 79), 18);
                self.line((160, 74), (160, 77), 18);
                self.line((160, 81), (160, 84), 18);
            }
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
        if state.mode != Mode::Docked {
            let name = view.name().to_ascii_uppercase();
            let width = self.font.width(&name).unwrap_or(0);
            self.text(317usize.saturating_sub(width), 1, &name, 15);
        }
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
                "DOCK {}  {}  H HELP  F2 RESET  F3 INPUT",
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
                "Original console, station and ship models.",
                "DEFAULT MODERN: W/S thrust; arrows yaw/pitch",
                "CLASSIC-LIKE: Enter/Shift thrust; ,/. yaw; A/Z pitch",
                "X brake  D dock  L launch  B buy  N sell  PGUP/DN ship",
                "VIEWS: F1 cycle F6 front F7 rear F8 turret F9 external",
                "External: arrows turn, +/- zoom. Turret: arrows aim",
                "F3 input. F4 lab. M models. P pause. Esc quit.",
                "F5 save snapshot; Shift+F5 load newest snapshot.",
                "F2 scenarios: 1 Docked / 2 Approach / 3 Free Flight",
                "Dock: range <80, speed <20, aim at station.",
                "No audio/combat/hyperspace/old saves/HD packs yet.",
                "H closes help. F2 opens playtest scenarios.",
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

    /// F6: every imported model executed by the verified port, in original order.
    /// Draw executed-model shapes in order into the 320x158 view.
    fn draw_shapes(&mut self, viewer: &ModelViewer, shapes: &[Shape]) {
        let value = |[r, g, b]: [u8; 3]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        for shape in shapes {
            match shape {
                Shape::Fill(points, rgb) => self.fill_value(points, value(*rgb)),
                Shape::Polygon(edges, rgb) => {
                    // F1560 keeps rows in 0..158 and F1600 clips x when an edge
                    // reaches off the view, so every pixel lands in the view.
                    let frame = &mut self.frame;
                    let _ = fill_polygon(edges, |x, y| {
                        if (0..320).contains(&x) && (0..VIEW_HEIGHT as i32).contains(&y) {
                            frame[y as usize * 320 + x as usize] = value(*rgb);
                        }
                    });
                }
                Shape::Flat(points, rgb) => {
                    // F1604/F1607 on an on-view entry: every pixel lands in the view.
                    let frame = &mut self.frame;
                    let p: Vec<[i16; 2]> =
                        points.iter().map(|&(x, y)| [x as i16, y as i16]).collect();
                    let plot = |x: i32, y: i32| frame[y as usize * 320 + x as usize] = value(*rgb);
                    if let [a, b, c] = p[..] {
                        fill_triangle([a, b, c], plot);
                    } else if let [a, b, c, d] = p[..] {
                        fill_quad([a, b, c, d], plot);
                    }
                }
                Shape::Line(a, b, rgb) => {
                    let on_view = |(x, y): (i32, i32)| {
                        (0..320).contains(&x) && (0..VIEW_HEIGHT as i32).contains(&y)
                    };
                    if on_view(*a) && on_view(*b) {
                        // F1616 on end points F1704 has already put on the view.
                        let frame = &mut self.frame;
                        let point = |(x, y): (i32, i32)| [x as i16, y as i16];
                        draw_line(point(*a), point(*b), |x, y| {
                            frame[y as usize * 320 + x as usize] = value(*rgb);
                        });
                    } else {
                        // Stand-in shapes (two-point faces): bounded Bresenham.
                        self.line_value(*a, *b, value(*rgb));
                    }
                }
                Shape::Curve(points, rgb) => {
                    // F1546 can leave a clipped end off the view; the original then
                    // draws outside it (over the console), which is dropped here.
                    let frame = &mut self.frame;
                    let _ = draw_curve(*points, |x, y| {
                        if (0..320).contains(&x) && (0..VIEW_HEIGHT as i32).contains(&y) {
                            frame[y as usize * 320 + x as usize] = value(*rgb);
                        }
                    });
                }
                Shape::Point((x, y), rgb) => {
                    if (0..VIEW_HEIGHT as i32).contains(y) && (0..320).contains(x) {
                        self.frame[*y as usize * 320 + *x as usize] = value(*rgb);
                    }
                }
                Shape::Textured(textured) | Shape::Glow(textured) => {
                    let frame = &mut self.frame;
                    // The rasteriser clips to x 0..320, y 0..158 itself.
                    viewer.rasterise(textured, |x, y, rgb| {
                        frame[y as usize * 320 + x as usize] = value(rgb);
                    });
                }
                Shape::Label((x, y), text) => {
                    if *x >= 0 && *y >= 0 {
                        self.text_bytes(*x as usize, *y as usize, text, 15);
                    }
                }
            }
        }
    }

    /// The station as the original model 74, executed by the verified port and
    /// placed by the synthetic flight camera. False when it cannot be drawn.
    fn draw_station(&mut self, scene: &ModelViewer, state: &Flight, camera: &Camera) -> bool {
        let Some(scale) = model_scale(scene) else {
            return false;
        };
        // Station rotation (docs/notes/animation.md): F866 spins the station
        // about its own z (docking) axis at the generated rate; the flight's
        // 50 Hz ticks are 994 game tics each from the new-game GameTics.
        let tics = flight_tics(state);
        let spin = scene.station_rows(tics, STATION_SPEED);
        let axes: [[f64; 3]; 3] = std::array::from_fn(|k| {
            std::array::from_fn(|i| (0..3).map(|j| spin[k][j] * STATION_AXES[j][i]).sum())
        });
        let rows = axes.map(|axis| camera.direction(axis));
        let origin = camera.point([0.0; 3]).map(|v| v * scale);
        // The original default light (F960), fixed in flight space.
        let light = camera.direction(scene.light_direction());
        let mut inputs = AnimationInputs::original(tics, 0, &scene.shades());
        inputs.jitter = self.jitter.cursor;
        match scene.render_scene_with(STATION_MODEL, rows, origin, light, &inputs) {
            Ok(frame) => {
                self.jitter.cursor = frame.jitter;
                self.draw_shapes(scene, &frame.shapes);
                true
            }
            Err(_) => false,
        }
    }

    /// The player's ship (its own model, at the station's scale) seen by `camera`.
    fn draw_ship(&mut self, scene: &ModelViewer, state: &Flight, camera: &Camera) -> bool {
        let Some(scale) = model_scale(scene) else {
            return false;
        };
        let rows = ship_axes(state).map(|axis| camera.direction(axis));
        let origin = camera.point(state.position).map(|v| v * scale);
        let light = camera.direction(scene.light_direction());
        // The original inputs over flight time (gear up), with global 15 as the
        // object's z thrust word the engine glows read: the throttle scaled to the
        // ship's own limits (F600). Without statistics (a cache before v12), full
        // thrust maps to 4095, where ship models stop lengthening the plume.
        let mut inputs = AnimationInputs::original(flight_tics(state), 0, &scene.shades());
        inputs.globals[THRUST_GLOBAL] = match scene.ship_stats(state.ship) {
            Some(stats) => stats.thrust_word(state.thrust),
            None => (state.thrust.max(0.0) * 4095.0) as u16,
        };
        // The plume's far end is a jittered vertex: it flickers as the stream runs.
        inputs.jitter = self.jitter.cursor;
        match scene.render_scene_with(state.ship, rows, origin, light, &inputs) {
            Ok(frame) => {
                self.jitter.cursor = frame.jitter;
                self.draw_shapes(scene, &frame.shapes);
                true
            }
            Err(_) => false,
        }
    }

    pub fn draw_model_viewer(&mut self, viewer: &ModelViewer) {
        // Dark blue backdrop so black (unlit) faces stay visible as silhouettes.
        self.frame[..320 * VIEW_HEIGHT].fill(0x00_00_10_28);
        let model = viewer.model();
        let frame = viewer.render();
        let status = match &frame {
            Ok(frame) => {
                self.draw_shapes(viewer, &frame.shapes);
                if let Some(error) = &frame.children_error {
                    self.text(3, 23, "PARTS NOT SHOWN YET:", 12);
                    let words: String = error.to_ascii_uppercase().chars().take(60).collect();
                    self.text(3, 34, &words, 12);
                }
                let mut status = format!("{} SHAPES", frame.shapes.len());
                if frame.skipped > 0 {
                    status = format!(
                        "{} SHAPES, {} NOT DRAWN YET",
                        frame.shapes.len(),
                        frame.skipped
                    );
                }
                if frame.texts > 0 {
                    status.push_str(&format!(", {} TEXT", frame.texts));
                }
                status
            }
            Err(error) => {
                // Shown on screen, not logged: this runs every frame.
                self.text(3, 23, "CANNOT SHOW THIS MODEL YET:", 12);
                let words: String = error.to_ascii_uppercase().chars().take(60).collect();
                self.text(3, 34, &words, 12);
                "NOT DRAWN".to_owned()
            }
        };
        self.text(
            3,
            1,
            &format!(
                "MODEL VIEWER {}/{}  MODEL {}  M BACK",
                viewer.selected() + 1,
                viewer.len(),
                model.index,
            ),
            18,
        );
        self.text(3, 12, &status, 15);
        // The animation inputs: the test constant, or the original inputs over
        // the viewer's clock (global 0 = tics >> 10, global 1 = gear/door word).
        let (tics, door, rate) = viewer.clock();
        let engines = match viewer.thrust() {
            Some(thrust) => format!("ENGINES {thrust}"),
            None => "ENGINES OFF".to_owned(),
        };
        let inputs = match viewer.animation() {
            Some(value) => format!("TEST INPUT {value}  {engines}"),
            None => format!(
                "TIME {:04X}  DOOR {door:04X}  X{rate}  {engines}",
                ffe_core::model_animation::time_word(tics)
            ),
        };
        self.text(3, 126, &inputs, 15);
        self.text(3, 137, "PGUP/PGDN MODEL  ARROWS TURN  +/- ZOOM", 15);
        self.text(3, 148, "0/5/9 TEST C CLOCK G GEAR T RATE E ENGINES", 18);
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

/// Original model table slot drawn as the flight station.
const STATION_MODEL: u32 = 74;
/// Flight-space radius the station model is scaled to (the old placeholder size).
const STATION_RADIUS: f64 = 40.0;
/// Station object axes in flight space: model 74's docking port is at its +z end,
/// turned to face the approach from -z.
const STATION_AXES: [[f64; 3]; 3] = [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];
/// F866 rotation speed of a generated station with model 74 (F892 case 2,
/// template `DATA_008795`: hand-coded Abraham Lincoln, speed 10).
const STATION_SPEED: i8 = ffe_core::model_animation::ORBITAL_CITY_SPEED;

/// The game clock the flight view shows: the new-game `GameTics` plus 994 tics
/// (20 ms at acceleration 1) per 50 Hz flight step.
pub fn flight_tics(state: &Flight) -> u32 {
    ffe_core::model_animation::START_TICS
        .wrapping_add((state.ticks as u32).wrapping_mul(ffe_core::model_animation::TICS_PER_FRAME))
}

/// Model units per flight unit: station 74's extent fills the old 40-unit
/// placeholder (docking and safety distances unchanged), and every other model
/// shares that scale, so ships keep their true size relative to the station.
pub fn model_scale(scene: &ModelViewer) -> Option<f64> {
    scene
        .find(STATION_MODEL)
        .and_then(|m| m.radius())
        .map(|radius| radius / STATION_RADIUS)
}

/// Model units per flight unit when the cache has no station model.
const DEFAULT_MODEL_SCALE: f64 = 9700.0;

/// A console button click: select a view (F6/F7/F9), the turret (F8) or cycle
/// (F1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewButton {
    Select(ViewMode),
    Turret,
    Cycle,
}

/// A model's bounding radius in flight units, at the shared model scale.
pub fn model_radius(scene: &ModelViewer, model: u32) -> Option<f64> {
    Some(scene.find(model)?.radius()? / model_scale(scene)?)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
