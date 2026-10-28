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
//!
//! Third technique: **a replacement font.** The screen records each glyph it draws (code,
//! position, colour, clip rows) and the pixels it overwrote ([`TextLayer`]). With a
//! higher-resolution atlas of the main font (for now `img-export/images/font/main.png`, the
//! owner's upscale of the `export-images` atlas), the engine takes the original glyphs out of
//! the frame before upscaling it and draws each glyph still on screen from the atlas instead,
//! in its colour, at the window's resolution.
//!
//! Better still, **a real font** ([`TrueFont`]): a TrueType or OpenType file drawn at the
//! window's resolution, in place of the atlas. Each glyph keeps the original's place: its
//! capitals are the original's seven rows tall on the original's baseline, and the font is
//! condensed evenly so its letters take the original's advances on average, each centred in
//! its own. h⁻¹ and s⁻¹ are the letter and a small raised "-1"; the game's arrows and box stay
//! the original's.

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
    /// Every fourth pixel whose colour differs from its right-hand neighbour: the
    /// art's detail, which a flat screen cannot match by chance.
    detail: Vec<u32>,
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
                detail: (0..LOGICAL_WIDTH * CABIN_ROWS)
                    .filter(|&i| {
                        i % LOGICAL_WIDTH + 1 < LOGICAL_WIDTH && original[i] != original[i + 1]
                    })
                    .step_by(4)
                    .map(|i| i as u32)
                    .collect(),
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

    /// Whether the frame shows this cabin: at least a quarter of its detail pixels and
    /// their (different) right-hand neighbours are in the frame unchanged, which no flat
    /// screen can do. Returns the matching count.
    fn matches(&self, logical: &[u32]) -> Option<usize> {
        let count = self
            .detail
            .iter()
            .filter(|&&i| {
                let i = i as usize;
                logical[i] == self.original[i] && logical[i + 1] == self.original[i + 1]
            })
            .count();
        (!self.detail.is_empty() && count * 4 >= self.detail.len()).then_some(count)
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

/// Decode an 8-bit (or 16-bit, stripped) RGB, RGBA, grey or palette PNG as packed RGB, with
/// the alpha (255 when the image has none) in the top byte.
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
            let a = match channels {
                2 => p[1],
                4 => p[3],
                _ => 255,
            };
            u32::from(a) << 24 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
        })
        .collect();
    Ok((pixels, (width as usize, height as usize)))
}

/// One glyph the screen drew: its font code, pen position, advance, RGB colour, the rows it
/// was clipped to, and its pixels' entries in [`TextLayer::pixels`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextGlyph {
    pub code: u8,
    pub x: i32,
    pub y: i32,
    pub advance: u8,
    pub colour: u32,
    pub top: i32,
    pub bottom: i32,
    pub pixels: std::ops::Range<usize>,
}

/// The text the screen drew into the logical frame since the presenter last took it:
/// every glyph, and each pixel it wrote as (frame index, previous value, glyph value).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextLayer {
    pub glyphs: Vec<TextGlyph>,
    pub pixels: Vec<(u32, u32, u32)>,
}

impl TextLayer {
    /// The most glyphs kept: a frame never draws this many, so frames drawn without being
    /// presented (captures) cannot grow it without bound.
    pub const LIMIT: usize = 4096;

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// The main font's atlas cell: 16 glyphs a row, 10x11 cells with the 8x9 glyph at (1, 1)
/// (`ffe-tools export-images`).
const ATLAS_COLUMNS: usize = 16;
const ATLAS_CELL: [usize; 2] = [10, 11];
/// A glyph is redrawn when at least this share of its pixels is still in the frame.
const GLYPH_VISIBLE: f64 = 0.6;

/// A higher-resolution atlas of the main font (codes 32.., ARGB).
pub struct HdFont {
    atlas: Vec<u32>,
    size: (usize, usize),
    /// Atlas pixels per logical pixel.
    scale: f64,
}

impl HdFont {
    /// The atlas as `export-images` lays it out, at any uniform scale over 160 pixels wide.
    pub fn new(atlas: Vec<u32>, size: (usize, usize)) -> Option<Self> {
        let cells = ATLAS_COLUMNS * ATLAS_CELL[0];
        (size.0 > cells && atlas.len() == size.0 * size.1).then(|| Self {
            scale: size.0 as f64 / cells as f64,
            atlas,
            size,
        })
    }

    /// Coverage (0..=65536) of `code` at logical offset (`fx`, `fy`) from its cell's corner,
    /// bilinear: the alpha times the brightness, so a white-on-black or a transparent atlas
    /// both work.
    fn coverage(&self, code: u8, fx: f64, fy: f64) -> u32 {
        let index = usize::from(code.saturating_sub(32));
        let cell_x = (index % ATLAS_COLUMNS * ATLAS_CELL[0]) as f64;
        let cell_y = (index / ATLAS_COLUMNS * ATLAS_CELL[1]) as f64;
        let ax = (cell_x + fx) * self.scale - 0.5;
        let ay = (cell_y + fy) * self.scale - 0.5;
        let sample = |x: i64, y: i64| -> f64 {
            if x < 0 || y < 0 || x as usize >= self.size.0 || y as usize >= self.size.1 {
                return 0.0;
            }
            let p = self.atlas[y as usize * self.size.0 + x as usize];
            let alpha = f64::from(p >> 24) / 255.0;
            let bright = f64::from((p >> 16 & 0xff).max(p >> 8 & 0xff).max(p & 0xff)) / 255.0;
            alpha * bright
        };
        let (x0, y0) = (ax.floor(), ay.floor());
        let (tx, ty) = (ax - x0, ay - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = sample(x0, y0) * (1.0 - tx) + sample(x0 + 1, y0) * tx;
        let bottom = sample(x0, y0 + 1) * (1.0 - tx) + sample(x0 + 1, y0 + 1) * tx;
        ((top * (1.0 - ty) + bottom * ty) * 65536.0).round() as u32
    }
}

/// The character a main-font code stands for, or `None` for the game's own symbols (h⁻¹ and
/// s⁻¹ for km/h and m/s, which the real font builds from a letter and "-1"; the four arrows and
/// the box, which only the original draws).
/// The font is ASCII with European letters in some slots (the export's atlas shows them).
pub fn character(code: u8) -> Option<char> {
    Some(match code {
        36 => '$',
        38 => 'ö',
        60 => 'ñ',
        62 => 'è',
        91 | 147 => 'ü',
        93 => 'ß',
        94 => '°',
        96 => 'ä',
        130 => 'ç',
        131 => 'é',
        132 => 'ù',
        133 => 'î',
        134 => 'â',
        135 => 'ô',
        136 => 'ê',
        137 => 'û',
        138 => 'à',
        139 => 'ò',
        140 => '¿',
        141..=146 => char::from(b'A' + (code - 141)),
        64 => '©',
        123..=129 => return None,
        32..=122 => char::from(code),
        _ => return None,
    })
}

/// The original's capitals stand on row 7 of the glyph and are seven rows tall; the real font's
/// are a little shorter, so its descenders and accents keep clear of the next line.
const BASELINE_ROW: f32 = 7.0;
const CAP_HEIGHT: f32 = 6.5;

/// A real font for the modern engine's text, fitted to the original's metrics.
pub struct TrueFont {
    font: fontdue::Font,
    /// Its capitals' height per pixel of font size.
    cap: f32,
    /// The original's mean advance over its letters and digits (logical pixels) per pixel of
    /// font size of this font's: how much it is condensed.
    condense: f32,
    /// Glyphs drawn so far, by character and size (in quarter pixels).
    glyphs: std::collections::HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl TrueFont {
    /// The font in `bytes`, fitted to the original font's `advances` (by code from 32).
    pub fn new(bytes: &[u8], advances: &[u8]) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(str::to_owned)?;
        let cap = font.metrics('H', 100.0).height as f32 / 100.0;
        if cap <= 0.0 {
            return Err("it has no capital H".to_owned());
        }
        let (mut original, mut own) = (0.0f32, 0.0f32);
        for code in (b'0'..=b'9').chain(b'A'..=b'Z').chain(b'a'..=b'z') {
            if let Some(&advance) = advances.get(usize::from(code - 32)) {
                original += f32::from(advance);
                own += font.metrics(char::from(code), 1.0).advance_width;
            }
        }
        if own <= 0.0 || original <= 0.0 {
            return Err("it has no letters".to_owned());
        }
        Ok(Self {
            font,
            cap,
            condense: original / own,
            glyphs: Default::default(),
        })
    }

    /// Whether the font draws `code`: its character, or for h⁻¹ and s⁻¹ the letter and "-1".
    fn draws(&self, code: u8) -> bool {
        let has = |c: char| c == ' ' || self.font.lookup_glyph_index(c) != 0;
        match unit(code) {
            Some(letter) => [letter, '-', '1'].into_iter().all(has),
            None => character(code).is_some_and(has),
        }
    }

    /// Draw `glyph` into `rect` of `target`: the original's baseline, capitals a little under
    /// the original's height, condensed to the original's widths and centred in the glyph's own
    /// advance, clipped to its rows. h⁻¹ and s⁻¹ are the letter and a small raised "-1".
    fn draw(&mut self, glyph: &TextGlyph, target: &mut [u32], width: usize, rect: Rect) {
        let sx = rect.width as f32 / LOGICAL_WIDTH as f32;
        let sy = rect.height as f32 / LOGICAL_HEIGHT as f32;
        let pen = rect.x as f32 + glyph.x as f32 * sx;
        let mut put = |c: char, scale: f32, left: Option<f32>, raise: f32| {
            self.put(c, scale, left, raise, glyph, target, width, rect)
        };
        if let Some(letter) = unit(glyph.code) {
            let after = put(letter, 1.0, Some(pen), 0.0);
            let after = put('-', 0.6, Some(after), CAP_HEIGHT * 0.55 * sy);
            put('1', 0.6, Some(after), CAP_HEIGHT * 0.55 * sy);
        } else if let Some(c) = character(glyph.code) {
            put(c, 1.0, None, 0.0);
        }
    }

    /// One character of `glyph`, `scale` times the text's size, `raise` window pixels above the
    /// baseline: from `left` (window pixels) or, without one, centred in the glyph's advance.
    /// Returns where the next character starts.
    #[allow(clippy::too_many_arguments)]
    fn put(
        &mut self,
        c: char,
        scale: f32,
        left: Option<f32>,
        raise: f32,
        glyph: &TextGlyph,
        target: &mut [u32],
        width: usize,
        rect: Rect,
    ) -> f32 {
        let sx = rect.width as f32 / LOGICAL_WIDTH as f32;
        let sy = rect.height as f32 / LOGICAL_HEIGHT as f32;
        let size = CAP_HEIGHT * sy / self.cap * scale;
        let key = (c, (size * 4.0).round() as u32);
        let (metrics, coverage) = self
            .glyphs
            .entry(key)
            .or_insert_with(|| self.font.rasterize(c, key.1 as f32 / 4.0));
        // Window pixels across per pixel of the drawn glyph (drawn at `size`).
        let across = self.condense * sx * scale / size;
        let pen = left.unwrap_or_else(|| {
            let room = f32::from(glyph.advance) * sx;
            rect.x as f32 + glyph.x as f32 * sx + (room - metrics.advance_width * across) * 0.5
        });
        let next = pen + metrics.advance_width * across;
        if metrics.width == 0 || metrics.height == 0 {
            return next;
        }
        let start = pen + metrics.xmin as f32 * across;
        let baseline = rect.y as f32 + (glyph.y as f32 + BASELINE_ROW) * sy - raise;
        let top = (baseline - (metrics.ymin as f32 + metrics.height as f32)).round() as i64;
        let clip_top = (rect.y as f32 + glyph.top.max(0) as f32 * sy).floor() as i64;
        let clip_bottom = (rect.y as f32 + glyph.bottom.min(200) as f32 * sy).ceil() as i64;
        let (x0, x1) = (
            start.floor() as i64,
            (start + metrics.width as f32 * across).ceil() as i64,
        );
        for row in 0..metrics.height {
            let y = top + row as i64;
            if y < clip_top.max(rect.y as i64)
                || y >= clip_bottom.min((rect.y + rect.height) as i64)
            {
                continue;
            }
            let line = &coverage[row * metrics.width..][..metrics.width];
            for x in x0.max(rect.x as i64)..x1.min((rect.x + rect.width) as i64) {
                // The drawn glyph's column under this pixel's centre, linearly between two.
                let u = (x as f32 + 0.5 - start) / across - 0.5;
                let k = u.floor();
                let t = u - k;
                let at = |k: f32| {
                    if k < 0.0 || k as usize >= metrics.width {
                        0.0
                    } else {
                        f32::from(line[k as usize])
                    }
                };
                let value = at(k) * (1.0 - t) + at(k + 1.0) * t;
                if value <= 0.0 {
                    continue;
                }
                if let Some(pixel) = target.get_mut(y as usize * width + x as usize) {
                    *pixel = mix(*pixel, glyph.colour, (value * 257.0) as u32);
                }
            }
        }
        next
    }
}

/// The letter of the original's h⁻¹ (123) and s⁻¹ (124) unit glyphs.
fn unit(code: u8) -> Option<char> {
    match code {
        123 => Some('h'),
        124 => Some('s'),
        _ => None,
    }
}

/// Where the modern engine looks for a real font: the first TrueType or OpenType file in the
/// owner's `img-export/fonts`, else Windows' Bahnschrift, else Segoe UI.
pub fn font_candidates(art: &Path) -> Vec<std::path::PathBuf> {
    let mut own: Vec<std::path::PathBuf> = std::fs::read_dir(art.join("fonts"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf"))
        })
        .collect();
    own.sort();
    let windows = std::env::var_os("WINDIR")
        .map_or_else(|| std::path::PathBuf::from(r"C:\Windows"), Into::into)
        .join("Fonts");
    own.extend(["bahnschrift.ttf", "segoeui.ttf"].map(|name| windows.join(name)));
    own
}

/// The modern engine's working buffers, kept between frames.
#[derive(Default)]
pub struct ModernRenderer {
    upscaled: Vec<u32>,
    backdrops: Vec<Backdrop>,
    /// The cabin mask of the current frame (1 where the original cabin shows).
    mask: Vec<u8>,
    font: Option<HdFont>,
    /// A real font, drawn in place of the atlas for the characters it has.
    true_font: Option<TrueFont>,
    /// This frame's text ([`Self::set_text`]) and the glyphs the frame still shows.
    text: TextLayer,
    shown: Vec<usize>,
    /// The frame with the shown glyphs taken out, when there are any.
    clean: Vec<u32>,
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

    /// Load the replacement font atlas from `path`, when it is larger than the original.
    pub fn load_font(&mut self, path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        let (atlas, size) = match decode_art(&bytes) {
            Ok(decoded) => decoded,
            Err(error) => return Some(format!("Modern graphics: {}: {error}", path.display())),
        };
        self.font = Some(HdFont::new(atlas, size)?);
        Some(format!(
            "Modern graphics: main font from {}x{} art at {}",
            size.0,
            size.1,
            path.display()
        ))
    }

    /// Load the first real font of [`font_candidates`] that reads, fitted to the original
    /// font's `advances` (by code from 32). Says which, or why none.
    pub fn load_true_font(&mut self, art: &Path, advances: &[u8]) -> String {
        let mut problems = Vec::new();
        for path in font_candidates(art) {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            match TrueFont::new(&bytes, advances) {
                Ok(font) => {
                    self.true_font = Some(font);
                    return format!("Modern graphics: text in {}", path.display());
                }
                Err(error) => problems.push(format!("{}: {error}", path.display())),
            }
        }
        format!("Modern graphics: no real font ({})", problems.join("; "))
    }

    /// Whether the modern engine redraws `code` itself.
    fn redraws(&self, code: u8) -> bool {
        self.true_font.as_ref().is_some_and(|font| font.draws(code)) || self.font.is_some()
    }

    /// The text the screen drew this frame ([`crate::screen::Screen::take_text`]).
    pub fn set_text(&mut self, text: TextLayer) {
        self.text = text;
    }

    /// The frame the upscaler and the GPU pass see: the logical frame with the glyphs the
    /// replacement font redraws taken out, or the logical frame itself.
    pub fn frame<'a>(&'a self, logical: &'a [u32]) -> &'a [u32] {
        if self.shown.is_empty() {
            logical
        } else {
            &self.clean
        }
    }

    /// Decide which recorded glyphs the frame still shows and take them out of a copy.
    fn prepare_text(&mut self, logical: &[u32]) {
        self.shown.clear();
        if (self.font.is_none() && self.true_font.is_none()) || self.text.is_empty() {
            return;
        }
        let text = &self.text;
        for (index, glyph) in text.glyphs.iter().enumerate() {
            if !self.redraws(glyph.code) {
                continue;
            }
            let pixels = &text.pixels[glyph.pixels.clone()];
            let still = pixels
                .iter()
                .filter(|(at, _, new)| logical.get(*at as usize) == Some(new))
                .count();
            if !pixels.is_empty() && still as f64 >= GLYPH_VISIBLE * pixels.len() as f64 {
                self.shown.push(index);
            }
        }
        if self.shown.is_empty() {
            return;
        }
        self.clean.clear();
        self.clean.extend_from_slice(logical);
        // Later glyphs over earlier ones: undo in reverse drawing order.
        for &index in self.shown.iter().rev() {
            let glyph = &text.glyphs[index];
            for &(at, old, new) in text.pixels[glyph.pixels.clone()].iter().rev() {
                if let Some(pixel) = self.clean.get_mut(at as usize) {
                    if *pixel == new {
                        *pixel = old;
                    }
                }
            }
        }
    }

    /// Draw the shown glyphs over `target` (the real font where it has them, else the
    /// atlas), then forget the text.
    pub fn finish(&mut self, target: &mut [u32], width: usize, rect: Rect) {
        let text = std::mem::take(&mut self.text);
        let shown = std::mem::take(&mut self.shown);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let sx = rect.width as f64 / LOGICAL_WIDTH as f64;
        let sy = rect.height as f64 / LOGICAL_HEIGHT as f64;
        for index in shown {
            let glyph = &text.glyphs[index];
            if let Some(font) = self
                .true_font
                .as_mut()
                .filter(|font| font.draws(glyph.code))
            {
                font.draw(glyph, target, width, rect);
                continue;
            }
            let Some(font) = self.font.as_ref() else {
                continue;
            };
            // The atlas cell's corner is one logical pixel up and left of the pen.
            let (cx, cy) = (f64::from(glyph.x - 1), f64::from(glyph.y - 1));
            let top = f64::from(glyph.top.max(glyph.y - 1).max(0));
            let bottom = f64::from(
                glyph
                    .bottom
                    .min(glyph.y - 1 + ATLAS_CELL[1] as i32)
                    .min(200),
            );
            let left = cx.max(0.0);
            let right = (cx + ATLAS_CELL[0] as f64).min(LOGICAL_WIDTH as f64);
            if bottom <= top || right <= left {
                continue;
            }
            let x0 = (left * sx).floor() as usize;
            let x1 = ((right * sx).ceil() as usize).min(rect.width);
            let y0 = (top * sy).floor() as usize;
            let y1 = ((bottom * sy).ceil() as usize).min(rect.height);
            for dy in y0..y1 {
                let fy = (dy as f64 + 0.5) / sy;
                if fy < top || fy >= bottom {
                    continue;
                }
                let row = (rect.y + dy) * width + rect.x;
                for dx in x0..x1 {
                    let fx = (dx as f64 + 0.5) / sx;
                    let coverage = font.coverage(glyph.code, fx - cx, fy - cy).min(65536);
                    if coverage == 0 {
                        continue;
                    }
                    if let Some(pixel) = target.get_mut(row + dx) {
                        *pixel = mix(*pixel, glyph.colour, coverage);
                    }
                }
            }
        }
    }

    /// The edge-directed 4x image of a 320x200 logical frame (1280 by 800).
    pub fn upscale(&mut self, logical: &[u32]) -> &[u32] {
        let twice = scale2x(logical, LOGICAL_WIDTH, LOGICAL_HEIGHT);
        self.upscaled = scale2x(&twice, LOGICAL_WIDTH * 2, LOGICAL_HEIGHT * 2);
        &self.upscaled
    }

    /// Draw the logical frame into `rect` of `target` (a `width`-pixel-wide window buffer).
    /// With a replacement font the frame's glyphs are left out; [`Self::finish`] draws them
    /// after any later pass (the presenter's GPU pass sees [`Self::frame`]).
    pub fn render(&mut self, logical: &[u32], target: &mut [u32], width: usize, rect: Rect) {
        if logical.len() < LOGICAL_WIDTH * LOGICAL_HEIGHT || rect.width == 0 || rect.height == 0 {
            return;
        }
        self.prepare_text(logical);
        let clean = std::mem::take(&mut self.clean);
        let logical = if self.shown.is_empty() {
            logical
        } else {
            &clean
        };
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
        self.clean = clean;
    }

    /// Where the frame shows a replaced cabin, blend its art over the upscaled frame. The
    /// cabin is recognised when at least a quarter of its detail is still visible; the mask
    /// is sampled bilinearly in the frame's own coordinates, so the art meets the text and
    /// shapes drawn over it with the same smooth edge the upscaled frame has.
    fn draw_backdrop(&mut self, logical: &[u32], target: &mut [u32], width: usize, rect: Rect) {
        let Some(backdrop) = self
            .backdrops
            .iter_mut()
            .filter_map(|backdrop| Some((backdrop.matches(logical)?, backdrop)))
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
        // Art with detail: alternate columns of two dark greys.
        let original: Vec<u32> = (0..LOGICAL_WIDTH * CABIN_ROWS)
            .map(|i| if i % 2 == 0 { 0x101010 } else { 0x111111 })
            .collect();
        let art = vec![0xabcdef; 640 * 316];
        let mut renderer = ModernRenderer::new();
        renderer
            .backdrops
            .push(Backdrop::new("cabin0", original.clone(), art, (640, 316)).unwrap());
        let mut logical = vec![0x202020; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        logical[..LOGICAL_WIDTH * CABIN_ROWS].copy_from_slice(&original);
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
        // Inside the cabin (away from the bilinear edge of the alternating art).
        assert_eq!(target[10 * 1280 + 10] & 0xffffff, 0xabcdef);
        assert_eq!(target[200 * 1280 + 480], 0xffffff);
        assert_eq!(target[780 * 1280 + 10], 0x202020);

        // A frame that does not show the cabin is left alone, even a flat one in
        // one of the cabin's own colours.
        for flat in [0x303030, 0x101010] {
            let other = vec![flat; LOGICAL_WIDTH * LOGICAL_HEIGHT];
            renderer.render(&other, &mut target, 1280, rect);
            assert_eq!(target[10 * 1280 + 10], flat);
        }
    }

    #[test]
    fn the_main_font_codes_are_ascii_with_european_letters_and_game_symbols() {
        assert_eq!(character(b'A'), Some('A'));
        assert_eq!(character(b'z'), Some('z'));
        assert_eq!(character(b'0'), Some('0'));
        assert_eq!(character(38), Some('ö'));
        assert_eq!(character(93), Some('ß'));
        assert_eq!(character(131), Some('é'));
        assert_eq!(character(141), Some('A'));
        // h⁻¹, s⁻¹, the arrows and the box are the game's own.
        for code in 123..=129 {
            assert_eq!(character(code), None, "code {code}");
        }
        assert_eq!(
            (unit(123), unit(124), unit(125)),
            (Some('h'), Some('s'), None)
        );
    }

    /// With a real font on this machine (Windows' Bahnschrift or Segoe UI), a glyph is drawn in
    /// its colour inside the original's place: its advance across, its rows down, clipped to
    /// its clip rows. Skipped where neither font is installed.
    #[test]
    fn a_real_font_draws_each_glyph_in_the_originals_place() {
        let Some(bytes) = font_candidates(Path::new("no-such-export"))
            .iter()
            .find_map(|path| std::fs::read(path).ok())
        else {
            eprintln!("no real font installed; skipped");
            return;
        };
        let advances = vec![5u8; 116];
        let mut font = TrueFont::new(&bytes, &advances).unwrap();
        assert!(font.draws(b'H') && font.draws(123) && !font.draws(125));
        let rect = Rect {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        };
        let mut glyph = TextGlyph {
            code: b'H',
            x: 100,
            y: 50,
            advance: 5,
            colour: 0xff0000,
            top: 0,
            bottom: 200,
            pixels: 0..0,
        };
        let mut target = vec![0u32; 1280 * 800];
        font.draw(&glyph, &mut target, 1280, rect);
        let painted: Vec<(usize, usize)> = (0..1280 * 800)
            .filter(|&i| target[i] != 0)
            .map(|i| (i % 1280, i / 1280))
            .collect();
        assert!(!painted.is_empty());
        // 4 window pixels a logical pixel: the advance is x 400..420, the rows y 200..228
        // (seven rows of capitals over the baseline at row 7).
        assert!(painted
            .iter()
            .all(|&(x, y)| (398..422).contains(&x) && (200..229).contains(&y)));
        assert!(painted
            .iter()
            .any(|&(x, y)| target[y * 1280 + x] == 0xff0000));
        // Clipped to rows 50..52: only the glyph's top two rows' worth shows.
        glyph.bottom = 52;
        target.fill(0);
        font.draw(&glyph, &mut target, 1280, rect);
        assert!((0..1280 * 800).all(|i| target[i] == 0 || i / 1280 < 208));
    }

    #[test]
    fn a_replacement_font_redraws_glyphs_and_removes_the_originals() {
        // A 2x atlas whose every cell is solid white.
        let atlas = vec![0xffff_ffff; 320 * 176];
        let mut renderer = ModernRenderer::new();
        renderer.font = HdFont::new(atlas, (320, 176));
        assert!(renderer.font.is_some());
        let mut logical = vec![0x000000; LOGICAL_WIDTH * LOGICAL_HEIGHT];
        // One glyph pixel at (100, 50), drawn in red over black.
        logical[50 * LOGICAL_WIDTH + 100] = 0xff0000;
        renderer.set_text(TextLayer {
            glyphs: vec![TextGlyph {
                code: b'A',
                x: 100,
                y: 50,
                advance: 5,
                colour: 0xff0000,
                top: 0,
                bottom: 200,
                pixels: 0..1,
            }],
            pixels: vec![((50 * LOGICAL_WIDTH + 100) as u32, 0x000000, 0xff0000)],
        });
        let rect = Rect {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        };
        let mut target = vec![0; 1280 * 800];
        renderer.render(&logical, &mut target, 1280, rect);
        // The upscaler saw the frame without the glyph.
        assert_eq!(renderer.frame(&logical)[50 * LOGICAL_WIDTH + 100], 0);
        renderer.finish(&mut target, 1280, rect);
        // The atlas cell covers the glyph's 10x11 cell from (99, 49) in red.
        assert_eq!(target[(52 * 4) * 1280 + 103 * 4], 0xff0000);
        // Outside the cell stays black.
        assert_eq!(target[(70 * 4) * 1280 + 103 * 4], 0);
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
