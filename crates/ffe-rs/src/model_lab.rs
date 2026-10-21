use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::{evaluate, VertexContext};
use ffe_core::model_commands::{decode, Face, Program};
use ffe_core::model_normals::{self, Normal, NormalContext};

pub struct StationVertices {
    pub index: u32,
    pub count: u32,
    pub scale: u32,
    records: Vec<u8>,
    radius: f64,
    program: Option<Program>,
    normals: Option<(Vec<u8>, u32)>,
}

impl StationVertices {
    pub fn new(index: u32, count: u32, scale: u32, records: &[u8]) -> Result<Self, String> {
        if !matches!(index, 74 | 75) {
            return Err("Model lab supports only station vertex programs 74/75.".to_owned());
        }
        let mut radius = 1.0f64;
        for animation in [0, 65535] {
            let context = context(scale, animation);
            for point in evaluate(records, count, &context)? {
                radius = radius.max(
                    point
                        .into_iter()
                        .map(|p| f64::from(p).powi(2))
                        .sum::<f64>()
                        .sqrt(),
                );
            }
        }
        Ok(Self {
            index,
            count,
            scale,
            records: records.to_vec(),
            radius,
            program: None,
            normals: None,
        })
    }

    pub fn with_commands(mut self, bytes: &[u8]) -> Result<Self, String> {
        let program = decode(bytes, self.count)?;
        if let Some((_, count)) = &self.normals {
            validate_face_normals(&program, *count)?;
        }
        self.program = Some(program);
        Ok(self)
    }

    pub fn with_normals(mut self, bytes: &[u8], count: u32) -> Result<Self, String> {
        model_normals::validate(bytes, count, &self.records, self.count)?;
        if let Some(program) = &self.program {
            validate_face_normals(program, count)?;
        }
        self.normals = Some((bytes.to_vec(), count));
        Ok(self)
    }

    pub fn program(&self) -> Option<&Program> {
        self.program.as_ref()
    }
}

fn validate_face_normals(program: &Program, count: u32) -> Result<(), String> {
    if program
        .faces
        .iter()
        .any(|face| u32::from(face.normal) >= count)
    {
        return Err("Face references an out-of-range normal.".to_owned());
    }
    Ok(())
}

fn context(scale: u32, animation: u16) -> VertexContext {
    VertexContext {
        scale,
        globals: [animation; 64],
        locals: [i32::from(animation); 64],
        ..Default::default()
    }
}

pub struct ModelLab {
    models: [StationVertices; 2],
    selected: usize,
    yaw: i32,
    pitch: i32,
    animation: u16,
    view: LabView,
    face: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabView {
    Points,
    Wire,
    Face,
    Normals,
}

impl ModelLab {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let load = |index| {
            let id = format!("base:model-vertices/{index:03}");
            let record = cache.manifest().files.iter().find(|r| r.id == id)
                .ok_or("Model lab requires cache v3. Build-data to a NEW file, verify it, then replace only your generated data.bin.")?;
            let model = match record.asset {
                AssetKind::ModelVertices {
                    index: source_index,
                    vertex_count,
                    scale,
                    ..
                } if source_index == index => StationVertices::new(
                    index,
                    vertex_count,
                    scale,
                    cache.asset(&id).ok_or("Missing model payload.")?,
                ),
                _ => Err("Invalid model vertex descriptor.".to_owned()),
            }?;
            if cache.manifest().format_version < 4 {
                return Ok(model);
            }
            let command_id = format!("base:model-commands/{index:03}");
            let commands = cache
                .manifest()
                .files
                .iter()
                .find(|r| r.id == command_id)
                .ok_or(
                    "Cache v4 is missing station drawing commands; rebuild the generated cache.",
                )?;
            let model = match (&commands.asset, &record.asset) {
                (
                    AssetKind::ModelCommands {
                        index: command_index,
                        vertex_count: command_count,
                        descriptor_offset: command_descriptor,
                        ..
                    },
                    AssetKind::ModelVertices {
                        vertex_count,
                        descriptor_offset,
                        ..
                    },
                ) if *command_index == index
                    && command_count == vertex_count
                    && command_descriptor == descriptor_offset =>
                {
                    model.with_commands(cache.asset(&command_id).ok_or("Missing command payload.")?)
                }
                _ => Err("Command/vertex descriptor mismatch.".to_owned()),
            }?;
            if cache.manifest().format_version < 5 {
                return Ok(model);
            }
            let normal_id = format!("base:model-normals/{index:03}");
            let normals = cache
                .manifest()
                .files
                .iter()
                .find(|r| r.id == normal_id)
                .ok_or("Cache v5 is missing station normals; rebuild the generated cache.")?;
            match (&normals.asset, &record.asset) {
                (
                    AssetKind::ModelNormals {
                        index: normal_index,
                        vertex_count: normal_vertices,
                        normal_count,
                        descriptor_offset: normal_descriptor,
                        ..
                    },
                    AssetKind::ModelVertices {
                        vertex_count,
                        descriptor_offset,
                        ..
                    },
                ) if *normal_index == index
                    && normal_vertices == vertex_count
                    && normal_descriptor == descriptor_offset =>
                {
                    model.with_normals(
                        cache.asset(&normal_id).ok_or("Missing normal payload.")?,
                        *normal_count,
                    )
                }
                _ => Err("Normal/vertex descriptor mismatch.".to_owned()),
            }
        };
        Ok(Self::from_models([load(74)?, load(75)?]))
    }

    pub fn from_models(models: [StationVertices; 2]) -> Self {
        let view = if models[0]
            .program
            .as_ref()
            .is_some_and(|p| !p.faces.is_empty())
        {
            LabView::Wire
        } else {
            LabView::Points
        };
        Self {
            models,
            selected: 0,
            yaw: 5,
            pitch: -3,
            animation: 0,
            view,
            face: 0,
        }
    }

    pub fn select(&mut self, selected: usize) -> Result<(), String> {
        if selected >= self.models.len() {
            return Err("Invalid model lab selection.".to_owned());
        }
        self.selected = selected;
        self.face = 0;
        if self.view == LabView::Normals && self.model().normals.is_none() {
            self.view = LabView::Wire;
        }
        if self
            .model()
            .program
            .as_ref()
            .is_none_or(|p| p.faces.is_empty())
        {
            self.view = LabView::Points;
        }
        Ok(())
    }

    pub fn view(&self) -> LabView {
        self.view
    }

    pub fn cycle_view(&mut self) {
        if self
            .model()
            .program
            .as_ref()
            .is_some_and(|p| !p.faces.is_empty())
        {
            self.view = match self.view {
                LabView::Points => {
                    if self.model().normals.is_some() {
                        LabView::Normals
                    } else {
                        LabView::Wire
                    }
                }
                LabView::Wire => LabView::Face,
                LabView::Face => LabView::Points,
                LabView::Normals => LabView::Wire,
            };
        }
    }

    pub fn step_face(&mut self, step: i32) -> Result<(), String> {
        if !(-1..=1).contains(&step) {
            return Err("Face step must be in -1..1.".to_owned());
        }
        if let Some(program) = &self.model().program {
            if !program.faces.is_empty() {
                self.face =
                    (self.face as i32 + step).rem_euclid(program.faces.len() as i32) as usize;
            }
        }
        Ok(())
    }

    pub fn selected_face(&self) -> Option<(usize, &Face)> {
        self.model()
            .program
            .as_ref()?
            .faces
            .get(self.face)
            .map(|f| (self.face, f))
    }

    pub fn turn(&mut self, yaw: i32, pitch: i32) -> Result<(), String> {
        if !(-1..=1).contains(&yaw) || !(-1..=1).contains(&pitch) {
            return Err("Model lab turn must be in -1..1.".to_owned());
        }
        self.yaw = (self.yaw + yaw).rem_euclid(72);
        self.pitch = (self.pitch + pitch).rem_euclid(72);
        Ok(())
    }

    pub fn set_animation(&mut self, value: u16) {
        self.animation = value;
    }

    pub fn animation(&self) -> u16 {
        self.animation
    }

    pub fn model(&self) -> &StationVertices {
        &self.models[self.selected]
    }

    pub fn normals(&self) -> Result<Option<Vec<Normal>>, String> {
        let model = self.model();
        let Some((records, count)) = &model.normals else {
            return Ok(None);
        };
        let yaw = (f64::from(self.yaw) * 5.0).to_radians();
        let pitch = (f64::from(self.pitch) * 5.0).to_radians();
        // Only this diagnostic camera uses floating point. The original normal test
        // receives explicit local integer observer/light inputs, not flight state.
        let observer = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        ]
        .map(|v| (-1024.0 * v) as i32);
        model_normals::evaluate(
            records,
            *count,
            &model.records,
            model.count,
            &NormalContext {
                scale: model.scale,
                observer_scale: model.scale,
                observer,
                light: [0, 0, -65536],
                shades: std::array::from_fn(|i| [i as u8; 3]),
            },
        )
        .map(Some)
    }

    pub fn points(&self) -> Result<Vec<(i32, i32, bool)>, String> {
        let model = self.model();
        let points = evaluate(
            &model.records,
            model.count,
            &context(model.scale, self.animation),
        )?;
        let radius = points.iter().fold(model.radius, |radius, point| {
            radius.max(
                point
                    .iter()
                    .map(|p| f64::from(*p).powi(2))
                    .sum::<f64>()
                    .sqrt(),
            )
        });
        let yaw = (f64::from(self.yaw) * 5.0).to_radians();
        let pitch = (f64::from(self.pitch) * 5.0).to_radians();
        Ok(points
            .iter()
            .enumerate()
            .map(|(i, point)| {
                let [x, y, z] = point.map(|p| f64::from(p) / radius);
                let rx = x * yaw.cos() - z * yaw.sin();
                let rz = x * yaw.sin() + z * yaw.cos();
                let ry = y * pitch.cos() - rz * pitch.sin();
                (
                    160 + (rx * 52.0) as i32,
                    74 - (ry * 35.0) as i32,
                    !matches!(model.records[i / 2 * 6], 0..=2 | 9 | 10),
                )
            })
            .collect())
    }
}
