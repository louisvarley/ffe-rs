# Physics objects and system placement (F927, F1533, F880, F879)

`ffe_core::objects`; cache v16 (`base:model/records`, `base:gameplay/tables`).

The physics list is a flag byte per object (indexes 0 to 0x72) followed by
0x152-byte objects at `0x74 + 0x152 * index`.

- **F927** takes a free slot (flag byte zero) from 0x72 down; slots below 0x14 are
  kept back, and when none is free the topmost object flagged 0x20 at index 0x14
  or above is replaced. It sets the flag byte to the kind, runs F1533 and stores
  the model and the object's own index (`+0x82`, `+0x86`).
- **F1533** clones the template (the parent's object, or the default object
  `DATA_007134`) and, for a model, sets the clock (`+0xa8`, `+0xac`), the two
  radii (`+0x138` outer, `+0x140` collision, ffp mantissa words of the model
  shifted by `scale + scale offset + 7 - 15`), the class byte (`+0x14c`: the model's
  `+0x24 >> 4 & 7`, 0x80 for a complex collision shape), the thrust limits and the
  other statistics fields for models that have them.
- **F880** creates one physics object per generated system object (a parent's
  object is the clone template): seed, spin, depth, temperature and tilt; the
  mass in earth masses (`F1482` by 0x4082 x 2^-43, normalised) with the display
  exponent adjusted for binaries (JJFFE's fixes: +2 for a barycentre, -6 for
  its children); stations and starports get their weights, sizes, class bits and
  flag bytes; a body that is not a starport gets its parent, orbital rate
  (`0x5460 x 2^-18 / velocity`, +0x40 exponent, F1494), phase
  (`((clock-high : clock-low) * rate) << 3`, with the **low clock word in the
  high position**, as the original passes it), then F1507 places it on its orbit
  (F1506's direction from longitude and angle, times the radius, plus F1508's
  interpolation between two steps) and F1675 builds its orientation.
- **F879** places a starport at a latitude and longitude on its planet.

## What is not verified

A starport's position (`+0x3e`), orientation (`+0x00`) and velocity (`+0x8c`):
F879 measures the planet's surface with F574, which for a planet (a complex
collision shape) runs the terrain query through F576, F579, F582 and F581 into
F1876's distance mode, and the contact code after it also reorients the
starport. The Rust port treats the planet as simple and the test masks those
three ranges for starports (flag byte 0x0f) until the terrain query is ported.

## Oracle

`--worldgen`: 1,000 generated and 24 hand-coded systems, each created at a random
clock into a zeroed list: 1,024 systems, 17,762 physics objects, every flag byte,
the counter and every object byte (names zeroed, starport ranges masked) match;
every tampered field and an empty trace are rejected.
