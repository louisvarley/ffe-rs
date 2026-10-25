//! Local-only intro render check against the real cache. Run with
//! `FFE_INTRO_CACHE=<data.bin> cargo test -p ffe-rs --test intro_view -- --ignored`.
//!
//! This is a smoke test of the assembled screen, not a pixel oracle: the DOS
//! executable is not runnable here, and the dynamic upper palette is not ported
//! (see `docs/notes/intro.md`).
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
    let first = non_black(&screen);
    assert!(
        first > 2_000,
        "first credit/bitmap frame is nearly empty: {first}"
    );
    // The bitmap occupies the bottom rows.
    let bottom = screen.logical_pixels()[320 * 170..320 * 200]
        .iter()
        .filter(|pixel| **pixel != 0)
        .count();
    assert!(bottom > 1_000, "the 0x42 strip is missing: {bottom}");

    // A mid-credit fly-past renders the ship/model above the strip.
    screen.draw_intro(Some(&scene), 0x181534, false).unwrap();
    let middle = non_black(&screen);
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
