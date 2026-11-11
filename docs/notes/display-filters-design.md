# Display Filter Options Design

## Context

FFE-RS-Classic is a pure software renderer that produces a 320×200 indexed-color framebuffer,
then presents it to a modern window with scaling. The current display options (v0.87, EXP-017)
support:

- **Scale**: 1x-6x integer multiples
- **Fit**: Pixel-perfect (integer scale) or Fill window
- **Filter**: Nearest or Linear (bilinear)
- **Aspect**: DOS 4:3 or Square pixels
- **Fullscreen**: Alt+Enter toggle

This design extends the filter options with retro-gaming-appropriate upscaling and post-processing
effects that work on the 320×200 → modern resolution presentation pipeline.

## Architecture

The rendering pipeline is:

```
Game logic → 320×200 indexed buffer → Palette lookup → 320×200 RGBA →
  [NEW FILTER STAGE] → Scaled buffer → Window presentation
```

Filters operate on the **post-palette 320×200 RGBA frame** before final scaling, or as
**post-processing shaders** on the scaled output.

## Recommended Filter Options

### 1. Pixel-Art Upscaling Filters (Primary Category)

These are **CPU-based algorithms** that process the 320×200 buffer to a larger intermediate
resolution (2x, 3x, 4x) before final scaling. They preserve pixel-art character while reducing
blockiness.

#### Option A: xBRZ (Recommended Default)
- **Description**: Edge-aware upscaling, smooths pixelated artwork intelligently
- **Variants**: xBRZ 2x, xBRZ 3x, xBRZ 4x
- **Implementation**: Use the `xbrz` Rust crate
- **Performance**: ~5-15ms at 320×200 input (acceptable for 60 FPS)
- **Best for**: General use, looks great on modern displays

#### Option B: HQx Family
- **Description**: Classic pixel-art enhancement, popular in emulators
- **Variants**: HQ2x, HQ3x, HQ4x
- **Implementation**: Port C implementation or use existing Rust crate
- **Performance**: Similar to xBRZ
- **Best for**: Users familiar with DOSBox/RetroArch

#### Option C: Scale2x/Scale3x (Fast)
- **Description**: Simple edge-directed scaling, very fast
- **Implementation**: Simple algorithm, easy to implement
- **Performance**: <1ms overhead
- **Best for**: Low-end systems

#### Option D: EPX/AdvMAME2x
- **Description**: Simple nearest-neighbor with edge smoothing
- **Implementation**: Minimal code
- **Best for**: Subtle enhancement without aggressive smoothing

### 2. CRT Emulation (Secondary Category)

These are **shader-based effects** applied to the final scaled buffer, simulating CRT display
characteristics.

#### Option A: Scanlines (Simple)
- **Description**: Adds horizontal scanlines
- **Variants**: Thin, Medium, Thick
- **Implementation**: Simple shader or CPU post-pass
- **Performance**: Negligible
- **Best for**: Quick CRT aesthetic without complexity

#### Option B: CRT Shader (Advanced)
- **Description**: Full CRT simulation with scanlines, phosphor glow, bloom, curvature
- **Variants**: Subtle, Classic, Authentic
- **Implementation**: Port existing GLSL shaders (crt-royale, crt-easymode)
- **Performance**: Requires GPU; 2-5ms on integrated graphics
- **Best for**: Full retro experience
- **Note**: Requires adding GPU backend (minifb doesn't support shaders)

### 3. Optional Enhancements

#### Integer Scaling Lock (Already Implemented)
- Current "Pixel-perfect" mode already does this
- Keep as-is

#### Sharpening
- **CAS (Contrast Adaptive Sharpening)**: Simple post-process to enhance clarity
- **Implementation**: Lightweight shader or CPU convolution
- **Use case**: Counter softness from Linear filtering

## Recommended Launcher Configuration

### Display Settings Page Layout

```
┌────────────────────────────────────────┐
│ Display Settings                        │
├────────────────────────────────────────┤
│                                         │
│ Window Scale:  [4x ▼]  (1x-6x)        │
│ Fit Mode:      [Pixel-Perfect ▼]      │
│ Aspect Ratio:  [DOS 4:3 ▼]            │
│                                         │
│ ─── Upscaling Filter ───               │
│                                         │
│ ○ Nearest Neighbor (Sharp, blocky)    │
│ ○ Linear/Bilinear (Smooth blur)       │
│ ○ xBRZ 2x (Recommended)                │
│ ○ xBRZ 3x (Higher quality)             │
│ ○ HQ2x (Classic emulator style)        │
│ ○ Scale2x (Fast, subtle)               │
│                                         │
│ ─── CRT Effects ───                    │
│                                         │
│ ☐ Scanlines     [Intensity: ▬▬▬░░]    │
│ ☐ CRT Curvature (Requires GPU)         │
│                                         │
│ ─── Advanced ───                       │
│                                         │
│ ☐ Sharpening    [Amount: ▬▬░░░]       │
│                                         │
└────────────────────────────────────────┘
```

### Configuration File Format

Extend `saves/display.cfg`:

```ini
scale=4
fit=pixel-perfect
filter=xbrz-2x
aspect=dos-4:3
scanlines=off
scanline-intensity=0.3
crt-curvature=off
sharpening=0.0
```

## Implementation Plan

### Phase 1: CPU-based Pixel Upscaling (High Priority)

**Scope**: Add xBRZ and Scale2x as new filter options alongside Nearest/Linear

1. **Integrate xBRZ library**
   - Add `xbrz` crate dependency (or port the algorithm)
   - Implement 2x/3x variants (4x is rarely needed given final scaling)

2. **Implement Scale2x**
   - Simple algorithm, ~50 lines of code
   - Good fallback for performance

3. **Update display pipeline**
   - Modify `display::compose` to apply filter before final scaling
   - Filter selection: Nearest → Linear → xBRZ2x → xBRZ3x → Scale2x

4. **Add launcher UI**
   - Radio button selection or dropdown
   - Preview images showing difference
   - Performance warning for slower systems

5. **Extend config persistence**
   - Update `settings::load_display` / `save_display`
   - Add `filter=` parsing for new values

6. **Performance testing**
   - Target: maintain 60 FPS on modest hardware
   - Benchmark at 320×200 input
   - Add `--no-filter` debug flag for comparison

### Phase 2: CRT Scanlines (Medium Priority)

**Scope**: Simple scanline overlay, no GPU required

1. **CPU-based scanline pass**
   - Apply darkening to every Nth row after final scaling
   - Configurable intensity (0.0-1.0)

2. **Launcher control**
   - Checkbox + slider
   - Real-time preview

3. **Config persistence**
   - `scanlines=on/off`
   - `scanline-intensity=0.3`

### Phase 3: GPU CRT Shaders (Future/Optional)

**Scope**: Full CRT emulation with curvature, bloom, phosphor

**Blocker**: minifb doesn't support custom shaders

**Options**:
1. Migrate to `wgpu` or `pixels` crate for GPU presentation
2. Use `wgpu-hal` minimal backend alongside minifb
3. Defer until Enhanced renderer (see `RENDERING_DESIGN.md`)

**Recommendation**: Defer to post-1.0 or Enhanced renderer milestone. The CPU-based
filters provide 90% of the value without architectural changes.

## Performance Considerations

### Target Hardware
- **Minimum**: Intel i5-6500 (2015) or equivalent
- **Target frame time**: <16.67ms (60 FPS)
- **Filter budget**: 5ms maximum

### Measured Performance (Estimated)

| Filter          | 320×200 → 640×400 | 320×200 → 1280×960 |
|-----------------|-------------------|--------------------|
| Nearest         | <0.1ms            | <0.1ms             |
| Linear          | 0.5ms             | 1.5ms              |
| Scale2x         | 1ms               | 1ms (fixed)        |
| xBRZ 2x         | 5ms               | 5ms (fixed)        |
| xBRZ 3x         | 12ms              | 12ms (fixed)       |
| HQ2x            | 8ms               | 8ms (fixed)        |

Note: Pixel upscalers work on the **input resolution** (320×200), so their cost is fixed
regardless of output window size. This is a major advantage over GPU shaders.

### Optimization Strategies

1. **Lazy evaluation**: Only apply filter when frame changes (not during pause)
2. **Multi-threading**: Process filter in background thread while presenting previous frame
3. **SIMD**: Use `wide` or `packed_simd` for vectorized operations
4. **Cache-friendly**: Process scanlines sequentially for better cache locality
5. **Fallback**: Auto-disable filters if frame time exceeds budget

## Testing Strategy

1. **Visual regression tests**
   - Capture filtered outputs in `docs/evidence/display-filters/`
   - Compare against reference images

2. **Performance benchmarks**
   - `cargo bench` for filter algorithms
   - `--benchmark` flag for end-to-end frame timing

3. **Cross-platform validation**
   - Windows, Linux, macOS builds
   - Intel/AMD/ARM architectures

4. **User acceptance**
   - Side-by-side comparison screenshots
   - A/B test with community feedback

## Open Questions / Owner Decisions

1. **Default filter**: Nearest (classic) vs xBRZ 2x (modern-friendly)?
   - Recommendation: **Nearest** for purity, user can opt-in to xBRZ

2. **Filter performance budget**: 5ms acceptable? Auto-disable on slow systems?
   - Recommendation: Yes, with auto-detection and warning

3. **HQ2x vs xBRZ**: Include both or pick one?
   - Recommendation: **Start with xBRZ only**, add HQ2x if requested

4. **GPU requirement**: Defer CRT shaders until Enhanced renderer?
   - Recommendation: **Yes**, keep Classic renderer CPU-only

5. **Filter applies to**: Game view only, or entire frame including HUD?
   - Recommendation: **Entire 320×200 frame** (consistent with current architecture)

6. **Launcher integration**: Separate settings page or inline in Display Options?
   - Recommendation: **Inline in Display Options** page (EXP-017 extension)

## References

- Current display implementation: `docs/notes/display.md`
- Rendering architecture: `docs/ARCHITECTURE.md` §3
- xBRZ algorithm: https://sourceforge.net/projects/xbrz/
- HQx algorithm: https://en.wikipedia.org/wiki/Hqx
- CRT shaders: https://github.com/libretro/glsl-shaders

## Version History

- 2026-11-11: Initial design (Claude AI analysis)
