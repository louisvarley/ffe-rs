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


def db_bytes(text: str) -> bytearray:
    data = bytearray()
    for line in text.splitlines():
        line = line.split(";", 1)[0].strip()
        if not line.lower().startswith("db"):
            continue
        for match in re.finditer(r"'([^']*)'|0x([0-9a-fA-F]+)", line[2:]):
            if match.group(1) is not None:
                data += match.group(1).encode("latin-1")
            else:
                data.append(int(match.group(2), 16))
    return data


def data_block(asm: str, label: str) -> str:
    match = re.search(rf"^{label}:\s*(?:;.*)?$", asm, re.MULTILINE)
    if not match:
        raise RuntimeError(f"ffedat.asm is missing {label}")
    end = re.search(r"^[A-Za-z_][A-Za-z0-9_]*:\s*(?:;.*)?$", asm[match.end():],
                    re.MULTILINE)
    return asm[match.end():match.end() + end.start()] if end else asm[match.end():]


def services_and_keys(ffe: str, data: str) -> int:
    """F4 and action 9, from the tables and the routines.

    The DOS key table is 256 bytes: 0x80+scancode is the unshifted row (the
    shift state byte is 0x80 when *up*), so F4 (0x3e) is index 0xbe and Tab
    (0x0f) is 0x8f. F1056's action 0x09 is the launch; F343 opens the services
    board from 0x2a or 0x30.
    """
    keys = db_bytes(data_block(data, "DATA_007688"))
    actions = db_bytes(data_block(data, "DATA_004893"))
    checks = 0
    if keys[0xBE] != 0xF4:
        raise RuntimeError(f"plain F4 maps to {keys[0xBE]:#x}, not the station action 0xf4")
    if keys[0x8F] != 0x09:
        raise RuntimeError(f"plain Tab maps to {keys[0x8F]:#x}, not launch action 9")
    if keys[0xA6] != 0x6C:
        raise RuntimeError(f"plain L maps to {keys[0xA6]:#x}, not the label action 0x6c")
    if actions[12] != 0xF4:
        raise RuntimeError(f"module 12 (station) has action {actions[12]:#x}, not 0xf4")
    checks += 4
    station_table = data_block(data, "DATA_005285")
    check(station_table, "dd FUNC_000320", "DATA_005285")
    check(station_table, "dd FUNC_000343", "DATA_005285")
    checks += 2
    f343 = routine(ffe, "FUNC_000343")
    for needle in ("cmp al,0x2a", "cmp al,0x30", "call FUNC_000333",
                   "mov ebx,0x2c"):
        check(f343, needle, "F343")
        checks += 1
    f1056 = routine(ffe, "FUNC_001056")
    for needle in ("sub eax,byte +0x9", "jz near JUMP_004417",
                   "sub eax,byte +0x63"):
        check(f1056, needle, "F1056")
        checks += 1
    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin")
    parser.add_argument("--sources", required=True, type=Path, help="jjffe-master")
    args = parser.parse_args()
    cache = args.cache.resolve(strict=True)
    asm = (args.sources.resolve(strict=True) / "ffe.asm").read_text(
        encoding="utf-8", errors="replace")
    data = (args.sources.resolve(strict=True) / "ffedat.asm").read_text(
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
    extra = services_and_keys(asm, data)
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
    if "modes [36, 38]" not in output:
        raise RuntimeError("the takeoff test did not observe 0x24 -> 0x26:\n" + output[-2000:])
    if "cleared true" not in output or "mode 0x0" not in output:
        raise RuntimeError("the takeoff test did not reach free flight:\n" + output[-2000:])
    # The DOS default start (F829 record 0, Hope / Old Blackelk) must take off
    # through the same machine with the planet as the ship's parent.
    command[command.index("new_game_takes_off_from_the_pad_into_free_flight")] = \
        "default_start_takes_off_from_hope"
    result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=900)
    output = (result.stdout + result.stderr).decode(errors="replace")
    if result.returncode:
        raise RuntimeError(f"default-start takeoff test failed:\n{output[-3000:]}")
    if "start 0 modes [36, 38]" not in output or "cleared true" not in output:
        raise RuntimeError("the default start did not take off from Hope:\n" + output[-2000:])
    print(json.dumps({
        "scenario": "ffe-takeoff-v1",
        "f1013_guard": "altitude > 0x30d40 or DATA_008918 != 0 sets DATA_008870 = 0x26",
        "f1031_handoff": "F1033 result 0x20/station 0x14c bit 0x20: 0x24; cleared: 0x00",
        "observed_modes": [36, 38],
        "cleared": True,
        "final_mode": "0x0",
        "services_action": "plain F4 -> 0xf4 -> module 12 (station) -> F343 from 0x2a/0x30",
        "launch_action": "plain Tab -> action 9 -> F1056 JUMP_004417 (L is the label action 0x6c)",
        "asm_checks": 11 + extra,
        "cache": str(cache),
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
