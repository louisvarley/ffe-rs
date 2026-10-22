use ffe_rs::camera::{Camera, ExternalCamera, ExternalKeys, ViewMode, DEFAULT_DISTANCE};
use ffe_rs::flight::Flight;
use ffe_rs::screen::{Screen, ViewButton};
use ffe_rs::ships::ShipList;

#[test]
fn f1_cycles_front_rear_external_without_turrets() {
    assert_eq!(ViewMode::Front.next(), ViewMode::Rear);
    assert_eq!(ViewMode::Rear.next(), ViewMode::External);
    assert_eq!(ViewMode::External.next(), ViewMode::Front);
    assert_eq!(ViewMode::External.name(), "External View");
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
    let front = Camera::new(&state, ViewMode::Front, ExternalCamera::default(), 100.0);
    assert_eq!(front.position, state.position);
    assert_eq!(front.axes[2], [0.0, 0.0, 1.0]);
    let rear = Camera::new(&state, ViewMode::Rear, ExternalCamera::default(), 100.0);
    assert_eq!(rear.axes[2], [-0.0, -0.0, -1.0]);
    // Yaw and pitch 0: distance / 256 model units behind, here / 100 per flight unit.
    let level = ExternalCamera {
        yaw: 0,
        pitch: 0,
        distance: -256 * 1000,
    };
    let external = Camera::new(&state, ViewMode::External, level, 100.0);
    let expected = [10.0, 20.0, 20.0];
    for (actual, wanted) in external.position.iter().zip(expected) {
        assert!((actual - wanted).abs() < 1e-9);
    }
    // The ship is straight ahead of the external camera.
    assert_eq!(external.project(state.position, true), Some((160, 79)));
    // A quarter-turn yaw moves the camera to the ship's side.
    let side = Camera::new(
        &state,
        ViewMode::External,
        ExternalCamera {
            yaw: 0x4000,
            ..level
        },
        100.0,
    );
    assert!((side.position[2] - 30.0).abs() < 1e-9 && (side.position[0] - 10.0).abs() > 9.0);
    assert_eq!(side.project(state.position, true), Some((160, 79)));
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
    assert_eq!(Screen::view_button(70, 180), Some(ViewButton::Cycle));
    assert_eq!(Screen::view_button(10, 10), None);
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
