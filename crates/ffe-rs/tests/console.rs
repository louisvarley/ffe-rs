use ffe_rs::console::{
    acceleration_for_digit, click, Click, ConsoleState, ACCELERATION_NAMES, BAR, TIME_ACCELERATION,
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
    assert!(Click::Screen(2).unported().is_some());
    assert!(Click::Front.unported().is_none());
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
