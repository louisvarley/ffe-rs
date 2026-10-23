# Physics (cphys): F601, F602

Ported in `crates/ffe-core/src/physics.rs`; verified by
`tools/reference/verify-physics.py` (v0.49b). More of `cphys.txt` follows.

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
