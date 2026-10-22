# Textured model faces

How the original draws a textured model face, from the face emitters to the
pixels, with routine numbers from `jjffe-master/ffe.asm` (JJFFE 2.8a7). Status
of each claim: **verified** (compared against the executed original), **read**
(from the assembly, not separately executed) or **unknown**.

Rust: `ffe-core::model_texture` (entry colours, flat fallback, path rule,
rasterisers), `ffe-rs::textures` (cache resources), F6 viewer
(`ffe-rs::model_viewer`, `Screen::draw_model_viewer`). Oracle:
`tools/reference/verify-textures.py`, `crates/ffe-rs/tests/texture_reference.rs`.

## 1. Material word

F1692 decodes a material word into a 16-byte record: RGB nibbles, texture,
unlit (`0x2000`) and local (`0x1000`). Bits 14-15 non-zero make it textured:
RGB is zero and the texture is `value & 0xfff`. Texture 0 takes the untextured
path (F1708 tests the texture field after F1695). **Read**; the decoding was
already verified (`model_commands::material`).

## 2. Path choice in F1708 (triangles) / F1712 (quads)

Arguments arrive in the order `model_commands::Face::vertices` holds them
(v0..v2 / v0..v3). After the normal cull (normal flag bit 0) and projecting every
vertex (F1471), the textured path is taken when all of:

- `DATA_008812 >= 0` (texture rendering on; the adapter alias `_ref_detail_primary`);
- every screen point within x `-2000..=2320` (0xf830..0x910), y `-2000..=2158` (0x86e);
- F1695 leaves a non-zero texture (it only adds the instance colour `+0xf4..+0xf6`
  for local materials and clears the local flag);
- the cyclic screen perimeter (sum of |dx| + |dy| over v0→v1→…→v0) is at least 6.

The F1717/`DATA_007815` outcode test after that compares an address with zero and
never rejects. Otherwise the face takes the flat path (F1693 + F1691, then F1709 /
the quad equivalent). **Verified** (queue oracle, every face of every model view;
`model_texture::textured_path`).

## 3. The textured queue entry

The key is F1777 of v0's view, as for flat faces. F1778 allocates 12 dwords
(triangle, draw routine F1807) or 13 (quad, F1805):

| Words | Triangle (F1807) | Quad (F1805) |
|---|---|---|
| 0 | F1807 | F1805 |
| 1.. | screen v0, v1, v2 (packed x/y words) | screen v0, v1, v2, v3 |
| next | descriptor dword | descriptor dword |
| next 7 | F1326 colours | F1326 colours |

The descriptor is record `texture` of `DATA_007811` (30 bytes each, 148 records,
record 0 empty): dword bitmap (bits 0-11 bitmap id, bits 12-14 quad orientation),
dword count (1..=7), 7 nibble RGB triples, one pad byte. The emitter writes
`count` colours: for each record colour, add the shade byte-wise (wrapping), clamp
each component to 15, pack with F1326 (`r << 8 | g << 4 | b`). The shade is the
instance colour (local materials, via F1695) plus the face normal's colour unless
the material is unlit. Slots beyond `count` are not written (stale queue memory).
**Verified**: 2,677 textured entries in 1,720 model views match word for word.

Flat fallback colour of a textured face (F1693 + F1691): material RGB (0) +
instance colour if local + normal colour if lit (F1693 does **not** test the
texture) + the descriptor's first colour (F1691: `DATA_007812 + 30 * texture`).
**Verified** for every non-culled, non-textured-path face (the F1691 output is
captured). Note: `model_execute::resolve_colour` omits the normal colour and the
F1691 addition for textured materials; that matches F1693/F1691 only for
untextured materials. It is used for colour commands (F1762) and curves (F1737);
whether either ever sees a textured material was not checked here.

## 4. Drawing: F1807 / F1805

Both copy their 7 colour words through F1327 into `DATA_009252[0..7]`, then call
F1644 (`DrawTexTriangle(v0, v1, v2, dword)`) or F1647 (`DrawTexQuad(v0..v3,
dword)`). F1327 maps a 12-bit colour through `DATA_007770` (4096 bytes, fixed
low-palette index + 1) or, when that is 0, to the runtime-allocated upper palette
entry (`DATA_009139`/`DATA_009140`, filled by F1326's request flag and the
allocator around F1329).

### Texture coordinates (implicit, no per-vertex UVs)

F1643 (`dword & 0xfff`) takes the bitmap from F1620, `DATA_008240[id]`. That table
is the cache's **primary** table (96 entries); `DATA_008241` (**secondary**, 251)
follows it directly in memory in JJFFE and in both releases (RVA difference
0x180), so ids 96.. read `secondary/(id − 96)`. The descriptor table names ids
50..94 (primary) and 99..342 (secondary). F1643 picks the span routine: width 64
→ 64-stride, any other width → 128-stride; header flag bit 0 → the transparent
variant (texel 0 skipped). It points `DATA_009249` at the corner table
`DATA_007791` (64: (0,0), (63,0), (0,63), (63,63)) or `DATA_007792` (127).

- Triangles (F1644): v0 → corner 0, v1 → corner 1, v2 → corner 2. Orientation
  bits are ignored.
- Quads (F1647): `dword >> 12 & 7` selects the corners for v0..v3 from
  `DATA_000047`: 0 `[0,1,3,2]`, 1 `[1,3,2,0]`, 2 `[3,2,0,1]`, 3 `[2,0,1,3]`,
  4 `[0,2,3,1]`, 5 `[2,3,1,0]`, 6 `[3,1,0,2]`, 7 `[1,0,2,3]` (rotations, then
  mirrored rotations). Only record 21 (bitmap 63) uses a non-zero orientation (3).

### Rasterisation (affine, no perspective)

F1646 / F1649 rotate the point list until the first point has the smallest y,
set up two edges with F1642 (16.16 x and u/v at the start point, per-row steps
`(Δ << 16) / Δy` truncated, `0x7fffffff`/`0x80000001` for Δy = 0) and walk the
triangle (one edge change) or quad (two) through the span routine with row counts
that may be zero or negative (negative counts only move the edges). F1650-F1653
clip rows to 0..158 and columns to 0..320, draw from `x_left >> 16` (exclusive
right end), step u/v per pixel by the truncated span difference / width, read
texel `(u >> 16) + ((v >> 16) << 6)` (or `<< 7`), and write `DATA_009252[texel]`.
**Verified**: 4,096 synthetic triangles/quads (ten bitmaps: 64/128 wide, masked
and opaque, block 94; all orientations; on-screen, clipped, twisted and
degenerate shapes) match the whole 320x200 buffer. No case read outside its
bitmap; the port counts such reads instead of inventing memory.

### Texel values and palette

Texels are slot numbers, not palette indices: bitmaps used by faces hold 0..6
(bitmap 51: 0..3, matching its 4-colour records). Bitmaps 74, 81, 88 and 91 also
hold texel 7 (and block 94 up to 20): `DATA_009252[7..]` is not written by face
entries, so those pixels show whatever an earlier F1806/F1808 entry left there
(zero at start-up). The final colour is therefore the descriptor colour + shade,
clamped, as a 12-bit colour; the fixed low palette is used only when
`DATA_007770` lists that 12-bit colour (e.g. `0x777` → low 7). **Read** (F1326,
F1327); the 12-bit values are verified, palette allocation is not.

## 5. Bitmaps 94/95 (64-wide texture blocks)

Primary 94/95 have header width 64, height 0 and 32,768 data bytes (64 x 512).
Block 94 is queued by F1840 (via F1808 / F1806, the "Det" routines with explicit
texture coordinates up to v = 0x1ff and a 22-entry slot remap at an offset):
the planet/surface renderer, not model faces. Descriptor record 63 also names
94 (64-stride, v 0..63, first 64 rows). Block 95: no literal user found in the
face path or F1840/F1841; **unknown**.

## 6. Cache v8

The descriptor table and `DATA_007770` are imported from floppy 1.02 (RVAs
`0x102da8`, `0x101b10`, byte-identical to JJFFE). The CD 1.01 executable was not
available to check, so its v8 cache omits both (stated in `unresolved`); the
viewer then keeps the grey stand-in. See `docs/FORMATS.md`.

## 7. F6 viewer

Textured faces now use the original path rule, entry colours and rasteriser with
the real bitmaps. Diagnostic parts, not the original: the camera, light and the
normal shades (`2 * (7 - i)`, up to 14, so textured faces often clamp towards 15),
and 12-bit colours outside `DATA_007770` (shown as nibble x 4 in six-bit VGA
units; the original allocates them at run time). Unwritten slots (texel 7+) are
drawn black. Faces on the flat path keep the viewer's flat colour mapping, now
with the F1693/F1691 colour for textured materials.

## 8. Unknown / not covered

- Runtime upper-palette allocation (F1326 request flags, F1329 and its callers).
- Real normal shading values (the oracle used synthetic normal colours).
- Faces whose normal 0 colour is rewritten by F1762 mid-program are skipped by
  the queue oracle (per-face timing is not carried by the emissions).
- F1806/F1808 (explicit-coordinate textures) and their callers F1840/F1841.
- Block 95's user.
- 21 model slots crash the original in the four synthetic views and are not
  compared (listed by `verify-textures.py`).
