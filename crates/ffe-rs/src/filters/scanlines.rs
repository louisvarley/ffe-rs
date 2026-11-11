// CRT scanline effect.
//
// Adds horizontal dark lines to simulate CRT display phosphor structure.
// Negligible performance cost (~0.1ms).

/// Apply CRT-style scanlines to buffer.
///
/// Darkens every other row to simulate the horizontal scanlines visible on
/// CRT monitors. The effect is configurable via intensity parameter.
///
/// # Arguments
///
/// * `buffer` - RGBA pixel buffer to modify in-place
/// * `width` - Buffer width in pixels
/// * `height` - Buffer height in pixels
/// * `intensity` - Darkening amount (0.0 = no effect, 1.0 = full black lines)
///
/// # Example
///
/// ```ignore
/// let mut buffer = vec![0xFFFFFFFFu32; 640 * 480];
/// apply_scanlines(&mut buffer, 640, 480, 0.3); // 30% darkening
/// ```
pub fn apply_scanlines(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    intensity: f32,
) {
    debug_assert_eq!(buffer.len(), width * height, "Buffer size mismatch");
    
    if intensity <= 0.0 {
        return; // No effect
    }
    
    let darken = (1.0 - intensity).clamp(0.0, 1.0);
    
    // Process every other row
    for y in (1..height).step_by(2) {
        let row_start = y * width;
        let row_end = row_start + width;
        
        for pixel in &mut buffer[row_start..row_end] {
            *pixel = darken_pixel(*pixel, darken);
        }
    }
}

/// Darken a single RGBA pixel by multiplying RGB components.
#[inline]
fn darken_pixel(pixel: u32, factor: f32) -> u32 {
    let r = ((pixel >> 16) & 0xFF) as f32;
    let g = ((pixel >> 8) & 0xFF) as f32;
    let b = (pixel & 0xFF) as f32;
    let a = (pixel >> 24) & 0xFF;
    
    let r = (r * factor) as u32;
    let g = (g * factor) as u32;
    let b = (b * factor) as u32;
    
    (a << 24) | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scanlines_zero_intensity() {
        let mut buffer = vec![0xFFFFFFFFu32; 640 * 400];
        let original = buffer.clone();
        
        apply_scanlines(&mut buffer, 640, 400, 0.0);
        
        // Buffer should be unchanged
        assert_eq!(buffer, original);
    }
    
    #[test]
    fn test_scanlines_darken_every_other_line() {
        let mut buffer = vec![0xFFFFFFFFu32; 10 * 4]; // 10×4 white image
        apply_scanlines(&mut buffer, 10, 4, 0.5);
        
        // Lines 0 and 2 should be unchanged (white)
        for x in 0..10 {
            assert_eq!(buffer[0 * 10 + x], 0xFFFFFFFFu32);
            assert_eq!(buffer[2 * 10 + x], 0xFFFFFFFFu32);
        }
        
        // Lines 1 and 3 should be darkened
        for x in 0..10 {
            let pixel = buffer[1 * 10 + x];
            assert!(pixel < 0xFFFFFFFFu32);
            assert!(pixel > 0xFF000000u32); // Not fully black
            
            let pixel = buffer[3 * 10 + x];
            assert!(pixel < 0xFFFFFFFFu32);
            assert!(pixel > 0xFF000000u32);
        }
    }
    
    #[test]
    fn test_scanlines_preserve_alpha() {
        let mut buffer = vec![0x80FF0000u32; 10 * 4]; // Semi-transparent red
        apply_scanlines(&mut buffer, 10, 4, 0.5);
        
        // Alpha channel should be unchanged
        for &pixel in &buffer {
            assert_eq!((pixel >> 24) & 0xFF, 0x80);
        }
    }
    
    #[test]
    fn test_scanlines_full_intensity() {
        let mut buffer = vec![0xFFFFFFFFu32; 10 * 4];
        apply_scanlines(&mut buffer, 10, 4, 1.0);
        
        // Odd rows should be fully black (except alpha)
        for x in 0..10 {
            assert_eq!(buffer[1 * 10 + x], 0xFF000000u32);
            assert_eq!(buffer[3 * 10 + x], 0xFF000000u32);
        }
    }
    
    #[test]
    fn test_darken_pixel() {
        // White darkened by 50%
        let darkened = darken_pixel(0xFFFFFFFF, 0.5);
        let expected = 0xFF7F7F7Fu32; // RGB ~127
        assert!((darkened as i32 - expected as i32).abs() < 2); // Allow rounding
        
        // Black stays black
        assert_eq!(darken_pixel(0xFF000000, 0.5), 0xFF000000);
        
        // Alpha preserved
        let darkened = darken_pixel(0x80FFFFFF, 0.5);
        assert_eq!((darkened >> 24) & 0xFF, 0x80);
    }
}
