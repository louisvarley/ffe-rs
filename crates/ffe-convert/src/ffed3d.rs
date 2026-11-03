//! Finding a player's own FFED3D download and its replacement models
//! (`docs/notes/ffed3d-models.md`). The player puts FFED3D (for example the FFED3DAJ "full
//! download") in an `ffed3d` folder beside the game; nothing from it is part of this project.
//! Each `MODELS/<slot>/` folder replaces original model `slot`: one `.X` mesh and its skins.

use std::path::{Path, PathBuf};

/// The folder the game looks for beside its root.
pub const FOLDER: &str = "ffed3d";

/// One replacement model folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The original model slot it replaces.
    pub slot: u32,
    pub dir: PathBuf,
    /// The `.X` mesh in it (`model.x`, else the first `.x` file).
    pub mesh: Option<PathBuf>,
}

/// The `MODELS` folder under `root`: `root/MODELS`, or one level down
/// (`root/full-download/MODELS`, as the download unpacks), case-insensitively.
pub fn models_dir(root: &Path) -> Option<PathBuf> {
    let find = |dir: &Path| -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("MODELS"))
            })
    };
    if let Some(found) = find(root) {
        return Some(found);
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    children.iter().find_map(|child| find(child))
}

/// Every numbered model folder under `models`, by slot.
pub fn entries(models: &Path) -> Vec<Entry> {
    let mut list: Vec<Entry> = std::fs::read_dir(models)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let slot = dir.file_name()?.to_str()?.parse::<u32>().ok()?;
            dir.is_dir().then(|| Entry {
                slot,
                mesh: mesh_in(&dir),
                dir,
            })
        })
        .collect();
    list.sort_by_key(|entry| entry.slot);
    list
}

fn mesh_in(dir: &Path) -> Option<PathBuf> {
    let mut meshes: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("x"))
        })
        .collect();
    meshes.sort();
    meshes
        .iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem.eq_ignore_ascii_case("model"))
        })
        .or(meshes.first())
        .cloned()
}

/// A skin file named by a material, looked up in the model's folder case-insensitively.
pub fn texture_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let wanted = Path::new(name).file_name()?.to_str()?.to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase() == wanted)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_folder_is_found_at_the_top_or_one_level_down() {
        let root = tempdir();
        let models = root.join("full-download").join("MODELS");
        std::fs::create_dir_all(models.join("23")).unwrap();
        std::fs::create_dir_all(models.join("notes")).unwrap();
        std::fs::write(models.join("23").join("Model.X"), b"xof 0303txt 0032").unwrap();
        std::fs::create_dir_all(models.join("7")).unwrap();
        assert_eq!(models_dir(&root), Some(models.clone()));
        let list = entries(&models);
        assert_eq!(list.iter().map(|e| e.slot).collect::<Vec<_>>(), [7, 23]);
        assert_eq!(list[0].mesh, None);
        assert!(list[1].mesh.as_ref().unwrap().ends_with("Model.X"));
        std::fs::write(models.join("23").join("Skin.PNG"), b"").unwrap();
        assert!(texture_path(&models.join("23"), "skin.png").is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(
            format!(
                "ffe-ffed3d-test-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            )
            .replace([':', ' '], "-"),
        );
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
