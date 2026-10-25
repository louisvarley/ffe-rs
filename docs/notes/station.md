# The docked station and starport screens (F320-F362, F536-F558, F382-F445)

Status (2026-10-25): **the services root, shipyard root, stockmarket, Upgrades,
New and Reconditioned Ships and the Bulletin Board are ported**. The equipment
table and fitted loadout live in `ffe-core::equipment` (embedded, cache still
v26); the starport tech level is now read from F869 (below), so F383's rows,
F387/F388's prices and F386's Fit/Remove buttons work end to end for general
equipment, cabins, lasers, shield generators and missiles. Drives and mining
machines are listed but their buttons report that the category has no port
state. The repairs screen builds F444's three service rows (their actions are
not wired yet). The shipyard's hull table is generated into
`ffe-core::shipyard` from the DOS statistics blocks and `ffedat.asm`, so F437's
rows, F438/F442's Buy/part exchange and F634/F635's stock are real; the
Bulletin Board generates F621's advertisements from `DATA_006170` and the
0x981a..0x982d templates, and F633's daily update replaces entries. The screen
map, strings, layout, hot areas and keys below are read from
`jjffe-master/ffe.asm`/`ffedat.asm` and checked against frames of the reference
game (JJFFE 2.8a7 built from that asm; see "Evidence").

Rust: `ffe_rs::station` (screen table, layout, hot areas, state),
`Screen::draw_dock`/`Screen::draw_market`, wired in `main.rs` by `DockState`.
Cache: **no new asset and no cache bump**: the station name, the 33 commodity
names, the market tables (v20), the model statistics (v12) and the system
tables (v15) are already cached (v28); the screen strings, the shipyard table
and the bulletin templates are constants in `ffe-core`/`station.rs`. The
shipyard generator reads the existing `base:ship/stats` asset; no cache version
change.

## The screen tables

The BBS module is game module 12; its event table is `DATA_005285`
(`ffedat.asm` 6296): event 0 `F320`, event 1 `F343`, event 2 `F321`, event 3
`F354`, event 4 `F357`, event 8 `F339`. `F343` (ffe.asm 26770) shows the
services screen when the player state (`DATA_008870`, `D8870`) is `0x2a`
(docked at a station) or `0x30` (landed at an open-air starport); otherwise it
shows the comms list.

`F331` (26336) sets the current screen and `F327` (25969) renders it. Each
screen is a 40-byte row in `DATA_005304` (`ffedat.asm` 7328): header ffcode,
header x, header height, extra-string x, entry height, two button x positions,
entry text x, back flag, entry type. The rows (index 0..8) are:

| index | screen | header | entry x | pitch | buttons | back | name |
|---|---|---|---|---|---|---|---|
| 0 | services root (`F333` 26408) and shipyard root (`F382` 28443) | - | 140 | 33 | 1 | no | 0x98d2/0x98d3 |
| 1 | comms (`F343`) | 0x98da Suggested Responses | 140 | 10 | 1 | no | 0x98d4 |
| 2 | bulletin board (`F453` 31582) | - | 140 | 33 | 1 | yes | 0x98d5 |
| 3 | stockmarket (`F536` 34732) | 0x98e0 | 152 | 10 | 2 | no | 0x98d6 |
| 4 | upgrades (`F383` 28564) | 0x98f5 | 152 | 10 | 2 | yes | 0x98d3 |
| 5 | ships (new/reconditioned) (`F445` 31254) | 0x98f6 | 152 | 10 | 2 | yes | 0x98d3 |
| 6 | ship view (`F493` 33190) | 0x98f6 | 152 | 10 | 2 | yes | 0x98d3 |
| 7 | bulletin ad (`F453`/`F523` 34311) | 0x98da | 140 | 33 | 1 | ? | 0x98d5 |
| 8 | repairs/servicing (`F436`/`F385` 28696) | 0x98f4 | 140 | 10 | 1 | 0x980a | 0x98d3 |

Entries are `F345`/`F348` records of 48 bytes; `F329` (26157) draws the
button(s) with `F328` (26118) and then the entry text. The button bitmap is
`0x2e`, 8x9, at x 124 and (for the two-button screens) x 136. `F330` (26283)
draws the down arrow `0x30` at (124, 147) while entries remain below the
window, the up arrow `0x2f` at (136, 147) once scrolled, and "More..." at
(148, 148). `F566` draws the Go Back button `0x2e` at (200, 147) and
"Go Back..." at (210, 147).

Geometry, from `F327`/`F329`/`F330` and the captures:

- The screen name (`D9052`, message 0x98d2/0x98d3/0x98d5/0x98d6) is drawn at
  (4, 6) in colour 0x7c.
- The column header is at y 2 in colour 0x7d; entry text starts at the row's
  y + 1 in colour 0x7c, and a second line is +10.
- The first entry is at y 15 (the header height, `DATA_005306`); the loop stops
  before y 148. Four 33-pixel rows fit (15, 48, 81, 114) or thirteen 10-pixel
  rows (15..135).
- `F558` (35547) draws the cargo/cash/cabins line from message 0x98e8:
  `Cargo` at (1, 138), `used : capacity` at (44, 138), `Cash` at (82, 138),
  `Cabins` at (1, 148), `used : total` at (44, 148) and the cash right-aligned
  to x 96 on the second line.

The **launch action** is row 0, "Launch Request" (0x9819). `F361`/`F360`
dispatch it through `DATA_005288[0x19]` to `F381` (ffe.asm 27790 region), which
asks F654 for permission and starts the 0x1a lift stages; the lift itself is
`F1088` (79830) and the release is `F363` (27768) / `F651` (44537). The port's
`Navigator::begin_launch` is that sequence (`docs/notes/docking.md`).

The **starport variant** is the same screen: `F343` tests `0x2a`/`0x30` and
`F333` tests only the system allegiance, not the object kind. The open-air
difference is in the pad allocation (`F650` uses `DATA_006175..6179`) and in
the launch/landing wording, not in the menu. `F333` omits Shipyard, Bulletin
Board and Stockmarket for a Thargoid system (`w[D8996] == 4`); the port has no
Thargoid system.

**Entering and leaving it from state `0x30`.** Plain F4 is `DATA_007688`'s
`0xf4` (the unshifted half of the table; the shift byte is `0x80` when up), and
`DATA_004893` entry 12 sends it to the station module's event 1, `F343`. At
`0x30` F343 sets the console buttons, calls `F333` and leaves the state at
`0x30`; the board is drawn over the landed view with the flight console below.
F4 again re-displays the root. The way back is the console's screen button
`0xf1` (plain F1: module 3 event 1, `FUNC_001003`, which switches back to the
game view); the port also accepts Backspace from the root. The reference
captures are `docs/evidence/start-flow/original-services.png` and
`original-services-back.png`; the port's are the `ours-*` pair.

## The starport tech level (F869/F622)

`F869` (`ffe.asm:60899`) returns the system record's fields; `F622`
(`ffe.asm:42654`) stores the byte at record `+5` in `DATA_008995`, then copies
`+6` to `DATA_008993`, `+7` to `DATA_009010`, `+8` to `DATA_008994`, `+9` and
the word at `+10` to its locals. The record base is `DATA_007106 + 54 * index`,
which is the galaxy record at `DATA_007110` for an in-table system and one of
the eight class records before it for a procedural one (F868 returns -1..-8).
`ffe_core::galaxy::Tables::details` already returns those bytes as
`Details::bytes[5]`; `Details::tech_level()` and `Tables::tech_level()` name
`bytes[0]` as the tech level, and `Navigator::tech_level` reads it for the
current system. No new cache asset or cache bump is involved.

The reference Sirocco frame was captured at a tech-0x75/0x88 system: the first
thirteen rows are exactly the records at or below that tech in
`DATA_005289` order. The port's frame in the sandbox is Mars High (tech 0xfd),
which offers 51 records. `F383`'s price block is commented `; FIX: incorrect
tech prices` in the recovered asm; the existing `equipment::tech_multiplier`
keeps the documented monotonic 9/10/11/12 thresholds, which is the reported
deviation from the asm's descending compares.

## Strings

The `0x98xx` messages are `DATA_005299[code & 0x1ff]` (`ffedat.asm` 6778),
handled by `F319` (ffe.asm 25633) and `F1347`. The ones these screens use:

| code | string |
|---|---|
| 0x980a/0x980b/0x980c | Shipyard / Bulletin Board / Stockmarket (services rows) |
| 0x980d..0x9810 | Upgrades / Repairs and Servicing / New and Reconditioned Ships / Contact Local Police |
| 0x9819 | Launch Request |
| 0x98d2..0x98d6 | `{station} Services` / `{station} Shipyard` / `{station} Traffic Control` / `{station} Bulletin Board` / `{station} Stockmarket` |
| 0x981a..0x982d | BBS advertisements (WANTED/SHIP REQUIRED/GOODS BOUGHT.../military/crew/charity); the generator templates, embeds 0x9918..0x991c passage descriptions, 0x99b4..0x99c3 removal titles and 0x99d7..0x99de charities |
| 0x98dc | Which service do you require? |
| 0x98dd/0x98de | Please choose advertisement... / Sorry, no advertisements have been placed. |
| 0x98df | Choose Buy or Sell to trade one unit. |
| 0x98e0 | Buy Sell Item / Price per t / Stock / Cargo |
| 0x98e8 | Cargo/cash/cabins template |
| 0x98f4/0x98f5/0x98f6 | Repair Item / Cost; Fit Remove Item / $ to Fit / $ for Removal / Wt; Buy View Ship / Cost / Pt.Exchg / Cap. |
| 0x9889/0x9890/0x9894 | Choose Fit or Remove...; Select Buy to part exchange...; Select desired repair... |

The market's entry template is 0x9832 (`F538` 34797): the commodity, price,
stock and cargo. The stockmarket header is 0x98e0; the column tab positions
read from the capture are x 120, 213, 255, 286 for `Buy Sell Item`, `Price per
t`, `Stock`, `Cargo`.

The DOS screen prints the credit unit as the font's currency glyph. The
port's 116-glyph cache maps `$` to the same glyph, so `$100.0`/`$1.0` render
exactly as the original's currency mark (checked in the capture).

## Keys and mouse

`F354` (27228, event 3) reads the GUI action and `F361` (27683) dispatches the
entry:

- keys `1`..`4` are actions 0x31..0x34 and activate the visible row with that
  number (row = first visible + digit - 1);
- the up/down arrows are actions 0x8f/0x9f and call `F355(±1)`, which moves the
  first visible row (`D9050`) one entry;
- action 3 is the Go Back arrow (`F362` 27720), drawn only on screens whose
  table has a back flag (bulletin, upgrades, ships, repairs).
- mouse clicks use the `F328` hot areas: 8x9 on each row button; the left
  button is action row+0x80, the right row+0x90. A click on the text is not a
  hot area in the original. On the market the left button buys and the right
  sells one tonne; on the equipment/ship screens they are Fit/Buy/Remove/View.

The port keeps the original row buttons and arrows, and additionally accepts
`Up`/`Down` for the row and `Enter` to activate a menu row, `Backspace` to go
back, and `B`/`N` to buy/sell the selected market line (the old playable's
keys, kept so the existing playtest still works). `L` still launches from
anywhere while docked. These are called out as port conveniences in
`docs/DEVIATIONS.md` if the owner wants them listed there.

## The port's screens

- **Services root** (`F333`): Launch Request, Shipyard, Bulletin Board,
  Stockmarket. `L`/the first row calls `Navigator::begin_launch`, which is
  F654 and the 0x1a launch stages.
- **Shipyard root** (`F382`): Upgrades, Repairs and Servicing, New and
  Reconditioned Ships, Contact Local Police. The last is reported as not
  ported (fines), not drawn as a fake screen.
- **Stockmarket** (`F536`/`F538`): the cached legal goods in `trade::listing`
  order, 13 rows with the Buy and Sell buttons, the original's column header
  and the cargo/cash/cabins line. Buying and selling keep the verified F555/
  F556 core. The market has no Go Back arrow in the original; `Backspace`
  leaves it in the port.
- **Bulletin Board** (`F453`/`F621`/`F622`/`F633`): the 18-entry `+0x132`
  board is generated on first use from `DATA_006170` (`F621`: 32 slots, 18
  stored) and the 0x981a..0x982d templates. F622 makes `population * 4 + 1`
  attempts; F633's daily roll replaces one entry when `random(7) < population`.
  The four 33-pixel rows wrap at 177 pixels and the up/down/More arrows page
  them; selecting a post shows its full text on the console (F523's mission
  reply actions are not ported). The goods, military, passenger/parcel,
  removal, crew and charity generators are ported; the recent-systems wanted
  board (F636), missing-person (F637) and special-offer commodity (F641)
  generators need state the port does not keep and return no entry.
- **Upgrades** (`F383`/`F386`): the rows are `equipment::outfitting` at the
  current system's F869 tech level, in `DATA_005289` order, with the 0x98f5
  header, the measured x 242/272/302 price/removal/Wt columns and the 0x2e row
  buttons. F329's second (Remove) button is drawn only for a fitted row. The
  left button/`1`..`4`/Enter calls `outfitting::fit`; the right button calls
  `outfitting::remove`; both charge/refund ten times the row's whole-credit
  price, because the player's cash is in tenths. General bits, cabins, lasers,
  shield generators and missiles apply to `Loadout`/`Combat`; drives and
  mining machines have no port state and report that instead of inventing one.
- **Repairs and Servicing** (`F443`/`F444`): the three service rows
  (Deluxe 80*mass, Drive Only 50*mass, Equipment Only 50*mass) with the
  0x9815-0x9817 strings, the stored service dates (`Flight::service_days`,
  `DATA_008926`/`DATA_008925`) and the reference frame's `$` cost column at
  x 292. F444's damage rows are built from real state: `0x9813` (250 kg for
  $10), `0x9818` (the whole hull at $10 per hull point = $40/t), `0x9814`
  (`Repair <Left/Right/Bottom/Top/Main/Reverse> thruster`, $100) and `0x9812`
  (`Repair <item>`, `DATA_005303` = 2.5 x the equipment price, e.g. Fuel Scoop
  $875). The row buttons run F446-F452 (`station::apply_repair`): they charge
  ten times the whole-credit cost (cash is in tenths), restore the hull point
  or words, clear the `ship+0xcc` damaged bit and set `ship+0xc8`, and reset
  the Deluxe/Drive/Equipment 500-day counters and dates. Messages are
  0x98f1-0x98f3; the refusal is the port's "You cannot afford ..." wording
  (F440's 0x99e4 is "You cannot afford a new <item>").
- **New and Reconditioned Ships** (`F437`/`F438`): F622/F634/F635's `+0x2e6`
  stock (13 hulls at most, from `DATA_006184..6198` and `DATA_006193`) with
  F437's columns. Cost is the statistics word at `+0xa` times 1000; Pt.Exchg is
  Cost less `950 * (drive mass + word[+0xa] - drive mass[required drive])`; Cap.
  is the `+0x8` cargo word; the name is the `0x4000 | word[+0xe]` ship-string
  code expanded through `DATA_004682`. The left button/digit/Enter buys
  (F442/F441): the cash check is ten times the whole-credit net price, the new
  model is installed, all fitted items are cleared and the bought row is
  replaced by the player's old hull. The right button prints F439's statistics
  on the console (the View page itself is not drawn). The table is generated by
  `tools/gen-shipyard-table.py` from `ffedat.asm` and the cache's
  `base:ship/stats` (the DOS `FIRSTENC.EXE` blocks); see
  `docs/notes/equipment.md` for the statistics layout.

The commander/options screen is **not** a station service: there is no entry
for it in `F333`/`F382`. In the original it is the separate status screen
(the F2 console button); its commander/legal/Elite block starts at
`ffedat.asm` 5807 (`MILITARY STANDING` ... `MEDALS AND PERMITS`). I did not
map that screen's routine or layout in this slice, and nothing was invented
for it: it is a separate player screen, not part of the docked module. If the
owner wants it, the next step is to follow the F2 console action
(`DATA_005100`/`DATA_007684`) to its routine and port its panel.

## Shipyard and BBS source map

| source | what it is | port |
|---|---|---|
| `ffedat.asm:6118` `DATA_005268..5272` | the arrays of ship-name-list pointers F382 selects (`sh1`.., `fsh1`.., `ish1`..); **not prices** | `station::SHIPYARD`, F634's lists (price comes from the statistics block) |
| `ffedat.asm:9622` `DATA_006199/6200` | the 19 `{count, pointer}` ship-type lists (`DATA_006184..6198`) | `ffe_core::npc::SHIP_TYPES` |
| `ffe.asm:30441` F437 | the list rows: cost, trade-in, capacity, name | `ffe_core::shipyard::{row, trade_in}` |
| `ffe.asm:30561` F438 | Buy (bit 0x10 clear) or View (set) | `station::Hot::Ship`, `main::buy_ship/view_ship` |
| `ffe.asm:30738` F440/F441/F442 | insufficient cash, loadout reset, part exchange | `shipyard::buy/part_exchange`, `main::buy_ship` |
| `ffe.asm:43543/43576` F634/F635 | append a hull (1/8 skip, 13 cap) and pick a model | `shipyard::stock/daily/pick` |
| `ffedat.asm:7266` `DATA_005300` | drive masses for the trade-in | `shipyard_table::DRIVE_MASS` |
| `ffedat.asm:9443` `DATA_006170` | 32 BBS generator slots F621 indexes | `bulletin_table::GENERATORS` |
| `ffe.asm:42635` F621 | dispatch `F922(0x20)` to a generator | `bulletin::generate_one` |
| `ffe.asm:42654` F622 | make the BBS/stock for each starport (`population * 4 + 1` attempts) | `navigation::station_offers` |
| `ffe.asm:43279` F633 | daily BBS/stock update | `bulletin::daily`, `shipyard::daily` |
| `ffe.asm:43626..44257` F636-F646 | the generators and `F646`'s six-dword entry | `bulletin::{goods, military, ship_job, removal, crew, charity}` |
| `ffedat.asm:6778` `DATA_005299` | the 0x981a..0x982d templates and 0x99xx embeds | `bulletin_table::{TEMPLATES, STRINGS}` |
| `ffedat.asm:9476..9505` `DATA_006171..6174` | passage multipliers, extra danger money and danger multipliers | `bulletin::{PASSAGE, DANGER_}*` |
| `ffedat.asm:28488` `DATA_008999` | 18 starport records: `+0x130` object, `+0x131` flags, `+0x132` BBS (18 x 24), `+0x2e2` count, `+0x2e6` hulls, `+0x31e` hull count | `navigation::StationOffers` |

## Remaining (with the data)

1. **Equipment (upgrades)**. Ported: the `DATA_005289` table (66 records, 22
   bytes) is embedded in `ffe-core::equipment`, F869/F622's `DATA_008995` tech
   level is read through `galaxy::Tables`, and `station::Upgrades` builds F383's
   rows with `equipment::outfitting`; `outfitting::fit`/`remove` implement
   F386's cash/space checks and the general, cabin, laser, shield and missile
   handlers. Remaining: drive (`F409`/`F411`) and mining-machine state, and the
   second-hand random branch of F383's prices (see `docs/notes/equipment.md`).
2. **Shipyard ships**. `F445` reads the station's generated entries
   (`+0x2e6`, up to 13 `{model, index}` words from `F634/F635` and
   `DATA_006199..6200`) and the ship statistics. `DATA_005268..5272` is not a
   price table: it is the array of ship-name-list pointers (`sh1`.., `fsh1`..,
   `ish1`..) F382 selects. The price is F437's `word[+0xa] * 1000` and the
   trade-in is its `DATA_005300` formula; both are now in
   `ffe-core::shipyard`/`shipyard_table.rs`. Ported: the stock, rows, buy and
   part exchange, and a console View summary. Remaining: F439's full View page
   (F991-style stats panel) and F441's passenger/registration side effects.
3. **Repairs/servicing**. Ported: F444's three service rows and the hull,
   thruster and item rows; F446-F452's actions, charges, dates and counters;
   the six thruster damage words (`+0xbc..+0xc6`); `DATA_005303`'s costs
   (= 2.5 x the equipment price). Remaining: F952's outer
   `F922(cargo space) < DATA_008889` cargo split (the port has no combat score
   word), F440's 0x99e4 wording, and the smoke/particle timers (`+0x14e`/
   `+0x14f`).
4. **Bulletin board entries**. Ported: `F622`/`F633`'s generation and daily
   update, the `+0x132` 18 x 24-byte array, `DATA_006170`, the templates
   0x981a..0x982d and the embedded 0x99xx name/passage strings. Remaining:
   F636's recent-systems wanted board, F637's missing-person destination picker
   (F857's grid perturbation) and F641's special-offer commodity table, plus the
   mission reply actions F524-F532 (accept/decline, permits, advance) and the
   BBS entry's service flags. Those generators return no entry rather than
   inventing one.
5. **Commander/options**: F2 status screen, outside the station module.

## Differences and uncertainties

- **Ship display names are not the model-file names.** The cache's
  `base:text/model-names` (`DATA_004682` read as model-slot names) is offset
  from the shipyard's display name: model 0x2b's statistics `+0xe` is
  `0x4028`, which `F1345` expands to "Spar attack fighter", while
  `model-names[0x2b]` is "Turner Class". `ffe-core::shipyard` follows F437 and
  the `0x4000 | index` code, which is what the reference frame shows. The
  `model-names` use by `ships.rs` is unchanged here.
- The statistics block's `+0xe` is the ship-name ffcode (F437, F439 and the
  in-flight ship list F282 all expand it). `crates/ffe-rs/src/ships.rs` calls
  `+0xe` "shield strength"; the shipyard table reads the shield word at `+0xc`
  (150 for the Cobra Mk III), which is the consistent field. This is a
  pre-existing label in the flight slice, flagged for its owner; nothing in
  this slice changes it.
- F639/F640's hidden "goods bought and sold" availability byte (`DATA_008990`
  and the 37-byte stock string) is not carried by the port: F639 always offers
  the notice and F640 offers it on one draw in two. The template and company
  name generator are the originals.
- The original draws the stock and BBS from its one global random stream,
  interleaved with traffic and everything else. The port seeds a station stream
  from the system code and station index (like `StationMarket`), so the *same
  station* is stable but its hull names and post mix differ from a reference
  capture. Prices, capacities, templates and generation counts are the
  original's.
- `F437`'s Cost uses the `+0xa` word, not the `+0x6` hull mass. The two differ
  (Eagle 0x17: 38 vs 25), and the reference frame confirms 38,000 for the
  Eagle-sized row.
- The table bytes were compared through the cache: `base:ship/stats` is
  generated by `ffe-tools build-data` from the DOS `FIRSTENC.EXE` (floppy 1.02)
  and the shipyard script reads those blocks. The reference frame's Sirocco
  rows (Spar 71000/34900/37t, Hawk 48000/11900/7t, Constrictor
  143000/106900/90t, Lion 348000/311900/235t, Cobra Mk III 124000/87900/80t,
  Puma 1265000/1228900/825t, Tiger 472000/435900/320t) match the generated
  table exactly with the new game's 36,100 trade-in. The JJFFE listing's ship
  *statistics* are pointers into its executable, so there is no second numeric
  listing to diff; the model lists `DATA_006184..6198` match
  `ffe_core::npc::SHIP_TYPES` entry for entry.

## Evidence

The reference harness (`jjffe-smoke --game-frame <frames> <scan codes...>`)
reaches the real screens: F4 (scan 0x3e) is the comms/services action, then
`4` (0x05) Stockmarket, `2` (0x03) Shipyard, `3` (0x04) from the shipyard
Bulletin Board/Upgrades, etc. The smoke adapter cannot decode the station's
AVI, so a local (ignored) copy of `headless.c` with `DirMakeAVIName` returning
a missing path was used for the captures; the game then draws the menus
instead of failing at frame 231. These are JJFFE 2.8a7 frames (the reference),
not the DOS executable itself; the recovered asm's tables and strings agree
with them, and no JJFFE-vs-DOS menu difference was seen, but the DOS binary
was not run for this comparison. Captures (320x200, palette applied) are in
`docs/evidence/station/`:

- `services.png` — Sirocco Station Services, four rows at y 15/48/81/114.
- `stockmarket.png` — Sirocco Station Stockmarket, 13 rows, header at y 2.
- `shipyard.png` — Sirocco Station Shipyard, four rows.
- `equipment.png` — Upgrades list with Fit/Remove buttons.
- `ships.png` — New and Reconditioned Ships list.
- `repairs.png` — Repairs and Servicing list.
- `bulletin.png` — Sirocco Station Bulletin Board with generated ads.

The port's own frames are produced by
`cargo test -p ffe-rs --test station_view -- --ignored` with
`FFE_WORLD_CACHE` set and `FFE_SHOT_DIR` pointing at a directory; the test
checks the title/header/row pixels and saves `Services-ours.ppm`,
`Shipyard-ours.ppm`, `Market-ours.ppm`, `Bulletin-ours.ppm`,
`Upgrades-ours.ppm`, `Upgrades-buy-laser.ppm`, `Ships-ours.ppm`,
`Ships-buy.ppm`, `Repairs-ours.ppm`, `Repairs-damaged.ppm` and
`Repairs-repaired.ppm`. It also asserts the current system's
tech level (Mars High's 0xfd), the 51-row F383 list, F444's 3040/1900/1900
service rows at mass 38, a populated BBS (with every 0xff code expanded and no
`[0x....]` fallbacks), a populated ship list at F634's 13-hull cap, and the buy
sequence: ten times the net whole-credit charge, the new model, and the old
hull replacing the bought row. The ship rows use the new game Eagle (model 23)
for the trade-in, so the frame's Pt.Exchg values follow the reference rule
(Cost - 36100). The damaged/repaired pair uses a hull at 140/152, the Fuel
Scoop's damaged bit and the Main thruster zeroed: the list grows to seven rows
(three services, both hull rows, Main and Fuel Scoop) and returns to the three
service rows. The committed PNGs are
`docs/evidence/station/repairs-damaged-ours.png` and
`repairs-repaired-ours.png`.

What was not captured: the commander/F2 status screen (not part of the
station menu) and a Thargoid system's services (the port has none). The
default start's starport services at Old Blackelk are captured in
`docs/evidence/start-flow/` (`default_start_services_board_is_old_blackelk`).
