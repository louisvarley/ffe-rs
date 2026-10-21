use ffe_rs::indexed::BitmapFont;
use ffe_rs::indexed::IndexedImage;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    glyph: u8,
    x: i32,
    y: i32,
    min_y: i32,
    max_y: i32,
    advance: usize,
    width: usize,
    pixels_fnv1a64: String,
}

#[test]
#[ignore = "Local-only executed JJFFE comparison; use tools/reference/verify-font.py"]
fn executed_reference_glyphs_and_clipping() {
    let cache_path = std::env::var_os("FFE_FONT_CACHE").expect("FFE_FONT_CACHE required");
    let vectors_path = std::env::var_os("FFE_FONT_VECTORS").expect("FFE_FONT_VECTORS required");
    let cache_bytes = std::fs::read(cache_path).unwrap();
    let cache = ffe_content::read(&cache_bytes).unwrap();
    let font = BitmapFont::new(cache.asset("base:font/main").unwrap()).unwrap();
    let input = std::fs::read_to_string(vectors_path).unwrap();
    assert!(input.len() < 256 * 1024);
    let cases = [
        (0, 0, 200),
        (157, 0, 200),
        (195, 0, 200),
        (-4, 0, 200),
        (3, 5, 8),
        (200, 0, 200),
    ];
    let mut count = 0;
    for (index, line) in input.lines().enumerate() {
        let vector: Vector = serde_json::from_str(line).unwrap();
        assert_eq!(vector.glyph, 32 + (index / cases.len()) as u8);
        assert_eq!(vector.x, 7);
        assert_eq!(
            (vector.y, vector.min_y, vector.max_y),
            cases[index % cases.len()]
        );
        let mut pixels = [3; 64000];
        let advance = font
            .draw(
                &mut pixels,
                vector.glyph,
                77,
                vector.x,
                vector.y,
                vector.min_y..vector.max_y,
            )
            .unwrap();
        assert_eq!(advance, vector.advance, "row {index}");
        assert_eq!(font.advance(vector.glyph), Some(vector.width));
        let hash = pixels.iter().fold(14695981039346656037u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
        });
        assert_eq!(format!("{hash:016x}"), vector.pixels_fnv1a64, "row {index}");
        count += 1;
    }
    assert_eq!(count, 116 * cases.len());
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlitVector {
    slot: u32,
    x: i32,
    y: i32,
    pixels_fnv1a64: String,
}

#[test]
#[ignore = "Local-only executed JJFFE comparison; use tools/reference/verify-font.py"]
fn executed_reference_console_sprites_and_clipping() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_FONT_CACHE").unwrap()).unwrap())
            .unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_BLIT_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 * 1024);
    let slots = [43, 0, 44, 66];
    let cases = [
        (0, 158),
        (-5, -3),
        (319, 199),
        (0, 0),
        (320, 200),
        (-400, -300),
        (-1, 170),
    ];
    let mut count = 0;
    for (index, line) in input.lines().enumerate() {
        let vector: BlitVector = serde_json::from_str(line).unwrap();
        assert_eq!(vector.slot, slots[index / cases.len()]);
        assert_eq!((vector.x, vector.y), cases[index % cases.len()]);
        let id = format!("base:bitmap/primary/{:03}", vector.slot);
        let record = cache
            .manifest()
            .files
            .iter()
            .find(|record| record.id == id)
            .unwrap();
        let bitmap = match record.asset {
            ffe_content::AssetKind::IndexedBitmap {
                width,
                height,
                offset_x,
                offset_y,
                masked_zero,
                ..
            } => IndexedImage::new(
                width,
                height,
                [offset_x, offset_y],
                masked_zero,
                cache.asset(&id).unwrap(),
            )
            .unwrap(),
            _ => panic!("Expected bitmap"),
        };
        let mut pixels = [3; 64000];
        bitmap.blit(&mut pixels, vector.x, vector.y);
        let hash = pixels.iter().fold(14695981039346656037u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
        });
        assert_eq!(format!("{hash:016x}"), vector.pixels_fnv1a64, "row {index}");
        count += 1;
    }
    assert_eq!(count, 28);
}
