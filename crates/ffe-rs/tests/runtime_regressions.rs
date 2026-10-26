//! Regression guards for the runtime bugs reported against `127db08`: the
//! source key map (F3 is the galaxy map, the preset is Port Options only), the
//! retired port pause menu, the clock running outside the flight model, and the
//! pad services board keeping the flight state alive.
//!
//! These are source-level checks because the affected routing lives in the
//! event loop (`crates/ffe-rs/src/main.rs`), which no integration test drives
//! without a window. The behaviour has separate tests where it can be pure:
//! `input::f3_is_the_source_map_key_and_not_a_preset_switch`,
//! `tests/prefs.rs::the_port_page_flips_the_preset_and_shows_it`, and the HUD,
//! clock and landed-input tests.

use std::path::Path;

fn main_rs() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join("src/main.rs")).unwrap()
}

#[test]
fn the_port_pause_menu_is_gone() {
    let source = main_rs();
    for removed in [
        "screen.overlay(",
        "\"PAUSED\"",
        "P RESUMES",
        "F2 STATUS PAGE",
    ] {
        assert!(
            !source.contains(removed),
            "the retired port pause menu is back: {removed:?}"
        );
    }
}

#[test]
fn f3_is_not_a_control_preset_toggle() {
    let source = main_rs();
    assert!(
        !source.contains("prefs.toggle_modern()"),
        "F3 must not flip the preset; Port Options owns that choice"
    );
    assert!(
        source.contains("input::galaxy_map_key(key)"),
        "F3 must route through the source galaxy-map key"
    );
}

#[test]
fn the_clock_is_not_owned_by_the_original_flight_branch() {
    let source = main_rs();
    assert!(
        source.contains("if clock_runs("),
        "the clock gate helper must be wired into the frame"
    );
    assert!(
        source.contains("fn clock_runs("),
        "clock_runs documents that pausing, pages, death and dev views stop time"
    );
    // The only nav.advance call is the frame clock, not a second one inside the
    // original model's branch (which froze the date on the pad and in stations).
    assert_eq!(
        source.matches("nav.advance(&cache").count(),
        1,
        "there must be exactly one game-clock advance per frame"
    );
}

#[test]
fn the_pad_services_board_does_not_change_flight_mode() {
    let source = main_rs();
    assert!(
        source.contains("screen.pad_services = pad_board;"),
        "the services board must draw over the landed view"
    );
    assert!(
        source.contains("let pad_services = nav.as_ref().is_some_and"),
        "F4 must recognise the starport pad without switching to Docked"
    );
    // The F4 pad branch no longer writes the docked mode; the only remaining
    // `state.mode = ...Docked` assignments belong to real docking/landing.
    let f4 = source
        .split("F4 is the original's station/comms action")
        .nth(1)
        .and_then(|tail| tail.split("if active && !death_control").next())
        .expect("the F4 branch");
    assert!(
        !f4.contains("state.mode = ffe_rs::flight::Mode::Docked"),
        "F4 from the pad must not switch to the docked mode"
    );
}

#[test]
fn a_landed_ship_runs_its_flight_input_and_contact_scan() {
    let source = main_rs();
    assert!(
        source.contains("let landed_parent = nav.as_ref().and_then"),
        "the landed branch must resolve its parent for F1033/F1037"
    );
    assert!(
        source.contains("model.set_mode(mode);\n                    if let Some((index, parent, at, info)) = landed_parent"),
        "the landed mode must fall through to step_landing, not stop at set_mode"
    );
}
