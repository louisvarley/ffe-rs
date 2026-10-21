"""Compare original integer projection and lazy per-instance vertex slots."""
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
    scenarios = [
        ("projection", "FFE_PROJECTION_VECTORS", 2197,
         "executed_reference_integer_projection", ["rejected", "screen"]),
        ("live-vertices", "FFE_LIVE_VERTEX_VECTORS", 8192,
         "executed_reference_live_vertex_slots",
         ["object", "view", "screen", "stamps"]),
    ]
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-instance-") as directory:
        for mode, variable, count, name, fields in scenarios:
            outputs = []
            for _ in range(2):
                result = subprocess.run([str(reference), "--" + mode], cwd=directory,
                                        capture_output=True, timeout=60)
                if result.returncode:
                    raise RuntimeError(f"{mode} reference failed ({result.returncode}): "
                                       + result.stderr.decode(errors="replace"))
                if ("JJFFE " + mode + "-v1").encode() not in result.stderr:
                    raise RuntimeError("wrong instance reference scenario")
                if len(result.stdout) > 8 * 1024 * 1024:
                    raise RuntimeError("unbounded instance trace")
                outputs.append(result.stdout)
            if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != count:
                raise RuntimeError("instance traces differ or wrong case count")
            trace = Path(directory) / (mode + ".jsonl")
            trace.write_bytes(outputs[0])
            env = dict(os.environ, **{variable: str(trace)})
            command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                       "instance_reference", name, "--", "--ignored", "--exact"]
            discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                        capture_output=True, check=True, timeout=180)
            names = [line for line in discovered.stdout.decode().splitlines()
                     if line.endswith(": test")]
            if names != [name + ": test"]:
                raise RuntimeError("instance comparison not discovered")
            subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
            rows = outputs[0].splitlines()
            for field in fields:
                # Alter a case the comparison actually reaches for this field.
                index = next(i for i, row in enumerate(rows)
                             if (json.loads(row)["rejected"] == 0
                                 if field == "screen" and mode == "projection"
                                 else True))
                row = json.loads(rows[index])
                value = row[field]
                if isinstance(value, list):
                    row[field] = [item + 1 for item in value]
                elif isinstance(value, str):
                    row[field] = "%016x" % (int(value, 16) ^ 1)
                else:
                    row[field] = value ^ 1
                modified = rows.copy()
                modified[index] = json.dumps(row).encode()
                trace.write_bytes(b"\n".join(modified) + b"\n")
                if subprocess.run(command, cwd=root, env=env, capture_output=True,
                                  timeout=300).returncode == 0:
                    raise RuntimeError("altered " + field + " accepted")
            trace.write_bytes(b"")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError("empty instance reference accepted")
            trace.write_bytes(outputs[0])
            subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
            summary[mode] = {
                "cases": count, "repeat_matches": True, "rust_matches": True,
                "all_tamper_fields_and_empty_trace_rejected": True,
                "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
            }
    summary["external_screen_random_vertices_submodels_or_pixels_validated"] = False
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
