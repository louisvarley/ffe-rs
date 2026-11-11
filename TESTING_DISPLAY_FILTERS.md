# Testing Display Filters - Quick Start Guide

## How to Test the New Display Filters

The display filter implementation is complete and ready for testing. Here's how to try it out:

### Step 1: Build and Run

```powershell
# Build the game (if not already built)
cargo build --release

# Run the game
.\play.bat
# Or directly:
.\target\release\ffe-launcher.exe
```

### Step 2: Open Display Options

Once in-game:

1. Press **Escape** to open the preferences menu
2. Click on **Display Options** (or navigate with arrow keys)

You should now see the updated Display Options page.

### Step 3: Test Filter Options

You'll see 5 filter choices (radio buttons):

#### 1. **Nearest** (Default - Classic Look)
- Original sharp, blocky pixels
- Fastest performance
- Authentic DOS experience
- **Try it**: Should look exactly like before

#### 2. **Linear** (Smooth Blur)
- Bilinear filtering
- Smooths pixels when scaling
- Slightly blurry
- **Try it**: Most noticeable at non-integer scales

#### 3. **xBRZ 2x** (Recommended - New!)
- Edge-aware pixel upscaling
- Smooth curves, sharp edges
- Best for modern displays
- **Try it**: Should look noticeably smoother while keeping detail
- **Expected**: ~5ms added frame time

#### 4. **xBRZ 3x** (Highest Quality - New!)
- Higher quality than 2x
- Slower processing
- **Try it**: Best quality, but may impact performance
- **Expected**: ~12ms added frame time
- **Note**: May drop below 60 FPS on older systems

#### 5. **Scale2x** (Fast - New!)
- Simple edge-directed scaling
- Subtle smoothing
- Very fast
- **Try it**: Slight improvement over Nearest
- **Expected**: <1ms added frame time

### Step 4: Test Scanlines

Below the Fullscreen toggle, you'll see:

#### **Scanlines** (Checkbox)
- Click to toggle CRT scanline effect on/off
- Should see horizontal dark lines appear

#### **Intensity** (Appears when Scanlines enabled)
- Click to cycle through intensity levels
- Watch the percentage change: 20% → 30% → 50% → 70% → 90%
- Higher percentage = darker scanlines

### Step 5: Visual Comparison

Compare the filters side-by-side:

1. **Spaceships**: Look at edges and curves
   - Nearest: Sharp, blocky
   - xBRZ 2x: Smooth curves, sharp lines
   - Scale2x: Slightly softer than Nearest

2. **Planet Surfaces**: Look at terrain texture
   - Nearest: Visible triangles
   - xBRZ 2x: Smoother gradients
   - Linear: Soft/blurry

3. **HUD Text**: Check readability
   - Nearest: Crisp
   - xBRZ 2x: Smooth but clear
   - Scale2x: Sharp with slight smoothing

4. **Station Interiors**: Look at posters and details
   - xBRZ should make pixel art look polished
   - Scanlines should add CRT nostalgia

### Step 6: Performance Check

Monitor frame rate (if dev tools available):

- **Target**: 60 FPS (16.67ms per frame)
- **Nearest/Linear/Scale2x**: Should maintain 60 FPS easily
- **xBRZ 2x**: Should maintain 60 FPS on modern CPUs (2015+)
- **xBRZ 3x**: May drop to 50-55 FPS on older systems

**Performance test scenarios**:
1. Idle at station - lowest load
2. Flying near planet with ships - medium load
3. Combat with explosions - highest load

If frame rate drops noticeably, fall back to Scale2x or Nearest.

### Step 7: Persistence Test

1. Choose a filter (e.g., xBRZ 2x)
2. Enable scanlines at 50%
3. Exit the game (Escape → Quit)
4. Restart the game
5. Open Display Options again

**Expected**: Your filter and scanline settings should be preserved.

**Config file**: Check `saves/display.cfg` - should contain:
```ini
filter=xbrz-2x
scanlines=on
scanline-intensity=0.50
```

### Step 8: Edge Cases

Test unusual scenarios:

1. **Fullscreen Mode**:
   - Press Alt+Enter
   - Filters should work in fullscreen

2. **Window Resize**:
   - Drag window corners
   - Filters should scale properly

3. **Different Scales**:
   - Try 1x, 2x, 3x, 6x window scales
   - Filters should look good at all scales

4. **Aspect Ratios**:
   - Try "4:3 (DOS)" vs "Square" pixels
   - Filters should work with both

5. **Fit Modes**:
   - Try "Pixel-perfect" vs "Fill window"
   - Filters should handle both

### Expected Results Summary

| Filter | Visual Quality | Performance | Best For |
|--------|---------------|-------------|----------|
| Nearest | ⭐⭐⭐ Sharp | ⚡⚡⚡ Instant | Authenticity |
| Linear | ⭐⭐ Blurry | ⚡⚡⚡ Fast | Smoothing |
| Scale2x | ⭐⭐⭐ Crisp | ⚡⚡⚡ Fast | Light touch |
| xBRZ 2x | ⭐⭐⭐⭐ Smooth | ⚡⚡ Good | Modern displays |
| xBRZ 3x | ⭐⭐⭐⭐⭐ Best | ⚡ OK | High-end systems |

| Scanlines | Intensity | Effect |
|-----------|-----------|--------|
| Off | N/A | No effect |
| On | 20% | Subtle CRT look |
| On | 30% | **Recommended** |
| On | 50% | Pronounced CRT |
| On | 70-90% | Heavy CRT |

### Common Issues & Solutions

#### Issue: Filters don't apply
- **Check**: Are you in the Display Options page?
- **Check**: Did you click the radio button icon (not just the label)?
- **Solution**: Restart the game if needed

#### Issue: Performance drop with xBRZ 3x
- **Expected**: This is normal on older CPUs
- **Solution**: Use xBRZ 2x or Scale2x instead

#### Issue: Scanlines too dark
- **Solution**: Click "Intensity" to cycle to lower value (20-30%)

#### Issue: Can't see scanlines
- **Check**: Is your window at least 400 pixels tall?
- **Check**: Is the checkbox checked?
- **Check**: Try increasing intensity

#### Issue: Settings don't persist
- **Check**: Does `saves/display.cfg` exist and is writable?
- **Check**: Look in game directory for the file

### Taking Evidence Screenshots

For documentation:

1. **Comparison grid**:
   - Same scene (e.g., docked at station)
   - Capture with Nearest, Scale2x, xBRZ 2x

2. **Scanline examples**:
   - Off, 30%, 70%
   - Show the visible difference

3. **Use Windows Snipping Tool or**:
   ```powershell
   # If --capture-display is available:
   .\ffe-rs.exe --capture-display docs/evidence/display-filters/
   ```

Save to: `docs/evidence/display-filters/`

### Feedback Checklist

After testing, note:

- [ ] Which filter looks best to you?
- [ ] Any visual artifacts or glitches?
- [ ] Performance acceptable on your system?
- [ ] Settings persist correctly?
- [ ] Scanlines look good?
- [ ] Any missing features?
- [ ] UI clear and easy to use?

---

## Quick Reference

**Open Display Options**: Escape → Display Options  
**Toggle Scanlines**: Click the Scanlines checkbox  
**Change Intensity**: Click "Intensity" (cycles 20%→30%→50%→70%→90%)  
**Config File**: `saves/display.cfg`  

**Recommended Settings**:
- Modern displays: xBRZ 2x, scanlines off or 30%
- Retro nostalgia: Nearest or Scale2x, scanlines 50%
- Performance: Scale2x, scanlines 20%
- Authenticity: Nearest, scanlines off

Enjoy the enhanced display options!
