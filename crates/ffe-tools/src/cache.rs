use crate::external::{output_destination, recorded_bytes};
use crate::{inspect_game, read_bounded, Severity};
use ffe_content::{Asset, AssetKind, Manifest, SourceFile};
use ffe_convert::embedded::{
    decode_bitmaps, decode_colour_map, decode_default_lighting, decode_font, decode_model_commands,
    decode_model_descriptor, decode_model_names, decode_model_normals, decode_model_vertices,
    decode_ship_stats, decode_sine_table, decode_star_shades, decode_texture_descriptors,
    BitmapLayout,
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

/// Texture descriptor (`DATA_007811`) and 12-bit colour map (`DATA_007770`) RVAs
/// (v8). Verified byte for byte against JJFFE 2.8a7 for floppy 1.02 only; the CD
/// 1.01 executable was not available, so its cache omits both tables.
pub(crate) fn texture_layout(profile: &str) -> Option<(u32, u32)> {
    match profile {
        "ffe-floppy-1.02-en" => Some((0x102da8, 0x101b10)),
        _ => None,
    }
}

/// Model name table (`DATA_004682`) RVA (v11), matched against JJFFE 2.8a7 in
/// floppy 1.02 only.
pub(crate) fn names_layout(profile: &str) -> Option<u32> {
    match profile {
        "ffe-floppy-1.02-en" => Some(0xd1444),
        _ => None,
    }
}

/// Default lighting (`DATA_007231`) and star shade table (`DATA_007759`) RVAs (v9),
/// matched byte for byte against JJFFE 2.8a7 in floppy 1.02 only.
pub(crate) fn lighting_layout(profile: &str) -> Option<(u32, u32)> {
    match profile {
        "ffe-floppy-1.02-en" => Some((0xeec60, 0x101858)),
        _ => None,
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
    let mut models = 0;
    let mut descriptor_table = vec![0u8; ffe_convert::model_extents::MODEL_COUNT * 8];
    let stats_bytes = ffe_convert::embedded::SHIP_STATS_BYTES;
    let mut stats_table = vec![0u8; ffe_convert::model_extents::MODEL_COUNT * stats_bytes];
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
            // Every table slot whose normals, commands and vertices all decode (v7).
            // Stations 74/75 must decode; other slots that cannot are skipped.
            for index in 0..ffe_convert::model_extents::MODEL_COUNT as u32 {
                let decoded =
                    decode_model_normals(&bytes, model_table_rva, index).and_then(|normals| {
                        let commands = decode_model_commands(&bytes, model_table_rva, index)?;
                        let model = decode_model_vertices(&bytes, model_table_rva, index)?;
                        Ok((normals, commands, model))
                    });
                let (normals, commands, model) = match decoded {
                    Ok(decoded) => decoded,
                    Err(error) if matches!(index, 74 | 75) => return Err(error),
                    Err(_) => continue,
                };
                models += 1;
                // F1750 reads +0x18 (scale offset) and +0x1c (cull radius) for child calls.
                let descriptor = decode_model_descriptor(&bytes, model_table_rva, index)?;
                let entry = index as usize * 8;
                descriptor_table[entry..entry + 4]
                    .copy_from_slice(&descriptor.scale_offset.to_le_bytes());
                descriptor_table[entry + 4..entry + 8]
                    .copy_from_slice(&descriptor.radius.to_le_bytes());
                // Object set-up copies thrust limits, gun mounts and mass from here (v12).
                if let Some(stats) = decode_ship_stats(&bytes, model_table_rva, index)? {
                    let entry = index as usize * stats_bytes;
                    stats_table[entry..entry + stats_bytes].copy_from_slice(&stats);
                }
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
            // The font models' per-character glyph programs (v14). A set whose
            // table cannot be read is a build error, not silence: text would
            // silently lose that set.
            for index in 0..=2u32 {
                let glyphs = ffe_convert::embedded::glyph_programs(&bytes, model_table_rva, index)?;
                let mut payload = Vec::with_capacity(glyphs.starts.len() * 4 + glyphs.bytes.len());
                for start in &glyphs.starts {
                    payload.extend_from_slice(&start.to_le_bytes());
                }
                payload.extend_from_slice(&glyphs.bytes);
                assets.push(Asset {
                    id: format!("base:model-glyphs/{index:03}"),
                    path: format!("models/{index:03}.glp"),
                    kind: AssetKind::GlyphPrograms {
                        index,
                        count: glyphs.starts.len() as u32,
                        first_char: u32::from(ffe_convert::embedded::GLYPH_FIRST_CHAR),
                        vertex_count: glyphs.vertex_count,
                        scale: glyphs.scale,
                        source_offset: glyphs.source_offset as u64,
                    },
                    bytes: payload,
                });
            }
            if let Some((descriptor_rva, colour_map_rva)) = texture_layout(profile.id) {
                let table = decode_texture_descriptors(&bytes, descriptor_rva)?;
                assets.push(Asset {
                    id: "base:texture/descriptors".to_owned(),
                    path: "textures/descriptors.bin".to_owned(),
                    kind: AssetKind::TextureDescriptors {
                        count: ffe_core::model_texture::DESCRIPTOR_COUNT as u32,
                        source_offset: table.source_offset as u64,
                    },
                    bytes: table.bytes,
                });
                let map = decode_colour_map(&bytes, colour_map_rva)?;
                assets.push(Asset {
                    id: "base:palette/fixed-map".to_owned(),
                    path: "palettes/fixed-map.bin".to_owned(),
                    kind: AssetKind::FixedColourMap {
                        source_offset: map.source_offset as u64,
                    },
                    bytes: map.bytes,
                });
            }
            assets.push(Asset {
                id: "base:model/descriptors".to_owned(),
                path: "models/descriptors.bin".to_owned(),
                kind: AssetKind::ModelDescriptors {
                    count: ffe_convert::model_extents::MODEL_COUNT as u32,
                },
                bytes: std::mem::take(&mut descriptor_table),
            });
            assets.push(Asset {
                id: "base:ship/stats".to_owned(),
                path: "ships/stats.bin".to_owned(),
                kind: AssetKind::ShipStats {
                    count: ffe_convert::model_extents::MODEL_COUNT as u32,
                },
                bytes: std::mem::take(&mut stats_table),
            });
            if let Some(names_rva) = names_layout(profile.id) {
                let table = decode_model_names(&bytes, names_rva)?;
                assets.push(Asset {
                    id: "base:text/model-names".to_owned(),
                    path: "text/model-names.bin".to_owned(),
                    kind: AssetKind::ModelNames {
                        count: table.count as u32,
                        source_offset: table.source_offset as u64,
                    },
                    bytes: table.bytes,
                });
            }
            if let Some((default_rva, stars_rva)) = lighting_layout(profile.id) {
                let table = decode_default_lighting(&bytes, default_rva)?;
                assets.push(Asset {
                    id: "base:lighting/default".to_owned(),
                    path: "lighting/default.bin".to_owned(),
                    kind: AssetKind::DefaultLighting {
                        source_offset: table.source_offset as u64,
                    },
                    bytes: table.bytes,
                });
                let table = decode_star_shades(&bytes, stars_rva)?;
                assets.push(Asset {
                    id: "base:lighting/star-shades".to_owned(),
                    path: "lighting/star-shades.bin".to_owned(),
                    kind: AssetKind::StarShades {
                        count: ffe_core::model_lighting::STAR_SHADE_TABLES as u32,
                        source_offset: table.source_offset as u64,
                    },
                    bytes: table.bytes,
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
    // Non-model assets: +2 v8 texture tables, +2 v9 lighting tables, one name
    // table and three v14 glyph sets, plus vertices, commands and normals per
    // imported model.
    let tables = if texture_layout(profile.id).is_some() {
        2
    } else {
        0
    } + if lighting_layout(profile.id).is_some() {
        2
    } else {
        0
    } + usize::from(names_layout(profile.id).is_some())
        + 3;
    // Plus the v10 model descriptor table and the v12 ship statistics table.
    if models < 2 || assets.len() != 381 + tables + 3 * models {
        return Err(format!(
            "unexpected imported asset count {} ({models} models); no cache written",
            assets.len()
        ));
    }
    let manifest = Manifest {
        format: "ffe-content-cache".to_owned(), format_version: 14,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(), source_profile: profile.id.to_owned(),
        source_files: inspection.files.into_iter().map(|record| SourceFile {
            path: record.path, size: record.size, sha256: record.sha256,
        }).collect(),
        complete_game_assets: false,
        unresolved: vec![
            format!("{models} of {} model vertex/command/normal programs (those that decode), the two bitmap tables, main bitmap font, fixed low palette, external cabins and PCM effects are imported.", ffe_convert::model_extents::MODEL_COUNT),
            "Upper 128 palette colours are runtime state; bitmap RGBA previews and upscale eligibility are not inferred.".to_owned(),
            if tables == 0 {
                "Texture descriptors and the 12-bit colour map are not imported: their RVAs are verified for floppy 1.02 only.".to_owned()
            } else {
                "Texture descriptors and the 12-bit fixed colour map are imported; textured faces are drawn only by the diagnostic F6 viewer.".to_owned()
            },
            if lighting_layout(profile.id).is_some() {
                "The default light and shade table and the star shade tables are imported (v9).".to_owned()
            } else {
                "Lighting tables are not imported: their RVAs are verified for floppy 1.02 only.".to_owned()
            },
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
