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

## What the port keeps and leaves out

Kept: the record and every routine above, the stage order and timeouts, the clearance countdown. Left out: the
lift scenes the original draws during the stages (F1070-F1080 build a lift object, models 0xe9/0xea/0xec, and
slide it); the player's velocity matching the station's rotation (F1025 sets it from F589/F591, F1088 at the end);
sounds; the open-air starport pads (F650's tables `DATA_006175..6179`) and landing; the crash and bounce of a
bad touch (the playable pushes the ship clear instead, `docs/NAVIGATION.md`).

## Verification

`traffic.rs` is tested by `crates/ffe-core/tests/traffic.rs` (10 tests written from the assembly). The
differential oracle is written but has **not been run**: `headless.c` gained `--traffic` (F650-F655, F657, F867 on
8,192 synthetic records; the C was syntax-checked in isolation), `exports.inc` the aliases, and
`tools/reference/verify-traffic.py` with `crates/ffe-rs/tests/traffic_reference.rs` the comparison (its plumbing
was checked against a trace the Rust code produced itself). The harness no longer builds on Windows (it includes
`sys/time.h` and friends since the x87 work), so run it on the Linux build:
`tools/reference/build-linux.sh && python3 tools/reference/verify-traffic.py --reference .cache/jjffe-reference-linux/jjffe-smoke`.
Until then the port is "read from the assembly and unit-tested", not "verified against the executed original".
