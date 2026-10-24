//! Local-only comparison of `ffe_core::pads` with the executed original: F650's
//! open-air pad offset (`--pads`). Run through `tools/reference/verify-pads.py`.
//!
//! The 216 bytes are the decoded block at RVA 0xde20c of floppy 1.02: the five
//! `DATA_006175..6179` tables and the first six dwords of `DATA_006180`, which
//! the default table's bays 6 and 7 read. `verify-assets.py` checks the cache
//! import against the executable; this test checks the arithmetic against the
//! executed routine.
use ffe_core::pads::{Pads, MAX_BAY, PADS_BYTES};
use serde::Deserialize;

const WORDS: [i32; 54] = [
    200, 0, -200, -200, 0, -200, // DATA_006175
    262, 0, 0, -200, 0, -200, // DATA_006176
    250, 0, 250, -250, 0, 250, // DATA_006177
    -480, 0, -60, 480, 0, -60, // DATA_006178
    -840, 0, -900, 840, 0, -900, -960, 0, -560, -960, 0, 560, 0, 0, -1120, 0, 0, 1120, 960, 0,
    -560, 960, 0, 560, // DATA_006179, six pads
    23, 24, 31, 20, 33, 21, // DATA_006180 dwords 0..5 (the overrun window)
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: usize,
    model: u32,
    bay: u16,
    open_air: u8,
    player: u8,
    existing: u8,
    ship: u32,
    rows: [i32; 9],
    result: i64,
    flag: i64,
    pad: i64,
    out: [i64; 6],
}

#[test]
#[ignore = "Local-only original open-air pads; use tools/reference/verify-pads.py"]
fn executed_reference_pads() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_PADS_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20, "trace too large");
    let bytes: Vec<u8> = WORDS.iter().flat_map(|word| word.to_le_bytes()).collect();
    assert_eq!(bytes.len(), PADS_BYTES);
    let pads = Pads::from_bytes(&bytes).unwrap();
    let mut models = [0usize; 10];
    let mut bays = [0usize; 8];
    let (mut open, mut indoor, mut player, mut other) = (0, 0, 0, 0);
    let mut existing = 0;
    let mut cases = 0;
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases, "case order");
        let open_air = case.open_air != 0;
        let position = pads
            .position(case.model, case.bay, case.rows, open_air)
            .unwrap_or_else(|error| panic!("case {}: {error}", case.case));
        // The out block is x, sign(x), y, sign(y), z, sign(z).
        let got = [position[0], position[1], position[2]];
        assert_eq!(
            got,
            [case.out[0], case.out[2], case.out[4]],
            "position of case {} ({line})",
            case.case
        );
        for (axis, value) in got.iter().enumerate() {
            let sign = if *value < 0 { -1 } else { 0 };
            assert_eq!(
                case.out[2 * axis + 1],
                sign,
                "sign of axis {axis} in case {} ({line})",
                case.case
            );
        }
        // F650's allocation: the requested bay, -1 only when it was taken empty.
        assert_eq!(case.result, 0, "case {} failed to allocate", case.case);
        assert_eq!(case.pad, i64::from(case.bay), "case {} pad", case.case);
        assert_eq!(
            case.flag,
            if case.existing != 0 { 0 } else { -1 },
            "case {} flag ({line})",
            case.case
        );
        models[(case.model - 0x51) as usize] += 1;
        bays[usize::from(case.bay)] += 1;
        if open_air {
            open += 1;
        } else {
            indoor += 1;
        }
        if case.player != 0 {
            player += 1;
        } else {
            other += 1;
        }
        if case.existing != 0 {
            existing += 1;
        }
        assert!(case.bay <= MAX_BAY);
        assert!((1..=12).contains(&case.ship), "case {} ship", case.case);
        cases += 1;
    }
    eprintln!(
        "compared {cases} pad cases: models {models:?}, bays {bays:?}, open {open}/indoor {indoor}, \
         player {player}/other {other}, existing {existing}"
    );
    assert_eq!(cases, 8192);
    assert!(models.iter().all(|count| *count > 0), "{models:?}");
    assert!(bays.iter().all(|count| *count > 0), "{bays:?}");
    assert!(open > 4000 && indoor > 4000, "{open} open, {indoor} indoor");
    assert!(
        player > 4000 && other > 4000,
        "{player} player, {other} other"
    );
    assert!(existing > 4000, "{existing} existing");
}
