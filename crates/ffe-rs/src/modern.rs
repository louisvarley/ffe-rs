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
//!
//! Second technique: **replacement backdrops.** A higher-resolution image of a cabin backdrop
//! (for now read from `img-export/images/cockpit/cabin<n>.png`, the owner's upscaled export)
//! replaces that cabin wherever the finished frame still shows the original cabin's pixels.
//! The engine recognises the cabin by comparing the frame with the original art, so text,
//! menus, models and the console drawn over it stay on top and the game is not consulted.

use crate::display::{linear_row, mix, Rect};
use crate::indexed::{LOGICAL_HEIGHT, LOGICAL_WIDTH};
use std::io::Cursor;
use std::path::Path;

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

/// Logical rows a cabin backdrop covers; the console starts below them.
pub const CABIN_ROWS: usize = 158;
/// The largest replacement image side accepted.
const MAX_ART_SIDE: u32 = 8192;

/// A higher-resolution replacement for a cabin backdrop.
pub struct Backdrop {
    name: String,
    /// The original 320x158 art, as the game copies it into the frame.
    original: Vec<u32>,
    art: Vec<u32>,
    art_size: (usize, usize),
    /// `art` reduced to the window's cabin rectangle, rebuilt when that changes.
    fitted: Vec<u32>,
    fitted_size: (usize, usize),
}

impl Backdrop {
    /// `original` is the 320x158 cabin; `art` any size, stretched over the same rectangle.
    pub fn new(
        name: &str,
        original: Vec<u32>,
        art: Vec<u32>,
        art_size: (usize, usize),
    ) -> Option<Self> {
        (original.len() == LOGICAL_WIDTH * CABIN_ROWS
            && art_size.0 > 0
            && art.len() == art_size.0 * art_size.1)
            .then(|| Self {
                name: name.to_owned(),
                original,
                art,
                art_size,
                fitted: Vec::new(),
                fitted_size: (0, 0),
            })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// How many of every fourth pixel of the frame's cabin rows match the original art.
    fn matches(&self, logical: &[u32]) -> usize {
        (0..LOGICAL_WIDTH * CABIN_ROWS)
            .step_by(4)
            .filter(|&i| logical[i] == self.original[i])
            .count()
    }
}

/// Average-area resize of `source` (`width` by `height`) to `size`: each target pixel is the
/// mean of the source pixels under it (at least one), so large art shrinks without aliasing.
pub fn resize_area(source: &[u32], width: usize, height: usize, size: (usize, usize)) -> Vec<u32> {
    let span = |index: usize, target: usize, source: usize| {
        let first = index * source / target;
        (
            first,
            ((index + 1) * source / target).max(first + 1).min(source),
        )
    };
    let columns: Vec<_> = (0..size.0).map(|x| span(x, size.0, width)).collect();
    let mut out = Vec::with_capacity(size.0 * size.1);
    for y in 0..size.1 {
        let (top, bottom) = span(y, size.1, height);
        for &(left, right) in &columns {
            let mut sum = [0u64; 3];
            for row in top..bottom {
                for pixel in &source[row * width + left..row * width + right] {
                    sum[0] += u64::from(pixel >> 16 & 0xff);
                    sum[1] += u64::from(pixel >> 8 & 0xff);
                    sum[2] += u64::from(pixel & 0xff);
                }
            }
            let count = ((bottom - top) * (right - left)) as u64;
            let channel = |total: u64| ((total + count / 2) / count) as u32;
            out.push(channel(sum[0]) << 16 | channel(sum[1]) << 8 | channel(sum[2]));
        }
    }
    out
}

/// Decode an 8-bit (or 16-bit, stripped) RGB, RGBA, grey or palette PNG as packed RGB.
pub fn decode_art(png: &[u8]) -> Result<(Vec<u32>, (usize, usize)), String> {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("PNG: {e}"))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_ART_SIDE || height > MAX_ART_SIDE {
        return Err(format!(
            "{width}x{height} is outside 1..={MAX_ART_SIDE} per side"
        ));
    }
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut output)
        .map_err(|e| format!("PNG pixels: {e}"))?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("PNG palette did not expand".to_owned()),
    };
    let pixels = output[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|p| {
            let (r, g, b) = if channels < 3 {
                (p[0], p[0], p[0])
            } else {
                (p[0], p[1], p[2])
            };
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
        })
        .collect();
    Ok((pixels, (width as usize, height as usize)))
}

/// The modern engine's working buffers, kept between frames.
#[derive(Default)]
pub struct ModernRenderer {
    upscaled: Vec<u32>,
    backdrops: Vec<Backdrop>,
    /// The cabin mask of the current frame (1 where the original cabin shows).
    mask: Vec<u8>,
}

impl ModernRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load `cabin<n>.png` replacements from `dir` for the game's original `cabins` (320x158
    /// each). Images still at the original size are skipped. Returns one line per file used
    /// or rejected.
    pub fn load_cabins(&mut self, dir: &Path, cabins: &[Vec<u32>]) -> Vec<String> {
        let mut report = Vec::new();
        for (index, original) in cabins.iter().enumerate() {
            let path = dir.join(format!("cabin{index}.png"));
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let name = format!("cabin{index}");
            match decode_art(&bytes) {
                Ok((_, (320, 158))) => {}
                Ok((art, size)) => match Backdrop::new(&name, original.clone(), art, size) {
                    Some(backdrop) => {
                        report.push(format!(
                            "Modern graphics: {name} replaced by {}x{} art from {}",
                            size.0,
                            size.1,
                            path.display()
                        ));
                        self.backdrops.retain(|other| other.name != name);
                        self.backdrops.push(backdrop);
                    }
                    None => report.push(format!(
                        "Modern graphics: {name}: the game's cabin is not 320x158"
                    )),
                },
                Err(error) => report.push(format!("Modern graphics: {}: {error}", path.display())),
            }
        }
        report
    }

    pub fn backdrops(&self) -> &[Backdrop] {
        &self.backdrops
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
        self.draw_backdrop(logical, target, width, rect);
    }

    /// Where the frame shows a replaced cabin, blend its art over the upscaled frame. The
    /// cabin is recognised when at least a quarter of its pixels are still visible; the mask
    /// is sampled bilinearly in the frame's own coordinates, so the art meets the text and
    /// shapes drawn over it with the same smooth edge the upscaled frame has.
    fn draw_backdrop(&mut self, logical: &[u32], target: &mut [u32], width: usize, rect: Rect) {
        let samples = LOGICAL_WIDTH * CABIN_ROWS / 4;
        let Some(backdrop) = self
            .backdrops
            .iter_mut()
            .map(|backdrop| (backdrop.matches(logical), backdrop))
            .filter(|(count, _)| count * 4 >= samples)
            .max_by_key(|(count, _)| *count)
            .map(|(_, backdrop)| backdrop)
        else {
            return;
        };
        self.mask.clear();
        self.mask.extend(
            logical[..LOGICAL_WIDTH * CABIN_ROWS]
                .iter()
                .zip(&backdrop.original)
                .map(|(shown, art)| u8::from(shown == art)),
        );
        let rows = (rect.height * CABIN_ROWS).div_ceil(LOGICAL_HEIGHT);
        if backdrop.fitted_size != (rect.width, rows) {
            backdrop.fitted = resize_area(
                &backdrop.art,
                backdrop.art_size.0,
                backdrop.art_size.1,
                (rect.width, rows),
            );
            backdrop.fitted_size = (rect.width, rows);
        }
        let columns: Vec<_> = (0..rect.width)
            .map(|dx| linear_row(dx, rect.width, LOGICAL_WIDTH))
            .collect();
        let shown = &self.mask;
        let mask = |x: usize, y: usize| {
            if y < CABIN_ROWS {
                u32::from(shown[y * LOGICAL_WIDTH + x]) << 16
            } else {
                0
            }
        };
        let lerp = |a: u32, b: u32, weight: u32| {
            ((u64::from(a) * u64::from(65536 - weight) + u64::from(b) * u64::from(weight)) >> 16)
                as u32
        };
        for dy in 0..rows {
            let (y0, y1, fy) = linear_row(dy, rect.height, LOGICAL_HEIGHT);
            let row = (rect.y + dy) * width + rect.x;
            let Some(destination) = target.get_mut(row..row + rect.width) else {
                continue;
            };
            let art = &backdrop.fitted[dy * rect.width..][..rect.width];
            for ((pixel, &(x0, x1, fx)), &replacement) in
                destination.iter_mut().zip(&columns).zip(art)
            {
                let top = lerp(mask(x0, y0), mask(x1, y0), fx);
                let bottom = lerp(mask(x0, y1), mask(x1, y1), fx);
                let alpha = lerp(top, bottom, fy);
                if alpha > 0 {
                    *pixel = mix(*pixel, replacement, alpha);
                }
            }
        }
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
    fn area_resize_averages_and_enlarges() {
        let image = [0x000000, 0xff0000, 0x00ff00, 0x0000ff];
        assert_eq!(resize_area(&image, 2, 2, (1, 1)), vec![0x404040]);
        assert_eq!(
            resize_area(&image, 2, 2, (4, 4))[..4],
            [0, 0, 0xff0000, 0xff0000]
        );
    }

    #[test]
    fn a_replaced_cabin_shows_its_art_except_where_the_game_drew_over_it() {
        let original = vec![0x101010; LOGICAL_WIDTH * CABIN_ROWS];
        let art = vec![0xabcdef; 640 * 316];
        let mut renderer = ModernRenderer::new();
        renderer
            .backdrops
            .push(Backdrop::new("cabin0", original, art, (640, 316)).unwrap());
        let mut logical = vec![0x101010; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        logical[LOGICAL_WIDTH * CABIN_ROWS..].fill(0x202020);
        // A block of "text" over the cabin.
        for y in 40..60 {
            logical[y * LOGICAL_WIDTH + 100..y * LOGICAL_WIDTH + 140].fill(0xffffff);
        }
        let rect = Rect {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        };
        let mut target = vec![0; 1280 * 800];
        renderer.render(&logical, &mut target, 1280, rect);
        assert_eq!(target[10 * 1280 + 10], 0xabcdef);
        assert_eq!(target[200 * 1280 + 480], 0xffffff);
        assert_eq!(target[780 * 1280 + 10], 0x202020);

        // A frame that does not show the cabin is left alone.
        let other = vec![0x303030; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        renderer.render(&other, &mut target, 1280, rect);
        assert_eq!(target[10 * 1280 + 10], 0x303030);
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
