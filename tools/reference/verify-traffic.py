"""Compare ffe_core::traffic with the executed original.

--traffic (jjffe-smoke or jjffe-commands): F650 (an indoor bay), F651, F652, F653, F654, F655
(for a ship other than the player), F657 and F867's clearance countdown, on 8,192 synthetic
station records. The Rust test (`executed_reference_traffic`) must reproduce the record after
each call and every return value; altered fields and an empty trace must be rejected.

The adapter's current sources need the Linux build (tools/reference/build-linux.sh); pass its
jjffe-smoke or jjffe-commands.
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
BANNER = b"JJFFE traffic-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--traffic"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--traffic reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --traffic reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--traffic traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def tampers():
    def out(rows, call, field, change):
        i = first(rows, lambda r: r["call"] == call)
        change(rows[i]["out"], field)

    def flip(record, field):
        record[field] ^= 1

    def flip_pad(record, field):
        record["pads"][3] ^= 1

    return [
        ("F654 mode", lambda r: out(r, 0, "mode", flip)),
        ("F650 bays", lambda r: out(r, 1, "pads", flip_pad)),
        ("F651 current", lambda r: out(r, 2, "current", flip)),
        ("F652 timeout", lambda r: out(r, 3, "timeout", flip)),
        ("F653 mode", lambda r: out(r, 4, "mode", flip)),
        ("F655 pads", lambda r: out(r, 5, "pads", flip_pad)),
        ("F867 timeout", lambda r: out(r, 7, "timeout", flip)),
        ("F654 result", lambda r: r[first(r, lambda x: x["call"] == 0)].update(
            result=1 - r[first(r, lambda x: x["call"] == 0)]["result"])),
        ("F657 result", lambda r: r[first(r, lambda x: x["call"] == 6)].update(
            result=r[first(r, lambda x: x["call"] == 6)]["result"] ^ 0xffffffff)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands built by build-linux.sh")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-traffic-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "traffic.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_TRAFFIC_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "traffic_reference",
                   "executed_reference_traffic", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty traffic trace accepted")
    print(json.dumps({
        "scenario": "jjffe-traffic-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
