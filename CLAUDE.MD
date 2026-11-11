# First Encounters Rust reimplementation: approved direction and plan

Assessment date: 2026-10-20. Owner answers recorded: 2026-10-20.
Development branch: `main` (owner, 2026-10-22). Work, commit and push directly
on `main`; the earlier `claude/code-commit-capabilities-k4hm9l` history was
fast-forwarded into it.

**Classic-only checkout (owner, 2026-11-10).** This repository is a copy whose
aim is a like-for-like Rust reimplementation of the original game. The Modern GPU
engine, the FFED3D integration, `assets.bin` art packs, the `ffe-render` crate
and the Classic/Modern graphics switch were removed; the `ffe-launcher` crate was
rebuilt on 2026-11-11 as a Classic launcher. The Enhanced renderer, graphics
packs and modern-art material (sections 4-5, phases 6-7, Q9-Q13) are **out of
scope** here; do not reintroduce them. Their full text is kept in
[docs/archive/claude-out-of-scope.md](docs/archive/claude-out-of-scope.md). The
Classic/Modern *control* presets are input only and stay.

**For the current state, the verified figures and the next tasks, read
[docs/STATUS.md](docs/STATUS.md) first**; what is left is in
[docs/BURNDOWN.md](docs/BURNDOWN.md) and [docs/README.md](docs/README.md) maps
the documentation. This file records the owner's decisions, scope, rules and
gates. The milestone narrative that used to open it (v0.1 to v0.43) and the
foundation record of section 9 are in
[docs/archive/claude-history.md](docs/archive/claude-history.md).

## 1. Project goal and working rules

Reimplement Frontier: First Encounters (Elite 3) as a native Rust game for modern
hardware, using a player's locally supplied original assets. Preserve the chosen
gameplay baseline. (The original plan also allowed optional modern rendering and
replacement art; neither is in scope for this Classic checkout.)

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
  development input default to Modern (W/S and arrows); retain the original-key
  Classic preset via F3 (hold the right mouse button to steer, Enter/Shift set
  speed). Offer the preset separately from graphics settings.
- No customization in this checkout: graphics packs, meshes, gameplay mods,
  scripting and modernized gameplay are out of scope (the original plan made
  image-only packs the v1 customization).
- Record approved deviations in [DEVIATIONS.md](docs/DEVIATIONS.md) and decisions
  in [PORT_LOG.md](docs/PORT_LOG.md). Distinguish approved, planned and implemented.
- **Keep the burndown current.** Every build or commit that changes milestone
  progress must update `dashboard/burndown.js` in the same commit: set the item
  statuses and append a dated history point with the new remaining count (see
  `dashboard/README.md`). The dashboard is the whole-port burndown at milestone
  level (current milestone through release hardening), so new milestones are
  added there as scope is planned.

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
[RENDERING_DESIGN.md](docs/archive/RENDERING_DESIGN.md),
[FORMATS.md](docs/FORMATS.md) and the (removed) modding design as design inputs,
not proof that their features exist or were approved.

[PHASE0_SUMMARY.md](docs/archive/PHASE0_SUMMARY.md) reports a historical FFED3D study
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

## 4. Modern renderer — out of scope

Not part of this Classic checkout. The assessment (a wgpu renderer, its
difficulty table and the Enhanced vertical slice) is kept in
[docs/archive/claude-out-of-scope.md](docs/archive/claude-out-of-scope.md).

## 5. Graphics packs and options — out of scope, except audio

Graphics packs, the upscale export and 4K sizing, and the Enhanced options UX
are not part of this checkout; they are kept in the same archive file. The audio
decision below still applies.

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
| Q11 | Images first; meshes later. The 2026-11-04 Modern/FFED3D direction was removed with the Classic-only split on 2026-11-10. |
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

The playable is built and started with `play.bat` or `launcher.bat`
([README.md](README.md)); its controls are in `docs/CONTROLS.md`. It is
experimental and not yet the faithful slice: legacy save import and input
remapping are not implemented, so do not request tests for them. Test 5 below
(the image pack) is out of scope in this checkout.

The checklist below remains the **eventual faithful-slice acceptance contract**.
Automated numeric/importer/replay tests are the implementer's responsibility.

Each test build must include `ffe-rs.exe`, a version/build ID, controls
documentation (`docs/CONTROLS.md`) showing actual bindings, and an optional
`--dev` inspection mode. No guessed key sequences or manual save editing.
Place an installed supported release in `game/` beside the executable, then run
`.\ffe-rs.exe`. Do not put only the BIN/CUE image or patch installer in `game/`.
The test sheet must identify which source hashes that build supports.

| Test | Steps | Expected result | Owner time |
|---|---|---|---|
| 1. Import and start | First run from a fresh test folder, then relaunch. | Clear progress/detected version; Classic cockpit appears; second run reuses cache. No source files changed. | 2 min |
| 2. Launch, fly, dock | Launch with L, turn/thrust, use time controls, then dock using the documented autopilot/docking route. | Stable controls, readable HUD, no obvious jumps/collisions or docking failure. | 5 min |
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

## 9. Next implementation step

The next tasks and their order are in [docs/STATUS.md](docs/STATUS.md#work-list-to-finish-the-game--2026-11-04-handoff)
and [docs/BURNDOWN.md](docs/BURNDOWN.md). The foundation record that this
section used to hold (inspect-game, the reference adapter, the content cache
versions, the v0.x model engine milestones and their verification hashes) is in
[docs/archive/claude-history.md](docs/archive/claude-history.md).
