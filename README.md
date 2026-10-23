# ffe-rs

A native Rust reimplementation of **Frontier: First Encounters** (Elite 3),
using **JJFFE 2.8a7** as the gameplay reference and FFED3D as rendering research.
The goal is original-style graphics by default with optional Enhanced graphics;
the current experimental example is not that faithful renderer.

> **Status: experimental Windows x64 playable example, plus verified asset/core tools.**
> Launch, fly, dock, trade and save/reload now work in a native Rust window.
> This is **not yet faithful First Encounters**: flight mechanics, the starfield
> and one-item economy are synthetic development scaffolding.
> Original cabin, proportional bitmap font and console artwork are imported.
> Version 0.4 extends the **F4 station model lab** with original declared
> triangle/quad outlines and individual-face inspection, not finished models.
> The **F6 model viewer** (cache v14) runs every imported ship, station and object
> program through the verified model executor and draws its faces, textured faces
> (original bitmaps), complex polygons, lines, points and curves in the original
> render order, including child models (station rings, ship parts). Curved
> surfaces, text and labels are not drawn yet.
> In flight, the station is the original **model 74**, executed by the verified
> model port with its textures; flight mechanics remain synthetic. No legacy runtime is linked.
> **449 of the 466 original model programs** now execute natively and match the
> original command for command; the rest are the slots the original itself
> cannot run.
> Complete content, JJFFE-equivalent gameplay and graphics packs remain unfinished.
> [CLAUDE.MD](CLAUDE.MD) records the owner's answers and current plan.

## Play the experimental example

The Windows executable is built in this checkout. From the repository root:

```powershell
.\tools\play.ps1
# Equivalent:
.\target\release\ffe-rs.exe --root .
```

**Easiest on Windows:** double-click `play.bat`. It builds the release game and
tools, creates or updates `data.bin` from your files in `.\game` when it is
missing or older than this build needs (currently cache v14), and launches the
game; on any error the window stays open with the message. Rebuilding by hand:
`cargo build --release --locked -p ffe-rs -p ffe-tools`.
The root directory must contain your locally generated **v14** `data.bin` (build command
below, or let `play.bat` make it). The game verifies it before opening a window.
Without `--root`, it looks beside its own executable and then up to four parent
folders, so `target\release\ffe-rs.exe` finds the checkout's `data.bin`. If
startup fails, the console waits for Enter so the error can be read. No original
assets, reference source or x86 executable are linked into the native game
binary. Do not distribute the cache or captures.

Build ID: **0.9 experimental**. Resizable 1280x960 initial window. Rendering
uses a **320x200 logical canvas**, with 320x158 view plus console starting at
row 158, nearest sampling and 4:3 non-square-pixel correction (640x480 output).
The original 116-glyph proportional font, fixed low palette and console
bitmap (primary slot 43) are imported from your executable. Cabin0 remains
the docked backdrop with its own original palette. The diagnostic labels,
scanner contact, synthetic geometry and gameplay remain development scaffolding;
this is not full Classic fidelity. Console artwork is not a clickable original UI.

**This checkout's cache is already upgraded**, with previous generated caches
retained privately at `.cache/data-v1-backup.bin` through
`.cache/data-v5-backup.bin`. If updating another
checkout, build a new cache to a fresh path, inspect it, keep a backup of your
previous generated cache, then rename the new file to `data.bin`. Old v1 caches
remain readable by `ffe-tools`, but the game requires the v2 font and reports
a clear rebuild error rather than silently substituting a different font.
Existing experimental save snapshots and bindings are unchanged.
V2 still runs the flight loop; F4 reports a required v3 rebuild rather than
silently substituting invented model data.
V3 retains its existing points-only lab; v4 adds drawing records and face views.

| Action | Binding |
|---|---|
| Help / named reset scenarios / input preset | H / F2 / F3 |
| Views (as the original): cycle / front / rear / turret / external; or click the console view icons (any window size) | F1 / F6 / F7 / F8 / F9 |
| External view: turn camera; zoom out / in (both of a pair resets) | arrows; + / - |
| Change ship (the original's named craft) | Page Up / Page Down |
| Original flight model on / off (set speed, original steering and physics) | O |
| Original flight: raise / lower the set speed | Enter / Shift |
| Original flight: pitch, yaw, roll | Z A, `,` `.`, `[` `]` |
| Time acceleration 1x / 10x / 100x / 1000x / 10000x (original model) | 6 7 8 9 0, or click the meter at the lower left |
| Pause mark on the meter | click it (or P) |
| Station model lab (pauses gameplay) / return | F4 |
| Model viewer, every imported model (pauses gameplay) / return | M |
| Viewer: previous / next model; turn; zoom; test input | Page Up / Page Down; arrows; + / -; 0 / 5 / 9 |
| Viewer: original animation clock / landing gear or door / time rate / engines off, half, full | C / G / T / E |
| Launch / request or cancel docking | L / D |
| Classic-like thrust / reverse; yaw; pitch | Enter / Left Shift; comma / period; A / Z |
| Modern thrust / reverse; yaw; pitch | W / S; Left / Right; Up / Down |
| Brake / pause / quit | X / P / Escape |
| Buy / sell one unit of food (docked only) | B / N |
| New save snapshot / load newest snapshot | F5 / Shift+F5 |

Modern input (W/S and arrows) is the initial preset on every launch. F3 switches
to the optional Classic-like preset; these are **not yet verified original
bindings** and remapping/settings persistence are not implemented. F3 changes
input only. Help, scenario menus, the model lab, pause and focus loss stop the simulation;
refocus and press controls afresh. No audio, combat, planets, hyperspace,
time acceleration, old-save import or Enhanced/HD-pack toggle is implemented.

### Current work: point/line emission and composed-order oracle (v0.13)

V0.13 decodes and emits point (opcode 1) and line (opcode 2) operands. The local
command probe now captures the original F1699/F1703 arguments;
`verify-point-line.py` matches point material/angle/vertex/flag and line
material/vertices (trace `afa955be…`, six tamper fields + empty trace rejected).
`Execution.ordered` is now a single `Primitive` stream (`Face`/`Point`/`Line`)
with aligned `ordered_keys`. This is operand emission, not projection, clipping
or pixels. See `docs/MODEL_COMMANDS.md`.

V0.12 adds a composed-order oracle: the local adapter drives the original
F1696/F1777/F1778/F1780/F1788 with marker callbacks, and Rust
`Execution.ordered_keys` matches order `[2,3,1]`, keys `[9,9,0]`
(trace `ce0d1b4f…`). It is a helper composition, not a dispatcher, face-handler
or pixel oracle. See `docs/MODEL_COMMANDS.md` for the full opcode map and what
text/submodel still requires.

V0.11 composes the verified depth commands and group ordering into the bounded
executor. `ffe-core::model_execute::execute_live` resolves depth vertices through
the live `model_instance` slots, opens/closes F1780 groups, and returns a
render-ordered `Execution::ordered` stream while keeping command-order
`emissions`. Errors leave context, vertex slots and queue unchanged. The real
station streams now advance past opcode 6 and stop at the next unsupported
command (model 74 at `0x000a`/`0xf4`, model 75 at `0x1c6e`/`0x1c8`). This is still
operand emission, not geometry evaluation, shading or pixels; text, submodels,
other transform forms and colour state remain next. Cache v6 does not
need rebuilding; F4 and flight are unchanged.

V0.9 adds original depth-command resolution and abstract depth-group ordering.
**11,700 depth-command cases** match original operand consumption, keys and
group transitions; **128 synthetic 64-operation sequences** match the original
queue allocator/group dispatcher/sorter. This includes empty/replaced groups,
unsigned descending sort keys and earlier-allocation ties. List minimum/maximum
uses signed comparisons, unlike the final sorter.

These remain bounded core helpers; v0.10 adds the live view-space slots and
v0.11 composes depth commands/groups into `execute_live`, which resolves points
lazily rather than requiring pre-evaluated static input. Nested submodel queues
and game primitive allocation/rasterization remain unimplemented. F4 and flight
visuals are unchanged; Modern input remains the default and cache v6 does not
need rebuilding.

V0.8 makes Modern input the default, retaining F3 for Classic-like input. Held
controls must be released and pressed afresh after pause, overlays, focus loss
or preset changes; input latched while an overlay is open cannot leak into flight.
The real-window regression checks W/S, all four arrow directions, both presets,
held thrust across lab/pause exit and the existing gameplay/save loop.

The original F1777 integer render-order depth key now matches **17,303 original
results**, including signed/wrapping magnitudes, masked shifts and overflow
encodings. This is preparation for depth-order blocks, not a z-buffer, sorting
implementation or new visible model rendering. The cache stays v6.

V0.7 composes the verified helpers into a bounded command executor: returns,
arithmetic, bit/visibility branches, pure local-basis rotations and ordinary/
reflected triangle/quad operand emissions. Three synthetic programs match the
original dispatcher in **900 contexts**, including loops and emission-time state.
Errors discard partial results and preserve the input context; command budgets
prevent infinite loops. This is core progress, not a new F4 rendering mode.
The real station programs still require point/text, submodels and other commands;
they fail explicitly at the next unsupported command rather than being rendered
incompletely.

V0.6 adds the original integer sine table, exact quadrant lookup, Q31 local-basis
rotations, packed-angle operand decoding, pure rotation commands and the remaining
two arithmetic operations. These are verified core
helpers, not a complete command executor; the visible lab remains the v0.5 views.
Normal/LOD visibility predicates and conditional branch outcomes are now verified
as well, including detail selection, overflow checks and signed branch offsets.

Press **F4** to inspect the original vertex programs for model **74 (0x4a)** and
**75 (0x4b)**, with 54 and 88 vertices. **1/2** selects the model, **arrows**
rotate the inspection view, and **0/5/9** supplies diagnostic variable values
0/32768/65535. **Tab** cycles declared-face wire outlines, one-face inspection
and the previous vertex view, then a normal-filtered outline view. **Comma/period** selects a face; its command offset,
raw material and normal index are displayed. Green outlines are reflected
emissions; in point mode green means computed vertices.
F4 returns to flight without changing your ship, economy or snapshots.

This is deliberately a **declared-face catalogue**, not the station renderer:
it draws exact triangle/quad perimeters from the physical command records,
including records in alternate branches. It does not execute those branches or
claim all these faces would be visible together. Text, point sources, submodel
calls and render-order blocks are recognized structurally, not rendered or
silently treated as supported execution. Unknown record forms fail explicitly.
No invented hull or captured mesh is used.
The native integer interpreter handles signed/mirrored coordinates, Q31 transforms,
dependency resolution, midpoint arithmetic and variable-driven vertices. The
inspection view uses its own orthographic camera and test variable values, not
the game's projection or actual station animation state. Native arithmetic,
bit-branch and material-descriptor helpers are differentially checked, but are
not yet wired into a full command executor. V0.5 imports both original normal
blocks and evaluates their mirrored visibility and lighting with exact integer
semantics. The fourth view filters the declared faces by these visibility flags
using a diagnostic observer derived from the lab camera; it is still not original
projection, branch execution or shaded rendering. V3/v4 caches retain their
previous views. Transforms, submodels, runtime materials and rasterization remain next.
The gameplay cube stays labelled synthetic until those are implemented.

### Short owner playtest (about 3 minutes)

1. **Start and trade:** run the command above; F1 closes Help. Press B:
   credits **1000 -> 990**, cargo **0 -> 1**. Press N: **999**, cargo **0**.
   Look for the original proportional text and grey/red console at the bottom.
2. **Launch and fly:** L launches. Briefly hold W (about 0.4 seconds) to
   reach roughly **12 units/s**; release to coast. Try the arrow keys,
   then X to brake. F3 selects Classic-like controls; F1 lists both presets.
3. **Easy docking check:** F2, then **2** resets to **Approach**. Press D and
   do not thrust/turn: it docks in about **20 seconds**. This reset discards
   unsaved progress. The real launch route is also flyable: from L, face the
   station, request docking and coast below 20 units/s until within 80 units.
   Alignment must be within about 18 degrees; unsafe close approaches reset
   the ship with an explicit warning, not damage/death.
4. **Persistence:** while docked, B buys one; F5 saves. Quit, launch again,
   F1 closes Help, F9 restores the saved credits/cargo. F2 then 1/2/3 resets
   the live scenario, never deletes saves.

Saves are immutable numbered JSON snapshots under local `saves/`. F9 loads the
highest number. Invalid/newer-version saves report an error without replacing
live state. Originals are never read as saves or overwritten. Prototype saves
are a separate format, not a promise of compatibility with the eventual game.
There is no autosave or quit confirmation; use F5 before leaving.

Optional **implementer** Windows input test (opens/closes temporary game windows,
needs the desktop, takes about a minute, cleans its isolated cache/save copies):

```powershell
.\tools\test-playable.ps1
```

This exercises actual window input, model-lab pausing/action/save isolation,
trade, launch/thrust/docking, both presets,
braking/pause, corrupt-load rejection and persistence across a new process.
Keep the desktop available: it explicitly acquires/verifies both foreground and
the target thread's active/focused window, and fails if Windows refuses focus.
The coast check polls actual saved docking state rather than assuming 40 seconds
of wall time equals 40 seconds of simulation across focus pauses.
It is separate from the technical milestone checks below. Automated simulation
tests do not establish JJFFE equivalence; the faithful phase-3 exit gate remains open.

## Run the implemented inspection tool

Install stable Rust with the Windows MSVC build prerequisites. From this
repository root:

```powershell
cargo run --locked -p ffe-tools -- inspect-game --game .\game
cargo run --locked -p ffe-tools -- inspect-game --game .\game --json

# Build and run the standalone Windows x64 tool:
cargo build --release --locked -p ffe-tools
.\target\release\ffe-tools.exe inspect-game --game .\game
```

On this machine Rust is installed under `%USERPROFILE%\.cargo\bin` but that folder
is not currently on the shell PATH. If `cargo` is not recognized, use
`& "$env:USERPROFILE\.cargo\bin\cargo.exe"` in place of `cargo` above.

This command is read-only: it does not execute, patch, convert or copy game files.
It accepts an installed/extracted directory, not BIN/CUE images or patch packages.
Without `--game`, it looks for `game/` beside **the tool executable**, not the shell
working directory. Use an explicit path when running through Cargo.

- Recognizes the verified English floppy 1.02 and CD 1.01 executables by exact
  size **and SHA-256**, not their embedded version strings.
- Fingerprints 48 required files; verifies original journal/cabin hashes,
  required effects/music presence, PCM byte alignment and HMP signatures.
- Counts optional AVI/BMP files under `DATA/`. Does not decode media or fully
  validate HMP/PCM content. Audio hash equivalence is not yet enforced.
- Prints actionable missing/corrupt/unknown-version diagnostics. Unknown patch
  outputs are not accepted merely because the installer has a familiar name.
- JSON has `report_version`, `source_profile`, `inspection_passed`,
  `conversion_implemented`, sorted file records, media counts and diagnostics.
  `conversion_implemented` is currently always false.
- Exit codes: **0** inspection passed/help, **1** installation has errors,
  **2** command-line or fatal directory/output failure. Per-file read failures
  appear in the report and cause exit 1.

Locally validated against both supplied floppy installations and the actual CD's
required files in a temporary test directory. Neither this check nor a recognized
profile proves gameplay or complete conversion support.

## Export the external images and effects

```powershell
# Destination parent must exist; destination itself must NOT exist:
cargo run --locked -p ffe-tools -- export-external --game .\game --out .\extracted
```

The local [extracted/](extracted/) directory has been generated on this checkout.
Open [index.html](extracted/index.html) for the six cockpit previews and
[sizes.csv](extracted/sizes.csv) for per-image UHD recommendations.
These local files are ignored and must not be committed or distributed.

- Original-size 320x158 RGBA PNGs in `images/`; lossless indexed PNGs in `classic/`
  and original 8-bit RGB palettes in `palettes/`.
- For a corrected 4:3 canvas on a 3840x2160 display, the six cockpit images
  recommend **3520x1738 (11x)**. No resizing is performed. This is input guidance
  for the owner, not yet an accepted replacement-pack format.
- All 23 signed 16-bit, mono 22050 Hz RAW effects are wrapped losslessly as WAV;
  `NOWT.RAW` is an empty sentinel.
- `manifest.json` records partial scope, stable image IDs, all 48 inspected
  source fingerprints, output hashes, dimensions and exclusions. It is written
  last as the completion marker.
- Refuses existing destinations and destinations inside the game directory.
  Export failures exit 2 with a diagnostic; interrupted output is left explicitly
  incomplete and must not be reused. Originals are not modified.

This is **not the complete upscale exporter**: embedded executable images,
fonts, models, journal decoding, music and optional CD media are not converted.
It does not produce `data.bin` or `upscaled.bin`.

## Build and verify the partial content cache

```powershell
# Must be a NEW file outside the player's game folder:
cargo run --locked -p ffe-tools -- build-data --game .\game --out .\data.bin
cargo run --locked -p ffe-tools -- inspect-data --data .\data.bin
cargo run --locked -p ffe-tools -- inspect-data --data .\data.bin --json
```

**Cache v14** imports every model that decodes (462 of 466 on floppy 1.02) with
its descriptor fields for child models, the original texture descriptor table and
fixed-colour map, the original lighting tables (default light, shade tables), the
original ship statistics, the full original string table (219 entries: model
and ship names plus the markings the text and label commands draw) and the three
font models' per-character glyph programs the 3D text runs.
`build-data` refuses an existing destination, so build to a new file,
then replace your old `data.bin` with it:

```powershell
.\target\release\ffe-tools.exe build-data --game .\game --out .\data-v14.bin
Move-Item -Force .\data-v14.bin .\data.bin
```

The local cache has been generated on this checkout. It is ignored and must
not be distributed. Unlike the reference smoke executable, the Rust importer
does not need JJFFE sources, NASM, or execution of the original game.

The versioned, deterministic ZIP now contains **1782 assets (cache v15)**:

- 345 embedded indexed images, with original dimensions, signed drawing offsets,
  mask-zero flags, table/slot IDs and executable source offsets.
- Two 32768-byte texture blocks from primary slots 94/95. Their headers encode
  zero height; they remain raw blocks, not guessed 64x512 images.
- The original six-bit fixed low 128-colour palette. The upper 128 colours are
  assigned at runtime; no inferred full-colour embedded previews are generated.
- The original 12-bit fixed colour map, the texture descriptor table and the
  lighting tables (the default light, the shade table and the six star tables).
- Six lossless indexed cabin PNGs, preserving their separate 8-bit palettes,
  and 23 lossless WAV effects.
- The main original bitmap font: 116 records, each nine MSB-first row masks
  plus one proportional advance byte. No copied font data is compiled into Rust.
- The vertex programs, drawing-command streams and normal blocks of **462 of the
  466 model table slots**, with their source descriptor/record offsets, original
  scale and counts, plus the descriptor fields the child-model calls need.
  These are original programs, not static meshes or captured geometry.
- The 16,384-entry signed integer sine quarter-wave table (65,536 bytes), imported
  byte-exactly from the player's executable. No floating-point recreation.

Both exact-hash source profiles are supported. The two JJFFE bitmap tables have
96 and 251 slots, not the 96/261 counts from the historical FFED3D study.
Manifest provenance includes all 48 inspected inputs and every cached asset hash.
The Rust reader validates format/version, paths, IDs, inventory, bounded sizes,
ZIP CRCs, SHA-256 and basic typed payload shapes before returning asset bytes.
Hashes detect corruption, **not authenticity** of a third-party pack; this is
local base content only. `inspect-data` does not compare against the current
game folder or automatically rebuild stale caches.

V1 (377 assets), v2 (378) and v3 (380) remain readable. V4 is a deliberate schema
bump: older tools reject it, rather than misinterpreting the new command kind.
V7 to V14 add model, texture, lighting, ship-statistics, string-table and glyph-program assets;
except for v4 the format did not change again, so a rebuilt cache from the same
files holds more models than an older one. V11 and v12 held the first 209 model
strings (JJFFE's declared count); **v13 scans the executable's table to its real
end (219)** so every text and label code can resolve.

This is **not a complete game cache**: the four model slots that do not decode,
the gameplay tables, additional fonts and text control anchors, the journal,
music and optional CD media remain unconverted. Scope is explicit in
the manifest (`complete_game_assets: false`). Automatic import/freshness checking/rebuild,
the complete upscale export and replacement-pack loading are still planned.
All build/inspection failures exit 2; existing outputs are never overwritten.

Optional local real-data verification against the supplied JJFFE declarations:

```powershell
python .\tools\reference\verify-assets.py --tool .\target\release\ffe-tools.exe --game .\game --reference .\jjffe-master
```

This checks every table payload against the original executable and JJFFE source,
cabins and audio losslessly, repeated-build equality, corruption rejection,
no-overwrite and unchanged source files, in auto-cleaned temporary directories.
It is not part of public CI, which uses synthetic fixtures only.

## Build the local JJFFE reference smoke test (Windows)

Requires the supplied `jjffe-master`, NASM and MSVC **x86** tools/Windows SDK.
NASM is not bundled; supply its executable path. `-VcVars` can override the
default Visual Studio 18 Community `vcvars32.bat` path.

```powershell
.\tools\reference\build-smoke.ps1 -Nasm .\.cache\tools\nasm-3.01\nasm.exe
python .\tools\reference\verify-smoke.py --exe .\.cache\jjffe-reference\jjffe-smoke.exe
```

The build leaves legacy sources unchanged and writes only under ignored
`.cache/jjffe-reference/`, including a source/tool hash manifest. Existing
orphan-label and deprecated-C warnings come from the original sources.
The linked executable contains recovered game data: **never distribute it**.

Verification runs twice in disposable working directories and checks matching
64-frame JSONL traces, changing framebuffers, advancing synthetic time and
video blits. This is a **headless startup smoke test**, not a simulation oracle:
synthetic clock, one scripted acceptance key, no audio/device/video decoding,
no save/load and no flight/docking/mission validation. Nothing is shown onscreen.

## New milestone: native core/reference checks

`ffe-core` now implements the two-word gameplay RNG, timer-counter arithmetic
and day/fraction carry, with no legacy assembly, I/O, platform or rendering
dependency. This is the beginning of simulation, not a complete flight model.

The same local reference executable has two additional modes:

- `--vectors`: calls the **executed JJFFE** RNG routine 1536 times across six
  seed scenarios and its timer routine for 60 elapsed/acceleration cases,
  including pause, fractional truncation and wrapping counter additions.
- `--state 400`: runs the original game normally with scripted acceptance Y
  and intro choice 1, then samples selected numeric state, framebuffer/palette
  hashes. It reaches a new game at frame 161 with day `0x121cf7`, fraction
  `0x80000000`, start type 1, player state `0x30`, index 107 and cash 1000
  (raw game units). RNG state is recorded, not a port of the complete call order.

NASM preincludes adapter-only symbol aliases; supplied legacy source files
remain unchanged. State samples omit pointer addresses and full memory dumps.
They are not save files or full deterministic game-state serialization.

```powershell
# Rebuild the local reference after changing the adapter:
.\tools\reference\build-smoke.ps1 -Nasm .\.cache\tools\nasm-3.01\nasm.exe
cargo build --release --locked -p ffe-tools

# Runs both modes twice, verifies Rust equality and rejects a modified vector:
python .\tools\reference\verify-core.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --tool .\target\release\ffe-tools.exe
```

Expected: 1536 RNG vectors, 60 timer vectors, 400 state frames and **238 matching
in-game clock transitions**. Temporary traces are removed automatically.
`ffe-tools verify-reference --vectors <jsonl> --state <jsonl> [--json]` is the
bounded standalone verifier for these exact versioned scenarios. It rejects
missing/reordered/extra rows, invalid fields, incomplete scenario coverage and
numeric mismatches. Trace hashes identify inputs; they do not authenticate the
executable or turn an arbitrary capture into a trusted oracle.

The state mode still uses the smoke adapter's synthetic per-read clock, not
faithful real-time pacing. Timer arithmetic truncates each update independently;
no remainder is silently carried. Day carry is 32-bit overflow, not 49,710 ticks
per day (the historical inventory's unit interpretation is corrected).
Faithful flight, docking, trading, save/load, object physics, mission/RNG call scheduling,
complete 50 Hz input processing and timer/wall-clock wrap policy remain unported.

### Owner check: test both milestones

From this checkout, the updated release tool/reference and local cache are ready.
Python must be installed; no source rebuild is required for this check:

```powershell
.\tools\test-milestones.ps1
```

It verifies the existing v1..v6 cache, rebuilds temporary v6 caches and compares
their pixels/audio/source bytes, then performs the new reference/core checks
twice. Expect **`PASS: both technical milestones verified`**, followed by a
reminder that these checks do not open a window. This is separate from the
experimental playable example and does not validate its gameplay.

Report which numbered step failed and its error, or the final PASS line.
Do not send the cache, game files or full traces. To inspect the existing visual
output separately, open [the cockpit contact sheet](extracted/index.html);
that remains the six-cabin partial export, not all embedded artwork.

### Developer verification

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python -m unittest ci.test_check_forbidden
python ci\check_forbidden.py
```

Automated Rust tests use synthetic data, not proprietary game files.
Windows validation currently runs 203 synthetic tests; an additional
case/link-handling test is enabled on Unix. CI is configured for Windows and
Linux; remote CI has not been run by this local implementation session.

**All the local reference comparisons in one command** (needs the reference
binaries, `data.bin`, `game/`, the release tools and the supplied sources):

```powershell
python .\tools\reference\verify-all.py
```

It checks the inputs exist, runs all 29 oracles against the executed JJFFE
reference, prints one line each and a pass/fail summary, and exits non-zero on
the first failure. The individual commands below remain useful when working on
one family of helpers.

For the local-only original-font/bitmap comparison (requires the rebuilt
reference and your v2 cache; two tests are deliberately ignored in public CI):

```powershell
python .\tools\reference\verify-font.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --data .\data.bin
```

It executes JJFFE's character-width, glyph rasterizer and clipped bitmap blitter:
**696 glyph cases + 28 console/sprite cases**. Rust pixels and advances match,
both captures repeat, and deliberately altered font/bitmap hashes fail.
No original pixels or font bytes are retained in test fixtures.

For the new local-only station vertex interpreter check (v3 cache):

```powershell
python .\tools\reference\verify-models.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --data .\data.bin
```

It compares **13,632 vertex results in 192 cases** against executed JJFFE
F1472/F1477/F1521: two station programs, four matrices, three scales and eight
variable seeds, including overflow/extrapolation. Global/local slots receive
distinct values to check selector decoding. Traces repeat and altered coordinates
must fail. It does **not** validate drawing commands, original projection or a
complete model renderer. Only hashes/metadata leave the temporary reference run;
no original vertex records are retained as test fixtures.

For the command-helper check, first rebuild the local references, then:

```powershell
python .\tools\reference\verify-commands.py --reference .\.cache\jjffe-reference\jjffe-commands.exe
```

For original station normal visibility/lighting (v5 cache):

```powershell
python .\tools\reference\verify-normals.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --data .\data.bin
```

This compares **26,712 normal results in 504 contexts** with executed JJFFE
F1480/F1481, including mirrored anchors, masked shifts and wrapping dot products.
Repeated traces match; altered hashes and empty traces are rejected. This validates
normal colours/visibility, not full command execution or rendered station pixels.

For integer trigonometry and basis transforms (v6 cache):

```powershell
python .\tools\reference\verify-transforms.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --data .\data.bin
```

This checks all **65,536 low-word angles plus five wrap cases**, **135 basis
rotations** and **210 trig arithmetic cases** against executed original routines.
Each result family rejects tampering, as does an empty trace. The rotation helper
accepts a resolved angle; a separate rotation-command helper resolves the original
packed signed literal or global/local operand.
Invalid axis 3 reports an error rather than reproducing undefined pointers.

For the packed operand and actual rotation-handler comparison:

```powershell
python .\tools\reference\verify-rotation-operands.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe --data .\data.bin
```

All **65,536 encoded angle words** and **135 rotation-command outcomes** match
original F1474/F1742/F1743. Both result families reject tampering and empty traces
fail. Only the pure `0x8000` local-basis transform form executes in this helper;
base/object-relative reconstruction and vertex-defined frames remain unsupported.

For the original normal/LOD visibility branch checks (no cache required):

```powershell
python .\tools\reference\verify-visibility.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe
```

This verifies **966 predicates and 5,796 branch outcomes** against F1751/F1752/
F1755, including extreme scales/depths, detail-selection flags, masked shifts,
normal flags, inverse conditions, returns and forward/backward offsets.
Evaluated synthetic normals isolate branch semantics; original normal generation
is covered separately. Altered predicates/branches and empty traces must fail.

For composed command execution (v6 cache and the separate command probe):

```powershell
python .\tools\reference\verify-execution.py --reference .\.cache\jjffe-reference\jjffe-commands.exe --data .\data.bin
```

The original F1772 dispatcher executes the same three synthetic programs as Rust.
Across **900 contexts**, face operand order/count, emission-time locals/matrices,
final locals/matrices and original dispatcher-reset state match. All five altered
result fields and empty traces fail. The ordinary smoke oracle rejects this
probe-only mode. No original station geometry or captured pixels are fixtures.
The same script also runs the local-only real-station check: models 74/75 must
advance past opcode 6 and stop at the next unsupported command without mutating
context. To run just that check:

```powershell
$env:FFE_EXECUTION_CACHE = (Resolve-Path .\data.bin).Path
cargo test -p ffe-rs --test execution_reference executed_reference_station_depth_composition_advances -- --ignored --exact
```

The command requires the private v6 cache and is not part of public CI.

For point/line handler operands:

```powershell
python .\tools\reference\verify-point-line.py --reference .\.cache\jjffe-reference\jjffe-commands.exe
```

It compares two synthetic events (point material/angle/vertex/flag and line
material/vertices) with original F1698/F1702 via generated-copy F1699/F1703
probes; six tamper fields and an empty trace fail. This is operand
decomposition, not projection, clipping or pixels.

The ordinary smoke oracle still assembles the supplied source unchanged. A
**separate instrumented probe** redirects exactly four calls at the triangle/quad
emission boundary in a private generated copy; the supplied files stay unchanged.
Original handlers supply the arguments, not Rust or a second native decoder.
Checks cover 1,315 arithmetic cases, 360 bit branches, all 65,536 material
descriptors and 32 triangle/quad command cases, including reflected emissions.
Repeat results match; corrupt results in every family and an empty trace fail.
This proves these helpers/operand orders, **not** whole-station control flow,
visibility, projection, shading or pixels.

For the original render-order integer depth key (no cache required):

```powershell
python .\tools\reference\verify-depth.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe
```

This compares **17,303 view-space point/instance-scale combinations** with original
F1777. Repeated traces match; an altered key and an empty trace must fail.
It does not exercise depth-order command handlers, group state, sorting or pixels.

For original depth-command/group transitions and abstract group sorting:

```powershell
python .\tools\reference\verify-order.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe
```

The three discovered local-only Rust tests check **11,700 F1696 cases**,
**128 grouped marker sequences**, and **one composed F1696/F1777/F1778/F1780/
F1788 program** (order `[2,3,1]`, keys `[9,9,0]`, trace `ce0d1b4f…`), with
repeat, altered-result and empty-trace rejection. The composed case is a helper
composition, not the original dispatcher or face handlers. The original vertex
cache is explicitly primed with synthetic view-space points; marker callbacks
replace game primitives, not original allocation, group control or sorting. The
native queue's 1..4096-operation budget is development protection, not the legacy
50,000-word allocator limit. Negative/external or missing point indices error;
no lazy evaluation is guessed. Nested submodel queues, original polygon culling
and pixels remain unverified.

For original integer projection and lazy per-instance vertex slots:

```powershell
python .\tools\reference\verify-instance.py --reference .\.cache\jjffe-reference\jjffe-smoke.exe
```

This compares **2,197 projection cases** (original F1479) and **8,192 live-vertex
cases** (original command cache check plus F1470/F1471/F1478) against executed
JJFFE. Repeat traces match; altered projection/live-vertex fields and empty
traces fail. It does not validate external vertices, screen-derived forms,
submodels, clipping or pixels. These helpers are composed into the model executor
by `execute_live` (v0.11); see `docs/MODEL_EXECUTION_PLAN.md`.

## Approved direction

See [CLAUDE.MD](CLAUDE.MD) for the assessment of the currently supplied
`game/`, JJFFE, FFED3D, additional CD/floppy releases and patches. It contains
the decisions, implementation gates, 4K upscale-export requirements, audio
approach and five short owner playtests. It supersedes conflicting older
proposals. Technical approval does not resolve source or derived-art rights.

## What this is

- **Windows x64 first:** the first playable slice is launch, fly, dock, trade and save/reload.
- **Classic by default:** target the original 320×200 indexed look, 4:3 presentation, palette, bitmap font, affine textures, original cockpit and nearest-neighbour scaling.
- **JJFFE gameplay, documented fixes:** verified bugs may be fixed with regression tests and deviation records; FFED3D's Anisotropic changes are not the default.
- **Live Enhanced switching:** native resolution, HD images, filtering and MSAA first; modest bloom next, advanced effects later. Prepare resources before switching; no game restart.
- **Gameplay-affecting options are separate** (extended FOV, input smoothing, aim assist, fill-widescreen). They are never touched by the master button and are listed in `docs/DEVIATIONS.md`.
- **Graphics packs only for v1:** images first; replacement meshes, gameplay mods and scripts are deferred.
- **Classic and Modern controls:** selectable independently of graphics. Import legacy saves; exporting new saves back to the old game is not required.
- **No game data in the repository or release.** The player supplies an installed copy in `game/`; `data.bin` and optional `upscaled.bin` are generated locally.

## Documentation

| Document | Contents |
|---|---|
| [CLAUDE.MD](CLAUDE.MD) | Current decisions, scope, plan, upscale sizing and owner playtests. |
| `docs/PHASE0_SUMMARY.md` | Historical FFED3D study; superseded decisions are marked. |
| `docs/INVENTORY.md` | Complete source/game inventory, global state, file formats, content audit, numeric model, timing, build report. |
| `docs/FFED3D_RENDERING_NOTES.md` | How the original software renderer and the FFED3D D3D9 path work, and what Classic must reproduce. |
| `docs/RENDERING_DESIGN.md` | Proposed Enhanced feature list/order, dependencies, master button, performance budgets, quirk checklist, test tolerances. |
| `docs/ARCHITECTURE.md` | Proposed Rust workspace, `data.bin`/`upscaled.bin` contracts, version detection, startup auto-conversion, mod layering. |
| `docs/FORMATS.md` | Why RON/TOML/JSON, exact schemas, determinism rules, upscaling exclusion/resize rules. |
| `docs/GAME_FILES.md` | The oracle copy: every file, size and SHA-256, version detection and floppy/CD notes. |
| `docs/LEGAL.md`, `docs/TAKEDOWN.md` | Copyright status, project stance, rights-holder process. |
| `docs/KNOWN_ISSUES.md` | Known original/reference issues and the bug policy. |
| `docs/DEVIATIONS.md` | Every intentional deviation, including the gameplay-affecting group. |
| `docs/PORT_LOG.md` | Decisions, discrepancies, open questions. |
| `docs/STATUS.md` | Current milestone status, trace hashes, and handoff entry point. |
| `docs/MODEL_EXECUTION_PLAN.md` | Design record for the implemented v0.11–v0.13 depth/point/line work and its follow-ups. |
| `docs/MODEL_COMMANDS.md` | Full F1772 opcode map and current Rust coverage; probe-first notes for point/line/text/submodel. |
| `docs/REFERENCE_HARNESS.md` | How to add a differential test against the executed JJFFE reference. |
| `docs/CONVENTIONS.md` | Code conventions: errors, numeric model, budgets, safety, process. |
| `dashboard/` | Milestone burndown page and its data; update both with progress. |

## Repository layout

Implemented: `ffe-core` (RNG/clock/counter primitives),
`ffe-tools` (inspection, external export, cache build/verify, reference comparison),
`ffe-convert` (embedded PE bitmaps, BMP/PNG, PCM/WAV and sizing), `ffe-content`
(cache writer/reader), `ffe-rs` (isolated experimental flight/market/save loop,
software renderer and window/input) and `tools/reference/` (local smoke and
asset verification). Faithful full simulation and dedicated rendering/platform
crates remain planned:

```
crates/ffe-core/        RNG, game clock and timer counters; full simulation planned
crates/ffe-content/     partial base-cache reader/writer; replacement packs planned
crates/ffe-convert/     verified partial import; full game conversion planned
crates/ffe-render/      render description, software reference, wgpu Classic, features
crates/ffe-platform/    window/input/audio/config
crates/ffe-tools/       inspect-game/export-external/build-data/inspect-data/verify-reference
crates/ffe-rs/          experimental playable binary; not the faithful engine yet
tests/golden/           planned: hashes and numeric dumps only (not yet present)
dashboard/              milestone burndown page (open index.html locally)
```

## Data pipeline (to be implemented)

The partial `build-data` command and startup cache loading above work now;
the following commands and automatic first-run conversion are proposed,
not currently runnable. The game will use the
same conversion library on first run when its base cache is missing or stale.

```powershell
# Or just run the game: if data.bin is missing/outdated and ./game/ is next to
# the exe, it converts with progress output and starts.

# Upscaling:
cargo run -p ffe-tools -- export-upscale --data data.bin --out to-be-upscaled --target-width 3840 --target-height 2160
# run your upscaler over to-be-upscaled/ -> upscaled/
cargo run -p ffe-tools -- pack-upscaled --data data.bin --upscaled upscaled --out upscaled.bin
```

Both archives stay local. `data.bin` contains converted original content;
`upscaled.bin` contains optional replacement images with stable IDs, checksums
and explicit dimensions. A partial pack uses original images for missing entries.
The upscale export will include images, a contact sheet, CSV and manifest with
per-asset UHD size recommendations, not a blanket 4096×4096 requirement.
The existing LFS attribute is historical configuration, not publication approval.

See `docs/ARCHITECTURE.md` and `docs/FORMATS.md` for the manifest/provenance rules.

## What is committed / never committed

**Allowed:** original project code, docs, tests, schemas and approved non-reconstructable golden hashes/minimal diagnostics.

**Local only:** `game/`, `legacy/`, `jjffe-master/`, `ffed3d-master/`, all supplied
`Frontier-First-Encounters*` release/patch directories, `data.bin`, `upscaled.bin`,
upscale working folders, extracted art, original/derived captures and saves.

`ci/check_forbidden.py` fails CI if any forbidden path is tracked. `.gitignore` covers them locally.

### Local references

The recovered source has no established redistribution grant. Keep it local;
a submodule does not solve licensing. Record revisions/hashes for reproducibility.
The supplied CD image has a different executable (English 1.01) from the floppy
copy (English 1.02), plus 323 videos and companion stills. Patch outputs still
need verification. See [GAME_FILES.md](docs/GAME_FILES.md); observed versions
do not imply full gameplay/content support. Both exact installed profiles now
support the partial cache importer; raw BIN/CUE input and patches do not.

## Building the legacy reference (study only)

The historical Phase 0 study reports an FFED3D intro build, not a validated JJFFE
baseline or complete gameplay. It was not reproduced during this assessment.
A reproducible local JJFFE startup smoke build and verified partial importer
now exist (commands above). Bounded new-game state and numeric traces are also
verified against the first Rust core primitives. Full flight/object/mission/save
traces remain future milestones. Local reference instrumentation is approved.

## Legal

This is an unofficial, AI-assisted preservation/reimplementation project, not
endorsed by the original or community authors. Players supply their own game.
Source-informed rewrites and AI-upscaled artwork still need rights review
before distribution. No original or derived game art ships with this project.
See [LEGAL.md](docs/LEGAL.md) and [TAKEDOWN.md](docs/TAKEDOWN.md).

**No open-source licence has been chosen.** `LICENSE` is a placeholder for the project owner to fill in. Do not assume redistribution rights for the Rust code until it is replaced.
