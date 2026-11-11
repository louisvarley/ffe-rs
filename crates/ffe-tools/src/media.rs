//! Lossless, fingerprinted CD media copied beside data.bin for future streaming.
use crate::{find_child, read_bounded, MAX_ASSET_BYTES};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MediaFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct MediaManifest {
    pub format: &'static str,
    pub version: u32,
    pub files: Vec<MediaFile>,
}

/// Copy all DATA AVI/BMP files byte-for-byte; never modify the supplied files.
/// An existing identical output is accepted; stale/mixed outputs are rejected.
/// Absence of DATA is normal for a floppy installation and creates no folder.
pub fn export_media(root: &Path, output: &Path) -> Result<MediaManifest, String> {
    let mut manifest = MediaManifest {
        format: "ffe-cd-media",
        version: 1,
        files: Vec::new(),
    };
    let Some(data) = find_child(root, "DATA")? else {
        return Ok(manifest);
    };
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if parent.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("FMVs output must be outside the source installation".to_owned());
    }
    let destination = parent.join(output.file_name().ok_or("missing FMVs folder name")?);
    let mut entries = Vec::new();
    for entry in fs::read_dir(&data).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        if !name.ends_with(".AVI") && !name.ends_with(".BMP") {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
        {
            return Err(format!("invalid CD media file: {name}"));
        }
        entries.push((name, entry.path()));
    }
    entries.sort();
    if entries.len() > 2048 || entries.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("too many or ambiguous CD media files".to_owned());
    }
    if entries.is_empty() {
        return Ok(manifest);
    }
    let existing = destination.try_exists().map_err(|e| e.to_string())?;
    if existing
        && fs::symlink_metadata(&destination)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("FMVs output must not be a symbolic link".to_owned());
    }
    let staging = parent.join(format!(".FMVs-{}.tmp", std::process::id()));
    if !existing {
        fs::create_dir(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    }
    let mut total = 0u64;
    for (name, source) in entries {
        let bytes = read_bounded(&source, MAX_ASSET_BYTES)?;
        total += bytes.len() as u64;
        if total > 1024 * 1024 * 1024 {
            return Err("CD media exceeds 1 GiB limit".to_owned());
        }
        if name.ends_with(".AVI")
            && !(bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI "))
            || name.ends_with(".BMP") && !bytes.starts_with(b"BM")
        {
            return Err(format!("{name}: unexpected media header"));
        }
        let record = MediaFile {
            path: name,
            size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        };
        if existing {
            if read_bounded(&destination.join(&record.path), MAX_ASSET_BYTES)? != bytes {
                return Err(
                    "existing FMVs differ from selected installation; choose a fresh output folder"
                        .to_owned(),
                );
            }
        } else {
            fs::write(staging.join(&record.path), &bytes).map_err(|e| e.to_string())?;
            if read_bounded(&staging.join(&record.path), MAX_ASSET_BYTES)? != bytes {
                return Err("FMVs copy verification failed".to_owned());
            }
        }
        manifest.files.push(record);
    }
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    if existing {
        if fs::read(destination.join("manifest.json")).map_err(|e| e.to_string())? != json
            || fs::read_dir(&destination)
                .map_err(|e| e.to_string())?
                .count()
                != manifest.files.len() + 1
        {
            return Err(
                "existing FMVs manifest/files differ; choose a fresh output folder".to_owned(),
            );
        }
    } else {
        fs::write(staging.join("manifest.json"), json).map_err(|e| e.to_string())?;
        fs::rename(staging, destination).map_err(|e| e.to_string())?;
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_both_formats_and_rejects_stale_media() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("data")).unwrap();
        fs::write(game.join("data/a.avi"), b"RIFF1234AVI payload").unwrap();
        fs::write(game.join("data/a.bmp"), b"BMstill").unwrap();
        let output = temp.path().join("FMVs");
        let report = export_media(&game, &output).unwrap();
        assert_eq!(report.files.len(), 2);
        assert_eq!(
            fs::read(output.join("A.AVI")).unwrap(),
            b"RIFF1234AVI payload"
        );
        assert_eq!(fs::read(output.join("A.BMP")).unwrap(), b"BMstill");
        export_media(&game, &output).unwrap();
        fs::write(game.join("data/a.bmp"), b"BMchanged").unwrap();
        assert!(export_media(&game, &output).is_err());
    }
    #[test]
    fn floppy_does_not_create_media_folder() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("FMVs");
        assert!(export_media(temp.path(), &out).unwrap().files.is_empty());
        assert!(!out.exists());
    }
}
