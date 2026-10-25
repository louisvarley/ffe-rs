//! F650's pad selection, the documented `DATA_006180` overrun and the indoor
//! branch. The bytes here are the 216 decoded at RVA 0xde20c of floppy 1.02
//! (the five tables and the first six dwords of the ship index table); the
//! executed comparison is `crates/ffe-rs/tests/pads_reference.rs`.
use ffe_core::pads::{Pads, INDOOR_OFFSET, MAX_BAY, PADS_BYTES};

const WORDS: [i32; 54] = [
    200, 0, -200, -200, 0, -200, // DATA_006175
    262, 0, 0, -200, 0, -200, // DATA_006176
    250, 0, 250, -250, 0, 250, // DATA_006177
    -480, 0, -60, 480, 0, -60, // DATA_006178
    -840, 0, -900, 840, 0, -900, -960, 0, -560, -960, 0, 560, 0, 0, -1120, 0, 0, 1120, 960, 0,
    -560, 960, 0, 560, // DATA_006179, six pads
    23, 24, 31, 20, 33, 21, // DATA_006180 dwords 0..5 (the overrun window)
];

fn pads() -> Pads {
    let bytes: Vec<u8> = WORDS.iter().flat_map(|word| word.to_le_bytes()).collect();
    assert_eq!(bytes.len(), PADS_BYTES);
    Pads::from_bytes(&bytes).unwrap()
}

fn identity() -> [i32; 9] {
    [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX]
}

#[test]
fn the_model_switch_is_the_original_chain() {
    // 0x56 -> DATA_006175, 0x59 -> 6176, 0x55 -> 6177, < 0x53 -> 6178,
    // everything else -> 6179 (0x53, 0x54, 0x57, 0x58 and 0x5a included).
    for (model, base) in [
        (0x50, 18),
        (0x52, 18),
        (0x56, 0),
        (0x59, 6),
        (0x55, 12),
        (0x53, 30),
        (0x54, 30),
        (0x57, 30),
        (0x58, 30),
        (0x5a, 30),
        (0x5b, 30),
        (0x00, 18),
    ] {
        assert_eq!(Pads::table(model), base, "model {model:#x}");
    }
}

#[test]
fn pads_are_three_dwords_at_the_selected_table() {
    let pads = pads();
    assert_eq!(pads.pad(0x56, 0).unwrap(), [200, 0, -200]);
    assert_eq!(pads.pad(0x56, 1).unwrap(), [-200, 0, -200]);
    assert_eq!(pads.pad(0x59, 1).unwrap(), [-200, 0, -200]);
    assert_eq!(pads.pad(0x55, 0).unwrap(), [250, 0, 250]);
    assert_eq!(pads.pad(0x52, 2).unwrap(), [-840, 0, -900]);
    assert_eq!(pads.pad(0x5a, 5).unwrap(), [960, 0, 560]);
}

#[test]
fn the_small_tables_run_into_the_next_table_but_the_last_runs_into_006180() {
    let pads = pads();
    // Table 0x6175 has only two pads; bay 7 continues through 0x6178's second pad.
    assert_eq!(pads.pad(0x56, 7).unwrap(), [480, 0, -60]);
    // The default table's bays 6 and 7 are DATA_006180's first six dwords.
    assert_eq!(pads.pad(0x5a, 6).unwrap(), [23, 24, 31]);
    assert_eq!(pads.pad(0x5a, 7).unwrap(), [20, 33, 21]);
    assert!(pads.pad(0x5a, MAX_BAY + 1).is_err());
    assert!(Pads::from_bytes(&vec![0; PADS_BYTES - 1]).is_err());
    assert!(Pads::from_bytes(&vec![0; PADS_BYTES + 1]).is_err());
}

#[test]
fn offset_turns_the_pad_and_shifts_ten_with_the_original_wrap() {
    let pads = pads();
    let rows = identity();
    // 200 * 0x7fffffff >> 31 is 199 (not 200); the original then shifts 10.
    assert_eq!(pads.offset(0x56, 0, rows).unwrap(), [203_776, 0, -204_800]);
    // A zero matrix gives zero; the indoor branch is the fixed -1 in z.
    assert_eq!(pads.offset(0x56, 0, [0; 9]).unwrap(), [0, 0, 0]);
    assert_eq!(pads.position(0x56, 0, rows, false).unwrap(), INDOOR_OFFSET);
    assert_eq!(
        pads.position(0x56, 0, rows, true).unwrap(),
        [203_776, 0, -204_800]
    );
    // The default table's bay 6 reads DATA_006180 but still goes through F1669.
    assert_eq!(
        pads.offset(0x5a, 6, rows).unwrap(),
        [22 << 10, 23 << 10, 30 << 10]
    );
}

#[test]
fn f994_lifts_the_ship_off_the_pad_along_its_up_row() {
    let pads = pads();
    let rows = identity();
    let base = [1_000, 2_000, 3_000];
    // The second row is the ship's up (F994 copies the station's rows first).
    let up = [0, i32::MAX, 0];
    // F1521's product: 5200 * 0x7fffffff >> 31 is 5199, not 5200.
    assert_eq!(
        pads.seat(base, 0x56, 0, rows, true, 5_200, up).unwrap(),
        [1_000 + 203_776, 2_000 + 5_199, 3_000 - 204_800]
    );
    // No up component in the row, no lift; the indoor branch still adds it.
    assert_eq!(
        pads.seat(base, 0x56, 0, rows, false, 5_200, [i32::MAX, 0, 0])
            .unwrap(),
        [1_000 + 5_199, 2_000, 3_000 - 1]
    );
}
