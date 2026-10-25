# The docked station and starport screens (F320-F362, F536-F558, F382-F445)

Status (2026-10-25): **the services root, shipyard root, stockmarket and the
Upgrades list are ported**. The equipment table and fitted loadout live in
`ffe-core::equipment` (embedded, cache still v26); the starport tech level is
now read from F869 (below), so F383's rows, F387/F388's prices and F386's
Fit/Remove buttons work end to end for general equipment, cabins, lasers,
shield generators and missiles. Drives and mining machines are listed but their
buttons report that the category has no port state. The repairs screen builds
F444's three service rows (their actions are not wired yet); the ship list and
the bulletin board's generated entries remain faithful empty states because
their tables/generators are not imported. The screen map, strings, layout, hot
areas and keys below are read from `jjffe-master/ffe.asm`/`ffedat.asm` and
checked against frames of the reference game (JJFFE 2.8a7 built from that asm;
see "Evidence").

Rust: `ffe_rs::station` (screen table, layout, hot areas, state),
`Screen::draw_dock`/`Screen::draw_market`, wired in `main.rs` by `DockState`.
Cache: the first slice needs **no new asset and no cache bump**: the station
name, the 33 commodity names and the market tables are already cached (v26);
the fixed screen strings are constants in `station.rs`. Cache v27 is needed
only for the remaining data listed at the end.

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
- **Bulletin Board**: empty, with the original's 0x98de console line. The
  port does not generate BBS entries yet, so it does not invent any.
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
  x 292. The service actions (F446-F452), the six thruster damage words
  (`+0xbc..+0xc6`) and the hull-damage rows need state the port does not carry
  yet, so a row button reports that the action is not ported.
- **New and Reconditioned Ships**: the original screen shell (title, 0x98f6
  header, Go Back). The shipyard list needs F622/F634's generated `+0x2e6`
  entries, which are not imported; F437's empty-state message (0x98ed) is the
  faithful content for a station with no hulls.

The commander/options screen is **not** a station service: there is no entry
for it in `F333`/`F382`. In the original it is the separate status screen
(the F2 console button); its commander/legal/Elite block starts at
`ffedat.asm` 5807 (`MILITARY STANDING` ... `MEDALS AND PERMITS`). I did not
map that screen's routine or layout in this slice, and nothing was invented
for it: it is a separate player screen, not part of the docked module. If the
owner wants it, the next step is to follow the F2 console action
(`DATA_005100`/`DATA_007684`) to its routine and port its panel.

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
   `DATA_006199..6200`) and the ship statistics; prices come from
   `DATA_005268..5272`. The model table is already `ffe_core::npc::SHIP_TYPES`
   (types 2/3 + 2*allegiance are the military/shipyard lists) and F634's 13-row
   cap is known, but the `DATA_005268..5272` price/exchange table and F438-F442's
   buy/view actions are not imported, so the port keeps F437's empty state
   rather than showing an unpriced list.
3. **Repairs/servicing**. The three F444 service rows are ported. Remaining:
   F446-F452's actions, the six thruster damage words (`+0xbc..+0xc6`),
   `DATA_005303`'s cost table and the hull-damage rows (0x9812-0x9814/0x9818);
   the port's `Flight` has hull/shield words but no thruster damage.
4. **Bulletin board entries**. `F622`/`F633` generate them from `F621` and
   `DATA_006170` (F636-F646), with the mission strings 0x981a..0x982c and
   names. This is also the mission/BBS slice; the port must port the starport
   record's BBS array (`+0x132`, 18 entries of 24 bytes), the
   `DATA_006170`/`DATA_006199`-style generator tables and the 0x981a..0x982c
   strings. None of those are in the current cache, so the port keeps the
   original empty state (0x98de) rather than inventing advertisements.
5. **Commander/options**: F2 status screen, outside the station module.

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
`Upgrades-ours.ppm`, `Upgrades-buy-laser.ppm`, `Ships-ours.ppm` and
`Repairs-ours.ppm`. It also asserts the current system's tech level (Mars
High's 0xfd), the 51-row F383 list, and F444's 3040/1900/1900 service rows at
mass 38.

What was not captured: the commander/F2 status screen (not part of the
station menu), a Thargoid system's services (the port has none), and a
starport's services under a different title (the layout is the same; the
difference is in `F650`'s pads, not the screen).
