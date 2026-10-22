# Thruster glows (opcode 9, F1738 / F1704)

Status: **ported and verified** (2026-10-22, second attempt). The first attempt
(recorded below under "History") was withdrawn after a screen-position
difference; that difference does not reproduce with the current port, and the
open questions it left are answered in "The screen-position question".

Rust: `ffe-core::model_thruster` (F1738 after vertex resolution, F1666, F1704 /
F1705 / F1561-F1563 / F1715-F1716, the colour ramp), `model_execute` opcode 9
(`ThrusterEmission::{entries, screens, width}`, `Primitive::Thruster`,
`QueueEvent::{Thruster, ThrusterLine}`), F6 viewer (`Shape::Glow`,
`ModelViewer::set_thrust`). Oracles: `tools/reference/verify-thrusters.py`
(`crates/ffe-rs/tests/thruster_reference.rs`), plus the thruster events in
`verify-station-models.py` and `verify-child-models.py` (shared encodings in
`crates/ffe-rs/tests/common/mod.rs`).

## The original (F1738), read from the assembly

Record (8 bytes): word +0 opcode, word +2 material, byte +4 vertex A, byte +5
vertex B (both sign-extended: 0x80.. are external references), signed word +6
width. The handler returns record + 8.

1. **Colour.** `F1692(material)` -> `F1694(instance, ...)` -> `F1691`. F1694
   clears the unlit flag and adds the instance colour (+0xf4..+0xf6) for a local
   material (bit 12); it adds **no normal colour**. A textured material (bits
   14-15) has RGB 0. F1691 then adds the texture descriptor's first colour
   (`DATA_007812 + 30 * texture`) - that sum (F1691) is used **only** for the
   narrow line; the glow ramp is built from F1694's bytes.
2. **Vertex A** through the callers' inline cache test (stamp == +0xf0 uses the
   entry as it is, otherwise F1471 projects and stamps). If `A.view_z <= 0x200`
   (signed) return: nothing queued, **B is not resolved**.
3. **Vertex B** the same way; `B.view_z <= 0x200` returns.
4. **Width** `w = word +6` (sign-extended), shifted by `s = instance[+0x148] - 6`
   (the instance scale: descriptor +0x14 at top level, F1750's child scale in a
   child): `s >= 0` -> `shl w, s`, else `sar w, -s` (x86 masks both counts to
   5 bits). Then `w = F1666(w, A.view_z)`.
5. **F1666** (`SpecialDiv(v, d)`): while `d > 0x7fff` halve both (`sar`);
   `d == 0` returns `v`; else `(v << 8) / d` (`idiv`, truncation toward zero).
6. `w == 0` returns. `w <= 4` (signed: **negative widths too**) calls
   `F1704(instance, A, B, F1691 colour)` and returns.
7. Otherwise `dx = A.sx - B.sx`, `dy = A.sy - B.sy` (16-bit screen words
   sign-extended), `L = F1467(dx*dx + dy*dy)` (32-bit wrapping sum, the table
   square root - L is the screen length in pixels).
   - `L < w`: **core** entry (below). Then if `8L < w` (wrapping shift, signed
     compare) return - core only.
   - **shaft** entry (below) - reached when `L >= w` (shaft only) or
     `w <= 8L` after a core (both).
8. **Queue entries.** Both glows: key `F1777(instance, A.view)`, allocation
   `F1778(13 dwords, key, 0)` (so inside an open depth group the key is ignored,
   like every primitive), word 0 = `F1805`, words 1..4 the four screen points
   (`x | y << 16`, 16-bit wrapping arithmetic), word 5 the texture word (0x123
   core / 0x65 shaft), word 6 **not written** (stale queue memory), words 7..12
   the colour ramp.
   - core: `m = ((B.s + 2 A.s) / 3)` per axis (idiv by 3, truncating), `q = w >> 2`,
     `n = (-w) >> 2` (both arithmetic, so `n` is not `-q` for widths not divisible
     by 4): points `(mx+n, my+q)`, `(mx+q, my+q)`, `(mx-n, my-q)`, `(mx-q, my-q)`.
   - shaft: `nx = (dx << 15) / L`, `ny = (dy << 15) / L` truncated to 16 bits;
     `px = (ny * w) >> 17`, `py = -(nx * w) >> 17` (32-bit products, arithmetic
     shift): points `A + p`, `B + p`, `B - p`, `A - p`. L is at least `w / 8 > 0`
     here, so the divisions cannot fault.
   - ramp: slot `i = 1..6` is `(c * (i + 1)) / 7` per F1694 byte `c`, packed by
     F1326 (`r << 8 + g << 4 + b`, which also flags upper-palette colours).
9. **F1704** (the narrow line; also F1703's line queueing): copies both screen
   dwords. Both on screen (`x <u 0x140`, `y <u 0x9e`): queue. One off screen:
   F1705 clips it toward the other; queued if that succeeds. Both off: F1715 /
   F1716 outcodes (`-32766` -> 8, on -> 2, negative -> 1, else 4) OR-ed per axis
   reject 1, 4 or 8 (`DATA_007818`); then F1705 on A, then on B, either failing
   rejects. Queue: key `F1777(A.view)`, `F1778(6, key, 0)`, word 0 = `F1803`,
   words 1..4 = `x0 > 0 ? x0 - 1 : 0`, `y0`, `x1 > 0 ? x1 - 1 : 0`, `y1`, word 5
   F1326 of the colour.
   - **F1705** (point, this entry, other entry): `this.z < 0x40` -> (`other.z <
     0x40` rejects) the point becomes F1564's near-plane crossing (the polygon
     helper, `model_polygon::near_point`); then F1561 = F1562 (x, limit 0x13f)
     and F1563 (y, limit 0x9d) move it along the line to the *other entry's*
     screen point onto the edge it lies beyond (`(p.y - o.y) * (edge - o.x) /
     (p.x - o.x) + o.y`, 32-bit product, 16-bit sum; skipped when `p.x == o.x`);
     rejected unless the result is on screen.

**F1805** (the draw routine) maps the seven colour words +0x18.. through F1327
into `DATA_009252[0..7]` and calls **F1647 DrawTexQuad**(p0..p3, texture word):
the glow is an ordinary textured quad. Bitmap 0x123 (secondary 195) is a round
blob and 0x65 (secondary 5) a flame tapering from u = 0; both are 64x64, use
texels 0..6 and have the transparent-zero flag, so slot 0 - the word F1738
never writes - is never drawn. Orientation 0 maps the points to corners
`[0, 1, 3, 2]`, so the shaft's bright end (u = 0) is at A. The rasteriser is the
already verified `model_texture::draw_quad` (4,096 cases, `docs/notes/textures.md`).
**F1803** draws its two points with F1616 (DrawLine), not ported.

## What controls a thruster's size (thrust)

- The **width word is a literal**; the glow width also follows the instance
  scale (+0x148) and A's depth. No variable feeds it.
- The **plume length** is vertex B. In the ship models B is a **curve vertex
  (vertex opcode 19)** interpolating from A (P0) to a **jittered vertex (opcode
  7)** (P1, an F1476 random offset of a fixed point, so the plume flickers),
  with `t` from **local 1**. Example, model 30: A 30 `[1,0,4,0,-17]`, B 36
  `[19,0,0xc1,30,34]`, 34 `[7,0,21,0,32]`, 32 `[1,0,4,0,-63]`.
- The ship models with thrusters (19-70, 180, 191, 193, 201, 215, 222 and
  others; 3 and 220 use a variant with further locals, and 224 runs its first
  block only when bit 15 *is* set, with `local1 = 0 - global15`) begin their thruster
  block with the same preamble, which reads **global word 15**:

  ```
  op20 [0x8f, 16]   skip unless bit 15 of global 15 is clear (negative thrust: no glow)
  op19 [0x8f, 0]    skip if global 15 == 0
  op29 local1 = min(global15, 4095)      (op 7, unsigned, literal 4 << 10)
  op29 local1 = local1 << 4
  op29 local1 = local1 < 5120 ? 0 : local1   (op 11)
  op19 [0xc1, 0]    skip if local 1 == 0
  ```

  Global word `i` is the word at `[instance + 0x14c] + 0x9c + 2 i` (F1473,
  `movzx`), and +0x14c is the object itself (F1683), so **global 15 is object
  word +0xba**. F598 (and F600) store a three-word vector at +0xb6/+0xb8/+0xba
  after clamping each axis to the ship's limits at +0xbc..+0xc6 (z: the input
  `<< 3`); F938 sums the absolute values of the three words. Global 15 is
  therefore the **z (forward) component of the ship's thrust vector**: glows
  appear for 320..=0x7fff and lengthen up to 4095 (local 1 = 65520).
  Globals 13/14 (+0xb6/+0xb8) were not seen in a thruster preamble. Some other
  thruster blocks sit behind visibility branches (opcode 11/12) or other locals.
- Models 102, 311, 375, 429 (no preamble) run their thrusters unconditionally;
  311's look like fires (B = a jittered point above A).

The viewer exposes this as `ModelViewer::set_thrust(Some(value))`
(`THRUST_GLOBAL = 15`); `None` leaves global 15 to the animation input.

## The screen-position question (first attempt)

The first attempt reported glow points that differed from the original's for
model 311 while the keys matched, with two candidate causes. Findings:

1. **F1471 with a non-zero, non-current stamp** recomputes the view from the
   entry's object coordinate. This cannot hit a screen-space midpoint: F1478's
   cases 6 and 11 stamp the entry with the instance generation (+0xf0), so
   F1738's inline test uses it as it is. It applies only to entries stamped 1
   by F1472 (dependencies), whose object coordinate *is* written - the port
   does the same.
2. **Midpoints average the screens their sources already hold.** True, and
   order-dependent; the port reproduces the order. In `--station-model` the
   face handlers are replaced by recorders and do not project, while the port's
   faces do, but no thruster vertex in the table depends on that.

With the current port both whole-table comparisons match every thruster entry
(station-model mode 25 glows; child-model mode at z 2000: 163 glows and 15
lines; at z 0x50000: 210 glows and 17 lines), model 311 included. The
withdrawn attempt's difference is not reproduced and was not in the vertices;
its code was not kept, so its exact cause cannot be re-checked.

## Verification

Adapter (`tools/reference/headless.c`, probe hooks in `make-probe.py`):
scoped hooks in F1738..F1739 (F1777 x2, F1778 x2, F1326 x2, the F1704 call) and
F1704..F1705 (F1778, F1326). New exports `_ref_thruster` (F1738) and
`_ref_line_queue` (F1704). Nested events:

| Event | Fields |
|---|---|
| 41 | glow key |
| 42 | points 0, 1, 2 (`x | y << 16`) |
| 43 | point 3, texture word, 0 |
| 44 / 45 | ramp slots 1..3 / 4..6 as F1326 receives them (`r | g << 8 | b << 16`) |
| 46 | F1704 call from F1738: A screen, B screen, F1691 colour |
| 47 | line key |
| 48 | line entry points (stored x and y per end) |

Queue events 7 (glow, F1805) and 8 (line, F1803). `--child-model N Z` places
the model at z `Z` (default 2000; the stdout of the default is unchanged).
`--thruster-cases` calls F1738 directly on 8192 synthetic instances (both
entries stamped with the generation, scale 0..31, key scale -4..8, widths from
small to full 16-bit, depths on both sides of 0x200 up to 0x7fffffff, screens
coincident to far apart and off screen, sentinel points, local and textured
materials) and F1704 on 4096 entry pairs (depths both sides of 0x40).

Results (floppy 1.02, Linux reference, v10 cache built from this branch):

- `verify-thrusters.py`: **12,288 cases, 0 mismatched** (1,271 cores, 660
  shafts, 3,133 lines queued, 2,088 direct lines rejected, 5,351 thrusters that
  queue nothing); repeat identical; tampered key, glow point, texture word, ramp
  colour, line colour, line point and queue key, and the empty trace, rejected.
  Trace SHA-256 `5a8eddfef857882db96d935d5fca41cb6f4a53ea2c6c9a984e91c6f5c01f4160`.
- `verify-station-models.py`: 448 compared, 0 mismatched, 25 thruster glows.
  Trace `2fa662e416d61c698d9d5469ba82626af59dc4cc30e3aa2c8d0c10dda25ecaa1`
  (was `34c1bf26…`: the trace now carries the thruster events 41-45).
- `verify-child-models.py`: 440 compared, 0 mismatched, 163 glow and 15 line
  entries (23 F1704 calls), queue log included; tampers now include events 42,
  45 and 48. With `--depth 0x50000`: 437 compared, 0 mismatched, 210 glows, 17
  lines (8 models do not execute natively at that depth, all explicit errors).
  The child trace hash still moves with each build (event 26).
- `verify-all.py` now also runs `verify-thrusters.py` and the deep child run:
  **all 28 oracles pass** (Linux reference, `.exe` symlinks to the Linux
  binaries). Unchanged traces: station-queue `350b6d2c…`, textures
  `640cc781…`/`65922b3c…`, vertex-programs `8609c46e…` (the same hash with the
  main checkout's reference build, so not changed by these hooks; the
  `1afe0c45…` recorded for v0.42 is an older value).

Synthetic tests: `crates/ffe-core/tests/model_thruster.rs` (5: F1666 and the
width shift, colour ramp, F1704 clipping and the narrow line, core/shaft/both
geometry and the depth gate, executor ordering and cache side effects),
`model_execute.rs` (the existing thruster test now checks the zero width),
`crates/ffe-rs/tests/model_viewer.rs` (thrust global gates the glow).

## Viewer

`Primitive::Thruster` glows become `Shape::Glow` (drawn exactly like
`Shape::Textured`: the F1647 rasteriser over the real bitmap with the F1326 /
fixed-palette slot colours) and are left out of the viewer's auto-fit so the
model keeps its size as the plume grows. Narrow thrusters draw as lines with
the F1691 colour. The viewer now passes the model scale as the instance's
+0x148 (`operand_scale`), as F1683 does; it was 0, which made glows 2^scale
too narrow (it also affects depth-offset keys and polygon circles, now as in
the original).

## Not done

- F1616 line rasterisation (F1803) and pixel-level comparison of the glows
  (the quad rasteriser itself is verified separately).
- F1326's upper-palette request flag and the runtime palette (shared gap).
- Which value real flight puts in global 15: the flight slice is synthetic.
- Thrusters drawn by other code paths (e.g. the player's own ship view) were
  not looked for.

## History (first attempt, withdrawn)

The first reading of F1738 (steps above) was correct; its `--thruster N` mode
(z 0x50000) found 7 slots reaching thrusters and a screen-point difference that
led to the withdrawal. It used event kinds 27-29, which are now superseded by
41-48 (30-33 are the polygon events).
