//! Save/load integration: a complete commander (flight, docked, landed, combat,
//! legal, rank, missions) survives the native format and the navigator rebuilds
//! from its system code and clock.
//!
//! The tests need the private content cache; set `FFE_WORLD_CACHE` and run with
//! `--ignored`, as the other cache-dependent suites do.

use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::Navigator;
use ffe_rs::saves::{self, Commander, Mission, SaveGame};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn commander() -> Commander {
    Commander {
        name: "Jameson".to_owned(),
        ranks: [4, 7],
        missions: vec![
            Mission {
                id: 1,
                system: Some(0x1234_5678),
                completed: false,
            },
            Mission {
                id: 2,
                system: None,
                completed: true,
            },
        ],
    }
}

fn flight() -> Flight {
    let mut hold = vec![0u16; ffe_core::market::NAMES];
    hold[0] = 5;
    hold[ffe_core::hyperspace::HYDROGEN] = 4;
    Flight {
        mode: Mode::Flight,
        system_flight: true,
        position: [1000.0, -2000.0, 3000.0],
        velocity: [10.0, 0.0, -5.0],
        yaw: 0.75,
        pitch: -0.25,
        credits: 98_765,
        hold,
        combat: ffe_rs::combat::Combat::new_game(),
        loadout: ffe_core::equipment::Loadout::new_game(),
        shields: [0x40, 0x40],
        hull: [4608, 4608],
        axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ..Flight::default()
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_complete_flight_commander_round_trips_through_the_native_file() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    nav.docked = None;
    nav.takeoff = None;
    nav.landed = None;
    nav.legal.wanted = true;
    nav.legal.alert = true;
    nav.legal.offences = 3;
    nav.legal.fines[2] = 6000;
    nav.clock += 3_600.0;
    nav.altitude = 0x00b7_1b00;
    nav.destination = Some(ffe_rs::navigation::Destination {
        code: nav.system_code(nav.current).unwrap(),
        position: [12, -34, 56],
    });
    let original = nav.save_state();
    let save = SaveGame::new(commander(), flight(), Some(original.clone()));
    let bytes = saves::encode_game(&save).unwrap();
    let loaded = saves::decode_game(&bytes).unwrap();
    assert_eq!(loaded.commander.ranks, [4, 7]);
    assert_eq!(loaded.commander.missions.len(), 2);
    assert_eq!(loaded.state, flight());
    assert!(loaded.navigation.as_ref().unwrap().legal.wanted);
    assert_eq!(loaded.navigation.as_ref().unwrap().legal.fines[2], 6000);
    let resumed = Navigator::from_save(&cache, loaded.navigation.as_ref().unwrap()).unwrap();
    assert_eq!(resumed.save_state(), original);
    assert_eq!(resumed.system_name(), nav.system_name());
    assert_eq!(resumed.clock, nav.clock);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn docked_and_landed_states_keep_their_body_and_stage() {
    let cache = cache();
    // Docked on the new game's pad.
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    assert!(nav.docked.is_some());
    nav.legal.fines[0] = 1234;
    let docked = SaveGame::new(commander(), Flight::default(), Some(nav.save_state()));
    let restored = saves::decode_game(&saves::encode_game(&docked).unwrap()).unwrap();
    let resumed = Navigator::from_save(&cache, restored.navigation.as_ref().unwrap()).unwrap();
    assert_eq!(resumed.docked, nav.docked);
    assert_eq!(resumed.save_state(), nav.save_state());

    // Landed on a planet's surface: the landed state and its body survive.
    let mut nav = Navigator::new(&cache).unwrap();
    let index = nav
        .bodies
        .iter()
        .position(|body| body.kind == ffe_rs::navigation::Kind::Planet)
        .expect("a planet");
    assert!(nav.land(index));
    assert!(nav.landed.is_some());
    let landed = SaveGame::new(
        commander(),
        Flight {
            mode: Mode::Flight,
            system_flight: true,
            ..Flight::default()
        },
        Some(nav.save_state()),
    );
    let restored = saves::decode_game(&saves::encode_game(&landed).unwrap()).unwrap();
    let resumed = Navigator::from_save(&cache, restored.navigation.as_ref().unwrap()).unwrap();
    assert_eq!(resumed.landed, nav.landed);
    assert_eq!(resumed.landed_body(), nav.landed_body());
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_missing_body_or_bad_stage_is_rejected_on_restore() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    nav.docked = None;
    let save = SaveGame::new(commander(), flight(), Some(nav.save_state()));
    let bytes = saves::encode_game(&save).unwrap();
    let mut loaded = saves::decode_game(&bytes).unwrap();
    loaded.navigation.as_mut().unwrap().docked = Some(9_999);
    assert!(Navigator::from_save(&cache, loaded.navigation.as_ref().unwrap()).is_err());
    let mut loaded = saves::decode_game(&bytes).unwrap();
    loaded.navigation.as_mut().unwrap().takeoff = Some(0x99);
    assert!(saves::encode_game(&loaded).is_err());
}

#[test]
fn quick_save_round_trip_rejects_a_corrupt_file_without_touching_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut nav = ffe_rs::navigation::NavigatorSave {
        system_code: 0,
        clock: 10.0,
        start_clock: 0.0,
        world_epoch: None,
        new_game: None,
        page: 0,
        view: ffe_rs::navigation::View::Closed,
        selected: 0,
        target: None,
        locked: None,
        destination: None,
        altitude: i32::MAX,
        docked: None,
        legal: ffe_core::npc::Legal::default(),
        ship_lock: None,
        missile_lock: None,
        missile_view: None,
        sequence: None,
        takeoff: None,
        landed: None,
        final_leg: false,
        spawned: None,
    };
    let save = SaveGame::new(commander(), Flight::default(), Some(nav.clone()));
    let path = saves::save_quick(directory.path(), &save).unwrap();
    assert_eq!(
        saves::load_quick(directory.path()).unwrap().1.commander,
        commander()
    );
    // A corrupt quick save is an error; it is not replaced or silently ignored.
    std::fs::write(&path, b"{ broken").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(saves::load_quick(directory.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // A dead commander is rejected by the state validation.
    nav.legal.wanted = true;
    let dead = Flight {
        death: Some(ffe_rs::death::Death::exploding()),
        ..Flight::default()
    };
    assert!(saves::encode_game(&SaveGame::new(commander(), dead, Some(nav))).is_err());
}
