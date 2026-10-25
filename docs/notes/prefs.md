# Preference pages (Escape)

Ported from `jjffe-master/ffedat.asm` and `ffe.asm` (JJFFE 2.8a7). The DOS 1.02
release's `FIRSTENC.EXE` carries the same strings, in the same order, in its text
block (`Preference pages available:`, `Graphic Detail`, ... `Quit to DOS`); JJFFE
reuses the DOS layout and the same icon bitmaps, so the layout below is also the
DOS release's.

Rust: `ffe_rs::prefs` (state, page table, hot areas, keys) and
`Screen::draw_prefs` (the 320x158 page over the console). Frames:
`ffe-rs --capture-prefs DIR` writes every page and the two space colours.

## The page list

`DATA_004894` (ffedat.asm 3767-3813) is the SysMenu string table. Its first five
entries are the pages, in this order, shown in the main menu's left column:

| Page | Menu label | String label | String data |
|---|---|---|---|
| 0 | (the menu itself) | `Preference pages available:` | `DATA_004963` (4174) |
| 1 | Graphic Detail | `Level of Shape Detail:` / `Space Colour:` | `DATA_004964` (4181) |
| 2 | Sound Effects | `Sound Effects:` / `Choose Music From:` | `DATA_004965` (4190) |
| 3 | Others | `Other Options:` | `DATA_004966` (4210) |
| 4 | Maps | `Map Options:` | `DATA_004967` (4218) |
| 5 | Port Options (port-only) | - | none |
| 6 | Display Options (port-only) | - | none |

The main menu (`DATA_004963`), exactly as the string draws it:

```
 Commander Jameson
Preference pages available:
 Graphic Detail   @Load
 Sound Effects    @Save
 Others           @Delete
 Maps             @Return to Game
                 (blank row)
 Quit to DOS      @New Game
```

(`0xfd '@label'` moves to the right column; the leading spaces indent the left
column. The commander name is the original default `Jameson`, `DATA_004916`.)

## Layout

`FUNC_000225_SysMenuDisplayNonFile` (ffe.asm 18376) clears the upper 320x158 view
to `DATA_009222` (the palette index nearest colour 3,3,3 - index 3, 12,12,12),
writes the page string with `FUNC_000179` (F1399 shadowed text; the captured
frame renders headings red, rows white, the commander line green and
Quit/New Game red), then blits the on/off icon `0x2c`/`0x2d` at each hot area's
top-left. Only the 8x8 icon square is a hot area.

`0xfe <x> <y>` in the strings sets the text pen; x is twice the byte (0x10 ->
32 px, 0x18 -> 48 px, 0x40 -> 128 px). The resulting coordinates:

- Commander line at (32, 8) green; `Preference pages available:` at (32, 18) red.
- Left column: icon at x 32, label at x 48, rows y = 28, 38, 48, 58, 68, 78.
- Right column: icon at x 112, label at x 128, rows y = 28, 38, 48, 58, 78.
- Page 1: headings at (32, 14) and (32, 70); rows at y = 24..54 (shape detail)
  and 80..120 (space colour); `Main Menu` at (48, 136).
- Page 2: heading at (32, 18); rows y = 28..118; `Main Menu` at (48, 138);
  `Choose Music From:` at (60, 0); the 16 songs at x 160, y = 0, 9, ... 135.
- Page 3: heading at (32, 18); rows y = 28..118; `Main Menu` at (48, 136).
- Page 4: heading at (32, 18); rows y = 28..68; `Main Menu` at (48, 136).
- Page 5 (port-only `Port Options`): heading (32, 18); `Controls:` (32, 28);
  `Modern`/`Classic` rows at x 32, y 38/48; control help at (48, 62..92);
  `Display Options` row at (32, 104); `Main Menu` at (48, 136).
- Page 6 (port-only `Display Options`): heading (32, 18); `Window scale:` (32, 26)
  and 1x..6x rows at x 32, y 36..86; `Fit:` (176, 26) and two rows at y 36/46;
  `Filter:` (176, 60) and two rows at y 70/80; `Pixels:` (176, 94) and two rows
  at y 104/114; `Fullscreen` at (32, 104); help at (32, 122); `Main Menu` at
  (48, 136). The original main menu and pages 0-4 are unchanged.

The text is white (15), headings and the two leaving actions red (16) and the
commander line green (17); `0x2c` is a red square with a dark red body, `0x2d`
its grey twin (primary bitmap table ids 44 and 45, both imported in the cache as
`base:bitmap/primary/044`/`045`).

## Hot areas and actions

`DATA_004907` (ffedat.asm 4040) points at one list per page; each entry is
`(left, top, right, bottom, action)` with the icon rectangle. `DATA_004896`
(3822) is the per-page offset of the displayed state bytes inside the player
struct at `+0x3f4c`, and `DATA_004897` is the current page.

Main menu (`DATA_004898`, includes the commander-name area 72,7-150,16):

| Hot area | Action | Handler |
|---|---|---|
| 72,7-150,16 | 0x8c commander name | `FUNC_000213` |
| 32,28-40,36 | 0x82 Graphic Detail | page 1 (`JUMP_001400`) |
| 32,38-40,46 | 0x83 Sound Effects | page 2 (`JUMP_001401`) |
| 32,48-40,56 | 0x84 Others | page 3 (`JUMP_001402`) |
| 32,58-40,66 | 0x85 Maps | page 4 (`JUMP_001403`) |
| 32,78-40,86 | 0x86 Quit to DOS | `SysExit` (17992) |
| 112,28-120,36 | 0x88 Load | load dialog |
| 112,38-120,46 | 0x87 Save | save dialog |
| 112,48-120,56 | 0x89 Delete | delete dialog |
| 112,58-120,66 | 0x8a Return to Game | `FUNC_000224` (1) |
| 112,78-120,86 | 0x8b New Game | `FUNC_000230` |

Page 1 (`DATA_004903`): 0x81..0x84 shape detail (`0x3f4c` radio, copied to
`0x3f7e` for display), 0x85..0x89 space colour (`0x3f82` radio, selected index
at `0x3f88`), 0x9e Main Menu. `JUMP_001357`/`JUMP_SpaceColor` (ffe.asm
17631-17658).

Page 2 (`DATA_004902`): 0x81..0x8a the ten sound toggles (`0x3f5c..0x3f65`:
`DATA_009028`..`DATA_009037`), 0x8b..0x9b the 16 songs (index - 0x8b), 0x9e
Main Menu. `JUMP_001383` (17817-17924).

Page 3 (`DATA_004904`): 0x85..0x8e the ten toggles (`0x3f51..0x3f5a`:
`DATA_009018`..`DATA_009027`), 0x9e Main Menu. `JUMP_001361` (17676-17815).
The order is exactly `Space dust, clouds etc`, `Background stars`, `Lock
external view to ship`, `Reverse left-right controls`, `Reverse up-down
controls`, `Elite control method in space`, `Show targeting tunnels`, `Allow
targeting tunnels for ships`, `Use filename extensions`, `Video`.

Page 4 (`DATA_004905`): 0x81..0x85 the five map toggles (`0x3f78..0x3f7c`:
`DATA_009039`..`DATA_009043`; they also change the global bits `DATA_008949` and
`DATA_008982`), 0x9e Main Menu. `JUMP_001374` (17760).

## Setting application

`FUNC_000228_SysMenuUpdateGfxDetail` (18499) maps the shape-detail radio to
`player+0x10/14/18`: Low `(-1, -2, 0x4100)`, Medium `(0, 0, 0x4030)`, High and
Very High `(1, 1, 0)` (identical in JJFFE 2.8a7). The port stores the radio but
always draws full detail, so the page marks the rows grey and says so.

The space colour is the 3-byte `DATA_008815..817` (declared at ffedat.asm
28120; the inventory calls it the background colour): `(0, 0, index)` with
four-bit components. (`DATA_008653` at 27795, "3-byte used for background
setting by SysMenu", is the *menu* background F229 sets to 3,3,3, not the space
colour.)
`FUNC_001575` copies it, `FUNC_001578`/`FUNC_001579` pick the nearest palette
entry for the clear colour, `FUNC_001618_ClearUpperBuf` clears the flight view
with it, and the surface renderer mixes the same three bytes into the sky. The
default is index 4, `Light Blue`, from `FUNC_000219` (18164-18168).

The original quantises the request to the nearest entry in its runtime 256-colour
palette; the captured reference frame's default is palette entry 182, six-bit
(3,3,19) = `#0c0c4c`. The port has no runtime upper palette and cannot repeat the
lookup, so it draws that captured entry for the default and derives the other
four settings from it: each six-bit component scaled by index/4, rounded to the
nearest six-bit value, then expanded with `v * 255 / 63`
(`ffe_rs::prefs::space_colour_rgb`). The five values are therefore `Black
#000000`, `Very Dark Blue #040414`, `Dark Blue #080828`, `Medium Blue #080838`,
`Light Blue #0c0c4c`; the ramp is a documented interpolation (`docs/DEVIATIONS.md`
EXP-013), and Black keeps the stars drawn.

## Defaults (`DATA_004895`, FUNC_000219)

`0x32`-byte flags copy, page offsets from `DATA_004896`: shape detail **High**,
space colour **Light Blue**, sound all **on**, others `[on, on, on, off, off,
off, on, off, on, on]`, maps all **on**. `FUNC_000219` also sets
`0x3f86 = 0xff` and `0x3f88 = 4` directly.

## Escape flow

`DATA_007688` maps scancode 1 (Escape) to action 7; `DATA_004893` has 7 at
entry 10, so `FUNC_000150` (15071) sends event 1 to module 10 (SysMenu).
`FUNC_000224` (18314) first stops time if it is running (18549 `FUNC_000220`),
and on the next Escape signals the flight module to return. `FUNC_000229`
switches the video to the menu and clears the view to 3,3,3. The port opens the
pages on the first Escape and closes them on the next, as requested; it does not
reproduce the intermediate pause step (`docs/DEVIATIONS.md`, EXP-012).

## In the port

- Pages 0-4 use the original labels, order, coordinates, colours, defaults and
  hot areas; the on/off icons are the original primary bitmaps.
- Applied: space colour, background stars, lock external view to ship, reverse
  left-right/up-down, Modern vs Classic controls (port page 5). Everything else
  keeps the original label, default and icon but is greyed and inert, with a
  page footnote.
- Port additions (grey/yellow, documented): page 5 `Port Options` in the empty
  row at y 68, with `Modern`/`Classic` and the `Display Options` entry; page 6
  `Display Options` with window scale 1x-6x, pixel-perfect/fill-window fit,
  nearest/linear filter, 4:3/square pixels and a full-screen toggle (see
  `docs/notes/display.md`); the label `Quit` instead of `Quit to DOS`; keyboard
  navigation (arrow keys, Enter, Backspace) and a `>` marker.
- Persisted: `saves/controls.cfg` (Modern/Classic) and `saves/display.cfg`
  (scale, fit, filter, pixels). Full-screen and the other page values are
  session-only.
- Not ported: commander rename (`0x8c`), Load/Save/Delete/New Game dialogs, the
  music list selection, shape-detail application, targeting tunnels, filename
  extensions, video, map toggles, and the original's runtime upper-palette lookup
  itself (its captured default and a documented ramp stand in, EXP-013).
- The strings and hot-area tables are **not** in the cache: the port hard-codes
  them from the assembly (source offsets above). Importing `DATA_004894` and the
  text block would make them cache assets; the icon bitmaps already are.
