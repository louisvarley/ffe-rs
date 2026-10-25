# Intro rendering — findings and fix tasks

> Hand-off for another agent. This documents the three reported intro bugs
> (mirrored/vertically-flipped text, colour drift, ship clipping behind the
> planet) plus every other issue found while reviewing the intro path, with the
> probable root cause and the fix for each.
>
> Everything below was read from the current tree (`main` @ `bc77ab4`). The
> intro renderer is `crates/ffe-rs/src/screen.rs::draw_intro` +
> `crates/ffe-rs/src/model_viewer.rs` (model/scene execution) +
> `crates/ffe-rs/src/intro_text.rs` (bitmap credits) +
> `crates/ffe-rs/src/palette.rs` / `textures.rs` (runtime upper palette). The
> original source of truth is the DOS 1.02 `game/FIRSTENC.EXE`; the executable
> oracle is `jjffe-smoke` built from `jjffe-master` (see `docs/notes/intro.md`
> "Boot drawing and timing", `docs/notes/palette.md`, `docs/notes/fills.md`,
> `docs/notes/text.md`, `docs/notes/child-models.md`).

---

## 1. The spaceship clips behind the planet (depth / painter's order)

**Root cause — confirmed.** In `model_viewer.rs::render_matrix` the draw order
is fixed and wrong:

```rust
// model_viewer.rs:1284-1288
self.draw_primitives(&result, &mut frame)?;      // root + child faces (the ship,
                                                 // asteroids) in depth order
for run in &text_runs {
    self.draw_primitives(run, &mut frame)?;      // glyph text
}
self.draw_surface_batches(&surfaces, &mut frame); // the planet's curved surfaces,
                                                 // ALWAYS drawn last
```

The planet is not a face model: it is a **curved surface** (opcode 31, model
449) that lives in the child executions and is collected separately into
`SurfaceBatch` by `collect_surfaces` (lines 1390-1512). `draw_surface_batches`
(1581-1675) runs **after** all primitives, so the planet always paints over the
ship, no matter which is nearer. The original (`F1772`) interleaves the surface
entries into the same depth-ordered queue as the faces, so the nearer object
wins.

The surfaces are *internally* sorted far-to-near, but only among themselves:

```rust
// model_viewer.rs:1505-1506
let mut order = queue.headers();
order.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
```

They are never merged with `result.ordered` / `result.ordered_keys` (the
primitives' aligned `F1777` depth keys, `model_execute.rs:306-308`).

**Fix (probable).**
1. Generate each surface's shapes together with its `F1777` key (the key is
   already in `SurfaceBatch::order`, `(key, word_index)`).
2. Merge the surface `(key, Shape)` items with the primitive
   `(ordered_keys[i], Shape)` items into one list, sort by key (same
   ascending/descending convention the `Order` sorter already uses), and draw in
   that merged order instead of the three separate passes above.
3. Because surfaces and faces now share one queue, the ship and planet resolve
   correctly by depth. Text (`text_runs`) should join the same merge as well —
   the original queues glyph geometry into the same order (see §4.4).
4. Keep the far-to-near surface ordering within a single surface batch
   (`b.0.cmp`) intact; only the interleave between batches and primitives is new.

Regression tests: `crates/ffe-rs/tests/intro_view.rs` currently only checks the
planet/ship are *present* and the palette is stable; add a test (or extend the
`--capture-intro` comparison) that asserts the ship pixels are drawn **over**
the planet when the ship is nearer, e.g. compare `fffe-rs --capture-intro` at
`0x181534` with `jjffe-smoke --intro-live 1515`.

---

## 2. Background / space / planets change colour through the intro

The intro clears to F961's `F1575` colour and draws the model-176 scene through
the **runtime upper palette** (`F1326`/`F1327`/`F1330`/`F1331`,
`palette.rs`). The palette is rebuilt every frame from a 64-box median cut of
the colours that frame registers (`textures.rs::build_palette`). Several things
make that reconstruction drift or diverge from the original:

### 2a. The median cut re-partitions as the animation moves (drift mechanism)

- The model-176 scene is animated (global 0 = `tics >> 10`), so the set of
  registered colours (visible ship/asteroid faces, star points, surface ramp
  colours) changes frame to frame. With >64 occupied cells the builder
  median-cuts the whole occupied extent (`palette.rs::make_boxes`), so the
  representative for *every* cell — including the background `(0,0,4)` cell and
  the planet's ramp cells — is re-derived from the current colour set.
- The cube is now cleared per frame (`model_viewer.rs:1248`,
  `textures.rs::clear_palette_cube`), which fixes the *accumulation* drift that
  turned the planet/galaxy into noise after ~1500 frames (see
  `docs/evidence/visuals/README.md`). But the **per-frame re-cut** still moves
  the shared representatives whenever the visible colour set changes, so the
  background and planet tint shift as the intro animates. The original produces
  identical representatives on consecutive frames only because its colour set is
  stable across those frames; the port must match the original's *set*, not just
  clear the cube.

### 2b. Dot (galaxy star) colours are not registered before the build

`register_execution_colours` registers a point's material colour
(`model_viewer.rs:1723-1728`) but **not** the halo/core colours that
`draw_primitives` actually computes for an unlit point via `point_dot`
(lines 1863-1917). Those halo/core `[u8;3]` colours are resolved afterwards with
`self.rgb(...)`, so they fall back to the pre-palette nibble×4 approximation
(`textures.rs:94-115`) instead of the original's quantised palette entry. This
makes the intro's galaxy/starfield colours wrong (and it is a real bug in its own
right), contributing to "space changes colour".

**Fix (probable).**
- Port the dot halo/core colour words into `register_execution_colours` so they
  are marked in the cube before `prepare_palette()` (line 1280). Concretely,
  factor the `point_dot(...)` colour computation (halo/core) so it can be run
  *before* the build to register the two `pack(...) & 0xfff` words, then reuse
  the same values when drawing.
- Compare the intro's upper palette against the executed original at several
  tics (`jjffe-smoke --intro-live`), not just `MODEL_SWITCH`. `docs/notes/palette.md`
  §5 already has a small table; extend it across the full animation range and
  assert the port's `upper6()`/background matches. `tests/intro_view.rs:92`
  (`the_intro_planet_is_drawn_and_the_palette_is_stable`) only checks
  determinism of one tic, not agreement with the original's palette.
- Record the remaining "one build per model render vs one per scene frame"
  difference (`docs/notes/palette.md` §6) if it still shows a divergence after
  the dot fix; for the intro (one model) it should be exact once the colour set
  matches.

---

## 3. Text at the start is mirrored (vertically flipped)

**Symptom (owner):** the logo text at the start of the intro is mirrored, and a
**vertical flip** is the needed correction. This is the 3D glyph text — model
177 → child 361's `GAME!TEK` / `PRESENTS` / `DAVID BRABEN'S` (opcode 10,
`F1747`), drawn by `collect_texts` in `model_viewer.rs`, **not** the bitmap
credits in `intro_text.rs` (those are the verified main font and are fine).

**What is already correct** (so the fix does not re-break these):
- The text record decode (`model_commands.rs:441-459`): `vertex` = payload[4],
  `frame` = word at +4 (bytes 4-5, low byte = position vertex, high byte =
  orientation), `string` = word at +6. Verified (v0.14).
- `frame_rows` (`model_math.rs:231-261`) = `F1739`: permutation `word>>8&7`,
  negation bits `0x2000/0x1000/0x800` on source rows 0/1/2, odd permutations and
  negations flip handedness. This is the same routine the verified child-call
  path uses.
- The projection (`model_instance.rs::project`) = `F1479`, shared with all
  geometry, so a pure Y-flip of *only* text is **not** the projection.

**What to verify first (diagnostic), then fix.** The three things that could
produce a text-only vertical flip are, in order of likelihood:

1. **The glyph frame's handedness is discarded** in `collect_texts` — the one
   *code* difference between text frames and the verified child-call frames.
   `frame_rows` is correct (it returns the same `matrix` the child path uses),
   but the handedness is dropped, and the glyph `VertexContext` is then built
   with `..Default::default()` so `base_handedness`/`handedness` are `0`:

   ```rust
   // model_viewer.rs:2084-2095  (`.0` drops the handedness)
   let matrix = if word >> 8 & 7 == 6 {
       emission.context.basis
   } else {
       frame_rows(&emission.context.matrix, emission.context.base_handedness, word)
           .map_err(...)?
           .0
   };
   // model_viewer.rs:2111-2119  (glyph context, defaulted handedness)
   let mut glyph_context = VertexContext { matrix, basis: matrix, scale: size, ..Default::default() };
   ```

   The verified child-call path keeps it:
   ```rust
   // model_execute.rs:1799-1812
   let (matrix, base_handedness, base_handedness_defined) = if word >> 8 & 7 == 6 {
       (parent.basis, parent.handedness, defined.handedness)
   } else {
       let (matrix, handedness) = frame_rows(&parent.matrix, parent.base_handedness, word)?;
       (matrix, handedness, defined.base_handedness)
   };
   ```

   Because glyph faces are currently drawn with *all normals visible and unlit*
   (`text.md` "Glyph lighting"), this handedness gap is latent (it does not yet
   change culling), but it is the text-specific deviation to close first, and it
   becomes load-bearing once glyph normals are evaluated.

2. **Glyph normal evaluation is still missing** (`text.md` "Not done"; also
   `PORT_LOG.md:1419-1423`). The font model's normal block is not evaluated in
   the text frame, so every glyph face is treated as front-facing/unlit. When a
   text frame is mirrored (odd permutation or a row negation in its frame word),
   this is the most plausible place a Y reflection is being lost.

3. **The font glyph programs' own Y convention.** The frame word, decode and
   projection are all verified correct (see above), so if 1-2 do not fix it, the
   remaining suspect is that the font glyphs' object-space Y is the opposite of
   what the text frame assumes, and the original compensates somewhere the port
   does not. Confirm this against the original, not by guessing.

**Fix (probable), in order:**
1. Keep the handedness from `frame_rows` in `collect_texts` and set the glyph
   `VertexContext::{base_handedness, handedness}` from it (mirror the child-call
   logic for mode 6 too: `basis` + `parent.handedness`).
2. Evaluate the font model's normal block in the text frame (implement the
   missing glyph lighting) instead of filling every normal "visible/unlit".
3. Diagnose against the original: run `ffe-rs --root . --capture-intro out` and
   `jjffe-smoke --intro-live 62` (original GAME!TEK letters, animation 63) and
   compare the letter bounding-box orientation. Only after that, if a Y
   correction is genuinely needed, apply it **inside the glyph/text frame setup**
   (e.g. where the glyph `matrix`/`basis` is assembled), never in the shared
   `project()`.

**Do not** put a blanket Y-negation into the shared projection: it would flip the
ship, planet and station too. The flip must stay inside the glyph/text frame
setup.

---

## 4. Other issues found in the intro path

### 4.1 Text drawn after the hull, not depth-interleaved (known, affects overlap)
`render_matrix` draws `text_runs` after `draw_primitives` (lines 1285-1287).
The original queues glyph geometry into the same `F1772` depth order as the
hull (`text.md` "Depth interleaving"). When merged with §1's surface fix, text
should join the single depth-ordered pass too.

### 4.2 Glyph lighting not implemented
`collect_texts` fills all glyph normals "visible / unlit"
(`model_viewer.rs:2153-2161`), so mirrored/back-facing glyph strokes are never
culled and lighting is flat. This is the other half of §3 and is the likely
co-cause of the mirrored look.

### 4.3 `0x3022` wrap not wired into model text
`expanded_text` (`model_viewer.rs:2232-2246`) expands `0x3016`/`0x3022`
unwrapped. `F1398`'s word wrap needs the 256-byte `DATA_007648` width table,
which is now imported as `base:text/char-widths` (used by the intro credits) but
not passed into the model-text path. Not currently on the intro logo, but it is
the same subsystem and should share the width table.

### 4.4 Control codes below 0x20 stop the string
`collect_texts` treats a byte `< 0x20` as a newline and stops the record in
`Frame::skipped` if the font's component anchors are missing
(`model_viewer.rs:2170-2190`). The multi-line logo/station strings are affected
if any model uses them.

### 4.5 `0x9c00`-`0x9c03` (planet/system names) unresolved
`expanded_text` rejects them (`model_viewer.rs:2244`). Not used by the intro
logo, but the same text table.

### 4.6 Background clear colour is re-derived from the palette each frame
`draw_intro` resolves the clear colour via `viewer.palette_rgb(background)`
(`screen.rs:1505-1508`) after the model already built the palette. This is
correct only if the palette build is stable; it is a downstream consumer of §2,
not an independent bug, but worth asserting against `F1330`'s `DATA_009222`.

---

## 5. Verification plan for the fixing agent

1. Build and run the capture: `ffe-rs --root . --capture-intro out` (writes
   `intro-*.ppm` at the sample tics). Build the original reference once:
   `tools/reference/build-smoke.ps1 -Nasm .\.cache\tools\nasm-3.01\nasm.exe`.
2. Compare each `intro-<tics>.ppm` against `jjffe-smoke --intro-live <frames>`
   (see `docs/evidence/visuals/README.md` for the exact commands). The original
   `--intro-live` is the pixel/palette oracle.
3. For §1, assert ship-over-planet at `0x181534` (ship nearer) and
   planet-over-ship where the planet is nearer.
4. For §2, extend `docs/notes/palette.md` §5's upper-palette table across the
   animation range and assert `upper6()` + background equality, not just
   determinism.
5. For §3, diff the GAME!TEK letters' bounding-box orientation at animation 63.
6. Re-run the local suite: `python tools/reference/verify-all.py`, plus
   `cargo test -p ffe-rs --test intro_view -- --ignored` (with
   `FFE_INTRO_CACHE=<v28 data.bin>`), and `cargo fmt --check`,
   `cargo clippy --workspace --all-targets --locked -- -D warnings`,
   `cargo test --workspace --locked`.

---

## 6. Source map

| Piece | Original (DOS / JJFFE) | Port |
|---|---|---|
| F961 model/bitmap/animation/loop | `game/FIRSTENC.EXE` `0x2f298` / `ffe.asm:71649` | `front_end::IntroTimeline`, `screen::draw_intro` |
| F962 credits + V | `0x2f380` / `ffe.asm:71724` | `front_end::INTRO_LINES`, `intro_text.rs` |
| F959 object (identity 0x7f000000 rows, origin `(0,0,0x14820)`, model 0xb0) | `0x2e9d8` / `ffe.asm:71516` | `screen::draw_intro` rows |
| F1682/F1683 display instance | `0x5b678`/`0x5b69c` / `ffe.asm:102201/102221` | `model_viewer::render_scene_with` |
| F1747 text / F1740 frame / F1739 | `ffe.asm` (see `docs/notes/text.md`, `child-models.md`) | `model_viewer::collect_texts`, `model_math::frame_rows` |
| F1326/F1327/F1330/F1331 palette | `ffe.asm:82761-83427` | `palette.rs`, `textures.rs` |
| F1772 depth order | — | `model_execute.rs` `Execution::{ordered, ordered_keys}` |
| Curved surface (planet, opcode 31) | `F1876`/`F1840` | `model_viewer::{collect_surfaces, draw_surface_batches}` |
| Primary bitmap 0x42 strip | `0x2f3a0` / `ffe.asm:71738` | `screen::draw_intro` `intro_bitmap` |
