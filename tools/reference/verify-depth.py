"""Compare original render-order integer depth keys, including overflow encodings."""
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
    with tempfile.TemporaryDirectory(prefix="ffe-depth-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--depth"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE depth-v1" not in result.stderr or len(result.stdout) > 4*1024*1024:
                raise RuntimeError("wrong/unbounded depth reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 17303:
            raise RuntimeError("depth results differ or wrong case count")
        trace = Path(directory) / "depth.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_DEPTH_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "depth_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        if names != ["executed_reference_render_order_depth_keys: test"]:
            raise RuntimeError("depth comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        row = json.loads(rows[0])
        row["key"] ^= 1
        trace.write_bytes(json.dumps(row).encode()+b"\n"+b"\n".join(rows[1:])+b"\n")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("altered depth key accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty depth reference accepted")
    print(json.dumps({"scenario":"jjffe-depth-v1","cases":17303,
                      "repeat_matches":True,"rust_integer_depth_keys_match":True,
                      "altered_key_and_empty_trace_rejected":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "sorting_depth_blocks_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
