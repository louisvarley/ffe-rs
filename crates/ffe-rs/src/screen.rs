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

/// A star, planet or moon seen as a lit disc: flight-space centre and radius, the
/// unit direction toward the light, and the palette indices (lit, dark).
#[derive(Debug, Clone, Copy)]
pub struct Disc {
    pub centre: [f64; 3],
    pub radius: f64,
    pub light: [f64; 3],
    pub colours: (u8, u8),
    pub star: bool,
    /// The body's model and surface seed, when it has a drawable surface.
    pub model: Option<(u32, u32)>,
    /// The object's rows in flight space.
    pub orientation: [[f64; 3]; 3],
}

/// A station of the system in the flight view: its own model at its place in the
/// system, turning about its docking axis as F866 turns it (set by the caller each
/// frame).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemStation {
    pub model: u32,
    /// Centre in flight space.
    pub centre: [f64; 3],
    /// The object's rows in flight space.
    pub orientation: [[f64; 3]; 3],
    /// F866's signed speed byte (`+0xc8`); 0 does not turn.
    pub speed: i8,
    /// The station record's door word (`+0x9e`, global 1): 0 is open, 0xffff
    /// closed (F865).
    pub door: u16,
    /// The game tics the spin and the animation are taken at.
    pub tics: u32,
}

/// The farthest a station is drawn from (millimetres from the eye): the viewer's
/// view space is 32-bit, and an orbital city is under a pixel beyond it.
const STATION_DRAW_RANGE: f64 = 2.0e9;

/// The lift scene of a docking or launching stage (`ffe_core::lift`), drawn in the
/// front view after the outside world (or alone, inside the station).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiftView {
    pub objects: Vec<ffe_core::lift::Object>,
    /// Draw the stars, planets, stations and the ship first (docking, outside); false
    /// inside the station (launching) where only the lift is drawn.
    pub world: bool,
    /// The game tics the models' time word reads.
    pub tics: u32,
    /// The station's random seed (object `+0xa0`, global words 2 and 3): it picks the
    /// posters on the walls, so every station has its own.
    pub seed: u32,
    /// An open-air starport (`+0x14c` bit 5): global word 4 is 0 there, 1 in an orbital
    /// station (F1070).
    pub starport: bool,
}

/// The galaxy map as the screen draws it: the scene's shapes over the cabin art and the corner
/// and console texts (`galaxy_map`).
#[derive(Debug, Clone, Default)]
pub struct MapScene {
    pub shapes: Vec<Shape>,
    /// "Sector: x,y", at the lower left of the view.
    pub sector: String,
    /// The fuel the selected jump needs, or why it cannot be made.
    pub fuel: Option<String>,
    /// The game's date and time, in green at the top of the console text.
    pub date: String,
    /// The selected star's description, wrapped to the console text's width here.
    pub info: String,
}

/// One commodity line of the station's market screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRow {
    pub name: String,
    pub price: i32,
    pub stock: i32,
    pub held: u16,
}

/// The station's market as the docked screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarketView {
    pub station: String,
    pub system: String,
    pub rows: Vec<MarketRow>,
    pub selected: usize,
    pub cash: i32,
    pub used: i32,
    pub capacity: i32,
}

/// Rows of the market list shown at once.
pub const MARKET_ROWS: usize = 8;

/// The controls button in the top bar: left edge and right edge (logical pixels).
pub const CONTROLS_BUTTON: (i32, i32) = (118, 236);

/// The autopilot buttons beside them: target the next body, fly to the locked one (and dock
/// at a station), stop.
pub const AUTOPILOT_BUTTONS: [(i32, i32, &str); 3] = [
    (154, 204, "TARGET"),
    (208, 268, "AUTOPILOT"),
    (272, 312, "STOP"),
];

/// The locked body as it shows in the view: its screen position, a pick radius in pixels,
/// and whether the autopilot is flying to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetMark {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    pub engaged: bool,
}

/// The map buttons along the bottom of the view: left edge, right edge and label.
pub const MAP_BUTTONS: [(i32, i32, &str); 2] = [(2, 62, "SYSTEMS"), (66, 150, "THIS SYSTEM")];
const MAP_BUTTONS_Y: (i32, i32) = (128, 138);

pub struct Screen {
    pub pixels: Vec<u32>,
    frame: Vec<u32>,
    cabin: Vec<u32>,
    panel: IndexedImage,
    font: BitmapFont,
    palette: [u32; 128],
    indexed: Box<[u8; 64000]>,
    stars: Vec<[f64; 3]>,
    /// The system's bodies as discs (set by the caller each frame).
    pub discs: Vec<Disc>,
    /// The system's stations (set by the caller each frame, in system flight).
    pub stations: Vec<SystemStation>,
    /// Direction toward the local star in flight space (`None` outside a system):
    /// ships, stations and lift objects are lit by it instead of the original's
    /// fixed default light. Planet discs carry their own light.
    pub sun: Option<[f64; 3]>,
    /// The market the docked screen shows; the prototype's one-item screen without it.
    pub market: Option<MarketView>,
    /// The lift scene to draw this frame, in a docking or launching sequence.
    pub lift: Option<LiftView>,
    /// The view is from inside a station (a launch before the ship is in
    /// flight): the port's own status block and map buttons are left out.
    pub station_view: bool,
    /// The system and the station the player is at, for the docked screen and the top bar.
    pub system: String,
    pub station: String,
    /// The locked-on target and its range, while the autopilot has one.
    pub lock: String,
    /// Where the locked body shows in the view (set by the caller each frame).
    pub target_mark: Option<TargetMark>,
    /// The galaxy map to draw instead of the flight view, when it is open.
    pub galaxy: Option<MapScene>,
    /// Whether a body is locked, for the autopilot buttons.
    pub has_lock: bool,
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
        screen.icons = (0..=0xfdu32)
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
        match crate::console::click(x, y)? {
            crate::console::Click::Front => Some(ViewButton::Select(ViewMode::Front)),
            crate::console::Click::Rear => Some(ViewButton::Select(ViewMode::Rear)),
            crate::console::Click::External => Some(ViewButton::Select(ViewMode::External)),
            crate::console::Click::Turret => Some(ViewButton::Turret),
            crate::console::Click::CycleView => Some(ViewButton::Cycle),
            _ => None,
        }
    }

    /// Any console click under logical pixel (x, y): the views, the time
    /// acceleration meter and the buttons whose screens are not ported.
    pub fn console_click(x: i32, y: i32) -> Option<crate::console::Click> {
        use crate::console::Click;
        if (MAP_BUTTONS_Y.0..=MAP_BUTTONS_Y.1).contains(&y) {
            if (MAP_BUTTONS[0].0..=MAP_BUTTONS[0].1).contains(&x) {
                return Some(Click::Screen(3));
            }
            if (MAP_BUTTONS[1].0..=MAP_BUTTONS[1].1).contains(&x) {
                return Some(Click::Navigation);
            }
        }
        if (MAP_BUTTONS_Y.0..=MAP_BUTTONS_Y.1).contains(&y) {
            for ((x0, x1, _), click) in AUTOPILOT_BUTTONS.into_iter().zip([
                Click::Target,
                Click::Autopilot,
                Click::AutopilotOff,
            ]) {
                if (x0..=x1).contains(&x) {
                    return Some(click);
                }
            }
        }
        crate::console::click(x, y)
    }

    /// The body under logical pixel (x, y): of `bodies` (an index, its centre and radius in
    /// flight units), the one whose screen position is nearest the click and within its
    /// pick radius.
    pub fn pick(
        state: &Flight,
        views: &ViewState,
        scene: Option<&ModelViewer>,
        bodies: &[(usize, [f64; 3], f64)],
        (x, y): (i32, i32),
    ) -> Option<usize> {
        bodies
            .iter()
            .filter_map(|(index, centre, radius)| {
                let (sx, sy, reach) = Self::body_screen(state, views, scene, *centre, *radius)?;
                let miss = f64::from((sx - x).pow(2) + (sy - y).pow(2)).sqrt();
                (miss <= f64::from(reach)).then_some((*index, miss))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    /// Where a body shows in the view `state` and `views` give: its screen position and a
    /// pick radius in pixels (the body's own size, at least 10), when it is in front of
    /// the eye and on the view. `centre` and `radius` are in flight units.
    pub fn body_screen(
        state: &Flight,
        views: &ViewState,
        scene: Option<&ModelViewer>,
        centre: [f64; 3],
        radius: f64,
    ) -> Option<(i32, i32, i32)> {
        let stats = scene.and_then(|scene| scene.ship_stats(state.ship));
        let scale = scene.and_then(model_scale).unwrap_or(DEFAULT_MODEL_SCALE);
        let camera = Camera::new(state, views, stats.as_ref(), scale);
        let [x, y, z] = camera.point(centre);
        if z <= 2.0 {
            return None;
        }
        let (sx, sy) = (160.0 + 256.0 * x / z, 79.0 - 256.0 * y / z);
        let pixels = (radius * 256.0 / z).clamp(10.0, 120.0);
        ((-20.0..340.0).contains(&sx) && (-20.0..178.0).contains(&sy)).then_some((
            sx as i32,
            sy as i32,
            pixels as i32,
        ))
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
            discs: Vec::new(),
            stations: Vec::new(),
            sun: None,
            market: None,
            lift: None,
            station_view: false,
            target_mark: None,
            galaxy: None,
            has_lock: false,
            system: String::new(),
            station: String::new(),
            lock: String::new(),
            icons: Vec::new(),
            jitter: Jitter::default(),
        })
    }

    fn pixel(&mut self, x: i32, y: i32, colour: u8) {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            self.frame[y as usize * 320 + x as usize] = self.palette[usize::from(colour)];
        }
    }

    /// Planets and moons as shaded discs, the farthest first (a stand-in for the
    /// surface renderer: size, position and phase are right, the surface is not).
    fn draw_discs(&mut self, camera: &Camera, viewer: Option<&ModelViewer>) {
        let mut order: Vec<(f64, Disc)> = self
            .discs
            .iter()
            .map(|d| (camera.point(d.centre)[2], *d))
            .filter(|(z, d)| *z > d.radius.max(2.0))
            .collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (z, disc) in order {
            // The real surface when the model fits the view space at this range.
            if let (Some((model, seed)), Some(viewer)) = (disc.model, viewer) {
                // The planet's radius in the model's units is its surface axes' length;
                // one render at the origin reads it (the model's vertices reach farther).
                if viewer.find(model).is_some() {
                    viewer.surface_extent.set(0.0);
                    let unit = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                    let _ = viewer.render_scene(model, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
                    let radius = viewer.surface_extent.get();
                    if radius > 0.0 {
                        let factor = radius / disc.radius;
                        let mut position = camera.point(disc.centre).map(|v| v * factor);
                        viewer.surface_seed.set(seed);
                        let light = camera.direction(disc.light);
                        // The view space is 32-bit: a distant planet is drawn through a
                        // matrix shrunk by a power of two (the position shrinks with it),
                        // which leaves the picture as it was.
                        let reach = position.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                        // The terrain's 32-bit arithmetic keeps the planet's radius and distance well
                        // inside the range the original runs it in (radii of 2^20 to 2^26).
                        let shrink = (reach / 1.0e9)
                            .max(radius / 16_777_216.0)
                            .max(1.0)
                            .log2()
                            .ceil()
                            .min(16.0);
                        let shrunk = 0.5_f64.powf(shrink);
                        position = position.map(|v| v * shrunk);
                        let rows = disc
                            .orientation
                            .map(|axis| camera.direction(axis).map(|v| v * shrunk));
                        if let Ok(frame) = viewer.render_scene(model, rows, position, light) {
                            self.draw_shapes(viewer, &frame.shapes);
                            continue;
                        }
                    }
                }
            }
            let [cx, cy, _] = camera.point(disc.centre);
            let scale = 256.0 / z;
            let radius = disc.radius * scale;
            if radius < 0.6 {
                self.pixel(
                    (160.0 + cx * scale) as i32,
                    (79.0 - cy * scale) as i32,
                    disc.colours.0,
                );
                continue;
            }
            let light = camera.direction(disc.light);
            let (x0, y0) = (160.0 + cx * scale, 79.0 - cy * scale);
            let top = (y0 - radius).floor().max(0.0) as i32;
            let bottom = (y0 + radius).ceil().min(157.0) as i32;
            for y in top..=bottom {
                let dy = (f64::from(y) + 0.5 - y0) / radius;
                if dy.abs() > 1.0 {
                    continue;
                }
                let half = (1.0 - dy * dy).sqrt() * radius;
                let left = (x0 - half).round().max(0.0) as i32;
                let right = (x0 + half).round().min(319.0) as i32;
                for x in left..=right {
                    let dx = (f64::from(x) + 0.5 - x0) / radius;
                    let depth = (1.0 - dx * dx - dy * dy).max(0.0).sqrt();
                    // The sphere normal in view space (y is up on screen).
                    let lit = dx * light[0] - dy * light[1] + depth * light[2];
                    let colour = if disc.star || lit > 0.0 {
                        disc.colours.0
                    } else {
                        disc.colours.1
                    };
                    self.pixel(x, y, colour);
                }
            }
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
    /// A triangle with the colour blended from its corners (barycentric), clipped to
    /// the view.
    fn gouraud(&mut self, p: [(i32, i32); 3], c: [[u8; 3]; 3]) {
        let (x0, y0) = (f64::from(p[0].0), f64::from(p[0].1));
        let area = (f64::from(p[1].0) - x0) * (f64::from(p[2].1) - y0)
            - (f64::from(p[2].0) - x0) * (f64::from(p[1].1) - y0);
        if area == 0.0 {
            return;
        }
        let top = p.iter().map(|q| q.1).min().unwrap_or(0).max(0);
        let bottom = p
            .iter()
            .map(|q| q.1)
            .max()
            .unwrap_or(0)
            .min(VIEW_HEIGHT as i32 - 1);
        let left = p.iter().map(|q| q.0).min().unwrap_or(0).max(0);
        let right = p.iter().map(|q| q.0).max().unwrap_or(0).min(319);
        for y in top..=bottom {
            for x in left..=right {
                let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
                let edge = |a: (i32, i32), b: (i32, i32)| {
                    (f64::from(b.0) - f64::from(a.0)) * (py - f64::from(a.1))
                        - (f64::from(b.1) - f64::from(a.1)) * (px - f64::from(a.0))
                };
                let w = [
                    edge(p[1], p[2]) / area,
                    edge(p[2], p[0]) / area,
                    edge(p[0], p[1]) / area,
                ];
                if w.iter().any(|v| *v < -1e-9) {
                    continue;
                }
                let channel = |k: usize| {
                    (w[0] * f64::from(c[0][k])
                        + w[1] * f64::from(c[1][k])
                        + w[2] * f64::from(c[2][k]))
                    .round()
                    .clamp(0.0, 255.0) as u32
                };
                self.frame[y as usize * 320 + x as usize] =
                    channel(0) << 16 | channel(1) << 8 | channel(2);
            }
        }
    }

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
            // The screen buttons (0x0b-0x0e): the views button is lit (F1).
            for (x, y, off, on, key) in crate::console::SCREEN_BUTTONS {
                let id = if key == 1 { on } else { off };
                if let Some(Some(icon)) = self.icons.get(usize::from(id)) {
                    icon.blit(&mut self.indexed, x, y);
                }
            }
            // The time acceleration meter (0x26-0x2b): one lit step, a radio group.
            let lit = views.console.lit_step();
            for (step, (x, y, grey, red)) in crate::console::BAR.into_iter().enumerate() {
                let id = if step == lit { red } else { grey };
                if let Some(Some(icon)) = self.icons.get(usize::from(id)) {
                    icon.blit(&mut self.indexed, x, y);
                }
            }
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
        if let Some(map) = self.galaxy.take().filter(|_| scene.is_some()) {
            self.draw_galaxy(scene.unwrap(), &map);
            self.present();
            return;
        }
        if state.mode == Mode::Docked && self.market.is_some() {
            self.frame[..320 * 158].copy_from_slice(&self.cabin);
            self.draw_market();
        } else if state.mode == Mode::Docked {
            self.frame[..320 * 158].copy_from_slice(&self.cabin);
            self.rect(24, 24, 272, 92, 1);
            self.text(
                36,
                30,
                &format!("{} STATION", self.station.to_ascii_uppercase()),
                18,
            );
            self.text(
                36,
                44,
                &format!("DOCKED IN THE {} SYSTEM", self.system.to_ascii_uppercase()),
                15,
            );
            self.text(
                36,
                59,
                &format!("FOOD: BUY 10  SELL 9  STOCK {}", state.stock),
                17,
            );
            self.text(36, 73, "B BUY ONE     N SELL ONE", 15);
            self.text(36, 87, "L LAUNCH  F5 SAVE  SHIFT+F5 LOAD", 15);
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
            let world = self.lift.as_ref().is_none_or(|lift| lift.world);
            for index in 0..self.stars.len() {
                if !world {
                    break;
                }
                if let Some((x, y)) = camera.project(self.stars[index], false) {
                    if y < 158 {
                        self.pixel(x, y, 12);
                    }
                }
            }
            if world {
                self.draw_discs(&camera, scene);
            }
            // FUNC_001054 hides the player's own ship only in the front view.
            // Painter's order between the two objects: the farther one first.
            let own_ship = view != ViewMode::Front;
            if state.system_flight && world {
                if let Some(scene) = scene {
                    self.draw_system_stations(scene, state, &camera, own_ship);
                }
            }
            let ship_first = own_ship
                && world
                && !state.system_flight
                && distance(camera.position, state.position) > distance(camera.position, [0.0; 3]);
            if ship_first {
                if let Some(scene) = scene {
                    self.draw_ship(scene, state, &camera);
                }
            }
            // In a system the prototype's station at the origin is not there.
            let drawn = !world
                || state.system_flight
                || scene.is_some_and(|scene| self.draw_station(scene, state, &camera));
            if !ship_first && own_ship && world && !state.system_flight {
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
            if let (Some(lift), Some(scene)) = (self.lift.take(), scene) {
                self.draw_lift(scene, &lift, &camera, stats.map_or([0, 0], |s| s.eye));
                self.lift = Some(lift);
            }
            if state.system_flight {
            } else if let Some((x, y)) = camera.project([0.0; 3], true) {
                if x > 8 && x < 260 && y > 12 && y < 115 {
                    self.text(x as usize + 5, y as usize, "STATION", 17);
                }
            } else {
                self.text(8, 16, "STATION BEHIND / OFF VIEW", 18);
            }
            // The locked body: corner brackets around it, yellow while the autopilot is
            // flying to it.
            if let Some(mark) = self.target_mark {
                let (x, y, r) = (mark.x, mark.y, mark.radius.min(40));
                let colour = if mark.engaged { 18 } else { 17 };
                let arm = (r / 2).max(4);
                for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    let corner = (x + sx * r, y + sy * r);
                    self.line(corner, (corner.0 - sx * arm, corner.1), colour);
                    self.line(corner, (corner.0, corner.1 - sy * arm), colour);
                }
            }
            if view == ViewMode::Front {
                self.line((155, 79), (158, 79), 18);
                self.line((162, 79), (165, 79), 18);
                self.line((160, 74), (160, 77), 18);
                self.line((160, 81), (160, 84), 18);
            }
            // Prototype contact on the original scanner artwork, not original scanner math.
            if !state.system_flight {
                let relative = station_relative(state);
                let scale = state.distance().max(200.0);
                let sx = (153.0 + relative[0] / scale * 38.0) as i32;
                let sy = (188.0 - relative[2] / scale * 6.0) as i32;
                self.pixel(sx, sy, 17);
                self.pixel(sx - 1, sy, 17);
                self.pixel(sx + 1, sy, 17);
            }
        }
        self.rect(0, 0, 320, 11, 0);
        self.text(
            3,
            1,
            &format!("{} SYSTEM", self.system.to_ascii_uppercase()),
            18,
        );
        // The controls button: modern or classic, clicked to change.
        let label = if modern {
            "MODERN CONTROLS"
        } else {
            "CLASSIC CONTROLS"
        };
        self.rect(CONTROLS_BUTTON.0 as usize, 0, 118, 11, 1);
        self.text(CONTROLS_BUTTON.0 as usize + 3, 1, label, 17);
        if state.mode != Mode::Docked {
            // ffe-rs readout (not the original's): speed, set speed, time scale.
            let console = views.console;
            let mut line = format!("SPEED {} M/S", console.speed);
            if let Some(set) = console.set_speed {
                line.push_str(&format!("  SET {set} M/S"));
            }
            line.push_str(&format!(
                "  TIME {}",
                crate::console::ACCELERATION_NAMES[console.acceleration.min(4)]
            ));
            if console.paused {
                line.push_str("  PAUSED");
            }
            self.text(3, 13, &line, 15);
        }
        if state.mode != Mode::Docked {
            let name = view.name().to_ascii_uppercase();
            let width = self.font.width(&name).unwrap_or(0);
            self.text(317usize.saturating_sub(width), 1, &name, 15);
        }
        if !self.lock.is_empty() {
            let lock = self.lock.clone();
            self.text(3, 22, &lock, 17);
        }
        // The port's own status block is not the original's, and it would clear
        // the bottom 39 rows of a lift scene (the door closes over them) or of the
        // station view while a launch's outer door opens.
        if self.lift.is_none() && !self.station_view {
            self.rect(0, 119, 320, 39, 0);
            self.text(
                3,
                120,
                &format!(
                    "SPD {:5.1}  RNG {:7.1}  CR {}  CARGO {} T",
                    state.speed(),
                    state.distance(),
                    crate::trade::credits(i32::try_from(state.credits).unwrap_or(i32::MAX)),
                    state.cargo + state.hold.iter().map(|t| u32::from(*t)).sum::<u32>()
                ),
                15,
            );
            // The map buttons (the system list and this system's list), where the old dock
            // status line was.
            for (x0, x1, label) in MAP_BUTTONS {
                self.rect(
                    x0 as usize,
                    MAP_BUTTONS_Y.0 as usize,
                    (x1 - x0) as usize,
                    10,
                    1,
                );
                self.text(x0 as usize + 3, MAP_BUTTONS_Y.0 as usize + 1, label, 18);
            }
            if state.mode == Mode::Flight {
                for (index, (x0, x1, label)) in AUTOPILOT_BUTTONS.into_iter().enumerate() {
                    self.rect(
                        x0 as usize,
                        MAP_BUTTONS_Y.0 as usize,
                        (x1 - x0) as usize,
                        10,
                        1,
                    );
                    // Dim a button that has nothing to act on.
                    let live = match index {
                        1 => self.has_lock,
                        2 => self.target_mark.is_some_and(|mark| mark.engaged),
                        _ => true,
                    };
                    let engaged = index == 1 && self.target_mark.is_some_and(|mark| mark.engaged);
                    let colour = if engaged {
                        17
                    } else if live {
                        18
                    } else {
                        15
                    };
                    self.text(x0 as usize + 3, MAP_BUTTONS_Y.0 as usize + 1, label, colour);
                }
            }
        }
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
                "NATIVE RUST PLAYABLE SLICE - DOCKING IS FFE-RS RULES",
                "Original console, station and ship models.",
                "DEFAULT MODERN: W/S thrust; arrows yaw/pitch",
                "CLASSIC-LIKE: Enter/Shift thrust; ,/. yaw; A/Z pitch",
                "X brake  D clearance  L launch  B/N trade  PGUP/DN ship",
                "VIEWS: F1 cycle F6 front F7 rear F8 turret F9 external",
                "External: arrows turn, +/- zoom. Turret: arrows aim",
                "F12 models (Shift+F12 lab). F11 screenshot. P pause. Esc quit.",
                "F5 save snapshot; Shift+F5 load newest snapshot.",
                "Dock: D clearance (100 km), in the slot under 250 m/s",
                "O original flight: Enter/Shift speed, Z/A , . [ ] steer",
                "TAB system map. G galaxy map: arrows, PgUp/Dn, C, H jumps",
                "Click a body/T: lock. K: autopilot (docks). Bksp: stop.",
                "H closes help. Map icon: systems. Nav icon: this system.",
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

    /// The navigation map: a scrolling list at the left and a plot of the bodies
    /// around the ship (log scale, the ship at the centre) at the right.
    /// The list row of the map under logical pixel (x, y), as `draw_map` lays them out.
    pub fn map_row(selected: usize, count: usize, x: i32, y: i32) -> Option<usize> {
        let rows = 11usize;
        if !(10..=190).contains(&x) || y < 21 {
            return None;
        }
        let row = ((y - 21) / 11) as usize;
        let first = selected
            .saturating_sub(rows / 2)
            .min(count.saturating_sub(rows));
        (row < rows && first + row < count).then_some(first + row)
    }

    /// The row of the navigation list under logical pixel (x, y): the original's two
    /// columns of nine, 15 pixels apart from y 5, the second column from x 230.
    pub fn nav_row(x: i32, y: i32) -> Option<usize> {
        if !(5..140).contains(&y) || !(10..310).contains(&x) {
            return None;
        }
        let row = ((y - 5) / 15) as usize;
        let column = usize::from(x >= 230);
        (row < 9).then_some(column * 9 + row)
    }

    /// The navigation computer: a marker square and a name per body, nine rows in two
    /// columns; `rows` are (name, selected, locked on).
    pub fn draw_nav_list(&mut self, rows: &[(String, bool, bool)], detail: &str) {
        self.rect(0, 0, 320, 158, 1);
        for (k, (name, selected, locked)) in rows.iter().enumerate() {
            let (x, y) = (if k < 9 { 10 } else { 230 }, 5 + 15 * (k % 9));
            let colour = if *locked {
                17
            } else if *selected {
                18
            } else {
                12
            };
            self.rect(x, y, 10, 10, colour);
            let room = if k < 9 { 28 } else { 10 };
            let text: String = name.chars().take(room).collect();
            self.text(
                x + 15,
                y + 1,
                &text.to_ascii_uppercase(),
                if *selected { 18 } else { 15 },
            );
        }
        self.rect(0, 142, 320, 1, 15);
        self.text(5, 146, detail, 18);
    }

    pub fn draw_map(
        &mut self,
        title: &str,
        lines: &[String],
        selected: usize,
        points: &[(f32, f32, crate::navigation::Kind, bool)],
        status: &str,
    ) {
        use crate::navigation::Kind;
        self.rect(4, 4, 312, 152, 1);
        self.text(10, 8, title, 18);
        let rows = 11usize;
        let first = selected
            .saturating_sub(rows / 2)
            .min(lines.len().saturating_sub(rows));
        for (row, line) in lines.iter().skip(first).take(rows).enumerate() {
            let colour = if first + row == selected { 18 } else { 15 };
            let clipped: String = line.chars().take(30).collect();
            self.text(10, 22 + row * 11, &clipped, colour);
        }
        // The plot: a frame, the ship at the centre and a dot per body.
        let (cx, cy, half) = (244usize, 80usize, 66.0f32);
        self.rect(cx - 68, cy - 68, 136, 1, 15);
        self.rect(cx - 68, cy + 67, 136, 1, 15);
        self.rect(cx - 68, cy - 68, 1, 136, 15);
        self.rect(cx + 67, cy - 68, 1, 136, 15);
        self.rect(cx - 1, cy - 1, 3, 3, 12);
        for (x, y, kind, chosen) in points {
            let colour = if *chosen {
                18
            } else {
                match kind {
                    Kind::Star => 14,
                    Kind::Planet => 10,
                    Kind::Moon => 9,
                    Kind::Station | Kind::Starport => 13,
                }
            };
            let size = if matches!(kind, Kind::Star) { 3 } else { 2 };
            let px = (cx as f32 + x * half).round() as usize;
            let py = (cy as f32 + y * half).round() as usize;
            self.rect(
                px.saturating_sub(1),
                py.saturating_sub(1),
                size,
                size,
                colour,
            );
        }
        self.text(10, 146, status, 15);
        self.present();
    }

    /// F6: every imported model executed by the verified port, in original order.
    /// Draw executed-model shapes in order into the 320x158 view.
    fn draw_shapes(&mut self, viewer: &ModelViewer, shapes: &[Shape]) {
        let value = |[r, g, b]: [u8; 3]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        for shape in shapes {
            match shape {
                Shape::Fill(points, rgb) => self.fill_value(points, value(*rgb)),
                Shape::Gouraud(points, colours) => {
                    if let ([a, b, c], [ca, cb, cc]) = (&points[..], &colours[..]) {
                        self.gouraud([*a, *b, *c], [*ca, *cb, *cc]);
                    }
                }
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
                Shape::Spans(spans, rgb) => {
                    let frame = &mut self.frame;
                    for [x, y, width] in spans {
                        if (0..VIEW_HEIGHT as i32).contains(y) {
                            for column in *x..*x + *width {
                                if (0..320).contains(&column) {
                                    frame[*y as usize * 320 + column as usize] = value(*rgb);
                                }
                            }
                        }
                    }
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

    /// The market screen: the station, cash and hold, then the commodity list with the
    /// selected row lit (a window of [`MARKET_ROWS`] around it).
    fn draw_market(&mut self) {
        let Some(view) = self.market.take() else {
            return;
        };
        self.rect(6, 13, 308, 106, 1);
        self.text(
            12,
            15,
            &format!(
                "{} STATION, {} SYSTEM",
                view.station.to_ascii_uppercase(),
                view.system.to_ascii_uppercase()
            ),
            18,
        );
        self.text(
            12,
            25,
            &format!(
                "CASH {} CR     HOLD {}/{} T",
                crate::trade::credits(view.cash),
                view.used,
                view.capacity
            ),
            17,
        );
        self.text(12, 36, "COMMODITY", 12);
        self.text(150, 36, "PRICE", 12);
        self.text(208, 36, "STOCK", 12);
        self.text(262, 36, "HELD", 12);
        let first = view
            .selected
            .saturating_sub(MARKET_ROWS / 2)
            .min(view.rows.len().saturating_sub(MARKET_ROWS));
        for (k, row) in view.rows.iter().enumerate().skip(first).take(MARKET_ROWS) {
            let y = 46 + 9 * (k - first);
            let lit = k == view.selected;
            if lit {
                self.rect(8, y, 304, 9, 4);
            }
            let colour = if lit { 18 } else { 15 };
            self.text(12, y, &row.name.to_ascii_uppercase(), colour);
            self.text(150, y, &crate::trade::credits(row.price), colour);
            self.text(208, y, &row.stock.to_string(), colour);
            if row.held > 0 {
                self.text(262, y, &row.held.to_string(), 17);
            }
        }
        if view.rows.len() > MARKET_ROWS {
            self.text(
                250,
                15,
                &format!("{}/{}", view.selected + 1, view.rows.len()),
                12,
            );
        }
        self.market = Some(view);
    }

    /// The station as the original model 74, executed by the verified port and
    /// placed by the synthetic flight camera. False when it cannot be drawn.
    /// The light ships, stations and lift objects are drawn with: the local
    /// star's direction in system flight, else the original's fixed F960 light.
    fn scene_light(&self, scene: &ModelViewer, camera: &Camera) -> [f64; 3] {
        match self.sun {
            // `sun` points toward the star; the original's light vector (F1535,
            // F1481's `dot < 0` test) points from the star toward the observer.
            Some(direction) => camera.direction(direction.map(|value| -value)),
            None => camera.direction(scene.light_direction()),
        }
    }

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
        let light = self.scene_light(scene, camera);
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

    /// The lift models of a docking or launching stage, the farthest first, in view space.
    fn draw_lift(&mut self, scene: &ModelViewer, lift: &LiftView, camera: &Camera, eye: [i16; 2]) {
        let mut objects = lift.objects.clone();
        objects.sort_by_key(|object| std::cmp::Reverse(object.depth));
        // The lift is inside the station: the original's fixed default light,
        // not the local star (which leaves the door faces black as it closes).
        let light = camera.direction(scene.light_direction());
        for object in objects {
            let position = ffe_core::lift::view_position(&object, eye).map(f64::from);
            let mut inputs = AnimationInputs::original(lift.tics, object.word, &scene.shades());
            inputs.jitter = self.jitter.cursor;
            inputs.globals[2] = lift.seed as u16;
            inputs.globals[3] = (lift.seed >> 16) as u16;
            inputs.globals[4] = u16::from(!lift.starport);
            if let Ok(frame) = scene.render_scene_with(
                object.model,
                scene.lift_rows(object.angle),
                position,
                light,
                &inputs,
            ) {
                self.jitter.cursor = frame.jitter;
                self.draw_shapes(scene, &frame.shapes);
            } else {
                // A missing lift model is an empty (black) picture, not an error
                // the caller can see; say so once per frame.
                eprintln!("lift model {:#x} not drawn", object.model);
            }
        }
    }

    /// The system's stations, the farthest first, with the player's own ship (when
    /// it shows) in painter's order among them.
    fn draw_system_stations(
        &mut self,
        scene: &ModelViewer,
        state: &Flight,
        camera: &Camera,
        own_ship: bool,
    ) {
        let mut order: Vec<(f64, SystemStation)> = self
            .stations
            .iter()
            .map(|s| (distance(camera.position, s.centre), *s))
            .collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));
        let ship_range = distance(camera.position, state.position);
        let mut ship_drawn = !own_ship;
        for (range, station) in order {
            if !ship_drawn && range < ship_range {
                self.draw_ship(scene, state, camera);
                ship_drawn = true;
            }
            self.draw_system_station(scene, &station, camera);
        }
        if !ship_drawn {
            self.draw_ship(scene, state, camera);
        }
    }

    /// One station of the system: F866's spin about its z (docking) axis applied to
    /// the object's rows, lit from the system's star at the origin.
    fn draw_system_station(
        &mut self,
        scene: &ModelViewer,
        station: &SystemStation,
        camera: &Camera,
    ) -> bool {
        let Some(scale) = model_scale(scene) else {
            return false;
        };
        let origin = camera.point(station.centre).map(|v| v * scale);
        if origin.iter().any(|v| v.abs() > STATION_DRAW_RANGE) {
            return false;
        }
        let spin = scene.station_rows(station.tics, station.speed);
        let axes: [[f64; 3]; 3] = std::array::from_fn(|k| {
            std::array::from_fn(|i| (0..3).map(|j| spin[k][j] * station.orientation[j][i]).sum())
        });
        let rows = axes.map(|axis| camera.direction(axis));
        // One frame-wide light from the local star (F1535), not a per-station
        // direction; the sign convention is `scene_light`'s.
        let light = self.scene_light(scene, camera);
        let mut inputs = AnimationInputs::original(station.tics, station.door, &scene.shades());
        inputs.jitter = self.jitter.cursor;
        match scene.render_scene_with(station.model, rows, origin, light, &inputs) {
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
        let light = self.scene_light(scene, camera);
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

    /// The galaxy map (F840, F828, F863): the cabin art behind the scene, the sector and fuel
    /// lines with a shadow at the lower left, and the date and the star's text in the console.
    fn draw_galaxy(&mut self, viewer: &ModelViewer, map: &MapScene) {
        self.frame[..320 * VIEW_HEIGHT].copy_from_slice(&self.cabin[..320 * VIEW_HEIGHT]);
        self.draw_shapes(viewer, &map.shapes);
        let shadowed = |screen: &mut Self, x: usize, y: usize, text: &str| {
            screen.text(x + 1, y + 1, text, 0);
            screen.text(x, y, text, 15);
        };
        shadowed(self, 6, 147, &map.sector);
        if let Some(fuel) = &map.fuel {
            shadowed(self, 6, 137, fuel);
        }
        // The console text: the date, then the description wrapped to 120 pixels.
        self.text(195, 164, &map.date, 17);
        let mut y = 173;
        for paragraph in map.info.split('\r') {
            let mut line = String::new();
            for word in paragraph.split(' ') {
                let trial = if line.is_empty() {
                    word.to_owned()
                } else {
                    format!("{line} {word}")
                };
                if self.font.width(&trial).is_ok_and(|width| width > 120) && !line.is_empty() {
                    self.text(195, y, &line, 15);
                    y += 8;
                    line = word.to_owned();
                } else {
                    line = trial;
                }
            }
            self.text(195, y, &line, 15);
            y += 8;
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
                "MODEL VIEWER {}/{}  MODEL {}  F12 BACK",
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
        self.text(3, 148, "SHIFT+F12 RETURN - NO FLIGHT OR SAVE CHANGES", 18);
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
