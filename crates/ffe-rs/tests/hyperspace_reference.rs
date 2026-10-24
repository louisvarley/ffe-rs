//! Local-only comparison of `ffe_core::hyperspace` with the executed original: F903, the
//! hyperdrive's range, fuel and jump time (`--hyperspace`). Run through
//! `tools/reference/verify-hyperspace.py`.
use ffe_core::hyperspace::plan;
use ffe_core::sector::{Drive, Tables};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

#[test]
#[ignore = "Local-only hyperspace comparison; use tools/reference/verify-hyperspace.py"]
fn executed_reference_hyperspace() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_HYPER_CACHE").expect("FFE_HYPER_CACHE")).unwrap(),
    )
    .unwrap();
    let tables = Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let drives: Vec<Drive> = (0..15).map(|c| tables.drive(c).unwrap()).collect();
    let stats = cache.asset("base:ship/stats").unwrap();
    let mass = |model: usize| {
        i32::from(i16::from_le_bytes([
            stats[48 * model + 6],
            stats[48 * model + 7],
        ]))
    };
    let path = std::env::var_os("FFE_HYPER_VECTORS").expect("FFE_HYPER_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 32 << 20, "trace too large");
    let (mut jumps, mut refused, mut short, mut military, mut tests) = (0, 0, 0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let class = int(&case["class"]) as i32;
        let player = int(&case["player"]) == 1;
        let jump = int(&case["mode"]) == 2;
        let fuel_in = int(&case["fuel_in"]) as i32;
        let report = plan(
            &drives,
            mass(int(&case["model"]) as usize),
            class,
            int(&case["distance_in"]) as i32,
            fuel_in,
            jump,
            player,
        )
        .unwrap();
        assert_eq!(
            i64::from(report.result),
            int(&case["result"]),
            "result: {line}"
        );
        assert_eq!(
            i64::from(report.name),
            int(&case["name"]),
            "drive name: {line}"
        );
        assert_eq!(
            i64::from(report.fuel_needed),
            int(&case["fuel"]),
            "fuel needed: {line}"
        );
        assert_eq!(
            i64::from(report.range_now),
            int(&case["range"]),
            "range now: {line}"
        );
        assert_eq!(
            i64::from(report.max_range),
            int(&case["max_range"]),
            "max range: {line}"
        );
        let time = report.jump_time.map_or(0x5a5a_5a5a, i64::from);
        assert_eq!(time, int(&case["time"]), "jump time: {line}");
        // The state the call leaves behind.
        let left = fuel_in - report.consumed;
        let military_class = class >= 10;
        let (ship, hydrogen, military_fuel) = match (player, military_class) {
            (false, _) => (left, 0, 0),
            (true, true) => (0, 0, left),
            (true, false) => (0, left, 0),
        };
        assert_eq!(
            i64::from(ship as u8),
            int(&case["ship_fuel"]),
            "ship fuel: {line}"
        );
        assert_eq!(
            i64::from(hydrogen),
            int(&case["hydrogen"]),
            "hydrogen: {line}"
        );
        assert_eq!(
            i64::from(military_fuel),
            int(&case["military"]),
            "military fuel: {line}"
        );
        let waste = if player && military_class {
            report.consumed
        } else {
            0
        };
        assert_eq!(
            int(&case["radio_in"]) + i64::from(waste),
            int(&case["radioactives"]),
            "waste: {line}"
        );
        let used = int(&case["used_in"])
            - if player && !military_class {
                i64::from(report.consumed)
            } else {
                0
            };
        assert_eq!(
            used as i32,
            int(&case["used"]) as u32 as i32,
            "cargo used: {line}"
        );
        let flags_in = int(&case["flags_in"]);
        let flags = match report.flag {
            Some(true) => flags_in | 0x100,
            Some(false) => flags_in & 0xfeff,
            None => flags_in,
        };
        assert_eq!(flags, int(&case["flags"]), "flags: {line}");
        jumps += usize::from(jump && report.result >= 0);
        tests += usize::from(!jump && report.result >= 0);
        refused += usize::from(report.result < 0 && report.fuel_needed < 0);
        short += usize::from(report.result < 0 && report.fuel_needed >= 0);
        military += usize::from(player && military_class && report.consumed > 0);
    }
    eprintln!("{jumps} jumps, {tests} tests, {refused} out of range, {short} short of fuel, {military} military jumps");
    assert!(jumps > 500 && tests > 500 && refused > 500 && short > 200 && military > 100);
}
