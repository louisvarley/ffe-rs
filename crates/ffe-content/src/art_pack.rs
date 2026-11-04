//! `assets.bin`: the owner's high-definition images for Modern graphics, packed
//! from the image export folder (`assets/`, written by `ffe-tools export-images`
//! and filled by the owner's upscaler). Only images whose size differs from the
//! original's (`source_size` in the export's `manifest.json`) go in, with the
//! indexed key each one is recoloured through (`indexed/...`) and the font
//! atlas's `main.json`; an image still at its original size is the original and
//! is left out. Local and untracked like `data.bin`: built from the player's own
//! files, never distributed.
//!
//! It also packs, as they are, every file under `Textures/` and `MODELS/`:
//! the owner's replacements named as FFED3D names its own (`Textures/tex450.png`,
//! `Textures/CABIN0.png`, later `MODELS/<n>/skin.png`), which override FFED3D's.
//!
//! The archive is a deterministic stored ZIP: `manifest.json` first, then the
//! files in path order, each with its size and SHA-256 in the manifest. Paths
//! keep the export's layout, so a reader finds `images/cockpit/cabin0.png` in
//! the pack as in the folder.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The file the build writes beside `data` (the game's root).
pub const PACK_FILE: &str = "assets.bin";
/// The export folder it is built from.
pub const FOLDER: &str = "assets";
pub const FORMAT: &str = "ffe-rs art pack";
pub const FORMAT_VERSION: u32 = 1;
/// Bounds on what a pack may hold.
const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PACK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// One high-definition image: its export id and category, the original's size,
/// the picture's size and the files it brings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackImage {
    /// `upscale` (an export image larger than the original) or `override` (a file
    /// named as FFED3D's, under `Textures/` or `MODELS/`).
    #[serde(default = "upscale")]
    pub kind: String,
    pub id: String,
    pub category: String,
    pub source_size: [u32; 2],
    pub size: [u32; 2],
    pub image: String,
    pub indexed_image: Option<String>,
}

/// One file of the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub format: String,
    pub format_version: u32,
    /// The export manifest's `source_profile` (which release the export came from).
    pub source_profile: serde_json::Value,
    pub images: Vec<PackImage>,
    pub files: Vec<PackFile>,
}

/// A read and verified pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtPack {
    pub manifest: PackManifest,
    pub files: BTreeMap<String, Vec<u8>>,
}

fn upscale() -> String {
    "upscale".to_owned()
}

/// The files under `dir/folder`, any letter case for `folder`, as pack paths
/// (`folder` as FFED3D spells it, then the rest as on disk), in path order.
fn overrides(dir: &Path, folder: &str) -> Vec<(String, std::path::PathBuf)> {
    let Some(root) = std::fs::read_dir(dir).ok().and_then(|entries| {
        entries
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(folder)
                    && entry.path().is_dir()
            })
            .map(|entry| entry.path())
    }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pending = vec![(folder.to_owned(), root)];
    while let Some((prefix, path)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = format!("{prefix}/{name}");
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

/// A relative path inside the export: forward slashes, no `..`, no root.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The export manifest's image records, as far as the pack reads them.
#[derive(Deserialize)]
struct ExportManifest {
    #[serde(default)]
    source_profile: serde_json::Value,
    images: Vec<ExportImage>,
}

#[derive(Deserialize)]
struct ExportImage {
    id: String,
    category: String,
    source_size: [u32; 2],
    image: String,
    indexed_image: Option<String>,
}

/// Build the pack from the export folder `dir`: every image whose PNG size
/// differs from its `source_size`, with its indexed key and, beside a font
/// atlas, its `.json`. Returns the pack's bytes and its manifest.
pub fn build_pack(dir: &Path) -> Result<(Vec<u8>, PackManifest), String> {
    let manifest_path = dir.join("manifest.json");
    let export: ExportManifest = if manifest_path.is_file() {
        serde_json::from_slice(
            &std::fs::read(&manifest_path)
                .map_err(|e| format!("{}: {e}", manifest_path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?
    } else {
        ExportManifest {
            source_profile: serde_json::Value::Null,
            images: Vec::new(),
        }
    };
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut images = Vec::new();
    let read = |path: &str, files: &mut BTreeMap<String, Vec<u8>>| -> Result<(), String> {
        if !safe(path) {
            return Err(format!("unsafe path in the export manifest: {path}"));
        }
        if files.contains_key(path) {
            return Ok(());
        }
        let full = dir.join(path);
        let bytes = std::fs::read(&full).map_err(|e| format!("{}: {e}", full.display()))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{}: larger than {MAX_FILE_BYTES} bytes",
                full.display()
            ));
        }
        files.insert(path.to_owned(), bytes);
        Ok(())
    };
    for image in &export.images {
        if !safe(&image.image) {
            return Err(format!(
                "unsafe path in the export manifest: {}",
                image.image
            ));
        }
        let Ok(bytes) = std::fs::read(dir.join(&image.image)) else {
            continue;
        };
        let size = png_size(&bytes)
            .ok_or_else(|| format!("{}: not a PNG", dir.join(&image.image).display()))?;
        if size == image.source_size {
            continue;
        }
        read(&image.image, &mut files)?;
        if let Some(indexed) = &image.indexed_image {
            if dir.join(indexed).is_file() {
                read(indexed, &mut files)?;
            }
        }
        // A font atlas's glyph table (`main.json` beside `main.png`).
        if let Some(stem) = image.image.strip_suffix(".png") {
            let json = format!("{stem}.json");
            if dir.join(&json).is_file() {
                read(&json, &mut files)?;
            }
        }
        images.push(PackImage {
            kind: upscale(),
            id: image.id.clone(),
            category: image.category.clone(),
            source_size: image.source_size,
            size,
            image: image.image.clone(),
            indexed_image: image
                .indexed_image
                .clone()
                .filter(|indexed| files.contains_key(indexed)),
        });
    }
    // The owner's replacements under FFED3D's names, as they are.
    for (path, full) in ["Textures", "MODELS"]
        .iter()
        .flat_map(|folder| overrides(dir, folder))
    {
        let bytes = std::fs::read(&full).map_err(|e| format!("{}: {e}", full.display()))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{}: larger than {MAX_FILE_BYTES} bytes",
                full.display()
            ));
        }
        if let Some(size) = png_size(&bytes) {
            images.push(PackImage {
                kind: "override".to_owned(),
                id: format!("ffed3d:{path}"),
                category: "ffed3d".to_owned(),
                source_size: [0, 0],
                size,
                image: path.clone(),
                indexed_image: None,
            });
        }
        files.insert(path, bytes);
    }
    images.sort_by(|a, b| a.image.cmp(&b.image));
    let manifest = PackManifest {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        source_profile: export.source_profile,
        images,
        files: files
            .iter()
            .map(|(path, bytes)| PackFile {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: digest(bytes),
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
    for image in &manifest.images {
        if !files.contains_key(&image.image) {
            return Err(format!("{}: listed but not in the art pack", image.image));
        }
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
    fn only_images_bigger_than_the_original_are_packed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for folder in [
            "images/cockpit",
            "indexed/cockpit",
            "images/font",
            "indexed/font",
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(root.join("images/cockpit/cabin0.png"), png(1280, 632, 1)).unwrap();
        std::fs::write(root.join("indexed/cockpit/cabin0.png"), png(320, 158, 2)).unwrap();
        std::fs::write(root.join("images/cockpit/cabin1.png"), png(320, 158, 3)).unwrap();
        std::fs::write(root.join("indexed/cockpit/cabin1.png"), png(320, 158, 4)).unwrap();
        std::fs::write(root.join("images/font/main.png"), png(1600, 880, 5)).unwrap();
        std::fs::write(root.join("images/font/main.json"), b"{}").unwrap();
        let manifest = serde_json::json!({
            "source_profile": {"name": "floppy"},
            "images": [
                {"id": "base:cockpit/cabin0", "category": "cockpit", "source_size": [320, 158],
                 "image": "images/cockpit/cabin0.png", "indexed_image": "indexed/cockpit/cabin0.png"},
                {"id": "base:cockpit/cabin1", "category": "cockpit", "source_size": [320, 158],
                 "image": "images/cockpit/cabin1.png", "indexed_image": "indexed/cockpit/cabin1.png"},
                {"id": "base:font/main", "category": "font", "source_size": [160, 88],
                 "image": "images/font/main.png", "indexed_image": "indexed/font/main.png"}
            ]
        });
        std::fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
        // An FFED3D-named replacement, packed as it is.
        std::fs::create_dir_all(root.join("textures")).unwrap();
        std::fs::write(root.join("textures/tex450.png"), png(512, 512, 6)).unwrap();
        let (bytes, manifest) = build_pack(root).unwrap();
        let ids: Vec<&str> = manifest.images.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "ffed3d:Textures/tex450.png",
                "base:cockpit/cabin0",
                "base:font/main"
            ]
        );
        assert_eq!(manifest.images[0].kind, "override");
        let pack = read_pack(&bytes).unwrap();
        let paths: Vec<&str> = pack.files.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "Textures/tex450.png",
                "images/cockpit/cabin0.png",
                "images/font/main.json",
                "images/font/main.png",
                "indexed/cockpit/cabin0.png"
            ]
        );
        assert_eq!(
            pack.manifest.images[2].indexed_image, None,
            "no font key on disk"
        );
        // Deterministic: the same folder packs to the same bytes.
        assert_eq!(build_pack(root).unwrap().0, bytes);
        // A changed payload fails its checksum.
        let mut bad = bytes.clone();
        let at = bad.windows(4).rposition(|w| w == b"IHDR").unwrap();
        bad[at + 4] ^= 1;
        assert!(read_pack(&bad).is_err());
    }

    #[test]
    fn unsafe_paths_are_refused() {
        assert!(!safe("../x.png"));
        assert!(!safe("/x.png"));
        assert!(!safe("images\\x.png"));
        assert!(safe("images/cockpit/cabin0.png"));
    }
}
