# Planetary rings: findings and hand-over (2026-10-28)

Status: **rings are not drawn in the playable.** This note records what is
known so the work can be picked up.

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

## Why the playable shows no ring

`Screen::draw_discs` renders a planet with `AnimationInputs::test(0)`, so
global 2 is 0 and the branch never calls 314. Passing the seed
(`globals[2] = seed`, `globals[3] = seed >> 16`) makes the ring draw, but
wrongly: a huge fan/band across the view, or nothing, depending on tilt.

## What is verified (committed)

- **F1771's non-zero case** is now ported (`model_execute` opcode 25 →
  `model_math::table_root`, `fraction`, `rotate_by` = F1744). The port used to
  refuse a non-zero direction.
- **The top-level direction** (+0x38) is now set to the light in the object
  frame (F1685), in `ModelViewer::render_matrix`; it was always zero.
- The harness's `--child-model` mode (`jjffe-commands`) and
  `child_models_reference` take `FFE_CHILD_GLOBAL2`, `FFE_CHILD_GLOBAL3`,
  `FFE_CHILD_DIRECTION` (x,y,z) and `FFE_CHILD_MATRIX` (nine Q31 values) so a
  slot can be run as a seeded, lit, tilted body. With these, model 134 with
  Saturn's seed **matches the original event for event** (child set-up, the
  ring's polygon/curve events, queue) at identity and at a 0.5 rad tilt, for
  depth 2000, 4e8 and 1.5e9 and two light directions:

  ```
  FFE_CHILD_GLOBAL2=0xffa1 FFE_CHILD_DIRECTION=12000,-20000,8000 \
    FFE_CHILD_MATRIX=<9 Q31> jjffe-commands --child-model 134 1500000000 > t.json
  FFE_CHILD_GLOBAL2=0xffa1 FFE_CHILD_DIRECTION=12000,-20000,8000 \
    FFE_CHILD_MATRIX=<9 Q31> FFE_CHILD_DEPTH=1500000000 \
    FFE_CHILD_EXE=game/FIRSTENC.EXE FFE_CHILD_TABLE_RVA=0x0d0cfc \
    FFE_CHILD_CACHE=data.bin FFE_CHILD_VECTORS=t.json FFE_CHILD_VERBOSE=1 \
    cargo test -p ffe-rs --test child_models_reference -- --ignored --nocapture
  ```

## The open problem: the planet's instance set-up

In that harness set-up **the original's ring is garbage too**: its polygon
entries sit at packed (160,79), the screen centre, and its curve runs to
(-847,-242) and (11022,70). The reason is the scales. The harness copies
F1683: the planet's `+0x148` is descriptor `+0x14` (22) and `+0x130` is 0.
F1750 then gives the ring child scale `0 + 22 + 9 - 0 = 31` (trace event
`[20,1,31,0]`), and 96 << 31 overflows. The port's planet draw makes the same
assumption, so it inherits the same garbage.

The real game cannot be drawing planets that way. Its planet objects must
reach F1683/F1684 with a different `+0x148`/`+0x130` (or `+0x44` observer
shift), which keeps the ring child in range. The next step is to find how the
original draws a planet object:

- `ffe.asm:102329` (F1684) sets `+0x148` from the descriptor's `+0x14`;
  `ffe.asm:36024..36058` and `71607` also write `+0x148` (36049/36058 set
  0x7f), and `102400/102443` read it. Find which path a planet takes and what
  it does to `+0x148`, `+0x130` and `+0x44` (F1774 raises `+0x44`).
- Check the object's distance word `+0x7e` for planets (F1680's class) and
  whether it feeds those words.
- A cheap empirical check: dump a Saturn object's instance (`+0x130`,
  `+0x148`, `+0x44`) in the reference while Saturn is on screen, and rerun
  `--child-model 134` with those values (add env overrides for them).

The port's own trick is separate: `draw_discs` shrinks a distant planet's
matrix by a power of two instead of changing its scale. Forcing the shrink to
0 at 12 radii gives the same broken ring, so the shrink is not the cause, but
once the set-up is known the shrink may need to become a scale change so the
child's F1750 scale follows it.

When the ring draws correctly, pass the seed in `draw_discs` (globals 2/3) to
switch it on, and add an `effects_view`-style frame test of Saturn.
