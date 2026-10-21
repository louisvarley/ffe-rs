use ffe_rs::indexed::{palette_rgb6, BitmapFont, IndexedImage};

fn font() -> Vec<u8> {
    let mut bytes = vec![0; 1160];
    for record in bytes.chunks_exact_mut(10) {
        record[0] = 0x81;
        record[1] = 0x40;
        record[8] = 0x20;
        record[9] = 4;
    }
    bytes
}

#[test]
fn font_bit_order_proportional_advances_and_transparent_gaps() {
    let mut bytes = font();
    bytes[19] = 7;
    let font = BitmapFont::new(&bytes).unwrap();
    let mut pixels = [3; 64000];
    assert_eq!(font.draw(&mut pixels, b' ', 77, 7, 5, 0..200).unwrap(), 4);
    assert_eq!(pixels[5 * 320 + 7], 77);
    assert_eq!(pixels[5 * 320 + 14], 77);
    assert_eq!(pixels[5 * 320 + 8], 3);
    assert_eq!(pixels[6 * 320 + 8], 77);
    assert_eq!(pixels[13 * 320 + 9], 77);
    assert_eq!(font.width(" !").unwrap(), 11);
    assert_eq!(font.advance(31), None);
    assert_eq!(font.advance(147), Some(4));
    assert_eq!(font.advance(148), None);
    assert!(font.width("\u{00e9}").is_err());
}

#[test]
fn font_clips_without_wrapping_or_panicking_on_extreme_coordinates() {
    let font = BitmapFont::new(&font()).unwrap();
    for (x, y) in [
        (-7, -8),
        (319, 199),
        (i32::MAX, i32::MAX),
        (i32::MIN, i32::MIN),
    ] {
        let mut pixels = [3; 64000];
        font.draw(&mut pixels, b'A', 77, x, y, 0..200).unwrap();
        if x == 319 {
            assert_eq!(pixels[199 * 320 + 319], 77);
            assert_eq!(pixels[0], 3);
        }
    }
    let mut pixels = [3; 64000];
    font.draw(&mut pixels, b'A', 77, 10, 3, 5..8).unwrap();
    assert_eq!(pixels, [3; 64000]);
    assert_eq!(
        font.draw(&mut pixels, b'A', 77, 10, 200, 0..200).unwrap(),
        0
    );
    assert!(font.draw(&mut pixels, 0, 77, 0, 0, 0..200).is_err());
    assert!(font.draw(&mut pixels, b'A', 77, 0, 0, -1..200).is_err());
}

#[test]
fn font_rejects_truncated_oversized_or_invalid_advances() {
    let valid = font();
    for length in [0, 9, 1159] {
        assert!(BitmapFont::new(&valid[..length]).is_err());
    }
    assert!(BitmapFont::new(&vec![1; 1161]).is_err());
    for width in [0, 10, 255] {
        let mut invalid = valid.clone();
        invalid[1159] = width;
        assert!(BitmapFont::new(&invalid).is_err());
    }
}

#[test]
fn indexed_images_preserve_zero_mask_offsets_and_clip_all_edges() {
    let mut pixels = [5; 64000];
    let opaque = IndexedImage::new(2, 2, [-1, 1], false, &[0, 1, 2, 3]).unwrap();
    opaque.blit(&mut pixels, 1, 0);
    assert_eq!(&pixels[320..322], &[0, 1]);
    assert_eq!(&pixels[640..642], &[2, 3]);
    let masked = IndexedImage::new(2, 2, [0, 0], true, &[0, 1, 2, 3]).unwrap();
    masked.blit(&mut pixels, 319, 199);
    assert_eq!(pixels[63999], 5);
    masked.blit(&mut pixels, -1, -1);
    assert_eq!(pixels[0], 3);
    for (x, y) in [(i32::MAX, i32::MAX), (i32::MIN, i32::MIN)] {
        masked.blit(&mut pixels, x, y);
    }
    for data in [vec![], vec![128], vec![1, 2]] {
        assert!(IndexedImage::new(1, 1, [0, 0], false, &data).is_err());
    }
}

#[test]
fn fixed_palette_uses_six_bit_left_shift_not_full_range_or_cabin_palette() {
    let palette = palette_rgb6(&vec![63; 384]).unwrap();
    assert_eq!(palette, [0xfcfcfc; 128]);
    assert!(palette_rgb6(&vec![64; 384]).is_err());
    assert!(palette_rgb6(&vec![63; 383]).is_err());
}
