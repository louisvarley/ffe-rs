"""Compare every model slot's vertex program (all supported vertex opcodes) with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SLOTS = 466
TABLES = {
    "d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72": "0x0d0cfc",
    "9957dc34aa9ef93d9a69c5f5be8e13d243046e758ad83c8a35241e31b7e6e616": "0x0d0cf8",
}


def capture(reference, directory):
    rows, crashed = [], []
    for index in range(SLOTS):
        result = subprocess.run([str(reference), "--vertex-program", str(index)],
                                cwd=directory, capture_output=True, timeout=30)
        if result.returncode:
            crashed.append(index)
            continue
        if b"JJFFE vertex-program-v1" not in result.stderr:
            raise RuntimeError(f"wrong reference output for slot {index}")
        rows.append(result.stdout)
    return b"".join(rows), crashed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--game", required=True, type=Path, help="installed game directory")
    parser.add_argument("--minimum", type=int, default=445)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    exe = next(p for p in args.game.resolve(strict=True).iterdir()
               if p.name.upper() == "FIRSTENC.EXE")
    table = TABLES.get(hashlib.sha256(exe.read_bytes()).hexdigest())
    if table is None:
        raise RuntimeError("unrecognised FIRSTENC.EXE")
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-vertex-programs-") as directory:
        first, crashed = capture(reference, directory)
        second, _ = capture(reference, directory)
        if first != second:
            raise RuntimeError("vertex-program traces differ between runs")
        trace = Path(directory) / "vertex-programs.jsonl"
        trace.write_bytes(first)
        env = dict(os.environ, FFE_VERTEX_VECTORS=str(trace), FFE_VERTEX_EXE=str(exe),
                   FFE_VERTEX_TABLE_RVA=table, FFE_VERTEX_MINIMUM=str(args.minimum))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "vertex_program_reference",
                   "executed_reference_vertex_programs", "--", "--ignored", "--exact",
                   "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600)
        summary = [l for l in result.stderr.decode(errors="replace").splitlines()
                   if l.startswith("compared")]
        if result.returncode:
            raise RuntimeError("Rust/original mismatch:\n" + result.stderr.decode(errors="replace")[-2000:])
        rows = [json.loads(line) for line in first.splitlines()]
        live = [n for n, row in enumerate(rows) if "coordinates" in row]
        for number, field in [(live[0], "coordinates"), (live[1], "state"), (live[-1], "coordinates")]:
            altered = [dict(row) for row in rows]
            if field == "state":
                altered[number][field] = [altered[number][field][0] ^ 1, altered[number][field][1]]
            else:
                value = altered[number][field]
                altered[number][field] = ("0" if value[0] != "0" else "1") + value[1:]
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered row {number} {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty vertex-program reference accepted")
    print(json.dumps({
        "scenario": "jjffe-vertex-programs-vertices-v1",
        "summary": summary,
        "original_crashed_slots": crashed,
        "repeat_matches": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(first).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
