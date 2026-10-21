use ffe_convert::embedded::{
    decode_bitmaps, decode_font, decode_model_commands, decode_model_normals,
    decode_model_vertices, decode_sine_table, BitmapLayout,
};

#[test]
fn command_import_uses_descriptor_pointer_and_verified_extent_not_first_return() {
    let mut bytes = executable();
    bytes.resize(0x2000, 0);
    put32(&mut bytes, 0x108, 0x1e00);
    put32(&mut bytes, 0x200 + 74 * 4, 0x401600);
    put32(&mut bytes, 0x800, 0x401800);
    put32(&mut bytes, 0x808, 6);
    bytes[0xa02..0xa0a].copy_from_slice(&[3, 0, 0x23, 0x01, 0, 2, 4, 3]);
    let commands = decode_model_commands(&bytes, 0x1000, 74).unwrap();
    assert_eq!(
        (
            commands.descriptor_offset,
            commands.source_offset,
            commands.vertex_count,
            commands.bytes.len()
        ),
        (0x800, 0xa00, 6, 516)
    );
    assert_eq!(
        ffe_core::model_commands::decode(&commands.bytes, 6)
            .unwrap()
            .faces
            .len(),
        1
    );
    for (offset, value) in [
        (0x200 + 74 * 4, 0),
        (0x800, 0),
        (0x800, 0x402fff),
        (0x808, 129),
    ] {
        let mut bad = bytes.clone();
        put32(&mut bad, offset, value);
        assert!(decode_model_commands(&bad, 0x1000, 74).is_err());
    }

    assert!(decode_model_commands(&bytes[..0xa00 + 515], 0x1000, 74).is_err());
    assert!(decode_model_commands(&bytes, 0x1000, 73).is_err());
    bytes[0xa00] = 5;
    assert!(decode_model_commands(&bytes, 0x1000, 74).is_err());
}

#[test]
fn normal_import_preserves_mirrored_pair_count_implicit_entries_and_provenance() {
    let mut bytes = executable();
    put32(&mut bytes, 0x200 + 74 * 4, 0x401600);
    put32(&mut bytes, 0x804, 0x401780);
    put32(&mut bytes, 0x808, 2);
    put32(&mut bytes, 0x80c, 0x401790);
    put32(&mut bytes, 0x810, 4);
    put32(&mut bytes, 0x814, 13);
    bytes[0x980..0x986].copy_from_slice(&[1, 0, 2, 3, 4, 0]);
    bytes[0x990..0x996].copy_from_slice(&[1, 0, 127, 0, 0, 0]);
    let normals = decode_model_normals(&bytes, 0x1000, 74).unwrap();
    assert_eq!(
        (
            normals.descriptor_offset,
            normals.source_offset,
            normals.vertex_count,
            normals.normal_count
        ),
        (0x800, 0x990, 2, 4)
    );
    assert_eq!(normals.bytes, [1, 0, 127, 0, 0, 0]);
    for (offset, value) in [
        (0x200 + 74 * 4, 0),
        (0x80c, 0),
        (0x80c, 0x4017ff),
        (0x810, 0),
        (0x810, 3),
        (0x810, 258),
    ] {
        let mut bad = bytes.clone();
        put32(&mut bad, offset, value);
        assert!(decode_model_normals(&bad, 0x1000, 74).is_err());
    }
    let mut bad = bytes.clone();
    bad[0x990] = 2;
    assert!(decode_model_normals(&bad, 0x1000, 74).is_err());
    bad = bytes.clone();
    bad[0x980] = 11;
    assert!(decode_model_normals(&bad, 0x1000, 74).is_err());
    assert!(decode_model_normals(&bytes[..0x995], 0x1000, 74).is_err());
    assert!(decode_model_normals(&bytes, 0x1000, 73).is_err());
}
#[test]
fn model_vertex_import_follows_bounded_pointers_counts_scale_and_rejects_bad_programs() {
    let mut bytes = executable();
    put32(&mut bytes, 0x200, 0x401600);
    put32(&mut bytes, 0x804, 0x401780);
    put32(&mut bytes, 0x808, 2);
    put32(&mut bytes, 0x814, 13);
    bytes[0x980..0x986].copy_from_slice(&[1, 0, 2, 3, 4, 0]);
    let model = decode_model_vertices(&bytes, 0x1000, 0).unwrap();
    assert_eq!(
        (
            model.index,
            model.descriptor_offset,
            model.source_offset,
            model.vertex_count,
            model.scale
        ),
        (0, 0x800, 0x980, 2, 13)
    );
    assert_eq!(model.bytes, [1, 0, 2, 3, 4, 0]);
    for (offset, value) in [
        (0x200, 0),
        (0x200, 0x4017ff),
        (0x804, 0x4017ff),
        (0x808, 0),
        (0x808, 3),
        (0x808, 130),
        (0x814, 32),
    ] {
        let mut invalid = bytes.clone();
        put32(&mut invalid, offset, value);
        assert!(decode_model_vertices(&invalid, 0x1000, 0).is_err());
    }
    for bad in [[7, 0, 0, 0, 0, 0], [5, 0, 0, 0, 0, 0], [5, 0, 0, 0, 255, 0]] {
        bytes[0x980..0x986].copy_from_slice(&bad);
        assert!(decode_model_vertices(&bytes, 0x1000, 0).is_err());
    }
    assert!(decode_model_vertices(&bytes, 0x1000, 512).is_err());
    assert!(decode_model_vertices(&bytes, u32::MAX, 1).is_err());
}

const LAYOUT: BitmapLayout = BitmapLayout {
    primary_rva: 0x1000,
    secondary_rva: 0x1180,
    low_palette_rva: 0x1600,
};

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn font_import_preserves_all_rows_advances_and_provenance_with_bounds_checks() {
    let mut bytes = executable();
    bytes.resize(0x2000, 0);
    put32(&mut bytes, 0x108, 0x1e00);
    for (index, record) in bytes[0xa00..0xa00 + 1160].chunks_exact_mut(10).enumerate() {
        record[0] = index as u8;
        record[9] = 4;
    }
    let font = decode_font(&bytes, 0x1800).unwrap();
    assert_eq!(font.source_offset, 0xa00);
    assert_eq!(font.bytes, &bytes[0xa00..0xa00 + 1160]);
    assert!(decode_font(&bytes, 0x0fff).is_err());
    assert!(decode_font(&bytes, 0x2fff).is_err());
    for width in [0, 10, 255] {
        let mut invalid = bytes.clone();
        invalid[0xa09] = width;
        assert!(decode_font(&invalid, 0x1800).is_err());
    }
    for end in [0, 0xa00, 0xa00 + 1159] {
        assert!(decode_font(&bytes[..end], 0x1800).is_err());
    }
}

#[test]
fn sine_import_preserves_full_table_and_rejects_bss_truncation_and_invalid_shapes() {
    let mut bytes = executable();
    bytes.resize(0x10a00, 0);
    put32(&mut bytes, 0x108, 0x10800);
    let table: Vec<_> = (0..16384i32)
        .flat_map(|i| (i * 65536).to_le_bytes())
        .collect();
    bytes[0xa00..].copy_from_slice(&table);
    let imported = decode_sine_table(&bytes, 0x1800).unwrap();
    assert_eq!(imported.source_offset, 0xa00);
    assert_eq!(imported.bytes, table);
    for rva in [0, 0x17ff, 0x1801, u32::MAX] {
        assert!(decode_sine_table(&bytes, rva).is_err());
    }
    assert!(decode_sine_table(&bytes[..bytes.len() - 1], 0x1800).is_err());
    bytes[0xa00] = 1;
    assert!(decode_sine_table(&bytes, 0x1800).is_err());
}

fn executable() -> Vec<u8> {
    let mut bytes = vec![0; 0xa00];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 60, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut bytes, 0x84, 0x14c);
    put16(&mut bytes, 0x86, 1);
    put16(&mut bytes, 0x94, 96);
    put16(&mut bytes, 0x98, 0x10b);
    put32(&mut bytes, 0xb4, 0x400000);
    bytes[0xf8..0x100].copy_from_slice(b"DATA\0\0\0\0");
    put32(&mut bytes, 0x100, 0x10000); // BSS is deliberately not file-backed.
    put32(&mut bytes, 0x104, 0x1000);
    put32(&mut bytes, 0x108, 0x800);
    put32(&mut bytes, 0x10c, 0x200);
    for index in 0..347 {
        put32(&mut bytes, 0x200 + index * 4, 0x401780);
    }
    bytes[0x800..0x980].fill(63);
    bytes[0x980..0x98e].copy_from_slice(&[1, 2, 3, 0, 0xfe, 0xff, 4, 0, 0, 1, 2, 255, 4, 5]);
    bytes
}

#[test]
fn tables_keep_slot_identity_aliases_offsets_masking_and_palette() {
    let bytes = executable();
    let decoded = decode_bitmaps(&bytes, LAYOUT).unwrap();
    assert_eq!(decoded.low_palette_rgb6, vec![63; 384]);
    assert_eq!(decoded.bitmaps.len(), 347);
    for (slot, bitmap) in decoded.bitmaps.iter().enumerate() {
        assert_eq!(
            bitmap.table,
            if slot < 96 { "primary" } else { "secondary" }
        );
        assert_eq!(
            bitmap.index,
            if slot < 96 { slot } else { slot - 96 } as u32
        );
        assert_eq!((bitmap.width, bitmap.height), (3, 2));
        assert_eq!((bitmap.offset_x, bitmap.offset_y), (-2, 4));
        assert_eq!(bitmap.source_offset, 0x980);
        assert!(bitmap.masked_zero);
        assert_eq!(bitmap.indices, [0, 1, 2, 255, 4, 5]);
    }
    let mut opaque = bytes;
    opaque[0x980] = 0;
    assert!(!decode_bitmaps(&opaque, LAYOUT).unwrap().bitmaps[0].masked_zero);
}

#[test]
fn truncated_executables_and_non_file_backed_pointers_are_rejected() {
    let bytes = executable();
    for end in 0..bytes.len() {
        assert!(
            decode_bitmaps(&bytes[..end], LAYOUT).is_err(),
            "prefix {end}"
        );
    }
    for pointer in [0, 0x400fff, 0x4017fc, 0x401800, 0x402000, u32::MAX] {
        let mut invalid = bytes.clone();
        put32(&mut invalid, 0x200, pointer);
        assert!(
            decode_bitmaps(&invalid, LAYOUT).is_err(),
            "pointer {pointer:#x}"
        );
    }
}

#[test]
fn invalid_pe_headers_palette_and_bitmap_shapes_fail_explicitly() {
    let bytes = executable();
    for (offset, replacement) in [
        (0, b"NO".to_vec()),
        (60, u32::MAX.to_le_bytes().to_vec()),
        (0x80, b"LE\0\0".to_vec()),
        (0x84, 0x8664u16.to_le_bytes().to_vec()),
        (0x86, 97u16.to_le_bytes().to_vec()),
        (0x94, 95u16.to_le_bytes().to_vec()),
        (0x98, 0x20bu16.to_le_bytes().to_vec()),
        (0xf8, b"NONE".to_vec()),
        (0x104, u32::MAX.to_le_bytes().to_vec()),
        (0x108, 0u32.to_le_bytes().to_vec()),
        (0x10c, 0u32.to_le_bytes().to_vec()),
        (0x800, vec![64]),
        (0x980, vec![2]),
        (0x981, vec![0]),
        (0x982, 0u16.to_le_bytes().to_vec()),
        (0x982, 321u16.to_le_bytes().to_vec()),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            decode_bitmaps(&invalid, LAYOUT).is_err(),
            "offset {offset:#x}"
        );
    }
    let mut duplicate = bytes;
    put16(&mut duplicate, 0x86, 2);
    let section = duplicate[0xf8..0x120].to_vec();
    duplicate[0x120..0x148].copy_from_slice(&section);
    assert!(decode_bitmaps(&duplicate, LAYOUT)
        .unwrap_err()
        .contains("ambiguous"));
}

#[test]
fn only_verified_special_slots_preserve_zero_height_texture_blocks() {
    let mut bytes = executable();
    bytes.resize(0xa00 + 32776, 0);
    let data_size = bytes.len() as u32 - 0x200;
    put32(&mut bytes, 0x108, data_size);
    bytes[0xa00..0xa08].copy_from_slice(&[0, 0, 64, 0, 0, 0, 0, 0]);
    bytes[0xa08..].fill(123);
    let pointer = 0x401800;
    for slot in [94, 95] {
        put32(&mut bytes, 0x200 + slot * 4, pointer);
    }
    let decoded = decode_bitmaps(&bytes, LAYOUT).unwrap();
    for slot in [94, 95] {
        let bitmap = &decoded.bitmaps[slot];
        assert_eq!(
            (bitmap.width, bitmap.height, bitmap.masked_zero),
            (64, 0, false)
        );
        assert_eq!(bitmap.indices, vec![123; 32768]);
    }
    put32(&mut bytes, 0x200, pointer);
    assert!(decode_bitmaps(&bytes, LAYOUT).is_err());
    put32(&mut bytes, 0x200, 0x401780);
    bytes[0xa00] = 1;
    assert!(decode_bitmaps(&bytes, LAYOUT).is_err());
}
