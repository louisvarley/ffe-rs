"""Compare one whole player tick in a world (F1041, F601/F602, F1512, F569/F570) with the executed original.

--world-tick (jjffe-smoke or jjffe-commands): a whole tick of the original for
the player as the last body of a synthetic seven-body tree with ffp masses,
radii, rotating bytes, orientations and a stale DATA_008704. The Rust test
(`executed_reference_world_tick`) must reproduce the player's matrix, rates,
thrust, caps, fractions, countdown, scanner, auto-roll, gravity, flags, every
body, the contact flags/object/pad/distance and both distance registers for all
4,096 cases, including the contacts. Needs data.bin (cache v17+) for the models'
collision streams, records and statistics. Altered fields and an empty trace must
be rejected.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 4096
BANNER = b"JJFFE world-tick-v1"
FIELDS = ["time", "matrix", "rate", "drift", "bodies", "after", "contact", "player", "keys", "tics"]


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--world-tick"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--world-tick reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (112 << 20):
            raise RuntimeError("wrong or unbounded --world-tick reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--world-tick traces differ or have the wrong line count")
    return outputs[0]


def bump(value):
    if isinstance(value, bool):
        return not value
    if isinstance(value, int):
        return value + 1
    if isinstance(value, list):
        if not value:
            return value
        out = list(value)
        out[0] = bump(out[0])
        return out
    if isinstance(value, dict):
        out = dict(value)
        for key in out:
            if isinstance(out[key], (int, list, dict)) and not isinstance(out[key], bool):
                out[key] = bump(out[key])
                break
        return out
    raise RuntimeError(f"cannot alter {value!r}")


def alter(row, field):
    """Change one field of every row (one changed input need not bind one case)."""
    if field == "bodies":
        for body in row["bodies"]:
            body["before"]["position"][0] += 1
        return
    row[field] = bump(row[field])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path,
                        help="data.bin (the models' collision streams, records and statistics)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-tick-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "tick.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_TICK_VECTORS=str(trace), FFE_WORLD_CACHE=str(cache))
        command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                   "physics_reference", "executed_reference_world_tick", "--", "--ignored", "--exact"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=1200)
        if result.returncode:
            raise RuntimeError("Rust/original world-tick mismatch:\n"
                               + result.stderr.decode(errors="replace")[-2000:])
        rows = [json.loads(line) for line in output.splitlines()]
        for field in FIELDS:
            altered = [dict(row) for row in rows]
            for row in altered:
                alter(row, field)
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"altered world-tick field {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=1200).returncode == 0:
            raise RuntimeError("empty world-tick trace accepted")
    print(json.dumps({
        "scenario": "jjffe-world-tick-v1",
        "cases": LINES,
        "contacts": sum(1 for row in rows if row["contact"]["flags"] & 0xc0),
        "repeat_matches": True,
        "rust_results_match": True,
        "tamper_fields_rejected": FIELDS,
        "empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
