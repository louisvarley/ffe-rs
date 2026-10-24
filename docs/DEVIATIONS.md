# Deviations from the reference / original

This is the register of intentional differences between `ffe-rs` and the selected
reference: **JJFFE 2.8a7** (owner decision, 2026-10-20).
FFED3D/Aniso was the earlier proposal, not the approved baseline.
A behavior change without a row here is an unrecorded deviation.
See [CLAUDE.MD](../CLAUDE.MD) for authoritative scope.

Status values: **proposed** (awaiting owner approval), **approved**, **implemented**, **rejected**.

---

## 1. Simulation

The faithful base simulation must match executed-reference evidence as it is
ported; only numeric primitives currently have differential coverage. The separate
experimental prototype below is not a substitute for faithful simulation.

| ID | Deviation | Status | Rationale / scope |
|---|---|---|---|
| SIM-001 | Use FFED3D as simulation oracle. | rejected | Owner selected the separate JJFFE 2.8a7 tree; see SIM-009. |
| SIM-002 | Inherit FFED3D's `FIX:`/`ANISO` behavior wholesale. | rejected | Changes must instead be justified against JJFFE and tested. |
| SIM-003 | Exclude FFED3D-added content from base data. | approved | No additional ships/models/shaders/scripts supplied by the engine. |
| SIM-004 | Exclude “Frontier 3500” extended-universe start in v1. | approved | Not the selected baseline. |
| SIM-005 | Import legacy saves read-only; write atomic, versioned native saves. | approved | One-way compatibility is enough; original-save export not required. Preserve original files. |
| SIM-006 | Allow verified bug fixes by default. | approved | Owner superseded the no-default-fixes proposal. Add regression tests and a specific deviation row for each fix. |
| SIM-007 | Deterministic 20 ms/50 Hz harness timestep for traces; the real game still uses measured elapsed time. | proposed | Proves equivalence; does not change the game. |
| SIM-008 | Gameplay mods can add/override/patch content. | deferred | V1 image packs cannot alter gameplay data or RNG. |
| SIM-009 | Use supplied JJFFE 2.8a7 as gameplay reference. | approved | Keep unmodified reference traces and expected corrected behavior separately. |
| SIM-010 | Model drawing (vertex jitter, opcodes 7/8) uses its own random stream; the simulation keeps the original generator separately. | approved | Owner decision 2026-10-23. Original F1476 is shared with about 35 simulation routines, so drawing changes outcomes there; isolating keeps replays, saves and graphics packs independent of what is on screen. Combat and AI rolls will not match the original roll for roll. |

### Temporary experimental example (not an approved change to Classic)

| ID | Deviation | Status | Rationale / scope |
|---|---|---|---|
| EXP-001 | Isolated playable development loop with synthetic flight/economy/world and software presentation. | implemented | Responds to the request for a playable example. Lives in `ffe-rs`, not verified `ffe-core`. Explicit experimental labels in title/help/HUD. Fixed 20 ms floating-point integration, 250-unit/s cap, simplified clearance/range/speed/alignment docking, collision/boundary safety resets, fixed one-item market. V0.2 uses original cabin/font/console art; station, stars, diagnostic layout and scanner math remain placeholders. Not original gameplay or completed phase 3. |
| EXP-002 | Separate version-1 experimental JSON snapshots, no old-save import. | implemented | New-only numbered files, strict finite/range/state validation, bounded reads and exact float round trips. Previous snapshots never overwritten. Interrupted writes may leave an invalid newest file; loading reports the error, retains live state and never silently falls back. Not the final native save format or completed SIM-005. |
| EXP-003 | Classic-like and Modern keyboard presets, no remapping/settings persistence. | implemented | Owner requested Modern W/S + arrows as the startup default in v0.8. F3 retains Classic-like input. Original bindings not validated. F1 documents actual controls; pause/help/scenarios/focus loss stop simulation. Release and press controls afresh after modal/focus/preset changes. Independent of graphics and faithful input policy. |
| EXP-006 | Trading at a station in the playable uses the original's market formulas with its own random stream and a simplified hold. | implemented | 2026-10-24. Prices and stock (F619, F620), the first market (F622), the daily drift (F633's market half) and buying and selling (F555, F556) are the original's (`ffe_core::market`, `docs/notes/market.md`). Differences: each market is made from a stream seeded by the system and the station and drifted from its own stream, not the game's single interleaved stream (F633 also generates missions, which are not ported); the hold is the ship model's cargo statistic less its hyperdrive (equipment effects not modelled); only legal goods are listed (no black market); Shift trades ten tonnes per press; the prototype one-item trade remains for a playable without a station. The price and stock formulas match the executed original (8,192 cases). |
| EXP-008 | The galaxy map is the original's scene with this port's controls, calendar and jump rule. | implemented | 2026-10-24. The scene (`ffe_rs::galaxy_map`) is the original's: F840's camera, the nine sectors' stars through model 0xab, the grid, sphere and marker, the selection nearest the centre, and the console text (`docs/notes/galaxy-map.md`); its pixels match the executed original's map frame in position (8,249 of 8,270). The controls are the port's: arrows scroll, Page Up/Down or +/- zoom, drag turns, C centres, H or Enter jumps (the original's keys are not decoded). The date uses the plain calendar. A jump needs the selected star to be one of the 16 systems the game has bodies for; any other is refused as unexplored. The sphere is the drive's range for the player's ship (the port's Eagle has a class 10 drive; the original's new game starts with 10 ly). The "CORE" label's letters, and colours the original approximates in its runtime palette, differ. |
| EXP-009 | The flight view gives planet surfaces a larger order queue than the original's 50,000 words. | implemented | 2026-10-24. F1876's triangles queue through F1778, whose buffer holds 50,000 words; the original exits when one more is asked for. Approaching a planet (3 to 2 radii, where the autopilot flies) needs 59,190 to 63,125 words, and the port drew only the entries queued before `Fault::QueueFull`, leaving a large unpainted (black) wedge. `ModelViewer::draw_surfaces` now uses `Queue::with_limit(0x2_0000)`; the queued words and their pixels are the same. `Queue::default`, the emitters and every reference oracle keep the original limit and fault, so the whole-table and surface oracles are unaffected. The original's behaviour at that range is to exit, which a playable cannot do. |
| EXP-010 | A launch keeps the ship in the station until the outer door opens, and ignores the station's hull until it is clear. | implemented | 2026-10-24. F1088 copies the player's docked position (the station) to the ship when the door starts opening; F1089 puts it into flight when the door word reaches 0. The port now draws the launch's inside view from that seat (the station's own position, facing along the docking axis), animates the station's `+0x9e` door word (`docs/notes/docking.md`), and changes the mode to flight only on `LaunchDone`. The prototype's hull push would otherwise treat the launched ship as a collision at the station; it is skipped until the ship is past the slot. The lift scene, its models and the station's pixels are otherwise the original's. |
| EXP-007 | Hyperspace jumps use the original's range, fuel and time but skip the jump itself. | implemented | 2026-10-24. Range, fuel, jump time and the system distances (F903, F833, F841, F853, F856, F861; `ffe_core::{hyperspace, sector}`) match the executed original. The playable then moves the ship to the new system at once: no countdown, no time acceleration, no hyperspace scene (F908-F910), no misjumps, no arrival procedure, and no restriction on where the jump starts (the original needs 12,000 km from the planet). The 16 systems on offer are the 12 hand-coded ones and four test systems placed in real sectors near Sol. A new game starts with no fuel. |
| EXP-004 | Separate F4 original-station model lab, not a gameplay geometry replacement. | implemented | V0.3 interprets original model 74/75 vertices. V0.4 adds declared triangle/quad outlines and single-face inspection from all physical root command records, including alternate branches. Exact perimeters, not invented hull connectivity; no claim all faces execute or are visible together. Diagnostic orthographic camera/test variables, white/green outline colours, no original shading. Gameplay, snapshots and held controls remain isolated. Does not certify full control flow, projection, visibility, submodels or station-animation fidelity. |
| EXP-005 | Docking at a system's real stations by ffe-rs rules, not the original's traffic control and docking states. | implemented | 2026-10-24. Stations are drawn from their own models at their generated places, spinning at their F866 speed. D asks the nearest station within 100 km for clearance through the original's traffic control (F654, F650, F655, F657, F867; `ffe_core::traffic`; bays and busy replies included, the fines check wired to zero). The slot is the model's +z face (verified by eye for models 72, 73, 74, 80): a cleared ship crossing that face within a corridor of 15% of the face distance (at least 60 m) under 250 m/s touches the pad (F652) and starts the original's timed docking stages (about 26 s, ship held still, with the original's lift scenes); launching runs its stages (about 20 s); anything else inside the face distance is pushed clear with no damage. The autopilot to a station flies to a point 20 km out on the docking axis, takes clearance and flies in. Near a station or world the ship is carried with that body (a distance rule, not F604-F606). The pad hit (collision stream flags and driving F575's crash and bounce, which `ffe_core` now ports and verifies), the ship's motion with the rotating station and open-air starports remain to port; see `docs/NAVIGATION.md` and `docs/notes/docking.md`. |

These are temporary scaffolding, not acceptance of a changed faithful baseline.
V0.5 extends EXP-004 with normal-filtered declared outlines. Exact F1480/F1481
visibility/lighting is separately verified, but the observer derives from the
diagnostic floating-point camera and the light/shades are test inputs. Filtering
does not resolve alternate branches, submodels, original projection or shading.
V0.6's standalone rotation helper rejects invalid axis 3 instead of reproducing
the original routine's uninitialized pointer use. No valid-axis or gameplay
behavior is intentionally changed; complete transform-command execution is absent.
V0.7's bounded executor rejects loops exceeding the supplied 1..4096-command
budget, unsupported reached operations and local selectors that alias unmodeled
legacy instance fields. Errors roll back the complete invocation, not just the
last operation. This is validation/development scaffolding, not changed Classic
gameplay; neither the F4 catalogue nor flight uses incomplete execution output.
V0.9's abstract flat depth queue has a separate 1..4096-operation budget, not the
original 50,000-word arena. Budget failure preserves queue state; finalizing an
open group errors rather than inventing an implicit end marker. The standalone
depth resolver rejects unresolved/negative external vertices instead of reading
unmodeled cache memory. Original valid resolved-point math and ordering are
verified independently; v0.11 composes them in `execute_live`, which advances
past opcode 6 while still rejecting unsupported reached commands.
V0.2 replaces the earlier 640x480 development canvas with 320x200 logical
composition, nearest 4:3 correction and original font/console art. Glyph raster
and proportional advances match 696 executed JJFFE cases; panel/sprite clipping
matches 28 cases. Diagnostic layout, RGB composition across separate cabin/low
palettes, synthetic scanner contact, 60 Hz presentation and non-clickable panel
remain scaffolding. This is partial progress on CLS-001/002/003/004, not full
acceptance of those rows. Horizontal font clipping is bounds-safe rather than
reproducing the original routine's unsafe cross-row writes. UI characters outside
printable ASCII display as `?`; full error diagnostics also go to stderr.
No Enhanced/HD-pack option exists yet.

---

## 2. Presentation — Classic

The owner approved the original DOS visual target, including 4:3 correction.
The detailed table remains a verification checklist, not proof that every numeric
claim from the FFED3D study is already validated against JJFFE.
The following are intended corrections of FFED3D-only presentation back to DOS:

| ID | Topic | Status | Note |
|---|---|---|---|
| CLS-001 | 320×200 indexed framebuffer, 320×158 3D + 42-row HUD | proposed | FFED3D uses a 640×400 software buffer; DOS uses 320×200. |
| CLS-002 | Original 4:3 presentation with non-square pixels | approved | Owner selected original visuals; verify exact palette/projection behavior against JJFFE. |
| CLS-003 | Original bitmap font, not Arial/TTF | implemented (partial) | Main 116-glyph font imported, exact row masks/advances verified against JJFFE. Full FF-code text layout and other font data remain unported. |
| CLS-004 | Original HUD panel/icons, not a 3D panel render target | proposed | FFED3D uses `panel.X` + 1024×1024 texture. |
| CLS-005 | Affine texture mapping, nearest sampling, no mips/aniso | proposed | FFED3D uses linear/mip filtering. |
| CLS-006 | Original flat/quantised shading, not D3D Gouraud/specular | proposed | FFED3D uses D3D lighting. |
| CLS-007 | Original draw order/transparency, no arbitrary z-buffer effects | proposed | FFED3D uses D24S8. |
| CLS-008 | Original procedural planets, not GPU shader planets | proposed | FFED3D uses `planet.fx`/height maps. |
| CLS-009 | Original bitmap starfield, not `stars.X`/skybox TGAs | proposed | FFED3D adds HD assets. |
| CLS-010 | 50 Hz pacing, no interpolation/smoothing | proposed | Enhanced frame interpolation is off by default. |
| CLS-011 | Original mouse pointer, not the OS cursor | proposed | FFED3D draws a software pointer. |

**If a CLS row is rejected**, the resulting look is an explicit owner-approved deviation and must be recorded here with a reference image/test and a rationale. It may not be introduced by an “implementation detail”.

---

## 3. Presentation — Enhanced (opt-in, off by default)

All rows are **presentation-only**, off by default, switchable at runtime, and never feed the simulation. They are **not** deviations when off; they are documented here because they intentionally change the look when on. The master button covers all of these except where noted.

| ID | Feature | Status | Dependencies / notes |
|---|---|---|---|
| ENH-001 | HD textures (`upscaled.bin`, per-asset fallback) | proposed | Classic ignores the pack. |
| ENH-002 | Native-resolution rendering | proposed | — |
| ENH-003 | Smooth texture filtering | proposed | native-res |
| ENH-004 | Anisotropic filtering | proposed | smooth-filtering |
| ENH-005 | MSAA | proposed | native-res |
| ENH-006 | TAA | proposed | native-res |
| ENH-007 | Sharp scalable HUD/text | proposed | native-res; must match Classic metrics when off |
| ENH-008 | HDR + tone mapping | proposed | native-res |
| ENH-009 | HDR display output | proposed | HDR + capable surface/OS |
| ENH-010 | Bloom/lens effects | proposed | HDR |
| ENH-011 | Shadows (incl. ship self-shadowing) | proposed | native-res |
| ENH-012 | Ambient occlusion | proposed | native-res |
| ENH-013 | Atmospheric scattering + clouds | proposed | native-res, HDR |
| ENH-014 | Higher-quality starfield/nebulae | proposed | native-res; no gameplay RNG |
| ENH-015 | Engine glows/particles | proposed | native-res; spawn timing from core |
| ENH-016 | Material maps | proposed | native-res, smooth filtering, HD/map pack |
| ENH-017 | Widescreen/ultrawide **presentation** (letterbox/pillarbox, HUD anchoring, no frustum change) | proposed | Does not reveal more of the scene. |
| ENH-018 | Frame interpolation | proposed | Continuous transforms only; no event interpolation. |
| ENH-019 | Master button (all presentation-only on/off) | proposed | Never touches §4. |
| ENH-020 | Reset to Classic | proposed | Master off + selection cleared. |

Every implemented ENH row must have an “off equals Classic” test and an explicit
capability/error path. None may change the simulation. Live switching and
image-only packs are approved; the advanced effects listed here are deferred
research, not a requirement to implement them all in the first slice.

---

## 4. Gameplay-affecting (separate group, never master)

These are **not** Enhanced graphics. They are off by default, labelled “Gameplay-affecting — changes how the game plays”, and excluded from the master button. They are deviations from the original when on.

| ID | Option | Status | Why it affects play |
|---|---|---|---|
| GP-001 | Extended FOV | proposed | Reveals more of the scene; changes situational awareness. |
| GP-002 | Fill widescreen (extended horizontal FOV) | proposed | Reveals more horizontally than the original frustum. Presentation-only `widescreen` does not. |
| GP-003 | Input smoothing | proposed | Changes flight response. |
| GP-004 | Aim assist (future) | proposed | Changes combat. Not implemented in v1. |

If the owner considers any of these presentation-only, move it to §3 with an explicit note; until then it stays here.

---

## 5. Platform / plumbing (free-to-improve)

These do not change the simulation or the Classic look, but are listed for transparency.

| ID | Change | Status | Notes |
|---|---|---|---|
| PLT-001 | Modern windowing/input backend (winit/gilrs) with remapping and gamepad support | proposed | Default bindings and pointer mapping must match. |
| PLT-002 | Modern audio backend/mixing; HMP→MIDI + WAV effects | proposed | Trigger points/levels unchanged; synth choice is an open question. |
| PLT-003 | Config file (`ffe-rs.toml`) + CLI flags | proposed | Original `ffewin.cfg` bindings remain honoured/imported. |
| PLT-004 | Data conversion to `data.bin`; game/ not needed at runtime | proposed | Byte-identical content; deterministic. |
| PLT-005 | File dialogs use a native/modern dialog, not the original bitmap UI | proposed | The in-game options/file menu remains Classic-styled; OS dialogs are platform chrome. |
| PLT-006 | Atomic saves, clear errors, no panics on corrupt input | approved | Preserve imported legacy files; new native saves may use a different versioned format. |
| PLT-007 | 64-bit build (original is 32-bit) | proposed | Must preserve 32-bit integer widths/overflow semantics explicitly. |
| PLT-008 | Linux support (if easy) | proposed | Not required for v1; Windows first. |

---

## 6. Content / data

| ID | Deviation | Status | Note |
|---|---|---|---|
| DAT-001 | `data.bin` contains converted original content, generated from the player's files; never committed. | proposed | Required by the brief. |
| DAT-002 | FFED3D-added assets/scripts are excluded. | proposed | No licence; not base content. |
| DAT-003 | `upscaled.bin` is derived from original art; provenance recorded; LFS. | proposed | HD option only. |
| DAT-004 | Mod assets can add Classic/HD art and material maps; same options apply. | proposed | `validate-mod` rejects base-asset hash collisions. |
| DAT-005 | Hand-authored systems/planets/stations are inserted/overridden by ID/coords without perturbing other systems. | proposed | Test required. |

---

## 7. Process rules

- Every deviation is listed here **before** it becomes default.
- Every fix/change gets a failing test first.
- The master button never touches §4.
- Enhanced options never feed the simulation; the golden traces must be identical with every option on and off (the renderer may not call the gameplay RNG or mutate state).
- With every enhancement off, output matches the Classic reference.
- If a change is ambiguous, it is treated as frozen and the owner is asked.
