"""Compare native station normal visibility/lighting to unmodified executed JJFFE routines."""
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
    with tempfile.TemporaryDirectory(prefix="ffe-normals-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--normals"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE normals-v1" not in result.stderr or len(result.stdout) > 128*1024:
                raise RuntimeError("wrong/unbounded normal reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 504:
            raise RuntimeError("normal results differ or wrong case count")
        trace = Path(directory) / "normals.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_NORMAL_CACHE=str(data), FFE_NORMAL_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "normal_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_station_normal_visibility_and_lighting: test"]:
            raise RuntimeError("normal comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        row = json.loads(rows[0])
        row["hash"] = "0"*16
        trace.write_bytes(json.dumps(row).encode()+b"\n"+b"\n".join(rows[1:])+b"\n")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("altered normal result accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty normal reference accepted")
    print(json.dumps({"scenario":"jjffe-normals-v1","cases":504,"normal_results":26712,
                      "repeat_matches":True,"rust_visibility_and_lighting_match":True,
                      "altered_hash_and_empty_trace_rejected":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "full_model_execution_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
