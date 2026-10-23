# The player's free-flight update (F1041 and friends)

Ported in `crates/ffe-core/src/player.rs` (with `physics::Motion::set_speed_request`);
verified by `tools/reference/verify-physics.py` (`--player-orient`, v0.49d).

## F1041, in free space

`Player::orient_and_thrust` is F1041 for a craft with no parent frame of
reference (`+0x56` zero) and far from any planet (`DATA_008867` above
0x2625a00). Those two limits are what make three steps vanish: F603 (gravity)
returns at once without a parent, F606 (frame-of-reference update) finds
nothing to do in an empty object list, and F609 (the auto-roll toward a
planet) only runs at or below that altitude. The port returns an error in
either situation instead of guessing.

1. F585 twice, each over `DATA_009155 >> 1` (the frame's time, logical shift),
   autopilot off.
2. The frame countdown `+0x14d` is decremented; below zero F606 would run and the
   counter becomes 0xd.
3. The thrust request: take-off (`DATA_008870 == 0x24`) is (0, 0xcc6, 0);
   engines off (`DATA_008871` bit 0) is F1043; otherwise F607 for the set speed
   (`DATA_008918`). F599 clamps it to the limits into `+0xb6/+0xb8/+0xba`.

## F607 (set speed), F1515, F1516

`target = speed + speed / 100` (truncating). Each coordinate of the forward row
(`+0x18..+0x20`, high 16 bits) times `|target|` through F1516, negated for a
negative target, less the velocity, is the velocity change. It is stored as the
F601 caps (`+0xf2/+0xf6/+0xfa`), carried through the orientation (F1670, the
verified `transform_rows`) and shifted left two to give the request. F1515 is
`((a & 0xffff) * b >> 16) + (a >> 16) * b` (a taken unsigned); F1516 doubles it
and applies b's sign.

## F1043 (engines-off thrust)

In combat mode (`DATA_009046` bit 1) the velocity relative to the reference
(`DATA_008839..41`) is projected on the right and up rows (F1521 products); a
projection beyond +-256 is opposed (negated, times four) on that axis. Each
pressed key then sets its axis to +-0x7fff, later keys over earlier ones, in
this order: forward `DATA_007692` (+), reverse `DATA_007694` (-), key-table
offsets 0x48 (+) and 0x4c (-) on z; 0x4b and 0x4d (+), 0x4f and 0x51 (-) on y;
0x49 (+) and 0x47 (-) on x. The key bytes are zero while pressed. The original
also calls `SoundModifyStationNoise` here; that is not ported.

## Oracle

`--player-orient` runs the original F1041 on a synthetic player (near-identity
and random orientations, limits, velocities, set speeds, take-off, engines on
and off, combat mode, random pressed keys; empty object list, large altitude):
4096 compared, 0 mismatched. The orientation, drift counter, rates, thrust
words, caps, countdown and scanner word are compared; every tampered field and
an empty trace are rejected.

## Not yet ported

F603 (gravity), F606/F604/F605 (frames of reference), F609 (auto-roll), F1037/
F1038 (the set-speed and heading input), F1046 (position, smoke, engine noise)
and F1033 (collision and altitude) are the rest of the free-flight chain.

## One whole tick: `Player::step`

F1021 (free flight) runs F1037, F1041, then F602 (engines off) or F601 for the
velocity over the low dword of the time (`DATA_008868`) and, through F1046 and
F1045, F1512 for the position over the 64-bit time (`DATA_008869:DATA_008868`).
`Player::step(keys, time)` is F1041 + that velocity update + `Motion::advance`.
The `--flight-step` mode runs the original chain on the player cases plus a
64-bit time (small to above 2^40) and a starting position: 4096 compared, 0
mismatched, over the orientation, drift counter, rates, thrust words, caps,
frame countdown, scanner word, velocity and 64-bit position.

Still outside it: the set-speed and heading input (F1037/F1038), F1046's smoke
and engine noise, the collision and altitude test (F1033), and frames of
reference, gravity and the planet auto-roll.

## The input layer: F1037, F1038, F1121

Ported as `Player::update_set_speed`, `Player::steer` and `Player::set_speed_step`
(`--steer`, v0.49g). The JJFFE build differs from the older `cplayer.txt`
notes: steering is key driven through `FUNC_CalcKeyInput` as well as the mouse.

- **CalcKeyInput(scancode)**: nothing when key input is off (`DATA_007706`
  zero) or the key's accumulator (`DATA_KeyAccumTable`, in 50 Hz ticks) is
  zero; otherwise `input time (DATA_009122) * (min(held, 70) + 2) >> 6`.
- **F1121 (set-speed step)**: the speed doubled up to five times while its top
  bit is clear (0x7ffeffff if it gets there), `(v >> 16) + 1`, scaled by
  `DATA_007690 / 16` when that dword's low byte exceeds 0x10 (capped at 0x7fff),
  times the input time, `>> 6`.
- **F1037**: nothing unless the ship module is active (`DATA_008630` negative).
  With the engines on (`DATA_008871` bit 0 clear): Return held raises the set
  speed (`DATA_008918`) by F1121 (to 0x7fffffff on overflow), but a negative set
  speed is only reset to zero and then just the heading is updated; Shift held
  lowers it: a positive speed by F1121's step (flag 0x80 set, floored at zero), a
  non-positive one by `input time >> 8` per tick unless flag 0x80 is already set
  (and it cannot pass 0x80000000); Shift released clears flag 0x80. Then F1038.
- **F1038 (heading)**: the mouse accumulator (`DATA_009188`, x low word, y high
  word) minus the previous one (kept at `+0xd0`), each difference shifted left 6
  as a 16-bit value, negated when the option byte (`+0x3f54` x, `+0x3f55` y) is
  zero. With the mouse-roll byte (`+0x3f56`) set outside the turret views the x
  delta becomes the roll, **with its low byte replaced by the view mode
  (`+0xea`)**: the original's `mov al,[+0xea]` clobbers it, kept as it is. Held
  keys add or subtract: 0x2c/0x1e to pitch, 0x33/0x34 to yaw, 0x1a/0x1b to roll.
  Outside the turret views the deltas are added to the rates with 16-bit
  saturation (pitch to `+0xb0`, roll to `+0xb2`, the negated yaw to `+0xb4`); in
  a turret view they turn the turret (`+0x204` yaw, `+0x206` pitch) and select
  view 2 (pitch negative, clamped at -0x3fff) or, with 4 or more gun mounts, 3
  (clamped at 0x3fff); with fewer the pitch is held at -1.

`--steer` runs the original F1037 on the player struct with synthetic keys,
accumulators, mouse words and every view: 4096 compared, 0 mismatched (the first
run exposed the roll low-byte quirk).
