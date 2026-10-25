"""Compare F989's label walk and F1108's name write to executed JJFFE; keep no game pixels."""

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
    reference = args.reference.resolve(strict=True)
    data = args.data.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    outputs = []
    with tempfile.TemporaryDirectory(prefix="ffe-labels-") as directory:
        for _ in range(2):
            result = subprocess.run([str(reference), "--labels"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE labels-v1" not in result.stderr or len(result.stdout) > 64 * 1024:
                raise RuntimeError("wrong/unbounded label reference scenario")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1]:
            raise RuntimeError("label reference differs between runs")
        rows = outputs[0].splitlines()
        if len(rows) != 13:
            raise RuntimeError("unexpected label case count")
        trace = Path(directory) / "labels.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_LABEL_CACHE=str(data), FFE_LABEL_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "labels_reference", "--", "--ignored"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = {line.split(": ")[0] for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")}
        if names != {"executed_reference_label_walk_positions_and_pixels"}:
            raise RuntimeError("missing/extra executed-reference tests")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)

        def altered(field, value, row_index=0):
            changed = [json.loads(row) for row in rows]
            changed[row_index][field] = value
            trace.write_bytes(b"".join(json.dumps(row).encode() + b"\n" for row in changed))
            result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120)
            if result.returncode == 0:
                raise RuntimeError(f"altered label {field} was accepted")
            trace.write_bytes(outputs[0])

        altered("bbox", [0, 0, 0, 0])
        altered("pixels_fnv1a64", "0" * 16)
        altered("callback", [0, 0])
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty label trace was accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
    print(json.dumps({"scenario": "jjffe-labels-v1", "cases": 13,
                      "repeat_matches": True, "rust_positions_and_pixels_match": True,
                      "altered_bbox_rejected": True, "altered_hash_rejected": True,
                      "altered_callback_rejected": True, "empty_trace_rejected": True,
                      "label_trace_sha256": hashlib.sha256(outputs[0]).hexdigest()}, indent=2))


if __name__ == "__main__":
    main()
