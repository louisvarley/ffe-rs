# NPC station traffic: approach, dock, launch (F689/F745/F746/F747/F733/F734/F735/F651/F736/F761/F762)

Frames from the scripted sequence in `crates/ffe-rs/tests/npc_docking.rs`,
produced 2026-10-25 with

```sh
FFE_WORLD_CACHE=<repo>/data-v27.bin FFE_SHOT_DIR=<dir> \
  cargo test -p ffe-rs --test npc_docking -- --ignored --nocapture
```

The run uses the new game's Ross 154 traffic (the player starts docked at the
open-air starport). The trader is `Lanner II 1` (model 0x2c) and the station is
body 4 (radius 4,499,968 mm). The test gives heavy freighters the port's
`STATION_CRUISE_SPEED` (40,000 mm/s) floor, so the approach takes 128.7 s; all
the turn, bay, door and record arithmetic is the source's.

## The logged sequence

```
F688 trader Lanner II 1 (model 0x2c) to station 4: radius 4499968 mm, orbit 5413807 mm
approach speed 40000 mm/s
docked on frame 6436 after 128.7 s; F747 interval 2.40 days
F745: Lanner II 1 cleared for bay 0 (F655 granted)
F652 touch: Lanner II 1 on the pad; F747 waits 57.6 h with bay 0 occupied
F734/F654: Lanner II 1 cleared to launch; waiting for the door
F735/F865: door open; F651 frees bay 0
F761/F762: Lanner II 1 set course for station Some(4)
```

Frames:

| file | frame |
|---|---|
| `approach.png` | after F655 granted: the trader on the final leg, its bay allocated |
| `docked.png` | after F652/F747: the ship is inside the station (not drawn); the bay is occupied and the 2.40-day object time runs |
| `launch.png` | after F735/F865/F651: the ship is out again and `npc_docking` asserts the bay no longer holds it |

## Source map

The labels are from JJFFE 2.8a7 `jjffe-master/ffe.asm`/`ffedat.asm` (the same
tables as the DOS `FIRSTENC.EXE`). `Fxxx` is the disassembler's `FUNC_000xxx`.

| Routine | Line | What it does |
|---|---|---|
| F688 | 45818 | system traffic spawner; F689's traders, F691's station-bay ships, F698's patrols |
| F689 | 45951 | a trader: state 0x11, `+0xf8 = 8`, `+0x100` = a starport, a random object time |
| F700 | 46774 | F762's random starport pick from `DATA_008999` (count `DATA_008997`) |
| F733 | 48396 | state 0x24: F747's object time; at zero F734 asks for launch |
| F734 | 48445 | state 0x1f: F654 launch permission; granted → state 0x21 |
| F735 | 48487 | state 0x21: wait for the station mode 0xfe/0, state 7 + formation offset + F736 |
| F736 | 48555 | state 7: fly away; at distance 6 F651 releases the bay, then F761/F741/F745 |
| F737 | 48631 | state 0x11: wait out the jump time, hide, state 0x12 (the port's spawn/HS timer) |
| F740 | 48751 | state 0x12: jump exit countdown; creates the cloud remnant, restores the model and `+0xf8` state |
| F741 | 48840 | state 8: set `+0xfe = +0x100`, formation offset, state 0 |
| F742 | 48912 | state 0 nav: move to `+0xfe`; at distance 6 F651, F761 or F741 or F745 |
| F743 | 48983 | state 0x0a: patrol/station-keeping offset (randomises `+0x102..+0x106`) |
| F745 | 49168 | F655 request and bay vector: sets `+0x102..+0x106` from the pad, state 0x23 |
| F746 | 49220 | state 0x23: final approach with F596, F744's F652 at contact |
| F747 | 49310 | state 0x20: wait for mode 3 to end, F653, mark docked, state 0x24, `+2` days |
| F748 | 49373 | state 0x22: the player autopilot's launch check (not an NPC state) |
| F749 | 49404 | state 5: random reposition, state 4 |
| F750 | 49480 | state 4: interception; at distance 10 state 1 and the pirate escort hand-off |
| F752/F753 | 49647/49665 | state 1/2: F732 + F753's distance tests (<5 state 3, >=10 F749) |
| F755/F756 | 49849/49867 | state 3: evasion/attack worker |
| F760 | 50112 | state 0x10: trading timer, F761 |
| F761 | 50131 | F775 twice; otherwise F762; no destination → state 0x1d + F738 |
| F762 | 50173 | F700's starport → `+0x100`, state 0; none → F925 |
| F770 | 50404 | state 9: trading approach; at distance 10 combat, else the battle message |
| F651 | 44537 | release the ship's bay, mode 1, current 0, timeout 0 |
| F652 | 44568 | the pad touch: only the current ship; mode 3, timeout 0x7080 |
| F653 | 44588 | reset the handling record (mode/current/timeout 0) |
| F654 | 44601 | permission: mode 0xff, current = ship, timeout 0x7080 |
| F655 | 44627 | F654 + F650, the 0x98b7/0x98b8/0x98ba/0x98bc replies, fines |
| F657 | 44777 | is this ship current |
| F867 | 60703 | clearance countdown; at zero F651 and the message |
| `DATA_006255..6258` | `ffedat.asm:9855-9861` | the AI rows: F706/F704/F780/F778's four slots |

The corrected slot order matters for reading the table: state 0 is
`F742 (control), F727 (movement), 0 (death notification), F789 (damage
response)`, not the shifted `F789, F752, F725, F782` the old port read from
`DATA_006258`. See `docs/notes/npc.md` §2 and the `the_ai_table_is_data_006255`
test.

## What the port's test asserts

- F655 granted a bay (`F745: ... bay 0`) and F650 kept it occupied.
- F652 set mode 3; F747 waited for the door and F653 cleared the record; the
  bay stayed allocated.
- F747's interval was 2.40 days (the test accepts 2.0..3.0).
- The launch went F734/F654 → F735/F865 (door 0) → F651/F736, after which
  `bay_of` no longer reports the ship.
- F761/F762 put the ship back in `Transit` with the next station.
