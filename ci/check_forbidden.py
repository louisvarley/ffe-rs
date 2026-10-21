#!/usr/bin/env python3
"""Fail CI if any forbidden material is tracked.

The rules are deliberately path/suffix based: this is a guard rail, not a
content scanner.  See docs/LEGAL.md and docs/TAKEDOWN.md for the policy.
"""

from __future__ import annotations

import fnmatch
import subprocess
import sys

# Prefixes are matched after normalising to forward slashes.  A prefix ending
# in "/" matches a directory and everything under it.
FORBIDDEN_PREFIXES = (
    "game/",
    "legacy/",
    "jjffe-master/",
    "ffed3d-master/",
    "to-be-upscaled/",
    "upscaled/",
    "extracted/",
    "frames/",
    "screenshots/",
    "shots/",
    "tests/frames/",
    "tests/screenshots/",
    "tests/golden/frames/",
    "tests/golden/screenshots/",
    "traces/",
    "saves/",
)

FORBIDDEN_ROOT_GLOBS = ("frontier-first-encounters*",)

FORBIDDEN_EXACT = {
    "data.bin",
    "data.bin.tmp",
    "upscaled.bin",
    "upscaled.bin.tmp",
}

# Suffixes that are either original game assets, unlicensed reference
# binaries, or captures derived from them.
FORBIDDEN_SUFFIXES = (
    ".bmp",
    ".tga",
    ".dds",
    ".raw",
    ".hmp",
    ".sav",
    ".sve",
    ".exe",
    ".dll",
    ".lib",
    ".pdb",
    ".ilk",
    ".exp",
)

# Frame/screenshot naming conventions.  These are never committed even if
# they live outside a forbidden directory.
FORBIDDEN_GLOBS = (
    "frame_*.png",
    "shot_*.png",
    "screenshot*.png",
    "classic_*.png",
    "enhanced_*.png",
    "baseline_*.png",
    "comparison_*.png",
)

# Paths that are explicitly allowed despite a broad suffix/prefix rule.
ALLOWED = {
    ".gitattributes",
    ".gitignore",
    "LICENSE",
    "README.md",
}


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout
    return [p.replace("\\", "/") for p in out.decode("utf-8", "surrogateescape").split("\0") if p]


def violation(path: str) -> str | None:
    path = path.replace("\\", "/")
    if path in ALLOWED:
        return None
    low = path.lower()
    if low in FORBIDDEN_EXACT:
        return "exact forbidden path"
    for prefix in FORBIDDEN_PREFIXES:
        if low == prefix.rstrip("/") or low.startswith(prefix):
            return f"under forbidden directory '{prefix}'"
    root = low.split("/", 1)[0]
    for pattern in FORBIDDEN_ROOT_GLOBS:
        if fnmatch.fnmatchcase(root, pattern):
            return f"under local release/patch directory '{root}'"
    for suffix in FORBIDDEN_SUFFIXES:
        if low.endswith(suffix):
            return f"forbidden file type '{suffix}'"
    base = low.rsplit("/", 1)[-1]
    for pattern in FORBIDDEN_GLOBS:
        if fnmatch.fnmatchcase(base, pattern):
            return f"frame/screenshot name '{pattern}'"
    return None


def main() -> int:
    try:
        files = tracked_files()
    except subprocess.CalledProcessError as exc:
        print("error: could not run git ls-files:", exc, file=sys.stderr)
        return 2

    bad = [(p, r) for p in files if (r := violation(p)) is not None]
    if bad:
        print("Forbidden material is tracked in git:\n", file=sys.stderr)
        for path, reason in bad:
            print(f"  {path}  ({reason})", file=sys.stderr)
        print(
            "\nRemove it from the index and history. See docs/LEGAL.md and "
            "docs/TAKEDOWN.md.  Game data, legacy source, data.bin, original "
            "art, frames and screenshots must never be committed.",
            file=sys.stderr,
        )
        return 1

    print(f"ok: {len(files)} tracked file(s), none forbidden")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
