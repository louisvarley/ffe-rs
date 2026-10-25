# Intro: complete-implementation TODO

> **Status 2026-10-25:** the drawing and timing slice is implemented: the boot
> plays F961/F962 through the v27 cache, `--skip-intro` bypasses it,
> `--capture-intro` writes the sample frames, and `front_end`/`intro_text`
> tests cover the clock, thresholds, keys and text layout. See
> `docs/notes/intro.md` §"Boot drawing and timing" for the source map and the
> remaining items (music, start types 2-5, the dynamic upper palette, model
> 176's late undefined-stack child basis). The checklist below is the original
> plan and is kept for the slices not yet done.

Planning note, 2026-10-24. **No code is changed by this note.** It is the
work breakdown for the boot flow, not the feature record; no `docs/notes/intro.md`
existed when this was written (the in-progress `verify-intro.py` docstring already
points at one, so create it when the feature lands, alongside this checklist).

**Owner decision (2026-10-24): the original DOS FFE is the source of truth.**
JJFFE (`jjffe-master/ffe.asm`) is the executable gameplay oracle only where its
data matches the DOS release. Where it does not, the DOS value wins and the
difference is recorded below. This supersedes the older "JJFFE is selected"
sentence still in `CLAUDE.MD` §3; reconcile that file when the feature lands.

Base commit of this worktree: `c82909a` (F650 pads). A parallel stream has
**uncommitted** work that implements the first part of this feature (timeline,
oracle and part of the new-game slice); it is listed as
*in progress* below and must not be re-done. The specific review points in
§4 Slice 0 come from reading that working tree on 2026-10-24; re-check them against the
merged code before fixing.

---

## 0. Bottom line

The intro is **not implemented**. There is no intro screen, no logo rendering,
no credits renderer, no boot state machine and no music. The data side is partly
transcribed (front-end timeline in the parallel stream) and the new-game state
is partly ported (parallel stream). The largest gaps, in order:

1. The DOS credit **byte strings** and the **character-width table** are not in
   `data.bin`; `front_end.rs` currently hardcodes a plain-text transcription and
   loses the embedded `0xfd`/`0xfe` layout codes.
2. There is no `F1399`/`F1587`/`F1589` text writer (shadow, newline, `0x1e`/`0x1f`
   positioning, `0x03` wrap) and no `F1348` expander in Rust.
3. There is no intro screen: model 177/176, bitmap `0x42`, text and palette are
   never drawn.
4. There is no boot state machine: `GameTics` 0x400, the V/1-5 keys, the
   `0x7e7b76` loop, and the transition to `F147` are absent.
5. No music asset and no audio backend; the original plays song 0, `qqfront`.
6. Start types 2-5 are not ported or verified (they differ in ship, cash, hold,
   state, dock slot and system, not just the system table).

The **intended end state** is a first-run flow that looks and behaves like the
DOS 1.02 floppy release, with a deterministic `--intro`/`--state` oracle for the
data and a documented set of things that can only be checked against a DOS
screenshot.

---

## 1. Definition of "fully complete intro" (scope)

"Fully complete" for this project means: **from process start to the first
controllable in-game frame, the DOS 1.02 floppy boot path is reproduced
observably and deterministically, and the transition lands on the verified
new-game state.** Concretely:

1. No JJFFE legal/acceptance splash. The boot goes straight to the original's
   logo screens. (The DOS `game/FIRSTENC.EXE` contains no `JJFFE`, no
   "replacement executable", no "reverse engineered", no `Press "y"` string;
   searched 2026-10-24. The splash strings are only in
   `jjffe-master/ffe.asm:68222-68378`.)
2. Model **177** from `GameTics` 0x400 to 0x8276d, then model **176** from
   0x8276e, drawn through the original's `F1682`/`F1683` path with animation
   `GameTics >> 10` (the second model rebases at 0x8276e), the scene object's
   frame/position exactly as `F1683` reads them, and the default lighting shades
   (`F960`). See §3.3 for the unresolved frame question.
3. Primary bitmap **0x42** (`base:bitmap/primary/066`, 311×42, the
   "FIRST ENCOUNTERS" strip) blitted at **x = 5, y = 158** once
   `GameTics > 0x795cc`.
4. The **18 credit lines** `0x941b..0x942c` in their exact `GameTics` windows,
   at their exact x/y/wrap/colour, with the **raw DOS byte strings** and the
   original text writer (shadow, embedded position codes, wrapping).
5. The version line `0x942d` on `V`, in the DOS release's text
   **"English Release 1.02 - 21/04/95"**, at x 10, y 60, wrap 310, colour 16,
   only from the DOS gate `GameTics >= 0x719668` (not JJFFE's 0x8276e).
6. The intro loops: at `GameTics >= 0x7e7b76` the buffer is cleared, flipped,
   cleared again, the pointer bit is set, `GameTics` is reset to 0, and the
   sequence repeats. Keys 1-5 start; V shows the version.
7. Song **0** (`MUSIC/QQFRONT.HMP`, the DOS song table's first entry) plays in
   the intro, restarts when done, and stops when a start key is chosen.
8. Pressing 1 starts the **verified new game**: Ross 154, Eagle (model 23), 1000
   credit-units (100.0 CR), day 0x121cf7, fraction 0x80000000, display
   3250-01-01 12:00, player state 0x30, player index 107, time acceleration 994.
9. The intro is deterministic (no wall clock in the timeline; fixed steps) and
   is covered by the committed `--intro` and `--state`/`verify-reference`
   oracles plus synthetic tests.
10. Start types 2-5 are either implemented to the same standard or explicitly
    **refused by an owner-approved deviation**. Silently mapping them to type 1
    is not "complete".

### Deliberately out of scope

- **JJFFE's legal splash** (`ffe.asm:68222-68378`) and its 100-tick input
  lockout (`splashTimeout`). It is the mod's, not the DOS release's. Do not add
  a "dev splash" unless the owner asks.
- **CD AVI / bulletin-board video.** The floppy `FIRSTENC.EXE` contains no
  `.avi` string at all, so the DOS floppy boot path cannot play the CD's video.
  The CD carries 323 `DATA/*.AVI` clips (`BB*`, `C*`, `F*`, `FR*`, `FSH*`,
  `G*`, `I*`, `ISH*`, `ISY*`, `M*`, `P*`, `R*`, `SH*`, `ST*`), all listed in
  `docs/GAME_FILES.md`; they are CD-only content. Supporting them is a separate
  importer/player feature (module 13, `DATA_002319`).
- **CD 1.01 executable boot differences.** The cache can recognise the CD
  profile, but this note, the oracle traces and the data offsets are floppy 1.02
  only.
- **The modules' full `F144` initialisation** beyond what is observable in the
  frame-161 state (combat, missions, BBS, trade, equipment, markets). That is the
  gameplay slice, not the intro. Record anything the first frame needs.
- **The two ASCII digits** `DATA_009130`/`DATA_009131` that `F961` increments at
  the restart (ffe.asm:71706-71715). Nothing in the DOS/JJFFE code reads them;
  they are not rendered. Do not invent a display for them.
- **Localisation.** The supplied release is English; the SysMenu strings are
  English.
- **In-game station menus/UI.** Out of the intro's exit condition.
- **Sound effects** (the 23 cached `base:effect/*` WAVs). The intro's only audio
  is the song.

---

## 2. Inventory

State key: **done** = shipped and verified; **partial** = some code/data, not
complete or not wired; **missing** = nothing usable; **blocked** = needs an
owner decision or a missing dependency; **in progress** = parallel stream,
uncommitted on 2026-10-24; **excluded** = owner decision.

| # | Item | State | Where it lives (source / Rust) | Evidence / test | Owner decision |
|---|---|---|---|---|---|
| 1 | Boot without JJFFE splash | excluded | DOS `FIRSTENC.EXE`; `ffe.asm:68222-68378` | DOS EXE has none of the splash strings | confirmed excluded |
| 2 | Front-end timeline (models, thresholds, bitmap, animation) | **in progress** | `ffe.asm:71624-72058`; `crates/ffe-rs/src/front_end.rs` (parallel, uncommitted) | `--intro` oracle rows; current slice has the review bugs in §4 Slice 0 | — |
| 3 | 18 credit strings (raw bytes) | **partial** | DOS file `0xbd361..`; `ffedat.asm:4271-4330`; hardcoded plain text in `front_end.rs` | text matches DOS; raw `0xfd`/`0xfe` codes lost; not cache content | approve caching (legal) |
| 4 | Credit text writer (F1399/F1587/F1589) | **missing** | `ffe.asm:85096-85126`, `93822-94060`, `94028` | none | — |
| 5 | FF-code expander (F1348) | **missing** | `ffe.asm:83979-84016` | none | — |
| 6 | Wrap (F1398 + `DATA_007648`) | **missing** | `ffe.asm:85127`; width table DOS file `0xe9998` | width table verified byte-for-byte against DOS; no Rust code | — |
| 7 | Primary bitmap 0x42 at (5,158) | **partial** (data done) | `ffedat.asm`/DOS; cache `base:bitmap/primary/066`; `ffe.asm:71734-71760` | asset 311×42, sha256 `dc65f833…`; no blit in the intro path | — |
| 8 | Model 177/176 drawing | **partial** (data + renderer primitive done) | DOS model table; `crates/ffe-rs/src/model_viewer.rs::render_scene_with`; `ffe.asm:71624-71648`, `102201` | census: 176 has 2 vertices/68 executed commands, 177 has 6/29; a probe renders 6-8 shapes for 177 and 44 for 176 at 0x181534 (frame not yet proven, see C1) | — |
| 9 | Intro palette / `F1575` calls | **partial** | `ffe.asm:71649-71715`, `93539`, `82962`; `screen.rs` static low palette | no dynamic palette; intro likely low-palette only; check hashes | — |
| 10 | Boot state machine (`GameTics` 0x400, loop, keys) | **missing** | `ffe.asm:68405-68495`; `main.rs` | none | default boot = intro |
| 11 | V / version line | **partial**, wrong data | `ffe.asm:71776-71796`; DOS gate `0x719668`; `front_end.rs::VERSION_LINE` | DOS string differs; gate differs | — |
| 12 | Start keys 1-5 | **partial** | `ffe.asm:68443-68452`; key table at `DATA_007191`; `front_end.rs::Intro::key` | only type 1 state verified; gating wrong | ship 2-5 or map with deviation |
| 13 | New game `F147`/`F998`/`F994` | **in progress** | `ffe.asm:14912-14965`, `73601`, `74028-74230`; `navigation.rs::new_game`, `START_SYSTEMS` | type-1 state verified; types 2-5 constants not ported; RNG seed differs | DOS seed vs oracle |
| 14 | Frame-161 invariants | **done** in reference, **in progress** in Rust | `crates/ffe-tools/src/reference.rs::verify_state`; `tests/*state*` | 400 frames, frame 161, 238 clock transitions | — |
| 15 | Music asset (HMP) | **missing** | `MUSIC/*.HMP`; `ffe-tools` only inspects; `FORMATS.md:583` | 17 files match CD/floppy; `qqfront` sha256 `28ded21f…` | cache losslessly? |
| 16 | Music playback | **blocked** | no audio backend in `ffe-rs`; `ffe.asm:132560` (F1902) | 23 effects cached but never played | choose backend |
| 17 | CD AVI/video | **excluded** | CD `DATA/*.AVI`; module 13 `DATA_002319` | 323 clips, floppy has no `.avi` | keep excluded |
| 18 | Intro loop/restart | **partial** | `ffe.asm:71700-71720`; `front_end.rs::IntroTimeline::at` | `finished` flag exists; boot missing | — |
| 19 | `--intro` oracle | **in progress**, inconsistent | `tools/reference/headless.c --intro`, `exports.inc`, `verify-intro.py`, `tests/intro_reference.rs` | 76 rows by construction; script says 75; version case uses JJFFE gate | — |
| 20 | `--state` oracle | **done** | `headless.c`, `build-smoke.ps1`, `crates/ffe-tools/src/reference.rs` | frame 161, 238 transitions, repeat-identical | RNG seed caveat |
| 21 | Intro pixel/screenshot oracle | **missing** | no mode; `--game-frame` only starts the game | none | — |
| 22 | Docs (feature record, STATUS, PORT_LOG, DEVIATIONS) | **missing** | `docs/notes/intro.md` absent; `docs/STATUS.md`, `docs/PORT_LOG.md` | this note is the TODO, not the record | — |
| 23 | Synthetic tests | **in progress** | `crates/ffe-rs/tests/front_end.rs` | tests currently assert the wrong first-credit window (§4 Slice 0) | — |
| 24 | Burndown items | todo | `dashboard/burndown.js` `ui-7`, `audio-3`, `content-3` | **not edited by this note** | — |

---

## 3. DOS evidence and the JJFFE differences to flag

All offsets below are in the supplied floppy 1.02 `game/FIRSTENC.EXE`
(SHA-256 `d9195661…`, 2,097,152 bytes). They were checked by searching bytes and
disassembling with `ndisasm` on 2026-10-24. Nothing here is an inferred RVA.

### 3.1 The DOS front end is the same code as JJFFE's

The DOS credit-window and model-switch logic is the same as JJFFE's
`F961`/`F962` (different addresses and one input gate; see §3.2):

| What | DOS file offset | JJFFE |
|---|---|---|
| Front-end draw (F961) | `0x2f298` | `ffe.asm:71649` |
| Front-end credits (F962) | `0x2f380` | `ffe.asm:71724` |
| Main intro loop | `0x2d2ef` | `ffe.asm:68405` |
| `F147` new game | `0xa1ac` | `ffe.asm:14912` |
| Boot seed (`F958`) | `0xa164` | `ffe.asm:71508` |

The DOS code contains the exact thresholds `0x795cc`, `0x8276e`, `0x7e7b76`,
`0xfbd3a`, `0x181534`, `0x1ee8d2`, … `0x77a7d8` and the exact credit calls
`0x941b..0x942d` with the same x/y/wrap/colour arguments. So the JJFFE oracle is
valid for the timeline and credit geometry; it is **not** valid for the items
in §3.2.

### 3.2 Real DOS-vs-JJFFE differences

| # | Item | DOS 1.02 | JJFFE 2.8a7 | Consequence |
|---|---|---|---|---|
| D1 | Legal splash | absent (no such strings) | `ffe.asm:68222-68378` | Excluded by owner. |
| D2 | RNG seed B at new game (`F147`) | `0x89abcdef` (file `0xa1c9`) | `0x456789ab` (`ffe.asm:14924`) | Any ported new-game RNG must use DOS. The `--state` trace's RNG values are JJFFE's; do not assert them as DOS. |
| D3 | Version line `0x942d` | `"English Release 1.02 - 21/04/95"` (file `0xbd951`) | `"English Release 1.1 (Remastered Version)"` (`ffedat.asm:4331`) | The in-progress Rust string is wrong for DOS. |
| D4 | V input gate in F962 | `GameTics >= 0x719668` (file `0x2f6a1`) | `>= 0x8276e` (`ffe.asm:71765`) | V is unavailable for most of the DOS credits; the `--intro` version case at 0x8276e is JJFFE-only. |
| D5 | Model-string 0x408a | `GAME!TEK` (file `0xbbb32`) | `FRONTIER` (`ffedat.asm:3569`) | The port's cache is DOS-derived and already has `GAME!TEK`; a JJFFE pixel reference of model 177 will show `FRONTIER`. |
| D6 | Credit text order/codes | 0x9428 = Journal Entries (x20,y100), 0x9427 = Representation (x140,y10) | same code, same order | The in-progress Rust table has `+12`/`+13` swapped (see §4 Slice 0). |

Also verified against DOS, and matching:

- **Credits block**: 18 NUL-terminated strings in sequence starting at file
  `0xbd361`; the version is the 19th at `0xbd951`. The SysMenu table
  (`DATA_004894`) has 46 entries; `0x941b..0x942d` are table indices 27..45
  (`ffe.asm:18456`, `ffedat.asm:3767-3813`).
- **Character widths**: the full 256-byte `DATA_007648` table is at file
  `0xe9998` and matches JJFFE's declaration byte-for-byte. This is the table
  `F1398` uses for wrapping.
- **Song table**: file `0xefd10` starts `qqfront`, `qqatmosp`, `qqtravel`, …
  exactly the 17-name order in `jjffe-master/win32/w32sound.cpp:79-96`; song 0
  is `qqfront`, and `F1902(0)` is called in the boot loop
  (`ffe.asm:68419`, `68428`).
- **Model strings**: the cache's `base:text/model-names` (219 entries,
  `source_offset` 766020) already carries the DOS `GAME!TEK`/`PRESENTS`/
  `DAVID BRABEN'S` strings; model 177's descendant texts are `0x408a..0x408c`
  (GAME!TEK, PRESENTS, DAVID BRABEN'S) and model 176's descendants include
  `0x40ce` ("1st Encounter"), per a `text_census` run over `data.bin`.

### 3.3 What the DOS code actually does

- `F961` (DOS `0x2f298`): while `GameTics < 0x8276e`, animation =
  `GameTics >> 10`, model 177, and `F1575` is called with `(0,0,0)`; from
  0x8276e, animation = `(GameTics - 0x8276e) >> 10`, model 176, and `F1575`
  with the game state's `(0,0,4)`. `F960` sets object +0x82 (model) and +0x84
  (animation), copies the default light/shades, and calls `F1682`
  (`DATA_007807`) to draw the object. `F1682` → `F1683` sets up the instance
  from the **scene object** (`DATA_009127`) and the model block returned by
  `F1538` (frame, position and scale fields). The scene object is BSS-zero
  except +0x82/+0x84. **Which field is the display frame, and how a zeroed
  object still draws the logo, is not yet established**; the C1 probe with
  identity/zero does not reproduce a usable 177 frame.
- `F962` (DOS `0x2f380`): blits bitmap 0x42 when `GameTics > 0x795cc`;
  then returns when `GameTics < 0x8276e`; then draws the applicable credit calls
  in code order; from `GameTics >= 0x719668` reads the GUI action, latches `V`
  into `DATA_007237`, and draws 0x942d at (10,60), wrap 310, colour 16; then
  draws 0x942b/0x942c at (5,10)/(5,68), wrap 310, colour 15, from 0x77a7d8.
- The 18 calls' windows/positions are in `ffe.asm:71796-72058` and reproduced
  by the `--intro` trace; the deviations from the in-progress table are the
  first-credit gate (0.1), D3/D4, the swapped codes D6 and the plain-text loss
  (0.5).
- **The strings contain layout codes, not decoration.** `0xfd XX` expands to
  `0x1e XX` and sets the writer's x to `2*XX`; `0xfe XX YY` expands to
  `0x1f XX YY` and sets x=`2*XX`, y=`YY`; `0x0d` is a newline (y += 10,
  x = line start); `0xff XX YY` recursively expands code `XXYY` (not used by
  these 18 strings). The first credit therefore draws the production line at
  (90,15) and the copyright at (76,146); the last credit draws three aligned
  copyright lines at x=76 and x=136. A plain-text transcription drawn at one
  point is **visibly wrong**. `F1348` is `ffe.asm:83979`; the writer is
  `F1587`/`F1589` (`93822`/`94028`); `F1398` is `85127`.
- `F147` (DOS `0xa1ac`): for start <5 resets RNG to `0x12345678`/`0x89abcdef`,
  sets `[DATA_008619]=0`, `GameTics=0x80000000`, days=0x121cf7,
  `[DATA_008837]=-1`, `[DATA_008808]=0`, then calls `F144(0, module,
  (start<<8)|esi)` for `esi=1..4`, `module=0..20`. `F998` branches on the
  start type: type 1 uses ship 0x17, cash 0x3e8, hold loss 0x10, object state
  byte (`+0x56`) 0x6f, dock slot 0x6e; type 2 uses ship 0x17, cash 0x3e8, hold
  loss 0xc, state 0x60, slot 0x5f; type 3 uses ship 0x26, cash 0x3e8, hold loss
  0x1f, state 0x70, slot 0x70; types 4/5 use ship 0x1f, cash 0x2710, hold loss
  0x10, state 0x6c, slot 0x6a (`ffe.asm:74028-74230`). The port's
  `NEW_GAME_*` constants are type 1 only (and `NEW_GAME_HOLD_LOSS` is F998's
  type 2 value 0xc, not type 1's 0x10).

---

## 4. TODO checklist (shippable slices)

Every item has: **what**, **touch**, **depends**, **accept**, **size**
(S ≤ half a day, M ≤ 2 days, L > 2 days), **unblocks**.

### Slice 0 — already in progress (do not duplicate)

- `crates/ffe-rs/src/front_end.rs`: `MODEL_FIRST/SECOND`, `MODEL_SWITCH`,
  `PRIMARY_BITMAP_*`, `INTRO_END`, `INTRO_LINES`, `IntroTimeline`, `Intro`,
  `VERSION_LINE`.
- `crates/ffe-rs/src/navigation.rs`: `START_SYSTEMS` (`DATA_006820`), `NEW_GAME_*`,
  `NewGameState`, `Navigator::new_game(type)`, `start_clock`/`elapsed`, body
  metadata slot/rows/class/open_air and the pad placement.
- `crates/ffe-rs/src/main.rs`: `Boot`, `--start 1..5|sol`, `--skip-intro`,
  default new game at Ross 154.
- `tools/reference`: `--intro` mode, `exports.inc` aliases, `verify-intro.py`,
  `verify-all.py` entry, `build-smoke.ps1` manifest.
- `crates/ffe-rs/tests/front_end.rs`, `tests/intro_reference.rs`.

**Review before building on it** (found in the 2026-10-24 working tree):

- [ ] **0.1 First credit window is wrong.** `INTRO_LINES[0].first` is 0; the DOS
  and JJFFE code return before 0x8276e (`ffe.asm:71765`; DOS `0x2f3d4`), so the
  first line is visible only from **0x8276e** to 0xfbd3a. The test
  `the_model_bitmap_and_animation_follow_the_original_thresholds` asserts one
  line at tics 0 and must change. **(S)**
- [ ] **0.2 Codes 0x9427 and 0x9428 are swapped.** Journal Entries is **0x9428**
  at x=20,y=100; Representation is **0x9427** at x=140,y=10 (DOS `0x2f612`,
  `0x2f63f`; `ffe.asm:71952`, `71976`). The table's `text`/threshold order is
  right; only `code` is wrong. **(S)**
- [ ] **0.3 Version text and gate are JJFFE's.** Use
  `"English Release 1.02 - 21/04/95"` and `INTRO_INPUT_FROM = 0x719668`
  (DOS `0x2f6a1`). The oracle's version row at 0x8276e is JJFFE-only because
  0x8276e < 0x719668 (D4); add a synthetic DOS-only test at 0x719667 (no
  version) and 0x719668 (version). **(S)**
- [ ] **0.4 Start keys are not gated on `INTRO_END`.** The main loop reads the
  action every frame (`ffe.asm:68443-68452`); the `--state` trace scripts a
  choice at frame 160 and gets a new game at 161. `Intro::key` currently
  returns `None` until 0x7e7b76. Accept 1-5 at any tics after boot. **(S)**
- [ ] **0.5 The strings are not plain text.** The `0xfd`/`0xfe` bytes are
  position codes (§3.3); the plain-text field is only suitable for the oracle's
  comparison, not for drawing. Do not render `line.text` as one string. **(M,
  folded into slice B)**
- [ ] **0.6 The oracle's row count is inconsistent.** `verify-intro.py` expects
  75 lines; the mode emits 7 + 16×4 + 2×2 + 1 = **76** and
  `intro_reference.rs` asserts 76. Fix the script to 76. **(S)**
- [ ] **0.7 `--intro`'s `firsts[0]` is 0x8276e** (correct), so 0.1 is caught as
  soon as the Rust test is run. Run it before trusting the slice. **(S)**

### Slice A — DOS data assets (no rendering)

- [ ] **A1. Cache the intro/SysMenu strings raw.**
  - **What:** add `base:text/intro-credits` = 19 NUL-terminated byte strings
    from DOS file `0xbd361` (18 credits + version at `0xbd951`), preserving
    `0x0d`/`0xfd`/`0xfe` (no plain-text conversion). Optionally add the full
    46-entry `DATA_004894` SysMenu table instead, but resolve its pointers from
    the DOS data; only the 19 intro strings are contiguous.
  - **Touch:** `crates/ffe-tools/src/cache.rs` (asset builder), new
    `AssetKind::IntroStrings`/`SysMenuStrings` in `crates/ffe-content/src/lib.rs`,
    `docs/FORMATS.md`, cache version (v27 or next free), `tools/reference/verify-assets.py`.
  - **Depends:** none.
  - **Accept:** a deterministic asset whose SHA-256 and byte count are pinned;
    a cache test rejects a truncated/missing terminator; the 19 strings equal the
    DOS bytes (including codes).
  - **Size:** S-M. **Unblocks:** B1, B2, C1.
  - **Owner:** approve committing extracted text only as cache data, never in
    Rust source (legal/provenance rule).
- [ ] **A2. Cache the character-width table.**
  - **What:** `base:text/char-widths`, exactly the 256 signed bytes of
    `DATA_007648`, DOS file `0xe9998` (verified byte-for-byte against
    `ffedat.asm:23138`).
  - **Touch:** same as A1; `docs/FORMATS.md`.
  - **Accept:** length 256, byte-for-byte against the DOS file; a synthetic
    wrap test uses it.
  - **Size:** S. **Unblocks:** B1.
- [ ] **A3. Remove the hardcoded credit text from Rust.**
  - **What:** `front_end.rs::INTRO_LINES[].text` is a transcription; replace the
    renderer's source with the A1 asset and keep only codes/thresholds/x/y/
    wrap/colour in Rust. Keep a *test-only* plain form if the oracle needs it,
    generated from the asset (not a second hardcoded copy).
  - **Depends:** A1. **Accept:** `git grep` finds no credit sentence in
    `crates/`; tests build the expected plain text from the asset.
  - **Size:** S-M. **Unblocks:** C1, legal review.

### Slice B — credits text engine

- [ ] **B1. Port `F1348`/`F1398`/`F1587`/`F1589` semantics.**
  - **What:** an expander that maps `0xfd XX→0x1e XX`, `0xfe XX YY→0x1f XX YY`,
    `0xff XX YY→` recursive FF code (at least `F1343`’s families; for the intro
    only `0xfd`/`0xfe` occur), and a writer that: newline `0x0d` → y += 10 and
    x = line start; `0x1e` → x = 2×byte; `0x1f` → x = 2×byte, y = byte;
    `0x03` → wrap to the line start; `0x01` → colour from `DATA_007776[byte]`;
    printable bytes via the cache's main font; shadow at (+1,+1) colour 0;
    clip to 320×200 and stop at x ≥ 316. Wrap via `F1398`’s word algorithm with
    A2’s widths, inserting `0x03`.
  - **Touch:** new module (suggest `crates/ffe-rs/src/intro_text.rs`) or
    `front_end.rs`; reuse `indexed::BitmapFont`; unit tests in
    `crates/ffe-rs/tests/intro_text.rs`.
  - **Depends:** A1, A2, the cache font (`base:font/main`).
  - **Accept:** for every credit string, the sequence of (x,y,colour,text runs)
    after expansion/wrap equals the DOS code: line 0 = (90,15) production +
    (76,146) copyright; Journal Entries = (20,100) then (100,110); last line =
    three blocks at x=76/136 at y=126/136/146; the wrapped paragraph stays
    within 310 px. Add a `--intro-text` oracle that calls `F1399` for each line
    into a 320×200 buffer and hashes it; the Rust renderer must match exactly.
  - **Size:** M. **Unblocks:** C1; also the `0x3022` wrap gap in
    `docs/notes/text.md`.
- [ ] **B2. Verify the writer against the original, not just by eye.**
  - **What:** extend `headless.c` with `--intro-text` (each of the 19 strings at
    its x/y/wrap/colour, plus the shadow) and `--intro-wrap` (the expanded
    bytes after `F1398` for the two wrap lines). Add
    `tools/reference/verify-intro-text.py` and an ignored Rust test.
  - **Depends:** B1. **Accept:** byte-identical wrapped strings and
    pixel-identical 320×200 hashes; tamper each field and reject.
  - **Size:** M. **Unblocks:** C1 acceptance.

### Slice C — intro screen and boot

- [ ] **C1. Draw an intro frame at one `GameTics`.**
  - **What:** a `Screen`/`front_end` method that (a) clears the 320×200 buffer,
    (b) draws model 177/176 through the `F1682`/`F1683` path with animation
    `tics` for 177 and `tics - 0x8276e` for 176,
    (c) draws `base:bitmap/primary/066` at (5,158) when `tics > 0x795cc`,
    (d) draws the credits/version via B1, (e) presents the low palette.
  - **Frame question (do this first):** `F1682`/`F1683` set up the instance from
    the scene object and its model block; the intro object (`DATA_009127`) is
    zeroed BSS. A probe of `ModelViewer::render_scene_with` with identity/zero
    rendered 6-8 shapes for 177 (some far outside the screen: x -7918..7218,
    y 161..7757), a screen-scale 44-shape frame for 176 at 0x181534
    (x -69..145, y 26..172), and **nothing** for 176 with an all-zero frame.
    Determine the real frame/position (and whether the viewer needs the model
    block's display scale) against the original; do not assume identity/zero is
    the logo frame.
  - **Touch:** `crates/ffe-rs/src/screen.rs` (public intro draw + shape draw),
    `front_end.rs`, `main.rs`; tests in `crates/ffe-rs/tests/front_end.rs`.
  - **Depends:** A1/A2/B1, model 176/177 in cache, bitmap 066; the frame probe.
  - **Accept:** screenshots at the tics in §6; at 0x181534 the 176 logo +
    credits are on-screen; the 177 logo/text is on-screen; no `children_error`;
    the bitmap appears at 0x795cd not 0x795cc.
  - **Size:** M (plus the frame probe). **Unblocks:** C2, F2.
  - **Risk:** model 176's children 347..360 read locals 1/2; the original's
    `F1683` leaves them as stale stack (animation.md §1.2). Confirm the logo
    with a DOS screenshot; if it depends on those values, document the
    undefined-stack deviation.
- [ ] **C2. Boot state machine.**
  - **What:** a `Boot::Intro` state before the flight loop. `GameTics` starts at
    **0x400** (ffe.asm:68415), advances by the timer's tics per 20 ms step (994
    at 1×), and drives C1. `V` at `>= 0x719668` latches the version line; `1`-`5`
    at any tics select a start type; at `>= 0x7e7b76` clear/flip/clear, reset
    `GameTics=0`, repeat. On a start key: stop the song, build
    `Navigator::new_game(type)` (or `Boot::Sandbox` for `--start sol`), switch
    to the normal loop. `--skip-intro` bypasses C1/C2 for CI and screenshots.
  - **Touch:** `crates/ffe-rs/src/main.rs` (loop/mode), `front_end.rs` (state),
    `input.rs` if keys are mapped there; adjust `--smoke-frames` tests.
  - **Depends:** C1, in-progress `Navigator::new_game`.
  - **Accept:** a fresh run shows the intro and does not enter the game without
    a key; 1 enters at frame-equivalent state of `--state`; the intro loops at
    0x7e7b76; `--skip-intro` still starts the game; no auto-start in tests.
  - **Size:** M. **Unblocks:** the whole feature; F3.
- [ ] **C3. Palette fidelity.**
  - **What:** reproduce `F961`'s `F1575` calls (`(0,0,0)` for 177, `(0,0,4)` for
    176) and decide whether the intro needs `F1330_DPalBuild`. The static low
    palette may be sufficient because the models have no faces; confirm by
    comparing the rendered colours against the reference.
  - **Touch:** `screen.rs` / a small palette module; `front_end.rs`.
  - **Depends:** C1. **Accept:** palette-index values in the screenshot match
    the reference at the sample tics, or the difference is documented.
  - **Size:** S-M. **Unblocks:** pixel acceptance.
- [ ] **C4. First in-game frame.**
  - **What:** decide what the player sees at the moment the intro exits. The
    port currently shows the market/docked screen; the original draws its own
    first docked frame (the reference's `--game-frame` is the evidence). Either
    port that frame or record an approved presentation deviation.
  - **Depends:** C2. **Accept:** screenshot/state at frame 161 + 1, compared to
    the reference's `--game-frame`.
  - **Size:** M-L (may be gameplay-slice work). **Unblocks:** a clean intro→game
    acceptance.

### Slice D — new game (parallel stream, review/extend)

- [ ] **D1. DOS RNG seed at `F147`.** Use `0x12345678`/`0x89abcdef`, not
  JJFFE's `0x456789ab`; if the port seeds the gameplay RNG at all. Add a
  comment naming the DOS source. **(S)**
- [ ] **D2. Start types 2-5 constants.** Port `F998`'s branches: cash, ship,
  hold loss, player state/class, dock slot and system. The current
  `NEW_GAME_*` constants are type 1 only, and `NEW_GAME_HOLD_LOSS = 0xc` is
  F998's **type 2** subtraction; type 1 subtracts 0x10. Fix type 1 first, then
  the others. **(M)**
- [ ] **D3. Verify types 2-5 in the reference.** Extend the headless adapter's
  state mode with a start selector (scan codes 0x02..0x06 for 1..5), record
  frame-161 invariants for each, and add a Rust test. Do not claim support from
  the `DATA_006820` table alone. **(M)**
- [ ] **D4. Module init scope.** `F147` calls `F144(0,0..20,(type<<8)|1..4)`;
  the port sets state directly. List which modules' init is observable in the
  first frame and either port them or record the deviation. **(M)**
- [ ] **D5. Date/clock.** The in-progress `start_clock`/`elapsed`/absolute-date
  fix must keep the sandbox at 0 and the new game at 0x121cf7.5 days; test both.
  **(S)**

### Slice E — music

- [ ] **E1. Cache the HMP losslessly.** `base:music/qqfront` (and the other 16)
  from `MUSIC/*.HMP`, byte-for-byte with the SHA-256s in `docs/GAME_FILES.md`;
  cache v27+, `docs/FORMATS.md`, `verify-assets.py`. **(S-M)**
- [ ] **E2. HMP→SMF.** Port `jjffe-master/hmp.cpp` semantics deterministically
  (or implement the HMP reader directly); test by re-parsing the SMF and
  comparing events. **(M)**
- [ ] **E3. Playback backend.** Choose a MIDI synth/OS MIDI/`rustysynth` +
  a freely licensed SoundFont; do **not** ship the game's `.BNK`. **(L,
  blocked on owner)**.
- [ ] **E4. Boot behaviour.** Song 0 at the intro start, restart on
  `F1904` done, stop on the start key (`ffe.asm:68419-68428`, `68483`). Do not
  play `INTRO.HMP`: it is song 10 in the same 17-name table (the video/message
  path uses it), while the boot calls `F1902(0)`. **(S after E3)**

### Slice F — verification, polish, docs

- [ ] **F1. Commit the oracles.** Fix D6/D4/row count, add `verify-intro.py` to
  `verify-all.py`, record the trace SHA in `docs/PORT_LOG.md`/`STATUS.md`. **(S)**
- [ ] **F2. Add an intro screenshot mode.** `--intro-frame <tics>` in
  `headless.c` (real `F961`+`F962`+`F1330`+flip, no draw hook, 320×200 pixels +
  palette) and `--capture-intro` in `ffe-rs`; compare bytes or FNV. Exclude or
  document model 177's `FRONTIER`/`GAME!TEK` difference (D5). **(M)**
- [ ] **F3. Smoke/CI.** `--skip-intro` is the default for `--smoke-frames` and
  capture tests; the full-intro path gets its own bounded test. **(S)**
- [ ] **F4. Docs.** Create `docs/notes/intro.md` (feature record), append
  `docs/PORT_LOG.md`, add a `docs/STATUS.md` milestone row, and record any
  deviation in `docs/DEVIATIONS.md` (JJFFE splash excluded, types 2-5 if
  refused, first in-game frame if changed, music if deferred). Update
  `dashboard/burndown.js` items `ui-7`, `audio-3`, `content-3` in the same
  commit (not in this note). **(S)**
- [ ] **F5. Format/clippy/tests.** `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`. **(S)**

---

## 5. Missing pieces another agent can pick up now

Prioritised; each is startable without re-reading this whole note.

1. **Fix the in-progress timeline (S).**
   `crates/ffe-rs/src/front_end.rs` + `crates/ffe-rs/tests/front_end.rs`:
   first credit 0x8276e; swap 0x9427/0x9428; DOS version text/gate; start keys
   not gated on `INTRO_END`; `verify-intro.py` 76 rows. Run
   `python tools/reference/verify-intro.py --reference <jjffe-smoke>` afterwards
   and add the DOS-only version-boundary test at 0x719667/0x719668 (D4).
2. **Cache the DOS strings and widths (S-M).**
   `game/FIRSTENC.EXE` file `0xbd361` (19 strings) and `0xe9998` (256 bytes);
   `crates/ffe-tools/src/cache.rs`, `crates/ffe-content/src/lib.rs`,
   `docs/FORMATS.md`, `tools/reference/verify-assets.py`. Extract with a small
   Python/Rust script and pin SHA-256s; do not commit the bytes in source.
3. **Write the text engine (M).**
   New `crates/ffe-rs/src/intro_text.rs` implementing §3.3 and B1; unit tests
   for line 0's (90,15)+(76,146), Journal's (100,110), the last three blocks,
   and the 310-px wrap. Add `--intro-text`/`--intro-wrap` oracles.
4. **Draw the intro screen (M).**
   `crates/ffe-rs/src/screen.rs`: a public frame path that clears, draws the
   model through the `F1682`/`F1683` frame determined in C1 (probe first: the
   identity/zero frame is not proven to place model 177 on screen), blits
   `base:bitmap/primary/066` at (5,158), draws the text, presents. Screenshot
   tics in §6.
5. **Wire the boot (M).**
   `main.rs`: `Boot::Intro` with `GameTics=0x400`, timer steps, V/1-5, 0x7e7b76
   restart, `Navigator::new_game(type)` on choice; `--skip-intro` for CI.
6. **DOS RNG + start types (S-M).**
   `0x89abcdef`; `F998` branches for 2-5; extend the state adapter with a start
   selector and record frame-161 invariants. Do not claim 2-5 from the system
   table alone.
7. **Music (L, blocked).**
   Cache `MUSIC/QQFRONT.HMP` first (safe, useful), then convert HMP→SMF and
   pick a backend. Keep the intro silent only with an explicit deviation.
8. **Pixel oracle (M).**
   `--intro-frame` + `--capture-intro`; beware D5 in any JJFFE-sourced logo
   reference.

Quick data extraction recipe (not committed):

```python
from pathlib import Path
b = Path("game/FIRSTENC.EXE").read_bytes()
credits = b[0xbd361:0xbd951].split(b"\0")           # 18 credits (last split empty)
assert len(credits) == 19 and credits[-1] == b""
version = b[0xbd951:0xbd980].split(b"\0")[0]        # "English Release 1.02 - 21/04/95"
widths = b[0xe9998:0xe9998 + 256]                   # DATA_007648
assert len(widths) == 256 and version.startswith(b"English Release 1.02")
```

The version starts at `0xbd951`; do not assume the end of the 18th credit is
enough (the block needs a 0xbd980 read bound).

---

## 6. Verification plan

### 6.1 The `--intro` data oracle

- Mode: `jjffe-smoke --intro` (in progress), prints 76 JSONL rows:
  7 boundary rows (`0`, 497000, 0x795cc, 0x795cc+1, 0x8276e-1, 0x8276e,
  0x7e7b75), 68 rows around the 18 credit windows (first-1, first, end-1, end;
  two rows for the unbounded lines), and one version row.
- Rust: `crates/ffe-rs/tests/intro_reference.rs` (`FFE_INTRO_VECTORS`),
  `#[ignore]`, exact count.
- `tools/reference/verify-intro.py`: run the reference twice, compare bytes,
  run the Rust test, tamper each field (`tics`, `model`, `animation`, `version`,
  text x/y/wrap/code/colour/string), reject empty; print the trace SHA.
- **DOS caveat:** the version row at 0x8276e is JJFFE only (D4; 0x8276e <
  0x719668). Keep it as a separately named JJFFE case, or move it to a tic
  >= 0x719668 (which both engines accept). Add a DOS-only synthetic test at
  0x719667 (no version) and 0x719668 (version); JJFFE cannot verify the boundary
  because it has no early gate. The DOS string (D3) and codes (D6) must be
  asserted from DOS data, not from the JJFFE trace.
- Row count must be 76, not `verify-intro.py`'s current 75.

### 6.2 The `--state` new-game oracle

- Mode: `jjffe-smoke --state` (400 frames; scripts Y at frame 2 and choice 1 at
  frame 160; synthetic 1 ms clock).
- `crates/ffe-tools/src/reference.rs::verify_state` / `verify-reference` checks:
  row 0 zeroed; at frame 161 and later `start_type=1`, `game_tics=0x80000000`,
  `game_days=0x121cf7`, `player_state=0x30`, `player_index=107`, `cash=1000`,
  `time_accel=994`; 238 in-game clock transitions; no return to the intro;
  framebuffer/palette activity; both runs byte-identical.
- Add a Rust test for `Navigator::new_game(1)` that asserts the same fields
  (`NEW_GAME_*`, date 3250-01-01 12:00, Ross 154, Eagle (model 23), 1000 units).
- **RNG caveat (D2):** the trace's `rng_a`/`rng_b` are JJFFE's
  (`0x456789ab` seed). Do not assert them as DOS; if you port `F147`'s seed, use
  the DOS value and document the trace difference.

### 6.3 Screenshots / frames

Sample tics (fixed, no wall clock): `0x400`, `0x795cc`, `0x795cd`, `0x8276e`,
`0xfbd3a`, `0x181534`, `0x77a7d8`, `0x7e7b75`, `0x7e7b76`. For each, capture
320×200 palette indices and palette RGB. Compare against the reference where
possible; for model 177 expect the D5 word difference if the reference is JJFFE.
Also capture the first in-game frame after the choice (C4).

### 6.4 What cannot be verified locally

- **Executing DOS 1.02.** The reference is JJFFE; DOS evidence is disassembly
  and byte comparison only. D2/D3/D4/D5 are read from the DOS binary, not
  executed.
- **CD 1.01 boot** and the 323 AVIs (out of scope).
- **Music playback** (no audio capture; HMP hashes are known, synthesis is not).
- **Start types 2-5** until the state adapter is extended and a DOS/JJFFE
  comparison is made.
- **Model 176's stale-stack locals** (animation.md §1.2) and any behaviour that
  depends on undefined original memory.
- **The dynamic upper palette** if the intro turns out to need `F1330`; the
  current port has only the fixed low palette.

---

## 7. Risks / unknowns / owner decisions

| Risk or decision | Why it matters | Suggested default |
|---|---|---|
| **JJFFE vs DOS** | The in-progress slice uses JJFFE's version string, V gate and RNG seed. | DOS wins; fix D2-D4/D6; keep the JJFFE oracle only for the geometry it shares. |
| **JJFFE model-string `FRONTIER`** | Any JJFFE pixel reference of model 177 differs from the DOS cache (`GAME!TEK`). | Port the DOS cache; exclude/document the word in pixel oracles. |
| **No music backend** | The DOS intro plays `qqfront`; silence is not "fully complete". | Owner chooses MIDI synth/OS MIDI/`rustysynth` + free SoundFont, or approves a silent deviation. |
| **Start types 2-5** | The original starts them; they differ in ship/cash/hold/state/slot. | Implement/verify them, or record a deviation that only type 1 is offered (do not silently map). |
| **Default boot** | The original always shows the intro; the port currently starts the game. | Default intro; `--skip-intro` for dev/CI; `--start sol` for the sandbox. |
| **Esc/skip** | The original has no skip key. | Do not add one to the default path; a dev flag is fine. |
| **First in-game frame** | The port shows the market; the original draws its own first docked frame. | Port it or record a presentation deviation. |
| **Hardcoded credits** | Committing the original text in Rust violates the content rule. | Extract to `data.bin`; keep only codes/positions in source. |
| **`CLAUDE.MD` says JJFFE is selected** | Contradicts the 2026-10-24 decision. | Reconcile when the feature lands. |
| **`--intro` 75 vs 76** | The in-progress script and Rust test disagree. | 76; fix the script. |

Open questions for the owner:

1. Music: which backend, and may the HMP content be cached losslessly and
   converted at runtime?
2. Start types 2-5: implement all, or only type 1 with a recorded deviation?
   If only type 1, what should keys 2-5 do (current slice maps them to 1 with a
   notice)?
3. Is the intro allowed to be skipped in the normal build (e.g. Esc) or only in
   dev/CI?
4. Should the first in-game frame be the original docked frame (more work)
   or is the current docked/market view an accepted presentation deviation?
5. Confirm the CD AVI/video and the JJFFE splash stay out permanently.
6. Confirm the DOS RNG seed `0x89abcdef` is authoritative and that the
   `--state` trace's JJFFE RNG values are not to be matched.

---

## 8. Source index (for the implementer)

- Boot/front end: `jjffe-master/ffe.asm:68269-68520` (`_asmmain`, splash,
  `GameTics=0x400`, key loop, `F147` call), `71624-72058` (`F960/F961/F962`),
  `14912-14965` (`F147`), `73601` (`F994`), `74028-74230` (`F998`),
  `74271` (`F1000`), `93539` (`F1575`), `82962` (`F1330`), `102201` (`F1682`).
- Text: `83979` (`F1348`), `85096`/`85127` (`F1397`/`F1398`), `93822`
  (`F1587`), `94028` (`F1589`), `94090` (`F1598`), `97104`/`97118` (bitmap).
- Music: `132560` (`F1902`), `jjffe-master/win32/w32sound.cpp:79-96` (song
  names), `MUSIC/QQFRONT.HMP`.
- DOS data: `game/FIRSTENC.EXE` offsets in §3 (F961 0x2f298, F962 0x2f380,
  F147 0xa1ac, credits 0xbd361, version 0xbd951, GAME!TEK 0xbbb32, widths
  0xe9998, songs 0xefd10, V gate 0x2f6a1).
- Rust: `crates/ffe-rs/src/front_end.rs` (in progress), `navigation.rs`
  (`NewGameState`/`new_game`), `main.rs` (`Boot`), `model_viewer.rs`
  (`render_scene_with`), `screen.rs`, `test/front_end.rs`,
  `tests/intro_reference.rs`; `crates/ffe-tools/src/cache.rs`,
  `crates/ffe-content/src/lib.rs`.
- Docs: `docs/STATUS.md`, `docs/FORMATS.md`, `docs/REFERENCE_HARNESS.md`,
  `docs/notes/animation.md` (model inputs), `docs/notes/text.md` (F1344/F1747),
  `docs/GAME_FILES.md` (HMP hashes), `docs/INVENTORY.md` §3, `docs/LEGAL.md`.
