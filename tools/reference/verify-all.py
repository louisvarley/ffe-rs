"""Run every local reference oracle and report one PASS/FAIL/SKIP line each.

These comparisons need the player's own files and locally built reference
binaries; nothing here is part of public CI. Build once, then run this:

    cargo build --release --locked -p ffe-tools -p ffe-rs
    .\\tools\\reference\\build-smoke.ps1 -Nasm .\\.cache\\tools\\nasm-3.01\\nasm.exe
    .\\target\\release\\ffe-tools.exe build-data --game .\\game --out .\\data.bin
    python .\\tools\\reference\\verify-all.py

Each script compares the native Rust code with the executed JJFFE reference (or,
for the view oracles, with values pinned from `ffe.asm`/`ffedat.asm`) and rejects
altered results and empty traces. They do not prove gameplay fidelity: that is
the playable slice's own contract, not a model-helper check.

Exit status: 0 when every oracle passes or is environment-gated (SKIP), 1 on a
real failure, 2 when a required input is missing.
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

# (label, script, arguments). {smoke}, {commands}, {cache}, {game}, {tool} and
# {sources} are the local inputs. The instrumented adapter replaces
# primitive-emission call sites; the plain smoke adapter is the real new game.
ORACLES = [
    ("smoke", "verify-smoke.py", ["--exe", "{smoke}"]),
    ("core", "verify-core.py", ["--reference", "{smoke}", "--tool", "{tool}"]),
    ("assets", "verify-assets.py", ["--tool", "{tool}", "--game", "{game}",
                                    "--reference", "{sources}"]),
    ("font", "verify-font.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("models", "verify-models.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("normals", "verify-normals.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("transforms", "verify-transforms.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("rotation-operands", "verify-rotation-operands.py",
     ["--reference", "{smoke}", "--data", "{cache}"]),
    ("visibility", "verify-visibility.py", ["--reference", "{smoke}"]),
    ("depth", "verify-depth.py", ["--reference", "{smoke}"]),
    ("instance", "verify-instance.py", ["--reference", "{smoke}"]),
    ("jitter", "verify-jitter.py", ["--reference", "{smoke}", "--game", "{game}"]),
    ("order", "verify-order.py", ["--reference", "{smoke}"]),
    ("commands", "verify-commands.py", ["--reference", "{commands}"]),
    ("execution", "verify-execution.py", ["--reference", "{commands}", "--data", "{cache}"]),
    ("point-line", "verify-point-line.py", ["--reference", "{commands}"]),
    ("text-submodel", "verify-text-submodel.py", ["--reference", "{commands}"]),
    ("colour", "verify-colour.py", ["--reference", "{commands}"]),
    ("base-transform", "verify-base-transform.py",
     ["--reference", "{commands}", "--cache", "{cache}"]),
    ("polygon", "verify-polygon.py", ["--reference", "{commands}"]),
    ("textures", "verify-textures.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("lines", "verify-lines.py", ["--reference", "{commands}"]),
    ("fills", "verify-fills.py", ["--reference", "{commands}"]),
    ("thrust", "verify-thrust.py", ["--reference", "{commands}"]),
    ("physics", "verify-physics.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    # The Windows-safe scenarios (worldgen, terrain, planet, names, galaxy).
    ("world", "verify-world.py", ["--reference", "{commands}", "--cache", "{cache}",
                                  "--scenarios", "worldgen", "terrain-mid", "terrain",
                                  "planet", "names", "galaxy"]),
    # sysgen and surface-draw need POSIX facilities on Windows (a working
    # SIGALRM watchdog and fork/open_memstream); they run on Linux.
    ("world-linux", "verify-world.py", ["--reference", "{commands}", "--cache", "{cache}",
                                        "--scenarios", "sysgen", "surface-draw"]),
    ("station", "verify-station.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("station-queue", "verify-station-queue.py",
     ["--reference", "{commands}", "--cache", "{cache}"]),
    ("station-models", "verify-station-models.py",
     ["--reference", "{commands}", "--game", "{game}", "--cache", "{cache}"]),
    ("child-models", "verify-child-models.py",
     ["--reference", "{commands}", "--game", "{game}", "--cache", "{cache}"]),
    ("vertex-programs", "verify-vertex-programs.py",
     ["--reference", "{commands}", "--game", "{game}"]),
    ("thrusters", "verify-thrusters.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    # The same child-model comparison with the model deep enough (z 0x50000) for
    # far thruster vertices to clear F1738's 0x200 depth test.
    ("child-models (depth 0x50000)", "verify-child-models.py",
     ["--reference", "{commands}", "--game", "{game}", "--cache", "{cache}",
      "--depth", "0x50000"]),
    ("animation", "verify-animation.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("traffic", "verify-traffic.py", ["--reference", "{commands}"]),
    ("pads", "verify-pads.py", ["--reference", "{smoke}"]),
    ("market", "verify-market.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("lift", "verify-lift.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("intro", "verify-intro.py", ["--reference", "{smoke}"]),
    ("collide", "verify-collide.py", ["--reference", "{smoke}", "--cache", "{cache}"]),
    ("world-tick", "verify-world-tick.py", ["--reference", "{smoke}", "--cache", "{cache}"]),
    ("sectors", "verify-sectors.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("hyperspace", "verify-hyperspace.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("system-names", "verify-system-names.py",
     ["--reference", "{commands}", "--cache", "{cache}"]),
    ("map-stars", "verify-map-stars.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("dots", "verify-dots.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("galaxy-map", "verify-galaxy-map.py", ["--smoke", "{smoke}", "--cache", "{cache}"]),
    # Features that landed after the reference adapters: the executed original
    # where a probe exists (combat's --state-combat), and pinned asm/data oracles
    # for the rest.
    ("combat", "verify-combat.py", ["--reference", "{smoke}", "--cache", "{cache}",
                                    "--sources", "{sources}"]),
    ("ship-types", "verify-ship-types.py", ["--sources", "{sources}"]),
    ("npc", "verify-npc.py", ["--cache", "{cache}"]),
    ("dashboard", "verify-dashboard.py", ["--cache", "{cache}"]),
    ("takeoff", "verify-takeoff.py", ["--cache", "{cache}", "--sources", "{sources}"]),
    ("prefs", "verify-prefs.py", ["--sources", "{sources}"]),
]

SKIP = 77


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--reference-dir", type=Path,
                        default=Path(".cache/jjffe-reference"))
    parser.add_argument("--game", type=Path, default=Path("game"))
    parser.add_argument("--cache", type=Path, default=Path("data.bin"))
    parser.add_argument("--tool", type=Path, default=Path("target/release/ffe-tools.exe"))
    parser.add_argument("--sources", type=Path, default=Path("jjffe-master"))
    parser.add_argument("--timeout", type=int, default=3600,
                        help="per-oracle timeout in seconds (default 3600)")
    parser.add_argument("--only", nargs="*", metavar="NAME",
                        help="run only these oracles (substring match)")
    parser.add_argument("--list", action="store_true", help="print the oracle names and exit")
    args = parser.parse_args()

    root = args.root.resolve()
    paths = {
        "smoke": root / args.reference_dir / "jjffe-smoke.exe",
        "commands": root / args.reference_dir / "jjffe-commands.exe",
        "game": root / args.game,
        "cache": root / args.cache,
        "tool": root / args.tool,
        "sources": root / args.sources,
    }
    needed = {
        "smoke": "the local reference smoke build",
        "commands": "the local instrumented command probe",
        "game": "your installed game files",
        "cache": "a locally built data.bin",
        "tool": "a release ffe-tools build",
        "sources": "the supplied JJFFE sources",
    }
    if args.list:
        for label, script, _ in ORACLES:
            print(f"{label:28} {script}")
        return 0
    missing = [f"{key:9} {needed[key]} ({path})"
               for key, path in paths.items() if not path.exists()]
    if missing:
        print("Missing inputs:\n  " + "\n  ".join(missing) + "\n")
        print(__doc__.strip())
        return 2

    selected = [entry for entry in ORACLES
                if not args.only or any(name in entry[0] or name in entry[1]
                                        for name in args.only)]
    if not selected:
        print(f"no oracle matches {args.only}")
        return 2
    # Child scripts print UTF-8 and may echo paths outside the console codepage.
    env = dict(os.environ, PYTHONIOENCODING="utf-8", PYTHONUTF8="1")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    results = {"PASS": [], "SKIP": [], "FAIL": []}
    started = time.monotonic()
    for label, script, arguments in selected:
        command = [sys.executable, str(root / "tools" / "reference" / script)]
        command += [argument.format(**paths) for argument in arguments]
        oracle_started = time.monotonic()
        try:
            result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                    timeout=args.timeout)
            returncode = result.returncode
            output = (result.stdout + result.stderr).decode("utf-8", errors="replace")
        except subprocess.TimeoutExpired as error:
            returncode = 1
            output = (f"timed out after {args.timeout}s\n"
                      + (error.stdout or b"").decode("utf-8", errors="replace")[-2000:])
        elapsed = time.monotonic() - oracle_started
        if returncode == 0:
            status = "PASS"
        elif returncode == SKIP:
            status = "SKIP"
        else:
            status = "FAIL"
        results[status].append(label)
        print(f"{status} {label} ({elapsed:.1f}s)", flush=True)
        if status == "FAIL":
            for line in output.strip().splitlines()[-12:]:
                print(f"     {line}")
        elif status == "SKIP":
            reasons = [line for line in output.splitlines() if line.startswith("SKIP ")]
            for reason in reasons or ([output.strip().splitlines()[-1]]
                                      if output.strip() else []):
                print(f"     {reason}")
    elapsed = time.monotonic() - started
    print()
    print(f"{len(results['PASS'])} passed, {len(results['SKIP'])} skipped, "
          f"{len(results['FAIL'])} failed of {len(selected)} oracles in {elapsed:.1f}s")
    if results["FAIL"]:
        print("failed: " + ", ".join(results["FAIL"]))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
