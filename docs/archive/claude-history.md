# CLAUDE.MD history (archived)

> Archived from `CLAUDE.MD` on 2026-11-11: the milestone narrative that opened
> the file (v0.1 to v0.43) and section 9's foundation record. Kept for the
> evidence, hashes and reasoning it records; the current state is in
> [../STATUS.md](../STATUS.md).

## Milestone narrative (formerly the header)

Pure Rust RNG/timer-counter/day-fraction primitives exist. An experimental
native playable example now supports launch/fly/dock/trade/save/reload with
cached cabin art and synthetic mechanics/geometry/economy. V0.2 adds the original
proportional font and console panel, low-palette drawing and a corrected 320x200
logical view. V0.3 adds a separate F4 station vertex lab with native, differentially
verified vertex-program interpretation. V0.4 adds declared triangle/quad outlines
and individual-face inspection plus verified standalone command helpers.
V0.5 adds verified original normals, v0.6 integer trig/transforms/visibility,
v0.7 bounded composed command execution, and v0.8 original depth-key preparation
plus the owner-requested Modern input default and release-to-rearm protection.
V0.9 adds separately verified depth-command resolution and flat group ordering.
V0.10 adds separately verified original integer projection (F1479) and bounded
live view-space vertex/instance slots (F1470/F1471/F1478).
V0.11 composes F1696 depth commands and F1780/F1788 group ordering with those
slots in `model_execute::execute_live`, returning a depth-ordered stream.
V0.12 adds a composed-order oracle; V0.13 adds point (1) and line (2) operand
decoding/emission in the same ordered `Primitive` stream. V0.14 adds verified
text (10) and child-model call (14) operand events and a Linux build of the
reference adapter. V0.15 adds the verified opcode 26 colour command; v0.16 the
verified opcode 28 base-relative transforms, after which both real station
programs run to completion; v0.17 matches both against the original F1772 run of
the same programs, event for event in command order; v0.18 matches their face
draw queues. V0.19 adds a whole-table model census: 180 of 466 models already
execute natively, and v0.20 matches all 180 against the original F1772 in
command order. V0.21 adds complex polygons (opcode 5) and v0.22 jittered vertices
(vertex opcodes 7/8), v0.23 thrusters (opcode 9), v0.24 coloured child calls
(opcode 27), v0.25 curved surfaces (opcode 31), v0.26 nested depth blocks
(opcode 21), v0.27 trailing declaration data, v0.28 opcodes 17/24/25, v0.29 curve vertices
(0x15/0x16) and a whole-table vertex oracle, v0.30 opcode 30 frame restore, v0.31
opcode 22 curves, v0.32 offset curve vertices, v0.33 vertex-defined basis (opcode 28
`0xc000`), v0.34 screen labels (opcode 18), v0.35 empty light points, v0.36 sound requests: 439
models now execute and all 434 the original completes match. v0.37 adds cache v7
(every decodable model) and the F6 model viewer, the first visible use of the
executor (diagnostic camera/colours, not the faithful renderer). v0.38 (three
parallel tasks) ports complex polygons, textured faces (cache v8) and child-model
execution: 443 models execute and match; see docs/notes/. v0.39-v0.41 draw the
original station in the flight view, use the original lighting tables and colour
packing (cache v9) and draw child models in the viewer and the station (cache
v10). v0.42 ports the original's screen-space midpoint vertex opcodes (3/4 and
13/14) and the remaining external vertex references: 446 of 466 models execute
and 437 models with their children match the original. v0.43 accepts a
declared normal count of zero and the original's return record (any opcode-zero
word): 449 of 466 models execute and 448 match the original, 440 with their
children.
It does not execute the complete model program or display completed models,
and does not replace synthetic flight geometry/physics. It is not JJFFE-equivalent
or the completed faithful phase-3 slice. Full content/gameplay remain unfinished.
This file takes precedence over conflicting older proposals.


## Foundation record (formerly section 9)

Implemented foundation: `ffe-tools inspect-game --game <installed directory>`
recognizes the exact verified floppy 1.02/CD 1.01 executables, fingerprints
48 required files, verifies the journal/cabin hashes, checks HMP signatures and
PCM lengths, and counts optional CD media. It is read-only, emits stable JSON or
text with actionable diagnostics, and does not claim conversion support.
The Windows x64 release passed against both floppy inputs and real required
CD files in a temporary directory. Synthetic tests cover invalid inputs/CLI
behavior. The original installation remains unchanged.

The next foundation milestone is implemented: the unmodified **JJFFE 2.8a7**
sources build with a local headless adapter; two 64-frame startup runs produce
identical traces (49 distinct framebuffer hashes). This does not validate
gameplay, real timing, audio or save/load. See [README.md](../../README.md) for build
and verification commands, prerequisites and local-only distribution restrictions.

`export-external --game .\game --out .\extracted` exports six cabin images as
RGBA/indexed PNG plus palettes and 23 PCM effects as WAV, with source/output
hashes, a contact sheet, exclusions and UHD sizing CSV. Originals stay unchanged.
The six images recommend 3520x1738 (11x); exports remain original-sized. Four
original BMPs leave the file-size field zero; pixel/header bounds are still
validated. BMP palette values are 8-bit, not inferred six-bit game palettes.
This is only a partial owner upscaling input, not a complete asset export or pack.

Embedded import and a partial base-cache reader/writer are now implemented.
`build-data --game .\game --out .\data.bin` requires a new file; `inspect-data
--data .\data.bin` validates its manifest and every entry. Both exact floppy
1.02/CD 1.01 layouts were verified against every JJFFE declaration: 96 primary
and 251 secondary slots. This is 345 images plus two special 32768-byte texture
blocks with zero header height, not 347 ordinary images. Keep those raw and
do not guess their display dimensions/upscale eligibility. The historical
FFED3D 96/261 table counts are not the JJFFE layout.

The initial v1 deterministic ZIP contains 377 assets: those 347 records, the fixed
six-bit low 128-colour palette, six indexed cabin PNGs and 23 WAVs. V2 now adds
the 1160-byte main font (378 total), with original MSB-first rows and advances.
V3 adds two original station vertex programs (380 assets total); V4 adds their
physical drawing-command streams (382 assets total). All four
versions remain readable by the tools; gameplay requires the v2 font, and F4
requires the v3 records. V2 still plays; missing lab data reports an explicit upgrade.
V3 retains points-only inspection; v4 adds declared-face views, not a full executor.
Preserve
signed drawing offsets, mask flags and stable table/slot IDs. Upper palette
colours remain runtime state. The manifest explicitly says content is incomplete,
records all inspected source hashes and hashes every payload. Full archive/pack
designs elsewhere are proposals; the actual partial v1..v6 cache contract is documented in
[FORMATS.md](../../docs/FORMATS.md). Cache validation checks integrity, not external
pack authenticity or freshness against a changed installation.

The next bounded reference/core milestone is implemented:
`tools/reference/exports.inc` exposes adapter-only aliases via NASM preinclude,
without editing supplied sources. The original executable runs a scripted Y
acceptance and intro choice 1 in headless mode for 400 frames. Selected state
reaches a new game at frame 161 and repeats byte-identically. This is selected
state sampling, not full serialization or validation of flight/save behavior.

`ffe-core` implements the gameplay RNG, timer-counter arithmetic and game-day
fraction carry in pure Rust. Actual legacy routines provide 1536 RNG and 60
timer vectors. The Rust verifier also matches 238 new-game clock transitions.
Preserve unsigned wide multiply/divide, per-update quotient truncation, wrapping
counter addition and old-seed rotate semantics. The game day fraction wraps at
2^32; the older 49,710-ticks-per-day interpretation was wrong. Timer scheduling/
input cadence, clock wrap policy and full mission/physics/RNG ordering are not
yet ported. The headless per-read clock is synthetic, not a real-time oracle.

The owner can run `.\tools\test-milestones.ps1` from the checkout to check both
the content and reference/core milestones. It retains no traces or temporary
caches and does not overwrite originals. Expected results and build commands
are in [README.md](../../README.md). Those technical checks do not open a window;
the separate experimental game is launched with `tools/play.ps1`.

Owner verification: on 2026-10-20 the owner ran the combined milestone checks
and reported PASS. Content-cache and reference/core technical checks are
owner-confirmed; this does not certify any of the deferred gameplay playtests.

An experimental native Rust example is implemented in the separate `ffe-rs`
crate; do not fold its synthetic mechanics into the verified `ffe-core`.
It uses a 50 Hz fixed step, 60 Hz software presentation, simple inertial thrust/
steering/braking, range/speed/alignment docking and explicit collision/boundary
safety resets. It has one fixed-price commodity and a 10-unit cargo hold.
Original cabin0 is read from the verified cache for the docked backdrop.
V0.2 also reads main font and primary panel 43 from that cache, uses the fixed
low six-bit palette expanded by shift 2, and composes at 320x200 with console
starting at y158. Nearest 4:3 correction preserves the original non-square-pixel
target. Original cabin colours remain a separate 8-bit palette; no runtime
upper palette is invented. Stars, station, status layout and scanner math remain
development presentation, not original models or original functional console UI.
Source files and the cache remain read-only. Native JSON saves are bounded,
versioned, validated and new-only, never original commander files. Immutable
numbered snapshots preserve previous saves; failed writes can leave an incomplete
new snapshot, which reports an error on load (no silent fallback).
An actual Windows input test covers the full launched flight/dock/trade cycle,
save/reset/reload, both input presets, pause/braking, corrupt-load rejection and
cross-process persistence. No legacy executable is used by the playable binary.
This satisfies an experimental playable example, **not** JJFFE behavior equality
or the phase-3 reference exit gate. See EXP-001 in [DEVIATIONS.md](../../docs/DEVIATIONS.md).

Owner feedback: the owner ran the initial v0.1 playable example and said it
looked good while explicitly acknowledging it was not like the original yet.
That is prototype acceptance, not confirmation of faithful gameplay or v0.2.

The main font is identified byte-for-byte in the exact floppy and CD executables
(RVAs 0x1013b0 and 0x1003d0). V2 cache builds validate all 116 nine-row masks and
advance bytes, retain source offsets/digests, and remain partial/untracked.
Executed JJFFE F1598/F1597 provide 696 glyph/clipping cases; F1623 provides 28
opaque/masked bitmap clipping cases. Rust matches pixels/advances and altered
hashes must fail. Public CI uses synthetic font/panel data; the real comparisons
are local ignored tests run explicitly by `tools/reference/verify-font.py`.
No original font/panel bytes are compiled into the game or retained as fixtures.
The generated local cache was upgraded only after verifying it, with the exact
old cache retained under `.cache/data-v1-backup.bin`. Original game files and
experimental save snapshots were not modified. Save format and flight behavior
are unchanged.

Station inspection confirms original models are drawing programs with packed/
computed vertices, visibility branches, transforms and submodel calls, not static
triangle meshes. Do not invent a vertex hull and describe it as the original
station. The current visible replacement is still the explicitly synthetic cube;
the model-command interpreter needs its own bounded import/executed-reference work.

The first bounded model slice is now implemented (2026-10-21): `ffe-core::model`
interprets object-space vertex records, not drawing commands. Model slots 74/75
(0x4a/0x4b) have 54/88 mirrored vertices, with 27/44 six-byte records and scales
13/14. Their original PE pointer table is RVA 0x0d0cfc in floppy 1.02 and
0x0d0cf8 in CD 1.01; do not extrapolate the bitmap/font layout delta. Both
payloads/descriptors/pointers match the actual executables and JJFFE declarations.
Private cache v3 has 380 assets/381 entries; previous generated v2 is retained at
`.cache/data-v2-backup.bin`. No originals or existing save snapshots were modified.

Literal/reflected coordinates, per-term signed Q31 products, wrapped midpoint
sums truncated towards zero, vector arithmetic, variable selection and interpolation
are native integer code with fresh per-evaluation memo tables. External references,
unsupported screen/random/curve operations, invalid counts/scales and dependency
cycles fail explicitly. No static hull, captured mesh or zero fallback substitutes
for unimplemented commands. Executed JJFFE F1472/F1477/F1521 matches 13,632 vertex
results in 192 contexts: two stations, four matrices, three scales, eight variable
seeds with distinct global/local slots. Captures repeat and altered hashes fail.
Run `tools/reference/verify-models.py` explicitly with the local reference/v3 cache;
public CI uses synthetic programs only.

F4 opens the separate station vertex lab. 1/2 selects the original record set;
arrows rotate its diagnostic orthographic camera; 0/5/9 supplies test variable
values 0/32768/65535. White points are literal and green computed. All global/local
inputs receive that test value; these are not the actual game's animation state.
Flight, economy and save/load actions pause; exiting disarms held controls and
does not accumulate simulation catch-up. The native-window test checks this as
well as the existing full flight/trade/save loop. V0.3 had no faces/edges.
No station-class names or invented static mesh topology have been inferred.
V0.3 is an implementation milestone, not owner acceptance of a complete renderer.

The next bounded slice is implemented in v0.4: physical root command streams for
74/75 (516/758 bytes) are imported via verified descriptor +0 pointers; the same
core structural decoder is used by importer/cache/lab. Bounds, known record
forms, branch destinations and face vertex references are checked. F4 defaults
to declared-face wire outlines; Tab cycles single-face and old point views.
Comma/period steps faces and shows command offset, raw material and normal ID.
All physical faces, including alternate branches, are catalogued; no full
control-flow/visibility/transform/submodel execution, shading or original camera
is claimed. Diagnostic colours distinguish reflected emissions, not materials.

Standalone command helpers match executed JJFFE: 1,315 arithmetic cases,
360 bit branches, all 65,536 material descriptors and 32 triangle/quad record
cases including reflection. The original handlers confirm triangle payload
normal byte placement and exact perimeter order; do not assume FFED3D layout.
The regular smoke oracle still assembles unchanged supplied sources. The
separate `jjffe-commands.exe` redirects exactly four primitive-emission calls
in an ignored generated copy; no source files are edited. Run
`tools/reference/verify-commands.py`; repeated traces match and altered results
in every family/empty traces fail. This is helper/operand validation, not a
whole-station or pixel oracle. Sine/cosine were unsupported in v0.4, not float
approximations. Root cache v4 preserved v3 privately at
`.cache/data-v3-backup.bin`; original inputs and snapshots remain unchanged.

V0.5 is verified: cache v5 imports original station normal blocks (48/258 bytes,
18/88 expanded entries) through descriptor +12/+16. Exact F1480/F1481 semantics
match 26,712 results in 504 contexts; repeated trace SHA-256
`55b2060dfd8970cfc906c45f91944cbf06627e72c83f6bda417ef18aae1368f3`.
Both floppy/CD imports match original bytes/pointer provenance and repeat builds.
F4 Tab adds a fourth normal-filtered outline view. Its camera-derived observer,
light and shade slots are diagnostic inputs, not original instance state.
Normal colours/visibility are verified; original shaded pixels are not.
87 synthetic tests, strict clippy/fmt/release build and actual window-based
lab isolation plus the full flight/dock/trade/save regression pass.
Root cache v5 has 384 assets; v4 is preserved at `.cache/data-v4-backup.bin`.

V0.6 is verified: original 16,384-entry sine table imported into cache v6
(385 assets/386 ZIP entries), exact Sin32 quadrants/wrapping, Q31 F1744 basis
rotation and both remaining F1758 trig operations. Against unmodified executed
JJFFE: 65,541 sine cases, 135 rotations, 210 trig arithmetic cases; all three
tamper families/empty traces reject. Trace SHA-256
`b7ea38a86882aff69e3757c408387bf817d2da229ec6a332517cedc98fa53a58`.
Both release imports match full table bytes. CD sine RVA is 0x105334, not the
font-layout delta. 94 synthetic tests and earlier reference hashes pass.
Root v6 preserves v5 privately at `.cache/data-v5-backup.bin`.
Additional v0.6 verification covers all 65,536 F1474 packed angle words and 135
actual F1742/F1743 rotation-command outcomes. Both tamper families/empty traces
fail; SHA-256
`08ce2914ec9f6a9654933b4948c49cd1b521d21de1487400d7cde2d66c1530c7`.
The pure 0x8000 local-basis transform form executes through a native helper;
base/object-relative and vertex-defined transforms remain explicitly unsupported.
Full model execution remains next. F4 remains the four v0.5 diagnostic views.
F1751/F1752/F1755 normal/LOD visibility and inverse branches also match executed
JJFFE: 966 predicates, 5,796 branch outcomes, repeat/tamper/empty checks pass.
Trace SHA-256
`d7e8cc0b57b0fa8f17e7823a23866b08ced025961df0bcc1c912b9fc05ae425c`.
Inputs include explicit depth/detail/scale and evaluated normals; lazy-buffer
management and the complete command loop are not claimed.

V0.7 is verified: `ffe-core::model_execute` composes return/arithmetic,
bit/visibility branches, pure local-basis rotation and ordinary/reflected face
operand emissions in a bounded instruction loop. Ordered emission-time state
and final locals/matrices match original F1772 dispatch on three synthetic
programs in 900 contexts. Repeat trace SHA-256
`d2a76e9caf8a94abf0ca1a06b3adcd6a785c24507bab258eca30ee05163c852b`;
all five tampered fields/empty traces fail. Uses existing four emission probes
only; supplied sources and ordinary reference oracle remain unchanged.
98 synthetic Rust tests pass. Execution errors discard outputs and preserve
caller state. Exact command-budget threshold is tested; bounds are 1..4096.
Unmodeled aliased local slots 18..63 and unsupported reached commands error.
Actual station 74/75 stop at stateful opcode 6 at 0xc/0x8; do not skip it.
These are operand events, not evaluated/cull-tested/shaded geometry. F4 remains
the four diagnostic catalogue views; flight remains synthetic. Cache stays v6.
Strict lint/format/release build, actual native-window full regression, fresh
cache provenance/repeat/corruption checks and earlier core/startup hashes pass.

V0.8 is verified: Modern W/S + arrows is the startup default; F3 retains the
Classic control preset, independently of original graphics. A release-to-rearm
latch prevents held keys leaking across pause, overlays, focus loss and preset
changes. Synthetic tests cover held/released/new presses, disabled input,
spurious repeat press reports and explicit mode transitions. The native-window
test checks both thrust directions/all four arrows, Classic controls, held-W
isolation across lab/pause exit and the full gameplay/save loop.
103 synthetic Rust tests, strict formatting/clippy and release build pass.

`ffe-core::model_depth::key` matches original F1777 for 17,303 point/scale
combinations, including wrapping magnitudes, negative/nonpositive depth,
masked shifts, overflow encodings and the unsigned threshold equality boundary.
Repeat trace SHA-256
`61fffc80501112118d10975309a52bab10dd8598bd14c26b4cd07ed39332655f`.
Altered key and empty traces fail. The composed-executor oracle still matches
its prior hash; cache v6/provenance/repeat/corruption and RNG/timer/new-game
state regressions pass unchanged. No new cache assets or station rendering.
This verifies key generation only: opcode 6 group state/allocation/sorting
remain unimplemented, and the station executor must still reject that command.

V0.9 is verified: F1696 depth-command decoding/resolution uses explicit resolved
view-space points, instance position and distinct key/operand scales. Physical
record decoding reuses it. All modes, list termination, signed list extrema,
wrapping scaled z offsets and special position/-1/literal-depth forms are covered.
Missing/negative external points error; no lazy resolver is silently approximated.
11,700 original handler cases match operand consumption, active-group state,
depth key and original allocation size. Repeat trace SHA-256
`ab5da34693bcb68d8df26253f7199be280fda09886106184ecf9d0e3739950d1`.

`ffe-core::model_order` owns abstract primitive-token groups. Closing/replacing
groups, empty groups, contiguous member order, unsigned descending keys and
earlier-allocation tie order match original F1778/F1780/F1788/F1792/F1793 in
128 synthetic 64-operation sequences. Repeat trace SHA-256
`4ab5453ea1fb9bf67daea15b42fcc11e05cfa36b603c9c625d174509cac1c813`.
The development budget is 1..4096 operations, not original byte/word allocation.
Budget failure preserves queue state; unclosed groups cannot be finalized.
Both comparisons reject altered fields and empty traces. Original game primitives,
nested child-model queues, lazy state and pixels are not validated by markers.
111 synthetic Rust tests, strict lint/format/release build, original command/
executor/startup/core hashes and fresh cache checks pass. Native-window complete
gameplay/save/Modern/Classic/lab/pause regression passes; a concurrent run was
blocked by Windows focus, then passed without other reference processes.
Cache stays v6/385 assets. No visible rendering change or extra owner test needed.

The v0.11 depth composition and v0.13 point/line emission are implemented:
`execute_live` composes F1696/F1780/F1788 with `model_instance`, returns a
`Primitive` ordered stream (`Face`/`Point`/`Line`) with aligned keys, and
`execute` keeps the no-live-vertex rejection of opcode 6. Point/line operands
are verified against original F1698/F1702 via generated-copy F1699/F1703
probes. See docs/PORT_LOG.md, docs/FORMATS.md and docs/MODEL_COMMANDS.md.
V0.14 (2026-10-21) adds verified text (10) and child-model call (14) operand
events; opcode 14 is the F1749/F1750 child-model call, not text. See
docs/PORT_LOG.md. V0.15 adds the verified opcode 26 colour command (6- or
20-byte record). V0.16 adds the opcode 28 base-relative forms and the top-level
group close (F1683/F1684), so both stations complete; v0.17 verifies them
whole-program in command order, and v0.18 ports the F1708/F1712 face queueing
(cull, textured guard, outcode rejection, vertex-a key) verified against the
original's queue. V0.19 imports any model slot (generated extents, 466 slots
including the null slot 149) and adds `ffe-tools model-census`. The next single
task is the largest census blocker (opcode 30 light points, vertex opcodes 3/4/13, validation of external vertex references; see
docs/MODEL_COMMANDS.md) and a whole-table queue oracle (v0.21-v0.26 ported
opcodes 5, 9, 21, 27, 31 and vertex opcodes 7/8; the command-order oracle checks
every newly executable model). For each, first add an original handler probe and
document the payload; do not infer fields from record size. Do not claim static
resolved points remain valid through transform or animation changes. Preserve
the catalogue-vs-execution distinction and keep errors transactional; F4,
synthetic flight and cache v6 stay unchanged. Exact F1778 arena allocation and
real child-model content (cache v7) remain deferred; the abstract `Order` queue
is the approved default.

Compare executed primitive streams and then pixels before changing the flight
view.
Next port object/state and fixed-point physics with targeted executed-reference
vectors, extend state traces to flight/launch and work through models, tables
and remaining content toward the playable slice.
Music and optional CD media remain unconverted. Patch outputs still need
controlled unpacking/installation in disposable local copies and fingerprinting.
Do not substitute a FFED3D intro, static graphics demo, or partial BMP exporter
for the approved playable slice or the complete image-export requirement.
