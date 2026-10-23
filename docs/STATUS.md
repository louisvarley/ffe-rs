# Status and handoff

Authoritative, current status of the reimplementation. This file supersedes the
status snippets in `README.md`, `CLAUDE.MD` and `docs/PORT_LOG.md` where they
disagree with it. Read this first, then `docs/MODEL_COMMANDS.md` for the opcode
coverage and `docs/notes/` for the feature records.

## How to read this repo

- `CLAUDE.MD` — owner decisions, scope, phases/exit gates, and per-milestone narrative.
- `docs/PORT_LOG.md` — append-only record of decisions, discrepancies, trace hashes, newest first.
- `docs/FORMATS.md` — the implemented partial `data.bin` cache contract (v1..v10) and the model/command/vertex/normal/depth helper semantics.
- `docs/ARCHITECTURE.md` — proposed workspace, crate responsibilities, determinism rules.
- `docs/INVENTORY.md` — reference-source inventory: formats, numeric model, timing, gotchas.
- `docs/KNOWN_ISSUES.md` — FFED3D-vs-JJFFE differences and port gaps.
- `docs/DEVIATIONS.md` — register of intentional deviations (approved / implemented / rejected).
- `docs/CONVENTIONS.md` — code conventions for this workspace.
- `docs/REFERENCE_HARNESS.md` — how to add a differential test against the executed JJFFE reference.
- `docs/MODEL_EXECUTION_PLAN.md` — design record for the v0.11 depth/point/line work and its follow-ups.
- `docs/MODEL_COMMANDS.md` — full F1772 opcode map and current Rust coverage.
- `docs/notes/` — per-feature records: polygons, textures, child models,
  thrusters, animation, views, text/labels and curved surfaces.

## Verdict on handoff readiness

**READY.** The model port is as complete as the original allows: **449 of 466
table slots execute natively**, all 448 the original also completes match it in
command order, **440 match with their children run**, and all 454 comparable
vertex programs match including each vertex entry's screen and view position.
The 17 slots left are the 15 whose external vertex references the original
resolves through null pointers (it crashes on them standalone), model 89's
release-data difference and the null table entry at 149. Two more slots (94,
148) execute standalone but stop a child scene on the original's own undefined
child state.

Since v0.43 the flight view uses that port for real: the original ships
(cache v11 names, v12 statistics), the original's views and console view
buttons, cockpit eyes and turrets, verified thruster glows with flickering
plumes, animated parts and the station spinning at its original rate.

What is *not* done is presentation and gameplay, in this order of readiness:

1. Drawing features the executor already queues but the viewer does not draw:
   text (opcode 10) and labels (18) need the message-string handlers and the
   font models; curved surfaces (31) need F1874/F1876. **This is the next
   milestone (below).**
2. Rasterisation fidelity: flat polygon fills use a diagnostic scanline fill
   (the original's F1795/F1796/F1797 spans), lines use a stand-in (F1616), and
   curves draw a stand-in cubic (F1811). These need a pixel-level oracle, not
   the current command/queue one. Also part of the next milestone.
3. The gameplay slice itself: flight physics, economy, missions, saves and
   audio are still the synthetic scaffolding described in `CLAUDE.MD`; the
   model port is not a substitute for that port.

## Getting to a verified state

```powershell
cargo build --release --locked -p ffe-tools -p ffe-rs
.\tools\reference\build-smoke.ps1 -Nasm .\.cache\tools\nasm-3.01\nasm.exe
.\target\release\ffe-tools.exe build-data --game .\game --out .\data-new.bin
Move-Item -Force .\data-new.bin .\data.bin
python .\tools\reference\verify-all.py
```

`verify-all.py` checks that every input exists first and then runs all 29
oracles (about five minutes), printing one line each and a final pass/fail. It
needs the player's own files, NASM and the MSVC x86 tools; none of it is public
CI. Rebuild the reference after any change to `tools/reference/`, and note that
the child oracle's trace SHA-256 changes with every rebuild (see below).

## Milestone status

"Code" = implemented in `crates/`. "Tests" = synthetic tests that run in `cargo
test --workspace --locked`. "Reference" = a differential comparison against the
executed JJFFE binary, driven by a `tools/reference/verify-*.py` script and an
`#[ignore]`d Rust test. Trace hashes are the SHA-256 of the disposable JSONL
capture produced by two repeat runs of the local reference.

| Milestone | Code | Tests | Reference (cases) | Trace SHA-256 | Notes |
|---|---|---|---|---|---|
| v0.0 tools: inspect/export/cache | `ffe-tools`, `ffe-convert`, `ffe-content` | `inspect.rs`, `reference.rs`, `cache.rs`, `decoders.rs`, `embedded.rs`, `sine.rs` | `verify-assets.py` | — | exact floppy 1.02 + CD 1.01 profiles; cache v6 / 385 assets |
| v0.0 core: RNG/timer/clock | `ffe-core` (lib.rs) | `primitives.rs` | `verify-core.py` (1536 RNG, 60 timer, 238 clock) | `accbe92f9…` | day fraction wraps at 2^32, not 49,710 |
| v0.1 experimental playable | `ffe-rs` | `playable.rs`, `indexed.rs`, `input.rs` | none (synthetic gameplay) | — | synthetic flight/market/save; **not faithful** |
| v0.2 font/console | `ffe-rs::indexed`/`screen` | 5 + 3 | `verify-font.py` (696 glyph + 28 blit) | `5642024b…`, `0cd9357e…` | cache v2 |
| v0.3 vertex interpreter | `ffe-core::model` | `model.rs` | `verify-models.py` (13,632) | `7432278a…` | models 74/75; cache v3 |
| v0.4 command decode + helpers | `ffe-core::model_commands` | `model_commands.rs` | `verify-commands.py` (1315/360/65536/32) | `2c0c1a67…` | cache v4 |
| v0.5 normals | `ffe-core::model_normals` | `model_normals.rs` | `verify-normals.py` (26,712) | `55b2060d…` | cache v5 |
| v0.6 trig/transforms/visibility | `ffe-core::model_math` | `model_math.rs` | `verify-transforms.py`, `verify-rotation-operands.py`, `verify-visibility.py` | `b7ea38a8…`, `08ce2914…`, `d7e8cc0b…` | cache v6 |
| v0.7 bounded executor | `ffe-core::model_execute` | `model_execute.rs` | `verify-execution.py` (900) | `d2a76e9c…` | `execute` (no live vertices) still rejects opcode 6 |
| v0.8 depth key | `ffe-core::model_depth::key` | `model_depth.rs` | `verify-depth.py` (17,303) | `61fffc80…` | F1777 |
| v0.9 depth commands + order | `ffe-core::model_depth`, `model_order` | `model_depth.rs`, `model_order.rs` | `verify-order.py` (11,700 + 128) | `ab5da346…`, `4ab5453e…` | F1696/F1780/F1788; abstract tokens |
| v0.10 projection + live slots | `ffe-core::model_instance` | `model_instance.rs` (5), `instance_reference.rs` (2, ignored) | `verify-instance.py` (2,197 + 8,192) | `c794945388b5bae086ceb6cb8ad615d17d954a85e2b200bd89367166c1890f70` (projection), `6fe8a6470d42d884d11e54f1cc9db26107794b5220cbfbb051a6ebf826e422cc` (live vertices) | F1479 + F1470/F1471/F1478 |
| v0.11 depth composition | `ffe-core::model_execute::execute_live` | `model_execute.rs` (12), `execution_reference.rs` (+1, ignored) | local `executed_reference_station_depth_composition_advances`; existing depth/order/executor traces | — (no new composed trace) | composes F1696/F1780/F1788 with `model_instance`; `Execution.ordered`; real stations advance past opcode 6 |
| v0.12 composed-order oracle | `model_execute::ordered_keys`, `model_order::finish_with_keys` | `order_reference.rs` (+1, ignored) | `verify-order.py` composed-order (1 program, order `[2,3,1]`, keys `[9,9,0]`) | `ce0d1b4fc55e2ac5533aa848dbe4642da8d0c00161de7959b561e184af021d2a` | composition of original F1696/F1777/F1780/F1788 with markers; not F1772/face handlers/pixels |
| v0.13 point/line emission | `model_commands` (1/2), `model_execute::Primitive` | `model_commands.rs` (6), `model_execute.rs` (14), `point_line_reference.rs` (1, ignored) | `verify-point-line.py` (2 events) | `afa955be148192eaf2be79fb0b818386eafbc8d008e67e3fc56c3b7dd3e758a3` | operand events from original F1698/F1702; ordered with faces; not geometry/pixels |
| v0.14 text/submodel operands | `model_commands` (`Text`/`Submodel`), `model_execute` (`texts`/`submodels`) | `model_execute.rs` (+3), `text_submodel_reference.rs` (1, ignored) | `verify-text-submodel.py` (7 cases, 11 tamper fields) | `5ba06002572b4395481a3651d84c588dbdec4285a210b8fe7e4b0d1a8041f73d` (CRLF; Linux LF `b095c38c…`) | F1747 text; F1749/F1750 child-model call (opcode 14 is **not** text); operand events, not in `ordered`; glyphs/children not executed |
| v0.15 colour command | `model_commands::Colour`, `model_execute` (`colours`, `normal_zero_colour`), `VertexContext::colour` | `model_execute.rs` (+2), `colour_reference.rs` (1, ignored) | `verify-colour.py` (32 cases, 9 tamper fields) | `7d23226568032c92b4f062d4553cee39f4c89f8e51b742e9f958fb48ddaaa4b4` (CRLF; Linux LF `95383fff…`) | F1762; 6- or 20-byte record (decoder size fixed); instance or normal-0 colour; textured normal-0 writes error |
| v0.16 base transforms; stations complete | `model_math::rotate_command` (base forms), `VertexContext::{basis, base_handedness, handedness}`, top-level group close in `execute_live` | `model_math.rs`, `model_execute.rs` (updated), `base_transform_reference.rs` (1, ignored) | `verify-base-transform.py` (6,048 cases, 8 tamper fields) | `c9a87a6d023bacf3900dc0587285ac7522abc6353a80b9b6b62db43088c4a29c` (CRLF; Linux LF `03070a9b…`) | F1741/F1739; real stations 74/75 run to completion on synthetic inputs, not yet compared whole-program |
| v0.17 whole-station oracle | (no new core code) | `station_reference.rs` (1, ignored) | `verify-station.py` (models 74/75, 10 tamper fields) | `0ea8f4a7b9df2616dc6537a032b8527ab3ca0fde89e63856c260d23392f722b2` (CRLF; Linux LF `b9e3c831…`) | real station streams through original F1772; command-order events and final state match; render order/glyphs/children/pixels not covered |
| v0.18 face draw queue | `model_execute` (face queue rule, `Execution.queue`, `InstanceContext::textures`) | `model_execute.rs` (+1), `station_reference.rs` (+1, ignored) | `verify-station-queue.py` (12 cases, 7 tamper fields) | `35e55a26689eb1e520dffa6b4c36f4a25dff36066d1b4c9ba969c7345425fb56` (CRLF; Linux LF `350b6d2c…`) | F1708/F1712/F1709/F1713/F1714 queueing; faces in `ordered` now match the original draw order; points/lines not yet |
| v0.19 model census | `ffe-convert::model_extents` (generated), `model_command_bytes`, `ffe-tools model-census` | `embedded.rs` (+1, updated) | descriptor shapes cross-checked for all 466 slots (floppy 1.02) | — | 180 of 466 models execute natively (synthetic inputs, unverified); ranked blockers in `docs/MODEL_COMMANDS.md` |
| v0.20 whole-table comparison | (no new core code) | `station_models_reference.rs` (1, ignored) | `verify-station-models.py` (all 466 slots; 4 tamper fields) | `3e04ba2e…` (Linux LF) | all 180 natively executed models match the original F1772 in command order; 13 slots crash the original in the synthetic setup |
| v0.21 complex polygons | `model_commands::Polygon`, `model_execute` (`PolygonEmission`, `Primitive::Polygon`) | `model_execute.rs` (+1) | `verify-station-models.py` (polygon keys, event 11) | — | opcode 5 decoded/keyed/queued; segments not executed; 229 of 466 models execute and match |
| v0.22 jittered vertices | `model::{jitter, evaluate_with_jitter}`, opcodes 7/8, `Vertices::{jitter, set_jitter}` | `model.rs` (+1), `jitter_reference.rs` (1, ignored) | `verify-jitter.py` (118 evaluations, 59 models) | `0fc5445f…` (Linux LF) | F1476 state caller-owned (open question in KNOWN_ISSUES); 232 of 466 models execute and match |
| v0.23 thrusters | `model_commands::Thruster`, `ThrusterEmission` | `model_execute.rs` (+1) | `verify-station-models.py` (command flow after thrusters) | — | opcode 9 operand events; glow queueing not ported; 245 of 466 models execute and match |
| v0.24 coloured child calls | `Submodel::{material, scale}`, `ChildScale`, opcode 27 | `model_execute.rs` (+1) | `verify-station-models.py` (F1687 cull hook) | — | 277 of 466 models execute; all the original completes match |
| v0.25 curved surfaces | `model_commands::Surface`, `SurfaceEmission` | `model_execute.rs` (+1) | `verify-station-models.py` | — | opcode 31 operand events (F1823 entry list); 312 of 466 models execute; all the original completes match |
| v0.26 nested depth blocks | `Order::nested`, opcode 21, `Execution.blocks`, `QueueEvent::Nested` | `model_order.rs` (+1), `model_execute.rs` (+1) | `verify-station-models.py` (block keys) | — | 317 of 466 models execute; all the original completes match; nested sort composition not yet oracle-checked |
| v0.27 trailing declaration data | `model_commands::decode` (longest self-contained prefix), `Program::trailing` | `model_commands.rs` (+1) | `verify-station-models.py` | — | 368 of 466 models execute; all the original completes match |
| v0.28 opcodes 17/24/25 | `Beam`, `Chain`, `VertexContext::direction`, opcode 25 basis reset | `model_execute.rs` (+1) | `verify-station-models.py` | — | 403 of 466 models execute; all the original completes match; opcode 25 rotation needs the sqrt table |
| v0.29 curve vertices; vertex oracle | vertex opcodes 0x15/0x16 in `model::ObjectCache` | `model.rs` (+1), `vertex_program_reference.rs` (1, ignored) | `verify-vertex-programs.py` (445 slots) | `b3ed1b7d…` (Linux LF) | every supported vertex program matches; model 361 data differs between JJFFE and floppy 1.02; 405 of 466 models execute |
| v0.30 opcode 30 frame restore | `model_execute` opcode 30 (bit 15) | `model_execute.rs` (+1) | `verify-station-models.py` (408 models) | `1c46a942…` (Linux LF) | 413 of 466 models execute; all 408 the original completes match; light points and vertex-base offsets remain errors |
| v0.31 opcode 22 curves | `model_execute` opcode 22, `Primitive::Spline` | `model_execute.rs` (+1) | `verify-station-models.py` (418 models; 21 curve entries) | `6ffe0c7b…` (Linux LF) | key, screen points and colour of every queued curve match; F1811 drawing not ported; 423 of 466 models execute |
| v0.32 offset curve vertices | vertex opcodes 0x17/0x18 via `ObjectCache::curve` | `model.rs` (+1) | `verify-vertex-programs.py` (452 slots) | `b3ed1b7d…` (Linux LF) | 452 of 452 vertex programs match; 423 of 466 models execute |
| v0.33 vertex-defined basis | opcode 28 `0xc000`, `model_math::{square_root, length, fraction, vertex_basis}`, `Vertices::object` | `model_math.rs` (+1), `model_execute.rs` (+1) | `verify-station-models.py` (430 models) | `6ffe0c7b…` (Linux LF) | non-trivial bases of models 351-359 match; 435 of 466 models execute |
| v0.34 screen labels | opcode 18, `Primitive::Label` | `model_execute.rs` (+1) | `verify-station-models.py` (432 models) | `a80247eb…` (Linux LF) | label key and screen point match (1 queued); model 89 release data differs; 437 of 466 models execute |
| v0.35 empty light points | opcode 30 `0x801e` with a zero count | `model_execute.rs` (extended) | `verify-station-models.py` (433 models) | `a80247eb…` (Linux LF) | non-zero counts need DATA_007820..7828; 438 of 466 models execute |
| v0.36 sound requests | opcode 15, `SoundEmission` | `model_execute.rs` (+1) | `verify-station-models.py` (434 models) | `4bcf902e…` (Linux LF) | sample and volume match; playback not ported; 439 of 466 models execute |
| v0.37 cache v7; F6 model viewer | `ffe-tools build-data` (all decodable models), `ffe_rs::model_viewer`, `Screen::draw_model_viewer` | `model_viewer.rs` (1), cache/sine version tests | executor output only (diagnostic camera/colours) | — | first visible models: 420 of 439 render; textures grey; polygons/surfaces/text/children not drawn |
| v0.38 polygons, textures, child models | `model_polygon`, `model_texture`, `execute_scene`, cache v8 | `model_polygon.rs`, `model_texture.rs`, `model_children.rs` | `verify-polygon.py`, `verify-textures.py`, `verify-child-models.py`, `verify-station-models.py` (443) | `fab32ea5…`, `640cc781…`/`65922b3c…`, `eaee2ad1…`, `b4c3aecf…` | 0 mismatched everywhere; viewer does not run children yet; upper palette unknown |
| v0.39 the real station in flight | `ModelViewer::render_scene`, `Screen::draw_station` | `model_viewer.rs`, `screen.rs` | none (diagnostic camera) | — | the flight view draws original model 74; mechanics stay synthetic |
| v0.40 original lighting and colours | `model_lighting`, cache v9 | `model_lighting.rs` | (lighting tables imported) | — | the default light, the shade table and F1326 colour packing |
| v0.41 child models in the viewer | `execute_scene`, cache v10 | `model_execute.rs`, `model_viewer.rs` | `verify-child-models.py` (417) | `eaee2ad1…` | the station's rings and side pods draw; 424 of 456 models render |
| v0.42 screen-space midpoints | `model::{evaluate_entries, screen_midpoint}`, external references in the thruster/beam/chain | `model.rs`, `model_commands.rs` | `verify-vertex-programs.py` (454), `verify-child-models.py` (437), `verify-station-models.py` (445) | `1afe0c45…`, `cbe87dd9…`, `34c1bf26…` | vertex opcodes 3/4 and 13/14; census 446 of 466 |
| v0.43 zero normals and the return record | `model_normals::validate_records`, `model_commands::decode` | `model_normals.rs`, `model_commands.rs` | `verify-station-models.py` (448), `verify-child-models.py` (440) | `34c1bf26…`, `cbe87dd9…` | F1754 returns for any opcode-0 word; F1480 needs no records for a count of zero; census 449 of 466 |
| v0.44 ships and views | `ffe_rs::ships`, `ffe_rs::camera`, cache v11 (model names) | `views.rs` (7) | none (UI; research in `docs/notes/views.md`) | — | real craft (slots 14..=70, Page Up/Down); F1 cycle, F6/F7/F9, console icons; external camera exact |
| v0.45 thruster glows | `ffe-core::model_thruster`, opcode 9 queueing, `Shape::Glow` | `model_thruster.rs` (5), `model_viewer.rs` (+1) | `verify-thrusters.py` (12,288) + thruster events in the whole-table oracles | `5a8eddfe…` | F1738/F1704/F1805; the first attempt's screen-position difference does not reproduce |
| v0.45 animation inputs, station spin | `ffe-core::model_animation` | `model_animation.rs` (6) | `verify-animation.py` (2,640 rotation + 4,096 orbital + F1537/F1680/F1683 set-ups) | `10ed4039…`, `2ab63f4d…` | global 0 = GameTics >> 10, global 1 gear/door (F865); F866/F1675 spin, model 74 at speed 10 |
| v0.46 ship statistics, eyes, turrets | cache v12 `base:ship/stats`, `ShipStats`, `camera` eye/turret views | `views.rs` | none (data imported byte-exact; views from the assembly) | — | thrust limits feed global 15; F8 turret views for > 2 gun mounts |
| v0.48c surface resolution | `SurfaceEmission::{origin, axes}` (F1874) | `model_execute.rs` (+1) | census: 40 surfaces in 39 models, 224 entries | — | origin (F1470 view) and the three axis vectors ported; F1876's patch rendering not ported (see `docs/notes/surfaces.md`) |
| v0.48b 3D text | `ffe-convert::glyph_programs`, `GlyphSet`, `ModelViewer::draw_texts`, cache v14 | `text.rs` (1), `glyphs.rs` (1, ignored), `embedded.rs` (+1), `cache.rs` (+1) | census: all three font sets import; models 15/30/66/453 draw text with nothing skipped | — | F1747: each character's program runs through `execute_live` in an F1740 frame, its return word sets the next pen; `0x3016`/`0x3022` expand the object string |
| v0.48a labels, full string table | `ModelViewer::model_strings`, `Shape::Label`, scanned `decode_model_names`, cache v13 | `model_viewer.rs` (+1), `embedded.rs` (+1), `text_census.rs` (1, ignored) | census: all 6 label codes resolve | — | F1812/F1583 labels drawn at the queued point in font 15; string table scanned to 219 entries (209 in v11/v12 left six codes unresolved) |
| v0.47 console clicks, plume flicker | `Screen::logical_point`, `AnimationInputs::jitter`, `Frame::jitter`, viewer `cycle_thrust` | `views.rs` (+1), `model_viewer.rs` (+1) | none (presentation) | — | view icons now take clicks at any window size; F1476 stream runs across frames so plumes flicker; viewer E key sets engine thrust |

Facts verified on 2026-10-22 (test counts and the smoke launch re-checked
for v0.47; the oracle figures are from the last full `verify-all.py` run):

- `cargo test --workspace --locked` passes, as do `cargo fmt --check` and
  `cargo clippy -D warnings`. 242 `#[test]` functions total; 36 are `#[ignore]`
  local-only tests; one more is `#[cfg(unix)]`. On Windows that is
  **205 runnable synthetic tests** (206 on Linux).
- `ffe-rs.exe --root . --smoke-frames 30` presents frames with the local
  cache v14.
- `verify-all.py`: **29 of 29 oracles pass** (the 26 below plus
  `verify-thrusters.py`, the deep child-model run and `verify-animation.py`).
- `verify-station-models.py`: all 448 models that both sides complete match the
  original command flow and final state; `verify-child-models.py`: all 440
  models that execute match, with 1330 child runs and no mismatch;
  `verify-vertex-programs.py`: all 454 comparable vertex programs match, now
  including every vertex entry's screen position and view position.
  The child trace's SHA-256 moves with every rebuild of the reference: it
  contains event 26, the original's uninitialised base handedness, which the
  comparison excludes. Compare the reported *numbers*, not that one hash.
- `ffe-tools model-census --game game` (floppy 1.02): 449 of 466 slots execute
  natively with synthetic inputs; see the blocker table in `docs/MODEL_COMMANDS.md`.
- `tools/reference/build-linux.sh` builds `jjffe-smoke`/`jjffe-commands` under
  `.cache/jjffe-reference-linux/`; their traces, hashed with CRLF endings, match
  every Windows hash in the table above.
- `python tools/reference/verify-instance.py --reference .cache/jjffe-reference/jjffe-smoke.exe`
  passes both scenarios (repeat/tamper/empty all behave as expected).
- The local-only real-station test now requires both programs to complete:
  model 74 runs 77 commands (25 ordered primitives, 4 texts, 5 submodel calls),
  model 75 runs 100 (74 ordered, 1 text, 2 submodel calls, 2 colours), with
  descending render keys. Inputs are synthetic (identity basis, visible
  normals, zero position).
- The local reference build (`jjffe-smoke.exe`, `jjffe-commands.exe`) and its
  `build-manifest.json` exist under `.cache/jjffe-reference/`.
- `game/` has 71 files; `jjffe-master/` and `ffed3d-master/` are present; root
  `data.bin` is cache v14 (1781 assets, 462 of 466 models, 219 model strings,
  3 glyph sets); `.cache/data-v1..v5-backup.bin`, `data-v10-backup.bin`,
  `data-v12-backup.bin` and `data-v13-backup.bin` are retained.
- The three glyph sets import with 95, 52 and 95 programs (3,008 / 874 / 624
  bytes); every program decodes and names an in-range pen vertex.
- `text_census` (local-only): 236 text records with 136 codes (130 model
  strings, all resolvable; `0x3016`, `0x3022`, `0x9c00`-`0x9c03` need handlers);
  glyph sets 0/1/2 = model slots 0/1/2; 6 labels, all model strings.
- `verify-assets.py` checks the whole v10 cache against the executable's own
  bytes and the JJFFE pointer provenance. **34 of the 462 imported models
  differ from the JJFFE 2.8a7 declarations** by a few bytes each: the mod's
  model data is not the floppy 1.02 release's. The port imports the player's
  executable (the original game's data); the local reference build embeds the
  mod's, so the differential oracles compare the models whose bytes agree, and
  they all do.
- `python tools/reference/verify-all.py` runs the whole set in about five
  minutes on this checkout.

## Next milestone: v0.48 "Every model fully drawn"

**Goal.** The F6 viewer and the flight view draw every primitive the executor
queues, with the original's own rasterisers, verified at the pixel level. Today
the viewer still reports "NOT DRAWN YET" for labels, skips text and curved
surfaces, and fills/lines/curves with diagnostic stand-ins. Gameplay is out of
scope (see "After this milestone").

**Work, in order.** Each step lands separately with its notes file, synthetic
tests and an oracle, as the earlier milestones did.

1. **Text (opcode 10) and labels (opcode 18).** Ships' registrations and
   station names.
   - **Done (v0.48a):** the census and F1344 dispatch are recorded in
     `docs/notes/text.md`; cache v13 carries the executable's full 219-entry
     string table (v11/v12 held JJFFE's declared 209, so six codes above 208
     could not resolve); opcode 18 labels now draw at their queued point in
     the original font (F1812 -> F1583, colour 15), all six of them resolving.
   - **Done (v0.48b):** cache v14 imports each font model's per-character
     programs; F1747's placement is ported (F1740 frame, `execute_live` per
     glyph, return-word pen), and `0x3016`/`0x3022` expand the object string.
     Ships and the station now draw their registrations; models 15, 30, 66 and
     453 render with nothing skipped.
   - **Left in this step:** control codes below 0x20 (the font's `+0x3c`
     component anchors, needed by the logo/station multi-line strings),
     `0x3022`'s wrap (`DATA_007648` width table), `0x9c00`-`0x9c03`
     (`DATA_005104`), depth interleaving and glyph lighting.
   - Oracle: the existing `trace_text_*`/`trace_label_*` probe hooks, extended
     to a whole-table text/label comparison (not yet run for text).
2. **Curved surfaces (opcode 31).** The rounded hulls and station parts.
   - **Started (v0.48c):** F1874's resolution is ported (origin through F1470,
     the three axis vectors) and tested; the census counts 40 surfaces in 39
     models (224 entries). `docs/notes/surfaces.md` records the full F1874 →
     F1876 contract, F1876's size (~1,580 lines), its helper set and the
     two constant topology tables.
   - **Next:** port F1876 bottom-up (F1875's 64-bit cross-product sign first,
     then F1816/F1820/F1823/F1824/F1825, then F1833/F1834/F1835), with scoped
     probe hooks in F1874/F1876 and a `verify-surfaces.py` whole-table oracle
     before the quads are drawn.
3. **Lines and curves.** F1616 (DrawLine) for F1803 entries (opcode 2 lines,
   narrow thrusters) and F1811 for opcode 22 curves, replacing the stand-ins.
4. **Pixel oracle.** Add a framebuffer mode to the reference adapter that
   draws one queued scene with the original's F1795/F1796/F1797 spans, F1647
   textures, F1616 lines and F1811 curves. Then replace the viewer's diagnostic
   scanline fill and compare whole frames byte for byte.

**Exit criteria.**
- The viewer draws all 449 executable models with no "NOT DRAWN YET" count and
  no skipped text or surfaces.
- The text/label and surface oracles report 0 mismatched across the table, and
  `verify-all.py` passes with them added.
- The pixel oracle matches station 74 and at least three ships (one with
  textures, one with curved surfaces, one with glows) at two distances each,
  byte for byte, over the fixed low palette. The upper (runtime) palette stays
  a recorded gap unless it is ported along the way.
- Census unchanged at 449 of 466 (re-run after each step; also check the CD
  1.01 executable).
- Owner check (2 minutes): in M, page to models 66 and 453 and see their name
  labels; then to the station and an Eagle and see rounded parts (and, once
  the glyph step lands, their registration text); in flight, F9 shows the
  ship's plume flickering under W.

**Risks.** Text handlers that read game state may need small pieces of
the flight/commander state that do not exist yet; record those as explicit
placeholders rather than inventing values. The pixel oracle needs the
adapter's framebuffer capture to run without the original's video set-up.

## After this milestone

The phase-3 gameplay slice (`CLAUDE.MD` section 6) replaces the synthetic
flight: the player control and thrust vector (F598/F600, which also gives the
glows their real global 15 instead of the throttle mapping), the original
flight integration, docking and the station's real position and orientation,
traced against the reference with scripted inputs. Before porting flight or
AI, settle the shared F1476 stream question in `docs/KNOWN_ISSUES.md` with the
owner. Smaller open items: the F1467 square-root table for opcode 25's
direction alignment (needed once flight feeds a direction), the
missile/combat views, the Alt-arrow nudge, and the turret angles' starting
values.

## Known stale spots

- `README.md` and the executable title still say "0.9" because the product
  version label has not been bumped (cosmetic).
- `README.md` still describes cache v11 and "26 oracles" in places; the figures
  above are current (cache v12, 29 oracles).
- The milestone numbers v0.44-v0.47 were assigned in this file after the fact;
  `docs/PORT_LOG.md` records that work by date and title.
- `README.md` still says the cache is v11 in the build section (v13 is current).
- `tests/golden/` still does not exist; it is labelled "planned" in `README.md`.
- The child oracle's trace SHA-256 is not reproducible across reference
  rebuilds; read that oracle by its counts (440 compared, 0 mismatched).
- `docs/FORMATS.md` documents the cache through v10, but several older sections
  still describe v6-era totals; the authoritative cache figures are in the
  facts above and in the cache section of `README.md`.
