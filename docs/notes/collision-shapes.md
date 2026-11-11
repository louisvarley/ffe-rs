# Collision against complex shapes (F576, F579-F583)

`crates/ffe-core/src/surface.rs`. A body with `+0x14c` negative (planets, stations,
big ships) is tested by F576 instead of the simple radius test F577. The shape is the
model's collision stream (`+0x34`), read from the cache's `base:model/collision`.

- **F576(ship, shape, relative)**: clears `DATA_008697`; the ship's own spheres are
  the region words *before* its model's stream, read backwards in pairs (a distance
  word, zero at the end, then a direction/radius word: axis `& 6` of the ship's
  orientation, negated by bit 0, `(component >> 16) * distance`, shifted down by
  `0x17 - scale`; the radius is `(word & 0xfff8) << 15` likewise). The first sphere is
  the ship's inner radius at its own place; a later one is tried only when the ship
  is itself complex. Each is F579; a positive result sets bit 7 of `DATA_008697`, a
  non-zero `DATA_008697` becomes the flags (`DATA_008696`) and records the object and
  the stream entry; zero or more stops with a hit (0), otherwise -1.
- **F579**: the relative position (64-bit) is cut to 28 bits (F1658), shifted
  further so the model's `+0x14` plus `+0x18` zoom fits (`0x17`), the radius scaled by
  the same shift, and the position turned into the shape's frame by its
  orientation's rows (F1670) unless the ship is rotating in that shape's frame. Then F582.
- **F582**: entries of a type byte and a flag byte; type `0x20` gates the entry on a
  bit of one of the shape's animation words (`+0x9c`). By `type & 0xc0`: 0 a list of
  surfaces the sphere must not be outside of (any surface it is outside of ends the
  test as clear), or the `0xff` terrain record; 0x40 a list the sphere is outside of
  all of (inside one skips the entry; outside all ends the test as clear); 0x80 a
  child model at one of the parent's vertices (F580: the position minus the vertex,
  with sign flips and one of six axis permutations); 0xc0 a list the sphere is inside
  of (a hit if inside all and bit 7 of the flags is set). Flags (`flag & 0x7f`, with
  bit 7 meaning a hit) collect in
  `DATA_008697`.
- **F583**: a sphere against surface `index` of the mesh: the normal record is
  `normals[(index >> 1) - 1]`, its vertex reference gives the plane's point
  (the x negated when the index and the reference differ in parity), and the plane
  distance `-2 * dot` is compared with the radius, each shifted down so the products
  fit.
- **F581**: the terrain record (`0xff`, six bytes of the four vertices' indexes and
  the planet type, then the feature list). The vertices a (the centre), b, c and d
  give the basis (b-a, c-a, d-a); a minus the position is the query point. Outside the
  outer radius is clear, inside the inner radius a hit; between them the point is
  turned onto the z axis (two rotations from F1523 ratios) and the terrain query F1876 gives
  the distance (see `terrain.md`), which is clear if the ship's radius is not below it. The
  results go to `DATA_008703/8705` (shifted back up by the scale) and `DATA_008707`.

`--planet` runs the original F576 for 4,096 pairs: any model with a collision stream
standing in as the complex shape (half the cases are the 36 planets) against any
ship model (a third with region chains), random rotations, radii from the model's
extent, positions 0.2 to 3 radii away and random animation words: the return value,
`DATA_008696..8707` and the distances all match (cases whose animation words enable an
entry that names a surface the model lacks are skipped; the original reads past its data).

Not ported: F578 and the contact code of F575 (landing, crash, bounce) that a hit leads to.
