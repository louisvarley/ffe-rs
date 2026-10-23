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
