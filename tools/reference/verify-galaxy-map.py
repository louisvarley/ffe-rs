"""Compare the galaxy map the port draws with the one the executed original draws.

`jjffe-smoke --game-frame 400 0x3d` runs the real game from a new game (Ross 154, sector -1,0) and
presses F3: the 320x200 frame and its palette are written. The Rust test `the_map_draws` renders the
same map (the home position, the drive's range of 1000 the new game starts with, the original's inputs)
over the same cabin art and saves its frame. What each draws over the art (grid, sphere, stalks, stars,
labels, marker) is compared pixel by pixel: nearly all of them must be at the same places. Colours of
the unlit materials differ where the original's runtime palette approximates a 12-bit colour, and the
textured "CORE" letters are not coloured as the original's are; those are counted, not required.
Needs data.bin (cache v25).
"""
import argparse
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

import numpy as np
from PIL import Image

HEIGHT = 150  # the view's rows below the status text, above the console


def read_frame(path):
    data = path.read_bytes()
    pixels = np.frombuffer(data[:64000], dtype=np.uint8).reshape(200, 320)
    palette = np.frombuffer(data[64000:64768], dtype=np.uint8).reshape(256, 3).astype(int) * 4
    return palette[pixels]


def overlay(frame, cabin):
    return np.abs(frame[:HEIGHT] - cabin[:HEIGHT]).sum(2) > 12


def compare(ours, real, cabin):
    ours_mask, real_mask = overlay(ours, cabin), overlay(real, cabin)
    both = ours_mask & real_mask
    same = both & (np.abs(ours[:HEIGHT] - real[:HEIGHT]).sum(2) <= 24)
    return {"ours": int(ours_mask.sum()), "real": int(real_mask.sum()), "both": int(both.sum()),
            "same_colour": int(same.sum())}


def accepted(result):
    return (result["real"] > 5000 and result["both"] >= 0.985 * result["real"]
            and result["both"] >= 0.985 * result["ours"]
            and result["same_colour"] >= 0.80 * result["both"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--smoke", required=True, type=Path, help="jjffe-smoke.exe")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v25)")
    args = parser.parse_args()
    smoke = args.smoke.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-galaxy-map-") as directory:
        directory = Path(directory)
        runs = []
        for _ in range(2):
            subprocess.run([str(smoke), "--game-frame", "400", "0x3d"], cwd=directory,
                           capture_output=True, timeout=120, check=True)
            runs.append((directory / "game-frame.bin").read_bytes())
        if runs[0] != runs[1]:
            raise RuntimeError("the original's map frames differ between runs")
        real = read_frame(directory / "game-frame.bin")
        env = dict(os.environ, FFE_SHOT_DIR=str(directory),
                   FFE_WORLD_CACHE=str(args.cache.resolve(strict=True)))
        subprocess.run([cargo, "test", "--locked", "-p", "ffe-rs", "--test", "galaxy_map_view",
                        "the_map_draws", "--", "--ignored", "--exact"],
                       cwd=root, env=env, check=True, timeout=600)
        ours = np.array(Image.open(directory / "galaxy-map.ppm").convert("RGB")).astype(int)
        with zipfile.ZipFile(args.cache) as archive:
            cabin = np.array(Image.open(io.BytesIO(archive.read("cockpits/cabin0.png")))
                             .convert("RGB")).astype(int)
        result = compare(ours, real, cabin)
        if not accepted(result):
            raise RuntimeError(f"the map differs from the original's: {result}")
        # The comparison must be able to fail: a shifted copy, a blank frame and another map.
        for label, altered in (("shifted", np.roll(ours, 3, axis=1)),
                               ("blank", np.zeros_like(ours)),
                               ("flipped", ours[::-1].copy())):
            if accepted(compare(altered, real, cabin)):
                raise RuntimeError(f"a {label} map was accepted")
        digest = __import__("hashlib").sha256(runs[0]).hexdigest()
    print(json.dumps({"scenario": "jjffe-galaxy-map-v1", **result, "original_frame_sha256": digest,
                      "tamper_rejected": True}, indent=2))


if __name__ == "__main__":
    main()
