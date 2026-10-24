"""Compare ffe_core::lift and the docking stage machine with the executed original.

--lift (jjffe-smoke or jjffe-commands): the lift scene stage functions F1069-F1087 and F1086 with
the draw call F1070 makes captured, on 8,192 random stage timeouts, frame times, ships and stations.
The Rust test (`executed_reference_lift`) must reproduce the objects each stage draws (model,
view-space position, orientation, animation words), the stage state and the timeout after the call;
altered fields and an empty trace must be rejected. Needs data.bin (cache v20) for the sine table
and the ship statistics.
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
BANNER = b"JJFFE lift-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--lift"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--lift reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --lift reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--lift traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def tampers():
    def in_object(key, index=None, where=lambda o: True):
        def change(rows):
            i = first(rows, lambda r: any(where(o) for o in r["objects"]))
            for o in rows[i]["objects"]:
                if where(o):
                    if index is None:
                        o[key] = flip(o[key])
                    else:
                        o[key][index] = flip(o[key][index])
                    return
        return change

    def top(key, where=lambda r: True):
        def change(rows):
            i = first(rows, where)
            rows[i][key] = flip(rows[i][key])
        return change

    def count(rows):
        i = first(rows, lambda r: r["count"] == 2)
        rows[i]["count"] = 1

    return [
        ("model", in_object("model")),
        ("animation word", in_object("word")),
        ("depth", in_object("position", 4)),
        ("height", in_object("position", 2)),
        ("orientation", in_object("matrix", 2, lambda o: o["matrix"][2] != 0)),
        ("time word", in_object("time_word")),
        ("state after", top("after_state", lambda r: r["after_state"] != r["stage"])),
        ("timeout after", top("after_timeout")),
        ("object count", count),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v20)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-lift-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "lift.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_LIFT_VECTORS=str(trace),
                   FFE_LIFT_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "lift_reference",
                   "executed_reference_lift", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty lift trace accepted")
    print(json.dumps({
        "scenario": "jjffe-lift-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
