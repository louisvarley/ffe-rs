//! F6 model viewer: every imported model executed by the verified port and drawn
//! in the original render order. A development view, not the faithful renderer:
//! the camera, light, flat colour mapping and fill are diagnostic choices, and
//! polygons, curved surfaces, text, labels and child models are not drawn.
//! Textured faces follow the original F1708/F1712 entry and F1644/F1647
//! rasterisers with the cache v8 descriptor table (verified, see
//! `docs/notes/textures.md`); without v8 they show as a lit grey stand-in.
use crate::textures::TextureSet;
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::{evaluate, VertexContext};
use ffe_core::model_commands::{material, VisibilityContext};
use ffe_core::model_execute::{execute_live, resolve_colour, InstanceContext, Primitive};
use ffe_core::model_instance::{Vertices, NEAR_SENTINEL};
use ffe_core::model_lighting::{default_lighting, pack, Lighting};
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::{self, NormalContext};
use ffe_core::model_texture::{
    draw_quad, draw_triangle, flat_colour, quad_uv, textured_colours, textured_path, triangle_uv,
};

/// One screen-space primitive in draw order (logical 320x158 view coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Fill(Vec<(i32, i32)>, [u8; 3]),
    Line((i32, i32), (i32, i32), [u8; 3]),
    Point((i32, i32), [u8; 3]),
    Textured(Textured),
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
    /// Ordered primitives the viewer does not draw yet (polygons, labels).
    pub skipped: usize,
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
        let instance = InstanceContext {
            position,
            generation: GENERATION,
            key_scale: 0,
            operand_scale: 0,
            textures: true,
        };
        let result = execute_live(
            &model.commands,
            &mut vertices,
            &mut context,
            // Detail branches see the nearest depth: the viewer wants full detail.
            &VisibilityContext {
                depth: 1,
                scale: 0,
                detail: [0, 0],
                normals: &normals,
            },
            Some(&self.table),
            instance,
            4096,
        )?;
        let mut frame = Frame::default();
        let mut screen =
            |index: u8, context: &VertexContext| -> Result<Option<(i32, i32)>, String> {
                let vertex = vertices.resolve(index as i8, context, position, GENERATION, true)?;
                Ok((vertex.screen != NEAR_SENTINEL)
                    .then(|| (i32::from(vertex.screen[0]), i32::from(vertex.screen[1]))))
            };
        for primitive in &result.ordered {
            match primitive {
                Primitive::Face(emission) => {
                    let face = &emission.face;
                    let normal = normals
                        .get(usize::from(face.normal))
                        .ok_or("Face normal outside the evaluated table.")?;
                    let textured = material(face.material).texture != 0;
                    let descriptors = self.textures.descriptors();
                    let colour = emission.context.colour;
                    let rgb = self.rgb(match (textured, descriptors) {
                        // F1693/F1691 flat colour (the textured path is chosen below).
                        (true, Some(table)) => {
                            flat_colour(face.material, colour, normal.colour, table)?
                        }
                        // No v8 descriptor table: lit grey stand-in.
                        (true, None) => std::array::from_fn(|i| 5 + normal.colour[i]),
                        (false, _) => resolve_colour(face.material, colour, normal.colour),
                    });
                    let mut points = Vec::with_capacity(face.vertices.len());
                    for vertex in &face.vertices {
                        match screen(*vertex, &emission.context)? {
                            Some(point) => points.push(point),
                            None => break,
                        }
                    }
                    if points.len() != face.vertices.len() {
                        continue;
                    }
                    let screens: Vec<[i32; 2]> = points.iter().map(|&(x, y)| [x, y]).collect();
                    if let Some(table) = descriptors.filter(|_| points.len() >= 3) {
                        if textured_path(true, face.material, &screens) {
                            let entry =
                                textured_colours(face.material, colour, normal.colour, table)?;
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
                    if let (Some(a), Some(b)) = (
                        screen(line.vertices[0], &line.context)?,
                        screen(line.vertices[1], &line.context)?,
                    ) {
                        frame.shapes.push(Shape::Line(a, b, rgb));
                    }
                }
                Primitive::Point(point) => {
                    let rgb =
                        self.rgb(resolve_colour(point.material, point.context.colour, [0; 3]));
                    if let Some(p) = screen(point.vertex, &point.context)? {
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
