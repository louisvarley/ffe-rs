# Flight views (JJFFE 2.8a7)

Researched from `jjffe-master/ffe.asm` and `srctools/` (cplayer.txt 640-685,
gmstruct.txt, plstate.txt). Ported in `crates/ffe-rs/src/camera.rs`.

## View mode

`DATA_008872` (= player struct `DATA_008804 + 0xea`): 0 front, 1 rear, 2 top
turret, 3 bottom turret, 4 external, 5 missile, 6 combat-computer chase. Camera
dispatch `FUNC_001054` (ffe.asm 77546, jump table `DATA_000039`). Forced to 0 on
hyperspace (`FUNC_001066`), docking (75945/75961), escape capsule (`FUNC_001098`)
and when a viewed missile dies; forced to 4 on death (`FUNC_001104`). The strings
'Front View' .. 'External View' (ids 0x861a..0x861e) appear unused in this build.

## Keys and console

DOS scancodes map through `DATA_007688` (`FUNC_001429`): F1-F10 -> actions
0xf1-0xfa, F11 0xfb, F12 0xfc. **F1** (event 1 of module 3, `FUNC_001002` ->
`FUNC_001001`) cycles front -> rear -> turret (2, or 3 when the turret pitch
>= 0; skipped when the ship has <= 2 gun mounts, `ship[+0xd1]`) -> external ->
front. Direct keys (`FUNC_001056`, cockpit only): **F6** front, **F7** rear,
**F8** turret (mounts > 2), **F9** external, **F10** missile (missile viewer and
a tracked missile), **F11** combat computer (if fitted).

Console buttons (`DATA_005100`, radio group 1: one lit at a time; bitmaps are
console ids, 96 and up = secondary table):

| Button | x, y | off/on | Hot area (`DATA_007684`) | Key |
|---|---|---|---|---|
| 0x18 Front | 142,163 | 0xa2/0xb1 | 146,163-161,182 | F6 |
| 0x17 Rear | 162,163 | 0xa1/0xb0 | 166,163-181,182 | F7 |
| 0x16 Turret | 181,163 | 0xa0/0xaf | 185,163-200,182 | F8 |
| 0x15 External | 198,165 | 0x9f/0xae | 202,165-217,184 | F9 |
| 0x14 Missile | 211,168 | 0x9e/0xad | 215,168-230,187 | F10 |
| 0x13 Combat | 224,169 | 0x9d/0xac | 228,169-243,188 | F11 |

The F1 hot area is 64,172-79,191. Clicks act on the left-button press edge.

## External camera

Player `+0x128` yaw, `+0x12c` pitch (16-bit angles), `+0x130` signed distance
(negative = behind; model units * 256). Start (`FUNC_001000`): yaw 0, pitch
0x200, distance 0xff400000. Arrow keys (hard-coded scancodes; not with Left Alt)
turn yaw/pitch by `d = (elapsed * (min(held, 70) + 2)) >> 6`, with elapsed in
1/49710 s (`ms * 0x3e2 / 20`) and held in 50 Hz ticks; Numpad+ or `=` zooms out
(`dist -= d << 10`, clamped to 0x80000001 if it reaches >= 0), Numpad- or `-`
zooms in (`dist += d << 10`, then no closer than the model's descriptor radius
word << (scale + 8), also applied on a ship change by `FUNC_001110`). Opposite
keys together reset yaw to 0, pitch to 0, distance to 0xff400000.

Camera (`FUNC_001054`, 77722), on the ship's axes A (right), B (up), C
(forward) with `FUNC_001513` rot(v1, v2, a) = (v1 cos a + v2 sin a, v2 cos a -
v1 sin a): rot(C, A, yaw), then rot(B, C, pitch); with "Lock external view to
ship" on, the camera is at ship + C' * dist / 256 with the rotated axes. (Lock
off uses the unrotated C for the offset; its default is not established.)

## Rear, turrets, overlay

Rear negates A and C (a half turn about B) and mirrors the front eye offset
(model `+0x38` data); the player's ship is hidden only in the front view.
Turret views place the eye at model offsets and turn by the turret angles
(`+0x204/+0x206`). The console is the same in every view; the crosshair is not
drawn in views 4 and 6, the velocity marker only in the front view.

## In ffe-rs

Front, rear and external, F1 cycle, F6/F7/F9 and the three console icons/hot
areas are ported as above (external camera exact; "lock to ship" on). Not
ported: turret, missile and combat views (equipment and mounts are not
modelled), eye offsets (the rear view does not draw the own ship), the
Alt-arrow mouse nudge. ffe-rs moved its own development keys off the original
ones: help F1 -> H, model viewer F6 -> M, load F9 -> Shift+F5.
