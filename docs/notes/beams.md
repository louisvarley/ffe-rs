# Beams (opcode 17): landing-gear legs

Status 2026-10-28. Opcode 17 draws a tube between two vertices, which is how
ships' landing-gear legs (and a few struts) are modelled. The port read the
operand records but never drew them, so lowered gear showed floating wheels.

## Source

- `F1760` (`ffe.asm`, Pos 60050) passes the record's six words to `F1761`.
- `F1761` projects both vertices (`F1471`, skipped when either is within
  `z <= 0x40`), builds an ellipse at each end (`F1553`, then `F1555`'s six
  projections), keys the beam by the midpoint of the two views (`F1777`) and
  queues one complex-polygon entry: the tube's side, split along its length
  into an unlit and a lit half when the material is lit and the light runs
  across the beam (`|axis x light| < 0x1000`), else one fill; then each end
  whose word has bit 7 and whose cap normal (`w & 0x7f`) is not culled, as a
  filled ellipse.
- Colours: the side is the material through `F1694` and `F1691` (no normal
  colour), the lit half adding the instance's shade 7 (`+0x84 + 21`) byte by
  byte; a cap is its own material through `F1693` with the cap normal's
  colour, then `F1691`.

## Port

- `ffe_core::model_beam` (`run`, `ellipse`, the arc tables) ports `F1761`'s
  geometry and fill outlines as `F1794`-family entries.
- `ffe_core::model_execute` runs it for opcode 17 in `execute_live` and queues
  `Primitive::Beam` at the midpoint key (`QueueEvent::Beam`).
- `ffe_rs::model_viewer` resolves each fill's colour (`beam_colour`) and fills
  it with the complex-polygon span rasteriser, in queue order.

## Verified / not verified

- Verified: the arc tables and the split/curve emission helpers
  (`model_beam` unit tests).
- Not yet: a whole-beam oracle against the executed original. The station and
  child-model queue oracles leave `QueueEvent::Beam` out because the probe does
  not hook `F1761`; adding that hook is the next check.
- Modern graphics: beams have no GPU solid yet, so they are rasterised into the
  frame under the GPU-drawn faces.
