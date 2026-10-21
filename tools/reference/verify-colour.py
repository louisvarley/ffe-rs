"""Compare original F1762 colour commands (through F1772) with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CASES = 32
# (row, field, label): rows cover each program and both instance colours.
TAMPERS = [
    (0, "colour", "lit colour"), (3, "colour", "local colour"), (4, "colour", "unlit colour"),
    (7, "colour", "textured colour"), (15, "colour", "selected material"),
    (28, "normal0", "normal-0 write"), (31, "colour", "wrapped colour"),
    (12, "lines", "20-byte continuation"), (1, "alive", "alive"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-colour-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--colour"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"colour reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE colour-v1" not in result.stderr or len(result.stdout) > 65536:
                raise RuntimeError("wrong/unbounded colour reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != CASES:
            raise RuntimeError("colour traces differ or wrong line count")
        trace = Path(directory) / "colour.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_COLOUR_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "colour_reference", "executed_reference_colour_commands",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=300)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_colour_commands: test"]:
            raise RuntimeError("colour comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
        for row, field, label in TAMPERS:
            rows = [json.loads(line) for line in outputs[0].splitlines()]
            if field == "lines":
                rows[row]["lines"][0][1] ^= 1
            elif field == "alive":
                rows[row]["alive"] = 0
            else:
                rows[row][field][0] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in rows))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=300).returncode == 0:
            raise RuntimeError("empty colour reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
    print(json.dumps({
        "scenario": "jjffe-colour-v1",
        "cases": CASES,
        "repeat_matches": True,
        "rust_colour_results_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "face_shading_texture_table_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
