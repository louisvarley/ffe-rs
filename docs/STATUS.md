# Status and handoff

Authoritative, current status of the reimplementation. This file supersedes the
status snippets in `README.md`, `CLAUDE.MD` and `docs/PORT_LOG.md` where they
disagree with it. Read this first, then `docs/MODEL_COMMANDS.md` for the opcode
coverage and `docs/notes/` for the feature records.

## Navigation aid

The default boot plays the original's intro (`ffe_rs::front_end`/`Screen::draw_intro`,
`docs/notes/intro.md`) and then starts the original's new game
(`ffe_rs::navigation::new_game`): Ross 154, the Eagle (model 23), 1000 CR,
1 Jan 3250 12:00 and player state 0x30 on the starport's pad; `--start sol`
keeps the Sol/Mars High development sandbox and `--skip-intro` (or any scripted
capture/smoke run) starts directly. The JJFFE legal splash is not part of the
intro. `L` takes off from the open-air starport pad through the original's
F1013 `0x24` -> F1031 `0x26` -> `0x00` path (EXP-010); the earlier refusal is
gone.

`docs/NAVIGATION.md`: system and galaxy maps (Tab, G) and an autopilot to planets
and stations, for testing the generated systems. Not part of the original game.
Hyperspace jumps use the original's drive range, fuel and jump time over real star positions
(`docs/notes/hyperspace.md`; both match the executed original; the jump scene is not drawn). Docking and launching show the original's lift scenes (`ffe_core::lift`, verified on 8,192 cases against the
executed original; `docs/notes/docking.md`). The default new game's open-air
starport takeoff is the original's F1013 `0x24` -> F1031 `0x26` -> `0x00` path
over a persistent two-body pad contact (`ffe_core::traffic::Takeoff`,
`docs/notes/intro.md`), so the ship no longer needs the old refusal or a
teleport; indoor stations keep the lift. Since 2026-10-24 the docked screen is the original's station services menu and stockmarket
(`docs/notes/station.md`: the `DATA_005304` screen table, the `0x98xx` strings, the row buttons,
hot areas and keys; checked against reference (JJFFE 2.8a7) game frames; the services and shipyard roots
and the market are ported, the equipment/ship/repair lists and BBS generation are not). The
market itself is `docs/notes/market.md` (the original's price and stock formulas, daily drift,
buying and selling, cache v20; F619 and F620 match the executed original). The system's stations are drawn in flight and the player can dock
at them (D for clearance, or the autopilot), which closes the launch, fly, dock, trade
loop inside a generated system. Traffic control and the timed docking and launch stages are the
original's (`ffe_core::traffic`, `docs/notes/docking.md`; the record routines match the executed
original on 8,192 cases); the slot test and the missing lift scenes are
ffe-rs's own (EXP-005). Since 2026-10-25 the flight view draws the original's
crosshair, velocity marker and the three F270 gauges, and Space fires the
new-game Eagle's front 1MW pulse laser with the original's heat, energy, refire
and damage rules (`docs/notes/combat.md`; hitscan, no laser projectile). Since
2026-10-25 generated systems also carry NPC traffic (`docs/notes/npc.md`:
traders, shuttles, police and pirates from F688/F635), so ships are drawn,
labelled and lockable, take F953/F942 damage, and the target hull/shield
readout and F1536's model-157 shield flash are reachable. Since 2026-10-25 the
AI is the encoded `DATA_006258` state machine (transit/hold/attack/escort/pass/
flee/dead/escaped) with F585's turn limits and statistics-based speeds; pirates
fire through F931/F930 and F953's damage reaches the player through
`Flight::take_damage`, police/wanted and F674/F675/F680 jump persistence are in,
and the port follows DOS FFE in letting AI lasers damage the player where JJFFE
disables it. Full docking AI, missiles, police spawns/fines/HUD badge and
Thargoids remain (`docs/notes/npc.md`). Since
2026-10-25 the game also posts the original's sound cues and scene music
(`docs/notes/sound-cues.md`): F1906/F1907/F1908 dispatch and gates, F1902's
song rules and the live event sites, with the Sound Effects page applied at
boot and on change. The effect mixer/device is still a null sink, so cues are
recorded/no-op on a real run; music plays through the HMP/WinMM player, and
song 0 waits for the intro to be drawn. Since 2026-10-25 the fitted equipment is
real too (`docs/notes/equipment.md`): the 66-record table (DOS bytes), F393's
general bits, ten missile pylons, cabins, shield generators and F383's
availability/prices; the flight state carries the loadout and applies F953's
shields-then-hull damage to the player, and F991/F1120's icons and lamps read
it. NPC fire now calls `Flight::take_damage`, so the shields/hull lines move in
play; F938 recharge/auto-repair, the death/capsule screens and the station
Upgrades rows (which need the starport tech level from F869) remain. Since
2026-10-25 the console is
the original's: a ten-entry dated message log and clock, the shields/hull,
Set/Act/Alt and turret readouts, the F1116 status lamps and F1120's engines-off
page (F2), and the 82-record `DATA_005100` button table driving the flight page;
the port's own top bar, speed line and status block are retired
(`docs/notes/dashboard.md`). Since 2026-10-25 the scanner draws F275's contacts
(bodies and NPC ships, the 19-colour table, stalks and the orientation globe)
and the combat/navigation targeter tunnels (`docs/notes/scanner.md`). Still
open: auto-targeter arming/equipment, per-target range colours, altitude units,
and F1104/F1098's death/capsule screens.

## How to read this repo

- `CLAUDE.MD` — owner decisions, scope, phases/exit gates, and per-milestone narrative.
- `docs/PORT_LOG.md` — append-only record of decisions, discrepancies, trace hashes, newest first.
- `docs/FORMATS.md` - the implemented partial `data.bin` cache contract (v1..v25) and the model/command/vertex/normal/depth helper semantics.
- `docs/ARCHITECTURE.md` — proposed workspace, crate responsibilities, determinism rules.
- `docs/INVENTORY.md` — reference-source inventory: formats, numeric model, timing, gotchas.
- `docs/KNOWN_ISSUES.md` — FFED3D-vs-JJFFE differences and port gaps.
- `docs/DEVIATIONS.md` — register of intentional deviations (approved / implemented / rejected).
- `docs/CONVENTIONS.md` — code conventions for this workspace.
- `docs/REFERENCE_HARNESS.md` — how to add a differential test against the executed JJFFE reference.
- `docs/MODEL_EXECUTION_PLAN.md` — design record for the v0.11 depth/point/line work and its follow-ups.
- `docs/MODEL_COMMANDS.md` — full F1772 opcode map and current Rust coverage.
- `docs/notes/` - per-feature records: polygons, textures, child models,
  thrusters, animation, views, text/labels, curved surfaces, lines, fills,
  docking, station screens (`station.md`), market, hyperspace, navigation, the
  galaxy map, the front end/new game (`intro.md`), combat (`combat.md`), NPC
  traffic (`npc.md`), sound cues (`sound-cues.md`), equipment (`equipment.md`),
  scanner and targeter (`scanner.md`), and the **ship dashboard audit**
  (`dashboard.md`: the complete original console/HUD gap list, with the
  byte-validated DATA_005100 button and DATA_007684 hot-area tables).
- `dashboard/` - the whole-port burndown at milestone level (`index.html`, data
  in `burndown.js`; 14 milestones, 137 items, 68 done and 69 left on
  2026-10-25). **Update it in the same commit as any progress change**:
  statuses plus a new dated history point. Its last point is checked against the
  task lists.

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

Since 2026-10-24 the playable is a generated system rather than a prototype:
system flight with the original flight model, autopilot, docking at stations,
the station market, lift scenes, hyperspace over real star positions, and the
galaxy map drawn the original's way (dots verified pixel for pixel, map frame
8,249 of 8,270 overlay pixels).

What is *not* done is presentation and gameplay, in this order of readiness:

1. Curved surfaces (opcode 31): F1874's geometry is ported and verified, and the
   flat, line and curve rasterisers are now the original's; F1876's patch
   emission (quads) is the remaining piece, before a planet can appear in the
   flight scenario. **This is the current milestone step.**
2. The whole-frame pixel oracle (a framebuffer mode in the reference) and the
   runtime upper palette; sprites, particles beyond the dot path, and circles.
3. The gameplay slice itself: combat, missions, starport services, planet
   surfaces and landing, lighting, saves, audio and the remaining systems work
   (see `dashboard/`).
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

`verify-all.py` checks that every input exists first and then runs all 47 oracle
entries (about five minutes), printing one line each and a final pass/fail. It
needs the player's own files, NASM and the MSVC x86 tools; none of it is public
CI. Rebuild the reference after any change to `tools/reference/`, and note that
the child oracle's trace SHA-256 changes with every rebuild (see below).

**2026-10-25 re-run.** After the combat and NPC reference rebuilds, the full
suite gave **46 of 47 passed** in 660 s; the only failure is
`verify-world.py`'s `--worldgen` repeatability check, the known Windows ASLR
nondeterminism (the same case recorded on 2026-10-24), not a regression. The
recorded trace SHA-256 values in the table below were re-derived for every
oracle that lists one (instance, order, point-line, text-submodel, colour,
base-transform, station, station-queue, intro, collide, world-tick) and are
**unchanged**, so the column is current; re-record an entry if its script's
adaptor changes. No oracle exposed a behavioural regression.

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
| v0.48e surface plumbing | `trace_surface` queue words (events 60/61) | — | whole-table oracle unchanged (448, 0 mismatched) | `4431b861…` | F1823 (entry hash map) and F1833 (mode dispatch) read and documented; surfaces are planet-scale, so the synthetic z-2000 station context clips them all |
| v0.48p front end and new game | `ffe_rs::front_end`, `navigation::{Body, START_SYSTEMS, NewGameState, new_game}`, date helper | `front_end.rs` (8), `navigation.rs` (+1, ignored), `intro_reference.rs` (1, ignored) | `verify-intro.py` (76 cases, 10 tamper fields) | `a9489bbe5d0a2765938c1d14f8ba70505c0726bcd110d492eb01959616eafc3d` | F961/F962's 18 credits, model 177/176, bitmap 0x42, animation and the F829/F998/F994 start (Ross 154, Eagle model 23, 1000 CR, state 0x30, pad seat); default boot is the new game, `--start sol` the sandbox; JJFFE legal splash and V line are not boot data; open-air starport takeoff ported 2026-10-25 (F1013/F1031/F1032, EXP-010) |
| v0.48d surface helpers | `ffe_core::model_surface::{cross_sign, mix32, max_abs_msb}` | `model_surface.rs` (2) | `--surface-sign` (12 pairs), `--surface-helpers` (8 mixes, 9 points) pinned | — | F1875, F1824, F1825 ported; F1816/F1817/F1820 are arena allocators to replace, not reproduce |
| v0.48c surface resolution | `SurfaceEmission::{origin, axes}` (F1874), `trace_surface` probe | `model_execute.rs` (+1) | `verify-station-models.py`: 448 models, 40 surface resolutions, 0 mismatched | `2036461a…` | origin (F1470 view), word0/6/10 and the three axis vectors verified; F1876's patch rendering not ported (see `docs/notes/surfaces.md`) |
| v0.48b 3D text | `ffe-convert::glyph_programs`, `GlyphSet`, `ModelViewer::draw_texts`, cache v14 | `text.rs` (1), `glyphs.rs` (1, ignored), `embedded.rs` (+1), `cache.rs` (+1) | census: all three font sets import; models 15/30/66/453 draw text with nothing skipped | — | F1747: each character's program runs through `execute_live` in an F1740 frame, its return word sets the next pen; `0x3016`/`0x3022` expand the object string |
| v0.48a labels, full string table | `ModelViewer::model_strings`, `Shape::Label`, scanned `decode_model_names`, cache v13 | `model_viewer.rs` (+1), `embedded.rs` (+1), `text_census.rs` (1, ignored) | census: all 6 label codes resolve | — | F1812/F1583 labels drawn at the queued point in font 15; string table scanned to 219 entries (209 in v11/v12 left six codes unresolved) |
| v0.47 console clicks, plume flicker | `Screen::logical_point`, `AnimationInputs::jitter`, `Frame::jitter`, viewer `cycle_thrust` | `views.rs` (+1), `model_viewer.rs` (+1) | none (presentation) | — | view icons now take clicks at any window size; F1476 stream runs across frames so plumes flicker; viewer E key sets engine thrust |
| v0.49j right-hand console buttons | `console::RIGHT_BUTTONS`, `navigation::{View::Places, place_indices}`, `Screen::{draw_places, place_row, label_positions}`, `ConsoleState::{labels, navigation}`; main `l`/`n` | `console.rs` (+2), `navigation.rs` (+1), `console_view.rs` (3, ignored) | DOS `FIRSTENC.EXE` hot-area table 0xea68c and button records 0xbe912/0xbe950/0xbe98e byte-identical; no new oracle | — | labels F989/F1108 (player `+0xe9` bit 2); places F1099/F1020 (stations/starports, 18 rows, action 0xd1+n); `docs/notes/labels.md` |
| v0.48p Escape preference pages, space colour | `ffe_rs::prefs`, `Screen::draw_prefs`, `Screen::{sky, stars_visible}`, `IndexedImage::blit_rgb` | `prefs.rs` (8 + unit tests), `prefs.rs` test (8) | none (layout/strings from `ffedat.asm`; the original Escape frame is a manual capture) | — | pages 0-4 are `DATA_004963`..`DATA_004967` with `DATA_004903`..`DATA_004905` hot areas and primary icon bitmaps 44/45; space colour is `DATA_008815..817` (default Light Blue, six-bit `#0c0c4c`); port page 5 (Modern/Classic) and `Quit`; grey unimplemented rows; see `docs/notes/prefs.md`, EXP-012/013 |
| v0.57 crosshair, lasers, combat HUD | `ffe_core::laser`, `ffe_rs::combat::Combat` (on `Flight`), `Screen::{draw_gunsight, draw_velocity_marker, draw_gauges, draw_target_hud}`, `IndexedImage::blit_clipped`, main Space fire | `laser.rs` (8), `combat.rs` (4), `combat_view.rs` (6, ignored), local frames | `--state-combat` prints F998's loadout from the executed original (model 23, one `0x88` mount, two `0x82` missiles, 1 t energy); no new verify-all entry | — | bitmap 97 gunsight at (129,48) per mount (not views 4/6), bitmap 98 velocity marker (front only), F270 bars, F1100 range; F930/F931/F929/F730/F938 firing, F953/F942 damage; no NPC ships yet, so the target readout and model-157 flash are unreachable; `docs/notes/combat.md` |
| v0.58 original HUD, message log, console table | `ffe_rs::{messages, hud, console, console_table}`, `Screen::{draw_messages, draw_console, draw_gauges}`, `ShipSystems`, `PlayerHud` | `dashboard_view.rs` (3, ignored), `console_table.rs` (13 + unit tests), `console.rs` (F261/262/263/264/991/1116/1117/1120) | none (the layout/strings are from `ffe.asm`; `tools/gen-console-table.py` imports the 82 records byte-exactly) | — | ten-entry dated log and clock, shields/hull/Set/Act/Alt and turret text, lamps and the engines-off page (F2/record 0x0c), the table-driven console and the retired port top bar/status block; player damage/equipment state, scanner contacts, F991 0x11-0x14, altitude/F1393 units and the F2/F3 key mappings remain; `docs/notes/dashboard.md` |
| v0.59 open-air starport takeoff | `ffe_core::traffic::Takeoff`, `Pads::seat`, `Navigator::{begin_launch, takeoff_step, takeoff_seat}`, `OriginalFlight::{begin_pad_takeoff, step_pad, end_pad_takeoff}`, main F1013/F1031 wiring | `takeoff.rs` (4), `pads.rs` (+1), `navigation.rs` (+1 ignored, cached takeoff) | none (the state path and constants are from `ffe.asm`; the new-game seat is checked against F650's tables) | — | F994/F650 seat, F381/F654 clearance, F1013 `0x24`, F1031's `0x20` region test and `0x26` hand-off, F1032/F651 bay release, F573/F1677 return, F575/F576 in a persistent two-body pad world; the refusal and the 20 km teleport are gone. The only deviation left is the DATA_008867 climb fallback (EXP-010); `docs/notes/intro.md` |
| v0.60 NPC traffic and ship combat | `ffe_core::npc` (F688/F635/F1533/F953/F942/F1536/F922), `Navigator::{ships, spawn_traffic, tick_ships, lock_ship, damage_ship}`, `Screen::{draw_system_ships, SystemShip, TargetDamage}`, `--capture-traffic` | `npc.rs` (6), `npc_view.rs` (1, ignored), cached frames | none (F635's table and F688's counts from `ffe.asm`; the placement is the recorded deviation) | — | traders, shuttles, police and a pirate group spawn per system; ships are drawn, labelled, click/lock/T-targeted with F1100 range and F1099 hull/shields, take F953/F942 damage, show F1536's model-157 shield flash and a death marker; full AI, police response, ship fire, missiles, F942's explosion and persistence remain; `docs/notes/npc.md` |
| v0.61 sound cues and scene music | `ffe_rs::sound::{GameAudio, Cue}`, F1906/F1907/F1908 dispatch, F1902-F1904 music rules, prefs `SoundFlags`, live wiring in `main.rs` | `sound_cues.rs` (2 + unit tests), `prefs.rs` (+2) | none (cue indices/levels and gates from `ffe.asm`/`w32sound.cpp`; no audio-device capture) | — | every effect cue the port can reach is posted (prefs, station/console, lock, messages, lasers, shield/hull, explosions, siren, docking/launch doors, engine/station beds, camera, hyperspace) and scene music follows F1902/F919/F750/F656/F1094/F232; the Sound Effects toggles apply at boot and on change. **The effect mixer/device is still missing** (`NullSampleSink`), so cues are recorded/no-op; music is audible through the HMP/WinMM player, but song 0 is unreachable until the intro draws. ECM/missile/wind cues are API+test only; `docs/notes/sound-cues.md` |
| v0.62 equipment, loadout and player damage | `ffe_core::equipment` (66-record `DATA_005289`, F393/F383), `Flight::{loadout, ensure_damage, hit, destroyed}`, `Loadout`, console F991/F1120 icon state, `tools/gen-equipment-table.py` | `equipment.rs` (8 + unit tests), `combat.rs` (+7), `console_table.rs` (+), `dashboard_view.rs` (4, ignored) | none (the table bytes are checked against DOS `FIRSTENC.EXE`; no new oracle) | — | general bits, ten missile pylons and cabins in `Loadout`; F383 availability/prices (deterministic tech multiplier); F953's shields-then-hull order for the player over `laser::apply_damage`; F991/F1120 icons and lamps read the real state and the new-game's two `0x82` missiles draw their icons. Nothing fires at the player yet; `player+0xef`, F938 recharge/auto-repair, death/capsule and the station Upgrades rows (needs `DATA_008995` from F869) remain; `docs/notes/equipment.md` |
| v0.63 NPC flight/combat AI, fire and player damage | `ffe_core::npc` (`DATA_006258`, F585/F601/F597/F931/F930, F674/F675/F680/F927, F697), `Navigator::{tick_ships, player_attack, legal, provoke}`, `Flight::take_damage` (reconciled with `ensure_damage`/`hit`), NPC/police messages, `--capture-traffic` | `npc.rs` (16), `npc_ai.rs` (3, ignored), `combat.rs` (9), frames in `docs/evidence/npc-ai/` | none (state table and constants from `ffe.asm`; JJFFE's disabled AI laser damage is a recorded DOS-vs-JJFFE difference) | — | the AI states transit/hold/attack/escort/pass/flee/dead/escaped with turn limits and statistics speeds, pirate fire through F931/F930, F953 player damage through one `take_damage` path, police/wanted (`legal`, `player_attack`, provoke, F697 bounty), jump persistence/recycle. Full docking AI, missiles, police spawns/fines/HUD badge and Thargoids remain; EXP-015 records the reconstruction; `docs/notes/npc.md` |
| v0.64 intro playback and cache v27 | `ffe_rs::{front_end::IntroTimeline, intro_text::IntroText}`, `Screen::draw_intro`, `main::run_intro`, cache v27 (`base:text/intro-credits`, `base:text/char-widths`), `--capture-intro` | `front_end.rs` (9), `intro_view.rs` (2, ignored), `intro_text.rs` unit tests, `intro_reference.rs` (ignored) | `verify-intro.py` 76 cases unchanged (trace `a9489bbe…`); cache import verified by `verify-assets.py` | — | default boot plays F961/F962 (models 177/176, bitmap 0x42, the 18 DOS credits with layout codes, the `0x400` GameTics clock and the `0x7e7b76` reset), V's version gate and keys 1-5; scripted captures use `--skip-intro`; old v26 caches still load but the intro reports the missing assets. Model 176's stale-basis frames, the dynamic upper palette and an intro music backend remain; `docs/notes/intro.md` |
| v0.65 scanner contacts and targeter | `ffe_rs::scanner` (F275 filters, `DATA_008683` colours, stalk geometry, orientation), `ffe_rs::targeter` (F1113 tunnels, F1114 edges, F1612 circle, F1100 size), `ShipSystems::ca`, `Screen::draw_scanner`; main feeds bodies and ships | `scanner_view.rs` (2, ignored), `scanner.rs`/`targeter.rs` unit tests (8), cached frames | none (the geometry/colour table are from `ffe.asm`; no new oracle) | — | contacts for the system's bodies and NPC ships (starports filtered), 19-colour mass table, up/down stalk clipping and the 8-frame globe; the combat/navigation targeter tunnels and circle draw when the preference/flags allow. Auto-targeter arming/equipment, per-target range colours and two simultaneous targets remain; `docs/notes/scanner.md` |

Facts verified on 2026-10-22 (test counts and the smoke launch re-checked
for v0.47; the oracle figures are from the last full `verify-all.py` run):

- `cargo test --workspace --locked` passes, as do `cargo fmt --check` and
  `cargo clippy -D warnings`. 561 `#[test]` functions total; 131 are `#[ignore]`
  local-only tests; one more is `#[cfg(unix)]`. On Windows that is
  **429 runnable synthetic tests** (430 on Linux).
- Collision contacts: F575's soft landing, crash and bounce tail and F578's
  momentum response are ported (`ffe_core::landing`, `ffe_core::collision`) and
  **all 4,096 `--collide` cases and all 4,096 `--world-tick` cases match**,
  including 950 and 584 contacts (`verify-collide.py`,
  `verify-world-tick.py`; `docs/notes/collision.md`). The playable still pushes
  the ship clear of a station instead of running it.
- `ffe-rs.exe --root . --smoke-frames 30` presents frames with the local
  cache v14.
- `ffe-rs.exe --root . --capture-planet DIR` flies the original flight model and
  the real autopilot from three radii to the nearest planet and saves frames
  through the approach; it exposed the surface queue's 50,000-word limit
  truncating the planet at 3-2 radii (black wedge). The flight view now draws
  surfaces with 131,072 words (EXP-009); the oracles keep the original limit.
- `verify-all.py` (2026-10-24, including the new `verify-intro.py`): **46 of
  47 oracles pass**; the only failure is `verify-world.py`'s Windows
  `--worldgen` run-to-run comparison (ASLR; see Known stale spots).
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
  `data.bin` is cache v27 (1796 assets, 462 of 466 models, 219 model strings,
  3 glyph sets, the F650 pad tables and the intro credits/char widths); `.cache/data-v1..v5-backup.bin`,
  `data-v10-backup.bin`, `data-v12-backup.bin` and `data-v13-backup.bin` are
  retained.
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

**Progress.** Tracked in `dashboard/`: the burndown covers the whole project
from the first commit to a finished like-for-like port (133 items, 52 done,
81 left on 2026-10-24; v0.48 itself is 11 of 19). Keep it current with every
progress commit.

**Goal.** The F6 viewer and the flight view draw every primitive the executor
queues, with the original's own rasterisers, verified at the pixel level.
Labels, 3D text, lines, curves, flat faces and polygon spans now draw with the
original's rasterisers; curved surfaces (F1876) and the runtime upper palette
remain, then the whole-frame pixel oracle. Gameplay work beyond the current
system slice is tracked in `dashboard/`.

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
     the three axis vectors) and **verified against the original**: the probe
     records events 50-54 at the F1876 call and the whole-table oracle compares
     40 surface resolutions across 448 models, 0 mismatched. `docs/notes/
     surfaces.md` records the full F1874 → F1876 contract, F1876's size
     (~1,580 lines), its helper set and the two constant topology tables.
   - **Next:** port F1876 bottom-up (F1875's 64-bit cross-product sign first,
     then F1816/F1820/F1823/F1824/F1825, then F1833/F1834/F1835), with scoped
     probe hooks in F1874/F1876 and a `verify-surfaces.py` whole-table oracle
     before the quads are drawn.
3. **Lines and curves.** F1616 (DrawLine) for F1803 entries (opcode 2 lines,
   narrow thrusters) and F1811 for opcode 22 curves, replacing the stand-ins.
   - **Done (v0.48f):** F1616/F1601 ported (`ffe-core::model_line`,
     `docs/notes/lines.md`) and verified by `verify-lines.py` (4096 cases, 0
     mismatched). Opcode 2 lines now take F1704's clipped points
     (`model_thruster::line`), so every line entry draws with it.
   - **Done (v0.48g):** F1811's F1546 ported (`draw_curve`: F1549 steps,
     F1561 clip, F1616) and verified by `verify-lines.py` (4096 curves, 0
     mismatched); curve entries draw with it instead of the stand-in cubic.
5. **A planet in the flight scenario.** Once F1876 draws, put a planet-scale
   surface model (slots 113-148, 445, 447, 449) in the flight scene at a
   distance where its patch is visible, with the original's parameters, and
   capture the probe at that distance as its oracle.

4. **Pixel oracle.**
   - **Done (v0.48h):** the flat-face rasterisers F1604 (triangles) and
     F1607/F1608 (quads, F1602/F1603/F1601) are ported (`ffe-core::model_fill`,
     `docs/notes/fills.md`) and verified by `verify-fills.py` (4096 faces, 0
     mismatched); on-view flat faces draw with them.
   - **Done (v0.48i):** the polygon span path (F1468, F1560/F1559/F1558,
     F1613/F1600) verified by `verify-fills.py` (4096 polygons, 0
     mismatched), and F1714's outlines for faces reaching off the view
     verified by the texture-queue oracle (5,666 outlines, 0 mismatched).
     Polygons and every queued flat face now draw with the original
     rasterisers; faces crossing the near plane are no longer skipped.
   - **Left:** add a framebuffer mode to the reference adapter that draws one
     queued scene with the original's F1795/F1796/F1797 spans, F1647 textures,
     F1616 lines and F1811 curves, then compare whole frames byte for byte.

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
- A planet-scale surface model draws in flight at a plausible distance (see
  step 5), with the probe's captured quads as its oracle.
- Owner check (2 minutes): in M, page to models 66 and 453 and see their name
  labels; then to the station and an Eagle and see rounded parts (and, once
  the glyph step lands, their registration text); in flight, F9 shows the
  ship's plume flickering under W, and a planet is visible in the scenario.

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

- `verify-all.py` on 2026-10-24 (Windows, with the new `win-compat.h` shims): **46 of 47 oracles pass**
  (the new `verify-intro.py` runs on Windows), including
  `verify-traffic.py`, `verify-market.py`, `verify-lift.py`, `verify-sectors.py` and `verify-hyperspace.py`; `verify-world.py` fails only on `--worldgen`'s run-to-run comparison
  (Linux pins ASLR with `personality`, Windows cannot). The scenarios that fork a child (`--surface-draw`) need Linux.

- `verify-all.py` on 2026-10-23: **all 31 oracles pass** (the two broken since
  v0.48b were repaired: `verify-assets.py` now expects cache v14 with 1,781
  files, and the child-model oracle masks the surface events 50-54 and 60/61,
  which the whole-table oracle covers).
- `README.md`'s playtest/controls sections still describe the prototype slice
  (food trading, F2 scenarios, M for the viewer, Modern/Classic presets); the
  system slice, the market and the current keys are in `docs/NAVIGATION.md` and
  the executable's help.
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
