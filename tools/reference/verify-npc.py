"""Check the NPC traffic and ship-damage view against its pinned expectations.

The port has no reference probe for F688's spawner or F1536's flash: the
executed original would need the whole system/player state. What is available is
the local-only `npc_view` test, whose expectations are the original's values
(F635's table, F688's counts, F953/F942's damage, F1536's model 0x9d), plus the
unit tests for those routines. This script runs both and reports the test names,
so a removed or newly-ignored test is visible in the gate.

`verify-ship-types.py` is the separate source oracle for the F635 table itself.

Run:

    python tools/reference/verify-npc.py --cache data.bin
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin")
    args = parser.parse_args()
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    env = dict(os.environ, FFE_WORLD_CACHE=str(cache))
    runs = [
        ("F635/F688/F953/F942 unit tests",
         [cargo, "test", "--locked", "-p", "ffe-core", "--lib", "npc"]),
        ("npc_view", [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "npc_view",
                      "--", "--ignored", "--nocapture"]),
    ]
    results = {}
    for label, command in runs:
        result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                timeout=900)
        output = (result.stdout + result.stderr).decode(errors="replace")
        if result.returncode:
            raise RuntimeError(f"{label} failed:\n{output[-3000:]}")
        passed = re.findall(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)
        if not passed:
            raise RuntimeError(f"{label} reported no passing tests")
        results[label] = passed
    expected = {
        "npc_view": "system_traffic_draws_ships_names_hit_and_kill",
        "F635/F688/F953/F942 unit tests": "f688_counts_come_from_the_system_record_and_are_capped",
    }
    for label, name in expected.items():
        if not any(test.endswith(name) for test in results[label]):
            raise RuntimeError(f"{label} did not run {name}")
    print(json.dumps({
        "scenario": "ffe-npc-view-v1",
        "tests": results,
        "cache": str(cache),
        "note": "expectations are the original's table/counts/damage/flash; see docs/notes/npc.md",
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
