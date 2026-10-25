# Equipment, the fitted loadout and player damage

Status (2026-10-25): the equipment table and the fitted-loadout words are ported
to `ffe-core`/`ffe-rs`; the HUD, the F991 view row, the F1116/F1120 lamps and
the engines-off equipment page now read real state. The starport Upgrades list
is wired: `galaxy::Tables::tech_level` supplies F869's `DATA_008995`,
`station::upgrades` builds F383's rows through `equipment::outfitting`, and
`ffe-rs::outfitting::fit`/`remove` implement F386's Fit/Remove actions for
general equipment, cabins, lasers, shield generators and missiles. Drives and
mining machines are listed (F383 offers them) but have no port state, so their
buttons report that rather than inventing a drive word.

Source: `jjffe-master/ffe.asm` / `ffedat.asm` (DOS-era recovered asm; JJFFE 2.8a7
is the reference build). No cache change: the table is static game data with no
asset dependency (it sits in the executable's data section, like `DATA_005100`).

## The table (`DATA_005289`)

`ffedat.asm:6425` (records), `ffedat.asm:6701` (`DATA_005297`, the category
handler pointers) and `ffedat.asm:6778` (`DATA_005299`, the name string
pointers). `DATA_005289` is 66 records of 22 bytes; `DATA_005292` is a label
19 bytes into the array (the tech-level field of record 0), which is why
`F383`'s base plus `0x13` accesses look odd at first.

| offset | size | meaning |
|---|---|---|
| +0x00 | dword | price before the tech multiplier |
| +0x04 | dword | removal price before the multiplier |
| +0x08 | dword | mass in tonnes (the `Wt` column) |
| +0x0c | dword | unused (3 in every record) |
| +0x10 | byte | category / `DATA_005297` handler index |
| +0x11 | byte | flags (0x80 usually; 0x00 on two, 0x83 on one) |
| +0x12 | byte | code: bit selector, laser code, missile code, ... |
| +0x13 | byte | minimum tech level |
| +0x14 | byte | second-hand flag (`F383`'s other price branch) |
| +0x15 | byte | pad |

Names are message codes `0x9875 + index` (`DATA_005299[0x75 + index]`), i.e. the
`DATA_005776`.. string sequence. The generated Rust is
`crates/ffe-core/src/equipment_table.rs`, produced by
`tools/gen-equipment-table.py`; do not edit it by hand.

Categories (`record+0x10`):

| code | category | stored in |
|---|---|---|
| 0 | general equipment | bitfield dword `ship+0xc8` |
| 1 | passenger cabins | cabin count (`player+0x120`) |
| 2 | lasers | `ship+0xd2..+0xd5` (the record's `+0x12` is the laser code) |
| 3 | drives | drive words, not modelled |
| 4 | shield generator | `ship+0xe2` += 0x40 (`F413`), -= 0x40 (`F415`) |
| 5 | missiles | `ship+0xd6..+0xdf`, one byte per pylon (`F418`) |
| 6 | mining machines | machine words, not modelled |

`F393` (`ffe.asm:29030`) turns a general record's `+0x12` into a bit:

```text
bit = (record[0x12] ^ 0x10) & 0x1f
```

so bits 0..13 and 17..24/26..31 are used; bits 14, 15 and 16 have no record
(`GENERAL_MASK = 0xff_fe_3f_ff`).

## Availability and prices (`F383`, `ffe.asm:28564`)

- A record is offered when the starport's tech level (`DATA_008995`) is at least
  `record+0x13` (`cmp station, record; jl skip`).
- `DATA_009064`, the tech multiplier, is 0xc at tech >= 0xcc, 0xb at 0xb3, 0xa
  at 0x17, else 9. Both the fit and the removal price are multiplied by it.
- The second-hand branch (`record+0x14 != 0`) uses a value returned by
  `F34`/`F35` (a per-visit random). The port uses the same tech multiplier for
  both branches, so prices are deterministic (a reported deviation).
- The row template is `0x98f5` (`Fit Remove Item / $ to Fit / $ for Removal /
  Wt`); the fitted flag is `F392`/`F393`.
- Rust: `equipment::{outfitting, fit_price, removal_price, tech_multiplier}`.

The station piece is now wired: `F869` returns the system record's byte `+5`,
`F622` stores it in `DATA_008995`, and `galaxy::Details::tech_level()` /
`galaxy::Tables::tech_level()` expose it. `station::upgrades` maps
`equipment::outfitting` rows to the 0x98f5 list, and `outfitting::fit`/`remove`
apply F386's cash (`F387`/`F388`) and space checks plus the category handlers
(F390/F393, F399/F400, F428, F413/F415, F418/F421). Drives and mining machines
are shown but not modelled.

## The flags the console reads

| asm | Rust | meaning |
|---|---|---|
| `ship+0xc8..+0xcb` | `Loadout::general` | general-equipment dword (four bytes) |
| `ship+0xd6..+0xdf` | `Loadout::missiles` | ten missile pylons; `byte & 0xf` selects the F1120 icon |
| `ship+0xd2..+0xd5` | `Combat::lasers` | four laser codes |
| `ship+0xe0/+0xe2` | `Flight::shields` | current/maximum shields (0x40 per generator) |
| `ship+0xe4` | `Flight::hull` | hull, descriptor mass word * 4 (`F1533`, `ffe.asm:90970`) |
| `player+0xc6` word | `ShipSystems::status` | F1116's 0x45 condition: low byte 0x40, high byte bit 0 |
| `player+0xe8/e9/ec/f0` | `ShipSystems` | engine/autopilot/target/GUI bytes |
| `player+0xef` | `ShipSystems::ef` | missile-view byte (0x14 also needs general bit 9) |
| `player+0x174` word | `ShipSystems::p174` | chaff count (`F1120`'s 0x51 and the F1056 launch) |
| `player+0x268` | `ShipSystems::p268` | GUI/docked state byte (0x45 and the station redraw) |
| `player+0x511e` | `ShipSystems::p511e` | missiles in flight (F1120's 0x4e) |
| `player+0x3fb6` bit 1 | `ShipSystems::b6` | the labels/0x48 image condition in F1116 |

Note the bases: F393/F991/F1120 read the equipment dword through the ship pointer
at `player+0xc8`, while F1116's +0xc6/+0xc7 and +0x268 are fields of the player
block itself. The recovered asm's `mov ebx,DATA_008804` uses the player block
base; `[ebx+0x3fb6]` is the field the older notes call `DATA_003fb6`.

### F991 view-row icons (`ffe.asm:73469`)

- `0x10` always (labels toggle).
- `0x11`: general bit 5 (Navigation computer).
- `0x12`: general bit 27 (Escape Capsule) **or** bit 3 (Stowmaster fighter).
- `0x13`: general bit 6 (Combat Computer).
- `0x14`: `player+0xef != 0` and general bit 9 (Missile Viewer).
- `0x15` always, `0x16` when mounts > 2, `0x17..0x19` always.

### F1120 engines-off page (`ffe.asm:82429`)

- Hides 0x45-0x49, shows 0x3a (60x45 diagram), 0x4f when `DATA_007247 == 0`.
- Ten missile icons 0x3b-0x44: slot non-zero, image `slot & 0xf`. The icons are
  missile pylons, not general equipment: F998's two `0x82` missiles draw image 2
  (bitmap 346).
- `0x51` when general bit 11 (chaff) and `player+0x174 != 0`.
- `0x4a`: image 1 on general bit 23 (Naval ECM), image 0 on bit 19 (ECM).
- `0x4b` bit 26 (energy bomb), `0x4c` bit 24 (hyperspace cloud analyser),
  `0x4d` bit 2 (military cameras).
- `0x4e` when `player+0x511e != 0 && player+0xe8 == 0x34`, or `player+0xc6`
  bit 0x10.
- `0x19` image 1 and `player+0xe9 |= 8`.

## Player damage

`F953` (`ffe.asm:71190`) is shared by the player and the NPC ships:
shields absorb the hit first (`+0xe0`), then the hull (`+0xe4`); a hit at least
the remaining hull sets it to 0 and F942 destroys the object. Only objects with
control byte `+0x87 == 3` (player) or `0xb` (AI) take this path.

- Rust: `Flight::{ensure_damage, hit, destroyed}` over
  `ffe_core::laser::{DamageState, apply_damage}` (the same F953 core the NPCs
  already use). `ensure_damage` sets the hull full from the model statistics and
  the shield maximum from the fitted generators.
- **NPCs do not fire and the port has no station/police attack path**, so nothing
  calls `hit` yet. The API is wired and tested; the future NPC-fire slice should
  call `state.hit(damage)` where F930's hit test would run.
- Not ported: F938's shield recharge (`+0xe2 * tics / 32`, `/64` without the
  energy booster) and hull auto-repair (general bit 31), F1104's death screen and
  F1098's escape capsule. The hull/shield HUD lines now show the real words
  (a new-game ship has no shield generator, so the shield line stays hidden).

## Cache impact

None. The equipment table is 66 * 22 = 1,452 bytes of static data and is
generated into `ffe-core`, like the console table. The market goods tables were
already cached in v20; the cache stays **v26** and no asset count changes.

## Differences and uncertainties

- The table bytes were checked against the DOS `FIRSTENC.EXE` (floppy 1.02) at
  file offset `0xc1674`: 1,451 of the 1,452 bytes match `ffedat.asm`. The one
  difference is record 53's tech level ("Class 3 Military Drive"): the DOS
  binary has `0xeb`, the JJFFE listing `0xd7`. The port follows DOS.
- `F383`'s pricing block carries a `; FIX: incorrect tech prices` comment in the
  recovered JJFFE asm. The thresholds (0xcc/0xb3/0x17 → 12/11/10/9) are ported as
  written, but they may be a JJFFE correction of the DOS values; the DOS
  executable was not run to compare prices.
- The table's laser category codes disagree with the names in
  `ffe-core::laser`: the "30MW Mining Laser" record is `0x9a`, "Small Plasma
  Accelerator" is `0xc6`, "Large Plasma Accelerator" `0xcf` and "Thargoid Laser"
  `0xd5`, while `laser.rs` calls 0xc6 the 30MW mining laser and 0xcf the Thargoid
  laser. The equipment table and `DATA_006265` are the data; the Rust names are
  the combat slice's and are flagged for its owner, not changed here.
- `F383`'s second-hand price uses a per-visit random multiplier the port has not
  reproduced; deterministic tech-multiplier prices are a reported deviation.


## Tests

- `crates/ffe-core/src/equipment.rs`: names/prices/mass/category/tech decoding,
  the bit formula, `GENERAL_MASK`, fitted storage per category, icon nibbles,
  availability/prices and validation.
- `crates/ffe-rs/tests/combat.rs`: new/old save round-trips, F953 damage order,
  destruction, the no-generator new-game state, and a fitted generator keeping
  existing damage.
- `crates/ffe-core/tests/galaxy.rs`: the record's `+5` is `DATA_008995` and the
  procedural classes read the records before `DATA_007110`.
- `crates/ffe-rs/src/outfitting.rs`: F387/F388's ten-times price and refund,
  the general/cabin/laser/shield/missile handlers, cash/space refusals, mount
  and pylon limits, missing-item messages and the unmodelled drive/mining
  categories.
- `crates/ffe-rs/src/station.rs`: F383's available rows at tech 0xfd/0x16,
  fitted flags and the Upgrades Fit/Remove hot areas.
- `crates/ffe-rs/tests/console_table.rs`: F991 equipment icons, F1120 lamps from
  the dword bits, and F998's two real missile icons.
- `crates/ffe-rs/tests/station_view.rs` (ignored, needs `FFE_WORLD_CACHE`):
  the Upgrades, buy-a-laser, empty-ships and repairs frames and the current
  system's tech level.
- `crates/ffe-rs/tests/dashboard_view.rs` (ignored, needs
  `FFE_WORLD_CACHE`): `dashboard-equipment-empty.ppm` vs
  `dashboard-equipment-newgame.ppm` checks the two icon rectangles at
  (14,165) and (33,164).
