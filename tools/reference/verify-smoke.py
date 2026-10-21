"""Run a locally built legacy reference twice, retaining no legacy data/captures."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=64)
    args = parser.parse_args()
    if not 3 <= args.frames <= 10000:
        parser.error("--frames must be in 3..10000")
    executable = args.exe.resolve(strict=True)
    traces = []
    for _ in range(2):
        with tempfile.TemporaryDirectory(prefix="jjffe-smoke-") as directory:
            result = subprocess.run(
                [str(executable), str(args.frames)],
                cwd=directory,
                capture_output=True,
                timeout=30,
                check=True,
            )
            if b"Not a gameplay oracle" not in result.stderr:
                raise RuntimeError("reference did not identify its smoke-adapter limitations")
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            if len(rows) != args.frames:
                raise RuntimeError(f"expected {args.frames} frames, found {len(rows)}")
            for index, row in enumerate(rows):
                if row["frame"] != index:
                    raise RuntimeError("nonsequential frame trace")
                if index and row["clock_ms"] <= rows[index - 1]["clock_ms"]:
                    raise RuntimeError("synthetic clock did not advance")
            if rows[-1]["blits"] <= 0:
                raise RuntimeError("no video blits observed")
            if len({row["pixels_fnv1a64"] for row in rows}) < 2:
                raise RuntimeError("framebuffer never changed")
            traces.append(result.stdout)
    if traces[0] != traces[1]:
        raise RuntimeError("bounded reference traces differ between runs")
    print(json.dumps({
        "reference": "jjffe-2.8a7",
        "adapter": "bounded-headless-smoke-v1",
        "frames": args.frames,
        "repeat_trace_matches": True,
        "trace_sha256": hashlib.sha256(traces[0]).hexdigest(),
        "unique_framebuffer_hashes": len({row["pixels_fnv1a64"] for row in rows}),
        "gameplay_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
