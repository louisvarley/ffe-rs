# Visual regressions after the palette and fills merges

Frames from this checkout (`cargo build`, cache v28 `data.bin`). "Before" was
captured on `main` before this branch's fixes; "after" is the same command on
this branch. The original frames come from the local JJFFE smoke adapter
(`tools/reference/build-smoke.ps1`).

| Pair | Before | After |
|---|---|---|
| Intro model 176 at `0x181534` (planet + ship + galaxy) | `intro-planet-before.png` | `intro-planet-after.png` |
| Model 177's GAME!TEK letters at animation 100 (mirrored before) | `game-tek-letters-before.png` | `game-tek-letters-after.png` |
| Earth approach at `--capture-planet` frame 1000 | `planet-approach-before.png` | `planet-approach-after.png` |

The intro frame's 128 upper palette entries after the fix are in
`intro-palette-after.txt` (six-bit RGB, bank order: entry 0 is video 0x80).

Commands:

```text
ffe-rs --root . --capture-intro out/intro
ffe-rs --root . --start sol --capture-planet out/planet
jjffe-smoke --intro-live 1516        # original at the same credit frame
jjffe-smoke --intro-live 65          # original GAME!TEK orientation (animation 64)
```

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
  frame; without it every glyph was mirrored (`GAME` read `ƎMAפ`). There is no
  second logo draw path. The executed reference's model string is `FRONTIER`
  (`intro-todo.md` D5); the port keeps the DOS cache's `GAME!TEK`.
* **Child-model oracle.** A model whose child now fails is skipped by
  `verify-child-models.py`: the original would execute it against whatever its
  stale stack held, so there is no source-correct sibling-preserving trace to
  compare. The skipped subtrees are the captured `Failed` outcomes, not a
  changed compare rule.
