"""Check the crosshair, F931 hitscan and F998 loadout against the original.

Two layers:

* `jjffe-smoke --state-combat 170` runs the real new game and prints the player
  object's laser, missile, shield and energy words. Two runs must agree, and the
  start-type 1 record (F998, `ffe.asm:74028`) must be model 23 (Eagle long range
  fighter), one gun mount, front `0x88` (1MW pulse), rear/turrets empty, two
  `0x82` missiles, drive 2, no shields, hull 100, 1 t energy and 1000 CR.
* F931 (`ffe.asm:69550`) is checked for the hitscan structure the port follows:
  the gate words (`+0xe8`, `+0xea`, `+0x11e`), F802/F817's object scan, F930's
  damage, F953's apply and F939's effects; it must not call the missile or
  object-creation routines.
* The local-only `combat_view` test draws the original bitmap 97 (61x61 at
  129,48) in laser views only, bitmap 98 (35x35) for the velocity marker in the
  front view, F270's bars, F1099's target text, F1100's range and F998's loadout,
  and fires through the port's F931/F953 path.

Run:

    python tools/reference/verify-combat.py --reference jjffe-smoke.exe \
        --cache data.bin --sources jjffe-master
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

EXPECTED = {
    "player": 107,
    "model": 23,
    "mounts": 1,
    "lasers": [0x88, 0, 0, 0],
    "missiles": [0x82, 0x82, 0, 0, 0, 0],
    "drive": 2,
    "shields": [0, 0],
    "hull": 100,
    "refire": 0,
    "heat": 0,
    "energy": 0x20000000,
    "cash": 1000,
}
COMBAT_TESTS = {
    "gunsight_is_f1111s_bitmap_97_in_laser_views_only",
    "velocity_marker_and_range_text_follow_f1112_f1100",
    "gauges_show_f270s_energy_and_heat_bars",
    "new_game_hull_carries_f998s_one_front_pulse_laser",
    "target_damage_readout_draws_f1099s_text",
    "beam_is_model_156_at_the_gun",
}


def run(reference: Path, directory: Path) -> list[dict]:
    result = subprocess.run([str(reference), "--state-combat", "170"], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"reference failed ({result.returncode}): "
                           f"{result.stderr.decode(errors='replace')}")
    return [json.loads(line) for line in result.stdout.splitlines() if line]


def routine(asm: str, label: str) -> str:
    match = re.search(rf"^{label}:\s*(?:;.*)?$", asm, re.MULTILINE)
    if not match:
        raise RuntimeError(f"ffe.asm is missing {label}")
    end = re.search(r"^FUNC_[0-9A-F]+:\s*(?:;.*)?$", asm[match.end():], re.MULTILINE)
    return asm[match.end():match.end() + end.start()] if end else asm[match.end():]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True, help="data.bin")
    parser.add_argument("--sources", type=Path, required=True, help="jjffe-master")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    asm = (args.sources.resolve(strict=True) / "ffe.asm").read_text(
        encoding="utf-8", errors="replace")
    with tempfile.TemporaryDirectory(prefix="ffe-combat-reference-") as directory:
        directory = Path(directory)
        first, second = directory / "first", directory / "second"
        first.mkdir()
        second.mkdir()
        rows = run(reference, first)
        again = run(reference, second)
        if rows != again:
            raise RuntimeError("repeated reference --state-combat traces differ")
        if not rows:
            raise RuntimeError("--state-combat printed no rows")
        start = [row for row in rows if row["frame"] >= 161]
        if len(start) != len(rows) or not start:
            raise RuntimeError("--state-combat did not start at frame 161")
        for row in rows:
            if row["frame"] < 161:
                raise RuntimeError(f"unexpected pre-start row {row['frame']}")
        last = rows[-1]
        for key, value in EXPECTED.items():
            if last.get(key) != value:
                raise RuntimeError(f"{key}: {last.get(key)!r} != {value!r}")
    # F931: the cited calls (no projectile object is created).
    f931 = routine(asm, "FUNC_000931")
    for needle in ("cmp word [ebx+0xe8],byte +0x0",
                   "cmp word [ebx+0xea],0xf000",
                   "cmp dword [ebx+0x11e],byte +0x0",
                   "call near [DATA_006791]    ; FUNC_000802",
                   "call FUNC_000930",
                   "call FUNC_000953",
                   "call FUNC_000939"):
        if needle not in f931:
            raise RuntimeError(f"F931 no longer contains {needle!r}")
    for forbidden in ("FUNC_000941", "FUNC_000773", "FUNC_001533"):
        if forbidden in f931:
            raise RuntimeError(f"F931 calls {forbidden}, so it is no longer hitscan")
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    env = dict(os.environ, FFE_WORLD_CACHE=str(cache))
    result = subprocess.run([cargo, "test", "--locked", "-p", "ffe-rs", "--test", "combat_view",
                             "--", "--ignored", "--nocapture"],
                            cwd=root, env=env, capture_output=True, timeout=900)
    output = (result.stdout + result.stderr).decode(errors="replace")
    if result.returncode:
        raise RuntimeError(f"combat_view failed:\n{output[-3000:]}")
    passed = {test.rsplit("::", 1)[-1] for test in
              re.findall(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)}
    missing = COMBAT_TESTS - passed
    if missing:
        raise RuntimeError(f"combat_view did not run {sorted(missing)} (ran {sorted(passed)})")
    print(json.dumps({
        "rows": len(rows),
        "repeat_traces_match": True,
        "new_game": EXPECTED,
        "f931_hitscan": "F802/F930/F953/F939; no F941/F773/F1533",
        "combat_view_tests": sorted(passed),
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
