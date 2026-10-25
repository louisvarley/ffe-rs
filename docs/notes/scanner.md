# The scanner and the combat targeter

Ported 2026-10-25 from JJFFE 2.8a7 `ffe.asm` (the repo's gameplay reference) to
`crates/ffe-rs/src/scanner.rs`, `targeter.rs`, `screen.rs` and `main.rs`.
Evidence: `cargo test -p ffe-rs --lib scanner targeter`, the cache-backed
`tests/scanner_view.rs` frames, and a live `--capture-traffic` frame.

Where this note says JJFFE, the DOS release has not been executed. The routines
below carry no JJFFE `; was`/`; Big mod` markers in `ffe.asm`, so no
JJFFE-vs-DOS difference is identified; the DOS-only pixels/keys remain **not
established** rather than guessed. The one *asset* difference found is recorded
in §6.

## 1. Source map

| Piece | Routine/data | Reference |
|---|---|---|
| Scanner scan | `F274` | `ffe.asm:21239-21292` |
| Contact projection | `F275` | `ffe.asm:21296-21525` |
| Stalk draw | `F1569_DrawScannerStalk`, `F1570_DrawUpStalk`, `F1571_DrawDownStalk` | `ffe.asm:93327-93480` |
| Box fill | `F1627_DrawBoxToBuf`, `F1628_DrawBoxToBufMasked` | `ffe.asm:97425-97519` |
| Relative position | `F1678`, `F1677` | `ffe.asm:101683-101842` |
| Frame cut | `F1658_Vec64Truncate` | `ffe.asm:100208-100234` |
| Rotation | `F1670_VecMatTMul` | `ffe.asm:100801` |
| Length / scaling | `F1466`, `F1501`, `F1521` | `ffe.asm:86709`, `89627`, `90809` |
| Contact colours | `DATA_008683`, initialised by `F259` | `ffedat.asm:27855`, `ffe.asm:20181-20223` |
| Orientation overlay | secondary 254..261, `(DATA_008933 >> 9) & 7` | `ffe.asm:21023-21029`, `21114-21117` |
| Scanner gate | `player+0xca & 4` | `ffe.asm:21032-21035` |
| Targeter tunnels | `F1113`, `F1114` (called by `F1100`) | `ffe.asm:82024-82201`, `81185-81237` |
| Cursor box/circle | `F1111`, `F1612_DrawCircle` | `ffe.asm:81954-81991`, `96457` |
| Blink | `F1023`, `DATA_007252` | `ffe.asm:75589-75602` |
| Auto-targetter flags | `DATA_007246`/`247` (navigation/combat) | `ffedat.asm:18536-18539` |

## 2. The scanner (`F274`/`F275`)

`F272` calls `F274` when `player+0xca & 4` (the scanner is fitted). `F274` walks
the live object slots `0x72` down to 1, keeping each object whose presence byte
has bit 0x8 set and bit 0x20 clear, and calls `F275` with the player's parent
frame (`player+0x56`) and index (`player+0x86`).

`F275` keeps, in this order:

1. **Frame filter**: the object's `+0x56` equals the player's, or the object
   *is* the player's parent body (`object+0x86 == player+0x56`); the player's own
   object (`+0x86` equal) is skipped.
2. **Starports**: `object+0x14c & 0x20` (F880's type 0x23) are skipped.
3. **Relative position** `F1678` (same frame: `obj.pos - player.pos`; the
   player's parent: `-obj.pos`; otherwise the absolute positions' difference),
   then `F1658` shifts the three 64-bit coordinates right until the largest MSB
   is at bit 28, and each is divided by `0x131` (305).
4. **Rotation** by the player's orientation matrix transpose (`F1670`), i.e. the
   vector in the ship's right/up/forward frame.
5. **Length** `F1466`, then: longer than `0x50000` -> skip; longer than
   `0x10000` -> scale to `0x10000`; shorter than `0x3333` -> scale out to
   `0x3333`; otherwise leave it (F1501/F1521's Q16 scale).
6. **Screen mapping**: each scaled component times 45 and shifted right 16
   (truncated toward zero), then
   `x = 0x93 + dx`, `y = 0xbb - dz*10/45`.
7. **Stalk**: `up/2` (arithmetic shift, so it floors). `F1569` draws two
   2-pixel-wide bars `x-1..x` and `x+1..x+2`: above the contact when the stalk
   is positive (`F1570`, the bottom clamped to row 0xc6), below it when negative
   (`F1571`, `F1628` skips the scanner grid's colours 0x28..0x2b). With the ship
   module active (`DATA_008630` negative) the clip flag is 0, so an up stalk may
   cross y 158 into the view; otherwise it is clipped to y 0x9e.
8. **Colour**: `DATA_008683[(stats[+0x6] >> 4)]` while the mass word is below
   0x12c, else 0x0f (`F275` reads the statistics block through
   `F1538()->[+0x38]`). `DATA_008683` is 19 dwords, initialised by `F259`:
   `43, 11, 73, 73, 10, 10, 5f, 5f, 5f, 12, 12, 12, 08, 09, 0a, 0b, 0c, 0d, 0e`.
   Objects without a statistics block (stars, planets, stations) are 0x0f.

The scanner is a top-down plan view: x is lateral, y is forward/back, and the
stalk is elevation. The centre (147,187) is the player; the panel art's rings
are the range marks and are **not** computed. The 92x30 globe is the eight
pre-rendered orientation overlays (secondary 254..261), selected by
`(DATA_008933 >> 9) & 7`; `DATA_008933` is the accumulated yaw word, decreased
when the ship yaws to its right (F611's 0x70 branch), so it is minus the port's
yaw.

## 3. The targeter

### 3.1 Tunnels (`F1100`/`F1113`/`F1114`)

`F1099` calls `F1100` twice: the combat target `player+0xed` (colour 0x10, or
0x5f in the missile view) and the navigation target `player+0xec` (colour 0x11).
When `DATA_009024` ("Show targeting tunnels") is set, the view is not 4
(external) or 6 (combat computer), the target's screen words are inside
`-1000..1000`, and — when `DATA_009025` is clear — the target has no statistics
block (so is not a ship), `F1100` calls `F1113` four times with shift 0..3.
The combat target's tunnels are additionally gated on the missile view (5), so
in normal views only the navigation target draws the four boxes
(`ffe.asm:81185-81250`):

```
w = 0x80 - shift*0x20
x' = (shift*0x20 * x + w*0xa0) >> 7
y' = (shift*0x20 * y + w*0x4f) >> 7
half = (0x40 + ((-range_mantissa & 0x7ff) >> 5)) >> shift
```

The squares therefore run from a large box at the view centre (shift 0) to a
small one three quarters of the way to the target (shift 3); `F1100` passes the
target screen x minus one. `F1114` draws the four edges, clipped to x
0..=0x13f and y 0..=0x9d, each edge only when its own coordinate is inside.

### 3.2 The blinking cursor (`F1111`/`F1612`)

`F1111` draws the fixed gunsight and then, while `DATA_007252` (the blink) is
set:

- `DATA_007246` (navigation auto-targetter): `F1113(x=0xa0, y=0x4f,
  size=4, colour=0x11, shift=0)` — a 9x9 box at (160,79);
- `DATA_007247` (combat auto-targetter): `F1612((0x9f,0x4f), 0xc, 0x10)` — an
  outline circle of radius 6 at (159,79), colour 0x10.

`F1023` sets `DATA_007252` from `DATA_008799 / 0xc22e & 1`: 0xc22e is 49,710,
the tics per second, so the cursor blinks once per game second. Both marks are
drawn only in a view where F1111 draws the gunsight (a laser in that mount, not
external/combat).

The flags are *armed targetter modes*, not locks: the original's `t`/target
button toggles them only with the target computer (`ship+0xc8 & 0x80`), and the
next object under the crosshair clears the armed flag (F1010/F1060/F1062). They
do not draw the tunnel.

## 4. What the port implements

- `scanner.rs`: F1678/F1658/F1670/F1466/F1501's projection in `f64` (the
  305 divisor, 28-bit cut and integer truncations kept), the frame/starport
  filters, the colour table, the orientation word, and the unit tests.
- `screen.rs`: the globe overlay in F272's order (panel, globe, buttons,
  gauges), the stalk fills with the downward grid mask, and the stalk band.
  The up-stalk may cross into the view, as in flight.
- `main.rs`: scanner contacts from `Navigator::bodies` (position, parent frame,
  model/mass) plus live NPC ships (the player's frame), gated by
  `ShipSystems::ca & 4` (the new-game default now sets 0x04); the orientation
  from the ship's yaw. A body lock is the navigation target (0x11) and an NPC
  ship lock the combat target (0x10); the navigation target's four tunnels are
  gated by `prefs.others[6]` (show), `[7]` (ships) and not the external view.
  F1100's off-view fixed spot is (50,50) for the navigation target and (50,60)
  for the combat target. The unclipped projection (`Screen::body_projection`)
  feeds the off-view arrow choice.
- `targeter.rs`: F1113's four boxes (size from the range mantissa), F1114's
  clipped edges, F1612's circle, and the box/circle cursor on `blink`.
- Removed the port's own corner brackets around the locked target; the original
  lock display is the tunnel and the range text, not a bracket.

## 5. Differences and owner decisions

- **Auto-targetter arming is wired (2026-10-25).** The Auto targetter is
  general bit 7 (`ship+0xc8 & 0x80`, record 0x17) and can be fitted at a
  starport. With it, `T`/`Shift+T` arm the navigation/combat cursor,
  `targeter::acquire` implements F1061/F1063's six-pixel crosshair rule and
  F1010's per-frame acquisition, and F1111 draws the box/circle. Without it,
  the port's `T` still cycles locks (reported deviation). See
  `docs/notes/combat.md` §8.2.
- **Range text.** Ported as F1100: the text colour is the target's COLPAL
  (0x11 nav green / 0x10 combat red, 0x5f in missile view). The number is in
  hundredths with two decimals and no space (`5.40AU`, `1.00km`), drawn one
  10-row line below the target's `-5` origin. The off-view Behind, Out of
  Range and four arrow forms (0x7d/0x7e left/right, 0x7f/0x80 below) are
  ported. Two simultaneous targets (0xec and 0xed) are still collapsed to one
  display. The missile view now exists and uses 0x5f (`docs/notes/views.md`).
- **Scanner same-frame source.** The original's `player+0x56` is the physics
  frame body; the port uses `Navigator::frame()`, chosen by
  `Navigator::follow` from the station/planet proximity. Traffic is marked as
  the player's frame because the port's ships have an anchor position rather
  than a parent index.
- **Starports.** `F275` skips `+0x14c & 0x20`; the port's `Body::kind ==
  Starport` is the same F880 type 0x23. The port does not model a separate
  class flag.
- **Scanner art vs palette.** The globe/rings are the cached DOS art; the
  contact colours and the art are low-palette indices (all below 128), so the
  runtime upper palette does not affect the scanner itself. It does affect the
  view's models/FX: the port builds the same upper palette as the original
  (`docs/notes/palette.md`) and resolves their 12-bit colours through it. The
  earlier note that the port's scanner/HUD was brighter than JJFFE described
  the pre-palette model approximation, not the scanner contacts.
- **Reference bitmap difference (JJFFE vs DOS).** A `jjffe-smoke --game-frame`
  capture of the original's console differs from the cache's primary 43 by
  small index shifts (e.g. 9 vs 8, 15 vs 9) across the panel, so a pixel-exact
  console-oracle comparison cannot use JJFFE's embedded `ffebmp.asm`; the
  scanner globe/ring placement and the contact position still agree.
- **F1100 target-visibility test.** JJFFE has the original
  `test byte [ebx+0x14c],0x10 / jz` commented out at `ffe.asm:81093`, so its
  range/tunnel display does not require the 0x10 flag (F880's stations). The
  port follows JJFFE and shows the range/tunnels for any locked object; if the
  DOS executable ran the test, only station-flagged objects would show them.
  This is the one routine-level JJFFE-vs-DOS candidate found here; the DOS
  binary was not executed.
- **Unarmed equipment.** `+0xca & 4` is defaulted on for every ship because the
  port has no equipment model; the original's new-game ship carries the scanner
  in its equipment block.

## 6. Evidence

- `cargo test -p ffe-rs --lib`: 8 `scanner`/`targeter` unit tests (colour
  table, 28-bit cut, centre/rim/near clamps, rotation, frame/starport filters,
  orientation frames, tunnel geometry, clipped edges, cursor circle).
- `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test scanner_view --
  --ignored` (set `FFE_SHOT_DIR` to keep the frames): `scanner-system.ppm`,
  `scanner-system-empty.ppm`, `scanner-tunnels.ppm`, `scanner-blink-on.ppm`,
  `scanner-blink-off.ppm`, `scanner-nav-cursor.ppm`, `scanner-clipped.ppm`.
  They assert the traffic contact's ring position/colour, that contacts change
  >20 scanner pixels, that the orientation overlay changes >50, that the
  tunnels draw >20 view pixels, that the combat circle and navigation box both
  change in the cursor area, and that every synthetic contact pixel lies in
  x 101..194 and y 164..198.
- `ffe-rs.exe --root <repo> --skip-intro --capture-traffic <dir>` (live game):
  `traffic-00.ppm` shows the scanner contact at (147,186) for the locked pirate
  200 m ahead (the near ring compresses a close contact toward the centre) and
  the range text.
- `jjffe-smoke.exe --game-frame 400 0xff` (the executed original at Ross 154's
  starport) has the same globe at (101,175) and a contact in the scanner area;
  the globe placement is the reference confirmation for the overlay draw.
