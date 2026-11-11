//! The CD 1.01 videos, loaded from the `FMVs` folder beside the data root.
//!
//! The launcher (or `ffe-tools export-media`) copies the CD's `DATA\*.AVI` and
//! `*.BMP` into `FMVs` with a `manifest.json` of names, sizes and SHA-256
//! digests. The library is offered only when the game's content was built from
//! the CD 1.01 executable, so a floppy installation never picks up another
//! release's videos. Each clip is checked against the manifest's size before
//! [`ffe_convert::avi::decode_avi`] decodes it.
//!
//! Which game event plays which clip is not known yet: the CD executable
//! builds each path (`%c:` + `\DATA` + name + `AVI`) in code that JJFFE 2.8a7,
//! built from the floppy release, does not contain (burndown `content-7`).

use ffe_convert::avi::{decode_avi, Video, MAX_AVI_BYTES};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The source profile whose installation has videos.
pub const CD_PROFILE: &str = "ffe-cd-1.01-en";
/// The folder name beside the data root.
pub const FOLDER: &str = "FMVs";
/// The largest manifest read.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    files: Vec<ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    path: String,
    size: u64,
}

/// The CD's videos: clip name (upper case, no extension) to file and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    dir: PathBuf,
    clips: BTreeMap<String, (PathBuf, u64)>,
}

impl Library {
    /// The library for content built from `profile`, if that is the CD release
    /// and `root/FMVs/manifest.json` lists videos that are all present with
    /// their recorded sizes. `Ok(None)`: no videos apply (not the CD, or no
    /// folder). `Err`: the folder is there but does not match its manifest.
    pub fn find(root: &Path, profile: &str) -> Result<Option<Library>, String> {
        if profile != CD_PROFILE {
            return Ok(None);
        }
        let dir = root.join(FOLDER);
        let manifest = dir.join("manifest.json");
        if !manifest.is_file() {
            return Ok(None);
        }
        let size = std::fs::metadata(&manifest)
            .map_err(|error| format!("{}: {error}", manifest.display()))?
            .len();
        if size > MAX_MANIFEST_BYTES {
            return Err(format!("{} is too large", manifest.display()));
        }
        let text = std::fs::read_to_string(&manifest)
            .map_err(|error| format!("{}: {error}", manifest.display()))?;
        let parsed: Manifest = serde_json::from_str(&text)
            .map_err(|error| format!("{}: {error}", manifest.display()))?;
        let mut clips = BTreeMap::new();
        for file in parsed.files {
            let name = Path::new(&file.path);
            let is_video = name
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("avi"));
            if !is_video {
                continue;
            }
            // Manifest entries are bare file names; anything else is refused.
            if name.components().count() != 1 {
                return Err(format!(
                    "{}: {} is not a file name",
                    manifest.display(),
                    file.path
                ));
            }
            let path = dir.join(name);
            let actual = std::fs::metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .len();
            if actual != file.size {
                return Err(format!(
                    "{} is {actual} bytes, not the {} the manifest records",
                    path.display(),
                    file.size
                ));
            }
            let stem = name
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| format!("{}: unreadable name", file.path))?
                .to_ascii_uppercase();
            clips.insert(stem, (path, file.size));
        }
        if clips.is_empty() {
            return Ok(None);
        }
        Ok(Some(Library { dir, clips }))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Whether clip `name` (`BB1`, `ST12`, case-insensitive) is present.
    pub fn contains(&self, name: &str) -> bool {
        self.clips.contains_key(&name.to_ascii_uppercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.clips.keys().map(String::as_str)
    }

    /// Read and decode clip `name`.
    pub fn load(&self, name: &str) -> Result<Video, String> {
        let (path, size) = self
            .clips
            .get(&name.to_ascii_uppercase())
            .ok_or_else(|| format!("no video {name} in {}", self.dir.display()))?;
        if *size > MAX_AVI_BYTES as u64 {
            return Err(format!(
                "{} is larger than the decoder accepts",
                path.display()
            ));
        }
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if bytes.len() as u64 != *size {
            return Err(format!(
                "{} changed size since it was listed",
                path.display()
            ));
        }
        decode_avi(&bytes).map_err(|error| format!("{}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(files: &[(&str, &[u8])], manifest: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(FOLDER);
        std::fs::create_dir(&dir).unwrap();
        for (name, bytes) in files {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        root
    }

    #[test]
    fn only_the_cd_profile_with_a_matching_folder_has_videos() {
        let root = folder(
            &[("BB1.AVI", b"abcd"), ("BB1.BMP", b"xy")],
            r#"{"format":"ffe-cd-media","version":1,"files":[
                {"path":"BB1.AVI","size":4,"sha256":""},
                {"path":"BB1.BMP","size":2,"sha256":""}]}"#,
        );
        let library = Library::find(root.path(), CD_PROFILE).unwrap().unwrap();
        assert_eq!(library.len(), 1);
        assert!(library.contains("bb1"));
        // Not a decodable clip: reported, not played.
        assert!(library.load("BB1").is_err());
        assert!(library.load("ST1").is_err());
        // The floppy release never takes the CD's videos.
        assert_eq!(
            Library::find(root.path(), "ffe-floppy-1.02-en").unwrap(),
            None
        );
        // No folder: nothing to play.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(Library::find(empty.path(), CD_PROFILE).unwrap(), None);
    }

    #[test]
    fn a_folder_that_differs_from_its_manifest_is_an_error() {
        let root = folder(
            &[("BB1.AVI", b"abc")],
            r#"{"files":[{"path":"BB1.AVI","size":4,"sha256":""}]}"#,
        );
        assert!(Library::find(root.path(), CD_PROFILE).is_err());
        let escape = folder(
            &[],
            r#"{"files":[{"path":"../X.AVI","size":1,"sha256":""}]}"#,
        );
        assert!(Library::find(escape.path(), CD_PROFILE).is_err());
    }
}
