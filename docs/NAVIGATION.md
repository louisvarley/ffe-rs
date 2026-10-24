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

Wired in `main.rs`:

- Tab: system map (planets, moons, stations of the current system, with ranges); G: galaxy map (the systems on offer).
- Up/Down: choose; Enter: fly to the chosen body (autopilot) or jump to the chosen system.
- Backspace: autopilot off. O: the original flight model.

Planets move on the game clock (days plus a fraction), children relative to their parent. Long legs are transfers (the clock jumps and the ship lands short of the body's future position); the last stretch is flown in the target's frame (`Navigator::follow`) because the autopilot cannot catch a body moving at 30 km/s.

## Planets in the flight view

`Screen::discs` (set each frame in `main.rs` from the navigator's bodies). A body
with a model is drawn as its real surface: the planet model runs through
`ModelViewer::render_scene`, whose curved-surface command goes through the ported
F1876 and emitter (`ModelViewer::draw_surfaces`; the body's `+0xa0` is the seed).
The viewer's view space is 32-bit, so a distant planet is drawn through a model
matrix shrunk by a power of two with its position (the picture is unchanged); the
body is a shaded disc only when the model cannot be drawn, and for stars. Height
bands pick the ramp colour of the detail entries and the middle slot colour of the
textured ones (the texture fills themselves are not drawn).

The terrain's 32-bit arithmetic overflows for radii near 2^29 (the oversized slivers
at close range were that), so the matrix is also shrunk until the radius is about
2^24, the range the original runs it in. Known gaps: F1877's haze is not ported (it is about 1,200 instructions and needs F1521, F1878,
F1879, F1780 and F1479 first; the oracle already prints its inputs and the queue).
The planet is oriented by its object's rows (the spin is `clock[0] >> 17`, so they
are identity until the clock passes 2^17 days). Looks checked by eye only (`tests/planet_view.rs`, `FFE_SHOT_DIR`).

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Planets are not drawn in the flight view yet (the surface renderer is partly ported:
  `docs/notes/terrain.md`), so the map and the autopilot's messages are the way to see
  where bodies are.
