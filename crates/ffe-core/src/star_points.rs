//! The galaxy map's star light points: the four per-type tables F1769 reads
//! (`DATA_007820`, `DATA_007821`, `DATA_007822` and `DATA_007828`, 14 dwords each).
//!
//! A star's type is the low nibble of its object byte at `+0xa6 + 2 i`. The type picks
//! the point's material word, its size (a packed angle, `>> 7`), the farthest depth that
//! still labels it and the material of its stalk line.

/// Star types the tables hold.
pub const TYPES: usize = 14;
/// Four tables of 14 dwords.
pub const TABLE_BYTES: usize = 4 * TYPES * 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarTables {
    /// `DATA_007820`: the point's material word.
    pub material: [u32; TYPES],
    /// `DATA_007821`: the point's size before `>> 7`.
    pub size: [u32; TYPES],
    /// `DATA_007822`: the label shows while the depth is at most this.
    pub label_depth: [u32; TYPES],
    /// `DATA_007828`: the stalk line's material word.
    pub stalk: [u32; TYPES],
}

impl StarTables {
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != TABLE_BYTES {
            return Err(format!(
                "The star point tables must be {TABLE_BYTES} bytes, not {}.",
                bytes.len()
            ));
        }
        let mut tables = [[0u32; TYPES]; 4];
        for (table, chunk) in tables.iter_mut().zip(bytes.chunks_exact(TYPES * 4)) {
            for (value, word) in table.iter_mut().zip(chunk.chunks_exact(4)) {
                *value = u32::from_le_bytes(word.try_into().unwrap());
            }
        }
        let [material, size, label_depth, stalk] = tables;
        if material.iter().chain(&stalk).any(|m| *m > 0xffff) {
            return Err("A star material is not a 16-bit word.".to_owned());
        }
        Ok(Self {
            material,
            size,
            label_depth,
            stalk,
        })
    }

    /// The type's table index, or `None` beyond the 14 types the tables hold.
    pub fn index(star: u8) -> Option<usize> {
        let index = usize::from(star & 15);
        (index < TYPES).then_some(index)
    }
}

/// What a map object tells F1769 about its stars: the type byte of each (`+0xa6 + 2 i`),
/// whether stalks are drawn (global word 1 bit 5) and the sector code the labels name
/// (global words 2 and 3, `+0xa0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarField {
    pub types: Vec<u8>,
    pub stalks: bool,
    pub code: u32,
}
