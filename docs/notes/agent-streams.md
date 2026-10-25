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
| soundfx | `f985ea1`, `d88036f` | `0d2a768` | Sound cues and scene music: F1906/F1907/F1908 dispatch and flags, F1902-F1904 music rules, cue→sample table and live event wiring, preference sound toggles | Conflict-free. `audio-2`/`audio-3` ticked (v0.48q/v0.61), `audio-1`/`audio-4` in progress. The effect mixer/device is still a null sink, so cues are recorded/no-op; music plays through HMP/WinMM; song 0 waits for the intro. `docs/notes/sound-cues.md`. |
| equipment | `da4eb8c`, `f5a1d7d` | `7675906` | 66-record `DATA_005289`, `Loadout` (general bits, missile pylons, cabins), F393/F383, `Flight.{loadout, ensure_damage, hit, destroyed}`, F991/F1120 icon state | Conflict-free. `Flight`/`Loadout` API kept public for `shipai`/`missiles`. `gameplay-17` added and done (v0.62); `gameplay-2`/`gameplay-3` in progress. Record 53 follows DOS; laser-code naming flagged for the combat owner. Upgrades rows need F869's `DATA_008995` (don't invent). `docs/notes/equipment.md`. |
| shipai | `dd573c2`, `543f660` | `8788643` | `DATA_006258` AI table, transit/hold/attack/escort/pass/flee/dead/escaped, F585/F601/F597 speeds, F931/F930 fire, F953 player damage, police/wanted, F674/F675/F680 persistence | Conflicts in `npc.rs` doc, `flight.rs`, `tests/combat.rs`; damage API reconciled to `take_damage` with `ensure_damage`/`hit` kept. Follows DOS FFE where JJFFE disables AI laser damage. `gameplay-18` added and done (v0.63); `gameplay-6`/`gameplay-7` in progress. Owner decisions in `docs/notes/npc.md` §5. |
| intro | `b4cd65e` | `7d5c8a3` | F961/F962 intro playback (models 177/176, bitmap 0x42, DOS credits, GameTics, V gate, keys 1-5), `--capture-intro`, cache v27 (`intro-credits`, `char-widths`) | Conflict-free. Scripted captures skip the intro; a stale `console_view` navigation fixture was fixed for F991's equipment gate. `ui-7` done (v0.64). Model 176's stale-basis frames, the dynamic palette and intro music remain; owner decisions recorded. `docs/notes/intro.md`. |
| scanner | `7a82070`, `eda3790` | `b48d981` | F275 scanner contacts (filters, 19-colour table, geometry, orientation), F1113/F1114/F1612 targeter, F1100 size, draw-order fixes | Conflicts in `console.rs` (`ShipSystems`), `combat.md`, `dashboard.md`. Kept equipment's `status`/`general` and added only `ca` (scanner gate) instead of the branch's overlapping bytes. `ui-1` done (v0.65). Auto-targeter arming, per-target range colours and two targets remain. `docs/notes/scanner.md`. |
| effects | `9974338` | `7ccea33` | F939 hit particle (model 158), F942 explosion (model 155), F926 debris, `ffe_core::effects`, `Screen::SystemEffect`, `--capture-traffic` timing | Conflicts across core/nav/screen/tests/docs; `damage_ship_at` reconciled to return `Attack` and call `player_attack`, `tick_ships` keeps the events signature and ages effects. `ui-12` added and done (v0.66). EXPLOSION_MASS 0x4000 and effect lifetimes are owner decisions; model 155 wireframe until the fills. `docs/notes/combat.md` §7.1. |
| missiles | `ef41062` | `c7a980a` | `ffe_core::missile` (tables, F941 impulse, F758 guidance/lifetime/fuse, F718/F721), `ffe_rs::missile`, `Combat::missiles`, `Navigator` missile API, `M`/`Shift+T`, F1114 circle, F1120 lamps, `--missile-probe` | Conflicts in `main.rs` (NPC events vs missiles, lamps), `navigation.rs`, `screen.rs` (effect/missile structs, gunsight interleave), `combat.md`. `gameplay-19` added and done (v0.67); `gameplay-5` in progress. Missile view/NPC launch/ECM and F1113 lamps remain. `docs/notes/combat.md` §8. |
| surface | `f05c403`, `2bbfa20` | `20b3475` | F1021 rough touch 0x32 → F1047 0x34, starport 0x2e/0x30, `Takeoff::from_landed`, F1050/F1054 landed view, F1877 haze sky, `verify-surface.py` | Merge was conflict-free; a `planet_view` fixture that only drew under `FFE_SHOT_DIR` was fixed. `world-12` added and done (v0.68); `flight-8`/`ui-11` in progress. Owner decisions: collision/drawn-radius mismatch, pad landing routing, 0x30 services (EXP-014), landed save, depth/fills. `docs/NAVIGATION.md`. |
| oracles | `f812f2b` | `84e2568` | Worldgen name masking + `world`/`world-linux` split, PASS/FAIL/SKIP `verify-all.py`, combat/ship-types/npc/dashboard/takeoff/prefs oracles, F635 slot-0x3 fix, `tools/reference/README.md` | Conflict in `verify-all.py` only (kept surface's entry/fixes + oracles' list/logic; 55 entries). `foundation-8` added and done (v0.69). Suite 54 PASS / 1 SKIP / 0 FAIL; `ship-types` trace `ae421da6…`. Remaining gaps recorded. |
| audiomix | `638f32a` | `ff6ddc2` | `ffe-audio::mixer` (WAV, 40 samples, 16 voices, F1908 rules, F1907 centibels), WinMM `waveOut` sink, `GameAudio` wiring, intro song 0, cache v28 `base:music/qqfront` | Conflict-free. Cache rebuilt to v28 (1,797 assets / 1,798 entries); suite 54 PASS / 1 SKIP / 0 FAIL. `audio-5` added and done (v0.70), `audio-1` done, `audio-4` in progress. Continuous beds/positional/`.mid`/other songs/device picker remain. `docs/notes/sound.md`. |
| palette | `fe4aa1d` | `ee508d5` | F1326/F1327/F1328/F1329/F1330/F1331 runtime palette (64-box median cut, two banks, fixed low palette/map), textures/model_viewer registration, intro F1575 colours, `palette_reference` | Conflict in `docs/notes/intro.md` only (combined; kept music/model-176 notes). `v0.48-15` done, `ui-10` in progress (v0.71). EXP-013 preserved. Terrain queue colours/one build per render/intro ramp remain. `docs/notes/palette.md`. |
| npc-dock | `f42b9b2` | `acfa1fd` | Corrected `DATA_006255` rows, Approach/Docked/Launch/Depart, `DockRequest`, bay occupancy, F747 door wait, destination traders, hidden docked ships, `npc_docking` | Conflict-free. `gameplay-6` done (v0.72). Owner decisions: 2-3 day interval, 30 s give-up/next station, cruise-speed floor, despawn range. Missiles/police spawns/Thargoids remain. `docs/notes/npc.md`. |
| station-services | `13ae64c` | `570b276` | F869/F622 starport tech level, `outfitting` Fit/Remove + F387/F388 prices/refunds, F394 fuel scoop, F329 Remove button, F444 repairs rows, shipyard/BBS empty states | Conflict in `lib.rs` only (kept `palette` + `outfitting`); the rest auto-merged with npc-dock/missiles/effects. `gameplay-20` added and done (v0.73); `gameplay-2`/`gameplay-3`/`world-6` in progress. Owner decisions: drive/mining rows visible-but-disabled, ship/BBS imports later, repairs before thruster damage, price multiplier. `docs/notes/station.md`. |
| fills | `210d749` | `88916b0` | `Shape::TexturedUv` (F1645/F1648 UVs), `rasterise_uv`, `terrain_texture` (F1805/F1807/F1806/F1808 + slot table), `Screen::draw_shapes`, fills evidence | Conflict-free. Colours resolve through palette's runtime entries first (fixed map only for unbuilt words); EXP-013 preserved. `v0.48-6` done, `v0.48-7` in progress (v0.74). Owner decisions: model-155 placement/+0x7e, upper-palette approximation, shading, no F1806/F1808 pixel oracle, EXPLOSION_MASS/lifetimes. `docs/notes/fills.md`. |
| labels | `653f539`, `236b4a8` | see `PORT_LOG.md` | F1108 pen + (4,−4), F1584/F1598 top clipping, F1587 0x13c limit, `--labels` adapter/`verify-labels.py` (13 cases), cached label frames | Conflict-free. `ui-13` added and done (v0.75). Suite now 56 entries (55 PASS / 1 SKIP / 0 FAIL). Owner decisions: registrations vs model+serial, 3D hull text scaling, near-target suppression/overlap/mission labels. `docs/notes/labels.md`. |

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
