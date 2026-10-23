"""Compare the physics routines with the executed original.

--accelerate (jjffe-commands): F601 and F602 (velocity from thrust over a time step) on
4096 synthetic objects in the small, medium and large time-step regimes; the
velocity dwords and fraction words are compared. Needs no cache.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

TAG = b"JJFFE accelerate-v1"
TAMPERS = ["function", "tics", "matrix", "thrust", "fraction", "velocity", "cap", "after"]


def run(reference, directory):
    result = subprocess.run([str(reference), "--accelerate"], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or TAG not in result.stderr or len(result.stdout) > 1 << 22:
        raise RuntimeError("--accelerate failed or wrong/unbounded output")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-physics-") as directory:
        trace_bytes = run(reference, directory)
        if trace_bytes != run(reference, directory):
            raise RuntimeError("physics traces differ between runs")
        trace = Path(directory) / "physics.jsonl"
        trace.write_bytes(trace_bytes)
        env = dict(os.environ, FFE_ACCELERATE_VECTORS=str(trace))
        command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                   "physics_reference", "executed_reference_accelerate", "--", "--ignored",
                   "--exact", "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=1200)
        lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                 if line.startswith("compared")]
        if result.returncode:
            raise RuntimeError("Rust/original physics mismatch:\n" +
                               result.stderr.decode(errors="replace")[-2000:])
        rows = [json.loads(line) for line in trace_bytes.splitlines()]
        for field in TAMPERS:
            # Every row is altered: one changed limit need not bind one request.
            altered = [dict(row) for row in rows]
            for row in altered:
                value = row[field]
                if field == "function":
                    row[field] = 601 if value != 601 else 602
                    continue
                if isinstance(value, dict):
                    row[field] = {**value, "velocity": [v + 1 for v in value["velocity"]]}
                elif isinstance(value, list):
                    # Only a matrix coordinate's high 16 bits matter.
                    step = 0x10000 if field == "matrix" else 1
                    row[field] = [v + step for v in value]
                else:
                    row[field] = value + 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"altered physics field {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=1200).returncode == 0:
            raise RuntimeError("empty physics trace accepted")
    print(json.dumps({
        "scenario": "jjffe-physics-limits-v1",
        "repeat_matches": True,
        "empty_trace_rejected": True,
        "summary": lines,
        "tamper_fields_rejected": TAMPERS,
        "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
