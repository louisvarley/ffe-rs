# Agent streams

Record of the parallel work streams (one branch each), what each owns, the
commits that carry it and how it was integrated. Read with `docs/STATUS.md` and
`docs/PORT_LOG.md`: this file is the branch/merge ledger, not a second status
document.

The integrator owns `docs/PORT_LOG.md`, `docs/STATUS.md`, `dashboard/burndown.js`
and this file. A stream must not edit them; it records its own work in its
`docs/notes/*.md` and leaves the integration entries to the integrator.

## Integration rule

- Merge **one branch at a time** into `main`, never stack two on each other.
- If the branch is behind, integrate it by merging `main` into it or by merging
  it into `main` and resolving there; do not force-push a stream branch.
- Resolve a conflict by combining both intents. Never drop another stream's work
  to make a build green.
- After each merge: `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --check`, plus that stream's local cached-frame tests
  (`FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test <name> -- --ignored`),
  then commit the merge and push. `main` stays green.

## Streams merged on 2026-10-25

All five were based on older `main`s; each was merged in queue order, verified
and pushed before the next one started.

| Stream | Branch commits | Merge commit | Scope | Notes |
|---|---|---|---|---|
| intro plan | `b6c0e16` | `5fc9992` | `docs/notes/intro-todo.md` only | No code; no dashboard item. |
| planets | `1bd1b71`, `a1e96f6` | `4eb7478` | Vanishing-planet fix, surfaces to the horizon, surface starports/buildings, F575 landing wiring | `navigation::Body` gained `inner_radius`/`parent`; one `tests/navigation.rs` initialiser on `main` was updated. `world-5` done (v0.54). |
| lift fix | `204d717` | `ebacdb3` | Launched ship stays in the station until F1089's door opens, hull skipped until clear | Adds `SystemStation.door`; planet tests set it to `0xffff`. DEVIATIONS launch entry renumbered **EXP-011** (EXP-010 was taken). `flight-11` done (v0.55). |
| station UI | `f3cb3d8`, `d93b1ad` | `89581bc` | Original docked services, shipyard, stockmarket, upgrades and ships screens; market becomes one screen | Rewrote the docked screen and the docked input; conflicts in `main.rs`, `screen.rs`, `STATUS.md` resolved keeping the planets/lift guards and the console icons. `ui-4` done (v0.56), EXP-014. |
| prefs | `d37cb84`, `bf80d08` | `7e40a46` | Escape preference pages, space colour, Modern/Classic, Quit | Conflicts in `indexed.rs`, `main.rs`, `screen.rs`, `STATUS.md`; EXP renumbered to EXP-012/013. `ui-9` done (v0.48p). Followed by the owner's space-colour decision (`a780418`): default `#0c0c4c`, Black stays `#000000`. |
| combat | `0c28c87` | `e181f2f` | Crosshair (F1111 bitmap 97, F1112 bitmap 98 velocity marker), Space laser firing (F930/F931/F929/F730/F938), F953/F942 damage, F270 gauges, F1100 range, F998 loadout | Conflicts in `indexed.rs`, `main.rs`, `screen.rs`, same pattern as prefs. `gameplay-4` done; `ui-1`/`ui-2` in progress. Target readout/flash unreachable with no NPC ships. `docs/notes/combat.md`. Also fixed the `ship_position` `[0,0,0]` fallback and corrected the Eagle (model 23) ship in the intro/status docs. |
| hud | `2cccccd`, `4d4f2be` | `1a62452` | Message log/clock, HUD readouts, F1116 lamps, F1120 engines page, 82-record console table | Conflict-free; retired the port's 119..157 block (the lift/nav guards it held are gone with it, not reintroduced). `ui-2` done (v0.58); `ui-1`/`ui-5` in progress. `docs/notes/dashboard.md` records what remains: player damage/equipment words, F991 0x10-0x14, altitude/F1393 units, scanner contacts, F3/F2 key mappings. |
| takeoff | `74da8d8` | `eb1a537` | Open-air starport takeoff (F994/F650 seat, F1013 0x24, F1031 0x26 and 0x20 region, F1032/F651 bay release, F575/F576 pad contact, F573/F1677 return) | Conflict-free. The refusal and the 20 km teleport are gone; EXP-010 keeps only the DATA_008867 climb fallback. `flight-13` added and done (v0.59); remaining count unchanged. Docs: intro/NAVIGATION/STATUS/dashboard no longer say takeoff is blocked. |
| npcs | `81569a2` | `e56b662` | NPC traffic (F688/F635/F1533/F953/F942/F1536/F922), ship drawing/labels/lock/T, F1100 range and F1099 target readout, `--capture-traffic` | Conflicts in `navigation.rs` (kept takeoff's `pads.seat`/models and added the spawner's `launch_axis`) and `screen.rs` (kept the retired `MAP_BUTTONS` note and added `SystemShip`). Target readout/flash now reachable. `gameplay-16` added and done (v0.60); `gameplay-6`/`gameplay-7` in progress. EXP-015 records the placement/AI deviation. `docs/notes/npc.md`. |
| soundfx | `f985ea1`, `d88036f` | see `PORT_LOG.md` | Sound cues and scene music: F1906/F1907/F1908 dispatch and flags, F1902-F1904 music rules, cue→sample table and live event wiring, preference sound toggles | Conflict-free. `audio-2`/`audio-3` ticked (v0.48q/v0.61), `audio-1`/`audio-4` in progress. The effect mixer/device is still a null sink, so cues are recorded/no-op; music plays through HMP/WinMM; song 0 waits for the intro. `docs/notes/sound-cues.md`. |

## Branches not in this round

`nav-buttons`, `ship-ui`, `sound` and `claude/code-commit-capabilities-k4hm9l`
were integrated earlier; see `docs/PORT_LOG.md` for their entries.

## Housekeeping conventions

- Screenshots and `.ppm` frames are never committed. A stream's evidence goes to
  `docs/evidence/<feature>/` as PNG, and `--capture-*` output directories
  (`shots-*`, `shots/`, `screenshots/`) are ignored.
- Each stream's milestone row belongs in `docs/STATUS.md`; the integrator adds
  the matching `PORT_LOG.md` entry, ticks `dashboard/burndown.js` and appends the
  dated history point for the stream with the new remaining count.
