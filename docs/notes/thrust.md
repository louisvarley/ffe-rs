# Thrust limits (F598, F599, F600)

Ported in `crates/ffe-core/src/thrust.rs`; verified by
`tools/reference/verify-thrust.py` (v0.49a).

An object keeps a maximum and a minimum word per axis: x `+0xbc`/`+0xbe`,
y `+0xc0`/`+0xc2`, z `+0xc4`/`+0xc6`. Object set-up (the routine at 0x125c0)
fills them from the ship's statistics block: z maximum = stats `+0x0` (forward
thrust), z minimum and x/y minimum = stats `+0x2` (reverse thrust, negative),
x/y maximum = its negation. The routines store their result in the words
`+0xb6` (x), `+0xb8` (y) and `+0xba` (z); `+0xba` is also what the thruster
glows read as model global 15.

| Routine | Behaviour |
|---|---|
| F598 | x and y become -32767 (request < 0) or +32767; z becomes `request << 3`; then each axis is capped at its maximum and then raised to its minimum |
| F599 | the request, each axis capped at its maximum and then raised to its minimum |
| F600 | one scale for the vector: starts at 0x7fffffff; each axis in order whose request is above its maximum sets it to `F1523(max, request)`, below its minimum to `F1523(-min, -request)` (a later axis replaces an earlier one, so the last out-of-range axis wins, not the smallest); each component is then `F1521(scale, request)` |

F598 is called from three places (49743, 49758, 49951), F600 from 48591;
F599 has no caller in this build.

## Oracle

`--thrust-limits` runs the original F598, F599 and F600 on a zeroed object with
4096 synthetic limit words and requests (in range, beyond a limit, negative,
zero, at the limits). The returned dwords and the stored words match: 4096
compared, 0 mismatched; every tampered field and an empty trace are rejected.

## Not yet wired

The in-game thrust still maps the throttle linearly onto the forward or reverse
limit (`ShipStats::thrust_word`), which is a stand-in until the player's
request vector comes from the original control code (F598's callers). The
statistics' limits are available as `ShipStats::limits()`.
