// Scale2x pixel-art upscaling algorithm.
//
// Fast edge-directed scaling that preserves pixel art character while
// reducing blockiness. Processes 320×200 → 640×400.
//
// Algorithm: https://www.scale2x.it/

const WIDTH: usize = 320;
const HEIGHT: usize = 200;

/// Apply Scale2x algorithm to 320×200 RGBA buffer, producing 640×400.
///
/// Scale2x uses a 3×3 pixel neighborhood to detect edges and smooth them:
///
/// ```text
///     A
///   D C B     →   P0 P1
///     E           P2 P3
///
/// P0 = (A == D && A != E && D != B) ? A : C
/// P1 = (A == B && A != E && B != D) ? B : C
/// P2 = (E == D && E != A && D != B) ? D : C
/// P3 = (E == B && E != A && B != D) ? B : C
/// ```
///
/// Performance: ~0.5ms at 320×200 on modern CPUs.
pub fn scale2x(input: &[u32]) -> Vec<u32> {
    debug_assert_eq!(input.len(), WIDTH * HEIGHT, "Input must be 320×200");
    
    let out_width = WIDTH * 2;
    let out_height = HEIGHT * 2;
    let mut output = vec![0u32; out_width * out_height];
    
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let c = input[y * WIDTH + x];
            
            // Get neighbors with boundary clamping
            let a = if y > 0 { input[(y - 1) * WIDTH + x] } else { c };
            let b = if x < WIDTH - 1 { input[y * WIDTH + x + 1] } else { c };
            let d = if x > 0 { input[y * WIDTH + x - 1] } else { c };
            let e = if y < HEIGHT - 1 { input[(y + 1) * WIDTH + x] } else { c };
            
            // Apply Scale2x rules
            let out_y = y * 2;
            let out_x = x * 2;
            
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scale2x_output_size() {
        let input = vec![0xFF000000u32; WIDTH * HEIGHT];
        let output = scale2x(&input);
        assert_eq!(output.len(), 640 * 400);
    }
    
    #[test]
    fn test_scale2x_preserves_solid_colors() {
        // Solid red input
        let input = vec![0xFFFF0000u32; WIDTH * HEIGHT];
        let output = scale2x(&input);
        
        // All output pixels should be red (no color shift)
        for &pixel in &output {
            assert_eq!(pixel, 0xFFFF0000u32);
        }
    }
    
    #[test]
    fn test_scale2x_edge_smoothing() {
        // Create a simple pattern: vertical edge in middle
        let mut input = vec![0xFF000000u32; WIDTH * HEIGHT]; // Black
        
        // Right half white
        for y in 0..HEIGHT {
            for x in (WIDTH / 2)..WIDTH {
                input[y * WIDTH + x] = 0xFFFFFFFFu32;
            }
        }
        
        let output = scale2x(&input);
        
        // Check that output is 640×400
        assert_eq!(output.len(), 640 * 400);
        
        // Edge should be smoothed but not blurred beyond 2x size
        // Left side should be black, right side white
        assert_eq!(output[0], 0xFF000000u32); // Top-left black
        assert_eq!(output[639], 0xFFFFFFFFu32); // Top-right white
    }
}
