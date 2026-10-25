# Visual regressions after the palette and fills merges

Frames from this checkout (`cargo build`, cache v28 `data.bin`). "Before" was
captured on `main` before this branch's fixes; "after" is the same command on
this branch. The original frames come from the local JJFFE smoke adapter
(`tools/reference/build-smoke.ps1`).

| Pair | Before | After |
|---|---|---|
| Intro model 176 at `0x181534` (planet + ship + galaxy) | `intro-planet-before.png` | `intro-planet-after.png` |
| Model 177's GAME!TEK letters at animation 100 (mirrored before) | `game-tek-letters-before.png` | `game-tek-letters-after.png` |
| GAME!TEK word at animation 200 | `intro-game-tek-before.png` | `intro-game-tek-after.png` |
| PRESENTS at animation 248 | `intro-presents-before.png` | `intro-presents-after.png` |
| DAVID BRABEN'S at animation 328 | `intro-david-before.png` | `intro-david-after.png` |
| Intro ship/planet depth order at `0x181534` | `intro-ship-order-before.png` | `intro-ship-order-after.png` |
| Earth approach at `--capture-planet` frame 1000 | `planet-approach-before.png` | `planet-approach-after.png` |

The intro frame's 128 upper palette entries after the fix are in
`intro-palette-after.txt` (six-bit RGB, bank order: entry 0 is video 0x80);
the per-frame colour count and run statistics are in `intro-colour-stats.txt`.

Commands:

```text
ffe-rs --root . --capture-intro out/intro
ffe-rs --root . --start sol --capture-planet out/planet
jjffe-smoke --intro-live 1516        # original at the same credit frame
jjffe-smoke --intro-live 65          # original GAME!TEK orientation (animation 64)
```

The word frames are logical 320x200 captures at `animation << 10`; the
"before" images remove only F1747's first-row negation, so the comparison
isolates the handedness fix. The ship-order pair is `--capture-intro` at
`0x181534`, before and after the curved surfaces join the depth queue.

## What changed

* **Intro planets.** Model 176's scene is a tree of children; model 349's
  surface child (449) is the big planet. One failing descendant (model 456 ->
  321, a frame-mode-6 call whose basis the original reads from stale stack) used
  to make `execute_scene` discard the *whole* scene. A failed child is now
  reported in its own outcome and its siblings still run, and child curved
  surfaces are collected (they stay in the child `Execution`, not the root).
* **Surface colours.** The surface queue's F1326 colours are registered before
  the build. `F1786` clears the request cube once per frame, so each build is
  cut around the colours that frame queues. The previous bank still claims its
  slots, so the planet and galaxy keep stable indices. Frames 1514 and 1515 of
  the executed original have identical upper palettes.
* **Median-cut heap.** `F1330`'s heap key is `F1329`'s longest extent, and the
  shrunken root sinks down the larger-child path before the new right box sifts
  up. The port used the cell count and split `heap[1]` repeatedly, leaving most
  of the 64 boxes empty; their `31,31,31` representative filled the previous
  bank and speckled the band texture.
* **Shape detail.** The curved-surface patch is generated at `DATA_008812`, the
  Graphic Detail "Level of Shape Detail" row less one. The executed boot (and
  the in-game default) is 1. The old hard-coded 2 over-subdivided the intro's
  polar banded planet, so texture block 95 was minified into per-texel noise.
* **GAME!TEK.** The logo is model 177 -> child 361: one slab face plus text
  `0x408a`. The port drew the slab but not the letters' colour, leaving black
  holes; the text colour is resolved from the text material (F1692/F1693).
  F1747 also negates the text frame's first row when F1740 leaves a left-handed
  frame (`cmp [..+0xf8],0; jnl`); without it every glyph was mirrored. The fix
  is the shared `model_math::text_frame` used by all three word children, so
  GAME!TEK, PRESENTS and DAVID BRABEN'S are no longer mirrored. There is no
  second logo draw path. The executed reference's model string is `FRONTIER`
  (`intro-todo.md` D5); the port keeps the DOS cache's `GAME!TEK`.
* **Depth order.** F1840/F1841 queue their curved-surface entries into the same
  F1778/F1788 list as the model faces, keyed by F1777. The port drew every
  surface after every primitive, so the intro planet painted over the nearer
  ship. `Frame::keys` now carries each shape's key and `merge_surface_shapes`
  inserts the surface entries in the descending-key order. The old and new
  frames are `intro-ship-order-before.png` / `intro-ship-order-after.png`.
* **Atmosphere ring colours.** F1879's ring is an F1805 textured quad (bitmap
  0x43 and seven slot colours) plus an F1804 flat quad. The port approximated
  the textured quad with two Gouraud triangles, which invented 268 RGB values
  on the planet's limb that no palette entry held. The ring entry now uses the
  regular F1805/F1647 texture path and its slot colours are registered with the
  frame, so the frame has 71 distinct colours like the executed original (was
  330). See `intro-colour-stats.txt`.
* **Child-model oracle.** A model whose child now fails is skipped by
  `verify-child-models.py`: the original would execute it against whatever its
  stale stack held, so there is no source-correct sibling-preserving trace to
  compare. The skipped subtrees are the captured `Failed` outcomes, not a
  changed compare rule.
