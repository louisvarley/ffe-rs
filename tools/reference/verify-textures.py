"""Compare textured-face rasterisation and queueing with the executed original.

--texture-raster (jjffe-smoke or jjffe-commands): F1644/F1647 pixels for 4096
synthetic cases. --texture-queue (jjffe-commands): F1708/F1712 entries per face for
every model slot in four views. Needs a v8 data.bin (descriptor table, bitmaps).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SLOTS = 466


def run(reference, args, directory, tag):
    result = subprocess.run([str(reference), *args], cwd=directory, capture_output=True,
                            timeout=60)
    if result.returncode:
        return None
    if tag not in result.stderr or len(result.stdout) > 1 << 22:
        raise RuntimeError(f"wrong/unbounded reference output for {args}")
    return result.stdout


def capture_queue(reference, directory):
    rows, crashed = [], []
    for index in range(SLOTS):
        row = run(reference, ["--texture-queue", str(index)], directory, b"JJFFE texture-queue-v1")
        if row is None:
            crashed.append(index)
        else:
            rows.append(row)
    return b"".join(rows), crashed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="v8 data.bin")
    parser.add_argument("--minimum", type=int, default=1600,
                        help="fewest model views that must be compared")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-textures-") as directory:
        raster = run(reference, ["--texture-raster"], directory, b"JJFFE texture-raster-v1")
        if raster is None or raster != run(reference, ["--texture-raster"], directory,
                                           b"JJFFE texture-raster-v1"):
            raise RuntimeError("texture-raster traces differ between runs or failed")
        queue, crashed = capture_queue(reference, directory)
        queue_again, crashed_again = capture_queue(reference, directory)
        if queue != queue_again or crashed != crashed_again:
            raise RuntimeError("texture-queue traces differ between runs")
        scenarios = [
            ("raster", raster, "FFE_TEXTURE_RASTER_VECTORS", "executed_reference_texture_raster",
             ["tex", "points", "written", "bounds", "pixels_fnv1a64"]),
            ("queue", queue, "FFE_TEXTURE_QUEUE_VECTORS", "executed_reference_texture_queue",
             ["descriptor", "colour", "point", "path", "flat"]),
        ]
        for name, trace_bytes, variable, test, tampers in scenarios:
            trace = Path(directory) / f"texture-{name}.jsonl"
            trace.write_bytes(trace_bytes)
            env = dict(os.environ, FFE_TEXTURE_CACHE=str(cache), **{variable: str(trace)},
                       FFE_TEXTURE_QUEUE_MINIMUM=str(args.minimum))
            command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                       "texture_reference", test, "--", "--ignored", "--exact", "--nocapture"]
            result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=1200)
            lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                     if line.startswith(("compared", "  model"))]
            if result.returncode:
                raise RuntimeError(f"Rust/original {name} mismatch:\n" + "\n".join(lines))
            rows = [json.loads(line) for line in trace_bytes.splitlines()]
            for field in tampers:
                altered = [dict(row) for row in rows]
                if name == "raster":
                    row = altered[777]
                    if field == "tex":
                        row[field] ^= 1 << 12 if row["kind"] == 4 else 1
                    elif field == "points":
                        row[field] = [[x + 1, y] for x, y in row[field]]
                    elif field == "pixels_fnv1a64":
                        row[field] = ("0" if row[field][0] != "0" else "1") + row[field][1:]
                    elif field == "bounds":
                        row[field] = [row[field][0] + 1] + row[field][1:]
                    else:
                        row[field] += 1
                else:
                    # First row with a textured entry (path 1) or on-screen flat entry.
                    target = next(r for r in altered if any(f[3] == 1 for f in r.get("faces", [])))
                    faces = [list(f) for f in target["faces"]]
                    face = next(f for f in faces if f[3] == 1)
                    count = face[0]
                    if field == "descriptor":
                        face[5 + count] ^= 1
                    elif field == "colour":
                        face[6 + count] ^= 1
                    elif field == "point":
                        face[5] ^= 1
                    elif field == "path":
                        face[3] = 2
                    else:
                        flat = next(f for f in faces if f[4] >= 0)
                        flat[4] ^= 1
                    target["faces"] = faces
                trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
                if subprocess.run(command, cwd=root, env=env, capture_output=True,
                                  timeout=1200).returncode == 0:
                    raise RuntimeError(f"altered {name} field {field} accepted")
            trace.write_bytes(b"")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"empty {name} trace accepted")
            trace.write_bytes(trace_bytes)
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode:
                raise RuntimeError(f"untouched {name} trace rejected")
            summary[name] = {
                "summary": lines,
                "tamper_fields_rejected": tampers,
                "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
            }
    print(json.dumps({
        "scenario": "jjffe-texture-raster-v1 + jjffe-texture-queue-v1",
        "original_crashed_slots": crashed,
        "repeat_matches": True,
        "empty_traces_rejected": True,
        **summary,
        "palette_allocation_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
