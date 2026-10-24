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

- The console's map icon (or G): the systems list. Up/Down or a click chooses; Enter, H or the console's H icon hyperspaces (an instant jump) to the chosen system.
- The console's navigation-computer icon (or Tab): everything in this system. A click on a row (or Enter) engages the autopilot to it and closes the list. Backspace turns it off.
- The top bar's button changes between modern and classic controls (modern by default; F3 too). Classic is the original's keys: Enter/Shift set speed, Z/A pitch, comma/period yaw, [ ] roll. Modern adds W/S (set speed), Up/Down (pitch), Left/Right (yaw) and Q/E (roll).
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

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Planets are not drawn in the flight view yet (the surface renderer is partly ported:
  `docs/notes/terrain.md`), so the map and the autopilot's messages are the way to see
  where bodies are.
