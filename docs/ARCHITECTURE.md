# Proposed workspace architecture and data pipeline

Status: **proposal — awaiting approval.** No workspace has been created yet.

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
ffe-content -> ffe-convert (readers) / no ffe-core logic
ffe-core -> ffe-content (registry trait only), ffe-render-desc (emission)
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
- No `unsafe` unless justified and documented; `cargo clippy -- -D warnings`, `cargo fmt`.
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

`ffe-tools build-data --game ./game --out data.bin`:

1. Locate `FIRSTENC.EXE` (and required files) under `--game`.
2. Hash/detect version. Known versions:
   - `ffe-floppy-1.02-en` (this snapshot): `FIRSTENC.EXE` SHA-256 `d919...`, `English Release 1.02 - 21/04/95`, `BLD_INFO.CFG` build `eng-fp`.
   - `ffe-cd-...` (to be added when the user supplies a CD; hashes unknown).
3. If unknown: fail with a specific message listing the expected executable name, size, hash prefix, and the supported versions; do not guess.
4. Convert to a temp file, validate the manifest and per-file hashes, atomically rename to `data.bin`.
5. `--force` rebuilds; `--check` only verifies; `--jobs` parallelises.

At startup, `ffe-rs.exe`:

- If `data.bin` exists and is current (manifest source hashes match the detected `game/` or the recorded source is explicitly accepted), load it.
- If `data.bin` is missing/outdated and `./game/` exists next to the exe, run the same `ffe-convert` path with progress output, then start.
- If neither exists, show a clear message: where to put the player's files, the supported version, and the `ffe-tools build-data` command.
- After conversion, `./game/` is not needed at runtime.

### 4.6 CD version

Design for it now, but do not guess:

- A version descriptor includes the executable hash, the list of required files, optional external directories (`bmp_8/`, `DATA/`), and a converter layout. The converter reads external indexed BMPs/AVI if present.
- `ffe-floppy-1.02-en` uses the embedded EXE data; `ffe-cd-*` may externalise art/data. The manifest's `game_version` and per-file hashes make packs version-specific.
- Document the differences in `docs/GAME_FILES.md` when known. Until then, an unrecognised CD copy produces the “unsupported version” message.

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
3. `ffe-tools pack-upscaled --data data.bin --upscaled upscaled --out upscaled.bin` validates missing/extra files and aspect ratio, resizes each image to its exact target size with Lanczos3 when needed, preserves alpha, logs every resize, writes `upscaled.bin`.
4. `upscaled.bin` is committed via Git LFS; `to-be-upscaled/` and `upscaled/` are never committed.

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

## 7. Mod system (Phase 8 design)

- All content is data, never code. The core loads it into a registry with stable IDs.
- Mods can add, override, patch. Load order deterministic; conflicts reported with both sources.
- Procedural galaxy safety test as §3.4.
- `ffe-tools validate-mod` validates types/fields, dependencies, asset collisions, schema; `new-mod` scaffolds; dev-mode hot reload.
- `docs/MODDING.md` will include tested examples: add a ship, a star system, a planet/station, a trade good, a mission template.
- No scripting in v1. **Phase 0 recommendation:** Rust-native data + templates are sufficient for v1; if scripting is added later, use **Rhai** (small, sandboxable, deterministic) over Lua. Lua is already in FFED3D, but it is not deterministic/sandboxed by default and would make mods able to change simulation. Leave extension points: `EventHook`/`Action` data types, `MissionTemplate` conditions/actions, a `ScriptHost` trait not implemented in v1. Any scripting must be opt-in and clearly gameplay-affecting, never Enhanced.

---

## 8. Packaging and CI

- Windows `.exe` with static CRT (`+crt-static`), 64-bit; Linux if straightforward.
- GitHub Actions:
  - CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `python ci/check_forbidden.py`.
  - Release on tag: build, package `ffe-rs-<tag>-windows-x64.zip` containing exe, `upscaled.bin` (LFS checkout), empty `game/` + README, `mods/` + README, `README.txt`, `LICENSE`. No `data.bin`, no game data.
- LFS: `.gitattributes` marks `upscaled.bin`; README explains `git lfs install`, checkout, and that CI must use `lfs: true`.
- Forbidden-file check runs before build and fails if any path in `game/`, `legacy/`, `data.bin`, `to-be-upscaled/`, `upscaled/`, `extracted/`, or frame dumps is tracked.

---

## 9. Phase 0 decisions requested

1. Approve the crate layout and dependency direction.
2. Approve RON for content tables/text with canonical serialization, `mod.toml` for mods, JSON for manifests/schemas (see `docs/FORMATS.md`).
3. Approve glTF 2.0 GLB + `extras.ffe` for models, with a semantic `ffe.ron` sidecar for exact Classic command streams.
4. Approve the `data.bin` manifest/`upscaled.bin` manifest fields and the asset-ID scheme.
5. Approve the auto-convert-on-start behaviour and the “game/ not needed after conversion” rule.
6. Approve the version-detection policy (only known hashes; specific error otherwise).
7. Approve no scripting in v1 and Rhai as the future option, or choose Lua.
8. Approve the mod layering and galaxy-safety test design.

No production code will be written until Phase 0 and `docs/RENDERING_DESIGN.md` are approved.
