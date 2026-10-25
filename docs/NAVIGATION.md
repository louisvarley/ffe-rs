# Getting around: systems, maps and the autopilot (progress)

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

The default boot is now the original's new game: Ross 154, the Eagle (model 23),
1000 CR, 1 Jan 3250 12:00, landed on the starport pad (`navigation::new_game`,
`docs/notes/intro.md`). `L` takes off from the open-air pad through the
original's F1013 `0x24` -> F1031 `0x26` -> `0x00` path (EXP-010). The
development sandbox (`--start sol`)
starts docked at a station **in a planet's orbit** - the closest such station,
else the first station - so the launch puts that world in view (Sol: Mars High,
1.61 Mars radii). That is a starting-scenario choice, not the original's fixed
start.
| Stations in the flight view and docking | done (2026-10-24), ffe-rs rules: see "Docking" below |
| Open-air starport takeoff | done (2026-10-25): F1013 `0x24`, F1031 `0x26`, F1032/F651 bay release (`docs/notes/intro.md`) |
| Galaxy beyond the 12 hand-coded systems | not done: the other table systems need the sector star generator (F832-F856, tables `DATA_006799..6808`) and their name strings |

## The autopilot

`autopilot(flight, target, stop, memory) -> Command` each frame. It turns the nose with
the pitch and yaw keys (the keys build a turn rate and their amounts grow the longer they
are held, so they are pulsed and released), then raises or lowers the set speed with
Return/Shift to follow a braking envelope `sqrt(2 * 0.4 * 90 * distance)` (the reverse
thrust slows the ship by about 90 m/s per second; forward thrust gives about 300).

Legs longer than 1.5 million km (`APPROACH` 1.5e9 mm beyond the standoff) cannot be flown
in real time with the nose staying true (time acceleration multiplies every turn-rate
error: at 1000x a hair of drift swings the ship round). Once the nose is on the target the
command carries `skip = Some((landing, world_seconds))`: a transfer the caller carries out
by placing the ship at the approach point at rest and advancing the game clock by
`transfer_seconds` (full thrust up to the speed the braking can stop, then the braking).
The last 1.5 million km are flown for real. Tests: Sol's Pluto (39 AU: a 412,844 s
transfer then about 300 s of flight), Mercury and Daedalus.

## Keys (the playable)

The game starts docked at the first station of Sol (L launches, which puts the ship out from that station on the original flight model). Docked, the screen is the original's station services root (`docs/notes/station.md`): Launch Request, Shipyard, Bulletin Board and Stockmarket, with the original's title, rows, buttons, hot areas and `1`-`4` keys. Up/Down choose a row, Enter or a click on the row's button activates it, Backspace goes back, and the Stockmarket opens the verified market (B/N still buy and sell the selected line). Wired in `main.rs` (not the original's map screens):

- The console's map icon (or G): the **galaxy map**, the original's 3D scene (`docs/notes/galaxy-map.md`): the stars around the
  centre with their names, the grid, the range sphere of the drive and a marker on the star nearest the centre, whose name,
  distance and description show in the console and whose jump cost shows under the sector. Arrows scroll, Page Up/Down or
  +/- zoom (Shift slower), dragging turns the view, C returns to your system, H or Enter jumps to the selected star if it is
  one of the systems on offer, G closes. The jump is the hyperdrive's as below; the list of systems is gone from the playable.
- The console's navigation-computer icon (or N): the original's list of the places in this system — its
  stations and starports, two columns of nine with the original marker bitmap and names. A click on a marker row
  (only the marker is a hot area, as in F1099) locks that body and engages the autopilot. Left/Right (or Page
  Up/Down) page it if a generated system has more than 18 places. Tab remains the ffe-rs system map of every body:
  a click on a row (or Enter) engages the autopilot to it and closes the list. Backspace turns the autopilot off.
- The labels icon or L toggles the original's text labels: the names of the system's bodies over the flight view
  (colour 15, at the object's screen position). The console icon lights while they are on.
- The top bar's button changes between modern and classic controls (modern by default; F3 too). Classic is the original's keys: Enter/Shift set speed, Z/A pitch, comma/period yaw, [ ] roll. Modern adds W/S (set speed), Up/Down (pitch), Left/Right (yaw) and Q/E (roll).
- **Target and autopilot buttons** (below the view, beside SYSTEMS and THIS SYSTEM): a click on a planet, moon or station in the
  view locks it (green corner brackets and a `TARGET:` line with its range); **TARGET** (or T, Shift+T the previous; the
  console's `t` icon too) locks the next body by range, nearest first. **AUTOPILOT** (or K) flies to the locked body: at a
  station it flies to the approach point on the docking axis, asks for clearance and docks by itself; **STOP** (or
  Backspace) turns it off. The brackets turn yellow and the line reads `AUTOPILOT:` while it is flying. The autopilot starts
  the original flight model if it was not running (as O does). Open-air starports are flown to but not landed on yet.
- D, flying in a system, asks the nearest station within 100 km for docking clearance (again cancels it). Fly into the slot on the station's docking face; or engage the autopilot on a station, which docks by itself.
- F12 is the model viewer, Shift+F12 the model lab. The other console buttons and the playtest scenarios are not in this build.

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

**The close-range fix (2026-10-24).** Two things used to make a planet disappear on
approach. The flight view dropped every disc with `z <= radius` (the centre nearer than
one radius along the view axis), which is exactly the geometry of a landing approach;
and the surface was scaled so its base sphere had the *outer* radius `+0x138` (the
atmosphere's), while F879 places starports on the terrain at the *inner* radius
`+0x140` (the solid body; Earth is 4,970 km against 7,810 km). At starport altitude the
eye was therefore inside the drawn sphere and every terrain triangle was a backface.
The filter is gone and `main.rs` now uses the inner radius for the surface; the course
from 3 radii down to the ground stays drawn (regression:
`planet_is_drawn_at_and_inside_one_radius`, frames `surface-k1.00.ppm` ... in
`FFE_SHOT_DIR`). The outer radius is still the collision/atmosphere radius.

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
- **Autopilot.** A station target is flown in two legs: to a point 20 km out on the docking
  axis (stopping within 1 km), then, with clearance taken there, toward the centre braking
  for a stop just inside the face, which the contact turns into docking. Sol's Daedalus,
  Abraham Lincoln and Galileo dock from the start (`tests/navigation.rs`,
  `autopilot_docks_at_stations`; `FFE_SHOT_DIR` saves frames on the way in).
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
  settles the ship on the surface; a faster one sets 0x80 and bounces it (damage from F953's
  amount is shown on the status line). Regression: `soft_touch_lands_and_fast_touch_bounces`.
- **The landed state (2026-10-25).** The DOS default new game is record 0 of F829's
  start table: Gateway [-1,4] on the planet **Hope**, at the starport **Old Blackelk**
  (`slot 0x6a`), with F998's default Saker Mk III, 10,000 CR and state `0x30`. (Record 1
  is Ross 154's Eagle, `slot 0x6e`; `--start 1` keeps it.) The boot's state `0x30` is
  F1050/F1054's landed surface view -- the flight view from F994's pad seat with the
  ground, horizon, starport, scanner, gunsight, console and message log -- not the docked
  services board. The HUD is F1099's: Set/Act at (2,148) in F1393's km/h conversion
  (`*0x156/100` one decimal below `0x4705d`, `/0x69` two decimals above), Alt at
  (266,139) and `DATA_009124`'s parent name (Hope) beneath it. `L` starts F381's F654 and
  F1013's climb.
  A soft touch on a planet or moon takes the original's states: F1021 sets `0x32` for the
  frame of the touch (F1042 zeroes the speed, rates, thrust and set speed), and F1047's
  view frame makes it `0x34`, the guarded landed-rough state (`Navigator::land`,
  `landing_frame`, `ffe_core::traffic::Landing`). The landed screen is F1050/F1054: the
  ordinary flight view from the settled ship, the console and message log unchanged, and
  the world still turns around it. F1012 runs no flight step while landed (no
  F1037/F1041/F601/F603/F1512), so the ship is held still; `nav.follow` still carries it
  with its planet. `L` is F1056's launch action: F1013 starts at `0x24`, the fixed up
  thrust climbs, F1013 hands over to `0x26` and F1031 clears it to `0x00` (the same
  `Takeoff` machine as the open-air pad, without the pad world). The open-air starport's
  parallel `0x2e` -> `0x30` pair is in the machine too (`Landing::at_starport`). Tests:
  `takeoff.rs` pins the transitions;
  `planet_view.rs::landing_reaches_the_surface_state_and_launches_again` runs the whole
  cached sequence with frames; `planet_view.rs::new_game_hope_landed_surface_view` pins
  the default start, its Hope HUD strings and the landed frame;
  `tools/reference/verify-surface.py` compares the type-1 new game's landed-at-starport
  view with the executed original's (`--game-frame 500 0`).
- **Services.** The docked services board remains the original's state `0x2a`, opened by a
  completed docking (`Traffic::Docked`) and by the docked screen's own navigation; the
  new game's `0x30` no longer opens it at boot (EXP-014).
- **The landed background.** F1052 clears the upper view to the previous scene's
  `DATA_008814..8817` colour and draws F1775's stars only while the flag is clear. At a
  starport or in a rough landing above the haze surface the flag stays 0, so the sky is the
  original's space colour; F1877's atmosphere colour is carried into the next frame when the
  flag is set (`ModelViewer::haze_sky`, `Screen::surface_sky`).
- **Not the original yet.** The pad hit when docking at an orbital station is this port's slot
  test, not the station's collision stream; the sounds and the ship's motion with the rotating
  station during the stages are not ported. The starport/planet scan does not yet draw the ship
  in the rotating frame between ticks (it is flattened to the parent's linear frame each frame),
  and starports on the far side of their planet are not hidden by it (there is no depth buffer
  between the planet surface and the station models).

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Docking entry speeds through the autopilot are 110-170 m/s: the set speed steps down at the
  keys' fixed rate, so it lags the braking curve over the last few hundred metres.
- Planet surfaces draw their triangles, but the textured fills are flat colours or
  blends (see above), so the detail is blockier than the original's; the night side is
  drawn with the shade table and has no atmospheric glow except the ported haze ring.
- **Terrain collision fidelity.** F581's terrain record tests the ship against the model's
  surface at the body's `+0x138` (outer) radius, while `Screen::draw_discs` scales the drawn
  surface to `+0x140` (inner, `docs/notes/terrain.md`). A rough landing away from a starport
  therefore settles the ship at the collision sphere while the drawn terrain's base sphere is
  up to ~1.4 times smaller: the ground under the eye can be thousands of kilometres below and
  the view can show the space colour where the original shows the surface. F879's starports sit
  on the high ground, which is why their frames (and the reference comparison) show the terrain
  and buildings. Fixing this needs one radius for both (and a check that the starport does not
  fall inside the drawn sphere), an owner decision.
- **Starport landings from flight.** The machine has F1019/F1021's open-air `0x2e` -> `0x30`
  pair, but the playable still gives a starport pad touch the in-flight contact (no `0x20`
  effect-region flag is modelled) instead of the `0x30` services state. The new game's `0x30`
  and its F381 launch are the working path (`docs/notes/intro.md`).
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
The autopilot controller itself is ffe-rs's own (a direct turn toward the target, set
speed from the braking distance, time transfers for the long legs), tested on every
system after a jump (50 of 52 bodies arrive).
