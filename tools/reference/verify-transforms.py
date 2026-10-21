"""Compare original integer sine, Q31 basis rotation and trig command arithmetic."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--data", required=True, type=Path)
    args = parser.parse_args()
    reference, data = args.reference.resolve(strict=True), args.data.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-transforms-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--transforms"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE transforms-v1" not in result.stderr or len(result.stdout) > 8*1024*1024:
                raise RuntimeError("wrong/unbounded transform reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 65886:
            raise RuntimeError("transform results differ or wrong case count")
        trace = Path(directory) / "transforms.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_TRANSFORM_CACHE=str(data), FFE_TRANSFORM_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "transform_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_integer_sine_rotations_and_trig_arithmetic: test"]:
            raise RuntimeError("transform comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        for index, field in [(0, "value"), (65541, "hash"), (65676, "value")]:
            row = json.loads(rows[index])
            row[field] = "0"*16 if field == "hash" else row[field] ^ 1
            trace.write_bytes(b"\n".join(rows[:index]+[json.dumps(row).encode()]+rows[index+1:])+b"\n")
            if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
                raise RuntimeError(f"altered {row['kind']} result accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty transform reference accepted")
    print(json.dumps({"scenario":"jjffe-transforms-v1","sine_cases":65541,
                      "basis_rotations":135,"trig_arithmetic":210,
                      "repeat_matches":True,"rust_integer_results_match":True,
                      "all_three_tamper_families_and_empty_trace_rejected":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "full_model_execution_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
