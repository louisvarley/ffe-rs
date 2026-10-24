# The console's buttons (DATA_005100, DATA_007684)

Ported in `crates/ffe-rs/src/console.rs` and `screen.rs` (v0.49i).

## Buttons

`DATA_005100` holds 82 `ConButton` records of 0x3e bytes: x, y, ten bitmap ids,
visible, current image, an unused word and a radio group (setting one of a group
to a non-zero image resets the others to 0). Bitmap ids from 96 are the secondary
table. The console draws each visible button with `bmpindex[current image]` in
index order (`FUNC_000261`). The flight console shows:

| Buttons | Where | What |
|---|---|---|
| 0x0b-0x0e | 107,164 / 91,165 / 70,166 / 60,172 | the screen buttons F4 (phone), F3 (map), F2 (clipboard), F1 (eye); F1 starts lit |
| 0x26-0x2b | 40,192 / 41,187 / 43,180 / 44,173 / 45,165 / 48,158 | the **time acceleration meter**: a pause mark and five steps, grey with a red lit twin, one lit at a time |
| 0x15-0x19 | 198,165 .. 125,163 | the view buttons (front, rear, turret, external, missile, combat), see `views.md` |
| 0x0f-0x11 | 296,178 / 274,178 / 261,177 | the right-hand buttons: hyperspace (0x99/0xa8), text labels (0x9a/0xa9) and navigation computer (0x9b/0xaa); see `labels.md` |

The meter is not a throttle. `FUNC_000208` turns a click (actions 0xc1-0xc5) or a
key (6, 7, 8, 9 and 0, the latter remapped from `0x30` to `0x3a`) into the frame
time scale `DATA_009012` per 20 ms: 0x3e2 (1x), 0x26d6 (10x), 0x1845c (100x),
0xf2b98 (1000x), 0x97b3f0 (10000x), lighting button 0x27 + step. The world time
the game steps by (`DATA_009155`, `DATA_008868`) is scaled by it; the key time
(`DATA_009122`) is a real-time accumulator, so steering stays responsive. The
original also subtracts 0xc22e from `DATA_008808` on a step above 1x (not yet
understood, not ported).

There is no speed or engine button in this build, and no key sets the engines-off
bit: it is only set by combat mode. The set speed is keyboard only.

## Hot areas

`DATA_007684` is a list of `(x0, y0, x1, y1, action)` dwords. The mouse poll
scans the whole list and the **last** area containing the point wins, with all
four edges inclusive; a click in the 3D view (y < 158) is action 0xff. The list:
the meter (0x07 pause mark, 0xc1-0xc5 steps), 0xf1-0xfc for F1-F12 along the
console, then 0x6e (`n` navigation computer), 0x6c (`l` text labels), 0x48 (`H`
hyperspace) and 0x74 (`t` target). F1 and F2 overlap at x 74-79, where F2 wins.

## In ffe-rs

- The meter and the four screen buttons are drawn from the cache's bitmaps.
- Clicking a step, or keys 6 7 8 9 0, sets the time acceleration; it applies to
  the original flight model (key O) and is remembered for when it starts. The
  bottom mark pauses (and resumes), as P does.
- The right-hand icons are drawn: the labels button (lit while `l` is on), the
  navigation computer (lit while its list is open) and hyperspace. The
  navigation list, the labels toggle, hyperspace and target are ported; see
  `labels.md`.
- F2-F5, the missile and combat views and the escape capsule answer with "not
  ported yet".
- A readout under the title shows speed, the set speed (original model) and the
  time scale; it is ffe-rs's, not the original's.
