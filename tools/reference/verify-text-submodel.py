"""Compare original F1747/F1749/F1750 text and submodel operands with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CASES = 7
# (row, event index or None for lines/alive, label)
TAMPERS = [
    (0, 0, "text vertex"), (0, 1, "text glyph set"), (0, 2, "text frame"),
    (0, 4, "text string"), (2, 0, "submodel vertex"), (2, 1, "submodel model"),
    (2, 2, "submodel frame"), (4, 3, "extra vertex order"), (6, 3, "extra stop"),
    (1, "lines", "continuation"), (3, "alive", "alive"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-text-submodel-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--text-submodel"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"text/submodel reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE text-submodel-v1" not in result.stderr or len(result.stdout) > 8192:
                raise RuntimeError("wrong/unbounded text/submodel reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != CASES:
            raise RuntimeError("text/submodel traces differ or wrong line count")
        trace = Path(directory) / "text-submodel.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_TEXT_SUBMODEL_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "text_submodel_reference", "executed_reference_text_submodel_operands",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=300)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_text_submodel_operands: test"]:
            raise RuntimeError("text/submodel comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
        for row, index, label in TAMPERS:
            rows = [json.loads(line) for line in outputs[0].splitlines()]
            if index == "lines":
                rows[row]["lines"][0][1] ^= 1
            elif index == "alive":
                rows[row]["alive"] = 0
            else:
                rows[row]["events"][index][1] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in rows))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=300).returncode == 0:
            raise RuntimeError("empty text/submodel reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
    print(json.dumps({
        "scenario": "jjffe-text-submodel-v1",
        "cases": CASES,
        "repeat_matches": True,
        "rust_text_submodel_operands_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "glyph_child_execution_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
