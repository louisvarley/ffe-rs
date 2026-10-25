# Flight controls

The port has two flight presets, chosen on the Escape pages' `Port Options` page
or with **F3**:

- **Modern** (default): the port's own WASD/arrow scheme.
- **Classic**: the original's scheme. **Hold the right mouse button and move the
  mouse to steer**; **Enter raises the set speed** and Shift lowers it; the
  keyboard steering is `,`/`.` yaw, A/Z pitch and `[`/`]` roll.

The routing lives in `ffe_rs::input` (`flight_controls`, `original_keys`,
`turret_keys`, `MouseAccumulator`) and is applied in `main.rs`. It is pure and
unit-tested, so a preset cannot silently pick up the other preset's keys.

## Where the bindings come from

- **Set speed:** F1037 (`DATA_007692` Return raises it, `DATA_007694` Shift
  lowers it), ported as `Player::update_set_speed`
  (`crates/ffe-core/src/player.rs`).
- **Steering:** F1038, ported as `Player::steer`. It reads the mouse accumulator
  `DATA_009188` (x in the low word, y in the high word), subtracts the previous
  value, shifts each delta left six and negates it when the corresponding
  Reverse option is on. The held keys are `0x2c`/`0x1e` pitch, `0x33`/`0x34`
  yaw and `0x1a`/`0x1b` roll (`docs/notes/player.md`).
- **Original key names:** Z/A pitch, comma/period yaw, `[`/`]` roll, Enter/Shift
  set speed (`docs/notes/player.md`, `docs/NAVIGATION.md`, README).
- **Fire, missiles, targets, autopilot, views, maps:** the port's existing
  bindings, unchanged by the preset (see the shared table below).

The original does not use a visible fire button for the mouse in flight: the
right button is the steering hold. The left button remains the console pointer
(buttons, target picking, map dragging and docked hot areas).

## Classic, full table

| Action | Classic binding |
|---|---|
| Steer the ship (pitch and yaw) | Hold the **right mouse button** and move the mouse |
| Raise / lower the set speed | **Enter** / **Shift** (left or right) |
| Pitch up / down | **A** / **Z** |
| Yaw left / right | **,** / **.** |
| Roll left / right (original flight model) | **[** / **]** |
| Brake | **X** |
| Fire the current view's laser | **Space** (hold; F931's refire/heat gates apply) |
| Launch a missile at the missile target | **M** |
| Fire the E.C.M. (standard or Naval) | **E**, **;** or **Insert** (needs the fitted system) |
| Deploy chaff | **C** (needs the Chaff Dispenser and Chaff in the hold) |
| Lock next / previous body or ship | **T** / **Shift+T** (or click a body; Shift+T also toggles the combat/missile lock) |
| Autopilot to the locked body (docks at a station) | **K** (or the AUTOPILOT button) |
| Stop the autopilot | **Backspace** |
| Request / cancel docking clearance | **D** |
| Launch from a station or landed state / toggle labels in flight | **L**, or the original action 9: **Tab** while landed |
| System map | **Tab** (in flight only; on a pad or the surface Tab launches, as `F1056`'s action 9) |
| Navigation computer (stations and starports) | **N** |
| Galaxy map: choose the star at the centre as the jump destination | **G** |
| Hyperspace to the chosen destination (also after G closes) | **H** |
| Re-centre the galaxy map | **C** |
| Map page (while the map is open): controls / select / details / picture | **F5 / F6 / F7 / F8** |
| Map page: details with next / previous; zoom in / out | **F9 / F10; F11 / F12** |
| Time acceleration 1x/10x/100x/1000x/10000x | **6 7 8 9 0** (or the meter) |
| Pause | **P** (or the meter) |
| Views: cycle / front / rear / turret / external | **F1 / F6 / F7 / F8 / F9** |
| Aim the turret | **A**/**Z** pitch, **,**/**.** yaw, or the held right mouse button |
| External camera: turn / zoom | **arrows** / **+** **-** |
| Status page | **F2** |
| Combat computer (source F11; the port has no view 6 yet) | **F11** (shows the not-ported note) |
| Escape capsule or Stowmaster (if fitted) | **F12** |
| Preference pages | **Escape** twice: the first press pauses, the second opens them; Escape again returns to the game with time still stopped (the original's `FUNC_000224` order) |
| Starport services at a pad | **F4** (or the console's phone icon), as `F343`; **F1** or **Backspace** returns to the landed view |
| Save / load newest snapshot | **F5** / **Shift+F5** |
| Change ship | **Page Up / Page Down** |
| Docked rows / actions | **Up/Down**, **Enter**, **1**..**4**, **Backspace**, **B**/**N** trade |
| Journal / communications pages (docked) | **J** / **C** (port pages; the original journal is the ship library's viewer and the comms list is `F343`'s; see `docs/notes/missions.md`) |
| Quit from the preference pages | click `Quit`, or select it and press Enter |

The galaxy map's F5-F12 are **map-local**: they drive the original's F834/F840
page only while the map is showing. Outside it the global keys are unchanged
(F5 save, Shift+F5 load, F6-F10 views, F11 screenshot, F12/Shift+F12 model
viewer/lab). The Tab system map and the `n` places list are not the original
map page and keep the global keys. Clicking the map's console icons does not
change the flight view while the page is open.

An open-air takeoff is flown, not automatic: the launch action starts F1013's
fixed up-thrust (`0x24`); above 200,000 mm it hands over to `0x26`, and the
starport's `0x20` effect region only clears when the ship flies out of it. Raise
the set speed (**W** Modern, **Enter** Classic) and pitch up (**Up** Modern,
**A** Classic) to leave the pad; F1031 releases the bay when the region is
behind.

The mouse steering is relative, as F1038 is: while the right button is held each
frame's movement adds to the ship's rates. Releasing the button stops the input
and clears the accumulator, so a later press starts from zero instead of
snapping the ship. In the **prototype** flight model (no O) the same movement is
mapped to a short-lived stick deflection because that model has no rate state;
one full deflection is 50 logical pixels. Both paths use the same gate (Classic
only, right button held, ship or external view).

## Modern, full table

| Action | Modern binding |
|---|---|
| Raise / lower the set speed | **W** / **S** |
| Pitch up / down | **Up** / **Down** |
| Yaw left / right | **Left** / **Right** |
| Roll left / right (original flight model) | **Q** / **E** |
| Brake | **X** |
| Turret aim | **arrows** |
| Mouse steering | none (the right button does not steer) |

Every other action in the Classic table above is shared: Space, M, T/Shift+T,
K, Backspace, D, L, Tab, N, G, H, C, 6-0, P, F1/F6-F10, F2, F12, Escape, F5,
Shift+F5, Page Up/Down, the docked keys and the left-button console pointer.
In the external view the arrows belong to the camera in both presets; Modern has
no ship steering there, while Classic's keys and right-button mouse still steer.

Both presets honour the original's `Reverse left-right controls` and
`Reverse up-down controls` (`Others`, indices 3 and 4); for Classic this swaps
the yaw keys/axis and the pitch keys/axis, and flips the mouse x and y.

## Switching and persistence

- **F3** and the two `Port Options` rows call `Prefs::toggle_modern` /
  `Action::Modern`. The routing reads the new value on the next frame, so the
  change applies immediately.
- Switching disarms every held key and resets the mouse accumulator; a key held
  through the switch must be released and pressed again (the same rule as pause,
  overlays and focus loss).
- The choice is stored in `saves/controls.cfg` as `modern` or `classic`
  (`ffe_rs::settings`). A missing or malformed file falls back to Modern. The
  other preference values remain session-only.

## Display (port extension)

- **Alt+Enter** toggles a borderless full-screen window. The same toggle is the
  `Fullscreen` row on `Escape` -> `Port Options` -> `Display Options`.
- The display page also sets window scale 1x-6x, pixel-perfect or fill-window
  fit, nearest or linear filtering and 4:3 or square pixels. Choices apply
  immediately (the window is reopened because minifb 0.28 cannot resize live)
  and are stored in `saves/display.cfg`. Full-screen is session-only.
- These are presentation choices only: the 320x200 framebuffer, the simulation
  and every capture are unchanged (`docs/notes/display.md`, `EXP-017`).

## Audit and fixes (2026-10-25)

| Action | Before | Now |
|---|---|---|
| Right-button steering | Not wired at all: the right button did nothing in flight and the original mouse word was always zero | Classic only; F1038 accumulator in original flight, deflection in the prototype; right button held |
| Classic thrust | Enter/Left Shift (right Shift missed in the prototype) | Enter / either Shift |
| Classic yaw/pitch/roll | Comma/period, A/Z, brackets | Same (unchanged, now routed explicitly) |
| Modern thrust/steer | W/S, arrows, Q/E in original flight **plus** all the Classic keys (`both`) | W/S, arrows, Q/E only; Classic keys do nothing |
| Modern mouse | n/a | Right button does not steer |
| Turret aim | Both presets accepted A/Z, comma/period **and** arrows | Modern arrows only; Classic A/Z and comma/period plus mouse |
| Modern external view | Fell back to the Classic comma/period/A/Z keys | Arrows are the camera; no Modern ship steering there |
| Preset switch | Changed `prefs.modern`; held keys were not always cleared; no persistence | Immediate, disarms keys, resets the mouse, persists to `saves/controls.cfg` |
| Page/help text | "Classic-like", stale roll/toggle wording | `docs/CONTROLS.md` and `Port Options` name the real scheme; the port's `?` help page was removed, so F1 stays the original view cycle |

No action was duplicated between the presets after the change. `Enter` and
`Shift` are still shared with the menu/station contexts and Shift+T /
Shift+F5, but the flight branch only sees them when no list, page or overlay is
open, so these are context chords rather than preset leakage. `X`, Space, M, T,
K, Backspace, D, L, Tab, N, G, C, F1-F10, F12, F5, Shift+F5, Page Up/Down, 6-0,
P and the left-button pointer are intentionally shared. F11 is the source
combat-computer key and has no action until view 6 is ported.

## Tests

- `crates/ffe-rs/src/input.rs`: per-preset routing, Classic Enter/Shift thrust,
  no Classic keys in Modern and no Modern keys in Classic, right-button steering
  only in Classic, reverse options, the mouse accumulator, immediate preset
  switching through `Prefs`, the source view keys (F1 cycle, F6-F9 direct,
  F10 missile), and the developer-command gate (nothing without `--dev`, then
  Ctrl+F11/F12 and O only).
- `crates/ffe-rs/tests/views.rs`: `f1_cycles_front_rear_turret_external` and the
  optional `f1_cycles_the_view_without_painting_the_removed_help_page` frame
  pair (`FFE_WORLD_CACHE` + `FFE_RS_CLEANUP_CAPTURE`).
- `crates/ffe-rs/tests/controls.rs`: 60-frame flights showing Classic
  Enter+right-button moving the ship and Modern ignoring the same input; an
  optional frame/input strip (set `FFE_RS_CONTROLS_CAPTURE`).
- `crates/ffe-rs/tests/original_flight.rs`: the F1038 mouse word turns the
  original model, and a disabled mouse is ignored without a jump.
- `crates/ffe-rs/tests/prefs.rs`: the Port Options rows and F3 switch, including
  the radio marks on the page.
- `crates/ffe-rs/src/settings.rs`: the `controls.cfg` round trip and malformed
  file fallback.
- `crates/ffe-rs/src/display.rs` and `tests/prefs.rs`: fit rectangles, nearest
  and linear scaling, mouse mapping, the display page and the `display.cfg`
  round trip (`docs/notes/display.md`).
- `crates/ffe-rs/tests/playable.rs`: the retired help page draws no frame.
- Evidence: `docs/evidence/controls/README.md`, `docs/evidence/display/` and
  `docs/evidence/cleanup/` (F1 front/rear pair).

## Removed port-only UI (2026-10-25)

The player-facing build no longer has the `?` help page, the playtest scenario
menu, the F11 screenshot, the F12 model viewer/Shift+F12 model lab or the O
prototype-flight toggle. These were port additions, not source behaviour. F1 is
the original view cycle again, F11 is left for the source combat computer (view 6
is not ported yet) and F12 is the source escape-capsule action.

The developer capability is still available behind `--dev` or `FFE_DEV=1`:
Ctrl+F11 screenshot, Ctrl+F12 model viewer, Ctrl+Shift+F12 model lab and O
prototype flight. The CLI `--capture*`, `--smoke-frames`, `--skip-intro` and
`--start 0..5|sol` inputs are unchanged for the oracles and evidence runs.

## Remaining differences and owner decisions

- The original has no separate "Modern" scheme; Modern is a port addition and
  keeps its own keys. No remapping UI.
- The prototype flight model has no roll axis, so Modern's Q/E roll only has an
  effect on the original flight model (as before).
- The original's `Elite control method in space` and the other unimplemented
  `Others` rows do not change the scheme; only the two Reverse rows do.
- Mouse sensitivity is one logical pixel per F1038 count. The original scales
  DirectInput mickeys, so this may need a sensitivity option after an owner
  playtest.
- `saves/config.cfg` (version 2) is the persisted preference file; it holds the
  preset, display choices and the other applied pages. `saves/controls.cfg` and
  `saves/display.cfg` are read once as a migration. The original's commander-file
  preference block is not imported. `bind.*` keys are reserved for the still
  unported remapping screen (`docs/notes/persistence.md`).
