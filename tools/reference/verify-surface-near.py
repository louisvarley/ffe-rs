"""Check original F1876 near-pad terrain, including inside the base sphere."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--reference", type=Path, required=True)
parser.add_argument("--cache", type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
rows = []
for index in range(4):
    result = subprocess.run([str(args.reference.resolve()), "--surface-near", str(index)],
                            check=True, capture_output=True)
    case = json.loads(result.stdout)
    assert not case["fault"] and case["emits"] > 0
    rows.append(result.stdout)
output = root / ".cache" / "surface-near.jsonl"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(b"".join(rows))
env = os.environ.copy()
env["FFE_SURFACE_VECTORS"] = str(output)
env["FFE_WORLD_CACHE"] = str(args.cache.resolve())
subprocess.run(["cargo", "test", "-p", "ffe-rs", "--test", "surface_draw_reference",
                "executed_reference_near", "--locked", "--", "--ignored"],
               cwd=root, env=env, check=True)
print(json.dumps({"scenario": "near-pad-original-terrain-v1", "cases": 4,
                  "sha256": hashlib.sha256(output.read_bytes()).hexdigest()}, indent=2))
