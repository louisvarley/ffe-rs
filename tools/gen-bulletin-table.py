#!/usr/bin/env python3
"""Generate crates/ffe-core/src/bulletin_table.rs from the original's tables.

Sources (``ffedat.asm``):
  * ``DATA_005299``: the ``0x98xx``/``0x99xx`` message pointer table. The BBS
    advertisement templates are 0x981a..0x982d; the ff80 strings they embed
    (ship-job descriptions 0x9918..0x991c, removal titles 0x99b4..0x99c3 and
    charity names 0x99d7..0x99de) are copied too.
  * ``DATA_006170``: the 32-entry generator dispatch table ``F621`` indexes
    with ``F922(0x20)``. The listing names are mapped to the Rust variants used
    by ``ffe_core::bulletin``.

The templates are kept as the original's raw bytes (0xff escape codes intact);
``bulletin::render`` expands them the way ``F1343``/``F1348`` do for the codes
the advertisements use.

Usage: tools/gen-bulletin-table.py [ffedat.asm] [out.rs]
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASM = pathlib.Path(
    sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "jjffe-master" / "ffedat.asm"
)
OUT = pathlib.Path(
    sys.argv[2] if len(sys.argv) > 2 else ROOT / "crates" / "ffe-core" / "src" / "bulletin_table.rs"
)

TEMPLATES = list(range(0x981A, 0x982E))
STRINGS = (
    # The mission reply menu (0x9833..0x983f) and the reply/completion pages
    # F521/F534/F303/F306/F307 expand (0x991d..0x9930). They live in the same
    # DATA_005299 table as the advertisements.
    list(range(0x9833, 0x9840))
    + list(range(0x9854, 0x9858))
    + list(range(0x9918, 0x991D))
    + list(range(0x991D, 0x9931))
    + list(range(0x996E, 0x997E))
    + list(range(0x99B4, 0x99C4))
    + list(range(0x99D7, 0x99DF))
)

# DATA_005104 is the 0x9cxx table F1343's ff9c handler expands: the civilian
# and military completion pages F303/F306/F307 write.
COMPLETION = list(range(0x9C29, 0x9C3B)) + list(range(0x9C67, 0x9C71))

GENERATOR_NAMES = {
    "FUNC_000639": "Goods",
    "FUNC_000640": "GoodsRandom",
    "FUNC_000636": "HistoryWanted",
    "FUNC_000637": "MissingPerson",
    "FUNC_000638": "ShipJob",
    "FUNC_000641": "Commodity",
    "FUNC_000642": "Military",
    "FUNC_000643": "Removal",
    "FUNC_000644": "Crew",
    "FUNC_000645": "Charity",
}


def pointers(lines, label):
    start = next(i for i, line in enumerate(lines) if line.strip().startswith(label + ":"))
    out = []
    for line in lines[start + 1:]:
        text = line.strip()
        if text.startswith("dd "):
            out.append(text.split()[1].split(";")[0].strip())
        elif text == "":
            continue
        else:
            break
    return out


def main():
    lines = ASM.read_text(encoding="latin-1").splitlines()
    pointers_5299 = pointers(lines, "DATA_005299")
    pointers_5104 = pointers(lines, "DATA_005104")
    assert len(pointers_5299) == 0x1E8 or len(pointers_5299) >= 0x1DF, len(pointers_5299)
    assert len(pointers_5104) >= 0x71, len(pointers_5104)
    labels = {}
    for i, line in enumerate(lines):
        name = line.strip()
        if name.startswith("DATA_") and name.endswith(":"):
            labels[name[:-1]] = i

    def raw(pointer):
        index = labels[pointer]
        out = []
        for line in lines[index + 1:]:
            text = line.strip()
            if not text.startswith("db"):
                break
            body = text[2:].split(";")[0]
            # Quoted strings may contain commas ("permit, and if so ..."), so
            # split with the quote groups as single tokens rather than on every
            # comma.
            for token in re.findall(r"'[^']*'|\"[^\"]*\"|0x[0-9A-Fa-f]+|\d+", body):
                token = token.strip()
                if token.startswith("'") or token.startswith('"'):
                    out.extend(token[1:-1].encode("latin-1"))
                elif token.lower().startswith("0x"):
                    value = int(token, 16)
                    if value == 0:
                        return bytes(out)
                    out.append(value)
                elif token:
                    value = int(token)
                    if value == 0:
                        return bytes(out)
                    out.append(value & 0xFF)
        return bytes(out)

    def rust_bytes(data):
        # Byte string with every byte escaped; keeps the ff codes visible.
        return 'b"' + "".join("\\x%02x" % byte for byte in data) + '"'

    def dwords(label, count):
        start = next(i for i, line in enumerate(lines) if line.strip().startswith(label + ":"))
        values = []
        for line in lines[start + 1:]:
            text = line.strip()
            if not text.startswith("db"):
                break
            for token in text[2:].split(";")[0].split(","):
                token = token.strip()
                if token:
                    values.append(int(token, 16) if token.startswith("0x") else int(token))
        assert len(values) >= 4 * count, (label, len(values))
        return [
            int.from_bytes(bytes(values[4 * i:4 * i + 4]), "little") for i in range(count)
        ]

    danger_money = dwords("DATA_006173", 16)
    danger_mult = dwords("DATA_006174", 16)

    dispatch_start = next(
        i for i, line in enumerate(lines) if line.strip().startswith("DATA_006170:")
    )
    dispatch = []
    for line in lines[dispatch_start + 1:]:
        text = line.strip()
        match = re.match(r"dd\s+(\w+)", text)
        if not match:
            break
        name = match.group(1)
        if name not in GENERATOR_NAMES:
            raise SystemExit(f"unknown BBS generator {name}")
        dispatch.append(GENERATOR_NAMES[name])
    assert len(dispatch) == 32, len(dispatch)

    out = [
        "//! The original's bulletin-board templates and generator table.",
        "//!",
        "//! `DATA_005299`'s 0x981a..0x982d advertisements keep their 0xff escape",
        "//! codes; `DATA_006170` is the 32-entry dispatch table F621 calls with",
        "//! `F922(0x20)`. Generated by `tools/gen-bulletin-table.py`; do not edit",
        "//! by hand.",
        "",
        "use crate::bulletin::Generator;",
        "",
        "/// `DATA_005299`'s advertisement templates, in code order.",
        "pub const TEMPLATES: [(u16, &[u8]); %d] = [" % len(TEMPLATES),
    ]
    for code in TEMPLATES:
        out.append(f"    (0x{code:04x}, {rust_bytes(raw(pointers_5299[code & 0x1FF]))}),")
    out.append("];")
    out.append("")
    out.append("/// The embedded ff80 strings the advertisement templates use.")
    out.append("pub const STRINGS: [(u16, &[u8]); %d] = [" % len(STRINGS))
    for code in STRINGS:
        out.append(f"    (0x{code:04x}, {rust_bytes(raw(pointers_5299[code & 0x1FF]))}),")
    out.append("];")
    out.append("")
    out.append("/// `DATA_005104`: the 0x9cxx completion/message pages (F303/F306/F307).")
    out.append("pub const COMPLETION: [(u16, &[u8]); %d] = [" % len(COMPLETION))
    for code in COMPLETION:
        out.append(f"    (0x{code:04x}, {rust_bytes(raw(pointers_5104[code & 0x1FF]))}),")
    out.append("];")
    out.append("")
    out.append("/// `DATA_006170`: the generator F621 selects for each of its 32 slots.")
    out.append("pub const GENERATORS: [Generator; 32] = [")
    for name in dispatch:
        out.append(f"    Generator::{name},")
    out.append("];")
    out.append("")
    out.append("/// `DATA_006173`: the extra danger money by the random draw's high nibble.")
    out.append("pub const DANGER_MONEY: [u32; 16] = [")
    out.append("    " + ", ".join(str(value) for value in danger_money) + ",")
    out.append("];")
    out.append("")
    out.append("/// `DATA_006174`: the local danger multiplier for pass/parcel rewards.")
    out.append("pub const DANGER_MULTIPLIER: [u32; 16] = [")
    out.append("    " + ", ".join(str(value) for value in danger_mult) + ",")
    out.append("];")
    out.append("")
    OUT.write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")
    print(f"{OUT}: {len(TEMPLATES)} templates, {len(STRINGS)} strings, {len(dispatch)} generators")


if __name__ == "__main__":
    raise SystemExit(main())
