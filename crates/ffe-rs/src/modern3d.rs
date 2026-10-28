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
    /// The textured faces' bitmaps (their original indices), each once.
    pub bitmaps: Vec<WorldBitmap>,
    /// The logical frame as the world phase left it (without the set-aside faces).
    pub snapshot: Vec<u32>,
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
    let batches = world
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
            for solid in solids {
                let [r, g, b] = colour(solid.rgb);
                let rgba = [r, g, b, 1.0];
                let point = |v: [i32; 3]| v.map(|c| c as f32);
                if let (Some(sprite), [at]) = (&solid.sprite, &solid.views[..]) {
                    batch
                        .sprites
                        .push(light(sprite, point(*at), [r, g, b], pixel));
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
                // A face: a fan from its first corner.
                if let [first, rest @ ..] = &solid.views[..] {
                    let texture = texture_of(solid);
                    for (k, pair) in rest.windows(2).enumerate() {
                        batch.triangles.push(ffe_render::Triangle {
                            positions: [point(*first), point(pair[0]), point(pair[1])],
                            colours: [rgba; 3],
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
    ffe_render::Scene {
        width: width as u32,
        height: height as u32,
        background: Some(ffe_render::Image {
            width: (LOGICAL_WIDTH * BACKGROUND_SCALE) as u32,
            height: (VIEW_ROWS * BACKGROUND_SCALE) as u32,
            pixels: upscaled,
        }),
        // F1479: x lands at 160 + 256x/z of 320 columns, y at 79 - 256y/z of 158 rows.
        projection: ffe_render::Projection {
            x_scale: 256.0 / 160.0,
            y_scale: 256.0 / 79.0,
            near: NEAR,
        },
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
