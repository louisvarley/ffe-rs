#!/usr/bin/env python3
"""Generate crates/ffe-rs/src/console_table.rs from JJFFE's DATA_005100.

The console button table is flat data in the reference listing
(`jjffe-master/ffedat.asm:4601`): a dword bitmap base, then 82 records of
0x3e bytes.  This script collects the `db` bytes after the label (labels that
alias fields are skipped) and decodes the documented layout:

    +0x00 long  xpos
    +0x04 long  ypos
    +0x08 long  bmpindex[10]
    +0x30 long  visible
    +0x34 long  curindex
    +0x38 word  unused (1 in every record, read by nothing)
    +0x3a long  radioID

Usage:  python tools/gen-console-table.py [path/to/ffedat.asm]
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = pathlib.Path(
    sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "jjffe-master" / "ffedat.asm"
)
OUT = ROOT / "crates" / "ffe-rs" / "src" / "console_table.rs"
COUNT = 82
STRIDE = 0x3E


def main():
    lines = SOURCE.read_text(encoding="latin-1").splitlines()
    start = next(
        index for index, line in enumerate(lines) if line.strip().startswith("DATA_005100:")
    )
    data = bytearray()
    index = start + 1
    while len(data) < 4 + COUNT * STRIDE and index < len(lines):
        code = lines[index].split(";")[0].strip()
        if code.startswith("db "):
            for token in code[3:].split(","):
                token = token.strip()
                if token:
                    data.append(int(token, 16) if token.lower().startswith("0x") else int(token))
        index += 1
    if len(data) < 4 + COUNT * STRIDE:
        raise SystemExit(f"{SOURCE}: only {len(data)} bytes after DATA_005100")

    records = []
    for number in range(COUNT):
        record = data[4 + number * STRIDE : 4 + (number + 1) * STRIDE]
        records.append(
            (
                int.from_bytes(record[0:4], "little", signed=True),
                int.from_bytes(record[4:8], "little", signed=True),
                [
                    int.from_bytes(record[8 + 4 * k : 12 + 4 * k], "little")
                    for k in range(10)
                ],
                int.from_bytes(record[0x30:0x34], "little"),
                int.from_bytes(record[0x34:0x38], "little"),
                int.from_bytes(record[0x3A:0x3E], "little"),
            )
        )

    out = [
        "//! The original console button table `DATA_005100` (`ffedat.asm:4601`):",
        "//! 82 `ConButton` records of 0x3e bytes after a dword bitmap base.",
        "//!",
        "//! Decoded byte-exactly from the JJFFE 2.8a7 data listing by",
        "//! `tools/gen-console-table.py`; `tests/console_table.rs` checks the shape,",
        "//! stride, radio groups and the handful of records the flight console shows.",
        "",
        "use crate::console::Button;",
        "",
        "/// `DATA_005100`'s 82 records, in table order (index = button id).",
        "pub const BUTTONS: [Button; 82] = [",
    ]
    for number, (x, y, bitmaps, visible, current, radio) in enumerate(records):
        ids = "[" + ", ".join(str(value) for value in bitmaps) + "]"
        out.append(
            f"    Button::new({x}, {y}, {ids}, {visible}, {current}, {radio}), // 0x{number:02x}"
        )
    out += ["];", ""]
    OUT.write_text("\n".join(out), encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(records)} records)")


if __name__ == "__main__":
    main()
