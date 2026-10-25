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

/// The GAME!TEK logo model 177 -> 361 draws one slab and its own text: the
/// port has no second (substituted) logo and no label path for it. At animation
/// 64 the letters are the fixed light grey `(192, 192, 192)`.
#[test]
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
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
#[ignore = "Local-only intro frame render; set FFE_INTRO_CACHE to a v28 data.bin"]
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
