//! F6 model viewer: every imported model executed by the verified port and drawn
//! in the original render order. A development view, not the faithful renderer:
//! the camera, light, flat colour mapping and fill are diagnostic choices, and
//! curved surfaces, text and labels are not drawn. Thruster glows (F1738) are
//! the original's queued F1805 quads drawn by the ported textured rasteriser.
//! Textured faces follow the original F1708/F1712 entry and F1644/F1647
//! rasterisers with the cache v8 descriptor table (verified, see
//! `docs/notes/textures.md`); without v8 they show as a lit grey stand-in.
use crate::textures::TextureSet;
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::{evaluate, VertexContext};
use ffe_core::model_commands::{material, VisibilityContext};
use ffe_core::model_execute::{
    execute_live, execute_scene, resolve_colour, ChildEnvironment, ChildModel, ChildModels,
    InstanceContext, Primitive,
};
use ffe_core::model_instance::{Vertices, NEAR_SENTINEL};
use ffe_core::model_lighting::{default_lighting, pack, Lighting};
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::{self, NormalContext};
use ffe_core::model_texture::{
    draw_quad, draw_triangle, flat_colour, quad_uv, textured_colours, textured_path, triangle_uv,
};
use ffe_core::model_thruster::{line_colour, Shape as ThrusterShape};

/// One screen-space primitive in draw order (logical 320x158 view coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Fill(Vec<(i32, i32)>, [u8; 3]),
    Line((i32, i32), (i32, i32), [u8; 3]),
    Point((i32, i32), [u8; 3]),
    Textured(Textured),
    /// A thruster glow (F1738's F1805 entry): drawn like [`Shape::Textured`] (F1647
    /// over bitmap 0x123 or 0x65), but left out of the viewer's fit so that the
    /// model keeps its size as the plume grows with thrust.
    Glow(Textured),
}

/// A textured F1807/F1805 queue entry: screen points in queue order (the face's
/// vertices v0..), the descriptor's bitmap dword and the slot colours that
/// `DATA_009252` receives (RGB of the F1326 colours).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Textured {
    pub points: Vec<(i32, i32)>,
    pub bitmap: u32,
    pub slots: Vec<[u8; 3]>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Frame {
    pub shapes: Vec<Shape>,
    /// Ordered primitives the viewer does not draw yet (labels).
    pub skipped: usize,
    /// Why child models were left out of this frame, if they were.
    pub children_error: Option<String>,
}

pub struct ViewerModel {
    pub index: u32,
    pub vertex_count: u32,
    pub scale: u32,
    vertices: Vec<u8>,
    commands: Vec<u8>,
    normals: Vec<u8>,
    normal_count: u32,
    /// Largest object-space vertex distance at input 0, or the evaluation error.
    radius: Result<f64, String>,
}

pub struct ModelViewer {
    models: Vec<ViewerModel>,
    table: SineTable,
    selected: usize,
    yaw: i32,
    pitch: i32,
    zoom: i32,
    animation: u16,
    textures: TextureSet,
    lighting: Lighting,
    /// Descriptor +0x18 and +0x1c per table slot (cache v10); empty without it.
    descriptors: Vec<[i32; 2]>,
    /// Global word 15 (the ship's forward thrust, object +0xba); `None` leaves it
    /// to the animation input. See [`ModelViewer::set_thrust`].
    thrust: Option<u16>,
}

/// The model global that ship models read as their forward thrust: global word 15
/// is object word `+0x9c + 2 * 15` = `+0xba` (F1473), the z word of the vector F598
/// clamps to the ship's per-axis limits and stores at +0xb6/+0xb8/+0xba. The ships'
/// thruster preamble skips the glows when this word is zero or has bit 15 set, sets
/// local 1 to `min(thrust, 4095) << 4` (zero below 5120) and interpolates each
/// plume end (vertex B, curve vertex opcode 19) by local 1. See
/// `docs/notes/thrusters.md`.
pub const THRUST_GLOBAL: usize = 15;

/// The imported models as child calls see the model table.
struct ChildTable<'a>(&'a ModelViewer);

impl ChildModels for ChildTable<'_> {
    fn model(&self, index: u16) -> Result<ChildModel<'_>, String> {
        let viewer = self.0;
        let model = viewer
            .find(u32::from(index))
            .ok_or_else(|| format!("Child model {index} is not in the cache."))?;
        let [scale_offset, radius] = *viewer
            .descriptors
            .get(usize::from(index))
            .ok_or_else(|| format!("Child model {index} has no descriptor."))?;
        Ok(ChildModel {
            commands: &model.commands,
            vertices: &model.vertices,
            vertex_count: model.vertex_count,
            normals: &model.normals,
            normal_count: model.normal_count,
            scale: model.scale as i32,
            scale_offset,
            radius,
        })
    }
}

const GENERATION: u32 = 2;

/// Without the v9 lighting tables: a diagnostic light and a plain 7..0 ramp.
const FALLBACK_LIGHTING: Lighting = Lighting {
    light: [13107, -16384, 24576],
    shades: [
        [7; 3], [6; 3], [5; 3], [4; 3], [3; 3], [2; 3], [1; 3], [0; 3],
    ],
};

impl ModelViewer {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let table = SineTable::from_bytes(
            cache
                .asset("base:math/sine")
                .ok_or("The model viewer needs cache v6 or later (sine table).")?,
        )?;
        let mut models = Vec::new();
        for record in &cache.manifest().files {
            let AssetKind::ModelCommands {
                index,
                vertex_count,
                ..
            } = record.asset
            else {
                continue;
            };
            let find = |kind: &str| {
                let id = format!("base:model-{kind}/{index:03}");
                let record = cache.manifest().files.iter().find(|r| r.id == id);
                Some((record?, cache.asset(&id)?))
            };
            let (Some((vertex_record, vertices)), Some((normal_record, normals))) =
                (find("vertices"), find("normals"))
            else {
                continue;
            };
            let (
                AssetKind::ModelVertices { scale, .. },
                AssetKind::ModelNormals { normal_count, .. },
            ) = (&vertex_record.asset, &normal_record.asset)
            else {
                continue;
            };
            models.push(ViewerModel::new(
                index,
                vertex_count,
                *scale,
                vertices,
                cache.asset(&record.id).ok_or("Missing command payload.")?,
                normals,
                *normal_count,
            ));
        }
        let mut viewer = Self::from_models(models, table)?;
        viewer.textures = TextureSet::from_cache(cache);
        if let Some(lighting) = cache
            .asset("base:lighting/default")
            .and_then(|bytes| default_lighting(bytes).ok())
        {
            viewer.lighting = lighting;
        }
        if let Some(table) = cache.asset("base:model/descriptors") {
            viewer.descriptors = table
                .chunks_exact(8)
                .map(|entry| {
                    std::array::from_fn(|i| {
                        i32::from_le_bytes([
                            entry[4 * i],
                            entry[4 * i + 1],
                            entry[4 * i + 2],
                            entry[4 * i + 3],
                        ])
                    })
                })
                .collect();
        }
        Ok(viewer)
    }

    /// Use these textures (bitmaps, descriptor table, colour map).
    pub fn set_textures(&mut self, textures: TextureSet) {
        self.textures = textures;
    }

    /// Rasterise a textured shape with the original affine mapping (F1644/F1647
    /// and the F1650-F1653 spans), calling `plot` with each pixel's RGB.
    /// Texels without a slot colour (7 and above, never written by face entries;
    /// stale `DATA_009252` contents in the original) are drawn black, the zeroed
    /// start-up value. Returns false when the bitmap is missing.
    pub fn rasterise(&self, shape: &Textured, mut plot: impl FnMut(i32, i32, [u8; 3])) -> bool {
        let Some(map) = self.textures.map(shape.bitmap & 0xfff) else {
            return false;
        };
        let point = |i: usize| [shape.points[i].0 as i16, shape.points[i].1 as i16];
        let colour = |texel: u8| {
            shape
                .slots
                .get(usize::from(texel))
                .copied()
                .unwrap_or([0; 3])
        };
        let plot = |x, y, texel| plot(x, y, colour(texel));
        match shape.points.len() {
            3 => draw_triangle(
                [point(0), point(1), point(2)],
                triangle_uv(map.width),
                map,
                plot,
            ),
            4 => draw_quad(
                [point(0), point(1), point(2), point(3)],
                quad_uv(map.width, (shape.bitmap >> 12 & 7) as u8),
                map,
                plot,
            ),
            _ => return false,
        };
        true
    }

    /// A nibble colour as the original shows it: packed by F1326 (no clamp), then
    /// the fixed low palette where `DATA_007770` maps it, else 4-bit VGA levels.
    /// The 12-bit mask only guards the table lookup against an out-of-range sum.
    fn rgb(&self, colour: [u8; 3]) -> [u8; 3] {
        self.textures.rgb(pack(colour) & 0xfff)
    }

    pub fn from_models(mut models: Vec<ViewerModel>, table: SineTable) -> Result<Self, String> {
        if models.is_empty() {
            return Err("The cache holds no models. Build a v7 data.bin to view them.".to_owned());
        }
        models.sort_by_key(|m| m.index);
        Ok(Self {
            models,
            table,
            selected: 0,
            yaw: 30,
            pitch: -20,
            zoom: 0,
            animation: 0,
            textures: TextureSet::default(),
            lighting: FALLBACK_LIGHTING,
            descriptors: Vec::new(),
            thrust: None,
        })
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn model(&self) -> &ViewerModel {
        &self.models[self.selected]
    }

    /// Move through the model list, wrapping.
    pub fn step(&mut self, step: i32) {
        let count = self.models.len() as i32;
        self.selected = (self.selected as i32 + step).rem_euclid(count) as usize;
    }

    /// Rotate in 5-degree steps.
    pub fn turn(&mut self, yaw: i32, pitch: i32) {
        self.yaw = (self.yaw + 5 * yaw.clamp(-1, 1)).rem_euclid(360);
        self.pitch = (self.pitch + 5 * pitch.clamp(-1, 1)).rem_euclid(360);
    }

    /// Zoom in quarter-octave steps, bounded.
    pub fn zoom(&mut self, step: i32) {
        self.zoom = (self.zoom + step.clamp(-1, 1)).clamp(-8, 12);
    }

    pub fn set_animation(&mut self, value: u16) {
        self.animation = value;
    }

    pub fn animation(&self) -> u16 {
        self.animation
    }

    /// Set the forward thrust ships read from global word 15 ([`THRUST_GLOBAL`],
    /// object +0xba), or `None` to leave that word to the animation input. Ship
    /// thruster glows appear for 320..=0x7fff and lengthen with it up to 4095.
    pub fn set_thrust(&mut self, value: Option<u16>) {
        self.thrust = value;
    }

    pub fn thrust(&self) -> Option<u16> {
        self.thrust
    }

    /// Execute the selected model and project its ordered primitives. A first pass
    /// at the bounding-radius distance measures the drawn extent; the second fits
    /// it to about 70 pixels from the centre, then applies the zoom.
    pub fn render(&self) -> Result<Frame, String> {
        let radius = self.model().radius.clone()?.max(1.0);
        let first = (radius * 256.0 / 60.0).max(128.0);
        let probe = self.render_at(first)?;
        let extent = probe
            .shapes
            .iter()
            .flat_map(|shape| match shape {
                Shape::Fill(points, _) => points.clone(),
                Shape::Line(a, b, _) => vec![*a, *b],
                Shape::Point(p, _) => vec![*p],
                Shape::Textured(textured) => textured.points.clone(),
                Shape::Glow(_) => Vec::new(),
            })
            .map(|(x, y)| (x - 160).abs().max((y - 79).abs()))
            .max()
            .unwrap_or(0);
        let fitted = if extent > 0 {
            (first * f64::from(extent) / 70.0).max(128.0)
        } else {
            first
        };
        self.render_at(fitted * 2f64.powf(-f64::from(self.zoom) / 4.0))
    }

    /// The light direction as a unit vector: the original default light (F960,
    /// `DATA_007231`) from a v9 cache.
    pub fn light_direction(&self) -> [f64; 3] {
        let light = self.lighting.light.map(f64::from);
        let length = light.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
        light.map(|v| v / length)
    }

    /// FUNC_001110's closest external camera for a ship model: its descriptor
    /// radius word (+0x1c) shifted left by its scale (+0x14) + 8, in the camera's
    /// distance units. `None` without the v10 descriptor table.
    pub fn camera_minimum(&self, index: u32) -> Option<i32> {
        let model = self.find(index)?;
        let [_, radius] = *self.descriptors.get(index as usize)?;
        Some(((radius & 0xffff) as u32).wrapping_shl(model.scale + 8) as i32)
    }

    /// The model with this table index, if imported.
    pub fn find(&self, index: u32) -> Option<&ViewerModel> {
        self.models.iter().find(|m| m.index == index)
    }

    /// Render model `index` in a scene camera. `rows[k]` is object axis k in view
    /// space (unit vectors), `position` the model origin in view space, both in
    /// the model's own (post-scale) units; `light_view` the light direction in
    /// view space. Full detail, input 0.
    pub fn render_scene(
        &self,
        index: u32,
        rows: [[f64; 3]; 3],
        position: [f64; 3],
        light_view: [f64; 3],
    ) -> Result<Frame, String> {
        let model = self
            .find(index)
            .ok_or_else(|| format!("Model {index} is not in the cache."))?;
        if position.iter().any(|v| v.abs() > 1.5e9) {
            return Err("Model too far for the 32-bit view space.".to_owned());
        }
        self.render_with(model, rows, position.map(|v| v as i32), light_view, 0)
    }

    fn render_at(&self, distance: f64) -> Result<Frame, String> {
        let distance = distance.max(128.0);
        let (yaw, pitch) = (
            f64::from(self.yaw).to_radians(),
            f64::from(self.pitch).to_radians(),
        );
        let rotate = |[x, y, z]: [f64; 3]| {
            let (rx, rz) = (x * yaw.cos() - z * yaw.sin(), x * yaw.sin() + z * yaw.cos());
            [
                rx,
                y * pitch.cos() - rz * pitch.sin(),
                y * pitch.sin() + rz * pitch.cos(),
            ]
        };
        // Row k is object axis k in view space, as vertex evaluation reads the frame.
        let rows = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(rotate);
        if distance > 1.5e9 {
            return Err("Model too large for the viewer's 32-bit view space.".to_owned());
        }
        self.render_with(
            self.model(),
            rows,
            [0, 0, distance as i32],
            // The default light, taken as view-relative in this inspection view.
            self.light_direction(),
            self.animation,
        )
    }

    fn render_with(
        &self,
        model: &ViewerModel,
        rows: [[f64; 3]; 3],
        position: [i32; 3],
        light_view: [f64; 3],
        animation: u16,
    ) -> Result<Frame, String> {
        let mut matrix = [0i32; 9];
        for (k, row) in rows.iter().enumerate() {
            for (axis, value) in row.iter().enumerate() {
                matrix[3 * k + axis] = (value * 2_147_483_647.0).round() as i32;
            }
        }
        // Camera and light in object space, record units, for the original normal
        // test: the camera sits at -position in view space.
        let unit = 2f64.powi(model.scale as i32);
        let observer = rows.map(|row| {
            let along: f64 = row
                .iter()
                .zip(position)
                .map(|(a, p)| a * f64::from(p))
                .sum();
            (-along / unit) as i32
        });
        let length = light_view.iter().map(|v| v * v).sum::<f64>().sqrt();
        let light = rows.map(|row| {
            // F1518 normalises the light to length 0x8000.
            (row.iter().zip(light_view).map(|(a, b)| a * b).sum::<f64>() / length * 32_768.0) as i32
        });
        let normals = model_normals::evaluate(
            &model.normals,
            model.normal_count,
            &model.vertices,
            model.vertex_count,
            &NormalContext {
                scale: model.scale,
                observer_scale: model.scale,
                observer,
                light,
                shades: self.lighting.shades,
            },
        )?;
        let mut vertices =
            Vertices::new(&model.vertices, model.vertex_count)?.with_normal_records(&model.normals);
        let mut context = VertexContext {
            matrix,
            basis: matrix,
            scale: model.scale,
            globals: [animation; 64],
            locals: [i32::from(animation); 64],
            ..Default::default()
        };
        // Light points (opcode 30 0x801e) count global word 4; keep it empty.
        context.globals[4] = 0;
        if let Some(thrust) = self.thrust {
            context.globals[THRUST_GLOBAL] = thrust;
        }
        // F1683 sets +0x148 from descriptor +0x14 (the model scale) and +0x130 to 0;
        // thruster widths (F1738) shift by it.
        let instance = InstanceContext {
            position,
            generation: GENERATION,
            key_scale: 0,
            operand_scale: model.scale as i32,
            textures: true,
        };
        // Detail branches see the nearest depth: the viewer wants full detail.
        let visibility = VisibilityContext {
            depth: 1,
            scale: 0,
            detail: [0, 0],
            normals: &normals,
        };
        // With the v10 descriptor table, child models run inside their parent
        // (`execute_scene`); without it, child calls are only recorded.
        let mut children_error = None;
        let scene = if self.descriptors.is_empty() {
            None
        } else {
            let environment = ChildEnvironment {
                models: &ChildTable(self),
                shades: self.lighting.shades,
                // The top-level light (+0x134) in the frame children are set up in.
                light: light_view.map(|v| (v / length * 32_768.0) as i32),
                arena: 0,
            };
            // Transactional: on an error, vertices and context are unchanged.
            match execute_scene(
                &model.commands,
                &mut vertices,
                &mut context,
                &visibility,
                Some(&self.table),
                instance,
                4096,
                &environment,
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    children_error = Some(error);
                    None
                }
            }
        };
        // Without children (no v10 table, or a child that cannot run yet).
        let result = match scene {
            Some(result) => result,
            None => execute_live(
                &model.commands,
                &mut vertices,
                &mut context,
                &visibility,
                Some(&self.table),
                instance,
                4096,
            )?,
        };
        let mut frame = Frame {
            children_error,
            ..Frame::default()
        };
        let point =
            |p: [i16; 2]| (p != NEAR_SENTINEL).then_some((i32::from(p[0]), i32::from(p[1])));
        for primitive in &result.ordered {
            match primitive {
                Primitive::Face(emission) => {
                    let face = &emission.face;
                    // The shade of the face's own (parent or child) normal table.
                    let shade = emission.shade;
                    let textured = material(face.material).texture != 0;
                    let descriptors = self.textures.descriptors();
                    let colour = emission.context.colour;
                    let rgb = self.rgb(match (textured, descriptors) {
                        // F1693/F1691 flat colour (the textured path is chosen below).
                        (true, Some(table)) => flat_colour(face.material, colour, shade, table)?,
                        // No v8 descriptor table: lit grey stand-in.
                        (true, None) => std::array::from_fn(|i| 5 + shade[i]),
                        (false, _) => resolve_colour(face.material, colour, shade),
                    });
                    let points: Vec<(i32, i32)> =
                        emission.screen.iter().filter_map(|p| point(*p)).collect();
                    if points.is_empty() || points.len() != face.vertices.len() {
                        continue;
                    }
                    let screens: Vec<[i32; 2]> = points.iter().map(|&(x, y)| [x, y]).collect();
                    if let Some(table) = descriptors.filter(|_| points.len() >= 3) {
                        if textured_path(true, face.material, &screens) {
                            let entry = textured_colours(face.material, colour, shade, table)?;
                            frame.shapes.push(Shape::Textured(Textured {
                                points,
                                bitmap: entry.bitmap,
                                slots: entry
                                    .colours
                                    .iter()
                                    .map(|c| self.textures.rgb(*c))
                                    .collect(),
                            }));
                            continue;
                        }
                    }
                    frame.shapes.push(if points.len() == 2 {
                        Shape::Line(points[0], points[1], rgb)
                    } else {
                        Shape::Fill(points, rgb)
                    });
                }
                Primitive::Line(line) => {
                    let rgb = self.rgb(resolve_colour(line.material, line.context.colour, [0; 3]));
                    if let Some([a, b]) = line.screen {
                        if let (Some(a), Some(b)) = (point(a), point(b)) {
                            frame.shapes.push(Shape::Line(a, b, rgb));
                        }
                    }
                }
                Primitive::Point(emitted) => {
                    let rgb = self.rgb(resolve_colour(
                        emitted.material,
                        emitted.context.colour,
                        [0; 3],
                    ));
                    if let Some(p) = emitted.screen.and_then(point) {
                        frame.shapes.push(Shape::Point(p, rgb));
                    }
                }
                Primitive::Spline(spline) => {
                    // F1811's curve drawing is not ported: a cubic through the entry's
                    // four points in entry order stands in for it.
                    if let Some(points) = spline.points {
                        let p = points.map(|[x, y]| (f64::from(x), f64::from(y)));
                        let at = |t: f64| {
                            let u = 1.0 - t;
                            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                            let x: f64 = (0..4).map(|i| w[i] * p[i].0).sum();
                            let y: f64 = (0..4).map(|i| w[i] * p[i].1).sum();
                            (x.round() as i32, y.round() as i32)
                        };
                        let colour = self.rgb(spline.colour);
                        for step in 0..16 {
                            frame.shapes.push(Shape::Line(
                                at(f64::from(step) / 16.0),
                                at(f64::from(step + 1) / 16.0),
                                colour,
                            ));
                        }
                    }
                }
                Primitive::Polygon(polygon) => {
                    // The outline is the original's queued entries and F1547 edge
                    // subdivision; the fill is the diagnostic even-odd scanline fill.
                    // Textured fills (F1691 texture colour not ported) show grey.
                    let path = ffe_core::model_polygon::fill_path(&polygon.outline.entries)?;
                    if path.len() >= 3 {
                        let colour = if material(polygon.polygon.material).texture != 0 {
                            [5; 3]
                        } else {
                            polygon.colour
                        };
                        let points = path
                            .iter()
                            .map(|[x, y]| (i32::from(*x), i32::from(*y)))
                            .collect();
                        frame.shapes.push(Shape::Fill(points, self.rgb(colour)));
                    }
                }
                Primitive::Thruster(entry) => match &entry.shape {
                    // F1805: the slot colours through F1326/F1327, then the original
                    // textured quad (F1647) over bitmap 0x123 (core) or 0x65 (shaft).
                    // Both bitmaps skip texel 0, so slot 0, which F1738 never writes
                    // (stale queue memory in the original), is never drawn.
                    ThrusterShape::Glow {
                        points,
                        texture,
                        colours,
                    } => {
                        let mut slots = vec![[0; 3]];
                        slots.extend(colours.iter().map(|c| self.rgb(*c)));
                        frame.shapes.push(Shape::Glow(Textured {
                            points: points
                                .iter()
                                .map(|[x, y]| (i32::from(*x), i32::from(*y)))
                                .collect(),
                            bitmap: *texture,
                            slots,
                        }));
                    }
                    // F1803: the stored end points (x - 1 when positive) in the F1691
                    // colour; the line rasteriser (F1616) is the viewer's own.
                    ThrusterShape::Line { points, colour } => {
                        let colour = match self.textures.descriptors() {
                            Some(table) => line_colour(entry.material, *colour, table)?,
                            None => *colour,
                        };
                        let [a, b] = points.map(|[x, y]| (x, y));
                        frame.shapes.push(Shape::Line(a, b, self.rgb(colour)));
                    }
                },
                Primitive::Label(_) => frame.skipped += 1,
            }
        }
        Ok(frame)
    }
}

impl ViewerModel {
    /// Largest object-space vertex distance at input 0, when evaluable alone.
    pub fn radius(&self) -> Option<f64> {
        self.radius.as_ref().ok().copied()
    }

    pub fn new(
        index: u32,
        vertex_count: u32,
        scale: u32,
        vertices: &[u8],
        commands: &[u8],
        normals: &[u8],
        normal_count: u32,
    ) -> Self {
        let radius = evaluate(
            vertices,
            vertex_count,
            &VertexContext {
                scale,
                ..Default::default()
            },
        )
        .map(|points| {
            points
                .iter()
                .map(|p| p.iter().map(|c| f64::from(*c).powi(2)).sum::<f64>().sqrt())
                .fold(0.0, f64::max)
        });
        Self {
            index,
            vertex_count,
            scale,
            vertices: vertices.to_vec(),
            commands: commands.to_vec(),
            normals: normals.to_vec(),
            normal_count,
            radius,
        }
    }
}
