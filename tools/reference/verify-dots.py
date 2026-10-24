"""Compare the particle rasteriser and the unlit point dots with the executed original.

--particle (F1610/F1611) on 6,000 centres and sizes and --dots (F1699, F1700, F1701 and the queue entries
F1800/F1801 drawn through F1789) on 6,000 unlit points, far and near path, over a prepared vertex slot.
The Rust tests (`executed_reference_particles`, `executed_reference_dots`) must reproduce every pixel
set; altered hashes, counts and an empty trace must be rejected. Needs data.bin (cache v25).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

BANNERS = {"--particle": b"JJFFE particle-v1", "--dots": b"JJFFE dots-v1"}
LINES = 6000


def run_twice(binary, directory, flag):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), flag], cwd=directory, capture_output=True, timeout=300)
        if result.returncode:
            raise RuntimeError(f"{flag} reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNERS[flag] not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError(f"wrong or unbounded {flag} reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError(f"{flag} traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-smoke or jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v25)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    digest = hashlib.sha256()
    with tempfile.TemporaryDirectory(prefix="ffe-dots-") as directory:
        particles = run_twice(reference, directory, "--particle")
        dots = run_twice(reference, directory, "--dots")
        digest.update(particles)
        digest.update(dots)
        trace_p = Path(directory) / "particle.jsonl"
        trace_d = Path(directory) / "dots.jsonl"
        trace_p.write_bytes(particles)
        trace_d.write_bytes(dots)
        env = dict(os.environ, FFE_PARTICLE_VECTORS=str(trace_p), FFE_DOT_VECTORS=str(trace_d),
                   FFE_DOT_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "dots_reference", "--",
                   "--ignored"]
        subprocess.run(command, cwd=root, env=env, check=True, timeout=900)
        rows_p = [json.loads(line) for line in particles.splitlines()]
        rows_d = [json.loads(line) for line in dots.splitlines()]

        def rejected(label, p=None, d=None):
            trace_p.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in (p or rows_p)))
            trace_d.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in (d or rows_d)))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=900).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")

        altered = json.loads(json.dumps(rows_p))
        altered[first(altered, lambda r: r["written"] > 0)]["pixels_fnv1a64"] = "0" * 16
        rejected("particle pixels", p=altered)
        altered = json.loads(json.dumps(rows_p))
        altered[first(altered, lambda r: r["written"] > 0)]["written"] += 1
        rejected("particle count", p=altered)
        altered = json.loads(json.dumps(rows_d))
        altered[first(altered, lambda r: r["written"] > 0)]["positions"] ^= 1
        rejected("dot positions", d=altered)
        altered = json.loads(json.dumps(rows_d))
        altered[first(altered, lambda r: r["centre_written"] > 0)]["centre_written"] += 1
        rejected("dot centre colour class", d=altered)
        altered = json.loads(json.dumps(rows_d))
        altered[first(altered, lambda r: r["written"] > 0)]["depth"] += 7919
        rejected("dot depth", d=altered)
        trace_p.write_bytes(b"")
        trace_d.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=900).returncode == 0:
            raise RuntimeError("empty dot traces accepted")
    print(json.dumps({
        "scenario": "jjffe-dots-v1",
        "cases": 2 * LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": digest.hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
