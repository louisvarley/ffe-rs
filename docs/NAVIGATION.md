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
The viewer's view space is 32-bit, so this only fits within about 3.4 radii of the
planet (model 129's vertices reach 4.4e8 against a limit of 1.5e9); farther out, and
for stars, the body is a shaded disc of the right size and phase.

Known gaps: triangles with a corner near or behind the eye are dropped or show as
wedges (the original's draw routines clip edges; the queue's coordinates wrap in
16 bits here), textured entries are drawn flat in their first colour, F1877's haze
is not ported, and the planet is not oriented by its object's matrix. Looks
checked by eye only (`tests/planet_view.rs`, `FFE_SHOT_DIR`).

## Known limits

- The orientation the physics integrates drifts off orthonormal (rows 0.985 long after a
  while); the autopilot orthonormalises before steering.
- Planets are not drawn in the flight view yet (the surface renderer is partly ported:
  `docs/notes/terrain.md`), so the map and the autopilot's messages are the way to see
  where bodies are.
