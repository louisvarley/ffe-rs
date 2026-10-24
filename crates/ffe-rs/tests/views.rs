use ffe_rs::camera::{
    key_amount, Camera, ExternalCamera, ExternalKeys, TurretCamera, ViewMode, ViewState,
    DEFAULT_DISTANCE,
};
use ffe_rs::flight::Flight;
use ffe_rs::screen::{Screen, ViewButton};
use ffe_rs::ships::{ShipList, ShipStats};

fn views(mode: ViewMode, external: ExternalCamera) -> ViewState {
    ViewState {
        mode,
        external,
        ..ViewState::default()
    }
}

/// The Eagle long range fighter's statistics block (model 20, JJFFE `DATA_002592`).
const EAGLE: [u8; 48] = [
    0xde, 0x3f, 0xd7, 0xec, 0x1, 0x0, 0x10, 0x0, 0xb, 0x0, 0x2e, 0x0, 0x64, 0x0, 0x10, 0x40, 0x1,
    0x0, 0x2, 0x0, 0xa, 0x80, 0x0, 0x1, 0x4e, 0x7, 0x7f, 0x0, 0x25, 0x5, 0x7f, 0x0, 0xa2, 0x8, 0x0,
    0x0, 0x2c, 0x0, 0x1e, 0x0, 0xc0, 0x0, 0x2a, 0x0, 0x4, 0x0, 0x7, 0x0,
];

#[test]
fn f1_cycles_front_rear_turret_external() {
    // Two or fewer mounts (signed): no turret.
    assert_eq!(ViewMode::Front.next(2, 0), ViewMode::Rear);
    assert_eq!(ViewMode::Rear.next(2, 0), ViewMode::External);
    assert_eq!(ViewMode::Rear.next(-1, 0), ViewMode::External);
    assert_eq!(ViewMode::External.next(2, 0), ViewMode::Front);
    // More: the top turret while its pitch is negative, else the bottom one.
    assert_eq!(ViewMode::Rear.next(3, -1), ViewMode::TopTurret);
    assert_eq!(ViewMode::Rear.next(3, 0), ViewMode::BottomTurret);
    assert_eq!(ViewMode::TopTurret.next(4, 0), ViewMode::External);
    assert_eq!(ViewMode::BottomTurret.next(4, 0), ViewMode::External);
    // F8.
    assert_eq!(ViewMode::turret(2, -1), None);
    assert_eq!(ViewMode::turret(4, 100), Some(ViewMode::BottomTurret));
    assert_eq!(ViewMode::External.name(), "External View");
}

#[test]
fn turret_aiming_follows_the_original_clamps() {
    let mut turret = TurretCamera::default();
    // Three mounts: a non-negative pitch is held at -1, then the top view follows.
    assert_eq!(
        turret.step(ViewMode::BottomTurret, 0, 0, 3),
        ViewMode::BottomTurret
    );
    assert_eq!(turret.pitch, -1);
    assert_eq!(
        turret.step(ViewMode::BottomTurret, 0, 0, 3),
        ViewMode::TopTurret
    );
    // Yaw subtracts the comma-minus-period amount; pitch stops at -0x3fff.
    assert_eq!(
        turret.step(ViewMode::TopTurret, -0x5000, 100, 3),
        ViewMode::TopTurret
    );
    assert_eq!((turret.yaw, turret.pitch), (-100, -0x3fff));
    // Four mounts: pitching down past 0 moves to the bottom turret, up to 0x3fff.
    assert_eq!(
        turret.step(ViewMode::TopTurret, 0x7fff, 0, 4),
        ViewMode::BottomTurret
    );
    assert_eq!(turret.pitch, 0x3fff);
    // The FUNC_CalcKeyInput amount: 20 ms, freshly pressed.
    assert_eq!(key_amount(20, Some(0)), 31);
    assert_eq!(key_amount(20, None), 0);
}

#[test]
fn ship_stats_decode_the_original_block() {
    let eagle = ShipStats::from_block(&EAGLE).unwrap();
    assert_eq!(
        (eagle.forward_thrust, eagle.reverse_thrust, eagle.gun_mounts),
        (16350, -4905, 1)
    );
    assert_eq!((eagle.mass, eagle.missiles), (16, 2));
    assert_eq!(eagle.eye, [1870, 127]);
    assert_eq!((eagle.top_turret, eagle.bottom_turret), ([192, 42], [4, 7]));
    // Object set-up (0x125c0): z [reverse, forward], x and y +-|reverse|.
    assert_eq!(
        eagle.limits().axes,
        [[4905, -4905], [4905, -4905], [16350, -4905]]
    );
    assert_eq!(ShipStats::from_block(&[0; 48]), None);
    // F600: full throttle gives the forward limit, full reverse the reverse one.
    assert_eq!(eagle.thrust_word(1.0), 16350);
    assert_eq!(eagle.thrust_word(0.5), 8175);
    assert_eq!(eagle.thrust_word(0.0), 0);
    assert_eq!(eagle.thrust_word(-1.0), (-4905i16) as u16);
}

#[test]
fn eyes_sit_at_the_ship_statistics_offsets() {
    let state = Flight {
        position: [10.0, 20.0, 30.0],
        ..Flight::free_flight()
    };
    let mut stats = ShipStats::from_block(&EAGLE).unwrap();
    // 4096 * 32767 / 4096 model units, at 32767 per flight unit: one unit.
    stats.eye = [4096, -4096];
    // The turret eye is 0x3e above the top turret, 0x3e below the bottom one.
    stats.top_turret = [0, 4096 - 0x3e];
    stats.bottom_turret = [4096, 0x3e];
    let at = |mode: ViewMode| {
        Camera::new(
            &state,
            &views(mode, ExternalCamera::default()),
            Some(&stats),
            32767.0,
        )
        .position
    };
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
    assert!(near(at(ViewMode::Front), [10.0, 19.0, 31.0]));
    assert!(near(at(ViewMode::Rear), [10.0, 19.0, 29.0]));
    assert!(near(at(ViewMode::TopTurret), [10.0, 21.0, 30.0]));
    assert!(near(at(ViewMode::BottomTurret), [10.0, 20.0, 31.0]));
    // Turret angles turn the view as the external camera's do.
    let mut turned = views(ViewMode::TopTurret, ExternalCamera::default());
    turned.turret.pitch = -0x4000;
    let camera = Camera::new(&state, &turned, Some(&stats), 32767.0);
    assert!((camera.axes[2][1] - 1.0).abs() < 1e-9, "{:?}", camera.axes);
}

#[test]
fn external_keys_follow_the_original_ramp_resets_and_limits() {
    // 20 ms is 20 * 0x3e2 / 20 = 994 units; a fresh key moves (994 * 2) >> 6 = 31.
    let mut camera = ExternalCamera::default();
    assert_eq!(
        (camera.yaw, camera.pitch, camera.distance),
        (0, 0x200, DEFAULT_DISTANCE)
    );
    camera.step(
        ExternalKeys {
            left: Some(0),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.yaw, 31);
    // Held 70 ticks or more: (994 * 72) >> 6 = 1118.
    camera.step(
        ExternalKeys {
            right: Some(500),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.yaw, 31u16.wrapping_sub(1118));
    camera.step(
        ExternalKeys {
            down: Some(0),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.pitch, 0x200 - 31);
    // Opposite keys together reset (pitch to 0, not the start value).
    camera.step(
        ExternalKeys {
            left: Some(3),
            right: Some(1),
            up: Some(0),
            down: Some(9),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!((camera.yaw, camera.pitch), (0, 0));
    // Zooming out moves further behind; past the end it clamps to the far limit.
    camera.step(
        ExternalKeys {
            zoom_out: Some(0),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.distance, DEFAULT_DISTANCE - (31 << 10));
    camera.distance = i32::MIN + 5;
    camera.step(
        ExternalKeys {
            zoom_out: Some(0),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.distance, i32::MIN + 1);
    // Zooming in stops at the ship's minimum; both zoom keys restore the start.
    camera.distance = -0x10_0000;
    camera.step(
        ExternalKeys {
            zoom_in: Some(70),
            ..Default::default()
        },
        1000,
        0x0f_0000,
    );
    assert_eq!(camera.distance, -0x0f_0000);
    camera.step(
        ExternalKeys {
            zoom_in: Some(0),
            zoom_out: Some(0),
            ..Default::default()
        },
        20,
        0,
    );
    assert_eq!(camera.distance, DEFAULT_DISTANCE);
}

#[test]
fn cameras_sit_on_the_ship_or_behind_it_along_the_turned_axes() {
    let state = Flight {
        position: [10.0, 20.0, 30.0],
        ..Flight::free_flight()
    };
    let at = |mode, external| Camera::new(&state, &views(mode, external), None, 100.0);
    let front = at(ViewMode::Front, ExternalCamera::default());
    assert_eq!(front.position, state.position);
    assert_eq!(front.axes[2], [0.0, 0.0, 1.0]);
    let rear = at(ViewMode::Rear, ExternalCamera::default());
    assert_eq!(rear.axes[2], [-0.0, -0.0, -1.0]);
    // Yaw and pitch 0: distance / 256 model units behind, here / 100 per flight unit.
    let level = ExternalCamera {
        yaw: 0,
        pitch: 0,
        distance: -256 * 1000,
    };
    let external = at(ViewMode::External, level);
    let expected = [10.0, 20.0, 20.0];
    for (actual, wanted) in external.position.iter().zip(expected) {
        assert!((actual - wanted).abs() < 1e-9);
    }
    // The ship is straight ahead of the external camera.
    assert_eq!(external.project(state.position, true), Some((160, 79)));
    // A quarter-turn yaw moves the camera to the ship's side.
    let side = at(
        ViewMode::External,
        ExternalCamera {
            yaw: 0x4000,
            ..level
        },
    );
    assert!((side.position[2] - 30.0).abs() < 1e-9 && (side.position[0] - 10.0).abs() > 9.0);
    assert_eq!(side.project(state.position, true), Some((160, 79)));
    // "Lock external view to ship" off: the same quarter turn leaves the offset
    // on the ship's own forward axis (`DATA_009020`).
    let mut unlocked = views(
        ViewMode::External,
        ExternalCamera {
            yaw: 0x4000,
            ..level
        },
    );
    unlocked.lock_external = false;
    let unlocked = Camera::new(&state, &unlocked, None, 100.0);
    assert!((unlocked.position[2] - 20.0).abs() < 1e-9);
    assert!((unlocked.position[0] - 10.0).abs() < 1e-9);
    // It still turns; the ship is no longer centred because the offset did not turn.
    assert_ne!(unlocked.axes[2], [0.0, 0.0, 1.0]);
}

#[test]
fn console_hot_areas_select_views() {
    assert_eq!(
        Screen::view_button(150, 170),
        Some(ViewButton::Select(ViewMode::Front))
    );
    assert_eq!(
        Screen::view_button(170, 170),
        Some(ViewButton::Select(ViewMode::Rear))
    );
    assert_eq!(
        Screen::view_button(210, 175),
        Some(ViewButton::Select(ViewMode::External))
    );
    assert_eq!(Screen::view_button(190, 170), Some(ViewButton::Turret));
    assert_eq!(Screen::view_button(70, 180), Some(ViewButton::Cycle));
    assert_eq!(Screen::view_button(10, 10), None);
}

/// Mouse positions arrive in window client pixels; the 640x480 picture is
/// stretched to the client area keeping 4:3, with bars on the long side.
#[test]
fn window_clicks_map_to_logical_pixels_through_scale_resize_and_letterbox() {
    // The initial 1280x960 window (scale 2): the front icon's hot area at (150, 170).
    assert_eq!(
        Screen::logical_point(600.0, 816.0, (1280, 960)),
        Some((150, 170))
    );
    let button =
        |x, y, size| Screen::logical_point(x, y, size).and_then(|(x, y)| Screen::view_button(x, y));
    let front = Some(ViewButton::Select(ViewMode::Front));
    assert_eq!(button(600.0, 816.0, (1280, 960)), front);
    // The old mapping (positions already divided by the scale) hit the view.
    assert_eq!(button(300.0, 408.0, (1280, 960)), None);
    // Resized to 640x480: the same icon at half the client position.
    assert_eq!(
        Screen::logical_point(300.0, 408.0, (640, 480)),
        Some((150, 170))
    );
    assert_eq!(button(300.0, 408.0, (640, 480)), front);
    // A wide 1920x960 window: 320-pixel bars left and right.
    assert_eq!(Screen::logical_point(100.0, 500.0, (1920, 960)), None);
    assert_eq!(button(320.0 + 600.0, 816.0, (1920, 960)), front);
    // A tall 1280x1160 window: 100-pixel bars top and bottom.
    assert_eq!(Screen::logical_point(640.0, 50.0, (1280, 1160)), None);
    assert_eq!(button(600.0, 100.0 + 816.0, (1280, 1160)), front);
    assert_eq!(Screen::logical_point(1280.0, 959.0, (1280, 960)), None);
    assert_eq!(Screen::logical_point(10.0, 10.0, (0, 0)), None);
}

#[test]
fn ship_list_steps_and_wraps_in_table_order() {
    let ships = ShipList::from_ships(vec![
        (20, "Eagle".into()),
        (35, "Cobra".into()),
        (56, "Python".into()),
    ]);
    assert_eq!(ships.step(20, 1), Some(35));
    assert_eq!(ships.step(56, 1), Some(20));
    assert_eq!(ships.step(20, -1), Some(56));
    assert_eq!(ships.step(99, 1), Some(20));
    assert_eq!(ships.name(35), Some("Cobra"));
    assert_eq!(ShipList::default().step(20, 1), None);
}

#[test]
fn saves_keep_the_ship_and_older_saves_load_the_eagle() {
    let state = Flight {
        ship: 35,
        ..Flight::default()
    };
    let bytes = ffe_rs::saves::encode(&state).unwrap();
    assert_eq!(ffe_rs::saves::decode(&bytes).unwrap().ship, 35);
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["state"].as_object_mut().unwrap().remove("ship");
    let old = serde_json::to_vec(&json).unwrap();
    assert_eq!(
        ffe_rs::saves::decode(&old).unwrap().ship,
        ffe_rs::ships::DEFAULT_SHIP
    );
    // A ship outside the original's ship range is rejected.
    json["state"]["ship"] = 300.into();
    assert!(ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap()).is_err());
}
