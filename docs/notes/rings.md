# Planetary rings: findings and hand-over (2026-10-28)

Status: **fixed for rendered planets (2026-10-28); no frame capture yet.**
Saturn's ring now draws. This note records the cause, the fix and what remains.

## How the original decides a ring

- Saturn and Jupiter both use model **134** (Uranus/Neptune 135). The ring is a
  **child model, 314**, called from model 134 at offset 0x2c (`4e 27 06 06`:
  opcode 14, vertex 6 = the planet's centre, frame word 0x0606, literal scale 0).
- The call sits behind `4d c1 82 0e` (local 1 = **global 2 >> 14**) and the
  bit-branch `13 00 c1 00`. Global 2 is the body's seed low word (object
  `+0xa0`). Saturn's seed 0x0123ffa1 gives 3 (ring); Jupiter's 0x12346000
  gives 1 (a thin line in the port's probe, maybe its faint ring).
- Before the call the planet runs opcode 28 `0x003c` (F1739 base basis) and
  opcode 25 `0x0039` (**F1771**: align the basis with the +0x38 direction
  vector, axis mode 1, components (d[2], d[0])).
- Model 314 has 98 plain type-1 vertices in its XZ plane, out to 96 units at
  scale 22, drawn as F1721 polygons with curved segments and F1737 curves (op
  22). Its descriptor (FIRSTENC.EXE table at 0x0d0cfc) is scale 22, scale
  offset **9**, radius 112; model 134's is scale 22, offset 10, radius 116.

## Why the playable showed no ring

`Screen::draw_discs` rendered a planet through the **F1683/F1684 stand-alone
display path** (which sets `+0x130` = 0) and with `AnimationInputs::test(0)`, so
global 2 was 0 and the branch never called model 314. Forcing the seed alone
made the ring draw wrongly: F1750 gives the ring child
`scale = 0 + 22 + 9 - 0 = 31`, and `96 << 31` overflows the 32-bit vertex
arithmetic.

The flight view's planets are scene objects drawn through **F1680**, not
F1683. F1680 (`ffe.asm:102006..102027`) sets:

```
e = desc[+0x18] + obj[+0x7e] - shift
if e < 0:  +0x130 = desc[+0x18] - e;  +0x148 = max(desc[+0x14] + e, 0)
else:      position >>= e;            +0x130 = desc[+0x18];  +0x148 = desc[+0x14]
```

For any visible planet `e >= 0`, so the instance key scale `+0x130` is the
descriptor's scale offset (10 for model 134), not 0. The ring child scale is
then `0 + 22 + 9 - 10 = 21`: `96 << 21` fits, and the ring surrounds the
planet. Only beyond ~2^38 mm (about 40 planet radii, where the planet is a dot)
does `e` go negative; `draw_discs` does not render the surface that far out.

## The fix (2026-10-28)

- `ModelViewer::render_scene_with_instance` passes F1680's key scale into the
  instance; `render_matrix` no longer hard-codes `key_scale: 0`.
- `ModelViewer::model_scale_offset` exposes descriptor `+0x18`.
- `Screen::draw_discs` passes the body seed as object words 2/3
  (`globals[2] = seed`, `globals[3] = seed >> 16`) and the descriptor's scale
  offset as the key scale.
- Regression: `crates/ffe-rs/tests/planet_view.rs::saturn_draws_its_ring`
  (local-only, `FFE_WORLD_CACHE`) runs model 134 with Saturn's seed at
  `key_scale = 10` and at 0. Observed: **24 ring polygon/curve entries spanning
  63 px** at the F1680 key scale, versus **1 degenerate entry at 0 px** with the
  old 0. The `saturn_draws_its_ring` numbers are from the current `data.bin`.

## Still open

- A frame-level comparison (Saturn through the reference and the port) would
  confirm the ring's exact width, tilt and shade. The current test proves the
  child runs with sane geometry, not pixel equality.
- `draw_discs` still shrinks a distant planet's matrix by a power of two instead
  of using F1680's full `e`/`+0x148` adjustment. That is now independent of the
  ring's child scale, so it no longer breaks the ring, but a very distant
  planet's ring size may still differ slightly. Visible planets keep
  `key_scale = descriptor offset`, so the difference starts beyond the range
  where the surface renderer is used.

## Earlier harness notes (unchanged, still useful)

The harness's `--child-model` mode (`jjffe-commands`) and
`child_models_reference` take `FFE_CHILD_GLOBAL2`, `FFE_CHILD_GLOBAL3`,
`FFE_CHILD_DIRECTION` (x,y,z) and `FFE_CHILD_MATRIX` (nine Q31 values) so a
slot can be run as a seeded, lit, tilted body. With these, model 134 with
Saturn's seed matched the original event for event (child set-up, the ring's
polygon/curve events, queue) at identity and at a 0.5 rad tilt, for depth 2000,
4e8 and 1.5e9 and two light directions:

```
FFE_CHILD_GLOBAL2=0xffa1 FFE_CHILD_DIRECTION=12000,-20000,8000 \
  FFE_CHILD_MATRIX=<9 Q31> jjffe-commands --child-model 134 1500000000 > t.json
FFE_CHILD_GLOBAL2=0xffa1 FFE_CHILD_DIRECTION=12000,-20000,8000 \
  FFE_CHILD_MATRIX=<9 Q31> FFE_CHILD_DEPTH=1500000000 \
  FFE_CHILD_EXE=game/FIRSTENC.EXE FFE_CHILD_TABLE_RVA=0x0d0cfc \
  FFE_CHILD_CACHE=data.bin FFE_CHILD_VECTORS=t.json FFE_CHILD_VERBOSE=1 \
  cargo test -p ffe-rs --test child_models_reference -- --ignored --nocapture
```

That harness used the F1683-style `+0x130 = 0`, which is correct for its
synthetic set-up but not for a planet in the flight view; the ring it produced
there was the original's own (garbage) output for those inputs.

## Oracle status after opcode 25 (2026-11-02)

With opcode 25's alignment ported, models **273 and 287** now execute natively
(before, they stopped at the alignment and were counted as "not executed"). At
the default depth all 439 compared models match; at depth `0x50000`
(`verify-child-models.py --depth 0x50000`) these two differ. With
`--verbose` (which now reports the masked events actually compared), the first
difference is event 25 for a child called after the alignment: the hash of the
child's +0x48 observer point and +0x38 direction. Event 24 (the child's +0x14
matrix and locals) still matches, F1771/F1744's arithmetic matches the
source, and F1685 computes +0x38 as `+0x134 x +0x14` and +0x48 as the shifted,
negated position `x +0x14` (`FUNC_001670_VecMatTMul`, rows, as
`transform_rows`). So the open question is the child's +0x134 (or the
position/shift feeding +0x48) after an aligned parent. Whoever continues the
rings should start there; the `FFE_CHILD_DIRECTION`/`FFE_CHILD_MATRIX`
overrides in the harness make single cases easy to replay.
