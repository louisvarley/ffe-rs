# Station markets and trading (F619, F620, F622, F633, F555, F556)

`crates/ffe-core/src/market.rs`; cache v20 (`base:market/tables`); wired into the playable by
`ffe_rs::navigation::Navigator::market` and `ffe_rs::trade`. Read from `jjffe-master/ffe.asm`
(not only the `srctools` notes: they have the shape right and a few details wrong).

## Data

`DATA_006164..6169` (704 bytes at RVA `0xdde08` in floppy 1.02, matching JJFFE's declarations byte for
byte): the supply multipliers (8 dwords), the population price multipliers (10), the danger price
multipliers (8), the population stock multipliers (10), the danger stock multipliers (8), then 33
commodities of four dwords: base price, price variance, base stock, stock variance. Multipliers are
Q15 fractions (`0x7fff` is one). The 38 commodity names `DATA_006216..6253` (RVA `0xde970`) follow in
the asset, each with its zero; the first 33 are traded, the last five are mission items.

A system's flags per commodity come from its stock string (F870, `galaxy::Tables::market`): bits 0-2 the
supply class, bit 3 a negative price variance, bit 7 an illegal good. The 33rd flag is the quirk
`0x0d` (byte 32 of the buffer is the first byte of the next label), as the original has it.

## The formulas (all 32-bit, arithmetic shifts, as the assembly)

- **Price (F619)**: `v = pop_price[pop] * price_variance >> 15`; `v = supply[supply] * v >> 15`; negated
  if bit 3; `v += base_price`; doubled if illegal; `v = danger_price[danger] * v >> 15` (the low 32 bits
  of the product); `v > 0x7fff` becomes `0x7fff`, otherwise doubled (a capped price is **not** doubled);
  times `(draw & 0x3ff) + 0x4000 - 0x200`, then `<< 2` up to `0x1fffffff`, doubled up to `0x3fffffff`,
  else `0x7fffffff`; the result is `>> 16`. One draw of the generator (F1530).
- **Stock (F620)**: `v = supply * stock_variance >> 15`, negated unless bit 3; `v += base_stock`;
  `v = pop_stock * v >> 15`; `>> 3` if illegal; `v = danger_stock * v >> 15`; then a spread of
  `((draw & 0x3fff) + 0x4000) * v >> 15` whose sign is decided by a first draw and sized by a second
  (two draws either way); added to `v`, and zero when negative.
- **First market (F622)**: for each of the 33 commodities, the price, then the stock.
- **Daily drift (F633)**: the same draws per commodity (price, then stock), each stored value becoming
  `(new + 3 * old) / 4` truncating toward zero. F633 also generates missions and bulletin entries from
  the same stream, which the port does not have.
- **Buying one tonne (F555)**: refused when the hold is full, when the station has none, or when the
  price is positive and over the cash; otherwise the cash pays the price (a negative price pays the
  player: radioactives and rubbish), stock down, cargo up. **Selling (F556)**: refused with none held,
  or when a negative price is more than the cash; otherwise the cash gains the price. The messages are
  0x98e1..0x98e5 (refusals, each naming the commodity through `FF30`) and 0x98e6/0x98e7
  (`Buying <commodity>.` / `Selling <commodity>.`).
- **Black market (F454/F538)**: a `0x9829` bulletin advertisement opens F536's market with F537/F538's
  illegal-goods mask `0x80`: `movsx eax,byte [edi]; xor eax,[DATA_009055]; test al,0x80; jnz skip`.
  F454 sets the screen_title to 0x98d7, whose `0x920d` name code expands to the advertisement's
  company name (`F639` stores the seed in StrVars[3]); the ordinary market uses 0x98d6
  (`<station> Stockmarket`). F639's advertisement is the real one. F640's is the police's fake: its
  `+8` word is `-1`, not 0.
- **Fake advertisement (F554/F542/F543)**: on a market opened by the fake `0x9829` ad, every row
  action takes F554's `[DATA_009053] != 0` branch instead of F555/F556. F542 calls F237(5,
  station) (`Trading in illegal goods`, no F236 message because its flags are 0) and F543 adds
  `0x1388` (5000 tenths = 500.0 CR) to the starport's ledger; F554 returns, so no tonne moves.
- **Customs scan (F233 event 5)**: `random & 7 < DATA_009010` (the system record's byte `+7`,
  `galaxy::Details::bytes[2]`) and then, if any held commodity's stock flag has bit 7, F237(7)
  records `Smuggling illegal goods`: 2500 printed, 25000 in the ledger, and **no wanted bit**
  (`DATA_005039[7].flags == 2`). In the original the scan is triggered by the special-slot writes
  around F931/F941 (player fire) and F291 (jettison); the port exposes it as
  `Navigator::smuggling_scan` and runs it at launch from a station.

Prices and cash share one unit: the new game starts with 1000, shown as 100.0 credits. Sol's Daedalus
at the first visit: water 0.9, grain 38.0, precious metals 1735.6, gem stones 3002.6, radioactives -8.1.

## Source map

| source | what it is | port |
|---|---|---|
| `ffedat.asm:9476` `DATA_006164..6169` | supply/population/danger price and stock multipliers, 33 16-byte commodity records | `ffe_core::market::Tables` (cache `base:market/tables`) |
| `ffedat.asm` `DATA_006216..6253` | the 38 commodity names (33 traded, 5 mission) | `Tables::name` |
| `ffe.asm:60899` F869 / `60948` F870 | system record bytes and the stock string | `galaxy::{Details, Market}` |
| `ffe.asm:42500` F619 | price formula | `Tables::price` |
| `ffe.asm:42571` F620 | stock formula | `Tables::availability` |
| `ffe.asm:42654` F622 | per-station price/stock generation | `Market::generate`, `Navigator::market` |
| `ffe.asm:43279` F633 | daily `(new + 3 old) / 4` drift | `Market::daily`, `Navigator::market` (30-day cap) |
| `ffe.asm:34732` F536 / `34797` F538 | stockmarket screen, the `0x80` illegal mask and the 0x9832 row | `trade::listing_with`, `screen::MarketView` |
| `ffe.asm:31701` F454 | a `0x9829` ad opens the black market (`0x80`) with the 0x98d7 company title | `station::DockState::open_black_market`, `Navigator::company_name` |
| `ffe.asm:35348` F554 | row action: F542/F543 on the fake ad, else F555/F556 | `main` trade action, `trade::police_sting` |
| `ffe.asm:34909` F542 / `34963` F543 | trading-in-illegal-goods trap and its 5000-tenths charge | `npc::Legal::record(5)`, `Legal::charge`, `trade::police_sting` |
| `ffe.asm:35391` F555 / `35463` F556 | buy/sell one tonne/loop, cash/stock/cargo | `Market::buy`, `Market::sell` |
| `ffe.asm:18610` F233 | event-5 smuggling scan | `Navigator::smuggling_scan` |
| `ffedat.asm:4362` `DATA_005039` | offence rows 5 (`Trading…`, flags 0) and 7 (`Smuggling…`, flags 2) | `npc::OFFENCES`, `npc::TRADING_ILLEGAL`, `npc::SMUGGLING` |
| `ffedat.asm:6218` `DATA_005276..5279` / `7953` `DATA_005589..5601` | F542's police screen and its pay-fine rows | `trade::police_sting` message (the pay-fine UI is not ported) |

## The playable

- Docked at a station of a system, the services root is the original's screen
  (`docs/notes/station.md`); Stockmarket opens the market: the original's column header, the
  legal goods (25 at Sol) with price, stock and the tonnes held, the Buy and Sell buttons and the
  original's cargo/cash/cabins line. Up/Down choose, Enter/`1`-`4` select, B buys a tonne, N sells
  one (Shift: ten, stopping at the first refusal); the left/right row buttons buy and sell too.
- Selecting a `0x9829` "GOODS BOUGHT AND SOLD" bulletin advertisement (row button or Enter) opens
  F454's black market: the same screen with the illegal goods listed (33 at Sol) and the
  advertisement's company name as its 0x98d7 title. F640's fake advertisement is one of these; any
  row action there is F542/F543's 500.0 CR trading-in-illegal-goods trap (offence 5), not a trade.
- Launching from a station runs F233's event-5 customs scan. If the hold has any bit-7 commodity
  and the system's `+7` danger byte beats `random & 7`, the smuggling offence (7) records 2500
  printed / 25000 owed with no wanted bit. The scan's stream is seeded from the system, clock,
  offence count and illegal tonnage because the port does not keep the original's global stream.
- The hold is the ship model's `+8` statistics word less the hyperdrive's mass (`Models::cargo_space`,
  the value F1533 stores at `+0x116` and F992 copies): 5 t for the default Eagle long range fighter.
  The original lowers it further for equipment (F911 and the new-game set-up); not modelled.
- A market is made on the first visit to a station from a stream seeded by the system code and the
  station (the same every run), and brought up to the day by the daily drift when seen again. Every
  crossed day runs once (`ffe_core::calendar`, `Navigator::tick_days`; `docs/notes/time.md`); there
  is no 30-day catch-up cap, so a long acceleration or jump applies each missed day in order.
  Markets and the traffic records are not saved; the cash and the hold are.
- Not here: mission cargo, the price events the original raises (`F48(0x18...)`), jettisoning, the
  equipment that changes the hold, F542's pay-fine screen (the debt is only recorded), and the exact
  interleaving of the random draws with the rest of the game. Starports (planet-side) are not
  dockable yet, so only stations have markets. The black market/F233 scan above are ported.

## Verification

- `crates/ffe-core/tests/market.rs`: thirteen synthetic-table tests (formulas, caps, the two draws, the
  daily quarter, buying and selling, refusals and their exact messages, illegal tonnage, a scripted
  profit/loss sequence).
- `crates/ffe-core/tests/legal.rs`: the offence table and messages, including offence 5's free record
  plus F543's 500.0 credit charge and offence 7's 2500/25000 with no wanted bit.
- `tools/reference/verify-assets.py`: the imported tables and names equal JJFFE's `DATA_006164..6169` and
  `DATA_006216..6253`, and the executable's own bytes at the recorded offset (run on 2026-10-24).
- All 329 table systems generate a market (population at most 9, danger at most 7, so every table index
  is in range); `tests/market_view.rs` (local-only) trades, drifts, saves and draws. The day boundary
  itself (one drift per crossed day, acceleration equivalence) is `tests/timed_events.rs`
  (`docs/notes/time.md`). Its new cases cover Sol's 25 legal / 33 black-market rows, an illegal buy's
  real price/stock/cargo, F542/F543's fake-ad fine, F233's smuggling scan (clean hold never caught;
  two tonnes eventually caught at Sol's danger 5), and the black-market frame
  `docs/evidence/station/market-black-ours.png` (title "Harrison's Trading Company", all 33
  commodities).
- **Verified against the executed original** (2026-10-24): `--market` in `headless.c` runs F619 and F620 with
  the game's own tables and generator on 8,192 random commodities, populations, dangers and flags;
  `tools/reference/verify-market.py` with `crates/ffe-rs/tests/market_reference.rs` finds every price, every
  stock and the generator state after each call equal (4,137 negative-variance and 4,051 illegal cases);
  repeat runs match, six tampered fields and an empty trace are rejected. Trace SHA-256
  `edf4725858dabdbbb857dd0ff782e3d3408d8d7db3839ff74058636e341b7d97` (CRLF, Windows build). F622's loop,
  F633's quarter-drift and F555/F556 are composed from those two routines and the assembly and are tested
  synthetically, not run against the original.

## Remaining / owner decisions

- **F542's pay-fine screen** (0x984b "Pay fine", 0x9949 Sergeant's demand and F543's row list) is not
  drawn. The port records the 500.0 CR debt and prints a one-line message instead. The money is not
  taken (F236/F543 only record it, as with every fine, until the unported fines office).
- **The smuggling trigger** is not the original's special-slot event: DOS runs F233's scan when F931/
  F941 or F291 write slot 5, while the port runs it at launch. This is a deliberate call-site
  deviation; owner may prefer firing or jettison as the trigger.
- **The random stream** for the smuggling scan is seeded deterministically (system, clock, offence
  count, tonnage) because the port has no single global F1530 stream. Prices, stock and drift are
  unaffected.
- **Mission cargo and F641's special-offer commodity** are not carried; the five mission item names
  are parsed but not traded.
- **The equipment that lowers the hold** (F911 and the new-game set-up) is still not modelled beyond
  F998's fixed starting allowance.
