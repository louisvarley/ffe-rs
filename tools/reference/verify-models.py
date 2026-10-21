"""Compare native station vertex programs with executed JJFFE; retain no model bytes."""

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
    with tempfile.TemporaryDirectory(prefix="ffe-model-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--models"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE model-vertices-v1" not in result.stderr or len(result.stdout) > 128 * 1024:
                raise RuntimeError("wrong/unbounded model reference scenario")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 192:
            raise RuntimeError("model reference differs between runs or has wrong case count")
        trace = Path(directory) / "models.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_MODEL_CACHE=str(data), FFE_MODEL_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "model_reference", "--", "--ignored"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_station_vertex_programs: test"]:
            raise RuntimeError("model comparison test not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        row = json.loads(rows[0])
        row["coordinates_fnv1a64"] = "0" * 16
        trace.write_bytes(json.dumps(row).encode() + b"\n" + b"\n".join(rows[1:]) + b"\n")
        altered = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120)
        if altered.returncode == 0:
            raise RuntimeError("altered model coordinates were accepted")
    print(json.dumps({
        "scenario": "jjffe-model-vertices-v1", "models": [74, 75], "cases": 192,
        "vertices_compared": 13632, "repeat_matches": True, "rust_coordinates_match": True,
        "altered_hash_rejected": True, "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "drawing_commands_and_projection_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
