use ffe_core::hyperspace::plan;
use ffe_core::sector::{
    distance, position, Drive, Generator, Tables, DRIVE_BYTES, SECTOR_BYTES, SPECIAL_SECTORS,
    TABLE_BYTES, VECTORS_BYTES,
};

/// A synthetic galaxy block: 49 special sectors (x = 0..49, y = 0) holding 117 stars between
/// them, a flat density grid of 8 and a hash table of one half, and 15 drives.
fn block() -> Vec<u8> {
    let mut bytes = vec![0u8; TABLE_BYTES];
    let put = |bytes: &mut Vec<u8>, at: usize, value: u32| {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    };
    // Vector k: byte 0 is 9, bytes 2..5 are k, k + 1, k + 2; type word k.
    for k in 0..117usize {
        bytes[6 * k] = 9;
        bytes[6 * k + 2] = k as u8;
        bytes[6 * k + 3] = k as u8 + 1;
        bytes[6 * k + 4] = k as u8 + 2;
        bytes[702 + 2 * k] = k as u8;
    }
    let special = VECTORS_BYTES;
    for k in 0..SPECIAL_SECTORS {
        put(&mut bytes, special + 4 * k, k as u32);
        put(&mut bytes, special + 196 + 4 * k, (117 * k / 49) as u32);
    }
    put(&mut bytes, special + 196 + 4 * 49, 117);
    // The hash table (128 dwords at +588) is one half; the grid (128 by 129 at +1164) is 8.
    for k in 0..128 {
        put(&mut bytes, special + 588 + 4 * k, 0x8000);
    }
    for cell in 0..128 * 129 {
        bytes[special + 1164 + cell] = 8;
    }
    let drives = VECTORS_BYTES + SECTOR_BYTES;
    for class in 0..DRIVE_BYTES / 16 {
        put(&mut bytes, drives + 16 * class, 30_000);
        put(&mut bytes, drives + 16 * class + 8, 6);
        put(&mut bytes, drives + 16 * class + 12, 40_000 + class as u32);
    }
    bytes
}

#[test]
fn the_block_is_validated() {
    let bytes = block();
    assert!(Tables::new(&bytes).is_ok());
    assert!(Tables::new(&bytes[..bytes.len() - 1]).is_err());
    // An index that does not end at 117, and a repeated sector key.
    let mut bad = bytes.clone();
    let at = VECTORS_BYTES + 196 + 4 * 49;
    bad[at] = 100;
    assert!(Tables::new(&bad).is_err());
    let mut bad = bytes.clone();
    bad[VECTORS_BYTES + 4..VECTORS_BYTES + 8].copy_from_slice(&0u32.to_le_bytes());
    assert!(Tables::new(&bad).is_err());
}

#[test]
fn hand_made_sectors_come_from_the_table() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    // Sector (0, 0) is hand-made: stars 0 and 1 of the table.
    assert_eq!(tables.star_count(0, 0, 0), 2 << 10);
    let stars = tables.stars(0, 0);
    assert_eq!(stars.len(), 2);
    assert_eq!(stars[1].vector, [9, 0, 1, 2, 3, 0]);
    assert_eq!(stars[1].kind, 1);
    assert!(!tables.special_free(0, 0) && tables.special_free(0, 1));
}

#[test]
fn generated_sectors_use_the_density_and_the_generator() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    // A flat grid of 8: the density is 8 << 16 >> 8 = 2048; the half hash makes 1024 (one star).
    assert_eq!(tables.star_count(100, 200, 16), 2048);
    assert_eq!(tables.star_count(100, 200, 0), 1024);
    // Outside the galaxy there is nothing.
    assert_eq!(tables.star_count(0x2000, 5, 0), 0);
    assert_eq!(tables.star_count(5, 0x2000, 16), 0);
    // The one star comes from the generator seeded by the sector: after three steps, one more.
    let stars = tables.stars(100, 200);
    assert_eq!(stars.len(), 1);
    let mut g = Generator::new(100 | 200 << 16);
    for _ in 0..4 {
        g.step();
    }
    assert_eq!(stars[0].vector[2], ((g.a & 0xff_0000) >> 16) as u8);
    assert_eq!(stars[0].vector[0], 9);
    // The same sector always gives the same star.
    assert_eq!(tables.stars(100, 200), stars);
}

#[test]
fn the_generator_rotates_and_adds() {
    let mut g = Generator::new(1);
    assert_eq!((g.a, g.b), (0x1_0000, 1));
    g.step();
    // sum = 0x10001; a' = rotl(0x10000, 3) + sum = 0x80000 + 0x10001; b' = rotl(sum, 5).
    assert_eq!((g.a, g.b), (0x9_0001, 0x20_0020));
}

#[test]
fn positions_follow_the_sector_and_the_vector() {
    // x and y: the sector in the high half, the byte times 512 plus half a step below.
    let at = |b2: u8, b3: u8, b4: u8| position(3, 7, &[9, 0, b2, b3, b4, 0]);
    assert_eq!(at(0, 0, 0), [3 << 16 | 0x8000, 7 << 16 | 0x8000, 0]);
    assert_eq!(at(0, 1, 2), [3 << 16 | 0x8400, 7 << 16 | 0x8200, 0]);
    assert_eq!(at(0, 0xff, 0xfe), [3 << 16 | 0x7c00, 7 << 16 | 0x7e00, 0]);
    // z: F856's rounding quirk keeps one more than a plain scaling for a positive byte.
    assert_eq!(at(2, 0, 0)[2], 513);
    assert_eq!(at(0xfd, 0, 0)[2], -1536);
}

#[test]
fn distances_scale_and_overflow_as_the_original() {
    // 256 (a sector, after the >> 8) along one axis: 256 * 0xc8b4 >> 14.
    assert_eq!(distance([0, 0, 0], [0x10000, 0, 0]), 802);
    assert_eq!(distance([0x10000, 0, 0], [0, 0, 0]), 802);
    assert_eq!(distance([5, 5, 5], [5, 5, 5]), 0);
    // Large differences are halved until each fits in 15 bits, the shift following.
    // 0x4000_0000 >> 8 = 2^22: eight halvings give 2^14, shift 14 - 8, so 2^14 * 0xc8b4 >> 6.
    assert_eq!(
        distance([0, 0, 0], [0x4000_0000, 0, 0]),
        (16_384 * 0xc8b4) >> 6
    );
    // All three near 0x7fff: the sum of squares overflows 31 bits and the root is lost.
    let d = 0x7fff_i32 << 8;
    assert_eq!(distance([0, 0, 0], [d, d, d]), 0);
}

fn drives() -> Vec<Drive> {
    (0..15)
        .map(|c| Drive {
            power: 30_000,
            fuel_modifier: if c == 13 { 0x4000 } else { 0 },
            mass: 6,
            name: 40_000 + c,
        })
        .collect()
}

#[test]
fn a_jump_is_planned_from_range_fuel_and_time() {
    let drives = drives();
    // Mass 6: a range of 5000. A jump of 5000 needs one tonne and takes 7 * (65535) units.
    let ok = plan(&drives, 6, 2, 5000, 3, false, false).unwrap();
    assert_eq!(
        (ok.result, ok.fuel_needed, ok.max_range, ok.jump_time),
        (5000, 1, 5000, Some(7 * 65_535))
    );
    assert_eq!(
        ok.range_now, 5000,
        "three tonnes reach farther than the drive"
    );
    assert_eq!((ok.consumed, ok.flag), (0, None));
    // Doing it burns the fuel.
    let jump = plan(&drives, 6, 2, 5000, 3, true, true).unwrap();
    assert_eq!((jump.consumed, jump.flag), (1, Some(false)));
    assert!(plan(&drives, 6, 2, 5000, 3, false, true).unwrap().flag == Some(true));
    // Too far, or no distance: refused with the drive's numbers and no fuel figure.
    for distance in [5001, 0] {
        let no = plan(&drives, 6, 2, distance, 3, true, true).unwrap();
        assert_eq!(
            (no.result, no.fuel_needed, no.range_now, no.jump_time),
            (-1, -1, 0, None)
        );
        assert_eq!(no.max_range, 5000);
        assert_eq!(no.flag, if distance == 0 { None } else { Some(false) });
    }
    // Not enough fuel: the need is reported.
    let short = plan(&drives, 6, 2, 5000, 0, true, true).unwrap();
    assert_eq!(
        (short.result, short.fuel_needed, short.consumed),
        (-1, 1, 0)
    );
    // A heavier ship goes less far; an unknown class and a massless ship are errors.
    assert_eq!(
        plan(&drives, 12, 2, 10, 1, false, false).unwrap().max_range,
        2500
    );
    assert!(plan(&drives, 6, 15, 10, 1, false, false).is_err());
    assert!(plan(&drives, 0, 2, 10, 1, false, false).is_err());
}

#[test]
fn a_drive_modifier_saves_fuel() {
    let drives = drives();
    // Class 13 (modifier 0x4000 << 15 is a quarter): 3000 * 6 / 30001 + 1 = 1 tonne, so use
    // a heavy ship: mass 600 over 3000 gives 59 + 1 = 60 tonnes, less a quarter.
    let mut heavy = drives.clone();
    heavy[13].power = 3_000_000;
    let report = plan(&heavy, 600, 13, 3000, 100, false, false).unwrap();
    assert_eq!(report.fuel_needed, 60 - 15);
}
