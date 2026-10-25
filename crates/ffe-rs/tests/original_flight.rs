use ffe_rs::flight::{Flight, Mode};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};
use ffe_rs::ships::ShipStats;

/// Model 20's statistics block (0x14, the Falcon attack fighter), as in
/// `views.rs`. The name code at +0xe is 0x4010, not the Eagle's.
const EAGLE: [u8; 48] = [
    0xde, 0x3f, 0xd7, 0xec, 0x1, 0x0, 0x10, 0x0, 0xb, 0x0, 0x2e, 0x0, 0x64, 0x0, 0x10, 0x40, 0x1,
    0x0, 0x2, 0x0, 0xa, 0x80, 0x0, 0x1, 0x4e, 0x7, 0x7f, 0x0, 0x25, 0x5, 0x7f, 0x0, 0xa2, 0x8, 0x0,
    0x0, 0x2c, 0x0, 0x1e, 0x0, 0xc0, 0x0, 0x2a, 0x0, 0x4, 0x0, 0x7, 0x0,
];

fn run(model: &mut OriginalFlight, state: &mut Flight, keys: Keys, seconds: f64) {
    let frames = (seconds * 50.0) as usize;
    for _ in 0..frames {
        model.step(0.02, &keys).unwrap();
        model.apply(state);
    }
}

#[test]
fn raising_the_set_speed_accelerates_along_the_nose_within_the_ship_limits() {
    let stats = ShipStats::from_block(&EAGLE).unwrap();
    let mut state = Flight::free_flight();
    let mut model = OriginalFlight::from_flight(&state, Some(&stats));
    run(&mut model, &mut state, Keys::default(), 1.0);
    // Coasting from rest stays at rest.
    assert_eq!(state.velocity, [0.0; 3]);
    run(
        &mut model,
        &mut state,
        Keys {
            faster: true,
            ..Keys::default()
        },
        0.5,
    );
    assert!(model.set_speed_metres() > 0.0);
    run(&mut model, &mut state, Keys::default(), 1.0);
    // The ship speeds up straight ahead (+z toward the station) and has moved.
    assert!(state.velocity[2] > 1.0, "{:?}", state.velocity);
    assert!(state.velocity[0].abs() < 1e-6 && state.velocity[1].abs() < 1e-6);
    assert!(state.position[2] > -1500.0);
    // Forward acceleration is bounded by the thrust limit (about 16350 units of
    // 0.097 m/s per second, i.e. under 31 flight units/s per second).
    assert!(state.velocity[2] < 31.0 * 2.0, "{:?}", state.velocity);
    assert!(state.axes.is_some());
    assert!(state.thrust.abs() <= 1.0);
}

#[test]
fn steering_keys_turn_the_ship_and_keep_the_axes_orthonormal() {
    let stats = ShipStats::from_block(&EAGLE).unwrap();
    let mut state = Flight::free_flight();
    let mut model = OriginalFlight::from_flight(&state, Some(&stats));
    run(
        &mut model,
        &mut state,
        Keys {
            pitch_up: true,
            ..Keys::default()
        },
        1.0,
    );
    run(&mut model, &mut state, Keys::default(), 1.0);
    let axes = state.axes.unwrap();
    // The nose has left the horizontal plane.
    assert!(axes[2][1].abs() > 0.05, "{axes:?}");
    assert!((state.pitch - axes[2][1].asin()).abs() < 1e-9);
    // Right, up and forward stay unit length and perpendicular (to a few percent).
    let dot = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    for i in 0..3 {
        assert!((dot(axes[i], axes[i]) - 1.0).abs() < 0.05, "{axes:?}");
        for j in i + 1..3 {
            assert!(dot(axes[i], axes[j]).abs() < 0.05, "{axes:?}");
        }
    }
}

#[test]
fn the_model_starts_from_the_flight_state_and_round_trips_its_position() {
    let mut state = Flight::approach();
    state.velocity = [0.0, 0.0, 5.0];
    let mut model = OriginalFlight::from_flight(&state, None);
    let before = state.position;
    model.step(0.0, &Keys::default()).unwrap();
    model.apply(&mut state);
    for (now, then) in state.position.iter().zip(before) {
        assert!((now - then).abs() < 2.0 / UNITS_PER_FLIGHT_UNIT);
    }
    // A zero-length step keeps the speed (within the velocity unit's rounding).
    assert!((state.velocity[2] - 5.0).abs() < 0.01);
    assert_eq!(state.mode, Mode::Flight);
}
#[test]
fn the_classic_mouse_word_steers_and_a_disabled_mouse_never_does() {
    let stats = ShipStats::from_block(&EAGLE).unwrap();
    let mut state = Flight::free_flight();
    let mut model = OriginalFlight::from_flight(&state, Some(&stats));
    let before = ship_axes_yaw(&state);
    // One pixel of mouse x is one count in the low word; F1038 shifts it by 6.
    run(
        &mut model,
        &mut state,
        Keys {
            mouse: 64,
            mouse_enabled: true,
            ..Keys::default()
        },
        0.5,
    );
    let turned = ship_axes_yaw(&state);
    assert!((turned - before).abs() > 0.01, "{before} -> {turned}");

    // The word is ignored while the button is not held: the previous value is
    // cleared instead of applied, so releasing or switching presets cannot snap
    // the ship round.
    let mut untouched = Flight::free_flight();
    let mut model = OriginalFlight::from_flight(&untouched, Some(&stats));
    let before = ship_axes_yaw(&untouched);
    run(
        &mut model,
        &mut untouched,
        Keys {
            mouse: u32::MAX,
            mouse_enabled: false,
            ..Keys::default()
        },
        0.5,
    );
    assert!((ship_axes_yaw(&untouched) - before).abs() < 1e-9);
}

fn ship_axes_yaw(state: &Flight) -> f64 {
    let forward = ffe_rs::camera::ship_axes(state)[2];
    forward[0].atan2(forward[2])
}
