# Planet terrain query (F1816-F1834, F1846, F1849-F1867, F1875, F1876)

`crates/ffe-core/src/terrain.rs`. A planet is an icosahedron (twelve corners,
twenty faces, thirty edges, tables `DATA_007892..7894`) whose faces are
subdivided edge by edge; F1876 either draws the result (mode 0; the subdivision and
the triangles it hands the emitter are ported, see "Drawing" below) or measures the
distance from a point to the surface (mode 1, the collision query).

## Arenas

Two fixed arenas back the subdivision: 1500 nodes of 0x48 bytes (F1818 aborts the
whole query when 0x5dc are outstanding) and 3000 edge records of 16 bytes (F1820
aborts at 0xbb8). The abort is a `longjmp` back into F1876, which returns the
stream pointer it had read. F1819 and F1821 give back (F1821 frees an edge,
everything below it and the chain behind it). The nodes keep their old bytes in
the original; the port starts them zeroed, which only matters for a field read
before it is written (none is).

## Nodes and edges

A node is `+0` `+2` the map coordinates (words), `+4 +8 +0xc` the position,
`+0x24 +0x26` quadrant words (F1831/F1832/F1848 in this mode), `+0x28 +0x2c +0x30`
the edge vector, `+0x34..0x3a` the two layers' map coordinates, `+0x3c` the
height, `+0x40` the clamped height and `+0x44` the detail level (F1829:
`level_base - msb(max(z, 0x400))`, at least 1). An edge record holds its head node,
links to its two halves (records whose head is the half's far end) and the
midpoint node.

## The midpoint routines

F1849-F1867 (table `DATA_007844`, 32 slots, ranks in the cache) make an edge's
midpoint. They are one pattern with six switches (`Variant`): the edge vector is
the scaled sum of the ends' (F1849 family) or the first end's; the height is none,
the mean or the mean plus F1846's noise; the detail test uses one level less when
the heights straddle one (`signed`); the displacement also takes the new height's
deviation from the mean; the height has a clamped copy; and two feature layers
turn the height by the 128 by 128 maps. Where both ends are coarser than the
requested level the midpoint is the ends' mean (with its map coordinates and, from
the corner adjustment tables `DATA_007854`.., the face's orientation); otherwise it
is displaced along the edge vector.

`--terrain-mid` runs each routine 323 times on random nodes, levels and layer
tables, including an already-made midpoint: 6,144 cases, 5,115 new nodes, all four
nodes, the links and the arena counts match.

## The distance query (F1876, mode 1)

`Terrain::distance(seed, max, stream, translation, matrix, kind, detail)`.

1. F1823 files the stream's records (16 bytes: key, feature type, six words) into a
   tree: the key's highest bit marks its length, the five bits after it are the
   face and each further pair a sub-triangle. A node has two slots (a record, or a
   non-positive count). A record filed above another is undefined in the original
   (it indexes a table with a pool address); the port reports `Fault::Overlap`.
2. The matrix rows' longest length (F1825/F1467) sets `level_base`:
   `msb(2 * F1521(length, 0x54000000)) + detail - 1`, `detail` being `DATA_008812`.
3. The twelve corners are the unit directions turned by the matrix, plus the
   translation, with a hash-seeded height (or a latitude-based one for the polar
   type, whose first function is F1842).
4. For each face F1834 gives the corners their level and their features' heights
   (`height >> 7`, times the multiplier map, plus the height map's signed byte
   shifted by 21), choosing the midpoint routines by the type's third dword, plus
   8 if the face has a tree and plus 2 if the point is not far (`z <= 2 * length`).
   The routine for the first four levels is read four slots on (`DATA_007845`).
5. The first face whose three corners' cross products (F1875, in x and y relative to
   the point) are all non-negative is subdivided: each level makes the three edge
   midpoints and continues in the sub-triangle holding the point (corner A, B, C or
   the centre), until level 9 or until no corner has the level. The feature
   records at the tree node of each level fill the layer tables for the levels
   they cover (cleared only after a successful search).
6. The final triangle's plane gives the height at the point: positions shifted
   below 14 bits, two ratios by `idiv` (the divisor shifted down and made 1 if it
   becomes zero), the interpolation shifted back.

The result is the interpolated distance, or the limit if it is larger or no face
held the point. Types 3, 5 and 6 (third dword 0x18) have no routine pair for faces
with features (the original jumps through garbage), and features of kinds other than
12 (the only one with a height map) crash it; neither occurs in the game's models.

`--terrain` runs 2,048 synthetic planets (random orientation and size, a point far
away, near and a few units from the surface, off-axis, fourteen types, details 0 to 4,
streams of random non-overlapping records and of a record on every face or sub
triangle): the distance, bytes consumed, the unlimited distance, the arena counts
and the layer tables match.

## Drawing (F1876, mode 0)

`crates/ffe-core/src/terrain_draw.rs`, `Terrain::draw(scene, stream, emit)`. F1874
calls F1876 with its twelfth argument 0. The set-up differs from the query's only in
three places: `DATA_007837` (the detail minus one, plus one for each of 1/16 and 1/8
of the planet's radius that the view is away) feeds the level base; each corner is also
projected (F1479) into its screen words `+0`/`+2`; and F1833 installs the screen
functions `DATA_009280..9282` (F1715, F1716 and F1847 in place of F1831, F1832 and
F1848, `Projection::Flat`), the emitter `DATA_009270` (F1840) and its set-up (F1841),
and `DATA_009264`, `0x1dcd6500 >> zoom`, the depth added to a triangle's key position.
With `Projection::Flat` a node's `+0`/`+2` are screen coordinates (the midpoint routines
average them where the texture mode averages map coordinates), `+0x24`/`+0x26` their
region codes (1 below, 2 inside, 4 above, 8 for a node nearer than 0x40).

Each of the twenty faces goes through F1834 (as in the query) and then F1868, or F1869
when the planet type's span (third dword) is odd and at least 8 (the types with a sea).
F1868 takes the three nodes, the three edges, the level, the slack bound and the
feature tree node:

1. The slack is the bound, or from level 10 on the bound plus the height spread
   (`(max - min) >> 3` of the clamped heights, times the sum of the first node's edge
   vector magnitudes). The triangle is dropped if the first node (plus the slack) is
   nearer than 0x40, or lies outside the view's pyramid: `|x|` or `|y|` minus the slack
   beyond the depth plus the slack.
2. With all three nodes at least 0x40 deep: the screen area is the cross product of two
   edges. The triangle is final when the level exceeds 13, when it exceeds all three
   nodes' own levels, or when its area is small (0 to 0x95) and the level above 8 (F1869
   counts the nodes at or below the sea and, when it is not 0 or 3, takes one level off
   the tests). A final triangle is dropped if the OR of its region codes is wholly outside
   (`DATA_007838`, 16 dwords) or if it faces away (area not positive; F1869 keeps
   triangles across the sea whatever their facing); otherwise it goes to the emitter with
   its unit normal (F1827, 15 bits), the depth key (F1777) of the first node's view
   position pushed forward by `DATA_009264`, and the level.
3. A triangle that is not final is dropped at the first two levels when it lies wholly
   beyond the horizon (each corner's reduced position (F1826) dotted with the normal
   against `(-0x5000 >> level) * length`).
4. With a corner behind the view (a depth under 0x40): dropped if the bound is under
   0x1f4 or all three corners lie to the same side in x or in y.
5. Otherwise the feature layers of the tree node are put in place for the levels they
   cover (and the corners' map coordinates saved), the three midpoints are made (the
   routine pair of F1834: level below five or not), three edge records are made, and
   the four sub-triangles are drawn (corner A, B, C, the middle; the bound halves);
   afterwards the layers are cleared and the coordinates restored, and the three edge
   records (with whatever hangs below them) are given back.

An arena overflow ends the whole drawing (the triangles already emitted stand).
`--surface-draw` (probe build) runs 512 planet-scale patches through the original F1876,
one child process each (the original exits when the 50,000-word order buffer is full,
which 51 of them do): the model's own record (70%) or a random type and records, a random
orientation and distance (1.02 to 60 radii), the instance's scale, zoom, light and
colours, and detail -1 to 4. The probe hook on the emitter call records each triangle's
nodes (screen and view positions, region codes, heights, level), normal, key and level:
all 461 completing cases match, 207,689 triangles (391 cases drew), with the arena counts.

### The emitter (F1840 and the other planet types)

`crates/ffe-core/src/terrain_emit.rs`, `draw_surface`. F1833 installs the planet type's
emitter and set-up (`DATA_007843`, first two dwords; the cache holds ranks): rank 1 F1840 and
F1841 (types 0, 1, 2, 4, 7, 10-13), rank 2 F1842 and F1843 (the polar types 3, 5, 6), rank 3
F1844 (type 8, a lit flat triangle) and rank 4 F1845 (type 9, flat in the material colour); the
set-up of 3 and 4 is F1836, which does nothing.

F1841 builds three colour tables from the type's palette (dwords 3 to 11 of its entry, as
packed nibble colours) with two recipe tables of the exe (`DATA_007850`, `DATA_007851`): 22
ramp steps (`DATA_007848`), 21 land rows of seven (`DATA_007849`) and four rows of seven sea
colours (`DATA_007846`; the fourth is `DATA_007847`). Every colour is a sum of two palette
colours weighted in sixteenths, kept as `0x0b0g0r` dwords and packed to 12 bits (`r << 8 +
g << 4 + b`) by F1326 where it enters the queue. F1843 builds `DATA_007852`, 32 bands, the
same way.

F1840 takes the triangle's three nodes ordered by height (highest first), its normal and
key. The light (the instance's `+0x134`) against the normal picks a base colour out of the
instance's eight shade colours (`+0x84`), none when the triangle faces away. A haze (F1877,
for planets with a word-8 parameter) blends the tables toward a colour by how far the
triangle is. With the detail setting below zero, or a far triangle (key above `0x1fffffff`)
with all nodes at or below sea level, or a very far one (key above `0x7fffffff`) whose
heights differ by under `0x1fffffff`, the triangle is one flat F1804 entry. Otherwise it is
textured by how many of its nodes are at or below sea level (heights of zero or less):

| Below | Entries |
|---|---|
| 0 | F1807 (bitmap `0x40`, seven land colours by the lowest node's height) when the heights differ by under `0x400000`; else F1808 with explicit coordinates and the 22 ramp steps |
| 1 | the part above the sea as a quad F1805 (bitmap `0x40`; F1806 with coordinates when the highest node is `0x400000` or more), the two corners being where the edges to the lowest node cross sea level (F1837), then the part below as F1807 (bitmap `0x121`, the fourth sea row) |
| 2 | the part below as an F1805 quad (bitmap `0x121`) and the part above as F1807 or F1808 |
| 3 | one F1807 (bitmap `0x121`) with the sea row picked by the heights' sum |

Each entry is queued by F1778 with a header of the key and the position of the previous
header; the queue holds 50,000 words, and the original exits when one more is asked for
(the port reports `Fault::QueueFull`).

`--surface-draw` also prints the queue: its length and a hash, the first 600 words and (for
a single case, `--surface-draw N`) all of them, with the draw routines' addresses as
`0xf18000nn` and the header links as `0xf2000000 | position`. The 102 completing cases
whose record has no word-8 parameter (so F1877 does nothing) match word for word, and the
10 that overflow the queue overflow in the port. The cases with a haze are compared to the
triangle level only until F1877 is ported.

## The atmosphere (F1877, F1878, F1879)

`crates/ffe-core/src/terrain_haze.rs`, `terrain_emit::draw_planet`. F1876 shifts the
record's word `+10` by the instance's scale less eight, then F1877 runs when the record's
word `+8` (shifted left sixteen) is non-zero: the axes are put in the order that makes
each dominate its coordinate (`DATA_007895`, the six permutations in order), the origin is
solved in their frame in single-precision x87 steps (`x87::Ext`: loads exact, every
`fstp dword` rounds, the result times 32768 truncated), the haze state `DATA_009262..9267`
is set (flag, the view distance of the ring, the colour, the scale as `scale * far /
(far - near)` truncated), the sky colour is mixed or taken from the light, and the ring is
queued in a group of its own (F1780: a header with key `0xffffffff` and the open marker,
bare entries, the close marker): 32 steps round the planet, each a textured quad (F1805,
13 words: four corners, bitmap `0x43`, seven colours from the material to the sky) and a
ring quad (F1810, 6 words) from F1879, shaded by F1878's light on the ring's direction.
A division by zero in the solve faults like the original's (`SIGFPE`, 2 of 512 cases).

Oracle: `--surface-draw` now compares the haze state, the sky and the whole queue for the
361 cases with a haze (311 with a ring; 2 change the sky, 4 are inside the haze), 465
queues word for word and the 47 faults (45 queue overflows, 2 divisions by zero).

## The close-range base sphere (ffe-rs flight view)

`Screen::draw_discs` scales a planet's surface axes so their world radius is the body's
**inner** radius (`+0x140`), not the outer atmosphere radius (`+0x138`). F879 places an
open-air starport on the terrain at about 0.9 of the outer radius (Earth: 7,032 km against
7,810 outer and 4,970 inner), so scaling to the outer radius put the eye inside the drawn
base sphere at starport altitude; F1876 then dropped every triangle as a backface and the
ground vanished. The inner radius also matches the surface model's base sphere in the
starport frame (`surface-starport` in `tests/planet_view.rs`). The old `z > radius` disc
filter is gone as well: it removed the body at exactly the range a landing approach reaches.
The outer radius remains the collision/atmosphere radius, so the haze (F1877) still spans it.
