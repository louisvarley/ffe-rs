"""Compare ffe_core::hyperspace with the executed original.

--hyperspace (jjffe-smoke or jjffe-commands): F903, the hyperdrive's range, fuel and jump time (the
test and the jump), for a non-player ship and the player (hydrogen or military fuel in the hold,
the radioactive waste, the cargo used and the "jump possible" flag), on 8,192 random ships, drive
classes, distances, fuel and modes. The Rust test (`executed_reference_hyperspace`) must reproduce
every output and the state left behind; altered fields and an empty trace must be rejected. Needs
data.bin (cache v21) for the drive records and the ship statistics.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 8192
BANNER = b"JJFFE hyperspace-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--hyperspace"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--hyperspace reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --hyperspace reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--hyperspace traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def tampers():
    def top(field, where=lambda r: True):
        def change(rows):
            i = first(rows, where)
            rows[i][field] = flip(rows[i][field])
        return change

    ok = lambda r: r["result"] >= 0
    return [
        ("result", top("result", ok)),
        ("fuel needed", top("fuel", ok)),
        ("range now", top("range", ok)),
        ("jump time", top("time", ok)),
        ("max range", top("max_range")),
        ("drive name", top("name")),
        ("ship fuel", top("ship_fuel", lambda r: r["player"] == 0 and r["mode"] == 2 and ok(r))),
        ("hydrogen", top("hydrogen", lambda r: r["player"] == 1 and r["class"] < 10 and r["mode"] == 2 and ok(r))),
        ("military fuel", top("military", lambda r: r["player"] == 1 and r["class"] >= 10 and r["mode"] == 2 and ok(r))),
        ("radioactive waste", top("radioactives", lambda r: r["player"] == 1 and r["class"] >= 10 and r["mode"] == 2 and ok(r))),
        ("cargo used", top("used", lambda r: r["player"] == 1 and r["class"] < 10 and r["mode"] == 2 and ok(r))),
        ("flags", top("flags", lambda r: r["player"] == 1)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v21)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-hyperspace-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "hyperspace.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_HYPER_VECTORS=str(trace),
                   FFE_HYPER_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "hyperspace_reference",
                   "executed_reference_hyperspace", "--", "--ignored", "--exact"]
        subprocess.run(command, cwd=root, env=env, check=True, timeout=600)
        rows = [json.loads(line) for line in output.splitlines()]
        for label, change in tampers():
            altered = json.loads(json.dumps(rows))
            change(altered)
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty hyperspace trace accepted")
    print(json.dumps({
        "scenario": "jjffe-hyperspace-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
