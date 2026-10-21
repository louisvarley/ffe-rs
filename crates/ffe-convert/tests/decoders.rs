use ffe_convert::{decode_indexed_bmp, pcm16_mono_22050_wav, recommended_scale};
use std::io::Cursor;

fn bmp(top_down: bool) -> Vec<u8> {
    let mut bytes = vec![0; 70];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&70u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&62u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&3i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&(if top_down { -2i32 } else { 2 }).to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&8u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&8u32.to_le_bytes());
    bytes[46..50].copy_from_slice(&2u32.to_le_bytes());
    bytes[54..62].copy_from_slice(&[30, 20, 10, 0, 60, 50, 40, 0]);
    if top_down {
        bytes[62..].copy_from_slice(&[0, 1, 0, 99, 1, 0, 1, 99]);
    } else {
        bytes[62..].copy_from_slice(&[1, 0, 1, 99, 0, 1, 0, 99]);
    }
    bytes
}

#[test]
fn bmp_rows_palette_and_padding_are_lossless() {
    let image = decode_indexed_bmp(&bmp(false)).unwrap();
    assert_eq!([image.width, image.height], [3, 2]);
    assert_eq!(image.indices, [0, 1, 0, 1, 0, 1]);
    assert_eq!(image.palette_rgb, [10, 20, 30, 40, 50, 60]);
    assert_eq!(image, decode_indexed_bmp(&bmp(true)).unwrap());
    let mut unspecified_size = bmp(false);
    unspecified_size[2..6].fill(0);
    assert_eq!(image, decode_indexed_bmp(&unspecified_size).unwrap());
    assert_eq!(
        &image.rgba().unwrap()[..8],
        &[10, 20, 30, 255, 40, 50, 60, 255]
    );
}

#[test]
fn indexed_and_rgba_png_roundtrip_with_deterministic_bytes() {
    let image = decode_indexed_bmp(&bmp(false)).unwrap();
    for (bytes, expected, color) in [
        (
            image.indexed_png().unwrap(),
            image.indices.clone(),
            png::ColorType::Indexed,
        ),
        (
            image.rgba_png().unwrap(),
            image.rgba().unwrap(),
            png::ColorType::Rgba,
        ),
    ] {
        let mut reader = png::Decoder::new(Cursor::new(&bytes)).read_info().unwrap();
        assert_eq!(reader.info().width, 3);
        assert_eq!(reader.info().height, 2);
        assert_eq!(reader.info().color_type, color);
        if color == png::ColorType::Indexed {
            assert_eq!(
                reader.info().palette.as_deref(),
                Some(image.palette_rgb.as_slice())
            );
            assert_eq!(bytes, image.indexed_png().unwrap());
        } else {
            assert_eq!(bytes, image.rgba_png().unwrap());
        }
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(&pixels[..frame.buffer_size()], expected);
    }
}

#[test]
fn every_truncated_bmp_prefix_is_rejected_without_panicking() {
    let input = bmp(false);
    for end in 0..input.len() {
        assert!(decode_indexed_bmp(&input[..end]).is_err(), "prefix {end}");
        let mut resized_header = input[..end].to_vec();
        if end >= 6 {
            resized_header[2..6].copy_from_slice(&(end as u32).to_le_bytes());
            assert!(
                decode_indexed_bmp(&resized_header).is_err(),
                "adjusted prefix {end}"
            );
            resized_header[2..6].fill(0);
            assert!(
                decode_indexed_bmp(&resized_header).is_err(),
                "zero-size prefix {end}"
            );
        }
    }
}

#[test]
fn invalid_headers_offsets_indices_and_limits_are_rejected() {
    for (offset, replacement) in [
        (0, b"NO".to_vec()),
        (2, 69u32.to_le_bytes().to_vec()),
        (14, 108u32.to_le_bytes().to_vec()),
        (18, 0u32.to_le_bytes().to_vec()),
        (18, u32::MAX.to_le_bytes().to_vec()),
        (18, 8193u32.to_le_bytes().to_vec()),
        (22, 0u32.to_le_bytes().to_vec()),
        (22, i32::MIN.to_le_bytes().to_vec()),
        (26, 2u16.to_le_bytes().to_vec()),
        (28, 24u16.to_le_bytes().to_vec()),
        (30, 1u32.to_le_bytes().to_vec()),
        (46, 257u32.to_le_bytes().to_vec()),
        (10, 58u32.to_le_bytes().to_vec()),
        (10, u32::MAX.to_le_bytes().to_vec()),
        (34, 7u32.to_le_bytes().to_vec()),
        (62, vec![2]),
    ] {
        let mut bytes = bmp(false);
        bytes[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            decode_indexed_bmp(&bytes).is_err(),
            "header offset {offset}"
        );
    }
}

#[test]
fn public_image_fields_cannot_make_encoding_panic() {
    let mut image = decode_indexed_bmp(&bmp(false)).unwrap();
    image.indices[0] = 255;
    assert!(image.rgba_png().is_err());
    assert!(image.indexed_png().is_err());
    image.indices.clear();
    assert!(image.rgba().is_err());
}

#[test]
fn wav_header_and_samples_are_exact_and_empty_sentinel_is_valid() {
    for samples in [b"".as_slice(), b"\x00\x80\xff\x7f\x00\x00"] {
        let wav = pcm16_mono_22050_wav(samples).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()),
            (samples.len() + 36) as u32
        );
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 22050);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(&wav[44..], samples);
    }
    assert!(pcm16_mono_22050_wav(&[1]).is_err());
}

#[test]
fn uhd_recommendations_are_per_asset_and_overflow_checked() {
    assert_eq!(recommended_scale([320, 200], [2880, 2160]).unwrap(), 11);
    assert_eq!(recommended_scale([64, 64], [256, 256]).unwrap(), 4);
    assert_eq!(recommended_scale([320, 158], [2880, 1707]).unwrap(), 11);
    assert_eq!(recommended_scale([320, 200], [160, 100]).unwrap(), 1);
    assert!(recommended_scale([0, 1], [1, 1]).is_err());
    assert!(recommended_scale([1, 1], [0, 1]).is_err());
    assert!(recommended_scale([2, 1], [u32::MAX, 1]).is_err());
}
