//! Temporary: hidden-state probe for the texture rasteriser.
use ffe_rs::model_viewer::{ModelViewer, Shape};

fn cache() -> ffe_content::VerifiedCache {
    let path = std::env::var_os("FFE_GAMEOVER_CACHE").expect("FFE_GAMEOVER_CACHE");
    ffe_content::read(&std::fs::read(path).unwrap()).unwrap()
}

fn rows(viewer: &ModelViewer, yaw: i32) -> [[f64; 3]; 3] {
    let matrix = viewer.matrix_yzt(0, yaw);
    std::array::from_fn(|k| std::array::from_fn(|j| f64::from(matrix[3 * k + j]) / 2_147_483_648.0))
}

#[test]
#[ignore = "local dump"]
fn dump_shapes() {
    let cache = cache();
    let mut viewer = ModelViewer::new(&cache).unwrap();
    viewer.set_game_over_text(b"JAMESON", b"1-JAN-3250");
    let frame = viewer
        .render_scene(172, rows(&viewer, 0), [0.0, 0.0, 35000.0], [23170.0, 0.0, 23170.0])
        .unwrap();
    println!(
        "order: {:?}",
        frame
            .shapes
            .iter()
            .map(|s| match s {
                Shape::Textured(t) => format!("{:#x}", t.bitmap),
                _ => "?".to_owned(),
            })
            .collect::<Vec<_>>()
    );
    let glyph = frame
        .shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Textured(t) if t.bitmap == 0x12a => Some(t),
            _ => None,
        })
        .unwrap();
    let slab = frame
        .shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Textured(t) if t.bitmap == 0x40 => Some(t),
            _ => None,
        })
        .unwrap();
    let mut pixels = vec![[0u8; 3]; 320 * 200];
    let mut count = |pixels: &Vec<[u8; 3]>| pixels.iter().filter(|p| p != &&[0, 0, 0]).count();
    viewer.rasterise(glyph, |x, y, rgb| {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            pixels[y as usize * 320 + x as usize] = rgb;
        }
    });
    println!("glyph 1: {}", count(&pixels));
    viewer.rasterise(slab, |x, y, rgb| {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            pixels[y as usize * 320 + x as usize] = rgb;
        }
    });
    println!("slab: {}", count(&pixels));
    pixels.iter_mut().for_each(|p| *p = [0, 0, 0]);
    viewer.rasterise(glyph, |x, y, rgb| {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            pixels[y as usize * 320 + x as usize] = rgb;
        }
    });
    println!("glyph 2: {}", count(&pixels));
    // Draw the slab first into a fresh buffer, then the glyph.
    pixels.iter_mut().for_each(|p| *p = [0, 0, 0]);
    viewer.rasterise(slab, |x, y, rgb| {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            pixels[y as usize * 320 + x as usize] = rgb;
        }
    });
    let after_slab = pixels.clone();
    viewer.rasterise(glyph, |x, y, rgb| {
        if (0..320).contains(&x) && (0..200).contains(&y) {
            pixels[y as usize * 320 + x as usize] = rgb;
        }
    });
    let changed = pixels
        .iter()
        .zip(&after_slab)
        .filter(|(a, b)| a != b)
        .count();
    println!("glyph after slab changed {changed}");
}
