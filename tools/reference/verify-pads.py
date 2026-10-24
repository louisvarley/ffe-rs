"""Compare ffe_core::pads with the executed original.

--pads (jjffe-smoke): the original F650 on 8,192 synthetic station records, with
models 0x51..0x5a, bays 0..7, both class bytes and both the player's and another
ship's index. Each case fixes the ship's bay so the allocation succeeds and prints
the 24-byte out position (x, sign x, y, sign y, z, sign z), the flag and the bay.
The Rust test (`executed_reference_pads`) must reproduce the position through the
model table switch, the DATA_006180 overrun, F1669 and <<10; altered fields and an
empty trace must be rejected.

The adapter's current sources need the local Windows or Linux build
(tools/reference/build-smoke.ps1 or build-linux.sh).
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
BANNER = b"JJFFE pads-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--pads"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--pads reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --pads reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--pads traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def tampers():
    def position(rows):
        i = first(rows, lambda r: r["open_air"] == 1 and r["out"][0] != 0)
        return rows[i]["out"]

    def flip_sign(value):
        return 0 if value else -1

    return [
        ("position x", lambda r: position(r).__setitem__(0, position(r)[0] + 1)),
        ("position z", lambda r: position(r).__setitem__(4, position(r)[4] + 1)),
        ("position sign", lambda r: position(r).__setitem__(
            1, flip_sign(position(r)[1]))),
        ("open-air branch", lambda r: r[first(
            r, lambda x: x["open_air"] == 1 and x["out"] != [0, 0, 0, 0, -1, -1]
        )].update(open_air=0)),
        ("allocation flag", lambda r: r[first(r, lambda x: x["existing"] == 1)].update(
            existing=0)),
        ("bay", lambda r: r[first(r, lambda x: x["bay"] < 7)].update(
            bay=r[first(r, lambda x: x["bay"] < 7)]["bay"] + 1)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke built by build-smoke.ps1 or build-linux.sh")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-pads-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "pads.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_PADS_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "pads_reference",
                   "executed_reference_pads", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty pads trace accepted")
        trace.write_bytes(output)
        subprocess.run(command, cwd=root, env=env, check=True, timeout=600)
    print(json.dumps({
        "scenario": "jjffe-pads-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
