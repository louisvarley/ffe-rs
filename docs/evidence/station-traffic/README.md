# Station-bay ships and F699's aux traders (F688/F691/F700/F650/F1530)

Frames from the scripted sequence in `crates/ffe-rs/tests/station_traffic.rs`,
produced 2026-10-25 with

```sh
FFE_WORLD_CACHE=<repo>/data.bin FFE_SHOT_DIR=<dir> \
  cargo test -p ffe-rs --test station_traffic -- --ignored --nocapture
```

The run uses the new game's **Gateway** system (start record 0, 12 place
bodies). F688's `RND(4 * places)` roll is capped by the port at
`MAX_BAY_SHIPS = 3`; this seed gives one bay ship, `Wyvern Explorer 3`
(model 0x29), which F700 places in Sarajevo's bay 5. F691's object time is
7.16 days (F1530's word without F747's `+2` days), so the test fast-forwards it
and runs the ordinary F733/F734/F735/F651/F736/F761/F762 launch.

## The logged sequence

```
F688 pass in Gateway: 12 place(s), 8 ship(s); F700 placed Wyvern Explorer 3 (model 0x29) in Sarajevo bay 5; F691 object time 171.91 h (7.16 days)
F733/F734/F654: Wyvern Explorer 3 cleared to launch; waiting for the door
F735/F865/F651: door open; bay 5 freed
F761/F762: Wyvern Explorer 3 set course for station Some(16)
visible traffic: 1 ship(s), 0 at the station
```

Frames:

| file | frame |
|---|---|
| `station-traffic.png` | the Gateway starport and its visible F688 traffic; the F691 ship is in bay 5 and `NpcShip::visible` is false |
| `station-bay-docked.png` | the same bay from 200 m outside the pad. F747 hides docked ships, so this evidence frame draws the ship explicitly; the live view does not |
| `station-bay-launch.png` | after F735/F865/F651: the F691 ship out of the slot, the bay no longer holds its traffic id |

## Source map

Labels are from JJFFE 2.8a7 `jjffe-master/ffe.asm`/`ffedat.asm` (the DOS
`FIRSTENC.EXE` has the same tables and routines). `Fxxx` is `FUNC_000xxx`.

| Routine | Line | What it does |
|---|---|---|
| F688 | 45818 | the traffic spawner. Traders, Thargoids, then (only while `DATA_009046` bit 0 is clear) F691's bay ships and F699's aux traders, then F698's patrols and F692's no-op |
| F686 | 45787 | system entry: sets `DATA_009046` bit 0 and calls F688 (no bay ships/F699) |
| F687 | 45801 | event 3 (after launch): clears bit 0 and calls F688 again |
| F617/F618 | 42459/42476 | build `DATA_008997`/`DATA_008999`: one 802-byte record per object with `+0x14c & 0x10` (stations and starports); `+0x130` is the object slot |
| F700 | 46774 | `RND(DATA_008997)` over those records, returns the station object |
| F691 | 46113 | F772(1) trader; F700; F650; copies a starport's position/orientation, adds F650's bay vector, state 0x24, F691's object time, `+0xfe` = station; F940 if F650 fails |
| F650 | 44371 | allocates the bay (`+0xce` down), returns the bay's local vector (station-model tables `DATA_006175..006179`) or `(0,0,-1)` for a starport, and the bay index/fresh-bay flag |
| F772/F773 | 50502/50526 | create the ship from F635's type table; F691 passes type 1, F699 type 0xf |
| F1530 | 90913 | advances `DATA_008818`/`DATA_008820` and returns the low word; F691 reads the full `DATA_008818` for `[+0xa8] = word << 4` (day tics) and `[+0xac] = word >> 28` (days) |
| F733 | 48396 | state 0x24's control: F747's object time; at zero F734 asks for launch |
| F734/F735 | 48445/48487 | F654 launch permission, then wait for the station's door word |
| F736 | 48555 | state 7: fly out; F651 frees the bay; F761/F762 choose the next station |
| F761/F762 | 50131/50173 | F700's random place → `+0x100`, state 0; none → F925 removes the ship |
| F786 | 51182 | the state-0x24 **damage response**, not a route: a hit on a docked ship sends it to F734's launch (and sets a Thargoid bounty). It is the `damage` slot of row 0x24, not a control routine |
| F699 | 46729 | F701 requires a starport, F772(0xf), F702 on `0x5000/0x18`, state 0x0d and `+0x56` = the station's object slot |
| `DATA_006255` row 0x0d | `ffedat.asm:9855` | all four slots zero except F790's damage response: F699's ships have no control or movement routine |
| F790 | 51291 | the row's damage response: reports the attack through F237; no movement |

## What the test asserts

- `Navigator::new_game`'s F688 pass placed a docked type-1 trader in a bay; the
  station's traffic record (`Starport::bay_of`) holds its `npc::traffic_id`, and
  `pads` contains that id.
- F691's object time is in `0..16 days` (F1530's day nibble plus its day-tics),
  and the ship is in `State::Docked`/`NpcShip::visible == false`.
- Fast-forwarding the timer runs F733 → F734/F654 → the door → F735/F865 →
  F651/F736, after which `bay_of` no longer reports the ship, and F761/F762 puts
  it in `Transit` with the next place.
- The launched ship is visible and draws (the frame's pixels change when the
  ship list is cleared).
- The ship is F691's `F772(1)` trader (`Role::Trader`, model in `SHIP_TYPES[1]`),
  not F699's Lynx.

## Data correction

`DATA_008997`/`DATA_008999` is **not** a cache ship-data table. It is the
runtime starport array F617/F618 build at system creation from the generated
station/starport objects, so no cache import or generator is needed (unlike the
equipment/shipyard tables). The port derives the count from the generated
place bodies and F700's destination from the same list.

F699 is not a shuttle route either: `F772(0xf)` selects `DATA_006195` (Lynx Bulk
Carrier/Long-range Cruiser), and state 0x0d has no control routine. The port
spawns them in `Hold` and does not invent a station target. Model 0x0f
(`Interplanetary Shuttle`) belongs to F635 table record 0xd, which F688 never
calls. F786 is the docked row's damage response (launch on a hit), not a route.
