use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 2048;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetKind {
    IndexedBitmap {
        table: String,
        index: u32,
        width: u32,
        height: u32,
        offset_x: i16,
        offset_y: i16,
        masked_zero: bool,
        source_offset: u64,
    },
    LowPaletteRgb6,
    SineTable {
        source_offset: u64,
    },
    BitmapFont {
        first_code: u8,
        glyph_count: u32,
        rows: u32,
        source_offset: u64,
    },
    ModelVertices {
        index: u32,
        vertex_count: u32,
        scale: u32,
        descriptor_offset: u64,
        source_offset: u64,
    },
    ModelCommands {
        index: u32,
        vertex_count: u32,
        descriptor_offset: u64,
        source_offset: u64,
    },
    ModelNormals {
        index: u32,
        vertex_count: u32,
        normal_count: u32,
        descriptor_offset: u64,
        source_offset: u64,
    },
    TextureBlock {
        table: String,
        index: u32,
        width: u32,
        source_offset: u64,
    },
    CabinPng {
        width: u32,
        height: u32,
    },
    EffectWav,
    /// The original texture descriptor table `DATA_007811` (v8).
    TextureDescriptors {
        count: u32,
        source_offset: u64,
    },
    /// The original 12-bit colour to fixed low-palette map `DATA_007770` (v8).
    FixedColourMap {
        source_offset: u64,
    },
    /// Per model table slot, descriptor +0x18 (child scale offset) and +0x1c (F1687
    /// cull radius) as two little-endian dwords; zero for slots not imported (v10).
    ModelDescriptors {
        count: u32,
    },
    /// The model name table `DATA_004682`: `count` NUL-terminated Latin-1 strings,
    /// entry n naming model table slot n (v11).
    ModelNames {
        count: u32,
        source_offset: u64,
    },
    /// Per model table slot, the 48-byte statistics block descriptor +0x38 points to
    /// (thrust limits, gun mounts, mass, name, drive); zero where the pointer is null
    /// or the slot is not imported (v12).
    ShipStats {
        count: u32,
    },
    /// The default light and shade table `DATA_007231..7234` (v9).
    DefaultLighting {
        source_offset: u64,
    },
    /// The star shade tables `DATA_007759` (v9).
    StarShades {
        count: u32,
        source_offset: u64,
    },
    /// The galaxy table and system-generation tables `DATA_007106..7138`, raw
    /// from the executable (v15); see `ffe_core::galaxy`.
    SystemTables {
        source_offset: u64,
    },
    /// Per model table slot, the 20-byte record of the descriptor fields object
    /// set-up reads (scales, radii, shape and statistics flags; v16); see
    /// `ffe_convert::embedded::decode_model_record`.
    ModelRecords {
        count: u32,
    },
    /// The gameplay tables `DATA_007207..7225` (647 raw bytes; v16).
    GameplayTables {
        source_offset: u64,
    },
    /// One font model's per-character glyph command programs (v14): `count`
    /// little-endian u32 start offsets relative to the payload's program block,
    /// then the block itself. The block follows the offsets at
    /// `4 * count` bytes in. `vertex_count` and `scale` are the font model's.
    GlyphPrograms {
        index: u32,
        count: u32,
        first_char: u32,
        vertex_count: u32,
        scale: u32,
        source_offset: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheFile {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub asset: AssetKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub tool_version: String,
    pub source_profile: String,
    pub source_files: Vec<SourceFile>,
    pub complete_game_assets: bool,
    pub unresolved: Vec<String>,
    pub files: Vec<CacheFile>,
}

pub struct Asset {
    pub id: String,
    pub path: String,
    pub kind: AssetKind,
    pub bytes: Vec<u8>,
}

pub struct VerifiedCache {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl VerifiedCache {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn into_manifest(self) -> Manifest {
        self.manifest
    }

    pub fn asset(&self, id: &str) -> Option<&[u8]> {
        let record = self.manifest.files.iter().find(|record| record.id == id)?;
        self.files.get(&record.path).map(Vec::as_slice)
    }
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
                })
        })
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Original model table slots (DATA_004681).
const MODEL_TABLE_SLOTS: u32 = 466;
/// Bytes per ship statistics block (descriptor +0x38).
pub const SHIP_STATS_BYTES: usize = 48;

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.format != "ffe-content-cache" || !matches!(manifest.format_version, 1..=16) {
        return Err("unsupported content cache format/version".to_owned());
    }
    if manifest.complete_game_assets || manifest.unresolved.is_empty() {
        return Err("partial cache must declare its incomplete asset scope".to_owned());
    }
    if manifest.source_profile.is_empty()
        || manifest.tool_version.is_empty()
        || manifest.source_files.is_empty()
        || manifest.source_files.len() > 128
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
    {
        return Err("invalid cache manifest counts or provenance".to_owned());
    }
    let mut source_paths = BTreeSet::new();
    for source in &manifest.source_files {
        if !safe_path(&source.path.to_ascii_lowercase())
            || !valid_digest(&source.sha256)
            || !source_paths.insert(source.path.to_ascii_lowercase())
        {
            return Err("invalid/duplicate source record".to_owned());
        }
    }
    let mut ids = BTreeSet::new();
    let mut previous = "";
    for record in &manifest.files {
        let model_index = match record.asset {
            AssetKind::ModelVertices { index, .. }
            | AssetKind::ModelCommands { index, .. }
            | AssetKind::ModelNormals { index, .. } => Some(index),
            _ => None,
        };
        // Up to v6 only stations 74/75 are imported; v7 imports any table slot.
        if model_index.is_some_and(|index| {
            index >= MODEL_TABLE_SLOTS || (manifest.format_version < 7 && !matches!(index, 74 | 75))
        }) {
            return Err("model programs outside stations 74/75 require cache v7".to_owned());
        }
        if manifest.format_version < 8
            && matches!(
                record.asset,
                AssetKind::TextureDescriptors { .. } | AssetKind::FixedColourMap { .. }
            )
        {
            return Err("Texture descriptors and the colour map require cache v8.".to_owned());
        }
        if manifest.format_version < 11 && matches!(record.asset, AssetKind::ModelNames { .. }) {
            return Err("The model name table requires cache v11.".to_owned());
        }
        if manifest.format_version < 12 && matches!(record.asset, AssetKind::ShipStats { .. }) {
            return Err("The ship statistics table requires cache v12.".to_owned());
        }
        if manifest.format_version < 16
            && matches!(
                record.asset,
                AssetKind::ModelRecords { .. } | AssetKind::GameplayTables { .. }
            )
        {
            return Err("The model records and gameplay tables require cache v16.".to_owned());
        }
        if manifest.format_version < 15 && matches!(record.asset, AssetKind::SystemTables { .. }) {
            return Err("The system tables require cache v15.".to_owned());
        }
        if manifest.format_version < 14 && matches!(record.asset, AssetKind::GlyphPrograms { .. }) {
            return Err("The glyph programs require cache v14.".to_owned());
        }
        if manifest.format_version < 10
            && matches!(record.asset, AssetKind::ModelDescriptors { .. })
        {
            return Err("The model descriptor table requires cache v10.".to_owned());
        }
        if manifest.format_version < 9
            && matches!(
                record.asset,
                AssetKind::DefaultLighting { .. } | AssetKind::StarShades { .. }
            )
        {
            return Err("Lighting tables require cache v9.".to_owned());
        }
        if manifest.format_version < 6 && matches!(record.asset, AssetKind::SineTable { .. }) {
            return Err("Sine tables require cache v6 or later.".to_owned());
        }
        if manifest.format_version < 5 && matches!(record.asset, AssetKind::ModelNormals { .. }) {
            return Err("model normals require content cache version 5".to_owned());
        }
        if manifest.format_version < 4 && matches!(record.asset, AssetKind::ModelCommands { .. }) {
            return Err("model commands require content cache version 4".to_owned());
        }
        if manifest.format_version < 3 && matches!(record.asset, AssetKind::ModelVertices { .. }) {
            return Err("model vertices require content cache version 3".to_owned());
        }
        if manifest.format_version == 1 && matches!(record.asset, AssetKind::BitmapFont { .. }) {
            return Err("bitmap fonts require content cache version 2".to_owned());
        }
        if !safe_path(&record.path)
            || record.path == "manifest.json"
            || record.path.as_str() <= previous
            || !valid_digest(&record.sha256)
            || record.size > MAX_CACHE_BYTES as u64
            || !record.id.starts_with("base:")
            || !safe_path(&record.id[5..])
            || !ids.insert(&record.id)
        {
            return Err(format!(
                "invalid/duplicate/unsorted cache record: {}",
                record.path
            ));
        }
        previous = &record.path;
    }
    Ok(())
}

fn validate_payload(record: &CacheFile, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != record.size || digest(bytes) != record.sha256 {
        return Err(format!("{}: size or SHA-256 mismatch", record.path));
    }
    match &record.asset {
        AssetKind::IndexedBitmap {
            width,
            height,
            table,
            index,
            ..
        } => {
            let count = match table.as_str() {
                "primary" => 96,
                "secondary" => 251,
                _ => return Err("unknown bitmap table".to_owned()),
            };
            if *index >= count
                || *width == 0
                || *width > 320
                || *height == 0
                || *height > 255
                || u64::from(*width) * u64::from(*height) != record.size
            {
                return Err(format!("{}: invalid indexed bitmap shape", record.path));
            }
        }
        AssetKind::LowPaletteRgb6 => {
            if bytes.len() != 384 || bytes.iter().any(|component| *component > 63) {
                return Err("invalid fixed low palette".to_owned());
            }
        }
        AssetKind::BitmapFont {
            first_code,
            glyph_count,
            rows,
            ..
        } => {
            if *first_code != 32
                || *glyph_count != 116
                || *rows != 9
                || bytes.len() != 1160
                || bytes
                    .chunks_exact(10)
                    .any(|glyph| !(1..=9).contains(&glyph[9]))
            {
                return Err("invalid original bitmap font".to_owned());
            }
        }
        AssetKind::ModelVertices {
            vertex_count,
            scale,
            ..
        } => {
            // Child-only programs read parent vertices; those stand in at the origin.
            ffe_core::model::validate_program(bytes, *vertex_count, *scale)?;
        }
        AssetKind::ModelCommands { vertex_count, .. } => {
            ffe_core::model_commands::decode(bytes, *vertex_count)?;
        }
        AssetKind::SineTable { .. } => {
            ffe_core::model_math::SineTable::from_bytes(bytes)?;
        }
        AssetKind::ModelNormals {
            vertex_count,
            normal_count,
            ..
        } => {
            ffe_core::model_normals::validate_records(bytes, *normal_count, *vertex_count)?;
        }
        AssetKind::TextureBlock {
            table,
            index,
            width,
            ..
        } => {
            if table != "primary"
                || !matches!(index, 94 | 95)
                || *width != 64
                || bytes.len() != 32768
            {
                return Err("invalid special texture block".to_owned());
            }
        }
        AssetKind::CabinPng { width, height } => {
            if *width != 320 || *height != 158 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err("invalid cabin PNG descriptor/signature".to_owned());
            }
        }
        AssetKind::TextureDescriptors { count, .. } => {
            if *count as usize != ffe_core::model_texture::DESCRIPTOR_COUNT {
                return Err("invalid texture descriptor count".to_owned());
            }
            ffe_core::model_texture::Descriptors::from_bytes(bytes)?;
        }
        AssetKind::FixedColourMap { .. } => {
            ffe_core::model_texture::validate_colour_map(bytes)?;
        }
        AssetKind::ModelDescriptors { count } => {
            if *count != MODEL_TABLE_SLOTS || bytes.len() != *count as usize * 8 {
                return Err("invalid model descriptor table".to_owned());
            }
        }
        AssetKind::ShipStats { count } => {
            if *count != MODEL_TABLE_SLOTS || bytes.len() != *count as usize * SHIP_STATS_BYTES {
                return Err("invalid ship statistics table".to_owned());
            }
        }
        AssetKind::GlyphPrograms {
            index,
            count,
            first_char,
            vertex_count,
            ..
        } => {
            if *index > 2
                || *first_char != u32::from(ffe_core::model_commands::GLYPH_FIRST_CHAR)
                || *count == 0
                || *count as usize > ffe_core::model_commands::GLYPH_CHAR_LIMIT
                || !(1..=128).contains(vertex_count)
                || vertex_count % 2 != 0
            {
                return Err("invalid glyph program descriptor".to_owned());
            }
            let offsets = *count as usize * 4;
            if bytes.len() <= offsets {
                return Err("glyph program payload is too short".to_owned());
            }
            let starts: Vec<u32> = bytes[..offsets]
                .chunks_exact(4)
                .map(|entry| u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]))
                .collect();
            let block = &bytes[offsets..];
            // Characters may alias another program (lowercase to uppercase, and
            // fonts with a partial set), so the starts are not sorted; the
            // distinct ones delimit the programs.
            if starts[0] != 0 || starts.iter().any(|start| *start as usize >= block.len()) {
                return Err("invalid glyph program starts".to_owned());
            }
            let mut distinct = starts.clone();
            distinct.sort_unstable();
            distinct.dedup();
            for (position, start) in distinct.iter().enumerate() {
                let end = distinct
                    .get(position + 1)
                    .map_or(block.len(), |next| *next as usize);
                let program = block
                    .get(*start as usize..end)
                    .ok_or("glyph program outside the block")?;
                ffe_core::model_commands::decode(program, *vertex_count)
                    .map_err(|error| format!("glyph program at {start:#x}: {error}"))?;
            }
        }
        AssetKind::ModelNames { count, .. } => {
            if *count == 0
                || *count > MODEL_TABLE_SLOTS
                || bytes.last() != Some(&0)
                || bytes.iter().filter(|b| **b == 0).count() != *count as usize
                || bytes.split(|b| *b == 0).any(|name| name.len() > 255)
            {
                return Err("invalid model name table".to_owned());
            }
        }
        AssetKind::DefaultLighting { .. } => {
            ffe_core::model_lighting::default_lighting(bytes)?;
        }
        AssetKind::StarShades { count, .. } => {
            if *count as usize != ffe_core::model_lighting::STAR_SHADE_TABLES {
                return Err("invalid star shade table count".to_owned());
            }
            ffe_core::model_lighting::star_shades(bytes)?;
        }
        AssetKind::ModelRecords { count } => {
            if *count != MODEL_TABLE_SLOTS || bytes.len() != *count as usize * 20 {
                return Err("invalid model record table".to_owned());
            }
        }
        AssetKind::GameplayTables { .. } => {
            if bytes.len() != 647 {
                return Err("invalid gameplay table block".to_owned());
            }
        }
        AssetKind::SystemTables { .. } => {
            ffe_core::galaxy::Tables::new(bytes)?;
        }
        AssetKind::EffectWav => {
            if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..16] != b"WAVEfmt " {
                return Err("invalid effect WAV descriptor/signature".to_owned());
            }
        }
    }
    Ok(())
}

/// The archive is local content, not a release artifact. Stored ZIP entries use
/// fixed DOS timestamps, sorted paths, and no archive comments.
pub fn build(mut manifest: Manifest, mut assets: Vec<Asset>) -> Result<Vec<u8>, String> {
    assets.sort_by(|a, b| a.path.cmp(&b.path));
    manifest.files = assets
        .iter()
        .map(|asset| CacheFile {
            id: asset.id.clone(),
            path: asset.path.clone(),
            size: asset.bytes.len() as u64,
            sha256: digest(&asset.bytes),
            asset: asset.kind.clone(),
        })
        .collect();
    validate_manifest(&manifest)?;
    let total: usize = assets.iter().map(|asset| asset.bytes.len()).sum();
    if total > MAX_CACHE_BYTES / 2 {
        return Err("asset payload exceeds cache build limit".to_owned());
    }
    for (record, asset) in manifest.files.iter().zip(&assets) {
        validate_payload(record, &asset.bytes)?;
    }
    let mut json = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    json.push(b'\n');
    if json.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("cache manifest exceeds size limit".to_owned());
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    writer
        .start_file("manifest.json", options)
        .map_err(|error| error.to_string())?;
    writer.write_all(&json).map_err(|error| error.to_string())?;
    for asset in &assets {
        writer
            .start_file(&asset.path, options)
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&asset.bytes)
            .map_err(|error| error.to_string())?;
    }
    let bytes = writer
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    if bytes.len() > MAX_CACHE_BYTES {
        return Err("encoded cache exceeds size limit".to_owned());
    }
    // Apply the same reader contract before an archive is persisted.
    read(&bytes)?;
    Ok(bytes)
}

pub fn read(bytes: &[u8]) -> Result<VerifiedCache, String> {
    if bytes.len() > MAX_CACHE_BYTES {
        return Err("cache exceeds size limit".to_owned());
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| format!("cache ZIP: {error}"))?;
    if archive.is_empty() || archive.len() > MAX_FILES + 1 || !archive.comment().is_empty() {
        return Err("invalid cache ZIP entry count or comment".to_owned());
    }
    let mut json = Vec::new();
    {
        let mut entry = archive.by_index(0).map_err(|error| error.to_string())?;
        if entry.name() != "manifest.json"
            || entry.size() > MAX_MANIFEST_BYTES
            || entry.compression() != CompressionMethod::Stored
        {
            return Err("cache must begin with a bounded stored manifest.json".to_owned());
        }
        entry
            .by_ref()
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut json)
            .map_err(|error| error.to_string())?;
    }
    let manifest: Manifest =
        serde_json::from_slice(&json).map_err(|error| format!("cache manifest: {error}"))?;
    validate_manifest(&manifest)?;
    if archive.len() != manifest.files.len() + 1 {
        return Err("cache entries do not match manifest".to_owned());
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for (index, record) in manifest.files.iter().enumerate() {
        total = total
            .checked_add(record.size)
            .ok_or("cache size overflow")?;
        if total > MAX_CACHE_BYTES as u64 {
            return Err("cache decoded payload exceeds size limit".to_owned());
        }
        let mut entry = archive
            .by_index(index + 1)
            .map_err(|error| error.to_string())?;
        if entry.name() != record.path
            || entry.size() != record.size
            || entry.compression() != CompressionMethod::Stored
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("{}: unexpected ZIP entry metadata", record.path));
        }
        let mut payload = Vec::new();
        entry
            .by_ref()
            .take(record.size + 1)
            .read_to_end(&mut payload)
            .map_err(|error| format!("{}: {error}", record.path))?;
        validate_payload(record, &payload)?;
        files.insert(record.path.clone(), payload);
    }
    Ok(VerifiedCache { manifest, files })
}
