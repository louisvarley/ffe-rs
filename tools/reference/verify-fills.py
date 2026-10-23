"""Compare flat-face and polygon-span rasterisation with the executed original.

--fill-raster (jjffe-commands): F1604 (triangles) and F1607/F1608 (quads, with
F1602/F1603/F1601) pixels for 4096 synthetic faces. --span-raster: F1468, F1560
per edge and F1613 for 4096 synthetic closed polygons. Each case hashes a
320x800 buffer. Needs no cache.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SCENARIOS = [
    ("fill", "--fill-raster", b"JJFFE fill-raster-v1", "FFE_FILL_RASTER_VECTORS",
     "executed_reference_fill_raster"),
    ("span", "--span-raster", b"JJFFE span-raster-v1", "FFE_SPAN_RASTER_VECTORS",
     "executed_reference_span_raster"),
]
TAMPERS = ["points", "colour", "written", "bounds", "pixels_fnv1a64"]


def run(reference, flag, tag, directory):
    result = subprocess.run([str(reference), flag], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or tag not in result.stderr or len(result.stdout) > 1 << 22:
        raise RuntimeError(f"{flag} failed or wrong/unbounded output")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-fills-") as directory:
        for name, flag, tag, variable, test in SCENARIOS:
            trace_bytes = run(reference, flag, tag, directory)
            if trace_bytes != run(reference, flag, tag, directory):
                raise RuntimeError(f"{flag} traces differ between runs")
            trace = Path(directory) / f"{name}.jsonl"
            trace.write_bytes(trace_bytes)
            env = dict(os.environ, **{variable: str(trace)})
            command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                       "fill_reference", test, "--", "--ignored", "--exact", "--nocapture"]
            result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                    timeout=1200)
            lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                     if line.startswith("compared")]
            if result.returncode:
                raise RuntimeError(f"Rust/original {name} mismatch:\n" +
                                   result.stderr.decode(errors="replace")[-2000:])
            rows = [json.loads(line) for line in trace_bytes.splitlines()]
            for field in TAMPERS:
                altered = [dict(row) for row in rows]
                row = altered[777]
                if field == "points":
                    row[field] = [[x + 1 if x < 319 else x - 1, y] for x, y in row[field]]
                elif field == "pixels_fnv1a64":
                    row[field] = ("0" if row[field][0] != "0" else "1") + row[field][1:]
                elif field == "bounds":
                    row[field] = [row[field][0] + 1] + row[field][1:]
                else:
                    row[field] += 1
                trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
                if subprocess.run(command, cwd=root, env=env, capture_output=True,
                                  timeout=1200).returncode == 0:
                    raise RuntimeError(f"altered {name} field {field} accepted")
            trace.write_bytes(b"")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"empty {name} trace accepted")
            summary[name] = {
                "summary": lines,
                "tamper_fields_rejected": TAMPERS,
                "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
            }
    print(json.dumps({
        "scenario": "jjffe-fill-raster-v1 + jjffe-span-raster-v1",
        "repeat_matches": True,
        "empty_traces_rejected": True,
        **summary,
    }, indent=2))


if __name__ == "__main__":
    main()
