# The galaxy map (F840-F863, F1684, F1699-F1701, F1769)

Read from `jjffe-master/ffe.asm`. The map is a 3D scene drawn by the ordinary model machinery: the
camera is a matrix built from the GUI's rotation accumulators, and the scene is a few model runs.

## The scene (F840)

- **Camera**: `F1675` builds the matrix from the accumulators (`F1445`), swapped into the
  object's matrix layout; the zoom is `DATA_008804+0x242` (added to the objects' z), the centre of the view
  `+0x23a/+0x23e/+0x250..+0x258`.
- **Grid** (model `0xa6`): drawn when bit 0 of the map flags (`+0x24e`) is set.
- **Stars** (`F841`, model `0xab`): the 3 by 3 sectors around the centre, each `F841(buffer, object, x, y, draw)`:
  it builds the sector's six-byte vertex records, the object's star fields (`+0xa0` the sector code, `+0xa4` the star
  count, `+0xa6 + 2 i` the star's type bytes) and, with `draw`, runs the model through `F1684`. A hand-made sector then
  also draws its own model (`DATA_008776`).
- **Range sphere** (model `0xaa`, `F862`) and the **selection marker** (model `0xa9`).

## Light points (opcode 30 `0x801e`, F1769) — ported, cache v23

Model `0xab` is one command, `0x801e`. `F1769` loops over the count in global word 4 (`+0xa4`); for star `i` of type
`t` (the low nibble of byte `+0xa6 + 2 i`):

1. if global word 1 has bit 5, a line from vertex `2i` to `2i + 1` (the stalk to the grid) in material `DATA_007828[t]`;
2. a point at vertex `2i` in material `DATA_007820[t]`, its size the packed angle `DATA_007821[t] >> 7`;
3. if the depth of **vertex slot 0** (`[DATA_009200] + 0xc`, not the star's own) is at most `DATA_007822[t]`, a label
   (`F1764`): the string selector `0x8400` (a system name, `sector::system_name`) and the sector code with the star's
   number `i` in bits 26..31. `F1764` queues the label only inside x 2..=317, y 4..=149.

Slot 0 is vertex 0, resolved when the first star's point is drawn, so the label test compares every star against the
first star's depth: the labels of a sector appear together as that star comes within each type's range.

`ffe_core::star_points` holds the tables (cache v23, `base:world/star-points`, 224 bytes: the four tables of 14 dwords
in the order `7820`, `7821`, `7822`, `7828`; RVAs `0x104038`, `0x104070`, `0x1040a8`, `0x10410c` in floppy 1.02);
`Vertices::with_stars` attaches them and the object's stars, `execute_live` runs `F1769`, and
`ModelViewer::execute_stars` runs model `0xab` over a sector's records.

## Verification

- `--map-stars` in `headless.c`: the real `F841` builds the records and star fields of 49 hand-made and 191 generated
  sectors, then the real `F1772` runs model `0xab` with the point, line and label calls recorded (the point probe
  resolves its vertex like `F1699`, so slot 0 is what the original leaves). `verify-map-stars.py` and
  `map_stars_reference.rs`: all 240 cases match (1,589 stars, 831 stalks, 214 queued labels and their depth keys); trace
  SHA-256 `59e7dec3e998d06d7d8011653c47e65ab0eda0df4ad824c3ecf08ab753cb8a9c`.
- `verify-assets.py`: the imported tables equal JJFFE's `DATA_007820`, `7821`, `7822` and `7828`.
- Synthetic tests: `crates/ffe-core/tests/star_points.rs`.

## The dots (F1699, F1700, F1701, F1800, F1801, F1610, F1611) — ported, cache v25

An unlit point (material bit `0x2000`: the stars) is a sphere seen from afar. F1699 turns its angle word (F1474) into a radius
(shifted by the instance scale less 8) and, when the radius is at most an eighth of the depth, divides it by the depth
(`special_divide`) into a size in pixels; nearer it computes the ellipse's centre and size from the exact projection (the near
path, `model_dot::unlit_dot`); a radius of the depth or more draws nothing, and a depth under 0x40 is too near. F1700 then
clips the dot to the view and queues F1800 (size 0 or over 9: one particle in the colour F1691 gives) or F1801 (sizes 1 to 9: a
halo particle, of the size `DATA_007836[size-1]` in the colour mixed a third towards the sky, then a core of
`DATA_007835[size-1]`). F1611 is the particle: sizes 0..5 are hand-drawn patterns, larger ones circles built from a midpoint
walk, drawn as runs clipped to 320 by 158. Lit points are shaded spheres drawn by polygons, which are not ported.

`ModelViewer` draws a point as `Shape::Spans` through `model_dot::point_dot`; a star's label is the system name (F830, selector
`0x8400`) in the bitmap font at the projected point.

## The scene (F840, F862, F828) — ported

`ffe_rs::galaxy_map::GalaxyMap`: the centre (16.16 sectors), the zoom depth (`0x30d40`), the flags (`0x69`: grid, stalks),
the two accumulators and the player's own position. The camera matrix is `build_yzt(lo << 7, hi << 7)` with its rows permuted
as F840 does; the centre sector sits at `M * (0, -fy, -fx) >> 14` (+ zoom in z) with `fx`, `fy` the centre's offset from the
sector middle, and the neighbours `M * (0, 0, 2^29) >> 13` and `M * (0, 2^29, 0) >> 13` further along. Order of drawing: the grid
(`0xa6`), then for the centre sector and its eight neighbours the stars (`0xab`) and, for the 49 hand-made sectors, the model
`DATA_008776` gives (`0xa8`, `0x155..0x159`, `0xad`, `0xae`: labels such as "CORE"), then the range sphere (`0xaa`, its radius word
`range * 65535 / 3200`) about the player's system and the marker (`0xa9`) on the selection (F862 places both). Each model is culled
as F1686 does. The selection is the star of the centre sector nearest the centre by `|dx| + |dy|` (the star bytes against the centre's
offset in 512 units), within 32; it is looked for again when the centre moves.

The console shows the date (green), then `name Dist. d light years`, a line break, the star's type text (`0x8488 + class`, with
`0x8498 + n` before it for multiple stars) and `: ` and the system's description (F869's first dword, a `0x84xx` string); the
corner shows `Sector: x,y` (relative to Sol's `0x1718, 0x1524`) and the fuel line (F828's strings `DATA_007004..7006`:
`Fuel Required: n` when the drive can reach it, `Insufficient Fuel (n required)` when the hold is short, `Out of Range` when
the drive cannot). `GalaxyMap::fuel_text` picks them from `Report::{result, fuel_needed}` exactly as F828's sign tests do.

The selection is the hyperspace destination: F832 stores its position, code and F833 distance (`+0x25c..0x264`, `+0x27c`,
`+0x26c`), and F908's console record 0x0f (key `H`) jumps to it. It is not a body lock; the port stores it as
`Navigator::destination` and clears it when F840 finds no star near the centre or when F909 arrives. The marker (`0xa9`) is
only its display. A click on the map is not a selection in the original (the centre picks), so the port ignores it; `H` works
after the map closes, when the original's console button is still live.

## Verification

- `--map-stars` (see above) for the stars' calls and labels.
- `--particle` and `--dots`: the particle rasteriser and unlit dots against the real F1610/F1611 and F1699 with the queue flushed;
  `verify-dots.py`, `dots_reference.rs`: all 12,000 cases match pixel for pixel; trace SHA-256 in `verify-dots.py`'s output.
- `verify-galaxy-map.py`: the real game's F3 map at a new game (Ross 154) against ours (`galaxy_map_view.rs`): 8,249 of the
  original's 8,270 overlay pixels are ours too (we draw 8,269); 85% in the same colour. The rest are the dim grid lines and stalks,
  whose 12-bit colours the original approximates in its runtime palette (not reproduced), and the textured "CORE" letters, which
  the original shows in a light peach and we in grey.
- Synthetic tests: `crates/ffe-core/tests/{star_points,model_dot}.rs`.

## The map's console page (F834, F840, F969) — ported 2026-10-25

The map is a separate console page, not the flight page with a map on top:

- **Entry (F834, `ffe.asm:56856-56896`)** shows the F3 map screen button `0x0c`
  (image 1) and the map row `DATA_005100` records **0x1c..0x24**, lighting the
  map icon **0x24**. It does not hide the flight console: the four screen
  buttons, the time-acceleration meter, the scanner globe and the three ship
  lamps (0x37-0x39) stay visible, exactly as the executed original's F3 frame
  shows (captured with `jjffe-smoke --game-frame 400 0x3d`). The map row sits
  at the view row's positions (0x12..0x19) and draws after it in `F261`'s index
  order, so its 25x25 icons cover the flight view icons.
- **F840's keys (`ffe.asm:57197-57478`)** re-image that row:

  | Record | Position | Image 0/1 | Original key and meaning |
  |---|---|---|---|
  | 0x1c | 261,177 | 192/203 | row |
  | 0x1d | 237,174 | 193/204 | row |
  | 0x1e | 224,169 | 194/205 | row |
  | **0x1f** | 211,168 | 195/206 | **F10** details + previous (radio 10) |
  | **0x20** | 198,165 | 196/207 | **F9** details + next (radio 10) |
  | **0x21** | 181,163 | 197/208 | **F8** picture mode |
  | **0x22** | 162,163 | 198/209 | **F7** details |
  | **0x23** | 142,163 | 199/210 | **F6** select/reset the row |
  | **0x24** | 125,163 | 200/211 | the map icon, lit by F834 |

  `F262`'s radio reset makes 0x21/0x22/0x23/0x24 (group 1) mutually exclusive;
  `0x1f`/`0x20` are group 10, so F9/F10 can light details and a direction
  together. `Console::apply_map` replays the original's call order to keep that
  state. The reference states (F3 then F6/F7/F8/F9/F10/F5) were captured and
  compared; each changes exactly the records in the table above.

- **Map-local keys**: the original's map page owns **F5-F12** while it is
  showing (`ffe_rs::map_keys`, `crates/ffe-rs/src/map_keys.rs`):

  | Key | Action | Console image |
  |---|---|---|
  | **F5** | re-show the map controls page (F834; also leaves details/picture) | 0x24 |
  | **F6** | select/reset the controls row | 0x23 |
  | **F7** | details page | 0x22 |
  | **F8** | picture mode | 0x21 |
  | **F9** | details with next | 0x22 + 0x20 |
  | **F10** | details with previous | 0x22 + 0x1f |
  | **F11** | zoom in (`sub` of the zoom word) | — |
  | **F12** | zoom out (`add` of the zoom word) | — |

  The preservation branch's note that F5 jumps and F11/F12 toggle stalks/grid
  is not what F840 does: F834 is the page/controls entry, `H` is the hyperspace
  key, and F11/F12 are the first branch's zoom. Only inside details/picture
  (the unported `ebx+0x299` branch) do F11/F12 toggle stalks (0x24e bit 5) and
  grid (bit 0); that second branch is not implemented yet.

- **Global keys are not stolen**: F5-F12 are map-local **only** while the
  F834 page is actually showing (`NavView::Galaxy` and the 3D map available).
  Outside the map F5/Shift+F5 still save/load, F6-F10 still change views, F11
  still writes a screenshot and F12/Shift+F12 still open the model viewer/lab.
  The port's Tab system map and `n` places list keep those global keys; they
  are not the original's map page. Clicks on the map row do not change the
  flight view while the page is open.

- **Hot areas**: F969 adds `DATA_007241`'s one extra area,
  `(268,156)-(319,177)`, action `0x80`; F840 itself ignores action `0x80`, so
  the port records the area (`MAP_EXTRA_HOT_AREA`) but does not route it.
- **Not yet**: the details page (F842) and picture mode (F844-F849) are still
  later slices. The port lights their original buttons and reports
  "not ported yet" instead of inventing a screen.

The port's arrow scrolling, Page Up/Down and +/- zoom, mouse drag, `C`
recentre, `G`/F3-icon entry and `H`/Enter jump are retained alongside the
original keys and are recorded in `docs/DEVIATIONS.md` (EXP-008). The frames
`map-page-*.ppm` and `map-script-f*.ppm` come from `the_map_console_page_states`
and `the_map_key_script_selects_reimages_and_jumps` in
`crates/ffe-rs/tests/galaxy_map_view.rs`.

## Not done

The drawn hyperspace jump scene (F908's mode 4/F909's F148/F912; the destination, gates and arrival are ported), the details
page (F842), picture mode (F844-F849), the string engine's templates (the fuel line's number is plain), the textured text
colour, the mouse's hover pick of a star, and the lit-sphere branch of F1700.
