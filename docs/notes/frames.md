# Frames of reference (F590, F595, F604, F605, F606, F1677, F1678)

An object's position (`+0x3e`) and velocity (`+0x8c`) are relative to its parent
body, the index byte at `+0x56` (zero: none). `crates/ffe-core/src/frames.rs`
ports the routines that move an object between frames and measure distances.
The object list is `index * 0x152 + list + 0x74` (F1531 uses the global list
pointer `DATA_007758`, F1532 an explicit one); the byte at `list + index` has
bit 0x10 set for a live object.

- **F1677** (absolute position): no parent: the position itself. Otherwise the
  cached value at `+0x26` when the stamp byte `+0x25` equals the frame stamp
  `DATA_008811`, else the parent's absolute position (recursively) plus the
  position. The result is cached with the stamp. (A rotating frame, `+0x57`
  non-zero, multiplies the position by the parent's matrix first: not ported.)
- **F590(a, b)**: refreshes `a`'s cache, then the vector from `a` to `b`: the
  difference of positions if they share a parent and rotation; `-a.position`
  if `a`'s parent is `b`; otherwise `b`'s absolute position less `a`'s cache. It
  is cut to 13 bits (F1658) and the shift returned.
- **F595** (velocity): `+0x8c..` unless the object is a station (`+0x87` is 1),
  which is zero when `+0x14c` bit 0x20 is set and otherwise comes from the
  station's spin (F1533/F866, not ported).
- **F604(a, parent)**: `a` takes the parent's parent byte; the parent's position
  and velocity are added. **F605(a, list, index)**: `a` takes `index` as its
  parent byte; that body's position and velocity are subtracted. Both raise
  F148 event `(0xb, 0)`.
- **F606(a)** picks the frame: skipped for rotating objects. With a parent `P`,
  the score is `(shift - radius) * 4 + 0x20` (radius is the word at `+0xda`);
  with no parent, 0x32. If `P` has a parent `G` and `G`'s score
  (`shift - radius + 0x28`) is not larger, `a` enters `G`'s frame (F604) at
  once. Otherwise every live body from index 0x72 down to 1 is scored: one with
  `a`'s parent by `(shift - radius) << 4` (closer than the best so far becomes
  the new frame); one whose parent is `G` by `(shift - radius) * 4 + 0x20` (if
  closer than the best, `a` leaves to `G`'s frame and the scan ends). At the end
  `a` enters the chosen body's frame (F605) if that is not its parent.

`--frames` runs the original on 4096 synthetic lists of seven bodies in a tree
(positions of every magnitude, radii 0 to 40, random live flags, frame stamps
and caches, stations locked): F590, F604, F605 and F606 each 1,024 cases; in the
F606 cases 215 enter the grandparent frame, 660 another body's, 149 are
unchanged. Every body's parent, position, velocity, stamp and cache match, and
F590's vector and shift; every tampered field and an empty trace are rejected.
