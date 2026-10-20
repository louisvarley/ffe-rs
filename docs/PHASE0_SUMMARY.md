# Phase 0 summary — stop and review

Status: **Phase 0 complete. Port not started. Waiting for your approval.**

This is the short version. Details are in `docs/INVENTORY.md`, `docs/FFED3D_RENDERING_NOTES.md`, `docs/RENDERING_DESIGN.md`, `docs/ARCHITECTURE.md` and `docs/FORMATS.md`.

---

## 1. What I did

- Inventoried the entire reference tree: every function (1,715 in `ffe.asm`), every data label (4,486 in `ffedat.asm`, 4,323 in `ffebmp.asm`), every C/C++ module, resource, build file and doc.
- Verified `game/` and `legacy/Elite3/` are byte-identical: 71 files, same SHA-256. Recorded every file/hash in `docs/GAME_FILES.md`.
- Identified the oracle: **First Encounters English 1.02 (21/04/95)**, PE32, image base `0x400000`; the reference source is **JJFFE 2.8a6 + Anisotropic mod 0.9c + FFED3D**.
- Decoded the installer metadata: `BLD_INFO.CFG` build dir `eng-fp`, `GAMETEK.1` is the compressed installer payload, `IMAGE.DAT` its manifest, `PATCHES.INI` is AIL sound configuration (not the FFE patch).
- Studied the original software renderer, the FFED3D D3D9 path, the model command streams, indexed bitmaps, palette, font, planets, starfield, HUD, input, audio, timing and saves.
- Built a local **study binary** of the legacy engine (VS 2026 + NASM 3.02 + D3DX9 NuGet + Wine `d3d9.h` + a DirectInput shim) and ran it against `legacy/Elite3/`. It shows the JJFFE/Aniso splash and the Frontier copyright intro; the render loop is live. Details and caveats in `docs/INVENTORY.md` §11.
- Produced the proposed workspace layout, data formats, `data.bin`/`upscaled.bin` contracts, mod design and rendering feature order/budgets.
- Set up repository hygiene: `.gitignore`, `.gitattributes` (Git LFS for `upscaled.bin`), `ci/check_forbidden.py`, CI job, `LICENSE` placeholder, `docs/LEGAL.md`, `docs/TAKEDOWN.md`, `docs/KNOWN_ISSUES.md`, `docs/DEVIATIONS.md`, `docs/PORT_LOG.md`.

I did **not** write Rust game code, commit game data, or commit any legacy/FFED3D source/art.

## 2. Key findings

- **The game content is inside `FIRSTENC.EXE`**: ship stats/models, equipment, goods, strings, models and most bitmaps. External files are the cabin BMPs, `FX/*.RAW` (16-bit PCM 22050 mono), `MUSIC/*.HMP`, and `MISSION.DAT` (journal text). All base content is obtainable from the player's files; FFED3D's added content is not base and will be excluded.
- **The simulation is integer/fixed-point and event/frame driven.** Key formats: 1.31 matrices (`0x7fff8000` = 1.0), 16.16 values, 16-bit binary angles, **FFP** (16-bit mantissa + 16-bit exponent), `Point64` positions with exponent bytes. 170 x87 instructions remain; `F1519_fp` (x87 vector normalisation) is in the rotation path.
- **Timing:** nominal 20 ms frame (50 fps), 50 Hz input timer, 49,710 tics/day, 994 tics per 20 ms at 1×. Start `GameTics=0x80000000`, `GameDays=0x121cf7`.
- **The reference differs from DOS 1.02.** It contains 142 `ANISO` markers and 26 `FIX:` comments (laser invulnerability, collision radius, missile retardation, orbital drift, starports below ground, groundhog day, hyperspace time-travel, etc.), an extended-universe start, missile thrust 120g, D3D/HD assets and a Lua console. The golden traces will be recorded from this reference, so the port must either match it (recommended) or undo selected changes. This is the biggest decision.
- **Classic vs FFED3D look:** FFED3D uses a 640×400 software buffer, 16:10 `aspectfix`, Arial text, linear filtering, a 3D panel and shader planets. DOS is 320×200 indexed, 4:3, bitmap font, affine/nearest, bitmap panel, procedural planets. **Classic should be the DOS look**, not FFED3D's; the design is built that way.
- **Data labels do not map 1:1 to original file offsets.** The converter will use a generated layout/symbol map or signature-based extraction and verify against the original section hash. This is a Phase 2 task, not a blocker.
- **The legacy source has no licence** and its splash says a reverse-engineered copy is illegal even with the original. Legal/takedown docs reflect that; the study binary is not distributed.

## 3. Proposed decisions (need your OK)

1. **Simulation oracle:** FFED3D/JJFFE reference (matches traces); log DOS 1.02 differences. **Recommended.**
2. **Classic look:** original DOS software look (320×200, indexed, 4:3, bitmap font, affine, flat shading), not FFED3D's D3D look.
3. **Aspect:** 4:3 with non-square pixels (DOS), not 16:10. FFED3D's 16:10 becomes a documented known issue.
4. **Palette expansion:** indexed reference; GPU display rule `v<<2` (FFED3D-compatible) or full-range `round(v*255/63)` — please choose.
5. **Content format:** RON for content, TOML for mod/config, JSON for manifests/schema; indexed PNG + palette sidecar; glTF GLB + lossless `ffe.ron` command stream; WAV + MIDI. See `docs/FORMATS.md`.
6. **Workspace:** `ffe-core`, `ffe-content`, `ffe-convert`, `ffe-render`, `ffe-platform`, `ffe-tools`, `ffe-rs`. See `docs/ARCHITECTURE.md`.
7. **Rendering order:** HD textures → native-res → filtering → AA → HDR → bloom → shadows → AO → atmosphere/clouds → starfield → particles → material maps → widescreen presentation → frame interpolation. See `docs/RENDERING_DESIGN.md`.
8. **Master button:** flips all presentation-only features between off (Classic) and the saved selection; gameplay-affecting group (extended FOV, fill-widescreen, input smoothing, aim assist) is never touched; “Reset to Classic”; hotkey configurable.
9. **No scripting in v1**; reserve `ScriptHost`; recommend Rhai later. See `docs/MODDING.md`.
10. **Legacy source as submodule?** Recommend a pinned submodule to upstream rather than vendoring. The same for `legacy/Elite3/` (or keep both local-only ignored).
11. **Music default:** OS MIDI, bundled SoundFont, or optional HMI emulation? This is audio plumbing, not simulation, but affects default sound.
12. **CD version:** supply a copy or hashes/file list when available; until then only floppy 1.02 is detected.

## 4. What I could not do

- Build with the original VS2010 + DirectX SDK Feb 2010 toolchain (not installed/supported). The study binary uses substitutes; it is not bit-identical to a 2010 build.
- Exercise full gameplay, saves, audio or scripted input. The splash was bypassed for observation; Phase 1 adds the deterministic harness.
- Verify the CD version or the pre-patch floppy (not supplied).
- Confirm the exact journal entry count/bit-permutation round-trip and the exact save-file subset; these are Phase 2 verification items.
- Confirm whether every `FIX:` change is trace-visible; Phase 1 diffing will tell us.

## 5. Proposed next step (after approval)

**Phase 1:** add a headless deterministic harness to a local copy of the reference (fixed seed/timestep, scripted input, per-tick state/RNG trace, software framebuffer hashes), record golden traces against `legacy/Elite3/` (galaxy seeds/systems, flight/combat, docking/trading, hyperspace, missions, save/load), and commit only hashes/numeric dumps under `tests/golden/`.

Then Phase 2 (`ffe-convert` + `build-data`) and Phase 3 (`ffe-core`).

**Please reply with approval or changes to the decisions above.** I will not write the port or the rendering code until you approve Phase 0 and `docs/RENDERING_DESIGN.md`.
