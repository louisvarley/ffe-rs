# Flat faces (F1604 DrawTriangle, F1607/F1608 DrawQuad)

Ported in `crates/ffe-core/src/model_fill.rs`; verified by
`tools/reference/verify-fills.py` (v0.48h).

## Who calls them

F1709 queues an F1804 entry (three points, F1326 colour at +0x10) and F1712
an F1809 entry (four points, colour at +0x14) only when every point is on the
view (`x <u 0x140`, `y <u 0x9e`); faces reaching off it go to F1713/F1714 and
the polygon path. F1804 calls F1604 and F1809 calls F1607 with the colour
through F1327. The entry's points are the face's screen points in vertex order
(already compared by the texture-queue oracle).

## F1603 / gradient

`(dx << 16) / dy` on the magnitudes (32-bit shift, truncating idiv), with the
sign applied after; `dy == 0` gives `0x7fffffff`, or `0x80000001` for
`dx < 0`. F1604 computes the same inline; F1616's slope is the same formula.

## F1604

Sort by y with three conditional swaps (p1/p2, p1/p3, p2/p3, strict `>`).
Rows run from the top row up to, not including, the bottom row. With
`y1 < y2`: both edges start at `x1 << 16`; the one with the smaller step
(`s12` vs `s13`) is the left edge; at `y2` the p1-p2 edge is replaced by p2-p3
(restarting at `x2 << 16`). With `y1 == y2` (flat top): left starts at the
smaller of x1, x2. Each row, only if `y <u 0x9e`: `memset` from
`max(left >> 16, 0)` to `min(right >> 16, 0x140)`, exclusive, when non-empty.

## F1607 / F1608 / F1602

F1607 calls F1608 with F1601 as the row routine. F1608 finds the smallest y
(sequential min), turns the points (a, b, c, d) -> (b, c, d, a) until the
first is on that row, and sets the "left" edge a-b and the "right" edge a-d
(F1603; the names are by order, not geometry). Then, by which of b, d (and c)
is higher, up to three F1602 segments, each side switching to its next edge
(b-c, c-d or d-c, c-b) when it reaches its point; a side's remaining height is
reduced by the segment drawn. F1602: a negative height, `y >= 0x9e` or
`y + height <= 0` only advances; rows above 0 are stepped over; each row is
`F1601(min, y, |right - left|)` (no +1, no x clipping) until the height runs
out or `y` reaches 0x9e; then the edges advance by any remaining rows.

## Oracle

`--fill-raster` runs the original F1604 and F1607 on 4096 synthetic faces
(random, small, convex and twisted quads, edge values; triangles also far off
the view) with the framebuffer pointer 300 rows into a 0xff-filled 320x800
buffer. `fill_reference.rs` compares count, bounds and hash: 4096 compared, 0
mismatched; tampered fields and an empty trace are rejected.

## In ffe-rs

Untextured triangles and quads whose points are all on the view become
`Shape::Flat` and draw with `fill_triangle` / `fill_quad`. Faces reaching off
the view (the original's F1713/F1714 polygon path) and polygons still use the
diagnostic even-odd fill (F1560/F1559/F1613 are not ported).
