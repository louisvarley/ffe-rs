# Known issues

Policy: **the base game is ported as it is.** We do not apply new bug fixes by default. A known bug is logged here; if a fix is later requested, it ships as an **opt-in toggle** with a failing test first and a note in `docs/DEVIATIONS.md`. Enhanced graphics options are presentation-only and never fix gameplay.

The reference (JJFFE 2.8a6 + Aniso 0.9c + FFED3D) already contains many changes labelled `FIX:`/`ANISO`. Those are part of the reference behaviour and are listed separately so the owner can decide whether the port should match the reference or restore DOS 1.02. Until that decision is made, the default is the reference behaviour (because that is what golden traces will record).

---

## 1. Reference behaviour vs DOS 1.02 (decision pending)

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

**Action:** after approval, decide per row whether to match the reference (recommended for trace compatibility) or restore DOS behaviour behind an opt-in. Do not silently mix.

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

- No Rust workspace/port code yet (Phase 0).
- No headless trace harness yet (Phase 1).
- No `data.bin` converter yet (Phase 2).
- No golden traces yet; no Classic framebuffer hashes yet.
- No CD version detection/support (hash/file list not supplied).
- No mod system yet (Phase 8).
- No packaging/CI Rust jobs yet (Phase 9); the forbidden-file check exists.

---

## 5. How to add an issue

Use a short ID, the affected subsystem, the exact source reference (file:line or function), whether it is original/reference/port, reproduction steps, expected vs observed, and whether a fix is requested. If a fix is requested, add a failing test first and an opt-in toggle; never change the default silently.
