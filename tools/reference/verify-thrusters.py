"""Compare the thruster glow queueing (F1738, F1704) with Rust on synthetic cases.

The adapter calls the original F1738 on synthetic instances whose two vertex
entries already carry the instance generation, with varied scales, widths,
depths, screen points, colours and materials, and F1704 on synthetic entry
pairs on both sides of the near plane (--thruster-cases). Real models are
compared by verify-child-models.py and verify-station-models.py.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def capture(reference, directory):
    result = subprocess.run([str(reference), "--thruster-cases"], cwd=directory,
                            capture_output=True, timeout=120)
    if result.returncode or b"JJFFE thruster-cases-v1" not in result.stderr:
        raise RuntimeError("wrong reference output for --thruster-cases")
    if len(result.stdout) > 32 << 20:
        raise RuntimeError("unbounded reference output")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--cache", required=True, type=Path,
                        help="data.bin with the texture descriptors")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-thrusters-") as directory:
        first = capture(reference, directory)
        if first != capture(reference, directory):
            raise RuntimeError("thruster traces differ between runs")
        trace = Path(directory) / "thrusters.jsonl"
        trace.write_bytes(first)
        env = dict(os.environ, FFE_THRUSTER_VECTORS=str(trace),
                   FFE_THRUSTER_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "thruster_reference",
                   "executed_reference_thruster_cases", "--", "--ignored", "--exact",
                   "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600)
        summary = [line for line in result.stderr.decode(errors="replace").splitlines()
                   if line.startswith("compared")]
        if result.returncode:
            raise RuntimeError("Rust/original mismatch:\n"
                               + result.stderr.decode(errors="replace")[-3000:])
        rows = [json.loads(line) for line in first.splitlines()]
        # Tamper: a key, a glow point, the texture word, a ramp colour, a line call's
        # colour, a line end point, a queue key; then an empty trace.
        tampers = [(41, 1), (42, 3), (43, 2), (44, 2), (46, 3), (48, 2), ("queue", 2)]
        for kind, field in tampers:
            altered = [json.loads(json.dumps(r)) for r in rows]
            number, row = next((n, r) for n, r in enumerate(altered)
                               if (r["queue"] if kind == "queue"
                                   else any(e[0] == kind for e in r["nested"])))
            if kind == "queue":
                row["queue"][0][field] ^= 1
            else:
                event = next(e for e in row["nested"] if e[0] == kind)
                event[field] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered {kind} in case {number} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty thruster reference accepted")
    print(json.dumps({
        "scenario": "jjffe-thruster-cases-v1",
        "summary": summary,
        "cases": len(rows),
        "repeat_matches": True,
        "tampered_events": [str(kind) for kind, _ in tampers],
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(first).hexdigest(),
        "pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
