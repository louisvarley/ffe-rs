# Traffic control and the docking and launch stages (F649-F657, F867, F1025, F1067-F1088)

`crates/ffe-core/src/traffic.rs`; wired into the playable by `ffe_rs::navigation::Navigator`
(`request_clearance`, `begin_docking`, `begin_launch`, `tick_traffic`). Read from
`jjffe-master/ffe.asm` and the `srctools` notes (`cstarport.txt`, `cplayer.txt`, `plstate.txt`).

## The station record

A station's physics object keeps its traffic state in bytes: `+0xc9` mode, `+0xca` the ship being
handled, `+0xce` the highest bay number (F880: 3, or 7 for kind 0x22; starports 5 or 1), `+0xd0..+0xd7`
one byte a bay (a ship index, 0 empty), `+0xd8` a timeout word and `+0x9e` the door word.

| Routine | What it does |
|---|---|
| F654 | Ask permission for a ship: refused (0) only when the mode is not 0 or 1 *and* another ship is current; otherwise mode `0xff`, current = ship, timeout `0x7080`. |
| F650 | Indoor station: scan the bays from `+0xce` down to 0; the ship's own bay is returned as is, else the first empty one is taken (flag -1). None: -1. For the player it first raises query event 0x17 (`F34`/`F35`), which only mission code can veto (not modelled). |
| F651 | Release: free the ship's bay, mode 1, current 0, timeout 0. |
| F652 | The ship touched the pad: only the current ship; mode 3, timeout `0x7080`. |
| F653 | Reset: mode, current, timeout 0. |
| F655 | The comms request: fines (player, 0x2710 or more), else F654, else F650, with the replies 0x98b9 / 0x98b7 / 0x98b8 (record reset) / 0x98ba new or 0x98bc already granted. |
| F657 | Is this ship current. |
| F867 | Event 6: the timeout counts down in game seconds; at or below the elapsed time the bay is released (F651), with a message when it was the player's. |
| F865 | The door word (`+0x9e`) follows the mode: negative counts down to 0 (open), positive up to `0xffff` (closed). Already ported and verified (`model_animation::door_step`). |

## The player's stages (`DATA_008870`, timeout `DATA_008884`, in game tics, 49,700 a second)

Docking, from a touch of the pad (F1019/F1024 set `0x70000`): 0x0a align (F1025, which also resets the
record, F653) → 0x0c open door (0x1ffff) → 0x0e into the lift (0x18000) → 0x10 turn (0x1ffff) → 0x12 close door
→ 0x14 wait → 0x16 open inner door → 0x18 into the bay (0x17530) → 0x28 → 0x2a docked (F1091). A thargoid
station (models 0x45, 0x4f) goes from 0x0a straight to 0x28. The whole docking is 1.3 million tics, about 26 s.
Each stage is `F1068(next, timeout)`: subtract the frame's tics, and below zero take the next state.

Launching: the services screen asks F654 and, if given, sets state 0x1a with `0x17530` (F363's caller): 0x1a
into the lift → 0x1c close → 0x1e wait → 0x20 open → 0x22 into the docking area (0x18000, F1088) → 0x36 leave
(the ship is put out, the station's mode is set to `0xff` and its door word to `0xffff` so the door opens)
→ 0x26 when the door word is 0 (F1089). The bay is released afterwards.

## The lift scenes (`ffe_core::lift`)

Every frame of a lift stage the original draws one or two models in the player's view (F1070, called through
F1071-F1080, F1086, F1092): model `0xea` (the cage), `0xec` (its interior) or `0xe9` (the bay beyond), at a
**depth in the ship's frame** less the eye's offset (the ship statistics' words `+0x18`/`+0x1a` times 8, so the
object sits at view-space `(0, -up*8, depth - along*8)`), turned about the vertical by an angle
(`MatBuildYZT(a = 0, b)`), with the animation word (global 1: the door, 0 open) and the time word (global 0).
`t` is what F1067/F1068 leave in `*t`: the timeout still to run, zero on the frame the stage ends.

| Stage | Drawn |
|---|---|
| 0x0c open door | cage, depth 0x30000, word `t >> 1`, turned half a turn |
| 0x0e into the lift | cage, depth `2t`, word 0, half a turn |
| 0x10 turn | cage, depth 0, word and angle `t >> 2` |
| 0x12 close door | cage, depth 0, word `~((t >> 1) & 0xffff)` |
| 0x14 / 0x1e wait | interior, depth -1, word 0 |
| 0x16 open inner door | interior at 0 and bay at 0x2ea60, word `t >> 1` |
| 0x18 into the bay | bay, depth and word `2t` |
| 0x1a out of the bay | bay, depth `0x2ea60 - 2t`, word `2t` |
| 0x1c close inner door | interior at 0 and bay at 0x2ea60, word `~(t >> 1)` |
| 0x20 open cage door | cage, depth 0, word `t >> 1` |
| 0x22 away (F1086) | cage, depth `max(2t, 0) - 0x30000`, word 0 |
| 0x2a docked (F1092) | bay, depth 0, word 0 |

In the playable (`Screen::lift`, `LiftView`): a docking draws them over the outside view (the stars, the station and
the ship are drawn first); a launch draws only the lift, from the cockpit, with the flight view's HUD. The ship is
held still during a docking. The models are the original's, run by the verified executor; the frames look like the
original's cage, shaft and bay (`tests/lift_view.rs` saves them with `FFE_SHOT_DIR`). One thing to look at against
the real game: the interior model with word 0 (the waiting stages) is nearly empty, so those frames are dark.

## The posters, and the launch's world (2026-10-24)

- F1070 gives each lift object the station's seed (`+0xa0`, global words 2 and 3) and global 4 = 1 (0 at an open-air
  starport); the posters of the bay and the lift are the child model `0xeb`, which picks its board by the seed, so every
  station has its own ("LITE", "JAGGED BANNER", "DREAM WARE", "RECYCLE OR DIE", "FIRST ENCOUNTERS ..."). They are text
  records of the child, which the viewer now draws (`docs/notes/text.md`).
- The launching lift scenes are drawn over the world like the docking ones: the ship waits in the slot facing out, so the
  stars (and the station seen from its slot) show past the cage's open doors; before they were black wherever the lift
  model has no wall (the waiting stage and the cage door). The original draws the world every frame.

## What the port keeps and leaves out

Kept: the record and every routine above, the stage order and timeouts, the clearance countdown, the lift scenes.
Left out: the player's velocity matching the station's rotation (F1025 sets it from F589/F591, F1088 at the end);
sounds; the open-air starport pads (F650's tables `DATA_006175..6179`) and landing; the playable's own docking
touch test still pushes the ship clear instead of running the collision stream and F575's crash and bounce
(ported in `ffe_core::landing`/`collision`, see `docs/notes/collision.md`, but not wired into `ffe_rs::navigation`,
`docs/NAVIGATION.md`).

## Verification

`traffic.rs` is tested by `crates/ffe-core/tests/traffic.rs` (10 tests written from the assembly) and
**verified against the executed original** (2026-10-24): `--traffic` in `headless.c` runs F650-F655, F657 and
F867 on 8,192 synthetic station records, and `tools/reference/verify-traffic.py` with
`crates/ffe-rs/tests/traffic_reference.rs` finds every record and return value equal; repeat runs match, nine
tampered fields and an empty trace are rejected. Trace SHA-256
`b3d95748de135d215fc85366263d820aef912cd699e76ab2ddebc61703004a3e` (CRLF, Windows build). The player-only parts of
F650/F655 (query event 0x17, fines, messages) are not in the oracle; the stage machine (F1025, F1067-F1088) is
tested against the assembly's constants only, not run against the original. The lift scenes and the stage
transitions of the timed stages (F1069-F1087, F1086) are verified: `--lift` runs the original stage functions with the
draw call captured on 8,192 random timeouts, frame times, ships and stations, and `verify-lift.py` finds every drawn
model, view-space position, orientation, animation and time word, and the state and timeout after the frame equal
(trace SHA-256 `08f72014103b925f72c0b89a0730b88458a7bcc4c0133c4a07867432ccfdcd0e`, CRLF).
