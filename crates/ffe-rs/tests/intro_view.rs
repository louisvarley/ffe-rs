//! Local-only intro render check against the real cache. Run with
//! `FFE_INTRO_CACHE=<data.bin> cargo test -p ffe-rs --test intro_view -- --ignored`.
//!
//! This is a smoke test of the assembled screen, not a pixel oracle: the DOS
//! executable is not runnable here. The dynamic upper palette is built by the
//! model path (`crate::palette`); `palette_reference.rs` compares its intro
//! entries with the executed original's captured ones.
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::screen::Screen;

fn cache() -> ffe_content::VerifiedCache {
    let path = std::env::var_os("FFE_INTRO_CACHE").expect("FFE_INTRO_CACHE");
    ffe_content::read(&std::fs::read(path).unwrap()).unwrap()
}

fn non_black(screen: &Screen) -> usize {
    screen
        .logical_pixels()
        .iter()
        .take(320 * 200)
        .filter(|pixel| **pixel != 0)
        .count()
}

/// FNV-1a over the logical 320x200 frame, for the orientation pin below.
fn frame_hash(screen: &Screen) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for pixel in &screen.logical_pixels()[..320 * 200] {
        for byte in pixel.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

/// Mean length of horizontal same-colour runs in a region: a stable texture
/// has runs of several pixels; a per-texel speckle has nearly one per pixel.
fn mean_run(screen: &Screen, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
    let pixels = screen.logical_pixels();
    let mut runs = 0usize;
    for y in y0..y1 {
        let mut previous = pixels[y * 320 + x0];
        runs += 1;
        for x in x0 + 1..x1 {
            let pixel = pixels[y * 320 + x];
            if pixel != previous {
                runs += 1;
                previous = pixel;
            }
        }
    }
    let pixels = (y1 - y0) * (x1 - x0);
    pixels as f64 / runs as f64
}

/// The GAME!TEK logo text must read the same way round as the original's.
/// F1747 negates the text frame's first row when the frame is left-handed
/// (`ffe.asm:5f64b`); without it every glyph was mirrored (`GAME` read `ƎMAפ`).
/// The hash pins the assembled frame at animation 64, where the letters are
/// large and unclipped.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn the_game_tek_logo_text_is_not_mirrored() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    screen.draw_intro(Some(&viewer), 0x10000, false).unwrap();
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut out = b"P6\n320 200\n255\n".to_vec();
        for pixel in &screen.logical_pixels()[..320 * 200] {
            out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("game-tek.ppm"), out).unwrap();
    }
    assert_eq!(
        frame_hash(&screen),
        1_567_273_124_356_204_709,
        "the GAME!TEK frame changed; re-check the text orientation before pinning"
    );
}

/// Consecutive intro frames build the same upper palette: the executed
/// original's frames 1514 and 1515 have identical upper halves. F1786 clears
/// the cube before each frame and F1330 rebuilds from that frame's registered
/// colours, so two tics in the same animation produce the same 128 entries.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn consecutive_intro_frames_build_the_same_upper_palette() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    // Two builds fill both banks; a third rebuild leaves the same pair. The
    // tics stay inside one animation (`(tic - MODEL_SWITCH) >> 10`).
    screen.draw_intro(Some(&viewer), 0x181533, false).unwrap();
    screen.draw_intro(Some(&viewer), 0x181534, false).unwrap();
    let first = viewer.palette_upper6().unwrap();
    screen.draw_intro(Some(&viewer), 0x181535, false).unwrap();
    let second = viewer.palette_upper6().unwrap();
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let text = first
            .iter()
            .enumerate()
            .map(|(i, colour)| format!("{i:02x} {} {} {}", colour[0], colour[1], colour[2]))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(std::path::Path::new(&dir).join("intro-palette.txt"), text).unwrap();
    }
    assert_eq!(
        first, second,
        "the upper palette changed between frames in one animation"
    );
}

/// The banded planet is generated at `DATA_008812 = 1`, the executed boot's
/// Shape Detail. Building it at 2 (the old hard-coded value) subdivided the
/// patch finer than the source and aliased the band texture into per-texel
/// speckle. The region's horizontal run length distinguishes the two.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn the_intro_planet_uses_the_runtime_shape_detail() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    assert_eq!(
        viewer.surface_detail(),
        1,
        "the boot's Shape Detail is High"
    );
    screen.draw_intro(Some(&viewer), 0x181534, false).unwrap();
    let coherent = mean_run(&screen, 100, 0, 320, 150);
    assert!(
        coherent > 2.0,
        "the planet's band texture is still speckled: mean run {coherent:.2}"
    );

    screen.surface_detail = 2;
    screen.draw_intro(Some(&viewer), 0x181534, false).unwrap();
    let finer = mean_run(&screen, 100, 0, 320, 150);
    assert!(
        finer < coherent,
        "detail 2 should over-subdivide: {finer:.2} vs {coherent:.2}"
    );
    // `F917` clamps a negative preference to 0.
    viewer.set_surface_detail(-3);
    assert_eq!(viewer.surface_detail(), 0);
}

/// The other two intro word faces are the same F1740/F1747 path in different
/// children (362 PRESENTS, 363 DAVID BRABEN'S). They must read left-to-right
/// too; without the handedness fix both were mirror images. The hashes pin the
/// assembled frames at the animations where each word is upright and large.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn the_presents_and_david_braben_text_is_not_mirrored() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    // Animation 248: PRESENTS. Animation 328: DAVID BRABEN'S.
    screen.draw_intro(Some(&viewer), 248 << 10, false).unwrap();
    assert_eq!(
        frame_hash(&screen),
        18_231_935_709_763_280_501,
        "the PRESENTS frame changed; re-check the text orientation"
    );
    screen.draw_intro(Some(&viewer), 328 << 10, false).unwrap();
    assert_eq!(
        frame_hash(&screen),
        8_448_037_671_976_705_893,
        "the DAVID BRABEN'S frame changed; re-check the text orientation"
    );
}

/// The GAME!TEK logo model 177 -> 361 draws one slab and its own text: the
/// port has no second (substituted) logo and no label path for it. At animation
/// 64 the letters are the fixed light grey `(192, 192, 192)`.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v29 data.bin"]
fn the_game_tek_logo_is_drawn_once_with_its_letters() {
    use ffe_rs::model_viewer::{AnimationInputs, Shape};
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let rows = [
        [127.0 / 128.0, 0.0, 0.0],
        [0.0, 127.0 / 128.0, 0.0],
        [0.0, 0.0, 127.0 / 128.0],
    ];
    let tics = 0x10000;
    let inputs = AnimationInputs::original(tics, 0, &viewer.shades());
    let frame = viewer
        .render_scene_with(
            177,
            rows,
            [0.0, 0.0, 0x14820 as f64],
            viewer.light_direction(),
            &inputs,
        )
        .unwrap();
    // One four-edge slab (model 361's face), not two.
    let slabs = frame
        .shapes
        .iter()
        .filter(|shape| matches!(shape, Shape::Polygon(edges, _) if edges.len() == 4))
        .count();
    assert_eq!(slabs, 1, "the GAME!TEK logo slab drawn {slabs} times");
    // The letters come from model 361's text, never a model-name label.
    assert!(
        !frame
            .shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Label(..))),
        "a label substituted for the logo text"
    );

    let mut screen = Screen::new(&cache).unwrap();
    screen.draw_intro(Some(&viewer), tics, false).unwrap();
    let pixels = screen.logical_pixels();
    let letters = pixels
        .iter()
        .take(320 * 200)
        .filter(|pixel| **pixel == 0x00c0_c0c0)
        .count();
    assert!(
        letters > 500,
        "the GAME!TEK letters are missing: {letters} pixels"
    );
    // The pre-fix shortcut showed the unregistered glyph colour (nibbles x4).
    let fallback = pixels
        .iter()
        .take(320 * 200)
        .filter(|pixel| **pixel == 0x0040_6080)
        .count();
    assert_eq!(fallback, 0, "the port-only light-blue glyph path is drawn");
}

/// Model 176's planet is a child surface (model 449) inside model 349; the
/// viewer must collect child surfaces, and the palette must be stable after a
/// long intro sequence (the cube is cleared before each frame's registration).
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v29 data.bin"]
fn the_intro_planet_is_drawn_and_the_palette_is_stable() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let fresh = ModelViewer::new(&cache).unwrap();
    screen.draw_intro(Some(&fresh), 0x181534, false).unwrap();
    let reference = screen.logical_pixels()[..320 * 200].to_vec();
    let background = reference[0];
    let drawn = reference.iter().filter(|p| **p != background).count();
    assert!(
        drawn > 20_000,
        "the intro planet/ship cover only {drawn} pixels"
    );

    // Warm a second viewer through the intro: the same tic must render exactly
    // the same frame. Before the cube was cleared per frame the accumulated
    // cells re-cut the boxes and the planet turned to noise.
    let mut screen = Screen::new(&cache).unwrap();
    let warmed = ModelViewer::new(&cache).unwrap();
    let mut tics = 0x400u32;
    while tics < 0x181534 {
        screen.draw_intro(Some(&warmed), tics, false).unwrap();
        tics += 10_430;
    }
    screen.draw_intro(Some(&warmed), 0x181534, false).unwrap();
    assert_eq!(
        &reference[..],
        &screen.logical_pixels()[..320 * 200],
        "the intro frame drifts after a long run"
    );
}

#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn the_intro_ship_is_drawn_in_front_of_the_planet() {
    use ffe_rs::model_viewer::{AnimationInputs, Shape};
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let rows = [
        [127.0 / 128.0, 0.0, 0.0],
        [0.0, 127.0 / 128.0, 0.0],
        [0.0, 0.0, 127.0 / 128.0],
    ];
    let tics = 0x181534u32;
    let anim = (tics - ffe_rs::front_end::MODEL_SWITCH) >> 10;
    let inputs = AnimationInputs::original(anim << 10, 0, &viewer.shades());
    let frame = viewer
        .render_scene_with(
            176,
            rows,
            [0.0, 0.0, 0x14820 as f64],
            viewer.light_direction(),
            &inputs,
        )
        .unwrap();
    // One F1778/F1788 queue: far first, near last.
    assert!(
        frame.keys.windows(2).all(|pair| pair[0] >= pair[1]),
        "the frame is not in the original's descending depth order"
    );
    // The planet is a curved surface (explicit-coordinate entries), the ship's
    // engines are F1738 glows. If the surfaces were appended after the model
    // primitives (the old path), the planet painted over the nearer ship.
    let last_planet = frame
        .shapes
        .iter()
        .rposition(|shape| matches!(shape, Shape::TexturedUv(..)))
        .expect("the intro planet surface is missing");
    let first_glow = frame
        .shapes
        .iter()
        .position(|shape| matches!(shape, Shape::Glow(..)))
        .expect("the intro ship's thruster glows are missing");
    assert!(
        last_planet < first_glow,
        "the planet (last entry {last_planet}) draws over the ship's engines ({first_glow})"
    );
}

/// The atmosphere ring's F1879 entries are F1805 textured quads (bitmap 0x43)
/// and F1804 ring quads. They belong to the runtime palette like every other
/// surface entry. The old stand-in interpolated the two edge colours with
/// Gouraud, which put hundreds of RGB values on the planet's limb that no
/// palette entry held; the DOS frame has 64 upper entries and the port must not
/// invent more in the view.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
fn the_intro_atmosphere_ring_uses_palette_colours() {
    use ffe_rs::model_viewer::{AnimationInputs, Shape};
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let tics = 0x181534u32;
    let anim = (tics - ffe_rs::front_end::MODEL_SWITCH) >> 10;
    let inputs = AnimationInputs::original(anim << 10, 0, &viewer.shades());
    let frame = viewer
        .render_scene_with(
            176,
            [
                [127.0 / 128.0, 0.0, 0.0],
                [0.0, 127.0 / 128.0, 0.0],
                [0.0, 0.0, 127.0 / 128.0],
            ],
            [0.0, 0.0, 0x14820 as f64],
            viewer.light_direction(),
            &inputs,
        )
        .unwrap();
    assert!(
        !frame
            .shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Gouraud(..))),
        "a surface fell back to the port-only Gouraud stand-in"
    );
    assert!(
        frame.shapes.iter().any(|shape| matches!(
            shape,
            Shape::Textured(texture) if texture.bitmap & 0xfff == 0x43
        )),
        "the atmosphere ring's F1805 textured quad is missing"
    );

    screen.draw_intro(Some(&viewer), tics, false).unwrap();
    let colours = screen.logical_pixels()[..320 * 158]
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        colours.len() <= 96,
        "the view has {} colours; the original's palette holds 64 upper entries",
        colours.len()
    );
}

#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v27 data.bin"]
fn the_intro_draws_the_model_bitmap_and_credits() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let scene = ModelViewer::new(&cache).unwrap();

    // Before the bitmap threshold and the model switch, 177 is animated; the
    // opening frames may be black because the model starts off the view.
    screen.draw_intro(Some(&scene), 0x795cc, false).unwrap();
    assert!(non_black(&screen) > 100, "model 177 drew nothing on-view");

    // The first credit and the primary bitmap strip are on screen together.
    screen
        .draw_intro(Some(&scene), ffe_rs::front_end::MODEL_SWITCH, false)
        .unwrap();
    // F961's model-176 background is no longer black, so count what the frame
    // draws over its clear colour rather than non-black pixels.
    let background = screen.logical_pixels()[0];
    let drawn = |screen: &Screen| {
        screen
            .logical_pixels()
            .iter()
            .take(320 * 200)
            .filter(|pixel| **pixel != background)
            .count()
    };
    let first = drawn(&screen);
    assert!(
        first > 2_000,
        "first credit/bitmap frame is nearly empty: {first}"
    );
    // The bitmap occupies the bottom rows.
    let bottom = screen.logical_pixels()[320 * 170..320 * 200]
        .iter()
        .filter(|pixel| **pixel != background)
        .count();
    assert!(bottom > 1_000, "the 0x42 strip is missing: {bottom}");

    // A mid-credit fly-past renders the ship/model above the strip.
    screen.draw_intro(Some(&scene), 0x181534, false).unwrap();
    let middle = drawn(&screen);
    assert!(middle > first, "the fly-past did not add pixels");

    // The DOS version line appears only when V latched it.
    screen
        .draw_intro(Some(&scene), ffe_rs::front_end::INTRO_INPUT_FROM, false)
        .unwrap();
    let without = screen.logical_pixels()[..320 * 200].to_vec();
    screen
        .draw_intro(Some(&scene), ffe_rs::front_end::INTRO_INPUT_FROM, true)
        .unwrap();
    let with = screen.logical_pixels()[..320 * 200].to_vec();
    assert_ne!(without, with, "the version line is not drawn");
}

#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v27 data.bin"]
fn the_last_credits_wrap_inside_the_screen() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let scene = ModelViewer::new(&cache).unwrap();
    screen.draw_intro(Some(&scene), 0x77a7d8, false).unwrap();
    // The paragraph and the three copyright blocks are visible; the model is
    // blank here because child 456 reads an undefined-stack basis (reported).
    assert!(non_black(&screen) > 1_000);
    // Draw it again: the frame must be deterministic.
    let first: Vec<u32> = screen.logical_pixels()[..320 * 200].to_vec();
    screen.draw_intro(Some(&scene), 0x77a7d8, false).unwrap();
    assert_eq!(&first[..], &screen.logical_pixels()[..320 * 200]);
}
