# Lines and curves (F1616 DrawLine, F1601 DrawHLine, F1546)

Ported in `crates/ffe-core/src/model_line.rs`; verified by
`tools/reference/verify-lines.py` (lines v0.48f, curves v0.48g).

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

## Curves (F1811 -> F1546)

F1811, the draw routine of opcode 22's curve entries, passes the entry's four
points (+4/+8, +0xc/+0x10, +0x14/+0x18, +0x1c/+0x20, entry order) and colour
(+0x24, F1327) to F1546. F1546 sets up F1549's forward differences (the same
set-up F1547 uses for polygon edges, `model_polygon::curve_edges`): `2^n` steps,
n = 2..5 from the Manhattan distance between the first point and the four
points' mean, count `2^n - 1` counted down to -1. Each step is the segment from
the current point to the next; unlike F1547 there is no closing edge to the end
point. Per segment (`x <u 0x140`, `y <u 0x9e` is on the view): both ends off,
skipped; one end off, F1561 (F1562 x onto 0..0x13f, then F1563 y onto
0..0x9d, each along the line to the other end) moves it, then F1616; both on,
F1616. F1561 does not reject, so an end can stay off the view and F1616 then
writes outside it.

`--curve-raster` runs the original F1546 on 4096 synthetic curves (on the view,
small, and spilling off it), with the framebuffer pointer 300 rows into a
0xff-filled 320x800 buffer so writes outside the view are captured; 4096
compared, 0 mismatched, the same tampers rejected.

## In ffe-rs

Opcode 2 lines take F1704's clipped, stored points (`model_thruster::line`,
already verified with the thrusters) in place of raw projections, so every
F1803 entry the viewer draws has both ends on the view; a line F1704 rejects
draws nothing (it is still queued for ordering, as before). `Screen::draw_shapes`
draws each with `draw_line`, and each curve entry (`Shape::Curve`) with
`draw_curve`, dropping the pixels the original would write outside the view
(over the console). Only two-point faces still use the bounded Bresenham.
