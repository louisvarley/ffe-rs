"""Compare ffe_core::sector with the executed original.

--sectors (jjffe-smoke or jjffe-commands): F841 (the stars of a sector, with the F861 generator and
the hand-made sectors), F853 (star counts at several detail levels), F856 (star positions) and F833
(distances) on every hand-made sector, random sectors, vectors and points. The Rust test
(`executed_reference_sectors`) must reproduce every result; altered fields and an empty trace must be
rejected. Needs data.bin (cache v21) for the galaxy tables.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 16049
BANNER = b"JJFFE sectors-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--sectors"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--sectors reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --sectors reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--sectors traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def tampers():
    def kind(name, field, change, where=lambda r: True):
        def apply(rows):
            i = first(rows, lambda r: r["k"] == name and where(r))
            change(rows[i], field)
        return apply

    def bump(row, field):
        row[field] = flip(row[field])

    def bump_in(row, field):
        row[field][0] = flip(row[field][0])

    def star_vector(rows):
        i = first(rows, lambda r: r["k"] == "sector" and r["n"] > 3)
        rows[i]["stars"][2]["v"][3] = flip(rows[i]["stars"][2]["v"][3])

    def star_kind(rows):
        i = first(rows, lambda r: r["k"] == "sector" and r["n"] > 3)
        rows[i]["stars"][1]["kind"] = flip(rows[i]["stars"][1]["kind"])

    def star_count(rows):
        i = first(rows, lambda r: r["k"] == "sector" and r["n"] > 3)
        rows[i]["n"] = rows[i]["n"] - 1
        rows[i]["stars"] = rows[i]["stars"][:-1]

    return [
        ("star vector", star_vector),
        ("star type", star_kind),
        ("star count of a sector", star_count),
        ("hashed density", kind("count", "value", bump, lambda r: r["quality"] == 0 and r["value"] > 0)),
        ("unhashed density", kind("count", "value", bump, lambda r: r["quality"] >= 16)),
        ("position", kind("position", "out", bump_in)),
        ("distance", kind("distance", "value", bump, lambda r: r["value"] > 0)),
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
    with tempfile.TemporaryDirectory(prefix="ffe-sectors-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "sectors.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_SECTOR_VECTORS=str(trace),
                   FFE_SECTOR_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "sector_reference",
                   "executed_reference_sectors", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty sectors trace accepted")
    print(json.dumps({
        "scenario": "jjffe-sectors-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
