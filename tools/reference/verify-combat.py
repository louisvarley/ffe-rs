"""Check the executed JJFFE new game's laser installation (--state-combat).

The reference adapter's `--state-combat <frames>` mode runs the real new game
and prints the player object's laser, missile, shield and energy words. This
script runs it twice (repeatability), then asserts the F998 start-type 1
record that `crates/ffe-rs/tests/combat_view.rs` checks against the cache:

    model 23 (Eagle long range fighter), one gun mount,
    front laser 0x88 (1MW pulse), rear/turrets empty,
    two 0x82 missiles, drive 2, no shields, hull 100,
    energy 0x20000000 (1 t), cash 1000.

It is local-only: the reference binary embeds the supplied game data and must
not be distributed.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile


def run(reference: Path, directory: Path) -> list[dict]:
    result = subprocess.run(
        [str(reference), "--state-combat", "170"],
        cwd=directory,
        capture_output=True,
        timeout=60,
    )
    if result.returncode:
        raise RuntimeError(
            f"reference failed ({result.returncode}): "
            f"{result.stderr.decode(errors='replace')}"
        )
    return [json.loads(line) for line in result.stdout.splitlines() if line]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="ffe-combat-reference-") as directory:
        root = Path(directory)
        first, second = root / "first", root / "second"
        first.mkdir()
        second.mkdir()
        rows = run(reference, first)
        again = run(reference, second)
        if rows != again:
            raise RuntimeError("repeated reference --state-combat traces differ")
        if not rows:
            raise RuntimeError("--state-combat printed no rows")
        # The start is set at frame 160; every row from there must agree.
        start = [row for row in rows if row["frame"] >= 161]
        if len(start) != len(rows) or not start:
            raise RuntimeError("--state-combat did not start at frame 161")
        for row in rows:
            if row["frame"] < 161:
                raise RuntimeError(f"unexpected pre-start row {row['frame']}")
        last = rows[-1]
        expected = {
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
        for key, value in expected.items():
            if last.get(key) != value:
                raise RuntimeError(f"{key}: {last.get(key)!r} != {value!r}")
        print(
            json.dumps(
                {
                    "rows": len(rows),
                    "repeat_traces_match": True,
                    "new_game": expected,
                },
                indent=2,
            )
        )


if __name__ == "__main__":
    main()
