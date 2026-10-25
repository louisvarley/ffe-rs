use ffe_rs::console::{
    acceleration_for_digit, click, Click, ConsoleState, ACCELERATION_NAMES, BAR, RIGHT_BUTTONS,
    TIME_ACCELERATION,
};
use ffe_rs::flight::Flight;
use ffe_rs::original_flight::{Keys, OriginalFlight};
use ffe_rs::screen::Screen;
use ffe_rs::ships::ShipStats;

const EAGLE: [u8; 48] = [
    0xde, 0x3f, 0xd7, 0xec, 0x1, 0x0, 0x10, 0x0, 0xb, 0x0, 0x2e, 0x0, 0x64, 0x0, 0x10, 0x40, 0x1,
    0x0, 0x2, 0x0, 0xa, 0x80, 0x0, 0x1, 0x4e, 0x7, 0x7f, 0x0, 0x25, 0x5, 0x7f, 0x0, 0xa2, 0x8, 0x0,
    0x0, 0x2c, 0x0, 0x1e, 0x0, 0xc0, 0x0, 0x2a, 0x0, 0x4, 0x0, 0x7, 0x0,
];

#[test]
fn the_meter_steps_are_the_originals_hot_areas() {
    // The five steps and the pause mark, from the top of the meter down.
    assert_eq!(click(50, 160), Some(Click::Acceleration(4)));
    assert_eq!(click(50, 170), Some(Click::Acceleration(3)));
    assert_eq!(click(50, 178), Some(Click::Acceleration(2)));
    assert_eq!(click(50, 185), Some(Click::Acceleration(1)));
    assert_eq!(click(46, 191), Some(Click::Acceleration(0)));
    assert_eq!(click(46, 197), Some(Click::Pause));
    // Edges are inclusive and the later area wins where they overlap: the
    // shared row y = 194 belongs to step 0 (listed after the pause mark).
    assert_eq!(click(46, 194), Some(Click::Acceleration(0)));
    assert_eq!(click(46, 200), Some(Click::Pause));
    assert_eq!(click(46, 201), None);
}

#[test]
fn screen_buttons_and_overlaps_follow_the_list_order() {
    assert_eq!(click(70, 180), Some(Click::CycleView));
    // F1 and F2 overlap at x 74-79: F2 is later in the list and wins.
    assert_eq!(click(76, 180), Some(Click::Screen(2)));
    assert_eq!(click(100, 175), Some(Click::Screen(3)));
    assert_eq!(click(118, 170), Some(Click::Screen(4)));
    assert_eq!(click(135, 170), Some(Click::Screen(5)));
    assert_eq!(click(150, 170), Some(Click::Front));
    assert_eq!(click(270, 185), Some(Click::Navigation));
    assert_eq!(click(1, 1), None);
    // The wrapper the views use still answers for the view buttons only.
    assert!(Screen::view_button(150, 170).is_some());
    assert!(Screen::view_button(76, 180).is_none());
    // F2 is ported now: the status/equipment page (`F1120`).
    assert!(Click::Screen(2).unported().is_none());
    assert!(Click::Front.unported().is_none());
    // The map button, the navigation computer and the H icon drive the ffe-rs maps.
    for click in [Click::Screen(3), Click::Navigation, Click::Hyperspace] {
        assert!(click.unported().is_none());
    }
    // The list rows of the map: 11 rows from y 22, the selected one kept in view.
    assert_eq!(ffe_rs::screen::Screen::map_row(0, 20, 20, 22), Some(0));
    assert_eq!(ffe_rs::screen::Screen::map_row(0, 20, 20, 33), Some(1));
    assert_eq!(ffe_rs::screen::Screen::map_row(19, 20, 20, 22), Some(9));
    assert_eq!(ffe_rs::screen::Screen::map_row(0, 3, 20, 80), None);
    assert_eq!(ffe_rs::screen::Screen::map_row(0, 20, 250, 22), None);
}

#[test]
fn keys_six_to_zero_pick_the_acceleration_steps() {
    let steps: Vec<_> = "67890".chars().map(acceleration_for_digit).collect();
    assert_eq!(steps, [Some(0), Some(1), Some(2), Some(3), Some(4)]);
    assert_eq!(acceleration_for_digit('1'), None);
    assert_eq!(TIME_ACCELERATION[0], 994);
    assert_eq!(ACCELERATION_NAMES[4], "10000X");
    // The lit step is the acceleration's, or the pause mark while paused.
    let mut console = ConsoleState::default();
    assert_eq!(console.lit_step(), 1);
    console.acceleration = 3;
    assert_eq!(console.lit_step(), 4);
    console.paused = true;
    assert_eq!(console.lit_step(), 0);
    assert_eq!(BAR.len(), 6);
}

#[test]
fn acceleration_scales_the_world_but_not_the_controls() {
    let stats = ShipStats::from_block(&EAGLE).unwrap();
    let run = |step: usize| {
        let mut state = Flight::free_flight();
        let mut model =
            OriginalFlight::from_flight(&state, Some(&stats)).with_time_acceleration(step);
        let faster = Keys {
            faster: true,
            ..Keys::default()
        };
        for frame in 0..100 {
            // Hold Enter for the first half second, then coast.
            let keys = if frame < 25 { faster } else { Keys::default() };
            model.step(0.02, &keys).unwrap();
            model.apply(&mut state);
        }
        (model.set_speed_metres(), state.position[2] + 1500.0)
    };
    let (set_1x, moved_1x) = run(0);
    let (set_10x, moved_10x) = run(1);
    // The set speed comes from key time, which is real time at every step.
    assert!((set_1x - set_10x).abs() < 1e-9, "{set_1x} {set_10x}");
    // The same two seconds cover much more ground at 10x.
    assert!(
        moved_1x > 0.0 && moved_10x > 5.0 * moved_1x,
        "{moved_1x} {moved_10x}"
    );
}

#[test]
fn the_retired_port_buttons_no_longer_intercept_clicks() {
    use ffe_rs::screen::{AUTOPILOT_BUTTONS, MAP_BUTTONS};
    // The port-only map and autopilot buttons are not drawn with the original
    // HUD, so their old row no longer answers (the original console icons do).
    for (x, y) in [(10, 130), (100, 130), (160, 130), (230, 133), (290, 130)] {
        assert_eq!(Screen::console_click(x, y), None, "{x},{y} is port-only");
    }
    // The constants stay for the integrator's layout audit.
    assert!(AUTOPILOT_BUTTONS[0].0 > MAP_BUTTONS[1].1);
    // The original target hot area (24,158-41,166) still works.
    assert_eq!(Screen::console_click(30, 160), Some(Click::Target));
    assert_eq!(Click::Target.unported(), None);
}

#[test]
fn the_right_hand_buttons_are_the_originals_table_entries() {
    // DATA_005100 records 0x0f-0x11: the byte at 296,178 is hyperspace's idle
    // image 0x99 and lit 0xa8; 274,178 is labels' 0x9a/0xa9; 261,177 is the
    // navigation computer's 0x9b/0xaa.
    assert_eq!(RIGHT_BUTTONS[0], (296, 178, 0x99, 0xa8, Click::Hyperspace));
    assert_eq!(RIGHT_BUTTONS[1], (274, 178, 0x9a, 0xa9, Click::Labels));
    assert_eq!(RIGHT_BUTTONS[2], (261, 177, 0x9b, 0xaa, Click::Navigation));
    // Their DATA_007684 hot areas are 15 wide: 265-280, 280-295 and 300-315.
    // The labels and navigation areas share x = 280, where the later area wins.
    assert_eq!(click(265, 177), Some(Click::Navigation));
    assert_eq!(click(265, 196), Some(Click::Navigation));
    assert_eq!(click(280, 185), Some(Click::Labels));
    assert_eq!(click(280, 196), Some(Click::Labels));
    assert_eq!(click(295, 197), Some(Click::Labels));
    assert_eq!(click(300, 178), Some(Click::Hyperspace));
    assert_eq!(click(315, 197), Some(Click::Hyperspace));
    assert_eq!(click(264, 185), None);
    assert_eq!(click(299, 185), None);
    assert_eq!(click(316, 185), None);
    assert_eq!(click(280, 198), None);
    // Both are ported, and the labels flag is the original's off state by default.
    assert_eq!(Click::Labels.unported(), None);
    assert_eq!(Click::Navigation.unported(), None);
    let console = ConsoleState::default();
    assert!(!console.labels && !console.navigation);
}

#[test]
fn the_navigation_list_rows_are_the_originals_marker_hot_areas() {
    use ffe_rs::screen::Screen;
    // F1099: markers at (5, 5+15r) and (230, 5+15r), 10 pixels wide and high.
    assert_eq!(Screen::place_row(5, 5), Some(0));
    assert_eq!(Screen::place_row(15, 15), Some(0));
    assert_eq!(Screen::place_row(5, 20), Some(1));
    assert_eq!(Screen::place_row(230, 5), Some(9));
    assert_eq!(Screen::place_row(240, 20), Some(10));
    assert_eq!(Screen::place_row(5, 125), Some(8));
    assert_eq!(Screen::place_row(230, 140), None);
    // The name to the right is not a button, unlike the ffe-rs system map.
    assert_eq!(Screen::place_row(16, 5), None);
    assert_eq!(Screen::place_row(241, 5), None);
    assert_eq!(Screen::place_row(100, 5), None);
    assert_eq!(Screen::place_row(4, 5), None);
    assert_eq!(Screen::place_row(5, 4), None);
}
