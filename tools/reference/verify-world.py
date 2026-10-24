"""Compare the galaxy and system-generation routines with the executed original.

--terrain-mid: the nineteen terrain midpoint routines F1849-F1867 (with F1829, F1846 and the
distance-mode vertex functions) on synthetic nodes. --names: F791 (name types 1 to 0x14). --planet: F576 (F579-F583) for a ship sphere against the collision streams of the game's models,
terrain records included. --surface-draw: F1876 in draw mode (F1833, F1834, F1868/F1869 and the emitter's arguments) on
planet records and random planets. --terrain: the terrain query F1876 in distance mode (F1823, F1833, F1834, F1875 and the arenas) on
synthetic planets. --worldgen: F880 (with F927, F1533, F879 and the orbit helpers) on generated systems at
synthetic clocks; physics object bytes with names zeroed (starports included: F879 uses the terrain
query through F574 and F575's reorienting contact). --sysgen: F875 and F878 (system generation: the hand-coded objects and 4,160 generated
or hand-coded systems; object bytes with the names zeroed). --galaxy (jjffe-commands): F868 (system index or class by distance from the
galactic centre), and for table systems F869, F870 (with the stock string left in
DATA_008790) and F871 (with the player's ranks) over every table code, the special
codes and random codes near and far. Needs the content cache (v15 system tables).
JJFFE appends eleven systems to the original's 329: codes only it knows are
skipped by the Rust test.
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
    ("worldgen", "--worldgen", b"JJFFE worldgen-v1", "FFE_WORLDGEN_VECTORS", "world_reference",
     "executed_reference_worldgen",
     ["seed", "mass", "stars", "code", "population", "low", "high", "stamp", "count", "counter",
      "system", "flags"]),
    ("sysgen", "--sysgen", b"JJFFE sysgen-v1", "FFE_SYSGEN_VECTORS", "world_reference",
     "executed_reference_sysgen",
     ["handcoded", "seed", "mass", "stars", "code", "population", "count", "ports", "stations",
      "state", "allegiance", "objects"]),
    ("terrain-mid", "--terrain-mid", b"JJFFE terrain-mid-v1", "FFE_TERRAIN_MID_VECTORS",
     "terrain_reference", "executed_reference_terrain_midpoints",
     ["which", "level", "base", "layers", "before", "after", "edge", "nodes", "edges"]),
    ("terrain", "--terrain", b"JJFFE terrain-v1", "FFE_TERRAIN_VECTORS", "terrain_reference",
     "executed_reference_terrain_distance",
     ["seed", "max", "detail", "kind", "translation", "matrix", "stream", "value", "consumed",
      "raw", "nodes", "edges"]),
    ("surface-draw", "--surface-draw", b"JJFFE surface-draw-v1", "FFE_SURFACE_VECTORS",
     "surface_draw_reference", "executed_reference_surface_draw",
     ["seed", "kind", "detail", "zoom", "origin", "vectors", "stream", "emits", "hash", "nodes",
      "edges", "first"]),
    ("planet", "--planet", b"JJFFE planet-v1", "FFE_PLANET_VECTORS", "surface_reference",
     "executed_reference_surface_hits",
     ["planet", "ship", "detail", "seed", "words", "planet_matrix", "relpos",
      "result", "flags", "more", "chunk", "distance", "altitude"]),
    ("names", "--names", b"JJFFE names-v1", "FFE_NAME_VECTORS", "names_reference",
     "executed_reference_names", ["kind", "seed", "length", "text"]),
    ("galaxy", "--galaxy", b"JJFFE galaxy-v1", "FFE_GALAXY_VECTORS", "world_reference",
     "executed_reference_galaxy", ["code", "index", "details", "market", "control"]),
]


def run(reference, flag, tag, directory):
    result = subprocess.run([str(reference), flag], cwd=directory,
                            capture_output=True, timeout=120)
    if result.returncode or tag not in result.stderr or len(result.stdout) > 1 << 26:
        raise RuntimeError(f"{flag} failed or wrong/unbounded output")
    return result.stdout


def alter(row, field):
    value = row.get(field)
    if value is None:
        return
    if isinstance(value, dict):
        key = "population" if "population" in value else next(iter(value))
        inner = value[key]
        value[key] = [v + 1 for v in inner] if isinstance(inner, list) else inner + 1
    elif isinstance(value, list) and value and isinstance(value[0], list):
        row[field] = [[(b + 1) & 0xff for b in inner] for inner in value]
    elif isinstance(value, list):
        row[field] = [v + 1 for v in value]
    elif isinstance(value, str):
        row[field] = "0" + value
    else:
        row[field] = value + 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v15)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-world-") as directory:
        for name, flag, tag, variable, test_file, test, tampers in SCENARIOS:
            trace_bytes = run(reference, flag, tag, directory)
            if trace_bytes != run(reference, flag, tag, directory):
                raise RuntimeError(f"{flag} traces differ between runs")
            trace = Path(directory) / f"{name}.jsonl"
            trace.write_bytes(trace_bytes)
            env = dict(os.environ, **{variable: str(trace), "FFE_WORLD_CACHE": str(cache)})
            command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                       test_file, test, "--", "--ignored", "--exact", "--nocapture"]
            result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                    timeout=1200)
            lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                     if line.startswith("compared")]
            if result.returncode:
                raise RuntimeError(f"Rust/original {name} mismatch:\n" +
                                   result.stderr.decode(errors="replace")[-2000:])
            rows = [json.loads(line) for line in trace_bytes.splitlines()]
            for field in tampers:
                altered = []
                for row in rows:
                    row = dict(row)
                    if isinstance(row.get(field), dict):
                        row[field] = dict(row[field])
                    alter(row, field)
                    altered.append(row)
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
                "tamper_fields_rejected": tampers,
                "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
            }
    print(json.dumps({
        "scenario": "jjffe-galaxy-v1 + jjffe-sysgen-v1 + jjffe-worldgen-v1",
        "repeat_matches": True,
        "empty_traces_rejected": True,
        **summary,
    }, indent=2))


if __name__ == "__main__":
    main()
