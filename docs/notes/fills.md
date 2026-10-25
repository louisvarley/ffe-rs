# Flat faces and polygon spans (F1604, F1607/F1608, F1714, F1560/F1613)

Ported in `crates/ffe-core/src/model_fill.rs`; verified by
`tools/reference/verify-fills.py` (v0.48h, spans v0.48i) and the texture-queue
oracle (F1714 outlines, v0.48i).

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

## Texture fills on the span path (F1805-F1808)

The queue's textured entries are the same span rasterisers with affine u/v
stepping: F1807/F1805 (F1644/F1647) take the descriptor's corner table
(`DATA_007791`/`007792`, `docs/notes/textures.md`), while F1808/F1806
(F1645/F1648) carry explicit per-vertex coordinates. The planet's emitter uses
the explicit form: `+0x10..` (quad: `+0x14..`) holds 3 (4) u/v word pairs, the
next dword is `F1620`'s bitmap in bits 0-11 and the `DATA_009252` slot base in
bits 16-31, then the colour word count and the F1326 colours in halfword pairs
(`ffe.asm:112316-112553`; the surface entries themselves are in
`docs/notes/terrain.md`). The renderer builds the slot table at that base, so
the bitmap's texel values select the original's land/sea colours.

## Polygon spans (F1794 -> F1795/F1796/F1797 -> F1799)

- **F1468** (F1794): a fresh table `DATA_009202`: 158 rows of 44 bytes (a count
  and ten x values), `+0x1b28` clip flag 0, `+0x1b2c` top 0x9e, `+0x1b30`
  bottom -1.
- **F1560** (per edge; F1795 its two points, F1796 the previous point and its
  own, F1797 F1548's curve edges): top end first (swap when the second is
  higher); dropped when `top.y >= 0x9e` or `bottom.y <= 0`; `bottom.y > 0x9e`
  clipped toward the top by F1563 (limit 0x9e), `top.y < 0` toward the bottom;
  either x off the view (`x >=u 0x140`) sets the clip flag; bottom/top widened;
  **F1559** steps `x = top.x << 16` by `(dx << 16) / dy` (dx > 0) or
  `-(-(dx << 16) / dy)` over `dy` rows from the top row, each row's
  **F1558** appending `x >> 16` when it holds fewer than ten.
- **F1613** (F1799, colour through F1327): nothing when `bottom < top`; rows
  from `bottom - 1` up to `top`; each row bubble-sorted (signed) and drawn in
  pairs from the last pair, `F(x[i], y, x[i+1] - x[i])` with F1600 (x clipped
  to 0..0x140, nothing when empty) if the flag is set, else F1601. An odd first
  value is left out.

`--span-raster` runs the original F1468, F1560 per edge and F1613 on 4096
synthetic closed polygons (3-12 points, on and off the view, self-crossing);
4096 compared, 0 mismatched.

## Faces reaching off the view (F1713/F1714)

F1709 sends a triangle (as a, b, c, c) and F1712 a quad to F1713 when no point
is on the view (rejected when the F1715/F1716 outcodes OR to all-left,
all-right or all-behind, `DATA_007818`), or straight to F1714 when some point
is. **F1714** turns (a, b, c, d) until a is in front (z >= 0x40), then queues an
F1794 outline: a's screen point, then for b, c, d in turn the screen point of
each vertex in front and F1564's near-plane point (from the vertex behind
toward its neighbour in front) at each crossing, back to a, and an F1799 fill
in the F1326 colour. `model_fill::near_outline` ports it; the executor now
records each queued face's views (`Emission::views`) for it.

The texture-queue oracle records these entries (path 3: the points, then the
colour) and compares them per face across every model and four views: 5,666
outlines, 0 mismatched (model 361, whose data differs between JJFFE and this
release, is left out).

## In ffe-rs

Untextured triangles and quads with every point on the view draw with
`fill_triangle` / `fill_quad` (`Shape::Flat`); any other queued flat face,
including those crossing the near plane (previously skipped), draws its F1714
outline with `fill_polygon` (`Shape::Polygon`), as do polygon entries
(`model_polygon::edges`). The diagnostic even-odd fill is left only for faces
with other point counts.

`Screen::draw_shapes` consumes the same shapes in the playable (`Screen::draw`,
`draw_discs`, `draw_station`, `draw_system_ships`/`_effects`): terrain and
planet surfaces, station/starport and building models, and model 155's child
fragments all go through `Shape::Flat`/`Shape::Polygon`/`Shape::Textured`.
`Shape::TexturedUv(Textured, uv)` carries the planet's explicit coordinates
(the descriptor `Shape::Textured` is unchanged) and `ModelViewer::rasterise_uv`
runs the F1644/F1647 affine rasteriser with them, using the cache's bitmap 0x40
(land), 0x121 (sea) or 0x5e (the explicit-coordinate terrain block) and the
emitter's slot colours. Without a cache bitmap the old flat/Gouraud stand-in
remains.

The curved-surface queue is generated before the frame's palette build (the
emitters' F1838/F1839 call F1326 while queueing), its colour words are
registered, and the queue is drawn after the build
(`ModelViewer::{collect_surfaces, register_surface_colours,
surface_shapes}`). Child executions keep their own `surfaces` list, so
`all_surfaces` walks the child tree: the intro's big planet is model 449, a
surface under model 349, not a face model. Clearing the palette cube before
each frame's registration is what keeps the surface representatives stable
(`docs/notes/palette.md` §3).

The queue entries keep their F1777 keys (`SurfaceBatch::order`), and
`ModelViewer::merge_surface_shapes` inserts them into the frame's primitive
order (`Frame::keys`, descending F1788 order). They are therefore *not* all
drawn after the model: at the intro's `0x181534` the far planet is queued
before the nearer ship, which draws over it. F1879's atmosphere-ring
`TEXTURE_QUAD` (handler 5 inside the F1780 group, bitmap `0x43`) also takes
`terrain_texture`/`texture_shape` rather than the old Gouraud stand-in; only a
cache without the bitmap falls back.
