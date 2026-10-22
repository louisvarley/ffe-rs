use ffe_content::{build, read, Asset, AssetKind, Manifest, SourceFile};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

fn manifest() -> Manifest {
    Manifest {
        format: "ffe-content-cache".to_owned(),
        format_version: 1,
        tool_version: "test".to_owned(),
        source_profile: "synthetic".to_owned(),
        source_files: vec![SourceFile {
            path: "FIRSTENC.EXE".to_owned(),
            size: 1,
            sha256: "a".repeat(64),
        }],
        complete_game_assets: false,
        unresolved: vec!["synthetic fixture only".to_owned()],
        files: Vec::new(),
    }
}

fn assets() -> Vec<Asset> {
    vec![
        Asset {
            id: "base:palette/low".to_owned(),
            path: "palettes/low.rgb6".to_owned(),
            kind: AssetKind::LowPaletteRgb6,
            bytes: vec![42; 384],
        },
        Asset {
            id: "base:bitmap/primary/000".to_owned(),
            path: "bitmaps/primary/000.idx".to_owned(),
            kind: AssetKind::IndexedBitmap {
                table: "primary".to_owned(),
                index: 0,
                width: 2,
                height: 2,
                offset_x: -1,
                offset_y: 3,
                masked_zero: true,
                source_offset: 123,
            },
            bytes: vec![0, 1, 2, 255],
        },
    ]
}

#[test]
fn model_command_payloads_require_v4_and_reject_invalid_boundaries_and_vertices() {
    let make = |bytes: Vec<u8>| Asset {
        id: "base:model-commands/074".to_owned(),
        path: "models/074.cmd".to_owned(),
        kind: AssetKind::ModelCommands {
            index: 74,
            vertex_count: 6,
            descriptor_offset: 128,
            source_offset: 256,
        },
        bytes,
    };
    let mut m = manifest();
    m.format_version = 4;
    let good = vec![3, 0, 0, 0, 0, 2, 4, 3, 0, 0];
    let bytes = build(m.clone(), vec![make(good.clone())]).unwrap();
    assert_eq!(
        read(&bytes).unwrap().asset("base:model-commands/074"),
        Some(good.as_slice())
    );
    for version in [1, 2, 3, 10] {
        assert!(read(&change_manifest(&bytes, |m| m["format_version"] = version.into())).is_err());
    }

    for bad in [
        vec![3, 0],
        vec![5, 0, 0, 0],
        vec![0x33, 0, 0, 0, 0, 0],
        // Vertex 7 is outside the six; 0x80..0xff are external (child) references.
        vec![3, 0, 0, 0, 0, 2, 4, 7, 0, 0],
    ] {
        assert!(build(m.clone(), vec![make(bad)]).is_err());
    }
}

#[test]
fn normal_payloads_require_v5_and_validate_counts_and_anchor_bounds() {
    let make = |data: Vec<u8>, count| Asset {
        id: "base:model-normals/074".to_owned(),
        path: "models/074.nrm".to_owned(),
        kind: AssetKind::ModelNormals {
            index: 74,
            vertex_count: 2,
            normal_count: count,
            descriptor_offset: 100,
            source_offset: 200,
        },
        bytes: data,
    };
    let mut m = manifest();
    m.format_version = 5;
    let good = vec![1, 0, 127, 0, 0, 0];
    let bytes = build(m.clone(), vec![make(good.clone(), 4)]).unwrap();
    assert_eq!(
        read(&bytes).unwrap().asset("base:model-normals/074"),
        Some(good.as_slice())
    );
    for version in [1, 2, 3, 4, 10] {
        assert!(read(&change_manifest(&bytes, |m| m["format_version"] = version.into())).is_err());
    }
    for count in [0, 1, 3, 6, 258, u32::MAX] {
        assert!(build(m.clone(), vec![make(good.clone(), count)]).is_err());
    }
    assert!(build(m.clone(), vec![make(vec![2, 0, 0, 0, 0, 0], 4)]).is_err());
    assert!(build(m, vec![make(vec![0; 5], 4)]).is_err());
}
#[test]
fn model_vertex_programs_require_v3_and_validate_before_exposing_payloads() {
    let make = |bytes: Vec<u8>, count, scale, index| Asset {
        id: "base:model-vertices/074".to_owned(),
        path: "models/074.vtx".to_owned(),
        kind: AssetKind::ModelVertices {
            index,
            vertex_count: count,
            scale,
            descriptor_offset: 128,
            source_offset: 256,
        },
        bytes,
    };
    let mut m = manifest();
    m.format_version = 3;
    let good = vec![1, 0, 1, 2, 3, 0];
    let bytes = build(m.clone(), vec![make(good.clone(), 2, 13, 74)]).unwrap();
    assert_eq!(
        read(&bytes).unwrap().asset("base:model-vertices/074"),
        Some(good.as_slice())
    );
    for version in [1, 2, 10] {
        assert!(read(&change_manifest(&bytes, |m| m["format_version"] = version.into())).is_err());
    }
    for (program, count, scale, index) in [
        (good.clone(), 3, 13, 74),
        (good.clone(), 2, 32, 74),
        (good.clone(), 2, 13, 73),
        (vec![5, 0, 0, 0, 0, 0], 2, 13, 74),
        (vec![7, 0, 0, 0, 0, 0], 2, 13, 74),
    ] {
        assert!(build(m.clone(), vec![make(program, count, scale, index)]).is_err());
    }
    // v7 imports any original table slot (0..466), not only stations 74/75.
    m.format_version = 7;
    assert!(build(m.clone(), vec![make(good.clone(), 2, 13, 73)]).is_ok());
    // A child-only program that reads its parent's vertex -1 still imports.
    let child = vec![1, 0, 1, 2, 3, 0, 17, 0, 0, 0xff, 0, 0];
    assert!(build(m.clone(), vec![make(child, 4, 13, 73)]).is_ok());
    assert!(build(m, vec![make(good, 2, 13, 466)]).is_err());
}

fn rewrite(bytes: &[u8], change: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.push((entry.name().to_owned(), bytes));
    }
    change(&mut files);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, payload) in files {
        writer
            .start_file(path, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&payload).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn change_manifest(bytes: &[u8], change: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    rewrite(bytes, |files| {
        let mut manifest = serde_json::from_slice(&files[0].1).unwrap();
        change(&mut manifest);
        files[0].1 = serde_json::to_vec(&manifest).unwrap();
    })
}

#[test]
fn stored_cache_is_deterministic_sorted_and_loads_by_stable_id() {
    let bytes = build(manifest(), assets()).unwrap();
    let mut reversed = assets();
    reversed.reverse();
    assert_eq!(bytes, build(manifest(), reversed).unwrap());
    let cache = read(&bytes).unwrap();
    assert_eq!(
        cache.asset("base:bitmap/primary/000"),
        Some([0, 1, 2, 255].as_slice())
    );
    assert_eq!(
        cache.asset("base:palette/low"),
        Some(vec![42; 384].as_slice())
    );
    assert_eq!(cache.asset("base:missing"), None);
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).unwrap();
        assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
        let date = entry.last_modified().unwrap();
        assert_eq!(
            (
                date.year(),
                date.month(),
                date.day(),
                date.hour(),
                date.minute(),
                date.second()
            ),
            (1980, 1, 1, 0, 0, 0)
        );
    }
}

#[test]
fn malformed_manifests_cannot_claim_complete_content_or_escape_paths() {
    let bytes = build(manifest(), assets()).unwrap();
    for (key, value) in [
        ("format_version", serde_json::json!(10)),
        ("format", serde_json::json!("other")),
        ("complete_game_assets", serde_json::json!(true)),
        ("unresolved", serde_json::json!([])),
        ("source_files", serde_json::json!([])),
    ] {
        let invalid = change_manifest(&bytes, |manifest| manifest[key] = value);
        assert!(read(&invalid).is_err(), "{key}");
    }
    for path in [
        "../escape",
        "/absolute",
        "c:/drive",
        "bad\\path",
        "a//b",
        "a/./b",
        "manifest.json",
        "UPPER.idx",
    ] {
        let invalid = change_manifest(&bytes, |manifest| {
            manifest["files"][0]["path"] = path.into()
        });
        assert!(read(&invalid).is_err(), "{path}");
    }
    let duplicate = change_manifest(&bytes, |manifest| {
        manifest["files"][1]["id"] = manifest["files"][0]["id"].clone();
    });
    assert!(read(&duplicate).is_err());
}

#[test]
fn v2_font_is_bounded_and_v1_cannot_silently_use_new_schema() {
    let mut manifest = manifest();
    manifest.format_version = 2;
    let mut payload = vec![0; 1160];
    for width in payload.iter_mut().skip(9).step_by(10) {
        *width = 4;
    }
    let font = Asset {
        id: "base:font/main".to_owned(),
        path: "fonts/main.fnt".to_owned(),
        kind: AssetKind::BitmapFont {
            first_code: 32,
            glyph_count: 116,
            rows: 9,
            source_offset: 99,
        },
        bytes: payload.clone(),
    };
    let bytes = build(manifest.clone(), vec![font]).unwrap();
    assert_eq!(
        read(&bytes).unwrap().asset("base:font/main"),
        Some(payload.as_slice())
    );
    let invalid = change_manifest(&bytes, |m| m["format_version"] = 1.into());
    assert!(read(&invalid).err().unwrap().contains("version"));
    for (key, value) in [("first_code", 31), ("glyph_count", 115), ("rows", 10)] {
        let invalid = change_manifest(&bytes, |m| m["files"][0]["asset"][key] = value.into());
        assert!(read(&invalid).is_err());
    }
    payload[9] = 0;
    let invalid = Asset {
        id: "base:font/main".to_owned(),
        path: "fonts/main.fnt".to_owned(),
        kind: AssetKind::BitmapFont {
            first_code: 32,
            glyph_count: 116,
            rows: 9,
            source_offset: 99,
        },
        bytes: payload,
    };
    assert!(build(manifest, vec![invalid]).is_err());
}

#[test]
fn payload_corruption_missing_extra_reordered_and_oversized_entries_fail() {
    let bytes = build(manifest(), assets()).unwrap();
    for change in 0..5 {
        let invalid = rewrite(&bytes, |files| match change {
            0 => files[1].1[0] ^= 1,
            1 => {
                files.pop();
            }
            2 => files.push(("extra.idx".to_owned(), vec![1])),
            3 => files.swap(1, 2),
            _ => files[1].0 = "other.idx".to_owned(),
        });
        assert!(read(&invalid).is_err(), "change {change}");
    }
    let invalid = change_manifest(&bytes, |manifest| {
        manifest["files"][0]["size"] = (u64::MAX).into()
    });
    assert!(read(&invalid).is_err());
    let invalid = change_manifest(&bytes, |manifest| {
        manifest["files"][0]["asset"]["width"] = 0.into()
    });
    assert!(read(&invalid).is_err());
    let invalid = change_manifest(&bytes, |manifest| {
        manifest["files"][0]["sha256"] = "b".repeat(64).into()
    });
    assert!(read(&invalid).is_err());
    for end in 0..bytes.len() {
        assert!(read(&bytes[..end]).is_err(), "prefix {end}");
    }
}

#[test]
fn builder_rejects_duplicate_paths_ids_and_invalid_typed_payloads() {
    for change in 0..5 {
        let mut assets = assets();
        match change {
            0 => assets[1].path = assets[0].path.clone(),
            1 => assets[1].id = assets[0].id.clone(),
            2 => assets[0].bytes[0] = 64,
            3 => assets[1].bytes.pop().map(|_| ()).unwrap(),
            _ => assets[0].path = "../bad".to_owned(),
        }
        assert!(build(manifest(), assets).is_err(), "change {change}");
    }
}

#[test]
fn texture_tables_require_v8_and_validate_payloads() {
    // Synthetic table: every record after the empty record 0 names bitmap 1 with
    // one colour.
    let mut table = vec![0u8; 148 * 30];
    for n in 1..148 {
        table[n * 30] = 1;
        table[n * 30 + 4] = 1;
        table[n * 30 + 8..n * 30 + 11].copy_from_slice(&[1, 2, 3]);
    }
    let descriptors = |bytes: Vec<u8>, count: u32| Asset {
        id: "base:texture/descriptors".to_owned(),
        path: "textures/descriptors.bin".to_owned(),
        kind: AssetKind::TextureDescriptors {
            count,
            source_offset: 1,
        },
        bytes,
    };
    let map = |bytes: Vec<u8>| Asset {
        id: "base:palette/fixed-map".to_owned(),
        path: "palettes/fixed-map.bin".to_owned(),
        kind: AssetKind::FixedColourMap { source_offset: 2 },
        bytes,
    };
    let mut m = manifest();
    m.format_version = 7;
    assert!(build(m.clone(), vec![descriptors(table.clone(), 148)]).is_err());
    assert!(build(m.clone(), vec![map(vec![0; 4096])]).is_err());
    m.format_version = 8;
    let bytes = build(
        m.clone(),
        vec![descriptors(table.clone(), 148), map(vec![1; 4096])],
    )
    .unwrap();
    let cache = read(&bytes).unwrap();
    assert_eq!(
        cache.asset("base:texture/descriptors").unwrap(),
        table.as_slice()
    );
    let mut bad = table.clone();
    bad[34] = 8; // record 1 colour count
    assert!(build(m.clone(), vec![descriptors(bad, 148)]).is_err());
    assert!(build(m.clone(), vec![descriptors(table.clone(), 147)]).is_err());
    assert!(build(m.clone(), vec![descriptors(table[..4410].to_vec(), 148)]).is_err());
    assert!(build(m.clone(), vec![map(vec![129; 4096])]).is_err());
    assert!(build(m, vec![map(vec![0; 4095])]).is_err());
}

#[test]
fn lighting_tables_require_v9_and_validate_payloads() {
    // Synthetic default light (0x8000 along z) with a 7..0 ramp, and six ramps.
    let ramp: Vec<u8> = (0..8u8).flat_map(|i| [7 - i; 3]).collect();
    let mut default = Vec::new();
    for v in [0i32, 0, 0x8000] {
        default.extend_from_slice(&v.to_le_bytes());
    }
    default.extend_from_slice(&ramp);
    let stars: Vec<u8> = ramp.iter().copied().cycle().take(6 * 24).collect();
    let light = |bytes: Vec<u8>| Asset {
        id: "base:lighting/default".to_owned(),
        path: "lighting/default.bin".to_owned(),
        kind: AssetKind::DefaultLighting { source_offset: 1 },
        bytes,
    };
    let shades = |bytes: Vec<u8>, count: u32| Asset {
        id: "base:lighting/star-shades".to_owned(),
        path: "lighting/star-shades.bin".to_owned(),
        kind: AssetKind::StarShades {
            count,
            source_offset: 2,
        },
        bytes,
    };
    let mut m = manifest();
    m.format_version = 8;
    assert!(build(m.clone(), vec![light(default.clone())]).is_err());
    assert!(build(m.clone(), vec![shades(stars.clone(), 6)]).is_err());
    m.format_version = 9;
    let bytes = build(
        m.clone(),
        vec![light(default.clone()), shades(stars.clone(), 6)],
    )
    .unwrap();
    assert_eq!(
        read(&bytes)
            .unwrap()
            .asset("base:lighting/default")
            .unwrap(),
        default.as_slice()
    );
    // Not a normalised light, a shade above 15, wrong sizes and counts.
    let mut bad = default.clone();
    bad[9] = 0;
    assert!(build(m.clone(), vec![light(bad)]).is_err());
    let mut bad = default.clone();
    bad[12] = 16;
    assert!(build(m.clone(), vec![light(bad)]).is_err());
    assert!(build(m.clone(), vec![light(default[..35].to_vec())]).is_err());
    assert!(build(m.clone(), vec![shades(stars.clone(), 5)]).is_err());
    assert!(build(m, vec![shades(stars[..120].to_vec(), 6)]).is_err());
}
