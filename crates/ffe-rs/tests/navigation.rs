use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{
    autopilot, bodies, describe, distance, galaxy, place_indices, standoff, Body, Kind, Memory,
    Traffic,
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

/// The original's new game: Ross 154 at the 3250 start, docked at the open-air
/// starport's allocated pad, with F998's cash, ship, hold and acceleration.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_starts_at_ross_154() {
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
/// Blackelk in Gateway [-1,4], with 10,000 CR and the observed 1 t hold.
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
    assert_eq!(state.cargo, 1, "the executed F2 status page's free hold");
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

/// The scripted takeoff: the new game starts landed on Ross 154's open-air
/// starport (`0x30`), `L` asks F654 and puts the ship in F1013's climb (`0x24`),
/// F1013 hands over to `0x26` at the altitude/set-speed test, and F1031's
/// contact scan clears the starport's effect region and releases the bay.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_takes_off_from_the_pad_into_free_flight() {
    use ffe_core::traffic::TakeoffStep;
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    use ffe_rs::original_flight::Keys as OriginalKeys;
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let index = nav.docked.expect("landed at the start starport");
    let state = nav.new_game.unwrap();
    let body = nav.bodies[index].clone();
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
    let station_body = landing_body(
        &body,
        &ffe_core::objects::ModelInfo::read(records, stats, body.model).unwrap(),
    );
    // The persistent two-body pad world, as the playable keeps it.
    model.begin_pad_takeoff(&station_body, body.position, &info);
    // Frames for an eye check, as the other view tests save them.
    let shots = std::env::var_os("FFE_SHOT_DIR");
    let viewer = shots
        .as_ref()
        .and_then(|_| ffe_rs::model_viewer::ModelViewer::new(&cache).ok());
    let save = |name: &str, model: &mut OriginalFlight, nav: &Navigator| {
        if let (Some(dir), Some(viewer)) = (&shots, &viewer) {
            let path = std::path::Path::new(dir).join(name);
            shot(&cache, viewer, nav, model, &path);
        }
    };
    save("takeoff-0000-pad.ppm", &mut model, &nav);
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
        // A complex starport shape leaves F1033's altitude at 0x7fffffff; the
        // playable measures the climb from the pad for F1013's test.
        let altitude = if contact.altitude() == i32::MAX {
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
                save("takeoff-0100-climb.ppm", &mut model, &nav);
            }
            Some(TakeoffStep::Cleared) => {
                model.set_mode(ffe_core::traffic::state::FREE);
                model.end_pad_takeoff();
                save("takeoff-0200-clear.ppm", &mut model, &nav);
                cleared = true;
                break;
            }
            Some(TakeoffStep::Retakeoff) => model.begin_takeoff(),
            None => {}
        }
    }
    let moved = distance(model.absolute_position(), state.seat);
    eprintln!(
        "takeoff modes {seen:?}, cleared {cleared}, moved {}, set speed {:.0} m/s, \
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
        let (command, _) = nav.command(&cache, &flight).expect("engaged");
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
    let (last, body) = nav.command(&cache, &flight).unwrap();
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
    screen.draw(&state, ViewState::default(), None, false, false, "");
    let before = screen.logical_pixels().to_vec();
    screen.discs = discs;
    screen.draw(&state, ViewState::default(), None, false, false, "");
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
            nav.target = Some(index);
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
            for _ in 0..200_000 {
                let Some((command, _)) = nav.command(&cache, &flight) else {
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
                if command.arrived {
                    arrived = true;
                    break;
                }
            }
            let range = distance(flight.player().motion.position, nav.bodies[index].position);
            eprintln!(
                "{name} -> {target_name} ({:?}): arrived {arrived} in {steps} steps, range {} stop {} set {} m/s aligned {last_aligned:.4} clock {:.0}",
                nav.bodies[index].kind,
                describe(range),
                describe(standoff(&nav.bodies[index])),
                flight.set_speed_metres(),
                nav.clock
            );
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
        let Some((command, _)) = nav.command(&cache, &flight) else {
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
            let Some((command, _)) = nav.command(&cache, &flight) else {
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
        .filter(|b| b.kind == Kind::Station)
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
        false,
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
    assert!(error.contains("Not enough fuel"), "{error}");
    assert_eq!(nav.system_name(), "Sol");
    nav.drive = Some(drive(3));
    let (distance, report) = nav.jump_report(centauri).unwrap();
    assert_eq!(
        (distance, report.fuel_needed, report.max_range),
        (435, 1, 1875)
    );
    let before = nav.clock;
    let jumped = nav.hyperspace(&cache, centauri).unwrap();
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
    assert!(error.contains("Out of range"), "{error}");
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
        radius: 1,
        inner_radius: 1,
        parent: None,
        model: 0,
        seed: 0,
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
