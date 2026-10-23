"""Compare the thrust limit routines with the executed original.

--thrust-limits (jjffe-commands): F598, F599 and F600 on 4096 synthetic limit
words and requests; the returned dwords and the stored words are compared.
Needs no cache.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

TAG = b"JJFFE thrust-limits-v1"
TAMPERS = ["function", "limits", "request", "out", "words"]


def run(reference, directory):
    result = subprocess.run([str(reference), "--thrust-limits"], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or TAG not in result.stderr or len(result.stdout) > 1 << 22:
        raise RuntimeError("--thrust-limits failed or wrong/unbounded output")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-thrust-") as directory:
        trace_bytes = run(reference, directory)
        if trace_bytes != run(reference, directory):
            raise RuntimeError("thrust traces differ between runs")
        trace = Path(directory) / "thrust.jsonl"
        trace.write_bytes(trace_bytes)
        env = dict(os.environ, FFE_THRUST_VECTORS=str(trace))
        command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                   "thrust_reference", "executed_reference_thrust_limits", "--", "--ignored",
                   "--exact", "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=1200)
        lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                 if line.startswith("compared")]
        if result.returncode:
            raise RuntimeError("Rust/original thrust mismatch:\n" +
                               result.stderr.decode(errors="replace")[-2000:])
        rows = [json.loads(line) for line in trace_bytes.splitlines()]
        for field in TAMPERS:
            # Every row is altered: one changed limit need not bind one request.
            altered = [dict(row) for row in rows]
            for row in altered:
                if field == "function":
                    row[field] = 598 if row[field] != 598 else 599
                elif isinstance(row[field], list):
                    row[field] = [value + 1 for value in row[field]]
                else:
                    row[field] += 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"altered thrust field {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=1200).returncode == 0:
            raise RuntimeError("empty thrust trace accepted")
    print(json.dumps({
        "scenario": "jjffe-thrust-limits-v1",
        "repeat_matches": True,
        "empty_trace_rejected": True,
        "summary": lines,
        "tamper_fields_rejected": TAMPERS,
        "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
