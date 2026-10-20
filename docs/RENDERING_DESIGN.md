# Rendering design proposal (Phase 0)

Status: **proposal — awaiting approval.** No renderer code has been written.

This document defines the Classic target, the Enhanced feature set and build order, the performance budget, dependency/capability rules, the master Enhanced button, the render-description contract, large-scene strategy, and the test plan. It is based on `docs/INVENTORY.md` and `docs/FFED3D_RENDERING_NOTES.md`.

---

## 1. Principles

1. **Classic is the default and the reference.** Every enhancement is off. Classic reproduces the original DOS look: 320×200 indexed framebuffer, 320×158 3D view + 42-row HUD, 6-bit palette, bitmap font, affine textures, flat/quantised shading, original aspect and nearest-neighbour scaling.
2. **The simulation never sees the renderer.** Render features are presentation-only. They cannot read or write `GameState`, call the gameplay RNG, alter time, or change input mapping. The render description is a one-way snapshot.
3. **Enhanced is a set of independent passes/features.** Each is switchable at runtime, has a dependency list, a capability check, a performance budget and an “off equals Classic” test.
4. **The master button is a mode switch, not a quality preset that changes gameplay.** It flips between “all presentation-only enhancements off (Classic)” and the player's saved presentation-only selection. Gameplay-affecting options are a separate group and are never touched by it.
5. **No effect may be silently on.** First run is Classic. A feature that cannot run on the adapter is shown disabled with an explanation, not silently substituted.
6. **The software reference is the oracle for Classic.** The GPU Classic path may differ only in documented, measured ways (rasterisation edge rules). The software reference is also the `--software` fallback.

---

## 2. Classic target (frozen)

| Property | Classic value |
|---|---|
| Internal framebuffer | 320 × 200, 8-bit palette indices |
| 3D viewport | 320 × 158 (top), HUD/console 42 rows |
| Palette | 256 × 6-bit RGB (0–63); display expansion per approved rule |
| Aspect | Original 4:3 (non-square pixels) — **pending approval**; FFED3D's 16:10 is not Classic |
| Scaling | Nearest-neighbour; integer scale where possible; letterbox/pillarbox; no filtering |
| 3D projection | Original FFE projection (35° vertical FOV, aspect 2.0 for 320×158); derive exact coefficients from the original code |
| Text | Original proportional bitmap font, palette colours, one-pixel shadow, clipping/wrap |
| Textures | Original indexed bitmaps, affine mapping, nearest sampling, palette lookup |
| Shading | Original material commands, ambient ramp, face normals, 8-level quantisation, no specular |
| Transparency | Original draw order and masked/transparent blits; no modern alpha blending unless the original did it |
| Depth | Original draw order/span behaviour; no arbitrary z-buffer in the Classic path |
| Models | Original command streams (submodels, mirror/rotate, conditionals, material IDs) |
| Planets | Original procedural icosahedron/LOD/colour algorithm |
| HUD | Original panel/icons/cockpit bitmaps; no TTF, no 3D panel |
| Audio-visual timing | 50 Hz simulation cadence; no interpolation, no smoothing |
| Enhancements | None |

The exact presentation of the 6-bit palette and 4:3 vs 16:10 is an open question in `INVENTORY.md` §10. The software reference stores **indexed** pixels, so palette expansion is a display-only decision and can be tested separately.

### 2.1 Classic renderer paths

- **Software reference renderer (`--software`, test oracle):** CPU port of the original rasteriser, text, sprites, model interpreter and planet LOD. Outputs a 320×200 `Vec<u8>` of palette indices plus the palette. Exact golden hashes.
- **GPU Classic (`wgpu`, default):** projects/transforms on the CPU with the same fixed-point math, snaps to the original 320×158 coordinate grid, submits triangles/sprites in original order, samples indexed textures with nearest filtering, looks up the palette, and writes an RGBA8 320×200 target (or a palette-index target if the backend supports it). Upscales with nearest/integer scaling. No depth buffer effects, no MSAA, no filtering, no tone mapping.
- **Comparison:** GPU Classic is compared to the software reference with documented tolerances (§7). Where a pixel differs, the reason must be a known GPU rasterisation rule (edge inclusion, half-pixel centre, interpolation), not a shading/texture/content difference.

A single `RenderFeatures` struct selects the path; `Classic = all false`. The master button sets/restores it atomically.

---

## 3. Enhanced feature list and build order

Each item is independently switchable. “Presentation-only” means it cannot feed back into simulation. “Gameplay-affecting” is a separate group and **never** part of the master button.

### 3.1 Presentation-only (master group)

Build order is chosen so each step is testable and later effects can depend on earlier infrastructure.

| # | Feature ID | Menu label | What it does | Depends on | Phase |
|---:|---|---|---|---|---|
| 1 | `hd_textures` | HD textures | Replaces Classic assets from `upscaled.bin` by stable asset ID; per-asset fallback to `data.bin`; optional normal/roughness/emissive maps. | — | 6 |
| 2 | `native_resolution` | Native-resolution rendering | Renders 3D at the window's native resolution instead of 320×158; HUD/text may stay pixel-sharp at native scale. | — | 7 |
| 3 | `smooth_filtering` | Smooth texture filtering | Bilinear/trilinear filtering instead of nearest. | `native_resolution` | 7 |
| 4 | `anisotropic` | Anisotropic filtering | 2×–16× anisotropic sampling. | `smooth_filtering`, `native_resolution` | 7 |
| 5 | `msaa` | Anti-aliasing (MSAA) | 2×/4×/8× MSAA (adapter-supported). | `native_resolution` | 7 |
| 6 | `taa` | Temporal anti-aliasing | TAA jitter/history resolve. | `native_resolution` | 7 |
| 7 | `sharp_hud` | Sharp scalable HUD/text | Re-rasterises HUD/text at native resolution from vector/bitmap sources, preserving layout/metrics. | `native_resolution` | 7 |
| 8 | `hdr` | HDR rendering + tone mapping | Float16/32 scene target, exposure, tone map; optional HDR surface output. | `native_resolution` | 7 |
| 9 | `hdr_output` | HDR display output | 10-bit/16-bit float swapchain + OS HDR metadata. | `hdr` | 7 |
| 10 | `bloom` | Bloom and lens effects | Threshold/blur/combine; optional lens flare/ghosting. | `hdr` | 7 |
| 11 | `shadows` | Shadows (including ship self-shadowing) | Cascaded/local shadow maps for ships/stations; self-shadowing; contact shadows. | `native_resolution` | 7 |
| 12 | `ao` | Ambient occlusion | HBAO/SSAO from depth/normals. | `native_resolution` | 7 |
| 13 | `atmosphere` | Atmospheric scattering and clouds | Planet atmosphere single-scattering + cloud layer. | `native_resolution`, `hdr` | 7 |
| 14 | `starfield` | Higher-quality starfield and nebulae | GPU starfield/nebulae from core star/nebula data; no gameplay RNG. | `native_resolution` | 7 |
| 15 | `particles` | Engine glows and particle effects | Higher-quality sprites/particles, additive glows; original spawn timing. | `native_resolution` (HDR recommended) | 7 |
| 16 | `material_maps` | Material maps | Normal/roughness/emissive maps where an asset provides them; per-asset fallback. | `native_resolution`, `smooth_filtering`, `hd_textures` (maps may be in either pack) | 7 |
| 17 | `widescreen` | Widescreen/ultrawide presentation | Correct letterbox/pillarbox, HUD anchoring, integer scaling; **does not change the 3D frustum** by default. A “fill widescreen (extended horizontal FOV)” variant is in the gameplay-affecting group. | — | 7 |
| 18 | `frame_interpolation` | Frame interpolation | Interpolates transforms/camera between fixed simulation ticks; never interpolates discrete events. | — | 7 |

Rationale for order: infrastructure (native-res targets, HDR, pass graph) first; filtering/AA; HDR/bloom; shadows/AO; planet atmosphere; starfield/particles; material maps; display/interpolation last. HD textures ships first because it is the checkpoint feature and exercises the asset pipeline.

### 3.2 Gameplay-affecting (separate group, never master)

| Feature ID | Menu label | Why it affects play | Notes |
|---|---|---|---|
| `extended_fov` | Extended FOV | Reveals more of the scene; changes situational awareness. | Off by default; never master; documented in `docs/DEVIATIONS.md`. |
| `widescreen_fill` | Fill widescreen (extended horizontal FOV) | Reveals more horizontally than the original frustum. | Separate from presentation `widescreen`. |
| `input_smoothing` | Input smoothing | Changes flight response. | Off by default; never master. |
| `aim_assist` (future) | Aim assist | Changes combat. | Not implemented in v1; listed for completeness. |

The menu labels this group “Gameplay-affecting — changes how the game plays”. The master button skips the entire group.

---

## 4. Master button and settings

### 4.1 Semantics

- **Classic mode:** every presentation-only feature is off. This is the default on first run and on `--classic`.
- **Enhanced mode:** the saved `EnhancedSelection` is applied. On first use, the selection defaults to every presentation-only feature the adapter supports (dependencies resolved), with the capability explanation shown for anything unavailable.
- **Master toggle:** flips between Classic and the saved selection instantly, mid-flight, with no restart, no state reload and no simulation step. It does not change gameplay-affecting options.
- **Reset to Classic:** sets master off and clears the selection to “none” (the next master-on starts from the default selection again). It does not change gameplay-affecting options.
- **On-screen confirmation:** a short, non-blocking message (“Classic” / “Enhanced”) in the game's own style, with the hotkey/menu path.
- **Persistence:** saved to the config file immediately on change; CLI flags override for the run.
- **Hotkey:** configurable; default `Ctrl+Shift+E` (does not collide with the original bindings). Documented on the controls screen. The existing `ffewin.cfg` bindings remain the source of truth for original controls.

### 4.2 Config

One config file (e.g. `ffe-rs.toml`) with:

```toml
[graphics]
mode = "classic"            # classic | enhanced
enhanced = ["hd_textures", "native_resolution", "hdr"]
gameplay_affecting = []     # never touched by master

[graphics.hd_textures]
enabled = true
pack = "upscaled.bin"

[display]
window_width = 1280
window_height = 800
integer_scaling = true
aspect = "original"         # original | stretch (stretch is presentation-only? see design)
hdr = "auto"

[input]
enhanced_toggle = "Ctrl+Shift+E"
master = "menu"
```

CLI (examples): `--classic`, `--enhanced`, `--feature hd_textures=on`, `--gameplay-feature extended_fov=off`, `--config <path>`, `--software`, `--list-features`, `--set display.integer_scaling=false`. Flags are applied after the file; the master button updates the file.

### 4.3 Dependency and capability rules

- Turning on a feature auto-enables its dependencies and displays a one-line notice (“Smooth filtering enabled native-resolution rendering”).
- Turning off a dependency turns off dependants (with a notice).
- If a capability is missing, the feature is shown disabled with a specific reason (e.g. “MSAA 4× is not supported by this adapter; 2× is available”, “HDR output requires a 10-bit surface and OS HDR”).
- Unsupported features are not silently downgraded; the player chooses the available level.
- The master button only enables features that are supported in the saved selection; unsupported ones remain off with a note.

---

## 5. Render description contract

The core emits a `RenderFrame` each tick. Renderers read it; they never read/mutate game logic. The description is deliberately high-level and ID-based so content/upscaled asset substitution is possible.

```rust
struct RenderFrame {
    tick: u64,
    sim_time: SimTime,             // day + tics, for HUD/animations
    camera: Camera,                // prev/current, relative to player frame
    view: Viewport,                // classic 320x158 / native / widescreen
    palette: PaletteId,            // base palette (Classic)
    items: Vec<DrawItem>,          // stable order
    lights: Vec<Light>,
    hud: Vec<HudItem>,
    text: Vec<TextItem>,
    effects: Vec<EffectItem>,
}

struct Camera {
    prev: Transform64,
    curr: Transform64,
    fov_y: f32,                    // original 35 degrees in Classic
    near: f64, far: f64,
    frame_of_reference: ObjectId,  // camera-relative origin
}

enum DrawItem {
    Model(ModelDraw),              // model asset id + submodel state + material overrides
    Planet(PlanetDraw),            // body id, radius, rotation, atmosphere params
    Star(StarDraw),                // star id/type/colour
    Sprite(SpriteDraw),
    Particle(ParticleDraw),
    Ring(RingDraw),
}

struct ModelDraw {
    asset: AssetId,                // base:<id> or mod:<id>
    prev: Transform64,
    curr: Transform64,
    scale: Scale64,
    submodel: SubmodelState,       // landing gear, turrets, etc.
    material: MaterialOverride,    // original command material + overrides
    lights: SmallVec<LightId>,
    flags: DrawFlags,              // cull, transparent, zclear, etc.
}

struct TextItem {
    asset: FontId,                 // original bitmap font id / HD font id
    text: TextId,                  // registry string, no formatting in renderer
    pos: (i16, i16),               // classic logical coords
    colour: PaletteIndex,
    shadow: bool,
    clip: Rect,
}

struct HudItem {
    asset: AssetId,                // panel/icon/cockpit bitmap id
    pos: (i16, i16),
    size: (u16, u16),
    layer: HudLayer,               // panel, scanner, text, pointer, confirmation
}
```

Rules:

- IDs are stable namespaced IDs (`base:cobra_mk3`, `mymod:my_ship`); the content registry resolves them.
- `prev`/`curr` let the renderer interpolate without touching the simulation; when interpolation is off it uses `curr`.
- `PlanetDraw`/`StarDraw` carry the parameters needed for Classic/Enhanced (type, radius, rotation, atmosphere, colour), not game logic.
- `TextItem` carries the *expanded* text or a string ID plus vars? **Decision:** the core expands FF codes into a glyph run (`Vec<GlyphRun>`) for Classic, and carries the Unicode/plain text for Enhanced fonts. This keeps text expansion (simulation-adjacent, deterministic) in core and prevents renderers from reimplementing FF codes. The renderer may choose a bitmap font (Classic) or a vector font (Enhanced), but layout metrics must match Classic when the enhancement is off.
- HUD coordinates are always the original 320×200 logical coordinates; Enhanced scaling is a renderer transform, not a core change.
- The description is versioned (`RENDER_DESC_VERSION`) and serializable for golden tests/fuzz.

The software reference renderer consumes the same `RenderFrame` and the same content registry. It is not allowed to special-case game rules.

---

## 6. Large-scale scene handling

Phase 0 established:

- 1 km ≈ 999,539.7368756 game units (≈ 1.00046 mm/unit).
- Positions are 64-bit with an exponent; object distances use exponent bytes, not floats.
- Ships ~10⁴–10⁵ units; planets ~10⁹–10¹⁰; stars much larger.
- FFED3D uses f32 D3D transforms and a 0.1–50,000 / 1–100,000 depth range, which is not sufficient for planet-scale scenes.

Proposed strategy:

1. **Floating origin.** The core supplies a camera frame-of-reference object. CPU transforms are f64 relative to that origin; GPU receives f32 camera-relative positions. Rebase every frame (the camera origin follows the player).
2. **Per-object local frames.** Planets/stations use their own local origin for vertices and rotation; model matrices are composed in f64 on the CPU, then converted to camera-relative f32.
3. **Depth.** Use `Depth32Float` with reversed-Z and an infinite far plane for the near pass; split into near/far passes only if needed (near: ships/station, far: planets/stars). Alternatively logarithmic depth for the far pass. Document the choice after Phase 4 measurements.
4. **Planet scale.** Render planet surfaces in a planet-local frame with a separate projection; atmosphere/clouds as analytic/screen-space passes; avoid putting planet vertices in the ship's depth range.
5. **Starfield at infinity.** Stars are direction-only; depth = far, no parallax; use a separate pass.
6. **LOD.** Classic uses the original distance thresholds; Enhanced uses screen-space error and the same object/planet LOD data from core. LOD selection is presentation-only.
7. **Culling.** Frustum culling in f64 camera-relative space; occlusion for planets/stations later. The core still emits all simulation-relevant items; the renderer may cull for performance.
8. **Precision tests.** Fixed camera/object scenes with known f64 positions; assert GPU vs CPU projected positions within 1 pixel; test near/far extremes; test rebasing at 10⁹ units.

This design is independent of the original exponent scheme; the original is used as a reference for LOD/visual behaviour, not for GPU depth.

---

## 7. Testing and tolerances

### 7.1 Golden software frames

- Fixed scenes (ship at known transform, station, planet, HUD, text, palette stress) and fixed simulation ticks.
- Software reference outputs a 320×200 index buffer + palette. Golden hashes are SHA-256 of the index buffer, palette, and a canonical metadata blob.
- Golden files are hashes/numeric dumps only, under `tests/golden/`; no frames/screenshots are committed.

### 7.2 GPU Classic vs software

Compare the GPU Classic 320×200 output to the software index buffer/palette:

- **HUD/text/sprites:** require 100% identical indices for the pixels the software drew (they are textured quads with integer positions and nearest sampling). If the GPU cannot match exactly, document the backend and why; do not relax silently.
- **3D geometry:** target ≥ 99.9% identical pixels, max per-channel difference ≤ 1/255 on ≤ 0.1% of pixels, and no difference > 2. Differences are allowed only on triangle edges/vertices due to GPU rasterisation rules; the comparison report lists the first differing pixel and the scene ID.
- **Palette expansion:** compare indices first; compare RGBA using the approved 6-bit expansion. Store both in the test output.
- **Backends:** run on at least one hardware adapter and one software adapter (WARP/lavapipe) in CI where available; record the backend/limits in the test artifact (not committed frames).

If a scene cannot meet the tolerance, the design must either (a) move the effect to the software reference/CPU path for that scene, or (b) document a precise, approved exception. “It looks close” is not sufficient.

### 7.3 “All off equals Classic”

For every feature and every combination:

- Build the frame with the feature off and with the master off; compare to the Classic software reference (exact) and GPU Classic (tolerance). The frame description, draw order, palette and text layout must be identical.
- Toggle the master at runtime (mid-flight) and back; the next frame must equal the Classic baseline, with no residual render-target/history state.
- Toggle a feature on then off; same check. History buffers (TAA) are cleared or ignored when off.

### 7.4 Enhanced smoke tests

- Headless `wgpu` on a software adapter: create device, run each feature pass with synthetic geometry/textures, validate no validation errors, read back a small target, assert non-empty/expected structural properties (e.g. bloom increases energy, shadow map has depth).
- Synthetic images only; no game assets. Tests are deterministic where possible; if a pass is non-deterministic (TAA jitter), assert invariants rather than pixels.
- Performance budget tests are manual/reporting, not blocking CI (GPU variability).

### 7.5 Determinism

- The software renderer is deterministic and has no wall-clock/random input.
- The GPU Classic path is deterministic for a fixed adapter/driver where possible (no TAA/MSAA in Classic). Enhanced TAA is allowed to be temporal/non-deterministic but must not affect simulation.
- Render tests record the adapter name/backend as metadata; golden GPU pixels are never committed.

---

## 8. Performance budget

Target: 1920×1080, 60 fps (16.6 ms), mid-range 2019 GPU (GTX 1660 / RX 5600 XT), 16 GB RAM, Vulkan/DX12. Budgets are targets to be measured; a feature that exceeds its budget must offer a lower level or be disabled with an explanation.

| Feature | GPU cost target | VRAM target | Notes |
|---|---:|---:|---|
| Classic (all off) | ≤ 2.0 ms | ≤ 64 MB assets + framebuffer | GPU only scales the 320×200 frame; CPU simulation dominates. |
| HD textures | +1–2 ms | +1–3 GB (4× assets) | Higher with material maps; streaming/cache. |
| Native resolution | +3–6 ms at 1080p | +50 MB targets | 4K scales ~4× pixels. |
| Smooth filtering | +0.3 ms | — | Bilinear/trilinear. |
| Anisotropic 8× | +0.5–1 ms | — | Texture-cache pressure. |
| MSAA 4× | +2–4 ms | +100 MB | 8× doubles. |
| TAA | +1–2 ms | +50 MB history | Jitter/resolve. |
| HDR + tone map | +0.5–1 ms | +50 MB float target | HDR output adds scanout bandwidth. |
| Bloom/lens | +0.7–1.5 ms | +30 MB | 5–6 mip chain. |
| Shadows | +2–4 ms | +100–300 MB | Local cascades; not planet-scale. |
| AO | +1.5–3 ms | +30 MB | HBAO/SSAO at half/native. |
| Atmosphere/clouds | +2–4 ms per planet | +100 MB | Only for nearby planets; LOD. |
| Starfield/nebulae | +0.5–1.5 ms | +50 MB | Direction-only, cheap. |
| Particles/glows | +1–2 ms | +20 MB | Fill-rate bound. |
| Material maps | +0.5–1.5 ms | +1–2 GB | 3 extra textures per asset. |
| Widescreen presentation | +0–0.5 ms | — | Different viewport/scissor. |
| Frame interpolation | +1–2 ms | +20 MB | Extra transforms; no sim change. |

Global budget: Classic ≤ 4 ms GPU at 1080p; Enhanced default selection ≤ 12 ms; “everything on” may exceed 16.6 ms on the target GPU, so the menu shows a live estimate/benchmark and the player can choose. At 4K, target 30–60 fps depending on features; document per-feature scaling.

CPU: `ffe-core` target ≤ 2 ms per 20 ms tick (50 Hz) on the target CPU; rendering must not block simulation. Loading `data.bin` target ≤ 2 s from SSD; `upscaled.bin` lazy/streamed.

---

## 9. Classic quirk reproduction table

This is the implementation checklist for the GPU Classic path and the software reference. Each row has a test scene.

| Quirk | Reproduction | Test |
|---|---|---|
| 320×200, 320×158 view | Fixed internal target; viewport/scissor; nearest/integer upscale | Resolution/aspect test |
| 6-bit palette | Palette texture/table; display expansion rule | Palette ramp test |
| Bitmap font | Glyph atlas from original font bitmaps; integer positions; shadow | Text layout test |
| Text control codes | Core expands to glyph runs; renderer draws glyphs | FF-code text test |
| Affine textures | No perspective-correct interpolation; integer UV steps | Checkerboard/UV skew test |
| Nearest sampling | `FilterMode::Nearest`, no mips | Texture test |
| Flat/quantised shading | Integer ambient ramp, face normal; per-face colour | Lighting ramp test |
| Face-normal culling | CPU test before submission; no GPU backface culling | Culling test |
| Draw-order transparency | Opaque then transparent list; no depth writes for transparent | Overlap test |
| Command-stream models | Core/renderer resolves submodels/conditionals; original material IDs | Model/submodel test |
| Procedural planets | Software reference CPU tessellation; GPU Classic either CPU tessellation or exact shader emulation | Planet hash test |
| Starfield | Procedural points with original colours | Starfield test |
| HUD panel/cockpit | Original bitmap assets at 320×200 coords | HUD test |
| Mouse pointer | Software pointer overlay in logical coords | Pointer test |
| 4:3/non-square pixels | Presentation transform (pending approval) | Aspect test |
| 50 Hz pacing | Simulation fixed 20 ms; render uses latest state | Timing test |

If a GPU Classic feature cannot reproduce a quirk exactly, the software reference remains the authoritative Classic output; the GPU path must document the divergence and tolerance.

---

## 10. Implementation shape

Proposed crates (see `docs/ARCHITECTURE.md`):

- `ffe-render-desc`: `RenderFrame`, IDs, transforms, serialization, version.
- `ffe-render`:
  - `software`: CPU reference (Classic exact).
  - `classic_gpu`: wgpu Classic path.
  - `features`: feature IDs, dependencies, capability, settings, pass graph.
  - `gpu`: device/surface/target management, upscale/present.
  - `enhanced`: one module per feature (HD textures, native, filtering, AA, HDR, bloom, shadows, AO, atmosphere, starfield, particles, materials, widescreen, interpolation).
- `ffe-content`: content registry, asset IDs, `data.bin`/`upscaled.bin` readers, palette/font/model assets.
- `ffe-platform`: window/input/audio/config; owns the swapchain and presents frames.
- `ffe-rs`: binary; owns the loop, simulation tick, render description emission, settings, menu.

The renderer is a library with a `Renderer` trait:

```rust
trait Renderer {
    fn resize(&mut self, size: PhysicalSize<u32>);
    fn draw(&mut self, frame: &RenderFrame, features: &RenderFeatures) -> FrameResult;
    fn set_features(&mut self, features: &RenderFeatures) -> Result<(), FeatureError>;
    fn capabilities(&self) -> &Capabilities;
}
```

`set_features` must be instantaneous and must not reset simulation. The Classic GPU path is just `RenderFeatures::classic()`; there is no separate “mode” that reloads state.

---

## 11. Risks and open design questions

1. **Classic GPU exactness.** GPU rasterisation edge rules may prevent bit-exact 3D geometry. Mitigation: CPU fixed-point projection/snap, no MSAA/depth effects, nearest sampling, and a documented tolerance. If a scene cannot meet it, keep the software path as the Classic renderer for that scene (performance is trivial at 320×200) or implement a GPU compute rasteriser that matches the original fixed-point rules. This is a design decision for Phase 5.
2. **Planets.** The original procedural planet algorithm is complex; a GPU Classic planet that exactly matches software may be harder than ships/HUD. Fallback: CPU tessellation into a vertex buffer (Classic only), GPU shading with palette lookup. This preserves exactness at the cost of CPU.
3. **Aspect/palette choices** (see `INVENTORY.md` §10).
4. **Widescreen semantics.** Presentation-only widescreen must not reveal more of the scene; the “fill” variant that does is gameplay-affecting. Confirm the grouping.
5. **HDR output** on Linux/Windows varies; capability detection and fallback.
6. **Frame interpolation** can expose simulation discreteness (e.g. hit/explosion) if applied to the wrong state; interpolate only continuous transforms/camera, and keep event-driven effects at the tick.
7. **HD textures/material maps** must not change Classic; ensure Classic ignores `upscaled.bin` entirely and the master off path never binds an HD asset.
8. **Performance** of CPU planet tessellation in Classic; measure and cache per planet/seed without changing the simulation RNG.

---

## 12. Approval gates

Before writing rendering code:

- Approve the feature list and build order (§3).
- Approve the master-button semantics and config/CLI shape (§4).
- Approve the render-description contract (§5), especially that text expansion/model command interpretation stays in core/content and renderers only draw.
- Approve the Classic target (4:3 vs 16:10, palette expansion, exact 320×200) (§2, `INVENTORY.md` §10).
- Approve the performance budgets and the “software reference is the oracle” policy (§7–8).

If approved, Phase 4 implements the render description and software reference; Phase 5 the GPU Classic path; Phase 6 the Enhanced framework, master button and HD textures.
