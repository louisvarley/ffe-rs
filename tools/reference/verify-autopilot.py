"""Run original F597/F587/F589 numeric oracle; no gameplay parity claim."""
import argparse, hashlib, json, os, subprocess
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument("--reference",type=Path,required=True)
a=p.parse_args()
root=Path(__file__).resolve().parents[2]
out=root/".cache"/"autopilot-vectors.jsonl"
out.parent.mkdir(parents=True,exist_ok=True)
r=subprocess.run([str(a.reference.resolve()),"--autopilot"],capture_output=True,check=True)
rows=[json.loads(line) for line in r.stdout.splitlines()]
assert len(rows)==512
out.write_bytes(r.stdout)
env=os.environ.copy(); env["FFE_AUTOPILOT_VECTORS"]=str(out)
subprocess.run(["cargo","test","-p","ffe-core","--test","autopilot","executed_original_guidance_and_turn_words","--locked","--","--ignored"],cwd=root,env=env,check=True)
print(json.dumps({"scenario":"source-autopilot-arithmetic-v1","cases":len(rows),"sha256":hashlib.sha256(r.stdout).hexdigest(),"covers":["F597 ordinary branch","F587","F589 same-frame"]},indent=2))
