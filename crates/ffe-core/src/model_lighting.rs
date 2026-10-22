//! The original model lighting inputs and colour packing.
//!
//! F960 sets the default light (`DATA_007231..7233`, a direction normalised to
//! length 0x8000) and copies the default shade table `DATA_007234` into
//! `DATA_008831`; F1535 replaces the shades with one of the star tables in
//! `DATA_007759` (24 bytes each, selected by the star's type). Every top-level
//! object (F1683/F1684) and child frame (F1740) copies those 8 shades; normal
//! evaluation picks one per face by the light dot product.

/// Shade tables in `DATA_007759` (F1535 indexes eight; six are defined).
pub const STAR_SHADE_TABLES: usize = 6;
/// Bytes of the default light (three dwords) plus its shade table.
pub const DEFAULT_LIGHTING_BYTES: usize = 12 + 24;

/// A light direction and the eight shade colours (nibble RGB, 0..7 in the
/// original tables) that normal evaluation chooses from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    pub light: [i32; 3],
    pub shades: [[u8; 3]; 8],
}

fn shades(bytes: &[u8]) -> Result<[[u8; 3]; 8], String> {
    if bytes.len() != 24 || bytes.iter().any(|b| *b > 15) {
        return Err("A shade table holds 8 nibble RGB colours (24 bytes).".to_owned());
    }
    Ok(std::array::from_fn(|i| {
        [bytes[3 * i], bytes[3 * i + 1], bytes[3 * i + 2]]
    }))
}

/// `DATA_007231..7234`: the default light and shade table set by F960.
pub fn default_lighting(bytes: &[u8]) -> Result<Lighting, String> {
    if bytes.len() != DEFAULT_LIGHTING_BYTES {
        return Err("Default lighting is 12 light bytes plus a 24-byte shade table.".to_owned());
    }
    let light: [i32; 3] = std::array::from_fn(|i| {
        i32::from_le_bytes([
            bytes[4 * i],
            bytes[4 * i + 1],
            bytes[4 * i + 2],
            bytes[4 * i + 3],
        ])
    });
    let length = light.iter().map(|v| i64::from(*v).pow(2)).sum::<i64>();
    // F1518 normalises to 0x8000; allow its rounding.
    if !(0x7f00i64.pow(2)..=0x8100i64.pow(2)).contains(&length) {
        return Err("The default light is not a normalised direction.".to_owned());
    }
    Ok(Lighting {
        light,
        shades: shades(&bytes[12..])?,
    })
}

/// `DATA_007759`: the star shade tables F1535 selects from.
pub fn star_shades(bytes: &[u8]) -> Result<Vec<[[u8; 3]; 8]>, String> {
    if bytes.len() != STAR_SHADE_TABLES * 24 {
        return Err("Expected six 24-byte star shade tables.".to_owned());
    }
    bytes.chunks_exact(24).map(shades).collect()
}

/// F1326: pack a nibble colour as `r << 8 + g << 4 + b` in 16 bits. There is no
/// clamp: a component above 15 carries into the next one, as in the original
/// (whose material and shade values keep the sums in range).
pub fn pack(colour: [u8; 3]) -> u16 {
    (u16::from(colour[0]) << 8)
        .wrapping_add(u16::from(colour[1]) << 4)
        .wrapping_add(u16::from(colour[2]))
}
