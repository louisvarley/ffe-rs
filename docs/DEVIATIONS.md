# Deviations from the reference / original

> **Current owner requirement — 26 October 2026:** original source gameplay;
> Port Options are the only general permitted extension. Historical SIM/EXP
> rows below describe prior decisions and implemented scaffolding. They are
> not waivers for altered gameplay under this requirement. In particular the
> 7 t start, prototype docking, separate gameplay RNG streams, simplified AI,
> incomplete persistence and transition substitutes require removal or source
> parity, not acceptance as finished. Any earlier explicitly requested input
> guard must remain disclosed and be reconciled in PAR-06; no new exception is
> inferred. See [the audit](audits/gameplay-parity-2026-10-26.md).


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
| EXP-002 | Separate version-1 experimental JSON snapshots, no old-save import. | implemented (superseded by EXP-018 for new writes) | New-only numbered files, strict finite/range/state validation, bounded reads and exact float round trips. Previous snapshots never overwritten. Interrupted writes may leave an invalid newest file; loading reports the error, retains live state and never silently falls back. Version-1 documents are still read and migrate in memory; EXP-018 is the final native format for new saves. |
| EXP-003 | Modern and Classic flight presets, no remapping. | implemented | Owner requested Modern W/S + arrows as the startup default in v0.8. F3 retains the original-key Classic preset: hold the right mouse button to steer, Enter/Shift raise/lower the set speed, A/Z pitch, comma/period yaw, [ ] roll; the port's shared fire/target/view/map bindings are unchanged. The presets do not share steering or thrust keys. `docs/CONTROLS.md` documents the actual controls; pause, the preference pages and focus loss stop simulation. Release and press controls afresh after modal/focus/preset changes. Independent of graphics. 2026-10-24: the Escape pages expose the same preset on their port-only `Port Options` page. 2026-10-25: F1038 mouse steering wired for Classic, and the choice persists in `saves/config.cfg`; the other preference values remain session-only. 2026-10-25: the port-only `?` help page, scenario menu, F11 screenshot and F12 model viewer/lab were removed from the player build; F1 is the source view cycle again, F12 is the source escape capsule, and the inspection tools are behind `--dev`. |
| EXP-006 | Trading at a station in the playable uses the original's market formulas with its own random stream and a simplified hold. | implemented | 2026-10-24. Prices and stock (F619, F620), the first market (F622), the daily drift (F633's market half) and buying and selling (F555, F556) are the original's (`ffe_core::market`, `docs/notes/market.md`). Differences: each market is made from a stream seeded by the system and the station and drifted from its own stream, not the game's single interleaved stream (F633 also generates missions, which are not ported); the hold is the ship model's cargo statistic less its hyperdrive (equipment effects not modelled); only legal goods are listed (no black market); Shift trades ten tonnes per press; the prototype one-item trade remains for a playable without a station. The price and stock formulas match the executed original (8,192 cases). |
| EXP-008 | The galaxy map is the original's scene with this port's controls, calendar and jump rule. | implemented | 2026-10-24. The scene (`ffe_rs::galaxy_map`) is the original's: F840's camera, the nine sectors' stars through model 0xab, the grid, sphere and marker, the selection nearest the centre, and the console text (`docs/notes/galaxy-map.md`); its pixels match the executed original's map frame in position (8,249 of 8,270). 2026-10-25: the map's original console page (F834/F840) is now drawn and keyed: records 0x1c..0x24 are overlaid on the flight console (its screen buttons, meter, scanner and lamps remain visible, as the executed original's F3 frame shows). While that page is showing, F5-F12 are map-local exactly as F840: F5 re-shows the controls page, F6 selects, F7 details, F8 picture, F9/F10 details with the next/previous image, F11 zooms in and F12 zooms out. The details and picture *screens* (F842/F844-F849) are not ported; their keys only light the original button and report that, and inside those unported screens F840's F11/F12 would toggle stalks/grid instead of zoom. Outside the map the port's global keys are unchanged: F5/Shift+F5 save/load, F6-F10 views, F11 screenshot and F12/Shift+F12 model viewer/lab; the port's Tab system map and `n` places list also keep them. The port-only map keys remain: arrows scroll, Page Up/Down or +/- zoom, drag turns, C centres, G or the F3 icon opens the map, H or Enter jumps. The date uses the plain calendar. A jump needs the selected star to be one of the systems on offer with bodies (the 12 hand-coded and four test systems plus, for a new game, the start system's galaxy-table neighbours within 30 LY; 2026-10-26); any other is refused as unexplored. The sphere is the drive's range for the player's ship (the port's Eagle has a class 10 drive; the original's new game starts with 10 ly). The "CORE" label's letters, and colours the original approximates in its runtime palette, differ. |
| EXP-009 | The flight view gives planet surfaces a larger order queue than the original's 50,000 words. | implemented | 2026-10-24. F1876's triangles queue through F1778, whose buffer holds 50,000 words; the original exits when one more is asked for. Approaching a planet (3 to 2 radii, where the autopilot flies) needs 59,190 to 63,125 words, and the port drew only the entries queued before `Fault::QueueFull`, leaving a large unpainted (black) wedge. `ModelViewer::draw_surfaces` now uses `Queue::with_limit(0x2_0000)`; the queued words and their pixels are the same. `Queue::default`, the emitters and every reference oracle keep the original limit and fault, so the whole-table and surface oracles are unaffected. The original's behaviour at that range is to exit, which a playable cannot do. |
| EXP-010 | The default boot is the original's new game at Gateway/Hope; the JJFFE front end is not ported, and the open-air takeoff still measures the climb from the pad. | implemented | 2026-10-25 (revised 2026-10-25). The default boot creates F998's start record 0 (`ffe_rs::navigation::new_game`): Gateway at absolute clock `0x121cf7.5` days, a Saker Mk III (model 31), 10,000 CR (unchanged) and the original's 7 t hold holding 1 t of hydrogen (EXP-019), player state 0x30 on Old Blackelk's F650 pad. `--start 1` is Ross 154's Eagle on Sirocco Station; `--start sol` keeps the Sol/Mars High sandbox. The pad seat is F994's: F650's offset plus the ship's collision radius (`+0x140`) along its up row. The DOS release is the source of truth: the nine-line JJFFE legal splash is **not** shown or modelled, the "English Release 1.1 (Remastered Version)" line is JJFFE-only (kept for the intro oracle), and start types 2-5 are unverified (they map to 1 with a notice). The open-air takeoff is the original's state path (`ffe_core::traffic::Takeoff`, `docs/notes/intro.md`): F654 provides clearance, the launch action (F1056's action 9, plain Tab; the port also accepts `L`, documented) starts F1013's `0x24`, the set-speed/altitude test hands over to F1031's `0x26`, and F1031's contact scan clears the starport's `0x20` effect region, puts the mode at `0x00` and F1032 releases the bay (F651). The pad contact runs the ported F575/F576 complex-shape scan in a persistent three-body world whose ship parent is the starport's planet (F994's `+0x56`), as the original's; that gives F603 its gravity (the planet's `+0xa4` from F880's `Placement`) and the reference's fall-back after the hand-off. A 120-frame grace after F1031 stops the port's separate approach/landing scan from re-settling the ship on the pad before it has left. The port's terrain contact reports the terrain's elevation rather than the height above it, so F1013's test still uses the climb measured from the pad seat along its up row (the only deviation left in this row). Both start records are tested end to end (`default_start_takes_off_from_hope`, `new_game_takes_off_from_the_pad_into_free_flight`). The earlier `begin_launch` refusal of an open-air starport is gone. Orbital stations keep the verified lift launch. `docs/notes/docking.md` records the traffic side. |
| EXP-011 | A launch keeps the ship in the station until the outer door opens, and ignores the station's hull until it is clear. | implemented | 2026-10-24. F1088 copies the player's docked position (the station) to the ship when the door starts opening; F1089 puts it into flight when the door word reaches 0. The port now draws the launch's inside view from that seat (the station's own position, facing along the docking axis), animates the station's `+0x9e` door word (`docs/notes/docking.md`), and changes the mode to flight only on `LaunchDone`. The prototype's hull push would otherwise treat the launched ship as a collision at the station; it is skipped until the ship is past the slot. The lift scene, its models and the station's pixels are otherwise the original's. |
| EXP-007 | Hyperspace jumps use the original's range, fuel and time but skip the jump itself. | implemented | 2026-10-24. Range, fuel, jump time and the system distances (F903, F833, F841, F853, F856, F861; `ffe_core::{hyperspace, sector}`) match the executed original. The playable then moves the ship to the new system at once: no countdown, no time acceleration, no hyperspace scene (F908-F910), no misjumps, no arrival procedure, and no restriction on where the jump starts (the original needs 12,000 km from the planet). The systems on offer are the 12 hand-coded ones, four test systems placed in real sectors near Sol, and (2026-10-26) the start system's real galaxy-table neighbours within 30 LY (`navigation::galaxy_around`): F852/F860 select each neighbouring star and its F841 type word, F832 derives seed/mass/stars from the F856 position and type word, and F830/F869 supply the name and population, so `new_game` can enter e.g. Titican (code `0x12a53716`, 4.42 LY from Gateway) and its generated station Argent. The development sandbox (`Navigator::new`, `--start sol`) keeps the 16-entry list. A new game starts with no fuel. |
| EXP-004 | Separate F4 original-station model lab, not a gameplay geometry replacement. | implemented | V0.3 interprets original model 74/75 vertices. V0.4 adds declared triangle/quad outlines and single-face inspection from all physical root command records, including alternate branches. Exact perimeters, not invented hull connectivity; no claim all faces execute or are visible together. Diagnostic orthographic camera/test variables, white/green outline colours, no original shading. Gameplay, snapshots and held controls remain isolated. Does not certify full control flow, projection, visibility, submodels or station-animation fidelity. |
| EXP-012 | Escape uses the original's pause-then-pages order, adds a port-only page, and greys the options the port cannot apply. | implemented | 2026-10-24 (revised 2026-10-25, 2026-10-26). The original's pages, labels, order, layout, hot areas, defaults and icon bitmaps are the ones in `DATA_004894`/`DATA_004907` and `FUNC_000216`/`FUNC_000225` (`docs/notes/prefs.md`). The `FUNC_000224` order is now the port's: the first Escape stops time/pauses, the second opens the pages, and Escape in the pages returns to the game with time still stopped (P or the meter resumes). Differences: `Quit to DOS` reads `Quit`; a `Port Options` page (page 5) sits in the original's empty row at y 68 and holds Modern/Classic controls; arrow/Enter/Backspace navigation and the `>` marker are additions. Applied: space colour, background stars, lock external view to ship, reverse left-right/up-down, Modern/Classic, and (2026-10-26) the `Space dust, clouds etc` row (`DATA_009018`, which gates F1035's spawn). The pages the port cannot apply (commander rename, Load/Save/Delete/New Game, sound, shape detail, elite control, targeting tunnels, filename extensions, video, map options) keep the original labels and defaults but are grey and inert, with a footnote. Strings and hot areas are hard-coded from the assembly because `DATA_004894` is not imported; the icons are the cached primary bitmaps 44/45. |
| EXP-013 | The space-colour preference uses the original's captured quantised palette entry for the default, with the other four settings scaled from it. | implemented | 2026-10-25. The setting is the original's (`DATA_008815..817 = (0,0,index)`, default `Light Blue` 4 from `FUNC_000219`; `FUNC_001575`/`FUNC_001618`). The original requests those bytes and clears to the nearest entry of its runtime 256-colour palette; the captured reference frame's default is palette entry 182, six-bit (3,3,19) = `#0c0c4c`. The port now implements that runtime upper palette (`F1326`/`F1327`/`F1330`/`F1331`, `ffe_rs::palette`, `docs/notes/palette.md`) and uses it for models, FX and the intro's F1575 background, but **by owner decision** the Graphic Detail preference still uses the captured entry for the default and derives the other four settings from it: each six-bit component is scaled by the requested index/4, rounded to the nearest six-bit value and expanded with `v * 255 / 63` (`ffe_rs::prefs::space_colour_rgb`). That gives `Black #000000`, `Very Dark Blue #040414`, `Dark Blue #080828`, `Medium Blue #080838`, `Light Blue #0c0c4c`; index 0 is exactly black and the original's stars stay drawn over it. Only the five settings and the default come from the source; the intermediate ramp is a documented interpolation, not a captured value. The intro's own `(0,0,4)` request is not this preference: it resolves through the built palette and is `#040444` at the switch, matching the executed original. |
| EXP-005 | Docking at a system's real stations by ffe-rs rules, not the original's traffic control and docking states. | implemented | 2026-10-24. Stations are drawn from their own models at their generated places, spinning at their F866 speed. D asks the nearest station within 100 km for clearance through the original's traffic control (F654, F650, F655, F657, F867; `ffe_core::traffic`; bays and busy replies included, the fines check wired to zero). The slot is the model's +z face (verified by eye for models 72, 73, 74, 80): a cleared ship crossing that face within a corridor of 15% of the face distance (at least 60 m) under 250 m/s touches the pad (F652) and starts the original's timed docking stages (about 26 s, ship held still, with the original's lift scenes); launching runs its stages (about 20 s); anything else inside the face distance is pushed clear with no damage. The autopilot to a station flies to a point 20 km out on the docking axis, takes clearance and flies in. Near a station or world the ship is carried with that body (a distance rule, not F604-F606). The pad hit (collision stream flags and driving F575's crash and bounce, which `ffe_core` now ports and verifies), the ship's motion with the rotating station and open-air starports remain to port; see `docs/NAVIGATION.md` and `docs/notes/docking.md`. |
| EXP-014 | Docked services and stockmarket use the original's screens, with port keys and empty unimported lists. | implemented | 2026-10-24 (revised 2026-10-25). The services root, shipyard root and market reproduce `DATA_005304`'s geometry, the `0x98xx` strings, the `0x2e` buttons, the arrows/Go Back and F558's cargo/cash/cabins line (`docs/notes/station.md`); the market keeps F619/F620/F622/F633/F555/F556. Added port keys: Enter activates the keyboard row, Backspace goes back (the original's way out is the console screen button), B/N buy/sell the selected market line, L launches. The stockmarket has no Go Back arrow in the original, so Backspace is the only way back. Equipment, ship and repair lists open the original screen shell with no rows (their tables/generators are not imported) and the original's console hint; the bulletin board is empty with 0x98de because F622/F633's BBS generation is not ported. The credit glyph is the font's `$` (the original's currency mark). Thargoid services (no Shipyard/Bulletin/Market) do not occur because the port has no Thargoid systems. **The new game's state `0x30` no longer boots into this board:** F1050/F1054 draw the landed surface view (ground, horizon, starport, flight HUD) and `L` runs F1013's climb. The board is the original's state `0x2a`, opened by a completed docking (`Traffic::Docked`), and it is also reachable from state `0x30` at an open-air starport: plain F4 (the console's phone icon, `DATA_007688` `0xf4` -> module 12 event 1) runs `F343`, which shows the same screen for `0x2a` and `0x30`; F1 (the console's screen button) or the port's Backspace returns to the landed view. The new game still boots into the landed view, not the board (F1050/F1054, `L`/Tab runs F1013's climb). The open-air starport's own pad landing is still the port's in-flight contact, so re-docking at it does not yet enter `0x2a` (owner decision). |
| EXP-015 | NPC traffic uses the port's station/player-based placement and a reduced flight AI, not F702's system-wide orbit and every per-state routine. | implemented | 2026-10-25 (deepened 2026-10-25). F688's spawn counts and F635's nineteen model records (with the allegiance shift), F922's bounded RNG, F1533's statistics, F953/F942 damage, F1536's shield flash, F931's AI laser gates and F930's costs, F585's turn limit, F601/F597's thrust and stopping-distance rules, F695's escort formation, F674/F675/F680's jump persistence and F927's recycle are the original's. Differences: traders and shuttles now request a bay and run the station cycle (F689/F745/F746/F747/F733/F734/F735/F651/F736/F761/F762) around the first station, while police patrol and the pirate group appears a fixed 3.5e6 units ahead of the player (the original F702 orbit is system-wide and needs the full AI transit); the port's state machine names the original rows but does not transcribe every routine (F691's station-bay ships, the missile rows, F770's exact trading motion and Thargoids are absent); the station traffic's deviations are the 2-3 day interval counted in game seconds, a 30 s give-up at a full station, next-station-in-order instead of F762's random draw, and STATION_CRUISE_SPEED's floor for heavy freighters; turn rates are the port's base with F585's model-size halves, and top speed/acceleration are scaled from the statistics' thrust/mass ratio because the original's thrust units do not carry over; the police response to a wanted player is reconstructed from `DATA_008646/8647` (the original's police are F688's traffic and F783 clears the alert); a destroyed player is recovered at the nearest starport instead of the original game-over sequence; names are the model name plus a serial, not F703/F791. F942's model-155 explosion is still the model-157 death fallback and AI sound cues are not played. See `docs/notes/npc.md`. |
| EXP-016 | The flight view draws the inner surface base on approach and the outer collision radius once landed; the shell's far side and atmosphere are hidden for an eye inside it. | implemented | 2026-10-25. The surface axes are the model's outer radius word (`+0x138`); F1533, F581's terrain query and F879's starports use that radius. On approach `Screen::draw_discs` keeps the inner base sphere (`+0x140`) so the eye stays outside the shell, and `navigation::surface_radius(body, true)` switches to the outer radius once the ship is settled, so a rough landing no longer settles at 0.9 outer over a 0.64-outer base. F1876 has no depth buffer, so `terrain_draw` drops a final triangle whose outward normal points away from an eye inside the base and skips the low-level horizon cull there; F1877's haze ring and sky are left out inside the base (the original landed frame clears to the space colour and shows only the near ground). Outside the base the haze is unchanged. Regression: `surface_and_landing_use_one_radius` and the landed ground-pixel check in `tests/planet_view.rs`; `verify-surface.py` still accepts the landed frame. | Superseded 2026-10-26 for planet rendering: source model scale offset now sets the geometry conversion; the extra inside-sphere cull, coarse-horizon bypass and haze suppression were removed after near-pad executed-original comparison (docs/notes/launch-ground.md).
| EXP-017 | Presentation-only display options: window scale, pixel-perfect or fill-window fit, nearest or linear filtering, 4:3 or square pixels, and Alt+Enter full-screen. | implemented | 2026-10-25, owner request "resolution options". The original has no resolution menu and its DOS 320x200/4:3 output is fixed; this is a port extension, marked yellow on a port-only `Display Options` page (reached from `Port Options`) and in this row. The authoritative framebuffer stays 320x200 (`screen::Screen::logical_pixels`); `display` only scales the finished frame. Default: 4x, pixel-perfect, nearest, 4:3 - a 1280x960 window reproducing the previous initial size and the original's non-square pixels. Window scale 1x-6x sets the initial client size (still resizable); pixel-perfect uses the largest integer multiple and centres it with black bars; fill-window fits the whole frame by aspect; linear filtering only differs in a fractional fit; square pixels are offered as a non-original alternative. Alt+Enter toggles a borderless monitor-sized window (Windows `GetSystemMetrics`; other platforms keep the configured size). Choices persist in `saves/config.cfg` (with the rest of the applied pages); full-screen is session-only. Gameplay, simulation, saves, `--capture*`, `--smoke-frames` and every capture byte are unchanged; `--capture-display` writes new scaled presentation evidence only. Mouse mapping now follows the chosen fit rectangle. `minifb` 0.28 cannot resize live, so a change reopens the window. |
| EXP-018 | Native version-2 JSON commander saves (quick, numbered and front-end load), with the original's commander files read-only. | implemented | 2026-10-25. The port stores the full `Flight`, commander name/rank/mission slots, system code, clock, body indices, docking/landing stages and legal record; the system is regenerated from the cache and every index checked. It does not store the source's 0x984a object array, traffic, missiles or its two RNG words: the port has no single interleaved RNG (EXP-006/EXP-015), so loading respawns traffic from the saved system and clock. F5 quick-saves, Ctrl+F5 writes a new numbered file, Shift+F5 loads, and `L` on the intro or `--load` loads from the front end. Saves after death/the tombstone are refused. Writes are atomic (temp+rename, or hard-link for new-only numbered files), reads are 1 MiB-bounded and a corrupt/newer file never replaces live state or falls back silently. Version-1 `ffe-rs-experimental` snapshots migrate in memory. A DOS/JJFFE commander file is recognised (size/version) and left byte-identical; importing it still needs a supplied reference specimen and field map. `saves/config.cfg` is the versioned preference file; `controls.cfg`/`display.cfg` are read once as migration and `bind.*` is reserved for the unported remap screen. See `docs/notes/persistence.md`. |
| EXP-019 | The starting hold: 7 t with 1 t of hydrogen, as the reference game. | resolved | 2026-10-27: read from the executed original (`--flight hover` with `--geometry` prints `DATA_008889`/`DATA_008891`): at Hope the hold is 7 t (the Saker's 23 t less F998's `0x10`) and 1 t of it is used by the tonne of hydrogen F998 loads (`DATA_008901 = 1`). F998 fits its equipment by setting the object's bytes, so no fitted mass is taken off; the original's cargo code takes free space as `DATA_008889 - DATA_008891` (6 t) and F558 prints used and hold (1 t of 7 t). The earlier "1 t free" reading of F2 was the 1 t used. `DEFAULT_CARGO = 23 - DEFAULT_HOLD_LOSS`, `DEFAULT_HYDROGEN = 1`. |
| EXP-020 | Space dust uses its own seeded stream, no per-frame velocity integration, and is not drawn yet. | implemented | 2026-10-26. `ffe_core::dust` implements F1035's spawn gates, F769's lifetime and F1680's distance cull; the source's shared `DATA_008818`/`DATA_008820` stream is replaced by a field-local `npc::Random` seeded from the system code, so dust does not disturb the NPC traffic's F922/F1530 rolls (the same isolation idea as SIM-010/EXP-006). F715 is the source's empty movement routine, so the port does not integrate `+0x8c`; the only motion is F1517's `velocity << 7` spawn offset and the camera passing the static object. The F1035 attempts follow the source's 20 ms timer cap, so a presentation rate above 50 fps cannot multiply them. Rendering (model `0x6d` through the camera) is a separate follow-up; no `screen.rs` change is in this slice, so the playable's picture is unchanged. See `docs/notes/dust.md`. |

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
| ENH-021 | Modern graphics engine switch (`modern::Graphics`): Classic / Modern on Port Options and Ctrl+G, persisted as `graphics=` | implemented | Presentation only: the engine reads the finished 320x200 frame and never touches game state; Classic is the default and unchanged. First technique: edge-directed 4x upscaling (Scale2x twice) and a smooth fit. See `docs/notes/modern-graphics.md`. |
| ENH-001 | HD textures (`upscaled.bin`, per-asset fallback) | model and planet textures implemented from `img-export` (no packed `upscaled.bin` yet) | Modern with a GPU: an upscaled `img-export/images/<category>/<name>.png` is turned back into the bitmap's colour indices (nearest colour of the `indexed/` key among the indices round the same texel) with a brightness detail, and every face colours it from its own slots, as the original does; files are read again when they change. A missing or unchanged file falls back to the original bitmap. Classic ignores it. See `docs/notes/modern-graphics.md`. |
| ENH-002 | Native-resolution rendering | implemented for the flight view's models | Modern graphics with a GPU: the executor's faces and the planet surfaces' triangles (recorded in view space beside F1840's queue, which is unchanged) are drawn by `ffe-render` (wgpu) at the window's view size with the original's projection, over the original's frame, under its HUD. Stars and dots stay the original's pixels for now. In Modern the planets' surfaces are subdivided one level finer than the "Level of Shape Detail" setting (presentation only), and faces are shaded smoothly across shared corners where the original's flat lighting steps (same texture or hue, similar normals). See `docs/notes/modern-graphics.md`. |
| ENH-003 | Smooth texture filtering | implemented for model textures | Perspective-correct, mipmapped; textures enlarged with Scale2x first. |
| ENH-004 | Anisotropic filtering | implemented for model textures (8x) | smooth-filtering |
| ENH-005 | MSAA | implemented (4x) for the GPU 3-D view, with 1.5x supersampling on top | native-res |
| ENH-006 | TAA | proposed | native-res |
| ENH-007 | Sharp scalable HUD/text | proposed | native-res; must match Classic metrics when off |
| ENH-008 | HDR + tone mapping | proposed | native-res |
| ENH-009 | HDR display output | proposed | HDR + capable surface/OS |
| ENH-010 | Bloom/lens effects | bloom, star glow, glare and lens flare implemented (Modern with a GPU) | The GPU picture's brightest parts (largest channel over 0.8) are blurred at half size and added back at 0.6; a star in view gets a halo sprite eight times its radius in its colour, drawn before the star. The 3-D view is drawn in floating point (colours up to white are shown exactly; brighter ones run towards white), and the star adds a white-hot core, a wide glare and four lens-flare ghosts, dimmed by how much of it the planets hide. See `docs/notes/modern-graphics.md`. |
| ENH-011 | Shadows (incl. ship self-shadowing) | planet eclipses implemented (Modern with a GPU); self-shadowing proposed | An object (station, ship) with a body between it and the star keeps only the share of the star's disc it still sees (umbra, penumbra, annular), down to 30 % of its light in full shadow. The original shadows nothing. See `docs/notes/modern-graphics.md`. |
| ENH-012 | Ambient occlusion | proposed | native-res |
| ENH-013 | Atmospheric scattering + clouds | atmosphere implemented (no clouds) | Modern graphics with a GPU: each planet with an F1877 haze record gets a shell from the surface to the ring's radius (`radius * (1 + word8 / 32768)`), traced per pixel in the record's haze colour and lit by the star, in place of the ring's dithered quads (which are set aside in Modern only). See `docs/notes/modern-graphics.md`. |
| ENH-014 | Higher-quality starfield/nebulae | generated sky implemented (Modern with a GPU) | In open space the GPU generates the sky for each pixel's direction: a Milky Way band along the original nebula band's plane with nebula clouds and dust lanes, and three layers of stars, over the player's space colour. One fixed seed for the whole galaxy, never the game's random numbers. It replaces the original's star points and nebula model (0x13b) in Modern only; "Background stars" off leaves the stars out, and inside a planet's haze the original's sky stays. See `docs/notes/modern-graphics.md`. |
| ENH-015 | Engine glows/particles | lights, engine glows, particle chains and glowing beams/explosions implemented | The models' dots (F1699 to F1801: lights with a halo and a core) are drawn as soft round GPU sprites at their view positions, hidden by nearer faces of their object; spawn timing and sizes stay the core's. F1805's engine glows (quads over bitmaps 0x123/0x65) are drawn as animated GPU flames from the nozzle along the plume (sized from the original's quad, in the glow's own colours; the thruster entry now also keeps A's and B's view positions); thin plumes (F1803 lines) as GPU lines. F1814's particle chains are GPU glows at their view positions (the chain entry now also keeps them), and beams, explosions and a hit's sparks give off light: their saturated colours run past white (grey debris stays grey) and the bloom spreads them. An explosion's coloured faces and complex polygons burn with an animated fire material. Complex polygons (opcode 5) are lifted onto their plane from their resolved vertices and filled on the GPU. |
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

## 2026-10-25 — intro input carry-over (owner requested)

Laser input is disarmed at game entry until held keys are released. Space used
to leave the intro cannot fire or incur a discharge fine. A later Space press
fires normally, including held fire. Implemented using HeldControls and tested
by `intro_space_must_be_released_before_a_new_press_can_fire`.
