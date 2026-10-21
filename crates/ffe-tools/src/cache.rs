use crate::external::{output_destination, recorded_bytes};
use crate::{inspect_game, read_bounded, Severity};
use ffe_content::{Asset, AssetKind, Manifest, SourceFile};
use ffe_convert::embedded::{
    decode_bitmaps, decode_font, decode_model_commands, decode_model_normals,
    decode_model_vertices, decode_sine_table, BitmapLayout,
};
use ffe_convert::{decode_indexed_bmp, pcm16_mono_22050_wav};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Model table and sine table RVAs for an exact-hash release.
pub(crate) fn model_layout(profile: &str) -> Result<(u32, u32), String> {
    match profile {
        "ffe-floppy-1.02-en" => Ok((0x0d0cfc, 0x106390)),
        "ffe-cd-1.01-en" => Ok((0x0d0cf8, 0x105334)),
        _ => Err("no verified model layout for this source profile".to_owned()),
    }
}

pub fn inspect_data(path: &Path) -> Result<Manifest, String> {
    let bytes = read_bounded(path, ffe_content::MAX_CACHE_BYTES as u64)?;
    Ok(ffe_content::read(&bytes)?.into_manifest())
}

pub fn build_data(root: &Path, output: &Path) -> Result<Manifest, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let destination = output_destination(&root, output)?;
    let inspection = inspect_game(&root)?;
    if !inspection.inspection_passed {
        let errors: Vec<_> = inspection
            .diagnostics
            .iter()
            .filter(|entry| entry.severity == Severity::Error)
            .map(|entry| format!("{} [{}]: {}", entry.path, entry.code, entry.message))
            .collect();
        return Err(format!(
            "source inspection failed; no cache written:\n{}",
            errors.join("\n")
        ));
    }
    let profile = inspection
        .source_profile
        .ok_or("inspection passed without a source profile")?;
    // RVAs, not offsets in the rebuilt JJFFE executable. These are verified
    // against all 347 table records in each exact-hash original release.
    let layout = match profile.id {
        "ffe-floppy-1.02-en" => BitmapLayout {
            primary_rva: 0x1b069c,
            secondary_rva: 0x1b081c,
            low_palette_rva: 0x101990,
        },
        "ffe-cd-1.01-en" => BitmapLayout {
            primary_rva: 0x1af640,
            secondary_rva: 0x1af7c0,
            low_palette_rva: 0x1009b0,
        },
        _ => return Err("no verified embedded bitmap layout for this source profile".to_owned()),
    };
    let font_rva = match profile.id {
        "ffe-floppy-1.02-en" => 0x1013b0,
        "ffe-cd-1.01-en" => 0x1003d0,
        _ => return Err("no verified font layout for this source profile".to_owned()),
    };
    let (model_table_rva, sine_table_rva) = model_layout(profile.id)?;
    let mut assets = Vec::new();
    for record in &inspection.files {
        if record.path == "FIRSTENC.EXE" {
            let bytes = recorded_bytes(&root, record)?;
            let table = decode_sine_table(&bytes, sine_table_rva)?;
            assets.push(Asset {
                id: "base:math/sine".to_owned(),
                path: "math/sine.i32".to_owned(),
                kind: AssetKind::SineTable {
                    source_offset: table.source_offset as u64,
                },
                bytes: table.bytes,
            });
            for index in [74, 75] {
                let normals = decode_model_normals(&bytes, model_table_rva, index)?;
                assets.push(Asset {
                    id: format!("base:model-normals/{index:03}"),
                    path: format!("models/{index:03}.nrm"),
                    kind: AssetKind::ModelNormals {
                        index,
                        vertex_count: normals.vertex_count,
                        normal_count: normals.normal_count,
                        descriptor_offset: normals.descriptor_offset as u64,
                        source_offset: normals.source_offset as u64,
                    },
                    bytes: normals.bytes,
                });
                let commands = decode_model_commands(&bytes, model_table_rva, index)?;
                assets.push(Asset {
                    id: format!("base:model-commands/{index:03}"),
                    path: format!("models/{index:03}.cmd"),
                    kind: AssetKind::ModelCommands {
                        index,
                        vertex_count: commands.vertex_count,
                        descriptor_offset: commands.descriptor_offset as u64,
                        source_offset: commands.source_offset as u64,
                    },
                    bytes: commands.bytes,
                });
                let model = decode_model_vertices(&bytes, model_table_rva, index)?;
                assets.push(Asset {
                    id: format!("base:model-vertices/{index:03}"),
                    path: format!("models/{index:03}.vtx"),
                    kind: AssetKind::ModelVertices {
                        index,
                        vertex_count: model.vertex_count,
                        scale: model.scale,
                        descriptor_offset: model.descriptor_offset as u64,
                        source_offset: model.source_offset as u64,
                    },
                    bytes: model.bytes,
                });
            }
            let font = decode_font(&bytes, font_rva)?;
            assets.push(Asset {
                id: "base:font/main".to_owned(),
                path: "fonts/main.fnt".to_owned(),
                kind: AssetKind::BitmapFont {
                    first_code: 32,
                    glyph_count: 116,
                    rows: 9,
                    source_offset: font.source_offset as u64,
                },
                bytes: font.bytes,
            });
            let decoded = decode_bitmaps(&bytes, layout)?;
            assets.push(Asset {
                id: "base:palette/low".to_owned(),
                path: "palettes/low.rgb6".to_owned(),
                kind: AssetKind::LowPaletteRgb6,
                bytes: decoded.low_palette_rgb6,
            });
            for bitmap in decoded.bitmaps {
                let name = format!("{}/{:03}", bitmap.table, bitmap.index);
                let kind = if bitmap.height == 0 {
                    AssetKind::TextureBlock {
                        table: bitmap.table.to_owned(),
                        index: bitmap.index,
                        width: bitmap.width,
                        source_offset: bitmap.source_offset as u64,
                    }
                } else {
                    AssetKind::IndexedBitmap {
                        table: bitmap.table.to_owned(),
                        index: bitmap.index,
                        width: bitmap.width,
                        height: bitmap.height,
                        offset_x: bitmap.offset_x,
                        offset_y: bitmap.offset_y,
                        masked_zero: bitmap.masked_zero,
                        source_offset: bitmap.source_offset as u64,
                    }
                };
                assets.push(Asset {
                    id: format!("base:bitmap/{name}"),
                    path: format!("bitmaps/{name}.idx"),
                    kind,
                    bytes: bitmap.indices,
                });
            }
        } else if record.path.starts_with("CABIN") && record.path.ends_with(".BMP") {
            let image = decode_indexed_bmp(&recorded_bytes(&root, record)?)?;
            let name = record.path.trim_end_matches(".BMP").to_ascii_lowercase();
            assets.push(Asset {
                id: format!("base:cockpit/{name}"),
                path: format!("cockpits/{name}.png"),
                kind: AssetKind::CabinPng {
                    width: image.width,
                    height: image.height,
                },
                bytes: image.indexed_png()?,
            });
        } else if record.path.starts_with("FX/") && record.path.ends_with(".RAW") {
            let name = record
                .path
                .trim_start_matches("FX/")
                .trim_end_matches(".RAW")
                .to_ascii_lowercase();
            assets.push(Asset {
                id: format!("base:effect/{name}"),
                path: format!("audio/fx/{name}.wav"),
                kind: AssetKind::EffectWav,
                bytes: pcm16_mono_22050_wav(&recorded_bytes(&root, record)?)?,
            });
        }
    }
    if assets.len() != 385 {
        return Err(format!(
            "unexpected imported asset count {}; no cache written",
            assets.len()
        ));
    }
    let manifest = Manifest {
        format: "ffe-content-cache".to_owned(), format_version: 6,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(), source_profile: profile.id.to_owned(),
        source_files: inspection.files.into_iter().map(|record| SourceFile {
            path: record.path, size: record.size, sha256: record.sha256,
        }).collect(),
        complete_game_assets: false,
        unresolved: vec![
            "Only two station vertex/command/normal programs, the two bitmap tables, main bitmap font, fixed low palette, external cabins and PCM effects are imported.".to_owned(),
            "Upper 128 palette colours are runtime state; bitmap RGBA previews and upscale eligibility are not inferred.".to_owned(),
            "Station commands are structurally decoded, not fully executed; submodels, other fonts, gameplay tables, journal, music and optional CD media remain unconverted.".to_owned(),
            "Not a playable game cache or installable graphics pack; patch-output layouts are unsupported.".to_owned(),
        ],
        files: Vec::new(),
    };
    let bytes = ffe_content::build(manifest, assets)?;
    let verified = ffe_content::read(&bytes)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| format!("{}: cannot create cache: {error}", destination.display()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "{}: cache write failed: {error}; output may be incomplete, choose a fresh path",
                destination.display()
            )
        })?;
    Ok(verified.into_manifest())
}
