# ffe-rs

A native Rust reimplementation of **Frontier: First Encounters** (Elite 3).
The gameplay reference is **JJFFE 2.8a7**; the aim of this checkout is a
like-for-like **Classic** game, drawn the original's way (320x200, the original
palette, font, cockpit and software rasterisers). The player supplies their own
installed copy of the game; nothing from it is in this repository.

**Status:** experimental and playable on Windows x64, not yet like-for-like.
[docs/STATUS.md](docs/STATUS.md) is the current state and work list, and
[docs/BURNDOWN.md](docs/BURNDOWN.md) is what is left to implement.
[CLAUDE.MD](CLAUDE.MD) records the owner's decisions and the working rules.

## Requirements

- Windows x64 and the Rust toolchain (`cargo`).
- An installed copy of the game in `game\` beside the checkout: the floppy
  English 1.02 or CD English 1.01 release (the shareware 1.06 data also loads).
  [docs/GAME_FILES.md](docs/GAME_FILES.md) lists the recognised files and hashes.
  The game reads its content straight from these files at every start; there is
  no cache to build. It never changes them.

## Play

| How | What it does |
|---|---|
| `play.bat` | Builds the release game and starts it on `.\game`. Errors stay on screen. |
| `launcher.bat` | Builds and opens the Classic launcher: game folder, verification, start location, settings ([notes](docs/notes/launcher.md)). |
| `target\release\ffe-rs.exe --root .` | The game directly. `--game <folder>` picks another game folder. |

Every run records your keyboard and mouse to `recordings\<date-time>.ffe-input`.
`replay.bat recordings\FILE` (or `ffe-rs --replay FILE [--replay-exit]`) plays a
recording back exactly, so a problem can be reproduced from the file;
`FFE_TRACE=1` prints the flight state while it runs. Saves and preferences are in
`saves\`.

The controls are in [docs/CONTROLS.md](docs/CONTROLS.md): a **Modern** preset
(W/S and the arrows, the default) and the original's **Classic** keys, switched on
the Escape pages' Advanced Options page. The window size, fit, filter and pixel
aspect are port extensions that never change the 320x200 frame
([docs/notes/display.md](docs/notes/display.md)).

## Developer options

`--dev` (or `FFE_DEV=1`) enables Ctrl+F11 (a screenshot of the presented frame),
Ctrl+F12 (the model viewer), Ctrl+Shift+F12 (the station model lab) and O (the
synthetic prototype flight model). Other inputs used by the tests and evidence:

```
ffe-rs [--game DIR] [--root DIR] [--start 0..5|sol] [--skip-intro] [--load SAVE.json]
       [--smoke-frames N] [--capture* DIR] [--record FILE] [--replay FILE [--replay-exit]]
```

`ffe-tools` inspects and converts a game installation without changing it:

```
ffe-tools inspect-game   [--game DIR]          recognise the release and check every file
ffe-tools export-images  --game DIR --out NEW  every bitmap as PNG, with sizes for upscaling
ffe-tools export-media   --game DIR --out DIR  the CD videos and stills into FMVs\
ffe-tools build-data     --game DIR --out NEW  the content cache the local tests read
ffe-tools inspect-data   --data FILE           verify a content cache
ffe-tools model-census   [--game DIR]          which model programs the port executes
```

## Tests

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Tests that need the original content are `#[ignore]`d and read a local cache:
build one with `ffe-tools build-data --game game --out .cache\w.bin`, then run
them with `FFE_WORLD_CACHE=.cache\w.bin cargo test -p ffe-rs -- --ignored`.
Some need other local captures or traces and say so by name. The differential
checks against the executed original JJFFE are in
[tools/reference/README.md](tools/reference/README.md), and
[docs/REFERENCE_HARNESS.md](docs/REFERENCE_HARNESS.md) is the recipe for adding
one.

## Repository layout

```
crates/ffe-core/      deterministic simulation: RNG, physics, models, galaxy, traffic, markets, missions
crates/ffe-rs/        the game: software renderer, window, input, sound, saves
crates/ffe-content/   the verified content cache read from the game files
crates/ffe-convert/   bounds-checked decoders for the executable and the asset files
crates/ffe-audio/     HMP music and effect playback
crates/ffe-launcher/  the Classic launcher
crates/ffe-tools/     the command-line tools above
tools/reference/      local oracles against the executed JJFFE reference
dashboard/            the burndown page (open index.html); data in burndown.js
docs/                 status, decisions, formats and per-subsystem notes (see docs/README.md)
```

## Never committed

`game\`, `jjffe-master\`, `ffed3d-master\`, the supplied
`Frontier-First-Encounters*` release and patch folders, generated caches,
extracted art, captures, recordings and saves stay local. `ci/check_forbidden.py`
fails CI if a forbidden path is tracked. Never use an indiscriminate `git add .`.

## Legal

An unofficial preservation and reimplementation project, not endorsed by the
original or community authors. Players supply their own game; no original or
derived game data ships with it. The Rust code has no licence yet (`LICENSE` is a
placeholder). See [docs/LEGAL.md](docs/LEGAL.md) and
[docs/TAKEDOWN.md](docs/TAKEDOWN.md).
