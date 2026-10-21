"""Compare original F1741 base-relative transform forms with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CASES = 6048
# (row, field, label). Row = ((((base*3+hand)*2+form)*7+perm)*8+negation)*9
# + axis*3 + operand.
TAMPERS = [
    (0, "matrix", "identity copy"), (216, "matrix", "odd row permutation"),
    (9, "matrix", "row negation"), (1, "matrix", "rotation after copy"),
    (1008 + 216, "handedness", "odd permutation handedness"),
    (1008 + 9, "handedness", "negation handedness"),
    (3024 + 2, "matrix", "second base frame"), (3000, "advance", "record advance"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with the sine table")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-base-transform-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--base-transform"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"base-transform reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE base-transform-v1" not in result.stderr or len(result.stdout) > (4 << 20):
                raise RuntimeError("wrong/unbounded base-transform reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != CASES:
            raise RuntimeError("base-transform traces differ or wrong line count")
        trace = Path(directory) / "base-transform.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_BASE_TRANSFORM_VECTORS=str(trace),
                   FFE_BASE_TRANSFORM_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "base_transform_reference", "executed_reference_base_transforms",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=300)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_base_transforms: test"]:
            raise RuntimeError("base-transform comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
        for row, field, label in TAMPERS:
            rows = [json.loads(line) for line in outputs[0].splitlines()]
            if field == "matrix":
                rows[row]["matrix"][row % 9] ^= 1
            else:
                rows[row][field] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in rows))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=300).returncode == 0:
            raise RuntimeError("empty base-transform reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
    print(json.dumps({
        "scenario": "jjffe-base-transform-v1",
        "cases": CASES,
        "repeat_matches": True,
        "rust_base_transform_results_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "vertex_defined_basis_or_model_execution_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
