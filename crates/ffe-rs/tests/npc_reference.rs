//! Local-only F635 ship-type oracle: the nineteen `(count, model list)` records
//! of the original's `DATA_006199`/`DATA_006180`..`DATA_006198` tables, parsed
//! from `jjffe-master/ffedat.asm` and compared with `ffe_core::npc::SHIP_TYPES`.
//!
//! Run through `tools/reference/verify-ship-types.py`, which extracts the table
//! (with its cited labels) into `FFE_NPC_VECTORS` and tampers with it. The data
//! is the JJFFE 2.8a7 source that the reference binary is assembled from; the
//! DOS release's `FIRSTENC.EXE` has the same table (`docs/notes/npc.md`).

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShipType {
    index: usize,
    label: String,
    count: usize,
    models: Vec<u32>,
}

#[test]
#[ignore = "Local-only original ship table; use tools/reference/verify-ship-types.py"]
fn executed_reference_ship_table() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_NPC_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 1 << 20);
    let mut records = 0;
    let mut models = 0;
    for line in input.lines() {
        let case: ShipType = serde_json::from_str(line).unwrap();
        assert_eq!(case.index, records, "F635 table order");
        let table = ffe_core::npc::SHIP_TYPES
            .get(case.index)
            .unwrap_or_else(|| panic!("missing F635 table {}", case.index));
        assert_eq!(
            table.len(),
            case.count,
            "F635 count at {} ({}); ffedat.asm {} says {}",
            case.index,
            case.label,
            case.label,
            case.count
        );
        assert_eq!(
            *table,
            case.models.as_slice(),
            "F635 models at {} ({}), ffedat.asm {}",
            case.index,
            case.label,
            case.label
        );
        records += 1;
        models += table.len();
    }
    assert_eq!(records, 19, "F635's nineteen records");
    eprintln!("compared {records} F635 ship-type records ({models} model slots)");
}
