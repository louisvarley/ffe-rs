//! The modern graphics engine: a presentation layer kept apart from the original's renderer.
//!
//! The game draws every frame the original's way into the authoritative 320x200 logical frame
//! (`screen::Screen::logical_pixels`). Classic graphics show that frame as it is, scaled by
//! `display`. Modern graphics hand the same finished frame to this module, which draws it at
//! the window's resolution with its own techniques. Nothing here reads or writes game state,
//! the simulation, the RNG or the logical frame: switching between the two is a per-frame
//! choice of how to show pixels the game has already decided, so it can flip at any time
//! without a restart and without changing how the game plays. See `docs/notes/modern-graphics.md`.
//!
//! First technique: **edge-directed upscaling.** The frame is enlarged 4x with Scale2x applied
//! twice (the AdvMAME rules: a pixel's corner takes a neighbour's colour only where two
//! neighbours agree and the others do not), which turns the staircases of polygon edges,
//! lines, circles and the bitmap font into smooth diagonals while leaving flat areas and
//! one-pixel details exactly as they were: no new colours, no blur. The 4x image is then
//! resampled bilinearly into the window's fit rectangle (which also applies the DOS 4:3
//! correction smoothly instead of repeating every fifth row).

use crate::display::Rect;
use crate::indexed::{LOGICAL_HEIGHT, LOGICAL_WIDTH};

/// Which engine shows the frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Graphics {
    /// The original's pixels, scaled by the display options only. The default.
    #[default]
    Classic,
    /// The modern engine.
    Modern,
}

impl Graphics {
    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Modern => "modern",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "classic" => Some(Self::Classic),
            "modern" => Some(Self::Modern),
            _ => None,
        }
    }

    /// The other engine.
    pub fn flipped(self) -> Self {
        match self {
            Self::Classic => Self::Modern,
            Self::Modern => Self::Classic,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic graphics",
            Self::Modern => "Modern graphics",
        }
    }
}

/// How much the edge-directed pass enlarges the frame (Scale2x twice).
pub const UPSCALE: usize = 4;

/// Scale2x (AdvMAME2x): `source` is `width` by `height`; returns the image at twice the size.
/// Each pixel `P` becomes four, from its neighbours above (`A`), right (`B`), left (`C`) and
/// below (`D`); edges repeat the border pixel.
pub fn scale2x(source: &[u32], width: usize, height: usize) -> Vec<u32> {
    let mut out = vec![0u32; width * height * 4];
    let at = |x: usize, y: usize| source[y * width + x];
    let out_width = width * 2;
    for y in 0..height {
        for x in 0..width {
            let p = at(x, y);
            let a = at(x, y.saturating_sub(1));
            let b = at((x + 1).min(width - 1), y);
            let c = at(x.saturating_sub(1), y);
            let d = at(x, (y + 1).min(height - 1));
            let (mut e0, mut e1, mut e2, mut e3) = (p, p, p, p);
            if a != d && c != b {
                if c == a {
                    e0 = a;
                }
                if a == b {
                    e1 = b;
                }
                if c == d {
                    e2 = c;
                }
                if d == b {
                    e3 = d;
                }
            }
            let row = 2 * y * out_width + 2 * x;
            out[row] = e0;
            out[row + 1] = e1;
            out[row + out_width] = e2;
            out[row + out_width + 1] = e3;
        }
    }
    out
}

/// The modern engine's working buffers, kept between frames.
#[derive(Default)]
pub struct ModernRenderer {
    upscaled: Vec<u32>,
}

impl ModernRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The edge-directed 4x image of a 320x200 logical frame (1280 by 800).
    pub fn upscale(&mut self, logical: &[u32]) -> &[u32] {
        let twice = scale2x(logical, LOGICAL_WIDTH, LOGICAL_HEIGHT);
        self.upscaled = scale2x(&twice, LOGICAL_WIDTH * 2, LOGICAL_HEIGHT * 2);
        &self.upscaled
    }

    /// Draw the logical frame into `rect` of `target` (a `width`-pixel-wide window buffer).
    pub fn render(&mut self, logical: &[u32], target: &mut [u32], width: usize, rect: Rect) {
        if logical.len() < LOGICAL_WIDTH * LOGICAL_HEIGHT || rect.width == 0 || rect.height == 0 {
            return;
        }
        self.upscale(logical);
        crate::display::resample_linear(
            &self.upscaled,
            LOGICAL_WIDTH * UPSCALE,
            LOGICAL_HEIGHT * UPSCALE,
            target,
            width,
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_areas_and_single_pixels_are_unchanged() {
        // A flat image stays flat; a lone pixel becomes a 2x2 block (no neighbours agree).
        let mut image = vec![1u32; 9];
        image[4] = 7;
        let out = scale2x(&image, 3, 3);
        assert_eq!(out.len(), 36);
        let at = |x: usize, y: usize| out[y * 6 + x];
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            assert_eq!(at(x, y), 7);
        }
        assert_eq!(out.iter().filter(|p| **p == 7).count(), 4);
    }

    #[test]
    fn a_staircase_becomes_a_diagonal() {
        // A diagonal step:
        //   1 0
        //   0 0   the top-left pixel's bottom-right corner stays 1, the zeros next to it fill
        // corners where their own neighbours agree, cutting the step diagonally.
        let image = [
            1, 1, 0, //
            1, 0, 0, //
            0, 0, 0,
        ];
        let out = scale2x(&image, 3, 3);
        let at = |x: usize, y: usize| out[y * 6 + x];
        // The centre pixel (0) has its top-left corner taken by the 1s above and left.
        assert_eq!(at(2, 2), 1);
        assert_eq!(at(3, 2), 0);
        assert_eq!(at(2, 3), 0);
        // No colour is invented.
        assert!(out.iter().all(|p| *p == 0 || *p == 1));
    }

    #[test]
    fn the_mode_parses_and_flips() {
        assert_eq!(Graphics::default(), Graphics::Classic);
        assert_eq!(Graphics::from_key("modern"), Some(Graphics::Modern));
        assert_eq!(Graphics::from_key("fancy"), None);
        assert_eq!(Graphics::Classic.flipped(), Graphics::Modern);
        assert_eq!(Graphics::Modern.key(), "modern");
    }
}
