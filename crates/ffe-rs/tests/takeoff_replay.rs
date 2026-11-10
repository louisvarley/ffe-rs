//! Local-only, frame-by-frame replay of the executed original's open-air takeoff
//! (`jjffe-smoke --flight takeoff 900`, start record 1, Ross 154) through the
//! port's `Navigator`/`OriginalFlight` (PAR-04).
//!
//! The original's script: the Tab launch action at frame 170 (mode `0x24` from
//! frame 171), then the faster key held from frame 260 (the set speed is 16 at
//! frame 261 and F1013 hands off to `0x26` there). Every frame is 21 ms (1,043
//! tics at 1x). Each port frame is compared with the trace row of the same
//! number: mode, F1033's altitude, F1121's set speed, the player's position in
//! its parent's frame and its velocity.
//!
//! Run with `FFE_WORLD_CACHE=<data.bin> FFE_TAKEOFF_TRACE=<takeoff.jsonl>`.
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{landing_body, Library, Navigator};
use ffe_rs::original_flight::{Keys as OriginalKeys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};
use serde::Deserialize;

const LAUNCH_FRAME: u64 = 170;
const FASTER_FROM: u64 = 260;
const FRAME_SECONDS: f64 = 0.021;

#[derive(Deserialize)]
struct Row {
    frame: u64,
    mode: u8,
    altitude: i64,
    set_speed: i64,
    position: [i64; 3],
    velocity: [i64; 3],
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct State {
    mode: u8,
    altitude: i64,
    set_speed: i64,
    position: [i64; 3],
    velocity: [i64; 3],
}

fn reference() -> Vec<Row> {
    let path = std::env::var_os("FFE_TAKEOFF_TRACE").expect("FFE_TAKEOFF_TRACE");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("{\"scenario\""))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The port's frames from the launch action on, keyed by the trace's frame number.
fn replay(last: u64) -> Vec<(u64, State)> {
    use ffe_core::traffic::{state as traffic_state, TakeoffStep};
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_WORLD_CACHE").expect("FFE_WORLD_CACHE")).unwrap(),
    )
    .unwrap();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let index = nav.docked.expect("landed at the start starport");
    let start = nav.new_game.unwrap();
    let body = nav.bodies[index].clone();
    let planet = nav.bodies[body.parent.unwrap()].clone();
    let records = cache.asset("base:model/records").unwrap();
    if std::env::var_os("FFE_TAKEOFF_VERBOSE").is_some() {
        eprintln!(
            "port: planet {:?} station {:?} seat {:?} station rows {:?} planet rows {:?}",
            planet.position, body.position, start.seat, body.rows, planet.rows
        );
    }
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ffe_core::objects::ModelInfo::read(records, stats, start.ship).unwrap();
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
    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        ..Flight::default()
    };
    flight.position = start.seat.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT);
    flight.axes = Some(body.orientation);
    // The ship's own statistics (F600's limits), as the game passes them.
    let ship_stats = stats
        .chunks_exact(48)
        .nth(start.ship as usize)
        .and_then(ffe_rs::ships::ShipStats::from_block);
    assert!(
        ship_stats.is_some(),
        "no statistics for ship {}",
        start.ship
    );
    let mut model = OriginalFlight::from_flight(&flight, ship_stats.as_ref());
    // Parked on the pad until the launch action.
    model.begin_pad_takeoff(
        &planet_body,
        planet.position,
        &station_body,
        body.position,
        &info,
    );
    // The original keeps the parked and climbing ship in the planet's rotating
    // frame (`+0x57`); the port's pad world keeps it in the planet's linear frame
    // with the planet's rows fixed. Turn the port's vectors into the rotating
    // frame (`local = rows * linear`) to compare them.
    let rows = planet.rows;
    let to_planet = move |v: [i64; 3]| -> [i64; 3] {
        std::array::from_fn(|k| {
            (0..3)
                .map(|axis| v[axis] as f64 * f64::from(rows[3 * k + axis]) / 2_147_483_648.0)
                .sum::<f64>()
                .round() as i64
        })
    };
    let state = |model: &OriginalFlight, mode: u8, altitude: i64| State {
        mode,
        altitude,
        set_speed: i64::from(model.set_speed_units()),
        position: to_planet(model.player().motion.position),
        velocity: to_planet(model.player().motion.velocity.map(i64::from)),
    };
    let mut rows = vec![(LAUNCH_FRAME, state(&model, 0x30, 0))];
    // The frame the launch action is processed (171 in the trace): as in the
    // game loop's docked-to-flight hand-off, the model is set up after that
    // frame's flight step, so the ship does not move until the next frame.
    nav.begin_launch().unwrap();
    model.begin_takeoff();
    rows.push((LAUNCH_FRAME + 1, state(&model, model.mode(), 0)));
    let mut mode;
    let mut altitude;
    for frame in LAUNCH_FRAME + 2..=last {
        let keys = OriginalKeys {
            faster: frame > FASTER_FROM,
            ..OriginalKeys::default()
        };
        let contact = if model.on_pad() {
            model.step_pad(FRAME_SECONDS, &keys, tools.as_ref())
        } else {
            // As the game loop chooses: the body under the ship, and for a
            // starport its planet with the port as F994's sibling.
            let at = model.absolute_position();
            let parent_index = nav.landing_parent(at, info.radii().0).unwrap_or(index);
            let parent = nav.bodies[parent_index].clone();
            let read =
                |model: u32| ffe_core::objects::ModelInfo::read(records, stats, model).unwrap();
            if parent.kind == ffe_rs::navigation::Kind::Starport {
                let ground = nav.bodies[parent.parent.unwrap()].clone();
                model.step_starport_landing(
                    FRAME_SECONDS,
                    &keys,
                    &landing_body(&ground, &read(ground.model)),
                    ground.position,
                    &landing_body(&parent, &read(parent.model)),
                    parent.position,
                    &info,
                    tools.as_ref(),
                )
            } else {
                model.step_landing(
                    FRAME_SECONDS,
                    &keys,
                    &landing_body(&parent, &read(parent.model)),
                    parent.position,
                    &info,
                    tools.as_ref(),
                )
            }
        };
        let contact = match contact {
            Ok(contact) => contact,
            Err(error) => {
                eprintln!("frame {frame}: {error}");
                break;
            }
        };
        // The game loop's hand-off (main.rs): F1013 runs inside the model, which
        // changes its own mode; the navigator follows once it is 0x26, and F1031
        // runs on the following frames with F1033's altitude.
        if nav.takeoff.is_some() {
            let step = if nav.taking_off() {
                if model.mode() == traffic_state::AFTER_TAKEOFF {
                    nav.takeoff_step(i32::MAX, model.set_speed_units(), contact.flags)
                } else {
                    None
                }
            } else {
                nav.takeoff_step(contact.altitude(), model.set_speed_units(), contact.flags)
            };
            match step {
                Some(TakeoffStep::HandOff) => model.set_mode(nav.takeoff_state().unwrap()),
                Some(TakeoffStep::Cleared) => {
                    model.set_mode(0);
                    if model.on_pad() {
                        model.end_pad_takeoff();
                    }
                }
                Some(TakeoffStep::Retakeoff) => model.begin_takeoff(),
                None => {}
            }
        }
        mode = model.mode();
        altitude = i64::from(model.player().altitude);
        rows.push((frame, state(&model, mode, altitude)));
    }
    rows
}

#[test]
#[ignore = "Local-only: needs FFE_WORLD_CACHE and FFE_TAKEOFF_TRACE"]
fn port_takeoff_replays_the_original_frame_by_frame() {
    let reference = reference();
    let last = reference.last().unwrap().frame;
    let port = replay(last);
    let mut first_difference = None;
    for (frame, ours) in &port {
        let Some(row) = reference.iter().find(|row| row.frame == *frame) else {
            continue;
        };
        let theirs = State {
            mode: row.mode,
            altitude: row.altitude,
            set_speed: row.set_speed,
            position: row.position,
            velocity: row.velocity,
        };
        let verbose = std::env::var_os("FFE_TAKEOFF_VERBOSE").is_some();
        if verbose || (*ours != theirs && first_difference.is_none()) {
            eprintln!("frame {frame}\n  original {theirs:?}\n  port     {ours:?}");
        }
        if *ours != theirs && first_difference.is_none() {
            first_difference = Some(*frame);
        }
    }
    assert_eq!(first_difference, None, "first differing frame");
}

/// The default start's seat (type 0, Hope) in Hope's frame against the executed
/// original's `--geometry` (ship minus port: (128961, 203981, 157118)).
#[test]
#[ignore = "Local-only: needs FFE_WORLD_CACHE"]
fn hope_seat_matches_the_original() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_WORLD_CACHE").expect("FFE_WORLD_CACHE")).unwrap(),
    )
    .unwrap();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.unwrap();
    let start = nav.new_game.unwrap();
    let body = &nav.bodies[index];
    let (_, planet_rows) = body.parent_frame.expect("Hope's port is in Hope's frame");
    let in_planet = |seat: [i64; 3]| -> [i64; 3] {
        std::array::from_fn(|k| {
            (0..3)
                .map(|axis| {
                    (seat[axis] - body.position[axis]) as f64 * f64::from(planet_rows[3 * k + axis])
                        / 2_147_483_648.0
                })
                .sum::<f64>()
                .round() as i64
        })
    };
    // The new game's seat and the one the playable parks and launches from.
    let parked = nav.parked_pad_seat(index, 4000).unwrap();
    for (name, seat) in [("new game", start.seat), ("parked", parked)] {
        let local = in_planet(seat);
        eprintln!(
            "{name} seat - port in Hope's frame {local:?}, bay {}",
            start.bay
        );
        for (got, want) in local.iter().zip([128_961i64, 203_981, 157_118]) {
            assert!((got - want).abs() <= 1, "{name}: {local:?}");
        }
    }
}
