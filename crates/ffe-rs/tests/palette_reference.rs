//! Local-only checks for the runtime upper palette against the content cache
//! and the executed original's captured intro frames.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test palette_reference --
//! --ignored --nocapture`
//!
//! The intro expectations come from the reference adapter's new
//! `--intro-palette`/`--intro-live` modes (`tools/reference/headless.c`), which
//! run the real F961/F962/F1330/F1331: just before the model switch the upper
//! palette is empty and the clear is black; at the switch it holds one entry,
//! six-bit `(1,1,17)`, at bank slot 63 and the clear resolves to it. One frame
//! later the model's animation adds entries.
use ffe_core::model_lighting::pack;
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::front_end::{MODEL_SECOND_BACKGROUND, MODEL_SWITCH};
use ffe_rs::galaxy_map::{date_text, GalaxyMap};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::prefs;
use ffe_rs::screen::{MapScene, Screen};

fn cache() -> ffe_content::VerifiedCache {
    let path = std::env::var_os("FFE_WORLD_CACHE").expect("FFE_WORLD_CACHE");
    ffe_content::read(&std::fs::read(path).unwrap()).unwrap()
}

fn word(colour: [u8; 3]) -> u16 {
    pack(colour) & 0xfff
}

/// F961's F1575 background at the model switch: the executed original's upper
/// palette has one entry, `(1,1,17)` six-bit, and the clear index is it.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_intro_switch_builds_the_original_background_entry() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();

    // Before the switch F961 asks for (0,0,0): black is the fixed low entry 0
    // and the executed original's upper half is empty.
    screen
        .draw_intro(Some(&viewer), MODEL_SWITCH - 1, false)
        .unwrap();
    assert_eq!(
        viewer.palette_rgb(word([0, 0, 0])),
        Some([0, 0, 0]),
        "the black background is the fixed low entry"
    );
    let upper = viewer.palette_upper6().unwrap();
    let used: Vec<[u8; 3]> = upper
        .iter()
        .copied()
        .filter(|colour| *colour != [0, 0, 0])
        .collect();
    assert!(
        used.is_empty(),
        "model 177's colours are all fixed, so the upper palette stays empty: {used:?}"
    );

    // At the switch F961 asks for (0,0,4). A single-cell box gives 4*cell+1
    // on each component: (1,1,17), exactly the reference adapter's entry.
    screen
        .draw_intro(Some(&viewer), MODEL_SWITCH, false)
        .unwrap();
    let background = viewer.palette_rgb(word(MODEL_SECOND_BACKGROUND)).unwrap();
    assert_eq!(background, [4, 4, 68], "six-bit (1,1,17) expanded by four");
    let upper = viewer.palette_upper6().unwrap();
    let used: Vec<[u8; 3]> = upper
        .iter()
        .copied()
        .filter(|colour| *colour != [0, 0, 0])
        .collect();
    assert_eq!(
        used,
        vec![[1, 1, 17]],
        "the switch frame has only F961's background in the upper palette"
    );
    // The screen is cleared to the resolved background.
    assert_eq!(screen.logical_pixels()[0], 0x04_04_44);
}

/// The fixed map names exact low palette entries: 0x000 black, 0xF00 red 16,
/// 0x444 grey 4, and the HUD/target colour 95 is a low entry.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn fixed_colour_words_use_the_low_palette() {
    let cache = cache();
    let palette = ffe_rs::palette::RuntimePalette::new(
        cache.asset("base:palette/fixed-map").unwrap().to_vec(),
        cache.asset("base:palette/low").unwrap().to_vec(),
    );
    assert_eq!(palette.resolve(0x000), Some([0, 0, 0]));
    assert_eq!(palette.resolve(0xf00), Some([240, 0, 0]));
    assert_eq!(palette.resolve(0x444), Some([64, 64, 64]));
    assert_eq!(palette.resolve(0x666), Some([96, 96, 96]));
    assert_eq!(palette.resolve(0xfff), Some([240, 240, 240]));
    // The target HUD's colour 95 is `.60 .42 .04` six-bit, the port's colour 4
    // is `.16 .16 .16`; both are fixed low entries.
    assert_eq!(palette.entry(95), Some([240, 168, 16]));
    assert_eq!(palette.entry(4), Some([64, 64, 64]));
}

/// At the switch the upper palette has F961's background only; a later intro
/// frame has grown it from the model's animation, as the reference adapter's
/// live intro does (65 entries one frame later, 128 after a few).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_intro_palette_grows_after_the_switch() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    screen
        .draw_intro(Some(&viewer), MODEL_SWITCH, false)
        .unwrap();
    let at_switch = viewer
        .palette_upper6()
        .unwrap()
        .iter()
        .filter(|colour| **colour != [0, 0, 0])
        .count();
    assert_eq!(at_switch, 1);
    screen.draw_intro(Some(&viewer), 0x181534, false).unwrap();
    let later = viewer
        .palette_upper6()
        .unwrap()
        .iter()
        .filter(|colour| **colour != [0, 0, 0])
        .count();
    assert!(later > at_switch, "the model's animation added no colours");
    let background = viewer.palette_rgb(word(MODEL_SECOND_BACKGROUND)).unwrap();
    assert!(
        background[2] > background[0] && background[2] < 96,
        "the intro background stays a dark blue: {background:?}"
    );
}

/// The owner decision stays: the default space colour is the captured entry
/// 182, six-bit (3,3,19) = #0c0c4c, and Black is exactly black.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_default_and_black_space_colours_stay_the_owners_values() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    assert_eq!(screen.sky, 0x0c_0c_4c);
    assert_eq!(prefs::space_colour_rgb(4), 0x0c_0c_4c);
    assert_eq!(prefs::space_colour_rgb(0), 0x00_0000);

    // Black clears the flight view to zero; the preference's RGB path is the
    // one that runs (the palette does not override the owner's choice).
    screen.sky = prefs::space_colour_rgb(0);
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    screen.draw(&state, ViewState::default(), None, true, "");
    assert_eq!(screen.logical_pixels()[0], 0);
}

/// The map's upper palette is built from the map model's colours and every
/// dynamic entry is an odd six-bit value in 1..=61 (F1330's representatives).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_map_builds_a_runtime_upper_palette() {
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let bytes = cache.asset("base:world/galaxy-tables").unwrap();
    let tables = ffe_core::sector::Tables::new(bytes).unwrap();
    let home = [
        0x1717_6a00u32 as i32,
        0x1524_3200u32 as i32,
        0xffff_cc00u32 as i32,
    ];
    let mut map = GalaxyMap::new(home, 1000);
    map.update_selection(&tables);
    let shapes = map.render(&viewer, &tables).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.galaxy = Some(MapScene {
        shapes,
        sector: map.sector_text(),
        fuel: None,
        date: date_text(5.0),
        info: "Ross 154".into(),
    });
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    screen.draw(&state, ViewState::default(), Some(&viewer), true, "");

    let upper = viewer.palette_upper6().unwrap();
    let used: Vec<[u8; 3]> = upper
        .iter()
        .copied()
        .filter(|colour| *colour != [0, 0, 0])
        .collect();
    assert!(!used.is_empty(), "the map registered no dynamic colours");
    assert!(
        used.iter()
            .flatten()
            .all(|value| *value % 2 == 1 && (1..=61).contains(value)),
        "non-representative upper entry in {used:?}"
    );
}
