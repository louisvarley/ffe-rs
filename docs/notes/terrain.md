# Planet terrain query (F1816-F1834, F1846, F1849-F1867, F1875, F1876)

`crates/ffe-core/src/terrain.rs`. A planet is an icosahedron (twelve corners,
twenty faces, thirty edges, tables `DATA_007892..7894`) whose faces are
subdivided edge by edge; F1876 either draws the result (mode 0, the x87 path
and the rasterisers, not ported) or measures the distance from a point to the
surface (mode 1, the collision query).

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
