# Port log

Running record of decisions, discrepancies and open questions. Newest entries at the top; append rather than rewrite, unless a decision supersedes an earlier one (mark it).

---

## 2026-10-23 — Auto-roll F609 and the whole tick in a world (v0.49l)

Notes: `docs/notes/physics.md`.

- `Motion::auto_roll` (F609) and `Player::orient_and_thrust_in` / `step_in`:
  gravity, auto-roll, and the F606 countdown now run inside the player's tick
  with a world of bodies. `--world-step` (4096 whole ticks, 482 frame changes,
  3,183 pulls) matches the original, every body's cache included.
- `Player::orient_and_thrust` (free space) no longer rejects a low altitude; it
  still rejects a parent frame, which needs a world.
- Still missing for planet flight: the altitude itself (F569/F1033 collisions
  compute `DATA_008867`), a planet in the flight scenario, the surface drawing.

---

## 2026-10-23 — Frames of reference ported and verified (v0.49k)

Notes: `docs/notes/frames.md`.

- `ffe_core::frames::Bodies` ports F590, F595 (non-station), F604, F605, F606
  and F1677/F1678 over an object list with the original's frame-stamp cache.
  4096 oracle cases (a seven-body tree, each routine 1,024 times) match,
  including every cache and stamp byte.
- Rotating frames (`+0x57`, stations) and a station's own velocity are not
  ported: those paths return `None`. They belong with station docking.
- Still missing for planet flight: auto-roll F609, collisions F569/F1033, wiring
  gravity and frame selection into `Player`, and the surface drawing.

---

## 2026-10-23 — Gravity F603 ported and verified (v0.49j)

Next step toward flying near a planet. Notes: `docs/notes/physics.md`.

- `Motion::gravity` ports F603 with the ffp helpers F1496/F1482/F1483 and the
  F1658 truncation (F1657/F1660 and F1466 already existed). 4096 oracle cases,
  1818 of which move the velocity, match exactly, including the truncated
  vector left in `DATA_008708..DATA_008710`.
- The cube step keeps the square's exponent for the cubed number, as the
  original does; the divide by the cube is then what makes the units work out.
- Still missing for planet flight: the frame-of-reference transfers (F604/F605/
  F606, F590), auto-roll F609, collisions F569/F1033 and the surface drawing.

---

## 2026-10-23 — Surface plumbing: entries, dispatch and queued words (v0.48e)

Milestone v0.48 step 2, continuing. Notes: `docs/notes/surfaces.md`.

- **F1823** builds a small hash map from the 16-byte entries (key word +0,
  FindMSB normalisation, `>> 11` bucket, two 16-byte records per bucket). It is
  plumbing; the port should build the map from `Surface::entries` instead of the
  legacy arena.
- **F1833** is a mode dispatch: mode 0 (always used by F1874) selects the flat
  emitters F1715/F1716/F1847, mode 1 the textured F1831/F1832/F1848, then calls
  the table entry `DATA_007843 + 48*mode`.
- **Queued words.** The probe records F1876's order-queue additions (events
  60/61). In the synthetic station context they are always empty: these surfaces
  are planet-scale (axis vectors ~1e8 units) and the instance sits at z 2000, so
  the patch is clipped. Exercising F1876 needs a distance-appropriate instance.
- The whole-table oracle is unchanged (448 models, 0 mismatched, trace
  `4431b861…`); events 60/61 are documentation only and excluded from it, and
  model 118's tamper now alters a surface event rather than dropping a
  documentation one.
- **`--station-model N Z`** sets the instance depth. At z `0x20000000`, model
  118 finally queues words (1,236), which is the capture the F1876 port will be
  built against. At the default 2000 every surface is clipped.

## 2026-10-23 — Surface helper ports (v0.48d)

Milestone v0.48 step 2, continuing. Notes: `docs/notes/surfaces.md`.

- **F1875, F1824 and F1825 ported** (`ffe_core::model_surface`):
  `cross_sign` (sign of the 64-bit 2D cross product, 0 or -1), `mix32` (the
  shift/or/add mixer) and `max_abs_msb` (F1656 MSB of the largest absolute
  coordinate; `i32::MIN` -> 31).
- **Probe modes:** `--surface-sign` (12 vector pairs) and `--surface-helpers`
  (8 words, 9 points), using the new `_ref_surface_sign`/`_ref_surface_mix`/
  `_ref_surface_vector_msb` aliases. The unit tests pin the original's values.
- **F1816/F1817/F1820 are arena allocators**, not geometry: a 1499-block 0x48
  arena and a 2999-block 0x10 arena, with a longjmp after 0xbb8 allocations.
  The port will use Rust collections instead; recorded rather than reproduced.
- Next helpers: F1823 (entry decode), F1833, then F1834/F1835 (quad emission).

## 2026-10-23 — Curved surface resolution (v0.48c)

Milestone v0.48 step 2, first part. Notes: `docs/notes/surfaces.md`.

- **F1874's geometry is ported.** The executor resolves the surface's origin
  through F1470 (view) and each axis through F1472 (object), and records the
  three `axis - origin` vectors F1874 passes to F1876, in
  `SurfaceEmission::{origin, axes}`. A synthetic test checks the resolved origin
  and vectors (including the Q31 identity's one-unit truncation).
- **The census:** 40 opcode 31 records in 39 models, 224 entries.
- **Verified against the original.** `make-probe.py` now redirects F1874's F1876
  call to `trace_surface`, which records events 50-54 (word0/6/10, the origin's
  view position, the three axis vectors) and then calls the original F1876.
  `verify-station-models.py` compares them over the whole table: 448 models,
  **0 mismatched, 40 surface resolutions** (trace `2036461a…`); tampering model
  118's nested events is rejected. The test summary now reports the count.
- **F1876 is not ported.** It is about 1,580 lines with nine helper routines and
  two 480/240-byte topology tables (`DATA_007893`/`DATA_007894`). The note
  records the twelve-argument call contract, the helper call counts and a
  bottom-up port plan with scoped probe hooks (`trace_spline_*` covers opcode 22,
  not surfaces) and a `verify-surfaces.py` oracle. No approximate patch
  rendering was invented.

## 2026-10-23 — 3D text: the glyph programs (v0.48b)

Milestone v0.48 step 1, second half. Notes: `docs/notes/text.md`.

- **F1747 executes a program per character.** The descriptor's table at +0x40
  points at small command programs inside the font model's command block. F1747
  calls F1772 on each one in a nested frame, then reads the word at the returned
  cursor - 2: the program's return record, whose bits 6.. name the font vertex
  that becomes the next pen. The table holds 95 entries (0x20..=0x7e); font 1
  defines 52, and lowercase letters alias the uppercase programs (the starts are
  therefore not sorted).
- **Cache v14** imports all three sets (`base:model-glyphs/000..002`, starts
  plus block): 95/52/95 programs, 3,008/874/624 bytes. Each program must decode
  over the font model's vertices and name an in-range pen vertex, or the build
  fails. 1,781 assets.
- **Rust** (`ModelViewer::draw_texts`): the text vertex (F1470) and the record's
  frame word set the frame (F1740 rules); each character runs through
  `execute_live` with a fresh font vertex cache, and its primitives go through
  the same drawing path as the model's. `0x3016`/`0x3022` expand the drawn
  object's string (+0x124); the viewer defaults it to the model's name.
- **Results:** models 15 and 30 went from 5/7 shapes to 24/26 with nothing
  skipped (their registrations now draw); model 66 from 59 to 74. All six
  labels still draw.
- **Not done:** control codes below 0x20 (the font's +0x3c anchors, used by the
  logo/station multi-line strings), `0x3022`'s 35-column wrap (`DATA_007648`),
  `0x9c00`-`0x9c03` (`DATA_005104`), depth interleaving of glyph geometry, and
  the font model's own normal evaluation in the text frame. No reference oracle
  for text yet; the synthetic test and the census are the current checks.

The previous v13 cache is retained at `.cache/data-v13-backup.bin`.

## 2026-10-23 — Labels, and the string table the text commands index (v0.48a)

Milestone v0.48 step 1, first half. Notes: `docs/notes/text.md`; census:
`crates/ffe-rs/tests/text_census.rs` (local-only, `FFE_TEXT_CACHE`).

- **The model string table was cut short.** `DATA_004682` is declared with 209
  pointers in JJFFE 2.8a7, and cache v11 imported exactly those. The release
  executable's table continues to **219** entries (`police`, the First
  Encounters logo, the station's `DOCK OTHER SIDE` artwork); the model text
  and label commands index the same table as `0x4000 | n`, so six codes above
  `0x40d0` could not resolve. `decode_model_names` now scans pointers while
  they resolve to a NUL-terminated printable string inside the data section,
  rejects a table shorter than the declaration (a misplaced RVA), and records
  the scanned count. Cache **v13** (1778 assets) carries all 219.
- **Labels (opcode 18) are drawn.** `execute_live` already queued F1812; the
  viewer now resolves the string as a model string and draws it with the
  original bitmap font at the projected point in colour 15, clipped to the
  view, which is what F1812 -> F1583 does. All six labels (models 66 and 453)
  resolve. Unsupported codes are counted as skipped, not drawn as something
  else.
- **The census** settles the scope of the remaining 3D text: 236 text records
  in 76 models, 130 distinct model-string codes (all resolvable), and only six
  codes needing handlers (`0x3016` in 50 models, `0x3022` in 10, `0x9c00`-
  `0x9c03` in models 172/175). The glyph sets are model slots 0/1/2, already
  imported.
- **F1344's `0x3016`/`0x3022` read one field.** F1747 fills the StrVars it
  passes with only `vars+0xc` = object+0x124, and both codes expand that field
  (the ship's registration); `0x3022` additionally wraps at 35 columns via
  F1398, which needs the `DATA_007648` width table. No other game state is
  read by the codes the models use.
- **Not ported:** F1747's per-character glyph placement (the glyph programs lie
  outside the font model's 10-byte top-level extent and are not imported), so
  text records are reported in the viewer status rather than drawn. The
  mechanism read from the assembly is recorded in `docs/notes/text.md`.

Cache v13 was rebuilt and inspected; the previous v12 cache is retained at
`.cache/data-v12-backup.bin`. Tests: 204 runnable synthetic tests on Windows
(+1 embedded scan test, +1 viewer label test), clippy clean.

## 2026-10-22 — Console view clicks and flickering plumes (v0.47)

- **Console view buttons now take clicks.** `main.rs` scaled minifb's
  `get_mouse_pos` (already divided by the creation scale, 2) by the client
  size from `get_size` (not divided), so clicks landed at half their position
  and never reached the icons; it also ignored resizing and the aspect-ratio
  letterbox. `Screen::logical_point` maps client pixels
  (`get_unscaled_mouse_pos`) through minifb's own AspectRatioStretch
  placement to the 320x200 canvas, then `view_button` applies the original hot
  areas as before. Test: initial size, resized, wide and tall windows.
- **Thruster plumes flicker.** The plume's far end is a jittered vertex
  (opcode 7, F1476), but every render started from `JITTER_SEED`, so the plume
  was frozen. `AnimationInputs::jitter` now seeds the vertex cache and
  `Frame::jitter` returns the state after the frame. Flight chains one stream
  through the drawn objects, advancing once per 50 Hz flight step (a paused
  picture holds still); the F6 viewer advances it with its clock. The stream
  stays presentation-only (KNOWN_ISSUES, F1476); how far the original's
  stream moves per frame depends on its other readers, which are not modelled.
- **Viewer engines.** Nothing set the viewer's thrust, and test inputs 5/9 put
  bit 15 in global 15, which the ships' preamble treats as reverse thrust, so
  glows could not be seen in the viewer. E now steps the engines off / 2048 /
  4095.
- Checked: every ship's forward thrust limit (cache v12) is 1635 or more, so
  full throttle always shows a plume; reverse limits are stored negative,
  giving a negative global 15 and no glow, as in the original.

`docs/STATUS.md` was refreshed and its "Next task" list replaced by the v0.48
milestone plan ("Every model fully drawn").

## 2026-10-22 — Engine glows, moving parts and the spinning station

Two parallel tasks, merged with the ships/views work; details in
`docs/notes/thrusters.md` and `docs/notes/animation.md`.

- **Thruster glows (opcode 9, F1738)**, `model_thruster`: nothing, a line (width
  <= 4) or one/two glow quads (core 0x123, shaft 0x65) keyed by vertex A, with
  six colour slots fading the material in sevenths; B is resolved only when A
  is deeper than 0x200. Thrust is **global 15** (object word +0xba, the forward
  thrust); most ship programs skip glows at or below zero and lengthen the plume
  up to 4095. `verify-thrusters.py`: 12,288 direct cases, 0 mismatched
  (`5a8eddfe…`); whole-table oracles carry the thruster events. The earlier
  "screen position" question does not reproduce. The viewer now passes the model
  scale as +0x148, as F1683 does.
- **Animation inputs**, `model_animation`: globals are the object's own words
  (global 0 = `GameTics >> 10` written by F1534 before drawing; global 1 the
  gear/door word, F865 moving it by half the frame's tic delta); top-level
  locals are 0 apart from the shades. **Station rotation** (F866/F1675) spins a
  station about its docking axis at `(tics << (speed - 1)) >> 16`; model 74 is
  generated with speed 10 (about 169 s per turn). `verify-animation.py`: 2,640
  rotation and 4,096 orbital-update cases, and the real F1537/F1680/F1683
  set-ups on synthetic objects, 0 mismatched (`10ed4039…`, `2ab63f4d…`).

In flight the station spins at its original rate and animates its time-driven
parts; in the external view the player's ship is drawn with the original inputs
over flight time and its engines glow with the commanded thrust (full thrust =
4095, a diagnostic mapping until ship thrust limits are modelled; `Flight::thrust`
is saved). The F6 viewer animates over the original clock: C clock, 0/5/9 test
input, G gear/door, T time rate (x1..x1000, diagnostic). All 29 oracles pass.

## 2026-10-22 — Ships and the original's views

The scenarios now fly a real ship. **Cache v11** imports the original model name
table (`DATA_004682`, RVA 0xd1444, 209 entries, identical to JJFFE); slots
14..=70 are the craft, and `ffe_rs::ships::ShipList` keeps those with a real
name and a drawable model (entries 0..110 match JJFFE; from 111 floppy 1.02
inserts ten digit entries and differs in decal texts, which `verify-assets.py`
reports as a data difference). Page Up/Down changes ship; the ship is saved
(`Flight::ship`, older saves load the Eagle long range fighter, slot 20).

Views follow the original (research in `docs/notes/views.md`): F1 cycles front,
rear, external (turret views skipped, as for a ship with <= 2 gun mounts);
F6/F7/F9 select directly; the console shows the original view icons
(secondary bitmaps 63..81) with the current one lit, and their hot areas take
mouse clicks. The external camera keeps the original's yaw/pitch/distance
units, start values, held-key ramp, opposite-key resets and zoom limits, and
FUNC_001054's two rotations of the ship's axes. In the external view the
player's ship is drawn (at the station's model scale: true relative size), the
crosshair hidden, and the arrows move the camera, so Modern steering uses the
Classic keys there. Docking returns to the front view. ffe-rs's own keys moved
off the original's: help H, model viewer M, load Shift+F5. Flight drawing now
goes through `camera::Camera` (one projection for stars, station and ships).
Not ported: turret/missile/combat views, eye offsets. Seven tests added. All 26
local oracles pass (`verify-all.py`; on Linux, link the reference binaries to
the `.exe` names it expects).

## 2026-10-22 — Thruster glow analysis (not shipped)

Opcode 9's queueing was read out of F1738/F1704 and attempted, then withdrawn
rather than shipped unverified: the differential check exposed differences in
the port's screen positions for the vertices involved, and no oracle could yet
say the glow arithmetic was right. The full reading - the three queue paths,
the entry layouts, the six-step colour fade, why only 7 of 466 slots reach the
opcode in the synthetic scenes, and the hook/event-number recipe - is in
`docs/notes/thrusters.md`. The operand event remains as v0.23 left it.

## 2026-10-22 — The last census blockers: zero normals and the return record (v0.43)

Two small pieces of the original's own behaviour were still rejected by the
port, and both held three models back.

**A declared normal count of zero.** F1480 writes the implicit pair 0 and 1
from the shades and computes the rest lazily, so a model that declares no
normals needs no records at all; the importer now accepts a count of zero the
way it already accepted a count of two with a null pointer. Models 0, 1 and 2
execute.

**The return record.** F1754 is the return: it clears the dispatcher's flag
whatever the material bits hold, so *any* word whose opcode is zero ends the
program. The decoder accepted only the zero word, and its search for the
program's end (the blocks that carry data after the terminator) looked for the
same. Both now use the opcode.

**Census 446 -> 449 of 466**; `verify-station-models.py` 445 -> **448 models
compared, 0 mismatched**; `verify-child-models.py` 437 -> **440 models compared,
0 mismatched**, with only five left that the port does not execute: 89 (the
release data differs from the JJFFE declarations), 94 (the original's own
undefined child state), 148 (a child scale of 32) and 229/319 (top-level
external references, which the original resolves through null pointers). The
station-queue, texture and vertex-program traces are unchanged.

The local cache was rebuilt again: 1776 assets, 462 of 466 models. The F6
viewer now draws **449 of the 466 models** (424 of 456 in v0.41).

**The assets oracle was stale and is repaired.** `verify-assets.py` still
expected the v6 cache (385 assets, two models), so it had not run since v7.
It now checks the whole v10 cache: every payload against the executable's own
bytes, the descriptor and table pointer provenance for every model, the
bitmaps, fonts, palette, cabins and effects, the repeated build and the
corruption rejection. It also reads the ship object table slot by slot (the
null entry at 149 is four zero bytes, not a pointer, which shifted every later
slot).

That repair exposed a difference worth recording: **the JJFFE 2.8a7 source's
model data is not the floppy 1.02 release's data**. 34 of the 462 imported
models differ from the declarations by a few bytes each (model 361's vertices
differ, as v0.29 already recorded). The port imports the *player's
executable*, which is the original game's own data; the local reference build
embeds the mod's data, so the differential oracles can only compare the models
whose bytes agree - and they do: the whole-table station and child oracles
still report no mismatch, because the differing bytes are in the trailing
data and in fields the compared events do not carry. The oracle now reports the
differing model slots instead of failing on them.

## 2026-10-22 — Screen-space midpoint vertices and the remaining external references (v0.42)

`model::ObjectCache` now runs the original's two remaining vertex opcodes.
**3/4** are F1478's case 11 and **13/14** its case 6: a *screen-space* midpoint
of two vertices. The original resolves both sources through **F1472** (the object
path), which writes no screen position, so the midpoint averages what its
sources already had - zero in a fresh arena, the projected position when a
drawing command projected the source first. It writes no object coordinates
either, and keeps the nearer of the two depths; case 6 also keeps the first
source's view x/y, while case 11 leaves them as the arena holds them. The port
mirrors all of that: the object stays zero, the screen and the view come from
the sources' entries, and `model_instance::Vertices::resolve` does not project
these vertices again. `VertexContext` gained the instance position because the
sources' own evaluation needs it.

**Census 443 -> 446 of 466** (models 315, 316 and 444 now execute) and the
child oracle **417 -> 437 models compared, 0 mismatched** (models 346-360,
448, 450 and 452 now run with their children). The three models that still do
not execute are unchanged: 0/1/2 (a declared normal count of zero), 89 (the
release data differs from the JJFFE declarations), 94 (the original's own
undefined child state), 148 (a child scale of 32), 229/319 (top-level external
references) and the 15 slots the original itself cannot run standalone.

Two smaller decoder changes came with it: the thruster (F1738), beam (F1761)
and chain (F1766) records read their vertex bytes sign-extended, so 0x80..0xff
are external references there too, exactly as for faces, lines, points, text
and child calls. Model 316's chain uses them.

Two oracle changes: the child oracle no longer compares the child's base
handedness (+0xf8), which the original leaves as stack memory for a child whose
own handedness is undefined - it is reported apart as event 26 - and the
vertex-program oracle now compares every vertex entry's screen words and view
dwords, not only the object coordinates, so the new opcodes are checked against
the original rather than merely accepted. Traces: station-models
`34c1bf26…` (445 compared), child-models `cbe87dd9…` (437 compared),
vertex-programs `1afe0c45…` (454 compared), station-queue `35e55a26…`,
textures `f3164e1c…`.

The child trace's SHA-256 is **not stable across reference rebuilds**: it
carries event 26, the original's uninitialised base handedness, which the
comparison deliberately excludes. Every measurement of the child oracle here
should be read by its numbers (models compared, mismatches), not that hash;
`cbe87dd9…` was one build, `3d334db8…` another, both 440 compared and 0
mismatched.

The local cache was rebuilt: 1767 assets, 459 of 466 models (the previous
v10 file is kept at `.cache/data-v10-backup.bin`). The cache format did not
change, so the version stays v10; `build-data` now imports three more models.

## 2026-10-22 — Child models in the viewer and flight (v0.41)

`execute_scene` (v0.38) now drives the F6 viewer and the flight station, so
child models (station rings, lattice frames, ship parts) are drawn. **Cache
v10** adds `base:model/descriptors`: per table slot, descriptor +0x18 (child
scale offset) and +0x1c (F1687 cull radius), zero for slots not imported. The
viewer's `ChildTable` serves child calls from the cache; the environment holds
the original shades and the top-level light (length 0x8000, in the frame the
children are set up in).

Child primitives land in the root render order but index the child's own vertex
cache, so emissions now carry what drawing needs: `Emission::screen` (the
points F1708/F1712 projected when queuing) and `Emission::shade` (the face
normal's colour in its own instance's table); `PointEmission::screen` and
`LineEmission::screen` are F1479 projections of the resolved views, the line's
second vertex resolved on a copy of the cache so the original cache state is
unchanged (and a failure there leaves the line undrawn, never failing the
execution). All oracles are unchanged (station-models 443 / 0, child-models 417
/ 0, station, station-queue, textures).

If a child cannot run yet (vertex opcode 0xd in 315/444, explicitly undefined
child state), the viewer draws the parent without children and says so. 424 of
456 models render; the rest are child-only parts or planet-sized objects.
Station 74 is now the full station (drum, side pods, ring segments: 69 shapes);
the drum stays scaled to the old 40-unit size. Two tests added.

## 2026-10-22 — Original lighting and colours in the viewer (v0.40)

The viewer's shades had been a diagnostic 14..0 ramp, about double the
original, which washed faces towards white. The original's inputs are now
imported: F960 sets the default light `DATA_007231..7233` ((-18919, -18919,
18919), length 0x8000) and copies the default shade table `DATA_007234`
(7,7,7 7,7,7 6,6,6 5,5,5 4,4,4 3,3,3 2,2,2 1,1,1) to `DATA_008831`; F1535 swaps
in one of six star tables from `DATA_007759`; F1683/F1684 and F1740 copy the
eight shades into every instance and child. **Cache v9** adds
`base:lighting/default` (RVA 0xeec60) and `base:lighting/star-shades` (RVA
0x101858), byte-identical to JJFFE in floppy 1.02 (`model_lighting` parses
them; a CD cache omits them).

The viewer and flight station use those shades, a light of length 0x8000 as
F1518 normalises it (fixed in flight space; view-relative in F6), and every
flat colour goes through F1326 (`model_lighting::pack`: `r << 8 + g << 4 + b`,
no clamp) and the original fixed colour map / low palette, the route textured
faces already used. Still diagnostic: which star table applies (F1535's star
type), the light's direction relative to a real star, and the runtime upper
palette for colours outside `DATA_007770`. Oracles unchanged on the v9 cache.
Two tests added; version-rejection tests move to 10.

## 2026-10-22 — The real station in the flight view (v0.39)

The flight view draws the station as the original model 74 instead of the
placeholder wireframe cube: `ModelViewer::render_scene` runs the same verified
pipeline as F6 (executor, normals, textured faces, polygons) with any camera
frame and position, and `Screen::draw_station` places it with the flight
camera. The model's bounding radius is scaled to the old 40-unit placeholder,
so docking and safety distances are unchanged. Model 74's docking port is at
its +z end (an octagonal cap with the slot; the -z cap carries windows; four
axis-facing side panels use unlit black `0x2000`), so it is turned 180 degrees
about y to face the approach from -z. The flight projection now uses the
original F1479 focal length (x * 256 / z) for stars, crosshair and station
alike, instead of the scaffold's 150. Without a drawable model 74 the
placeholder remains. Camera, light and colour mapping are the viewer's
diagnostic choices; flight mechanics remain synthetic. One test added.

## 2026-10-22 — Fix: cache build with child-only models

After the child-model merge the importer accepts programs that read parent
vertices, but the cache's payload check still evaluated every vertex
standalone, so `build-data` failed ("External vertex -1 needs a parent
instance"). The merge had been verified against a cache built before it.
`model::validate_program` now evaluates each vertex with parent slots -1..-5
standing in at the origin and skips a vertex that needs another parent slot
(e.g. -6); every other error remains. The v8 cache now holds **456 models**
(1,749 assets). The F6 viewer draws 422 and shows a message for 34 child-only
models. Oracles re-run on the freshly built cache: station-models 443 / 0,
child-models 417 / 0, textures 1,736 views / 0 mismatched; traces unchanged
apart from the texture count. The release game was launched under Xvfb with
and without `--root` (30 frames each).

## 2026-10-22 — Polygons, textures and child models, in parallel (v0.38)

Three tasks ran in parallel worktrees and were merged here; their full
findings, formats and routine numbers are in `docs/notes/polygons.md`,
`docs/notes/textures.md` and `docs/notes/child-models.md`.

- **Complex polygons (opcode 5).** `model_polygon` decodes the six segment
  kinds and runs the F1721 handlers: projections, near-plane clipping (F1564),
  circles as two curves (F1554-F1557; F1467's `DATA_007977` is exactly
  isqrt(i << 21), computed by `model_math::table_root`), abandonment, and the
  F1795/F1796/F1797 queue entries. `model_polygon::edges` reproduces the curve
  subdivision (F1547/F1549). `verify-polygon.py`: 12,328 helper calls match
  (trace `fab32ea5…`). Station-model mode now sets +0x154 (normal records) as
  F1774 does; 452 fills / 1,601 entries match. The textured fill colour
  (F1691 reads past a small table) is not ported or compared.
- **Textured faces.** F1708/F1712 take the textured path under exact
  conditions; the 148-record descriptor table `DATA_007811` gives the bitmap
  id (primary, then secondary from 96), quad orientation and up to 7 colours;
  texels are colour slots; mapping is affine from corner tables. **Cache v8**
  adds `base:texture/descriptors` and `base:palette/fixed-map` (`DATA_007770`)
  from floppy 1.02 (a CD cache omits them). `verify-textures.py`: 4,096 raster
  cases match the whole buffer (`640cc781…`); 2,677 textured queue entries in
  1,720 model views match word for word (`65922b3c…`). The upper runtime
  palette and real lighting values remain unknown.
- **Child models (opcodes 14/27).** `execute_scene` runs children recursively
  (F1687 cull, F1750 instance, F1740 frame, F1774 arena with silent no-room
  skip, generation 2), with external vertices (−k from the parent pointers at
  +0x6c) accepted where the original sign-extends. Undefined child state is an
  explicit error. `verify-child-models.py` (`--child-model`): 417 models, 161
  with children, 1,239 child runs, 0 mismatched.

Merge fix-ups: both new oracles had used nested probe events 19-22; polygon
events are now 30-33 and their encoding is shared (`tests/common`), so the
child oracle also checks polygons inside children (trace `eaee2ad1…`). Child
caches get their model's normal records. `verify-station-models.py`: 443
compared, 0 mismatched (`b4c3aecf…`); station, station-queue, text/submodel and
vertex-program traces unchanged. Census **443 of 466**. 202 `#[test]`, 29
ignored, one Unix-only (172 runnable on Windows). The F6 viewer fills polygons
and draws textured faces with the real bitmaps; it does not run children yet.

## 2026-10-21 — Cache v7 and the F6 model viewer (v0.37)

The first visible use of the ported model executor. **Cache v7** imports the
vertices, commands and normals of every model table slot whose three blocks
decode: 439 of 466 on floppy 1.02 (1696 assets). Stations 74/75 must decode;
other failures are skipped. `ffe-content` accepts model assets for any slot
below 466 from v7, and only 74/75 before.

**F6** opens `ModelViewer` (game paused): Page Up/Down through the models,
arrows to turn, +/- to zoom, 0/5/9 test input. Each frame runs `execute_live`
with a camera frame (also the basis), full-detail visibility (depth 1), normals
from `model_normals::evaluate` with the camera and a light in object space, and
draws `Execution::ordered` in order: faces filled (scanline, even-odd) in
`resolve_colour` (now public) colours, lines, points, and curves as a cubic
through their four entry points (F1811 not ported). Polygons and labels are
counted as not drawn. A first pass measures the drawn extent and the second
fits it on screen.

Diagnostic choices, not the faithful renderer: floating-point camera, light
direction, nibble-to-RGB mapping, textured faces as lit grey, dark blue
backdrop. 420 of the 439 models render; 18 planet/star-sized objects exceed the
32-bit view space and one stops on an executor error. Models that only call
child models show nothing yet. Oracles are unchanged with the v7 cache
(`verify-station-models.py` 434 compared, 0 mismatched). One test added;
version-rejection tests move from 7 to 8. 178 `#[test]`, 25 ignored, one
Unix-only (152 runnable on Windows).

## 2026-10-21 — Opcode 15 sound requests (v0.36)

F1759 (opcode 15, two bytes) sums the absolute instance position components
(+0x4..+0xc), divides unsigned by 0x20000 and, below 63, calls F1908 with sample
`code >> 5 & 15` and volume 63 minus that distance. It changes no model state.
`SoundEmission` records the request; playback is not ported. The probe hooks
F1908 inside F1759 (nested event 18). Model 250's request (sample 0, volume 63)
matches; census **439 of 466**; 434 compared, 0 mismatched (trace `4bcf902e…`,
Linux LF). Opcode 16 replaces 15 as the unsupported example in the transactional
test. One synthetic test added; 177 `#[test]`, 25 ignored, one Unix-only (151
runnable on Windows).

## 2026-10-21 — Opcode 30 light points with no points (v0.35)

F1769 (opcode 30 `0x801e`) loops over the count in global word 4 (+0xa4 of the
+0x14c block). Each point index i takes entry byte `+0xa6 + 2i` (low nibble) to
select DATA_007820/7821/7828 materials for an F1699 point at vertex 2i, an
optional F1703 line 2i..2i+1 (global word 1 bit 5), and an F1764 label (string
0x8400, pick id from global dword 2 with i in its top byte) when vertex 0's view
z is at most DATA_007822[b]. Those tables are not imported, so a non-zero count
is an explicit error; a zero count returns at once, as the original does.
Census: **438 of 466** execute; 433 compared, 0 mismatched (trace unchanged).

## 2026-10-21 — Opcode 18 screen labels (v0.34)

F1763 (opcode 18, four bytes) calls F1764 with vertex `code >> 5`, the word at
+2 and a zero pick id. F1764 projects the vertex through F1471 (generation
check) and, when the screen point lies within x 2..=317 and y 4..=149 (unsigned
range tests), queues a 4-word entry for F1812, which expands the word as a
string (F1344) and writes it at that point. It then compares the point with the
pointer when `DATA_008949` bit 1 is set and stores the pick id; that test is not
modelled. `Label`/`LabelEmission`/`Primitive::Label` carry it.

The probe hooks F1777/F1778 inside F1764 (nested event 17: key and the screen
dword; queue event 6). F1764 is also the light-point routine's emitter (opcode
30 `0x801e`). Census: **437 of 466** execute; `verify-station-models.py`
compares 432 with 0 mismatched (trace `a80247eb…`, Linux LF). Only model 66
queues a label in this setup (others are off-screen); it matches. Model 89 now
stops at a truncated command: its floppy 1.02 program differs from the JJFFE
declaration from +0x12 and runs past the JJFFE extent, like model 361's vertex
data. Station, station-queue and text/submodel traces are unchanged. One
synthetic test added; 176 `#[test]`, 25 ignored, one Unix-only (150 runnable on
Windows).

## 2026-10-21 — Vertex-defined basis, opcode 28 `0xc000` (v0.33)

F1741's `0xc000` form is six bytes: vertices A, B, C at bytes +2..+4, each read
as an object position through F1472 behind a non-zero-stamp check
(`Vertices::object`). It writes the basis rows z = norm(B − A), then
x = norm((C − A) × z), then y = z × x (unnormalised), all with F1521 products.
norm divides each component's magnitude by F1466(v) + 1 through F1523 and
restores the sign.

Three helpers are ported literally into `model_math`: F1502 `square_root`, a
bit-by-bit root that is approximate (≈ sqrt(x)·2^15.5) and returns 0xffff for
zero; F1466 `length`, which shifts the absolute components by 30 − msb, sums
their Q31 squares / 4, roots and shifts back, and never returns less than the
largest component; F1523 `fraction`, unsigned (a << 31) / b, saturating to
`u32::MAX` when a / 2 >= b. F1467 (opcode 25's table root) is a different
routine and is still not imported.

Census: **435 of 466** execute; `verify-station-models.py` compares 430 with 0
mismatched (trace unchanged, `6ffe0c7b…`). Among them, models 351, 353, 356, 357
and 359 finish with non-axis-aligned bases (e.g. `[-1970604551, 0, -853274219]`
for row x of 351) that match exactly. Two synthetic tests added; 175 `#[test]`,
25 ignored, one Unix-only (149 runnable on Windows).

## 2026-10-21 — Offset curve vertices 0x17/0x18 (v0.32)

F1478 sends vertex opcodes 0x17/0x18 to F1477 with the *unsigned byte 2* as the
vertex index and F1474(bytes 3 | 4 << 8) as an extra parameter. F1477 reads the
record of that index (pair `byte2 >> 1`, mirror bit `byte2 & 1`), not the
current one: a 0x13/0x14 record gives the line P0 + 2·m(P1 − P0, t), any other
record the cubic of v0.29 through its bytes 3, 4, 4 + 2 and 3 + 2, with
t = (variable(its byte 2) + extra) << 14. The current vertex's own mirror bit is
not used. `ObjectCache::curve` now serves 0x13-0x18; validation follows the
selected record's control points.

`verify-vertex-programs.py` (minimum now 452): **452 of 452** comparable vertex
programs match; trace unchanged (`b3ed1b7d…`). The seven models that needed it
now stop at opcode 28 (`0xc000` form, 12 models). Census unchanged at 423 of 466;
`verify-station-models.py` still 418 compared, 0 mismatched. One synthetic test
added; 173 `#[test]`, 25 ignored, one Unix-only (147 runnable on Windows).

## 2026-10-21 — Opcode 22 curves (v0.31)

F1737 (opcode 22) is ten bytes: material word, four vertex bytes (+2..+5) and a
normal byte (+6). When the normal is not culled it resolves the colour through
F1692/F1693/F1691 with that normal's colour, projects +5, +4, +3 and +2 through
F1471 (cache-checked, so the cache is stamped even when nothing is queued), and
only if all four have view z >= 0x40 allocates a 10-word entry (draw routine
F1811) keyed by F1777 of +4's view, holding the screen points of +4, +2, +5 and
+3 and the F1326 palette colour. `Spline`/`SplineEmission`/`Primitive::Spline`
carry it; F1811's drawing is not ported.

The probe hooks F1777, F1778 and F1326 inside F1737 and records the key, the
packed screen points and the colour as nested events 14-16 (queue event 5);
`verify-station-models.py` compares them. Over the table the original queues 21
curve entries in 5 models; all match. Census: **423 of 466** execute; 418
compared, 0 mismatched (trace `6ffe0c7b…`, Linux LF). The station, station
queue, text/submodel and vertex-program traces are unchanged. One synthetic test
added; 172 `#[test]`, 25 ignored, one Unix-only (146 runnable on Windows).

## 2026-10-21 — Opcode 30 vertex-frame restore (v0.30)

F1767 (opcode 30, two bytes) has three forms. With bit 15 set it calls F1768 with
`(i16)code >> 4`: `0x801e` draws the object's light points (F1769, through the
point/line emitters and F1764), and every other value copies the current basis
(+0x9c) over the vertex frame (+0x14), nine dwords. Without bit 15 it advances
the vertex record base (+0x150) by 6·n and the vertex cache base by 72·n, where
n = F1473(byte `code >> 5`). Nine models use `0xc01e` (restore), one uses
`0x801e`; no executed path reaches the offset form.

The restore is ported; already cached vertices stay cached, as in the original.
The light-point and offset forms are explicit errors (context unchanged).
Census: **413 of 466** execute; `verify-station-models.py` compares 408 models
with 0 mismatches (trace `1c46a942…`, Linux LF). One synthetic test added; 171
`#[test]`, 25 ignored, one Unix-only (145 runnable on Windows).

## 2026-10-21 — Curve vertices and a whole-table vertex oracle (v0.29)

Vertex opcodes 0x15/0x16 (F1478 -> F1477 with a zero extra parameter) compute a
cubic point from P0 = vertex(byte 3), P1 = vertex(byte 4), P2 = vertex(byte 4 +
2) and P3 = vertex(byte 3 + 2), each XOR the mirror bit, with
t = variable(byte 2) << 14 and m(a, b) = (a·b) >> 31, all wrapping:

P0 + 2·m(3P1 − 3P0, t) + 4·m(3P2 − 6P1 + 4P0, t²) + 8·m(P3 − 3P2 + 3P1 − 2P0, t³)

The `4P0` in the t² term is what the original computes (a textbook Bezier has
3P0); it is ported exactly. Control points are evaluated in that order (it
matters for jittered sources).

`--vertex-program N` evaluates every vertex of any slot in index order,
cache-checked like command callers, with the start-up F1476 seed; it also
reports a hash of the reference build's own vertex records.
`vertex_program_reference.rs` reads each slot from FIRSTENC.EXE and compares
every slot the native evaluator supports: **445 of 445 match** (coordinates and
final generator state). Model 361's vertex records differ between the JJFFE
declarations (z 64/69) and floppy 1.02 (56/64); it is reported as a data
difference, not compared. 10 slots crash the original in this setup.
`verify-vertex-programs.py` checks repeat, three tampered fields and an empty
trace; Linux LF trace `b3ed1b7d…`.

Census: **405 of 466** execute natively; all 400 that the original completes
match in command order. One synthetic test added; 170 `#[test]`, 25 ignored,
one Unix-only (144 runnable on Windows). Also added `build.bat` (debug and
release builds of `ffe-rs`/`ffe-tools`).

## 2026-10-21 — Opcodes 17, 24 and 25 (v0.28)

- **17 (F1760 -> F1761)**: 14 bytes; word +0 material, word +2 two vertices
  (low/high byte, each projected with a near-plane test), words +4..+10 passed
  to F1761, which only writes queue records. Decoded as `Beam`; drawing not
  ported.
- **24 (F1766)**: word +0 material, word +2 an F1474 size operand, then 2-byte
  vertex pairs up to and including the first pair holding 0x7f (both the drawn
  and skipped paths stop there). Decoded as `Chain`; drawing not ported.
- **25 (F1771)**: 2 bytes. With bit 15, F1770 resets the current basis to
  identity. Otherwise it takes two components of the instance vector at
  +0x38..+0x40 (axis 0: +0x3c/+0x40, 1: +0x40/+0x38, 2: +0x38/+0x3c; axis 3
  reads uninitialised registers), normalises them with F1467 (a 2,048-entry
  square-root table, `DATA_007977`) and F1523, and rotates the basis with F1744.
  `VertexContext::direction` holds the vector. A zero pair is a no-op
  (`DATA_007977[0]` is 0); a non-zero pair errors until the table is imported.

Census: **403 of 466** execute natively; all 398 that the original completes
match. One synthetic test added (168 `#[test]`, 24 ignored, one Unix-only; 143
runnable on Windows). Remaining blockers: vertex opcode 0x15 (13), opcode 30
(10), 22 (9), 18 (3), vertex 0xd (2), and a handful of validation errors.

## 2026-10-21 — Data after the final terminator (v0.27)

18 models failed with "lacks a final terminator" and 8 with "opcode 0": their
declaration blocks carry data after the program's last terminator (for example
model 58: a child call, its operand, the terminator, then `0x07d4 0x1f40`). The
decoder swept the whole extent and read that data as commands. `decode` now
tries the whole extent first and otherwise takes the longest prefix that ends
in a terminator and decodes strictly with every branch inside it; the rest is
`Program::trailing`. A zero word that is not a record boundary cannot satisfy
this (the record spanning it would be truncated), a branch into the data
rejects the prefix, and with no valid prefix the original error is reported.
Size/count bounds apply to the whole input first. The cache build is
byte-identical.

Census: **368 of 466** execute natively; all 367 the original completes match.
One synthetic test added (167 `#[test]`, 24 ignored, one Unix-only; 142
runnable on Windows). Top blockers: opcode 17 (16), 25 (15), vertex opcode
0x15 (13), 24 (10), 22 (9), 30 (5).

## 2026-10-21 — Nested depth blocks (opcode 21, v0.26)

F1697 is a 2-byte command. Without bit 15 it resolves vertex `(code >> 5) & 0xff`
(F1470), keys it (F1777) and calls F1781: close the open group (F1780(0,0)),
queue a node with that key in the current list (callback F1790), and push the
node's own list as current (stack `DATA_009254`, depth `DATA_007831`, current
head `DATA_007832`). With bit 15, F1782 closes the group, pops back to the
parent list and records where the block ended. When drawn, F1790 sorts the
block's list with F1788 and continues after it.

`model_order::Order` now holds a stack of lists; `Order::nested(Some(key))` and
`nested(None)` mirror F1781/F1782, and `finish_with_keys` sorts each block's
members inside its node. A block left open at the end is an error, as the
original cannot draw it. `execute_live` handles opcode 21 (`Execution.blocks`,
`QueueEvent::Nested`); `execute` still rejects it.

The probe records F1781/F1782 calls inside F1697 (nested events 12/13, queue
kind 4) and passes them through. Census: **317 of 466** execute natively; all
316 the original completes match, including every block key. The sort within a
block is the verified F1788 order but nested composition itself has not been
compared with an executed original queue yet. Two synthetic tests added (166
`#[test]`, 24 ignored, one Unix-only; 141 runnable on Windows). Earlier traces
are unchanged.

## 2026-10-21 — Curved surfaces (opcode 31, v0.25)

F1874 resolves an origin vertex (byte +2, F1470 view) and three axis vertices
(bytes +3..+5, F1472 object), then passes word +0, words +6/+8/+10 and a
sub-stream starting at +12 to F1876, the curved-surface/planet renderer
(1,580 lines). F1876 advances the stream only through F1823, which walks
16-byte entries while their first word is non-zero and returns past the zero
word; every F1876 exit follows that parse. So the record is
**2 + 12 + 16·n + 2 bytes**.

Rust decodes `Surface` (entries raw) and emits `SurfaceEmission` in command
order after resolving the same vertices; patch rendering stays with F1876 and
out of `ordered`. Census: **312 of 466** execute natively; all 311 the original
completes match. One synthetic test added (164 `#[test]`, 24 ignored, one
Unix-only; 139 runnable on Windows). Top blockers: opcode 21 (33), 25 (15),
vertex opcode 0x15 (13), 24 (12), 17 (11), missing terminator (11), 22 (8).

## 2026-10-21 — Coloured child-model calls (opcode 27, v0.24)

F1748 is a second child-model call into F1750. Operands: byte +0 position
vertex (F1471), word +0 frame, byte +3 scale adjustment — high nibble as a
signed literal, or local variable `byte >> 4` when bit 3 is set — and word +4 a
child material resolved through F1694 (opcode 14 passes the instance colour and
a zero adjustment instead). The four optional extra vertices (byte 1 bit 7)
follow at +6, so the record is 8 or 12 bytes. `Submodel` gains `material` and
`scale` (`ChildScale::{Literal, Local}`); opcodes 14 and 27 share execution.

Oracle fix: station modes forced child culling with a negative descriptor
radius, which a negative scale adjustment shifts to zero, so some opcode 27
children ran their (hooked) frame/run steps. F1750's F1687 cull call is now
hooked: it always culls in station modes and passes through to the original
elsewhere. Earlier traces are unchanged.

Census: **277 of 466** models execute natively; all 276 that the original also
completes in this setup match. One synthetic test added (163 `#[test]`, 24
ignored, one Unix-only; 138 runnable on Windows). Top blockers: opcode 31 (39),
21 (33), vertex opcode 0x15 (13), 24 (12), 17/25 (11), missing terminator (11).

## 2026-10-21 — Thrusters (opcode 9, v0.23)

F1738 records are always 8 bytes: word +0 material, bytes +2/+3 vertices A and
B, word +4 a signed width. Every exit returns past the record. When drawn, F1738
projects A (F1471) and gives up if A's view z is at most 0x200; otherwise it
projects B with the same test, scales the width by `+0x148 - 6` and A's depth
(F1666), and either queues a line (F1704, width at most 4) or one or two glow
quads keyed by F1777 of A's view, depending on the screen length (F1467).

Rust decodes `Thruster` and emits `ThrusterEmission` in command order,
performing the same projections (B only when A is deeper than 0x200). Glow and
line queueing need F1666/F1467/F1704 and stay out of `ordered`, like text and
child calls. Census: **245 of 466** models execute natively and all 245 match
the original in command order. One synthetic test added (162 `#[test]`, 24
ignored, one Unix-only; 137 runnable on Windows).

## 2026-10-21 — Jittered vertices (vertex opcodes 7/8, v0.22)

F1478 dispatches vertex opcodes in pairs through `DATA_000044`/`DATA_000045`
(0-2, 3/4, 5/6, 7/8, 9/10, 11/12, 13/14, 15/16, 17/18, 19/20, 21/22, 23/24;
21/22 and anything above 0x18 go to F1477). Opcodes 7/8 evaluate the whole pair
at once: base vertex byte 4, then three F1476 draws each arithmetically shifted
right by byte 2 (offsets ox, oy, oz), then the mirror (byte 4 XOR 1). Vertex 0
is base + (ox, oy, oz); vertex 1 is mirror + (oz, oy, ox) — the offsets are
applied in swapped order, exactly as the original does.

F1476: a = s0; s0 += s1; s1 = rotate-left-16(a); return s1, with state
`DATA_008818`/`DATA_008820` seeded 0x12345678/0x456789ab at start-up. **Open
question:** that state is also read by other game code, so in the original,
drawing a jittered vertex can influence non-rendering code. The port keeps the
state inside the vertex cache (`Vertices::jitter`/`set_jitter`,
`model::evaluate_with_jitter`), owned by the caller and never shared with the
gameplay RNG; whether any gameplay path depends on it must be checked before
the flight simulation is ported (see KNOWN_ISSUES.md).

Oracle note: F1472 has no cache check, so calling it once per index (as
`--models` does) re-evaluates a jittered pair for its second half with fresh
draws. Real callers check the cache first; `--jitter-vertices` now does the
same. It evaluates every model with opcode 7/8 records with two seeds and
compares coordinates and the final generator state: 118 evaluations (59
models) match, 3 models are skipped for other vertex forms, 2 slots crash the
original in this setup. `verify-jitter.py`: repeat, three tamper fields and an
empty trace rejected; Linux LF trace `0fc5445f…`. `--station-model` now seeds
F1476 explicitly, as the game's start-up would.

Census: 232 of 466 execute natively and all 232 match the original (most of
the 59 newly decodable vertex programs stop at command opcodes). Top blockers:
opcode 9 (49), 31 (39), 27 (38), vertex opcode 0x15 (13), 24/25 (11), 17 (10),
21 (9), 22 (8). One synthetic test added, one updated. 161 `#[test]`, 24
ignored, one Unix-only (136 runnable on Windows).

## 2026-10-21 — Complex polygons (opcode 5, v0.21)

F1721 layout: word +0 material; byte +2 low 7 bits normal, bit 7 a flag; byte
+3 the segment length L; then L bytes of outline segments. The record is
**6 + L** bytes whether or not the polygon is drawn: a culled normal returns
past the segments at once. Otherwise F1721 keys the polygon by F1777 of the
view (F1470) of the first segment byte's vertex (low 7 bits), allocates one
queue record, and runs the segments through `DATA_007817` (seven entries,
indexed by `word >> 9`: 0 terminator, then F1734, F1724, F1730, F1733, F1723,
F1736). The segment handlers only build outline state in the instance and the
queue; they do not touch the parent program's variables.

`ffe-core` decodes `Polygon` (segments kept raw) and `execute_live` emits
`PolygonEmission` with its key (or `None` when culled), queues it as
`Primitive::Polygon` and logs `QueueEvent::Polygon`. Segment execution, and the
vertex projections it performs, are not ported yet.

The probe now records the F1777 key call inside F1721 (event 11) and its F1778
allocation (queue kind 3). The whole-table comparison therefore checks every
polygon key in command order. Because segment handlers project vertices in the
original, the probe's cache-miss vertex events (1, 7) can no longer be
predicted without segment execution; `station_models_reference.rs` compares
all other nested events (the station 74/75 test, which has no polygons, still
checks them exactly).

Census: **229 of 466** models now execute natively (was 180), and all 229 match
the original. Remaining top blockers: vertex opcode 0x7 (59), opcodes 31 (39)
and 27 (35), vertex opcode 0x15 (13), opcodes 17/24/25 (10 each), 21/22 (8
each). One synthetic test added; the unsupported-opcode example moved to 15.
159 `#[test]`, 23 ignored, one Unix-only (135 runnable on Windows). Earlier
probe traces are unchanged.

## 2026-10-21 — Whole-table comparison with the original (v0.20)

`--station-model N` runs any model slot through the original F1772 as
`--station` does, with consistent synthetic inputs: identity frame and basis,
position z 2000 (also the visibility depth), the descriptor scale at +0x148
(which the original uses as vertex scale, operand scale and visibility scale
alike), every normal visible, glyphs and children recorded but not executed.
The probe's face, point and nested-event capacities were raised; earlier traces
are unchanged. One process per slot: the original crashes in this synthetic
setup for 13 slots (15, 153, 179, 198, 199, 227, 259, 270, 316, 335, 369, 388,
458); they are excluded, not counted as failures.

`station_models_reference.rs` reads model bytes straight from FIRSTENC.EXE (via
`ffe-convert`, now a dev-dependency of `ffe-rs`) and compares every slot the
native executor completes. **All 180 match** the original: face count and
the stream of face arguments with locals/basis at emission, points/lines, text
and child calls, final locals, basis, colour and handedness.

Two probe details had to be modelled in the expected events, not the port: F1749
calls F1471 and F1747 calls F1470 only on a vertex-cache miss, so the probe's
vertex events appear only for vertices no earlier child call has projected (in
the probe build faces are hooked and do not project; F1470 never stamps).
Both station tests now apply that rule.

`verify-station-models.py --game <dir>` selects the table RVA by executable
hash, captures all slots twice, compares, and rejects four tampered fields and
an empty trace. Trace SHA-256 (Linux LF)
`3e04ba2e481427b2f2300736bbef8464064bdb4db75f87676ab5e963892cb153`.

158 `#[test]`, 23 ignored, one Unix-only (134 runnable on Windows).

## 2026-10-21 — Model census across the whole table (v0.19)

The original model table (`DATA_004681`, "Ship object table") has **466 slots**,
not 465: slot 149 is declared as four zero bytes (a null entry), which a
reference-only parse skips, shifting every later index. The floppy 1.02
executable's table matches the JJFFE declarations slot for slot once that is
read as raw 4-byte slots: every non-null descriptor's vertex count, normal count
and scale agree.

`tools/reference/model-extents.py` generates `ffe-convert::model_extents`
(committed): each slot's physical command-program extent (programs have no
length field) and its declared (vertices, normals, scale). These are layout
numbers only; stations still give 516 and 758 bytes. `model_command_bytes`
imports any slot's raw program and refuses an extent when the executable's
descriptor differs from the declaration; `decode_model_commands` adds the
decoder check. Normal programs with only the implicit pair (count 2) may have a
null pointer, as the original leaves it; they now import as empty. The cache
build is byte-identical (SHA-256 prefix `173d9f93…`).

`ffe-tools model-census` reports, read-only, how far each slot gets through
native import, decode and `execute_live` (identity frame, position z 2000,
every normal visible; glyphs/children recorded only). On floppy 1.02:

| Blocker (floppy 1.02, first failing stage) | Models |
|---|---:|
| decode: opcode 5 (complex polygon) | 69 |
| vertices: vertex opcode 0x7 | 59 |
| decode: opcode 31 | 39 |
| decode: opcode 27 | 26 |
| vertices: vertex opcode 0x15 | 13 |
| decode: opcodes 17 / 24 / 25 | 10 each |
| decode: opcode 22 (spline) | 7 |
| decode: opcode 30 | 5 |
| decode: stream lacks a final terminator | 5 |
| other vertex-record errors | 10 |
| decode: opcode 21 (nested block) | 3 |
| normals: invalid count | 3 |
| decode: out-of-range vertex in face/line/submodel | 7 |
| decode: opcodes 0 (non-zero word), 4 (flags), 9, 18 | 7 |
| vertices: vertex opcode 0xd | 2 |
| null table entry (slot 149) | 1 |
| **Executed to completion** | **180 of 466** |

This is a coverage map, not verification: only stations 74/75 are compared
with the original. The CD 1.01 executable was not available in this session;
its table RVA differs (0x0d0cf8) and should be checked with the same census.

## 2026-10-21 — Face draw queue (v0.18)

The original queues faces inside its raster path. F1708 (triangle) and F1712
(quad) return without queueing for a culled normal; otherwise they project
every vertex through F1471 (order b, c, (d,) a) and then:

- **Textured path** (`DATA_008812 >= 0`, its default; material texture
  non-zero; every screen point within x -2000..2320 and y -2000..2158; screen
  perimeter |a-b|+|b-c|(+|c-d|)+|last-a| at least 6): always queued.
- **Flat path** otherwise (F1709 / F1712 tail): queued if any point is on
  screen (0 <= x < 320, 0 <= y < 158); if none is, F1713 ORs the F1715/F1716
  outcodes (1 left/above, 2 inside, 4 right/below, 8 near-plane sentinel) and
  `DATA_007818` rejects 1, 4 and 8, i.e. all left, all right (or all
  above/below) or all behind the near plane. Otherwise F1714 queues a clipped
  polygon.
- The key is always F1777 of vertex a's view (F1709, F1714 and both textured
  allocations).

`execute_live` now applies exactly this: culled faces and rejected faces stay in
`Execution.emissions` but not in `ordered`, face vertices are projected
(stamping the cache as F1471 does), and `InstanceContext::textures` mirrors
`DATA_008812`. `Execution.queue` logs every group change and allocation in
program order (excluding the top-level close).

`--station-queue` lets faces run through the real F1708/F1712 and logs every
F1778 call inside F1708..F1714 and every F1780 call inside F1696 before passing
it on. Six views per station (z 100/600, two offset views, a 45-degree tilted
frame, a view partly off the left edge and one almost entirely behind the near
plane): `verify-station-queue.py` matches all 12 queues with repeat, 7 tamper
fields and an empty trace rejected. Trace SHA-256
`35e55a26689eb1e520dffa6b4c36f4a25dff36066d1b4c9ba969c7345425fb56` (CRLF);
Linux LF `350b6d2c…`. Earlier probe traces are unchanged. A first comparison
found Rust queueing all-behind faces: the 9th entry of `DATA_007818` (code 8)
also rejects, which had been misread.

Because sorting these operations is the verified F1788 order (v0.9), Rust's
`ordered` faces now match the original draw order for these views. Points and
lines are still keyed by their first view vertex and queued unconditionally;
their originals (F1700 discs, F1704) are not yet ported. One synthetic test
added; the composed-order oracle test now supplies a visible normal; the local
station-completion test now uses position z 100. 156 `#[test]`, 22 ignored,
one Unix-only (133 runnable on Windows).

## 2026-10-21 — Whole-station command-order oracle (v0.17)

`--station` runs the real model 74/75 command streams (from the reference
binary's own model table) through the original F1772 in the instrumented
adapter. Depth commands, vertex resolution (F1470/F1471/F1478), colour,
visibility and transforms run as original code; the face/point/line emitters
are recorded with the locals and current basis at each emission (as in
`--execution`), and text/child calls are recorded with their vertices resolved
through the real F1470/F1471. In this mode text reports no glyph-buffer room
after F1745 and every child culls (negative descriptor radius), so neither
glyphs nor children execute.

Inputs are consistent between both sides: identity frame (+0x14) and basis
(+0x9c), position (0, 0, 100) — +0xc is both position z and the visibility
depth — generation 2, zero scales/globals/locals, every normal visible with
zero colour. (The older local station check used depth 100 with position z 0,
which the original cannot represent; it stays as a completion check only.)

`station_reference.rs` matches, for both models: face count and the FNV-1a
stream of every face's arguments with locals and basis at emission (20 and 69
faces), all five points per model with flags, every text and child-call event
in command order, final locals, basis, colour (model 75 ends at 2,2,2) and
handedness. `verify-station.py` passes with repeat, 10 tamper fields and an
empty trace rejected. Trace SHA-256
`0ea8f4a7b9df2616dc6537a032b8527ab3ca0fde89e63856c260d23392f722b2` (CRLF);
Linux LF `b9e3c831…`. The probe's face cap rose from 64 to 256; earlier traces
are unchanged.

This is the first end-to-end evidence for real models, replacing the
helper-composition-only evidence for command flow. Not covered: render order
(the original's face queueing happens inside the hooked F1708/F1712 raster
path), glyph and child execution, primitive colour, clipping and pixels.

Also fixed a pre-existing Linux-only flake: the `ffe-tools` CLI test that runs
a freshly copied binary could hit ETXTBSY when another test thread forked; it
now retries only that error. 154 `#[test]`, 21 ignored, one Unix-only (132
runnable on Windows).

## 2026-10-21 — Correction: vertex frame vs current basis

`VertexContext::matrix` had stood for two different instance fields. Vertex
evaluation (F1478 via F1671) multiplies by the instance frame at **+0x14**;
the rotation commands (F1742/F1744, opcode 28) change the current basis at
**+0x9c**, which child-model and text frames (F1740) read and vertex
evaluation never does. Each earlier oracle exercised only one side (model
vertices set +0x14; execution/rotation-operand oracles fingerprinted +0x9c), so
they all passed while `execute`/`execute_live` would have moved a model's own
vertices after a rotation command.

Fix: `matrix` is now documented as +0x14 (vertex evaluation, and the source of
opcode 28's base forms); a new `basis` field holds +0x9c (rotation target,
default identity). The v0.16 `base` field is folded into `matrix`. Rotation
commands and F1739 now write `basis`; vertex evaluation still reads `matrix`.
The execution, rotation-operand, base-transform, model-vertex and live-instance
comparisons were rerun against the original and pass unchanged; their trace
hashes are unaffected (the captures did not change). Real stations still
complete with the same primitive counts.

## 2026-10-21 — Base-relative transforms; stations run to completion (v0.16)

**Opcode 28 base forms.** F1741 has three forms by `code & 0xc000`: `0x8000`
local-basis rotation (v0.6), `0xc000` vertex-defined basis (6 bytes, still
unsupported and rejected by the decoder), and `0x0000`/`0x4000`, which both
stations use (`0x0c3c` at `0x1da` in 74, `0x06bc` at `0x2ec` in 75). The base
form calls F1739 then, only for a non-zero operand word, F1743 (F1474 angle,
axis `code >> 5 & 3`). F1739 copies the instance frame (+0x14) into the
current basis (+0x9c): code bits 7..9 permute the rows (0/6 identity, 1..5 the
other five; 7 is undefined in the original and errors here), bits 12/11/10
negate source rows 0/1/2, and the handedness word (+0xf8 in, +0xfc out) is
complemented for permutations 3..5 and once per negation.
`VertexContext` gains `base_handedness` and `handedness` (and, after the
correction above, `basis`).

`--base-transform` calls the original F1741 directly for 6,048 cases (two base
frames, three handedness seeds, both base forms, permutations 0..6, all
negation masks, three axes, operands 0/0x1234/0xffff). `verify-base-transform.py`
passes with repeat, 8 tamper fields and an empty trace rejected. Trace SHA-256
`c9a87a6d023bacf3900dc0587285ac7522abc6353a80b9b6b62db43088c4a29c` (CRLF);
Linux LF `03070a9b…`.

**Open group at return.** Both stations then returned with a depth group
open, which `Order::finish` rejects. The original flush (F1788) crashes on an
open group, so that rejection stays. What closes it: the top-level object-draw
routines F1683/F1684 call F1780(0,0) right after the model run (F1773/F1774);
nested child and glyph runs do not. `execute_live` is the top-level entry, so
it now closes a group left open at return before finishing. (A probe flushing
the order-v1 sequences without the close segfaulted; it was removed.)

**Result.** With the existing synthetic station inputs (identity basis, all
normals visible, position zero), both station programs now run to completion:
model 74 executes 77 commands (20 faces, 5 points, 4 texts, 5 submodel calls;
25 ordered primitives), model 75 executes 100 (69 faces, 5 points, 1 text, 2
submodel calls, 2 colour commands; 74 ordered). The local station test now
requires completion and descending render keys. This is not yet compared with
the original running the same station; that whole-program trace is next.

One synthetic test updated for the top-level close and one for the base forms;
153 `#[test]`, 20 ignored, one Unix-only (132 runnable on Windows). Every
earlier reference trace hash is unchanged. F4, synthetic flight and cache
v6/385 are unchanged.

## 2026-10-21 — Colour command (v0.15)

Opcode 26 (F1762) sets colour state. Layout: word +0 material, byte +2 low 7 bits
normal index (lighting source), bit 7 target, byte +3 F1473 selector. When byte
+3 is non-zero the record is **20 bytes, not 6**: the low 3 bits of the selected
variable pick word +0 (k = 0) or one of seven alternatives at +2+2k. The v0.4
decoder's fixed 6 was wrong for that form; it is corrected.

Colour = F1692 nibble RGB (zero when textured), plus the instance colour
(+0xf4..+0xf6) when the material's 0x1000 bit is set, plus the normal's colour
unless the material is unlit (0x2000) or textured. Byte additions wrap. Bit 7
of byte +2 writes the result to normal entry 0's colour through F1691; otherwise
it becomes the instance colour that text (F1693) and child-model calls (passed
as the child's colour) use.

`VertexContext` gains `colour` (instance +0xf4..+0xf6), committed
transactionally like locals. `Execution` gains `colours` (selected material,
target, RGB) and `normal_zero_colour`; a later command lit through normal 0 sees
that override. A textured write to normal 0 needs F1691's runtime texture colour
table (`DATA_007812`, zero in the static data) and errors explicitly.

`--colour` runs 7 synthetic programs through the original F1772 (32 cases:
both instance colours, selector locals 0..9, normal-0 write then read, byte
wrap). `verify-colour.py` passes with repeat, 9 tamper fields and an empty trace
rejected. Trace SHA-256 `7d23226568032c92b4f062d4553cee39f4c89f8e51b742e9f958fb48ddaaa4b4`
(CRLF); Linux LF `95383fff…`.

Model 75 now passes `0x001a`/`0x234`; both stations stop at an unsupported
opcode 28 transform form. Two synthetic tests added, one updated (opcode 5 is
the unsupported example); 152 `#[test]`, 19 ignored, one Unix-only (132
runnable on Windows). F4, synthetic flight and cache v6/385 are unchanged.

Not covered: colour application to faces/points/lines (their handlers'
F1692/F1693 use), textured normal-0 writes, palette mapping and pixels.

## 2026-10-21 — Linux reference build; text and submodel operands (v0.14)

**Linux oracle.** `tools/reference/build-linux.sh` builds the same local-only
adapter as 32-bit ELF (needs `nasm` and `gcc -m32`). The original
`jjffe-master/linux/lnxconv.c` converter writes a generated copy of `ffe.asm`
(underscore-free externs); `abs` is escaped there because current NASM treats it
as a keyword. Supplied sources stay unchanged. Every recorded trace hash
reproduces once LF output is hashed with CRLF endings: core `accbe92f…`, depth,
depth-commands, order, composed-order, visibility, normals, transforms,
rotation operands, projection, live vertices, commands `2c0c1a67…`, execution
`d2a76e9c…` and point-line `afa955be…`.

**Probe generation.** The command-probe copy is now written by one script,
`tools/reference/make-probe.py`, used by both `build-smoke.ps1` and
`build-linux.sh`. Besides the four whole-file emitter hooks, it redirects nested
calls only inside F1747 (text) and F1749/F1750 (submodel) to recorders that
return synthetic vertices/descriptors. The original handlers still decode their
operands and return their own cursor; the earlier probe traces are unchanged.

**Correction.** Opcode 14 is not a second text form. F1749 resolves byte 0
through F1471 and calls F1750 with model `code >> 5`; F1750 looks the model up
(F1538), culls it (F1687), and otherwise sets up a child frame (F1740) and runs
the child program (F1773/F1774). It is the child-model call. Opcode 10 (F1747)
is 3D text: each glyph is a model program run nested, after F1344 expands the
string. Both push and pop their own vertex/normal buffer frame (F1745/F1746), so
the parent program's state is unchanged apart from lazy vertex resolution.

Verified layouts (`--text-submodel`, 7 cases):

- Text, 10 bytes: +0 material word, +2 normal index (a culled normal skips the
  text), +3 high nibble glyph-model set (F1539) and low nibble stored but unread
  before glyph execution, +4 position vertex (F1470), word +4 frame (F1740),
  word +6 string ID (F1344).
- Submodel, 4 or 8 bytes: model `code >> 5`, +0 position vertex (F1471), word +0
  frame (F1740). Bit 7 of byte 1 adds bytes +2..+5, resolved in the order
  +4, +2, +3, +5 up to the first `0x7f`, and only for a visible child. The record
  is 8 bytes whether or not the child is culled.

`ffe-core` decodes these as `Text`/`Submodel` and `model_execute` emits
`TextEmission` (with the normal skip as `culled`) and `SubmodelEmission` in
command order. They are never in `Execution.ordered`: their nested primitives
come from glyph/child programs that are not executed. In `execute_live`, the
text position is resolved through F1470 unless culled and the submodel position
is projected through F1471; extra vertices wait for child culling.
`verify-text-submodel.py` passes with repeat, 11 tamper fields and an empty
trace rejected. Trace SHA-256 `5ba06002572b4395481a3651d84c588dbdec4285a210b8fe7e4b0d1a8041f73d`
(CRLF, Windows-equivalent); the Linux LF capture is `b095c38c…`.

Real stations now pass their v0.13 stop points: model 74 stops at an
unsupported opcode 28 transform form; model 75 at opcode 26 (`0x001a`/`0x234`).
Three synthetic tests added and one updated (opcode 26 replaces 10 as the
unsupported example); 149 `#[test]`, 18 ignored, one Unix-only (130 runnable on
Windows). F4, synthetic flight and cache v6/385 are unchanged.

Not covered: glyph and child execution, string expansion, child culling
(F1687 with the child radius), material/colour for text, nested primitives in
render order, and pixels.

## 2026-10-21 — Point and line primitive emission (v0.13)

`ffe-core::model_commands` now decodes opcode 1 (point source) and opcode 2
(line): point material, raw F1474 angle, flag bit and vertex; line material and
two vertices. Both validate their vertex references against the model count.

`ffe-core::model_execute` emits `PointEmission`/`LineEmission` in command order
and, in `execute_live`, allocates them into the same `model_order::Order` stream
as faces. `Execution.ordered` is now `Vec<Primitive>` (`Face`/`Point`/`Line`)
with aligned `ordered_keys`; `emissions` remains face-only command order.
Standalone points/lines use the F1777 key of their first view vertex; an active
depth group absorbs them.

The local command probe now patches F1699/F1703 in a generated assembly copy and
captures the original handler arguments. `verify-point-line.py` compares point
material/angle/vertex/flag and line material/vertices against
`jjffe-commands.exe`; repeated trace SHA-256
`afa955be148192eaf2be79fb0b818386eafbc8d008e67e3fc56c3b7dd3e758a3`, with all
six tamper fields and an empty trace rejected.

Three new synthetic tests cover decoder payloads/out-of-range references,
command-order point/line emission, and depth-group ordering with a standalone
point. Total: 127 runnable Windows tests (145 `#[test]`, 17 local-only ignored,
one Unix-only). The real station check is unchanged: models 74/75 still advance
past opcode 6 and stop at text (`0x000a`/`0xf4`, `0x1c6e`/`0x1c8`). F4, synthetic
flight and cache v6/385 are unchanged.

Point/line are operand emissions, not projected/clipped/rasterized primitives.
Text (10/14), nested/submodel (21/26), other transform forms (28) and colour
state remain unsupported; see docs/MODEL_COMMANDS.md.

## 2026-10-21 — Composed depth/order oracle (v0.12)

Added `--composed-order` to the local headless adapter. It drives the original
F1696 depth commands, F1777 face keys, F1778/F1780 allocation/group lifetime and
F1788 sorting with marker callbacks, then prints the ordered marker IDs and
effective keys. It does not run F1772 or the face handlers; it is a composition
of the original helpers, not a dispatcher/pixel oracle.

Rust `execute_live` now exposes `Execution.ordered_keys` (aligned with
`ordered`) and `model_order::Order::finish_with_keys`; group members inherit the
group key, standalone primitives keep their own. The synthetic program orders
group key 9, standalone key 9 (earlier allocation wins the tie), then group key 0:
C and Rust both produce order `[2,3,1]`, keys `[9,9,0]`.

`verify-order.py` now covers three scenarios: 11,700 depth-command cases
(`ab5da346…`), 128 group/sort sequences (`4ab5453e…`) and the composed-order
program (`ce0d1b4f…`), with repeat/tamper/empty-trace rejection. The full
workspace remains green; 141 `#[test]` total, 16 local-only ignored, 124
runnable on Windows.

Also recorded the complete F1772 handler table in `docs/MODEL_COMMANDS.md`.
Point/line/text/submodel semantics remain unimplemented: their payloads must be
captured from the original handlers before porting, not inferred from record
sizes.

## 2026-10-21 — Depth groups composed into the bounded executor (v0.11)

`ffe-core::model_execute` now composes the verified F1696 depth commands and
F1780/F1788 group ordering with the live view-space slots in
`ffe-core::model_instance`:

- `execute_live(bytes, &mut Vertices, &mut VertexContext, visibility, table,
  InstanceContext, budget)` resolves depth vertices lazily with F1470 (view only,
  no projection), computes F1777 keys with the instance key scale and operand
  scale, opens/closes F1780 groups, and pushes face emissions into
  `model_order::Order`. The vertex count comes from the live cache.
- Opcode 6 modes 0/4/5/6 close; 2 uses one vertex; 1/3 use signed list extrema;
  7 handles position, first (`-1`), origin+z and vertex+z. The physical decoder
  and executor share `model_depth::decode`.
- `Execution.emissions` stays in command order; the new `Execution.ordered` is
  the depth-sorted stream. Standalone faces key from their first emitted view
  vertex, matching F1708/F1712's F1777 call; an active group ignores member keys.
- The executor works on cloned vertex slots and a local queue, finalizes the
  queue, then commits context and slots. Out-of-range depth vertices, an open
  group at return and budget exhaustion leave all three unchanged.
- `execute` keeps the earlier no-live-vertex behaviour and still rejects opcode 6
  with the original message; `execute_live` is the composed path.

Eight new synthetic tests cover grouped/standalone ordering, max/min/offset/
position/first, empty/replaced groups, cache commit/reuse and transactional
failure. Total: 124 runnable Windows tests (140 `#[test]`, 15 local-only ignored,
one Unix-only). The local-only real-station check now advances past opcode 6 and
stops at the next unsupported command: model 74 at `0x000a`/`0xf4`, model 75 at
`0x1c6e`/`0x1c8`; context is unchanged on those errors. No new executed-JJFFE
trace was captured for the composition itself; the existing depth/order/executor
traces are unchanged. F4, synthetic flight and cache v6/385 are unchanged.

## 2026-10-21 — Original projection and live vertex/instance slots (v0.10)

Added `ffe-core::model_instance` with two bounded, pure-integer pieces:

- `project([x,y,z])` — original F1479 integer projection. Rejects `z < 64`;
  divides by `z >> 8` when `z > 65536` and `value << 8 / z` otherwise; adds the
  160/79 view centre; arithmetically shrinks x/y while their wrapped magnitude
  exceeds `0x1f00`; returns the 320x158 logical-viewport pair. Rejected points
  leave the caller's screen words untouched; callers that require a value use
  `NEAR_SENTINEL = [-32766, -32766]`.
- `Vertices` — bounded live view-space slots. `resolve` mirrors the original
  command cache check: a zero stamp re-evaluates the object program (F1478),
  translation (F1470) computes view but does not advance the stamp, and
  projection (F1471) stamps the slot with the caller's generation. A matching
  stamp short-circuits evaluation exactly as original command callers do.
  Generation 0 and scale > 31 error explicitly; no external, screen-derived or
  random vertex forms are evaluated. On failure all slots, including dependencies
  computed during the request, are preserved.

Verified against unmodified executed JJFFE via `tools/reference/verify-instance.py`:

- 2,197 projection cases; repeated trace SHA-256
  `c794945388b5bae086ceb6cb8ad615d17d954a85e2b200bd89367166c1890f70`.
- 8,192 live-vertex cases (128 seeds x 64 steps, matrices/scales/positions/
  generations, explicit clears); repeated trace SHA-256
  `6fe8a6470d42d884d11e54f1cc9db26107794b5220cbfbb051a6ebf826e422cc`.

Both tamper families and empty traces reject. Five new synthetic tests cover
projection boundary/shift/divisor/wrap cases and live-slot memoization/stamping/
atomic failure. Total is 116 runnable synthetic tests on Windows (131 `#[test]`
with 14 `#[ignore]` and one `#[cfg(unix)]`). No external/screen/random vertices,
submodels, clipping or pixels are validated. This is not yet composed into the
model executor: `model_execute` still rejects opcode 6 until the composition in
`docs/MODEL_EXECUTION_PLAN.md` lands. F4, synthetic flight and cache v6/385 are
unchanged; no owner playtest is required for this internal milestone.

## 2026-10-21 — Original depth commands and flat group ordering (v0.9)

Extended `ffe-core::model_depth` with F1696 command decoding and resolution from
explicit view-space points, position, key scale (+0x130) and operand scale
(+0x148). The physical model decoder now reuses this depth decoder, preserving
record boundaries with one implementation. Modes 0/4/5/6 close groups; the other
forms handle vertices, signed list min/max, position, fixed -1 key and wrapping
scaled z offsets. Operand pairs continue only while their high byte is 255.
Negative/external or unresolved point indices fail rather than access unmodeled
legacy memory. These points are not a substitute for original lazy cache state.

Added `ffe-core::model_order` for owned abstract primitive tokens. An active
group absorbs tokens and ignores their separate depth keys. Opening another
group closes the previous one, including empty groups. Finalization requires
explicit closure. Sorting uses descending unsigned keys and earlier allocation
on ties; members retain order. This differs intentionally from F1696's signed
list extrema. A 1..4096-operation development budget is transactional per queue
operation, not an emulation of the original 50,000-word allocator.

The unmodified original F1696 plus F1780 allocator matches 11,700 cases over
13 key scales, 10 operand shifts, three synthetic point sets, 15 command words
and two initial-group states. Check actual consumption, key, active state and
original allocation size. Repeated trace SHA-256
`ab5da34693bcb68d8df26253f7199be280fda09886106184ecf9d0e3739950d1`.
The original vertex cache is explicitly primed, so this comparison does not
validate lazy evaluation or external-instance vertices.

Original F1778/F1780/F1788/F1792/F1793 execute 128 synthetic 64-operation queue
sequences with marker callbacks, including all-equal-key cases. Native output
matches, including group contiguity, empty/replaced groups and tie ordering.
Repeated trace SHA-256
`4ab5453ea1fb9bf67daea15b42fcc11e05cfa36b603c9c625d174509cac1c813`.
The first marker adapter omitted the original primitive-completion flag and
crashed; reading the real primitive epilogues established state=1, after which
the original dispatcher and both repeated comparisons pass. No legacy sources
or additional assembly call sites were patched. Six altered-result families
and both empty traces reject; restoring the valid trace passes again.

Eight new synthetic tests cover all forms, signed-vs-unsigned behavior,
wrapping/masked offsets, malformed/truncated data, unresolved points, equal
keys, flat group lifetime and exact budget/rollback. Total: 111 synthetic Rust
tests. Strict formatting/clippy and release workspace builds pass. Prior
command/executor/core/startup hashes and fresh cache provenance/repeat/corruption
checks are unchanged. The native-window run initially encountered Windows
focus refusal during concurrent reference activity; run alone, the complete
Modern/Classic/held-input/lab/gameplay/save/relaunch regression passes.

This is standalone depth/group behavior, not game primitive storage, forced
headers, nested child-model lists or rasterization. The model executor still
stops explicitly at opcode 6 until lazy view-space point state and actual
primitive allocation are composed correctly. F4 and synthetic flight are
unchanged; cache v6/385 assets remains byte-identical. No additional owner
playtest is required for this internal milestone.

## 2026-10-21 — Modern default controls and original depth keys (v0.8)

The owner requested Modern W/S and arrows as the startup preset. That is now the
default on every launch, with the optional Classic-like preset still on F3.
Graphics defaults remain original-style; original mouse/keyboard fidelity is not
claimed. Help, README and the approved plan record the updated input decision.

Input now has an explicit release-to-rearm latch. Previously, keys pressed while
paused could remain armed on resume; clearing only on selected overlay toggles
was not sufficient. Disabled/modal/focus states block current keys, and explicit
modal/pause/preset transitions disarm them. A held key cannot be reintroduced by
a repeated press report until a release is observed. New keys remain usable.
Three synthetic regressions cover press/hold/release, disabled input, repeated
press reports and mode transitions.

The native-window test uses default W without first pressing F3, checks S and all
four arrow directions, verifies the optional Classic-like controls, and holds W
across both lab exit and pause resume. The initial modified test produced one
lab state mismatch before two clean repeats; per-field before/after diagnostics
were added. With the explicit latch, the expanded complete regression passes.
The negative-yaw assertion was corrected to the existing [0, 2pi) wrapping
convention rather than incorrectly expecting a negative stored yaw.
Trade/launch/thrust/dock/sell/save/reset/load, invalid-load rejection and
cross-process persistence remain covered.

Added `ffe-core::model_depth::key` for the original F1777 render-order key.
View-space integer x/y/z and instance scale are explicit inputs. Preserve
wrapping absolute magnitudes/addition, arithmetic magnitude shift, masked
logical scale shifts and the original packed overflow key, including equality
at its unsigned threshold. This is not Euclidean distance, a z-buffer,
depth-order command execution, group state or sorting.

The unmodified original routine matches 17,303 scale/point combinations on two
reference runs. Altered-key and empty traces reject. Trace SHA-256
`61fffc80501112118d10975309a52bab10dd8598bd14c26b4cd07ed39332655f`.
Two synthetic tests cover normal cases, nonpositive z, wrapping magnitudes,
shift masking and overflow threshold equality. The existing 900-context
composed command comparison still passes with its unchanged hash.

103 synthetic Rust tests, strict format/clippy, five hygiene tests and release
build pass. Fresh cache provenance/repeat/corruption and original RNG/timer/
new-game state comparisons pass unchanged. Cache remains v6/385 assets with
the previous hash. No supplied files or user snapshots were changed. F4 still
shows diagnostic catalogue views; flight remains synthetic. Next: model
depth-order group records and their state/lifetime before extending opcode 6.

## 2026-10-21 — Bounded composed model command execution (v0.7)

Added `ffe-core::model_execute`: follow physical instruction boundaries and actual
return/arithmetic/bit-branch/visibility-branch/local-basis-rotation control flow;
emit ordered triangle/quad operands (including reflected forms) with context
snapshots. Repeated loop emissions retain their individual state, unlike the
physical face catalogue. A real return commits state; any error discards the
invocation's output and leaves caller context unchanged.

Budget is 1..4096 commands, including the terminating instruction. Regression
checks prove the exact ten-fails/eleven-passes loop threshold, invalid budgets,
arithmetic failure, missing trig table, unknown reached operations, branch skips,
conditional returns, malformed boundaries and unmodeled aliased local selectors.
Legacy selectors 18..63 alias instance fields rather than independent locals;
the bounded executor rejects their use instead of inventing semantics.

The original F1772 dispatcher and original command handlers execute three newly
authored synthetic composed programs across 900 contexts. The separate reference
probe uses its existing four emission-call hooks, capturing operand order and
emission-time first-eight locals/matrix. Final locals/matrix and dispatcher-reset
state also match. Counted loops, forward branches, conditional returns, normal/
LOD predicates, rotations, sine/cosine math and mirrored faces all compose.
Repeat trace SHA-256
`d2a76e9caf8a94abf0ca1a06b3adcd6a785c24507bab258eca30ee05163c852b`.
All five altered result fields and empty traces fail; ordinary smoke mode rejects
the probe-only scenario.

Actual station streams are tested to stop at their first unsupported stateful
depth-order opcode 6 (74: 0xc; 75: 0x8), preserving the original input context.
These operations cannot be skipped to claim faithful station rendering.
Output is operand emissions, not transformed/cull-tested/shaded geometry.
Point/line/text, submodels, colour state, other transform forms, lazy instance
buffers and depth-order records remain next. No new F4 view or flight changes;
cache remains v6 and original files/snapshots stay untouched.

98 synthetic Rust tests, strict formatting/clippy and release build pass.
See the README executed-comparison command; reference traces are disposable.
The v0.7 native-window test also passes all four lab views and simulation/action/
snapshot isolation, then launch/thrust/dock/trade/save/reset/reload/relaunch.
Fresh cache import, repeat/corruption checks and prior RNG/timer/new-game state
checks pass; the root v6 cache hash and previous command/startup hashes are unchanged.

## 2026-10-21 — Exact integer trigonometry and basis rotation (v0.6)

Cache v6 adds the original 65,536-byte sine table (16,384 signed integer entries)
via verified executable DATA RVAs: floppy 0x106390, CD 0x105334. The first CD
validation rejected an incorrect font-relative RVA; unique full-table matching
established the actual location, then both source-profile validations passed.
No floating-point sine recreation or copied table in source/test fixtures.
385 assets/386 ZIP entries; older cache versions retain their existing scope.

Native Sin32 matches all 65,536 angle low words plus five wrapping cases.
F1744 basis rotation matches 135 cases over all axes and extreme/small matrices;
per-term Q31 rounding and source negation order are preserved. Both remaining
F1758 arithmetic operations match 210 cases. Repeated trace SHA-256
`b7ea38a86882aff69e3757c408387bf817d2da229ec6a332517cedc98fa53a58`.
Independent tampering in sine/rotation/arithmetic and empty traces all fail.
Rotation takes resolved angles, with an additional command helper described below.
Axis 3 explicitly errors rather than reproduce undefined legacy pointers.

Floppy v6 is 2,107,490 bytes, SHA-256
`173d9f932e65d46fa3a00426c273f9a845da386b931a0faba075fb85de92136f`.
CD v6 is 2,107,486 bytes, SHA-256
`5f18edada4436970ddf9b13f547c02cee47afb635030a630b8f43c1a89deafa5`.
Full reference bytes/provenance, repeat builds, corrupt cache rejection and
unchanged supplied source files pass for both releases.

Additional v0.6 work ports F1474 packed signed literals/global/local angle
operands and the pure 0x8000 transform form via F1742/F1743. All 65,536 encoded
words and 135 actual original handler matrix outcomes match; repeated trace
SHA-256 `08ce2914ec9f6a9654933b4948c49cd1b521d21de1487400d7cde2d66c1530c7`.
Tampering in either family and empty traces fails. The adapter treats local slot
62/global-pointer aliasing explicitly, so pointer addresses do not leak into
repeatable numeric traces. Native errors preserve the matrix. Base/object-relative
and vertex-defined transform forms are not silently approximated.

Further bounded work ports F1751/F1752/F1755 normal/LOD visibility predicates
and inverse branches. Explicit depth/scale/detail and evaluated-normal inputs
replace implicit pointers/globals. Preserve signed comparison, scale-add wrap,
masked shifts, threshold overflow round-trip, detail selector bit, conditional
return and forward/backward offsets. Missing normal IDs fail explicitly.
966 predicates and 5,796 branch outcomes match original routines; repeated
trace SHA-256
`d7e8cc0b57b0fa8f17e7823a23866b08ced025961df0bcc1c912b9fc05ae425c`.
Altered predicates/branches and empty traces fail. Shared branch-offset
handling preserves the previous 360 bit-branch results and command trace hash.
Synthetic evaluated normals isolate the predicate; generation remains covered
by the separate original station normal oracle.

94 synthetic Rust tests pass. Existing command, vertex, normal, font, bitmap,
startup and RNG/timer/new-game state hashes remain unchanged. Strict formatting,
clippy and release builds pass. The visible diagnostic views are unchanged from
v0.5; this is original-math preparation for command execution, not original
projection, full station rendering or faithful gameplay.
Actual native-window regression passes again with root v6, including all four
lab views and full launch/dock/trade/save/relaunch. Prior v5 is backed up privately;
supplied originals and existing user snapshots are unchanged.

## 2026-10-21 — Original station normal evaluation (v0.5)

Imported original normal records for models 74/75 via verified descriptor
pointers, retaining 48/258 bytes and 18/88 expanded entries. Cache v5 has
384 assets/385 ZIP entries; v1..v4 remain readable with their earlier lab views.
Importer and lab validate raw literal anchors, matching descriptors and face
normal bounds. Public tests use synthetic data only.

Native F1480/F1481 initialization, mirrored anchors/components, masked scale
shifts, wrapping observer/light dots and shade lookup match executed JJFFE:
504 contexts and 26,712 results, repeated trace SHA-256
`55b2060dfd8970cfc906c45f91944cbf06627e72c83f6bda417ef18aae1368f3`.
Altered hashes and empty traces are rejected.
Original vertex results and RNG/timer/state hashes remain unchanged.

Floppy/CD normal imports match all reference bytes and descriptor provenance;
deterministic repeat builds and corruption rejection pass. Floppy v5 cache is
2,041,663 bytes, SHA-256
`35838328430aaf673e53f0f96e3d33c3ceff670abb6fe57842211910300e31f6`.
CD v5 cache is 2,041,659 bytes, SHA-256
`9b218c4a7037e6195743eb71109fabc97fb6327cbb765c893adce41dd6704571`.
Root v5 activation atomically preserved the exact v4 archive privately.
Supplied releases/reference sources were not modified.

F4 Tab now cycles wire, face, points, then normal-filtered declared outlines.
The observer derives from the diagnostic lab camera; light and shade inputs
are test values. This still includes alternate command branches, not complete
station execution, projection or shaded pixels. Flight retains the synthetic
station. Private captures were inspected, then removed.

87 synthetic tests, strict clippy, formatting and release build pass.
Actual native-window tests pass all four lab views and isolation of simulation,
actions and snapshots, followed by launch/thrust/dock/trade/save/relaunch checks.
Next: original integer trig/transforms, bounded command execution and submodels.

## 2026-10-21 — Declared station surfaces and verified command helpers (v0.4)

- Continued the station drawing-command layer without replacing the flight
  cube with an incomplete model. F4 now has declared-face wire outlines,
  individual-face inspection (command offset/material/normal ID) and the old
  points view. Tab cycles views; comma/period steps faces; 1/2, arrows and
  0/5/9 retain their model/camera/test-input meanings.
- Model 74 has 96 physical records, 24 declared face emissions (4 triangles,
  20 quads); model 75 has 112 records, 71 faces (8 triangles, 63 quads).
  These include reflected emissions and alternate branches, not a list of
  primitives known to execute together. No invented hull/connectivity or captured
  mesh. Diagnostic white/green outlines are not original material shading.
- `ffe-core::model_commands` bounds every physical record, recognizes the actual
  root-stream forms, validates branch targets as record boundaries and bounds
  face vertex references. Unknown forms fail. Standalone F1758 arithmetic,
  F1753/1756/1757 bit-branch and F1692 material-descriptor helpers preserve
  wrapping, signed/unsigned comparisons, shift masking and 16-bit writes.
  Invalid lookup/division/unsupported integer trig fail without local mutation.
  This is not a complete command executor; normals, child programs, visibility,
  transforms/render ordering, actual animation and rasterization remain next.
- A separate local `jjffe-commands.exe` probes original triangle/quad handler
  arguments before emission. The build checks/replaces exactly two F1708 and
  two F1712 call sites only in an ignored generated assembly copy. Supplied
  source stays unchanged and ordinary smoke/font/vertex oracle assembly remains
  unmodified. The first comparison caught the triangle normal-byte placement
  (between B and C), which was corrected before validation.
- Executed-reference checks: 1,315 arithmetic cases, 360 bit branches,
  all 65,536 material descriptors, 32 face-record cases including reflection.
  Repeated traces match; corrupt math, branch, material and face results, and an
  empty trace all fail. Trace SHA-256:
  `2c0c1a67dd5cc297c54ebb8492bd98bcb123df4a19f56ae7d9c01c8177533635`.
  These certify helpers/operand order, not whole-station execution or pixels.
- Cache v4 adds `models/074.cmd` and `075.cmd` (516/758 bytes), stable IDs,
  descriptor and source offsets. Import follows original descriptor +0 pointers;
  fixed verified extents are deliberate because no program length is stored.
  Cache/import/lab share the decoder. There are 382 assets/383 ZIP entries.
  Tools retain v1..v3 compatibility; v3 keeps point inspection, v2 gameplay
  still runs and requests a lab-data upgrade. V4 command/vertex metadata must match.
- Floppy and CD imports match JJFFE declarations and original PE pointer chains;
  repeated builds are identical, corruption fails and source installations stay
  unchanged. CD verification extracts only required files into an auto-cleaned
  temporary directory, never alters the BIN/CUE or supplied installations.
  - Floppy v4: 2,040,600 bytes,
    `dc6de6b28642336cdceabc196f2cfc4aaba24d2d7335bf70825d641ea871dc35`.
  - CD v4: 2,040,596 bytes,
    `2782b831062ec53d6c43b5713d65f50d1866c9b70089c3a3e5b98f0ed91f3877`.
  Root cache activated with atomic replacement and exact v3 backup at
  `.cache/data-v3-backup.bin`; existing v1/v2 backups and user saves untouched.
- Existing original vertex/font/blit/startup/RNG/timer/state comparisons still
  pass with unchanged trace hashes. Strict Rust formatting/clippy/release
  builds and 82 synthetic tests pass; five repository-protection tests pass.
- Native-window validation exposed an automation issue: foreground ownership
  alone was not sufficient to prove minifb's target thread was active/focused.
  The driver now acquires and checks actual thread focus, reports denied focus
  explicitly, always detaches temporary thread-input attachments, and polls
  docking state rather than assuming uninterrupted wall time. The completed
  run passes all F4 modes and gameplay/save isolation, full launch/dock/trade,
  both control presets, pause/braking, invalid-load rejection and cross-process
  persistence. Private captures were inspected and removed.
- Next: complete bounded model execution with original integer trig,
  normals, visibility, transforms, render-order blocks and child models.
  Compare executed primitive streams and then pixels before replacing synthetic
  flight geometry. The faithful phase-3 gate remains open.

## 2026-10-21 — First original model interpreter slice and F4 vertex lab (v0.3)

- Owner asked to proceed with original model interpretation. Traced JJFFE's
  model descriptor/pointer table and F1472/F1477/F1521. This milestone implements
  the bounded vertex-program layer, not a completed station renderer. Keeping
  the main-view synthetic cube avoids inventing connectivity, skipping visibility
  branches or presenting an incomplete hull as the original station.
- Two original station record sets: model 74/0x4a (`DATA_002907`) and
  75/0x4b (`DATA_002912`), vertex records `DATA_002905`/`DATA_002910`.
  Expanded counts 54/88 include mirrored pairs, with 162/264 raw bytes and
  scales 13/14. PE model table RVA is 0x0d0cfc (floppy) / 0x0d0cf8 (CD);
  both actual executable pointer chains, descriptors and all record bytes
  verified against JJFFE, not inferred from bitmap/font layout deltas.
- Added pure `ffe-core::model`: signed literal coordinates, transformed mirror
  pairs, negation, midpoint, vector arithmetic and variable interpolation.
  Signed Q31 products truncate per term; additions/subtractions/shifts explicitly
  wrap. Nominal identity is 0x7fffffff and retains low-bit asymmetry. Midpoints
  wrap the sum then truncate towards zero. Variable selectors preserve separate
  literal/scaled/global-u16/local-i32 sources. Fresh evaluation memo tables avoid
  stale animated vertices; bounds, cycles, external references and unsupported
  screen-space/random/curve operations fail explicitly.
- Importer/cache share the same bounded validator. Cache v3 adds only
  `base:model-vertices/074` and `/075` (`models/074.vtx`, `075.vtx`), for
  380 assets/381 ZIP entries. V1/v2 stay readable; older schemas cannot contain
  the new kind. V2 still runs the game; F4 reports an explicit v3 upgrade error.
  Root cache was upgraded only after verification, preserving exact prior v2 at
  `.cache/data-v2-backup.bin` and the earlier v1 backup. No original game files
  or existing save snapshots were changed.
- F4 opens a separate paused point-cloud inspection view. 1/2 selects the record
  set; arrows rotate a diagnostic orthographic view; 0/5/9 supplies test values
  0/32768/65535 to global/local slots. White points are literal, green computed.
  This is not actual animation state, original projection, faces, shading,
  visibility or submodels. No original geometry is compiled into the executable.
  The next model milestone is drawing-command/normal/submodel import and
  executed-reference primitive emission before any gameplay replacement.
- Added local-only `--models` reference mode, exporting only context metadata and
  coordinate hashes. Two models x four matrices x three scales x eight variable
  seeds = 192 cases / 13,632 vertices, all matching Rust. Slot-specific values
  distinguish selector decoding; cases include extrema, wrapping scales and
  extrapolation. Traces repeat byte-identically and modified coordinates fail.
  `verify-models.py` also requires the expected ignored test to be discovered.
  Trace SHA-256:
  `7432278a3b1c032631f44668d5f43dcf2303adcb6b910ff7773a00ccc4bb2617`.
- Real-data v3 verification passes repeated cache equality, all payload/provenance
  comparisons, corruption/no-overwrite rejection and unchanged inputs, for both
  the floppy install and a disposable 48-file extraction of the actual CD ISO.
  Floppy cache: 2038563 bytes, SHA-256
  `d2ec47fb372f9668ade76fc5e10e586e8b08ad692598a556e18b807214210847`.
  CD cache: 2038559 bytes, SHA-256
  `9517947feba3b9ee9bfc0b2216ce8574564a9d4e3272155cfdcb188fccee1ce1`.
  ISO extraction is local test setup, not a shipped direct-ISO importer.
- Validation: 74 synthetic Rust tests, five repository-protection tests, strict
  clippy, formatting and release builds pass. Three local reference tests are
  deliberately ignored in public CI and pass explicitly. Previous 696 font/
  28 bitmap cases, RNG/timer/state vectors and 64-frame startup hash are unchanged.
  Native Windows test verifies F4 freezes simulation, blocks trade/save/load,
  does not catch up elapsed time, and requires a fresh thrust press after exit;
  then passes the full launch/dock/trade/save/reload loop and both input presets.
  Captures of both inspection views were examined locally. V1's required-font
  error and V2's working window/actionable F4 upgrade error were also checked.
  No remote CI run or complete model-renderer acceptance is claimed.

## 2026-10-20 — Original font/console presentation in playable v0.2

- Owner reported the v0.1 playable looked good while acknowledging its distance
  from the original; asked to continue. Recorded as prototype feedback, not a
  faithful-gameplay acceptance. V0.2 preserves flight, controls and save format.
- Inspected original model paths first: stations are drawing programs with
  computed/packed vertices, branches and submodel transforms, not static meshes.
  Replacing the cube faithfully needs an interpreter; no invented hull has been
  represented as original geometry. This bounded step instead replaces generic
  presentation with verified original font/console content.
- Main font: 116 records x 10 bytes, nine MSB-first row masks plus proportional
  advance. Identical in both exact original profiles and JJFFE
  DATA_007750/7751. Floppy RVA 0x1013b0/raw 0xeafb0; CD RVA 0x1003d0/raw 0xea3d0.
  Bounds-checked importer retains the source offset and bytes. No font/art bytes
  are compiled into code/tests; removed the `font8x8` dependency.
- New partial cache version 2 adds `base:font/main` (`fonts/main.fnt`), 378 assets/
  379 ZIP entries. Tools still read v1, but v1 cannot claim the new font asset
  schema; v0.2 game reports a required rebuild instead of a font substitution.
  The old generated root cache is retained privately as
  `.cache/data-v1-backup.bin`; verified v2 replaces only the generated cache.
- Original console primary slot 43 is blitted at y158, with clipping of its
  43rd source row to the 200-row canvas. This follows the observed source call,
  not a guessed panel resize. Fixed low RGB6 values expand by shift 2; cabin0
  keeps its separate 8-bit colours. Logical output is 320x200, nearest resampled
  to 640x480 for 4:3 correction and aspect-preserving window scaling. Diagnostic
  layout/scanner contact are still prototype UI, not original functional HUD.
  Help, scenario/reset controls and full-loop behavior are preserved.
- Local reference adapter adds bounded `--font` and `--blit` modes using executed
  JJFFE F1598/F1597/F1623, without source edits. All 116 glyphs at six vertical
  clip placements (696 cases), plus four opaque/masked images at seven positions
  (28 cases), match Rust framebuffer hashes/advances. Each capture repeats;
  deliberately altered font and bitmap hashes fail. The helper enforces both
  Rust comparison tests are discovered, not just a successful runner exit.
  Reference-only aliases handle MSVC C symbol decoration.
- Font trace SHA-256:
  `5642024bbc48ebda6affb820a2d54cde2bc0efbf53db278c152b7a69680486dc`.
  Bitmap trace SHA-256:
  `0cd9357e77769019b90c807505894202e68ecbb1a7b4d8d1c557ea5eb7e65489`.
  No pixel captures or raw glyphs are retained in public fixtures.
- All cache payloads and font bytes checked against floppy originals and JJFFE;
  repeated v2 output matches and corruption/no-overwrite tests pass. The actual
  CD's 48 inputs were extracted only into a disposable temporary installation
  and pass the same checks. Floppy v2: 2037328 bytes, SHA-256
  `a8475b311cc09c60d15e69330d4ba6b163e02ce9cb56c9e63a52bc183ffec70c`;
  CD v2: 2037324 bytes, SHA-256
  `a8929eb0907a7f7dd00c29dd054fb28d50c8828e0bdab9d44c372ff40a597962`.
  Original 71 game files are unchanged. Runtime upper palette remains unresolved.
- Full native-window test passes buy/launch/thrust/dock/sell/save/reset/load,
  Modern steering/brake/pause, invalid-load rejection and new-process reload.
  Original RNG/timing/state and 64-frame startup baselines remain unchanged.
  Synthetic tests cover bounds, masks, signed offsets, exact six-bit expansion,
  logical-to-output pixel mapping and rendering without gameplay mutation.
  No remote CI execution is claimed. Original models, faithful physics and
  economy, functional original console, audio and HD packs remain unfinished.

## 2026-10-20 — Experimental native Rust playable example

- Added a separate `ffe-rs` executable: real window/input, launch, inertial
  thrust/steering/braking, station approach/docking, one-item trade and native
  save/reload. F1 help, F2 named Docked/Approach/Free Flight reset scenarios,
  F3 Classic-like/Modern presets and pause/focus handling. Runs without any
  legacy executable, source tree, NASM or x86 runtime.
- This is explicitly a **prototype**, not faithful First Encounters or completion
  of the approved phase-3 gate. Flight is floating-point scaffolding, not the
  unported FFP/fixed-point physics. Station/stars/economy/HUD/font are synthetic.
  Original cached cabin0 is used as the docked backdrop; no game assets are
  bundled. `ffe-core` and its existing reference coverage are unchanged.
  Temporary differences are recorded as EXP-001 through EXP-003.
- minifb software presentation uses a 640x480 development canvas in an initially
  1280x960 resizable aspect-preserving window. Simulation uses 20 ms updates;
  presentation targets 60 Hz and caps per-frame catch-up to 100 ms. Help, pause,
  menus and focus loss stop simulation without catching up lost time. Held
  flight inputs are disarmed on focus loss to prevent latched thrust.
- Snapshot JSON has a distinct format/version, bounded 16 KiB reads, finite/
  range/docked-state validation, exact float round trips, unknown/duplicate-field
  rejection and exclusive-create numbered files. Prior files are never replaced.
  Failed/interrupted writes may leave an invalid newest snapshot; F9 reports
  this and retains live state, never silently falls back. No autosave, old-save
  import or final-save compatibility promise.
- Native unit tests cover the complete launched route, exact docking range/speed
  limits, alignment, steering, braking, speed cap, collision warnings, atomic
  trade errors, inventory/credits, persistence and corrupt-state rejection.
  Renderer tests use synthetic PNGs and assert presentation does not mutate state.
- `tools/test-playable.ps1` drives the actual Windows window with targeted key
  messages. Verified buy -> launch -> thrust -> coast/dock -> sell -> save ->
  reset/load -> Modern yaw/pitch/brake/pause -> bad-load rejection -> quit ->
  new process/reload. Inspected private window captures for help, station market,
  flight and visible load error; captures are not distributed. Temporary cache/
  saves and test processes are cleaned. The test helper's arrow messages needed
  extended scan-code flags, corrected before the final passing run.
- All 57 Windows Rust tests, formatting, strict clippy, release builds and five
  repository-protection tests pass. Existing combined asset/reference checks
  still pass; the legacy 64-frame smoke hash remains
  `d4439f95c9f2b0fca0edea7977edb76f21452772f8f228a1bb844dabae6a563d`.
  The original 71 input files remain unchanged. Remote CI is configured but not
  claimed executed. README provides a short owner test; owner acceptance of this
  new window is pending, distinct from the earlier technical PASS below.
- Still absent: original world/models, faithful physics/docking/economy, combat,
  planets/hyperspace, audio, legacy commander import, remapping/persisted options,
  automatic conversion, complete cache, HD image packs and Enhanced rendering.
  Next work replaces prototype scaffolding with executed-reference-tested ports,
  rather than treating it as verified original gameplay.

## 2026-10-20 — Owner confirmed both milestone checks pass

The owner ran `tools/test-milestones.ps1` and reported PASS for the content-cache
and reference/core checks. This confirms the combined technical test works in
the owner's run; it is not a flight, docking, trading or save/load playtest.
Next planned work remains object/state and fixed-point physics, targeted
executed-reference comparisons and launch/flight state traces.

## 2026-10-20 — Bounded new-game state traces and first native core primitives

- Extended the local reference adapter with original NASM symbol aliases,
  preincluded during assembly; supplied source files remain unchanged. The build
  manifest now fingerprints the aliases and records assembler commands, numeric
  and state scenario versions. Linked binary stays local and ignored.
- `--state 400` scripts Y then intro choice 1 after the input lockout. The actual
  JJFFE main loop reaches start type 1 at frame 161: day `0x121cf7`, fraction
  `0x80000000`, player state `0x30`, player index 107 and raw cash 1000.
  It samples selected scalar fields, RNG words and framebuffer/palette hashes,
  never raw pointers, personal saves or full game memory.
- Two independent disposable runs produce identical 400-row traces:
  SHA-256 `916e2d3c1abe847e89f9e38e1353c55c737e6ff032132e86a659935bd99c9e7b`.
  The old 64-frame smoke hash remains unchanged. Per-read synthetic time is
  explicitly not a real-time/fixed-timestep flight oracle.
- New pure `ffe-core` has no dependencies or legacy runtime. Implements the
  two-word RNG, unsigned wide timer multiply/divide with per-update truncation,
  wrapping system/accelerated counters, explicit division-overflow errors and
  wrapping 32-bit day-fraction carry. Error results leave counters unchanged.
- Numeric reference mode executes JJFFE F1530 and F1418 for 1536 RNG and 60 timer
  cases, including zero/edge seeds, pause, truncation and counter wrap. Rust
  matches all numeric outputs and 238 consecutive in-game clock transitions.
  Numeric JSONL SHA-256:
  `accbe92f957fc582a6b3f8600648d511d377503d55f6c140674073ee9bee0a84`.
- Corrected the historical inventory's reversed timer-counter roles and false
  49,710-ticks-per-day/one-real-second-per-day interpretation. The main game
  fraction carries on full 32-bit overflow; 994 is the normal 20 ms increment.
  This is a documentation correction from source/runtime evidence, not an
  intentional gameplay deviation.
- `verify-reference` enforces exact bounded scenario shape/coverage with explicit
  mismatches, no silent input defaults and a 2 MiB per-trace bound.
  `verify-core.py` runs both modes twice, delegates comparisons to Rust and
  deliberately changes an RNG result to verify failure. Temp files auto-clean.
  `tools/test-milestones.ps1` combines prior cache checks with the new reference/
  core milestone for the owner's short technical test; no window is expected.
- 45 synthetic Rust tests and five repository-protection tests pass; strict
  clippy, formatting and Windows x64 release build pass. Owner-facing combined
  script passes. Remote CI has not been run. Original game/source hashes remain
  intact; neither traces nor new proprietary fixtures are retained.
- Still absent: playable Rust game, complete physics/FFP/object state, faithful
  input/timing/clock-wrap policies, launch/fly/dock/trade/save scenarios, complete
  content, renderer and graphics-pack loading. The RNG call schedule is recorded
  but not fully ported. Next work is targeted numeric/object/physics differential
  tests and a flight/launch trace, not advanced rendering effects.

## 2026-10-20 — Verified embedded import and partial content cache

- Added original Rust PE32 DATA mapping and bounded bitmap-table decoding.
  Both exact-hash floppy 1.02/CD 1.01 layouts were independently validated
  against the JJFFE 2.8a7 declarations, not the rebuilt reference's addresses.
- Corrected the JJFFE-specific assumptions from the historical FFED3D inventory:
  96 primary/251 secondary slots, signed drawing offsets at header 4/6, mask-zero
  bit 0. There are 345 images plus two special 32768-byte zero-height texture
  blocks, not 347 ordinary images or a blanket 256/512-height encoding.
  Those blocks remain raw and excluded from inferred image dimensions.
- Imported the 128 fixed six-bit RGB colours. The upper half of the palette
  is assigned at runtime; no embedded RGBA/upscale previews guess its colours.
- Added `ffe-content`: bounded, deterministic stored-ZIP base-cache writer and
  reader with stable IDs, typed metadata, source provenance, payload SHA-256,
  CRC checks and explicit incomplete-content markers. Existing proposed full
  schemas remain future design; the actual v1 contract is in
  [FORMATS.md](FORMATS.md). Uses the Rust ZIP library with compression features
  disabled; no external unzip program is required.
- `build-data` imports 377 assets; `inspect-data` validates the local archive
  without extracting entries. New-only output, source containment guard, failed
  installation refusal and per-input rehashing reuse existing protections.
  A local ignored `data.bin` is generated; tools require no legacy source at runtime.
- `tools/reference/verify-assets.py` independently compares every full table
  record (including raw blocks), bitmap metadata, low palette, cabin PNG pixels/
  palettes and WAV samples with local originals/source. It verifies repeat
  equality, existing-output refusal, corruption rejection and source invariance.
  Floppy: 71 files unchanged; CD: 48 required files extracted temporarily from
  the supplied ISO and unchanged. Fixtures/caches are automatically cleaned.
- Both real caches contain 378 ZIP entries and are about 2.04 MB. Profile/source
  provenance and original executable offsets intentionally differ between them.
  No copyrighted payloads, source or test fixtures enter CI/repository artifacts.
- Windows synthetic suite: 34 Rust tests, including truncation, BSS/out-of-range
  pointers, aliases, zero-height special slots, path/ID/version/shape validation,
  missing/extra/reordered entries and payload damage. Remote CI remains unrun.
- Not done: gameplay/state traces, simulation, models, gameplay tables, remaining
  fonts, journal/music/CD conversion, startup cache freshness/rebuild, complete
  upscale export, pack loading or renderer. This milestone is content plumbing,
  not completion of the approved playable slice.

## 2026-10-20 — Local JJFFE smoke reference and external-asset decoders

- Built unmodified JJFFE 2.8a7 assembly/C with NASM 3.01 and MSVC x86,
  using a new original headless platform adapter. Scripts and hash manifest
  make the local build repeatable; legacy warnings are retained, not patched.
  Binary/objects remain ignored and must not be distributed.
- Two bounded 64-frame runs in disposable working directories produced identical
  JSONL traces: SHA-256
  `d4439f95c9f2b0fca0edea7977edb76f21452772f8f228a1bb844dabae6a563d`,
  with 49 distinct framebuffer hashes. This checks startup execution only:
  synthetic clock, scripted Y, no audio/video device/save support. It is not
  a deterministic gameplay oracle or proof of a playable Rust game.
- Added `ffe-convert`: bounded uncompressed 8-bit BMP decoding, lossless indexed
  and RGBA PNG, source 8-bit RGB palettes, PCM-to-WAV and UHD sizing.
  Real cabin files 1..4 have zero `bfSize`; support is explicit and tested,
  with row/palette/header validation unchanged.
- `export-external` requires a new destination outside the player's game folder.
  It writes six cockpit images, 23 effects, palette/indexed originals, previews,
  sizing CSV and a final manifest with all inspected source hashes/output hashes.
  The 44-file partial export is retained locally under ignored `extracted/`.
  It is not a full upscale export, base cache or installable graphics pack.
- Independently decoded real PNGs using Python chunk/CRC/zlib checks; every
  indexed row, RGB palette and RGBA pixel matched the original BMPs. Python
  `wave` verified all 23 WAV sample payloads unchanged, including the empty
  sentinel. UHD rows recommend 3520x1738 (11x), without resizing the source.
  Repeated export was byte-identical; no-overwrite behavior passed; all 71
  `game/` files were unchanged across validation.
- Windows synthetic Rust suite now has 23 tests; format, strict clippy and
  locked release build pass. Five repository-protection tests remain passing.
  Remote Windows/Linux CI is configured but not claimed as executed.
- Next: embedded data/image decoding, full cache provenance, gameplay state
  traces and simulation. Music, CD media, graphics switching and saves remain
  unimplemented. Current commands/limitations are in [README.md](../README.md).

## 2026-10-20 — First implemented Rust foundation

### Implemented

- Added a Cargo workspace, locked dependencies and unpublished `ffe-tools`
  package. Existing stable Rust was found at `%USERPROFILE%\.cargo\bin`;
  it was not necessary to install a toolchain. The prior PATH-only observation
  below is not a missing-toolchain blocker.
- `inspect-game --game <directory> [--json]` reads an installed copy without
  executing, patching or converting it. Default `game/` resolution is beside
  the executable, not shell cwd.
- Exact hash/size recognition for English floppy 1.02 and CD 1.01, independent
  of JJFFE simulation choice. Version-like strings alone cannot select a profile.
- Fingerprints 48 required files, compares cabin/journal hashes, checks expected
  sound/music file presence, even PCM lengths and HMP signatures, and counts
  optional AVI/BMP files. Audio is not fully decoded/hash-verified; optional
  video/still content is not decoded. All limitations appear in the report.
- Stable ordered JSON/text reports and explicit missing/mismatched/oversize/I/O/
  ambiguous-path diagnostics; unsupported versions do not use guessed offsets.
  Exit codes 0/1/2 distinguish inspection success, failed checks, and fatal
  argument/root/output errors.
- Configured Windows/Linux Rust CI jobs using synthetic inputs only.

### Verification completed locally

- Rust tests: 13 pass on Windows (2 profile tests and 11 synthetic/CLI tests).
  Another Unix-only ambiguity/symlink test is configured but not run locally.
- `cargo fmt --all -- --check`, strict all-target clippy and locked release build
  pass. Release executable PE machine is `0x8664` (Windows x64).
- Current `game/`: passed, 48 files; repeated JSON byte-identical. The additional
  floppy installation passed with an identical required-file manifest.
- Real required CD files were read from ISO9660 into an automatically cleaned
  local temporary directory; the inspector recognized CD 1.01. One real AVI/BMP
  pair verified optional media counts. This is a test fixture, not implementation
  of a disc-image importer.
- Temporary corruption/removal/spoofed-release cases failed with the expected
  diagnostic codes and exit status. Both patch package directories correctly
  failed as incomplete installations.
- All 71 original `game/` file hashes were identical before/after validation.
  No extracted assets/test fixture was retained or included in source.

### Still not implemented

No game, simulation, JJFFE reference build, converter/cache, embedded model/image
decoder, upscale exporter, renderer or save importer. This tool is the first
version/content inspection step, not completion of the approved playable slice.
Next work: reproducible JJFFE reference plus validated asset-layout decoding.
The latest runnable commands and limits are in [README.md](../README.md).

---

## 2026-10-20 — Owner decisions and new release inspection

This entry supersedes conflicting decisions/recommendations in the original
Phase 0 entry below. Current authority: [CLAUDE.MD](../CLAUDE.MD).

### Approved technical direction

- Use the supplied **JJFFE 2.8a7** tree for gameplay; FFED3D is rendering research,
  not the simulation oracle. Rebuild/instrument local copies as necessary.
- Windows x64 first; first playable outcome is launch/fly/dock/trade/save/reload.
- Match original visuals first; Classic first-run default. Live graphics switching
  is required, not a restart-based setting.
- Verified bug fixes may be enabled by default with regression tests and a
  deviation record. Do not import FFED3D's Aniso modifications wholesale.
- Player installs original files in `game/`; generate private `data.bin`.
  Private `upscaled.bin` overlays images only. Never distribute original files;
  source/derived-art redistribution and project license remain unresolved.
- Owner performs upscaling. Export all eligible images plus explicit per-asset
  3840 x 2160-profile guidance, manifest/CSV, contact sheet and exclusions.
  Do not give every texture the same output dimensions.
- Prioritize native resolution, HD images, filtering, MSAA, then modest bloom;
  meshes/gameplay mods/scripts and advanced effects are deferred.
- Legacy saves import one-way to a versioned native save; preserve originals.
  Offer Classic/Modern remappable controls independently of graphics.
- Start HMP-to-MIDI playback/synthesis. Pre-rendered music is acceptable if useful;
  MP3 requires synthesis first, and no converted music ships with the engine.
- Owner gets five short scenario-based playtests (about 15 minutes), not the full
  importer/numeric/version matrix. Future builds must supply exact running steps.

### Current evidence

- Added floppy installation: 69 files; its executable matches `game/` exactly
  (English 1.02), despite the unpatched description.
- CD input is a MODE1/2352 BIN/CUE image, volume `1STENC_US`, 713 files.
  Its executable identifies as English 1.01 and has a different hash/size.
  `DATA/` has 323 AVI clips and 323 BMPs. The six cabins, 23 effects,
  17 music tracks and journal match the current game files.
- Both new patches are installer/compressed-payload packages, not unpacked
  patched executables. Package labels 1.06/1.06d are not yet confirmed runtime
  versions. Inspect resulting executables before registering importer support.
- See [GAME_FILES.md](GAME_FILES.md) for exact hashes and limits. No new
  decompilation, extraction, installer execution or original-game launch occurred.

### Persistent changes and outstanding work

- Recorded all 18 answers in the plan and reconciled baseline, bug, pack,
  save, scope and distribution policies in related docs.
- Protected root reference/release folders and generated packs through `.gitignore`
  and CI. Added synthetic regression tests, including Windows path/case handling.
- No runnable Rust game/importer/exporter or JJFFE study build exists yet.
  Rust tooling was not found on PATH; this planning update did not install it.
  The next implementation work is the bounded JJFFE reference/importer foundation.
- No public redistribution approval, license choice, patch-output fingerprint,
  GPU minimum or delivery date was inferred from the owner's answers.

---

## 2026-10-20 — Phase 0 complete

### Snapshot

- Studied `legacy/ffed3d-master/` (called `legacy/ffed3d/` in the brief).
- Verified `legacy/Elite3/` and `game/` are byte-identical: 71 files, same SHA-256. Recorded in `docs/GAME_FILES.md`.
- `FIRSTENC.EXE` is a 32-bit PE, image base `0x00400000`, entry `0x00410000`, code section 429,568 raw bytes, data section 1,621,504 raw bytes (2,469,888 virtual). SHA-256 `d9195661…`.
- Executable self-identifies as **English Release 1.02 — 21/04/95**; the reference build is JJFFE 2.8a6 + Anisotropic mod 0.9c + FFED3D.
- `BLD_INFO.CFG` (byte−1 obfuscation) gives build dir `c:\games\firstenc\patches\eng-fp`, product `FIRST ENCOUNTERS`, disk `GAMETEK`, total 2052.00 KiB.
- `PATCHES.INI` is AIL/UltraSound patch configuration, not a First Encounters gameplay patch. `GAMETEK.1` is the installer's compressed executable payload; `IMAGE.DAT` is the installer manifest. Neither is read at runtime.
- No CD copy supplied; CD layout/hashes not guessed.

### Source study

- Parsed all 1,715 functions in `ffe.asm`: 323,169 bytes of function spans; 331 descriptive names; 142 `ANISO` markers and 26 `FIX:` comments.
- `ffedat.asm`: 4,486 labels / 1,203,054 bytes; `ffebmp.asm`: 4,323 labels / 1,314,768 bytes. Combined ≈ 2.52 MB, matching the original data virtual size.
- Main global state: one `0x556a`-byte block at `DATA_008804`, zeroed by `asmmain`; 116 object slots, 18 starports, 465 model slots, 33 goods, 64 equipment entries, 26 military missions, 60 packages, 6 contracts, 40 sample slots, 17 songs.
- Timing: `TimerAdvance` accumulates milliseconds; nominal 20 ms frame (50 fps configurable); 50 Hz input timer; 49,710 tics/day (`0xc22e`); 994 tics per 20 ms at 1×. Start `GameTics=0x80000000`, `GameDays=0x121cf7`.
- Numeric model: 16/32/64-bit integers, 1.31 matrices (`0x7fff8000` = 1.0), 16.16 values, 16-bit binary angles, FFP (16-bit mantissa + 16-bit exponent), and x87 in 170 instructions. `F1519_fp` is an x87 vector normaliser used by matrix orthogonalisation (FFED3D “TEST” change).
- Data formats: indexed bitmap header `u8 type, u8 height, u16 width, 4 zero bytes, pixels`; models are command streams, not triangle lists; `MISSION.DAT` is the journal text archive with a bit-permuted 4-byte encoding and FF-code text; saves are a compressed/scrambled subset (`0x984a` bytes plus selected globals); `FX/*.RAW` is 16-bit signed mono 22,050 Hz; music is HMP.

### Legacy build

- Original toolchain not installed (VS2010 + DirectX SDK Feb 2010). Built a **study binary** with VS 2026 (MSVC 14.51), NASM 3.02, Microsoft `Microsoft.DXSDK.D3DX` 9.29.952.8, Wine `d3d9.h`, and a DirectInput shim.
- Temporary compatibility patches in a scratch copy (never in `legacy/`): `#pragma pack(1)` around FFED3D structs, `round`→`xround`, `time`→`qtime`, `Dxerr.h` stub, DirectInput data formats/wrapper, splash bypass for observation.
- Result: 2.47 MB `ffed3d_test.exe` launches, displays the JJFFE/Aniso splash, then the Frontier copyright intro; render loop is live (window title updates). Not a supported build; not distributed.
- Could not exercise full gameplay, saves, audio or scripted input yet. Phase 1 harness will add fixed seed/timestep/input/trace.

### Decisions proposed (awaiting owner approval)

- **Simulation oracle:** treat the built FFED3D/JJFFE reference as frozen (matches Phase 1 traces); log DOS 1.02 differences. This is the only practical oracle with the supplied source. *Pending explicit approval.*
- **Classic look:** reproduce the original DOS software renderer, not FFED3D's D3D look. *Pending approval.*
- **Aspect:** original 4:3 with non-square pixels (DOS), not FFED3D's 16:10 `aspectfix`. *Pending approval.*
- **Palette expansion:** indexed reference; GPU display expansion rule to be chosen (`v<<2` vs full-range). *Pending approval.*
- **Content format:** RON for content, TOML for mod/config, JSON for manifests/schema; indexed PNG + palette sidecar; glTF GLB + semantic command-stream sidecar; WAV/MIDI. *Pending approval.*
- **Workspace:** `ffe-core`, `ffe-content`, `ffe-convert`, `ffe-render`, `ffe-platform`, `ffe-tools`, `ffe-rs`. *Pending approval.*
- **No scripting in v1**; propose Rhai later, leave `ScriptHost` extension point. *Pending approval.*

### Discrepancies / gotchas found

- FFED3D's software buffer is **640×400** (`DATA_009226` = 256,000 bytes; `F1601` stride 640; `F1882` blits 640×400), while the logical UI is 320×200. DOS is 320×200. Do not inherit the 640×400 buffer into Classic.
- FFED3D uses **1.6 aspect** (`curwidth/1.6`) and a 35°/2.0 projection; DOS was 320×200 on 4:3. Do not inherit 16:10 as Classic.
- FFED3D replaces the bitmap font with Arial (`D3DXFont`), the HUD panel with a 3D panel + 1024×1024 render target, and textures with linear filtering. None of these are Classic.
- FFED3D's `C_PlaceStation` comment says “FIX: change from >> 4 to >> 3” but the code uses `>> 4`; the comment is stale. Do not “fix” it in the port.
- `F1519_fp` (“TEST: New vector normalisation…”) is in the normal matrix path and uses x87 floats. A naive Rust `f32`/`f64` port will drift.
- Several `FIX:`/`ANISO` changes are unconditional (laser invulnerability, collision radius, missile retardation, orbital drift, starports below ground, hyperspace time-travel, groundhog day, etc.). These are deviations from DOS 1.02, not just presentation.
- The source labels do not map 1:1 to original file offsets. The converter needs a generated symbol/layout map or signature-based extraction; verify against the original data section hash.
- `FFED3D` starts by `_chdir("game")` if `mission.dat` is absent. The finished product should use an explicit path search, not a working-directory hack.
- `ffed3d.vcxproj` Release links the **debug** CRT (`/MTd`) and disables optimisation; both configs use `StructMemberAlignment=1Byte`. Our study build needed `#pragma pack(1)` around FFED3D structs to avoid heap corruption.
- `PATCHES.INI` is not the FFE patch; `IMAGE.DAT` says the installed EXE is 2,101,248 bytes while our `FIRSTENC.EXE` is 2,097,152 — the installer size includes a 4 KiB trailer/rounding, not a different file. Do not infer a version from that alone.
- Splash text explicitly says owning a reverse-engineered copy is illegal even with the original; legal docs reflect this.

### Open questions

See `docs/INVENTORY.md` §10 and `docs/PHASE0_SUMMARY.md`. The blocking ones are: simulation oracle, Classic look/aspect/palette, music synth, CD version, legacy submodule, rule-4 gate, trace-harness patches, and reference build toolchain.

---

## 2026-10-20 — Repository hygiene

- Added `.gitignore` for `game/`, `legacy/`, `data.bin`, `to-be-upscaled/`, `upscaled/`, `extracted/`, frames/screenshots, build artefacts, saves.
- Added `.gitattributes` with `upscaled.bin filter=lfs diff=lfs merge=lfs -text`.
- Added `ci/check_forbidden.py` and a GitHub Actions job that fails if forbidden material is tracked (game data, legacy source, `data.bin`, original art, frames, FFED3D binaries).
- Added `LICENSE` placeholder (no licence chosen), `docs/LEGAL.md`, `docs/TAKEDOWN.md`.
- Did **not** add a Rust workspace: the port starts only after Phase 0 approval.

## Ship statistics, eyes and turret views

Cache v12 adds `base:ship/stats` (466 x 48-byte blocks from descriptor +0x38).
The engine glow now uses the original thrust word (F600: throttle times the
ship's forward or reverse limit) instead of the 4095 approximation. Front and
rear views sit at the ship's eye offset (FUNC_001053), and the top/bottom turret
views (F1 cycle, F8, console button 0x16, FUNC_001038 aiming) are ported for
ships with more than two gun mounts. verify-assets counts were bumped to v12;
its byte-for-byte check of the stats blocks against JJFFE is still to be added.

## The original line rasteriser (v0.48f)

F1616 DrawLine with F1601 DrawHLine is ported in `ffe-core::model_line` and
verified against the executed original: the new `--line-raster` adapter mode
draws 4096 synthetic on-screen lines and `verify-lines.py` compares every
320x200 buffer (4096 compared, 0 mismatched; tampered fields and an empty
trace rejected). Opcode 2 lines now take F1704's clipped, stored points (the
verified `model_thruster::line`) instead of raw projections, so every line
entry, narrow thrusters included, is drawn with it. Notes:
`docs/notes/lines.md`.

## The original curve rasteriser (v0.48g)

F1811's curves now draw with the original F1546 (`ffe-core::model_line::
draw_curve`): F1549's forward-difference steps (shared with the verified polygon
edges), F1561's one-end clip and F1616 per segment, with no closing edge. The
new `--curve-raster` mode runs the original on 4096 synthetic curves into a
320x800 buffer (so writes F1561 leaves outside the view are captured);
`verify-lines.py` compares both traces (4096 + 4096, 0 mismatched). The
viewer's stand-in cubic is gone.

## The original flat-face rasterisers (v0.48h)

F1604 DrawTriangle and F1607/F1608 DrawQuad (with F1602, F1603 and F1601) are
ported in `ffe-core::model_fill` and verified by the new `--fill-raster` mode
and `verify-fills.py` (4096 faces, 0 mismatched). Every on-view untextured
triangle and quad now draws with them, so most of a model's surface is the
original's pixels; faces reaching off the view and polygons still use the
diagnostic fill. F1616's slope now shares F1603's gradient (re-verified). The
flight-view test no longer expects a pixel from a degenerate triangle, which
the original fills with nothing. Notes: `docs/notes/fills.md`.

## Polygon spans and off-view faces (v0.48i)

The polygon span path is ported in `ffe-core::model_fill` (`Spans`,
`fill_polygon`: F1468, F1560 with F1563/F1559/F1558, F1613 with F1600/F1601)
and verified by the new `--span-raster` mode (4096 synthetic polygons, 0
mismatched). F1714, which queues a flat face reaching off the view (or across
the near plane) as an outline, is ported as `near_outline`; the texture-queue
probe now records those entries and the oracle compares 5,666 of them, 0
mismatched (model 361 left out: its data differs between JJFFE and this
release). The executor keeps each queued face's views for it. Polygons and all
queued flat faces now use the original rasterisers, and faces crossing the
near plane, previously skipped, are drawn.

## Oracle repairs (v0.48j)

`verify-assets.py` now expects cache v14 with 1,781 files, and the child-model
oracle masks the surface-resolution events 50-54 (F1874, verified for 448
models by `verify-station-models.py`) and 60/61 (F1876 queue words, not ported
yet), which the original now emits and the child execution does not model.
Both child runs report 0 mismatched (440 and 437 models) and the full set
passes: 31 of 31 oracles.

## Thrust limits (v0.49a)

F598, F599 and F600 (the clamp, per-axis cap and proportional scale of a
thrust request against an object's limit words) are ported in
`ffe-core::thrust` and verified by the new `--thrust-limits` mode and
`verify-thrust.py` (4096 cases, 0 mismatched). The ship statistics give each
craft its limits (`ShipStats::limits`). The first flight-core step: the
in-game thrust is not wired to them yet. Notes: `docs/notes/thrust.md`. Also
recorded: the owner decision to isolate drawing from the simulation RNG
(SIM-010).

## Velocity from thrust (v0.49b)

F601 and F602 (velocity from thrust over a time step: the Q31 transform of the
thrust words, the three time-step regimes, F601's caps) are ported in
`ffe-core::physics` and verified by the new `--accelerate` mode and
`verify-physics.py` (4096 cases, 0 mismatched). F602 differs from F601 in its
halving start (0xf, not 0x10) and its arithmetic shift of the time. Notes:
`docs/notes/physics.md`.

## Orientation update (v0.49c)

F585 (angular rates spent as small rotations of the orientation rows), with
F584 (step sizes), F611 (the rotation and its drift counter) and F1520 with
F1519 and F1552 (re-orthogonalising the matrix) are ported in `ffe-core::physics`
and verified by the new `--orient` mode and `verify-physics.py` (4096 cases,
447 through F1520, 0 mismatched). Notes: `docs/notes/physics.md`.

## The player's free-flight update (v0.49d)

F1041 (the player's orientation and thrust request in free space) is ported in
`ffe-core::player` with F607 (the set-speed request, F1515/F1516) and F1043 (the
engines-off thrust), and verified by the new `--player-orient` mode and
`verify-physics.py` (4096 cases, 0 mismatched). Frames of reference, gravity and
the planet auto-roll are not ported yet and the port refuses those cases.
Notes: `docs/notes/player.md`.

## Position from velocity (v0.49e)

F1512/F1511 (the velocity over a 64-bit time added to the 64-bit position,
with the original's shift normalisation) is ported as `Motion::advance` and
verified by the new `--advance` mode in `verify-physics.py` (4096 cases, 0
mismatched). With F601, F585 and F1041 this completes the physics core of one
free-flight step; the input layer (F1037/F1038), frames of reference and the
unit calibration for the playable still remain.

## A whole free-flight tick (v0.49f)

`Player::step` composes F1041, F601/F602 and F1512 in the order F1021 runs
them, and `--flight-step` compares it with the original chain over 4096
synthetic players (orientation, thrust, velocity and 64-bit position all
match). The physics core of free flight is now verified end to end; the unit
calibration for the playable, the input layer and frames of reference are next.

## The input layer (v0.49g)

F1037 (set speed from Return and Shift), F1038 (mouse and key steering, or the
turret in a turret view) and F1121 (the set-speed step) are ported in
`ffe-core::player` and verified by the new `--steer` mode (4096 cases, 0
mismatched). The comparison found an original quirk, now kept: with mouse roll
on, the roll's low byte is replaced by the view mode. Notes:
`docs/notes/player.md`.

## Original flight in the playable (v0.49h)

Key O switches the playable to the original flight model
(`ffe_rs::original_flight`): the verified tick (set speed, original steering
keys, thrust limits from the ship's statistics, velocity and position, full
orientation including roll) with the result copied back into `Flight` for the
views, engine glows, docking checks and the safety reset. Saves, docking and the
prototype model are unchanged; the save validator's speed bound rose from 250 to
5,000 flight units per second for the original ship's real acceleration. Notes:
`docs/notes/player.md`.

## Console buttons and time acceleration (v0.49i)

The console's real buttons are decoded from `DATA_005100` and `DATA_007684`
(`docs/notes/console.md`): the lower-left bar is the time acceleration meter
(not a throttle), beside four screen buttons. The meter and buttons are drawn
from the cache, the meter's steps and the 6 7 8 9 0 keys set the time
acceleration (1x to 10000x, scaling the original flight model's world time but
not its key time), the pause mark pauses, and the screens that are not ported
say so. Hot areas follow the original: inclusive edges, last match wins. A
readout shows speed, set speed and the time scale. The original console has no
speed or engines button in this build.
