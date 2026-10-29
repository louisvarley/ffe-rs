//! The modern engine's 3-D view: the flight view's models drawn by the GPU at the window's
//! resolution (`ffe_render`), the way FFED3D hands the original's model programs to Direct3D,
//! but from the port's own executor results and with wgpu.
//!
//! In Modern graphics the flight view runs the original drawing as always, except that each
//! model face the executor produced is set aside as a [`Solid`] (its view-space corners and the
//! original's flat colour) instead of being rasterised into the 320x200 frame. The frame then
//! holds everything else: the space colour, stars, background nebula, planets and the effects,
//! and, after the world, the HUD. [`World`] carries the solids (one batch per object, in the
//! original's painter order) and a copy of the frame as the world phase left it.
//!
//! This module draws the frame behind the models (edge-upscaled), the models over it with 4x
//! multisampling and a depth buffer, and then puts back every pixel the game drew after the world
//! phase (the HUD, labels, gunsight, scanner contacts). The same view-space points give the same
//! picture as the original's projection (F1479: `160 + 256x/z`, `79 - 256y/z`), only finer.
//!
//! Nothing here reaches game state: the solids are copies made while drawing, and the Classic
//! engine never sets them aside. See `docs/notes/modern-graphics.md`.

use crate::display::Rect;
use crate::indexed::{LOGICAL_HEIGHT, LOGICAL_WIDTH};
use crate::model_viewer::{Shell, Solid};
use crate::modern::scale2x;
pub use crate::modern_art::WorldBitmap;

/// The flight view's rows; the console starts below them.
pub const VIEW_ROWS: usize = 158;
/// How many levels finer than the "Level of Shape Detail" setting the GPU view has the planets'
/// surfaces subdivided (presentation only: landing and collision use their own query). Each level
/// about doubles the triangles; three overflow the original's node arena on a whole planet.
pub const TERRAIN_DETAIL: i32 = 1;
/// Faces whose normals are closer than this (the cosine of 50 degrees) are shaded smoothly
/// across their shared corners; sharper edges stay sharp.
const SMOOTH_EDGE: f32 = 0.643;
/// How much brighter than its colour the fully saturated parts of an object that gives off light
/// are drawn (fire, a beam), so that they run past white and the bloom spreads them.
const EMISSIVE: f32 = 2.2;
/// The light an object keeps in a planet's full shadow (from the planet's day side and the sky),
/// as a share of its sunlit colour.
const SHADOW_AMBIENT: f32 = 0.3;
/// How many times the view's size the GPU draws the 3-D view at before filtering it down (on top
/// of its 4x multisampling): finer edges and texture detail for about twice the GPU work.
const SUPERSAMPLE: f32 = 1.5;
/// How strongly the brightest parts (the star, lights, sunlit white) glow: modest.
const BLOOM: f32 = 0.6;
/// How much the frame behind the models is enlarged before the GPU filters it.
const BACKGROUND_SCALE: usize = 2;
/// The nearest view depth the original projects (F1479 rejects `z < 64`).
const NEAR: f32 = 64.0;
/// A crisp HUD stroke's width, in original pixels: a little under one, so it reads as a fine line.
const HUD_LINE: f32 = 0.9;

/// What the flight view set aside for the modern engine this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// The drawn objects' faces, one batch per object in the order the original paints them.
    pub batches: Vec<Vec<Solid>>,
    /// The planets' atmospheres, each with the batch (its planet) it is drawn over.
    pub atmospheres: Vec<(usize, Shell)>,
    /// The light each batch was drawn in (view space, the way the light travels), by batch.
    pub lights: Vec<(usize, [i32; 3])>,
    /// How much of the star an object still sees past the planets (0..1), by batch, for the
    /// objects in a planet's shadow.
    pub eclipses: Vec<(usize, f32)>,
    /// The bodies' batches (star, planets, moons): [`Star::seen`] already counts how much of the
    /// star they hide.
    pub bodies: Vec<usize>,
    /// The star, when it is ahead, for its hot core, glare and lens flare.
    pub star: Option<Star>,
    /// The batches that give off light (beams, explosions, a hit's sparks).
    pub glowing: Vec<usize>,
    /// The batches on fire (explosions): their coloured faces burn.
    pub burning: Vec<usize>,
    /// The generated sky, in flight (open space; not inside a planet's haze).
    pub sky: Option<SkyView>,
    /// The textured faces' bitmaps (their original indices), each once.
    pub bitmaps: Vec<WorldBitmap>,
    /// The logical frame as the world phase left it (without the set-aside faces).
    pub snapshot: Vec<u32>,
    /// The speed dust's streaks, drawn over the world.
    pub dust: Vec<DustStreak>,
    /// HUD shapes drawn crisp over the 3-D view in place of their original pixels.
    pub hud: Vec<HudShape>,
    /// For each logical pixel, the value a crisp HUD shape put there (empty: none): while the
    /// frame still shows it, the 3-D view shows through instead.
    pub crisp: Vec<Option<u32>>,
}

/// One mote of speed dust: from where it is (`head`) to where it was (`tail`), in view space at
/// its own scale, how strongly it shows (`alpha`, above 1 when it streams past fast enough to
/// glow) and how near it is (`near`, 1 at the ship, 0 at the edge of its layer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DustStreak {
    pub head: [f32; 3],
    pub tail: [f32; 3],
    pub alpha: f32,
    pub near: f32,
}

/// The speed dust over the world: each streak a fine line, wider the nearer it is, blue-white and
/// brightest at the mote, fading to nothing along its trail (four steps). Over white at the mote,
/// a fast streak blooms.
fn dust(streaks: &[DustStreak], pixel: f32) -> ffe_render::Batch {
    const STEPS: usize = 4;
    let mut batch = ffe_render::Batch::default();
    for streak in streaks {
        let at = |t: f32| -> [f32; 3] {
            std::array::from_fn(|k| streak.head[k] + (streak.tail[k] - streak.head[k]) * t)
        };
        let width = pixel * (0.35 + 0.65 * streak.near);
        let bright = streak.alpha.max(0.0);
        for step in 0..STEPS {
            let (from, to) = (step as f32 / STEPS as f32, (step + 1) as f32 / STEPS as f32);
            let fade = 1.0 - from;
            let light = (bright * fade).min(1.6);
            batch.lines.push(ffe_render::Line {
                positions: [at(from), at(to)],
                colour: [0.82 * light, 0.88 * light, light, (bright * fade).min(1.0)],
                width,
            });
        }
    }
    batch
}

/// A HUD shape the modern engine draws as anti-aliased strokes at the output's resolution in place
/// of the original's pixels (the gunsight and the targetter's cursor): polylines in the logical
/// frame's coordinates (a pixel's centre at .5), in the frame's 4:3 shape.
#[derive(Clone, Debug, PartialEq)]
pub struct HudShape {
    pub rgb: [u8; 3],
    pub strokes: Vec<Vec<[f32; 2]>>,
}

impl HudShape {
    /// F1111's fixed gunsight, secondary bitmap 97 (61x61) with its top left at `at`: a ring of
    /// radius 28.8 about the bitmap's centre pixel, in four arcs 34 degrees long on the cross
    /// hairs and four 6-degree ones between them, and the cross hairs from 10 to 30 pixels out
    /// with three-pixel ticks 13, 18 and 23 out.
    pub fn gunsight(at: [f32; 2], rgb: [u8; 3]) -> Self {
        let centre = [at[0] + 30.5, at[1] + 30.5];
        let line = |a: [f32; 2], b: [f32; 2]| {
            vec![
                [centre[0] + a[0], centre[1] + a[1]],
                [centre[0] + b[0], centre[1] + b[1]],
            ]
        };
        let mut strokes = Vec::new();
        for sign in [-1.0f32, 1.0] {
            strokes.push(line([0.0, sign * 10.0], [0.0, sign * 30.0]));
            strokes.push(line([sign * 10.0, 0.0], [sign * 30.0, 0.0]));
            for out in [13.0, 18.0, 23.0] {
                strokes.push(line([-1.0, sign * out], [1.0, sign * out]));
                strokes.push(line([sign * out, -1.0], [sign * out, 1.0]));
            }
        }
        for k in 0..8 {
            let middle = k as f32 * 45.0;
            let half = if k % 2 == 0 { 17.0 } else { 3.0 };
            strokes.push(arc(centre, 28.8, middle - half, middle + half));
        }
        Self { rgb, strokes }
    }

    /// F1112's velocity marker, secondary bitmap 98 (35x35) with its top left at `at`: four corner
    /// arcs of radius 16.8 about the bitmap's centre pixel, each 56 degrees long on a diagonal, in
    /// `ring`, and on each diagonal a spoke from 7 to 18.4 pixels out in `spoke`.
    pub fn velocity_marker(at: [f32; 2], ring: [u8; 3], spoke: [u8; 3]) -> [Self; 2] {
        let centre = [at[0] + 17.5, at[1] + 17.5];
        let diagonals = [45.0f32, 135.0, 225.0, 315.0];
        let arcs = diagonals
            .iter()
            .map(|middle| arc(centre, 16.8, middle - 28.0, middle + 28.0))
            .collect();
        let spokes = diagonals
            .iter()
            .map(|angle| {
                let (sin, cos) = angle.to_radians().sin_cos();
                [7.0, 18.4]
                    .map(|out| [centre[0] + cos * out, centre[1] + sin * out])
                    .to_vec()
            })
            .collect();
        [
            Self {
                rgb: ring,
                strokes: arcs,
            },
            Self {
                rgb: spoke,
                strokes: spokes,
            },
        ]
    }

    /// A circle about a pixel's centre.
    pub fn circle(centre: (i32, i32), radius: f32, rgb: [u8; 3]) -> Self {
        let centre = [centre.0 as f32 + 0.5, centre.1 as f32 + 0.5];
        Self {
            rgb,
            strokes: vec![arc(centre, radius, 0.0, 360.0)],
        }
    }

    /// A square `half` pixels each way from a pixel's centre.
    pub fn square(centre: (i32, i32), half: i32, rgb: [u8; 3]) -> Self {
        let (x, y) = (centre.0 as f32 + 0.5, centre.1 as f32 + 0.5);
        let h = half as f32;
        Self {
            rgb,
            strokes: vec![vec![
                [x - h, y - h],
                [x + h, y - h],
                [x + h, y + h],
                [x - h, y + h],
                [x - h, y - h],
            ]],
        }
    }
}

/// An arc from `from` to `to` degrees (clockwise on the screen from the right), in two-degree steps.
fn arc(centre: [f32; 2], radius: f32, from: f32, to: f32) -> Vec<[f32; 2]> {
    let steps = ((to - from) / 2.0).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|k| {
            let angle = (from + (to - from) * k as f32 / steps as f32).to_radians();
            [
                centre[0] + radius * angle.cos(),
                centre[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

/// Draw the HUD's crisp shapes into `rect` of `target` (`width` pixels a row), above the console:
/// each stroke a round-ended line [`HUD_LINE`] original pixels wide, its edges anti-aliased. A
/// shape's strokes are covered together, so where they cross the colour is laid down once.
fn draw_hud(shapes: &[HudShape], target: &mut [u32], width: usize, rect: Rect) {
    let (sx, sy) = (
        rect.width as f32 / LOGICAL_WIDTH as f32,
        rect.height as f32 / LOGICAL_HEIGHT as f32,
    );
    let half = (sx * HUD_LINE).max(1.0) * 0.5;
    let bottom = rect.y + rect.height * VIEW_ROWS / LOGICAL_HEIGHT;
    for shape in shapes {
        let strokes: Vec<Vec<[f32; 2]>> = shape
            .strokes
            .iter()
            .map(|stroke| {
                stroke
                    .iter()
                    .map(|p| [rect.x as f32 + p[0] * sx, rect.y as f32 + p[1] * sy])
                    .collect()
            })
            .collect();
        let mut low = [f32::MAX; 2];
        let mut high = [f32::MIN; 2];
        for p in strokes.iter().flatten() {
            for k in 0..2 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
            }
        }
        let reach = half + 1.0;
        let left = ((low[0] - reach).floor().max(rect.x as f32)) as usize;
        let top = ((low[1] - reach).floor().max(rect.y as f32)) as usize;
        let right = ((high[0] + reach).ceil().max(0.0) as usize).min(rect.x + rect.width);
        let lower = ((high[1] + reach).ceil().max(0.0) as usize).min(bottom);
        if left >= right || top >= lower {
            continue;
        }
        let across = right - left;
        let mut cover = vec![0.0f32; across * (lower - top)];
        for pair in strokes.iter().flat_map(|stroke| stroke.windows(2)) {
            let (a, b) = (pair[0], pair[1]);
            let x0 = ((a[0].min(b[0]) - reach).floor().max(left as f32)) as usize;
            let x1 = ((a[0].max(b[0]) + reach).ceil().max(0.0) as usize).min(right);
            let y0 = ((a[1].min(b[1]) - reach).floor().max(top as f32)) as usize;
            let y1 = ((a[1].max(b[1]) + reach).ceil().max(0.0) as usize).min(lower);
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0] * d[0] + d[1] * d[1];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = [x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]];
                    let t = if length > 0.0 {
                        ((p[0] * d[0] + p[1] * d[1]) / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let (ex, ey) = (p[0] - d[0] * t, p[1] - d[1] * t);
                    let coverage = (half + 0.5 - (ex * ex + ey * ey).sqrt()).clamp(0.0, 1.0);
                    let cell = &mut cover[(y - top) * across + x - left];
                    *cell = cell.max(coverage);
                }
            }
        }
        let rgb = shape.rgb.map(f32::from);
        for (row, line) in cover.chunks(across).enumerate() {
            let start = (top + row) * width + left;
            let Some(pixels) = target.get_mut(start..start + across) else {
                continue;
            };
            for (pixel, &c) in pixels.iter_mut().zip(line) {
                if c <= 0.0 {
                    continue;
                }
                let old = [(*pixel >> 16) & 0xff, (*pixel >> 8) & 0xff, *pixel & 0xff];
                let mixed: [u32; 3] =
                    std::array::from_fn(|k| (old[k] as f32 + (rgb[k] - old[k] as f32) * c) as u32);
                *pixel = (mixed[0] << 16) | (mixed[1] << 8) | mixed[2];
            }
        }
    }
}

/// The generated sky for one frame: each flight-space axis in view space (how the camera is
/// turned), the space colour the original fills the sky with (shown through the frame, and the
/// generated sky's base), and whether the stars are on (the "Background stars" option).
#[derive(Clone, Debug, PartialEq)]
pub struct SkyView {
    pub rows: [[f32; 3]; 3],
    pub key: u32,
    pub stars: bool,
}

/// The sky's seed: one sky for the whole galaxy, the same every time (not the game's random
/// numbers).
const SKY_SEED: f32 = 17.0;

/// The system's star as the eye sees it: its view-space direction, its radius in the original's
/// pixels, its colour, and how much of its disc the eye sees past the bodies (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct Star {
    pub direction: [f32; 3],
    pub radius: f32,
    pub rgb: [u8; 3],
    pub seen: f32,
}

/// The GPU, opened on first use. A machine without one keeps the classic picture: the game only
/// sets faces aside when [`Gpu3d::available`] says they will be drawn.
#[derive(Default)]
pub struct Gpu3d {
    renderer: Option<Result<ffe_render::Renderer, String>>,
    maps: MapCache,
    /// When the GPU view first drew, the clock its fire flickers by (presentation only).
    started: Option<std::time::Instant>,
    /// Each view row's and column's logical pixel, for the current view size.
    rows: Vec<usize>,
    columns: Vec<usize>,
}

/// The bitmaps' maps for the GPU, by bitmap id: the owner's upscaled art where the export has it
/// (read again when it changes), else the original indices enlarged with Scale2x.
#[derive(Default)]
pub struct MapCache {
    original: Vec<(u32, ffe_render::IndexMap)>,
    pub art: crate::modern_art::Art,
}

impl MapCache {
    fn map(&mut self, bitmap: &WorldBitmap) -> ffe_render::IndexMap {
        if let Some(map) = self.art.map(bitmap) {
            return map;
        }
        if let Some((_, map)) = self.original.iter().find(|(id, _)| *id == bitmap.bitmap) {
            return map.clone();
        }
        // Scale2x works on the indices as on colours: it only copies neighbours.
        let (mut width, mut height) = (bitmap.width as usize, bitmap.height as usize);
        let mut texels: Vec<u32> = bitmap
            .indices
            .iter()
            .map(|i| u32::from(*i) | 128 << 8)
            .collect();
        texels.resize(width * height, 128 << 8);
        while width < 256 && height < 1024 {
            texels = scale2x(&texels, width, height);
            (width, height) = (width * 2, height * 2);
        }
        let map = ffe_render::IndexMap {
            id: 1 << 40 | u64::from(bitmap.bitmap),
            width: width as u32,
            height: height as u32,
            texels: texels.iter().map(|t| *t as u16).collect(),
        };
        self.original.push((bitmap.bitmap, map.clone()));
        map
    }
}

impl Gpu3d {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the GPU opened (opening it on the first call).
    pub fn available(&mut self) -> bool {
        self.renderer().is_ok()
    }

    /// The adapter's name, or why there is none.
    pub fn describe(&mut self) -> String {
        match self.renderer() {
            Ok(renderer) => renderer.adapter.clone(),
            Err(error) => error.clone(),
        }
    }

    /// Read upscaled textures from the owner's image export at `dir` (`img-export`).
    pub fn set_art_dir(&mut self, dir: std::path::PathBuf) {
        self.maps.art.set_dir(dir);
    }

    /// What went wrong reading upscaled textures since the last call.
    pub fn take_art_problems(&mut self) -> Vec<String> {
        std::mem::take(&mut self.maps.art.problems)
    }

    fn renderer(&mut self) -> &mut Result<ffe_render::Renderer, String> {
        self.renderer.get_or_insert_with(ffe_render::Renderer::new)
    }

    /// Draw `world` into the view part of `rect` in `target` (`width` pixels a row), which
    /// already holds the upscaled `logical` frame, keeping every pixel the game drew after
    /// the world phase.
    pub fn render(
        &mut self,
        logical: &[u32],
        world: &World,
        target: &mut [u32],
        width: usize,
        rect: Rect,
    ) -> Result<(), String> {
        let frame = LOGICAL_WIDTH * LOGICAL_HEIGHT;
        if logical.len() < frame || world.snapshot.len() < frame || rect.width == 0 {
            return Ok(());
        }
        let (view_width, view_height) = (rect.width, rect.height * VIEW_ROWS / LOGICAL_HEIGHT);
        if view_height == 0 {
            return Ok(());
        }
        let time = self
            .started
            .get_or_insert_with(std::time::Instant::now)
            .elapsed()
            .as_secs_f32();
        let scene = scene(world, view_width, view_height, &mut self.maps, time);
        let Ok(renderer) = self.renderer() else {
            return Ok(());
        };
        let pixels = renderer.render(&scene)?;
        if self.rows.len() != view_height || self.columns.len() != view_width {
            self.rows = (0..view_height)
                .map(|dy| dy * VIEW_ROWS / view_height)
                .collect();
            self.columns = (0..view_width)
                .map(|dx| dx * LOGICAL_WIDTH / view_width)
                .collect();
        }
        for (dy, &y) in self.rows.iter().enumerate() {
            let row = (rect.y + dy) * width + rect.x;
            let Some(destination) = target.get_mut(row..row + view_width) else {
                continue;
            };
            let source = &pixels[dy * view_width..][..view_width];
            let line = y * LOGICAL_WIDTH;
            let (drawn, before) = (
                &logical[line..line + LOGICAL_WIDTH],
                &world.snapshot[line..line + LOGICAL_WIDTH],
            );
            let crisp = world.crisp.get(line..line + LOGICAL_WIDTH);
            // Unchanged since the world phase: the 3-D view shows here.
            if drawn == before && crisp.is_none() {
                destination.copy_from_slice(source);
                continue;
            }
            for ((pixel, &x), &shown) in destination.iter_mut().zip(&self.columns).zip(source) {
                if drawn[x] == before[x] || crisp.is_some_and(|crisp| crisp[x] == Some(drawn[x])) {
                    *pixel = shown;
                }
            }
        }
        draw_hud(&world.hud, target, width, rect);
        Ok(())
    }
}

/// A point of light for the GPU: the original's dot sizes (particle diameters) in this view's
/// pixels, a one-pixel dot (and a star) a little over half an original pixel across, so it stays
/// a point at any resolution.
fn light(
    sprite: &crate::model_viewer::Sprite,
    position: [f32; 3],
    colour: [f32; 3],
    pixel: f32,
) -> ffe_render::Sprite {
    let across = |size: i32| (size.max(0) as f32).max(0.6) * pixel;
    let (halo_size, halo) = match sprite.halo {
        Some((size, rgb)) => (across(size), rgb.map(|c| f32::from(c) / 255.0)),
        None => (0.0, [0.0; 3]),
    };
    ffe_render::Sprite {
        position,
        size: across(sprite.size).max(1.2),
        colour,
        halo_size,
        halo,
    }
}

/// The outward unit normal of a face (towards the eye, since the original only draws faces
/// that face it) from its first three corners.
fn normal(views: &[[i32; 3]]) -> Option<[f32; 3]> {
    let [a, b, c] = [views.first()?, views.get(1)?, views.get(2)?].map(|v| v.map(|c| c as f32));
    let (u, v) = (
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    );
    let mut n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = n.iter().map(|c| c * c).sum::<f32>().sqrt();
    // Also catches a degenerate face's NaN.
    if length.is_nan() || length <= 0.0 {
        return None;
    }
    n = n.map(|c| c / length);
    // Facing the eye at the origin: against the direction to the face.
    if n[0] * a[0] + n[1] * a[1] + n[2] * a[2] > 0.0 {
        n = n.map(|c| -c);
    }
    Some(n)
}

/// How much of the star a point sees past the bodies (0 none, 1 all). Everything is in one space
/// (camera space, flight units): the point, each body's centre and radius, and the star's. A body
/// between the point and the star covers the star's disc by how far their apparent discs overlap:
/// fully in the umbra, partly in the penumbra (in a straight line across it), and at most its own
/// share of the star's area when it looks smaller than the star.
pub fn sunlight(place: [f64; 3], bodies: &[([f64; 3], f64)], star: ([f64; 3], f64)) -> f32 {
    let toward = |at: [f64; 3]| -> ([f64; 3], f64) {
        let v = [at[0] - place[0], at[1] - place[1], at[2] - place[2]];
        let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
        (v, length)
    };
    let (to_star, star_distance) = toward(star.0);
    if star_distance <= star.1 {
        return 1.0;
    }
    let star_angle = (star.1 / star_distance).asin();
    let mut seen = 1.0f64;
    for &(centre, radius) in bodies {
        let (to_body, body_distance) = toward(centre);
        // Inside the body (landed, or the eye under the ground), or past the star: no shadow.
        if body_distance <= radius || body_distance >= star_distance {
            continue;
        }
        let body_angle = (radius / body_distance).asin();
        let cosine = (to_star
            .iter()
            .zip(&to_body)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / (star_distance * body_distance))
            .clamp(-1.0, 1.0);
        let apart = cosine.acos();
        let full = (body_angle / star_angle).powi(2).min(1.0);
        let covered = if apart >= body_angle + star_angle {
            0.0
        } else if apart <= (body_angle - star_angle).abs() {
            full
        } else {
            full * (body_angle + star_angle - apart) / (2.0 * body_angle.min(star_angle))
        };
        seen *= 1.0 - covered.clamp(0.0, 1.0);
    }
    seen as f32
}

/// The star's place on the screen (the projection's units, -1..1 across the view), and the view
/// space point a million units along its direction, when it is ahead and not far off the view.
fn star_screen(star: &Star, projection: &ffe_render::Projection) -> Option<[f32; 2]> {
    let [x, y, z] = star.direction;
    let screen = [x / z * projection.x_scale, y / z * projection.y_scale];
    (z > 0.0 && screen[0].abs() <= 1.6 && screen[1].abs() <= 1.6).then_some(screen)
}

/// A point a million view units out at screen position `s`.
fn screen_point(s: [f32; 2], projection: &ffe_render::Projection) -> [f32; 3] {
    [
        s[0] / projection.x_scale * 1.0e6,
        s[1] / projection.y_scale * 1.0e6,
        1.0e6,
    ]
}

/// The star's hot core: a glow over its disc four times brighter than white (which the bloom
/// spreads and the last pass turns white), dimmed by how much of it the planets hide. Drawn first,
/// so whatever stands in front of the star hides it.
fn core(star: &Star, pixel: f32, projection: &ffe_render::Projection) -> ffe_render::Batch {
    let mut batch = ffe_render::Batch::default();
    let seen = star.seen.clamp(0.0, 1.0);
    let Some(screen) = star_screen(star, projection).filter(|_| seen >= 0.02) else {
        return batch;
    };
    let tint = star.rgb.map(|c| f32::from(c) / 255.0);
    batch.sprites.push(ffe_render::Sprite {
        position: screen_point(screen, projection),
        size: (star.radius * 2.0 * pixel).max(3.0) * 1.1,
        colour: tint.map(|c| (2.0 + 2.0 * c) * seen),
        halo_size: 0.0,
        halo: [0.0; 3],
    });
    batch
}

/// How much of the star's disc the objects in front of it leave uncovered (0..1): rays to 13
/// points across its disc (its middle and two rings of six), each tested against the faces of
/// every batch but the bodies' (whose share [`Star::seen`] already counts). A batch whose
/// bounding sphere no ray passes near is skipped.
fn unblocked(world: &World, star: &Star) -> f32 {
    let length = |v: [f32; 3]| v.iter().map(|c| c * c).sum::<f32>().sqrt();
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let unit = |v: [f32; 3]| {
        let l = length(v).max(1.0e-12);
        v.map(|c| c / l)
    };
    let ahead = unit(star.direction);
    if ahead[2] <= 0.0 {
        return 1.0;
    }
    // The star's angular radius: its radius in the original's pixels over the 256-pixel focal
    // length.
    let angle = (star.radius / 256.0).max(0.001);
    let side = unit(cross(ahead, [0.0, 1.0, 0.0]));
    let up = cross(side, ahead);
    let mut rays = vec![ahead];
    for (ring, count) in [(0.5f32, 6), (0.9, 6)] {
        for k in 0..count {
            let turn = (k as f32 + ring) * std::f32::consts::TAU / count as f32;
            let (sin, cos) = turn.sin_cos();
            rays.push(unit(std::array::from_fn(|i| {
                ahead[i] + (side[i] * cos + up[i] * sin) * angle * ring
            })));
        }
    }
    let mut blocked = vec![false; rays.len()];
    for (index, solids) in world.batches.iter().enumerate() {
        if world.bodies.contains(&index) {
            continue;
        }
        let faces: Vec<Vec<[f32; 3]>> = solids
            .iter()
            .filter(|s| s.sprite.is_none() && s.flame.is_none() && !s.path && s.views.len() >= 3)
            .map(|s| s.views.iter().map(|v| v.map(|c| c as f32)).collect())
            .collect();
        let points = faces.iter().flatten();
        let count = faces.iter().map(Vec::len).sum::<usize>().max(1) as f32;
        let centre: [f32; 3] =
            std::array::from_fn(|i| points.clone().map(|p| p[i]).sum::<f32>() / count);
        let radius = points
            .map(|p| length(std::array::from_fn(|i| p[i] - centre[i])))
            .fold(0.0, f32::max);
        // The nearest any ray passes to the centre, against the radius and the rays' spread.
        let along = dot(centre, ahead);
        if along + radius <= 0.0 {
            continue;
        }
        let miss = length(std::array::from_fn(|i| centre[i] - ahead[i] * along));
        if miss > radius + along.abs() * angle * 1.5 {
            continue;
        }
        for (ray, hit) in rays.iter().zip(blocked.iter_mut()) {
            if *hit {
                continue;
            }
            *hit = faces.iter().any(|face| {
                face[1..].windows(2).any(|pair| {
                    // Moller-Trumbore from the eye along the ray.
                    let (a, b, c) = (face[0], pair[0], pair[1]);
                    let e1 = std::array::from_fn(|i| b[i] - a[i]);
                    let e2 = std::array::from_fn(|i| c[i] - a[i]);
                    let p = cross(*ray, e2);
                    let det = dot(e1, p);
                    if det.abs() < 1.0e-9 {
                        return false;
                    }
                    let inverse = 1.0 / det;
                    let s = a.map(|v| -v);
                    let u = dot(s, p) * inverse;
                    if !(0.0..=1.0).contains(&u) {
                        return false;
                    }
                    let q = cross(s, e1);
                    let v = dot(*ray, q) * inverse;
                    v >= 0.0 && u + v <= 1.0 && dot(e2, q) * inverse > 0.0
                })
            });
        }
        if blocked.iter().all(|&hit| hit) {
            return 0.0;
        }
    }
    blocked.iter().filter(|&&hit| !hit).count() as f32 / rays.len() as f32
}

/// The star's light seen through a lens, drawn over everything as a last batch of glows: a wide
/// faint glare and four ghosts on the line from the star through the middle of the view, dimmed
/// by how much of the star is seen. Nothing when it is hidden or far off the view.
fn flare(star: &Star, width: usize, projection: &ffe_render::Projection) -> ffe_render::Batch {
    let mut batch = ffe_render::Batch::default();
    let seen = star.seen.clamp(0.0, 1.0);
    let Some(screen) = star_screen(star, projection).filter(|_| seen >= 0.02) else {
        return batch;
    };
    let tint = star.rgb.map(|c| f32::from(c) / 255.0);
    let at = |s: [f32; 2]| screen_point(s, projection);
    let glow = |position, size: f32, colour: [f32; 3], halo: f32, halo_colour: [f32; 3]| {
        ffe_render::Sprite {
            position,
            size,
            colour: colour.map(|c| c * seen),
            halo_size: halo,
            halo: halo_colour.map(|c| c * seen),
        }
    };
    let wide = width as f32;
    batch.sprites.push(glow(
        at(screen),
        0.0,
        [0.0; 3],
        wide * 0.5,
        tint.map(|c| c * 0.18),
    ));
    let ghosts = [
        (0.4, 0.05, [1.0, 0.6, 0.3]),
        (-0.3, 0.09, [0.3, 0.8, 0.7]),
        (-0.7, 0.04, [0.7, 0.4, 1.0]),
        (-1.1, 0.16, [0.5, 0.6, 1.0]),
    ];
    for (t, size, colour) in ghosts {
        batch.sprites.push(glow(
            at([screen[0] * t, screen[1] * t]),
            0.0,
            [0.0; 3],
            wide * size,
            colour.map(|c| c * 0.12),
        ));
    }
    batch
}

/// How saturated a colour is (0 grey, 1 pure).
fn saturation([r, g, b]: [f32; 3]) -> f32 {
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    if high > 0.0 {
        (high - low) / high
    } else {
        0.0
    }
}

/// The mean of a bitmap's warm colours (its fire), if it has any.
fn warm(slots: &[[u8; 3]]) -> Option<[f32; 3]> {
    let warm: Vec<[f32; 3]> = slots
        .iter()
        .map(|slot| slot.map(|c| f32::from(c) / 255.0))
        .filter(|rgb| saturation(*rgb) > 0.3 && rgb[0] >= rgb[2])
        .collect();
    (!warm.is_empty()).then(|| {
        std::array::from_fn(|k| warm.iter().map(|rgb| rgb[k]).sum::<f32>() / warm.len() as f32)
    })
}

/// A burning face (view space) as a flame: a round fireball over its middle when it is about as
/// long as it is wide, else a streak with rounded ends along its longest stretch, the ends kept
/// inside the face.
fn fire_flame(views: &[[i32; 3]], rgb: [f32; 3]) -> ffe_render::Flame {
    let points: Vec<[f32; 3]> = views.iter().map(|v| v.map(|c| c as f32)).collect();
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let length = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let count = points.len().max(1) as f32;
    let middle: [f32; 3] =
        std::array::from_fn(|k| points.iter().map(|p| p[k]).sum::<f32>() / count);
    // The longest stretch: the corners farthest apart.
    let (mut a, mut b, mut long) = (middle, middle, 0.0f32);
    for (i, p) in points.iter().enumerate() {
        for q in &points[i + 1..] {
            let apart = length(sub(*q, *p));
            if apart > long {
                (a, b, long) = (*p, *q, apart);
            }
        }
    }
    // The width across it: the corners' spread off that line.
    let axis = if long > 0.0 {
        sub(b, a).map(|c| c / long)
    } else {
        [0.0; 3]
    };
    let off: Vec<[f32; 3]> = points
        .iter()
        .map(|p| {
            let v = sub(*p, a);
            let along = v[0] * axis[0] + v[1] * axis[1] + v[2] * axis[2];
            sub(v, axis.map(|c| c * along))
        })
        .collect();
    let mut wide = 0.0f32;
    for (i, p) in off.iter().enumerate() {
        for q in &off[i + 1..] {
            wide = wide.max(length(sub(*q, *p)));
        }
    }
    // Fire's own colours, from a deep red-orange edge to a yellow-white core that just blooms,
    // a quarter the face's hue. It has body: drawn over the debris and the other fireballs
    // behind it rather than added to them, so they never run together to white.
    let top = rgb.iter().copied().fold(1.0e-3, f32::max);
    let hue = rgb.map(|c| c / top);
    let tint = |fire: [f32; 3], k: f32| -> [f32; 3] {
        std::array::from_fn(|i| fire[i] * 0.75 + hue[i] * k * 0.25)
    };
    let edge = tint([0.55, 0.14, 0.02], 0.5);
    let core = tint([1.0, 0.68, 0.25], 1.0);
    let seed = (views[0][0] ^ views[0][1] ^ views[0][2]) as f32 * 0.001 % 97.0;
    if long < wide * 1.6 {
        let width = long.max(wide) * 0.9;
        ffe_render::Flame {
            a: middle,
            b: middle,
            width,
            end: width,
            core,
            edge,
            seed,
            body: true,
        }
    } else {
        let inset = axis.map(|c| c * wide * 0.5);
        ffe_render::Flame {
            a: [a[0] + inset[0], a[1] + inset[1], a[2] + inset[2]],
            b: sub(b, inset),
            width: wide,
            end: wide * 0.7,
            core,
            edge,
            seed,
            body: true,
        }
    }
}

/// A triangle of a filled outline: its corners in view space and their places across the outline.
type OutlineTriangle = ([[i32; 3]; 3], [[f32; 2]; 3]);

/// A complex polygon's loops (view space, one after another with `loops` their lengths) as
/// triangles on the polygon's plane, ear-clipped so concave shapes fill correctly (a loop that
/// crosses itself falls back to a fan), each corner with its place across the whole outline
/// (0..1 both ways, for fire).
fn fill_outline(views: &[[i32; 3]], loops: &[usize]) -> Vec<OutlineTriangle> {
    let points: Vec<[f64; 3]> = views.iter().map(|v| v.map(f64::from)).collect();
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // The plane's axes: its normal from the widest corner, and two directions in it.
    let mut normal = [0.0; 3];
    for window in points.windows(2) {
        let n = cross(sub(window[0], first), sub(window[1], first));
        if dot(n, n) > dot(normal, normal) {
            normal = n;
        }
    }
    let length = dot(normal, normal).sqrt();
    if length <= 0.0 {
        return Vec::new();
    }
    let normal = normal.map(|c| c / length);
    let helper = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let across = cross(normal, helper);
    let across_length = dot(across, across).sqrt();
    let across = across.map(|c| c / across_length);
    let up = cross(normal, across);
    let flat: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [dot(sub(*p, first), across), dot(sub(*p, first), up)])
        .collect();
    let (mut low, mut high) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &flat {
        for k in 0..2 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    let place = |p: [f64; 2]| {
        std::array::from_fn(|k| ((p[k] - low[k]) / (high[k] - low[k]).max(1.0e-9)) as f32)
    };
    let mut out = Vec::new();
    let mut start = 0;
    for &count in loops {
        let ring: Vec<usize> = (start..start + count).filter(|&i| i < flat.len()).collect();
        start += count;
        for [a, b, c] in ear_clip(&ring.iter().map(|&i| flat[i]).collect::<Vec<_>>()) {
            let corners = [ring[a], ring[b], ring[c]];
            out.push((corners.map(|i| views[i]), corners.map(|i| place(flat[i]))));
        }
    }
    out
}

/// Ear clipping of a simple polygon (2-D points in order): triangles as index triples. A polygon
/// it cannot clip (crossing itself) is finished as a fan.
fn ear_clip(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let turn = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    };
    let inside = |p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        let (x, y, z) = (turn(a, b, p), turn(b, c, p), turn(c, a, p));
        (x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0)
    };
    let mut ring: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    while ring.len() > 3 {
        let m = ring.len();
        let ear = (0..m).find(|&i| {
            let (a, b, c) = (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]);
            let bend = turn(points[a], points[b], points[c]);
            // Convex the same way as the whole polygon, and no other corner inside.
            bend * area > 0.0
                && ring
                    .iter()
                    .filter(|&&k| k != a && k != b && k != c)
                    .all(|&k| !inside(points[k], points[a], points[b], points[c]))
        });
        let Some(i) = ear else {
            break;
        };
        out.push([ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]]);
        ring.remove(i);
    }
    for k in 1..ring.len().saturating_sub(1) {
        out.push([ring[0], ring[k], ring[k + 1]]);
    }
    out
}

/// The luminance of a colour (Rec. 601 weights), 0..255.
fn luma([r, g, b]: [u8; 3]) -> f32 {
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

/// Smooth shading for one batch: for each face, a brightness factor at each corner that brings
/// it to the mean brightness of the faces meeting there that belong with it (normals within
/// [`SMOOTH_EDGE`], and the same texture bitmap, or for untextured faces the same hue). The
/// original lights each face flat, in coarse steps; evening the steps out at the corners turns a
/// faceted sphere or hull round, while a different material next door (sea against land, a
/// stripe on a hull) keeps its own colour.
fn smooth(solids: &[Solid]) -> Vec<Option<Vec<f32>>> {
    let face = |solid: &Solid| {
        (solid.sprite.is_none()
            && !solid.path
            && solid.outline.is_none()
            && solid.flame.is_none()
            && solid.views.len() >= 3)
            .then(|| normal(&solid.views))
            .flatten()
    };
    let normals: Vec<Option<[f32; 3]>> = solids.iter().map(face).collect();
    let bright: Vec<f32> = solids
        .iter()
        .map(|solid| luma(solid.rgb).max(1.0))
        .collect();
    let hue = |solid: &Solid| {
        let sum = solid
            .rgb
            .iter()
            .map(|c| f32::from(*c))
            .sum::<f32>()
            .max(1.0);
        solid.rgb.map(|c| f32::from(c) / sum)
    };
    // The same surface lit differently: the same bitmap (or both untextured), the same hue, and
    // a brightness step no bigger than the original's lighting makes.
    let alike = |a: usize, b: usize| {
        let (x, y) = (&solids[a], &solids[b]);
        let same = match (&x.texture, &y.texture) {
            (Some(p), Some(q)) => p.bitmap & 0xfff == q.bitmap & 0xfff,
            (None, None) => true,
            _ => false,
        };
        let (p, q) = (hue(x), hue(y));
        let ratio = bright[b] / bright[a];
        same && (0..3).map(|k| (p[k] - q[k]).abs()).sum::<f32>() < 0.08
            && (0.7..1.43).contains(&ratio)
    };
    let mut corners: std::collections::HashMap<[i32; 3], Vec<usize>> = Default::default();
    for (index, (solid, n)) in solids.iter().zip(&normals).enumerate() {
        if n.is_some() {
            for corner in &solid.views {
                corners.entry(*corner).or_default().push(index);
            }
        }
    }
    solids
        .iter()
        .zip(&normals)
        .enumerate()
        .map(|(index, (solid, n))| {
            let n = (*n)?;
            Some(
                solid
                    .views
                    .iter()
                    .map(|corner| {
                        let (mut sum, mut count) = (0.0f32, 0.0f32);
                        for &other in &corners[corner] {
                            let m = normals[other].unwrap_or(n);
                            if m[0] * n[0] + m[1] * n[1] + m[2] * n[2] >= SMOOTH_EDGE
                                && alike(index, other)
                            {
                                sum += bright[other];
                                count += 1.0;
                            }
                        }
                        (sum / count.max(1.0) / bright[index]).clamp(0.6, 1.6)
                    })
                    .collect(),
            )
        })
        .collect()
}

/// A planet's atmosphere for the GPU: the shell between the surface and F1877's ring radius,
/// in the haze colour, lit from the star (against the light vector).
fn atmosphere(shell: &Shell) -> ffe_render::Atmosphere {
    let light = shell.light.map(|c| c as f32);
    let length = light.iter().map(|c| c * c).sum::<f32>().sqrt().max(1.0);
    ffe_render::Atmosphere {
        centre: shell.centre.map(|c| c as f32),
        radius: shell.radius as f32,
        outer: shell.outer as f32,
        colour: shell.rgb.map(|c| f32::from(c) / 255.0),
        sun: light.map(|c| -c / length),
        density: 1.0,
    }
}

/// The GPU scene for a `width` by `height` view: the edge-upscaled frame behind, then the
/// batches in order. A texture is its bitmap's index map (upscaled art, or the original enlarged
/// with Scale2x) coloured per face from the face's slots, as the original colours it; the GPU
/// maps it with perspective correction, where the original maps it linearly across the screen.
pub fn scene(
    world: &World,
    width: usize,
    height: usize,
    cache: &mut MapCache,
    time: f32,
) -> ffe_render::Scene {
    // The frame behind the models: Scale2x once, then the GPU's smooth filtering.
    let mut upscaled = scale2x(
        &world.snapshot[..LOGICAL_WIDTH * VIEW_ROWS],
        LOGICAL_WIDTH,
        VIEW_ROWS,
    );
    upscaled.truncate(LOGICAL_WIDTH * BACKGROUND_SCALE * VIEW_ROWS * BACKGROUND_SCALE);
    let colour = |[r, g, b]: [u8; 3]| [r, g, b].map(|c| f32::from(c) / 255.0);
    // One of the original's pixels in this view's pixels.
    let pixel = width as f32 / LOGICAL_WIDTH as f32;
    let maps: Vec<ffe_render::IndexMap> = world.bitmaps.iter().map(|b| cache.map(b)).collect();
    // One palette per distinct set of slot colours: masked bitmaps make slot 0 a hole, and
    // indices past the slots are black, as the original's lookup gives them.
    let mut palettes: Vec<ffe_render::Palette> = Vec::new();
    let mut palette_of: Vec<(Vec<[u8; 3]>, bool, u32)> = Vec::new();
    let mut texture_of = |solid: &Solid| {
        let texture = solid.texture.as_ref()?;
        let map = world
            .bitmaps
            .iter()
            .position(|known| known.bitmap == texture.bitmap & 0xfff)?;
        let masked = world.bitmaps[map].masked;
        let palette = match palette_of
            .iter()
            .find(|(slots, hole, _)| *hole == masked && *slots == texture.slots)
        {
            Some((_, _, index)) => *index,
            None => {
                let mut palette = [[0.0, 0.0, 0.0, 1.0]; ffe_render::PALETTE_SIZE];
                for (entry, [r, g, b]) in palette.iter_mut().zip(&texture.slots) {
                    let [r, g, b] = [r, g, b].map(|c| f32::from(*c) / 255.0);
                    *entry = [r, g, b, 1.0];
                }
                if masked {
                    palette[0] = [0.0; 4];
                }
                palettes.push(palette);
                let index = palettes.len() as u32 - 1;
                palette_of.push((texture.slots.clone(), masked, index));
                index
            }
        };
        // Texel centres: the original's corner 0 and corner 63 (or 127) are whole texels.
        let (width, height) = (texture.width as f32, texture.height as f32);
        let uv: Vec<[f32; 2]> = texture
            .uv
            .iter()
            .map(|[u, v]| {
                [
                    (f32::from(*u) + 0.5) / width,
                    (f32::from(*v) + 0.5) / height,
                ]
            })
            .collect();
        (uv.len() == solid.views.len()).then_some((map as u32, palette, uv))
    };
    let mut batches: Vec<ffe_render::Batch> = world
        .batches
        .iter()
        .enumerate()
        .map(|(index, solids)| {
            let mut batch = ffe_render::Batch {
                atmospheres: world
                    .atmospheres
                    .iter()
                    .filter(|(over, _)| *over == index)
                    .map(|(_, shell)| atmosphere(shell))
                    .collect(),
                ..ffe_render::Batch::default()
            };
            let shading = smooth(solids);
            // An object in a planet's shadow keeps only part of its light.
            let shadow = world
                .eclipses
                .iter()
                .find(|(over, _)| *over == index)
                .map_or(1.0, |(_, seen)| {
                    SHADOW_AMBIENT + (1.0 - SHADOW_AMBIENT) * seen
                });
            // An object that gives off light is lit by itself, not the star.
            let glowing = world.glowing.contains(&index);
            let burning = world.burning.contains(&index);
            for (solid, shade) in solids.iter().zip(&shading) {
                let [r, g, b] = colour(solid.rgb);
                // In an object that gives off light, its coloured parts (fire, a beam) glow by
                // how saturated they are; grey debris stays as it is. It is lit by itself, so a
                // planet's shadow does not darken it.
                let glow = if glowing {
                    1.0 + (EMISSIVE - 1.0) * saturation([r, g, b])
                } else {
                    1.0
                };
                let shadow = if glowing { glow } else { shadow };
                let rgba = [r * shadow, g * shadow, b * shadow, 1.0];
                let point = |v: [i32; 3]| v.map(|c| c as f32);
                if let (Some(sprite), [at]) = (&solid.sprite, &solid.views[..]) {
                    batch.sprites.push(light(
                        sprite,
                        point(*at),
                        [r * glow, g * glow, b * glow],
                        pixel,
                    ));
                    continue;
                }
                // An engine plume: a flame from the nozzle.
                if let (Some(plume), [a, b]) = (&solid.flame, &solid.views[..]) {
                    let seed = (a[0] ^ a[1] ^ a[2]) as f32 * 0.001 % 97.0;
                    let warm = |c: [u8; 3], k: f32| c.map(|v| f32::from(v) / 255.0 * k);
                    // The original's quads include their dark soft edges: the flame's bright body
                    // is narrower (the round glow at the nozzle more so).
                    let round = a == b;
                    let width = plume.widths[0] as f32 * if round { 0.5 } else { 0.65 };
                    batch.flames.push(ffe_render::Flame {
                        a: point(*a),
                        b: point(*b),
                        width,
                        end: if round { width } else { plume.widths[1] as f32 },
                        core: warm(plume.core, 1.1),
                        edge: warm(plume.edge, 0.9),
                        seed,
                        body: false,
                    });
                    continue;
                }
                // A burning object's coloured faces and polygons are flames, and so are its
                // textured faces (the original's dithered fire-cloud bitmaps, whose holes would
                // show as square speckles), in their bitmap's warm colours: no straight edge or
                // texel of the original's shapes shows.
                if burning && solid.views.len() >= 3 {
                    let fire = match &solid.texture {
                        Some(texture) => warm(&texture.slots),
                        None => (saturation([r, g, b]) > 0.4).then_some([r, g, b]),
                    };
                    if let Some(fire) = fire {
                        batch.flames.push(fire_flame(&solid.views, fire));
                        continue;
                    }
                }
                // A complex polygon: each loop filled on its plane.
                if let Some(loops) = &solid.outline {
                    for (corners, _) in fill_outline(&solid.views, loops) {
                        batch.triangles.push(ffe_render::Triangle {
                            positions: corners.map(point),
                            colours: [rgba; 3],
                            texture: None,
                            fire: None,
                        });
                    }
                    continue;
                }
                // Lines and curves about one of the original's pixels wide.
                if solid.path || solid.views.len() == 2 {
                    for pair in solid.views.windows(2) {
                        batch.lines.push(ffe_render::Line {
                            positions: [point(pair[0]), point(pair[1])],
                            colour: rgba,
                            width: pixel,
                        });
                    }
                    continue;
                }
                // A face: a fan from its first corner. With smooth shading each corner's colour
                // (or, for a texture, its brightness) takes the corner's factor.
                if let [first, rest @ ..] = &solid.views[..] {
                    let texture = texture_of(solid);
                    let base = if texture.is_some() {
                        [shadow, shadow, shadow, 1.0]
                    } else {
                        rgba
                    };
                    let corner = |k: usize| match shade {
                        Some(factors) => {
                            let f = factors[k];
                            [base[0] * f, base[1] * f, base[2] * f, 1.0]
                        }
                        None => base,
                    };
                    for (k, pair) in rest.windows(2).enumerate() {
                        batch.triangles.push(ffe_render::Triangle {
                            positions: [point(*first), point(pair[0]), point(pair[1])],
                            colours: [corner(0), corner(k + 1), corner(k + 2)],
                            texture: texture.as_ref().map(|(map, palette, uv)| {
                                ffe_render::FaceTexture {
                                    map: *map,
                                    palette: *palette,
                                    uvs: [uv[0], uv[k + 1], uv[k + 2]],
                                }
                            }),
                            fire: None,
                        });
                    }
                }
            }
            batch
        })
        .collect();
    let projection = ffe_render::Projection {
        x_scale: 256.0 / 160.0,
        y_scale: 256.0 / 79.0,
        near: NEAR,
    };
    if !world.dust.is_empty() {
        batches.push(dust(&world.dust, pixel));
    }
    if let Some(star) = &world.star {
        // The star's hot core first, behind everything, so the ships and stations in front of
        // it hide it and the bloom spreads what is left round their edges; its glare and ghosts
        // last, over everything, as much as the objects in front leave of the star.
        batches.insert(0, core(star, pixel, &projection));
        let open = Star {
            seen: star.seen * unblocked(world, star),
            ..*star
        };
        batches.push(flare(&open, width, &projection));
    }
    ffe_render::Scene {
        width: width as u32,
        height: height as u32,
        background: Some(ffe_render::Image {
            width: (LOGICAL_WIDTH * BACKGROUND_SCALE) as u32,
            height: (VIEW_ROWS * BACKGROUND_SCALE) as u32,
            pixels: upscaled,
        }),
        // F1479: x lands at 160 + 256x/z of 320 columns, y at 79 - 256y/z of 158 rows.
        projection,
        batches,
        maps,
        palettes,
        bloom: BLOOM,
        time,
        sky: world.sky.as_ref().map(|sky| ffe_render::Sky {
            rows: sky.rows,
            key: sky.key,
            stars: sky.stars,
            seed: SKY_SEED,
        }),
        supersample: SUPERSAMPLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_planet_between_an_object_and_the_star_eclipses_it() {
        // The star 1000 away along +x, radius 10 (an angle of 0.01); a planet of radius 1 at 10
        // along +x (an angle of 0.1): far bigger than the star, so a full shadow behind it.
        let star = ([1000.0, 0.0, 0.0], 10.0);
        let planet = [([10.0, 0.0, 0.0], 1.0)];
        assert_eq!(sunlight([0.0; 3], &planet, star), 0.0);
        // Well off to the side: full sun.
        assert_eq!(sunlight([0.0, 5.0, 0.0], &planet, star), 1.0);
        // On the star's side of the planet: full sun.
        assert_eq!(sunlight([20.0, 0.0, 0.0], &planet, star), 1.0);
        // At the shadow's edge (the planet's limb against the star's centre): about half.
        let edge = sunlight([0.0, 1.005, 0.0], &planet, star);
        assert!(edge > 0.2 && edge < 0.8, "penumbra {edge}");
        // A small moon in front of the star covers only its share of the disc.
        let moon = [([500.0, 0.0, 0.0], 1.0)];
        let annular = sunlight([0.0; 3], &moon, star);
        assert!((annular - 0.96).abs() < 0.01, "annular {annular}");
    }

    /// The faces in front of the star hide it, ray by ray across its disc; a body's batch does
    /// not count (its share is [`Star::seen`]).
    #[test]
    fn objects_in_front_of_the_star_hide_it() {
        let face = |views: Vec<[i32; 3]>| Solid {
            views,
            rgb: [128; 3],
            texture: None,
            path: false,
            outline: None,
            flame: None,
            sprite: None,
        };
        // A star straight ahead, 20 of the original's pixels in radius (about 4.5 degrees).
        let star = Star {
            direction: [0.0, 0.0, 1.0],
            radius: 20.0,
            rgb: [255, 200, 120],
            seen: 1.0,
        };
        let square = |left: i32| {
            vec![
                face(vec![
                    [left, -500, 1000],
                    [500, -500, 1000],
                    [500, 500, 1000],
                ]),
                face(vec![
                    [left, -500, 1000],
                    [500, 500, 1000],
                    [left, 500, 1000],
                ]),
            ]
        };
        let mut world = World::default();
        assert_eq!(unblocked(&world, &star), 1.0, "nothing in front");
        world.batches = vec![square(-500)];
        assert_eq!(unblocked(&world, &star), 0.0, "a ship over the whole disc");
        // Covering only the right half of the disc (x from 0): the middle ray and the right-hand
        // ones are blocked.
        world.batches = vec![square(0)];
        let half = unblocked(&world, &star);
        assert!(half > 0.3 && half < 0.7, "half covered: {half}");
        // The same face as a body's batch: left to `seen`.
        world.batches = vec![square(-500)];
        world.bodies = vec![0];
        assert_eq!(unblocked(&world, &star), 1.0);
        // Behind the eye, nothing is hidden.
        world.bodies.clear();
        world.batches = vec![vec![face(vec![
            [-500, -500, -1000],
            [500, -500, -1000],
            [0, 500, -1000],
        ])]];
        assert_eq!(unblocked(&world, &star), 1.0);
    }

    #[test]
    fn ear_clipping_fills_a_concave_outline() {
        // An L: six corners, four triangles, none outside it.
        let l = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ];
        let triangles = ear_clip(&l);
        assert_eq!(triangles.len(), 4);
        let area: f64 = triangles
            .iter()
            .map(|[a, b, c]| {
                let (a, b, c) = (l[*a], l[*b], l[*c]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
            })
            .sum();
        assert!((area - 3.0).abs() < 1.0e-9, "the L's area is 3, got {area}");
    }

    #[test]
    fn faces_become_fans_and_two_point_faces_lines() {
        let world = World {
            batches: vec![vec![
                Solid {
                    views: vec![[0, 0, 100], [10, 0, 100], [10, 10, 100], [0, 10, 100]],
                    rgb: [255, 0, 0],
                    outline: None,
                    flame: None,
                    sprite: None,
                    path: false,
                    texture: Some(crate::model_viewer::SolidTexture {
                        bitmap: 7,
                        slots: vec![[1, 2, 3]],
                        uv: vec![[0, 0], [63, 0], [63, 63], [0, 63]],
                        width: 64,
                        height: 64,
                    }),
                },
                Solid {
                    views: vec![[0, 0, 100], [5, 5, 100]],
                    rgb: [0, 255, 0],
                    texture: None,
                    outline: None,
                    flame: None,
                    sprite: None,
                    path: false,
                },
            ]],
            bitmaps: vec![WorldBitmap {
                bitmap: 7,
                width: 64,
                height: 64,
                indices: vec![0; 64 * 64].into(),
                masked: true,
            }],
            atmospheres: vec![],
            lights: vec![],
            eclipses: vec![],
            star: None,
            glowing: vec![],
            burning: vec![],
            sky: None,
            snapshot: vec![0; LOGICAL_WIDTH * LOGICAL_HEIGHT],
            dust: Vec::new(),
            bodies: Vec::new(),
            hud: Vec::new(),
            crisp: Vec::new(),
        };
        let scene = scene(&world, 640, 316, &mut MapCache::default(), 0.0);
        assert_eq!(scene.batches.len(), 1);
        assert_eq!(scene.batches[0].triangles.len(), 2);
        assert_eq!(scene.batches[0].lines.len(), 1);
        let background = scene.background.unwrap();
        assert_eq!((background.width, background.height), (640, 316));
        assert_eq!(background.pixels.len(), 640 * 316);
        // The quad's map is the bitmap's indices enlarged to 256, coloured from its own
        // palette (slot 0 a hole, as the bitmap is masked), and its fan keeps the corners'
        // coordinates.
        assert_eq!((scene.maps[0].width, scene.maps[0].height), (256, 256));
        assert_eq!(scene.palettes.len(), 1);
        assert_eq!(scene.palettes[0][0][3], 0.0);
        let half = 0.5 / 64.0;
        assert_eq!(
            scene.batches[0].triangles[1].texture,
            Some(ffe_render::FaceTexture {
                map: 0,
                palette: 0,
                uvs: [[half, half], [1.0 - half, 1.0 - half], [half, 1.0 - half]],
            })
        );
    }

    #[test]
    fn the_gpu_view_lands_where_the_original_projects_and_the_hud_stays_on_top() {
        let mut gpu = Gpu3d::new();
        if !gpu.available() {
            eprintln!("no GPU adapter; skipped");
            return;
        }
        // A white square centred ahead at z = 1000: the original puts its corners at
        // 160 +- 256*100/1000 (134.4..185.6) and 79 -+ 25.6 (53.4..104.6).
        let mut snapshot = vec![0x000010u32; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        snapshot[150 * LOGICAL_WIDTH..].fill(0x404040);
        let world = World {
            batches: vec![vec![Solid {
                views: vec![
                    [-100, -100, 1000],
                    [100, -100, 1000],
                    [100, 100, 1000],
                    [-100, 100, 1000],
                ],
                rgb: [255, 255, 255],
                texture: None,
                outline: None,
                flame: None,
                sprite: None,
                path: false,
            }]],
            bitmaps: vec![],
            atmospheres: vec![],
            lights: vec![],
            eclipses: vec![],
            star: None,
            glowing: vec![],
            burning: vec![],
            sky: None,
            snapshot: snapshot.clone(),
            dust: Vec::new(),
            bodies: Vec::new(),
            hud: Vec::new(),
            crisp: Vec::new(),
        };
        // The HUD drew one pixel at the square's centre after the world phase.
        let mut logical = snapshot;
        logical[79 * LOGICAL_WIDTH + 160] = 0x00ff00;
        let (width, height) = (1280usize, 800usize);
        let rect = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let mut target = vec![0x123456u32; width * height];
        gpu.render(&logical, &world, &mut target, width, rect)
            .unwrap();
        let at = |x: usize, y: usize| target[y * width + x];
        // 4x the logical frame: the square spans columns 538..742 and rows 214..418.
        assert_eq!(at(560, 240), 0xffffff);
        assert_eq!(at(720, 400), 0xffffff);
        // Beyond the white square's bloom (about 30 pixels), the sky as it was.
        assert_eq!(at(490, 240), 0x000010);
        assert_eq!(at(790, 300), 0x000010);
        // Just outside the square its glow lightens the sky.
        assert!(at(545 - 12, 300) & 0xff > 0x10);
        // The HUD pixel and the rows below the view are left as they were.
        assert_eq!(at(642, 318), 0x123456);
        assert_eq!(at(640, 700), 0x123456);
    }
}
