# Deviations from the reference / original

This is the register of every intentional difference between `ffe-rs` and the frozen reference. “Reference” means the JJFFE 2.8a6 + Aniso 0.9c + FFED3D build studied in Phase 0, unless a row says DOS 1.02. A deviation without a row here is a bug.

Status values: **proposed** (awaiting owner approval), **approved**, **implemented**, **rejected**.

---

## 1. Simulation

The base simulation is frozen against golden traces. No row here is implemented yet.

| ID | Deviation | Status | Rationale / scope |
|---|---|---|---|
| SIM-001 | Use the built FFED3D/JJFFE reference as the simulation oracle instead of the DOS 1.02 executable. | proposed | The DOS EXE cannot be instrumented with the supplied source; the reference runs against `legacy/Elite3/`. Differences from DOS are logged in `KNOWN_ISSUES.md` §1. |
| SIM-002 | Keep FFED3D's labelled `FIX:`/`ANISO` behaviour in the default path (laser invulnerability, collision radius, missile retardation, orbital drift, starports below ground, groundhog day, hyperspace time-travel, etc.). | proposed | Matches the reference traces. If the owner wants DOS-original behaviour, it becomes an opt-in per-fix toggle, not a default. |
| SIM-003 | Do not port FFED3D-added **content** (extra ships/models/textures/shaders/scripts/UI strings). | proposed | Not base content; no licence; excluded from `data.bin`. |
| SIM-004 | Do not implement the “Frontier 3500” extended-universe start in v1. | proposed | FFED3D-specific gameplay/content; not in the floppy base. Can be a mod/data pack later. |
| SIM-005 | Modern save-file I/O may write atomically and report errors cleanly, but the **bytes** of a valid save remain compatible. | proposed | Robustness only; no format change by default. A separate modern format is opt-in only. |
| SIM-006 | No new gameplay bug fixes by default. | approved (policy) | See `KNOWN_ISSUES.md`; fixes are opt-in toggles with tests. |
| SIM-007 | Deterministic 20 ms/50 Hz harness timestep for traces; the real game still uses measured elapsed time. | proposed | Proves equivalence; does not change the game. |
| SIM-008 | Mods can add/override/patch content, but the base game with no mods is byte-for-byte trace-identical. | approved (policy) | Golden tests run on base only. |

---

## 2. Presentation — Classic

Classic is frozen: **no deviations allowed** without the owner's explicit approval. The following are *not* deviations; they are corrections of FFED3D-only behaviour back to DOS:

| ID | Topic | Status | Note |
|---|---|---|---|
| CLS-001 | 320×200 indexed framebuffer, 320×158 3D + 42-row HUD | proposed | FFED3D uses a 640×400 software buffer; DOS uses 320×200. |
| CLS-002 | Original 4:3 presentation with non-square pixels | proposed | FFED3D uses 16:10 `aspectfix`; DOS is 4:3. **Owner decision required.** |
| CLS-003 | Original bitmap font, not Arial/TTF | proposed | FFED3D uses `D3DXFont`. |
| CLS-004 | Original HUD panel/icons, not a 3D panel render target | proposed | FFED3D uses `panel.X` + 1024×1024 texture. |
| CLS-005 | Affine texture mapping, nearest sampling, no mips/aniso | proposed | FFED3D uses linear/mip filtering. |
| CLS-006 | Original flat/quantised shading, not D3D Gouraud/specular | proposed | FFED3D uses D3D lighting. |
| CLS-007 | Original draw order/transparency, no arbitrary z-buffer effects | proposed | FFED3D uses D24S8. |
| CLS-008 | Original procedural planets, not GPU shader planets | proposed | FFED3D uses `planet.fx`/height maps. |
| CLS-009 | Original bitmap starfield, not `stars.X`/skybox TGAs | proposed | FFED3D adds HD assets. |
| CLS-010 | 50 Hz pacing, no interpolation/smoothing | proposed | Enhanced frame interpolation is off by default. |
| CLS-011 | Original mouse pointer, not the OS cursor | proposed | FFED3D draws a software pointer. |

**If a CLS row is rejected**, the resulting look is an explicit owner-approved deviation and must be recorded here with a reference image/test and a rationale. It may not be introduced by an “implementation detail”.

---

## 3. Presentation — Enhanced (opt-in, off by default)

All rows are **presentation-only**, off by default, switchable at runtime, and never feed the simulation. They are **not** deviations when off; they are documented here because they intentionally change the look when on. The master button covers all of these except where noted.

| ID | Feature | Status | Dependencies / notes |
|---|---|---|---|
| ENH-001 | HD textures (`upscaled.bin`, per-asset fallback) | proposed | Classic ignores the pack. |
| ENH-002 | Native-resolution rendering | proposed | — |
| ENH-003 | Smooth texture filtering | proposed | native-res |
| ENH-004 | Anisotropic filtering | proposed | smooth-filtering |
| ENH-005 | MSAA | proposed | native-res |
| ENH-006 | TAA | proposed | native-res |
| ENH-007 | Sharp scalable HUD/text | proposed | native-res; must match Classic metrics when off |
| ENH-008 | HDR + tone mapping | proposed | native-res |
| ENH-009 | HDR display output | proposed | HDR + capable surface/OS |
| ENH-010 | Bloom/lens effects | proposed | HDR |
| ENH-011 | Shadows (incl. ship self-shadowing) | proposed | native-res |
| ENH-012 | Ambient occlusion | proposed | native-res |
| ENH-013 | Atmospheric scattering + clouds | proposed | native-res, HDR |
| ENH-014 | Higher-quality starfield/nebulae | proposed | native-res; no gameplay RNG |
| ENH-015 | Engine glows/particles | proposed | native-res; spawn timing from core |
| ENH-016 | Material maps | proposed | native-res, smooth filtering, HD/map pack |
| ENH-017 | Widescreen/ultrawide **presentation** (letterbox/pillarbox, HUD anchoring, no frustum change) | proposed | Does not reveal more of the scene. |
| ENH-018 | Frame interpolation | proposed | Continuous transforms only; no event interpolation. |
| ENH-019 | Master button (all presentation-only on/off) | proposed | Never touches §4. |
| ENH-020 | Reset to Classic | proposed | Master off + selection cleared. |

Every ENH row must have an “off equals Classic” test and a capability/fallback path. None may change the simulation.

---

## 4. Gameplay-affecting (separate group, never master)

These are **not** Enhanced graphics. They are off by default, labelled “Gameplay-affecting — changes how the game plays”, and excluded from the master button. They are deviations from the original when on.

| ID | Option | Status | Why it affects play |
|---|---|---|---|
| GP-001 | Extended FOV | proposed | Reveals more of the scene; changes situational awareness. |
| GP-002 | Fill widescreen (extended horizontal FOV) | proposed | Reveals more horizontally than the original frustum. Presentation-only `widescreen` does not. |
| GP-003 | Input smoothing | proposed | Changes flight response. |
| GP-004 | Aim assist (future) | proposed | Changes combat. Not implemented in v1. |

If the owner considers any of these presentation-only, move it to §3 with an explicit note; until then it stays here.

---

## 5. Platform / plumbing (free-to-improve)

These do not change the simulation or the Classic look, but are listed for transparency.

| ID | Change | Status | Notes |
|---|---|---|---|
| PLT-001 | Modern windowing/input backend (winit/gilrs) with remapping and gamepad support | proposed | Default bindings and pointer mapping must match. |
| PLT-002 | Modern audio backend/mixing; HMP→MIDI + WAV effects | proposed | Trigger points/levels unchanged; synth choice is an open question. |
| PLT-003 | Config file (`ffe-rs.toml`) + CLI flags | proposed | Original `ffewin.cfg` bindings remain honoured/imported. |
| PLT-004 | Data conversion to `data.bin`; game/ not needed at runtime | proposed | Byte-identical content; deterministic. |
| PLT-005 | File dialogs use a native/modern dialog, not the original bitmap UI | proposed | The in-game options/file menu remains Classic-styled; OS dialogs are platform chrome. |
| PLT-006 | Atomic saves, clear errors, no panics on corrupt input | proposed | Valid save bytes unchanged. |
| PLT-007 | 64-bit build (original is 32-bit) | proposed | Must preserve 32-bit integer widths/overflow semantics explicitly. |
| PLT-008 | Linux support (if easy) | proposed | Not required for v1; Windows first. |

---

## 6. Content / data

| ID | Deviation | Status | Note |
|---|---|---|---|
| DAT-001 | `data.bin` contains converted original content, generated from the player's files; never committed. | proposed | Required by the brief. |
| DAT-002 | FFED3D-added assets/scripts are excluded. | proposed | No licence; not base content. |
| DAT-003 | `upscaled.bin` is derived from original art; provenance recorded; LFS. | proposed | HD option only. |
| DAT-004 | Mod assets can add Classic/HD art and material maps; same options apply. | proposed | `validate-mod` rejects base-asset hash collisions. |
| DAT-005 | Hand-authored systems/planets/stations are inserted/overridden by ID/coords without perturbing other systems. | proposed | Test required. |

---

## 7. Process rules

- Every deviation is listed here **before** it becomes default.
- Every fix/change gets a failing test first.
- The master button never touches §4.
- Enhanced options never feed the simulation; the golden traces must be identical with every option on and off (the renderer may not call the gameplay RNG or mutate state).
- With every enhancement off, output matches the Classic reference.
- If a change is ambiguous, it is treated as frozen and the owner is asked.
