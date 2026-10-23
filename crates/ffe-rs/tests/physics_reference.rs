use ffe_core::physics::{Motion, Scanner};
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
