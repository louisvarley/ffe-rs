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

## Starports

F879 measures the planet's surface with F574 twice (at the nominal radius and at
the radius less 7/8 of the first altitude), then places the starport from the
second result. For a planet (a complex collision shape) F574 runs F575, which sends
the pair through F576 (`surface.md`) and the terrain query (`terrain.md`). When the
second query lands inside the terrain F575's contact code settles the body: it is
moved up by the difference between its inner radius and the altitude, its second row
becomes the surface's up, the matrix is rotated (F571), re-orthogonalised (F1520) and
rotated back, and its velocity is cleared (`landing.rs`: the branch where the crash
mask `DATA_008700` is zero, which is F574's). A moon (a simple sphere) that the
starport touches takes the other branch of the same code: the body's position is set
from the planet's radius word and the sum of squares F577 left. The crash and bounce
branch (mask non-zero) is not ported.

## Oracle

`--worldgen`: 1,000 generated and 24 hand-coded systems, each created at a random
clock into a zeroed list: 1,024 systems, 17,762 physics objects, every flag byte,
the counter and every object byte (names zeroed) match, starports included;
every tampered field and an empty trace are rejected.
