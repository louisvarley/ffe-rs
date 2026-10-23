# Known issues

Policy (owner decision, 2026-10-20): **JJFFE 2.8a7 is the gameplay baseline;
verified bugs may be fixed.** Characterize the defect, add a regression test,
and record the intentional deviation in [DEVIATIONS.md](DEVIATIONS.md).
Default fixes are permitted; not every fix requires an opt-in toggle.
Enhanced graphics options remain presentation-only and never fix gameplay.

The previously studied FFED3D tree contains many `FIX:`/`ANISO` changes.
These are **not** automatically part of the selected separate JJFFE baseline.
The table below is historical comparison evidence, not a list of approved
changes. Reconfirm any suspected defect against JJFFE before treating it as a
port task. See [CLAUDE.MD](../CLAUDE.MD).

---

## 1. Historical FFED3D behaviour vs DOS 1.02 (not the selected baseline)

These are explicit changes in `ffe.asm`/the C++ modules. They are **not** bugs in the port; they are differences from the original DOS release. `docs/INVENTORY.md` §9 has the full context.

| Area | Reference change | Source marker | Notes |
|---|---|---|---|
| Lasers | Laser invulnerability changes | `FIX: Laser invulnerability` (15387, 49257, 82125) | Affects damage windows, including docking. |
| Equipment price | “incorrect tech prices” | `FIX` (29251) | Shop price formula. |
| Collision | Collision radius | `FIX` (38375) | Affects collisions/landing. |
| Missiles | Missile retardation | `FIX` (38534) | Missile physics. |
| FOR | Linear FOR switch velocity | `FIX` (41000) | Frame-of-reference transitions. |
| Autopilot | Acceleration fudge | `FIX` (41172) | Autopilot behaviour. |
| Smoke | Mispositioned smoke | `FIX` (48827) | Presentation timing/position. |
| Combat | Combat velocity fudge | `FIX` (49299, 51998) | Velocity handling. |
| Pulse lasers | Pulse laser non-limitation | `FIX` (49683) | Fire rate/capacity. |
| Hyperspace AI | AI time travel during HS | `FIX` (50131) | AI timing. |
| Orbits | Orbital drift | `FIX` (62299) | Long-term orbital positions. |
| Binaries | Binary light / over/under adjustment | `FIX` (65628, 66497, 66516) | Star/planet lighting and orbits. |
| Starports | Starports below ground | `FIX` (66283) | Station placement. |
| Asteroids | Asteroids run out | `FIX` (69361) | Asteroid spawn/lifetime. |
| Hyperspace | Player hyperspace time-travel | `FIX` (69426) | Time accounting. |
| Calendar | Groundhog day | `FIX` (70432) | Date rollback. |
| Missiles | Ray-collision disabled | `ANISO BUGFIX` (72632) | Missile collisions. |
| Cargo | Cargo space not lost | `ANISO bugfix` (73056) | Cargo capacity on destruction. |
| Cabin | Cabin temp related to game time | `ANISO FIX` (77067) | Atmospheric heating. |
| Turrets | Turrets cannot turn when paused | `ANISO bugfix` (78838) | Pause behaviour. |
| Turning | “limit turning speed?” / AI turning | `ANISO` (40065, 40128, 40143) | Turning-rate model. |
| Pirates | No instant snap to player | `ANISO` (41141) | AI approach. |
| Missiles | Max thrust 120g instead of 60 | comment at `F601` | Missile performance. |
| Start | “Frontier 3500” extended universe | `FUNC_000829`, `DATA_ExtendedUniverse` | Alternate start; disables hand-coded missions/journal and uses C++ overrides. |

**Action:** preserve the selected JJFFE behavior except tested, documented
fixes. Do not silently import these FFED3D modifications.

---

## 2. Confirmed/suspected defects in the reference or original

These are logged as-is. They are not fixed by default.

1. **`FUNC_000930` damage path** — `ffe.asm:71792` comment: “calculate damage! BUG HERE”. The comment does not describe the defect; Phase 1/3 must characterise it and decide whether a trace-visible bug exists.
2. **`F381`** — `ffe.asm:82126` comment: “Unfixed - actual bug in F381”. Related to laser/docking invulnerability; needs characterisation.
3. **3D system map crash** — `ffe.asm:96427` comments out a test that “crashed in 3d system map”. A known crash path in the original UI; reproduce/record before fixing.
4. **Save/load edge cases** — the compressed/scrambled save is a defined subset; corrupt/truncated files may not fail cleanly in the reference. The port must fail cleanly (error, no panic) while preserving valid-file compatibility. This is a robustness fix, not a simulation change.
5. **Journal parsing** — the `MISSION.DAT` bit-permutation/offset table is only partially documented in the notes; the exact entry count and trailing bytes are not confirmed. Treat as a Phase 2 verification item, not a known game bug.
6. **Starport/collision edge cases** — the reference fixes “starports below ground” and collision radius; other placement/collision edge cases may remain. Phase 1 traces should include near-surface/landing cases.
7. **Time rollback/date** — the reference fixes “groundhog day”; other calendar/hyperspace time-accounting edge cases (e.g. time accel + save/load) should be traced.
8. **AI freeze/pause** — turrets cannot turn when paused (reference fix); other pause interactions (autopilot, missiles, station traffic) are untested.
9. **Input focus** — the reference uses DirectInput; losing focus/reacquiring devices can drop keys/buttons. The port must handle focus robustly; default bindings must still match.
10. **Audio device loss** — DirectSound buffers can be lost; the reference restores them. The port's audio backend must handle device changes without changing trigger timing.
11. **Large time steps** — the physics code caps accelerations and uses shift-based time scaling; extreme time acceleration can produce different paths. This is part of the reference; trace it rather than “fix” it.
12. **Window/aspect** — FFED3D's 16:10 `aspectfix`, 640×400 software buffer, TTF font and linear filtering are deviations from DOS, not bugs. Classic will not inherit them.

---

## 3. Phase 0 study-build limitations (not product bugs)

- The study binary uses VS 2026, NASM 3.02, a D3DX9 NuGet package, Wine `d3d9.h`, a DirectInput shim, `#pragma pack(1)`, and a splash bypass. It is not a faithful 2010 build.
- Scripted input is not working yet; the splash was bypassed for the smoke test. Full gameplay/saves/audio were not exercised.
- The reference build links the debug CRT in Release and disables optimisation; performance numbers from it are not representative.
- The source has no licence; the binary is not distributed.

---

## 4. Port-phase gaps (to be closed)

- An experimental native window now runs launch/fly/dock/trade/save/reload.
  It is **not faithful First Encounters**: synthetic wireframe station, stars,
  floating-point prototype flight, fixed one-item market and safety resets.
  V0.2 uses original cabin0, console artwork and main font in a 320x200 logical
  view with 4:3 correction. Functional original HUD/buttons/scanner, dynamic
  palette, full FF-code text and world/models are still absent. The green scanner
  contact is prototype math, not original scanner behavior. Other UI characters
  outside printable ASCII display as `?`; full diagnostics go to stderr.
  V1 caches can be inspected but must be rebuilt to v2 for the new font.
  V0.4 extends F4 with actual declared root triangle/quad outlines and single-face
  inspection (cache v4); v3 keeps its points-only lab. These catalogue all physical
  face records, including alternate branches, not a full program execution.
  V0.5 adds exact normal visibility/lighting and a diagnostic normal-filtered
  outline view (cache v5), not branch execution or shaded pixels.
  Original integer projection (F1479) and live vertex slots are verified (v0.10);
  v0.11 composes them with the depth commands/groups in
  `model_execute::execute_live`. No complete station command execution or
  submodels yet.
  V0.7 executes a verified bounded command subset on composed synthetic programs.
  V0.8 verifies the standalone original depth-key calculation; v0.9 adds separately
  verified depth-command resolution and flat group ordering. Actual station streams
  now advance past opcode 6 and stop at the next unsupported command (point/text/
  submodel/other transform). Nested child-model queues and real primitive storage
  remain absent.
  The lab still catalogues faces, not executor output. Arithmetic/bit-branch/material
  helpers are verified;
  integer trig and Q31 basis rotation are now verified standalone helpers (v0.6),
  with packed angle operands and pure local-basis rotation commands also verified.
  Base/object-relative and vertex-defined transforms remain unimplemented.
  Normal/LOD visibility predicates and branch outcomes are verified independently,
  composed in the bounded executor, not a complete station command interpreter.
  Test values are not live animation
  state. Gameplay still uses the synthetic cube. Unsupported vertex operations
  fail explicitly; the interpreter is not yet suitable for arbitrary game models.
  See EXP-001 through EXP-004 and the actual short tests in [README.md](../README.md).
  No audio, combat, planets, hyperspace, time acceleration, old saves, remapping,
  persisted settings or image packs. F5 creates a new numbered snapshot; no
  autosave or quit prompt. Failed writes can leave an invalid newest snapshot;
  F9 reports errors rather than silently loading an older one. Explicit cache
  creation is still required before launch. No faithful gameplay goldens yet.
- Rust inspection, partial content cache and first RNG/clock/counter primitives
  exist; no complete faithful simulation or renderer yet. Inspector validates exact journal/cabin hashes
  but only presence/basic shape for audio and counts for optional media.
- Local JJFFE startup smoke and selected new-game state traces exist, with
  differential RNG/timer vectors. The bounded 400-frame scenario validates 238
  game-clock transitions, not flight, docking, trading or save/load. Full state/
  pointer normalization, faithful pacing, input scheduling and clock wrap remain absent.
- Partial `data.bin` builder/reader imports 345 images, two raw texture blocks,
  main font (v2), two station vertex programs (v3), two root command streams (v4),
  two station normal blocks (v5), sine quarter-wave table (v6), low palette,
  cabins and PCM effects. Submodels, gameplay tables, additional fonts, journal,
  music/CD conversion, complete cache and automatic startup conversion remain absent.
- Repeatable bounded startup framebuffer hashes exist, not gameplay goldens.
- Floppy 1.02/CD 1.01 embedded table layouts match JJFFE bytes in both real inputs.
  Upper palette colours remain runtime state; no colour-guessed embedded image
  export or automatic upscale eligibility. The cabin-only preview export is
  unchanged. This is not full original-game asset/gameplay support.
  Patch-output fingerprints remain unverified.
  Direct BIN/CUE ingestion is deferred.
- No mod system yet (Phase 8).
- Windows/Linux Rust CI jobs configured for formatting, strict clippy, synthetic
  tests and release tool builds; not remotely executed during local development.
  No game packaging yet. Repository-hygiene checks exist.

---

## 5. How to add an issue

Use a short ID, affected subsystem, exact source reference, baseline/build,
reproduction steps, expected vs observed, and planned correction. Add a failing
test first and record any intentional behavior change; default fixes are allowed.

## F1476 state shared between vertex jitter and other code (decided 2026-10-23)

The original's vertex opcodes 7/8 draw from F1476 (`DATA_008818`/`DATA_008820`),
whose state other game code also reads (about 35 routines, among them the
physics, agent and system-generation code). Rendering therefore affects
non-rendering behaviour in the original.

**Owner decision (2026-10-23): isolate drawing.** The port keeps the jitter
state in the vertex cache, owned by the caller, so presentation never disturbs
the simulation. The simulation keeps its own stream with the original's
generator. Consequence: replays and saves behave the same whatever is on
screen, but outcomes that depend on how many numbers drawing consumed (combat
rolls, agent behaviour) will not match the original roll for roll. Recorded in
`docs/DEVIATIONS.md`.

