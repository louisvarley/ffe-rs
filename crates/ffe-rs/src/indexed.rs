pub const LOGICAL_WIDTH: usize = 320;
pub const LOGICAL_HEIGHT: usize = 200;
pub const FONT_BYTES: usize = 1160;

pub struct BitmapFont {
    bytes: [u8; FONT_BYTES],
}

pub struct Glyph<'a> {
    pub rows: &'a [u8],
    pub advance: usize,
}

impl Glyph<'_> {
    pub fn pixels(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.rows.iter().enumerate().flat_map(|(row, mask)| {
            (0..8).filter_map(move |col| (mask & (0x80 >> col) != 0).then_some((col, row)))
        })
    }
}

impl BitmapFont {
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        let bytes: [u8; FONT_BYTES] = bytes
            .try_into()
            .map_err(|_| "Expected 116 ten-byte font records.")?;
        if bytes
            .chunks_exact(10)
            .any(|glyph| !(1..=9).contains(&glyph[9]))
        {
            return Err("Font advance must be in 1..9.".to_owned());
        }
        Ok(Self { bytes })
    }

    pub fn advance(&self, code: u8) -> Option<usize> {
        let index = code.checked_sub(32)? as usize;
        self.bytes
            .get(index * 10 + 9)
            .map(|width| usize::from(*width))
    }

    pub fn glyph(&self, code: u8) -> Option<Glyph<'_>> {
        let advance = self.advance(code)?;
        let record = usize::from(code - 32) * 10;
        Some(Glyph {
            rows: &self.bytes[record..record + 9],
            advance,
        })
    }

    pub fn draw(
        &self,
        pixels: &mut [u8; 64000],
        code: u8,
        colour: u8,
        x: i32,
        y: i32,
        clip_y: std::ops::Range<i32>,
    ) -> Result<usize, String> {
        let glyph = self
            .glyph(code)
            .ok_or_else(|| format!("Unsupported FFE font byte {code:#x}"))?;
        if clip_y.start < 0 || clip_y.end > 200 || clip_y.start > clip_y.end {
            return Err("Invalid text clip range.".to_owned());
        }
        if y >= 200 || y >= clip_y.end {
            return Ok(0);
        }
        for (col, row) in glyph.pixels() {
            let Some(py) = y.checked_add(row as i32) else {
                continue;
            };
            if !clip_y.contains(&py) {
                continue;
            }
            let Some(px) = x.checked_add(col as i32) else {
                continue;
            };
            if (0..320).contains(&px) {
                pixels[py as usize * 320 + px as usize] = colour;
            }
        }
        Ok(glyph.advance)
    }

    pub fn width(&self, text: &str) -> Result<usize, String> {
        text.bytes().try_fold(0, |total, code| {
            self.advance(code)
                .map(|advance| total + advance)
                .ok_or_else(|| format!("Unsupported FFE font byte {code:#x}"))
        })
    }
}

pub struct IndexedImage {
    width: usize,
    height: usize,
    offset: [i16; 2],
    masked_zero: bool,
    indices: Vec<u8>,
}

impl IndexedImage {
    pub fn new(
        width: u32,
        height: u32,
        offset: [i16; 2],
        masked_zero: bool,
        indices: &[u8],
    ) -> Result<Self, String> {
        if width == 0
            || width > 320
            || height == 0
            || height > 255
            || u64::from(width) * u64::from(height) != indices.len() as u64
            || indices.iter().any(|index| *index >= 128)
        {
            return Err("Expected a bounded bitmap using the fixed low palette.".to_owned());
        }
        Ok(Self {
            width: width as usize,
            height: height as usize,
            offset,
            masked_zero,
            indices: indices.to_vec(),
        })
    }

    /// The raw low-palette indices, row-major.
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }

    /// The image's dimensions.
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn blit(&self, pixels: &mut [u8; 64000], x: i32, y: i32) {
        for row in 0..self.height {
            let py = i64::from(y) + i64::from(self.offset[1]) + row as i64;
            if !(0..200).contains(&py) {
                continue;
            }
            for col in 0..self.width {
                let px = i64::from(x) + i64::from(self.offset[0]) + col as i64;
                if !(0..320).contains(&px) {
                    continue;
                }
                let index = self.indices[row * self.width + col];
                if index != 0 || !self.masked_zero {
                    pixels[py as usize * 320 + px as usize] = index;
                }
            }
        }
    }

    /// Blit straight into a palette-expanded logical frame (the navigation list's
    /// marker is drawn after the flight view, which is already in RGB).
    pub fn blit_colours(&self, pixels: &mut [u32], palette: &[u32; 128], x: i32, y: i32) {
        for row in 0..self.height {
            let py = i64::from(y) + i64::from(self.offset[1]) + row as i64;
            if !(0..200).contains(&py) {
                continue;
            }
            for col in 0..self.width {
                let px = i64::from(x) + i64::from(self.offset[0]) + col as i64;
                if !(0..320).contains(&px) {
                    continue;
                }
                let index = self.indices[row * self.width + col];
                if index != 0 || !self.masked_zero {
                    pixels[py as usize * 320 + px as usize] = palette[usize::from(index)];
                }
            }
        }
    }

    /// F1622's clipped blit: the same image, but only rows at or below `ymin`
    /// (F270 grows each gauge bar upward from the bottom by raising `ymin`).
    pub fn blit_clipped(&self, pixels: &mut [u8; 64000], x: i32, y: i32, ymin: i32) {
        for row in 0..self.height {
            let py = i64::from(y) + i64::from(self.offset[1]) + row as i64;
            if !(0..200).contains(&py) || py < i64::from(ymin) {
                continue;
            }
            for col in 0..self.width {
                let px = i64::from(x) + i64::from(self.offset[0]) + col as i64;
                if !(0..320).contains(&px) {
                    continue;
                }
                let index = self.indices[row * self.width + col];
                if index != 0 || !self.masked_zero {
                    pixels[py as usize * 320 + px as usize] = index;
                }
            }
        }
    }
}

pub fn palette_rgb6(bytes: &[u8]) -> Result<[u32; 128], String> {
    if bytes.len() != 384 || bytes.iter().any(|component| *component > 63) {
        return Err("Expected 128 six-bit RGB palette entries.".to_owned());
    }
    Ok(std::array::from_fn(|i| {
        (u32::from(bytes[i * 3]) << 18)
            | (u32::from(bytes[i * 3 + 1]) << 10)
            | (u32::from(bytes[i * 3 + 2]) << 2)
    }))
}
