"""Check the original HUD, message log and console layout at their pinned pixels.

There is no reference framebuffer mode that can be compared frame for frame with
the port's synthetic dashboard, so the local-only `dashboard_view` test carries
the original's coordinates from `ffe.asm` (F271's ten-entry log in the
195..320 x 158..200 window, F273's 0x18 auto-scroll, F1099's top/bottom texts,
F2/record 0x0c's lamps and engines-off page). This script runs it and checks the
three expected test names ran, so a silently ignored or removed check is visible
in the gate.

Run:

    python tools/reference/verify-dashboard.py --cache data.bin
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

TESTS = {
    "the_message_log_draws_only_in_the_original_right_hand_window",
    "the_player_hud_texts_are_at_f1099s_positions",
    "the_lamps_and_the_equipment_page_are_the_originals",
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin")
    args = parser.parse_args()
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    env = dict(os.environ, FFE_WORLD_CACHE=str(cache))
    command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "dashboard_view",
               "--", "--ignored", "--nocapture"]
    result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=900)
    output = (result.stdout + result.stderr).decode(errors="replace")
    if result.returncode:
        raise RuntimeError(f"dashboard_view failed:\n{output[-3000:]}")
    passed = {test.rsplit("::", 1)[-1] for test in
              re.findall(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)}
    missing = TESTS - passed
    if missing:
        raise RuntimeError(f"dashboard_view did not run {sorted(missing)} (ran {sorted(passed)})")
    print(json.dumps({
        "scenario": "ffe-dashboard-view-v1",
        "tests": sorted(passed),
        "cache": str(cache),
        "note": "the message log, F1099 HUD and F2 lamp/page positions are pinned from ffe.asm",
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
