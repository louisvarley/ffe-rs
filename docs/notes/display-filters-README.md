# Display Filter Options - Documentation Index

This directory contains the complete design and implementation documentation for **ui-24**: 
Pixel-art upscaling filters and CRT scanlines for FFE-RS-Classic.

## Quick Links

| Document | Purpose |
|----------|---------|
| **[display-filters-summary.md](display-filters-summary.md)** | 📋 **START HERE** - Executive summary and decision rationale |
| **[display-filters-design.md](display-filters-design.md)** | 📐 Full technical design specification |
| **[display-filters-comparison.md](display-filters-comparison.md)** | 👁️ Visual examples and filter characteristics |
| **[display-filters-implementation-guide.md](display-filters-implementation-guide.md)** | 🔧 Step-by-step implementation instructions |

## TL;DR

**Question**: Should we add more display filters beyond Nearest/Linear?

**Answer**: Yes! Add CPU-based pixel-art upscaling filters (xBRZ, Scale2x) and CRT scanlines.

**Why**: FFE-RS-Classic's 320×200 software-rendered architecture is **perfect** for retro upscaling 
filters. Modern 3D techniques (anisotropic filtering, MSAA, etc.) don't apply.

**What to Add**:
1. ✅ **xBRZ 2x/3x** - Edge-aware upscaling (recommended)
2. ✅ **Scale2x** - Fast alternative
3. ✅ **CRT Scanlines** - Optional overlay effect
4. ⏸️ **GPU CRT Shaders** - Deferred (requires architecture change)

## Architecture Summary

```
Game Rendering (CPU)
    ↓
320×200 indexed framebuffer
    ↓
Palette lookup
    ↓
320×200 RGBA buffer
    ↓
[NEW FILTER STAGE] ← xBRZ/Scale2x process here
    ↓
Scaled to window size
    ↓
[SCANLINES] ← Optional overlay
    ↓
Window presentation
```

**Key insight**: Filters operate on the **post-palette 320×200 frame** before final window scaling.
This is ideal for pixel-art upscaling algorithms.

## Recommended Filters

### Primary: xBRZ 2x (Default Recommendation)
- **What**: Edge-aware upscaling that smooths pixel art intelligently
- **When**: Modern displays (1080p, 1440p, 4K)
- **Performance**: ~5ms at 320×200 (acceptable for 60 FPS)
- **Example**: Sharp edges preserved, smooth curves, natural look

### Alternative: Scale2x (Performance)
- **What**: Simple edge-directed scaling
- **When**: Low-end systems or users who want subtle enhancement
- **Performance**: <1ms (very fast)
- **Example**: Slight smoothing, mostly sharp

### Overlay: CRT Scanlines
- **What**: Horizontal dark lines simulating CRT display
- **When**: Users want retro CRT aesthetic
- **Performance**: Negligible
- **Configurable**: Intensity slider (0-100%)

## What NOT to Add

❌ **Anisotropic filtering** - For 3D perspective textures (game uses affine mapping on CPU)
❌ **MSAA/SSAA/FXAA/SMAA** - For GPU-rasterized geometry anti-aliasing
❌ **TAA** - Requires motion vectors from GPU pipeline
❌ **Bloom/HDR** - Game uses 6-bit palette lighting

These are all for modern 3D GPU rendering and don't apply to a software-rendered 2D framebuffer.

## Configuration Example

Extend `saves/display.cfg`:

```ini
scale=4
fit=pixel-perfect
filter=xbrz-2x        # NEW
aspect=dos-4:3
scanlines=off         # NEW
scanline-intensity=0.3  # NEW
```

## UI Mockup

```
┌──────────────────────────────────────┐
│ Display Options                       │
├──────────────────────────────────────┤
│                                       │
│ Scale:    [4x ▼]                     │
│ Fit:      [Pixel-Perfect ▼]         │
│ Aspect:   [DOS 4:3 ▼]               │
│                                       │
│ ─── Upscaling Filter ───             │
│                                       │
│ ○ Nearest Neighbor (Classic)         │
│ ● xBRZ 2x (Recommended) ← Selected   │
│ ○ xBRZ 3x (Higher quality)           │
│ ○ Scale2x (Fast)                     │
│ ○ Linear/Bilinear                    │
│                                       │
│ ─── CRT Effects ───                  │
│                                       │
│ ☑ Scanlines  [▬▬▬░░░░░] 30%        │
│                                       │
└──────────────────────────────────────┘
```

## Performance Budget

Target: **60 FPS** (16.67ms frame time)

| Component | Time Budget |
|-----------|-------------|
| Game rendering | 8-12ms |
| **Filter (xBRZ 2x)** | **~5ms** ✅ |
| **Filter (Scale2x)** | **<1ms** ✅ |
| Scanlines | <0.1ms ✅ |
| Presentation | 1-2ms |
| **Total** | **14-20ms** |

xBRZ 3x takes ~12ms, which is borderline. May need optimization or warning for users.

## Implementation Phases

### Phase 1: Pixel Upscaling ⭐ High Priority
- Add xBRZ library (port or use crate)
- Implement Scale2x algorithm
- Integrate into display pipeline
- Update preferences UI
- Config persistence

### Phase 2: CRT Scanlines ⭐ Medium Priority
- Implement scanline overlay
- Add intensity control
- UI checkbox + slider

### Phase 3: GPU Shaders ⏸️ Future/Optional
- Requires migration from minifb to wgpu/pixels
- Full CRT simulation (curvature, bloom, phosphor)
- **Defer to post-1.0 or Enhanced renderer**

## Testing Strategy

1. **Unit tests**: Filter output sizes, color preservation
2. **Visual regression**: Golden images in `docs/evidence/display-filters/`
3. **Performance benchmarks**: `cargo bench` for filter algorithms
4. **Cross-platform**: Windows, Linux, macOS validation

## Evidence Location

Capture screenshots showing filter differences:
```
docs/evidence/display-filters/
  ├── nearest-neighbor.png
  ├── xbrz-2x.png
  ├── scale2x.png
  ├── scanlines-off.png
  ├── scanlines-30.png
  ├── scanlines-70.png
  └── comparison-grid.png
```

## Work Item Status

**Milestone**: Interface and presentation (phase 5)  
**Item ID**: ui-24  
**Title**: Pixel-art upscaling filters (xBRZ, HQx, Scale2x) and optional CRT scanlines  
**Status**: todo  
**Added**: 2026-11-11  

Track in `dashboard/burndown.js`

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-11-11 | Add xBRZ as primary filter | Best quality for pixel art on modern displays |
| 2026-11-11 | Add Scale2x as fast option | Performance fallback for low-end systems |
| 2026-11-11 | Keep Nearest as default | Preserve authentic DOS experience by default |
| 2026-11-11 | Defer GPU CRT shaders | Architectural blocker (minifb doesn't support shaders) |
| 2026-11-11 | Single unified filter (not separate texture/pixel) | Software renderer produces one 320×200 framebuffer |

## Owner Questions

Before implementation, confirm:

1. ✅ **Default filter**: Keep Nearest Neighbor as default? (Recommendation: Yes, for purity)
2. ✅ **Performance budget**: 5ms acceptable for filters? (Recommendation: Yes, with auto-disable option)
3. ✅ **Filter scope**: Apply to entire 320×200 frame including HUD? (Recommendation: Yes, consistent)
4. ✅ **GPU shaders**: Defer until Enhanced renderer or post-1.0? (Recommendation: Defer)
5. ⏳ **HQx inclusion**: Add HQ2x/HQ4x alongside xBRZ? (Recommendation: Start with xBRZ only)

## References

- Original display implementation: `docs/notes/display.md`
- Rendering design: `docs/RENDERING_DESIGN.md` (if exists)
- Architecture notes: `docs/ARCHITECTURE.md` §3
- xBRZ algorithm: https://sourceforge.net/projects/xbrz/
- Scale2x: https://www.scale2x.it/
- DOSBox scaling: https://www.dosbox.com/wiki/Configuration:Display

## Credits

Design and analysis: Claude (Anthropic) in consultation with project owner  
Date: 2026-11-11  
Context: FFE-RS-Classic pure software renderer architecture

---

**Ready to implement?** Start with `display-filters-implementation-guide.md` for step-by-step instructions.
