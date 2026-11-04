//! Where Modern graphics find the owner's own images: `assets.bin`, packed from
//! the `assets` folder by `ffe-tools build-assets` (`ffe_content::art_pack`),
//! else the `assets` folder itself (art not packed yet). Paths are the pack's:
//! `Textures/tex450.png` and `Textures/CABIN0.png` (named as FFED3D names its
//! files; they override FFED3D's), and the image export's `images/...` and
//! `indexed/...`. Names match in any letter case.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum ArtSource {
    /// A packed `assets.bin`: its path and files, keyed by lower-case path.
    Pack {
        path: PathBuf,
        files: Arc<BTreeMap<String, Vec<u8>>>,
    },
    /// The `assets` folder.
    Folder(PathBuf),
}

impl ArtSource {
    /// `root/assets.bin` when it reads, else `root/assets` when it exists.
    /// Says which, or why the pack was not used.
    pub fn open(root: &Path) -> (Option<Self>, String) {
        use ffe_content::art_pack::{read_pack, FOLDER, PACK_FILE};
        let pack = root.join(PACK_FILE);
        let folder = root.join(FOLDER);
        let fallback = |why: String| {
            if folder.is_dir() {
                (
                    Some(Self::Folder(folder.clone())),
                    format!("{why}; reading {} directly", folder.display()),
                )
            } else {
                (None, why)
            }
        };
        match std::fs::read(&pack) {
            Ok(bytes) => match read_pack(&bytes) {
                Ok(read) => {
                    let count = read.manifest.images.len();
                    let files = read
                        .files
                        .into_iter()
                        .map(|(path, bytes)| (path.to_ascii_lowercase(), bytes))
                        .collect();
                    (
                        Some(Self::Pack {
                            path: pack.clone(),
                            files: Arc::new(files),
                        }),
                        format!("{count} images from {}", pack.display()),
                    )
                }
                Err(error) => fallback(format!("{}: {error}", pack.display())),
            },
            Err(_) => fallback(format!("no {}", pack.display())),
        }
    }

    /// A file of the folder, its parts in any letter case.
    fn folder_file(folder: &Path, path: &str) -> Option<PathBuf> {
        path.split('/')
            .try_fold(folder.to_path_buf(), |dir, part| {
                crate::ffed3d::find(&dir, part)
            })
            .filter(|found| found.is_file())
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Self::Pack { files, .. } => files.get(&path.to_ascii_lowercase()).cloned(),
            Self::Folder(folder) => std::fs::read(Self::folder_file(folder, path)?).ok(),
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        match self {
            Self::Pack { files, .. } => files.contains_key(&path.to_ascii_lowercase()),
            Self::Folder(folder) => Self::folder_file(folder, path).is_some(),
        }
    }

    /// When the file last changed: a folder's file's time; a pack's files never change.
    pub fn stamp(&self, path: &str) -> Option<SystemTime> {
        match self {
            Self::Pack { .. } => self.exists(path).then_some(SystemTime::UNIX_EPOCH),
            Self::Folder(folder) => std::fs::metadata(Self::folder_file(folder, path)?)
                .and_then(|meta| meta.modified())
                .ok(),
        }
    }

    /// The file as the log names it.
    pub fn describe(&self, path: &str) -> String {
        match self {
            Self::Pack { path: pack, .. } => format!("{}:{path}", pack.display()),
            Self::Folder(folder) => folder.join(path).display().to_string(),
        }
    }

    /// The first of `paths` that exists.
    pub fn first<'a>(&self, paths: &[&'a str]) -> Option<&'a str> {
        paths.iter().copied().find(|path| self.exists(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_reads_names_in_any_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("assets/Textures")).unwrap();
        std::fs::write(dir.path().join("assets/Textures/CABIN0.png"), b"x").unwrap();
        let (source, line) = ArtSource::open(dir.path());
        let source = source.expect("the folder");
        assert!(line.contains("reading"), "{line}");
        assert_eq!(
            source.read("textures/cabin0.png").as_deref(),
            Some(&b"x"[..])
        );
        assert!(source.exists("Textures/CABIN0.png"));
        assert!(!source.exists("Textures/CABIN1.png"));
        assert_eq!(
            source.first(&["Textures/CABIN1.png", "Textures/cabin0.png"]),
            Some("Textures/cabin0.png")
        );
    }

    #[test]
    fn a_pack_is_preferred_to_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(assets.join("Textures")).unwrap();
        // A tiny real PNG header is enough for the pack.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(2u32.to_be_bytes());
        png.extend(2u32.to_be_bytes());
        png.extend([8, 2, 0, 0, 0]);
        std::fs::write(assets.join("Textures/tex450.png"), &png).unwrap();
        let (bytes, _) = ffe_content::art_pack::build_pack(&assets).unwrap();
        std::fs::write(dir.path().join("assets.bin"), bytes).unwrap();
        // The folder changes after packing: the pack is what is read.
        std::fs::remove_file(assets.join("Textures/tex450.png")).unwrap();
        let (source, line) = ArtSource::open(dir.path());
        let source = source.unwrap();
        assert!(matches!(source, ArtSource::Pack { .. }), "{line}");
        assert_eq!(source.read("TEXTURES/TEX450.PNG"), Some(png));
    }
}
