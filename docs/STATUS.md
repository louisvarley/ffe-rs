# Status and handoff

## Two workstreams

This checkout is **Classic only** (owner decision, 2026-11-10): a like-for-like
reimplementation drawn the original's way. The Modern GPU engine, the FFED3D
integration, `assets.bin` art packs, the launcher and the Enhanced/graphics-pack
milestones were removed. [BURNDOWN.md](BURNDOWN.md) summarises what is left.

| Workstream | Owns | Next work |
|---|---|---|
| **Game** | Shared simulation, controls, world data, collision, traffic, economy, missions and saves. | Verify the normal start, takeoff, flight, approach and docking loop against the original. Follow with autopilot and time acceleration, hyperspace, and save/resume. The parity audit below also tracks remaining economy, mission, combat and NPC behaviour. |
| **Classic Engine** | Original software drawing, model fidelity, palette, cockpit and screen presentation. | Close the remaining terrain and clipping, model and effect, and whole-frame comparison gaps in [Original drawing](#7-original-drawing-classic). |

Use the [burndown](../dashboard/burndown.js) for individual task status. The work
list below is the November 4 handoff, with the Modern section dropped. Verify
reopened tasks against current replay evidence before marking them complete.

Classify a bug by what changed. A ship occupying a pad it cannot dock at is a
Game bug if the simulation put it there; if the simulation has it in the right
place and Classic draws it wrongly, fix the drawing. The Game track uses state
and reference replays and Classic uses original frame comparisons.

## Work list to finish the game — 2026-11-04 (handoff)

This is the order to work the burndown's items in, by burndown id; the live
counts are in [BURNDOWN.md](BURNDOWN.md) (77 of 203 open on 11 November). Every item is ported from
`jjffe-master/ffe.asm` and checked against the executed original where a
reference tool can do so; nothing is guessed. Each change updates the burndown
in the same commit and records any approved difference in `docs/DEVIATIONS.md`.
The parity blockers and `flight-1` are the critical path: most of sections 2–4
depends on them.

### 1. Parity blockers (the 2026-10-26 audit; P0 first)

- **P0** `parity-01` fuel, shield and hull repair event units (F938/F929;
  F152's whole-second events now come from the day fraction and its marker,
  checked against the executed original's `DATA_008808` on 10 November, and
  no F938 while the autopilot flies (F1007's mode 2); the `+0x10e` countdown
  and a full reserve trace remain);
  (`parity-02`, refuelling and fuel bookkeeping, and `parity-03`, the drive
  state, are done: 4 November);
  `parity-04` the complete takeoff handler (F1013) and same-frame mode order
  (10 November: a frame-by-frame replay of the original's takeoff matches every
  mode change and set speed; the pad seat now matches; F1014's atmospheric
  drag and heating now run in flight, so free-flight speed tracks within 1.25%;
  a 1% climb drift and a 2-frame-early clear remain, tied to the pad world's
  frame);
  `parity-05` the original's frame and contact rules instead of the launch
  altitude stand-in and collision grace; (`parity-06`, set-speed controls,
  input gates and speed persistence, is done: 10 November; F1121's held-key
  scale and the 50 Hz key accumulators are ported, the Classic slow-down key is
  right Shift only, F1037 is off while the F2 status screen or the map shows,
  and a recorded-input chain test covers the release gate, reverse speed,
  engines off and reload); `parity-07` the original Hope start
  and cockpit view (11 November: the start frame's HUD text, message window
  and hull line now match the executed original pixel for pixel; an orange
  tower's depth, one small object and the scanner contacts remain); `parity-08` the full third-person explosion before GAME OVER.
- **P1** `parity-09` the original's docking contacts instead of the corridor
  tests and pushes; `parity-10` the indoor departure placement and moving
  lift/door state; `parity-11` the autopilot's world and target adapters,
  including F597's time-accelerated branch (it reads the approach words F669
  draws: without it a script attacker at 10000x holds a few hundred km off);
  `parity-12` one set of event clocks, time acceleration and physical
  integration; `parity-13` hyperjump start, transit, misjump and arrival;
  `parity-14` fuel-scooping gates and capacity; `parity-15` the NPC speed caps
  and simplified flight states (script attackers already fly with the
  original's guidance; traffic and pirates do not), now also F702's system-wide
  placement and F238's police response (`parity-16`, F688's spawn, is done);
  `parity-17` the original's RNG state and call order everywhere (the new game
  matches the executed original up to the end of F688); `parity-18` saving and restoring the
  complete live state; `parity-20` market, shipyard, bulletin-board and daily
  mission events from the original's state; `parity-21` every explosion, debris
  and hit effect from the original's object states; `parity-22` missile, ECM,
  escape and legal outcomes in the full event loop; `parity-23` no silent
  fallback to the synthetic playable; `parity-24` the original's new-game setup
  after death and on every resume path.

### 2. Flight core

`flight-1` the object system and agent records (`cstruct`/`gmstruct`; not
started, much depends on it); `flight-4` the fixed 50 Hz step, independent of
the frame rate; `flight-5` the original input processing, key accumulation and
remapping; `flight-6` time acceleration and its RNG use; `flight-8` player
events and the state machine; `flight-9` entering and leaving a station's
rotating frame; `flight-10` docking rules and the station's position and
orientation; `flight-11` the launch sequence; `flight-12` scripted flight
scenarios traced against the reference; `flight-13` open-air starport takeoff;
`flight-14` the original autopilot in full (guidance, avoidance, rotating
frames, whole-route replay).

### 3. The galaxy and its systems

`world-4` galaxy generation and its RNG stream question; `world-5` planet
surfaces and the landing approach; `world-6` starport services and events;
`world-7` hyperspace jumps and arrival; `world-8` the system and galaxy maps;
`world-9` lighting by the local star, day and night; `world-13` the model scale
and near-surface radius and frame transitions.

### 4. Gameplay systems

- `gameplay-41` mission scripts (`docs/notes/mission-scripts.md`): close; still
  open are the station screens other than the bulletin board signalling the
  scripts, F669's event 0x17 raised after the run rather than mid-statement,
  the attackers' time-accelerated closing (`parity-11`), and a check in play.
- In progress: `gameplay-1` market, economy, pricing and cargo; `gameplay-2`
  shipyard and outfitting; `gameplay-3` fuel, damage and repair; `gameplay-34`
  subsystem and crash damage and the repair rows; `gameplay-5` missiles and
  countermeasures; `gameplay-6` AI behaviour; `gameplay-18` NPC flight and
  combat AI, police and wanted state, ships kept across jumps; `gameplay-7`
  police, pirates and encounters; `gameplay-9` bulletin board and missions;
  `gameplay-10` journal and mission state; `gameplay-11` shuttles and station
  traffic; `gameplay-12` communications and messages; `gameplay-13` and
  `gameplay-25` death, the escape capsule and GAME OVER; `gameplay-26` the
  default Saker start, F2 loadout and start records; `gameplay-38` the module
  clock and event routing; `gameplay-39` the autopilot at run time;
  `gameplay-40` the starport pad descent; `gameplay-dust` space-dust spawning,
  lifetime and flight drawing.
- Not started: `gameplay-8` a combat scenario replayed against the reference.

### 5. Saves

`persist-1` read-only import of original commander saves; `persist-4` imported
saves replayed against the reference.

### 6. Content import

`content-1` the remaining gameplay tables (ships, equipment, economy);
`content-2` the remaining message strings, journal and fonts; `content-4` an
inventory of the optional CD media; `content-5` the import checked on CD 1.01 as
well as floppy 1.02.

### 7. Original drawing (Classic)

`v0.48-7` the planet surface's quad emission (F1834/F1835); `v0.48-8` a surface
quad check at planet distance; `v0.48-14` a whole-frame pixel check against the
original; `v0.48-16` sprites, particles and circles; `v0.48-17` the last 17 of
466 models and the CD 1.01 census; `ui-5` menus and text on every screen;
`ui-7` the intro and new-game flow; `ui-10` palette effects and screen
transitions; `ui-11` far/near transitions, clipping and planet approach;
`ui-12` hit particles, explosions and debris.

### 8. Classic split cleanup

Done (`classic-split-2`, 10 November): the viewer no longer records the
Modern engine's side data, and `ffe-core`'s terrain is back to its pre-split
code. The executor still records view-space points for some primitives; F1714's
near-plane outline uses them.

### 9. Release

`release-1` full-playthrough checkpoints against the reference; `release-4`
packaging and a distribution audit; `release-5` importing a player's data on a
clean machine; `release-6` choosing the licence for the Rust code; `release-7`
the owner's like-for-like sign-off.

## Where to look

- [docs/README.md](README.md) is the map of the documentation.
- [PORT_LOG.md](PORT_LOG.md) has every change with its evidence, newest first.
- [DEVIATIONS.md](DEVIATIONS.md) lists the approved differences from the source.
- [audits/gameplay-parity-2026-10-26.md](audits/gameplay-parity-2026-10-26.md)
  and its JSON are the parity items (PAR-nn) with their progress.
- `docs/notes/` holds one note per subsystem; `docs/evidence/` holds the
  captures and runs that back them.
- The status entries from October (incident fixes, the model-engine milestone
  record, the v0.48 plan) are kept in
  [archive/STATUS-history-2026-10.md](archive/STATUS-history-2026-10.md).
