# The runtime upper palette (`F1326`/`F1327`/`F1330`/`F1331`)

Ported 2026-10-25. Source of truth: DOS FFE; the labels are the JJFFE 2.8a7
disassembly (`jjffe-master/ffe.asm`, `ffedat.asm`), which the port's earlier
notes already cite. The executed original is `jjffe-smoke.exe` built from the
same assembly by `tools/reference/build-smoke.ps1`.

## 1. Source map

| Piece | Label | Reference |
|---|---|---|
| Mark a 12-bit colour in the request cube | `FUNC_001326_DPal32to16` | `ffe.asm:82761-82781` |
| Resolve a request to a palette index | `FUNC_001327_DPal16toPal` | `ffe.asm:82785-82803` |
| Clear the cube | `FUNC_001328_DPalClear` | `ffe.asm:82807-82818` |
| Count/tighten a box, find its longest axis | `FUNC_001329` | `ffe.asm:82822-82958` |
| Build the 64-entry bank | `FUNC_001330_DPalBuild` | `ffe.asm:82962-83394` |
| Upload the changed entries | `FUNC_001331_DPalSet` | `ffe.asm:83398-83427` |
| Fixed low palette upload (128 entries) | `FUNC_001641_SetWholePalette`, `DATA_007769` | `ffe.asm:98136-98161` |
| Fixed request map (4096 bytes) | `DATA_007770` | `ffedat.asm:24298` |
| Cube / box slots / changed list / 128 colours | `DATA_009139`, `DATA_009140`, `DATA_009141`, `DATA_009142`, `DATA_009143`, `DATA_009144` | `ffedat.asm:28768-28783`, addresses `0x47e6c..0x49074` in `ffedat.lst` |
| Two-bank toggle | `DATA_007634 ^= 0x40` | `ffe.asm:83423` |
| Clear colour index | `DATA_009222` | set by `F1330` 83384-83388 |

The exact layout is unusual and matters:

- `DATA_009139[4096]` is the cube. `0` = unused, `1` = registered for the next
  build, `box + 1` = built.
- `DATA_009140` is a one-byte prefix to `DATA_009141[64]`, so `DATA_009141`
  is indexed by `cube[cell] - 1`: it holds the palette index (`0x80 + slot`)
  of each box.
- `DATA_009142[67]`/`DATA_009143` are the changed-slot list and count;
  `DATA_009144` is 128 RGB entries (two 64-entry banks, 384 bytes).

## 2. Colour words and the fixed low half

A request is 12 bits, four per component: `r << 8 | g << 4 | b` (`F1326`;
`F1692` masks materials with `0xfff`). The palette itself is six-bit. The low
128 entries are the fixed imported palette `base:palette/low` (`DATA_007769`,
uploaded by `F1641`, expanded by `<< 2`, max 252). `DATA_007770`
(`base:palette/fixed-map`) maps 4096 request words to `0` or a low index + 1:
black (`0x000`), the grey ramp (`0x111`..`0xfff`), and a few primaries. A
fixed word never enters the cube and resolves to its low entry. Examples with
the real assets:

| word | low entry | six-bit | RGB |
|---|---|---|---|
| `0x000` | 0 | `(0,0,0)` | `#000000` |
| `0x444` | 4 | `(16,16,16)` | `#404040` |
| `0xf00` | 16 | `(60,0,0)` | `#f00000` |
| `0xfff` | 15 | `(60,60,60)` | `#f0f0f0` |
| `0x004` | 95 | `(60,42,4)` | `#f0a810` |

The last row is the fixed-map word for low entry 95 (amber): text/UI colours
are low indices, so they never go through the dynamic half. F1100's target
display is such an index: 0x11 is low entry 17 (green `#00f000`) and 0x10 is
16 (red `#f00000`), which the fixed map reaches from `0x0f0`/`0xf00`. The
scanner's contact table (`DATA_008683`) and the console art are also low
indices.

## 3. The builder

`F1330` is called once per rebuilt frame and writes **one** 64-entry bank,
toggling `DATA_007634` between 0 and 64. The upper 128 video entries are
therefore the last two builds: `0x80..0xbf` and `0xc0..0xff`. This lets an
object keep a palette index across a rebuild even though the palette moves.

1. `F1330` first registers the background colour (`DATA_009217`) and stores
   its key in `DATA_009222` (82970-82975).
2. If the occupied cube has more than 64 cells, it median-cuts the occupied
   extent into exactly 64 boxes: tighten a box to its occupied cells
   (`F1329`), split the heap root on its **longest-axis extent** at
   `(lo + hi) / 2`, tighten both halves, sink the shrunken root down the
   larger-child path, and insert the right half at the end of the heap, where
   it sifts up by the same extent key (83025-83159). The key is `box + 0xc`
   (the longest extent), not the cell count. With at most 64 cells it makes
   one box per occupied cell (83225-83273).
3. Each box's cells are written as `box + 1` in the cube; the slots array is
   zeroed first (83160-83223).
4. The previous bank's 64 colours claim the boxes that contain them
   (`DATA_009141[cell-1] = 0x80 + bank + entry`); colours whose box is
   already claimed, or whose cell is unused, go to a list (83274-83310).
5. Each still-unclaimed box takes an entry from the **end** of that list
   (83314-83330), and its representative colour is written:
   `2 * (lo + hi - 1) + 1` per component (`4 * cell + 1` for a single cell),
   so every dynamic component is odd and in `1..=61` (83331-83377).
6. `F1331` uploads the changed slots at `0x80 + slot`, then toggles the bank.

`F1327` returns `DATA_007770[key] - 1` for a fixed word, else
`DATA_009141[DATA_009139[key] - 1]`. The cube is cleared **once per frame** by
`F1786` (`ffe.asm:61644`), which the boot/intro and game loops call immediately
before F961/drawing; `F1328` is also called by the palette-file loads
(`F125`/`F126`) and after `F1641`'s low palette. Each build is therefore cut
around the colours that frame queues, and two frames of one animation build the
same 64 boxes (the executed original's frames 1514 and 1515 have identical
upper palettes). The previous bank still claims its boxes, so slot identity
within a frame is unchanged. The port clears the cube at the start of each
`ModelViewer::render_matrix`, matching `F1786`.

**The median-cut heap.** The port originally compared the wrong heap key (the
cell count) and never sifted the shrunken root: it split `heap[1]` again and
again until most of the 64 boxes were empty, whose `2 * (lo + hi - 1) + 1`
representative is `31,31,31` (six-bit grey). Those grey entries filled the
previous bank and the planet's band texture resolved through them. The builder
now uses `F1329`'s longest extent (`box + 0xc`) and the original's
sink-root/insert-right movement. `a_uniform_cube_cuts_into_sixty_four_non_empty_boxes`
pins it: an 8x8x8 cube yields 64 non-empty boxes and no grey representatives.

**When it runs:** the intro loop calls it after F961/F962 every frame
(`ffe.asm:68461`); the in-game loop calls it on the redraw branch
(`ffe.asm:68631`); the scene transitions call it in `F915`/`F919`
(`68027`, `68168`) and the splash/exit paths (`68123`, `68461`, `68631`).

## 4. The port

- `crates/ffe-rs/src/palette.rs` is the pure builder above:
  `RuntimePalette::{register, build, resolve, index, entry, entry6, changed,
  bank, upper6}`. The median cut, the odd representatives, the previous-bank
  slot identity and the two-bank toggle are reproduced. `changed()` is the
  list `F1331` would upload; the port has no indexed DAC, so it resolves RGB.
- `textures.rs` owns one palette per `TextureSet`. `rgb()` tries the runtime
  palette first (fixed words resolve to the low half), then falls back to the
  pre-palette nibble approximation only for a word that is registered but not
  yet mapped.
- `model_viewer.rs` registers every colour the executed frame will draw
  (faces, lit/flat/texture slot colours, lines, points, splines, polygons,
  thruster glows/lines) before `build()`, then draws. The background word is
  registered first, like `F1330`'s `DATA_009217` call. This is one build per
  model render; the cube is cleared at the start of each frame's registration
  and both banks persist on the viewer. Text glyph executions and the curved
  surface queue are built before the build, so the glyph faces' and the
  terrain's F1326 colours are in the cube. The text's own material colour is
  resolved through F1692/F1693 and given to the glyph instance (F1747). `F1747`
  negates the text frame's first row when F1740 leaves a left-handed frame
  (`5f64b-5f658`); without it the glyphs are mirrored (`GAME` read `ƎMAפ`).
- `ModelViewer::surface_detail` carries `DATA_008812`, the "Level of Shape
  Detail" F1876 uses for the curved-surface subdivision. The executed boot has
  **1** (the default High row, stored less one); the old hard-coded 2
  over-subdivided the patch and aliased the band texture into per-texel
  speckle. `screen.rs` passes `Screen::surface_detail`, which `main.rs` sets
  from `prefs.shape_detail - 1` and clamps at 0 like `F917`.
- `screen.rs::draw_intro` sets F961's background word before rendering and
  fills the frame with its resolved RGB. `front_end::IntroFrame::background`
  carries F961's `(0,0,0)`/`(0,0,4)`.
- `screen.rs::draw` sets the viewer's background from the effective
  space/haze colour (`surface_sky` or `sky`) before the models are drawn.
  The flight view itself still clears to `prefs::space_colour_rgb`.

## 5. Evidence

**Intro switch.** A new reference mode, `jjffe-smoke --intro-live <frames>`,
runs the original's real boot and intro loop (F961/F962/F1330/F1331) with no
start key and writes the final frame and palette. Results
(`tools/reference/headless.c`):

| reference tic | upper entries | clear index | background |
|---|---|---|---|
| `0x795cc` (model 177) | 0 | 0 | black |
| `0x82a00` (first model-176 frame) | 1 | 255 | `(1,1,17)` = `#040444` |
| one frame later | 65 | — | `(1,1,19)` = `#04044c` |
| a few frames later | 128 | — | full scene |

The port at `MODEL_SWITCH` has exactly one upper entry, six-bit `(1,1,17)`,
resolves `(0,0,4)` to it, and clears to `#040444`. This is pinned by
`tests/palette_reference.rs` (ignored, cache-backed).

**Galaxy map.** `verify-galaxy-map.py`'s frame (original
`--game-frame 400 0x3d`, port `galaxy_map_view`) compared pixel by pixel in the
320x158 view:

| | mean max-channel delta | pixels changed | within 4 | within 16 |
|---|---|---|---|---|
| before | 2.06 | 7,813 | 97.2% | 97.4% |
| after | 1.49 | 1,406 | 97.2% | 98.8% |

The remaining differences are dominated by a green shade: the original has
`(52,84,52)` where the port's smaller registered colour set gives `(36,68,36)`,
plus a few map-overlay positions the existing oracle already accepts.

**Banded intro planet.** Model 176's big planet is surface `kind 3` (polar,
F1842): 32 band colours and texture block 95, generated from `DATA_008812`.
The reference adapter's boot leaves `DATA_008812 = 1` (a local instrumented
`--intro-live`/`--game-frame` build prints it; it is 0 on frame 0, then 1).
Building the same patch at 2 subdivided it finer than the source and produced
the frame that the owner saw as orange/teal speckle. At 1 the planet's horizontal
same-colour run length is 2.26 (the original frame 1515 is 2.51; the pre-fix
port was 1.80), and the rebuilt upper palette shares 47 of its 56 colours with
the executed frame's 64. The band table's nine distinct values times the five
visible light shades keep this frame in the at-most-64-cells branch, so its
representatives are single-cell `4 * cell + 1`; the heap fix matters for the
frames that cross 64 (the galaxy and the in-game surfaces).
`the_intro_planet_uses_the_runtime_shape_detail` and
`consecutive_intro_frames_build_the_same_upper_palette` pin the two.
`docs/evidence/visuals/intro-palette-after.txt` is the frame's six-bit bank.

**Logo text.** Model 177's child 361 text (DOS `0x408a` = `GAME!TEK`; JJFFE's
string is `FRONTIER`, `intro-todo.md` D5) read mirrored on every animation
frame. F1747's handedness flip fixes it; the before/after pair is
`docs/evidence/visuals/game-tek-letters-*.png`, and
`the_game_tek_logo_text_is_not_mirrored` pins the assembled frame hash.

**Scanner and space.** The scanner view frames (`scanner-system`,
`scanner-tunnels`, `scanner-blink-*`) are pixel-identical before and after:
their art and contacts are all low-palette indices. `space-light-blue` still
clears to `#0c0c4c` and `space-black` to `#000000`. `landed-starport` changes
51 pixels by at most 4 (terrain dynamic colours); the sky is unchanged.

## 6. Remaining differences and decisions

- **Terrain queue colours** are registered before the build and the cube is
  cleared per frame (see §3). `verify-surface.py` still reports structural
  coverage differences (the port's starport hull draws less than the
  original's), but the surface colours are stable across frames and
  `landed_surface_pixels_are_stable_across_frames` pins it.
- **One build per model render** instead of one per scene frame. The port's
  build sees only that model's colours, whereas the original registers every
  object before one build. The intro (one model) is exact; a multi-object
  flight frame can partition the boxes differently.
- **The intro's backdrop content** now draws: model 176's selected branch
  (348/349) runs its child tree, and child curved surfaces (model 449) are
  collected from the child executions. The one failing descendant (456 -> 321,
  a mode-6 call whose basis is stale stack in the original) is reported and
  skipped without discarding the scene.
- **The atmosphere ring is a palette texture.** F1879's F1805 ring quad
  (bitmap `0x43` and its seven slot colours) used to be a two-colour Gouraud
  stand-in; the software fill invented 268 RGB values that no palette entry
  held (330 distinct colours in the `0x181534` view). It now takes the
  F1805/F1647 textured path through the frame's registered colours, giving 71
  distinct colours, the same as the executed original's 71
  (`docs/evidence/visuals/intro-colour-stats.txt`).
- **The non-default space colours** remain the owner's EXP-013 interpolation
  from the captured `#0c0c4c`; the runtime build is not used to derive the
  preference's four intermediate values.
- **No DAC upload:** the port resolves to RGB for its 32-bit framebuffer.
  `RuntimePalette::changed()`/`entry6()` expose what `F1331` would upload, and
  the alternation is kept in `bank()`/`upper6()`.
