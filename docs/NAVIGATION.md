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

The playable starts docked at a station **in a planet's orbit** — the closest
such station, else the first station — so the launch puts that world in view
(Sol: Mars High, 1.61 Mars radii). This is a starting-scenario choice, not the
original's fixed start.
| Stations in the flight view and docking | done (2026-10-24), ffe-rs rules: see "Docking" below |
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

The game starts docked at the first station of Sol (L launches, which puts the ship out from that station on the original flight model). Wired in `main.rs` (not the original's map screens):

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
frames and two minutes of wall time for Sol/Mars). `--capture DIR` remains the
scripted launch.

## Docking (ffe-rs rules; EXP-005 in docs/DEVIATIONS.md)

`ffe_rs::docking::Port` and the navigator's `request_clearance`, `contact` and `dock`.

- **Stations in the view.** `Screen::stations` (set each frame in `main.rs`): every station of
  the system drawn from its own model (72 trading post, 73 orbital station, 80 orbital city,
  ...) at its generated place, turned by F866 at its own speed byte (`Body::spin`, `+0xc8`)
  over the navigator's clock, lit from the star. Beyond 2,000 km from the eye a station is
  not drawn (under a pixel; the viewer's view space is 32-bit).
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
- **Not the original yet.** The pad hit is this port's slot test, not the station's collision stream
  (F569 flags 0x20 | 0x40, with F575's crash and bounce, which `ffe_core::collision`/`landing` now port and verify but
  the playable does not call); the sounds and the ship's motion with the rotating station
  during the stages are not ported. Starports
  (landing on a planet) are not dockable here.

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Docking entry speeds through the autopilot are 110-170 m/s: the set speed steps down at the
  keys' fixed rate, so it lags the braking curve over the last few hundred metres.
- Planet surfaces draw their triangles, but the textured fills are flat colours or
  blends (see above), so the detail is blockier than the original's; the night side is
  drawn with the shade table and has no atmospheric glow except the ported haze ring.

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
