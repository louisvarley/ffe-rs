"""Compare packed signed angle operands and actual original rotation command handlers."""
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
    with tempfile.TemporaryDirectory(prefix="ffe-rotation-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--rotation-operands"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE rotation-operands-v1" not in result.stderr or len(result.stdout) > 8*1024*1024:
                raise RuntimeError("wrong/unbounded rotation operand reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 65671:
            raise RuntimeError("rotation results differ or wrong case count")
        trace = Path(directory) / "rotation.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_ROTATION_CACHE=str(data), FFE_ROTATION_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "rotation_operand_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_packed_angles_and_rotation_commands: test"]:
            raise RuntimeError("rotation operand comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        for index, field in [(0, "value"), (65536, "hash")]:
            row = json.loads(rows[index])
            row[field] = "0"*16 if field == "hash" else row[field] ^ 1
            trace.write_bytes(b"\n".join(rows[:index]+[json.dumps(row).encode()]+rows[index+1:])+b"\n")
            if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
                raise RuntimeError(f"altered {row['kind']} result accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty rotation operand reference accepted")
    print(json.dumps({"scenario":"jjffe-rotation-operands-v1","encoded_operands":65536,
                      "rotation_commands":135,"repeat_matches":True,
                      "rust_operand_and_command_results_match":True,
                      "both_tamper_families_and_empty_trace_rejected":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "full_transform_dispatch_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
