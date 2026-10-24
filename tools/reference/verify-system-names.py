"""Compare ffe_core::sector's system names with the executed original.

--system-names (jjffe-smoke or jjffe-commands): F830, the name of a system code (a hand-made star from
the string table, the Gateway, or a generated pronounceable name), a table string by index and the
"(x,y)" sector form, on every hand-made star and thousands of random codes. The Rust test
(`executed_reference_system_names`) must reproduce every name; altered names and an empty trace must be
rejected. Needs data.bin (cache v22) for the syllables and the strings.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 8292
BANNER = b"JJFFE system-names-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--system-names"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--system-names reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --system-names reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--system-names traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def tampers():
    def name(where):
        def change(rows):
            i = first(rows, where)
            rows[i]["name"][0] = flip(rows[i]["name"][0])
        return change

    def count(rows):
        i = first(rows, lambda r: len(r["name"]) > 3)
        rows[i]["name"] = rows[i]["name"][:-1]

    return [
        ("hand-made name", name(lambda r: r["selector"] == 0x8400 and r["code"] == 44341016)),
        ("generated name", name(lambda r: r["selector"] == 0x8400 and r["code"] > 1_000_000_000)),
        ("table string", name(lambda r: 0 < r["selector"] < 0x1fe)),
        ("sector form", name(lambda r: r["selector"] == 0x1fe)),
        ("name length", count),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v22)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-names-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "names.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_NAME_VECTORS=str(trace),
                   FFE_NAME_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "names_map_reference",
                   "executed_reference_system_names", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty names trace accepted")
    print(json.dumps({
        "scenario": "jjffe-names-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
