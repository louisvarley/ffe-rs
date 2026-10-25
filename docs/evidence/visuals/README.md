# Visual regressions after the palette and fills merges

Frames from this checkout (`cargo build`, cache v28 `data.bin`). "Before" was
captured on `main` before this branch's fixes; "after" is the same command on
this branch. The original frames come from the local JJFFE smoke adapter
(`tools/reference/build-smoke.ps1`).

| Pair | Before | After |
|---|---|---|
| Intro model 176 at `0x181534` (planet + ship + galaxy) | `intro-planet-before.png` | `intro-planet-after.png` |
| Model 177's GAME!TEK letters at `0x10000` (crop x 40..320, 4x) | `game-tek-letters-before.png` | `game-tek-letters-after.png` |
| Earth approach at `--capture-planet` frame 1000 | `planet-approach-before.png` | `planet-approach-after.png` |

Commands:

```text
ffe-rs --root . --capture-intro out/intro
ffe-rs --root . --start sol --capture-planet out/planet
jjffe-smoke --intro-live 1515        # original at the same tics as 0x181534
jjffe-smoke --intro-live 62          # original GAME!TEK letters (animation 63)
```

## What changed

* **Intro planets.** Model 176's scene is a tree of children; model 349's
  surface child (449) is the big planet. One failing descendant (model 456 ->
  321, a frame-mode-6 call whose basis the original reads from stale stack) used
  to make `execute_scene` discard the *whole* scene. A failed child is now
  reported in its own outcome and its siblings still run, and child curved
  surfaces are collected (they stay in the child `Execution`, not the root).
* **Surface colours.** The surface queue's F1326 colours are registered before
  the build, and the request cube is cleared at the start of each frame's
  registration. Keeping the built cells across frames re-cut the 64 boxes
  around colours no longer on screen; after ~1,500 intro frames the planet and
  galaxy collapsed into purple/teal noise. Frames 1514 and 1515 of the executed
  original have identical upper palettes, and the per-frame build reproduces
  them.
* **GAME!TEK.** The logo is model 177 -> child 361: one slab face plus text
  `0x408a`. The port drew the slab but not the letters' colour, leaving black
  holes; the text colour is now resolved from the text material (F1692/F1693)
  and applied to the glyph instance. The letters are exactly the original's
  `(192,192,192)` at animation 63. There is no second logo draw path.
* **Child-model oracle.** A model whose child now fails is skipped by
  `verify-child-models.py`: the original would execute it against whatever its
  stale stack held, so there is no source-correct sibling-preserving trace to
  compare. The skipped subtrees are the captured `Failed` outcomes, not a
  changed compare rule.
