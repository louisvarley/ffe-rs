"""Compare the port's landed surface view with the executed original's.

`jjffe-smoke --game-frame 500 0` runs the real game from its new game and saves
the final 320x200 frame: Ross 154's open-air starport, player state `0x30`
(F1050/F1054's landed view, the flight view from F994's pad seat). The Rust test
`new_game_landed_surface_view` renders the same state and saves its frame.

The two engines do not draw the same starport or terrain (the port's textured
fills and some station faces are approximations), so the comparison is
structural:

* the upper view's background is the same colour (the original's
  `DATA_008815..817` space colour, `#0c0c4c` by default);
* everything the port draws above the console lies inside the original's drawn
  region, so the port has not invented sky objects or a HUD;
* the port's view covers at least half of what the original draws (the
  original's solid starport faces and platform fill more than the port's);
* the console's top rows (158..168) match the original's pixel for pixel
  (>= 98% within 8 levels): the panel art is the original's.

Needs `jjffe-smoke.exe`, `data.bin` (cache v26) and numpy/Pillow.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

import numpy as np
from PIL import Image

VIEW_ROWS = 158
HEADER_ROWS = (158, 168)
SKY = np.array([12, 12, 76])  # palette entry 182: six-bit (3, 3, 19)


def read_frame(path):
    data = path.read_bytes()
    pixels = np.frombuffer(data[:64000], dtype=np.uint8).reshape(200, 320)
    palette = np.frombuffer(data[64000:64768], dtype=np.uint8).reshape(256, 3).astype(int) * 4
    return palette[pixels]


def read_ppm(path):
    return np.array(Image.open(path).convert("RGB")).astype(int)


def measure(ours, real):
    mask = lambda frame: np.abs(frame[:VIEW_ROWS] - SKY).sum(2) > 24
    our_mask, real_mask = mask(ours), mask(real)
    both = int((our_mask & real_mask).sum())
    top, bottom = HEADER_ROWS
    header = np.abs(ours[top:bottom] - real[top:bottom]).sum(2) <= 24
    return {
        "sky_matches": bool(np.abs(ours[0, 0] - real[0, 0]).sum() <= 8),
        "header_close": round(float(header.mean()), 4),
        "view_inside": round(both / max(1, int(our_mask.sum())), 4),
        "view_coverage": round(float(our_mask.sum()) / max(1, int(real_mask.sum())), 4),
        "ours_view_pixels": int(our_mask.sum()),
        "real_view_pixels": int(real_mask.sum()),
    }


def accepted(result):
    return (result["sky_matches"] and result["header_close"] >= 0.98
            and result["view_inside"] >= 0.95
            and 0.5 <= result["view_coverage"] <= 1.2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--smoke", required=True, type=Path, help="jjffe-smoke.exe")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v26)")
    args = parser.parse_args()
    smoke = args.smoke.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-surface-") as directory:
        directory = Path(directory)
        runs = []
        for _ in range(2):
            subprocess.run([str(smoke), "--game-frame", "500", "0"], cwd=directory,
                           capture_output=True, timeout=180, check=True)
            runs.append((directory / "game-frame.bin").read_bytes())
        if runs[0] != runs[1]:
            raise RuntimeError("the original's landed frames differ between runs")
        real = read_frame(directory / "game-frame.bin")
        env = dict(os.environ, FFE_SHOT_DIR=str(directory),
                   FFE_WORLD_CACHE=str(args.cache.resolve(strict=True)))
        subprocess.run([cargo, "test", "--locked", "-p", "ffe-rs", "--test", "planet_view",
                        "new_game_landed_surface_view", "--", "--ignored", "--exact"],
                       cwd=root, env=env, check=True, timeout=600)
        ours = read_ppm(directory / "landed-starport.ppm")
        result = measure(ours, real)
        if not accepted(result):
            raise RuntimeError(f"the landed surface differs from the original's: {result}")
        # The comparison must be able to fail: a shifted frame, a blank one and a
        # frame with the drawn content in the wrong place are all rejected.
        for label, altered in (("shifted", np.roll(ours, 3, axis=1)),
                               ("blank", np.full_like(ours, SKY)),
                               ("inverted", 255 - ours)):
            if accepted(measure(altered, real)):
                raise RuntimeError(f"a {label} surface was accepted")
        digest = __import__("hashlib").sha256(runs[0]).hexdigest()
    print(json.dumps({"scenario": "jjffe-landed-surface-v1", **result,
                      "original_frame_sha256": digest, "tamper_rejected": True}, indent=2))


if __name__ == "__main__":
    main()
