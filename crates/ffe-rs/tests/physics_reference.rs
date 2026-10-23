use ffe_core::physics::{Motion, Scanner};
use ffe_core::player::{Controls, Player, SteerKeys, ThrusterKeys};
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
struct GravityAfter {
    velocity: [i32; 3],
    fraction: [u16; 3],
    truncated: [i32; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gravity {
    case: usize,
    parent: u8,
    mass: [i16; 2],
    time: i32,
    position: [i64; 3],
    velocity: [i32; 3],
    fraction: [u16; 3],
    after: GravityAfter,
}

/// Original F603 (`--gravity`): 4096 synthetic objects with a parent (or none),
/// an ffp mass (or none), positions of every magnitude and time steps of both
/// signs. The velocity, fraction words and the truncated position left in
/// `DATA_008708..DATA_008710` (untouched on an early return) must match.
#[test]
#[ignore = "Local-only original physics; use tools/reference/verify-physics.py"]
fn executed_reference_gravity() {
    const UNTOUCHED: i32 = 0x5a5a_5a5a;
    let input = std::fs::read_to_string(std::env::var_os("FFE_GRAVITY_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 << 20);
    let (mut cases, mut pulled) = (0, 0);
    for line in input.lines() {
        let case: Gravity = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let mut motion = Motion {
            parent: case.parent,
            position: case.position,
            velocity: case.velocity,
            fraction: case.fraction,
            ..Motion::default()
        };
        let mass = u32::from(case.mass[0] as u16) | u32::from(case.mass[1] as u16) << 16;
        let truncated = motion.gravity(mass, case.time);
        assert_eq!(
            (
                motion.velocity,
                motion.fraction,
                truncated.unwrap_or([UNTOUCHED; 3])
            ),
            (
                case.after.velocity,
                case.after.fraction,
                case.after.truncated
            ),
            "gravity case {} mass {:?} time {}",
            case.case,
            case.mass,
            case.time
        );
        pulled += usize::from(motion.velocity != case.velocity || motion.fraction != case.fraction);
        cases += 1;
    }
    assert_eq!(cases, 4096);
    assert!(pulled > 1000, "too few cases moved the velocity: {pulled}");
    eprintln!("compared {cases} gravity cases ({pulled} changed the velocity)");
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
            ..Player::default()
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
            ..Player::default()
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SteerAfter {
    flags: u8,
    view: u8,
    speed: i32,
    previous: u32,
    turret: [i16; 2],
    rate: [i16; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SteerCase {
    case: usize,
    active: u8,
    keys_on: u32,
    time: u32,
    boost: u32,
    flags: u8,
    view: u8,
    speed: i32,
    previous: u32,
    mouse: u32,
    flip: [u8; 2],
    roll: u8,
    turret: [i16; 2],
    gun_mounts: i8,
    rate: [i16; 3],
    #[serde(rename = "return")]
    return_key: u8,
    shift: u8,
    hold: [u32; 6],
    after: SteerAfter,
}

/// Original F1037 (`--steer`): 4096 synthetic inputs (Return and Shift held or not,
/// set speeds across the whole range including the wrap edges, the slow-down flag,
/// mouse motion, flipped axes, mouse roll, held steering keys, rates at their limits,
/// every view mode and gun-mount count). The set speed, flags, mouse word, view,
/// turret angles and rates after must match.
#[test]
#[ignore = "Local-only original steering input; use tools/reference/verify-physics.py"]
fn executed_reference_steer() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_STEER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: SteerCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let mut player = Player {
            motion: Motion {
                rate: case.rate,
                gun_mounts: case.gun_mounts,
                ..Motion::default()
            },
            set_speed: case.speed,
            flags: case.flags,
            active: (case.active as i8) < 0,
            input_time: case.time,
            speed_boost: case.boost,
            keys_enabled: case.keys_on != 0,
            mouse_previous: case.previous,
            mouse_flip: [case.flip[0] != 0, case.flip[1] != 0],
            mouse_roll: case.roll != 0,
            view: case.view,
            turret: case.turret,
            ..Player::default()
        };
        let h = case.hold;
        let controls = Controls {
            mouse: case.mouse,
            faster: case.return_key != 0,
            slower: case.shift != 0,
            steer: SteerKeys {
                pitch_down: h[0],
                pitch_up: h[1],
                left: h[2],
                right: h[3],
                roll_left: h[4],
                roll_right: h[5],
            },
        };
        player.update_set_speed(&controls);
        assert_eq!(
            (
                player.flags,
                player.view,
                player.set_speed,
                player.mouse_previous,
                player.turret,
                player.motion.rate
            ),
            (
                case.after.flags,
                case.after.view,
                case.after.speed,
                case.after.previous,
                case.after.turret,
                case.after.rate
            ),
            "steer case {}",
            case.case
        );
        cases += 1;
    }
    eprintln!("compared {cases} steer cases, 0 mismatched");
    assert_eq!(cases, 4096);
}
