//! The original's galaxy map (F828-F863, F840): a 3D scene of the stars around a centre, drawn
//! by the ordinary model machinery. Docs: `docs/notes/galaxy-map.md`.
//!
//! The scene is, in the original's call order: the grid (model `0xa6`), the nine sectors around
//! the centre (each its stars, model `0xab`, and for the hand-made ones a model of their own), the
//! range sphere (`0xaa`, scaled by the drive's range) and the selection marker (`0xa9`). The
//! camera is a matrix built from two accumulators and a zoom depth; the star nearest the centre
//! (within a quarter of a sector) is the selection.
use crate::model_viewer::{AnimationInputs, ModelViewer, Shape};
use ffe_core::frames::vec_mat_mul;
use ffe_core::model_execute::child_culled;
use ffe_core::sector::{self, position, Tables as SectorTables};
use ffe_core::star_points::StarField;

/// The zoom depth a new map starts at (`+0x242`).
pub const START_ZOOM: i32 = 0x30d40;
/// The flags a new map starts with (`+0x24e`): the grid (1), the stalks (0x20) and the bits the
/// models read besides.
pub const START_FLAGS: u16 = 0x69;
/// Bit 0 of the flags draws the grid; bit 5 draws each star's stalk.
pub const GRID: u16 = 1;
pub const STALKS: u16 = 0x20;
/// One sector in the 16.16 coordinates of the centre and the stars.
pub const SECTOR: i32 = 0x1_0000;

/// The models the map draws.
const GRID_MODEL: u32 = 0xa6;
const MARKER_MODEL: u32 = 0xa9;
const SPHERE_MODEL: u32 = 0xaa;
const STAR_MODEL: u32 = 0xab;

/// The model a hand-made sector draws besides its stars, by its index among the 49
/// (`DATA_008776`, built by F829); zero draws none.
fn special_model(index: usize) -> u32 {
    match index {
        0 => 0xa8,
        1 => 0x155,
        5 => 0x156,
        6 => 0x157,
        16 => 0x158,
        24 => 0x159,
        7 | 17 | 33 | 37 | 38 | 46 | 47 => 0xad,
        48 => 0xae,
        _ => 0,
    }
}

/// The star the map has selected (the one nearest the centre).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// F856: the star's position, 16.16 in each axis.
    pub position: [i32; 3],
    /// Its system code (sector, and the star's number in the sector in bits 26..31).
    pub code: u32,
    /// The star's type word (low byte: its class, high byte: how many stars share it).
    pub kind: u16,
}

#[derive(Debug, Clone)]
pub struct GalaxyMap {
    /// The centre of the view in sectors, 16.16 (`+0x23a`, `+0x23e`).
    pub centre: [i32; 2],
    /// The depth the centre sits at (`+0x242`).
    pub zoom: i32,
    pub flags: u16,
    /// The two accumulators the camera turns by (low word, high word).
    pub spin: [i16; 2],
    /// The player's own system (`+0x250..+0x258`).
    pub home: [i32; 3],
    pub selected: Option<Selected>,
    /// The centre moved: the selection is looked for again.
    pub dirty: bool,
    /// The range of the player's drive in its units (`DATA_008907`); the sphere's radius.
    pub reach: i32,
}

/// The system code of sector (`x`, `y`) as F841 makes it: `x` in bits 0..12, `y` in 13..25.
pub fn sector_code(x: u16, y: u16) -> u32 {
    u32::from(x & 0x1fff) | u32::from(y & 0x1fff) << 13
}

impl GalaxyMap {
    /// A map opened on the player's system (F829's set-up).
    pub fn new(home: [i32; 3], reach: i32) -> Self {
        Self {
            centre: [home[0], home[1]],
            zoom: START_ZOOM,
            flags: START_FLAGS,
            spin: [0, 0],
            home,
            selected: None,
            dirty: true,
            reach,
        }
    }

    /// The `c` key: back to the player's system, level, at the starting depth.
    pub fn recentre(&mut self) {
        self.centre = [self.home[0], self.home[1]];
        self.spin = [0, 0];
        self.zoom = START_ZOOM;
        self.dirty = true;
    }

    /// Move the centre by `x` and `y` (16.16 sectors).
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.centre = [
            self.centre[0].wrapping_add(x),
            self.centre[1].wrapping_add(y),
        ];
        self.dirty = true;
    }

    /// Move the centre in or out (the original's steps are `0x1f40`, half of that with shift).
    pub fn zoom_by(&mut self, amount: i32) {
        self.zoom = self.zoom.wrapping_add(amount);
    }

    /// Turn the view: the accumulators the mouse moves.
    pub fn rotate(&mut self, x: i16, y: i16) {
        self.spin = [self.spin[0].wrapping_add(x), self.spin[1].wrapping_add(y)];
    }

    /// The sphere's radius word: the drive's range against 32 light years, full at that.
    fn sphere_word(&self) -> u16 {
        if self.reach >= 0xc80 {
            0xffff
        } else {
            (((i64::from(self.reach) << 16) - i64::from(self.reach)) / 0xc80) as u16
        }
    }

    /// The camera matrix: the yaw and pitch of the accumulators (`<< 7`), the rows then
    /// permuted as F840 does so that the map's z points out of the screen.
    fn matrix(&self, viewer: &ModelViewer) -> [i32; 9] {
        let m = viewer.matrix_yzt(i32::from(self.spin[0]) << 7, i32::from(self.spin[1]) << 7);
        [m[5], m[2], m[8], m[3], m[0], m[6], m[4], m[1], m[7]]
    }

    /// F840's selection: the star of the centre sector nearest the centre, within 0x20 of the
    /// sector's 512-unit steps, else none.
    pub fn update_selection(&mut self, tables: &SectorTables) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let (sx, sy) = ((self.centre[0] >> 16) as u16, (self.centre[1] >> 16) as u16);
        let fraction =
            |value: i32| i32::from((((value & 0xfe00) as u16).wrapping_add(0x8000)) as i16) >> 9;
        let (fx, fy) = (fraction(self.centre[0]), fraction(self.centre[1]));
        let stars = tables.stars(sx, sy);
        let mut best = 0x20;
        let mut found = None;
        for (index, star) in stars.iter().enumerate() {
            let miss = (i32::from(star.vector[4] as i8) - fx).abs()
                + (i32::from(star.vector[3] as i8) - fy).abs();
            if miss < best {
                best = miss;
                found = Some(index);
            }
        }
        self.selected = found.map(|index| {
            let star = &stars[index];
            Selected {
                position: position(sx, sy, &star.vector),
                code: sector_code(sx, sy) | (index as u32) << 26,
                kind: star.kind,
            }
        });
    }

    /// An object F862 places: the point relative to the centre, in the map's permuted axes.
    fn place(&self, point: [i32; 3], matrix: &[i32; 9]) -> Option<[i32; 3]> {
        let y = point[1].wrapping_sub(self.centre[1]) >> 2;
        let x = point[0].wrapping_sub(self.centre[0]) >> 2;
        if !(-0x7fff..=0x7fff).contains(&y) || !(-0x7fff..=0x7fff).contains(&x) {
            return None;
        }
        let out = vec_mat_mul([point[2] >> 2 << 15, y << 15, x << 15], matrix);
        Some([
            out[0] >> 13,
            out[1] >> 13,
            (out[2] >> 13).wrapping_add(self.zoom),
        ])
    }

    /// Draw the scene: the shapes in the original's order, farthest object first as it calls
    /// them. Models the cache lacks are an error.
    pub fn render(
        &self,
        viewer: &ModelViewer,
        tables: &SectorTables,
    ) -> Result<Vec<Shape>, String> {
        let matrix = self.matrix(viewer);
        let rows: [[f64; 3]; 3] = std::array::from_fn(|k| {
            std::array::from_fn(|j| f64::from(matrix[3 * k + j]) / 2_147_483_648.0)
        });
        let offset = |value: i32| (-(i32::from((value as i16).wrapping_add(i16::MIN)) >> 1)) << 15;
        let to_view = vec_mat_mul([0, offset(self.centre[1]), offset(self.centre[0])], &matrix);
        let sector_step = vec_mat_mul([0, 0, 1 << 29], &matrix);
        let row_step = vec_mat_mul([0, 1 << 29, 0], &matrix);
        let at = |along: i32, across: i32| -> [i32; 3] {
            std::array::from_fn(|k| {
                let base = to_view[k] >> 14;
                let moved = base
                    .wrapping_add(along.wrapping_mul(sector_step[k] >> 13))
                    .wrapping_add(across.wrapping_mul(row_step[k] >> 13));
                if k == 2 {
                    moved.wrapping_add(self.zoom)
                } else {
                    moved
                }
            })
        };
        let light = viewer.light_direction();
        let mut shapes = Vec::new();
        let mut draw = |index: u32,
                        position: [i32; 3],
                        inputs: &AnimationInputs,
                        stars: Option<(&StarField, &[u8])>|
         -> Result<(), String> {
            let model = viewer
                .find(index)
                .ok_or_else(|| format!("Model {index:#x} is not in the cache."))?;
            // F1686: an object wholly behind or beside the view is not drawn.
            if let Some(radius) = viewer.model_radius(index) {
                if child_culled(position, radius) {
                    return Ok(());
                }
            }
            let frame =
                viewer.render_matrix(model, matrix, rows, position, light, inputs, stars)?;
            shapes.extend(frame.shapes);
            Ok(())
        };
        let inputs = |count: u16, code: u32, word: u16| {
            // The original's inputs for an object whose other words are zero (F1683, F1537).
            let mut inputs = AnimationInputs::original(0, self.flags, &viewer.shades());
            inputs.globals[2] = code as u16;
            inputs.globals[3] = (code >> 16) as u16;
            inputs.globals[4] = if word != 0 { word } else { count };
            inputs
        };
        // The grid.
        if self.flags & GRID != 0 {
            draw(GRID_MODEL, at(0, 0), &inputs(0, 0, 0), None)?;
        }
        // The nine sectors: the centre's, then round it from the next one along.
        let around = [
            (0, 0),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ];
        for (along, across) in around {
            let (x, y) = (
                (self.centre[0].wrapping_add(along << 16) >> 16) as u16,
                (self.centre[1].wrapping_add(across << 16) >> 16) as u16,
            );
            let stars = tables.stars(x, y);
            let code = sector_code(x, y);
            let position = at(along, across);
            if !stars.is_empty() {
                let field = StarField {
                    types: stars.iter().map(|s| s.kind as u8).collect(),
                    stalks: self.flags & STALKS != 0,
                    code,
                };
                let records: Vec<u8> = stars.iter().flat_map(|s| s.vector).collect();
                draw(
                    STAR_MODEL,
                    position,
                    &inputs(stars.len() as u16, code, 0),
                    Some((&field, &records)),
                )?;
            }
            if let Some(index) = tables.special_index(x, y) {
                let model = special_model(index);
                if model != 0 {
                    draw(model, position, &inputs(stars.len() as u16, code, 0), None)?;
                }
            }
        }
        // The range sphere about the player's own system and the marker on the selection.
        if let Some(position) = self.place(self.home, &matrix) {
            draw(
                SPHERE_MODEL,
                position,
                &inputs(0, 0, self.sphere_word()),
                None,
            )?;
        }
        if let Some(selected) = &self.selected {
            if let Some(position) = self.place(selected.position, &matrix) {
                draw(MARKER_MODEL, position, &inputs(0, 0, 0), None)?;
            }
        }
        Ok(shapes)
    }

    /// The sector the centre is in, relative to Sol's, as the map's corner text shows it.
    pub fn sector_text(&self) -> String {
        format!(
            "Sector: {},{}",
            (self.centre[0] >> 16) - 0x1718,
            (self.centre[1] >> 16) - 0x1524
        )
    }
}

impl GalaxyMap {
    /// F863's text for the selected star: its name and distance from the player's system, then
    /// its type and what is known of the system (F832 builds the pieces, `Details` the
    /// description code). `None` when nothing is selected.
    pub fn describe(
        &self,
        viewer: &ModelViewer,
        details: Option<&ffe_core::galaxy::Tables>,
    ) -> Option<String> {
        let selected = self.selected.as_ref()?;
        let text = |bytes: Vec<u8>| bytes.iter().map(|b| char::from(*b)).collect::<String>();
        let name = text(viewer.system_name(selected.code)?);
        let distance = sector::distance(self.home, selected.position);
        let mut star = String::new();
        let multiple = u32::from(selected.kind >> 8) & 7;
        if multiple != 0 {
            star.push_str(&text(viewer.system_string(0x8498 + multiple)?));
        }
        star.push_str(&text(
            viewer.system_string(0x8488 + (u32::from(selected.kind) & 0x1f))?,
        ));
        let mut lines = format!(
            "{name} Dist. {:.2} light years\r{star}",
            f64::from(distance) / 100.0
        );
        if let Some(details) = details {
            let code = details.details(selected.code).words[0];
            if let Some(description) = viewer.system_string(code) {
                lines.push_str(": ");
                lines.push_str(&text(description));
            }
        }
        Some(lines)
    }

    /// The line F828 shows below the sector: what the jump to the selected star needs, or why it
    /// cannot be made. `report` is the hyperdrive's answer for the distance.
    pub fn fuel_text(&self, report: Option<&ffe_core::hyperspace::Report>) -> Option<String> {
        let selected = self.selected.as_ref()?;
        if sector::distance(self.home, selected.position) == 0 {
            return None;
        }
        let report = report?;
        // F828's three strings (DATA_007004..7006): the distance is fine and
        // needs so much fuel, the drive cannot reach it, or the hold is short.
        Some(if report.result < 0 {
            if report.fuel_needed < 0 {
                "Out of Range".to_owned()
            } else {
                format!("Insufficient Fuel ({} required)", report.fuel_needed)
            }
        } else {
            format!("Fuel Required: {}", report.fuel_needed)
        })
    }
}

/// The game's date and time for `seconds` since the new game's start (1 Jan 3250,
/// noon), as the console shows it. The calendar is the plain one. Callers with an
/// absolute game clock (`Navigator::clock`, whose new game starts at
/// `0x121cf7.5` days) use [`crate::navigation::Navigator::date_text`], which
/// subtracts the start; otherwise the absolute day would be read as a year.
pub fn date_text(seconds: f64) -> String {
    const MONTHS: [(&str, u32); 12] = [
        ("Jan", 31),
        ("Feb", 28),
        ("Mar", 31),
        ("Apr", 30),
        ("May", 31),
        ("Jun", 30),
        ("Jul", 31),
        ("Aug", 31),
        ("Sep", 30),
        ("Oct", 31),
        ("Nov", 30),
        ("Dec", 31),
    ];
    let total = (seconds.max(0.0) as u64) + 12 * 3600;
    let (mut days, rest) = (total / 86_400, total % 86_400);
    let mut year = 3250u32;
    while days >= 365 {
        days -= 365;
        year += 1;
    }
    let mut month = 0;
    while days >= u64::from(MONTHS[month].1) {
        days -= u64::from(MONTHS[month].1);
        month += 1;
    }
    format!(
        "{}-{}-{} {:02}:{:02}:{:02}",
        days + 1,
        MONTHS[month].0,
        year,
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// F1358's date without the clock, as the tombstone engraves it:
/// `1-JAN-3250` (F1356 uses the current day count and uppercases the month).
pub fn date_short_text(seconds: f64) -> String {
    let full = date_text(seconds);
    let date = full.split(' ').next().unwrap_or("");
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(day), Some(month), Some(year)) => {
            format!("{day}-{}-{year}", month.to_ascii_uppercase())
        }
        _ => date.to_owned(),
    }
}
