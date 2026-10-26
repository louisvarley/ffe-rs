//! Enter at the intro, then no input: the commander must remain parked.
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{landing_body, Navigator};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT as UNIT};

#[test]
#[ignore = "Local-only: FFE_WORLD_CACHE"]
fn intro_enter_then_idle_keeps_the_ship_on_its_pad() {
    let intro = ffe_rs::front_end::Intro::new();
    assert!(matches!(
        intro.key(ffe_rs::front_end::BOOT_TICS, '\r'),
        Some(ffe_rs::front_end::IntroAction::Skip { chosen: 0, .. })
    ));
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let start = nav.new_game.unwrap();
    let index = nav.docked.unwrap();
    let port = nav.bodies[index].clone();
    let planet = nav.bodies[port.parent.unwrap()].clone();
    assert_eq!(nav.system_name(), "Gateway");
    assert_eq!((&*planet.name, &*port.name), ("Hope", "Old Blackelk"));
    let info = |model| {
        ffe_core::objects::ModelInfo::read(
            cache.asset("base:model/records").unwrap(),
            cache.asset("base:ship/stats").unwrap(),
            model,
        )
        .unwrap()
    };
    let ship_info = info(start.ship);
    let stats = ffe_rs::ships::ShipStats::from_block(&ship_info.stats.unwrap()).unwrap();
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start.seat.map(|v| v as f64 / UNIT),
        axes: Some(port.orientation),
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&state, Some(&stats));
    model.set_mode(start.player_state);
    model.begin_pad_takeoff(
        &landing_body(&planet, &info(planet.model)),
        planet.position,
        &landing_body(&port, &info(port.model)),
        port.position,
        &ship_info,
    );
    let offset: [i64; 3] = std::array::from_fn(|k| start.seat[k] - port.position[k]);
    for frame in 0..600 {
        nav.advance(&cache, 0.02);
        model.set_pad_bodies(
            nav.bodies[port.parent.unwrap()].position,
            nav.bodies[index].position,
        );
        model.step_landed(0.02, &Keys::default(), start.player_state);
        let actual = model.absolute_position();
        for k in 0..3 {
            let drift = actual[k] - nav.bodies[index].position[k] - offset[k];
            assert!(
                drift.abs() <= 2,
                "idle frame {frame}, axis {k}: drift {drift} mm, velocity {:?}",
                model.player().motion.velocity
            );
        }
        assert_eq!(model.player().motion.velocity, [0; 3]);
        assert!(nav.takeoff.is_none());
    }
    // Launch from services must release this parked state and start a real climb.
    nav.begin_launch().unwrap();
    model.begin_takeoff();
    let library = ffe_rs::navigation::Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    for tick in 0..100 {
        nav.advance(&cache, 0.02);
        model.set_pad_bodies(
            nav.bodies[port.parent.unwrap()].position,
            nav.bodies[index].position,
        );
        model
            .step_pad(0.02, &Keys::default(), Some(&tools))
            .unwrap();
        if tick == 0 {
            assert!(
                model.player().altitude < 200_000,
                "the first ground contact must not end the launch climb: {} mm",
                model.player().altitude
            );
        }
    }
    let current = model.absolute_position();
    let rise: f64 = (0..3)
        .map(|k| {
            (current[k] - nav.bodies[index].position[k] - offset[k]) as f64 * port.orientation[1][k]
        })
        .sum();
    assert!(rise > 10_000.0, "parked-to-launch did not climb: {rise} mm");
}

#[test]
fn parked_input_allows_turrets_without_throttle_or_body_motion() {
    let mut model = OriginalFlight::from_flight(&Flight::free_flight(), None);
    let position = model.absolute_position();
    let orientation = model.player().motion.orientation;
    let keys = Keys {
        faster: true,
        pitch_down: true,
        ..Keys::default()
    };
    for _ in 0..100 {
        model.step_landed(0.02, &keys, 0x30);
    }
    assert_eq!(model.set_speed_units(), 0);
    assert_eq!(model.absolute_position(), position);
    assert_eq!(model.player().motion.orientation, orientation);
    assert_eq!(model.player().motion.velocity, [0; 3]);
    model.set_view(2, [0; 2]);
    model.step_landed(0.02, &keys, 0x30);
    assert!(model.view().1[1] < 0, "F1039 still steers the turret");
    assert_eq!(model.set_speed_units(), 0);
    assert_eq!(model.player().motion.orientation, orientation);
}

#[test]
#[ignore = "Local-only: FFE_WORLD_CACHE"]
fn new_commander_and_restart_share_complete_pad_initialization() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    for start_type in [0, 1] {
        let nav = Navigator::new_game(&cache, start_type).unwrap();
        let start = nav.new_game.unwrap();
        assert_eq!(
            start.bay, 0,
            "F994 reserves +0xd1 while assigning the player"
        );
        let (first, first_model) = OriginalFlight::new_commander(&cache, &nav).unwrap();
        let (restart, mut model) = OriginalFlight::new_commander(&cache, &nav).unwrap();
        assert_eq!(first, restart);
        assert_eq!(first.mode, Mode::Flight);
        assert!(first.system_flight);
        assert_eq!(first.ship, start.ship);
        assert_eq!(first.credits, start.cash as u32);
        assert_eq!(first.combat, ffe_rs::combat::Combat::new_game());
        assert_eq!(first.loadout, ffe_core::equipment::Loadout::new_game());
        assert_eq!(model.player().mode, 0x30);
        let port = &nav.bodies[nav.docked.unwrap()];
        let axes = first.axes.unwrap();
        for (row, values) in axes.iter().enumerate() {
            let sign = if start_type == 0 && row != 1 {
                -1.0
            } else {
                1.0
            };
            for (value, expected) in values.iter().zip(port.orientation[row]) {
                assert!((*value - sign * expected).abs() < 1.0e-8);
            }
        }
        assert_eq!(model.absolute_position(), first_model.absolute_position());
        let position = model.absolute_position();
        for _ in 0..600 {
            model.step_landed(0.02, &Keys::default(), start.player_state);
        }
        assert_eq!(model.absolute_position(), position);
        assert_eq!(model.player().motion.velocity, [0; 3]);
    }
}
