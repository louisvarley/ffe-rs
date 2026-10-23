use ffe_core::physics::Motion;
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
