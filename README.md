# ffe-rs

A Rust port of **FFED3D** / **Frontier: First Encounters** (Elite 3), faithful by default, with optional **Enhanced graphics** that are off unless the player turns them on.

> **Status: Phase 0 complete — awaiting approval.** No game code has been written yet. The repository currently contains the Phase 0 study, the proposed rendering/architecture design, and repository hygiene. See `docs/PHASE0_SUMMARY.md`.

## What this is

- **Classic by default:** the original 320×200 indexed look, palette, bitmap font, affine textures, flat/quantised shading, original aspect, nearest-neighbour scaling. The simulation is frozen against golden traces.
- **Enhanced, one switch at a time:** HD textures, native resolution, filtering/AA, HDR, bloom, shadows, AO, atmosphere/clouds, starfield/nebulae, particles/glows, material maps, widescreen presentation, frame interpolation. Each is independently toggleable; a **single master button** flips all presentation-only enhancements on/off instantly, in the spirit of C&C Remastered.
- **Gameplay-affecting options are separate** (extended FOV, input smoothing, aim assist, fill-widescreen). They are never touched by the master button and are listed in `docs/DEVIATIONS.md`.
- **Data-driven and moddable:** all content is data with stable IDs; mods can add/override/patch; the galaxy is procedural and must not be perturbed by unrelated mods.
- **No game data in the repository.** The player supplies their own copy; `data.bin` is generated locally.

## Documentation

| Document | Contents |
|---|---|
| `docs/PHASE0_SUMMARY.md` | What was done, what was built, what is blocked, questions for the owner. |
| `docs/INVENTORY.md` | Complete source/game inventory, global state, file formats, content audit, numeric model, timing, build report. |
| `docs/FFED3D_RENDERING_NOTES.md` | How the original software renderer and the FFED3D D3D9 path work, and what Classic must reproduce. |
| `docs/RENDERING_DESIGN.md` | Proposed Enhanced feature list/order, dependencies, master button, performance budgets, quirk checklist, test tolerances. |
| `docs/ARCHITECTURE.md` | Proposed Rust workspace, `data.bin`/`upscaled.bin` contracts, version detection, startup auto-conversion, mod layering. |
| `docs/FORMATS.md` | Why RON/TOML/JSON, exact schemas, determinism rules, upscaling exclusion/resize rules. |
| `docs/GAME_FILES.md` | The oracle copy: every file, size and SHA-256, version detection and floppy/CD notes. |
| `docs/LEGAL.md`, `docs/TAKEDOWN.md` | Copyright status, project stance, rights-holder process. |
| `docs/KNOWN_ISSUES.md` | Known original/reference issues and the bug policy. |
| `docs/DEVIATIONS.md` | Every intentional deviation, including the gameplay-affecting group. |
| `docs/PORT_LOG.md` | Decisions, discrepancies, open questions. |

## Repository layout (planned)

```
crates/ffe-core/        deterministic simulation (no I/O, clock or globals)
crates/ffe-content/     stable IDs, data.bin/upscaled.bin readers, mod layers
crates/ffe-convert/     conversion library (shared by CLI and startup)
crates/ffe-render/      render description, software reference, wgpu Classic, features
crates/ffe-platform/    window/input/audio/config
crates/ffe-tools/       build-data, export-upscale, upscale, pack-upscaled, validate-mod, new-mod
crates/ffe-rs/          the game binary
tests/golden/           hashes and numeric dumps only
```

## Data pipeline (to be implemented)

```sh
# Build data.bin from the player's copy (both entry points share ffe-convert):
cargo run -p ffe-tools -- build-data --game ./game --out data.bin

# Or just run the game: if data.bin is missing/outdated and ./game/ is next to
# the exe, it converts with progress output and starts.

# Upscaling:
cargo run -p ffe-tools -- export-upscale --game ./game --data data.bin --out to-be-upscaled
# run your upscaler over to-be-upscaled/ -> upscaled/
cargo run -p ffe-tools -- upscale --cmd "<command template>"
cargo run -p ffe-tools -- pack-upscaled --data data.bin --upscaled upscaled --out upscaled.bin
```

`data.bin` is never committed. `upscaled.bin` is committed via **Git LFS**.

## Git LFS setup

`upscaled.bin` is the only large binary accepted in git. The attribute is already in `.gitattributes`:

```gitattributes
upscaled.bin filter=lfs diff=lfs merge=lfs -text
```

To work with it:

```sh
git lfs install                 # once per machine
git lfs track "upscaled.bin"    # already configured; this is a no-op check
git lfs ls-files                # see LFS objects
git lfs pull                    # fetch pack contents
```

Release/CI checkouts must use LFS (`actions/checkout` with `lfs: true`). If the pack is replaced, commit the new LFS pointer; never commit the unpacked `upscaled/` working files. If LFS is unavailable, the game still runs with Classic/`data.bin`; only the HD textures option is disabled.

See `docs/ARCHITECTURE.md` and `docs/FORMATS.md` for the manifest/provenance rules.

## What is committed / never committed

**Allowed:** Rust source, docs, tests, schemas, golden hashes/numeric dumps, `upscaled.bin` (LFS).

**Never:** `game/`, `legacy/Elite 3/`, `legacy/Elite3/`, `legacy/ffed3d/` (no licence), `data.bin`, `to-be-upscaled/`, `upscaled/` working files, `extracted/`, frame dumps or screenshots derived from game assets, FFED3D binaries/models/textures/scripts, save files.

`ci/check_forbidden.py` fails CI if any forbidden path is tracked. `.gitignore` covers them locally.

### About `legacy/ffed3d`

The reference source has **no licence** and descends from Frontier Developments' code. It is git-ignored. If reproducibility is wanted, the cleanest option is a **pinned git submodule** to the upstream repository (still not redistributed by this project), rather than vendoring the files. The same applies to `legacy/Elite3/`. This is an open question in `docs/PHASE0_SUMMARY.md`.

## Building the legacy reference (study only)

Phase 0 built a local Windows x86 binary with VS 2026, NASM 3.02, the Microsoft D3DX9 NuGet package and compatibility patches; it runs and renders the splash/intro. It is **not** distributed and is not the product build. See `docs/INVENTORY.md` §11 for exactly what was changed and what could not be built with the original 2010 toolchain.

## Legal

This is an unofficial, AI-assisted preservation/reimplementation project. It requires the player's own copy of First Encounters. It is not endorsed by Frontier Developments, David Braben, John Jordan, the FFED3D authors or the Anisotropic mod authors. The game and the reference source remain the property of their rights-holders. `upscaled.bin` is derived from original art and is provided subject to the rights-holder's tolerance. See `docs/LEGAL.md` and `docs/TAKEDOWN.md`.

**No open-source licence has been chosen.** `LICENSE` is a placeholder for the project owner to fill in. Do not assume redistribution rights for the Rust code until it is replaced.
