"""Compare the galaxy and system-generation routines with the executed original.

Repeatability: the reference trace is run twice and the two runs must agree.
`--worldgen` writes each object's name bytes and the original builds part of the
name from an uninitialised byte, so Windows ASLR can make the same case read
`Testname C,D1\xe7` in one run and `...\xf3` in the next. Only the object-name
fields (`0x26..0x3f` in system records, `0x124..0x138` in physics objects) are
masked for the repeatability decision, exactly the ranges `world_reference.rs`
treats as loose/uncertain through `unspecified_name()`; the raw first run is
still what the Rust oracle compares, and every other scenario is compared byte
for byte. `--scenarios NAME...` selects a subset (verify-all splits the
Windows-safe scenarios from the Linux-only `sysgen`/`surface-draw`, which need a
working SIGALRM watchdog and POSIX `fork`/`open_memstream` respectively).

--terrain-mid: the nineteen terrain midpoint routines F1849-F1867 (with F1829, F1846 and the
distance-mode vertex functions) on synthetic nodes. --names: F791 (name types 1 to 0x14). --planet: F576 (F579-F583) for a ship sphere against the collision streams of the game's models,
terrain records included. --surface-draw: F1876 in draw mode (F1833, F1834, F1868/F1869, the emitters F1840/F1842/F1844/F1845
and their tables, F1837-F1839, F1778): each triangle handed to the emitter and the order queue it
builds, on planet records and random planets (queues compared where F1877 has nothing to set up). --terrain: the terrain query F1876 in distance mode (F1823, F1833, F1834, F1875 and the arenas) on
synthetic planets. --worldgen: F880 (with F927, F1533, F879 and the orbit helpers) on generated systems at
synthetic clocks; physics object bytes with names zeroed (starports included: F879 uses the terrain
query through F574 and F575's reorienting contact). --sysgen: F875 and F878 (system generation: the hand-coded objects and 4,160 generated
or hand-coded systems; object bytes with the names zeroed). --galaxy (jjffe-commands): F868 (system index or class by distance from the
galactic centre), and for table systems F869, F870 (with the stock string left in
DATA_008790) and F871 (with the player's ranks) over every table code, the special
codes and random codes near and far. Needs the content cache (v15 or later system
tables; this checkout builds v26).
JJFFE appends eleven systems to the original's 329: codes only it knows are
skipped by the Rust test.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
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
      "edges", "first", "word0", "light", "shades", "words", "queue_hash", "queue"]),
    ("planet", "--planet", b"JJFFE planet-v1", "FFE_PLANET_VECTORS", "surface_reference",
     "executed_reference_surface_hits",
     ["planet", "ship", "detail", "seed", "words", "planet_matrix", "relpos",
      "result", "flags", "more", "chunk", "distance", "altitude"]),
    ("names", "--names", b"JJFFE names-v1", "FFE_NAME_VECTORS", "names_reference",
     "executed_reference_names", ["kind", "seed", "length", "text"]),
    ("galaxy", "--galaxy", b"JJFFE galaxy-v1", "FFE_GALAXY_VECTORS", "world_reference",
     "executed_reference_galaxy", ["code", "index", "details", "market", "control"]),
]


def run(reference, flag, tag, directory, timeout=120):
    result = subprocess.run([str(reference), flag], cwd=directory,
                            capture_output=True, timeout=timeout)
    if result.returncode or tag not in result.stderr or len(result.stdout) > 1 << 26:
        raise RuntimeError(f"{flag} failed or wrong/unbounded output")
    return result.stdout


def normalise_worldgen(rows):
    """Mask the generated name fields for the repeatability check only.

    F877/F878 build names from an uninitialised byte, so the reference's own
    object names vary between runs under ASLR (`Testname C,D1\\xe7` one run,
    `...\\xf3` the next). `world_reference.rs` already treats those names as
    loose/uncertain (`unspecified_name()`) and masks the same fields. The raw
    first run is still what the Rust oracle compares; this normalisation is
    used only to decide whether two reference runs agree.
    """
    for row in rows:
        for object in row.get("system", []):
            object[0x26:0x3f] = [0] * (0x3f - 0x26)
        for object in row.get("objects", {}).values():
            object[0x124:0x138] = [0] * (0x138 - 0x124)
    return rows


# Scenarios whose repeatability comparison needs normalisation of fields the
# oracle itself excludes. Keep this list short and documented.
NORMALISE = {"worldgen": normalise_worldgen}


def canonical(name, rows):
    """A stable byte serialisation for the repeatability check."""
    rows = NORMALISE.get(name, lambda value: value)(copy.deepcopy(rows))
    return b"".join(json.dumps(row, sort_keys=True, separators=(",", ":")).encode() + b"\n"
                    for row in rows)


def first_difference(name, first, second):
    """A short description of the first row/field that differs, for the report."""
    if len(first) != len(second):
        return f"{name} has {len(first)} and {len(second)} rows"
    for index, (a, b) in enumerate(zip(first, second)):
        if a == b:
            continue
        fields = [key for key in sorted(set(a) | set(b)) if a.get(key) != b.get(key)]
        return f"{name} row {index} differs in {fields}"
    return f"{name} rows are equal"


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
    parser.add_argument("--scenarios", nargs="*", metavar="NAME",
                        help="run only these scenarios (default: all)")
    args = parser.parse_args()
    selected = [entry for entry in SCENARIOS
                if not args.scenarios or entry[0] in args.scenarios]
    if not selected:
        raise SystemExit(f"no scenario matches {args.scenarios}")
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    summary = {}
    skipped = []
    with tempfile.TemporaryDirectory(prefix="ffe-world-") as directory:
        for name, flag, tag, variable, test_file, test, tampers in selected:
            try:
                # The sysgen sweep is the one case that can hang the original:
                # F878 follows uninitialised stack state on some Windows/ASLR
                # layouts and the harness's SIGALRM watchdog is a no-op there
                # (win-compat.h). On Linux the watchdog records the case as
                # `hung` and the sweep continues; on Windows we time it out and
                # report the sweep as environment-gated.
                #
                # The surface-draw probe runs every case in a forked child and
                # buffers it with open_memstream; the MSVC win-compat shim makes
                # both unavailable, so the case cannot run on Windows at all.
                if name == "surface-draw" and sys.platform == "win32":
                    reason = ("jjffe-surface-draw-v1 needs POSIX fork/open_memstream "
                              "for its per-case fault catcher; win-compat.h stubs "
                              "both out on Windows. Run the suite on Linux for this "
                              "case.")
                    print(f"SKIP {name}: {reason}", flush=True)
                    skipped.append({"scenario": name, "reason": reason})
                    continue
                trace_bytes = run(reference, flag, tag, directory,
                                  timeout=60 if name == "sysgen" else 120)
                again = run(reference, flag, tag, directory,
                            timeout=60 if name == "sysgen" else 120)
            except subprocess.TimeoutExpired:
                if name == "sysgen" and sys.platform == "win32":
                    reason = ("jjffe-sysgen-v1 did not finish on Windows: F878's "
                              "uninitialised stack state can make a synthetic case "
                              "loop and the harness's SIGALRM watchdog is a no-op "
                              "in win-compat.h. Run the suite on Linux for this case.")
                    print(f"SKIP {name}: {reason}", flush=True)
                    skipped.append({"scenario": name, "reason": reason})
                    continue
                raise
            first_rows = [json.loads(line) for line in trace_bytes.splitlines()]
            second_rows = [json.loads(line) for line in again.splitlines()]
            if name in NORMALISE:
                first_canonical = canonical(name, first_rows)
                second_canonical = canonical(name, second_rows)
                if first_canonical != second_canonical:
                    raise RuntimeError(
                        f"{flag} traces differ between runs: "
                        + first_difference(name, first_rows, second_rows))
            elif trace_bytes != again:
                raise RuntimeError(f"{flag} traces differ between runs: "
                                   + first_difference(name, first_rows, second_rows))
            trace = Path(directory) / f"{name}.jsonl"
            # The Rust oracle gets the first run unchanged: its own masking,
            # not this script's, decides what is comparable.
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
            rows = first_rows
            for field in tampers:
                altered = []
                for row in rows:
                    row = copy.deepcopy(row)
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
            print(f"PASS {name}", flush=True)
    print(json.dumps({
        "scenario": "jjffe-galaxy-v1 + jjffe-sysgen-v1 + jjffe-worldgen-v1",
        "repeat_matches": True,
        "empty_traces_rejected": True,
        "skipped": skipped,
        **summary,
    }, indent=2))
    return 77 if skipped else 0


if __name__ == "__main__":
    sys.exit(main())
