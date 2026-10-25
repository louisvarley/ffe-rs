"""Compare the Escape preference pages with the original's labelled data.

The original's page strings are `ffedat.asm` `DATA_004963`..`DATA_004967`; the
port's page tables are `ffe_rs::prefs`. There is no executable probe for a page
that needs a mouse, so this oracle decodes the original's `db` strings (with
`0xfe` position codes and `0x0d`/`0xfd` row separators) and checks that every
port label/value/music entry appears in the cited block, and that the page
titles are the original's. It also runs the five local unit tests for the page
list, defaults, space-colour ramp and Escape behaviour.

Port additions (page 5 "Port Options", "Quit" instead of "Quit to DOS", the
grey unimplemented rows) are deliberately not required of the original.

Run:

    python tools/reference/verify-prefs.py --sources jjffe-master
"""

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess

BLOCKS = {
    0: ("DATA_004963", ["Preference pages available:", "Graphic Detail",
                       "Sound Effects", "Others", "Maps"]),
    1: ("DATA_004964", ["Level of Shape Detail:", "Space Colour:"]),
    2: ("DATA_004965", ["Sound Effects:", "Choose Music From:"]),
    3: ("DATA_004966", ["Other Options:"]),
    4: ("DATA_004967", ["Map Options:"]),
}
# Port constant -> (page, minimum entries)
CONSTANTS = {
    "SHAPE_DETAIL": (1, 4),
    "SPACE_COLOURS": (1, 5),
    "SOUND_LABELS": (2, 10),
    "MUSIC": (2, 16),
    "OTHER_LABELS": (3, 10),
    "MAP_LABELS": (4, 5),
}


def block(asm: str, label: str) -> str:
    match = re.search(rf"^{label}:\s*(?:;.*)?$", asm, re.MULTILINE)
    if not match:
        raise RuntimeError(f"ffedat.asm is missing {label}")
    end = re.search(r"^[A-Za-z_][A-Za-z0-9_]*:\s*(?:;.*)?$", asm[match.end():],
                    re.MULTILINE)
    return asm[match.end():match.end() + end.start()] if end else asm[match.end():]


def db_bytes(text: str) -> bytearray:
    data = bytearray()
    for line in text.splitlines():
        line = line.split(";", 1)[0].strip()
        if not line.lower().startswith("db"):
            continue
        for match in re.finditer(r"'([^']*)'|0x([0-9a-fA-F]+)", line[2:]):
            if match.group(1) is not None:
                data += match.group(1).encode("latin-1")
            else:
                data.append(int(match.group(2), 16))
    return data


def runs(data: bytes) -> list[str]:
    """The drawn text runs: 0xfe x y is a position, 0x0d/0xfd a row."""
    out, current, index = [], bytearray(), 0

    def flush():
        text = current.decode("latin-1").strip()
        if len(text) >= 3:
            out.append(text)
        current.clear()

    while index < len(data):
        value = data[index]
        if value == 0xFE:
            flush()
            index += 3
            continue
        if value in (0x00, 0x0D, 0xFD):
            flush()
        elif 32 <= value < 127:
            current.append(value)
        else:
            flush()
        index += 1
    flush()
    return out


def constants(source: str) -> dict[str, list[str]]:
    found = {}
    for name, (page, minimum) in CONSTANTS.items():
        match = re.search(rf"pub const {name}: \[&str; \d+\] = \[(.*?)\];",
                          source, re.DOTALL)
        if not match:
            raise RuntimeError(f"prefs.rs is missing {name}")
        values = re.findall(r'"([^"]*)"', match.group(1))
        if len(values) < minimum:
            raise RuntimeError(f"prefs.rs {name} has {len(values)} entries, expected at least {minimum}")
        found[name] = values
    return found


def check(page_runs: dict[int, list[str]], port: dict[str, list[str]]) -> int:
    checks = 0
    for page, (label, titles) in BLOCKS.items():
        for title in titles:
            if title not in page_runs[page]:
                raise RuntimeError(f"{label} does not contain the page title {title!r}")
            checks += 1
    for name, (page, _) in CONSTANTS.items():
        for value in port[name]:
            if value not in page_runs[page]:
                raise RuntimeError(f"{BLOCKS[page][0]} does not contain {name} entry {value!r}")
            checks += 1
    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sources", required=True, type=Path,
                        help="jjffe-master (contains ffedat.asm)")
    parser.add_argument("--root", type=Path, default=None,
                        help="repository root (default: the script's checkout)")
    args = parser.parse_args()
    root = (args.root or Path(__file__).resolve().parents[2]).resolve()
    asm = (args.sources.resolve(strict=True) / "ffedat.asm").read_text(
        encoding="utf-8", errors="replace")
    page_runs = {page: runs(db_bytes(block(asm, label)))
                 for page, (label, _) in BLOCKS.items()}
    source = (root / "crates" / "ffe-rs" / "src" / "prefs.rs").read_text(
        encoding="utf-8", errors="replace")
    port = constants(source)
    checks = check(page_runs, port)
    # The checker must be able to fail: drop one original label and one port entry.
    broken_runs = {page: list(values) for page, values in page_runs.items()}
    broken_runs[1] = [value for value in broken_runs[1] if value != port["SPACE_COLOURS"][0]]
    try:
        check(broken_runs, port)
    except RuntimeError:
        pass
    else:
        raise RuntimeError("the preference checker accepted a missing Space Colour")
    broken_port = json.loads(json.dumps(port))
    broken_port["MAP_LABELS"][0] = "Stalks?"
    try:
        check(page_runs, broken_port)
    except RuntimeError:
        pass
    else:
        raise RuntimeError("the preference checker accepted a changed Map label")
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    result = subprocess.run([cargo, "test", "--locked", "-p", "ffe-rs", "--lib", "prefs"],
                            cwd=root, capture_output=True, timeout=600)
    output = (result.stdout + result.stderr).decode(errors="replace")
    if result.returncode:
        raise RuntimeError(f"prefs unit tests failed:\n{output[-3000:]}")
    passed = re.findall(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)
    # The six unit tests include the sound-row toggles added with the soundfx
    # merge and the captured space-colour ramp.
    if len(passed) != 6:
        raise RuntimeError(f"expected 6 prefs tests, ran {len(passed)}: {passed}")
    print(json.dumps({
        "scenario": "ffe-prefs-v1",
        "pages": {str(page): len(values) for page, values in page_runs.items()},
        "string_checks": checks,
        "constants": {name: len(values) for name, values in port.items()},
        "tests": passed,
        "tamper_rejected": True,
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
