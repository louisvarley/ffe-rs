"""Compare the executed original F866 geometric update, byte for byte."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
p = argparse.ArgumentParser()
p.add_argument("--reference", type=Path, required=True)
p.add_argument("--cache", type=Path, required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[2]
outputs = [subprocess.run([str(a.reference.resolve()), "--body-update"], capture_output=True,
                         check=True, timeout=30).stdout for _ in range(2)]
assert outputs[0] == outputs[1]
assert len(outputs[0].splitlines()) == 256
out = root / ".cache/body-update-vectors.jsonl"
out.write_bytes(outputs[0])
env = os.environ.copy()
env["FFE_BODY_UPDATE_VECTORS"] = str(out)
env["FFE_WORLD_CACHE"] = str(a.cache.resolve())
subprocess.run(["cargo", "test", "-p", "ffe-rs", "--test", "world_reference",
                "executed_reference_body_update", "--locked", "--", "--ignored"],
               cwd=root, env=env, check=True)
print(json.dumps({"scenario": "F866-geometry-v1", "cases": 256,
                  "sha256": hashlib.sha256(outputs[0]).hexdigest()}, indent=2))
