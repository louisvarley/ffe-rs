# Thruster glows (opcode 9, F1738 / F1704)

Status: **analysed, not ported.** The operand event is implemented (v0.23); the
queueing and drawing below were read out of the original and attempted, but the
differential check exposed differences in the port's screen positions and the
work was withdrawn rather than shipped unverified. This note records what the
original does so the next attempt does not have to re-read it.

`model_commands::Thruster` carries the material, the two vertex bytes and the
signed width. `execute_live` resolves A (and B when A is deeper than 0x200) and
reports a `ThrusterEmission`; nothing is queued.

## The original (F1738)

```
F1738(instance, material, record):
  colour = F1692(material) -> F1694(instance) -> F1691      (the three-step path
                                                            faces use)
  esi = record + 2
  width = signed word [esi + 2]          ; the record's +4
  a = F1471(vertex [esi])                ; projected
  if a.view_z <= 0x200: return          ; nothing is queued at all
  b = F1471(vertex [esi + 1])
  if b.view_z <= 0x200: return          ; nothing is queued
  width <<= or >>= (instance[+0x148] - 6)   ; arithmetic, the instance's scale
  if width == 0: return
  if width <= 4:
      F1704(instance, a, b, colour)     ; a line between the two vertices
      return
  glow_width = F1666(width, instance[+0xc])   ; (width * 256) / position z
  ; entry +0: the F1805 callback, +4..+0x12: four screen points (one per dword,
  ; low word x / high word y), +0x14: a flag, +0x1c..: six faded colours
  square = the corners (2a + b)/3 +- (width >> 2)        flag 0x123
  if 8 * F1467((b.x-a.x)^2 + (b.y-a.y)^2) < width: return
  shaft = a +- p, b +- p with p = (ny * width >> 17, -(nx * width >> 17))
          and n = ((b-a) << 15) / length                 flag 0x65
```

Both glow entries are queued with the key `F1777(a.view)`; the square and the
shaft are two separate entries, so a near glow queues **two** primitives (a
core and a shaft). Each entry's six colours fade the base colour by
`(i + 1) / 7` for `i = 1..6`.

`F1704` (the line) stores the two endpoints' x and y in four dwords, clamps a
positive x to `x - 1` and everything else to zero, and queues a six-word entry
keyed by `F1777(a.view)`.

## What made the check fail

- **Reachability.** The glow only queues when *both* vertices are deeper than
  0x200. The station and child scenes place the model at z 2000, which the
  ships' rear engines do not clear, so a deep scene (z 0x50000) is needed. Even
  then only **7 of 466 slots** reach a thruster command in the synthetic setup
  (102, 157, 311, 375, 429, 440, 454, 455); the rest sit behind conditional
  branches the synthetic globals do not satisfy. The census's "opcode 9 uses"
  count is *decoded* commands, not executed ones.
- **The port's screen positions.** With a `--thruster N` mode and the entry
  points logged, the port's midpoint and square corners differed from the
  original's while the depth key matched. That points at `Emission::screen` for
  the vertices involved, not at the glow arithmetic: the thruster's A and B can
  be the screen-space midpoint vertices (v0.42), whose screen position is the
  average of their *unprojected* sources - zero unless an earlier command
  projected those sources. Check what the original's entries hold after the
  preceding faces before changing the glow code.
- The six faded colours and F1805's rasterisation are not modelled; the viewer
  would fill the quad with the base colour.

## Reachability data

`--thruster N` (a copy of `--station-model` with the instance position z at
0x50000, added and then reverted) produced queue events for those 7 slots only.
The port reached thrusters in the same 7, which is why the entry comparison,
not reachability, is the open question.

## If picking this up

1. Add the scoped hooks again: `FUNC_001704` -> `FUNC_001705` (`FUNC_001777`,
   `FUNC_001778`, `near [DATA_007629]`) and `FUNC_001738` -> `FUNC_001739`
   (the same three, two sites each). `FUNC_001703` is globally hooked, so the
   F1704 hooks only serve the thruster's line.
2. Use event kinds 27 (line), 28 (glow flag + first point) and 29 (the other
   three points). **Do not use 30-33**: those are the polygon events.
3. Compare the *screen* inputs first, with a test that logs A/B for one model
   (311 is the simplest, scale 12, five thrusters) before adding the geometry.
