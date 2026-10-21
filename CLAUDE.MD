# First Encounters Rust reimplementation: approved direction and plan

Assessment date: 2026-10-20.
Owner answers recorded: 2026-10-20.
Status: Rust inspection, embedded import, partial base cache and external export
implemented; local JJFFE startup/new-game state and numeric traces verified.
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
execute natively (unverified); opcode 5, vertex opcode 7 and opcodes 31/27 block
most of the rest.
It does not execute the complete model program or display completed models,
and does not replace synthetic flight geometry/physics. It is not JJFFE-equivalent
or the completed faithful phase-3 slice. Full content/gameplay remain unfinished.
This file takes precedence over conflicting older proposals.

## 1. Project goal and working rules

Reimplement Frontier: First Encounters (Elite 3) as a native Rust game for modern
hardware, using a player's locally supplied original assets. Preserve the chosen
gameplay baseline while allowing optional modern rendering and replacement art.

For agents working on this project:

- The owner approved the launch/fly/dock/trade/save slice. Start with a
  reproducible JJFFE reference and asset-version checks, not advanced effects.
- Study supplied source and read original asset data as needed. No further
  decompilation is currently shown necessary. If a concrete gap appears, document
  it first; the owner has offered to supply additional decompilation if required.
- Keep `game/`, `jjffe-master/`, `ffed3d-master/`, and supplied release/patch
  folders unchanged. Reference builds, instrumentation and patch installation
  belong in separate local copies. Such local experiments are approved.
- Do not commit original assets, reconstructed game data, legacy source, captures,
  or reference binaries. Do not upload assets to external AI services.
- Do not assume that translating assembly into Rust creates redistribution rights,
  or describe a source-informed rewrite as a legally established clean-room effort.
- Keep simulation independent of GPU, frame rate, visual quality, asset packs,
  wall-clock time, and presentation-only randomness.
- Use **JJFFE 2.8a7** as the gameplay reference; FFED3D is a rendering research
  reference only. Fix verified bugs with regression tests and a recorded
  deviation; default fixes are allowed. Do not inherit unreviewed Aniso changes.
- Target Windows x64 first; original visuals by default. Owner updated the
  development input default to Modern (W/S and arrows); retain optional Classic-like
  input via F3. Original mouse/keyboard fidelity is not yet verified.
  Offer a Modern control preset separately from graphics settings.
- V1 customization is image-only graphics packs. Meshes, gameplay mods,
  scripting and modernized gameplay are deferred.
- Record approved deviations in [DEVIATIONS.md](docs/DEVIATIONS.md) and decisions
  in [PORT_LOG.md](docs/PORT_LOG.md). Distinguish approved, planned and implemented.

The exact requested filename is `CLAUDE.MD`. Automatic discovery of this uppercase
extension is agent/platform dependent; this document does not replace existing
editor-specific instructions.

## 2. What was inspected and what is actually known

The initial assessment inspected the three current trees, representative gameplay/data
assembly, platform interfaces, build definitions, renderer hooks, asset loaders,
configuration, and existing project documentation. It did not execute either
legacy game, build a reference executable, or prove all gameplay paths complete.

| Tree | Evidence | Implication |
|---|---|---|
| [game/](game/) | 71 files; all 71 documented sizes and SHA-256 hashes match [GAME_FILES.md](docs/GAME_FILES.md). | A specific, reproducible original-data baseline is available. |
| [jjffe-master/](jjffe-master/) | Large x86 gameplay assembly, embedded data/bitmaps, and Windows/SDL/Linux platform wrappers. | Substantial recovered game implementation, not just a launcher; not a portable high-level engine. |
| [ffed3d-master/](ffed3d-master/) | Assembly gameplay plus C++ support/gameplay changes, Direct3D 9 rendering, shaders, and replacement-asset hooks. | Useful renderer and behavior reference, but not an independent portable C++ game. |

### Original installation

- [FIRSTENC.EXE](game/FIRSTENC.EXE) is 2,097,152 bytes, PE32/x86, and contains
  the identification string `English Release 1.02 - 21/04/95`.
- Its SHA-256 is
  `d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72`.
- External assets include six cockpit BMPs, 23 files under [FX/](game/FX/),
  17 HMP tracks under [MUSIC/](game/MUSIC/), and
  [MISSION.DAT](game/MISSION.DAT). Root-level test audio files are additional.
- [CABIN0.BMP](game/CABIN0.BMP) has a 320 x 158, 8-bit BMP header.
  [INTRO.HMP](game/MUSIC/INTRO.HMP) starts with the HMP signature `HMIMIDIP`.
- [COMMANDR/](game/COMMANDR/) currently has no save specimens. CD-only material,
  including bulletin-board videos, is absent from this floppy copy.
- Much original content is embedded in the executable or represented in recovered
  assembly data. A complete asset inventory/importer is still needed; this is not
  a directory of ready-to-use PNGs and conventional 3D meshes.

### Additional release and patch inputs

Read-only filesystem/header/hash inspection of the newly supplied copies found:

- The [additional floppy installation](Frontier-First-Encounters_DOS_EN_Floppy-version/Elite3/)
  has 69 files and exactly the same `FIRSTENC.EXE` as `game/`, despite its folder's
  description as unpatched. Its differing installer metadata does not establish
  a different executable version.
- The [CD input](Frontier-First-Encounters_DOS_EN_ISO-Version/) is a
  MODE1/2352 BIN/CUE image, not an extracted installed directory. Its ISO9660
  volume is `1STENC_US`, with 713 files.
- The CD's `FIRSTENC.EXE` is **English 1.01 - 13/04/95**, 2,093,056 bytes,
  SHA-256 `9957dc34aa9ef93d9a69c5f5be8e13d243046e758ad83c8a35241e31b7e6e616`.
  It is not the floppy executable plus videos.
- CD `DATA/` contains 323 AVI clips and 323 companion BMPs. All six cabin
  images, the 23 FX files, the 17 music files, and `MISSION.DAT` match `game/`.
  One sampled companion BMP is 120 x 120, 24-bit; one sampled AVI uses DIB video
  plus audio. Do not infer all clips use that codec without an inventory.
- Both patch folders contain an installer and compressed `GAMETEK.1` payload,
  not an already unpacked patched executable. Metadata identifies `eng-cd`
  and `eng-fp`; folder labels say 1.06/1.06d but installed results are unverified.
- The floppy patch payload is byte-identical to `game/GAMETEK.1`. This does
  not prove it was applied to the current executable.

See [GAME_FILES.md](docs/GAME_FILES.md) for exact hashes and verification status.
All supplied installed variants are intended inputs, but **observed is not
implemented support**. First fingerprint each executable and required content;
then validate an explicit importer profile. Unknown hashes and mixed/broken
installations must receive useful diagnostics, never guessed offsets or automatic
patching. Read installed files from `game/`; a raw disc-image importer can wait.
Keep source-data version independent from the JJFFE simulation baseline.

### JJFFE source

- [ffe.asm](jjffe-master/ffe.asm) contains broad game logic, including commander
  file operations, simulation, procedural systems, rendering, and game UI.
- It includes [ffedat.asm](jjffe-master/ffedat.asm) at line 1790.
  That data file is not missing merely because it is not independently assembled.
  [ffebmp.asm](jjffe-master/ffebmp.asm) supplies substantial additional data.
- The embedded splash at [ffe.asm:68222](jjffe-master/ffe.asm#L68222)
  identifies this tree as **JJFFE 2.8a7**. The generic version-2.8
  [release notes](jjffe-master/jjffe28.txt) are not a precise patch identifier.
  The audit found no `ANISO`/Anisotropic identification in this tree; this is not
  proof of stock DOS behavior.
- [ffeapi.h](jjffe-master/ffeapi.h) describes the platform boundary: video,
  input, timing, sound/music, directories, and commander files. SDL/Windows
  wrappers invoke the assembly entry point rather than replacing the simulation.
- Legacy builds require 32-bit assumptions, NASM, and old platform dependencies.
  The Linux assembly-name converter is not the decompiler. Missing NASM-internal
  headers in the optional source-generation tools do not establish a runtime
  build blocker, and those tools are unnecessary for this plan.

### FFED3D source

- [ffe.asm](ffed3d-master/ffe.asm) includes
  [ffedat.asm](ffed3d-master/ffedat.asm) at line 2115.
  [ffed3d.vcxproj](ffed3d-master/ffed3d.vcxproj) builds the gameplay assembly
  and bitmap assembly alongside C++ modules.
- [w32video.cpp](ffed3d-master/win/w32video.cpp) invokes the assembly runtime;
  [gfx.cpp](ffed3d-master/win/gfx.cpp) consumes assembly-owned globals,
  vertex/model routines, visibility results, and planet routines.
- [RenderSystem.h](ffed3d-master/win/Render/RenderSystem.h) exposes Direct3D
  types, while callers also access the raw device. Replacing that one class
  will not replace the entire renderer.
- [XfileEntity.cpp](ffed3d-master/win/XFile/XfileEntity.cpp) uses D3DX `.X`
  hierarchy/animation loading. HLSL FX materials are not WGSL-compatible assets.
- Numbered texture lookup tries PNG/DDS/TGA; numbered model and effect paths
  already support implicit replacement conventions. There is no established
  versioned, validated, renderer-independent graphics-pack contract.
- The panel UI references eight PNGs under `textures/panelicons/`
  ([w32video.cpp:854](ffed3d-master/win/w32video.cpp#L854)); that resource
  directory is absent in the supplied resource tree. This limits reproducing
  that UI path; it does not prove missing original simulation code.
  Absent optional numbered replacement models are not automatically blockers.
- The source has `ANISO` gameplay changes. Do not confuse the Anisotropic mod
  with anisotropic texture filtering. Shader samplers use mixed filtering;
  configurable modern AA is not an existing completed feature.

### Relationship to existing planning documents

The repository already contains a substantial Phase 0 proposal. Reuse
[ARCHITECTURE.md](docs/ARCHITECTURE.md),
[RENDERING_DESIGN.md](docs/RENDERING_DESIGN.md),
[FORMATS.md](docs/FORMATS.md), and [MODDING.md](docs/MODDING.md) as design inputs,
not proof that their features exist or were approved.

[PHASE0_SUMMARY.md](docs/PHASE0_SUMMARY.md) reports a historical FFED3D study
build, references `legacy/` paths, and recommends a modified simulation oracle.
Those paths are not the current supplied layout; the historical build was not
reproduced during this assessment. Its FFED3D/JJFFE version description must not
be applied to the separate current JJFFE 2.8a7 tree. A small Rust workspace and
read-only `ffe-tools inspect-game` command now exist, alongside a tested partial
external exporter, partial base cache, pure core primitives and local JJFFE
startup/new-game/numeric harness. Full flight/gameplay traces, complete asset
conversion and a working Rust game remain absent.

Older statements about automatically distributing `upscaled.bin`, using source
submodules, matching FFED3D simulation, banning default fixes, or gameplay mods
are superseded by this file. Local game/reference/release directories, generated
caches and upscale packs are now excluded by ignore/CI rules. Never use an
indiscriminate `git add .` on this checkout. Distribution rights remain open;
the owner's no-original-files policy does not supply a source or artwork license.

## 3. Feasibility without further decompilation

**Yes: there is enough here to justify a native Rust reimplementation attempt,
with substantial implementation work. No inspected gap establishes a need to
decompile additional original code. This is not proof of complete compatibility.**

Both source trees already contain recovered game instructions and data. Reading
and understanding that existing assembly is still necessary; "no new decompiling"
does not mean "no reverse-engineered source to understand." A native rewrite is
not just wrapping the C++ code in Rust, and retaining x86 assembly behind Rust FFI
would not satisfy a fully native, architecture-independent end state.

Main risks:

1. Reconstructing types, state ownership, control flow, and invariants from
   anonymous assembly labels and shared memory.
2. Preserving fixed-point/extended numeric behavior, signedness, overflow,
   flags-dependent operations, rounding, RNG call order, and time acceleration.
3. Characterizing **JJFFE 2.8a7 vs DOS releases** rather than silently combining
   them. JJFFE is selected; FFED3D traces cannot certify JJFFE behavior. Keep
   unmodified-reference traces and intentional bug-fix expectations separate.
4. Decoding embedded content, model command streams, journal data, and saves
   with bounds checks and version-specific mappings. Assembly label addresses
   cannot simply be treated as offsets into the original executable.
5. Establishing a reproducible reference and representative playthroughs.
   A functioning intro would not validate combat, docking, missions, or saves.
6. Licensing/provenance: no suitable redistribution grant has been established
   for the recovered implementation. [LICENSE](LICENSE) is a placeholder.

Proceed through evidence gates. If a required behavior cannot be explained by
the existing source and permitted observations, record the exact missing routine,
format, or test case and return to the owner; do not silently decompile or invent it.

## 4. Modern renderer: difficulty and recommended approach

Use a new Rust renderer, provisionally **wgpu + winit**, rather than porting D3D9
state calls one-for-one. Verify the chosen versions/backends against target
hardware before committing. This is a recommendation, not an installed stack.

| Work | Relative difficulty | Why |
|---|---|---|
| Window, GPU setup, static textured-mesh demo | Low to medium | Established libraries; little game compatibility work. |
| Original model interpreter and faithful HUD/UI | High | Models are command streams with dynamic state, not just mesh files. |
| Native-resolution original geometry | Medium after the interpreter works | Projection, clipping, depth, sprites, text, and large scales need coherent handling. |
| HD image replacements and filtering | Low to medium after the asset registry | Identity, alpha, UV/layout and memory use matter more than image loading. |
| MSAA and modest bloom | Medium after the GPU pass structure | Capability checks, render-target formats, resolve/composition, and settings. |
| Planet surfaces, atmosphere, robust shadows | High | Procedural detail, horizon/LOD transitions, precision and extreme scene scale. |
| TAA, advanced lighting, HDR display output | High; defer | Motion/history, disocclusion, backend/OS support and new material requirements. |
| Exact original rasterization | High | GPU edge rules, affine mapping, palettes and original draw order differ. |

Keep simulation state in its original compatible numeric representations. Convert
only presentation snapshots into camera-relative GPU coordinates. Use local frames
and evaluate reversed-Z/depth partitioning with ship-to-planet precision tests;
do not convert the whole universe to global `f32` coordinates.

Start with one faithful visual path and one small enhanced vertical slice:

- For exact classic pixels, an original-style CPU indexed framebuffer presented
  through wgpu is a simpler first reference than demanding GPU pixel equivalence.
- For Enhanced, emit backend-neutral geometry/material/sprite/UI data and render
  at native resolution. Share semantic interpretation, not necessarily rasterizers.
- A low-resolution Classic output can remain available; exact DOS visual fidelity
  is required, with original 4:3 display correction and original palette/font/UI.
- Rendering must never mutate simulation, consume its RNG, or determine gameplay
  collisions, docking, targeting, or ship dimensions.

The challenging part is faithful behavior and geometry extraction, not obtaining
anti-aliasing from a modern API. This is a substantial multi-stage game-engine
project, not a short renderer swap. Estimate delivery dates only after the first
reference/importer/flight slice establishes throughput.

## 5. Graphics packs and options

### Content layering

Selected design (not implemented yet):

1. The Windows executable resolves `game/` beside itself (or an explicit override),
   not relative to an unpredictable working directory. On first use, generate
   `data.bin` locally through the same conversion library as the tools.
   Record input hashes, importer/profile version, source asset IDs and checksums.
   Refuse unsupported source layouts; never modify the player's installation.
2. Reuse a validated current cache. Rebuild stale caches to a temporary file and
   atomically replace only after validation. Retain `game/` as the normal source;
   cache-only use is allowed after conversion. If an existing `game/` changes,
   detect staleness before loading incompatible content. Explain cache-only mode.
3. Export eligible displayed images plus size guidance for the owner's local
   upscaler; never require the owner to write an export script or infer all sizes.
4. Validate returned images and build a separate `upscaled.bin`, never edit
   `data.bin` to install HD art. Use deterministic ZIP containers with JSON
   manifests initially, reusing the existing design rather than inventing a format.
5. Resolve stable asset IDs through `base -> selected graphics pack`.
   Missing overrides deliberately use the original asset; invalid overrides
   produce explicit diagnostics. Additional multi-pack ordering can wait.

The pack manifest should include schema/pack version, compatible game/base asset
fingerprints, stable source asset IDs, paths, source/output dimensions, asset kind,
color space, alpha mode, optional material maps, author/license and provenance.
Resolve IDs through the importer registry, never arbitrary executable addresses.
Keep pack paths portable and validate traversal, size/resource limits, duplicate
IDs, checksums, decoded dimensions, and unsupported formats.

Graphics packs may override appearance, not gameplay tables, physics, collision,
missions, saves, or RNG. Future gameplay mods must be a separate, clearly labeled
system with their own compatibility rules. Do not allow executable scripts or
arbitrary shader code in the first graphics-pack format.

### Owner-operated upscale export and 4K sizing

The exporter will produce `to-be-upscaled/images/`, `manifest.json`,
`sizes.csv`, an offline `index.html` contact sheet, and `excluded.json`.
The owner returns images under `upscaled/images/` using the same filenames.
These files are local outputs, not repository assets. Proposed tool commands
will be documented as runnable only once implemented.

Each CSV/manifest row must include asset ID, source filename/hash, category,
original pixel dimensions, logical rectangle/atlas region, alpha/mask mode,
recommended output width/height, scale, intended display size, sizing basis,
and any cap or manual-review warning. Export every eligible image exactly once,
record aliases, and list exclusions/unresolved assets rather than claiming
"all assets" after exporting only external BMPs.

**4K is a display target, not a requirement to make every image 4096 x 4096.**
Use 3840 x 2160 as the first export profile, with the Classic 4:3 canvas displayed
at 2880 x 2160. For an image of `w x h` intended to cover `W x H` screen pixels,
choose an aspect-preserving integer scale
`ceil(max(W / w, H / h))`, then optionally round up to a supported upscale factor.
Store the actual dimensions, not just a profile name.

- A full 320 x 200 canvas has a limiting scale of 10.8 at that display size:
  at least 11x for both axes, or a convenient 12x workflow (4x then 3x).
  A 320 x 158 cockpit image would be 3840 x 1896 at 12x, then composed using
  its original logical rectangle and display-aspect correction, not stretched
  to fill the entire UHD screen.
- A 64 x 64 texture occupying 256 x 256 pixels in a chosen reference view needs
  only 4x. Different views/distances can require more; no single upscale factor
  guarantees every close-up.
- Use measured screen coverage once representative scenes exist. Until then mark
  repeating/projected textures as estimates, allow per-asset overrides, and
  explicitly report adapter/VRAM caps. Full-frame art must not dictate the scale
  of all small textures.
- Preserve alpha, original anchors, UI metrics, frame alignment and atlas regions.
  Strict pack validation rejects incorrect sizes by default. Offer an explicit
  resize step with a report; do not silently resample finished owner artwork.
- Generate mips during pack/runtime processing where applicable. Record estimated
  decoded VRAM including mips; compressed PNG size does not measure GPU memory.

### What AI upscaling can and cannot do

- Upscale displayed color textures, cockpit/background art and suitable sprites.
  Preserve logical dimensions, UVs, pivots, layout, masks and intended transparency.
- Handle atlas regions and mip borders deliberately; validate halos, seams and
  palette-index transparency. Specify sRGB color vs linear data textures.
- Font glyphs and HUD text need separate review; more pixels do not guarantee
  legibility, faithful glyphs, or correct metrics.
- Do not blindly upscale palettes, lookup tables, collision data, procedural
  seeds, model commands, or simulation inputs.
- Many planets/surfaces and model details are procedural or geometric. Image
  upscaling cannot create improved silhouettes, detailed geometry, reliable UVs,
  or physically meaningful roughness/normal maps automatically.
- Plan optional glTF/GLB mesh replacements later, with explicit scale, origin,
  axes, animations/attachments and stable model IDs. Preserve original gameplay
  collision and attachment semantics. Importing existing `.X` packs is a separate
  compatibility task, not something wgpu provides.
- Higher-resolution images still look low-resolution if rendered only into a
  320 x 200 framebuffer. Expose texture quality and native resolution separately.

Keep conversion/export/upscaling local. Ownership of the game, AI transformation,
Git LFS, or requiring original files does not by itself authorize redistributing
derived art. Existing "rights-holder tolerance" language is not permission.
Default to private local packs until distribution rights are resolved.

### Proposed options UX

| Option | Initial design |
|---|---|
| Graphics mode | Classic / Enhanced; Classic first-run default. |
| Graphics pack | None / selected compatible pack; show version, coverage, validation errors and missing assets. |
| Hi-res textures | Off / On; capability depends on a loaded compatible pack. |
| Render resolution | Classic internal size / native / render scale. |
| Texture filtering | Nearest / linear / anisotropic levels supported by the adapter. |
| Anti-aliasing | Off / supported MSAA levels initially; optional later post-process/TAA modes. |
| Effects | Native resolution + HD images + filtering + MSAA first, then modest bloom; shadows/atmosphere later. TAA/HDR output/material maps deferred. |
| UI and accessibility | Separate HUD/text scaling and readability controls; do not silently change layout. |
| Enhanced master switch | Restore saved presentation settings or disable enhancements; never alter gameplay options. |

Use an AA mode rather than assuming every AA technique can be combined. Query
actual format/sample-count support, and explain unavailable choices. HDR scene
lighting/tone mapping and HDR monitor output are different features.

Live switching is required for Classic/Enhanced and the v1 image options; no
restart is required. Prepare resources before an atomic mode swap while simulation
continues, report loading, and keep the previous valid mode on failure.
Clear temporal history when relevant. Effects may not stall the simulation or
silently change the saved enhanced selection. "Live" does not promise zero
loading time; measure switching latency before setting a numerical target.

### Audio decision

The original music files are HMP (MIDI-like event data), not MP3 recordings.
Use `HMP -> Standard MIDI -> local synthesis` first. On Windows, the system MIDI
path is a practical initial option; evaluate it for timing/device behavior.
For consistent offline rendering use an explicitly supplied/appropriately licensed
SoundFont and synthesizer; a SoundFont is not automatically cleared for bundling.

MP3 is possible **after synthesis**: `HMP -> MIDI -> synth -> PCM -> MP3`.
It is not a direct file-format rename or event conversion, and needs instrument
samples. If pre-rendering simplifies playback, prefer a local lossless FLAC/PCM
cache, or Ogg for size; MP3 is optional. Keep source events/timing for diagnostics,
record synth/SoundFont/settings in the cache key, and test track/loop boundaries
and encoder delay. No converted music ships with the engine. Keep original RAW
effects losslessly as WAV and preserve trigger, pitch, loop and volume semantics.

## 6. Implementation phases and exit gates

Start with a small workspace/modules for deterministic core, content/import,
rendering, platform, and tools. Follow the existing crate split only where useful;
do not implement every proposed modding tool or abstraction before the first slice.

| Phase | Work | Required evidence before proceeding |
|---|---|---|
| 0. Decisions and provenance | Owner answers recorded; local input protection added; JJFFE/Windows x64/slice selected. Finish release/importer evidence and record unresolved distribution rights. | Technical direction approved, not a source/art redistribution grant. |
| 1. Reference and behavior map | Reproduce a permitted reference build or documented original-game observation workflow. Map subsystems, numeric operations and state; add fixed inputs/time and tracing in a local study copy where permitted. | Repeatable flight/docking/trade/save scenarios and deterministic trace hashes; limitations explicitly recorded. |
| 2. Asset import and tools | Identify executable data ranges from existing source/format evidence; implement versioned, bounds-checked import of images, palettes, models, tables, journal and audio inputs. | Known-version detection; unsupported/truncated inputs fail clearly; repeat conversion matches; representative model/image/audio/journal round trips. |
| 3. Native Rust vertical slice | Port numeric/RNG primitives, one generated system, player flight, a station, docking and minimal UI. Use placeholder/synthetic assets while testing tooling. | Same scripted inputs produce expected reference state; playable launch, fly, dock, trade, save/reload cycle. |
| 4. Complete core gameplay | Combat/AI, time acceleration, navigation, procedural galaxy, economy, missions, equipment, journals and audio. Import legacy saves read-only and write a versioned native save. | Representative long-running scenarios and imported saves pass; original saves remain untouched. Export back to DOS/JJFFE is not required. |
| 5. Faithful presentation | Model commands, cockpit/HUD/text, palettes, planets, clipping, original scene behavior and input presentation. | Approved Classic fidelity tests, including surfaces, docking and far/near transitions. Can overlap phase 3 once interfaces stabilize. |
| 6. Graphics-pack slice | Stable IDs, local export, one partial upscale pack, validation, settings and resource switching. | Original fallback works; unknown/corrupt packs report errors; changing pack never changes gameplay traces. |
| 7. Enhanced renderer | Native resolution, filtering and MSAA first; then lighting/bloom, shadows, atmosphere and later temporal features as justified. | Adapter tests, measured frame/VRAM budgets, visually reviewed scenes and all-off restoration. |
| 8. Release hardening | Target OS builds, controls/audio/device loss, saves/config migration, packaging and distribution audit. | Clean-machine player-data import and complete playthrough checkpoints; no original data or unapproved derivatives in releases. |

Do not require an x86 assembly runtime in the shipped native-Rust end state.
A temporary reference/FFI harness is optional research infrastructure, not a
shortcut that counts as completing the Rust simulation.

### Verification principles

- Compare numeric helpers and RNG sequences before porting complex game behavior.
  Preserve original rounding and RNG consumption; do not introduce new streams
  into base generation merely because a proposed mod architecture recommends them.
- Replays should cover multiple systems, flight/combat, stations/surfaces,
  hyperspace/time acceleration, economy/mission progression, and save/load.
- Assert presentation toggles/packs leave serialized simulation state and RNG
  traces unchanged. Test master off/on/off and missing/partial/corrupt packs.
- Separate simulation equality, asset-decoding correctness, pixel fidelity and
  GPU performance. A screenshot or successful build cannot stand in for all four.
- Keep proprietary inputs/captures private. Public tests use synthetic data;
  publish only approved hashes/minimal diagnostics, not reconstructable asset dumps.
- Once Rust exists, use focused tests, formatting, clippy and build checks with
  repository-supported commands. Proposed commands in older docs do not run yet.

## 7. Owner answers and remaining assumptions

| ID | Recorded decision |
|---|---|
| Q1 | Use supplied sources/data as necessary; owner can supply new decompilation if a demonstrated gap needs it. None is currently requested. |
| Q2 | Never distribute the original game. Player supplies `game/`; generate a private base cache and separate optional image pack. Source/art licensing remains unresolved before distribution. |
| Q3 | JJFFE is the gameplay baseline, using the supplied 2.8a7 source. |
| Q4 | Verified bugs may be fixed; regression tests and deviation records required, not mandatory opt-in toggles for every fix. |
| Q5 | Windows x64 first. Other platforms later. |
| Q6 | Approve launch/fly/dock/trade/save slice. |
| Q7 | Local reference builds, patches and instrumentation are permitted. |
| Q8 | Support supplied CD/floppy releases and patch states after fingerprint/layout validation. Do not trust folder names or assume CD differs only by videos. |
| Q9 | Match original visuals closely/exactly first; enhancement is optional and later. |
| Q10 | Live Classic/Enhanced switching required. Classic first-run default is the implementation choice. |
| Q11 | Images first; meshes later. |
| Q12 | Owner handles upscaling. Export all eligible images with a contact sheet, manifest/CSV and explicit recommended 4K sizes. |
| Q13 | Implementation chooses order: native resolution, HD images, filtering, MSAA, modest bloom, then evaluate shadows/atmosphere. |
| Q14 | Legacy save import is desired; native saves need not load in the old game. |
| Q15 | Both Classic and Modern control options, with remapping. Do not bundle control changes into graphics switching. |
| Q16 | Owner is open to pre-rendered music. Start HMP/MIDI; synthesize locally if cached recordings are useful. MP3 is optional, not required. |
| Q17 | Graphics packs only for v1. No gameplay mods or scripts. |
| Q18 | Owner playtests using a short checklist with exact steps and a supplied runnable build. |

Non-blocking implementation assumptions: begin with windowed 1280 x 960 Classic
presentation where the desktop permits and target 60 Hz presentation, while
preserving the validated simulation timing. UHD is an export/enhanced target, not
a minimum GPU specification. Minimum supported Windows/GPU, exact palette display
expansion, numerical performance budgets, synth choice and supported patch-output
hashes must be measured before their release gates, not guessed. No delivery date
is promised. Select Rust/source licensing before any public release.

## 8. Short owner playtest contract

An experimental playable build now exists: run `tools/play.ps1`, or
`target/release/ffe-rs.exe --root .`, from the checkout. [README.md](README.md)
contains its actual controls and four short tests. It is an explicitly isolated
prototype, not completion of the faithful slice. At the owner's request its default
input is Modern (W/S and arrows), with optional Classic-like via F3; neither is
asserted to match JJFFE. Named F2 scenarios are Docked,
Approach and Free Flight; F1 shows help. F5 creates a numbered native experimental
snapshot; F9 loads the newest. No legacy save import, audio, remapping, HD packs
or auto-conversion yet. Do not request tests for those absent features.

The checklist below remains the **eventual faithful-slice acceptance contract**.
Automated numeric/importer/replay tests are the implementer's responsibility.

Each test build must include `ffe-rs.exe`, a version/build ID, a Help/Controls
screen showing actual bindings, and a Playtest menu with named deterministic
starting scenarios and reset. No guessed key sequences or manual save editing.
Place an installed supported release in `game/` beside the executable, then run
`.\ffe-rs.exe`. Do not put only the BIN/CUE image or patch installer in `game/`.
The test sheet must identify which source hashes that build supports.

| Test | Steps | Expected result | Owner time |
|---|---|---|---|
| 1. Import and start | First run from a fresh test folder; select the supplied Docked scenario. Relaunch once. | Clear progress/detected version; Classic cockpit appears; second run reuses cache. No source files changed. | 2 min |
| 2. Launch, fly, dock | Reset Docked; launch with the binding shown in Help; turn/thrust, use time controls, then dock using the documented scenario route/autopilot. | Stable controls, readable HUD, no obvious jumps/collisions or docking failure. | 5 min |
| 3. Trade and save | In Docked, buy/sell one item; save to a new native slot; relaunch and load it. Once supplied, import a reference commander copy. | Credits/cargo/location persist; legacy import preserves supplied checkpoint values; original save untouched. | 3 min |
| 4. Presets and audio | Switch Classic/Modern controls via Options, verify one remapped action, play the short music/effect preview and mute/unmute. | Both presets work; no stuck inputs, missing channels or unintended gameplay change. | 2 min |
| 5. Live image pack (when available) | Load the small sample returned image pack; switch Classic/Enhanced twice during flight. Repeat with no pack. | Correct artwork/fallback, no restart, no position/time/cargo reset, no residual filtering in Classic; clear status without pack. | 3 min |

Keep the routine pass to these five tests (about 15 minutes). Version/patch
matrices, corrupt inputs, numeric equality, long simulation runs and GPU adapter
coverage are automated or implementer tests, not a request for the owner to
repeat every case. Later milestones may replace a test with combat/hyperspace/
surface/mission coverage, keeping routine owner testing similarly bounded.

Report only build ID, test number, pass/fail, source profile, observed result and
short reproduction steps. Private screenshots/logs if useful; do not publish
game assets or personal commander data.

## 9. Immediate next implementation step

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
gameplay, real timing, audio or save/load. See [README.md](README.md) for build
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
[FORMATS.md](docs/FORMATS.md). Cache validation checks integrity, not external
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
are in [README.md](README.md). Those technical checks do not open a window;
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
or the phase-3 reference exit gate. See EXP-001 in [DEVIATIONS.md](docs/DEVIATIONS.md).

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

V0.8 is verified: Modern W/S + arrows is the startup default; F3 retains optional
Classic-like controls, independently of original graphics. A release-to-rearm
latch prevents held keys leaking across pause, overlays, focus loss and preset
changes. Synthetic tests cover held/released/new presses, disabled input,
spurious repeat press reports and explicit mode transitions. The native-window
test checks both thrust directions/all four arrows, optional Classic-like
controls, held-W isolation across lab/pause exit and the full gameplay/save loop.
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
task is the largest census blocker, opcode 5 (complex polygon), then vertex
opcode 7 and opcodes 31/27, generalising the station oracles to those models. For each, first add an original handler probe and
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
