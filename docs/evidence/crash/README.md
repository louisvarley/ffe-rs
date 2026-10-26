# Surface crashes: launch, bounce, destruction (2026-10-25)

The owner's two reports: the launch appears to fly itself, and a planet impact
bounces (`Crash: damage 28. The ship bounced off the surface.`) instead of
destroying the ship. The trace below is from
`crates/ffe-rs/tests/crash.rs` (local, needs `FFE_WORLD_CACHE`):

```
FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test crash -- --ignored --nocapture
```

## Launch placement (Hope, Old Blackelk, Saker Mk III)

```
seat 288734.88 mm from the port centre, 5929748039.76 mm from the planet centre
port radii 2457600/1000000   planet radii 6709837824/4269801472
modes [0x24, 0x26]  hand-off frame 164 (altitude 200802)  clear frame 338
at the clear: 780089784 mm inside the +0x138 outer radius,
              1660242539 mm outside the +0x140 collision radius,
              set speed 311 m/s, 18.74 velocity units
```

- The seat is F994/F650's: the pad offset plus the ship's `+0x140` radius
  along the station's up row. It is **not** inside the planet's collision
  radius, so there is no placement/frame bug.
- `0x24` is F1013's fixed `(0, 0xcc6, 0)` up thrust (the source's automatic
  climb, 200,000 mm then hand-off). After the hand-off the ship has no lift:
  FFE's flight model needs the player to pitch up or raise the set speed and
  climb. A level run then meets the terrain (680 m below the seat on the
  starport's line).
- A launch scripted to pitch up clears the pad and flies without a contact.
- The first level contact after the clear (frame 298, flags `0x83`,
  damage 169) is a real terrain crash, and F953 now destroys the Saker
  (hull 112).

## Impact speeds (Saker: mass 28, hull 112, no shield generator)

The scan is F575 on Hope's terrain at the starport's surface point. 1 velocity
unit is 97 mm/s.

| Speed | Ship state | Contact | F953 | Hull |
|---|---|---|---|---|
| 300 units (29 m/s) | level | `0x43` landing | none | 112/112 |
| 300 units | 30° tilt | `0x83` crash, **damage 28** | hull | 84/112 |
| 900 units (87 m/s) | 30° tilt | `0x83` crash, damage 67 | hull | 45/112 |
| 2400 units (233 m/s) | level | `0x83` crash, damage 181 | **destroyed** | 0/112 |
| 4000 units (388 m/s) | level | `0x83` crash, damage 302 | **destroyed** | 0/112 |

Damage 28 is the owner's exact bounce: the mass floor of
`mass*4 * speed_lost / 0x708`, now taken off the hull instead of only being
logged. A lethal amount runs F942 and F1104: the death hold is 5.001 s
(`0x3cae6` tics) and ends at F917's tombstone.

Frames: `crash-death-explosion.png` (F942's model-155 explosion in F1104's
external view) and `crash-death-gameover.png` (the GAME OVER tombstone), from
`death_view` with `FFE_SHOT_DIR` (the render path the crash death reaches).
The port's invented `Crash: damage N. The ship bounced off the surface.` log
line is gone; the source prints no crash message (only F953's `0x1c`/`0x1d`
samples and the HUD words).

## Tests

- `crates/ffe-rs/src/flight.rs::tests` (4): F953's crash amount on the hull,
  shields first, a lethal amount destroys, and no damage for a landing/empty
  touch.
- `crates/ffe-rs/tests/crash.rs` (3, local): the launch placement and
  modes above; the impact table; a pitching climb still hands off.
- `cargo test --workspace --locked`, clippy and fmt: green.
