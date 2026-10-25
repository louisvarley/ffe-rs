#!/usr/bin/env python3
"""Generate crates/ffe-core/src/shipyard_table.rs from the original's tables.

Sources:
  * ``ffedat.asm`` ``DATA_004682`` -- the ship string table ``F1345`` expands
    from the ``0x4000 | index`` codes stored in each model's statistics block.
  * ``ffedat.asm`` ``DATA_005300`` -- the 15 drive masses ``F437``'s part
    exchange subtracts.
  * ``data.bin`` asset ``base:ship/stats`` -- the 466 by 48-byte statistics
    blocks the cache's model table points at (``decode_ship_stats``). F437
    reads the unladen mass word at +0xa (the price is 1000 times it), the cargo
    word at +8, the statistics mass at +6 and the required drive at +0x14.
  * ``ffedat.asm`` ``DATA_006199``/``DATA_006200`` -- the 19 ship-type lists
    ``F634``/``F635`` draw the stock from (already mirrored by
    ``ffe_core::npc::SHIP_TYPES``; the generator only prints their union).

The prices are not a separate table: ``DATA_005268..5272`` is the array of
ship-name-list pointers F382 selects (`sh1`.., `fsh1`.., `ish1`..); the row's
Cost is F437's ``mass_at_0xa * 1000`` and Pt.Exchg is that less the trade-in
``950 * (drive_mass + mass_at_0xa - drive_mass[required_drive])``.

Usage: tools/gen-shipyard-table.py [ffedat.asm] [data.bin] [out.rs]
"""

import pathlib
import re
import struct
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASM = pathlib.Path(
    sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "jjffe-master" / "ffedat.asm"
)
DATA = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else ROOT / "data.bin")
OUT = pathlib.Path(
    sys.argv[3] if len(sys.argv) > 3 else ROOT / "crates" / "ffe-core" / "src" / "shipyard_table.rs"
)

# The first 15 entries of DATA_005300 are the drive masses; the label after them
# is DATA_005301, the ship-class string pointers.
DRIVES = 15
FIRST_MODEL = 0x0E
LAST_MODEL = 0x47


def string_table(lines, label):
    start = next(i for i, line in enumerate(lines) if line.strip().startswith(label + ":"))
    pointers = []
    for line in lines[start + 1:]:
        text = line.strip()
        if text.startswith("dd "):
            pointers.append(text.split()[1].split(";")[0].strip())
        elif text == "":
            continue
        else:
            break
    labels = {}
    for i, line in enumerate(lines):
        name = line.strip()
        if name.startswith("DATA_") and name.endswith(":"):
            labels[name[:-1]] = i

    def string(pointer):
        index = labels[pointer]
        out = []
        for line in lines[index + 1:]:
            text = line.strip()
            if not text.startswith("db"):
                break
            for token in text[2:].split(";")[0].split(","):
                token = token.strip()
                if token.startswith("'"):
                    out.extend(token.strip("'").encode("latin-1"))
                elif token.startswith("0x"):
                    value = int(token, 16)
                    if value == 0:
                        return bytes(out).decode("latin-1")
                    out.append(value)
        return bytes(out).decode("latin-1")

    return [string(pointer) for pointer in pointers]


def drive_masses(lines):
    start = next(i for i, line in enumerate(lines) if line.strip().startswith("DATA_005300:"))
    values = []
    for line in lines[start + 1:]:
        text = line.strip()
        if not text.startswith("db"):
            break
        for token in text[2:].split(";")[0].split(","):
            token = token.strip()
            if token:
                values.append(int(token, 16) if token.startswith("0x") else int(token))
    assert len(values) >= 4 * DRIVES, len(values)
    return [int.from_bytes(bytes(values[4 * i:4 * i + 4]), "little") for i in range(DRIVES)]


def main():
    lines = ASM.read_text(encoding="latin-1").splitlines()
    names = string_table(lines, "DATA_004682")
    drives = drive_masses(lines)
    with zipfile.ZipFile(DATA) as archive:
        stats = archive.read("ships/stats.bin")
    assert len(stats) == 48 * 466, len(stats)

    records = []
    for model in range(FIRST_MODEL, LAST_MODEL + 1):
        block = stats[48 * model:48 * model + 48]
        word = lambda at: int.from_bytes(block[at:at + 2], "little")
        name_code = word(0x0E)
        assert name_code & 0xC000 == 0x4000, (hex(model), hex(name_code))
        name = names[name_code & 0x3FFF]
        records.append(
            (
                model,
                name.replace("\\", "\\\\").replace('"', '\\"'),
                word(0x0A) * 1000,
                word(0x0A),
                word(0x06),
                word(0x08),
                block[0x14],
                word(0x0C),
                block[0x04],
                word(0x12),
            )
        )

    out = [
        "//! The original's shipyard data, generated from `ffedat.asm` and the",
        "//! `FIRSTENC.EXE` statistics blocks (cache asset `base:ship/stats`).",
        "//!",
        "//! F437 (`ffe.asm:30441`) lists a station's `+0x2e6` hull models; the row's",
        "//! Cost is the statistics block's `+0xa` mass word times 1000, the cargo",
        "//! capacity is `+8`, and the trade-in is F437's formula over",
        "//! `DATA_005300`. The display name is the `0x4000 | index` code at `+0xe`",
        "//! expanded through `DATA_004682` by F1345. Generated by",
        "//! `tools/gen-shipyard-table.py`; do not edit by hand.",
        "",
        "/// One flyable model's shipyard numbers, keyed by the model table slot.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct Model {",
        "    pub model: u32,",
        "    pub name: &'static str,",
        "    /// F437's Cost column: the `+0xa` word times 1000, in whole credits.",
        "    pub price: u32,",
        "    /// The `+0xa` word F437 multiplies by 1000 and the trade-in uses.",
        "    pub price_mass: u16,",
        "    /// The statistics' hull mass (`+0x6`, object `+0xe4 = mass * 4`).",
        "    pub mass: u16,",
        "    /// The `+0x8` cargo word (the Cap. column).",
        "    pub cargo: i16,",
        "    /// The `+0x14` drive's required class (`F441` then sets `ship+0xd0`).",
        "    pub drive: u8,",
        "    /// The `+0xc` shield-strength word (the View page).",
        "    pub shield: u16,",
        "    /// The `+0x4` gun mounts byte.",
        "    pub gun_mounts: i8,",
        "    /// The `+0x12` missile mounts word.",
        "    pub missiles: u16,",
        "}",
        "",
        "/// The models the shipyard tables can draw (`DATA_006184..6198`).",
        f"pub const MODELS: [Model; {len(records)}] = [",
    ]
    for model, name, price, price_mass, mass, cargo, drive, shield, guns, missiles in records:
        out.append(
            f'    Model {{ model: 0x{model:02x}, name: "{name}", price: {price}, '
            f"price_mass: {price_mass}, mass: {mass}, cargo: {cargo}, drive: {drive}, "
            f"shield: {shield}, gun_mounts: {guns}, missiles: {missiles} }},"
        )
    out.append("];")
    out.append("")
    out.append("/// `DATA_005300`'s drive masses, indexed by a drive class (tonnes).")
    out.append(f"pub const DRIVE_MASS: [u32; {DRIVES}] = [")
    out.append("    " + ", ".join(str(value) for value in drives) + ",")
    out.append("];")
    out.append("")
    text = "\n".join(out)
    OUT.write_text(text + "\n", encoding="utf-8", newline="\n")
    print(f"{OUT}: {len(records)} models, {len(drives)} drive masses")


if __name__ == "__main__":
    raise SystemExit(main())
