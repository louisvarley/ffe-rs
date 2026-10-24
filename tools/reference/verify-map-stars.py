"""Compare the galaxy map's star light points with the executed original.

--map-stars (jjffe-commands): the real F841 builds a sector's star records and the object's star
fields, then the real F1772 runs model 0xab (opcode 30 0x801e, F1769): every stalk line, point and
label it emits, with the label depth keys. The Rust test (`executed_reference_map_stars`) runs the
same object through `ModelViewer::execute_stars` and must reproduce every call, label and key;
altered calls, labels, keys and an empty trace must be rejected. Needs data.bin (cache v23).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 240
BANNER = b"JJFFE map-stars-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--map-stars"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--map-stars reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --map-stars reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--map-stars traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def tampers():
    def point(rows):
        i = first(rows, lambda r: any(c[0] == 1 for c in r["calls"]))
        call = next(c for c in rows[i]["calls"] if c[0] == 1)
        call[1] ^= 0x10

    def line(rows):
        i = first(rows, lambda r: any(c[0] == 2 for c in r["calls"]))
        call = next(c for c in rows[i]["calls"] if c[0] == 2)
        call[3] ^= 1

    def label(rows):
        i = first(rows, lambda r: r["labels"])
        rows[i]["labels"][0][2] ^= 1 << 26

    def key(rows):
        i = first(rows, lambda r: r["keys"])
        rows[i]["keys"][0][0] ^= 1

    def dropped(rows):
        i = first(rows, lambda r: r["labels"])
        rows[i]["labels"].pop()
        rows[i]["keys"].pop()

    return [("point", point), ("stalk line", line), ("label code", label), ("label key", key),
            ("dropped label", dropped)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v23)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-map-stars-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "map-stars.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_MAP_VECTORS=str(trace),
                   FFE_MAP_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "map_stars_reference",
                   "executed_reference_map_stars", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty map star trace accepted")
    print(json.dumps({
        "scenario": "jjffe-map-stars-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
