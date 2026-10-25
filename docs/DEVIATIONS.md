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
| EXP-003 | Classic-like and Modern keyboard presets, no remapping/settings persistence. | implemented | Owner requested Modern W/S + arrows as the startup default in v0.8. F3 retains Classic-like input. Original bindings not validated. F1 documents actual controls; pause/help/scenarios/focus loss stop simulation. Release and press controls afresh after modal/focus/preset changes. Independent of graphics and faithful input policy. 2026-10-24: the Escape pages expose the same preset on their port-only `Port Options` page; the choice is still in memory only. |
| EXP-006 | Trading at a station in the playable uses the original's market formulas with its own random stream and a simplified hold. | implemented | 2026-10-24. Prices and stock (F619, F620), the first market (F622), the daily drift (F633's market half) and buying and selling (F555, F556) are the original's (`ffe_core::market`, `docs/notes/market.md`). Differences: each market is made from a stream seeded by the system and the station and drifted from its own stream, not the game's single interleaved stream (F633 also generates missions, which are not ported); the hold is the ship model's cargo statistic less its hyperdrive (equipment effects not modelled); only legal goods are listed (no black market); Shift trades ten tonnes per press; the prototype one-item trade remains for a playable without a station. The price and stock formulas match the executed original (8,192 cases). |
| EXP-008 | The galaxy map is the original's scene with this port's controls, calendar and jump rule. | implemented | 2026-10-24. The scene (`ffe_rs::galaxy_map`) is the original's: F840's camera, the nine sectors' stars through model 0xab, the grid, sphere and marker, the selection nearest the centre, and the console text (`docs/notes/galaxy-map.md`); its pixels match the executed original's map frame in position (8,249 of 8,270). The controls are the port's: arrows scroll, Page Up/Down or +/- zoom, drag turns, C centres, H or Enter jumps (the original's keys are not decoded). The date uses the plain calendar. A jump needs the selected star to be one of the 16 systems the game has bodies for; any other is refused as unexplored. The sphere is the drive's range for the player's ship (the port's Eagle has a class 10 drive; the original's new game starts with 10 ly). The "CORE" label's letters, and colours the original approximates in its runtime palette, differ. |
| EXP-009 | The flight view gives planet surfaces a larger order queue than the original's 50,000 words. | implemented | 2026-10-24. F1876's triangles queue through F1778, whose buffer holds 50,000 words; the original exits when one more is asked for. Approaching a planet (3 to 2 radii, where the autopilot flies) needs 59,190 to 63,125 words, and the port drew only the entries queued before `Fault::QueueFull`, leaving a large unpainted (black) wedge. `ModelViewer::draw_surfaces` now uses `Queue::with_limit(0x2_0000)`; the queued words and their pixels are the same. `Queue::default`, the emitters and every reference oracle keep the original limit and fault, so the whole-table and surface oracles are unaffected. The original's behaviour at that range is to exit, which a playable cannot do. |
| EXP-010 | The default boot is the original's new game at Ross 154; the JJFFE front end is not ported, and the open-air takeoff's altitude fallback is the port's. | implemented | 2026-10-25. The default boot creates F998's start type 1 (`ffe_rs::navigation::new_game`): Ross 154 at absolute clock `0x121cf7.5` days, Eagle (model 23), 1000 CR, an 8 t hold, player state 0x30 on the starport's F650 pad. The pad seat is F994's: F650's offset plus the ship's collision radius (`+0x140`) along its up row. `--start sol` keeps the Sol/Mars High sandbox. The DOS release is the source of truth: the nine-line JJFFE legal splash is **not** shown or modelled, the "English Release 1.1 (Remastered Version)" line is JJFFE-only (kept for the intro oracle), and start types 2-5 are unverified (they map to 1 with a notice). The open-air takeoff is now the original's state path (`ffe_core::traffic::Takeoff`, `docs/notes/intro.md`): F654 provides clearance, `L` starts F1013's `0x24`, the set-speed/altitude test hands over to F1031's `0x26`, and F1031's contact scan clears the starport's `0x20` effect region, puts the mode at `0x00` and F1032 releases the bay (F651). The pad contact runs the ported F575/F576 complex-shape scan in a persistent two-body world, so no 20 km teleport is involved; the starport's shape leaves F1033's altitude at `0x7fffffff`, so F1013's test uses the climb measured from the pad seat along its up row as the fallback (the only deviation left in this row). The earlier `begin_launch` refusal of an open-air starport is gone. Orbital stations keep the verified lift launch. `docs/notes/docking.md` records the traffic side. |
| EXP-011 | A launch keeps the ship in the station until the outer door opens, and ignores the station's hull until it is clear. | implemented | 2026-10-24. F1088 copies the player's docked position (the station) to the ship when the door starts opening; F1089 puts it into flight when the door word reaches 0. The port now draws the launch's inside view from that seat (the station's own position, facing along the docking axis), animates the station's `+0x9e` door word (`docs/notes/docking.md`), and changes the mode to flight only on `LaunchDone`. The prototype's hull push would otherwise treat the launched ship as a collision at the station; it is skipped until the ship is past the slot. The lift scene, its models and the station's pixels are otherwise the original's. |
| EXP-007 | Hyperspace jumps use the original's range, fuel and time but skip the jump itself. | implemented | 2026-10-24. Range, fuel, jump time and the system distances (F903, F833, F841, F853, F856, F861; `ffe_core::{hyperspace, sector}`) match the executed original. The playable then moves the ship to the new system at once: no countdown, no time acceleration, no hyperspace scene (F908-F910), no misjumps, no arrival procedure, and no restriction on where the jump starts (the original needs 12,000 km from the planet). The 16 systems on offer are the 12 hand-coded ones and four test systems placed in real sectors near Sol. A new game starts with no fuel. |
| EXP-004 | Separate F4 original-station model lab, not a gameplay geometry replacement. | implemented | V0.3 interprets original model 74/75 vertices. V0.4 adds declared triangle/quad outlines and single-face inspection from all physical root command records, including alternate branches. Exact perimeters, not invented hull connectivity; no claim all faces execute or are visible together. Diagnostic orthographic camera/test variables, white/green outline colours, no original shading. Gameplay, snapshots and held controls remain isolated. Does not certify full control flow, projection, visibility, submodels or station-animation fidelity. |
| EXP-012 | Escape opens the original preference pages directly, adds a port-only page, and greys the options the port cannot apply. | implemented | 2026-10-24. The original's pages, labels, order, layout, hot areas, defaults and icon bitmaps are the ones in `DATA_004894`/`DATA_004907` and `FUNC_000216`/`FUNC_000225` (`docs/notes/prefs.md`). Differences: the first Escape opens the pages instead of only stopping time (the original needs a second Escape; `FUNC_000224`); `Quit to DOS` reads `Quit`; a `Port Options` page (page 5) sits in the original's empty row at y 68 and holds Modern/Classic controls; arrow/Enter/Backspace navigation and the `>` marker are additions. Applied: space colour, background stars, lock external view to ship, reverse left-right/up-down and Modern/Classic. The pages the port cannot apply (commander rename, Load/Save/Delete/New Game, sound, shape detail, targeting tunnels, filename extensions, video, map options) keep the original labels and defaults but are grey and inert, with a footnote. Strings and hot areas are hard-coded from the assembly because `DATA_004894` is not imported; the icons are the cached primary bitmaps 44/45. |
| EXP-013 | The space colour uses the original's captured quantised palette entry for the default, with the other four settings scaled from it. | implemented | 2026-10-25. The setting is the original's (`DATA_008815..817 = (0,0,index)`, default `Light Blue` 4 from `FUNC_000219`; `FUNC_001575`/`FUNC_001618`). The original requests those bytes and clears to the nearest entry of its runtime 256-colour palette; the captured reference frame's default is palette entry 182, six-bit (3,3,19) = `#0c0c4c`. The port has no runtime upper palette and cannot run the original's lookup, so it uses that captured entry for the default and derives the other four selectable settings from the same entry: each six-bit component is scaled by the requested index/4, rounded to the nearest six-bit value and expanded with `v * 255 / 63` (`ffe_rs::prefs::space_colour_rgb`). That gives `Black #000000`, `Very Dark Blue #040414`, `Dark Blue #080828`, `Medium Blue #080838`, `Light Blue #0c0c4c`; index 0 is exactly black and the original's stars stay drawn over it. Only the five settings and the default come from the source; the intermediate ramp is a documented interpolation, not a captured value. |
| EXP-005 | Docking at a system's real stations by ffe-rs rules, not the original's traffic control and docking states. | implemented | 2026-10-24. Stations are drawn from their own models at their generated places, spinning at their F866 speed. D asks the nearest station within 100 km for clearance through the original's traffic control (F654, F650, F655, F657, F867; `ffe_core::traffic`; bays and busy replies included, the fines check wired to zero). The slot is the model's +z face (verified by eye for models 72, 73, 74, 80): a cleared ship crossing that face within a corridor of 15% of the face distance (at least 60 m) under 250 m/s touches the pad (F652) and starts the original's timed docking stages (about 26 s, ship held still, with the original's lift scenes); launching runs its stages (about 20 s); anything else inside the face distance is pushed clear with no damage. The autopilot to a station flies to a point 20 km out on the docking axis, takes clearance and flies in. Near a station or world the ship is carried with that body (a distance rule, not F604-F606). The pad hit (collision stream flags and driving F575's crash and bounce, which `ffe_core` now ports and verifies), the ship's motion with the rotating station and open-air starports remain to port; see `docs/NAVIGATION.md` and `docs/notes/docking.md`. |
| EXP-014 | Docked services and stockmarket use the original's screens, with port keys and empty unimported lists. | implemented | 2026-10-24. The services root, shipyard root and market reproduce `DATA_005304`'s geometry, the `0x98xx` strings, the `0x2e` buttons, the arrows/Go Back and F558's cargo/cash/cabins line (`docs/notes/station.md`); the market keeps F619/F620/F622/F633/F555/F556. Added port keys: Enter activates the keyboard row, Backspace goes back (the original's way out is the console screen button), B/N buy/sell the selected market line, L launches. The stockmarket has no Go Back arrow in the original, so Backspace is the only way back. Equipment, ship and repair lists open the original screen shell with no rows (their tables/generators are not imported) and the original's console hint; the bulletin board is empty with 0x98de because F622/F633's BBS generation is not ported. The credit glyph is the font's `$` (the original's currency mark). Thargoid services (no Shipyard/Bulletin/Market) do not occur because the port has no Thargoid systems. |
| EXP-015 | NPC traffic uses the port's station/player-based placement and a three-state flight AI, not F702's system-wide orbit and the original per-state routines. | implemented | 2026-10-25. F688's spawn counts and F635's nineteen model records (with the allegiance shift), F922's bounded RNG, F1533's statistics, F953/F942 damage and F1536's shield flash are the original's. Differences: traders/shuttles/police orbit the first station at a clamped radius, the pirate group appears a fixed 3.5e6 units ahead of the player, and ships use Hold/Cruise/Attack rather than F752/F725/F727/F740/F790/F694/F695; names are the model name plus a serial, not F703/F791. The original F702 orbit is not used because it would place traffic millions of kilometres away without the full AI transit. Ships do not fire, dock, use missiles, react as police or persist; F691's station-bay ships are absent because the ship-data table is not loaded; F942's model-155 explosion is the model-157 death fallback. See `docs/notes/npc.md`. |

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
