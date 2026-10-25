"""Compare `ffe_core::npc::SHIP_TYPES` with the original's F635 ship table.

The original's table is `DATA_006199`: nineteen `(count, pointer)` records whose
pointers are `DATA_006180`..`DATA_006198` in `jjffe-master/ffedat.asm`. This
script parses those labelled blocks (so every expected value cites the original
label), writes them as JSONL, and runs the ignored Rust test
`executed_reference_ship_table` in `crates/ffe-rs/tests/npc_reference.rs`.

F635 (`ffe.asm:43576`) selects a model from the record's model list with F922;
the table data itself is what this oracle pins. Altered counts/models/pointers
and an empty trace must be rejected.

Run:

    python tools/reference/verify-ship-types.py --sources jjffe-master
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

TABLE = "DATA_006199"


def blocks(asm: str) -> dict[str, str]:
    """Split ffedat.asm into the labelled blocks this oracle needs."""
    positions = [(match.start(), match.group(1))
                 for match in re.finditer(r"^([A-Za-z_][A-Za-z0-9_]*):", asm,
                                          re.MULTILINE)]
    found = {}
    for index, (start, label) in enumerate(positions):
        end = positions[index + 1][0] if index + 1 < len(positions) else len(asm)
        found[label] = asm[start:end]
    return found


def numbers(body: str) -> list[int]:
    """Every numeric literal in a block, comments removed."""
    body = "\n".join(line.split(";", 1)[0] for line in body.splitlines())
    return [int(value, 0)
            for value in re.findall(r"0x[0-9a-fA-F]+|\b\d+\b", body)]


def parse(asm: str) -> list[dict]:
    found = blocks(asm)
    if TABLE not in found:
        raise RuntimeError(f"ffedat.asm is missing {TABLE}")
    # The original table is written as DATA_006199 (the first count) followed by
    # DATA_006200 (the first pointer) and then interleaved count/pointer lines.
    table = found[TABLE] + found.get("DATA_006200", "")
    counts = re.findall(r"^\s*db\s+(0x[0-9a-fA-F]+)", table, re.MULTILINE)
    pointers = re.findall(r"^\s*dd\s+(DATA_0061[0-9a-fA-F]{2})", table, re.MULTILINE)
    if len(counts) != 19 or len(pointers) != 19:
        raise RuntimeError(f"F635 table has {len(counts)} counts and {len(pointers)} pointers")
    rows = []
    for index, (count, pointer) in enumerate(zip(counts, pointers)):
        if pointer not in found:
            raise RuntimeError(f"F635 0x{index:x} points at unknown {pointer}")
        declared = int(count, 0)
        models = numbers(found[pointer])
        if declared > len(models):
            raise RuntimeError(f"F635 0x{index:x} count {declared} > {len(models)} {pointer} entries")
        rows.append({"index": index, "label": pointer, "count": declared,
                     "models": models[:declared]})
    return rows


def run_test(root: Path, cargo: str, trace: Path) -> subprocess.CompletedProcess:
    env = dict(os.environ, FFE_NPC_VECTORS=str(trace))
    command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "npc_reference",
               "executed_reference_ship_table", "--", "--ignored", "--exact", "--nocapture"]
    return subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600)


def write(trace: Path, rows: list[dict]) -> bytes:
    data = b"".join(json.dumps(row).encode() + b"\n" for row in rows)
    trace.write_bytes(data)
    return data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sources", required=True, type=Path,
                        help="jjffe-master (contains ffedat.asm)")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    asm = (args.sources.resolve(strict=True) / "ffedat.asm").read_text(
        encoding="utf-8", errors="replace")
    rows = parse(asm)
    with tempfile.TemporaryDirectory(prefix="ffe-ship-types-") as directory:
        trace = Path(directory) / "ship-types.jsonl"
        trace_bytes = write(trace, rows)
        if write(trace, rows) != trace_bytes:
            raise RuntimeError("the parsed F635 table is not stable")
        result = run_test(root, cargo, trace)
        if result.returncode:
            raise RuntimeError("Rust/original F635 table mismatch:\n"
                               + result.stderr.decode(errors="replace")[-2000:])
        for label, change in (
            ("model", lambda r: r[5]["models"].__setitem__(0, r[5]["models"][0] ^ 1)),
            ("count", lambda r: r[0].__setitem__("count", r[0]["count"] + 1)),
            ("order", lambda r: r[3].__setitem__("index", r[4]["index"])),
        ):
            altered = json.loads(json.dumps(rows))
            change(altered)
            write(trace, altered)
            if run_test(root, cargo, trace).returncode == 0:
                raise RuntimeError(f"altered F635 {label} accepted")
        write(trace, [])
        if run_test(root, cargo, trace).returncode == 0:
            raise RuntimeError("empty F635 trace accepted")
    print(json.dumps({
        "scenario": "jjffe-ship-types-v1",
        "records": len(rows),
        "models": sum(row["count"] for row in rows),
        "labels": [row["label"] for row in rows],
        "repeat_matches": True,
        "rust_results_match": True,
        "altered_records_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
