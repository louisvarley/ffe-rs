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
/// How strongly the brightest parts (the star, lights, sunlit white) glow: modest.
const BLOOM: f32 = 0.6;
/// How much the frame behind the models is enlarged before the GPU filters it.
const BACKGROUND_SCALE: usize = 2;
/// The nearest view depth the original projects (F1479 rejects `z < 64`).
const NEAR: f32 = 64.0;

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
    /// The star, when it is ahead, for its hot core, glare and lens flare.
    pub star: Option<Star>,
    /// The batches that give off light (beams, explosions, a hit's sparks).
    pub glowing: Vec<usize>,
    /// The textured faces' bitmaps (their original indices), each once.
    pub bitmaps: Vec<WorldBitmap>,
    /// The logical frame as the world phase left it (without the set-aside faces).
    pub snapshot: Vec<u32>,
}

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
        let scene = scene(world, view_width, view_height, &mut self.maps);
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
            // Unchanged since the world phase: the 3-D view shows here.
            if drawn == before {
                destination.copy_from_slice(source);
                continue;
            }
            for ((pixel, &x), &shown) in destination.iter_mut().zip(&self.columns).zip(source) {
                if drawn[x] == before[x] {
                    *pixel = shown;
                }
            }
        }
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

/// The star's light seen through a lens, drawn over everything as a last batch of glows: a core
/// four times brighter than white (which the bloom spreads and the last pass turns white), a wide
/// faint glare, and four ghosts on the line from the star through the middle of the view, all
/// dimmed by how much of the star a planet hides. Nothing when it is hidden or far off the view.
fn flare(
    star: &Star,
    width: usize,
    pixel: f32,
    projection: &ffe_render::Projection,
) -> ffe_render::Batch {
    let mut batch = ffe_render::Batch::default();
    let [x, y, z] = star.direction;
    let seen = star.seen.clamp(0.0, 1.0);
    if z <= 0.0 || seen < 0.02 {
        return batch;
    }
    let screen = [x / z * projection.x_scale, y / z * projection.y_scale];
    if screen[0].abs() > 1.6 || screen[1].abs() > 1.6 {
        return batch;
    }
    let tint = star.rgb.map(|c| f32::from(c) / 255.0);
    let at = |s: [f32; 2]| {
        [
            s[0] / projection.x_scale * 1.0e6,
            s[1] / projection.y_scale * 1.0e6,
            1.0e6,
        ]
    };
    let disc = (star.radius * 2.0 * pixel).max(3.0);
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
        disc * 1.1,
        tint.map(|c| 2.0 + 2.0 * c),
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
        (solid.sprite.is_none() && !solid.path && solid.views.len() >= 3)
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
            for (solid, shade) in solids.iter().zip(&shading) {
                let [r, g, b] = colour(solid.rgb);
                // In an object that gives off light, its coloured parts (fire, a beam) glow by
                // how saturated they are; grey debris stays as it is. It is lit by itself, so a
                // planet's shadow does not darken it.
                let glow = if glowing {
                    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
                    let saturation = if high > 0.0 { (high - low) / high } else { 0.0 };
                    1.0 + (EMISSIVE - 1.0) * saturation
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
    if let Some(star) = &world.star {
        batches.push(flare(star, width, pixel, &projection));
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

    #[test]
    fn faces_become_fans_and_two_point_faces_lines() {
        let world = World {
            batches: vec![vec![
                Solid {
                    views: vec![[0, 0, 100], [10, 0, 100], [10, 10, 100], [0, 10, 100]],
                    rgb: [255, 0, 0],
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
            snapshot: vec![0; LOGICAL_WIDTH * LOGICAL_HEIGHT],
        };
        let scene = scene(&world, 640, 316, &mut MapCache::default());
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
                sprite: None,
                path: false,
            }]],
            bitmaps: vec![],
            atmospheres: vec![],
            lights: vec![],
            eclipses: vec![],
            star: None,
            glowing: vec![],
            snapshot: snapshot.clone(),
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
