# Status and handoff

Authoritative, current status of the reimplementation. This file supersedes the
status snippets in `README.md`, `CLAUDE.MD` and `docs/PORT_LOG.md` where they
disagree with it. Read this first, then `docs/MODEL_EXECUTION_PLAN.md` for the
v0.11 design record and follow-ups.

## How to read this repo

- `CLAUDE.MD` — owner decisions, scope, phases/exit gates, and per-milestone narrative.
- `docs/PORT_LOG.md` — append-only record of decisions, discrepancies, trace hashes, newest first.
- `docs/FORMATS.md` — the implemented partial `data.bin` cache contract (v1..v6) and the model/command/vertex/normal/depth helper semantics.
- `docs/ARCHITECTURE.md` — proposed workspace, crate responsibilities, determinism rules.
- `docs/INVENTORY.md` — reference-source inventory: formats, numeric model, timing, gotchas.
- `docs/KNOWN_ISSUES.md` — FFED3D-vs-JJFFE differences and port gaps.
- `docs/DEVIATIONS.md` — register of intentional deviations (approved / implemented / rejected).
- `docs/CONVENTIONS.md` — code conventions for this workspace.
- `docs/REFERENCE_HARNESS.md` — how to add a differential test against the executed JJFFE reference.
- `docs/MODEL_EXECUTION_PLAN.md` — design record for the implemented v0.11–v0.13 depth/point/line work and its follow-ups.
- `docs/MODEL_COMMANDS.md` — full F1772 opcode map and current Rust coverage.

## Verdict on handoff readiness

**READY for the next model task.** The v0.10 projection/live-slot work, v0.11
depth composition, v0.12 composed-order oracle, v0.13 point/line emission and
v0.14 text/child-model operand events, v0.15 colour commands and v0.16
base-relative transforms are implemented; both real station programs run to
completion and, since v0.17, match the original F1772 run of the same programs
event for event in command order; since v0.18 their face draw queues match too, tested and recorded. The
reference adapter now also builds on Linux (`tools/reference/build-linux.sh`)
and reproduces every recorded trace hash. The next task (text, submodels, other transform
forms and colour) is scoped in `docs/MODEL_EXECUTION_PLAN.md` §8 and
`docs/MODEL_COMMANDS.md`. Remaining gaps are explicit below.

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

Facts verified on 2026-10-22:

- `cargo test --workspace --locked` passes. 203 `#[test]` functions total; 29 are
  `#[ignore]` local-only reference tests; one more is `#[cfg(unix)]`. On Windows
  that is **173 runnable synthetic tests** (174 on Linux).
- `verify-station-models.py`: all 448 models that both sides complete match the
  original command flow and final state; `verify-child-models.py`: all 440
  models that execute match, with 1330 child runs and no mismatch;
  `verify-vertex-programs.py`: all 454 comparable vertex programs match, now
  including every vertex entry's screen position and view position.
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
  `data.bin` is cache v10 with 1776 assets and 462 of 466 models;
  `.cache/data-v1..v5-backup.bin` and `data-v10-backup.bin` are retained.

## Next task

Stations are verified for command flow (v0.17) and face draw queues (v0.18);
the v0.19 census ranks what blocks the other models. Next, in order:

1. The remaining census blockers (see `docs/MODEL_COMMANDS.md`): opcode 30
   light-point tables, 30 light points and the F1467 square-root table for
   opcode 25 rotations. Five models are left, and the original cannot run four
   of them standalone either.
2. Thruster glow queueing (F1666/F1467/F1704) with point/line queueing.
3. **Polygon segments** (F1734/F1724/F1730/F1733/F1723/F1736 and their curve
   helpers) — needed for outline rasterising and for exact vertex-cache events.
4. The command-order and queue oracles cover every slot since v0.20 and v0.38
   (`verify-station-models.py`, `verify-child-models.py`).
5. Primitive colour, rasterisation, and glyph/child execution as before.

Run the census after each step; check the CD 1.01 executable with it when
available.

## Known stale spots

- None of the earlier v0.10/v0.11 discrepancies remain; they were corrected on
  2026-10-21. `README.md` and the executable title still say "0.9" because the
  product version label has not been bumped (cosmetic).
- `tests/golden/` still does not exist; it is now labelled "planned" in
  `README.md`.
