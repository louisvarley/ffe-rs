# Animation inputs, rotating parts and station rotation

Status (2026-10-22): researched in the JJFFE 2.8a7 assembly, ported to
`ffe-core::model_animation` and verified against the executed original where
marked. The F6 viewer animates models over its own game clock and the flight
station spins at the original rate. This note does not replace
`docs/PORT_LOG.md`, `docs/STATUS.md` or `docs/MODEL_COMMANDS.md`; fold it in when
merging.

Claim status: **verified** = compared against the executed original by
`tools/reference/verify-animation.py`; **read** = from the assembly, not
executed; **notes** = from JJFFE's own reverse-engineering notes in
`jjffe-master/srctools/*.txt` (the author's annotations, cross-checked against
the assembly where stated, not otherwise verified); **unknown** = not
established.

## 1. Where a model's inputs come from

A model program reads three kinds of variable (F1473/F1474 selectors): constants,
**globals** (`0b10`, word `[instance+0x14c] + 0x9c + 2 * i`) and **locals**
(`0b11`, dword `instance + 0x54 + 4 * i`).

**The globals are the game object's own words.** Every top-level set-up stores the
*object record pointer* at instance +0x14c (F1680, F1683, F1684), so global word `i`
is object word `+0x9c + 2i`. A child instance copies the parent's +0x14c (F1750),
so a whole model tree reads the same object. **Verified** (the instance captured at
F1773 holds the object's address; 4,096 F1683 and 420 F1680 instances).

### 1.1 Top-level instance set-up

There are three entry points (all **read**; the parts listed as verified are
compared field by field at the F1773 call):

| Routine | Used for | Frame (+0x14) | Position (+0x4) | +0x148 / +0x130 |
|---|---|---|---|---|
| **F1680** (`DATA_007805`) | every object in the 3D view: F1536/F1537 frame loops (F1052, F982), and F1070/F1066 etc. | camera-relative (below) | object - camera (F1678), camera axes, scaled | descriptor +0x14 / +0x18, adjusted by distance |
| **F1683** via F1681 (`DATA_007806`, F1539 table) / F1682 (`DATA_007807`, F1538 table) | stand-alone display objects (callers F288, F562, F840, F841, F862, F917, F960, F1066, F1070) | object +0x00 copied | low dwords of object +0x26/+0x2e/+0x36 | descriptor +0x14 / 0 |
| **F1684** (`DATA_007808`) | as F1683 with the F1539 table and its own command pointer; F1774 receives `2 * word[obj+0xa4]` | object +0x00 | as F1683 | as F1683 |

All three then: +0xf8 (base handedness) = 0; F1686 cull (F1687 with radius
`desc[+0x1c] << +0x148`) and return when culled; **local 0 (+0x54) = 0**;
colour +0xf4..+0xf6 = 1,1,1; light +0x134 = `DATA_008821..8823` (F1680 uses
`DATA_008824..8826` when the camera is in a rotating frame); the eight shades
`DATA_008831` copied to +0x84 (locals 12..17); F1685 (observer +0x44/+0x48, light
in object axes +0x38); F1773 → F1774 (F1745 arena, +0x150/+0/+0x154, the +0x44
raise, F1689, F1480, +0xf0 = 2, F1772, F1746); then F1780(0, 0). **Verified**:
everything up to the F1773 call, for F1680 and F1683.

F1680 in detail (the same-frame branch is **verified**; the rotating-frame
branches are **read**):

1. F1678: relative 64-bit position object − camera (same parent and same +0x57:
   +0x3e − +0x3e; otherwise through global positions, F1677).
2. `msb` = F1657 (bit-length class of the vector: the high dwords' magnitudes
   when any is neither 0 nor −1, else the low dwords negated for negative
   components; result `floor(log2)` + 32 or + 0). If `msb > 0x1c` the vector is
   shifted right by `msb − 0x1c` (F1660).
3. Distance class `d = max(msb − 0xd, 0) − object[+0x7e]` → object +0x88. If
   `d >= desc[+0x20]`: not drawn; object +0x8a = `1 << (d − far)` (0x3f beyond
   5), screen +0x98/+0x9a = 0x8000. Otherwise +0x8a = 0.
4. Position = camera rows · low dwords (F1670). `e = desc[+0x18] + obj[+0x7e] −
   shift`: negative → +0x130 = `desc[+0x18] − e`, +0x148 = `max(desc[+0x14] + e,
   0)`; else position `>>= e` (count masked to 5 bits).
5. F1479 projection into object +0x98/+0x9a (0x8000 when it fails). F1686 cull.
6. z < 0x40 or `d < desc[+0x28]` → full model; otherwise an F1700 dot.
7. Frame: camera not in a rotating frame (+0x57 = 0): F1673(obj +0x00, camera)
   (`result[i][j]` = object row i · camera row j), or the object's global matrix
   (+0x5a, F1679) when the object itself is in one. Camera in a rotating frame:
   if the camera's parent is this object (`cam[+0x56] == obj[+0x86]`), the frame
   is the camera matrix transposed (F1665) — **the object's own rotation is not
   applied**; otherwise F1673 with the camera's global matrix (+0x5a).

### 1.2 Locals

Only **local 0** is written at top level (0). **Locals 12..17** are the shade bytes
(+0x84..+0x9b). **Locals 1..11 are not written**: they are F1680's/F1683's stack
frame, i.e. whatever the previous call at that stack depth left (in F1537's loop,
typically the previous object's instance). Locals 7..11 are a child's vertex
pointers (+0x70..+0x80). A child instance gets `local 0 = parent local 0 + 1`
(nesting depth) and the parent's locals 1 and 2 (F1750; child note).

So locals 1 and 2 are the parameters a parent model passes to its children, and
local 0 is the nesting depth. Census (`crates/ffe-rs/tests/animation_census.rs`,
executed natively with locals 1..11 marked undefined, 21 global settings): **55
models read local 1 or 2 before writing it**; 51 of them are called as children
(e.g. 347..360 by 176, 329/330 by 320, 235 by 227/233/236/368/...), which receive
the value from their parent. **4 are never called as a child — 216, 287, 328,
333** — and would read stale stack data if drawn at top level (unknown whether
the game ever draws them at top level).

The port marks locals 1..11 undefined at top level
(`TopLevel::context.undefined_locals`); the viewer, which must draw anything,
fills them with 0 (or the test input) and says so.

The basis (+0x9c) and its handedness (+0xfc) are also stack data at top level until
opcode 28 or 25 writes them; the port keeps treating them as caller-supplied, as
before.

### 1.3 The global words (object +0x9c..)

| Global | Object offset | Contents | Writer | Status |
|---:|---|---|---|---|
| 0 | +0x9c | `GameTics >> 10`, 16 bits | F1534 value, stored by F1536/F1537 into each non-sleeping object (+0x8b = 0) just before F1680; also at creation (F288, F562, F1066, F1070 compute their own) | **verified** (F1537 run on synthetic lists: 1,536 drawn objects, 240 sleeping) |
| 1 | +0x9e | landing gear (ships) / docking door (stations, starports) | ships: player code in F1008 (moves toward 0 while `DATA_008871` bit 1 is set, otherwise toward 0xffff) and AI F730 (toward 0 for AI modes < 0x1e, toward 0xffff for modes > 0xc and ≥ 0x1e); stations/starports: F865 from F866 | F865 **verified**; ship code **read** |
| 2, 3 | +0xa0 | random seed (dword) | creation | notes |
| 4 | +0xa4 | mass (FFP, planets); light-point count for opcode 30 0x801e; F1684 passes `2 * word` | creation | read |
| 6, 7 | +0xa8 | creation tics; orbitals: tics of the last F866 update | F866 | **verified** (F866) |
| 8, 9 | +0xac | creation day; orbitals: day of the last F866 update | F866 | **verified** |
| 10..12 | +0xb0..+0xb4 | ships: pitch, roll, yaw rates | physics | notes |
| 13..15 | +0xb6..+0xba | ships: x, y, z acceleration rates (thrust) | physics | notes |
| 16..21 | +0xbc..+0xc6 | ships: main, retro, top, bottom, right, left thruster accelerations | physics | notes |
| 22..25 | +0xc8..+0xcf | ships: equipment / damaged equipment flags; orbitals: rotation speed byte +0xc8, door direction +0xc9, orientation word +0xcc | | notes |
| 26.. | +0xd0.. | ships: drive, lasers, missiles; +0xe0/+0xe2 shields | | notes |

Which globals the models react to (census, perturbation of each word under the
same 21 settings; indicative, branch coverage limited to what those settings
reach): global 0 – 47 models (including stations 74/75 and many ships); 1 – 62
(gear, doors); 2 – 39 (seed); 3 – 4; 10 – 1; 13 – 18; 14 – 20; 15 – 37 (thrust);
23 – 13; 27 – 2; 29 – 5; 30 – 2; 33 – 1; 34/35 – 1 (shields); 37 – 1. Rendering
ships 30 and 37 with global 1 = 0xffff adds their gear shapes (7 → 10 and 6 → 25
shapes), so 0xffff is gear extended.

### 1.4 How global 0 and the gear/door word advance

- `GameTics` (`DATA_008804`) advances by the frame's accelerated delta
  `DATA_009155` = `elapsed_ms * DATA_007706 / 20` (994 at acceleration 1, i.e.
  49,700 tics per second). Global 0 therefore advances 48.5 units per second at
  1x and wraps every 2^26 tics (22.5 game minutes).
- F865 (stations/starports): with +0xc9 > 0 the word grows by `DATA_009155 >> 1`
  per update, stops at 0xffff and clears bit 0 of +0xc9; with +0xc9 < 0 it falls
  by the same and stops at 0 with +0xc9 = −2 (signed 32-bit compares). Full travel
  takes 2.6 s at 1x. Ships use the same rate in F1008/F730.

## 2. Orbital (station) rotation

**F866** is the orbital update (event handler; parameter `{tics, days, object}`;
**verified** on 4,096 synthetic records with zero angular velocity):

1. Old `+0xa8/+0xac` are kept; the new tics/day are stored.
2. Starports (+0x14c bit 0x20): F865 only, return — **starports never rotate**.
3. Otherwise the orbit (F1510 angular velocity × elapsed time, F1507/F1508
   position; **not** exercised: zero angular velocity), then F865 for stations
   (bit 0x10).
4. Rotation, when the speed byte +0xc8 is non-zero: `s = (signed)+0xc8 − 1`; for
   `s >= 0` both old and new tics are shifted left by `s` (count masked to 5
   bits); when bits 20.. of the two differ, the matrix is rebuilt by F1675:
   stations `MatBuildYZT(obj, new >> 16, (i16)+0xcc)`, other orbitals
   `MatBuildYZT(obj, (i16)+0xcc, new >> 16)`.

**F1675** (`MatBuildYZT(m, a, b)`, **verified**, 2,640 cases): with Sin32 values
and F1521 products, rows `[ca·cb, sa, −sb·ca]`, `[−sa·cb, ca, sa·sb]`,
`[sb, 0, cb]`. Row k is object axis k in the parent frame, so a station spins
about **its own z axis**, which points along `(sin b, 0, cos b)` of its parent's
frame; increasing `a` turns x towards y.

**Rate.** F892 (station generator) copies one of three hand-coded Sol templates
(F875 → F873 parameter 10, stored at +0x3e) and F880 copies that byte to +0xc8:

| F892 case | Template | Hand-coded object | Speed | Models (`DATA_007136..7138`) |
|---|---|---|---:|---|
| 0, 3 | `DATA_008794` | Daedalus (type 0x20) | 12 | 0x48, 0x4b, 0x4c, 0x4e; case 3: 0x45 |
| 1 | `DATA_008797` | Galileo (0x21) | 11 | 0x49, 0x4e, 0x4d, 0x4c |
| 2, 4 | `DATA_008795` | Abraham Lincoln (0x22) | 10 | 0x50, 0x4a; case 4: 0x4f |

F873 stores orientation +0x42 = 0, so generated stations have orientation 0: the
spin axis is the parent frame's z. One revolution takes `2^32 >> (speed − 1)`
tics: speed 12 → 2^21 tics ≈ 42 s, 11 → ≈ 84 s, 10 → 2^23 tics ≈ 169 s at
acceleration 1. **Model 74 (0x4a), the flight station, is case 2: speed 10.**
The rebuild rule (bits 20..) means the matrix is renewed every
`2^(21 − speed)` tics (every frame at 1x for speeds ≥ 10); at high acceleration the
angle is simply sampled at each update. Hand-coded starports (F874) have speed 0.
The orbital creation routine F40 (**read**) builds an initial
matrix from `(tics << speed) & 0xffff` instead; F866 replaces it at the next
rebuild.

### 2.1 What this means for docking

**read** (ffe.asm F570, F572, F573, F1680; notes ccoll.txt):

- Near its parent (when F575's collision test reports bit 1 in F570, with a 0x7f
  delay in +0x148 between switches) a ship is moved **into the parent's rotating
  frame**: F572 transforms its position (F1670 by the parent's matrix), its
  orientation (F1673) and its velocity into the station's frame and sets +0x57 = 1;
  F573 does the reverse.
- In that frame the ship's position and orientation are relative to the rotating
  station, so the station stands still relative to the ship and the rest of the
  universe turns; F1680 draws the station with the camera's matrix only (step 7
  above) and other objects through global matrices. Docking alignment (the slot's
  roll) is therefore judged in the station frame; in the outside view the player
  sees the station spin and must match its roll to enter the slot.
- The experimental flight (`crates/ffe-rs/src/flight.rs`) has no rotating frame:
  its docking test checks range, speed and facing only, and facing is unaffected
  by a spin about the docking axis. The flight view now draws the station spinning
  (outside view); flight mechanics are unchanged and the synthetic docking test
  still passes. A faithful port needs F570/F572/F573 and F575's thresholds — not
  done here.

## 3. The port

`ffe-core::model_animation`:

- `time_word(tics)` — global 0 (F1534); `START_TICS` (0x80000000),
  `TICS_PER_FRAME` (994); `global::{TIME, STATE, SEED, MASS, UPDATED_TICS,
  UPDATED_DAYS}`.
- `door_step(word, state, frame_time)` — F865.
- `build_yzt(table, a, b)` — F1675.
- `OrbitalRotation { speed, orientation, station }` with `angle(tics)`,
  `matrix(table, tics)`, `update(table, previous, tics)` (F866's rule) and
  `OrbitalRotation::station(speed)`; `TRADING_POST_SPEED` (12),
  `ORBITAL_STATION_SPEED` (11), `ORBITAL_CITY_SPEED` (10), `station_speeds(model)`.
- `ObjectState`, `Camera`, `Descriptor`; `setup_display` (F1683) and
  `setup_object` (F1680, same frame of reference) → `TopLevel { context,
  instance, light, observer_scale, observer }` / `Draw::{Far, Culled, Dot,
  Model}`; `TopLevel::execution_observer` (F1774's raise); helpers
  `vector_msb` (F1657), `shift_vector` (F1660), `rows_product` (F1673),
  `top_level_locals`.

`ffe-rs`:

- `model_viewer::AnimationInputs::{original(tics, state_word, shades), test(v)}`.
- `ModelViewer`: own clock starting at `START_TICS`, `advance(ms)` (=
  `timer_delta(ms, 994 × rate)`, then F865 on the gear/door word),
  `toggle_door`, `cycle_rate` (×1/×10/×100/×1000, diagnostic), `use_clock`,
  `set_animation` (test input), `inputs`, `clock`, `set_clock`, `shades`;
  `render_scene_with(.., &AnimationInputs)` (`render_scene` keeps test input 0);
  `station_rows(tics, speed)` — the station's axes at a game time.
- F6 keys: **C** original inputs over the clock (default), **0/5/9** test input
  0/32768/65535, **G** gear/door toggle, **T** acceleration ×1/×10/×100/×1000.
  The status line shows `TIME` (global 0), `GEAR/DOOR` (global 1) and the rate.
- Flight: `screen::flight_tics(state)` = `START_TICS + 994 × ticks`;
  `draw_station` composes `station_rows(tics, 10)` with `STATION_AXES` and runs
  model 74 with `AnimationInputs::original(tics, 0, ..)` (its ring segments also
  move with global 0).

## 4. Verification

New adapter modes in `tools/reference/headless.c` (exports in `exports.inc`,
probe hooks in `make-probe.py`: inside F1680 `F1773 → trace_object_run` and
`F1700 → trace_object_dot`; inside F1683 `F1773 → trace_display_run`; the hooks
capture the instance and do not run the model; no existing mode calls F1680 or
F1683, so existing traces are unchanged):

- `--animation` (both binaries): F1675 on 264 × 10 angle pairs (including
  out-of-range ints); F866 on 4,096 random synthetic orbital records (flags 0x10,
  0, 0x20, 0x30, 0x11; speeds 0, 1, 2, 9..13, 31..33, 127, −1, −128; door states
  −128..127; frame times 0..0xffffffff; tics pairs crossing and not crossing the
  bit-20 boundary).
- `--object-instance` (probe binary): 96 frames of the real F1537 over synthetic
  object lists (1..40 objects, random matrices, 64-bit positions from 2^0 to 2^50
  or near the camera, scales −3..8, random words, some sleeping) with a random
  camera and star direction, then 4,096 real F1683 calls on synthetic display
  objects for every non-null model descriptor.

`tools/reference/verify-animation.py` runs each mode twice (traces identical),
then `crates/ffe-rs/tests/animation_reference.rs`:

- F1675: 2,640 / 2,640 matrices equal; F866: 4,096 / 4,096 (1,373 rebuilds,
  1,883 door-word moves) — word, state, stored tics/day and matrix equal.
- F1537/F1534: light (star direction in camera axes) equal in all 96 frames;
  global 0 of every non-sleeping object = `time_word(tics)`; 240 sleeping objects
  only count down; words 1..63 unchanged.
- F1680: 722 far, 362 culled, 32 dots, 420 model runs; distance class, outside
  byte, screen position and every captured instance field equal (position, frame,
  +0x38, +0x44, +0x48, local 0, shades, +0x130, +0x134, +0x148, +0x14c, colour,
  +0xf8).
- F1683: 1,952 drawn (instance equal), 2,144 culled.
- 22 tamper cases (5 animation, 17 object-instance) and both empty traces are
  rejected.

Trace SHA-256 (Linux LF): animation
`10ed4039b00f3786c8766077fd0f955d85ee15da4bb7e197c898b36c3d4d2cb9`, object
instance `2ab63f4da4342b9239632938081299cac0d2775462cc56e63bf28494425554dd`.
`verify-all.py` now includes the new oracle (27 scripts).

Not verified: the ship gear code (F1008, F730), the orbit path of F866, the
rotating-frame branches of F1680, F572/F573, global words other than 0, 1, 6..9,
and anything the models do with these inputs beyond what the existing
command-order oracles already cover.

## 5. Unknown / open

- Whether the four models that read local 1/2 without being anyone's child (216,
  287, 328, 333) are ever drawn at top level, and if so with which stale values.
- The meaning of most ship globals is from JJFFE's notes, not traced to their
  writers.
- `DATA_008871` bit 1's polarity for the player's gear (the notes call it
  "undercarriage down", but the code moves the word towards 0 while it is set);
  the model renders show 0xffff as extended.
- The original's acceleration levels (`DATA_007706` values) are not modelled; the
  viewer's ×10/×100/×1000 are diagnostic.
