//! `inspect-ffed3d`: read every replacement model in a player's own FFED3D download and
//! report what the game would use (read-only; nothing is copied or converted).
use ffe_convert::ffed3d::{entries, models_dir, texture_path};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ModelReport {
    pub slot: u32,
    pub mesh: Option<String>,
    pub triangles: usize,
    pub materials: usize,
    /// Skins named by materials that are present / missing in the folder.
    pub skins: usize,
    pub missing_skins: Vec<String>,
    pub bounds: Option<([f32; 3], [f32; 3])>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Ffed3dReport {
    pub models_dir: String,
    pub models: Vec<ModelReport>,
    pub readable: usize,
}

pub fn inspect_ffed3d(root: &Path) -> Result<Ffed3dReport, String> {
    let models = models_dir(root).ok_or_else(|| {
        format!(
            "{}: no MODELS folder here or one level down; put the FFED3D download in it",
            root.display()
        )
    })?;
    let mut report = Ffed3dReport {
        models_dir: models.display().to_string(),
        models: Vec::new(),
        readable: 0,
    };
    for entry in entries(&models) {
        let mut row = ModelReport {
            slot: entry.slot,
            mesh: entry
                .mesh
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            triangles: 0,
            materials: 0,
            skins: 0,
            missing_skins: Vec::new(),
            bounds: None,
            error: None,
        };
        match entry.mesh.as_ref().map(std::fs::read) {
            None => row.error = Some("no .x mesh".to_owned()),
            Some(Err(error)) => row.error = Some(error.to_string()),
            Some(Ok(bytes)) => match ffe_convert::xfile::read(&bytes) {
                Ok(model) => {
                    row.triangles = model.triangles.len();
                    row.materials = model.materials.len();
                    row.bounds = model.bounds();
                    for name in model.materials.iter().filter_map(|m| m.texture.as_ref()) {
                        if texture_path(&entry.dir, name).is_some() {
                            row.skins += 1;
                        } else if !row.missing_skins.contains(name) {
                            row.missing_skins.push(name.clone());
                        }
                    }
                    report.readable += 1;
                }
                Err(error) => row.error = Some(error),
            },
        }
        report.models.push(row);
    }
    Ok(report)
}
