use ffe_core::physics::{Motion, Scanner};
use ffe_core::player::{Player, ThrusterKeys};
use ffe_core::thrust::Limits;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct After {
    velocity: [i32; 3],
    fraction: [u16; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accelerate {
    case: usize,
    function: u32,
    tics: u32,
    matrix: [i32; 9],
    thrust: [i16; 3],
    fraction: [u16; 3],
    velocity: [i32; 3],
    cap: [i32; 3],
    after: After,
}

/// Original F601 and F602 (`--accelerate`): 4096 synthetic objects over time steps in the
/// small, medium and large regimes, with positive, negative and zero caps and
/// zero thrust words. The velocity and fraction words must match.
#[test]
#[ignore = "Local-only original physics; use tools/reference/verify-physics.py"]
fn executed_reference_accelerate() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_ACCELERATE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Accelerate = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let m = case.matrix;
        let mut motion = Motion {
            orientation: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
            thrust: case.thrust,
            velocity: case.velocity,
            fraction: case.fraction,
            cap: case.cap,
            ..Motion::default()
        };
        match case.function {
            601 => motion.accelerate(case.tics),
            602 => motion.accelerate_uncapped(case.tics),
            other => panic!("unknown function {other}"),
        }
        assert_eq!(
            (motion.velocity, motion.fraction),
            (case.after.velocity, case.after.fraction),
            "accelerate case {} tics {}",
            case.case,
            case.tics
        );
        cases += 1;
    }
    eprintln!("compared {cases} accelerate cases, 0 mismatched");
    assert_eq!(cases, 4096);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrientAfter {
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    scanner: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Orient {
    case: usize,
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    autopilot: [i16; 3],
    model: u32,
    craft: u8,
    player: u32,
    scanner: u16,
    time: i32,
    ai: i32,
    ai_time: i32,
    after: OrientAfter,
}

/// Original F585 (`--orient`): 4096 synthetic objects (near-identity and random
/// orientations, drift counters around zero, small and large rates, every model
/// band, the autopilot on and off, time steps negative to huge). The matrix, drift
/// counter, rates and scanner word after must match, F584/F611/F1520 included.
#[test]
#[ignore = "Local-only original physics; use tools/reference/verify-physics.py"]
fn executed_reference_orient() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_ORIENT_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let (mut cases, mut renormalised) = (0, 0);
    for line in input.lines() {
        let case: Orient = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let m = case.matrix;
        let mut motion = Motion {
            orientation: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
            drift: case.drift,
            rate: case.rate,
            model: case.model,
            craft: case.craft,
            autopilot: case.autopilot,
            ..Motion::default()
        };
        let mut scanner = Scanner {
            player: case.player,
            rotation: case.scanner,
        };
        motion.orient(case.time, case.ai != 0, case.ai_time, &mut scanner);
        let a = case.after.matrix;
        let expected = [[a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]];
        assert_eq!(
            (
                motion.orientation,
                motion.drift,
                motion.rate,
                scanner.rotation
            ),
            (
                expected,
                case.after.drift,
                case.after.rate,
                case.after.scanner
            ),
            "orient case {}",
            case.case
        );
        if case.after.drift == 0x3c {
            renormalised += 1;
        }
        cases += 1;
    }
    eprintln!("compared {cases} orient cases ({renormalised} renormalised), 0 mismatched");
    assert_eq!(cases, 4096);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerAfter {
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    thrust: [i16; 3],
    cap: [i32; 3],
    counter: i8,
    scanner: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerCase {
    case: usize,
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    limits: [i16; 6],
    velocity: [i32; 3],
    model: u32,
    craft: u8,
    player: u32,
    scanner: u16,
    counter: i8,
    tics: u32,
    altitude: i32,
    mode: u8,
    flags: u8,
    combat: u8,
    combat_velocity: [i32; 3],
    set_speed: i32,
    keys: [u8; 10],
    after: PlayerAfter,
}

/// Original F1041 in free space (`--player-orient`): 4096 synthetic players with
/// near-identity and random orientations, limits, velocities, set speeds, the
/// take-off mode, engines on and off, combat mode and random pressed thruster
/// keys. The orientation, drift counter, rates, thrust words, caps (set by the
/// set-speed request), countdown and scanner word after must match.
#[test]
#[ignore = "Local-only original player update; use tools/reference/verify-physics.py"]
fn executed_reference_player_orient() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_PLAYER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 32 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: PlayerCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let m = case.matrix;
        let l = case.limits;
        let mut player = Player {
            motion: Motion {
                orientation: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
                drift: case.drift,
                rate: case.rate,
                velocity: case.velocity,
                model: case.model,
                craft: case.craft,
                ..Motion::default()
            },
            limits: Limits {
                axes: [[l[0], l[1]], [l[2], l[3]], [l[4], l[5]]],
            },
            set_speed: case.set_speed,
            mode: case.mode,
            flags: case.flags,
            altitude: case.altitude,
            frame_tics: case.tics,
            reference_counter: case.counter,
            combat: case.combat & 2 != 0,
            combat_velocity: case.combat_velocity,
            scanner: Scanner {
                player: case.player,
                rotation: case.scanner,
            },
        };
        let k = case.keys.map(|k| k != 0);
        let keys = ThrusterKeys {
            forward: k[0],
            reverse: k[1],
            numpad_forward: k[2],
            numpad_reverse: k[3],
            up_a: k[4],
            up_b: k[5],
            down_a: k[6],
            down_b: k[7],
            right: k[8],
            left: k[9],
        };
        player.orient_and_thrust(&keys).unwrap();
        let a = case.after.matrix;
        let expected = [[a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]];
        assert_eq!(
            (
                player.motion.orientation,
                player.motion.drift,
                player.motion.rate,
                player.motion.thrust,
                player.motion.cap,
                player.reference_counter,
                player.scanner.rotation
            ),
            (
                expected,
                case.after.drift,
                case.after.rate,
                case.after.thrust,
                case.after.cap,
                case.after.counter,
                case.after.scanner
            ),
            "player case {}",
            case.case
        );
        cases += 1;
    }
    eprintln!("compared {cases} player cases, 0 mismatched");
    assert_eq!(cases, 4096);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Advance {
    case: usize,
    time: i64,
    position: [i64; 3],
    velocity: [i32; 3],
    after: [i64; 3],
}

/// Original F1512/F1511 (`--advance`): 4096 synthetic positions and velocities
/// over small, large, huge and negative 64-bit times. The position after must
/// match, the original's wrapped 32-bit shift counts included.
#[test]
#[ignore = "Local-only original physics; use tools/reference/verify-physics.py"]
fn executed_reference_advance() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_ADVANCE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Advance = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let mut motion = Motion {
            position: case.position,
            velocity: case.velocity,
            ..Motion::default()
        };
        motion.advance(case.time);
        assert_eq!(
            motion.position, case.after,
            "advance case {} time {}",
            case.case, case.time
        );
        cases += 1;
    }
    eprintln!("compared {cases} advance cases, 0 mismatched");
    assert_eq!(cases, 4096);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepAfter {
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    thrust: [i16; 3],
    cap: [i32; 3],
    counter: i8,
    scanner: u16,
    velocity: [i32; 3],
    position: [i64; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepCase {
    case: usize,
    time: i64,
    position: [i64; 3],
    matrix: [i32; 9],
    drift: i8,
    rate: [i16; 3],
    limits: [i16; 6],
    velocity: [i32; 3],
    model: u32,
    craft: u8,
    player: u32,
    scanner: u16,
    counter: i8,
    tics: u32,
    altitude: i32,
    mode: u8,
    flags: u8,
    combat: u8,
    combat_velocity: [i32; 3],
    set_speed: i32,
    keys: [u8; 10],
    after: StepAfter,
}

/// One whole free-flight tick (`--flight-step`): the original F1041, then F601
/// or F602 and F1512, against `Player::step`, on the player cases' inputs with a
/// 64-bit time and a starting position. Everything F1021 changes must match.
#[test]
#[ignore = "Local-only original player update; use tools/reference/verify-physics.py"]
fn executed_reference_flight_step() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_STEP_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 48 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: StepCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let m = case.matrix;
        let l = case.limits;
        let mut player = Player {
            motion: Motion {
                orientation: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
                drift: case.drift,
                rate: case.rate,
                velocity: case.velocity,
                model: case.model,
                craft: case.craft,
                position: case.position,
                ..Motion::default()
            },
            limits: Limits {
                axes: [[l[0], l[1]], [l[2], l[3]], [l[4], l[5]]],
            },
            set_speed: case.set_speed,
            mode: case.mode,
            flags: case.flags,
            altitude: case.altitude,
            frame_tics: case.tics,
            reference_counter: case.counter,
            combat: case.combat & 2 != 0,
            combat_velocity: case.combat_velocity,
            scanner: Scanner {
                player: case.player,
                rotation: case.scanner,
            },
        };
        let k = case.keys.map(|k| k != 0);
        let keys = ThrusterKeys {
            forward: k[0],
            reverse: k[1],
            numpad_forward: k[2],
            numpad_reverse: k[3],
            up_a: k[4],
            up_b: k[5],
            down_a: k[6],
            down_b: k[7],
            right: k[8],
            left: k[9],
        };
        player.step(&keys, case.time).unwrap();
        let a = case.after.matrix;
        let expected = [[a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]];
        assert_eq!(
            (
                player.motion.orientation,
                player.motion.drift,
                player.motion.rate,
                player.motion.thrust,
                player.motion.cap,
                player.reference_counter,
                player.scanner.rotation,
                player.motion.velocity,
                player.motion.position
            ),
            (
                expected,
                case.after.drift,
                case.after.rate,
                case.after.thrust,
                case.after.cap,
                case.after.counter,
                case.after.scanner,
                case.after.velocity,
                case.after.position
            ),
            "flight step case {}",
            case.case
        );
        cases += 1;
    }
    eprintln!("compared {cases} flight step cases, 0 mismatched");
    assert_eq!(cases, 4096);
}
