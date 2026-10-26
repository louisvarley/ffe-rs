"""Execute F938/F929 and compare whole-second recharge/reserve/cargo outputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--reference", type=Path, required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[2]
outputs = [subprocess.run([str(a.reference.resolve()), "--recharge"], capture_output=True,
                         check=True, timeout=30).stdout for _ in range(2)]
assert outputs[0] == outputs[1], "source trace must repeat"
assert len(outputs[0].splitlines()) == 192
out = root / ".cache/recharge-vectors.jsonl"
out.write_bytes(outputs[0])
env = os.environ.copy()
env["FFE_RECHARGE_VECTORS"] = str(out)
subprocess.run(["cargo", "test", "-p", "ffe-rs", "--test", "fuel_reference", "--locked",
                "--", "--ignored"], cwd=root, env=env, check=True)
print(json.dumps({"scenario": "F938-F929-v1", "cases": 192,
                  "sha256": hashlib.sha256(outputs[0]).hexdigest()}, indent=2))
