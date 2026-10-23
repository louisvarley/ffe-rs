use ffe_core::thrust::{clamp, request_clamp, scale_to_limits, Limits};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: usize,
    function: u32,
    limits: [i16; 6],
    request: [i32; 3],
    out: [i32; 3],
    words: [u16; 3],
}

/// Original F598/F599/F600 (`--thrust-limits`): 4096 synthetic limit words and
/// requests (in range, beyond a limit, negative, zero, at the limits). The
/// returned dwords and the words stored in the object must match.
#[test]
#[ignore = "Local-only original thrust limits; use tools/reference/verify-thrust.py"]
fn executed_reference_thrust_limits() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_THRUST_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let l = case.limits;
        let limits = Limits {
            axes: [[l[0], l[1]], [l[2], l[3]], [l[4], l[5]]],
        };
        let result = match case.function {
            598 => request_clamp(&limits, case.request),
            599 => clamp(&limits, case.request),
            600 => scale_to_limits(&limits, case.request),
            other => panic!("unknown function {other}"),
        };
        assert_eq!(
            (result.vector, result.words),
            (case.out, case.words),
            "thrust case {} F{} {:?} {:?}",
            case.case,
            case.function,
            case.limits,
            case.request
        );
        cases += 1;
    }
    eprintln!("compared {cases} thrust cases, 0 mismatched");
    assert_eq!(cases, 4096);
}
