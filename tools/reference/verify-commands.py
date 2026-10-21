"""Verify bounded native command helpers, not full station execution or rasterization."""
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
    with tempfile.TemporaryDirectory(prefix="ffe-commands-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--commands"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE commands-v1" not in result.stderr or len(result.stdout) > 256 * 1024:
                raise RuntimeError("wrong/unbounded command reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1]:
            raise RuntimeError("command reference differs between runs")
        rows = [json.loads(line) for line in outputs[0].splitlines()]
        counts = {kind: sum(row["kind"] == kind for row in rows)
                  for kind in ("math", "branch", "material", "face")}
        if counts["branch"] != 360 or counts["material"] != 1 or counts["face"] != 32:
            raise RuntimeError("wrong reference case coverage")
        trace = Path(directory) / "commands.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_COMMAND_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "command_reference", "--", "--ignored"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_command_arithmetic_branches_materials_and_faces: test"]:
            raise RuntimeError("command comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        # Exercise each result family, and ensure missing cases cannot pass as an empty comparison.
        for kind, field, value in [("math", "result", None), ("branch", "relative", 999),
                                   ("material", "hash", "0"*16), ("face", "faces", [])]:
            altered_rows = [dict(row) for row in rows]
            row = next(row for row in altered_rows if row["kind"] == kind)
            row[field] = row[field] ^ 1 if value is None else value
            trace.write_text("".join(json.dumps(row)+"\n" for row in altered_rows), encoding="ascii")
            altered = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120)
            if altered.returncode == 0:
                raise RuntimeError(f"altered {kind} result accepted")
        trace.write_text("", encoding="ascii")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty command reference accepted")
    print(json.dumps({"scenario": "jjffe-commands-v1", "cases": counts,
                      "material_descriptors": 65536, "repeat_matches": True,
                      "altered_results_and_empty_trace_rejected": True,
                      "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
                      "full_model_execution_or_pixels_validated": False}, indent=2))


if __name__ == "__main__":
    main()
