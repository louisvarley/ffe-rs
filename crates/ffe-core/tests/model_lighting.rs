use ffe_core::model_lighting::{default_lighting, pack, star_shades};

#[test]
fn lighting_tables_parse_and_colours_pack_like_f1326() {
    // Light (-18919, -18919, 18919): about 0x8000 long, like F960's default.
    let mut bytes = Vec::new();
    for v in [-18919i32, -18919, 18919] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend((0..8u8).flat_map(|i| [7 - i, 7 - i, 6 - i.min(6)]));
    let lighting = default_lighting(&bytes).unwrap();
    assert_eq!(lighting.light, [-18919, -18919, 18919]);
    assert_eq!(lighting.shades[0], [7, 7, 6]);
    assert_eq!(lighting.shades[7], [0, 0, 0]);
    assert!(default_lighting(&bytes[..30]).is_err());
    assert_eq!(star_shades(&[3; 144]).unwrap().len(), 6);
    assert!(star_shades(&[3; 143]).is_err());
    // F1326 packs r << 8 + g << 4 + b with no clamp: 16 carries into the next nibble.
    assert_eq!(pack([1, 2, 3]), 0x123);
    assert_eq!(pack([15, 15, 15]), 0xfff);
    assert_eq!(pack([0, 16, 0]), 0x100);
}
