# Collision distance, frame switching and contact (F569, F570, F572-F578)

`crates/ffe-core/src/collision.rs`. F569 (F568 differs only in the planet crash
mask `DATA_008700`) calls F570, which F1033 uses to set the altitude
(`DATA_008867`: the distance, or 0x7fffffff when none). Flags (`DATA_008696`):
1 different radii (within an atmosphere), 2 distance valid, 0x40 landing, 0x80
crash.

- **F570(player)**: clears the flags and sets the low dword of the distance
  register (`DATA_008703`) to -1; the high dword (`DATA_008704`) is **not**
  reset, so a stale value from an earlier scan can turn a valid distance into
  -1 (ported with the register as state). With a parent: F575 against it with
  the player's own position; then, if the switch delay (`+0x148`) is non-zero,
  it counts down by half the frame time (rounded toward zero) and nothing
  switches; otherwise a result with the distance-valid flag puts a linear player
  into the rotating frame (F572) and the reverse (F573), each setting the delay
  to 0x7f. Then every physical object (list byte bit 2), from index 0x72 down to
  1, other than the player's own index (`+0x86`), not hidden (`+0x8b`), with the
  player's parent, and not a model-0x99 object unless the player is one, goes
  through F575 with the relative position (the difference of positions, or of
  the F1677 absolute positions when the rotating bytes differ).
- **F575(relative, a, b)**: out of reach (returns 0) when
  `(|x|+|y|+|z|) >> 1` is at least `a.outer + b.outer` (`+0x138`). A complex
  shape (`+0x14c` negative) goes to F576. Otherwise F577.
- **F577**: the 64-bit coordinates are cut to 32 bits: if any high dword (or
  `b`'s outer radius' high dword) is set, shift right by `msb + 3`, else left by
  `0x1d - msb` of the low dwords (and `b`'s outer low). The near branch compares
  `(a.inner + b.inner) << k` (`+0x140`, the JJFFE collision radius), the far
  branch the shifted sum of the **outer** radii. The sum of F1522 squares (the
  high dword of the signed square of each coordinate) is compared, unsigned,
  with F1522 of the radius; radius larger is a contact. Otherwise the distance
  is `F1502(sum >> 1) * 2` (zero if negative) shifted back by the same amount
  into the 64-bit register; flag 1 is added if `b.inner != b.outer`
  low dwords differ (`+0x140` against `+0x138`), flag 2 always. A contact
  returns 0 with the squares left in `[edi]` and **no** distance.
- **F572 / F573** (linear to rotating and back): position cut to 28 bits
  (F1658), turned by the parent's orientation (F1670 rows, or F1669 columns
  back), shifted up (F1517); orientation by F1673 (`a * bT`) or F1672 (`a * b`);
  velocity likewise; F572 sets `+0x58` to the frame stamp less one. F1677 adds a
  rotating body's position turned by the parent's orientation (F1669).

## The contact tail (F575 `JUMP_002410`) and F578

When F577 reports a contact, F575 checks whether the other body is the body's
own parent (`+0x86` against `+0x56`). If not, F578 is called; if it is, or when
F576's shape test finds the body inside the planet (`DATA_008707` non-zero), the
tail runs:

- flag 2 is set and `speed = |vx|+|vy|+|vz|`; the surface's up is
  `F1503(truncate(relpos, 0x1c), 0x7fffffff)` for the terrain, or
  `F1519(relpos low dwords)` for the sphere; `dot = up . row1` (F1521).
- A **soft landing** needs all three: `speed <= 0x258`, the word at `+0x9e`
  equal to `0xffff`, and `dot >= 0x78000000`. Then flag 0x40 is set, 0x80 and
  0x7f are cleared (`DATA_008699 = 0`) and the body is settled: its position is
  put on the surface (terrain: `+0x140` less the altitude along up; sphere: the
  planet's radius plus the body's collision radius, scaled to F577's squares
  along the relative position), row 1 becomes up, rows are swapped (F571),
  re-orthogonalised (F1520), swapped back, and the velocity is cleared.
- Otherwise `DATA_008699 = DATA_008700` (the crash mask: 0xff for F569, zero for
  F574/F568) and flag 0x80 is set. The player's body (`+0x86` against
  `DATA_008857`) bounces: the second row becomes up (F571, F1520, F571), the velocity is
  reflected as `v[k] = 2 v[k] / 3 - 10 (up[k] . v) / 12` with F1521, flipped to
  keep the saved row 2's forward sense (or set to it if it cancels), normalised
  and made row 1 before F1520 runs again. The damage handed to F953 is the
  model's signed mass word (`statistics + 6`) times four, times the speed lost,
  over `0x708`, but at least the mass over four. Any other ship with
  `+0x87` 0x0b or 0x0e (missiles, escape capsules) is turned to the surface, its
  velocity cleared and F953 given -1.
- Finally flag 1 is set when the planet's `+0x140` differs from `+0x138`, and
  `DATA_008698` records the planet's index when it is still clear.
- The position placement, the up vector and the bounce only need the object's
  bytes; the damage and the model details come from F1538's record. In the port
  these come from `Shapes::model` (`ModelInfo`), so a scan without the model
  records declines a touch instead of guessing.

**F578** (touching something that is not the parent): flag 0x80 and, when both
bodies are ships (model above 0xd), their statistics exist and the crash mask is
set, a momentum exchange: the bodies are ordered by their unsigned mass words,
the direction from the heavier to the lighter is normalised (F1658 to 0x1d bits
and F1519), the closing speed along it is the F1521 dot of the velocity
difference, and the lighter body gains `2 * closing` along the direction; F953
is given `lighter mass * 2 * closing / 5000` for both. A body hitting its own
parent or a station (`+0x87` 1) instead has its velocity quartered and reversed,
with `|v| + 0xa` damage for a ship. This is the JJFFE block with the local
`FIX: Missile retardation` comment; the port mirrors it exactly.

`DATA_008705` (the altitude F581 leaves) is **not** reset by F570. A shape pair
that finds the body inside the planet's inner radius (`DATA_008707 = 1`) leaves
the altitude at whatever the previous terrain record wrote, so the scan carries
it as world state (`Bodies::altitude`), exactly like the stale `DATA_008704`.

`--collide` runs the original on 4096 seven-body trees with the player last
(random radii, rotating bytes, orientations, stale `DATA_008704`, positions
aimed at 0.4 to 2.5 times the radius sum so many pairs touch): **all 4,096
cases match**, including the 950 contacts (soft landings, crashes, F578's
exchanges and frame switches) in the return value, flags, distance, both
distance registers, object, pad and every body. `verify-collide.py` rejects ten
altered fields and an empty trace; trace SHA-256
`29144e66671d1f214e6fa3f7fe47b5c62710ee3a3e8431fc2a3ae0ff4ec5c57d`.

## The player's tick (F1021) with the scan

`Player::tick_in` runs `step_in` (F1041, F601/F602, F1512) and then the scan
(F1033 calls F569): the player is copied into its body (`Body::take_motion`),
`Bodies::collide` runs, and the body is copied back (`give_motion`), which carries
a frame switch's new position, velocity, orientation, rotating byte, delay and
update stamp. The altitude is the distance or 0x7fffffff.

A body that is its own parent (possible only if the player's own list flag marks
it live to F606) aliases the matrix in F572/F573: the orientation is updated in
place, so the velocity is turned by the **new** orientation. The port reproduces
this.

`--world-tick` runs the original on 4096 whole ticks in a seven-body world (ffp
masses, radii, rotating bytes, orientations, ship models for the player, a stale
`DATA_008704`): **all 4,096 match** in every player field, the altitude
registers, flags and every body (207 with a distance, 1,312 frame switches, 246
parent changes), including the 584 contacts. `DATA_008857` (`Bodies::player`) is
the case's `player` field, so the bounce only runs where the original's craft
numbers agree. `verify-world-tick.py` rejects ten altered fields and an empty
trace; trace SHA-256
`4258a0b2756c09c53c102a805a365cce9e6b7c51770894d801875603fa43d6ac`.


## Planets in the scan

`Bodies::collide_with(a, Some(&SurfaceTools), crash)` runs the scan with complex
shapes: bodies carry their terrain seed (`+0xa0`) and animation words (`+0x9c`).
Half of the `--collide` and `--world-tick` cases make the player's parent a
planet (one of models 113 to 148, radii from the model's extent, a rotation, a
seed): 1,765 of the 4,096 collide cases and a similar share of the ticks are
around a planet, matching in flags, object index, pad, distance registers and
every body. A hit on the planet now runs F575's tail too (the terrain's altitude
and up vector, or a crash and bounce).
