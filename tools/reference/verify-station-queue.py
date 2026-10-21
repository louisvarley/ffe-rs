"""Compare the original draw queue (face allocations and depth groups) of real stations with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CASES = 12
# (row, change, label); rows are model 74 views 0..5 then model 75 views 0..5.
TAMPERS = [
    (0, "alloc", "face key"), (7, "alloc", "face key, model 75"),
    (2, "group", "group key"), (9, "active", "group open/close"),
    (4, "drop", "missing face"), (10, "drop", "missing event"), (11, "alive", "alive"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with station programs and the sine table")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-station-queue-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--station-queue"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"station reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE station-queue-v1" not in result.stderr or len(result.stdout) > 65536:
                raise RuntimeError("wrong/unbounded station reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != CASES:
            raise RuntimeError("station traces differ or wrong line count")
        trace = Path(directory) / "station-queue.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_STATION_QUEUE_VECTORS=str(trace),
                   FFE_STATION_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "station_reference", "executed_reference_station_queues",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=300)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_station_queues: test"]:
            raise RuntimeError("station comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
        for row, field, label in TAMPERS:
            rows = [json.loads(line) for line in outputs[0].splitlines()]
            queue = rows[row]["queue"]
            allocs = [e for e in queue if e[0] == 1]
            groups = [e for e in queue if e[0] == 2 and e[1] == 1]
            if field == "alloc":
                allocs[0][2] ^= 1
            elif field == "group":
                groups[0][2] ^= 1
            elif field == "active":
                groups[0][1] = 0
            elif field == "drop":
                del queue[len(queue) // 2]
            else:
                rows[row]["alive"] = 0
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in rows))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=300).returncode == 0:
            raise RuntimeError("empty station reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
    print(json.dumps({
        "scenario": "jjffe-station-queue-v1",
        "cases": CASES,
        "repeat_matches": True,
        "rust_draw_queues_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "points_lines_glyphs_children_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
