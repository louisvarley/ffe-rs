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
  or when a negative price is more than the cash; otherwise the cash gains the price.

Prices and cash share one unit: the new game starts with 1000, shown as 100.0 credits. Sol's Daedalus
at the first visit: water 0.9, grain 38.0, precious metals 1735.6, gem stones 3002.6, radioactives -8.1.

## The playable

- Docked at a station of a system, the services root is the original's screen
  (`docs/notes/station.md`); Stockmarket opens the market: the original's column header, the
  legal goods (25 at Sol) with price, stock and the tonnes held, the Buy and Sell buttons and the
  original's cargo/cash/cabins line. Up/Down choose, Enter/`1`-`4` select, B buys a tonne, N sells
  one (Shift: ten, stopping at the first refusal); the left/right row buttons buy and sell too.
- The hold is the ship model's `+8` statistics word less the hyperdrive's mass (`Models::cargo_space`,
  the value F1533 stores at `+0x116` and F992 copies): 5 t for the default Eagle long range fighter.
  The original lowers it further for equipment (F911 and the new-game set-up); not modelled.
- A market is made on the first visit to a station from a stream seeded by the system code and the
  station (the same every run), and brought up to the day by the daily drift when seen again (at most 30
  days at once). Markets and the traffic records are not saved; the cash and the hold are.
- Not here: the black market (the illegal goods, which the original lists behind a toggle), mission cargo,
  the price events the original raises (`F48(0x18...)`), jettisoning, the equipment that changes the hold,
  and the exact interleaving of the random draws with the rest of the game. Starports (planet-side) are
  not dockable yet, so only stations have markets.

## Verification

- `crates/ffe-core/tests/market.rs`: ten synthetic-table tests (formulas, caps, the two draws, the daily
  quarter, buying and selling, refusals).
- `tools/reference/verify-assets.py`: the imported tables and names equal JJFFE's `DATA_006164..6169` and
  `DATA_006216..6253`, and the executable's own bytes at the recorded offset (run on 2026-10-24).
- All 329 table systems generate a market (population at most 9, danger at most 7, so every table index
  is in range); `tests/market_view.rs` (local-only) trades, drifts, saves and draws.
- **Verified against the executed original** (2026-10-24): `--market` in `headless.c` runs F619 and F620 with
  the game's own tables and generator on 8,192 random commodities, populations, dangers and flags;
  `tools/reference/verify-market.py` with `crates/ffe-rs/tests/market_reference.rs` finds every price, every
  stock and the generator state after each call equal (4,137 negative-variance and 4,051 illegal cases);
  repeat runs match, six tampered fields and an empty trace are rejected. Trace SHA-256
  `edf4725858dabdbbb857dd0ff782e3d3408d8d7db3839ff74058636e341b7d97` (CRLF, Windows build). F622's loop,
  F633's quarter-drift and F555/F556 are composed from those two routines and the assembly and are tested
  synthetically, not run against the original.
