"""Compare original depth command/group state and abstract grouped sort order."""
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
        ("depth-commands", "FFE_DEPTH_COMMAND_VECTORS", 11700,
         "executed_reference_depth_commands_and_group_changes",
         ["consumed", "active", "words", "key"]),
        ("order", "FFE_ORDER_VECTORS", 128,
         "executed_reference_depth_groups_and_sort_order", ["order", "state"]),
        ("composed-order", "FFE_COMPOSED_ORDER_VECTORS", 1,
         "executed_reference_composed_order", ["order", "keys"]),
    ]
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-order-") as directory:
        for mode, variable, count, name, fields in scenarios:
            outputs = []
            for _ in range(2):
                result = subprocess.run([str(reference), "--" + mode], cwd=directory,
                                        capture_output=True, timeout=30)
                if result.returncode:
                    raise RuntimeError(f"{mode} reference failed ({result.returncode}): "
                                       + result.stderr.decode(errors="replace"))
                if ("JJFFE " + mode + "-v1").encode() not in result.stderr:
                    raise RuntimeError("wrong order reference scenario")
                if len(result.stdout) > 4 * 1024 * 1024:
                    raise RuntimeError("unbounded order trace")
                outputs.append(result.stdout)
            if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != count:
                raise RuntimeError("order traces differ or wrong case count")
            trace = Path(directory) / (mode + ".jsonl")
            trace.write_bytes(outputs[0])
            env = dict(os.environ, **{variable: str(trace)})
            command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                       "order_reference", name, "--", "--ignored", "--exact"]
            discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                        capture_output=True, check=True, timeout=120)
            names = [line for line in discovered.stdout.decode().splitlines()
                     if line.endswith(": test")]
            if names != [name + ": test"]:
                raise RuntimeError("order comparison not discovered")
            subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
            rows = outputs[0].splitlines()
            for field in fields:
                index = (next(i for i, row in enumerate(rows)
                              if json.loads(row)["key"] is not None)
                         if field == "key" else 0)
                row = json.loads(rows[index])
                if field == "order":
                    row[field] = list(reversed(row[field]))
                    if row[field] == json.loads(rows[0])[field]:
                        raise RuntimeError("order tamper did not change the sequence")
                elif field == "keys":
                    row[field] = [value + 1 for value in row[field]]
                else:
                    row[field] = 1 if row[field] is None else row[field] ^ 1
                modified = rows.copy()
                modified[index] = json.dumps(row).encode()
                trace.write_bytes(b"\n".join(modified) + b"\n")
                if subprocess.run(command, cwd=root, env=env, capture_output=True,
                                  timeout=120).returncode == 0:
                    raise RuntimeError("altered " + field + " accepted")
            trace.write_bytes(b"")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=120).returncode == 0:
                raise RuntimeError("empty order reference accepted")
            trace.write_bytes(outputs[0])
            subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
            summary[mode] = {
                "cases": count, "repeat_matches": True, "rust_matches": True,
                "all_tamper_fields_and_empty_trace_rejected": True,
                "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
            }
    summary["lazy_vertices_outside_synthetic_composition_nested_submodels_game_primitives_or_pixels_validated"] = False
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
