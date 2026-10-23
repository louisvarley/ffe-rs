# Collision distance and frame switching (F569, F570, F572, F573, F575, F577)

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
  shape (`+0x14c` negative) goes to F576 (not ported). Otherwise F577.
- **F577**: the 64-bit coordinates are cut to 32 bits: if any high dword (or
  `b`'s outer radius' high dword) is set, shift right by `msb + 3`, else left by
  `0x1d - msb` of the low dwords (and `b`'s outer low). The near branch compares
  `(a.inner + b.inner) << k` (`+0x140`, the JJFFE collision radius), the far
  branch the shifted sum of the **outer** radii. The sum of F1522 squares (the
  high dword of the signed square of each coordinate) is compared, unsigned,
  with F1522 of the radius: radius larger is a contact (not ported). Otherwise
  the distance is `F1502(sum >> 1) * 2` (zero if negative) shifted back by the
  same amount into the 64-bit register; flag 1 is added if `b.inner != b.outer`
  low dwords differ (`+0x140` against `+0x138`), flag 2 always.
- **F572 / F573** (linear to rotating and back): position cut to 28 bits
  (F1658), turned by the parent's orientation (F1670 rows, or F1669 columns
  back), shifted up (F1517); orientation by F1673 (`a * bT`) or F1672 (`a * b`);
  velocity likewise; F572 sets `+0x58` to the frame stamp less one. F1677 adds a
  rotating body's position turned by the parent's orientation (F1669).

`--collide` runs the original on 4096 seven-body trees with the player last
(random radii, rotating bytes, orientations, stale `DATA_008704`, positions aimed
at 0.4 to 2.5 times the radius sum so many pairs are in reach but not touching):
3,103 cases without contact match in the return value, flags, distance, both
distance registers and every body (892 with a distance, 962 frame switches), and
the 993 cases the original reports a contact for are exactly the ones the port
declines; every tampered field and an empty trace are rejected.
