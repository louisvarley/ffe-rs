# ECM, the auto-targetter cursor and the missile view

Source: DOS FFE `FUNC_000718`/`FUNC_000719` (missile ECM, `ffe.asm:47435`,
`47573`), `FUNC_000935` (`ffe.asm:69987`), `FUNC_001054`'s view dispatch
(`JUMP_004340`, `ffe.asm:42880`), `FUNC_001010`/`F1062`/`F1064`
(`ffe.asm:74816`, `78991`, `79072`), `FUNC_001111` (`81954`) and
`FUNC_001100`'s view-5 branch (`81055-81070`). The port code is
`crates/ffe-core/src/{equipment,missile,effects}.rs`,
`crates/ffe-rs/src/{missile,targeter,camera,combat,screen}.rs` and `main.rs`.

## ECM (`F718`/`F719`, `F935`)

`F935` checks the fitted equipment, not energy or ammo: Naval E.C.M.
(`ship+0xc8` bit 23) first and the E.C.M. System (bit 19) second. It sets
`DATA_008835` bits 2/3 for the frame and `ship+0x14f = 2`, `+0x151 |= 0x20`.
`F718` then reads `DATA_000029` for the missile's model:

| action | models | meaning |
|---|---|---|
| destroy | 5, 6 | both systems |
| destroy with `DATA_008835 & 8` | 7, 9, 10 | Naval E.C.M. only |
| immune | 4, 8, 11, 12, 13 | neither |
| `ja` past 13 | > 13 | the source's out-of-table branch |

The old note in `combat.md` had this table inverted (destroying 4/8/11/12/13 and
keeping 5/6); the listing's jump table is `[0,0,0,0,1,0,0,2,1,2,2,1,1,1]` with
entries `destroy / nothing / test-bit-3`. There is **no range or owner test** in
`F718`: the burst is global for the frame. `F719` creates one model-0x9b object
at the missile and removes it; it does not spawn F926's debris.

Frames: `ecm-before-crop.png` has the player's model-6 missile 20 m ahead of
the cockpit; `ecm-after-crop.png` is the same frame after `E` (the fitted
E.C.M. System). The missile is gone. F719's model-0x9b object is present in
`Navigator.effects` (the test asserts it), but its command stream collapses to
single-pixel flats in the port's model renderer — the same limitation as F942's
explosion in `docs/evidence/fills/explosion-debris.png`, where the visible burst is F926's debris
that F719 does not create.

## Auto targetter (`F1010`, `F1054`, `F1111`)

`T` and `Shift+T` only touch `DATA_007246`/`DATA_007247` with the Auto targetter
fitted (bit 7, record 0x17); arming stores the current target in
`DATA_007248`/`DATA_007249`. While a flag is set, `F1010` runs `F1062`/`F1064`
each frame: F989's first live object whose screen point is within six pixels
(Manhattan) of the view centre becomes the target (`player+0xec`/`player+0xed`)
and the cursor disarms; an empty crosshair clears the target. F1111 draws the
navigation box (4-half-size at (160,79), colour 0x11) and/or the combat circle
(radius 6 at (159,79), colour 0x10) on the game-second blink.

The recovered listing has no call to `F1010`; the port runs the acquisition from
the flight step so the armed cursor is not inert (reported in `combat.md`).

Frame: `targeter-armed-crop.png` is the front view with the navigation targetter
armed: the green box at the view centre over the pirate. The test toggles the
box and checks the cursor area (150..172, 70..90) changes.

## Missile view (F10 / view 5)

`F1054`'s F10 (action 0xfb) needs the Missile Viewer (bit 9) **and**
`player+0xef` (`DATA_008877`), the last missile the player launched; F941
stores it at launch. `F1054`'s `JUMP_004340` copies that missile object into the
camera and calls F1053 with the missile model's eye words, so the camera rides
the missile's position and axes. F1102 clears `DATA_008877` and forces view 0
when the missile dies. F1099's view-5 branch skips the navigation target, the
gunsight, the velocity marker and the player's own HUD; the combat target's
range uses colour 0x5f and F1100 measures from the missile.

Frame: `missile-view-crop.png` is the same scene as `missile-front-crop.png`
with the camera on the second missile: the pirate is seen from the missile,
the range reads `0.12km` in amber (0x5f), and there is no gunsight or player
hull/shield text. `missile-front-crop.png` is the cockpit frame for comparison.

## Reproduce

```text
FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=docs/evidence/ecm \
  cargo test -p ffe-rs --test ecm_view -- --ignored --nocapture
```

The PNGs here are 3x nearest-neighbour enlargements of the 320x200 logical
frames (the crops are 4x of the 120x100 centre).
