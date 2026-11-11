# Classic port burndown

What is still to implement for a like-for-like Rust reimplementation of
Frontier: First Encounters, as of 2026-11-10 (updated after PAR-01, PAR-04 and PAR-14 progress, PAR-06 done and a reconciliation with the parity audit and the split cleanup). This checkout is Classic only: the
Modern GPU engine, the FFED3D integration, `assets.bin` art packs, the
launcher and the Enhanced renderer and graphics-pack plans are out of scope and
have been removed.

The source of truth is [`dashboard/burndown.js`](../dashboard/burndown.js)
(open `dashboard/index.html` for the chart). This page is a summary of it. Update
the dashboard first, then regenerate or edit this page to match.

## Summary

**78 of 203 work items are open: 65 in progress, 13 not started.** The foundation,
model-engine, visible-slice and audio milestones are complete (45 items).

| Area | Open | In progress | Not started | Done / total |
|---|---:|---:|---:|---:|
| Parity blockers (2026-10-26 audit) | 20 | 19 | 1 | 4/24 |
| Flight core | 11 | 10 | 1 | 3/14 |
| Galaxy and systems | 7 | 7 | 0 | 6/13 |
| Gameplay systems | 21 | 20 | 1 | 22/43 |
| Original drawing: models | 5 | 2 | 3 | 14/19 |
| Original drawing: screens and effects | 5 | 5 | 0 | 19/24 |
| Content import | 2 | 2 | 0 | 5/7 |
| Saves | 2 | 0 | 2 | 3/5 |
| Classic split cleanup | 0 | 0 | 0 | 2/2 |
| Release and sign-off | 5 | 0 | 5 | 2/7 |
| **Total** | **78** | **65** | **13** | **125 / 203** |

## Critical path

1. **Parity blockers**: the P0 items first (event units, takeoff handler,
   contact rules, Hope start, player explosion), then the P1 items.
   Most of the flight, world and gameplay work depends on them.
2. **`flight-1`, the object system and agent records** (`cstruct`/`gmstruct`).
   Not started, and much of the remaining flight and gameplay work builds on it.
3. **Flight core, galaxy and gameplay systems**, in the order given in
   [STATUS.md](STATUS.md#work-list-to-finish-the-game--2026-11-04-handoff).
4. **Original drawing**: the planet surface quads, the whole-frame pixel
   oracle, sprites and particles, and the last 17 model slots.
5. **Saves, content and release**: legacy commander import, CD 1.01
   verification, full playthrough checkpoints and the owner's sign-off.

## Open items by area

`[~]` is in progress and `[ ]` is not started.

### Parity blockers (2026-10-26 audit) (20 open)

- [~] `parity-01` **P0** Repair F938/F929 event units for fuel, shields and hull repair
- [~] `parity-04` **P0** Translate the complete F1013 takeoff handler and same-frame mode order
- [~] `parity-05` **P0** Replace launch altitude substitution and collision grace with source frame/contact rules
- [~] `parity-07` **P0** Match the original Hope starting state and cockpit view to the supplied image
- [~] `parity-08` **P0** Restore the complete third-person player explosion before GAME OVER
- [~] `parity-09` **P1** Replace orbital docking corridor tests and harmless pushes with source contacts
- [~] `parity-10` **P1** Use original indoor departure placement and moving lift/door state
- [ ] `parity-11` **P1** Complete source autopilot world and target adapters
- [~] `parity-12` **P1** Unify source event clocks, time acceleration and physical integration
- [~] `parity-13` **P1** Complete hyperjump initiation, transit, misjump and source arrival
- [~] `parity-14` **P1** Verify source fuel scooping gates and capacity changes
- [~] `parity-15` **P1** Replace NPC speed caps and simplified flight state transitions
- [~] `parity-16` **P1** Restore system-wide encounter, station traffic and police placement rules
- [~] `parity-17` **P1** Restore source RNG state and call order for gameplay
- [~] `parity-18` **P1** Persist and restore the complete live simulation state
- [~] `parity-20` **P1** Replay market, shipyard, BBS and daily mission events using source state
- [~] `parity-21` **P1** Drive all explosion, debris and hit effects from source object states
- [~] `parity-22` **P1** Replay missile, ECM, escape and legal outcomes in the complete event loop
- [~] `parity-23` **P1** Remove silent fallback to the synthetic playable from normal starts/loads
- [~] `parity-24` **P1** Use the same source new-game setup after death and on every resume path

### Flight core (11 open)

- [ ] `flight-1` Object system and agent records (cstruct, gmstruct)
- [~] `flight-4` Flight integration decoupled from the frame; fixed 50 Hz step
- [~] `flight-5` Original input processing, key accumulation and remapping
- [~] `flight-6` Time acceleration and its RNG consumption
- [~] `flight-8` Player events and state machine (cplayer, plstate)
- [~] `flight-9` Entering and leaving the rotating station frame
- [~] `flight-10` Docking rules and station position/orientation
- [~] `flight-11` Launch sequence
- [~] `flight-12` Scripted flight scenarios traced against the reference
- [~] `flight-13` Open-air starport takeoff (F1013 0x24, F1031/F1032 bay release, F573 return)
- [~] `flight-14` Original autopilot: verified guidance, avoidance, rotating frames and full route replay

### Galaxy and systems (7 open)

- [~] `world-4` Galaxy generation and the RNG stream question (KNOWN_ISSUES F1476)
- [~] `world-5` Planet surfaces and landing approach
- [~] `world-6` Starports: services and events (cstarport)
- [~] `world-7` Hyperspace jumps and arrival (chyper)
- [~] `world-8` Navigation: system and galaxy maps
- [~] `world-9` Lighting by the local star (ships, stations, lift) and day/night
- [~] `world-13` Source model scale and near-surface radius/frame transitions

### Gameplay systems (21 open)

- [~] `gameplay-dust` Source space-dust spawning, lifetime and moving flight rendering
- [~] `gameplay-1` Market and economy tables, pricing and cargo
- [~] `gameplay-2` Equipment and ship purchase (shipyard, outfitting)
- [~] `gameplay-3` Fuel, damage and repair
- [~] `gameplay-5` Missiles and countermeasures
- [~] `gameplay-6` Agents: AI behaviour (cagent)
- [~] `gameplay-7` Police, pirates and other encounters
- [ ] `gameplay-8` Combat scenario replay against the reference
- [~] `gameplay-9` Bulletin board and missions (cbbs)
- [~] `gameplay-10` Journal and mission state
- [~] `gameplay-11` Shuttles and station traffic (shuttle)
- [~] `gameplay-12` Communications and messages
- [~] `gameplay-13` Player death and the escape capsule
- [~] `gameplay-18` NPC flight/combat AI (DATA_006258), ship fire, police/wanted and jump persistence
- [~] `gameplay-25` F1104 death hold, F1098 escape craft, capsule launch and GAME OVER strings
- [~] `gameplay-26` Saker Mk III default arm, F2 loadout and start records
- [~] `gameplay-34` Subsystem damage, F938/F929 repair rows and crash damage
- [~] `gameplay-38` Runtime module clock, source F3/F4 and landed/flight event routing
- [~] `gameplay-39` Source autopilot runtime: retire intercept/slot substitutes and verify cancel
- [~] `gameplay-40` Starport pad descent F741-F746/F652 with 0x59 geometry import
- [~] `gameplay-41` Mission-script engine (F41/F31): script import, journals and scripted missions

### Original drawing: models (5 open)

- [~] `v0.48-7` F1834 / F1835 quad emission (with F1868-F1873)
- [ ] `v0.48-8` Surface quad oracle at a planet-scale distance
- [ ] `v0.48-14` Pixel-level whole-frame oracle (framebuffer mode, station and three ships)
- [ ] `v0.48-16` Sprites, particles and circles (F1609-F1612)
- [~] `v0.48-17` Remaining 17 of 466 model slots and the CD 1.01 census

### Original drawing: screens and effects (5 open)

- [~] `ui-5` Menu and text rendering for every screen
- [~] `ui-7` Intro and new-game flow
- [~] `ui-10` Original palette effects and screen transitions
- [~] `ui-11` Far and near transitions, clipping and planet approach fidelity
- [~] `ui-12` Hit particles, explosions and debris (F939/F942/F926)

### Content import (2 open)

- [~] `content-1` Gameplay tables (ships, equipment, economy)
- [~] `content-2` Message strings, journal and remaining fonts
- [x] `content-4` Optional CD media inventory and lossless packaging (videos and stills) — 323 AVI + 323 BMP, verified FMVs manifest; see [CD media](notes/cd-media.md)

### Saves (2 open)

- [ ] `persist-1` Legacy commander import (read-only)
- [ ] `persist-4` Imported saves replayed against the reference

### Release and sign-off (5 open)

- [ ] `release-1` Full playthrough checkpoints against the reference
- [ ] `release-4` Packaging and distribution audit
- [ ] `release-5` Clean-machine player-data import
- [ ] `release-6` Choose the Rust code licence
- [ ] `release-7` Like-for-like parity sign-off by the owner
