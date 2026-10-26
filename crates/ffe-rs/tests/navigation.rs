use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{
    bodies, describe, distance, galaxy, place_indices, prototype_autopilot as autopilot, standoff,
    Body, Kind, Memory, Traffic,
};
use ffe_rs::original_flight::OriginalFlight;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// The systems on offer, their bodies by name, and an autopilot run to the first
/// planet of Sol and to a station.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn systems_and_autopilot() {
    let cache = cache();
    let entries = galaxy(&cache).unwrap();
    assert!(entries.len() >= 12, "{} systems", entries.len());
    for entry in &entries {
        let list = bodies(&cache, entry, [0x1000, 0]).unwrap();
        eprintln!("{}: {} bodies", entry.name, list.len());
        for body in list.iter().take(6) {
            eprintln!(
                "   {:?} {} {}",
                body.kind,
                body.name,
                describe(distance([0; 3], body.position))
            );
        }
        assert!(!list.is_empty(), "{}", entry.name);
    }
    let sol = entries.iter().find(|e| e.name == "Sol").expect("Sol");
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    // A long leg: from the first planet out to the farthest one.
    let far = list
        .iter()
        .filter(|b| b.kind == Kind::Planet)
        .max_by(|a, b| distance([0; 3], a.position).total_cmp(&distance([0; 3], b.position)))
        .expect("a planet");
    {
        let start = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let mut flight = OriginalFlight::from_flight(&start, None);
        flight.teleport([0, 0, -50_000_000_000]);
        let stop = standoff(far);
        let mut elapsed = 0.0_f64;
        let mut memory = Memory::default();
        let mut arrived = false;
        while elapsed < 3_000.0 {
            let command = autopilot(&flight, far.position, stop, &mut memory);
            if let Some((landing, seconds)) = command.skip {
                eprintln!("transfer of {seconds:.0} world seconds");
                flight.teleport(landing);
                continue;
            }
            flight.set_time_acceleration(command.acceleration);
            if let Some(target) = command.face {
                flight.turn_toward(target, 0.05, 1.0);
            }
            flight.step(0.05, &command.keys).unwrap();
            elapsed += 0.05;
            if command.arrived {
                arrived = true;
                break;
            }
        }
        let range = distance(flight.player().motion.position, far.position);
        eprintln!(
            "{}: arrived {arrived} after {elapsed:.0} real s at {}",
            far.name,
            describe(range)
        );
        assert!(
            arrived,
            "{} not reached; range {}",
            far.name,
            describe(range)
        );
    }
    for wanted in [Kind::Planet, Kind::Station] {
        let target = list.iter().find(|b| b.kind == wanted).expect("a body");
        let start = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let mut flight = OriginalFlight::from_flight(&start, None);
        let at = [
            target.position[0] + 3_000_000_000,
            target.position[1],
            target.position[2] - 9_000_000_000,
        ];
        flight.teleport(at);
        let stop = standoff(target);
        eprintln!(
            "{}: start range {} stop {}",
            target.name,
            describe(distance(at, target.position)),
            describe(stop)
        );
        let mut elapsed = 0.0_f64;
        let mut memory = Memory::default();
        let mut arrived = false;
        while elapsed < 20_000.0 {
            let command = autopilot(&flight, target.position, stop, &mut memory);
            if let Some((landing, seconds)) = command.skip {
                eprintln!("transfer of {seconds:.0} world seconds");
                flight.teleport(landing);
                continue;
            }
            flight.set_time_acceleration(command.acceleration);
            if let Some(target) = command.face {
                flight.turn_toward(target, 0.05, 1.0);
            }
            flight.step(0.05, &command.keys).unwrap();
            elapsed += 0.05;
            if command.arrived {
                arrived = true;
                break;
            }
        }
        let range = distance(flight.player().motion.position, target.position);
        eprintln!(
            "{}: arrived {arrived} after {elapsed:.0} s at {}",
            target.name,
            describe(range)
        );
        assert!(
            arrived,
            "{} not reached; range {}",
            target.name,
            describe(range)
        );
    }
}

/// The DOS default new game (F829 record 0): Gateway [-1,4], the planet Hope and
/// the starport Old Blackelk, with F998's default Saker Mk III, 10,000 CR
/// (shown as 1000.0 credits) and a 1 t free hold, and state 0x30. F832's seed/mass for
/// the record are checked against the executed original's `--state` probe
/// (seed `0x8d17f528`, mass 170,000,000, tech 230).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_starts_on_hope_in_gateway() {
    use ffe_rs::navigation::{Navigator, NEW_GAME_CLOCK};
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    assert_eq!(nav.system_name(), "Gateway");
    assert!((nav.clock - NEW_GAME_CLOCK).abs() < 1e-6);
    let state = nav.new_game.expect("a new-game state");
    assert_eq!(state.start_type, 0);
    assert_eq!(state.player_state, 0x30);
    assert_eq!(state.player_index, 87);
    assert_eq!(state.cash, 10_000);
    assert_eq!(state.ship, 0x1f);
    assert_eq!(
        state.cargo, 1,
        "the source delivered loadout leaves one tonne free"
    );
    assert_eq!(state.acceleration, 994);
    let index = nav.docked.expect("landed at the start starport");
    let body = &nav.bodies[index];
    assert_eq!(body.kind, Kind::Starport);
    assert_eq!(body.name, "Old Blackelk");
    assert_eq!(body.slot, 0x6a);
    assert!(body.open_air);
    let parent = body.parent.and_then(|p| nav.bodies.get(p)).unwrap();
    assert_eq!(parent.name, "Hope");
    // The record's body list is F878's, not a hand-coded template.
    assert!(nav.bodies.iter().any(|b| b.name == "Old Curie"));
    assert!(nav.bodies.iter().any(|b| b.name == "Dublin Citadel"));
}

/// Start record 1 (explicitly selected, not the default): Ross 154 at the 3250
/// start, docked at the open-air starport's allocated pad, with F998's cash,
/// ship, hold and acceleration.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn start_record_one_is_ross_154() {
    use ffe_core::pads::Pads;
    use ffe_rs::navigation::{Navigator, NEW_GAME_CLOCK};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    assert_eq!(nav.system_name(), "Ross 154");
    assert!((nav.clock - NEW_GAME_CLOCK).abs() < 1e-6);
    let state = nav.new_game.expect("a new-game state");
    assert_eq!(state.start_type, 1);
    assert_eq!(state.player_state, 0x30);
    assert_eq!(state.player_index, 107);
    assert_eq!(state.cash, 1000);
    assert_eq!(state.ship, 23);
    assert_eq!(
        state.cargo, 8,
        "the hold is the ship's 20 t less the equipment"
    );
    assert_eq!(
        nav.hold(&cache, state.ship),
        8,
        "the market uses the new hold"
    );
    assert_eq!(state.acceleration, 994);
    let index = nav.docked.expect("docked at the start starport");
    let body = &nav.bodies[index];
    assert_eq!(body.kind, Kind::Starport);
    assert_eq!(body.slot, 110, "F994 places the ship at object 0x6e");
    assert!(body.open_air);
    let pads = Pads::from_bytes(cache.asset("base:gameplay/pads").unwrap()).unwrap();
    let offset = pads
        .position(body.model, state.bay, body.rows, true)
        .unwrap();
    // F994: the pad offset, then the ship's collision radius along its up row
    // (the station's second row, copied to the ship first).
    let info = ffe_core::objects::ModelInfo::read(
        cache.asset("base:model/records").unwrap(),
        cache.asset("base:ship/stats").unwrap(),
        state.ship,
    )
    .unwrap();
    let radius = info.radii().1 as i32;
    let up = [body.rows[3], body.rows[4], body.rows[5]];
    assert_eq!(radius, 5_200);
    assert_eq!(up, [-1_938_310_550, -909_597_149, 163_923_668]);
    let expected = pads
        .seat(
            body.position,
            body.model,
            state.bay,
            body.rows,
            true,
            radius,
            up,
        )
        .unwrap();
    assert_eq!(state.seat, expected);
    assert_eq!(
        [
            state.seat[0] - body.position[0] - offset[0],
            state.seat[1] - body.position[1] - offset[1],
            state.seat[2] - body.position[2] - offset[2],
        ],
        [-4_694, -2_203, 396],
        "F1521's radius product, hand-checked"
    );
    // Hand-checked: model 0x55 uses DATA_006177; bay 1 is (-250, 0, 250); the
    // station rows are (Q31, from F1674)
    //   [-906354405, 1945245414, 76650739,
    //    -1938310550, -909597149, 163923668,
    //    180966058, 0, 2139827798],
    // so F1669 gives (126, -227, 240) and the <<10 wraps to this offset.
    assert_eq!(offset, [129_024, -232_448, 245_760]);
    eprintln!(
        "seat {:?} (bay {}, station at {:?}, offset {offset:?})",
        state.seat, state.bay, body.position
    );
    // The calendar counts from 0x121cf7.5 days, not from the absolute clock.
    assert_eq!(nav.date_text(), "1-Jan-3250 12:00:00");
    assert_eq!(nav.day(), 0);
    // F654 grants the launch and the open-air path starts F1013's climb (0x24)
    // with no lift stages: the ship stays on its pad.
    nav.begin_launch().unwrap();
    assert_eq!(nav.takeoff_state(), Some(ffe_core::traffic::state::TAKEOFF));
    assert!(nav.sequence.is_none());
    assert!(nav.cleared(index), "F654 made the player the current ship");
    assert_eq!(nav.takeoff_seat(index, radius), Some(state.seat));
}

/// The DOS default start: F998's type 0, the Saker Mk III on Hope at Old
/// Blackelk in Gateway [-1,4], with 10,000 CR and a 1 t free hold.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_default_is_the_saker_at_hope() {
    use ffe_rs::navigation::{
        Navigator, DEFAULT_CARGO, DEFAULT_CASH, DEFAULT_SHIP, DEFAULT_STARPORT_SLOT,
        DEFAULT_START_TYPE, NEW_GAME_CLOCK,
    };
    let cache = cache();
    let nav = Navigator::new_game(&cache, DEFAULT_START_TYPE).unwrap();
    assert_eq!(nav.system_name(), "Gateway");
    assert!((nav.clock - NEW_GAME_CLOCK).abs() < 1e-6);
    let state = nav.new_game.expect("a new-game state");
    assert_eq!(state.start_type, 0);
    assert_eq!(state.player_state, 0x30);
    assert_eq!(state.player_index, 87);
    assert_eq!(state.cash, DEFAULT_CASH);
    assert_eq!(state.cash, 10_000);
    assert_eq!(state.ship, DEFAULT_SHIP);
    assert_eq!(state.ship, 0x1f, "the Saker Mk III");
    assert_eq!(state.cargo, DEFAULT_CARGO);
    assert_eq!(state.cargo, 1, "23 t less F998's 0x10 allowance (EXP-019)");
    assert_eq!(nav.hold(&cache, state.ship), 1);
    assert_eq!(state.acceleration, 994);
    let index = nav.docked.expect("docked at the start starport");
    let body = &nav.bodies[index];
    assert_eq!(body.kind, Kind::Starport);
    assert_eq!(body.slot, DEFAULT_STARPORT_SLOT);
    assert_eq!(body.slot, 0x6a);
    assert_eq!(body.name, "Old Blackelk");
    assert!(body.open_air);
    let parent = body.parent.expect("the starport is on a planet");
    assert_eq!(nav.bodies[parent].name, "Hope");
    assert_eq!(nav.date_text(), "1-Jan-3250 12:00:00");
    // F1533's Saker statistics and F998's general-equipment dword.
    let info = ffe_core::objects::ModelInfo::read(
        cache.asset("base:model/records").unwrap(),
        cache.asset("base:ship/stats").unwrap(),
        state.ship,
    )
    .unwrap();
    let stats = ffe_rs::ships::ShipStats::from_block(&info.stats.unwrap()).unwrap();
    assert_eq!((stats.mass, stats.cargo, stats.gun_mounts), (28, 23, 2));
    assert_eq!(stats.eye, [1800, 400]);
    assert_eq!(stats.front_gun, [2200, 0]);
    assert_eq!(stats.rear_gun, [1900, -200]);
    assert_eq!((stats.missiles, stats.mass * 4), (2, 112));
}

/// The scripted takeoff: start record 1 begins landed on Ross 154's open-air
/// starport (`0x30`), `L` asks F654 and puts the ship in F1013's climb (`0x24`),
/// F1013 hands over to `0x26` at the altitude/set-speed test, and F1031's
/// contact scan clears the starport's effect region and releases the bay.
fn takes_off_from_the_pad(start_type: u8) {
    use ffe_core::traffic::TakeoffStep;
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    use ffe_rs::original_flight::Keys as OriginalKeys;
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, start_type).unwrap();
    let index = nav.docked.expect("landed at the start starport");
    let state = nav.new_game.unwrap();
    let body = nav.bodies[index].clone();
    // F994 parents the player to the starport's parent planet (`+0x56`); the
    // starport itself is a sibling.
    let planet = nav.bodies[body.parent.expect("the starport orbits a planet")].clone();
    nav.begin_launch().unwrap();
    assert_eq!(nav.takeoff_state(), Some(ffe_core::traffic::state::TAKEOFF));

    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: state.ship,
        ..Flight::default()
    };
    flight.position = state.seat.map(|v| v as f64 / unit);
    flight.axes = Some(body.orientation);
    let mut model = OriginalFlight::from_flight(&flight, None);
    model.begin_takeoff();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, state.ship).unwrap();
    let library = Library(&cache);
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let planet_body = landing_body(
        &planet,
        &ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let station_body = landing_body(
        &body,
        &ffe_core::objects::ModelInfo::read(records, stats, body.model).unwrap(),
    );
    // The persistent three-body pad world, as the playable keeps it.
    model.begin_pad_takeoff(
        &planet_body,
        planet.position,
        &station_body,
        body.position,
        &info,
    );
    // Frames for an eye check, as the other view tests save them.
    let shots = std::env::var_os("FFE_SHOT_DIR");
    let viewer = shots
        .as_ref()
        .and_then(|_| ffe_rs::model_viewer::ModelViewer::new(&cache).ok());
    let prefix = format!("takeoff-{start_type}");
    let save = |name: &str, model: &mut OriginalFlight, nav: &Navigator| {
        if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
            let path = std::path::Path::new(dir).join(format!("{prefix}-{name}"));
            shot(&cache, viewer, nav, model, &path);
        }
    };
    save("0000-pad.ppm", &mut model, &nav);
    let mut seen = Vec::new();
    let mut cleared = false;
    for frame in 0..8_000 {
        if nav.takeoff.is_none() {
            break;
        }
        let at = model.absolute_position();
        let parent_index = nav.landing_parent(at, info.radii().0).unwrap_or(index);
        let parent = nav.bodies[parent_index].clone();
        let parent_info = ffe_core::objects::ModelInfo::read(records, stats, parent.model).unwrap();
        let parent_body = landing_body(&parent, &parent_info);
        // F1013's climb is automatic (the fixed up thrust); once F1013 has
        // handed over, raise the set speed to fly out of the starport region.
        let climbing = nav.takeoff_state() == Some(ffe_core::traffic::state::TAKEOFF);
        let keys = OriginalKeys {
            faster: !climbing && model.set_speed_metres() < 500.0,
            ..OriginalKeys::default()
        };
        let contact = if model.on_pad() {
            model.step_pad(0.02, &keys, tools.as_ref())
        } else {
            model.step_landing(
                0.02,
                &keys,
                &parent_body,
                parent.position,
                &info,
                tools.as_ref(),
            )
        }
        .unwrap_or_else(|error| {
            panic!(
                "frame {frame}: parent {parent_index} {} {:?} model {:#x} class {:#x} \
                 tools {}: {error}",
                parent.name,
                parent.kind,
                parent.model,
                parent_info.class(),
                tools.is_some()
            )
        });
        let before = nav.takeoff_state().unwrap();
        // F1033's altitude on the port's terrain is the terrain's elevation,
        // not the ship's height above it, so measure the climb from the pad
        // seat along its up row (the source's 200,000 mm above the pad), as the
        // playable does.
        let altitude = if model.on_pad() {
            let at = model.absolute_position();
            (0..3)
                .map(|k| (at[k] - state.seat[k]) as f64 * body.orientation[1][k])
                .sum::<f64>()
                .clamp(0.0, f64::from(i32::MAX)) as i32
        } else {
            contact.altitude()
        };
        let step = nav.takeoff_step(altitude, model.set_speed_units(), contact.flags);
        if nav.takeoff_state() != Some(before) {
            seen.push(before);
        }
        match step {
            Some(TakeoffStep::HandOff) => {
                model.set_mode(nav.takeoff_state().unwrap());
                save("0100-climb.ppm", &mut model, &nav);
            }
            Some(TakeoffStep::Cleared) => {
                model.set_mode(ffe_core::traffic::state::FREE);
                model.end_pad_takeoff();
                save("0200-clear.ppm", &mut model, &nav);
                cleared = true;
                break;
            }
            Some(TakeoffStep::Retakeoff) => model.begin_takeoff(),
            None => {}
        }
    }
    let moved = distance(model.absolute_position(), state.seat);
    eprintln!(
        "takeoff start {start_type} modes {seen:?}, cleared {cleared}, moved {}, set speed {:.0} m/s, \
         mode {:#x}, docked {:?}",
        describe(moved),
        model.set_speed_metres(),
        model.player().mode,
        nav.docked
    );
    assert_eq!(
        seen.first(),
        Some(&ffe_core::traffic::state::TAKEOFF),
        "F1013's climb is the first mode"
    );
    assert!(
        seen.contains(&ffe_core::traffic::state::AFTER_TAKEOFF),
        "F1013 hand-off to 0x26: {seen:?}"
    );
    assert!(cleared, "F1031 cleared the starport region: {seen:?}");
    assert_eq!(model.player().mode, 0x00, "free flight");
    assert!(nav.takeoff.is_none());
    assert_eq!(nav.docked, None, "the ship has left the starport");
    assert!(
        moved > 0x30d40 as f64,
        "the ship climbed above F1013's altitude: {}",
        describe(moved)
    );
    // F1032 called F651: the bay is free and the starport awaits another ship.
    let port = nav.starport(index);
    assert_eq!(port.bay_of(ffe_rs::navigation::PLAYER), None);
    assert_eq!(port.mode, 1);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_takes_off_from_the_pad_into_free_flight() {
    takes_off_from_the_pad(1);
}

/// The DOS default start (F829 record 0) on Hope: the same `0x30` -> `0x24` ->
/// `0x26` -> `0x00` path, with F603's gravity from Hope and F1033's altitude
/// from its terrain instead of the old pad-climb fallback.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn default_start_takes_off_from_hope() {
    takes_off_from_the_pad(0);
}

/// Without input, F1013's climb hands over to `0x26` above the pad and Hope's
/// F603 gravity (the planet is the ship's parent, as F994 sets `+0x56`) pulls
/// it straight back down. This is the original's observed cycle: a pad takeoff
/// is flown out with the set speed, not automatic. It pins the planet frame the
/// pad world now builds and the altitude from the pad climb.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn pad_takeoff_hands_over_and_falls_back_under_gravity() {
    use ffe_core::traffic::TakeoffStep;
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("landed at the start starport");
    let state = nav.new_game.unwrap();
    let body = nav.bodies[index].clone();
    let planet = nav.bodies[body.parent.expect("a parent planet")].clone();
    nav.begin_launch().unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: state.ship,
        ..Flight::default()
    };
    flight.position = state.seat.map(|v| v as f64 / unit);
    flight.axes = Some(body.orientation);
    let mut model = OriginalFlight::from_flight(&flight, None);
    model.begin_takeoff();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, state.ship).unwrap();
    let library = Library(&cache);
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let planet_body = landing_body(
        &planet,
        &ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let station_body = landing_body(
        &body,
        &ffe_core::objects::ModelInfo::read(records, stats, body.model).unwrap(),
    );
    model.begin_pad_takeoff(
        &planet_body,
        planet.position,
        &station_body,
        body.position,
        &info,
    );
    let mut seen = Vec::new();
    let mut altitudes = Vec::new();
    let mut pad_flags = 0u8;
    for _ in 0..4_000 {
        if nav.takeoff.is_none() {
            break;
        }
        let contact = model
            .step_pad(0.02, &Default::default(), tools.as_ref())
            .expect("the pad contact");
        // The same pad-climb altitude the playable uses for F1013.
        let at = model.absolute_position();
        let altitude = (0..3)
            .map(|k| (at[k] - state.seat[k]) as f64 * body.orientation[1][k])
            .sum::<f64>()
            .clamp(0.0, f64::from(i32::MAX)) as i32;
        altitudes.push(altitude);
        pad_flags = contact.flags;
        let before = nav.takeoff_state().unwrap();
        let step = nav.takeoff_step(altitude, model.set_speed_units(), contact.flags);
        if nav.takeoff_state() != Some(before) {
            seen.push(before);
        }
        if let Some(TakeoffStep::HandOff) = step {
            model.set_mode(nav.takeoff_state().unwrap());
        }
    }
    eprintln!(
        "pad fallback modes {seen:?}, peak {:?}, last {:?}",
        altitudes.iter().copied().max(),
        altitudes.last()
    );
    // 0x24 -> 0x26 at F1013's altitude test, then F603 pulls the ship back.
    assert_eq!(seen.first(), Some(&ffe_core::traffic::state::TAKEOFF));
    assert_eq!(
        nav.takeoff_state(),
        Some(ffe_core::traffic::state::AFTER_TAKEOFF),
        "{seen:?}"
    );
    let peak = altitudes.iter().copied().max().unwrap_or(0);
    assert!(peak > ffe_core::traffic::TAKEOFF_ALTITUDE, "{peak}");
    assert!(
        *altitudes.last().unwrap() < peak,
        "F603 pulled the ship back down"
    );
    // The starport's effect region is still reported at the pad (F1031 waits
    // for it to clear before it frees the bay).
    assert_ne!(pad_flags & 0x20, 0, "{pad_flags:#x}");
}

/// The scan world is rebuilt every frame, so F570's switch into the planet's
/// rotating frame and the F573 return run every frame instead of once. Each
/// round trip truncated the position to 28 bits and turned it by the planet's
/// integrated rows (a hair under unit length), pulling a ship at rest about
/// 2 m a frame toward the planet's centre: every pad takeoff settled back on
/// the ground. With F603's pull removed, a ship at rest must stay put.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_ship_at_rest_above_the_surface_does_not_sink_through_the_frame_round_trip() {
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the start starport");
    let new_game = nav.new_game.expect("a new game");
    let port = nav.bodies[index].clone();
    let planet = nav.bodies[port.parent.expect("a parent planet")].clone();
    let up = port.orientation[1];
    // 600 m above the seat, out of the starport's own reach.
    let start: [i64; 3] = std::array::from_fn(|k| new_game.seat[k] + (up[k] * 600_000.0) as i64);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: new_game.ship,
        position: start.map(|v| v as f64 / unit),
        axes: Some(port.orientation),
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&flight, None);
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, new_game.ship).unwrap();
    let planet_body = ffe_core::frames::Body {
        mass: 0,
        ..landing_body(
            &planet,
            &ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap(),
        )
    };
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    let height = |at: [i64; 3]| distance(at, planet.position);
    let before = height(model.absolute_position());
    for _ in 0..150 {
        model
            .step_landing(
                0.02,
                &Default::default(),
                &planet_body,
                planet.position,
                &info,
                Some(&tools),
            )
            .expect("the contact scan");
    }
    let sunk = before - height(model.absolute_position());
    // Three seconds: the old round trip sank it about 370 m.
    assert!(sunk.abs() < 1_000.0, "moved {sunk} mm toward the centre");
}

/// F603 after a take-off: with the engines held at zero thrust, the planet's
/// pull on a ship above the start port points at the planet's centre and falls
/// off with the square of the distance.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn gravity_pulls_toward_the_planet_and_falls_off_with_distance() {
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the start starport");
    let new_game = nav.new_game.expect("a new game");
    let port = nav.bodies[index].clone();
    let planet = nav.bodies[port.parent.expect("a parent planet")].clone();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, new_game.ship).unwrap();
    let planet_body = landing_body(
        &planet,
        &ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    let up = port.orientation[1];
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    // Metres per second squared toward the centre, and the cosine between the
    // pull and the direction to the centre, over one second from rest.
    let pull = |height: f64| {
        let start: [i64; 3] = std::array::from_fn(|k| new_game.seat[k] + (up[k] * height) as i64);
        let flight = Flight {
            mode: Mode::Flight,
            system_flight: true,
            ship: new_game.ship,
            position: start.map(|v| v as f64 / unit),
            axes: Some(port.orientation),
            ..Flight::default()
        };
        let mut model = OriginalFlight::from_flight(&flight, None);
        model.set_thrust_override(Some([0; 3]));
        for _ in 0..50 {
            model
                .step_landing(
                    0.02,
                    &Default::default(),
                    &planet_body,
                    planet.position,
                    &info,
                    Some(&tools),
                )
                .expect("the contact scan");
        }
        let velocity = model
            .player()
            .motion
            .velocity
            .map(|v| f64::from(v) * ffe_rs::original_flight::MM_PER_SECOND / 1000.0);
        let centre: [f64; 3] = std::array::from_fn(|k| (planet.position[k] - start[k]) as f64);
        let range = centre.iter().map(|v| v * v).sum::<f64>().sqrt();
        let speed = velocity.iter().map(|v| v * v).sum::<f64>().sqrt();
        let cosine = (0..3).map(|k| velocity[k] * centre[k]).sum::<f64>() / (speed * range);
        (speed, cosine, range)
    };
    let (low, low_cosine, low_range) = pull(600_000.0);
    let (high, high_cosine, high_range) = pull(600_000_000.0);
    eprintln!(
        "{}: {low:.2} m/s2 at {:.0} km, {high:.2} m/s2 at {:.0} km",
        planet.name,
        low_range / 1.0e6,
        high_range / 1.0e6
    );
    assert!(low > 1.0 && low < 50.0, "surface gravity {low} m/s2");
    assert!(
        low_cosine > 0.99 && high_cosine > 0.99,
        "{low_cosine} {high_cosine}"
    );
    let expected = (low_range / high_range).powi(2);
    assert!(
        ((high / low) / expected - 1.0).abs() < 0.05,
        "inverse square: {} vs {expected}",
        high / low
    );
}

/// F1012/F1047's landed handlers call F1014/F1015, which run F1037 (set speed)
/// and F1041 (orientation/thrust) through the contact scan. The port used to
/// copy the mode word and stop, so Enter/Shift/W/S did nothing in the landed
/// view and F1099's Set stayed zero. This pins the source path the playable now
/// calls while landed.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn landed_set_speed_runs_f1037_and_accelerates() {
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    use ffe_rs::original_flight::Keys as OriginalKeys;
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the start starport");
    let new_game = nav.new_game.expect("a new game");
    let body = nav.bodies[index].clone();
    let planet = nav.bodies[body.parent.expect("a parent planet")].clone();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: new_game.ship,
        ..Flight::default()
    };
    flight.position = new_game.seat.map(|v| v as f64 / unit);
    flight.axes = Some(body.orientation);
    let mut model = OriginalFlight::from_flight(&flight, None);
    model.set_mode(ffe_core::traffic::state::LANDED);
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, new_game.ship).unwrap();
    let planet_body = landing_body(
        &planet,
        &ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let library = Library(&cache);
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let keys = OriginalKeys {
        faster: true,
        ..OriginalKeys::default()
    };
    for _ in 0..50 {
        model
            .step_landing(
                0.02,
                &keys,
                &planet_body,
                planet.position,
                &info,
                tools.as_ref(),
            )
            .expect("the landed contact scan");
    }
    assert!(
        model.set_speed_metres() > 0.0,
        "Enter/W must raise F1099's Set while landed"
    );
    assert!(
        model
            .player()
            .motion
            .velocity
            .iter()
            .any(|value| *value != 0),
        "the set speed must produce thrust through F1041/F601"
    );
    // Shift lowers it again through the same F1037 path.
    let slower = OriginalKeys {
        slower: true,
        ..OriginalKeys::default()
    };
    let before = model.set_speed_units();
    for _ in 0..20 {
        model
            .step_landing(
                0.02,
                &slower,
                &planet_body,
                planet.position,
                &info,
                tools.as_ref(),
            )
            .expect("the landed contact scan");
    }
    assert!(model.set_speed_units() < before, "Shift must lower Set");
}

/// F152's clock is not a flight-only counter: the green date must move while the
/// ship is on the pad, landed or in a station. `Navigator::advance` is the one
/// clock step `main` now calls every live frame; time acceleration scales it.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_clock_advances_with_and_without_a_flight_model() {
    use ffe_rs::navigation::Navigator;
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let start = nav.elapsed();
    let date = nav.date_text();
    // One second on the pad is enough for the display's second field.
    nav.advance(&cache, 1.0);
    assert!(
        (nav.elapsed() - (start + 1.0)).abs() < 0.01,
        "the clock must move by the live frame"
    );
    assert_ne!(nav.date_text(), date, "the green date must tick");
    // The 10000x meter step multiplies the same one-second frame.
    let before = nav.elapsed();
    let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[4])
        / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
    nav.advance(&cache, factor);
    // The absolute clock is ~10^13 seconds, so the subtraction of the start
    // clock leaves a few microseconds of f64 rounding at this magnitude.
    assert!(
        (nav.elapsed() - (before + factor)).abs() < 0.01,
        "before {before}, factor {factor}, after {}",
        nav.elapsed()
    );
}

/// F1011's open-air pad calls F1014 at a starport, which runs F1037/F1041. The
/// playable now builds the pad's `OriginalFlight` at start-up and steps it with
/// `step_pad`, so the pad is not the synthetic prototype and Enter/W updates
/// F1099's Set before the launch action.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn pad_set_speed_runs_in_the_pad_world_before_launch() {
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    use ffe_rs::original_flight::Keys as OriginalKeys;
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the start starport");
    let new_game = nav.new_game.expect("a new game");
    let station = nav.bodies[index].clone();
    let planet = nav.bodies[station.parent.expect("a parent planet")].clone();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: new_game.ship,
        ..Flight::default()
    };
    flight.position = new_game.seat.map(|v| v as f64 / unit);
    flight.axes = Some(station.orientation);
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, new_game.ship).unwrap();
    let station_info = ffe_core::objects::ModelInfo::read(records, stats, station.model).unwrap();
    let planet_info = ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap();
    let mut model = OriginalFlight::from_flight(&flight, None);
    model.begin_pad_takeoff(
        &landing_body(&planet, &planet_info),
        planet.position,
        &landing_body(&station, &station_info),
        station.position,
        &info,
    );
    // The pad world follows the navigator's bodies: a planet move must carry
    // the ship with it (F994 parents the player to the planet).
    let before = model.absolute_position();
    let moved_planet = [
        planet.position[0] + 1_000,
        planet.position[1],
        planet.position[2],
    ];
    let moved_station = [
        station.position[0] + 1_000,
        station.position[1],
        station.position[2],
    ];
    model.set_pad_bodies(moved_planet, moved_station);
    let after = model.absolute_position();
    assert_eq!(after[0] - before[0], 1_000, "the pad must follow the body");
    model.set_pad_bodies(planet.position, station.position);
    let library = Library(&cache);
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let keys = OriginalKeys {
        faster: true,
        ..OriginalKeys::default()
    };
    for _ in 0..25 {
        model
            .step_pad(0.02, &keys, tools.as_ref())
            .expect("the pad contact");
    }
    assert!(
        model.set_speed_metres() > 0.0,
        "Enter/W must raise F1099's Set on the pad"
    );
    assert!(
        model
            .player()
            .motion
            .velocity
            .iter()
            .any(|value| *value != 0),
        "the pad's F1041/F601 must apply the set speed"
    );
}

/// The map state the keys drive: open, choose, jump, fly.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn navigator_flow() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    assert_eq!(nav.system_name(), "Sol");
    assert!(nav.station().is_some());
    nav.open(View::Galaxy);
    assert_eq!(nav.view, View::Galaxy);
    let lines = nav.lines([0; 3]);
    assert!(lines.iter().any(|l| l.contains("Sol")), "{lines:?}");
    // Jump to Sol.
    nav.selected = lines.iter().position(|l| l.contains("Sol")).unwrap();
    let arrival = nav.jump(&cache, nav.selected).unwrap();
    assert_eq!(nav.system_name(), "Sol");
    nav.open(View::System);
    let lines = nav.lines(arrival);
    assert!(lines.iter().any(|l| l.contains("Earth")), "{lines:?}");
    nav.selected = lines.iter().position(|l| l.contains("Earth")).unwrap();
    assert_eq!(nav.engage().map(|b| b.name.as_str()), Some("Earth"));
    let mut flight = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            ..Flight::default()
        },
        None,
    );
    flight.teleport(arrival);
    let mut arrived = false;
    for _ in 0..100_000 {
        let (command, _) = nav.prototype_command(&cache, &flight).expect("engaged");
        if let Some((landing, seconds)) = command.skip {
            assert!(seconds > 0.0);
            flight.teleport(landing);
            nav.transfer(&cache, seconds);
            continue;
        }
        if let Some(target) = command.face {
            flight.turn_toward(target, 0.05, 1.0);
        }
        flight.step(0.05, &command.keys).unwrap();
        nav.advance(&cache, 0.05);
        if let Some(moved) = nav.follow(flight.player().motion.position) {
            flight.shift(moved);
        }
        if command.arrived {
            arrived = true;
            break;
        }
    }
    let (last, body) = nav.prototype_command(&cache, &flight).unwrap();
    eprintln!(
        "stop {} set speed {} m/s aligned {} accel {} step {}",
        describe(standoff(&body)),
        flight.set_speed_metres(),
        last.aligned,
        last.acceleration,
        flight.acceleration_step()
    );
    assert!(
        arrived,
        "Earth not reached; range {} clock {:.0}",
        describe(distance(
            flight.player().motion.position,
            nav.bodies[nav.target.unwrap()].position
        )),
        nav.clock
    );
    assert!(nav.clock > 0.0);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn orbit_rates() {
    use ffe_rs::navigation::clock_words;
    let cache = cache();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let mut previous: Option<[i64; 3]> = None;
    for seconds in [
        0.0,
        3600.0,
        86_400.0,
        30.0 * 86_400.0,
        182.6 * 86_400.0,
        365.25 * 86_400.0,
    ] {
        let list = bodies(&cache, sol, clock_words(seconds)).unwrap();
        let earth = list.iter().find(|b| b.name == "Earth").unwrap();
        let moon = list.iter().find(|b| b.name == "Moon").unwrap();
        eprintln!(
            "t={seconds:.0} s Earth {} from Sol, Moon {} from Earth",
            describe(distance([0; 3], earth.position)),
            describe(distance(earth.position, moon.position))
        );
        if let Some(last) = previous {
            eprintln!("   moved {}", describe(distance(last, earth.position)));
        }
        previous = Some(earth.position);
    }
}

/// Earth seen from 40,000 km out fills a disc of the right size, lit toward Sol.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_discs_are_drawn() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, Screen};
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.position = [0.0; 3];
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let radius = 6.371e9 / UNIT; // Earth, in flight units (mm / 9,700)
    let range = 4.0e10 / UNIT;
    screen.discs = vec![Disc {
        centre: [0.0, 0.0, range],
        radius,
        light: [1.0, 0.0, 0.0],
        colours: (100, 101),
        star: false,
        model: None,
        orientation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    }];
    let discs = std::mem::take(&mut screen.discs);
    screen.draw(&state, ViewState::default(), None, false, "");
    let before = screen.logical_pixels().to_vec();
    screen.discs = discs;
    screen.draw(&state, ViewState::default(), None, false, "");
    let changed = screen
        .logical_pixels()
        .iter()
        .zip(&before)
        .filter(|(a, b)| a != b)
        .count();
    // Disc area: pi * (256 * radius / range)^2 pixels.
    let expected = std::f64::consts::PI * (256.0 * radius / range).powi(2);
    eprintln!("expected {expected:.0}, changed {changed}");
    assert!((changed as f64 - expected).abs() < expected * 0.1);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn launch_surroundings() {
    use ffe_rs::navigation::Navigator;
    let cache = cache();
    let nav = Navigator::new(&cache).unwrap();
    let (name, at) = nav.station().unwrap();
    eprintln!("start at {name}");
    let mut near: Vec<_> = nav
        .bodies
        .iter()
        .map(|b| (distance(at, b.position), b.name.clone(), b.radius))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, n, r) in near.iter().take(5) {
        eprintln!("   {n}: {} (radius {})", describe(*d), describe(*r as f64));
    }
    // Launching faces the nearest world.
    let world = nav.nearest_world(at).unwrap();
    let mut flight = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            ..Flight::default()
        },
        None,
    );
    flight.teleport(at);
    flight.face(world);
    let command = autopilot(&flight, world, 1.0, &mut Memory::default());
    assert!(command.aligned > 0.999, "aligned {}", command.aligned);
}

/// Jump to every system, then autopilot to each kind of body there.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn autopilot_after_every_jump() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let count = nav.lines([0; 3]).len();
    let _ = count;
    nav.open(View::Galaxy);
    let systems = nav.lines([0; 3]).len();
    nav.view = View::Closed;
    for system in 0..systems {
        let arrival = nav.jump(&cache, system).unwrap();
        let name = nav.system_name().to_owned();
        let kinds: Vec<usize> = {
            let mut picks = Vec::new();
            for wanted in [
                Kind::Planet,
                Kind::Moon,
                Kind::Station,
                Kind::Starport,
                Kind::Star,
            ] {
                if let Some(i) = nav.bodies.iter().position(|b| b.kind == wanted) {
                    picks.push(i);
                }
            }
            picks
        };
        for index in kinds {
            nav.lock(index);
            nav.engage_locked();
            let target_name = nav.bodies[index].name.clone();
            let mut flight = OriginalFlight::from_flight(
                &Flight {
                    mode: Mode::Flight,
                    ..Flight::default()
                },
                None,
            );
            flight.teleport(arrival);
            let mut arrived = false;
            let mut steps = 0;
            let mut last_aligned = 0.0;
            let mut pushes = 0;
            for _ in 0..200_000 {
                let Some((command, _)) = nav.prototype_command(&cache, &flight) else {
                    break;
                };
                steps += 1;
                last_aligned = command.aligned;
                if let Some((landing, seconds)) = command.skip {
                    flight.teleport(landing);
                    nav.transfer(&cache, seconds);
                    continue;
                }
                if let Some(target) = command.face {
                    flight.turn_toward(target, 0.05, 1.0);
                }
                flight.step(0.05, &command.keys).unwrap();
                nav.advance(&cache, 0.05);
                if let Some(moved) = nav.follow(flight.player().motion.position) {
                    flight.shift(moved);
                }
                let velocity = flight.player().motion.velocity;
                let speed = velocity
                    .iter()
                    .map(|v| {
                        (f64::from(*v) * ffe_rs::original_flight::MM_PER_SECOND / 1000.0).powi(2)
                    })
                    .sum::<f64>()
                    .sqrt();
                if let Some((dock, contact)) = nav.contact(flight.player().motion.position, speed) {
                    match contact {
                        ffe_rs::docking::Contact::Docked => {
                            assert!(nav.begin_docking(dock));
                            arrived = true;
                            break;
                        }
                        ffe_rs::docking::Contact::Collision { clear, reason } => {
                            pushes += 1;
                            let port = nav.port(dock);
                            let at = flight.player().motion.position;
                            if let Some(port) = port {
                                let r: [f64; 3] =
                                    std::array::from_fn(|k| (at[k] - port.centre[k]) as f64);
                                let axial: f64 = (0..3).map(|k| r[k] * port.axis[k]).sum();
                                let range = r.iter().map(|v| v * v).sum::<f64>().sqrt();
                                let lateral = (range * range - axial * axial).max(0.0).sqrt();
                                let station = &nav.bodies[dock].name;
                                let corridor = port.corridor();
                                let front = port.front;
                                let final_leg = nav.final_leg;
                                eprintln!(
                                    "{name} -> {target_name}: push {pushes} by {station} ({reason:?}) step {steps} range {range:.0} axial {axial:.0} lateral {lateral:.0} corridor {corridor:.0} front {front:.0} final {final_leg} speed {speed:.0}"
                                );
                            } else {
                                eprintln!(
                                    "{name} -> {target_name}: push {pushes} by {} ({reason:?}) step {steps}",
                                    nav.bodies[dock].name
                                );
                            }
                            flight.teleport(clear);
                        }
                        ffe_rs::docking::Contact::None => {}
                    }
                }
                // An open-air starport's F745 hand-off: the target stays engaged
                // with the cleared pad for the descent (the autoland stream).
                if nav.bodies[index].kind == Kind::Starport && nav.final_leg && nav.cleared(index) {
                    arrived = true;
                    break;
                }
                if command.arrived {
                    arrived = true;
                    break;
                }
            }
            let range = distance(flight.player().motion.position, nav.bodies[index].position);
            eprintln!(
                "{name} -> {target_name} ({:?}): arrived {arrived} in {steps} steps, range {} stop {} set {} m/s aligned {last_aligned:.4} pushes {pushes} clock {:.0}",
                nav.bodies[index].kind,
                describe(range),
                describe(standoff(&nav.bodies[index])),
                flight.set_speed_metres(),
                nav.clock
            );
            assert!(
                arrived,
                "{name} -> {target_name} not reached: {} away after {steps} steps",
                describe(range)
            );
            assert_eq!(pushes, 0, "{name} -> {target_name}: {pushes} hull pushes");
        }
        nav.target = None;
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn trace_station_approach() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let arrival = nav.jump(&cache, 0).unwrap();
    nav.view = View::Closed;
    let index = nav
        .bodies
        .iter()
        .position(|b| b.kind == Kind::Station)
        .unwrap();
    nav.target = Some(index);
    let mut flight = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            ..Flight::default()
        },
        None,
    );
    flight.teleport(arrival);
    for step in 0..60_000 {
        let Some((command, _)) = nav.prototype_command(&cache, &flight) else {
            break;
        };
        if let Some((landing, seconds)) = command.skip {
            flight.teleport(landing);
            nav.transfer(&cache, seconds);
            continue;
        }
        if let Some(target) = command.face {
            flight.turn_toward(target, 0.05, 1.0);
        }
        flight.step(0.05, &command.keys).unwrap();
        nav.advance(&cache, 0.05);
        if let Some(moved) = nav.follow(flight.player().motion.position) {
            flight.shift(moved);
        }
        if step % 1500 == 0 {
            let v = flight.player().motion.velocity;
            let speed = (v.iter().map(|c| f64::from(*c).powi(2)).sum::<f64>()).sqrt()
                * ffe_rs::original_flight::MM_PER_SECOND
                / 1000.0;
            eprintln!(
                "t{step}: range {} speed {speed:.0} m/s set {:.0} aligned {:.5} accel {}",
                describe(distance(
                    flight.player().motion.position,
                    nav.bodies[index].position
                )),
                flight.set_speed_metres(),
                command.aligned,
                flight.acceleration_step()
            );
        }
        if command.arrived {
            break;
        }
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn body_counts() {
    let cache = cache();
    for entry in galaxy(&cache).unwrap() {
        let list = bodies(&cache, &entry, [0x1000, 0]).unwrap();
        let count = |k: Kind| list.iter().filter(|b| b.kind == k).count();
        eprintln!(
            "{}: {} bodies (star {}, planet {}, moon {}, station {}, starport {})",
            entry.name,
            list.len(),
            count(Kind::Star),
            count(Kind::Planet),
            count(Kind::Moon),
            count(Kind::Station),
            count(Kind::Starport)
        );
    }
}

/// F741/F742/F745/F746's starport hand-off and the final pad descent: the
/// autopilot flies to the 6,144,000 mm approach, F655 grants a bay, the
/// descent targets F650's pad seat, and F746/F744/F652's touch reaches the
/// open-air pad's `0x2e` -> F1048 `0x30`. Local-only: needs the content cache.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn starport_autopilot_lands_on_the_pad() {
    use ffe_rs::navigation::{distance, Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the starting starport");
    assert_eq!(nav.bodies[index].kind, Kind::Starport);
    let body = nav.bodies[index].clone();
    let approach = nav.starport_approach_point(index).expect("F741's approach");
    let start = nav.new_game.unwrap();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, start.ship).unwrap();
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&state, None);
    model.set_collision_radius(info.radii().1);
    model.teleport(approach);
    model.face(body.position);
    model.apply(&mut state);
    nav.lock(index);
    nav.engage_locked();
    let shots = std::env::var_os("FFE_SHOT_DIR");
    let viewer = shots
        .as_ref()
        .and_then(|_| ffe_rs::model_viewer::ModelViewer::new(&cache).ok());
    if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
        let path = std::path::Path::new(dir).join("autoland-0000-approach.ppm");
        shot(&cache, viewer, &nav, &mut model, &path);
    }
    let mut notices = Vec::new();
    for frame in 0..200 {
        nav.advance(&cache, 0.05);
        if let Some(moved) = nav.follow(model.absolute_position()) {
            model.shift(moved);
        }
        nav.tick_traffic(2485, 0.05);
        let Some((command, _body)) = nav.prototype_command(&cache, &model) else {
            break;
        };
        if let Some(notice) = nav.notice.take() {
            notices.push(notice);
        }
        if let Some(target) = command.face {
            model.turn_toward(target, 0.05, 1.0);
        }
        if let Some(rows) = command.align {
            model.align_to(rows);
        }
        model.set_thrust_override(command.thrust);
        if nav.final_leg {
            break;
        }
        let _ = model.step(0.05, &command.keys);
        model.apply(&mut state);
        if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
            if frame == 0 {
                let path = std::path::Path::new(dir).join("autoland-0001-approach-flight.ppm");
                shot(&cache, viewer, &nav, &mut model, &path);
            }
        }
    }
    assert!(nav.final_leg, "F745 handed the leg to the pad");
    assert!(
        notices.iter().any(|n| n.contains("clearance granted")),
        "{notices:?}"
    );
    // F746/F744/F652: the pad touch. The port's controller flies the last
    // kilometres; this places the ship on F650's seat to exercise the touch
    // path itself (the descent's lateral capture is the controller's).
    let bay = nav.starport(index).bay_of(PLAYER).expect("F650's bay");
    let pad = nav.starport_pad_point(index, bay).expect("F650's pad");
    let up = nav.bodies[index].orientation[1];
    let seat_now: [i64; 3] =
        std::array::from_fn(|k| pad[k] + (up[k] * info.radii().1 as f64).round() as i64);
    model.teleport(seat_now);
    model.align_to(nav.bodies[index].orientation);
    model.set_thrust_override(None);
    model.apply(&mut state);
    let _ = nav.prototype_command(&cache, &model);
    assert_eq!(
        nav.landed_state(),
        Some(ffe_core::traffic::state::LANDED),
        "F1048's 0x30"
    );
    assert!(
        nav.starport(index).bay_of(PLAYER).is_some(),
        "F650's bay is still the player's"
    );
    let at = model.absolute_position();
    let bay = nav.starport(index).bay_of(PLAYER).expect("F650's bay");
    let pad = nav.starport_pad_point(index, bay).expect("F650's pad");
    let up = nav.bodies[index].orientation[1];
    let seat_now: [i64; 3] =
        std::array::from_fn(|k| pad[k] + (up[k] * info.radii().1 as f64).round() as i64);
    assert!(
        distance(at, seat_now) < 20_000.0,
        "the pad seat is {} mm away",
        distance(at, seat_now)
    );
    if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
        let path = std::path::Path::new(dir).join("autoland-9999-pad.ppm");
        shot(&cache, viewer, &nav, &mut model, &path);
    }
    // F381/F654: the launch action from the pad is accepted.
    nav.begin_surface_launch().expect("F654 grants the launch");
    assert_eq!(nav.takeoff_state(), Some(ffe_core::traffic::state::TAKEOFF));
}

/// The manual pad landing: the same F569 world (the starport a sibling under
/// its planet) and F576/F575's contact tail, without the autopilot. The ship
/// descends on F599's request and the touch reaches `0x2e` -> `0x30`.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn manual_pad_contact_reaches_0x30() {
    use ffe_rs::navigation::{landing_body, Library, Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.expect("the starting starport");
    let body = nav.bodies[index].clone();
    let planet_index = body.parent.expect("the starport's planet");
    let planet = nav.bodies[planet_index].clone();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, 0x1f).unwrap();
    let planet_info = ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap();
    let port_info = ffe_core::objects::ModelInfo::read(records, stats, body.model).unwrap();
    let library = Library(&cache);
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let planet_body = landing_body(&planet, &planet_info);
    let port_body = landing_body(&body, &port_info);
    let bay = nav.starport(index).bay_of(PLAYER).expect("F650's bay");
    let pad = nav.starport_pad_point(index, bay).expect("F650's pad");
    let up = body.orientation[1];
    let seat: [i64; 3] =
        std::array::from_fn(|k| pad[k] + (up[k] * info.radii().1 as f64).round() as i64);
    let start: [i64; 3] = std::array::from_fn(|k| seat[k] + (up[k] * 15_000.0) as i64);
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: 0x1f,
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&state, None);
    model.set_collision_radius(info.radii().1);
    model.teleport(start);
    model.align_to(body.orientation);
    model.apply(&mut state);
    let shots = std::env::var_os("FFE_SHOT_DIR");
    let viewer = shots
        .as_ref()
        .and_then(|_| ffe_rs::model_viewer::ModelViewer::new(&cache).ok());
    if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
        let path = std::path::Path::new(dir).join("manual-0000-above.ppm");
        shot(&cache, viewer, &nav, &mut model, &path);
    }
    // F576/F580/F575 at the pad seat: the three-body world (the player and the
    // starport siblings under the planet) that F994 gives the pad. The ship is
    // the starport's orientation and its collision radius above the pad surface.
    let mut world = ffe_core::frames::Bodies {
        stamp: 1,
        frame_tics: 1,
        player: 1,
        ..Default::default()
    };
    for k in 1..=3 {
        world.live[k] = true;
        world.physical[k] = true;
    }
    let (outer, inner) = info.radii();
    let mut ship = port_body;
    ship.position = std::array::from_fn(|k| start[k] - planet.position[k]);
    ship.parent = 2;
    ship.index = 1;
    ship.outer_radius = outer;
    ship.inner_radius = inner;
    ship.model = 0x1f;
    ship.shape = info.class();
    ship.words.0[1] = 0xffff;
    world.body[1] = ship;
    let mut parent = planet_body;
    parent.parent = 0;
    parent.index = 2;
    world.body[2] = parent;
    let mut port = port_body;
    port.position = std::array::from_fn(|k| body.position[k] - planet.position[k]);
    port.parent = 2;
    port.index = 3;
    world.body[3] = port;
    let contact = world
        .collide_with(1, tools.as_ref(), 0xff)
        .expect("the F569/F576 scan");
    assert!(
        contact.flags & 0x80 == 0 || contact.flags & 0x40 != 0,
        "{contact:?}"
    );
    // The touch's state path (F1019/F1048) follows the scan.
    assert!(nav.land_at_starport(index), "F1019's 0x2e");
    assert_eq!(
        nav.landed_state(),
        Some(ffe_core::traffic::state::STARPORT_LANDING)
    );
    assert_eq!(
        nav.landing_frame(),
        Some(ffe_core::traffic::state::LANDED),
        "F1048's 0x30"
    );
    model.settle_starport_landing();
    model.set_mode(ffe_core::traffic::state::LANDED);
    if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
        let path = std::path::Path::new(dir).join("manual-9999-pad.ppm");
        shot(&cache, viewer, &nav, &mut model, &path);
    }
}

/// Every starport model's F576/F580 geometry imports: model 0x59's command
/// program does not decode (its last record spills into the next block), but
/// its vertex and normal records do and the collision scan needs only those.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn starport_model_0x59_collision_geometry_is_imported() {
    use ffe_core::surface::Shapes;
    use ffe_rs::navigation::Library;
    let cache = cache();
    let library = Library(&cache);
    let port = library.shape(0x59).expect("model 0x59's geometry");
    assert!(
        !port.mesh.vertices.is_empty(),
        "0x59 has its vertex records"
    );
    assert!(!port.mesh.normals.is_empty(), "0x59 has its normal records");
    let port_info = library.model(0x59).expect("0x59's model record");
    let ship_info = library.model(0x1f).expect("the Saker's record");
    let tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let mut world = ffe_core::frames::Bodies {
        stamp: 1,
        frame_tics: 1,
        player: 1,
        ..Default::default()
    };
    for k in 1..=3 {
        world.live[k] = true;
        world.physical[k] = true;
    }
    let (outer, inner) = ship_info.radii();
    let mut ship = ffe_core::frames::Body {
        position: [0, 0, 0],
        parent: 2,
        index: 1,
        model: 0x1f,
        shape: ship_info.class(),
        outer_radius: outer,
        inner_radius: inner,
        orientation: [0x7fff_ffff, 0, 0, 0, 0x7fff_ffff, 0, 0, 0, 0x7fff_ffff],
        ..Default::default()
    };
    ship.words.0[1] = 0xffff;
    world.body[1] = ship;
    world.body[2] = ffe_core::frames::Body {
        index: 2,
        ..Default::default()
    };
    world.body[3] = ffe_core::frames::Body {
        index: 3,
        parent: 2,
        model: 0x59,
        shape: port_info.class(),
        outer_radius: port_info.radii().0,
        inner_radius: port_info.radii().1,
        orientation: [0x7fff_ffff, 0, 0, 0, 0x7fff_ffff, 0, 0, 0, 0x7fff_ffff],
        ..Default::default()
    };
    let contact = world
        .collide_with(1, tools.as_ref(), 0xff)
        .expect("the F576 stream follows 0x59's child model");
    assert!(contact.distance != 0x7fff_ffff, "{contact:?}");
}

/// From the launch point of the starting station, the autopilot flies to stations
/// of different models (a trading post, an orbital city, an orbital station),
/// takes clearance at the approach point and enters the slot.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn autopilot_docks_at_stations() {
    use ffe_rs::docking::Contact;
    use ffe_rs::model_viewer::ModelViewer;
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    for wanted in ["Daedalus", "Abraham Lincoln", "Galileo"] {
        let mut nav = Navigator::new(&cache).unwrap();
        for model in [69, 72, 73, 74, 75, 76, 77, 78, 79, 80] {
            let found = viewer.find(model);
            nav.set_port_geometry(
                model,
                found.and_then(|m| m.front()),
                found.and_then(|m| m.radius()),
            );
        }
        let (start, at) = nav.station().map(|(n, at)| (n.to_owned(), at)).unwrap();
        let mut flight = OriginalFlight::from_flight(
            &Flight {
                mode: Mode::Flight,
                ..Flight::default()
            },
            None,
        );
        flight.teleport(at);
        nav.open(View::System);
        nav.selected = nav
            .listed()
            .iter()
            .position(|i| nav.bodies[*i].name == wanted)
            .unwrap_or_else(|| {
                nav.turn_page(1);
                nav.listed()
                    .iter()
                    .position(|i| nav.bodies[*i].name == wanted)
                    .expect(wanted)
            });
        let target = nav.engage().unwrap().name.clone();
        assert_eq!(target, wanted);
        let mut docked = None;
        let mut shots: Vec<&str> = Vec::new();
        let mut pushed = 0;
        let mut elapsed = 0.0;
        while elapsed < 2_000.0 {
            let Some((command, _)) = nav.prototype_command(&cache, &flight) else {
                break;
            };
            if let Some((landing, seconds)) = command.skip {
                flight.teleport(landing);
                nav.transfer(&cache, seconds);
                continue;
            }
            if let Some(face) = command.face {
                flight.turn_toward(face, 0.05, 1.0);
            }
            flight.step(0.05, &command.keys).unwrap();
            elapsed += 0.05;
            nav.advance(&cache, 0.05);
            if let Some(moved) = nav.follow(flight.player().motion.position) {
                flight.shift(moved);
            }
            let velocity = flight.player().motion.velocity;
            let speed = velocity
                .iter()
                .map(|v| (f64::from(*v) * ffe_rs::original_flight::MM_PER_SECOND / 1000.0).powi(2))
                .sum::<f64>()
                .sqrt();
            if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
                // Frames on the way in, at about 20 km, 2 km and 0.6 km from the centre.
                let range = distance(
                    flight.player().motion.position,
                    nav.bodies[nav.target.unwrap()].position,
                );
                for (mark, name) in [(20.0e6, "far"), (2.0e6, "near"), (0.6e6, "slot")] {
                    if range < mark && !shots.contains(&name) {
                        shots.push(name);
                        let path = std::path::Path::new(&dir)
                            .join(format!("dock-{}-{name}.ppm", wanted.replace(' ', "_")));
                        shot(&cache, &viewer, &nav, &mut flight, &path);
                    }
                }
            }
            match nav.contact(flight.player().motion.position, speed) {
                Some((index, Contact::Docked)) => {
                    docked = Some((index, speed));
                    assert!(nav.begin_docking(index));
                    break;
                }
                Some((index, Contact::Collision { clear, reason })) => {
                    eprintln!(
                        "{wanted}: pushed clear by {} ({reason:?})",
                        nav.bodies[index].name
                    );
                    flight.teleport(clear);
                    pushed += 1;
                }
                _ => {}
            }
        }
        let (index, speed) = docked.unwrap_or_else(|| {
            panic!(
                "{wanted} not docked from {start}: {} away after {elapsed:.0} s",
                describe(distance(
                    flight.player().motion.position,
                    nav.bodies[nav.target.unwrap()].position
                ))
            )
        });
        eprintln!(
            "{wanted}: docked after {elapsed:.0} s at {speed:.0} m/s ({pushed} pushes, clock {:.0} s)",
            nav.clock
        );
        assert_eq!(nav.bodies[index].name, wanted);
        assert_eq!(pushed, 0);
        assert!(nav.target.is_none() && nav.cleared(index) && nav.holding());
        // The timed stages of the original (about 26 s of game time), then docked.
        let mut frames = 0;
        let mut stages = Vec::new();
        loop {
            frames += 1;
            assert!(frames < 2000, "docking never finished");
            let events = nav.tick_traffic(994, 0.02);
            stages.extend(events.iter().copied());
            if events.contains(&Traffic::Docked(index)) {
                break;
            }
        }
        assert!((1300..1340).contains(&frames), "{frames} frames of docking");
        assert_eq!(stages.len(), 8, "{stages:?}");
        assert_eq!(nav.station().map(|(n, _)| n), Some(wanted));
        assert!(!nav.holding() && nav.sequence.is_none() && !nav.cleared(index));
        // Launch: permission, the stages, the ship put out, the door open, released.
        nav.begin_launch().unwrap();
        assert!(nav.begin_launch().is_err(), "already launching");
        let mut out = false;
        let mut done = false;
        for _ in 0..2000 {
            for event in nav.tick_traffic(994, 0.02) {
                out |= event == Traffic::LaunchOut(index);
                done |= event == Traffic::LaunchDone(index);
            }
            if done {
                break;
            }
        }
        assert!(out && done && nav.sequence.is_none());
        assert!(nav.starport(index).pads.iter().all(|p| *p == 0));
    }
}

/// The autopilot docks at orbital stations of several systems from arbitrary
/// bearings, where the straight line to the approach point can cut the hull and
/// the slot axis turns while the ship closes. Aiming at the centre used to be
/// pushed clear (a spinning station's hull) or stop off the axis; the final leg
/// now follows F745/F746's current slot.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn autopilot_docks_at_stations_from_every_bearing() {
    use ffe_rs::docking::Contact;
    use ffe_rs::model_viewer::ModelViewer;
    use ffe_rs::navigation::Navigator;
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    for system in 0..8 {
        let mut nav = Navigator::new(&cache).unwrap();
        for model in [69, 72, 73, 74, 75, 76, 77, 78, 79, 80] {
            let found = viewer.find(model);
            nav.set_port_geometry(
                model,
                found.and_then(|m| m.front()),
                found.and_then(|m| m.radius()),
            );
        }
        nav.jump(&cache, system).unwrap();
        let name = nav.system_name().to_owned();
        let stations: Vec<usize> = nav
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == Kind::Station)
            .map(|(i, _)| i)
            .collect();
        for index in stations {
            let centre = nav.bodies[index].position;
            for (label, dir) in [
                ("x", [1.0_f64, 0.0, 0.0]),
                ("z", [0.0_f64, 0.0, 1.0]),
                ("diagonal", [0.5_f64, 0.5, 0.707]),
            ] {
                let start: [i64; 3] =
                    std::array::from_fn(|k| centre[k] + (dir[k] * 40.0e6).round() as i64);
                nav.lock(index);
                nav.engage_locked();
                let mut flight = OriginalFlight::from_flight(
                    &Flight {
                        mode: Mode::Flight,
                        ..Flight::default()
                    },
                    None,
                );
                flight.teleport(start);
                let mut docked = false;
                let mut pushes = 0;
                let mut steps = 0;
                while steps < 30_000 {
                    steps += 1;
                    let Some((command, _)) = nav.prototype_command(&cache, &flight) else {
                        break;
                    };
                    if let Some((landing, seconds)) = command.skip {
                        flight.teleport(landing);
                        nav.transfer(&cache, seconds);
                        continue;
                    }
                    if let Some(face) = command.face {
                        flight.turn_toward(face, 0.05, 1.0);
                    }
                    flight.step(0.05, &command.keys).unwrap();
                    nav.advance(&cache, 0.05);
                    if let Some(moved) = nav.follow(flight.player().motion.position) {
                        flight.shift(moved);
                    }
                    let velocity = flight.player().motion.velocity;
                    let speed = velocity
                        .iter()
                        .map(|v| {
                            (f64::from(*v) * ffe_rs::original_flight::MM_PER_SECOND / 1000.0)
                                .powi(2)
                        })
                        .sum::<f64>()
                        .sqrt();
                    match nav.contact(flight.player().motion.position, speed) {
                        Some((dock, Contact::Docked)) => {
                            assert!(nav.begin_docking(dock));
                            docked = true;
                            break;
                        }
                        Some((_, Contact::Collision { clear, .. })) => {
                            pushes += 1;
                            flight.teleport(clear);
                        }
                        _ => {}
                    }
                }
                if docked {
                    eprintln!(
                        "{name} / {} / {label}: docked after {steps} steps ({pushes} pushes, clock {:.0})",
                        nav.bodies[index].name,
                        nav.clock
                    );
                }
                assert!(
                    docked,
                    "{name} / {} / {label}: not docked after {steps} steps ({} away, {pushes} pushes)",
                    nav.bodies[index].name,
                    describe(distance(
                        flight.player().motion.position,
                        nav.bodies[index].position
                    )),
                );
                assert_eq!(
                    pushes, 0,
                    "{name} / {} / {label}: hull pushes",
                    nav.bodies[index].name
                );
            }
        }
    }
}

/// F1018's cancel (Backspace / the autopilot action): the leg is dropped and the
/// set speed falls to 0 so the ship brakes. The old cancel only cleared the
/// target, leaving the ship coasting at the autopilot's speed.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn cancelling_the_autopilot_stops_the_ship_and_clears_the_leg() {
    use ffe_rs::navigation::Navigator;
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let (_, at) = nav.station().map(|(n, at)| (n.to_owned(), at)).unwrap();
    let target = nav
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b.kind, Kind::Planet | Kind::Moon))
        .max_by(|a, b| distance(at, a.1.position).total_cmp(&distance(at, b.1.position)))
        .map(|(i, _)| i)
        .expect("a planet");
    nav.lock(target);
    nav.engage_locked().expect("engaged");
    let mut flight = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            ..Flight::default()
        },
        None,
    );
    // Close enough that the short leg flies in real time (no transfer).
    let reach = standoff(&nav.bodies[target]) + 1.0e9;
    let start = [
        nav.bodies[target].position[0] + reach as i64,
        nav.bodies[target].position[1],
        nav.bodies[target].position[2],
    ];
    flight.teleport(start);
    for _ in 0..40 {
        let (command, _) = nav.prototype_command(&cache, &flight).expect("engaged");
        if let Some(face) = command.face {
            flight.turn_toward(face, 0.05, 1.0);
        }
        flight.step(0.05, &command.keys).unwrap();
    }
    assert!(nav.target.is_some() && flight.set_speed_metres() > 1.0);
    eprintln!(
        "engaged: target {} set {:.0} m/s",
        nav.bodies[target].name,
        flight.set_speed_metres()
    );
    assert!(nav.cancel(), "there was an engaged leg to cancel");
    flight.halt();
    assert_eq!(
        (nav.target, nav.final_leg, nav.docking()),
        (None, false, false)
    );
    assert_eq!(flight.set_speed_metres(), 0.0);
    eprintln!("cancelled: target/leg cleared, set 0 m/s");
    assert!(nav.prototype_command(&cache, &flight).is_none());
    let before = flight
        .player()
        .motion
        .velocity
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    for _ in 0..1_000 {
        flight
            .step(0.05, &ffe_rs::original_flight::Keys::default())
            .unwrap();
    }
    let after = flight
        .player()
        .motion
        .velocity
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    eprintln!(
        "braked from {:.0} to {:.0} velocity units after 50 s",
        before, after
    );
    assert!(after < before * 0.2, "still moving: {after} of {before}");
}

/// The flight view as the playable draws it, with the system's stations.
fn shot(
    cache: &ffe_content::VerifiedCache,
    viewer: &ffe_rs::model_viewer::ModelViewer,
    nav: &ffe_rs::navigation::Navigator,
    flight: &mut OriginalFlight,
    path: &std::path::Path,
) {
    use ffe_rs::original_flight::{TICS_PER_SECOND, UNITS_PER_FLIGHT_UNIT as UNIT};
    let mut screen = ffe_rs::screen::Screen::new(cache).unwrap();
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    flight.apply(&mut state);
    state.system_flight = true;
    let tics = ffe_core::model_animation::START_TICS
        .wrapping_add((nav.clock * TICS_PER_SECOND) as u64 as u32);
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Station | Kind::Starport))
        .map(|b| ffe_rs::screen::SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            door: 0xffff,
            tics,
        })
        .collect();
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        Some(viewer),
        true,
        "",
    );
    let mut ppm = b"P6
320 200
255
"
    .to_vec();
    for p in screen.logical_pixels() {
        ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    std::fs::write(path, ppm).unwrap();
}

/// Traffic control answers the player's request as F655 does: busy with another ship, no
/// bay free, and granted with a bay; and the station remembers the player's clearance.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn traffic_control_answers_requests() {
    use ffe_core::traffic::Reply;
    use ffe_rs::navigation::{Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let (_, at) = nav.station().map(|(n, at)| (n.to_owned(), at)).unwrap();
    let (index, reply) = nav.request_clearance(at).expect("a station within 100 km");
    assert!(
        matches!(reply, Reply::Granted(bay) if bay.fresh),
        "{reply:?}"
    );
    assert!(nav.cleared(index) && nav.cleared_anywhere());
    // Asking again keeps the same bay.
    assert!(matches!(
        nav.request_clearance(at),
        Some((_, Reply::Granted(bay))) if !bay.fresh
    ));
    // Give it up; another ship takes the station; the player is refused.
    assert!(nav.cancel_clearance() && !nav.cleared_anywhere());
    assert!(nav.starport(index).request(PLAYER + 1));
    assert!(matches!(nav.request_clearance(at), Some((_, Reply::Busy))));
    assert!(nav.begin_launch().is_err(), "launch refused while busy");
    // Out of range of every station: nothing to ask.
    assert!(nav.request_clearance([0, 0, 1_000_000_000_000]).is_none());
}

/// The hyperdrive in the navigator: distances are the original's (Sol to Alpha Centauri is
/// 4.35 light years), a jump needs fuel and takes game time, and range limits it.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn hyperspace_uses_range_fuel_and_time() {
    use ffe_rs::navigation::{DriveState, Navigator};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let names: Vec<String> = (0..16)
        .map_while(|i| nav.system_position(i).map(|_| i))
        .map(|i| format!("{i}"))
        .collect();
    assert!(names.len() >= 15, "{} systems have a position", names.len());
    let centauri = (1..15)
        .find(|i| nav.range_to(*i) == Some(435))
        .expect("Alpha Centauri at 4.35 light years");
    assert_eq!(nav.range_to(0), Some(0));
    // The test systems have places of their own, none at Sol.
    for i in 11..15 {
        let d = nav.range_to(i).unwrap();
        assert!(d > 0 && d < 5000, "test system {i} at {d}");
    }
    // The Eagle (mass 16, class 10): 30000 / 16 = 18.75 light years.
    let drive = |fuel| DriveState {
        mass: 16,
        class: 10,
        fuel,
    };
    nav.drive = Some(drive(0));
    let error = nav.hyperspace(&cache, centauri).unwrap_err();
    assert!(error.contains("Insufficient Fuel"), "{error}");
    assert_eq!(nav.system_name(), "Sol");
    nav.drive = Some(drive(3));
    let (distance, report) = nav.jump_report(centauri).unwrap();
    assert_eq!(
        (distance, report.fuel_needed, report.max_range),
        (435, 1, 1875)
    );
    let before = nav.clock;
    let jumped = nav.hyperspace(&cache, centauri).unwrap();
    nav.finish_transit(&cache);
    assert_eq!((jumped.distance, jumped.fuel), (435, 1));
    assert!(
        jumped.days > 0.5 && jumped.days < 2.0,
        "{} days",
        jumped.days
    );
    assert!((nav.clock - before - jumped.days * 86_400.0).abs() < 1e-6);
    assert!(nav.system_name().starts_with("Alpha Centauri"));
    // From Alpha Centauri, Lave is far beyond 18.75 light years.
    let lave = (0..11)
        .find(|i| nav.range_to(*i).is_some_and(|d| d > 4000))
        .unwrap();
    nav.drive = Some(drive(50));
    let error = nav.hyperspace(&cache, lave).unwrap_err();
    assert!(error.contains("Out of Range"), "{error}");
    assert!(nav.hyperspace(&cache, nav.current).is_err());
    // The list shows each system's distance and fuel (or why not).
    nav.open(ffe_rs::navigation::View::Galaxy);
    let lines = nav.lines([0; 3]);
    assert!(
        lines.iter().any(|l| l.contains("LY") && l.contains(" T")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("TOO FAR")), "{lines:?}");
}

/// F908/F909/F1094: a jump builds the destination at once and puts the ship on
/// F1507's orbit (an ffp radius of 0x4000-0x5fff, exponent 0x33: 7.5 to 11 AU)
/// about the root; the clock then runs at the hyperspace rate through exactly
/// the jump time while F1030 holds the ship, and the transit ends on time.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn hyperspace_transit_runs_the_jump_time_and_arrives_out_from_the_star() {
    use ffe_rs::navigation::{DriveState, Navigator, HYPERSPACE_RATE};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let centauri = (1..15)
        .find(|i| nav.range_to(*i) == Some(435))
        .expect("Alpha Centauri at 4.35 light years");
    nav.drive = Some(DriveState {
        mass: 16,
        class: 10,
        fuel: 3,
    });
    let before = nav.clock;
    let jumped = nav.hyperspace(&cache, centauri).unwrap();
    let transit = nav.transit.expect("mode 4 until the arrival time");
    assert!((transit.total - jumped.days * 86_400.0).abs() < 1e-6);
    assert_eq!(
        nav.clock, before,
        "the clock runs through the transit, not at once"
    );
    assert!(
        nav.system_name().starts_with("Alpha Centauri"),
        "F909 builds it at once"
    );
    // F912/F1507: 0x4000..0x5fff << (0x33 - 0x2e) times a unit vector.
    let au = ffe_rs::navigation::AU_MM;
    let range = jumped
        .arrival
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(range > 7.4 * au && range < 11.3 * au, "{} AU", range / au);
    assert_eq!(transit.animation(), 0);
    // A sixtieth of a second at 2^18 is about 73 minutes of game time.
    assert!(!nav.transit_step(&cache, 1.0 / 60.0));
    assert!((nav.clock - before - HYPERSPACE_RATE / 60.0).abs() < 1e-3);
    let half = nav.transit.unwrap();
    assert!(half.animation() > 0 && half.animation() < 145 << 8);
    assert!(
        nav.hyperspace(&cache, 0).is_err(),
        "no second jump in transit"
    );
    let mut frames = 1;
    while !nav.transit_step(&cache, 1.0 / 60.0) {
        frames += 1;
        assert!(frames < 100_000);
    }
    assert!((nav.clock - before - jumped.days * 86_400.0).abs() < 1e-3);
    eprintln!(
        "{:.2} days in {frames} frames ({:.2} s), arrival {:.2} AU out",
        jumped.days,
        f64::from(frames) / 60.0,
        range / au
    );
}

/// F901's fuel burn: hydrogen for a civil drive, military fuel leaving the same
/// tonnes of radioactive waste for class 10 and up.
#[test]
fn f901_burns_the_hold_and_leaves_military_waste() {
    use ffe_core::hyperspace::{HYDROGEN, MILITARY_FUEL, RADIOACTIVES};
    use ffe_rs::navigation::{burn_fuel, DriveState};
    let mut hold = vec![0u16; ffe_core::market::NAMES];
    hold[HYDROGEN] = 5;
    burn_fuel(
        &mut hold,
        Some(DriveState {
            mass: 16,
            class: 2,
            fuel: 5,
        }),
        3,
    );
    assert_eq!((hold[HYDROGEN], hold[MILITARY_FUEL]), (2, 0));
    let mut hold = vec![0u16; ffe_core::market::NAMES];
    hold[MILITARY_FUEL] = 5;
    burn_fuel(
        &mut hold,
        Some(DriveState {
            mass: 16,
            class: 10,
            fuel: 5,
        }),
        2,
    );
    assert_eq!(
        (hold[MILITARY_FUEL], hold[RADIOACTIVES], hold[HYDROGEN]),
        (3, 2, 0)
    );
    // A short hold is grown to the cargo array and cannot underflow.
    let mut hold = vec![0u16; 1];
    burn_fuel(
        &mut hold,
        Some(DriveState {
            mass: 16,
            class: 2,
            fuel: 0,
        }),
        4,
    );
    assert_eq!(hold.len(), ffe_core::market::NAMES);
    assert_eq!(hold[HYDROGEN], 0);
}

/// F832/F908: the star the galaxy map selects becomes the stored hyperspace
/// destination, survives closing the map, is gated by altitude/range/fuel and
/// resets the system's locks and effects on arrival.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_map_destination_survives_the_map_and_jumps() {
    use ffe_core::effects::{Effect, Kind as EffectKind, HIT_MODEL};
    use ffe_rs::galaxy_map::GalaxyMap;
    use ffe_rs::navigation::{DriveState, Navigator, View, HYPERSPACE_ALTITUDE};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let target = (1..15)
        .find(|i| nav.range_to(*i) == Some(435))
        .expect("Alpha Centauri");
    let position = nav.system_position(target).unwrap();
    let code = galaxy(&cache).unwrap()[target].params.code;
    let tables =
        ffe_core::sector::Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let mut map = GalaxyMap::new(nav.current_position().unwrap(), nav.reach().unwrap_or(0));
    map.centre = [position[0], position[1]];
    map.update_selection(&tables);
    let selected = map.selected.clone().expect("the target star");
    assert_eq!(selected.code, code);
    // Choosing a star is a destination, not a body lock.
    assert_eq!((nav.locked, nav.target), (None, None));
    nav.set_destination(selected.code, selected.position);
    assert_eq!((nav.locked, nav.target), (None, None));
    assert_eq!(nav.destination_entry(), Some(target));
    assert_eq!(nav.destination_distance(), Some(435));
    // F828's fuel line: the map shows the cost, the shortfall or the range.
    nav.drive = Some(DriveState {
        mass: 16,
        class: 10,
        fuel: 0,
    });
    let report = nav.destination_report().unwrap();
    assert_eq!(
        map.fuel_text(Some(&report)).as_deref(),
        Some("Insufficient Fuel (1 required)")
    );
    nav.drive = Some(DriveState {
        mass: 16,
        class: 10,
        fuel: 3,
    });
    let report = nav.destination_report().unwrap();
    assert_eq!(
        map.fuel_text(Some(&report)).as_deref(),
        Some("Fuel Required: 1")
    );
    // Closing the map does not clear the destination; H can still jump.
    nav.view = View::Closed;
    assert!(nav.destination.is_some());
    let drive = |fuel| DriveState {
        mass: 16,
        class: 10,
        fuel,
    };
    // F908's altitude floor.
    nav.drive = Some(drive(10));
    nav.altitude = HYPERSPACE_ALTITUDE - 1;
    let error = nav.hyperspace_to_destination(&cache).unwrap_err();
    assert!(error.contains("Too low"), "{error}");
    nav.altitude = HYPERSPACE_ALTITUDE;
    // F903's fuel gate uses the map's own message.
    nav.drive = Some(drive(0));
    let error = nav.hyperspace_to_destination(&cache).unwrap_err();
    assert!(error.contains("Insufficient Fuel"), "{error}");
    assert_eq!(nav.system_name(), "Sol");
    // A destination beyond the drive's range.
    let lave = (0..11)
        .find(|i| nav.range_to(*i).is_some_and(|d| d > 4000))
        .unwrap();
    let position = nav.system_position(lave).unwrap();
    nav.set_destination(galaxy(&cache).unwrap()[lave].params.code, position);
    nav.drive = Some(drive(50));
    let error = nav.hyperspace_to_destination(&cache).unwrap_err();
    assert!(error.contains("Out of Range"), "{error}");
    map.centre = [position[0], position[1]];
    map.dirty = true;
    map.update_selection(&tables);
    assert_eq!(
        map.selected.as_ref().map(|s| s.code),
        Some(galaxy(&cache).unwrap()[lave].params.code)
    );
    let report = nav.destination_report().unwrap();
    assert_eq!(
        map.fuel_text(Some(&report)).as_deref(),
        Some("Out of Range")
    );
    // Back to the reachable star; stale locks, effects and system objects go.
    let position = nav.system_position(target).unwrap();
    nav.set_destination(code, position);
    nav.drive = Some(drive(3));
    nav.altitude = HYPERSPACE_ALTITUDE;
    nav.locked = Some(0);
    nav.ship_lock = Some(99);
    nav.missile_lock = Some(99);
    nav.target = Some(0);
    nav.effects.push(Effect {
        kind: EffectKind::Hit,
        model: HIT_MODEL,
        position: [0; 3],
        velocity: [0.0; 3],
        forward: [0.0, 0.0, 1.0],
        up: [0.0, 1.0, 0.0],
        age: 0.0,
        life: 1.0,
    });
    let before = nav.clock;
    let jumped = nav.hyperspace_to_destination(&cache).unwrap();
    nav.finish_transit(&cache);
    assert_eq!((jumped.distance, jumped.fuel), (435, 1));
    assert_eq!(nav.current, target);
    assert!(nav.system_name().starts_with("Alpha Centauri"));
    assert!(!nav.bodies.is_empty());
    assert_eq!(nav.system_position(nav.current), Some(position));
    assert!((nav.clock - before - jumped.days * 86_400.0).abs() < 1e-6);
    assert_eq!(
        (nav.locked, nav.ship_lock, nav.missile_lock, nav.target),
        (None, None, None, None)
    );
    assert!(nav.effects.is_empty());
    assert!(nav.destination.is_none());
}

/// A click on a body in the view locks it, the target button cycles the bodies by range, and
/// the autopilot button's engage flies to the locked body (docking at a station).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn locking_and_engaging_a_target() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::navigation::Navigator;
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Screen;
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let (_, at) = nav.station().map(|(n, at)| (n.to_owned(), at)).unwrap();
    // The target button starts from the nearest body and walks outward, then wraps.
    let first = nav.cycle_lock(at, 1).map(|b| b.name.clone()).unwrap();
    let second = nav.cycle_lock(at, 1).map(|b| b.name.clone()).unwrap();
    assert_ne!(first, second);
    assert_eq!(nav.cycle_lock(at, -1).map(|b| b.name.clone()), Some(first));
    // Engaging flies to the locked body and starts the approach afresh.
    let locked = nav.locked.unwrap();
    nav.final_leg = true;
    assert_eq!(
        nav.engage_locked().map(|b| b.name.clone()),
        Some(nav.bodies[locked].name.clone())
    );
    assert_eq!(nav.target, Some(locked));
    assert!(!nav.final_leg);
    nav.disengage();
    assert_eq!(nav.target, None);
    // A station dead ahead 40 km away is picked by a click on its screen position and not
    // by a click far from it.
    let station = nav
        .bodies
        .iter()
        .position(|b| b.kind == Kind::Station)
        .unwrap();
    let centre = nav.bodies[station].position;
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        position: [
            centre[0] as f64 / UNIT,
            centre[1] as f64 / UNIT,
            centre[2] as f64 / UNIT - 40_000_000.0 / UNIT,
        ],
        axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ..Flight::default()
    };
    let bodies: Vec<(usize, [f64; 3], f64)> = nav
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| {
            (
                i,
                b.position.map(|v| v as f64 / UNIT),
                b.radius as f64 / UNIT,
            )
        })
        .collect();
    let views = ViewState::default();
    let (x, y, radius) =
        Screen::body_screen(&state, &views, None, bodies[station].1, bodies[station].2)
            .expect("the station is in view");
    assert!(radius >= 10);
    assert_eq!(
        Screen::pick(&state, &views, None, &bodies, (x, y)),
        Some(station)
    );
    assert_eq!(
        Screen::pick(&state, &views, None, &bodies, (x + 100, y + 50)),
        None
    );
    nav.lock(station);
    assert_eq!(nav.locked, Some(station));
}

/// The original navigation computer's entries: only the places (stations and
/// starports), in the system's own order (F880's object order).
#[test]
fn the_place_list_is_stations_and_starports_in_system_order() {
    let body = |name: &str, kind: Kind| Body {
        name: name.to_owned(),
        kind,
        position: [0; 3],
        velocity: [0; 3],
        radius: 1,
        inner_radius: 1,
        parent: None,
        model: 0,
        seed: 0,
        mass: 0,
        slot: 0,
        rows: [0; 9],
        class: 0,
        open_air: false,
        orientation: [[0.0; 3]; 3],
        spin: 0,
        bays: 0,
    };
    let list = [
        body("A", Kind::Star),
        body("B", Kind::Planet),
        body("C", Kind::Starport),
        body("D", Kind::Moon),
        body("E", Kind::Station),
        body("F", Kind::Station),
    ];
    assert_eq!(place_indices(&list), [2, 4, 5]);
    assert!(Kind::Station.is_place() && Kind::Starport.is_place());
    assert!(!Kind::Star.is_place() && !Kind::Planet.is_place() && !Kind::Moon.is_place());
}

/// The console's `n` icon: Sol's list, the original's names and order, a click
/// on a row locking and engaging the autopilot.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_navigation_computer_lists_the_systems_places() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    assert_eq!(nav.system_name(), "Sol");
    nav.open(View::Places);
    assert_eq!(nav.view, View::Places);
    let expected = place_indices(&nav.bodies);
    assert!(!expected.is_empty());
    let rows = nav.listed();
    assert_eq!(rows, expected.iter().copied().take(18).collect::<Vec<_>>());
    assert!(rows.iter().all(|i| nav.bodies[*i].kind.is_place()));
    // The names are the original's: just the object's own name, no distance.
    let names = nav.lines([0; 3]);
    eprintln!("{} places: {names:?}", names.len());
    assert_eq!(
        names,
        rows.iter()
            .map(|i| nav.bodies[*i].name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(nav.pages(), expected.len().div_ceil(18).max(1));
    // F1099's action 0xd1+n: lock the row's object and engage the autopilot.
    nav.selected = 0;
    let index = nav.selected_body().unwrap();
    let name = nav.bodies[index].name.clone();
    assert_eq!(nav.engage().map(|b| b.name.clone()), Some(name));
    assert_eq!(
        (nav.target, nav.locked, nav.view),
        (Some(index), Some(index), View::Closed)
    );
    // The hot areas follow the marker cells: x 10..20 / 230..240, y 5+15r.
    nav.open(View::Places);
    let rows = nav.listed();
    assert_eq!(ffe_rs::screen::Screen::place_row(10, 5), Some(0));
    assert_eq!(ffe_rs::screen::Screen::place_row(20, 20), Some(1));
    assert_eq!(ffe_rs::screen::Screen::place_row(230, 5), Some(9));
    // A click on the second column's first marker is the tenth place and
    // targets that body, exactly as action 0xd9 does.
    let row = ffe_rs::screen::Screen::place_row(230, 5).unwrap();
    assert!(rows.len() > row, "Sol has 18 places");
    nav.selected = row;
    let tenth = nav.selected_body().unwrap();
    assert_eq!(tenth, rows[9]);
    let tenth_name = nav.bodies[tenth].name.clone();
    assert_eq!(nav.engage().map(|b| b.name.clone()), Some(tenth_name));
    assert_eq!((nav.target, nav.locked), (Some(tenth), Some(tenth)));
    // The ffe-rs system map still lists every body, in its own order.
    nav.open(View::System);
    assert!(nav.listed().iter().any(|i| !nav.bodies[*i].kind.is_place()));
}

#[test]
#[ignore = "Local-only: needs FFE_WORLD_CACHE"]
fn pad_takeoff_stays_near_the_port_when_the_system_clock_advances() {
    use ffe_rs::navigation::{landing_body, Navigator};
    use ffe_rs::original_flight::Keys as OriginalKeys;
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.unwrap();
    let start = nav.new_game.unwrap();
    let station = nav.bodies[index].clone();
    let planet = nav.bodies[station.parent.unwrap()].clone();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = |model| ffe_core::objects::ModelInfo::read(records, stats, model).unwrap();
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start
            .seat
            .map(|v| v as f64 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT),
        axes: Some(station.orientation),
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&flight, None);
    model.begin_takeoff();
    model.begin_pad_takeoff(
        &landing_body(&planet, &info(planet.model)),
        planet.position,
        &landing_body(&station, &info(station.model)),
        station.position,
        &info(start.ship),
    );
    let library = ffe_rs::navigation::Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    for frame in 0..150 {
        nav.advance(&cache, 0.02);
        if let Some(at) = nav.follow(model.absolute_position()) {
            model.shift(at);
        }
        model
            .step_pad(0.02, &OriginalKeys::default(), Some(&tools))
            .unwrap();
        model.apply(&mut flight);
        let range = distance(model.absolute_position(), nav.bodies[index].position);
        assert!(
            range < 1_000_000.0,
            "frame {frame}: pad now {range} mm away after orbital refresh"
        );
    }
    let command = ffe_rs::navigation::prototype_autopilot(
        &model,
        nav.bodies[index].position,
        1_000_000.0,
        &mut ffe_rs::navigation::Memory::default(),
    );
    assert!(
        command.keys.slower && command.face.is_none(),
        "autopilot must brake at the nearby port, rather than aim for a distant point"
    );
    assert!(command.skip.is_none());
    let before = model.absolute_position();
    model.end_pad_takeoff();
    assert_eq!(
        model.absolute_position(),
        before,
        "clearing the pad must not teleport the ship"
    );
    model.apply(&mut flight);
    for (axis, value) in before.iter().enumerate() {
        let displayed = flight.position[axis] * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
        assert!(
            (displayed - *value as f64).abs() <= 1.0,
            "render position differs from system position"
        );
    }
}
