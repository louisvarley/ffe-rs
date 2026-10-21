use std::io::Write;

pub mod embedded;

const MAX_DIMENSION: u32 = 8192;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    /// Row-major, top to bottom, without BMP row padding.
    pub indices: Vec<u8>,
    /// Source RGB bytes, not an inferred six-bit game palette.
    pub palette_rgb: Vec<u8>,
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or("truncated BMP header")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("truncated BMP header")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub fn decode_indexed_bmp(bytes: &[u8]) -> Result<IndexedImage, String> {
    if bytes.get(..2) != Some(b"BM") {
        return Err("expected BMP signature BM".to_owned());
    }
    // Original CABIN1..4 files leave bfSize zero; validate their rows below.
    let declared_size = u32_at(bytes, 2)? as usize;
    if declared_size != 0 && declared_size != bytes.len() {
        return Err("BMP declared file size does not match input length".to_owned());
    }
    if u32_at(bytes, 14)? != 40 {
        return Err("only 40-byte BITMAPINFOHEADER is supported".to_owned());
    }
    let width = u32_at(bytes, 18)? as i32;
    let signed_height = u32_at(bytes, 22)? as i32;
    if width <= 0 || signed_height == 0 || signed_height == i32::MIN {
        return Err("BMP width/height must be positive in magnitude".to_owned());
    }
    let width = width as u32;
    let height = signed_height.unsigned_abs();
    if width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("BMP dimensions exceed decoding limits".to_owned());
    }
    if u16_at(bytes, 26)? != 1 || u16_at(bytes, 28)? != 8 || u32_at(bytes, 30)? != 0 {
        return Err("expected one-plane, uncompressed 8-bit indexed BMP".to_owned());
    }
    let declared_colors = u32_at(bytes, 46)?;
    let colors = if declared_colors == 0 {
        256
    } else {
        declared_colors
    };
    if colors > 256 {
        return Err("indexed BMP cannot have more than 256 colors".to_owned());
    }
    let offset = u32_at(bytes, 10)? as usize;
    let palette_end = 54 + colors as usize * 4;
    if offset < palette_end {
        return Err("BMP pixel offset overlaps header or palette".to_owned());
    }
    let palette_bytes = bytes.get(54..palette_end).ok_or("truncated BMP palette")?;
    let stride = (width as usize + 3) & !3;
    let pixel_bytes = stride * height as usize;
    let end = offset
        .checked_add(pixel_bytes)
        .ok_or("BMP pixel offset overflow")?;
    let source = bytes.get(offset..end).ok_or("truncated BMP pixel rows")?;
    let declared_image = u32_at(bytes, 34)? as usize;
    if declared_image != 0 && declared_image != pixel_bytes {
        return Err("BMP declared image size does not match padded rows".to_owned());
    }
    let mut palette_rgb = Vec::with_capacity(colors as usize * 3);
    for color in palette_bytes.chunks_exact(4) {
        palette_rgb.extend_from_slice(&[color[2], color[1], color[0]]);
    }
    let mut indices = Vec::with_capacity(width as usize * height as usize);
    for row in 0..height as usize {
        let source_row = if signed_height < 0 {
            row
        } else {
            height as usize - 1 - row
        };
        let pixels = &source[source_row * stride..source_row * stride + width as usize];
        if pixels.iter().any(|index| u32::from(*index) >= colors) {
            return Err("BMP pixel references an absent palette entry".to_owned());
        }
        indices.extend_from_slice(pixels);
    }
    Ok(IndexedImage {
        width,
        height,
        indices,
        palette_rgb,
    })
}

impl IndexedImage {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
            || u64::from(self.width) * u64::from(self.height) > MAX_PIXELS
            || self.indices.len() as u64 != u64::from(self.width) * u64::from(self.height)
        {
            return Err("invalid image dimensions or index count".to_owned());
        }
        let colors = self.palette_rgb.len() / 3;
        if colors == 0
            || colors > 256
            || self.palette_rgb.len() % 3 != 0
            || self
                .indices
                .iter()
                .any(|index| usize::from(*index) >= colors)
        {
            return Err("invalid image palette or index".to_owned());
        }
        Ok(())
    }

    pub fn rgba(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut rgba = Vec::with_capacity(self.indices.len() * 4);
        for index in &self.indices {
            let start = usize::from(*index) * 3;
            rgba.extend_from_slice(&self.palette_rgb[start..start + 3]);
            rgba.push(255);
        }
        Ok(rgba)
    }

    pub fn indexed_png(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode_png(
            self.width,
            self.height,
            png::ColorType::Indexed,
            &self.indices,
            Some(&self.palette_rgb),
        )
    }

    pub fn rgba_png(&self) -> Result<Vec<u8>, String> {
        encode_png(
            self.width,
            self.height,
            png::ColorType::Rgba,
            &self.rgba()?,
            None,
        )
    }
}

fn encode_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    pixels: &[u8],
    palette: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        encoder.set_filter(png::FilterType::NoFilter);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("PNG header: {error}"))?;
        writer
            .write_image_data(pixels)
            .map_err(|error| format!("PNG pixels: {error}"))?;
        writer
            .finish()
            .map_err(|error| format!("PNG finish: {error}"))?;
    }
    Ok(bytes)
}

pub fn pcm16_mono_22050_wav(pcm: &[u8]) -> Result<Vec<u8>, String> {
    if pcm.len() % 2 != 0 {
        return Err("16-bit PCM must have an even byte count".to_owned());
    }
    let length = u32::try_from(pcm.len()).map_err(|_| "PCM exceeds WAV size limit")?;
    let riff_size = length
        .checked_add(36)
        .ok_or("PCM exceeds RIFF size limit")?;
    let mut bytes = Vec::with_capacity(pcm.len() + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&22050u32.to_le_bytes());
    bytes.extend_from_slice(&44100u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.write_all(pcm).map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// Conservative aspect-preserving integer scale for a known displayed rectangle.
pub fn recommended_scale(source: [u32; 2], displayed: [u32; 2]) -> Result<u32, String> {
    if source.contains(&0) || displayed.contains(&0) {
        return Err("source and display dimensions must be nonzero".to_owned());
    }
    let scale = displayed[0]
        .div_ceil(source[0])
        .max(displayed[1].div_ceil(source[1]))
        .max(1);
    source[0]
        .checked_mul(scale)
        .ok_or("recommended width overflow")?;
    source[1]
        .checked_mul(scale)
        .ok_or("recommended height overflow")?;
    Ok(scale)
}
