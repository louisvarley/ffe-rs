//! `assets.bin`: the owner's replacements of FFED3D's art for Modern graphics,
//! packed from the `assets` folder by `ffe-tools build-assets` (run by the
//! build). The folder mirrors an FFED3D install (`Textures/CABIN0.png`,
//! `Textures/tex450.png`, `MODELS/<n>/skin.png`, ...): a file at the same path
//! as FFED3D's replaces it. Every file in the folder is packed, under its path.
//! Local and untracked like `data.bin`: built from the player's own files,
//! never distributed.
//!
//! The archive is a deterministic stored ZIP: `manifest.json` first, then the
//! files in path order, each with its size and SHA-256 in the manifest.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The file the build writes in the game's data folder.
pub const PACK_FILE: &str = "assets.bin";
/// The folder it is built from.
pub const FOLDER: &str = "assets";
pub const FORMAT: &str = "ffe-rs art pack";
/// Version 2: every file of the folder under its FFED3D path.
pub const FORMAT_VERSION: u32 = 2;
/// Bounds on what a pack may hold.
const MAX_FILES: usize = 8192;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PACK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// One file of the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// A PNG's width and height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub format: String,
    pub format_version: u32,
    pub files: Vec<PackFile>,
}

/// A read and verified pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtPack {
    pub manifest: PackManifest,
    pub files: BTreeMap<String, Vec<u8>>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A PNG's width and height from its header.
pub fn png_size(bytes: &[u8]) -> Option<[u32; 2]> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    Some([word(16), word(20)])
}

/// A relative path inside the pack: forward slashes, no `..`, no root.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Every file under `dir` as pack paths, in path order (Windows' `desktop.ini`
/// and `Thumbs.db` left out; links are not followed).
fn files_under(dir: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out = Vec::new();
    let mut pending = vec![(String::new(), dir.to_path_buf())];
    while let Some((prefix, path)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
                continue;
            }
            let child = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push((child, entry.path()));
            } else if kind.is_file() && safe(&child) {
                out.push((child, entry.path()));
            }
        }
    }
    out.sort();
    out
}

/// Build the pack from the folder `dir`: every file under its path.
pub fn build_pack(dir: &Path) -> Result<(Vec<u8>, PackManifest), String> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (path, full) in files_under(dir) {
        let bytes = std::fs::read(&full).map_err(|e| format!("{}: {e}", full.display()))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{}: larger than {MAX_FILE_BYTES} bytes",
                full.display()
            ));
        }
        files.insert(path, bytes);
    }
    if files.len() > MAX_FILES {
        return Err(format!("{}: more than {MAX_FILES} files", dir.display()));
    }
    let manifest = PackManifest {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        files: files
            .iter()
            .map(|(path, bytes)| PackFile {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: digest(bytes),
                image: png_size(bytes),
            })
            .collect(),
    };
    let mut json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    json.push(b'\n');
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default())
        .large_file(true);
    writer
        .start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    writer.write_all(&json).map_err(|e| e.to_string())?;
    for (path, bytes) in &files {
        writer
            .start_file(path, options)
            .map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
    }
    let bytes = writer.finish().map_err(|e| e.to_string())?.into_inner();
    // The reader's contract before the pack is written.
    read_pack(&bytes)?;
    Ok((bytes, manifest))
}

/// Read and verify a pack: a stored `manifest.json` first, then exactly the
/// manifest's files in order, each with its size and SHA-256.
pub fn read_pack(bytes: &[u8]) -> Result<ArtPack, String> {
    if bytes.len() as u64 > MAX_PACK_BYTES {
        return Err("art pack exceeds the size limit".to_owned());
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("art pack ZIP: {e}"))?;
    if archive.is_empty() || archive.len() > MAX_FILES + 1 {
        return Err("art pack has an invalid number of entries".to_owned());
    }
    let mut json = Vec::new();
    {
        let mut entry = archive.by_index(0).map_err(|e| e.to_string())?;
        if entry.name() != "manifest.json"
            || entry.size() > MAX_MANIFEST_BYTES
            || entry.compression() != CompressionMethod::Stored
        {
            return Err("art pack must begin with a stored manifest.json".to_owned());
        }
        entry
            .by_ref()
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut json)
            .map_err(|e| e.to_string())?;
    }
    let manifest: PackManifest =
        serde_json::from_slice(&json).map_err(|e| format!("art pack manifest: {e}"))?;
    if manifest.format != FORMAT || manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "art pack format {} version {} is not {FORMAT} version {FORMAT_VERSION}",
            manifest.format, manifest.format_version
        ));
    }
    if archive.len() != manifest.files.len() + 1 {
        return Err("art pack entries do not match its manifest".to_owned());
    }
    let mut files = BTreeMap::new();
    for (index, record) in manifest.files.iter().enumerate() {
        if !safe(&record.path) || record.size > MAX_FILE_BYTES {
            return Err(format!("{}: invalid art pack entry", record.path));
        }
        let mut entry = archive.by_index(index + 1).map_err(|e| e.to_string())?;
        if entry.name() != record.path
            || entry.size() != record.size
            || entry.compression() != CompressionMethod::Stored
            || entry.is_dir()
        {
            return Err(format!("{}: unexpected art pack entry", record.path));
        }
        let mut payload = Vec::new();
        entry
            .by_ref()
            .take(record.size + 1)
            .read_to_end(&mut payload)
            .map_err(|e| format!("{}: {e}", record.path))?;
        if payload.len() as u64 != record.size || digest(&payload) != record.sha256 {
            return Err(format!("{}: art pack checksum mismatch", record.path));
        }
        files.insert(record.path.clone(), payload);
    }
    Ok(ArtPack { manifest, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG header of `width` x `height` (enough for `png_size`).
    fn png(width: u32, height: u32, fill: u8) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 2, 0, 0, 0, fill]);
        bytes
    }

    #[test]
    fn every_file_is_packed_under_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Textures")).unwrap();
        std::fs::create_dir_all(root.join("MODELS/30")).unwrap();
        std::fs::write(root.join("Textures/CABIN0.png"), png(1280, 632, 1)).unwrap();
        std::fs::write(root.join("Textures/desktop.ini"), b"x").unwrap();
        std::fs::write(root.join("MODELS/30/skins.ini"), b"[SKINS]").unwrap();
        let (bytes, manifest) = build_pack(root).unwrap();
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["MODELS/30/skins.ini", "Textures/CABIN0.png"]);
        assert_eq!(manifest.files[1].image, Some([1280, 632]));
        assert_eq!(manifest.files[0].image, None);
        let pack = read_pack(&bytes).unwrap();
        assert_eq!(pack.files["MODELS/30/skins.ini"], b"[SKINS]");
        // Deterministic: the same folder packs to the same bytes.
        assert_eq!(build_pack(root).unwrap().0, bytes);
        // A changed payload fails its checksum.
        let mut bad = bytes.clone();
        let at = bad.windows(7).rposition(|w| w == b"[SKINS]").unwrap();
        bad[at] ^= 1;
        assert!(read_pack(&bad).is_err());
    }

    #[test]
    fn unsafe_paths_are_refused() {
        assert!(!safe("../x.png"));
        assert!(!safe("/x.png"));
        assert!(!safe("images\\x.png"));
        assert!(safe("Textures/CABIN0.png"));
    }
}
