# Lines (F1616 DrawLine, F1601 DrawHLine)

Ported in `crates/ffe-core/src/model_line.rs`; verified by
`tools/reference/verify-lines.py` (v0.48f).

## Who calls it

F1803, the draw routine of the line entries F1704 queues (opcode 2 lines via
F1703, and thrusters too narrow for a glow), passes the entry's two points
(+4/+8 and +0xc/+0x10, x already `x - 1` when positive) and its colour (+0x14,
through F1327) to F1616. F1704 has clipped both ends onto the 320x158 view
(`docs/notes/thrusters.md`), so F1616 does no clipping. F1616 has other callers
(the console, 35706-35732 and 82149-82192, 91661-91692) not ported here.

## F1616

- Same row: one F1601 run from the smaller x, `|dx| + 1` long.
- Otherwise the upper point (smaller y) first; `dy > 0`. Slope
  `s = (dx << 16) / dy` (idiv, truncated toward zero; for `dx < 0`
  `-((-dx << 16) / dy)`), and `x = top.x << 16`.
- `|s| <= 0x10000` (steep): one pixel at `x >> 16` per row from the top row up
  to the row before the end, `x += s` each row, then one pixel at the end point.
- `s > 0x10000` (shallow, leaning right): each row a run from `x >> 16` to
  `(x + s) >> 16` (exclusive), `x += s`; the end row runs from `x >> 16` to the
  end point inclusive.
- `s < -0x10000` (shallow, leaning left): one pixel at the top point; then for
  each row from `top.y + 1` to the end row inclusive, a run from `(x + s) >> 16`
  of `(x >> 16) - ((x + s) >> 16)` pixels, `x += s`. The left-leaning case is
  therefore a row ahead of the right-leaning one, and its last run need not
  reach the end point: the original's asymmetry, kept.
- The two branches for `dy < 0` and the `0x7fffffff` slope for `dy == 0` are
  unreachable after the swap and the same-row test.

## F1601

`F1601(x, y, length, colour)`: writes `length` bytes from `buffer[y * 320 + x]`
(byte stores up to a dword boundary, then dwords, then bytes); nothing when
`length <= 0`. No clipping.

## Oracle

`--line-raster` (headless adapter, export `_ref_draw_line`) runs the original
F1616 on 4096 synthetic on-screen lines (random, horizontal, vertical, short,
near the 1:1 slope, at the view's edges) into a 0xff-filled 320x200 buffer and
hashes it. `line_reference.rs` draws each with `draw_line` and compares the
count, bounds and FNV-1a hash: 4096 compared, 0 mismatched. Tampered points,
colour, count, bounds and hash, and an empty trace, are rejected.

## In ffe-rs

Opcode 2 lines now take F1704's clipped, stored points (`model_thruster::line`,
already verified with the thrusters) in place of raw projections, so every
F1803 entry the viewer draws has both ends on the view; a line F1704 rejects
draws nothing (it is still queued for ordering, as before). `Screen::draw_shapes`
draws each with `draw_line`. Only stand-in shapes (the cubic curve segments and
two-point faces) still use the bounded Bresenham.
