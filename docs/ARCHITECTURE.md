# Proposed workspace architecture and data pipeline

Status: **design; owner-approved direction recorded in [CLAUDE.MD](../CLAUDE.MD).**
The workspace contains `ffe-tools` with read-only inspection and partial external
export/cache commands, `ffe-convert` with verified PE bitmap tables, BMP/PNG,
PCM/WAV and sizing helpers, and `ffe-content` with a partial cache writer/reader.
`ffe-core` now has independently testable RNG, timer-counter and day-fraction
primitives and bounded original model vertex interpretation (`model`).
`tools/reference/` provides startup smoke, bounded new-game selected
state traces and executed numeric vectors; `ffe-tools verify-reference` compares
them to Rust. These are not a full flight/gameplay oracle.
The `ffe-rs` crate now provides an isolated experimental playable loop: synthetic
flight/market in `flight`, bounded versioned snapshots in `saves`, software
presentation in `screen`, a presentation scaler in `display`, and minifb
window/input/fixed-step scheduling in `main`.
V0.2 adds `indexed` with original proportional glyph masks/advances and
bounds-safe indexed blits. `screen` reads cabin0, main font, low palette and
console panel via `ffe-content`, composes a 320x200 logical frame and presents
nearest-scaled 4:3 output. `display` fits that finished frame to the window
(integer pixel-perfect or aspect-preserving, nearest or linear, 4:3 or square
pixels) and reopens the window for `Alt+Enter` full-screen; it never changes the
320x200 frame. Cabin RGB and low-palette content compose in RGB;
the original runtime upper palette (`F1326`/`F1327`/`F1330`/`F1331`) is built
by `palette` and resolves the model/scene colours to RGB
(`docs/notes/palette.md`).
No reference runtime is linked.
V0.3 adds `model_lab`: two imported station vertex programs are evaluated by
the pure integer `ffe-core::model`, then shown as points by a separate diagnostic
orthographic view. V0.4 adds `ffe-core::model_commands` for bounded physical
record decoding, original triangle/quad perimeters and standalone arithmetic/
bit-branch/material helpers. The lab can display all declared root faces or one
face, explicitly not executing branches, transforms or child programs.
Content/conversion share that structural decoder. A separate local probe
instruments exactly four emission calls in a generated JJFFE copy; the ordinary
startup/font/vertex oracle still builds unchanged supplied assembly.
F4 pauses simulation and disables trade/save/load actions;
arrows and test variables affect only the inspection view. No arbitrary edges,
visibility, submodel omission or captured mesh is presented as a faithful model.
Content/conversion use the same core vertex validator rather than duplicating it.
It deliberately does not reuse or replace the verified core as if its mechanics
were faithful. Splitting renderer/platform crates is deferred until warranted.
V0.5 adds an exact integer normal evaluator and optional normal-filtered outlines
in the lab. Diagnostic observer/light/shade inputs are separate from gameplay;
the filter still catalogues alternate branch faces, not executed primitives.
V0.6 adds the original quarter-wave table and exact Sin32, Q31 basis rotation
and trig arithmetic helpers. These do not yet execute complete model programs;
the lab camera remains diagnostic rather than the original transform dispatcher.
Packed angle operands and the pure local-basis rotation command form are also
verified independently; base/object-relative frame reconstruction is not.
F1751/F1752/F1755 normal/LOD visibility and branch helpers consume explicit
depth/detail/scale/evaluated-normal inputs. They share branch-offset handling
with bit branches; no implicit game globals or lazy pointers enter the core.
V0.7 adds `ffe-core::model_execute`, combining those helpers in a bounded
instruction-pointer loop. It returns visited command offsets and ordered face
operand emissions with context snapshots, committing final context only after a
real return. Unsupported reached operations, aliased instance selectors, invalid
inputs and exhausted budgets fail without partial output/state mutation.
These events are not evaluated geometry, shaded primitives or render-order
records. Normal flags are explicit evaluated inputs. The existing F4 catalogue
and synthetic flight renderer remain separate and unchanged.
V0.8 adds `ffe-core::model_depth::key`, an exact original F1777 integer depth-key
helper with explicit view-space point and instance scale inputs. It preserves
the overflow encoding rather than substituting a distance or z-buffer value.
V0.9 extends this module with F1696 command decoding/resolution, and adds
`ffe-core::model_order` for flat group lifetime and original sorting. The physical
command decoder reuses the new depth decoder, avoiding two record-size rules.
Depth context contains already resolved view-space points, object position,
key scale and operand scale, not implicit cache pointers. Negative/external
point indices fail explicitly. Native group entries own their primitive tokens;
opening another group closes the preceding group, including empty groups.
Sort descending unsigned keys, stably by allocation order on ties; primitives
inside a group retain insertion order and ignore their own keys. An open group
cannot be finalized. A separate 1..4096-operation budget bounds this abstraction;
it does not emulate original allocator words or nested child-model queues.
These helpers are now composed in `model_execute::execute_live` (v0.11), including
live view-space vertex state (v0.10) and the abstract `Order` token queue. The
executor still stops at point/text, submodels, other transform forms and colour;
see `docs/MODEL_EXECUTION_PLAN.md` §8 for the next task.
Experimental input now uses a release-to-rearm latch: blocked keys cannot become
flight controls merely because focus or a modal state changes. Modern is the
owner-selected startup input preset; F3 retains the Classic alternative
(right-button mouse steering, Enter/Shift set speed; `docs/CONTROLS.md`).
The actual v1/v2/v3/v4/v5/v6 partial cache
contract and its incomplete scope are in
[FORMATS.md](FORMATS.md). Full content, startup conversion and the remaining crates below are
planned, not present apart from the experimental `ffe-rs` crate. Current scope is Windows x64, JJFFE 2.8a7
gameplay, documented bug fixes, live image packs and one-way legacy save import.
Gameplay mods/mesh replacements below are deferred. The owner approved a local
base cache and separate image pack, not redistribution of either archive.

This document proposes the Rust workspace, crate responsibilities, the `data.bin`/`upscaled.bin` contracts, version detection, startup auto-conversion, mod layering, and the Phase 0 scripting recommendation. The data format is justified in `docs/FORMATS.md`.

---

## 1. Workspace layout

```
ffe-rs/
  Cargo.toml                     # workspace
  crates/
    ffe-core/                    # deterministic simulation; no I/O, no clock, no globals
    ffe-content/                 # registry, stable IDs, data.bin/upscaled.bin readers, mod layers
    ffe-convert/                 # conversion library shared by ffe-tools and the exe
    ffe-render-desc/             # RenderFrame contract (or module inside ffe-render)
    ffe-render/                  # renderer trait, software reference, wgpu Classic, features
    ffe-platform/                # window, input, audio, config, file dialogs, timing
    ffe-tools/                   # CLI: build-data, export-upscale, upscale, pack-upscaled, validate-mod, new-mod
    ffe-rs/                      # binary: startup, loop, menu, settings, trace hooks
  docs/
  tests/
    golden/                      # hashes/numeric dumps only
    synthetic/                   # generated test images/data (no game assets)
  ci/
  .github/workflows/
```

Reasons:

- `ffe-core` is independently testable with `cargo test -p ffe-core` and has no platform dependencies. It can be compiled to a headless trace runner.
- `ffe-content` owns all stable IDs and layering; `ffe-core` never parses ZIP/PNG/glTF directly.
- `ffe-convert` is shared by the CLI and the auto-convert-on-start path, so both entry points are byte-identical.
- `ffe-render-desc` is a small, serializable crate so golden tests and tools can consume frames without pulling in wgpu.
- `ffe-render` has no game rules; it only draws `RenderFrame` + content assets.
- `ffe-platform` isolates OS APIs; input remapping/gamepad/audio plumbing can improve without touching simulation.
- `ffe-tools` is a thin CLI over `ffe-convert`/`ffe-content`.
- `ffe-rs` is the product binary; it owns the 50 Hz loop, menu, settings and feature toggles.

### 1.1 Dependency direction

```
ffe-rs -> ffe-platform, ffe-render, ffe-content, ffe-core
ffe-render -> ffe-render-desc, ffe-content, ffe-core (read-only types)
ffe-content -> ffe-core (pure bounded vertex validation, no simulation state)
ffe-convert -> ffe-core (same vertex validator)
ffe-core -> no other workspace crate (future registry/emission interfaces remain design)
ffe-tools -> ffe-convert, ffe-content
```

`ffe-core` must not depend on `wgpu`, `winit`, `std::time`, `std::fs`, or any OS crate. Use `#[cfg(feature = "trace")]` for trace hooks.

---

## 2. `ffe-core`

### 2.1 Structure

```
state.rs        GameState (one &mut), object/entity storage, IDs
rng.rs          F1530, F894, fixed/float helpers, explicit seed state
math/           FFP, Fix16_16, Fix1_31, Vec32/64, Mat3, Ang16, Sin table
galaxy/         system ID/sector, names, hand-coded systems, F853/F857/F868/F878
economy/        market, stock, prices, tech/government, BBS/missions
ships/          ship defs, creation, equipment, damage, cargo
physics/        F574–F612: positions, FOR, thrust, gravity, collisions
combat/        lasers, missiles, ECM, damage, death, bounty
ai/             AI modes, spawns, traders, police, pirates, missions
missions/       contracts/packages/military/journal
save/           F170/F171 subset, compression/scramble, rounds
events/         module/event bus, player states, tick
render_desc.rs  emit RenderFrame (no drawing)
trace.rs        per-tick trace/hash hooks
content.rs      ContentRegistry trait (read-only lookups)
```

### 2.2 Determinism rules

- One `GameState` passed as `&mut`; entities by index/ID, no `Rc<RefCell>`, no global mut.
- No wall clock, no I/O, no unseeded randomness.
- All RNG calls explicit and traced (seed1/seed2, call site/tag).
- Integer operations use explicit widths and `wrapping_*`/`saturating_*`; no accidental `usize` math on serialized data.
- FFP/fixed-point newtypes; no f32/f64 in simulation except where the reference is known to use them, and then with an explicit precision policy.
- No `unsafe` unless justified and documented; `cargo clippy -- -D warnings`, `cargo fmt`. The sole current exception outside `ffe-audio` is `ffe-rs::display`: `GetSystemMetrics`/`GetAsyncKeyState` (Windows only) supply the monitor size for borderless full-screen and the physical Alt state that minifb 0.28 does not report. Both take scalar arguments and hold no pointers; non-Windows builds have safe fallbacks.
- The 50 Hz loop drives `tick(dt_tics, input)`; rendering is a separate consumer of `RenderFrame`.

### 2.3 Trace hooks

A `TraceSink` records per-tick:

- RNG seeds/state, tick/day/time accel;
- player state/position/orientation/velocity/shields/fuel/cash/cargo;
- current system and generated object list (IDs/positions/masses);
- market/stock/BBS/mission state;
- all entity states (index, type, model, transform, velocity, AI mode, target, flags);
- event log;
- a rolling hash of the serialized trace.

Failures report the first diverging tick and field, with a path (e.g. `objects[42].globalvars.mass`). Traces are hashes/numeric dumps, never screenshots.

---

## 3. `ffe-content` and stable IDs

### 3.1 IDs

Every content entry has a stable, namespaced ID:

```
base:<kind>/<name>        e.g. base:ship/cobra_mk3, base:texture/400, base:model/38
<mod-id>:<kind>/<name>    e.g. mymod:ship/my_ship
```

IDs are ASCII, case-sensitive, and never derived from file paths or array indices for mod-facing content. Base IDs may embed the original numeric index for byte-exact assets (`base:texture/400`), but the registry maps them to a stable name as well (`base:texture/cobra_mk3_hull`). The numeric index is part of the source asset identity, not the pack layout.

### 3.2 Registry layers

Load order is deterministic:

1. `base` (data.bin).
2. Mods in dependency order; ties broken by `load_order` then mod ID (lexicographic). Cycles are an error with the full cycle path.
3. Each layer can **add** entries, **override** an ID, or **patch** individual fields.

Conflicts are reported with both source locations. Overrides are explicit (`override = true` in `mod.toml` or a per-entry `override` field); a silent duplicate is an error.

### 3.3 Mod layout

```
mods/<mod-id>/
  mod.toml
  data/
    ships/*.ron
    systems/*.ron
    planets/*.ron
    goods/*.ron
    missions/*.ron
  assets/
    textures/*.png
    models/*.glb / *.ffe-mesh.ron
    audio/*.wav / *.mid
```

`mod.toml`:

```toml
id = "mymod"
name = "My Mod"
version = "0.1.0"
authors = ["..."]
description = "..."
game_version = "ffe-floppy-1.02-en"
load_order = 100
dependencies = [{ id = "othermod", version = ">=0.1,<1" }]

[[override]]
id = "base:ship/cobra_mk3"
fields = ["mass", "price"]
```

Validation errors include file, line, column (where available), field path, expected type, and a suggestion. `ffe-tools validate-mod` uses the Rust types directly and the generated JSON Schema is available for editors.

### 3.4 Procedural galaxy safety

System generation is keyed by `(sector_x, sector_y, star_number)` and uses a **per-system RNG stream** derived from those coordinates, not a shared global stream. Hand-authored systems/planets/stations are inserted/overridden by ID or coordinates in a lookup applied **after** the procedural system is generated (or before, for an exact replacement), but never consume RNG from other systems. Test: generate a fixed set of systems with no mods; add one hand-authored system/planet/station; assert every other system’s object list and RNG trace is byte-identical.

### 3.5 Base asset collision

`validate-mod` rejects any mod file whose SHA-256 matches a base asset in `data.bin` (prevents redistributing original art as a “mod”). It also warns if a mod asset has the same dimensions/ID as a base asset without `provenance`.

---

## 4. `data.bin`

### 4.1 Container

`data.bin` is a standard ZIP (deflate) with:

```
manifest.json
palettes/base.json
fonts/<font_id>.png
fonts/<font_id>.json
textures/<asset_id>.png          # indexed PNG + palette sidecar
textures/<asset_id>.json         # dimensions, transparent index, source index, kind
models/<asset_id>.glb            # glTF 2.0 binary, with extras.ffe
models/<asset_id>.ffe.ron        # exact command stream / shipdef / submodels
audio/fx/<asset_id>.wav          # 16-bit PCM mono 22050
audio/music/<asset_id>.mid       # SMF type 1
tables/<name>.ron                # goods, equipment, ships, governments, star tables, handcoded systems
text/<name>.ron                  # FF-code-escaped strings, mission/BBS text
missions/<name>.ron
journal/<name>.ron
README.md                        # format summary (generated)
```

The exact directory layout is versioned by `manifest.format_version`; readers reject unknown major versions and warn on minor versions.

### 4.2 Manifest

```json
{
  "format_version": 1,
  "game_version": "ffe-floppy-1.02-en",
  "source": {
    "executable": {"path": "FIRSTENC.EXE", "size": 2097152, "sha256": "d919..."},
    "files": [{"path": "MISSION.DAT", "size": 779951, "sha256": "3cf3..."}]
  },
  "tool": {"name": "ffe-tools", "version": "0.1.0"},
  "palette": "palettes/base.json",
  "asset_ids": ["base:texture/0", "base:model/38", "..."],
  "files": [{"path": "textures/base:texture/400.png", "size": 1234, "sha256": "..."}]
}
```

- `game_version` is the detected version key.
- `source` records the exact hashes used; this is how staleness is detected.
- `files` is sorted by path and includes every file except `manifest.json` itself (manifest hash is not self-referential; the ZIP hash can be recorded in a sidecar if needed).
- `asset_ids` is the canonical list used by `upscaled.bin`; it is independent of file paths and ZIP layout.

### 4.3 Determinism

- ZIP entries sorted by path; fixed DOS timestamp (1980-01-01 00:00:00); fixed compression level; no extra fields/comment; manifest first.
- PNG: fixed filter/compression, no `tIME`/text chunks; indexed PNGs preserve palette order; RGBA only where required.
- WAV/MIDI: canonical headers, no padding variance.
- RON: canonical serializer (field order from the Rust struct, arrays in source order, deterministic float formatting, no map iteration).
- JSON: canonical (sorted keys, no insignificant whitespace, fixed number formatting).
- The same input + tool version must produce a byte-identical `data.bin`; a test builds twice and compares SHA-256.

### 4.4 Classic requirements

- Textures are **indexed PNG** (`PLTE`, optional `tRNS`) with the palette index values preserved; a sidecar JSON records the 6-bit palette and transparent index. RGBA is derived only for export/upscale.
- Fonts are an atlas of the original glyph bitmaps + metrics; the Classic text renderer uses the atlas, not a TTF.
- Models preserve the command stream in `ffe.ron`; the GLB is for tools/HD and must not be the only representation.
- Text preserves FF codes via a documented escape; the core expands them, not the renderer.
- Palette is 6-bit; do not bake 8-bit values into Classic assets.

### 4.5 Version detection and staleness

Implemented precursor: `ffe-tools inspect-game` recognizes the two exact supplied
executable hashes and checks required files. It does not build/read a cache or
validate importer offsets. The conversion/startup behavior below remains planned.

`ffe-tools build-data --game ./game --out data.bin`:

1. Locate `FIRSTENC.EXE` (and required files) under `--game`.
2. Hash/detect version. Known versions:
   - `ffe-floppy-1.02-en` (this snapshot): `FIRSTENC.EXE` SHA-256 `d919...`, `English Release 1.02 - 21/04/95`, `BLD_INFO.CFG` build `eng-fp`.
   - Observed CD English 1.01: SHA-256 `9957dc34...`, 2,093,056 bytes.
     This needs a validated importer profile before claiming support.
   - Patch output profiles remain unknown; package names are not executable IDs.
3. If unknown: fail with a specific message listing the expected executable name, size, hash prefix, and the supported versions; do not guess.
4. Convert to a temp file, validate the manifest and per-file hashes, atomically rename to `data.bin`.
5. `--force` rebuilds; `--check` only verifies; `--jobs` parallelises.

At startup, `ffe-rs.exe`:

- If `data.bin` exists and is current (manifest source hashes match the detected `game/` or the recorded source is explicitly accepted), load it.
- If `data.bin` is missing/outdated and `./game/` exists next to the exe, run the same `ffe-convert` path with progress output, then start.
- If neither exists, show a clear message: where to put the player's files, the supported version, and the `ffe-tools build-data` command.
- After conversion, `./game/` is not needed at runtime.

### 4.6 CD version

The supplied CD has now been inspected; see [GAME_FILES.md](GAME_FILES.md).
It has an English 1.01 executable and `DATA/` with 323 AVIs and 323 companion
BMPs, not a verified `bmp_8/` directory. Its common cabins/effects/music/journal
match the floppy installation. Source release detection remains independent
from the selected JJFFE simulation behavior.

- Each profile needs an executable hash, content requirements and verified layout.
  Missing optional videos are different from missing required gameplay content.
- Initially read an installed/extracted directory in `game/`; raw BIN/CUE import
  is deferred. Do not execute installers or modify player files at startup.
- Mixed/unknown layouts must explain detected hashes and missing content. Match
  content identities, not unrelated installer metadata. Patch-derived profiles
  are added only after their installed output is examined.

---

## 5. `upscaled.bin`

### 5.1 Container

```
manifest.json
assets/<asset_id>.png           # RGBA, exact target size
assets/<asset_id>.normal.png    # optional material maps
assets/<asset_id>.roughness.png
assets/<asset_id>.emissive.png
```

### 5.2 Manifest

```json
{
  "format_version": 1,
  "source_game_version": "ffe-floppy-1.02-en",
  "asset_id_list_hash": "sha256:...",
  "assets": [
    {
      "id": "base:texture/400",
      "source_size": [64, 64],
      "target_size": [256, 256],
      "scale": 4.0,
      "file": "assets/base_texture_400.png",
      "provenance": "derived-from-original",
      "maps": {
        "normal": "assets/base_texture_400.normal.png",
        "roughness": "assets/base_texture_400.roughness.png",
        "emissive": "assets/base_texture_400.emissive.png"
      }
    }
  ],
  "tool": {"name": "ffe-tools", "version": "0.1.0"}
}
```

- `asset_id_list_hash` is a SHA-256 over the sorted list of source asset IDs + original sizes + kinds, not over `data.bin` bytes. A pack is valid for any `data.bin` with the same source game version and asset list.
- `provenance` is required: `derived-from-original` or `original-work`.
- Optional material maps are tied to the same asset ID; a missing map falls back per map.
- `target_size` is exact; the runtime requires it to match (or uses the PNG's dimensions after validation).

### 5.3 Runtime

- The `HD textures` option loads `upscaled.bin` if present and valid; each asset lookup falls back to `data.bin` if absent/invalid; a missing/corrupt pack shows a non-fatal message and disables only the HD feature.
- Classic ignores `upscaled.bin` entirely.
- Material maps are used only by the `material_maps` feature and only when the asset provides them.
- The loader verifies `source_game_version` and `asset_id_list_hash`; stale/mismatched packs are rejected with a specific message (not silently mixed).

### 5.4 Upscaling workflow

1. `ffe-tools export-upscale --game ./game --data data.bin --out to-be-upscaled` writes RGBA PNGs (palette applied, transparency handled) for every asset that benefits, plus `manifest.json` with ID, original size, target size, scale, transparency mode. It writes `excluded.json` explaining exclusions.
2. The player runs an external upscaler over `to-be-upscaled/` into `upscaled/` with the same filenames. `ffe-tools upscale --cmd "<template>"` can drive an installed tool (template placeholders `{in}`, `{out}`, `{scale}`).
3. Export `images/`, manifest, `sizes.csv`, offline contact sheet and exclusions.
   Recommend explicit sizes for a 3840 x 2160 display profile using asset screen
   coverage; do not make every asset 4K. See [CLAUDE.MD](../CLAUDE.MD).
4. `pack-upscaled` validates IDs, dimensions, aspect/alpha and checksums. Missing
   overrides are allowed and reported; invalid images are errors. Resizing is an
   explicit separate step, not a silent correction.
5. Both archives and working art stay local and untracked. Image replacement
   never changes collision, ship stats or other gameplay content. V1 packs do not
   include meshes, scripts or material maps; later schema versions may add them.

**Exclusion rule:** exclude assets that are not sampled as colour or whose values are data, not appearance:

- palettes and lookup tables;
- height/noise/roughness-like data textures used as inputs, not displayed;
- masks/collision data and font metrics;
- zero/one-pixel sentinels and assets with no visual output.

Include textures/sprites/HUD/fonts/backgrounds that are displayed. The manifest marks each excluded asset with a reason; the rule is documented in `docs/FORMATS.md`.

### 5.5 Resize and alpha

- Target size defaults to `round(original * scale)` with scale 4.0; global `--scale` and per-asset overrides.
- If the upscaler output differs from target, resize with Lanczos3 (high quality), preserving alpha; log source size, output size, target size, method.
- Aspect-ratio mismatch beyond tolerance (e.g. 1 pixel) is an error, not an auto-crop.
- Alpha is preserved; no premultiplied-alpha surprises. For indexed source assets, transparency is exported as alpha 0 on the transparent index; fully transparent pixels keep RGB 0 to avoid halo artifacts (document).

---

## 6. Startup and configuration

- Search order for `game/`: next to exe, then cwd; `--game` overrides.
- Search order for `data.bin`: next to exe, then cwd; `--data` overrides.
- `upscaled.bin`: next to exe, then cwd; `--upscaled` overrides.
- Config: `ffe-rs.toml` next to the exe (or `--config`); defaults are Classic.
- CLI flags override config for the run; the master button persists the selection.
- Errors are actionable: missing files, unsupported version, corrupt ZIP/PNG/model, mod validation errors, feature capability.

---

## 7. Mod system (deferred; not v1)

- All content is data, never code. The core loads it into a registry with stable IDs.
- Mods can add, override, patch. Load order deterministic; conflicts reported with both sources.
- Procedural galaxy safety test as §3.4.
- `ffe-tools validate-mod` validates types/fields, dependencies, asset collisions, schema; `new-mod` scaffolds; dev-mode hot reload.
- `docs/MODDING.md` will include tested examples: add a ship, a star system, a planet/station, a trade good, a mission template.
- No gameplay mods or scripting in v1. Earlier template/script-host sketches are
  future research only; do not build these abstractions for image replacement.
  Any future gameplay customization must remain separate from Enhanced graphics.

---

## 8. Packaging and CI

- Windows x64 first. CRT/build choices need verification; Linux is later scope.
- GitHub Actions:
  - CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `python ci/check_forbidden.py`.
  - Future release ZIP: engine/tools, input-placement instructions and applicable
    licenses. No original content, converted caches, derived image/music packs,
    reference source or private saves. Create `game/` locally for player files.
- The historical `upscaled.bin` LFS attribute is not a distribution contract.
- CI rejects local game/reference/release folders, caches, packs and captures;
  `python -m unittest ci.test_check_forbidden` tests the guardrails.

---

## 9. Historical detailed proposals

The following list is retained as design topics, not blocking questions.
Owner decisions and scope are recorded in [CLAUDE.MD](../CLAUDE.MD).

1. Approve the crate layout and dependency direction.
2. Approve RON for content tables/text with canonical serialization, `mod.toml` for mods, JSON for manifests/schemas (see `docs/FORMATS.md`).
3. Approve glTF 2.0 GLB + `extras.ffe` for models, with a semantic `ffe.ron` sidecar for exact Classic command streams.
4. Approve the `data.bin` manifest/`upscaled.bin` manifest fields and the asset-ID scheme.
5. Approve the auto-convert-on-start behaviour and the “game/ not needed after conversion” rule.
6. Approve the version-detection policy (only known hashes; specific error otherwise).
7. Approve no scripting in v1 and Rhai as the future option, or choose Lua.
8. Approve the mod layering and galaxy-safety test design.

Technical implementation of the first slice is approved; no design sketch
below a deferred heading expands that scope or authorizes redistribution.
