//! Local-only comparison of scripted flight scenarios with the executed original
//! (`--flight idle|takeoff|planet`). Run through `tools/reference/verify-flight.py`.
//!
//! The reference trace is deterministic and bounded: one row per frame from 140
//! (the intro still) with the player index, view mode, F1013's altitude and
//! F1121's set speed, the player's planet (`+0x56`, model and position), the
//! ship's position and velocity, and an FNV-1a chain over exactly those fields.
//! No framebuffer or game data is written.
//!
//! Two scenarios are compared with the port:
//!
//! * `idle`: the original stays in mode `0x30` on the pad (position and velocity
//!   unchanged, planet retained). `Flight::default()` must stay `Docked` likewise.
//! * `takeoff`/`planet`: the launch action produces the `0x30 -> 0x24 -> 0x26 ->
//!   0x00` machine (F1013's hand-off, F1031's cleared region), the ship keeps the
//!   same planet (`+0x56`) through free flight, and no frame moves it by a
//!   teleport-sized step. The port's `Navigator`/`OriginalFlight` takeoff must
//!   produce the same mode sequence and retain the same planet model.
use ffe_rs::flight::{Controls, Flight, Mode};
use ffe_rs::navigation::{landing_body, Kind, Library, Navigator, PLAYER};
use ffe_rs::original_flight::{Keys as OriginalKeys, UNITS_PER_FLIGHT_UNIT};
use serde::Deserialize;

const FIRST_FRAME: u64 = 140;
const FRAMES: usize = 900;
const ROWS: usize = FRAMES - FIRST_FRAME as usize;
const LAUNCH: u8 = 0x24;
const AFTER_TAKEOFF: u8 = 0x26;
const FREE: u8 = 0x00;
/// F1013's documented climb threshold (`0x30d40`); the hand-off also fires when
/// `DATA_008918` (the set speed) is non-zero.
const CLIMB_ALTITUDE: u64 = 0x30d40;

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Planet {
    index: u64,
    model: u64,
    parent: u64,
    position: [i64; 3],
}

impl Planet {
    /// The body the player frame keeps; its position drifts with its orbit.
    fn identity(&self) -> (u64, u64, u64) {
        (self.index, self.model, self.parent)
    }
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Row {
    scenario: String,
    frame: u64,
    mode: u8,
    tics: u64,
    player: u64,
    start_type: u64,
    altitude: u64,
    set_speed: u64,
    planet: Planet,
    position: [i64; 3],
    velocity: [i64; 3],
    prev_hash: String,
    state_hash: String,
}

fn mix(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

/// The exact order in `flight_trace_row` (`headless.c`).
fn row_hash(row: &Row, code: u64, previous: u64) -> u64 {
    let mut hash = 14_695_981_039_346_656_037;
    for value in [
        code,
        row.frame,
        u64::from(row.mode),
        row.tics,
        row.player,
        row.start_type,
        row.altitude,
        row.set_speed,
        row.planet.index,
        row.planet.model,
        row.planet.parent,
        row.planet.position[0] as u64,
        row.planet.position[1] as u64,
        row.planet.position[2] as u64,
        row.position[0] as u64,
        row.position[1] as u64,
        row.position[2] as u64,
        row.velocity[0] as u64,
        row.velocity[1] as u64,
        row.velocity[2] as u64,
        previous,
    ] {
        hash = mix(hash, value);
    }
    hash
}

fn hex(value: &str) -> u64 {
    u64::from_str_radix(value, 16).unwrap_or_else(|error| panic!("bad hash {value}: {error}"))
}

fn load() -> Vec<Row> {
    let path = std::env::var_os("FFE_FLIGHT_VECTORS").expect("FFE_FLIGHT_VECTORS");
    let input = std::fs::read_to_string(path).unwrap();
    assert!(input.len() < 8 << 20, "flight trace too large");
    input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Check the chain and split the trace into its three scenarios in file order.
fn scenarios(rows: &[Row]) -> [Vec<Row>; 3] {
    let mut groups: [Vec<Row>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut group: Option<usize> = None;
    let mut previous = 0u64;
    for row in rows {
        let index = match row.scenario.as_str() {
            "idle" => 0,
            "takeoff" => 1,
            "planet" => 2,
            other => panic!("unknown scenario {other}"),
        };
        if group != Some(index) {
            assert!(
                match group {
                    None => true,
                    Some(before) => before < index,
                },
                "scenario order"
            );
            group = Some(index);
            previous = 0;
        }
        assert_eq!(
            row.frame,
            FIRST_FRAME + groups[index].len() as u64,
            "frames"
        );
        assert_eq!(
            hex(&row.prev_hash),
            previous,
            "broken hash chain at frame {}",
            row.frame
        );
        assert_eq!(
            hex(&row.state_hash),
            row_hash(row, index as u64, previous),
            "state hash at frame {}",
            row.frame
        );
        previous = hex(&row.state_hash);
        groups[index].push(row.clone());
    }
    groups
}

/// The reference trace must be self-consistent and show the documented
/// scenarios: the still pad, F1013's climb/hand-off, and the planet the player
/// frame keeps after free flight.
#[test]
#[ignore = "Local-only executed original; use tools/reference/verify-flight.py"]
fn executed_reference_flight_scenarios() {
    let rows = load();
    assert_eq!(rows.len(), 3 * ROWS, "trace length");
    let [idle, takeoff, planet] = scenarios(&rows);

    for (name, group) in [("idle", &idle), ("takeoff", &takeoff), ("planet", &planet)] {
        assert_eq!(group.first().unwrap().frame, FIRST_FRAME);
        assert_eq!(group.last().unwrap().frame, FIRST_FRAME + ROWS as u64 - 1);
        assert!(group.iter().any(|row| row.player != 0), "{name}: no player");
    }

    // Idle boot: the intro is quiet, then the original holds the pad in 0x30.
    assert!(
        idle.iter().any(|row| row.mode == 0),
        "idle: no intro frames"
    );
    let idle_player: Vec<&Row> = idle.iter().filter(|row| row.player != 0).collect();
    assert!(idle_player.len() > 100, "idle: too few player frames");
    let pad = idle_player.first().unwrap();
    for row in &idle_player {
        assert_eq!(row.mode, 0x30, "idle mode at frame {}", row.frame);
        assert_eq!(row.start_type, 1, "idle start type");
        assert_eq!(row.player, pad.player, "idle player index");
        assert_eq!(
            row.planet.identity(),
            pad.planet.identity(),
            "idle planet identity"
        );
        assert_eq!(row.position, pad.position, "idle pad position");
        assert_eq!(row.velocity, [0; 3], "idle velocity");
        assert_eq!(row.altitude, 0, "idle altitude");
    }

    // Take-off and the retained planet: both scripts run the same launch and
    // differ only in length, so the hand-off frames agree.
    let modes = |group: &[Row]| {
        let mut modes = Vec::new();
        for row in group.iter().filter(|row| row.player != 0) {
            if modes.last() != Some(&row.mode) {
                modes.push(row.mode);
            }
        }
        modes
    };
    assert_eq!(
        modes(&takeoff),
        [0x30, LAUNCH, AFTER_TAKEOFF, FREE],
        "take-off modes"
    );
    assert_eq!(
        modes(&planet),
        [0x30, LAUNCH, AFTER_TAKEOFF, FREE],
        "planet modes"
    );
    let handoff = takeoff
        .iter()
        .find(|row| row.mode == AFTER_TAKEOFF)
        .expect("0x26 hand-off");
    assert!(
        handoff.altitude > CLIMB_ALTITUDE || handoff.set_speed != 0,
        "F1013 handed off at altitude {} and set speed {}",
        handoff.altitude,
        handoff.set_speed
    );

    let takeoff_planet = takeoff
        .iter()
        .find(|row| row.player != 0)
        .unwrap()
        .planet
        .clone();
    assert_ne!(takeoff_planet.index, 0, "no planet");
    assert_ne!(takeoff_planet.model, 0, "no planet model");
    let mut last_step = 0.0_f64;
    let mut last_planet_step = 0.0_f64;
    for group in [&takeoff, &planet] {
        let mut previous: Option<&Row> = None;
        for row in group.iter().filter(|row| row.player != 0) {
            assert_eq!(
                row.planet.identity(),
                takeoff_planet.identity(),
                "planet replaced at frame {}",
                row.frame
            );
            if let Some(before) = previous {
                let step = distance(before.position, row.position);
                let planet_step = distance(before.planet.position, row.planet.position);
                // The 10,812,465 mm frame switch reported in the takeover notes
                // must not appear: real per-frame motion is far below one km.
                assert!(
                    step < 1_000_000.0,
                    "teleport at frame {}: {step} mm",
                    row.frame
                );
                assert!(
                    planet_step < 1_000_000.0,
                    "planet jump at frame {}",
                    row.frame
                );
                last_step = last_step.max(step);
                last_planet_step = last_planet_step.max(planet_step);
            }
            previous = Some(row);
        }
    }
    // Free flight keeps the same planet body: distance from its centre changes
    // by the ship's own travel, not by a frame change.
    let free: Vec<&Row> = planet
        .iter()
        .filter(|row| row.player != 0 && row.mode == FREE)
        .collect();
    assert!(free.len() > 100, "planet: too few free-flight frames");
    assert_eq!(
        free.last().unwrap().planet.identity(),
        takeoff_planet.identity()
    );
    let travel = distance(
        free.first().unwrap().position,
        free.last().unwrap().position,
    );
    assert!(
        travel > 1_000_000.0,
        "planet: ship did not fly in free flight ({travel} mm)"
    );
    eprintln!(
        "flight reference: rows {} ({} each), planet {} model {}, max ship step {last_step:.0} mm, \
         max planet step {last_planet_step:.0} mm, free travel {travel:.0} mm",
        3 * ROWS, ROWS, takeoff_planet.index, takeoff_planet.model
    );
}

fn distance(a: [i64; 3], b: [i64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            let delta = *a as f64 - b as f64;
            delta * delta
        })
        .sum::<f64>()
        .sqrt()
}

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// The port's idle and takeoff scenarios must reach the same observable states
/// as the executed original: a still pad, the `0x24 -> 0x26 -> 0x00` hand-off,
/// and the same planet retained through free flight.
#[test]
#[ignore = "Local-only: needs the content cache; use tools/reference/verify-flight.py"]
fn port_flight_scenarios_match_the_reference() {
    use ffe_core::traffic::{state as traffic_state, TakeoffStep};

    let rows = load();
    let [idle, takeoff, planet_trace] = scenarios(&rows);
    let reference_planet = takeoff
        .iter()
        .find(|row| row.player != 0)
        .unwrap()
        .planet
        .clone();

    // Idle boot: the port's docked state is still, as the original's 0x30 pad.
    let mut flight = Flight::default();
    assert_eq!(flight.mode, Mode::Docked);
    let seat = flight.position;
    for _ in 0..200 {
        flight.step(Controls::default()).unwrap();
    }
    assert_eq!(flight.mode, Mode::Docked, "idle left the pad");
    assert_eq!(flight.position, seat, "idle position");
    assert_eq!(flight.velocity, [0.0; 3], "idle velocity");
    assert!(
        idle.iter()
            .filter(|row| row.player != 0)
            .all(|row| row.mode == 0x30),
        "the original did not idle in 0x30"
    );

    // Take-off: the same mode machine on the same start record (1 = Ross 154).
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let index = nav.docked.expect("landed at the start starport");
    let start = nav.new_game.unwrap();
    let body = nav.bodies[index].clone();
    let planet_index = body.parent.expect("the starport orbits a planet");
    let planet = nav.bodies[planet_index].clone();
    assert!(
        matches!(planet.kind, Kind::Planet | Kind::Moon),
        "starport parent is {:?}, not a planet or moon",
        planet.kind
    );
    assert_eq!(
        planet.model as u64, reference_planet.model,
        "port planet model differs from the executed original"
    );
    nav.begin_launch().unwrap();
    assert_eq!(nav.takeoff_state(), Some(traffic_state::TAKEOFF));

    let mut flight = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        ..Flight::default()
    };
    flight.position = start.seat.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT);
    flight.axes = Some(body.orientation);
    let mut model = ffe_rs::original_flight::OriginalFlight::from_flight(&flight, None);
    model.begin_takeoff();
    let records = cache.asset("base:model/records").unwrap();
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
    model.begin_pad_takeoff(
        &planet_body,
        planet.position,
        &station_body,
        body.position,
        &info,
    );

    let mut seen = vec![0x30u8];
    let mut pad_parents = Vec::new();
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
        let climbing = nav.takeoff_state() == Some(traffic_state::TAKEOFF);
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
        .unwrap_or_else(|error| panic!("frame {frame}: parent {parent_index}: {error}"));
        pad_parents.push(model.player().motion.parent);
        let before = nav.takeoff_state().unwrap();
        let altitude = if model.on_pad() {
            let at = model.absolute_position();
            (0..3)
                .map(|k| (at[k] - start.seat[k]) as f64 * body.orientation[1][k])
                .sum::<f64>()
                .clamp(0.0, f64::from(i32::MAX)) as i32
        } else {
            contact.altitude()
        };
        match nav.takeoff_step(altitude, model.set_speed_units(), contact.flags) {
            Some(TakeoffStep::HandOff) => {
                model.set_mode(nav.takeoff_state().unwrap());
                seen.push(before);
            }
            Some(TakeoffStep::Cleared) => {
                model.set_mode(FREE);
                model.end_pad_takeoff();
                seen.push(before);
                seen.push(FREE);
                cleared = true;
                break;
            }
            Some(TakeoffStep::Retakeoff) => model.begin_takeoff(),
            None => {}
        }
    }

    assert!(cleared, "the port did not clear the starport region");
    assert_eq!(
        seen,
        [0x30, LAUNCH, AFTER_TAKEOFF, FREE],
        "port mode machine"
    );
    assert_eq!(
        planet.model as u64, reference_planet.model,
        "retained planet model"
    );
    assert_eq!(
        nav.bodies[planet_index].model, planet.model,
        "the port's planet body was replaced"
    );
    // F994's pad world parents the player to body 2, the starport's parent
    // planet/moon; the original's `+0x56` does the same. The cleared state
    // returns to the system frame (body 0), as F573/F1677 do, while the planet
    // body stays in the system.
    assert!(
        !pad_parents.is_empty() && pad_parents.iter().all(|&parent| parent == 2),
        "the port's pad parent frame changed: {pad_parents:?}"
    );
    assert_eq!(
        model.player().motion.parent,
        0,
        "the port did not return to the system frame"
    );
    assert_eq!(nav.docked, None, "still docked");
    assert_eq!(nav.takeoff, None, "takeoff state not released");
    assert_eq!(
        nav.starport(index).bay_of(PLAYER),
        None,
        "F1032/F651 did not release the bay"
    );
    // Free flight continued: the original's planet is still the parent after
    // the same number of frames. The reference trace's free-flight tail is the
    // comparison, and both end in mode 0x00.
    assert_eq!(
        planet_trace
            .iter()
            .find(|row| row.player != 0)
            .unwrap()
            .planet
            .model,
        planet.model as u64
    );
    eprintln!(
        "port flight: modes {seen:?}, planet {planet_index} model {}, {} pad frames",
        planet.model,
        pad_parents.len()
    );
}
