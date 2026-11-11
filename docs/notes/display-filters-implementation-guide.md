# Display Filters - Implementation Guide

Quick reference for implementing **ui-24**: Pixel-art upscaling filters and CRT scanlines.

## Prerequisites

Read first:
- `docs/notes/display.md` - Current display implementation
- `docs/notes/display-filters-design.md` - Full design specification
- `docs/notes/display-filters-comparison.md` - Visual examples

## Step-by-Step Implementation

### Phase 1: xBRZ 2x/3x Filter

#### 1.1 Add Dependencies

```toml
# Cargo.toml
[dependencies]
# Option A: Use existing crate (if available)
xbrz = "1.0"  # Check crates.io for actual version

# Option B: Implement from reference
# No dependency needed, port the algorithm
```

Reference implementation: https://sourceforge.net/projects/xbrz/

#### 1.2 Extend Filter Enum

```rust
// src/display.rs or src/settings.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Filter {
    Nearest,
    Linear,
    #[serde(rename = "xbrz-2x")]
    XbrzX2,
    #[serde(rename = "xbrz-3x")]
    XbrzX3,
    Scale2x,
}

impl Filter {
    pub fn name(&self) -> &'static str {
        match self {
            Filter::Nearest => "Nearest Neighbor",
            Filter::Linear => "Linear/Bilinear",
            Filter::XbrzX2 => "xBRZ 2x",
            Filter::XbrzX3 => "xBRZ 3x",
            Filter::Scale2x => "Scale2x",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            Filter::Nearest => "Sharp, blocky pixels (classic)",
            Filter::Linear => "Smooth blur (current)",
            Filter::XbrzX2 => "Recommended: smooth edges, sharp details",
            Filter::XbrzX3 => "Higher quality, slower",
            Filter::Scale2x => "Fast, subtle enhancement",
        }
    }
}
```

#### 1.3 Implement xBRZ Wrapper

```rust
// src/filters/xbrz.rs (new file)

/// Apply xBRZ scaling to 320×200 RGBA buffer
pub fn xbrz_scale(
    input: &[u32],     // 320×200 RGBA pixels
    factor: usize,     // 2 or 3
) -> Vec<u32> {
    const WIDTH: usize = 320;
    const HEIGHT: usize = 200;
    
    let out_width = WIDTH * factor;
    let out_height = HEIGHT * factor;
    let mut output = vec![0u32; out_width * out_height];
    
    // Call xBRZ library or custom implementation
    // xbrz::scale(input, WIDTH, HEIGHT, &mut output, factor);
    
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_xbrz_2x_size() {
        let input = vec![0xFF000000u32; 320 * 200];
        let output = xbrz_scale(&input, 2);
        assert_eq!(output.len(), 640 * 400);
    }
    
    #[test]
    fn test_xbrz_3x_size() {
        let input = vec![0xFF000000u32; 320 * 200];
        let output = xbrz_scale(&input, 3);
        assert_eq!(output.len(), 960 * 600);
    }
}
```

#### 1.4 Implement Scale2x (Fast Alternative)

```rust
// src/filters/scale2x.rs (new file)

/// Fast Scale2x algorithm - simpler than xBRZ
pub fn scale2x(input: &[u32]) -> Vec<u32> {
    const WIDTH: usize = 320;
    const HEIGHT: usize = 200;
    let mut output = vec![0u32; WIDTH * 2 * HEIGHT * 2];
    
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let c = input[y * WIDTH + x];
            
            // Get neighbors (with boundary checks)
            let a = if y > 0 { input[(y - 1) * WIDTH + x] } else { c };
            let b = if x < WIDTH - 1 { input[y * WIDTH + x + 1] } else { c };
            let d = if x > 0 { input[y * WIDTH + x - 1] } else { c };
            let e = if y < HEIGHT - 1 { input[(y + 1) * WIDTH + x] } else { c };
            
            // Scale2x rule:
            // P0 = (A == D && A != E && D != B) ? A : C
            // P1 = (A == B && A != E && B != D) ? B : C
            // P2 = (E == D && E != A && D != B) ? D : C
            // P3 = (E == B && E != A && B != D) ? B : C
            
            let out_y = y * 2;
            let out_x = x * 2;
            let out_width = WIDTH * 2;
            
            output[out_y * out_width + out_x] = 
                if a == d && a != e && d != b { a } else { c };
            output[out_y * out_width + out_x + 1] = 
                if a == b && a != e && b != d { b } else { c };
            output[(out_y + 1) * out_width + out_x] = 
                if e == d && e != a && d != b { d } else { c };
            output[(out_y + 1) * out_width + out_x + 1] = 
                if e == b && e != a && b != d { b } else { c };
        }
    }
    
    output
}
```

#### 1.5 Integrate into Display Pipeline

```rust
// src/display.rs

use crate::filters::{xbrz, scale2x};

pub fn compose(
    logical: &[u32],      // 320×200 RGBA from Screen::present
    settings: &DisplaySettings,
    window_size: (usize, usize),
) -> Vec<u32> {
    // Step 1: Apply upscaling filter
    let (intermediate, intermediate_size) = match settings.filter {
        Filter::Nearest | Filter::Linear => {
            // Use existing path - scale directly to window
            (logical.to_vec(), (320, 200))
        }
        Filter::XbrzX2 => {
            let filtered = xbrz::xbrz_scale(logical, 2);
            (filtered, (640, 400))
        }
        Filter::XbrzX3 => {
            let filtered = xbrz::xbrz_scale(logical, 3);
            (filtered, (960, 600))
        }
        Filter::Scale2x => {
            let filtered = scale2x::scale2x(logical);
            (filtered, (640, 400))
        }
    };
    
    // Step 2: Scale to window (if needed)
    let scaled = if intermediate_size == window_size {
        intermediate
    } else {
        scale_to_window(&intermediate, intermediate_size, window_size, settings)
    };
    
    // Step 3: Apply scanlines (if enabled)
    if settings.scanlines {
        apply_scanlines(&mut scaled, window_size, settings.scanline_intensity);
    }
    
    scaled
}

fn scale_to_window(
    buffer: &[u32],
    buffer_size: (usize, usize),
    window_size: (usize, usize),
    settings: &DisplaySettings,
) -> Vec<u32> {
    // Use existing nearest/linear scaling logic
    // ...
}
```

### Phase 2: CRT Scanlines

#### 2.1 Extend DisplaySettings

```rust
// src/settings.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub scale: u8,
    pub fit: FitMode,
    pub filter: Filter,
    pub aspect: PixelAspect,
    
    // New fields:
    #[serde(default)]
    pub scanlines: bool,
    
    #[serde(default = "default_scanline_intensity")]
    pub scanline_intensity: f32,  // 0.0 to 1.0
}

fn default_scanline_intensity() -> f32 {
    0.3  // 30% darkening
}
```

#### 2.2 Implement Scanline Effect

```rust
// src/filters/scanlines.rs (new file)

/// Apply CRT-style scanlines to buffer
pub fn apply_scanlines(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    intensity: f32,  // 0.0 = no effect, 1.0 = full black lines
) {
    let darken = (1.0 - intensity).max(0.0).min(1.0);
    
    for y in 0..height {
        // Every other line is darkened
        if y % 2 == 1 {
            for x in 0..width {
                let idx = y * width + x;
                let pixel = buffer[idx];
                
                // Extract RGBA components
                let r = ((pixel >> 16) & 0xFF) as f32;
                let g = ((pixel >> 8) & 0xFF) as f32;
                let b = (pixel & 0xFF) as f32;
                let a = ((pixel >> 24) & 0xFF);
                
                // Darken RGB
                let r = (r * darken) as u32;
                let g = (g * darken) as u32;
                let b = (b * darken) as u32;
                
                buffer[idx] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scanlines_darken_every_other_line() {
        let mut buffer = vec![0xFFFFFFFFu32; 10 * 4]; // 10x4 white image
        apply_scanlines(&mut buffer, 10, 4, 0.5);
        
        // Lines 0 and 2 should be unchanged (white)
        assert_eq!(buffer[0], 0xFFFFFFFF);
        assert_eq!(buffer[20], 0xFFFFFFFF);
        
        // Lines 1 and 3 should be darkened (gray)
        assert!(buffer[10] < 0xFFFFFFFF);
        assert!(buffer[30] < 0xFFFFFFFF);
    }
}
```

### Phase 3: Configuration Persistence

#### 3.1 Update Config File Format

```rust
// src/settings.rs

impl DisplaySettings {
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let config = format!(
            "scale={}\nfit={}\nfilter={}\naspect={}\nscanlines={}\nscanline-intensity={:.2}\n",
            self.scale,
            self.fit.to_config_string(),
            self.filter.to_config_string(),
            self.aspect.to_config_string(),
            if self.scanlines { "on" } else { "off" },
            self.scanline_intensity,
        );
        fs::write(path, config)
    }
    
    pub fn load(path: &Path) -> io::Result<Self> {
        // Parse existing fields + new scanline fields
        // ...
    }
}

impl Filter {
    fn to_config_string(&self) -> &'static str {
        match self {
            Filter::Nearest => "nearest",
            Filter::Linear => "linear",
            Filter::XbrzX2 => "xbrz-2x",
            Filter::XbrzX3 => "xbrz-3x",
            Filter::Scale2x => "scale2x",
        }
    }
    
    fn from_config_string(s: &str) -> Option<Self> {
        match s {
            "nearest" => Some(Filter::Nearest),
            "linear" => Some(Filter::Linear),
            "xbrz-2x" => Some(Filter::XbrzX2),
            "xbrz-3x" => Some(Filter::XbrzX3),
            "scale2x" => Some(Filter::Scale2x),
            _ => None,
        }
    }
}
```

### Phase 4: Preferences UI

#### 4.1 Update Display Options Page

```rust
// src/prefs.rs or wherever the Display Options page is drawn

fn draw_display_page(&mut self, screen: &mut Screen) {
    // Existing options: scale, fit, aspect
    // ...
    
    // New filter selection
    self.draw_text(screen, "Upscaling Filter:", row);
    let filters = [
        Filter::Nearest,
        Filter::Linear,
        Filter::XbrzX2,
        Filter::XbrzX3,
        Filter::Scale2x,
    ];
    for (i, filter) in filters.iter().enumerate() {
        let row = base_row + i;
        let selected = self.settings.filter == *filter;
        self.draw_radio(screen, row, selected);
        self.draw_text(screen, filter.name(), row);
        self.draw_text(screen, filter.description(), row + 1);
    }
    
    // Scanline toggle
    self.draw_checkbox(screen, "Scanlines", self.settings.scanlines);
    
    // Scanline intensity slider (if enabled)
    if self.settings.scanlines {
        self.draw_slider(
            screen,
            "Intensity",
            self.settings.scanline_intensity,
            0.0,
            1.0,
        );
    }
}
```

### Phase 5: Performance Monitoring

#### 5.1 Add Frame Time Tracking

```rust
// src/main.rs or game loop

use std::time::Instant;

fn game_loop() {
    let mut frame_times = Vec::new();
    
    loop {
        let frame_start = Instant::now();
        
        // Game logic
        // ...
        
        // Display filtering
        let filter_start = Instant::now();
        let filtered = display::compose(&frame, &settings, window_size);
        let filter_time = filter_start.elapsed();
        
        // Presentation
        // ...
        
        let frame_time = frame_start.elapsed();
        
        // Warn if too slow
        if frame_time.as_millis() > 16 {
            eprintln!(
                "Warning: Frame time {}ms exceeds 60 FPS budget (filter: {}ms)",
                frame_time.as_millis(),
                filter_time.as_millis()
            );
        }
        
        // Optional: auto-disable filter if consistently slow
        if frame_time.as_millis() > 20 && settings.filter != Filter::Nearest {
            eprintln!("Auto-disabling filter due to performance");
            settings.filter = Filter::Nearest;
        }
    }
}
```

## Testing

### Unit Tests

```rust
// src/filters/mod.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_filter_sizes() {
        let input = vec![0xFF000000u32; 320 * 200];
        
        // xBRZ 2x should produce 640×400
        let out2x = xbrz::xbrz_scale(&input, 2);
        assert_eq!(out2x.len(), 640 * 400);
        
        // xBRZ 3x should produce 960×600
        let out3x = xbrz::xbrz_scale(&input, 3);
        assert_eq!(out3x.len(), 960 * 600);
        
        // Scale2x should produce 640×400
        let scale2x_out = scale2x::scale2x(&input);
        assert_eq!(scale2x_out.len(), 640 * 400);
    }
    
    #[test]
    fn test_filter_preserves_solid_colors() {
        // Solid red input
        let input = vec![0xFFFF0000u32; 320 * 200];
        let output = xbrz::xbrz_scale(&input, 2);
        
        // All output pixels should be red (no color shift)
        for &pixel in &output {
            let r = (pixel >> 16) & 0xFF;
            assert_eq!(r, 0xFF);
        }
    }
}
```

### Visual Regression Tests

```rust
// tests/display_filters.rs

#[test]
fn test_filter_visual_regression() {
    // Load reference 320×200 frame
    let frame = load_test_frame("test_data/ship_view.raw");
    
    // Apply each filter
    let nearest = apply_filter(&frame, Filter::Nearest);
    let xbrz2x = apply_filter(&frame, Filter::XbrzX2);
    let scale2x = apply_filter(&frame, Filter::Scale2x);
    
    // Compare against golden images
    compare_with_golden(&nearest, "test_data/golden/nearest.png");
    compare_with_golden(&xbrz2x, "test_data/golden/xbrz2x.png");
    compare_with_golden(&scale2x, "test_data/golden/scale2x.png");
    
    // Or save for manual inspection
    save_test_output(&xbrz2x, "test_output/xbrz2x.png");
}
```

### Performance Benchmarks

```rust
// benches/filters.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_filters(c: &mut Criterion) {
    let frame = vec![0xFF000000u32; 320 * 200];
    
    c.bench_function("xbrz_2x", |b| {
        b.iter(|| xbrz::xbrz_scale(black_box(&frame), 2))
    });
    
    c.bench_function("xbrz_3x", |b| {
        b.iter(|| xbrz::xbrz_scale(black_box(&frame), 3))
    });
    
    c.bench_function("scale2x", |b| {
        b.iter(|| scale2x::scale2x(black_box(&frame)))
    });
}

criterion_group!(benches, benchmark_filters);
criterion_main!(benches);
```

## Rollout Plan

1. ✅ **Design phase** (current)
   - Document architecture
   - Get owner sign-off

2. **Implementation phase**
   - Week 1: xBRZ integration + Scale2x
   - Week 2: Scanlines + config persistence
   - Week 3: UI integration + testing
   - Week 4: Performance tuning + evidence

3. **Testing phase**
   - Unit tests passing
   - Visual regression suite
   - Performance benchmarks
   - Cross-platform validation

4. **Release phase**
   - Add to changelog
   - Update user documentation
   - Capture evidence frames
   - Tag version

## Success Criteria

- [ ] xBRZ 2x/3x filters implemented and working
- [ ] Scale2x fast filter implemented
- [ ] Scanline effect with intensity control
- [ ] Config persistence in `saves/display.cfg`
- [ ] Preferences UI with radio buttons/checkboxes/sliders
- [ ] Performance: 60 FPS maintained on target hardware
- [ ] Tests: All unit tests passing
- [ ] Evidence: Screenshots in `docs/evidence/display-filters/`
- [ ] Documentation: Updated `docs/notes/display.md`

## References

- xBRZ source: https://sourceforge.net/projects/xbrz/
- Scale2x algorithm: https://www.scale2x.it/algorithm
- Rust xbrz crate (if exists): https://crates.io/search?q=xbrz
- Current display.rs implementation: (project-specific)

---

**Next Steps**: Review design docs with owner, then start Phase 1 implementation.
