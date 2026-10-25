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
  `[129024, -232448, 245760]` mm in the station's frame. F994 then adds the
  ship's collision radius (`+0x140`, 5,200 mm for the Eagle) along its second
  row (the up row, copied from the station): `(-4694, -2203, 396)`, so the seat
  is the station position plus `[124330, -234651, 246156]`. The local test
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

## Takeoff from the starport (F1013/F1031/F1032, 2026-10-25)

The new game is landed on an open-air starport pad in state `0x30`. The launch
action (F381's F654) starts F1013's climb (`0x24`): F1037's input, then the
hand-off to `0x26` once `DATA_008867` passes `0x30d40` or `DATA_008918` is set,
F1041's fixed up-thrust, F602/F1512 and F1033's scan. F1031 then flies the
state `0x26` ship normally and watches the same scan: while F1033's `0x20`
("within the starport's effect region", the starport collision stream's node
flags) is set it stays; when it clears, the mode becomes `0x00` and F1032 calls
F651 to free the bay. A landing (`0x40`) on the open-air pad during `0x26`
returns to `0x24`. `ffe_core::traffic::Takeoff` holds the mode word and its
transitions; `Navigator::{begin_launch, takeoff_step}` drive it and release the
bay. Indoor stations keep the verified lift launch.

The pad contact is the ported F575/F576 complex-shape scan, kept in a persistent
two-body world (`OriginalFlight::{begin_pad_takeoff, step_pad, end_pad_takeoff}`)
so F570's frame switch survives across frames. F994's placement is F650's offset
plus the ship's `+0x140` collision radius along its up row (`Pads::seat`, used by
`Navigator::new_game` and `takeoff_seat`). One difference: a starport's complex
shape leaves F1033's altitude at `i32::MAX`, so F1013's altitude test uses the
height climbed from the pad seat along its up row as the fallback; the original
would otherwise hand over on the first frame. `tests/navigation.rs` runs the
landed -> `0x24` -> `0x26` -> `0x00` sequence with the new game's cache
(`FFE_SHOT_DIR` also saves pad, climb and clear frames), and
`crates/ffe-core/tests/takeoff.rs` pins the mode transitions. The playable's
`--capture DIR` script flies the same takeoff off the default new game's pad
and saves every tenth frame. The old invented 20 km teleport stays removed;
`docs/DEVIATIONS.md` EXP-010 records the port's fallback.

## Oracle

`tools/reference/headless.c --intro` calls the real F961/F962 at 76 cases (0,
10.0 s, the bitmap boundary, the model switch, 0x7e7b75, and each credit's
last/first/end tics), capturing the text draw (`DATA_007644`) and the model
draw (`DATA_007807`). `verify-intro.py` runs it twice, drives
`crates/ffe-rs/tests/intro_reference.rs` (an `#[ignore]`d local test) and
rejects altered tics, model, animation, version flag and each text field, and
an empty trace. Trace SHA-256: `a9489bbe5d0a2765938c1d14f8ba70505c0726bcd110d492eb01959616eafc3d`.

## Boot drawing and timing (2026-10-25)

The boot now plays the intro instead of starting the game directly. The DOS
`FIRSTENC.EXE` addresses below are file offsets (the executable is a PE image;
its code disassembles with the same instructions as JJFFE's `F961`/`F962`).

**Source map.**

| Piece | DOS | JJFFE | Port |
|---|---|---|---|
| Boot loop: `GameTics=0x400`, timer add, key table, `F147` call | file `0x2c499`..`0x2c589` (`0x43c499`) | `ffe.asm:68405-68495` | `main.rs::run_intro` |
| F961 model/bitmap/animation/loop | file `0x2f298` (`0x43e498`) | `ffe.asm:71649` | `front_end::IntroTimeline`, `screen::draw_intro` |
| F962 credits and V | file `0x2f380` (`0x43e580`) | `ffe.asm:71724` | `front_end::INTRO_LINES`, `intro_text` |
| F959 object: identity 0x7f000000 rows, origin `(0,0,0x14820)`, model 0xb0 | file `0x2e9d8` (`0x43e444`) | `ffe.asm:71516` | `screen::draw_intro` |
| F960 default light/shades + `F1682` | file `0x2e970` | `ffe.asm:71624` | `ModelViewer::render_scene_with` |
| F1682/F1683 display instance | file `0x5b678`/`0x5b69c` | `ffe.asm:102201/102221` | `model_viewer` |
| F1399/F1398/F1587/F1589 text | file `0x4f7b7`/`0x4f6ac`/`0x555e8`/`0x557c8` | `ffe.asm:85259/85127/93822/94028` | `intro_text` |
| Primary bitmap 0x42 | file `0x2f3a0` | `ffe.asm:71738` | `screen::intro_bitmap` |
| V gate `0x719668` | file `0x2f6a1` | `ffe.asm:71765` | `front_end::INTRO_INPUT_FROM` |
| New game `F147`/`F998` | file `0xa1ac` | `ffe.asm:14912` | `navigation::Navigator::new_game` |

**Timeline.** `GameTics` starts at `0x400`; each frame adds
`elapsed_ms * 0x3e2 / 20` (994 per 20 ms = 49,700/s). F961 draws model 177
with global 0 = `tics >> 10` until `0x8276d`, then 176 with
`(tics - 0x8276e) >> 10`. F962 blits primary bitmap `0x42` from
`tics > 0x795cc` at `x = 160 - width/2` (5 for the 311-pixel strip), `y = 0x9e`.
The 18 credits start at `0x8276e`, not at 0. At `tics >= 0x7e7b76` F961 clears,
flips, clears and sets `GameTics=0`; the port models the reset in
`IntroTimeline::advance`. `--skip-intro` bypasses the boot screens; scripted
`--smoke-frames`/capture modes do the same. A normal run shows the intro.

**Input.** The DOS loop reads the action every frame from `0x400`: keys 1-5
start (table at 0x4ee8a0: 1 -> 1, 2/3 -> 0, 4 -> 6, 5 -> 5), V is a no-op, and
any other key exits with `F147(0)`. F962 latches the DOS version line on V only
from `0x719668`, at (10,60), wrap 310, colour 16, string
`English Release 1.02 - 21/04/95`. The port keeps the verified type 1 for keys
1-5 (2-5 with the existing notice) and maps the "any other key"/type 0 arm to
type 1 with a notice; an owner decision is still needed for types 2-5.

**Assets.** Cache v27 adds `base:text/intro-credits` (19 raw NUL-terminated
strings from `0xbd361`: 18 credits + the DOS version, layout codes preserved)
and `base:text/char-widths` (`DATA_007648`, 256 signed bytes from `0xe9998`).
Cache v28 adds `base:music/qqfront`, the player's `MUSIC/QQFRONT.HMP`
byte-for-byte, so the F919 theme is available at boot.
Model 176/177, the primary bitmap 0x42, the main font and the low palette were
already in the cache. The credits are drawn from the cache, not from Rust
strings; the old plain-text transcription and its `.text` field are gone.


**Model 176's scene.** F961 draws model 176, whose command stream selects one
of sixteen scene children (348/349 at the sampled tics). Those children carry
the ship (440), asteroids and the big planet, which is a *curved surface*
(model 449) inside the child tree. Child curved surfaces are collected from the
child executions (`all_surfaces`) and drawn. One late descendant (model 456 ->
321) makes a frame-mode-6 call whose parent basis the original copies from
stale stack; the port reports that single child and keeps its siblings, instead
of discarding the whole scene. `--capture-intro` therefore shows the planet,
ship, asteroids and galaxy at the same tics as `jjffe-smoke --intro-live`.

**GAME!TEK.** Model 177 -> child 361 is the logo: one slab face and text
`0x408a` (the DOS cache's "GAME!TEK", glyph set 0, size 15); 362/363 are
PRESENTS/DAVID BRABEN'S. The text record's material `0x0888` is resolved
through F1692/F1693 and passed to the glyph instance (F1747); without it the
glyph faces were black holes in the slab. F1747 also negates the text frame's
first row when F1740 leaves a left-handed frame (`ffe.asm:5f64b-5f658`): without
that every glyph was mirrored (`GAME` read `ƎMAפ`), while the slab stayed
symmetric. The letters are the original's fixed grey, matching the executed
frame at animation 63 (`(192,192,192)` on `(64,64,64)`). The executed JJFFE
reference draws its own `FRONTIER` string for this model (`intro-todo.md` D5);
the port keeps the DOS cache's GAME!TEK. See `docs/evidence/visuals/`.

Song 0 `QQFRONT` now plays during the intro:
`run_intro` calls F919's `front_end_music()`, ticks F1904, and stops the song on
the start key. Cache v28 stores it as `base:music/qqfront`.

**Runtime palette (2026-10-25).** `F1575`/`F1326`/`F1327`/`F1330`/`F1331` are
ported in `ffe_rs::palette`. The intro uses its own `F1575` colours: `(0,0,0)`
for model 177 (fixed low entry 0, black) and `(0,0,4)` for model 176. The
executed original at the switch frame has exactly one upper entry, six-bit
`(1,1,17)` (expanded `#040444`), and the same entry is the clear colour; the
port now produces that entry and background. `F1786` clears the request cube
once per frame before F961, so the intro's later frames (planet, galaxy) are
cut around that frame's colours. The median-cut heap uses F1329's longest
extent and the original's sink-root/insert-right movement; the old
count-keyed, root-repeated split filled most boxes with the empty `31,31,31`
representative and speckled the band texture. The planet is generated at
`DATA_008812 = 1` (the boot's Shape Detail; the old hard-coded 2 over-subdivided
it). See `docs/notes/palette.md` for the source map, the colour-word format and
the frame evidence.


**Tests and frames.** `crates/ffe-rs/tests/front_end.rs` covers the thresholds,
the `0x400` clock, the `0x7e7b76` reset, the DOS V gate, keys at any tics and
the skip arm; `intro_text.rs` unit tests cover wrap, `0xfd`/`0xfe` positioning,
the shadow and colour. `tests/intro_view.rs` (ignored; set
`FFE_INTRO_CACHE=<v27 data.bin>`) renders the assembled screen at `0x795cc`,
`0x8276e`, `0x181534`, `0x719668` and `0x77a7d8`. `--capture-intro DIR` writes
that sample sequence plus the version frame without opening a window;
`--skip-intro --smoke-frames N` presents the in-game frames.
