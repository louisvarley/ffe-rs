# Front end and a new game

Record for the intro screens and the original's new-game state. The DOS FFE
1.02 release is the source of truth; the executed JJFFE reference is used only
where its data and behaviour agree with that release, and every JJFFE-only
piece is called out below.

## New game (F829/F998/F994)

- **Start-system table** `DATA_006820` (`ffedat.asm:14448`): five 20-byte
  records of galaxy x, y, z (16.16), the star's number in its sector and one
  dword beside them. `ffe_rs::navigation::START_SYSTEMS` keeps them:
  records 0 and 4 are the same system (`0x17170200`, `0x15287600`, -222,
  star 6); **record 1 is Ross 154** (`0x17176a00`, `0x15243200`, `-0x3400`,
  star 1); records 2 (`0x17188000`, `0x1524a000`) and 3 (`0x1715e000`,
  `0x151ee000`) are the other two.
- **Clock**: the game's day counter is absolute, `0x121cf7.5` days =
  `102,562,286,400` seconds (`0x121cf7` whole days and the half-day fraction
  `0x80000000`). `Navigator::new_game` starts there; the calendar subtracts
  that start, so the console shows 1 Jan 3250, noon. The development sandbox
  keeps a 0-based clock (`start_clock`), so the date stays right in both.
- **F998's state for start type 1**: 1000 CR (`DATA_008888`), ship 23 (the
  Eagle long range fighter, `0x17`), the hold is the ship's 20 t less `0xc`
  (8 t), and time acceleration 994 (`DATA_009012`). F994 sets the player's state
  byte to `0x30` (landed at an open-air starport's pad) and `DATA_008857` (the
  player's object index) is 107.
- **The pad**: F994 calls F650 with the player's ship and object `0x6e`; that
  object is the system's starport. In this checkout Ross 154's starport is
  `slot 0x6e`, model `0x55` (an open-air model: class `+0x14c` bit `0x40`),
  bay 1. F650 turns the bay's pad `(-250, 0, 250)` by the station's F1674
  rows and shifts each component left ten. With the generated rows
  `[-906354405, 1945245414, 76650739, -1938310550, -909597149, 163923668,
  180966058, 0, 2139827798]` the offset is `(126, -227, 240) << 10` =
  `[129024, -232448, 245760]` mm in the station's frame. The local test
  (`new_game_starts_at_ross_154`) checks this and the date, hold and cash.
- **Boot**: the default boot is this new game. `--start sol` keeps the old
  Sol/Mars High development sandbox (and `Navigator::new`, which the existing
  tests use). `--skip-intro` is accepted for the eventual front-end wiring; the
  logo drawing is not wired yet.

## The intro screens (F961/F962)

- **F961** (`ffe.asm:71649`): while `GameTics < 0x8276e` it draws model 177;
  from `0x8276e` it draws model 176 and sets global 0 to
  `(tics - 0x8276e) >> 10` (before the switch it is `tics >> 10`). At
  `GameTics >= 0x7e7b76` it clears the buffer, flips, clears again and sets the
  clock back to 0: the intro restarts.
- **F962** (`ffe.asm:71724`):
  - From `GameTics > 0x795cc` (10.0 s) it blits primary bitmap `0x42` at
    `y = 0x9e` (158), x = `0xa0 - width/2`.
  - It returns before any credit while `GameTics < 0x8276e`; **all credits
    start at the model switch**, not at 0. The first credit's window is
    therefore `0x8276e..0xfbd3a`, not `0..0xfbd3a`.
  - The eighteen credit calls (`0x941b..0x942c`) with their exact thresholds,
    x, y and wrap width are in `ffe_rs::front_end::INTRO_LINES`: for example
    `0x941b` at x 90, y 15, wrap 0 for `0x8276e..0xfbd3a`, and the two last
    lines at x 5, y 10 and x 5, y 68, wrap 310, from `0x77a7d8` with no upper
    bound. Journal Entries (`0x9428`) uses the original's `jna`, so it starts at
    `0x6d0955`, one tic after `0x6d0954`. The overlapping entries (Journal
    Entries/Representation, Video Actors/Michel Moalem...) draw together.
  - The strings are the DOS table at `ffedat.asm:4272-4332`, flattened to
    plain text: `\r` becomes a newline and the in-line font codes
    (`0xfd`/`0xfe`, which carry a glyph and its operand) become a single
    space. `0x942d` ("English Release 1.1 (Remastered Version)") is JJFFE's,
    see below.
- **Input**: F962 reads the GUI action only from `0x8276e`; `'V'` draws
  `0x942d` at x 10, y 60, wrap 310, colour 16. The start choice follows the
  intro. Keys 1-5 are the JJFFE mapping: only start type 1 is verified, and
  2-5 map to 1 with a notice (no system or ship is invented for them).

## JJFFE vs the DOS release

| Piece | Where it is | Decision |
|---|---|---|
| The nine-line legal splash ("This is a replacement executable...") | `ffe.asm:68301-68348` | **JJFFE's, removed.** The front end has no splash; the default boot never shows it. |
| "English Release 1.1 (Remastered Version)" | `DATA_005008`, F962 `0x942d` | **JJFFE's.** Kept only for the intro oracle; not part of the DOS boot. |
| Start types 2-5 | F998's other branches | Unverified against DOS; mapped to 1 with a note. |
| The 18 credit strings and their windows/positions | `ffedat.asm:4272-4332`, F962 | Agree with the DOS string table; verified by `--intro`. |
| Model 177 then 176, the primary bitmap, the animation | F961/F962 | Data shared by the release; same oracle. |
| New-game state (Ross 154, cash, ship, hold, clock, state 0x30) | F829/F998/F994 | Same as DOS; `--state` reports state 48, index 107, cash 1000, start type 1 and time 994. |

## Takeoff from the starport (blocked)

The new game is landed on an open-air starport pad. The original's takeoff is
player state `0x24` (F1013: F1037, then switch to `0x26` when the altitude
word `DATA_008867` passes `0x30d40` or `DATA_008918` is set) followed by normal
flight; the earlier port's launch instead teleported the ship to the station's
`standoff` (20 km) with the lift geometry. That teleport is invented and has
been **removed**: `Navigator::begin_launch` now refuses an open-air starport
with a clear message, and `docs/DEVIATIONS.md` EXP-010 records the gap. Indoor
stations keep the verified lift launch. Porting F1013's altitude/timer hand-off
is the follow-up.

## Oracle

`tools/reference/headless.c --intro` calls the real F961/F962 at 76 cases (0,
10.0 s, the bitmap boundary, the model switch, 0x7e7b75, and each credit's
last/first/end tics), capturing the text draw (`DATA_007644`) and the model
draw (`DATA_007807`). `verify-intro.py` runs it twice, drives
`crates/ffe-rs/tests/intro_reference.rs` (an `#[ignore]`d local test) and
rejects altered tics, model, animation, version flag and each text field, and
an empty trace. Trace SHA-256: `a9489bbe5d0a2765938c1d14f8ba70505c0726bcd110d492eb01959616eafc3d`.
