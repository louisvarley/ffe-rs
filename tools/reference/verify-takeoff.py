"""Check the open-air starport takeoff: F1013/F1031's mode hand-off.

The executed original's takeoff path needs the whole player/planet state, so the
differential probe is F650's seat (`verify-pads.py`, open-air branch). The
transition itself is checked two ways here:

* the local-only `new_game_takes_off_from_the_pad_into_free_flight` test flies
  the new game from the pad and prints the modes it passed through; the original
  F1013 sets `DATA_008870 = 0x26` above 200,000 units and F1031 returns to
  `0x24` on a cleared `0x20` region (the test must see `[36, 38]` = 0x24, 0x26);
* the two routines in `jjffe-master/ffe.asm` are checked for the cited labels
  and constants, so a future edit that changes the port's constants without the
  original is caught.

Run:

    python tools/reference/verify-takeoff.py --cache data.bin --sources jjffe-master
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def routine(asm: str, label: str) -> str:
    match = re.search(rf"^{label}:\s*(?:;.*)?$", asm, re.MULTILINE)
    if not match:
        raise RuntimeError(f"ffe.asm is missing {label}")
    end = re.search(r"^FUNC_[0-9A-F]+:\s*(?:;.*)?$", asm[match.end():], re.MULTILINE)
    return asm[match.end():match.end() + end.start()] if end else asm[match.end():]


def check(text: str, needle: str, where: str) -> None:
    if needle not in text:
        raise RuntimeError(f"{where} no longer contains {needle!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin")
    parser.add_argument("--sources", required=True, type=Path, help="jjffe-master")
    args = parser.parse_args()
    cache = args.cache.resolve(strict=True)
    asm = (args.sources.resolve(strict=True) / "ffe.asm").read_text(
        encoding="utf-8", errors="replace")
    f1013 = routine(asm, "FUNC_001013")
    f1031 = routine(asm, "FUNC_001031")
    # F1013: the open-air climb hand-off and its 200,000-unit guard.
    for needle in ("mov byte [DATA_008870],0x26",
                   "cmp dword [DATA_008867],0x30d40",
                   "cmp dword [DATA_008918],byte +0x0",
                   "call FUNC_001116",
                   "call FUNC_001041"):
        check(f1013, needle, "F1013")
    # F1031: the bay/clearance decisions that return to 0x24 or hand to 0x26.
    for needle in ("cmp byte [DATA_008870],0x2c",
                   "test esi,0x20",
                   "mov byte [DATA_008870],0x0",
                   "test esi,0x40",
                   "test byte [eax+0x14c],0x20",
                   "mov byte [DATA_008870],0x24"):
        check(f1031, needle, "F1031")
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    env = dict(os.environ, FFE_WORLD_CACHE=str(cache))
    command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "navigation",
               "new_game_takes_off_from_the_pad_into_free_flight", "--",
               "--ignored", "--exact", "--nocapture"]
    result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=900)
    output = (result.stdout + result.stderr).decode(errors="replace")
    if result.returncode:
        raise RuntimeError(f"takeoff test failed:\n{output[-3000:]}")
    if "takeoff modes [36, 38]" not in output:
        raise RuntimeError("the takeoff test did not observe 0x24 -> 0x26:\n" + output[-2000:])
    if "cleared true" not in output or "mode 0x0" not in output:
        raise RuntimeError("the takeoff test did not reach free flight:\n" + output[-2000:])
    print(json.dumps({
        "scenario": "ffe-takeoff-v1",
        "f1013_guard": "altitude > 0x30d40 or DATA_008918 != 0 sets DATA_008870 = 0x26",
        "f1031_handoff": "F1033 result 0x20/station 0x14c bit 0x20: 0x24; cleared: 0x00",
        "observed_modes": [36, 38],
        "cleared": True,
        "final_mode": "0x0",
        "asm_checks": 11,
        "cache": str(cache),
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
