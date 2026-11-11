# Display Filter Options - Decision Summary

## Question
Should FFE-RS-Classic add more display filter options beyond the current Nearest/Linear choices, considering pixel scaling, texture filtering, and post-processing effects?

## Answer: Yes, but Carefully Targeted

After analyzing the rendering architecture, **pixel-art upscaling filters** make perfect sense, while most modern 3D rendering filters do not apply.

## Why?

FFE-RS-Classic is a **pure 2D software renderer**:
- Game renders to a 320×200 indexed-color framebuffer (CPU-based)
- Palette lookup produces 320×200 RGBA
- **This 320×200 frame is then scaled to modern resolution** for display
- No GPU 3D rendering, no perspective textures, no geometry anti-aliasing

This architecture is **ideal for retro pixel-art upscaling algorithms** but inappropriate for modern 3D techniques.

## What Makes Sense

### ✅ Recommended: Pixel-Art Upscaling Filters

These CPU-based algorithms process the 320×200 frame to detect edges and smooth pixels intelligently:

1. **xBRZ** (2x/3x) - Modern, edge-aware upscaling [PRIMARY RECOMMENDATION]
   - Smooths pixelation while preserving sharp edges
   - Popular in modern emulators
   - ~5-12ms processing time (acceptable at 60 FPS)

2. **HQx** (HQ2x/HQ3x/HQ4x) - Classic emulator filter
   - Well-known from DOSBox/RetroArch
   - Similar quality to xBRZ

3. **Scale2x/Scale3x** - Fast, subtle enhancement
   - <1ms overhead
   - Good for low-end systems

### ✅ Recommended: CRT Scanlines

Simple overlay effect simulating CRT displays:
- CPU-based, negligible performance cost
- Configurable intensity slider
- Enhances retro aesthetic

### ⏸️ Deferred: Full CRT Shader Suite

Advanced CRT emulation (curvature, phosphor glow, bloom):
- **Requires GPU shader support**
- Current `minifb` library doesn't support custom shaders
- Would need migration to `wgpu` or `pixels` crate
- **Recommendation**: Defer to post-1.0 or Enhanced renderer work

### ❌ Not Applicable

These are for 3D GPU rendering and don't apply to software-rendered 2D:

- **Anisotropic filtering** - for perspective-correct 3D textures (game uses affine mapping on CPU)
- **MSAA/SSAA** - for anti-aliasing GPU-rasterized geometry
- **FXAA/SMAA** - for fixing GPU rendering artifacts
- **TAA** - requires motion vectors from GPU pipeline

## Recommended Implementation

### Phase 1: Pixel Upscaling (High Priority)
Add to Display Options page:
```
Upscaling Filter:
○ Nearest Neighbor (Sharp, classic)
○ Linear/Bilinear (Smooth, current)
○ xBRZ 2x (Recommended for modern displays)
○ xBRZ 3x (Highest quality, slower)
○ Scale2x (Fast, subtle enhancement)
```

### Phase 2: CRT Scanlines (Medium Priority)
```
☐ Scanlines  [Intensity: ▬▬▬░░]
```

### Phase 3: GPU Shaders (Future/Optional)
Requires architecture change - defer to Enhanced renderer.

## Configuration Example

Extend `saves/display.cfg`:
```ini
scale=4
fit=pixel-perfect
filter=xbrz-2x
aspect=dos-4:3
scanlines=off
scanline-intensity=0.3
```

## Performance Budget

- Target: Maintain 60 FPS (16.67ms frame time)
- Filter budget: 5ms maximum
- xBRZ 2x: ~5ms at 320×200 input ✅
- xBRZ 3x: ~12ms at 320×200 input ⚠️ (may need optimization)
- Scale2x: <1ms ✅

## Work Item Added

**ui-24**: "Pixel-art upscaling filters (xBRZ, HQx, Scale2x) and optional CRT scanlines"
- Status: todo
- Milestone: Interface and presentation (phase 5)

## Design Documents

- Full design: `docs/notes/display-filters-design.md`
- Current display implementation: `docs/notes/display.md`

## Key Decisions

1. **Default filter**: Keep **Nearest Neighbor** for purity; users opt-in to upscaling
2. **Primary filter**: **xBRZ 2x** as the recommended modern option
3. **GPU shaders**: Deferred until post-1.0 (architectural blocker with minifb)
4. **Scope**: Filters apply to entire 320×200 frame (not just game view)
5. **Performance**: Auto-disable or warn if frame time exceeds budget

## Why Not Separate "Texture" and "Pixel" Filters?

The game doesn't have separate 3D textures in the modern sense:
- Everything is rendered to a single 320×200 2D framebuffer
- "Textures" (planet surfaces, building faces) are software-rasterized into this buffer
- By the time we apply display filters, all 3D/2D distinction is lost
- **One filter applies to the entire presented frame**

This is different from modern 3D games where you might filter UI (2D) and world textures (3D) separately.

## References

- xBRZ algorithm: https://sourceforge.net/projects/xbrz/
- HQx filters: https://en.wikipedia.org/wiki/Hqx
- DOSBox scaling options: https://www.dosbox.com/wiki/Configuration:Display
- RetroArch shaders: https://github.com/libretro/glsl-shaders

---

**Summary**: Add CPU-based pixel upscaling (xBRZ, Scale2x) and simple scanlines. These fit perfectly with the software-rendered architecture. Defer GPU shaders to future work.
