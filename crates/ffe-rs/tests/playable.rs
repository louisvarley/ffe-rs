use ffe_rs::flight::{Controls, Flight, Mode, CARGO_CAPACITY};
use ffe_rs::indexed::IndexedImage;
use ffe_rs::{saves, screen::Screen};

#[test]
fn complete_launch_fly_dock_trade_and_persistent_reload() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = Flight::default();
    state.buy().unwrap();
    assert_eq!((state.credits, state.cargo, state.stock), (990, 1, 49));
    state.launch().unwrap();
    for _ in 0..20 {
        state
            .step(Controls {
                thrust: 1.0,
                ..Controls::default()
            })
            .unwrap();
    }
    assert!((state.speed() - 12.0).abs() < 0.000001);
    assert!(state.position[2] > -500.0);
    state.request_docking().unwrap();
    let mut docked = false;
    for _ in 0..2000 {
        let event = state.step(Controls::default()).unwrap();
        if state.mode == Mode::Docked {
            assert!(event.unwrap().starts_with("Docked."));
            docked = true;
            break;
        }
    }
    assert!(docked, "full launched route must reach the station");
    state.sell().unwrap();
    assert_eq!((state.credits, state.cargo, state.stock), (999, 0, 50));
    let path = directory.path().join("snapshot.json");
    saves::save_new(&path, &state).unwrap();
    assert_eq!(saves::load(&path).unwrap(), state);
    assert!(saves::save_new(&path, &Flight::default()).is_err());
    assert_eq!(saves::load(&path).unwrap(), state);
}

#[test]
fn approach_scenario_docks_in_twenty_seconds_without_cheats() {
    let mut state = Flight::approach();
    state.request_docking().unwrap();
    for _ in 0..1010 {
        state.step(Controls::default()).unwrap();
    }
    assert_eq!(state.mode, Mode::Docked);
    state.validate().unwrap();
}

#[test]
fn docking_requires_clearance_low_speed_and_alignment() {
    for (requested, speed, yaw) in [(false, 5.0, 0.0), (true, 30.0, 0.0), (true, 5.0, 1.0)] {
        let mut state = Flight::approach();
        state.position[2] = -79.0;
        state.velocity[2] = speed;
        state.docking_requested = requested;
        state.yaw = yaw;
        state.step(Controls::default()).unwrap();
        assert_eq!(state.mode, Mode::Flight);
    }
}

#[test]
fn strict_docking_thresholds_are_enforced() {
    for (distance, speed, yaw, expected) in [
        (80.0, 0.0, 0.0, Mode::Flight),
        (79.999, 0.0, 0.0, Mode::Docked),
        (79.0, 20.0, 0.0, Mode::Flight),
        (79.0, 19.999, 0.0, Mode::Docked),
        (79.0, 0.0, 0.316, Mode::Docked),
        (79.0, 0.0, 0.319, Mode::Flight),
    ] {
        let mut state = Flight::approach();
        state.position[2] = -distance;
        state.velocity[2] = speed;
        state.yaw = yaw;
        state.docking_requested = true;
        state.step(Controls::default()).unwrap();
        assert_eq!(
            state.mode, expected,
            "range={distance} speed={speed} yaw={yaw}"
        );
    }
}

#[test]
fn collision_has_visible_event_and_safe_state() {
    let mut state = Flight::approach();
    state.position[2] = -45.0;
    let event = state.step(Controls::default()).unwrap().unwrap();
    assert!(event.starts_with("Unsafe approach"));
    assert_eq!(state.position, [0.0, 0.0, -180.0]);
    assert_eq!(state.speed(), 0.0);
    state.validate().unwrap();
}

#[test]
fn steering_braking_and_speed_limit_are_effective() {
    let mut state = Flight::free_flight();
    for _ in 0..500 {
        state
            .step(Controls {
                thrust: 1.0,
                yaw: 1.0,
                pitch: 1.0,
                brake: false,
            })
            .unwrap();
    }
    assert!(state.yaw > 0.0);
    assert_eq!(state.pitch, 1.4);
    let before = state.speed();
    for _ in 0..100 {
        state
            .step(Controls {
                brake: true,
                ..Controls::default()
            })
            .unwrap();
    }
    assert!(state.speed() < before / 100.0);
    let mut state = Flight::free_flight();
    state.yaw = std::f64::consts::PI;
    for _ in 0..1000 {
        state
            .step(Controls {
                thrust: 1.0,
                ..Controls::default()
            })
            .unwrap();
    }
    assert!((state.speed() - 250.0).abs() < 0.000001);
}

#[test]
fn trade_failures_leave_state_unchanged() {
    let full = Flight {
        cargo: CARGO_CAPACITY,
        ..Flight::default()
    };
    let empty_stock = Flight {
        stock: 0,
        ..Flight::default()
    };
    let poor = Flight {
        credits: 9,
        ..Flight::default()
    };
    for mut state in [full, empty_stock, poor, Flight::free_flight()] {
        let before = state.clone();
        assert!(state.buy().is_err());
        assert_eq!(state, before);
    }
    let mut state = Flight::default();
    let before = state.clone();
    assert!(state.sell().is_err());
    assert_eq!(state, before);
    for mut state in [
        Flight {
            cargo: 1,
            stock: 1000,
            ..Flight::default()
        },
        Flight {
            cargo: 1,
            credits: 1_000_000_000,
            ..Flight::default()
        },
    ] {
        let before = state.clone();
        assert!(state.sell().is_err());
        assert_eq!(state, before);
    }
}

#[test]
fn repeated_trades_conserve_stock_and_bound_credits() {
    let mut state = Flight::default();
    for _ in 0..10 {
        state.buy().unwrap();
    }
    assert!(state.buy().is_err());
    for _ in 0..10 {
        state.sell().unwrap();
    }
    assert_eq!((state.credits, state.cargo, state.stock), (990, 0, 50));
    assert!(state.sell().is_err());
}

#[test]
fn invalid_controls_are_atomic_and_docked_clock_stays_still() {
    let mut state = Flight::free_flight();
    let before = state.clone();
    assert!(state
        .step(Controls {
            thrust: f64::NAN,
            ..Controls::default()
        })
        .is_err());
    assert_eq!(state, before);
    assert!(state
        .step(Controls {
            yaw: 1.01,
            ..Controls::default()
        })
        .is_err());
    assert_eq!(state, before);
    let mut docked = Flight::default();
    let before = docked.clone();
    docked
        .step(Controls {
            thrust: 1.0,
            ..Controls::default()
        })
        .unwrap();
    assert_eq!(docked, before);
}

#[test]
fn save_rejects_corruption_version_unknown_fields_and_invalid_state() {
    let original = saves::encode(&Flight::default()).unwrap();
    assert!(saves::decode(&original[..original.len() - 1]).is_err());
    for (from, to) in [
        ("\"version\": 1", "\"version\": 99"),
        ("\"credits\": 1000", "\"credits\": 1000000001"),
        ("\"cargo\": 0", "\"cargo\": 11"),
        ("\"credits\": 1000", "\"credits\": -1"),
        ("\"credits\": 1000", "\"credits\": 1000, \"credits\": 1000"),
        ("\"stock\": 50", "\"stock\": 1001"),
        ("\"ticks\": 0", "\"surprise\": 0"),
        ("ffe-rs-experimental", "other-format"),
    ] {
        let changed = String::from_utf8(original.clone())
            .unwrap()
            .replace(from, to);
        assert_ne!(changed.as_bytes(), original);
        assert!(saves::decode(changed.as_bytes()).is_err(), "{from} -> {to}");
    }
    assert!(saves::decode(&vec![b' '; 16385]).is_err());
    let mut invalid = Flight::default();
    invalid.velocity[0] = 1.0;
    assert!(saves::encode(&invalid).is_err());
    invalid = Flight::free_flight();
    invalid.yaw = f64::INFINITY;
    assert!(saves::encode(&invalid).is_err());
}

#[test]
fn in_flight_save_retains_course_docking_and_economy() {
    let mut state = Flight::approach();
    state.yaw = 0.12345;
    state.request_docking().unwrap();
    state
        .step(Controls {
            thrust: 0.5,
            ..Controls::default()
        })
        .unwrap();
    let restored = saves::decode(&saves::encode(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
}

fn cabin_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 320, 158);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![100; 320 * 158 * 3]).unwrap();
    }
    bytes
}

#[test]
fn renderer_covers_each_screen_without_mutating_simulation() {
    let mut font = vec![0xff; 1160];
    for width in font.iter_mut().skip(9).step_by(10) {
        *width = 5;
    }
    let panel = IndexedImage::new(320, 43, [0, 0], false, &vec![2; 320 * 43]).unwrap();
    let palette = std::array::from_fn(|index| index as u32);
    let mut screen = Screen::from_assets(&cabin_png(), &font, panel, palette).unwrap();
    for state in [Flight::default(), Flight::free_flight(), Flight::approach()] {
        let before = state.clone();
        screen.draw(
            &state,
            ffe_rs::camera::ViewState::default(),
            None,
            false,
            "test",
        );
        let image = screen.pixels.clone();
        assert_eq!(image.len(), 640 * 480);
        assert!(image.contains(&18));
        assert_eq!(screen.logical_pixels()[158 * 320], 2);
        for y in 0..480 {
            for x in 0..640 {
                assert_eq!(
                    image[y * 640 + x],
                    screen.logical_pixels()[(y * 200 / 480) * 320 + x / 2]
                );
            }
        }
        // The retired help page must not paint anything: the same state drawn
        // again is byte-identical apart from the console/clock already there.
        screen.draw(
            &state,
            ffe_rs::camera::ViewState::default(),
            None,
            true,
            "help",
        );
        assert_eq!(
            image, screen.pixels,
            "the removed help overlay must not draw a frame"
        );
        screen.overlay("PAUSED", &["P RESUMES. F2 STATUS PAGE."]);
        let vertices = [1, 0, 10, 20, 30, 0];
        let lab = ffe_rs::model_lab::ModelLab::from_models([
            ffe_rs::model_lab::StationVertices::new(74, 2, 13, &vertices).unwrap(),
            ffe_rs::model_lab::StationVertices::new(75, 2, 14, &vertices).unwrap(),
        ]);
        screen.draw_model_lab(&lab).unwrap();
        assert_ne!(image, screen.pixels);
        let vertices = [
            1,
            0,
            10,
            20,
            30,
            0,
            1,
            0,
            30,
            -20i8 as u8,
            10,
            0,
            1,
            0,
            0,
            30,
            10,
            0,
        ];
        let commands = [7, 0, 0, 0, 0, 2, 4, 3, 0, 0];
        let normals = [0, 0, 127, 0, 0, 0, 0, 0, 0, 127, 0, 0, 0, 0, 0, 0, 127, 0];
        let mut surfaces = ffe_rs::model_lab::ModelLab::from_models([
            ffe_rs::model_lab::StationVertices::new(74, 6, 13, &vertices)
                .unwrap()
                .with_commands(&commands)
                .unwrap()
                .with_normals(&normals, 8)
                .unwrap(),
            ffe_rs::model_lab::StationVertices::new(75, 6, 14, &vertices)
                .unwrap()
                .with_commands(&commands)
                .unwrap()
                .with_normals(&normals, 8)
                .unwrap(),
        ]);
        screen.draw_model_lab(&surfaces).unwrap();
        let wire = screen.pixels.clone();
        surfaces.cycle_view();
        screen.draw_model_lab(&surfaces).unwrap();
        assert_ne!(wire, screen.pixels);
        surfaces.step_face(1).unwrap();
        screen.draw_model_lab(&surfaces).unwrap();
        surfaces.cycle_view();
        screen.draw_model_lab(&surfaces).unwrap();
        assert_ne!(wire, screen.pixels);
        surfaces.cycle_view();
        screen.draw_model_lab(&surfaces).unwrap();
        assert_eq!(surfaces.view(), ffe_rs::model_lab::LabView::Normals);
        assert_ne!(wire, screen.pixels);
        assert_eq!(state, before);
    }
    let panel = IndexedImage::new(320, 43, [0, 0], false, &vec![2; 320 * 43]).unwrap();
    assert!(Screen::from_assets(b"not png", &font, panel, palette).is_err());
}

#[test]
fn flight_view_draws_the_station_model_in_place_of_the_placeholder() {
    use ffe_rs::model_viewer::{ModelViewer, ViewerModel};
    let mut font = vec![0xff; 1160];
    for width in font.iter_mut().skip(9).step_by(10) {
        *width = 5;
    }
    let panel = IndexedImage::new(320, 43, [0, 0], false, &vec![2; 320 * 43]).unwrap();
    let palette = std::array::from_fn(|index| index as u32);
    let mut screen = Screen::from_assets(&cabin_png(), &font, panel, palette).unwrap();
    // Model 74 stand-in: one lit triangle across the origin, facing the camera.
    let vertices = [1, 0, 10, 0, 0, 0, 1, 0, 0, 10, 0, 0];
    let commands: Vec<u8> = [
        3u16,
        0x0123,
        u16::from_le_bytes([1, 0]),
        u16::from_le_bytes([0, 2]),
        0,
    ]
    .iter()
    .flat_map(|w| w.to_le_bytes())
    .collect();
    let model = ViewerModel::new(74, 4, 4, &vertices, &commands, &[], 2);
    let table: Vec<u8> = (0..16384i32).flat_map(|i| (i * 4).to_le_bytes()).collect();
    let table = ffe_core::model_math::SineTable::from_bytes(&table).unwrap();
    let scene = ModelViewer::from_models(vec![model], table).unwrap();
    let state = Flight::free_flight();
    let before = state.clone();
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        None,
        true,
        "placeholder",
    );
    let placeholder = screen.logical_pixels().to_vec();
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        Some(&scene),
        true,
        "placeholder",
    );
    let station = screen.logical_pixels().to_vec();
    // The model path replaces the placeholder wireframe. (With this synthetic
    // sine table the station's rotation collapses the triangle to one point, which
    // the original F1604 fills with nothing, so no model pixel is expected.)
    assert_ne!(placeholder, station);
    // The composed view is RGB: most of it is the configured space colour.
    assert!(
        station[..320 * 158]
            .iter()
            .filter(|p| **p == screen.sky)
            .count()
            > 320 * 158 / 2
    );
    assert_eq!(state, before);
    // A cache without model 74 keeps the placeholder.
    let other = ModelViewer::from_models(
        vec![ViewerModel::new(9, 4, 4, &vertices, &commands, &[], 2)],
        ffe_core::model_math::SineTable::from_bytes(
            &(0..16384i32)
                .flat_map(|i| (i * 4).to_le_bytes())
                .collect::<Vec<u8>>(),
        )
        .unwrap(),
    )
    .unwrap();
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        Some(&other),
        true,
        "placeholder",
    );
    assert_eq!(screen.logical_pixels(), placeholder.as_slice());
}
