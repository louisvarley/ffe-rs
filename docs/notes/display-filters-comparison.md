# Display Filter Visual Comparison

## Filter Options Illustrated

This document provides ASCII art illustrations and descriptions of how different filters affect the 320×200 display output.

## Original 320×200 Frame

Imagine a simple pixel-art spaceship:

```
Original (8×8 pixels):
████████
██    ██
██ ██ ██
████████
██    ██
██    ██
██    ██
████████
```

## 1. Nearest Neighbor (Current Default)

Each pixel becomes an exact NxN block:

```
Nearest Neighbor 2x (16×16):
████████████████
████████████████
████        ████
████        ████
████  ████  ████
████  ████  ████
████████████████
████████████████
████        ████
████        ████
████        ████
████        ████
████        ████
████        ████
████████████████
████████████████
```

**Characteristics:**
- ✅ Sharp, crisp pixels
- ✅ Authentic retro look
- ❌ Can look blocky on large modern displays
- ⚡ Fastest (negligible CPU)

## 2. Linear/Bilinear (Current Option)

Blends between pixels:

```
Linear/Bilinear 2x (conceptual):
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓░░  ▓▓  ░░  ▓█
█▓░░  ▓▓  ░░  ▓█
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓░░    ░░    ▓█
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
```

**Characteristics:**
- ✅ Smooth, reduces blockiness
- ❌ Can look blurry
- ❌ Loses pixel-art sharpness
- ⚡ Fast (~0.5-1.5ms)

## 3. xBRZ 2x (Recommended New Filter)

Edge-aware upscaling that smooths curves but keeps edges sharp:

```
xBRZ 2x (conceptual):
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
█▓██████████████
██░░        ░░██
██░░        ░░██
██░░  ████  ░░██
██░░  ████  ░░██
██████████████▓█
██████████████▓█
██░░        ░░██
██░░        ░░██
██░░        ░░██
██░░        ░░██
██░░        ░░██
██░░        ░░██
█▓██████████████
█▓▓▓▓▓▓▓▓▓▓▓▓▓▓█
```

**Characteristics:**
- ✅ Smooth diagonals and curves
- ✅ Sharp straight edges preserved
- ✅ Maintains pixel-art character
- ✅ Excellent for modern displays
- ⚡ Medium speed (~5ms at 320×200)

## 4. Scale2x (Fast Alternative)

Simple edge-directed scaling:

```
Scale2x (conceptual):
████████████████
████████████████
████░░    ░░████
████░░    ░░████
████  ████  ████
████  ████  ████
████████████████
████████████████
████░░    ░░████
████░░    ░░████
████░░    ░░████
████░░    ░░████
████░░    ░░████
████░░    ░░████
████████████████
████████████████
```

**Characteristics:**
- ✅ Subtle smoothing
- ✅ Very fast
- ✅ Good for low-end systems
- ⚡ Fastest upscaler (<1ms)

## 5. CRT Scanlines (Overlay Effect)

Adds horizontal dark lines to simulate CRT:

```
Nearest Neighbor + Scanlines:
████████████████
▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ ← darkened scanline
████        ████
▒▒▒▒        ▒▒▒▒
████  ████  ████
▒▒▒▒  ▒▒▒▒  ▒▒▒▒
████████████████
▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
████        ████
▒▒▒▒        ▒▒▒▒
████        ████
▒▒▒▒        ▒▒▒▒
████        ████
▒▒▒▒        ▒▒▒▒
████████████████
▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
```

**Characteristics:**
- ✅ Authentic CRT look
- ✅ Configurable intensity
- ✅ Combines with any base filter
- ⚡ Negligible performance cost

## Real-World Impact on FFE Content

### Spaceship Models
- **Nearest**: Crisp edges, geometric look
- **Linear**: Soft edges, can lose detail
- **xBRZ**: Smooth curves, sharp details - BEST
- **Scale2x**: Slightly smoother than nearest

### Planet Surfaces (Textured Terrain)
- **Nearest**: Chunky, visible triangles
- **Linear**: Soft textures, hides low-res
- **xBRZ**: Smooth terrain, natural gradients - BEST
- **Scale2x**: Subtle improvement

### HUD/Text (Bitmap Font)
- **Nearest**: Sharp, readable
- **Linear**: Slightly blurry
- **xBRZ**: Smooth but clear - BEST
- **Scale2x**: Sharp, slight smoothing

### Station Interiors (Lift Scenes)
- **Nearest**: Pixel-art posters, sharp lines
- **Linear**: Soft focus effect
- **xBRZ**: Polished look while keeping detail - BEST
- **Scale2x**: Crisp with edge enhancement

## Combination Recommendations

### "Pure Classic" (Current Default)
```
Filter: Nearest Neighbor
Scanlines: Off
Aspect: DOS 4:3
Scale: 4x Pixel-Perfect
```
→ Authentic DOS experience

### "Modern Enhanced" (Recommended)
```
Filter: xBRZ 2x
Scanlines: Off
Aspect: Square
Scale: Fill Window
```
→ Best for 1080p/1440p/4K displays

### "Retro CRT"
```
Filter: Nearest Neighbor or Scale2x
Scanlines: On (Medium intensity)
Aspect: DOS 4:3
Scale: Fill Window
```
→ Simulates playing on a CRT monitor

### "Performance" (Low-End Systems)
```
Filter: Scale2x
Scanlines: Off
Aspect: Square
Scale: 2x or 3x Pixel-Perfect
```
→ Visual improvement without heavy processing

## Performance Scaling

At 320×200 input resolution:

```
Filter Pipeline:

Game → 320×200 indexed buffer
         ↓
      Palette lookup
         ↓
      320×200 RGBA
         ↓
    [FILTER STAGE]  ← xBRZ: 5-12ms, Scale2x: <1ms, Linear: 0.5ms, Nearest: 0.1ms
         ↓
    Scaled buffer (640×400, 1280×960, or window size)
         ↓
    [SCANLINES]     ← <0.1ms
         ↓
    Window presentation
```

**Total budget**: 16.67ms for 60 FPS
- Game rendering: ~8-12ms
- Filter: 0-12ms (filter-dependent)
- Presentation: ~1-2ms
- **Margin**: 2-7ms

## Filter Decision Tree

```
Start Here
    ↓
Do you want the most authentic DOS experience?
    YES → Nearest Neighbor + DOS 4:3
    NO ↓
         ↓
    Do you have a modern high-res display (1080p+)?
        YES → xBRZ 2x + Square/DOS 4:3 (your choice)
        NO ↓
            ↓
        Is your system slow/old?
            YES → Scale2x
            NO → xBRZ 2x
```

## Implementation Notes

### Filter Application Point

```rust
// Current pipeline in display.rs:
Screen::logical_pixels() // 320×200 indexed
    → palette lookup
    → Screen::present()   // 320×200 RGBA
    → display::compose()  // NEW: filter here
    → minifb window

// Pseudo-code for new filter stage:
fn compose(frame_320x200: &[u32]) -> Vec<u32> {
    let filtered = match settings.filter {
        Filter::Nearest => upscale_nearest(frame_320x200),
        Filter::Linear => upscale_linear(frame_320x200),
        Filter::XbrzX2 => xbrz_2x(frame_320x200),
        Filter::Scale2x => scale2x(frame_320x200),
    };
    
    if settings.scanlines {
        apply_scanlines(&mut filtered, settings.intensity);
    }
    
    filtered
}
```

### Filter Parameter Tuning

xBRZ has a `blend` parameter (0-100):
- **0**: Maximum sharpness (like nearest neighbor)
- **50**: Balanced (recommended default)
- **100**: Maximum smoothing

Allow user to configure this:
```
☐ Advanced: xBRZ Blend [Sharp ▬▬▬▬░░░░░░ Smooth]
                              ↑
                           default: 50
```

## Conclusion

The **xBRZ 2x** filter is the ideal addition because:
1. ✅ Perfect fit for 320×200 software-rendered content
2. ✅ Makes pixel art look great on modern displays
3. ✅ Preserves artistic intent (smooth where intended, sharp where intended)
4. ✅ Reasonable performance cost (5ms on most systems)
5. ✅ Used successfully by DOSBox, RetroArch, and many emulators

Start with xBRZ 2x + Scale2x as the two new options, with scanlines as a separate toggle.
