# Port log

Running record of decisions, discrepancies and open questions. Newest entries at the top; append rather than rewrite, unless a decision supersedes an earlier one (mark it).

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
