//! Local-only comparison of `ffe_core::traffic` with the executed original: F650-F655, F657
//! and F867's clearance countdown (`--traffic`). Run through
//! `tools/reference/verify-traffic.py`, which needs the Linux reference build.
use ffe_core::traffic::{Reply, Starport};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

fn record(value: &Value) -> Starport {
    let pads = value["pads"].as_array().unwrap();
    Starport {
        mode: int(&value["mode"]) as u8,
        current: int(&value["current"]) as u8,
        timeout: int(&value["timeout"]) as u16,
        bays: int(&value["bays"]) as u16,
        pads: std::array::from_fn(|i| int(&pads[i]) as u8),
        door: 0,
    }
}

#[test]
#[ignore = "Local-only traffic comparison; use tools/reference/verify-traffic.py"]
fn executed_reference_traffic() {
    let path = std::env::var_os("FFE_TRAFFIC_VECTORS").expect("FFE_TRAFFIC_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 32 << 20, "trace too large");
    let mut counts = [0usize; 8];
    let mut granted = 0;
    let mut expired = 0;
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let call = int(&case["call"]);
        let ship = int(&case["ship"]) as u8;
        let mut port = record(&case["in"]);
        let want = record(&case["out"]);
        let (result, flag, pad) = (int(&case["result"]), int(&case["flag"]), int(&case["pad"]));
        let (mut got_result, mut got_flag, mut got_pad) =
            (0x5a5a_5a5a_i64, 0x5a5a_5a5a_i64, 0x5a5a_5a5a_i64);
        match call {
            0 => got_result = i64::from(port.request(ship)),
            1 => match port.allocate(ship) {
                Some(bay) => {
                    got_result = 0;
                    got_flag = if bay.fresh { -1 } else { 0 };
                    got_pad = i64::from(bay.index);
                }
                None => got_result = 0xffff_ffff,
            },
            2 => port.release(ship),
            3 => got_result = i64::from(port.touch(ship)),
            4 => port.reset(),
            5 => {
                got_result = match port.request_docking(ship, false, 0) {
                    Reply::Granted(bay) => {
                        // F655 hands F650 its own pad pointer and keeps the flag local.
                        assert_eq!(i64::from(bay.index), pad, "F655 pad: {line}");
                        granted += 1;
                        0
                    }
                    _ => 1,
                };
            }
            6 => {
                got_result = if port.is_docking(ship) {
                    0xffff_ffff
                } else {
                    0
                }
            }
            _ => {
                let elapsed = int(&case["elapsed"]) as u32;
                if port.elapse(elapsed).is_some() {
                    expired += 1;
                }
                got_result = 0x5a5a_5a5a;
            }
        }
        assert_eq!(port, want, "record after call {call}: {line}");
        if call != 7 {
            assert_eq!(got_result, result, "result of call {call}: {line}");
        }
        if call == 1 {
            assert_eq!((got_flag, got_pad), (flag, pad), "bay of call 1: {line}");
        }
        counts[call as usize] += 1;
    }
    eprintln!("traffic cases per call {counts:?}, {granted} requests granted, {expired} expired");
    assert!(counts.iter().all(|c| *c >= 1000), "{counts:?}");
    assert!(
        granted > 100 && expired > 30,
        "{granted} granted, {expired} expired"
    );
}
