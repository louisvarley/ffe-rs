//! F6 model viewer: every imported model executed by the verified port and drawn
//! in the original render order. A development view, not the faithful renderer:
//! the camera, light, flat colour mapping and fill are diagnostic choices, and
//! textures (grey stand-in), polygons, curved surfaces, text, labels and child
//! models are not drawn.
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::{evaluate, VertexContext};
use ffe_core::model_commands::{material, VisibilityContext};
use ffe_core::model_execute::{execute_live, resolve_colour, InstanceContext, Primitive};
use ffe_core::model_instance::{Vertices, NEAR_SENTINEL};
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::{self, NormalContext};

/// One screen-space primitive in draw order (logical 320x158 view coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Fill(Vec<(i32, i32)>, [u8; 3]),
    Line((i32, i32), (i32, i32), [u8; 3]),
    Point((i32, i32), [u8; 3]),
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
}

const GENERATION: u32 = 2;

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
        Self::from_models(models, table)
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

    fn render_at(&self, distance: f64) -> Result<Frame, String> {
        let model = self.model();
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
        let mut matrix = [0i32; 9];
        for (k, row) in rows.iter().enumerate() {
            for (axis, value) in row.iter().enumerate() {
                matrix[3 * k + axis] = (value * 2_147_483_647.0).round() as i32;
            }
        }
        if distance > 1.5e9 {
            return Err("Model too large for the viewer's 32-bit view space.".to_owned());
        }
        let position = [0, 0, distance as i32];
        // Camera and light in object space, record units, for the original normal test.
        let unit = 2f64.powi(model.scale as i32);
        let observer = rows.map(|row| (-distance * row[2] / unit) as i32);
        let light_view = [0.4, -0.5, 0.75];
        let length = light_view.iter().map(|v| v * v).sum::<f64>().sqrt();
        let light = rows.map(|row| {
            (row.iter().zip(light_view).map(|(a, b)| a * b).sum::<f64>() / length * 33_000.0) as i32
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
                shades: std::array::from_fn(|i| [2 * (7 - i as u8); 3]),
            },
        )?;
        let mut vertices = Vertices::new(&model.vertices, model.vertex_count)?;
        let mut context = VertexContext {
            matrix,
            basis: matrix,
            scale: model.scale,
            globals: [self.animation; 64],
            locals: [i32::from(self.animation); 64],
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
                    // Texture mapping is not ported: textured faces show as lit grey.
                    let rgb = rgb(if material(face.material).texture != 0 {
                        std::array::from_fn(|i| 5 + normal.colour[i])
                    } else {
                        resolve_colour(face.material, emission.context.colour, normal.colour)
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
                    frame.shapes.push(if points.len() == 2 {
                        Shape::Line(points[0], points[1], rgb)
                    } else {
                        Shape::Fill(points, rgb)
                    });
                }
                Primitive::Line(line) => {
                    let rgb = rgb(resolve_colour(line.material, line.context.colour, [0; 3]));
                    if let (Some(a), Some(b)) = (
                        screen(line.vertices[0], &line.context)?,
                        screen(line.vertices[1], &line.context)?,
                    ) {
                        frame.shapes.push(Shape::Line(a, b, rgb));
                    }
                }
                Primitive::Point(point) => {
                    let rgb = rgb(resolve_colour(point.material, point.context.colour, [0; 3]));
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
                        let colour = rgb(spline.colour);
                        for step in 0..16 {
                            frame.shapes.push(Shape::Line(
                                at(f64::from(step) / 16.0),
                                at(f64::from(step + 1) / 16.0),
                                colour,
                            ));
                        }
                    }
                }
                Primitive::Polygon(_) | Primitive::Label(_) => frame.skipped += 1,
            }
        }
        Ok(frame)
    }
}

impl ViewerModel {
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

/// Diagnostic mapping of the original's additive nibble colours (material plus
/// normal shade, up to 22 per channel) to 8-bit RGB; not the original palette.
fn rgb(colour: [u8; 3]) -> [u8; 3] {
    colour.map(|c| (u32::from(c).min(21) * 12) as u8)
}
