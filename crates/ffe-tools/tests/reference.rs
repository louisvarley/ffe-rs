use ffe_core::{RandomState, TimerCounters};
use ffe_tools::reference::verify_trace_bytes;
use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

// Synthetic validator fixtures, not proprietary reference captures or an oracle.
fn vectors() -> Vec<Value> {
    let mut rows = Vec::new();
    for (scenario, (a, b)) in [
        (0, 0),
        (0x12345678, 0x456789ab),
        (0x12345678, 0x89abcdef),
        (u32::MAX, u32::MAX),
        (0x80000000, 1),
        (1, 0x80000000),
    ]
    .into_iter()
    .enumerate()
    {
        let mut rng = RandomState::new(a, b);
        for step in 0..256 {
            let before = rng;
            let value = rng.next_u16();
            rows.push(json!({
                "kind": "rng", "scenario": scenario, "step": step,
                "before_a": before.a, "before_b": before.b,
                "after_a": rng.a, "after_b": rng.b, "value": value,
            }));
        }
    }
    for acceleration in [0, 1, 994, 1000, 10000, 1000000] {
        for (index, elapsed_ms) in [0, 1, 5, 19, 20, 21, 39, 40, 100, 1000]
            .into_iter()
            .enumerate()
        {
            let system = if index % 2 == 0 { 0 } else { 0xfffffff0 };
            let accelerated = if index % 2 == 0 { 0 } else { 0xffffffe0 };
            let mut counters = TimerCounters {
                system,
                accelerated,
            };
            counters.advance(elapsed_ms, acceleration).unwrap();
            rows.push(json!({
                "kind": "timer", "elapsed_ms": elapsed_ms, "acceleration": acceleration,
                "before_system": system, "before_accel": accelerated,
                "after_system": counters.system, "after_accel": counters.accelerated,
                "last_ms": 100 + elapsed_ms, "next_50hz_ms": 100 + (elapsed_ms / 20 + 1) * 20,
            }));
        }
    }
    rows
}

fn states() -> Vec<Value> {
    (0..400u32)
        .map(|frame| {
            let in_game = frame >= 161;
            json!({
                "frame": frame, "clock_ms": 1 + 21 * frame,
                "game_tics": if in_game { 0x80000000u32 + 1043 * (frame - 161) } else { 0 },
                "game_days": if in_game { 0x121cf7 } else { 0 },
                "rng_a": 1, "rng_b": 2,
                "player_state": if in_game { 0x30 } else { 0 },
                "player_index": if in_game { 107 } else { 0 },
                "cash": if in_game { 1000 } else { 0 },
                "start_type": if in_game { 1 } else { 0 },
                "time_accel": 994, "system_counter": 0, "accel_counter": 0,
                "pixels_fnv1a64": format!("{frame:016x}"),
                "palette_fnv1a64": "0000000000000000",
            })
        })
        .collect()
}

fn jsonl(rows: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut bytes, row).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

#[test]
fn complete_synthetic_scenarios_report_exact_coverage() {
    let report = verify_trace_bytes(&jsonl(&vectors()), &jsonl(&states())).unwrap();
    assert_eq!(
        (
            report.rng_vectors,
            report.timer_vectors,
            report.state_frames
        ),
        (1536, 60, 400)
    );
    assert_eq!(
        (report.game_clock_transitions, report.new_game_frame),
        (238, 161)
    );
    assert!(!report.full_gameplay_validated);
    assert_eq!(report.vectors_sha256.len(), 64);
    assert_eq!(report.state_sha256.len(), 64);
}

#[test]
fn numeric_wrong_values_order_coverage_and_unknown_fields_are_rejected() {
    let original = vectors();
    let state = jsonl(&states());
    for (index, field, value) in [
        (0, "value", json!(0)),
        (0, "after_b", json!(1)),
        (256, "before_a", json!(0)),
        (1536, "elapsed_ms", json!(20)),
        (1537, "after_accel", json!(1)),
        (1536, "next_50hz_ms", json!(100)),
        (0, "unexpected", json!(0)),
        (0, "value", json!(65536)),
    ] {
        let mut rows = original.clone();
        rows[index][field] = value;
        assert!(
            verify_trace_bytes(&jsonl(&rows), &state).is_err(),
            "row {index}/{field}"
        );
    }
    let mut rows = original.clone();
    rows.swap(0, 1);
    assert!(verify_trace_bytes(&jsonl(&rows), &state).is_err());
    assert!(verify_trace_bytes(&jsonl(&original[..1595]), &state).is_err());
    assert!(verify_trace_bytes(b"not json\n", &state).is_err());
}

#[test]
fn state_must_reach_expected_new_game_and_match_native_clock() {
    let original = states();
    let numeric = jsonl(&vectors());
    for (index, field, value) in [
        (161, "start_type", json!(0)),
        (161, "game_tics", json!(0)),
        (200, "game_tics", json!(0)),
        (200, "clock_ms", json!(0)),
        (200, "game_days", json!(0)),
        (200, "cash", json!(0)),
        (200, "player_index", json!(0)),
        (200, "time_accel", json!(0)),
        (200, "system_counter", json!(1)),
        (200, "frame", json!(199)),
        (200, "pixels_fnv1a64", json!("not a hash")),
        (200, "start_type", json!(2)),
        (200, "extra", json!(0)),
    ] {
        let mut rows = original.clone();
        rows[index][field] = value;
        assert!(
            verify_trace_bytes(&numeric, &jsonl(&rows)).is_err(),
            "row {index}/{field}"
        );
    }
    assert!(verify_trace_bytes(&numeric, &jsonl(&original[..399])).is_err());
}

#[test]
fn trace_encoding_and_size_limits_are_enforced() {
    let numeric = jsonl(&vectors());
    let state = jsonl(&states());
    let mut invalid_utf8 = numeric.clone();
    invalid_utf8[0] = 255;
    assert!(verify_trace_bytes(&invalid_utf8, &state).is_err());
    assert!(verify_trace_bytes(&vec![b' '; 2 * 1024 * 1024 + 1], &state).is_err());
    let mut crlf = Vec::new();
    for byte in &numeric {
        if *byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(*byte);
    }
    assert!(verify_trace_bytes(&crlf, &state).is_ok());
}

#[test]
fn cli_verifies_synthetic_trace_files_and_explicitly_rejects_damage() {
    let directory = TempDir::new().unwrap();
    let vectors = directory.path().join("vectors.jsonl");
    let state = directory.path().join("state.jsonl");
    fs::write(&vectors, jsonl(&self::vectors())).unwrap();
    fs::write(&state, jsonl(&states())).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_ffe-tools"))
            .arg("verify-reference")
            .arg("--state")
            .arg(&state)
            .arg("--json")
            .arg("--vectors")
            .arg(&vectors)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    let result = invoke();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["rng_vectors"], 1536);
    assert_eq!(report["game_clock_transitions"], 238);
    fs::write(&state, b"{}\n").unwrap();
    let result = invoke();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("expected 400"));
}

#[test]
fn cli_requires_both_reference_inputs_and_refuses_incompatible_options() {
    for args in [
        vec!["verify-reference"],
        vec!["verify-reference", "--vectors", "missing"],
        vec!["verify-reference", "--vectors", "--state", "x"],
        vec![
            "verify-reference",
            "--vectors",
            "a",
            "--vectors",
            "b",
            "--state",
            "c",
        ],
        vec![
            "verify-reference",
            "--vectors",
            "a",
            "--state",
            "b",
            "--game",
            "game",
        ],
        vec![
            "verify-reference",
            "--vectors",
            "a",
            "--state",
            "b",
            "--data",
            "cache",
        ],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_ffe-tools"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
    }
}
