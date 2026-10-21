"""Compare original normal/LOD visibility predicates and conditional branch outcomes."""
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
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-visibility-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--visibility"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE visibility-v1" not in result.stderr or len(result.stdout) > 512*1024:
                raise RuntimeError("wrong/unbounded visibility reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 966:
            raise RuntimeError("visibility results differ or wrong case count")
        trace = Path(directory) / "visibility.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_VISIBILITY_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "visibility_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_visibility_predicates_and_branch_outcomes: test"]:
            raise RuntimeError("visibility comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        for family in ["predicate", "branches"]:
            row = json.loads(rows[0])
            if family == "predicate":
                row[family] ^= 1
            else:
                row[family][0] = 99
            trace.write_bytes(json.dumps(row).encode()+b"\n"+b"\n".join(rows[1:])+b"\n")
            if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
                raise RuntimeError(f"altered {family} result accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty visibility reference accepted")
    print(json.dumps({"scenario":"jjffe-visibility-v1","predicates":966,"branch_outcomes":5796,
                      "repeat_matches":True,"rust_visibility_and_branch_results_match":True,
                      "both_tamper_families_and_empty_trace_rejected":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "full_model_execution_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
