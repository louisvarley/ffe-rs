# F1100 target display: green lock boxes and `5.40AU`

Source: DOS FFE `FUNC_001099` / `FUNC_001100` (`ffe.asm:80555-81340`) and the
strings at `ffedat.asm:18889-18915`. The target colour is a COLPAL index:
0x11 for the navigation target (low palette entry 17, green `#00f000`) and
0x10 for the combat target (entry 16, red `#f00000`). The old port hard-coded
95 (amber `#f0a810`) for the range text and treated a manually locked body as
the combat target.

The executed original (`jjffe-smoke.exe --game-frame 500 0`) uploads exactly
these low-palette entries: 16 `(60,0,0)`, 17 `(0,60,0)`, 95 `(60,42,4)`
(six-bit). The cache's `base:palette/low` matches, and the fixed map reaches
17/16 from the words `0x0f0`/`0xf00` (F1327's fixed path).

## Before

Port test frames from the pre-change tree:

- `before-range.png`, `before-range-crop.png`: the old range text was
  `100 km` at `target - 5`, in amber 95.
- `before-tunnels.png`, `before-tunnels-crop.png`: a manually locked body was
  drawn as the combat target, so the four tunnels were red.

## After

`cargo test -p ffe-rs --test combat_view target_lock_uses_f1100s_green_and_two_decimal_au -- --ignored`
with `FFE_WORLD_CACHE=data.bin` and `FFE_SHOT_DIR=docs/evidence/lockon`:

- `after-lock-nav-green.png`, `after-lock-nav-green-crop.png`: four nested
  green tunnel boxes (0x11), green `5.40AU` one 10-row line below the target at
  `(x-5, y+5)`. This is the owner's original layout.
- `after-nav-cursor-crop.png`: F1111's navigation auto-targetter box, also
  green 0x11.
- `after-lock-combat-red.png`: the combat target's range text uses 0x10
  (red), as F1099 passes; F1100 draws combat tunnels only in the missile view,
  which the port has no mode for.

PNGs are 3x nearest-neighbour enlargements of the 320x200 logical frame.
