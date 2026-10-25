# The console's right-hand buttons: places list and text labels

Ported 2026-10-24 from JJFFE 2.8a7 `ffe.asm`/`ffedat.asm`. The same data is in
the DOS release: `game/FIRSTENC.EXE` (floppy 1.02) holds the hot-area table at
file offset **0xea68c** and the three button records at **0xbe912** (hyperspace),
**0xbe950** (labels) and **0xbe98e** (navigation), record for record. JJFFE and
the DOS release therefore agree on every position, bitmap id and action here.

## The two buttons

`DATA_005104` is the first `ConButton` (62 bytes; `DATA_005100` is a small
global before it). The right-hand records in `ffedat.asm`:

| Button | Record | x, y | idle / lit bitmap | `DATA_007684` hot area | Action | Key |
|---|---|---|---|---|---|---|
| Hyperspace | 0x0f | 296,178 | 0x99 / 0xa8 | 300,178-315,197 | 0x48 | H |
| Text labels | 0x10 | 274,178 | 0x9a / 0xa9 | 280,178-295,197 | 0x6c | l |
| Navigation computer | 0x11 | 261,177 | 0x9b / 0xaa | 265,177-280,196 | 0x6e | n |

Bitmap ids below 96 are the primary table; from 96 they are
`secondary/[id-96]`. The idle/lit pairs are distinct images (the labels pair is
33x23, the navigation pair 25x25), not palettes of one image. `FUNC_000991`
shows button 0x10 unconditionally and 0x11 only with the navigation-computer
equipment bit (`[ship+0xc8]` bit 0x20); `FUNC_000261` draws each visible button's
current image in table order. The icons face the right-hand cockpit recesses at
the bottom of the 320x200 screen.

## Text labels (`l`, action 0x6c)

- The action handler at `ffe.asm` **78447-78465** (`JUMP_004394`) XORs player
  flag byte `+0xe9` with 0x04 and sets button 0x10's image to 1 when set, else
  0, then plays sample 0x14. This is the original's F1116/F1120 console update.
- The label drawing is in `FUNC_001099` at **80775-80781**: when
  `[player+0xe9]&4` (and the view is not combat computer 6) it calls
  `FUNC_000989` with `FUNC_001108`.
- `FUNC_000989` (**73373-73422**) walks object slots 0x72 down to 1, takes live
  objects (flag byte bit 0x8) that are not removed (bit 0x20), projects them
  with `FUNC_001531`, and rejects labels outside x 1..315, y 1..152.
- `FUNC_001108` (**81602-81687**) draws the object's name at `+0x124` with
  `FUNC_001584_TextWriteDefaultClip`: **font 15, colour 15, unshadowed**, at
  screen x + 1, y. It skips the object whose model is 0x9a and, near the player,
  ships with type byte 0x87 == 0xb and AI state 0x24 (a docked/landed state
  pair). The port labels bodies and its NPC traffic; its traffic has no 0x24
  state, so the near-target suppression is not reproduced.
- Older `srctools/cplayer.txt` says `b[D8871] & 4`, but that is a different
  address from the executed assembly's `player+0xe9`. The assembly is
  authoritative; the cplayer note belongs to an older address map.

## The places list (`n`, action 0x6e)

The list is the **navigation computer** record 0x11 (hot area 265,177-280,196,
action 0x6e, key `n`). The neighbouring record 0x10 (`l`, action 0x6c) is the
**text labels** toggle above, not the list; the DOS release's console has the
same split (`FIRSTENC.EXE` 0xbe950 labels, 0xbe98e navigation).

- The action handler at **78537-78567** (`JUMP_004405`) checks equipment bit
  0x20, flips `DATA_007250` (0/1), and sets button 0x11's image to 1 when the
  list opens, else 0. Closing it removes hot areas 0xd1..0xe2 and clears
  `DATA_007251`.
- `FUNC_001099` draws the list at **80511-80585** when `DATA_007250` is set:
  - it walks object slots 0x72 down to 1 and takes live objects whose flag byte
    at object+0x14c has bit **0x10**. F880 sets that bit for type codes
    **0x20-0x23**, its stations and starports (**64695-64699**). It is not set
    for stars, planets or moons, so the original list is exactly "the places in
    this system".
  - **First column x 10, second x 230** (`mov dword [ebp-0x4],0xa` at 80515 and
    `0xe6` at 80579); **y starts 5** (`mov edi,0x5` at 80516) and advances 15
    pixels a row (`add edi,0xf`, 80575) until it reaches 0x8c (140). That is
    **nine rows a column** (y 5, 20, ... 125) and at most 18 entries, actions
    0xd1..0xe2.
  - Each entry blits **bitmap 0x31** (primary table, 11x10, masked zero) at
    (x, y) and writes the object's name at `+0x198` at **(x+15, y)** with
    `TextWriteDefaultClip` **colour 0x11 (17)**. 0x31 is the grey raised square
    the manuals and screenshots show (the "checkbox"): body 6, edge 0xa/0xf,
    shadow 4.
  - The hot area is added at the marker's top-left corner, 10 pixels wide and
    high (80536-80551): x 10..=20 and x 230..=240, y 5+15r ..= 15+15r. The name
    is not a hot area; the start guide says to click the station's icon.
  - The list has no background, title, distance or selected-row highlight: it
    is drawn over the already-rendered view. The first column's rows 0..8 are
    y 5..125, all above the HUD's Set/Act line at (2,148) and F272's message
    window at (196,159); with the original geometry neither can cover the
    list (and the port draws the list last, so nothing drawn after the view
    can cover it).
- A click on marker k (hot area 0xd1+k) runs **78329-78399**: it finds the k-th
  matching object, stores the object's number in the player target byte
  `+0xec`, clears the target-select mode, calls `F1020` (autopilot) when the
  player mode `+0xe8` is not 2 and below 0x28 (and toggles it off/on when it is
  2), resets button 0x11 to image 0, removes the row hot areas, and closes the
  list. So one click both locks the target and engages the autopilot.

### The frame the owner reported

The FFE manual's Figure 20 ("The Navigation Computer at Gateway") is the
original frame: the 12 places of Gateway in exactly this layout — nine down
the left column (Fortress Saskatoon, Old Curie, Old Blackelk, Turner, Turner
Town, Dublin Citadel, Sarajevo, Seattle, Larson) and three down the second
(New Chernobyl, Sarajevo, Micca Town). The markers' visible grey squares begin
at x 12 and x 232 and the names at x 25 and x 245; rows are 15 pixels. The
frame also shows the Set/Act HUD at the lower left, Alt/Hope at the lower
right and the console below. A crop is in `docs/evidence/places/`.

The report's "left column x≈8, right x≈149, 10 left + 3 right" came from a
scaled/annotated copy of that frame; the executable and the manual agree on
x 10/x 230 and nine rows (the start guide also calls the station "below Dublin
Citadel" the one in the first column). The port's earlier list used x 5/x 230
and names at x+15 from x 5, five pixels left of F1099; that is the layout that
was wrong.

## In ffe-rs

- `console::RIGHT_BUTTONS` holds the three table rows; `screen::draw` blits
  idle/lit images in table order. The labels button is lit while
  `ConsoleState::labels` is set, the navigation button while the list is open.
  The hyperspace image has no lit state in the original's code, so it is
  always idle here.
- `main.rs` maps `l` and the labels icon to `ConsoleState::labels`; the flag
  starts off, as the player flag does. `Screen::labels` carries the names and
  camera positions; `Screen::draw_body_labels` applies F989's bounds and
  F1108's position, font and colour. The port's "objects" are the navigator's
  bodies plus its NPC ships (`docs/notes/npc.md`), so labels appear on stars,
  planets, moons, stations, starports and traffic.
- `Navigator::places()`/`navigation::place_indices` use only station/starport
  bodies, in `bodies` order (the system generator's creation order, which F927's
  descending slots preserve). The list takes the current page: 18 entries in
  two columns of nine, exactly F1099's hot-area count. `View::Places` pages
  18 at a time only when a generated system holds more than the original's 18;
  the original could never have more than 18 markers.
- `Screen::place_cell` is the marker's top-left cell (10, 5+15r / 230, 5+15r);
  `Screen::place_row` is the original marker hot area (x 10..=20 / 230..=240,
  y 5+15r..=15+15r). `Screen::draw_places` blits marker 0x31 and writes names
  at x+15 in colour 17 over the view, then presents. There is no background,
  and the port's own top bar and status block (not the original's) are gone.
- `n` and the navigation icon open/close `View::Places`; the 0x11 image is lit
  from `nav.view == Places`, so a click on a row (which closes the view) turns
  it off. A marker click sets the selected row and calls `Navigator::engage`,
  which sets `locked` (the original's target byte) as well as `target`, so the
  autopilot flies there. The ffe-rs all-body system map stays on Tab.
- `console_view`'s `the_gateway_places_list_is_the_originals_columns_and_cannot_be_covered`
  draws the manual's 12 names and checks every marker/name cell, the 9+3 split,
  the empty space between the columns, and that a populated F272 log changes no
  list pixel. `the_gateway_places_frame_over_a_landing_view` saves the
  before/after evidence frames.

## Known differences

- **Equipment.** The port has no equipment inventory, so the navigation button
  and `n` are always available; the original hides/ignores them without
  `[ship+0xc8]` bit 0x20. The labels button is unconditional in both.
- **Paging.** The original has one page of 18; the port pages with
  Left/Right/Page Up/Down only when a generated system has more than 18 places.
- **Object set.** The port's labels and list come from the navigator's bodies
  plus its NPC ships (`docs/notes/npc.md`); the original also labels mission
  objects, which the port does not have. The places list stays stations and
  starports only, as the original's does.
- **Starting system.** The manual's Gateway frame cannot be reproduced from
  this port's new game yet: F829's record 1 (the port's start) is Ross 154,
  whose generated system has one starport (Sirocco Station). The DOS original's
  default start is record 0 at Gateway [-1,4] (Old Blackelk on Hope), and
  Gateway is not among the 12 hand-coded systems `galaxy()` imports. Reproducing
  the 12 Gateway entries needs the start-type-0 system generation/import (an
  owner decision outside this list); the layout is verified against the manual
  frame with the same 12 names.
- **F1108's near-target suppression** (type 0x87 == 0xb and AI state 0x24) is
  not reproduced: the port's traffic AI has no 0x24 docked/landed state pair
  (its ships in station bays are not modelled).
