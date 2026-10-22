use ffe_core::model_texture::{
    draw_quad, draw_triangle, fixed_palette_index, flat_colour, pack_colour, quad_uv,
    textured_colours, textured_path, triangle_uv, validate_colour_map, Descriptors, TextureMap,
    DESCRIPTOR_BYTES, DESCRIPTOR_COUNT,
};

/// Synthetic table: record n (n >= 1) names bitmap n with orientation n & 7 and
/// `1 + n % 7` colours (n % 16, i, 15 - i).
fn table() -> Vec<u8> {
    let mut bytes = vec![0u8; DESCRIPTOR_COUNT * DESCRIPTOR_BYTES];
    for n in 1..DESCRIPTOR_COUNT {
        let record = &mut bytes[n * DESCRIPTOR_BYTES..(n + 1) * DESCRIPTOR_BYTES];
        let count = 1 + n % 7;
        record[..4].copy_from_slice(&((n as u32) | ((n as u32 & 7) << 12)).to_le_bytes());
        record[4..8].copy_from_slice(&(count as u32).to_le_bytes());
        for i in 0..count {
            record[8 + 3 * i..11 + 3 * i].copy_from_slice(&[(n % 16) as u8, i as u8, 15 - i as u8]);
        }
    }
    bytes
}

#[test]
fn descriptor_table_validation() {
    let bytes = table();
    let descriptors = Descriptors::from_bytes(&bytes).unwrap();
    assert_eq!(descriptors.len(), 148);
    let record = descriptors.get(9).unwrap();
    assert_eq!((record.bitmap_id(), record.orientation()), (9, 1));
    assert_eq!(record.colours.len(), 3);
    assert!(descriptors.get(148).is_err());
    // Wrong size, non-empty record 0, zero/too many colours, bitmap id, high bits,
    // non-nibble colour and non-zero padding are all rejected.
    assert!(Descriptors::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    let corrupt = |at: usize, value: u8| {
        let mut copy = bytes.clone();
        copy[at] = value;
        Descriptors::from_bytes(&copy).is_err()
    };
    let r = DESCRIPTOR_BYTES; // record 1: one colour
    assert!(corrupt(0, 1));
    assert!(corrupt(r + 4, 0));
    assert!(corrupt(r + 4, 8));
    assert!(corrupt(r + 1, 0x02)); // bitmap id 0x201 >= 347
    assert!(corrupt(r + 1, 0x80)); // bit 15
    assert!(corrupt(r + 8, 16));
    assert!(corrupt(r + 14, 1));
}

#[test]
fn textured_entry_colours_shade_wrap_and_clamp() {
    let descriptors = Descriptors::from_bytes(&table()).unwrap();
    // 0x4009: texture 9, lit, not local; colours (9,i,15-i), normal (3,4,5).
    let entry = textured_colours(0x4009, [1, 1, 1], [3, 4, 5], &descriptors).unwrap();
    assert_eq!(entry.bitmap, 9 | 1 << 12);
    assert_eq!(entry.colours, [0xc4f, 0xc5f, 0xc6f]);
    // Local (bit 12) adds the instance colour; unlit (bit 13) drops the normal.
    let entry = textured_colours(0x7009, [1, 2, 3], [3, 4, 5], &descriptors).unwrap();
    assert_eq!(entry.colours, [0xa2f, 0xa3f, 0xa4f]);
    // Byte wrap happens before the clamp: green 2 + 254 wraps to 0, 0 + 254 clamps.
    let entry = textured_colours(0x7009, [0, 254, 0], [9; 3], &descriptors).unwrap();
    assert_eq!(entry.colours, [0x9ff, 0x9fe, 0x90d]);
    assert!(textured_colours(0x0123, [0; 3], [0; 3], &descriptors).is_err());
    assert!(textured_colours(0x4000 | 200, [0; 3], [0; 3], &descriptors).is_err());
}

#[test]
fn flat_fallback_adds_normal_and_first_record_colour() {
    let descriptors = Descriptors::from_bytes(&table()).unwrap();
    // Textured: zero RGB + normal (F1693 does not test the texture) + colour 0.
    assert_eq!(
        flat_colour(0x4009, [1, 1, 1], [3, 4, 5], &descriptors).unwrap(),
        [12, 4, 20]
    );
    // Untextured: nibble RGB + local instance + normal.
    assert_eq!(
        flat_colour(0x1123, [1, 1, 1], [3, 4, 5], &descriptors).unwrap(),
        [5, 7, 9]
    );
    assert_eq!(
        flat_colour(0x2123, [9; 3], [9; 3], &descriptors).unwrap(),
        [1, 2, 3]
    );
    assert_eq!(pack_colour([1, 2, 3]), 0x123);
    assert_eq!(pack_colour([0, 16, 0]), 0x100);
}

#[test]
fn textured_path_guard_and_perimeter() {
    let square = [[10, 10], [12, 10], [12, 11]];
    assert!(textured_path(true, 0x4001, &square));
    assert!(!textured_path(false, 0x4001, &square));
    assert!(!textured_path(true, 0x0fff, &square));
    // Perimeter 2 + 1 + 3 = 6 passes, 4 does not.
    assert!(!textured_path(
        true,
        0x4001,
        &[[10, 10], [11, 10], [11, 11]]
    ));
    assert!(textured_path(
        true,
        0x4001,
        &[[-2000, -2000], [2320, 2158], [0, 0]]
    ));
    assert!(!textured_path(
        true,
        0x4001,
        &[[-2001, 0], [100, 0], [0, 100]]
    ));
    assert!(!textured_path(
        true,
        0x4001,
        &[[0, 2159], [100, 0], [0, 100]]
    ));
}

#[test]
fn corner_assignments() {
    assert_eq!(triangle_uv(64), [[0, 0], [63, 0], [0, 63]]);
    assert_eq!(triangle_uv(128)[1], [127, 0]);
    let (a, b, c, d) = ([0, 0], [63, 0], [0, 63], [63, 63]);
    assert_eq!(quad_uv(64, 0), [a, b, d, c]);
    assert_eq!(quad_uv(64, 1), [b, d, c, a]);
    assert_eq!(quad_uv(64, 3), [c, a, b, d]);
    assert_eq!(quad_uv(64, 4), [a, c, d, b]);
    assert_eq!(quad_uv(64, 7), [b, a, c, d]);
    assert_eq!(quad_uv(64, 15), quad_uv(64, 7));
}

fn render(points: [[i16; 2]; 4], uv: [[i16; 2]; 4], map: TextureMap<'_>) -> (Vec<u8>, u32) {
    let mut buffer = vec![255u8; 320 * 158];
    let unmodelled = draw_quad(points, uv, map, |x, y, texel| {
        assert!((0..320).contains(&x) && (0..158).contains(&y));
        buffer[y as usize * 320 + x as usize] = texel;
    });
    (buffer, unmodelled)
}

#[test]
fn quad_maps_texels_affinely_with_left_top_fill() {
    // Texel = column (0..63) in row 0..31, 64 + column below.
    let texels: Vec<u8> = (0..64 * 64)
        .map(|i| (i % 64 + if i / 64 >= 32 { 64 } else { 0 }) as u8)
        .collect();
    let map = TextureMap {
        width: 64,
        masked_zero: false,
        texels: &texels,
    };
    // A 64 x 64 screen square at (100, 50): one texel per pixel, rows 50..113,
    // columns 100..163 (right and bottom edges exclusive).
    let points = [[100, 50], [164, 50], [164, 114], [100, 114]];
    let (buffer, unmodelled) = render(points, quad_uv(64, 0), map);
    assert_eq!(unmodelled, 0);
    assert_eq!(buffer.iter().filter(|p| **p != 255).count(), 64 * 64);
    assert_eq!(buffer[50 * 320 + 100], 0);
    assert_eq!(buffer[50 * 320 + 163], 62);
    assert_eq!(buffer[113 * 320 + 100], 64);
    assert_eq!(buffer[50 * 320 + 164], 255);
    assert_eq!(buffer[114 * 320 + 100], 255);
    // Orientation 4 (mirrored) swaps the axes: the top row runs down column 0.
    let (buffer, _) = render(points, quad_uv(64, 4), map);
    assert_eq!(buffer[50 * 320 + 110], 0);
    assert_eq!(buffer[50 * 320 + 140], 64);
    // The corner is 63, so each row steps 63/64 of a texel: row 40 reads 39.
    assert_eq!(buffer[90 * 320 + 100], 39);
    // A masked bitmap skips texel 0.
    let masked = TextureMap {
        masked_zero: true,
        ..map
    };
    let (buffer, _) = render(points, quad_uv(64, 0), masked);
    assert_eq!(buffer[50 * 320 + 100], 255);
    assert_eq!(buffer[50 * 320 + 101], 255); // u 0.98: still texel 0
    assert_eq!(buffer[50 * 320 + 102], 1);
}

#[test]
fn rasteriser_clips_and_skips_flat_shapes() {
    let texels = vec![7u8; 128 * 128];
    let map = TextureMap {
        width: 128,
        masked_zero: false,
        texels: &texels,
    };
    // Far larger than the view: every visible pixel is written exactly once.
    let (buffer, unmodelled) = render(
        [[-1000, -900], [1300, -900], [1300, 1100], [-1000, 1100]],
        quad_uv(128, 2),
        map,
    );
    assert_eq!(unmodelled, 0);
    assert!(buffer.iter().all(|p| *p == 7));
    // Zero-height triangle draws nothing.
    let mut count = 0;
    draw_triangle(
        [[10, 20], [200, 20], [50, 20]],
        triangle_uv(128),
        map,
        |_, _, _| count += 1,
    );
    assert_eq!(count, 0);
    // Reads outside the bitmap are counted, not invented.
    let small = TextureMap {
        texels: &texels[..10],
        ..map
    };
    let (_, unmodelled) = render([[0, 0], [20, 0], [20, 20], [0, 20]], quad_uv(128, 0), small);
    assert!(unmodelled > 0);
}

#[test]
fn colour_map_validation_and_lookup() {
    let mut map = vec![0u8; 4096];
    map[0x777] = 8;
    validate_colour_map(&map).unwrap();
    assert_eq!(fixed_palette_index(&map, 0x777), Some(7));
    assert_eq!(fixed_palette_index(&map, 0x778), None);
    assert_eq!(fixed_palette_index(&map, 0x1000), None);
    map[1] = 129;
    assert!(validate_colour_map(&map).is_err());
    assert!(validate_colour_map(&map[..4095]).is_err());
}
