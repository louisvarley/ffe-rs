# Physics (cphys): F601, F602, F585 and its helpers

Ported in `crates/ffe-core/src/physics.rs`; verified by
`tools/reference/verify-physics.py` (F601/F602 v0.49b, orientation v0.49c). More
of `cphys.txt` follows (positions, frames of reference, gravity).

## Object fields used so far (`PhysObj`)

| Offset | Meaning |
|---|---|
| `+0x00..+0x23` | orientation: rows right (`+0x0`), up (`+0xc`), forward (`+0x18`); Q31 dwords, only the high 16 bits of each are used here |
| `+0x8c/+0x90/+0x94` | velocity (world units) |
| `+0xb6/+0xb8/+0xba` | thrust words along right, up, forward (the result of F598-F600) |
| `+0xec/+0xee/+0xf0` | sub-unit velocity accumulators (16-bit) |
| `+0xf2/+0xf6/+0xfa` | velocity-change caps (unaligned dwords) |

## F601 and F602

Both add the thrust over a time step `tics` (50 Hz ticks of 1/50 s scaled by the
caller) to the velocity:

1. `time = tics >> 3`; while `time > 0x7fff` it is halved and a shift count
   (starting at 0x10 in F601, 0xf in F602) decremented, ending at -1 when no
   halving was needed. F601 shifts `tics` logically, F602 arithmetically.
2. Each thrust word that is non-zero adds `((coordinate >> 16) * time >> 15) *
   word` for the three coordinates of its orientation row (right, up, forward);
   the sum is doubled.
3. Shift count negative (small and medium steps): the sum is added to the
   16-bit accumulators and the carry (`>> 16`) added to the velocity. F601 caps
   that carry (a cap >= 0 limits it to `0..=cap`, a negative one to `cap..=0`)
   when `time >= 0x4014` (a third of a second); F602 never caps.
4. Otherwise (large steps) the sum is shifted right by the count, capped (F601
   only) and added to the velocity; the accumulators are untouched.

`--accelerate` runs the original F601 and F602 on 4096 synthetic objects (random
orientations, thrust words including zero, accumulators, velocities, positive,
negative and zero caps) over time steps in all three regimes: 4096 compared, 0
mismatched, every tampered field and an empty trace rejected.

## Orientation: F585, F584, F611, F1520

More fields: `+0x24` the drift counter (signed byte), `+0xb0/+0xb2/+0xb4` the
angular rates (pitch, roll, yaw; words), `+0x82` the dword read as the model
number, `+0x86` the craft byte, `+0x110/+0x112/+0x114` the autopilot's turn
rates. Globals: `DATA_008857` the player's craft number, `DATA_008933` the
scanner rotation word.

- **F585** (object, time, autopilot flag, autopilot time): the time is
  arithmetically halved for model dwords from 0x35 (again from 0x3b). With the
  autopilot flag each rate gains `(autopilot rate * min(ai time, 0x7fff)) >> 16`
  (16-bit add). The time is then capped at 0x7fff. Pitch, roll and yaw in turn:
  `wanted = min(|rate|, time)`; below 0xf nothing happens; otherwise F584 names
  the step and the rate moves toward zero by it (`rate -= size` for a positive
  rate, `+=` for a negative one) and F611 turns the row pair (pitch: forward
  toward up, roll: up toward right, yaw: right toward forward; a negative rate
  turns the other way). Yaw also adds or subtracts the step from the scanner word
  when the object's craft byte equals the player's.
- **F584**: the largest of 0x9fb, 0x511, 0x28b, 0x146, 0xa3, 0x52, 0x29 not above
  the value, with codes 2..8; below 0x29 the size is 0x14 and the code 9. The
  code is the shift `n` of the rotation (a step of 2^-n).
- **F611** (object, row a, row b, direction, n): for `n <= 5` both rows first
  lose `v >> (2n + 1)` per coordinate; then `b' = b + (a >> n)` (or `b - (a >> n)`
  for the other direction) and `a' = a - (b >> n)` (or `+`), the old shrunk `b`
  used for `a'`. The drift counter falls by the byte `[0x12, 3, 3, 3, 5, 2, 1, 1,
  1, 1][n - 2]`; when it goes negative F1520 runs and the counter becomes 0x3c.
- **F1520** re-orthogonalises: right and forward normalised (F1519), up =
  forward x right normalised, right = up x forward (F1552, F1521 products).
  **F1519** scales a vector by the reciprocal of its length (F1466, then
  `F1523(0x3f800000, length << k >> 1) >> 1` less one, `F1521`, shifted back up
  by `k + 1`).

`--orient` runs the original F585 on 4096 synthetic objects (near-identity and
random orientations, drift counters around zero, rates from zero to full range,
every model band, autopilot on and off, time steps from negative to huge): 4096
compared, 447 of them through F1520, 0 mismatched; every tampered field and an
empty trace are rejected.

## Position: F1512 / F1511

The position (`+0x3e`, `+0x46`, `+0x4e`) is three 64-bit signed coordinates; the
parent byte `+0x56` follows them. F1045 calls `F1512(obj, DATA_008869,
DATA_008868)`, the 64-bit time as its high and low dwords, and F1512 passes the
velocity (`+0x8c`) to F1511:

1. A negative time is negated (two's complement over the 64 bits) and the sign
   applied to every delta at the end.
2. The magnitude is shifted right seven; if its high dword is non-zero the whole
   value is shifted right by `msb(high) + 1` into the low dword (that count is
   remembered); then the low dword is shifted right until its MSB is at most 14
   (the extra count is added).
3. Each velocity coordinate times that low value, as a signed 64-bit product,
   arithmetically shifted right two, then shifted left by the remembered count
   in the original's two 32-bit halves (so a count of 32 or more wraps), and
   negated if the time was negative, is added to the coordinate (wrapping).

`--advance` runs the original on 4096 synthetic positions and velocities over
small, large, huge and negative times: 4096 compared, 0 mismatched.
