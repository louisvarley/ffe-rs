# Equipment, the fitted loadout and player damage

Status (2026-10-25, fitting round): the equipment table and the fitted-loadout
words are ported to `ffe-core`/`ffe-rs`; the HUD, the F991 view row, the
F1116/F1120 lamps and the engines-off equipment page now read real state. The
starport Upgrades list is wired: `galaxy::Tables::tech_level` supplies F869's
`DATA_008995`, `station::upgrades` builds F383's rows through
`equipment::outfitting`, and `ffe-rs::outfitting::fit`/`remove` implement F386's
Fit/Remove actions for **all seven categories**: general equipment, cabins,
lasers, drives, shield generators, missiles and mining machines. A drive is one
byte at `ship+0xd0`; mining machines are `DATA_009104`'s ten-record list. There
is no listed-but-disabled row left: every category F383 offers has a handler.

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
| 3 | drives | one byte `ship+0xd0` (`F408`/`F410`), class = record `+0x12` |
| 4 | shield generator | `ship+0xe2` += 0x40 (`F413`), -= 0x40 (`F415`) |
| 5 | missiles | `ship+0xd6..+0xdf`, one byte per pylon (`F418`) |
| 6 | mining machines | `DATA_009104`'s ten 52-byte records (`FUNC_000248`), count `+0` |

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
apply F386's cash (`F387`/`F388`) and space checks plus every category handler:
F390/F393 and F391/F395 (general), F399/F398 and F401/F400 (cabins),
F404/F403 and F406/F405 (lasers), F409/F408 and F411/F410 (the one drive),
F414/F413 and F416/F415 (shield generators), F419/F418 and F421/F420
(missiles), F424/F423 and F426/F425 (mining machines). F392 also uses the
third-column fitted predicates F397/F402/F407/F412/F417/F422/F427, ported by
`Loadout::fitted`.

## The remaining category handlers

### Drives (category 3, F409/F411)

- `ship+0xd0` is **one** drive byte, not a list. F408 refuses a second drive
  with 0x98f9 ("You may have only one drive system."); F410 clears the byte and
  `ship+0x11e` (the energy pool). F411's messages are 0x98fa ("You have no ...
  to remove.") and 0x98fb ("The drive is an integral part of your ship.") when
  the model's statistics word at `+0x14` is negative.
- F408 also stamps `DATA_008924 = 0x1f4` and `DATA_008926 = DATA_008807`, the
  drive-service clock the F444 "Drive Only service (last: ...)" row reads. The
  port's `Flight::service_days[0]` is not yet written by `outfitting::fit`
  (a reported gap: the fit function has no Flight handle).
- The class comes from the equipment record's `+0x12` (codes 1..9 civil, 0xa..0xd
  military). F903's `sector::Drive` table gives the range power (`+0`), fuel
  modifier (`+4`) and name code (`+0xc`); `hyperspace::plan` already uses them,
  and `main::drive_state` now reads the fitted byte instead of the model's
  delivered statistics word. F270's `DATA_005089` energy scale is
  `laser::DRIVE_ENERGY` (class 2 0x7ff, class 3 0x3ff, ...), so fitting a drive
  changes the first HUD gauge. Military classes (>= 0xa) use the military tank
  and burn military fuel (`hyperspace::MILITARY_CLASS`).
- The drive's **mass** is the equipment record's `+0x8` (Class 1 10 t, Class 2
  25 t, ...); F390/F391 move it in and out of `DATA_008889`. F1533's separate
  `DATA_007210` table (10/25/45/...) and `sector::Drive.mass` (`+8` of the
  16-byte drive record: 6/12/24/...) are not the equipment mass; the former is
  the delivered-hull hold adjustment and the latter the drive-type record's
  third dword, which no ported routine reads.
- **Sublight speed is not a drive stat**: the hull's statistics `+0x0/+0x2`
  thrust limits set it (`ShipStats::limits`). The drive changes jump range,
  jump fuel/time and the energy gauge, not the thrust word.

### Mining machines (category 6, F423-F427)

- `DATA_009104` is the list count (ten records of 13 dwords, `DATA_009106`;
  `FUNC_000248` appends). `DATA_009105` is the available count; F427 returns it
  as the fitted value for **both** mining records, so an MB4 machine also lights
  the Vaccine row. F423/F424 refuse an eleventh record with 0x99d6
  ("Mining Machine list full"); `FUNC_000251` removes the last idle record and
  `FUNC_000249` compacts the list. F425 does **not** compare the row's code: the
  Vaccine row can remove an MB4 machine, and its removal price (0) is what F388
  refunds.
- The port keeps `Loadout.mining: [u8; 10]` (equipment codes) and derives the
  count. The 52-byte machine state (object, hopper, deployment) and the F243/
  F244 deployment UI are **not ported**; fitting/removing/refunding and the
  hold mass are. There is no market/hold difference between the two records
  beyond their 30 t / 20 t masses.

### Cabins (category 1, F399/F401)

- `DATA_008890` is the cabin count (F398 increments, F400 decrements);
  `DATA_008892` is the occupied count. F401 refuses with 0x98fc ("You cannot
  remove an occupied cabin.") when every cabin is occupied; an unoccupied
  cabin can always be removed. The port stores `cabins` and `occupied_cabins`
  and exposes `cabin_capacity`/`free_cabins`; passenger missions that would
  raise the occupied count are not ported, so it stays 0.
- Each cabin is 5 t (record 25) and fits without a numerical cap in the source;
  `Loadout::validate` keeps the port's conservative 100.

### Shield generators (category 4, F413/F415)

- Each generator adds exactly 0x40 to `ship+0xe2` (F413) and F415 refuses below
  0x40. There is no cap in the source; the port's `validate` keeps 10.
  `Loadout::shield_max` is `count * 0x40`. Removal does not clamp the current
  shields (`ship+0xe0`), which is the source behaviour and can leave
  `current > maximum`; the HUD then reads the source's ratio.
- The statistics' shield word is at `+0xc` (the shipyard View page); `+0xe` is
  the ship's name code (`0x4000 | index`), despite `ShipStats::shield_strength`
  in `ffe-rs/src/ships.rs` reading `+0xe`. That mislabel is a known owner
  decision (the NPC target readout uses it); the player's maximum shields come
  only from F413's generators, and F1533 starts both shield words at 0.

### Fuel-scoop dependency (F394)

F394's code-4 branch tests the statistics word at `+4` bit 0x100 (byte `+5` bit
0), not a general bit: a model without the mounting gets 0x9903 ("Your ship does
not have a fuel scoop mounting."). The Saker Mk III (0x1f) has no mounting; the
Eagle (0x17) does not either, while the Asp/Imperial Courier family do. The
code-0x0d branch (Cargo Scoop Conversion) still needs the Fuel Scoop general bit
(0x9904). `ShipStats::fuel_scoop_mounting` carries the byte and
`outfitting::Limits::fuel_scoop_mounting` gates the row.

### Mass and hold (F390/F391)

`Loadout::mass` sums every fitted record's `+0x8` (general bits, cabins, lasers,
shield generators, missiles, the drive and the mining machines). F390 subtracts
it from `DATA_008889` on fit and F391 adds it back on remove; the free-space
check is `DATA_008889 - DATA_008891`. The port's `Loadout::capacity(base,
delivered, lasers)` applies the **difference from the delivered loadout**:
F998's new game already prices its 6 t (plus the drive) into the 1 t
`Navigator::hold`, while a hull bought later (F441 clears the equipment) uses the
model's cargo statistic. `trade::capacity` is the single reader used by the
market, the Upgrades free-space check and the cash/cargo/cabins line. This is
why the Saker starts with a 1 t hold and why removing the delivered 10 t drive
frees 10 t (the source's F391 quirk, because F998 never charged for it).

## The new game's fitted loadout (`F998`, `ffe.asm:74028`)

The DOS default is F998's **type-0** arm: the Saker Mk III (model `0x1f`).

- `ship+0xc8 = 0x40240020`: Scanner (bit 18), Automatic Pilot (21),
  Atmospheric Shielding (30) and Navigation computer (5). Bit 5 is the only
  general bit the type-1 Ross 154 arm does not set; there is no Combat
  Computer, radar mapper, ECM, chaff or cargo life support.
- `ship+0xd6/+0xd7 = 0x82` twice: two KL760 Homing Missiles, image 2 on F1120's
  pylon records 0x3b/0x3c.
- `ship+0xd2 = 0x88`: the 1MW pulse laser in the front mount (see
  `docs/notes/combat.md` §4).
- no shield generator (`+0xe2 = 0`), so `Loadout::shield_max()` is 0 and the HUD
  shield line stays hidden; no passenger cabins (`cabins = 0`).
- cash `DATA_008888 = 0x2710` (10,000 credit units). The F2 page's free hold is
  1 t: the Saker's 23 t statistic less F998's `0x10` allowance and the 6 t of
  fitted items (laser 1, missiles 2, Scanner 1, Automatic Pilot 1, Atmospheric
  Shielding 1; Navigation computer 0).
- The type-1 arm uses `0x40240000` (the same three bits without the navigation
  computer) and the same missiles/combat words.

`Loadout::new_game()` is the default type-0 record. Saves from before the field
load with `Loadout::default()` (nothing fitted).

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

### Combat equipment bits (2026-10-25)

| bit | record (code) | use |
|---|---|---|
| 7 | Auto targetter (`0x17`) | F1054's `T`/`Shift+T` arm `DATA_007246`/`7247`; F1010 acquires under the crosshair |
| 9 | Missile Viewer (`0x19`) | `F10` (view 5) needs it and `player+0xef` |
| 11 | Chaff Dispenser (`0x1b`) | F1054's `c` needs it and `player+0x174` (no writer in the recovered build) |
| 19 | E.C.M. System (`0x03`) | F935 sets `DATA_008835 |= 4` |
| 23 | Naval E.C.M. System (`0x07`) | F935 sets `DATA_008835 |= 0xc` (also bit 3) |

Rust: `Loadout::{auto_targetter, missile_viewer, chaff_dispenser, ecm}`;
`ffe_core::equipment::{AUTO_TARGETTER_BIT, MISSILE_VIEWER_BIT, CHAFF_BIT,
ECM_BIT, NAVAL_ECM_BIT, Ecm}`. See `docs/notes/combat.md` §8 for the ECM model
table and the targeter/missile-view behaviour.

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

Status (2026-10-25, fuel/damage/repair slice): F953's shields-then-hull order,
F952/F950's subsystem roll, the six thruster words, F938's shield recharge and
hull auto-repair, F929's fuel/energy recovery, the fuel scoop and F444-F452's
repair actions are ported. The hull/shield HUD lines show the real words and
F938 now recharges them.

`F953` (`ffe.asm:71190`) is shared by the player and the NPC ships:
shields absorb the hit first (`+0xe0`), then the hull (`+0xe4`); a hit at least
the remaining hull sets it to 0 and F942 destroys the object. Only objects with
control byte `+0x87 == 3` (player) or `0xb` (AI) take this path.

- Rust: `Flight::{ensure_damage, hit, destroyed}` over
  `ffe_core::laser::{DamageState, apply_damage}` (the same F953 core the NPCs
  already use). `ensure_damage` sets the hull full from the model statistics,
  the shield maximum from the fitted generators and (on a new hull) fresh
  thruster words and damage.
- **F952/F950 subsystem damage.** `Flight::take_damage_with_systems` applies the
  hit and, on a hull hit at least a quarter of the remaining hull (or when
  F953's proportional draw fires), runs F950's roll: `>= 0x5555` damages a
  random fitted general item (F945/F933, `ship+0xcc`), `< 0x888` destroys the
  drive (drive 0, energy 0), `< 0x1c71` reduces it to class 1, otherwise one of
  the six thruster limit words is zeroed (F949). Messages are 0x98c9/0x98ca/
  0x98cb. F952's outer `F922(cargo space) < DATA_008889` split needs the
  player's combat score, which the port does not carry; the port runs F950
  directly (reported deviation).
- The six words (`ship+0xbc..+0xc6`) are Left/Right/Bottom/Top/Main/Reverse
  (`DATA_008935`'s names) and are the `ffe_core::thrust::Limits` the flight
  model clamps against, so a destroyed thruster removes that direction's
  thrust. `Flight::limits` feeds `OriginalFlight::set_limits` every frame.
  F929's engine failure (`engine_failed`) zeroes the limits without appearing
  as a damaged thruster.
- **F938** (`ffe.asm:70122`): shields regain `max * tics / 32` (halved without
  the Energy Booster, bit 28) per frame, capped at the maximum; the Hull
  Auto-Repair System (bit 31) adds `tics` hull points up to the full hull.
- **F929** (`ffe.asm:69365`): the thrust cost is
  `mass * (|x|+|y|+|z|) >> 9 * tics`; an emptied pool burns one tonne of fuel
  with the Auto Refueller (hydrogen below drive class 10, military fuel from
  class 10) for `0x20000000` energy. Without fuel the pool stays empty, F929's
  bit-0 warning latches and the thrust limits stay zero (message 0x9907).
  F930's shot path refuels through the same function when a shot empties the
  pool. The port's fuel is the hold's hydrogen/military-fuel slot, the same
  source F901 uses.
- **Fuel scoop** (`F1033`, `ffe.asm:76144`; equipment bit 20): a star contact
  within 30 km (`0x1c9c380` mm) at speed `>= 0x2328` adds one tonne of hydrogen
  to the hold when there is cargo space, message 0x9906. The recovered build's
  `+0x1e8` scoop timer has no visible decrement; the port carries the
  `0x2468a`-tic (about 3 s) cooldown itself.
- **Crash damage.** F575's tail damage (`mass * 4 * speed lost / 0x708`, at
  least mass/4) is now applied through `take_damage_with_systems` on a
  planetary crash, not only logged.
- **Service dates/counters.** `Flight::service_days` are `DATA_008926`/
  `DATA_008925`; `damage.service_counters` are `DATA_008924`/`DATA_008923`.
  F933/F934's daily roll (counter -1 a day, then `F922(0x100) > counter`)
  breaks a fitted item or the drive after the 500-day service interval; the
  Deluxe/Drive/Equipment repairs reset the counters and dates.
- F1104's death screen and F1098's escape capsule remain separate slices.

## Cache impact

None. The equipment table is 66 * 22 = 1,452 bytes of static data and is
generated into `ffe-core`, like the console table. The market goods tables were
already cached in v20; the cache is **v29** and the equipment table adds no assets.
The damage/repair slice adds no assets either (the saved words and the
`DATA_005303` table are computed or embedded like the equipment table).

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
- `ShipStats::shield_strength` in `ffe-rs/src/ships.rs` reads the statistics'
  `+0xe`, which is the ship name code (`0x4000 | index`), not a shield word; the
  shipyard View page and the model table use `+0xc`. The value reaches the NPC
  target readout and `npc::Spec`, so changing it is an owner decision (the
  player's own maximum shields do not read it: F413's generators set `+0xe2`).
- A hull bought in the port (F441) starts with an empty `Loadout`: the source
  sets `ship+0xd0` to the model's required drive class for free, which the port
  does not do yet, so the bought ship must have a drive fitted before a jump.
  F441's free delivered drive is the reason the port's first fit of a drive
  charges its 10 t against the hold where the source would not.
- F408's drive-service stamp (`DATA_008924`/`DATA_008926`), the mining-machine
  deployment state (F243/F244, the 52-byte records) and passenger missions that
  raise `DATA_008892` remain out of scope; the fitting, removal, refund and mass
  effects are in.
- Saves from before `Loadout::drive` adopt `Combat::drive` on load (the drive
  was not removable through the Upgrades screen then, so the combat byte is the
  only source). Missile pylons are **not** adopted: the previous `remove` could
  clear `Loadout.missiles` while `Combat.missiles` kept the pylon word, and an
  automatic adoption would resurrect a deliberately removed missile. A save
  version bump or an explicit migration is an owner decision.
- `DATA_005303` was checked against the DOS `FIRSTENC.EXE` at file offset
  `0xc2650`: its 32 `(name code, cost)` pairs give exactly `2.5 x` the
  equipment price (truncated) for every fitted general item, so the Rust
  `equipment::repair_cost` computes the value instead of embedding the table.


## Tests

- `crates/ffe-core/src/equipment.rs`: names/prices/mass/category/tech decoding,
  the bit formula, `GENERAL_MASK`, the default type-0 loadout (drive 2),
  fitted storage per category (including the shared mining list and the one
  drive byte), the ten-machine cap, F390/F391 mass and capacity, icon nibbles,
  availability/prices and validation.
- `crates/ffe-rs/tests/combat.rs`: new/old save round-trips, F953 damage order,
  destruction, the no-generator new-game state, and a fitted generator keeping
  existing damage.
- `crates/ffe-core/tests/galaxy.rs`: the record's `+5` is `DATA_008995` and the
  procedural classes read the records before `DATA_007110`.
- `crates/ffe-rs/src/outfitting.rs`: F387/F388's ten-times price and refund,
  every category handler (general/cabin/laser/drive/shield/missile/mining),
  cash/space refusals, mount/pylon/drive/machine limits, the occupied-cabin and
  integral-drive refusals, the fuel-scoop mounting/dependency gates, and the
  energy-pool clear on drive removal.
- `crates/ffe-rs/tests/fitting.rs`: the scripted fit -> effect -> remove
  sequence (drive mass/hold/energy gauge), per-category fit/remove/refund/mass,
  the ten-machine cap, the shared mining fitted flag, the occupied cabin and
  F394's two gates.
- `docs/evidence/fitting/`: `upgrades-drive.png`, `upgrades-mining.png`,
  `upgrades-cabin.png` (ignored `station_view` run at tech 0xfd) and
  `sequence.txt` (the scripted console trace).
- `crates/ffe-rs/src/station.rs`: F383's available rows at tech 0xfd/0x16,
  fitted flags, the Upgrades Fit/Remove hot areas, F444's damage/service rows
  and every F446-F452 charge and effect.
- `crates/ffe-rs/tests/damage.rs`: F950's four thresholds, F953's hull gate,
  the repair row costs/titles, ten-times charges and refusals, F450-F452's
  counters/dates, F938 recharge/auto-repair, F929 fuel/energy, the scoop cap,
  the daily servicing roll and a damage -> dock -> repair -> fly sequence.
- `crates/ffe-rs/tests/console_table.rs`: F991 equipment icons, F1120 lamps from
  the dword bits, and F998's two real missile icons.
- `crates/ffe-rs/tests/station_view.rs` (ignored, needs `FFE_WORLD_CACHE`):
  the Upgrades, buy-a-laser, empty-ships and repairs frames (including the
  damaged/repaired pair) and the current system's tech level.
- `crates/ffe-rs/tests/dashboard_view.rs` (ignored, needs
  `FFE_WORLD_CACHE`): `dashboard-equipment-empty.ppm` vs
  `dashboard-equipment-newgame.ppm` checks the two icon rectangles at
  (14,165) and (33,164).
