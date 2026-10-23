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
