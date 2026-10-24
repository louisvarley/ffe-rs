"""Run every local reference oracle and report one summary.

These comparisons need the player's own files and locally built reference
binaries; nothing here is part of public CI. Build once, then run this:

    cargo build --release --locked -p ffe-tools -p ffe-rs
    .\\tools\\reference\\build-smoke.ps1 -Nasm .\\.cache\\tools\\nasm-3.01\\nasm.exe
    .\\target\\release\\ffe-tools.exe build-data --game .\\game --out .\\data.bin
    python .\\tools\\reference\\verify-all.py

Each script compares the native Rust code with the executed JJFFE reference and
rejects altered results and empty traces. They do not prove gameplay fidelity:
that is the playable slice's own contract, not a model-helper check.
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path

# (script, arguments). {smoke} and {commands} are the two reference adapters:
# the instrumented one replaces primitive-emission call sites.
ORACLES = [
    ("verify-smoke.py", ["--exe", "{smoke}"]),
    ("verify-core.py", ["--reference", "{smoke}", "--tool", "{tool}"]),
    ("verify-assets.py", ["--tool", "{tool}", "--game", "{game}",
                          "--reference", "{sources}"]),
    ("verify-font.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("verify-models.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("verify-normals.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("verify-transforms.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("verify-rotation-operands.py", ["--reference", "{smoke}", "--data", "{cache}"]),
    ("verify-visibility.py", ["--reference", "{smoke}"]),
    ("verify-depth.py", ["--reference", "{smoke}"]),
    ("verify-instance.py", ["--reference", "{smoke}"]),
    ("verify-jitter.py", ["--reference", "{smoke}", "--game", "{game}"]),
    ("verify-order.py", ["--reference", "{smoke}"]),
    ("verify-commands.py", ["--reference", "{commands}"]),
    ("verify-execution.py", ["--reference", "{commands}", "--data", "{cache}"]),
    ("verify-point-line.py", ["--reference", "{commands}"]),
    ("verify-text-submodel.py", ["--reference", "{commands}"]),
    ("verify-colour.py", ["--reference", "{commands}"]),
    ("verify-base-transform.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-polygon.py", ["--reference", "{commands}"]),
    ("verify-textures.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-lines.py", ["--reference", "{commands}"]),
    ("verify-fills.py", ["--reference", "{commands}"]),
    ("verify-thrust.py", ["--reference", "{commands}"]),
    ("verify-physics.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-world.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-station.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-station-queue.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-station-models.py", ["--reference", "{commands}", "--game", "{game}",
                                  "--cache", "{cache}"]),
    ("verify-child-models.py", ["--reference", "{commands}", "--game", "{game}",
                                "--cache", "{cache}"]),
    ("verify-vertex-programs.py", ["--reference", "{commands}", "--game", "{game}"]),
    ("verify-thrusters.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    # The same child-model comparison with the model deep enough (z 0x50000) for
    # far thruster vertices to clear F1738's 0x200 depth test.
    ("verify-child-models.py", ["--reference", "{commands}", "--game", "{game}",
                                "--cache", "{cache}", "--depth", "0x50000"]),
    ("verify-animation.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-traffic.py", ["--reference", "{commands}"]),
    ("verify-pads.py", ["--reference", "{smoke}"]),
    ("verify-market.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-lift.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-sectors.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-hyperspace.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-system-names.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-map-stars.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-dots.py", ["--reference", "{commands}", "--cache", "{cache}"]),
    ("verify-galaxy-map.py", ["--smoke", "{smoke}", "--cache", "{cache}"]),
]


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
    parser.add_argument("--only", nargs="*", metavar="NAME",
                        help="run only these scripts (substring match)")
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
    missing = [f"{key:9} {needed[key]} ({path})"
               for key, path in paths.items() if not path.exists()]
    if missing:
        print("Missing inputs:\n  " + "\n  ".join(missing) + "\n")
        print(__doc__.strip())
        return 2

    selected = [
        entry for entry in ORACLES
        if not args.only or any(name in entry[0] for name in args.only)
    ]
    failed = []
    started = time.monotonic()
    for script, arguments in selected:
        command = [sys.executable, str(root / "tools" / "reference" / script)]
        command += [argument.format(**paths) for argument in arguments]
        result = subprocess.run(command, cwd=root, capture_output=True, timeout=3600)
        status = "ok  " if result.returncode == 0 else "FAIL"
        label = script + (" --depth " + arguments[-1] if "--depth" in arguments else "")
        print(f"{status} {label}", flush=True)
        if result.returncode != 0:
            failed.append(label)
            tail = (result.stdout + result.stderr).decode(errors="replace").strip().splitlines()
            for line in tail[-12:]:
                print(f"     {line}")
    elapsed = time.monotonic() - started
    print()
    if failed:
        print(f"{len(failed)} of {len(selected)} oracles failed in {elapsed:.1f}s: {failed}")
        return 1
    print(f"all {len(selected)} local reference oracles passed in {elapsed:.1f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
