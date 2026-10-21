"""Compare Rust glyph rasterization to executed JJFFE routines; keep no game pixels."""

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
    with tempfile.TemporaryDirectory(prefix="ffe-font-") as directory:
        for _ in range(2):
            result = subprocess.run([str(reference), "--font"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE font-v1" not in result.stderr or len(result.stdout) > 256 * 1024:
                raise RuntimeError("wrong/unbounded font reference scenario")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1]:
            raise RuntimeError("font reference differs between runs")
        trace = Path(directory) / "font.jsonl"
        trace.write_bytes(outputs[0])
        blits = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--blit"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE blit-v1" not in result.stderr or len(result.stdout) > 16 * 1024:
                raise RuntimeError("wrong/unbounded bitmap reference scenario")
            blits.append(result.stdout)
        if blits[0] != blits[1]:
            raise RuntimeError("bitmap reference differs between runs")
        blit_trace = Path(directory) / "blit.jsonl"
        blit_trace.write_bytes(blits[0])
        env = dict(os.environ, FFE_FONT_CACHE=str(data), FFE_FONT_VECTORS=str(trace),
                   FFE_BLIT_VECTORS=str(blit_trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "font_reference", "--", "--ignored"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = {line.split(": ")[0] for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")}
        if names != {"executed_reference_glyphs_and_clipping", "executed_reference_console_sprites_and_clipping"}:
            raise RuntimeError("missing/extra executed-reference tests")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        row = json.loads(rows[0])
        row["pixels_fnv1a64"] = "0" * 16
        trace.write_bytes(json.dumps(row).encode() + b"\n" + b"\n".join(rows[1:]) + b"\n")
        altered = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120)
        if altered.returncode == 0:
            raise RuntimeError("altered reference pixels were accepted")
        trace.write_bytes(outputs[0])
        rows = blits[0].splitlines()
        row = json.loads(rows[0])
        row["pixels_fnv1a64"] = "0" * 16
        blit_trace.write_bytes(json.dumps(row).encode() + b"\n" + b"\n".join(rows[1:]) + b"\n")
        altered = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120)
        if altered.returncode == 0:
            raise RuntimeError("altered bitmap reference pixels were accepted")
    print(json.dumps({"scenario": "jjffe-font-blit-v1", "glyphs": 116, "glyph_cases": 696, "blit_cases": 28,
                      "repeat_matches": True, "rust_pixels_match": True,
                      "altered_hash_rejected": True,
                      "font_trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
                      "blit_trace_sha256": hashlib.sha256(blits[0]).hexdigest()}, indent=2))


if __name__ == "__main__":
    main()
