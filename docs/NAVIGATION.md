# Getting around: systems, maps and the autopilot (progress)

Current source-parity correction (2026-10-26): the default Saker has **1 t free
hold**, source bay 0 via F994's temporary bay-1 reservation, and F998/F997's
half-turn after pad placement. Older 7 t/EXP-019 statements below are historical.
See [implementation record](notes/parity-ground-drive-2026-10-26.md).


> 2026-10-26: the player path now uses integer source guidance rather than the
> sandbox transfer controller. See [the source audit](notes/autopilot-source-audit.md)
> for the exact coverage, 512-case oracle and remaining gaps. Older sandbox
> arrival/transfer claims below are historical developer evidence.


Started 2026-10-24 at the owner's request: a system map, a way to move between the
systems, and an autopilot to planets, stations and other bodies for testing.

## Status

| Piece | State |
|---|---|
| `ffe_rs::navigation::galaxy` | done: the 12 hand-coded systems (Sol, Alpha Centauri, Sirius, ...) with their names, plus four generated test systems |
| `navigation::bodies` | done: a system's stars, planets, moons, stations and starports with the original's names, placed by F880 at a game clock (positions in millimetres) |
| `navigation::autopilot` | done and tested (`tests/navigation.rs`, local cache): steers the original flight model with its own keys (pulsed), holds a braking-curve set speed, hands back a transfer for long legs |
| Map and jump UI in the playable | in progress (keys below) |
| Console `n` icon: the system's places list | done 2026-10-24: the original's stations/starports list, marker clicks engage the autopilot (`docs/notes/labels.md`) |
| Console `l` icon: text labels | done 2026-10-24: the original player flag, F989/F1108 labels over the flight view |

The default boot is now F998's type 0: Gateway [-1,4], Hope and Old Blackelk,
the Saker Mk III (model `0x1f`), 10,000 CR and a 7 t free hold (EXP-019), 1 Jan
3250 12:00, landed on the pad (`navigation::new_game`, `docs/notes/intro.md`).
`--start 1` keeps Ross 154 and the Eagle (model 23). `L` takes off from the
open-air pad through the original's F1013 `0x24` -> F1031 `0x26` -> `0x00` path
(EXP-010).
The development sandbox (`--start sol`)
starts docked at a station **in a planet's orbit** - the closest such station,
else the first station - so the launch puts that world in view (Sol: Mars High,
1.61 Mars radii). That is a starting-scenario choice, not the original's fixed
start.
| Stations in the flight view and docking | done (2026-10-24), ffe-rs rules: see "Docking" below |
| Open-air starport takeoff | done (2026-10-25, verified from the default start): F1013 `0x24`, F1031 `0x26`, F1032/F651 bay release, F994's planet parent for F603 (`docs/notes/intro.md`) |
| Galaxy beyond the 12 hand-coded systems | partly done (2026-10-26): a new game offers its start system's galaxy-table neighbours within 30 LY, generated from the sector star generator and name strings exactly as F852/F860/F832/F878 do (`navigation::galaxy_around`; e.g. Titican, 4.42 LY from Gateway, with the station Argent). Systems beyond that radius are still not on offer |

## The autopilot

The live autopilot is the source's AI path (`Navigator::source_command`): F741's
destination (a station's `0xfa0` offset, a starport's point `0x1770` up its row, a
planet's centre less F610's radius), F592/F596/F597's guidance and F587's turns
(`ffe_core::autopilot`), F745's clearance and F746's pad leg. The older
`prototype_command` (time-skip transfers) is kept for development tests only.

- **F597's accelerated arrival.** With time accelerated (`DATA_009013`: 0, 3, 7, 10, 12
  for 1x..10000x), once `distance shift - max(velocity shift, 2) - (accel + accel/4)`
  drops below 6, the ship is put at the destination point with the target's velocity
  and no thrust. A clearance granted on that arrival runs F745/F746 at once, as the
  original's next accelerated AI tick does. Without it a 10000x frame near a station
  overshot back and forth ("jumping around") and never reached clearance range.
- **Port rule: detour.** F741/F742/F610 fly a straight line, which runs through the
  planet between two ports of one world. While the line's lowest interior point is
  within a world's outer radius plus a tenth of it (the outer radius bounds the
  terrain; `inner_radius` is only its lowest point), the autopilot steers elsewhere:
  below that clearance shell it climbs at 45 degrees toward the target's side, to a
  point as far ahead as the shell is above it; on or above the shell it aims at the
  line's closest point pushed out to the shell, which lies between the ship and the
  target, so the target stays ahead. (A waypoint a fixed step ahead of the ship, the
  first version, held the ship to a crawl away from the planet with the target behind
  it.) F597's arrival takes precedence.
- **Port rule: time acceleration near worlds.** Inside a planet's or moon's outer
  radius the acceleration is held at 100x: one 10000x frame is nearly three minutes of
  thrust in a fixed direction beside the ground of a 2 g world.
- **Frames.** Inside a world's atmosphere the ship moves with that world (or a starport
  on it within 200 km), never with an orbiting target; carrying it along a station's
  orbit from just above the ground threw it into the terrain.

`FFE_TRACE=1` prints the flight state (mode, velocity, contact, frame, target and range)
ten times a second, which with `--replay` shows exactly what a recorded flight did.

## Hyperspace (F908/F909/F1066/F1094)

- **Initiation (F908).** From mode 0 or 0x24 with a destination, the altitude at least
  0xb71b00 and F901's range and fuel; the fuel is burnt and sample 0x21 plays.
- **F909/F912.** The destination system is built at once and the ship put at rest (no
  set speed) on F1507's orbit about the root: ffp radius `random(0x2000) | 0x4000`,
  exponent 0x33 (7.5 to 11 AU), longitude and phase from the random words. Any
  autopilot ends. The original's random state is not reproduced.
- **Transit (mode 4).** F1030 runs no physics; the clock runs at F908's 0xf880000
  (2^18 x 1x) until the jump time is used, ending exactly on it (the main loop's
  rollback). F1066 draws model 0xa5 on F1575's (0, 0, 4) with global 0 = elapsed share
  x 145 << 8: about a third of a second a day of jump time.
- **Exit (F1094).** Mode 0 at 1x, velocity 0x71b7 along the system's z axis, the
  system's traffic, and the hyperspace song when that switch is on.
- **Not yet:** F909's mis-jumps (one jump in 128 by `F1530 > 0xfe00`, or Left Alt held:
  F910's outcomes from `DATA_007190` - the "Mis-Jump", "Forced Mis-Jump" and "Mis-Jump
  - Drive Destroyed" messages, deep space with no system, F925), the exit cloud F1094
  leaves (model 0x9a, AI state 0x1e) and the mission events at the special systems.

## Keys (the playable)

The DOS default start is landed at Old Blackelk on Hope (`--start sol` keeps the
older Sol/Mars High sandbox). At the pad the screen is the original's landed
flight view; the launch action (plain Tab, or the port's `L`) asks F654 and
flies the takeoff (`docs/notes/intro.md`). F4 opens the original's station
services root (`docs/notes/station.md`) over the landed view: Launch Request,
Shipyard, Bulletin Board and Stockmarket, with the original's title, rows,
buttons, hot areas and `1`-`4` keys. Up/Down choose a row, Enter or a click on
the row's button activates it, Backspace/F1 leaves the board, and the
Stockmarket opens the verified market (B/N still buy and sell the selected
line). Docked at a station the same board is the state `0x2a` screen. Wired in
`main.rs` (not the original's map screens):

- The console's map icon (or G): the **galaxy map**, the original's 3D scene (`docs/notes/galaxy-map.md`): the stars around the
  centre with their names, the grid, the range sphere of the drive and a marker on the star nearest the centre, whose name,
  distance and description show in the console and whose jump cost shows under the sector. The marker is the hyperspace
  destination (the original's `+0x268`/`+0x27c`), not a body lock: it is stored when the centre picks a star and survives
  closing the map, so H (or the console's hyperspace icon) jumps after G closes too. Arrows scroll, Page Up/Down or +/- zoom
  (Shift slower), dragging turns the view, C returns to your system, H or Enter jumps to the selected star if it is one of the
  systems on offer, G closes. The jump is the hyperdrive's as below; the list of systems is gone from the playable.
- The console's navigation-computer icon (or N): the original's list of the places in this system — its
  stations and starports, two columns of nine (markers at x 10 and 230, names at x 25 and 245, 15-pixel rows
  from y 5) with the original marker bitmap and green colour 17, over the view and with no panel. A click on
  a marker row (only the marker is a hot area, as in F1099) locks that body and engages the autopilot.
  Left/Right (or Page Up/Down) page it if a generated system has more than 18 places. Tab remains the ffe-rs
  system map of every body: a click on a row (or Enter) engages the autopilot to it and closes the list.
  Backspace turns the autopilot off. See `docs/notes/labels.md` and `docs/evidence/places/` (the original's
  Gateway frame from the manual).
- The labels icon or L toggles the original's text labels: the names of the system's bodies over the flight view
  (colour 15, at the object's screen position). The console icon lights while they are on.
- F3 (and Escape → `Port Options`) changes between Modern and Classic controls. Classic is the original's scheme: hold the right mouse button and move the mouse to steer, Enter/Shift raise/lower the set speed, A/Z pitch, comma/period yaw, [ ] roll. Modern uses W/S (set speed), Up/Down (pitch), Left/Right (yaw) and Q/E (roll). The presets do not share steering keys; `docs/CONTROLS.md` has the full binding table.
- **Target and autopilot buttons** (below the view, beside SYSTEMS and THIS SYSTEM): a click on a planet, moon or station in the
  view locks it (green corner brackets and a `TARGET:` line with its range); **TARGET** (or T, Shift+T the previous; the
  console's `t` icon too) locks the next body by range, nearest first. **AUTOPILOT** (or K) flies to the locked body: at a
  station it flies to the approach point on the docking axis, asks for clearance and docks by itself; **STOP** (or
  Backspace) turns it off. The brackets turn yellow and the line reads `AUTOPILOT:` while it is flying. The autopilot starts
  the original flight model if it was not running (as O does). Open-air starports are flown to but not landed on yet.
- D, flying in a system, asks the nearest station within 100 km for docking clearance (again cancels it). Fly into the slot on the station's docking face; or engage the autopilot on a station, which docks by itself.
- F11 is the original's combat-computer key; view 6 is not ported, so it has no
  action in the player build. F12 is the source escape capsule. The port's old
  F12 model viewer and Shift+F12 model lab are developer-only (`--dev`,
  Ctrl+F12 and Ctrl+Shift+F12); the help page and playtest scenarios are gone.

Planets move on the game clock (days plus a fraction), children relative to their parent. Long legs are transfers (the clock jumps and the ship lands short of the body's future position); the last stretch is flown in the target's frame (`Navigator::follow`) because the autopilot cannot catch a body moving at 30 km/s.

## Planets in the flight view

`Screen::discs` (set each frame in `main.rs` from the navigator's bodies). A body
with a model is drawn as its real surface: the planet model runs through
`ModelViewer::render_scene`, whose curved-surface command goes through the ported
F1876 and emitter (`ModelViewer::draw_surfaces`; the body's `+0xa0` is the seed).
The viewer's view space is 32-bit, so a distant planet is drawn through a model
matrix shrunk by a power of two with its position (the picture is unchanged); the
body is a shaded disc only when the model cannot be drawn. Stars are drawn the same way (their models 137-148 are surfaces of the emitting types, with a haze ring as the glow). Height
bands pick the ramp colour of the detail entries and the middle slot colour of the
textured ones (the texture fills themselves are not drawn).

The terrain's 32-bit arithmetic overflows for radii near 2^29 (the oversized slivers
at close range were that), so the matrix is also shrunk until the radius is about
2^24, the range the original runs it in. The atmosphere (F1877, F1878, F1879) is ported and verified (`terrain_haze.rs`), and
the ring around a planet with a haze is drawn from its queued group. Known gaps: the
textured fills (bitmaps 0x40, 0x43, 0x121) are drawn as flat colours or blends, the
sky colour starts black each frame (the game sets it elsewhere), and the surface seed
is the object's `+0xa0` only for bodies of the navigator. The planet is oriented by
its object's rows (the spin is `clock[0] >> 17`, so they are identity until the clock
passes 2^17 days). Looks checked by eye (`tests/planet_view.rs`, `FFE_SHOT_DIR`).

**The close-range fix (2026-10-25).** Two things used to make a planet disappear on
approach. The flight view dropped every disc with `z <= radius` (the centre nearer than
one radius along the view axis), which is exactly the geometry of a landing approach;
and the approach drew the *inner* base sphere (`+0x140`) while F1533, F581's terrain
query and F879's starports use the *outer* radius (`+0x138`). A rough landing settled
at about 0.9 outer over a 0.64-outer drawn base and saw only sky. `main.rs` now draws
the inner base on approach and the outer radius once the ship is settled
(`navigation::surface_radius`), so a rough landing sits on the ground the landed view
draws. F1876's far side is culled for an eye inside the base and F1877's ring/sky is
left out there, so the landed view shows ground under the space colour. The old filter
is gone; the course from 3 radii down to the ground stays drawn (regression:
`planet_is_drawn_at_and_inside_one_radius`, frames `surface-k1.00.ppm` ... in
`FFE_SHOT_DIR`; the cached landed test also asserts ground pixels below the horizon).

**The surface queue at close range.** F1876's triangles go through F1778's
order queue, whose original buffer is 50,000 words. A planet at 3 and 2 radii
needs 59,190 and 63,125 words, so the original exits there; the port used to
stop drawing at the limit and leave a large black (unpainted) wedge. The flight
view now draws surfaces with a 131,072-word queue (`Queue::with_limit`,
EXP-009 in `docs/DEVIATIONS.md`); the queued words and their pixels are
unchanged and the reference oracles keep the original limit. At the autopilot's
standoff (1.5 radii + 50 km) the queue fits in 50,000 anyway. The dark parts of
a planet that remain are its night side, not holes.

**Capturing an approach.** `ffe-rs.exe --root . --capture-planet DIR` puts the
ship three radii out from the nearest planet on the station's side, locks and
autopilots to it with the real controller and original flight model, presents a
fixed 0.1 s per frame and saves `planet-XXXX.ppm` every 100 frames (about 60
frames and two minutes of wall time for Sol/Mars). When the planet standoff is
reached it re-targets the nearest starport of the system and flies to its 1.5-hull
standoff, so the last frames show the buildings; the 6,000-frame cap is enough for
small worlds but large ones (Mars is still 6,000 km out at the cap) need the
surface test frames below. `--capture DIR` remains the scripted launch.

## Starports and cities

There are no separate surface city or
building objects in the original's system generator. `F891` (`ffe.asm:66229`) creates up to
four starports per planet, copying the template `DATA_008796`, naming them with type 6 and
taking their model from `DATA_007132` (the 0x51-0x5a range; Sol's Earth gets 0x53, 0x54 and
0x5a, the Moon 0x60). `F880` (`ffe.asm:64596`) makes them kind 0x23 objects, sets the
open-air class bit 0x40 for models 0x51-0x5a, gives them 5 or 1 bays and calls F879 to put
them on the planet at the generated latitude/longitude. The buildings are inside those
starport models (their child models), not separate system objects. The only "city" object
is the orbital city: kind 0x22, model 0x50, created by `F892` (`ffe.asm:66365`) in orbit,
which the flight view already drew. So the missing pieces were not extra city objects: the
port did not put `Kind::Starport` into `Screen::stations` at all, and the planet's close-range
scale hid what was there. Starport models 0x51-0x60 are now drawn from the surface, from
orbit and in the starport approach (`surface_frame_shows_a_starport`, frame
`surface-starport.ppm`). The original's models can be checked with `--station-model` and
`tests/station_models_reference.rs`.

## Docking (ffe-rs rules; EXP-005 in docs/DEVIATIONS.md)

`ffe_rs::docking::Port` and the navigator's `request_clearance`, `contact` and `dock`.

- **Stations and starports in the view.** `Screen::stations` (set each frame in `main.rs`): every
  station and starport of the system drawn from its own model (72 trading post, 73 orbital station,
  80 orbital city, 0x51-0x60 starports, ...) at its generated place, turned by F866 at its own
  speed byte (`Body::spin`, `+0xc8`) over the navigator's clock, lit from the star. Starports have
  speed 0 (F865/F866 only animate stations; `docs/notes/animation.md`), so their F879 surface
  orientation stands. Beyond 2,000 km from the eye a station is not drawn (under a pixel; the
  viewer's view space is 32-bit).
- **The slot.** Every generated station model has its slot on the +z face (looked at for
  72, 73, 74 and 80), so the docking axis is the object's third row. The face distance is the
  largest vertex z of the model (72: 197 m, 73: 238 m, 74: 156 m); the orbital city's root
  vertices stop short of its hub (a child model), so it takes a quarter of the radius (1.96
  km). The corridor into the slot is 15% of the face distance either side of the axis, at
  least 60 m.
- **Traffic control (the original's).** D is F655 on the nearest station's record
  (`ffe_core::traffic`, `docs/notes/docking.md`): granted with a bay, "already granted", busy with
  another ship, or no bay free; clearance runs out after 28,800 game seconds. The autopilot asks at
  its approach point and stops if refused.
- **Contact.** Within the face distance of the centre: in the corridor in front of the face
  with clearance and under 250 m/s, the ship touches the pad (F652) and the original's timed
  docking stages start (about 26 s: align, open the door, into the lift, turn, close, wait, open the
  inner door, into the bay) with the ship held still, then the docked screen at that station; in the
  corridor otherwise, it is pushed back out along the axis; anywhere else, pushed out from the
  centre (no damage). L from the docked screen asks permission (F654) and runs the launch stages
  (about 20 s) before the ship is put out on the docking axis, facing away; the door then opens, and
  the bay is released. The lift scenes are the original's (`ffe_core::lift`): the cage, shaft and bay models in the
  front view, over the outside view when docking and alone when launching; the status line also names each stage.
- **Autopilot.** The source map is F1020 (engage: target byte and AI state 8), F741
  (state 8's set-up: the target's approach offset), F742/F610/F597/F599/F587 (distance,
  steering rates and thrust), F745 (F655 clearance and the pad vector), F746 (state
  `0x23`'s final approach and F650's pad descent) and F1018 (cancel: mode 0 and set
  speed `DATA_008866` = 0). A station target is flown in two legs: to F741's point
  4,096,000 mm out on the current docking axis, then, with clearance taken there, into
  the slot. F742's arrival test is the distance exponent (F610's `+0x10e`, `dist_cam`),
  so the 1,024,000 mm standoff (`distance_byte` 6) is the port's 1 km `APPROACH_STOP`.
  The final leg re-aims at F650's slot on the **current** axis every frame
  (F745/F746): off the axis it lines up on a gate outside the face, then enters along
  the axis braking for the 250 m/s slot limit. A straight line to the approach point
  that would cut the hull (a jump arrival on the station's far side) is routed around
  a safety sphere first. Sol's Daedalus, Abraham Lincoln and Galileo dock from the
  start (`tests/navigation.rs`, `autopilot_docks_at_stations`; `FFE_SHOT_DIR` saves
  frames on the way in), and the eight-system, three-bearing run
  `autopilot_docks_at_stations_from_every_bearing` docks at every orbital station
  without a hull push. An open-air starport target is flown to F741's point 6,144,000
  mm above the port and asks F655 there; F745's grant keeps the target and marks
  `final_leg`, then F650's pad/F746's descent/F744/F652's touch reach F1048's `0x30`
  (`starport_autopilot_lands_on_the_pad`; `docs/notes/autoland.md`). The port's `0x59`
  collision geometry imports and the pad scan runs F576/F580 in F994's three-body
  frame (`step_starport_landing`).
- **Long legs and cancel.** The long leg is the port's own convenience (the source has
  no transfer). Its destination is the body's position at the advanced clock, solved by
  bracketing the transfer-time equation (`Navigator::intercept_time`), not by iterating
  from the current position: the old fixed-point loop diverged for fast planets (Sirius'
  Lucifer, Test-4 1) and teleported in circles. After `MAX_SKIPS` transfers in a row the
  remaining leg is flown, so a target the transfers cannot catch cannot strand the ship
  in the loop. Backspace cancels: F1018's leg state is dropped and `OriginalFlight::halt`
  sets the set speed to 0, so the ship brakes instead of coasting on.
- **Frames.** Bodies move between the navigator's refreshes; the ship is carried with the
  body it is near (the engaged target on its final approach, else a station within 200 km,
  else a planet or moon within 50 radii), so a station stays where the ship left it.
- **Landing (2026-10-24).** The playable now runs F569's scan through
  `OriginalFlight::step_landing` (`Player::tick_with` with the 0xff crash mask) whenever
  `Navigator::landing_parent` puts the ship in a body's frame: the nearest starport within its
  hull plus the ship's radius, else the nearest planet/moon whose outer radius contains the ship.
  A two-body world (player 1, parent 2) gives F575/F576 the parent-relative position, so the
  terrain or the starport's collision stream runs with `SurfaceTools`; the landing gear word
  `+0x9e` is 0xffff. A slow, level touch (speed <= 0x258, dot >= 0x78000000) sets flag 0x40 and
  settles the ship on the surface; a faster one sets 0x80 and bounces it, and F953 takes the
  bounce's amount off the shields and then the hull (F942/F1104 destroy the ship when it is
  used up: the game-over hold). The source prints no crash message, so the port no longer
  does either (`docs/notes/collision.md`). Regression: `soft_touch_lands_and_fast_touch_bounces`.
- **The landed state (2026-10-25).** The DOS default new game is record 0 of F829's
  start table: Gateway [-1,4] on the planet **Hope**, at the starport **Old Blackelk**
  (`slot 0x6a`), with F998's default Saker Mk III, 10,000 CR, a 7 t free hold
  (EXP-019; the verified original is 1 t) and state `0x30`. (Record 1
  is Ross 154's Eagle, `slot 0x6e`; `--start 1` keeps it.) The boot's state `0x30` is
  F1050/F1054's landed surface view -- the flight view from F994's pad seat with the
  ground, horizon, starport, scanner, gunsight, console and message log -- not the docked
  services board. The HUD is F1099's: Set/Act at (2,148) in F1393's km/h conversion
  (`*0x156/100` one decimal below `0x4705d`, `/0x69` two decimals above), Alt at
  (266,139) and `DATA_009124`'s parent name (Hope) beneath it. The boot message
  is only `Landed at {name}`: F687/F686's start entry spawns F688's traffic, but
  the port does not force a pirate group beside the pad (a normal pirate group
  uses the anchor orbit, not `PIRATE_DISTANCE`), so the stationary ship cannot
  be attacked before launching. `L` starts F381's F654 and F1013's climb. The
  same guard covers a surface landing: `Navigator::landed_view()` makes the main
  loop skip F688's NPC tick (F1012/F1048 run no flight step), so a landed ship
  takes no F953 fire even when the system still has traffic.
  A soft touch on a planet or moon takes the original's states: F1021 sets `0x32` for the
  frame of the touch (F1042 zeroes the speed, rates, thrust and set speed), and F1047's
  view frame makes it `0x34`, the guarded landed-rough state (`Navigator::land`,
  `landing_frame`, `ffe_core::traffic::Landing`). The landed screen is F1050/F1054: the
  ordinary flight view from the settled ship, the console and message log unchanged, and
  the world still turns around it. F1012 runs no flight step while landed (no
  F1037/F1041/F601/F603/F1512), so the ship is held still; `nav.follow` still carries it
  with its planet. The launch action is F1056's action 9, plain Tab (`L` is the
  port's convenience and the source's label action otherwise): F1013 starts at
  `0x24`, the fixed up thrust climbs, F1013 hands over to `0x26` and F1031
  clears it to `0x00` (the same `Takeoff` machine as the open-air pad, without
  the pad world). The open-air starport's
  parallel `0x2e` -> `0x30` pair is in the machine too (`Landing::at_starport`). Tests:
  `takeoff.rs` pins the transitions;
  `planet_view.rs::landing_reaches_the_surface_state_and_launches_again` runs the whole
  cached sequence with frames; `planet_view.rs::new_game_hope_landed_surface_view` pins
  the default start, its Hope HUD strings and the landed frame;
  `tools/reference/verify-surface.py` compares the type-1 new game's landed-at-starport
  view with the executed original's (`--game-frame 500 0`).
- **Services.** The docked services board is the original's state `0x2a`, opened by a
  completed docking (`Traffic::Docked`) and by the docked screen's own navigation.
  From state `0x30` at an open-air starport it is F4 (or the console's phone
  icon): plain F4 is action `0xf4` in `DATA_007688`, module 12's event 1,
  `F343`, which tests `0x2a`/`0x30` and calls `F333`; the board is drawn over
  the landed view, and F1 (the console screen button, action `0xf1`) or the
  port's Backspace returns to `0x30`. The new game still boots into the landed
  view, not the board (EXP-014); `docs/evidence/start-flow/` has the reference
  and port frames, and `default_start_services_board_is_old_blackelk` pins the
  title and rows.
- **The landed background.** F1052 clears the upper view to the previous scene's
  `DATA_008814..8817` colour and draws F1775's stars only while the flag is clear. At a
  starport or in a rough landing above the haze surface the flag stays 0, so the sky is the
  original's space colour; F1877's atmosphere colour is carried into the next frame when the
  flag is set (`ModelViewer::haze_sky`, `Screen::surface_sky`).
- **Not the original yet.** The pad hit when docking at an orbital station is this port's slot
  test, not the station's collision stream; the effect sounds and the ship's motion with the
  rotating station during the stages are not ported. Music is now the original's: F1091 and
  F1048 stop the song, F1047 changes none, and F1094's random song is the hyperspace-exit cue
  rather than a docking one. The starport/planet scan does not yet draw the ship
  in the rotating frame between ticks (it is flattened to the parent's linear frame each frame),
  and starports on the far side of their planet are not hidden by it (there is no depth buffer
  between the planet surface and the station models).

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Docking entry speeds through the autopilot are about 22-23 m/s: the set speed steps down
  at the keys' fixed rate, so the real speed is braked against the curve; the final leg's
  standoff keeps the face crossing under the 250 m/s slot limit. The off-axis and
  far-side runs in `autopilot_docks_at_stations_from_every_bearing` record zero hull
  pushes.
- Planet surfaces draw their triangles, but the textured fills are flat colours or
  blends (see above), so the detail is blockier than the original's; the night side is
  drawn with the shade table and has no atmospheric glow except the ported haze ring.
- **Terrain collision fidelity (resolved for the settled view, 2026-10-25).** F581's
  terrain record tests the ship against the model's surface at the body's `+0x138`
  (outer) radius. On approach the flight view draws the inner base sphere (keeping the
  eye outside the shell); once settled it draws the outer radius
  (`navigation::surface_radius(body, true)`), the same one the contact uses. A rough
  landing therefore settles on the drawn ground; the cached test
  `surface_and_landing_use_one_radius` checks both radii and `landing_body`'s outer
  radius. F1877's haze is left out for an eye inside the base so it cannot paint over
  the landed ground.
- **Starport landings from flight.** The F1019/F1021 open-air `0x2e` -> `0x30` pair is
  routed (`Navigator::land_at_starport`, F1048's frame, F654's launch request), and F1047/F1048
  follow the source's music rules. F745's grant keeps the target and the pad leg; the
  autopilot then takes F650's pad and F746's descent. The F569/F576 scan now runs in
  F994's three-body frame (`OriginalFlight::step_starport_landing`); the cache's
  model-data gap is fixed (model `0x59`'s geometry imports even though its command
  program does not, so F580's child anchor resolves for 0x51-0x60). Manual pad touches
  and the F746/F744/F652 touch are tested. A soft touch inside the port's `0x20` region
  lands only as the source's mode allows (`Navigator::region_touch`): F1019 with the
  autopilot (mode 2), F1024 with clearance (mode 6; another pad than the allocated bay
  only gets F1022's warning), and in free flight F1021 sets F1013's `0x24` climb again
  with F1022's warning once (message `0x98be`, in the port's wording). Before this any
  touch on a roof or the ground near a port "docked" the ship. The `D` clearance request
  (F655) now asks an open-air starport as well as a station. What remains is the automatic **lateral
  capture**: the port's controller (F599's request plus the navigator's one-second
  `follow` drift) can pass the pad ~100 m to the side and clip a building, so the
  end-to-end test places the ship at F650's seat for the touch. F596/F597's exact
  distance law and the rotating frame's velocity are the next step
  (`docs/notes/autoland.md`).
- **Landed dressing.** F253's undercarriage word (`+0x9e`) animation, the sounds and the
  player's AI mode byte (`player+0xff = 0x24`) are not modelled; the console is the flight one
  F1116 selects, as the original's is.
- Planet/station painter order has no depth test: a starport on the far side of its planet is
  drawn over it. A sphere line-of-sight test is the cheap fix; the original's queue is global.

## The original's navigation computer (read from the code, not yet a port)

From the executed binary (`ffelnx.asm`, the key handler at `JUMP_004383..004405` and the
list builder before `JUMP_004516`):

- The `n` icon toggles the list (`DATA_007250`) when the ship has the navigation-computer
  equipment bit (`+0xc8` bit `0x20`). The list walks the live objects from slot `0x72`
  down and takes those whose flag byte has bit `0x10`; each gets a 10x10 marker button
  (hot area `0xd1 + n`, bitmap `0x31`) and its name. Nine rows of 15 pixels from y 5,
  then a second column from x `0xe6`: at most 18 (`0xd1..0xe2`).
- A click on row k finds the k-th such object, stores its slot in the player's target
  byte (`+0xec`), clears the target-select mode (`DATA_007246`) and, when the player's
  mode (`+0xe8`) is neither 2 nor 0x28 or more, calls `F1020`: it sets the target
  object's AI state byte (`+0xff`) to 8, the player's mode to 2 (autopilot), plays the
  click and redraws the console buttons (`F1116`). The list closes. So one click both
  locks on and engages the autopilot.
- The `t` icon needs the target-computer bit (`+0xc8` bit `0x80`) and cycles the target
  select modes (`F1116`/`F1120`) over the ships around, which needs scanner contacts.
- The autopilot's steering runs in the AI state tables (`DATA_006255/6`, 16 bytes a
  state: set-up, step, and two more routines; state 8's set-up is `F741`, its step
  `F727`) with the turn rates `+0x110/+0x112/+0x114` that `F585` adds when its
  autopilot flag is set (docs/notes/physics.md). Those routines are not ported.

What ffe-rs does: the list has the original's layout (two columns of nine, marker
squares), page by page (Left/Right) because the systems have far more bodies than 18;
a click locks on and engages the autopilot, and the lock shows under the top bar.
The autopilot controller is ffe-rs's own implementation of the F742/F610/F597 law (a
direct turn toward the target, set speed from the braking curve) with the port's long-leg
transfer; the F741/F745 approach distances and the F746 final leg are the source's. It
is tested on every body of every system after a jump (`autopilot_after_every_jump`), at
every orbital station from three bearings (`autopilot_docks_at_stations_from_every_bearing`)
and with the F1018 cancel (`cancelling_the_autopilot_stops_the_ship_and_clears_the_leg`).
