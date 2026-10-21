"""Compare original F1698/F1702 point/line handler operands with Rust."""
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
    with tempfile.TemporaryDirectory(prefix="ffe-point-line-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--point-line"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"point/line reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE point-line-v1" not in result.stderr or len(result.stdout) > 4096:
                raise RuntimeError("wrong/unbounded point/line reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 1:
            raise RuntimeError("point/line traces differ or wrong line count")
        trace = Path(directory) / "point-line.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_POINT_LINE_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "point_line_reference", "executed_reference_point_line_operands",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_point_line_operands: test"]:
            raise RuntimeError("point/line comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        for index, field in [(0, "material"), (0, "angle"), (0, "vertex"),
                             (0, "flag"), (1, "material"), (1, "vertices")]:
            row = json.loads(rows[0])
            if field == "flag":
                row["events"][index][field] = 0
            elif field == "vertices":
                row["events"][index][field][0] ^= 1
            else:
                row["events"][index][field] ^= 1
            trace.write_bytes(json.dumps(row).encode() + b"\n")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=120).returncode == 0:
                raise RuntimeError(f"altered point/line {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=120).returncode == 0:
            raise RuntimeError("empty point/line reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
    print(json.dumps({
        "scenario": "jjffe-point-line-v1",
        "cases": 1,
        "events": 2,
        "repeat_matches": True,
        "rust_point_line_operands_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "geometry_clipping_shading_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
