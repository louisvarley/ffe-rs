# The galaxy's stars, system positions and the hyperdrive (F833, F841, F853, F856, F861, F899-F903)

`crates/ffe-core/src/sector.rs`, `hyperspace.rs`; cache v21 (`base:world/galaxy-tables`); wired into the
playable by `Navigator::{system_position, range_to, jump_report, hyperspace}`. Read from `jjffe-master/ffe.asm`
(the `srctools` notes have the shape but get F861's second rotation and a few details wrong).

## The galaxy

A system is a 32-bit code: bits 0-12 the sector x, 13-25 the sector y, 26-31 the star number in the sector
(`galaxy.rs`). The sectors are a 128 by 128 grid of 64 sectors a cell; 49 sectors around Sol are hand-made.

- **Star count (F853)**: the 128 by 129 byte density grid is interpolated bilinearly at the sector (fractions of 15
  bits), `* 2`, plus the corner value `<< 16`, `>> 8`; a hash of the fractions and the value picks a factor from the
  128-entry table, and `value * factor >> 16` is the count `<< 10`. A hand-made sector's count is its index
  difference. Sectors at or beyond 0x2000 hold nothing.
- **Stars of a sector (F841)**: a generated sector runs the F861 generator seeded by the sector key (`a` the key with
  its halves swapped, `b` the key), `a' = rotl(a, 3) + (a + b)`, `b' = rotl(a + b, 5)`; three steps are thrown away,
  then each star takes a step and reads byte 2 from `a` bits 16-23, byte 3 from bits 8-15 (arithmetic `>> 9`) and byte 4
  from bits 1-8 (a signed byte `>> 1`); its type word is `DATA_006810[b & 31] << 8 | DATA_006811[(b >> 16) & 31]`.
  A hand-made sector copies its vectors and words from the table. At most 62 stars.
- **Position (F856)**: x from byte 4 and y from byte 3, each the sector in the high half plus `byte * 512 + 0x8000`
  (low half), z from byte 2 with a rounding quirk (a positive byte gets one more).
- **Distance (F833)**: the three differences `>> 8` (absolute), halved together until each is at most 0x7fff, the
  square root of the sum of squares (a sum over 31 bits reads as negative and gives zero), `* 0xc8b4`, `>>` the
  halvings. The unit is a hundredth of a light year: Sol to Alpha Centauri is 4.35, Sirius 8.68, 61 Cygni 11.2.

## The hyperdrive (F903)

A drive of class `c` (15 records of power, fuel modifier, mass and name) reaches `power / ship mass`. A jump of `d`
needs `d * mass / 0x7531 + 1` tonnes less the modifier's share (at least one) and takes
`((d << 16) - 1) / range * 7` units of 2^-32 days, so a jump at the drive's full range takes about seven days. The
player's fuel is **cargo**: hydrogen (cargo slot 19), and for class 10 and up military fuel (slot 20), which turns
into the same tonnes of radioactive waste (slot 29). Hydrogen leaves the hold's used space too. The Eagle (class 10,
mass 16) reaches 18.75 light years.

## The playable

- The galaxy map (G) shows each star's distance and the fuel it needs, `TOO FAR` or `NEEDS n T`; H or Enter jumps. The
  map's selection (`F840`'s nearest star to the centre, `F832`'s `+0x25c..0x264`/`+0x27c`) is stored in
  `Navigator::destination`, **not** a body lock, and survives closing the map: F908's event 1 is the console's
  hyperspace record **0x0f**, key **H** (`docs/notes/labels.md`), so H jumps with the map open or closed.
- F908's gates are the port's: mode 0/0x24 (not landed, no take-off in progress), `DATA_008867 >= 0xb71b00` (the
  `Navigator::altitude` floor), and F903's range/fuel/class. A refusal uses the map's own strings
  (`Fuel Required: n`, `Insufficient Fuel (n required)`, `Out of Range`, `DATA_007004..7006`).
- The jump checks range and fuel, burns the fuel (F901's hydrogen, or military fuel leaving radioactive waste), moves the
  game clock by the jump time (markets drift, planets move) and arrives off the new system's first planet. F909 clears
  the old system's target and combat locks, missiles, hit particles/explosions/debris, traffic, station markets, docking
  and take-off state, then F905 regenerates the system's bodies; the scanner and the next map are rebuilt from those.
  A new game has no fuel: buy military fuel at the station's market.
- The test systems (Test-1 to Test-4) now have codes of real sectors near Sol, so they have positions of their own.
- Not here: the jump scene itself (F908's module mode 4, the `+0x106` visual timeout and F909's F148/F912 drawing),
  misjumps (F910, the 1 in 256 chance), the galaxy map's mouse hover pick, and the systems beyond the set on offer
  (the 12 hand-coded ones, four test ones and a new game's start-system neighbours). Since 2026-10-26 `Navigator::new_game` offers the
  start system's real galaxy-table neighbours within 30 LY (`navigation::galaxy_around`), generated from F852/F860's
  star and F841 type word through F832's seed/mass/stars and F830/F869's name/population; Titican (4.42 LY from
  Gateway) and its station Argent are reachable, while the `--start sol` sandbox keeps the 16-entry list. F909
  switches the world immediately, so the port's arrival is the same state without the drawn jump.
- Verification: `the_map_destination_survives_the_map_and_jumps` (destination survives the map, altitude/range/fuel
  gates, arrival locks/effects cleared), `f901_burns_the_hold_and_leaves_military_waste`, and
  `the_map_destination_and_jump_frames` (cached map frame, fuel line, jump and arrival frame/series under
  `FFE_SHOT_DIR`). `verify-hyperspace.py` still matches all 8,192 `--hyperspace` cases.

## System names (F830, cache v22)

`sector::system_name` gives a system code's name: a hand-made star's name from the string table (the 117 strings of
`DATA_006796`, imported in v22), the Gateway, or a generated pronounceable name built from the syllable table
(`DATA_006802`) with F861-style hashing of the code. `sector_label` is the "(x,y)" form. All 117 hand-made names and
8,000 generated codes match the executed original (`--system-names`, `verify-system-names.py`,
`names_map_reference.rs`: 8,292 cases).

## Verification

- `--sectors` in `headless.c`: F841 on the 49 hand-made sectors and 3,000 others, F853 on 4,000 sectors at seven detail
  levels, F856 on 3,000 vectors, F833 on 6,000 point pairs (including full 32-bit values). `verify-sectors.py` and
  `sector_reference.rs`: all 16,049 cases match (23,568 stars, 1,403 hashed counts, 2,655 far distances); trace SHA-256
  `d2e8f3261094289f2c4dd5c5b96f7ea28a443d43a9af102ca6cdb32947389109`.
- `--hyperspace`: F903 for non-player ships and the player (the fuel, waste, cargo-used and flag words) on 8,192 random
  ships, classes, distances, fuel amounts and modes; `verify-hyperspace.py` and `hyperspace_reference.rs`: all match; trace
  `e492b29f846e1c2aac4b88f7dc0b1ab89f9d777d6b73225c99e5fbd885b2c437`.
- `verify-assets.py`: the imported tables equal JJFFE's `DATA_006793..6795`, `DATA_006799..6811` and the first 240 bytes
  of `DATA_007187`, and the executable's own bytes. Synthetic tests: `crates/ffe-core/tests/galaxy_map.rs`.
