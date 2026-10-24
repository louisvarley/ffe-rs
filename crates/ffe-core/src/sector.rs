//! The galaxy's stars: how many there are in a sector (F853), where they are (F841's
//! generator with F861, F856), the distance between two positions (F833), and what the
//! hyperdrives of each class can do (the drive table F903 reads), ported from `ffe.asm`.
//!
//! A system is identified by a code (`galaxy`): bits 0-12 the sector x, 13-25 the sector
//! y, 26-31 the star number in the sector. Most sectors are generated from their own
//! coordinates; 49 near Sol are hand-made. Positions are 16.16 fixed point: the sector
//! number in the high half, the star's offset within the sector in the low half.
//!
//! The data (cache v21, `GalaxyTables`) is raw from the executable: the hand-made star
//! vectors and their type words (`DATA_006793..6795`), the special sector keys and their
//! index table, the density grid and its hash table, the two star type tables
//! (`DATA_006799..6811`), then the 15 drive records (`DATA_007187..`).

/// Bytes of the hand-made vectors (117 of 6) and type words (117 of 2).
pub const VECTORS_BYTES: usize = 936;
/// Bytes of `DATA_006799..6811`.
pub const SECTOR_BYTES: usize = 17_740;
/// Bytes of the drive records: 15 of 16.
pub const DRIVE_BYTES: usize = 240;
/// Bytes of the whole imported block.
pub const TABLE_BYTES: usize = VECTORS_BYTES + SECTOR_BYTES + DRIVE_BYTES;

const WORDS: usize = 702;
const SPECIAL: usize = VECTORS_BYTES; // 49 dwords: x | y << 16
const INDEX: usize = SPECIAL + 196; // 52 dwords of start indices
const HASH: usize = SPECIAL + 588; // DATA_006805: 128 dwords (576 bytes imported)
const GRID_BASE: usize = SPECIAL + 1164; // DATA_006806..6809: 128 by 129 bytes
const TYPE_LOW: usize = SPECIAL + 17_676; // DATA_006810
const TYPE_HIGH: usize = TYPE_LOW + 32; // DATA_006811
const DRIVES: usize = VECTORS_BYTES + SECTOR_BYTES;

/// Sectors with hand-made stars.
pub const SPECIAL_SECTORS: usize = 49;
/// The most stars a generated sector holds (`F841` clamps at `0x3e`).
pub const MAX_STARS: usize = 0x3e;

/// A star's position record: byte 0 is always 9; bytes 2 to 4 give its place in the sector.
pub type Vector = [u8; 6];

/// One star of a sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Star {
    pub vector: Vector,
    /// The type word the original keeps beside the vector: the low byte from
    /// `DATA_006811`, the high byte from `DATA_006810` (hand-made stars: the table's word).
    pub kind: u16,
}

/// A hyperdrive class (`DATA_007187`, 16 bytes each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drive {
    /// The numerator of the maximum range (divided by the ship's mass).
    pub power: i32,
    /// The fuel modifier (a Q15 fraction of the ship's mass the drive saves).
    pub fuel_modifier: i32,
    /// The drive's mass.
    pub mass: i32,
    /// The message code of the drive's name.
    pub name: u32,
}

/// A borrowed view of the imported block.
#[derive(Debug, Clone, Copy)]
pub struct Tables<'a> {
    bytes: &'a [u8],
}

fn dword(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

impl<'a> Tables<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() != TABLE_BYTES {
            return Err("galaxy tables have the wrong size".to_owned());
        }
        let tables = Self { bytes };
        // The special sectors' index table is non-decreasing and ends at the 117 vectors.
        let starts: Vec<u32> = (0..=SPECIAL_SECTORS).map(|k| tables.start(k)).collect();
        if starts[0] != 0
            || starts.windows(2).any(|w| w[0] > w[1])
            || starts[SPECIAL_SECTORS] != 117
        {
            return Err("galaxy tables: the special sector index is not consistent".to_owned());
        }
        // The hand-made keys are distinct sectors inside the galaxy.
        let keys: Vec<u32> = (0..SPECIAL_SECTORS).map(|k| tables.key(k)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != SPECIAL_SECTORS
            || keys
                .iter()
                .any(|k| (k & 0xffff) >= 0x2000 || (k >> 16) >= 0x2000)
        {
            return Err("galaxy tables: the special sector keys are not valid".to_owned());
        }
        if (0..=14).any(|class| tables.drive(class).is_none()) {
            return Err("galaxy tables: missing drive records".to_owned());
        }
        Ok(tables)
    }

    fn key(&self, k: usize) -> u32 {
        dword(self.bytes, SPECIAL + 4 * k)
    }

    fn start(&self, k: usize) -> u32 {
        dword(self.bytes, INDEX + 4 * k)
    }

    /// The index of the hand-made sector at (`x`, `y`) among the 49.
    pub fn special_index(&self, x: u16, y: u16) -> Option<usize> {
        self.special(x, y)
    }

    /// The index of the hand-made sector at (`x`, `y`).
    fn special(&self, x: u16, y: u16) -> Option<usize> {
        let key = u32::from(x) | u32::from(y) << 16;
        (0..SPECIAL_SECTORS).find(|k| self.key(*k) == key)
    }

    /// Whether sector (`x`, `y`) is a generated one (not one of the 49 hand-made sectors).
    pub fn special_free(&self, x: u16, y: u16) -> bool {
        self.special(x, y).is_none()
    }

    /// The grid cell `index` of the density map (128 by 129 bytes).
    fn grid(&self, index: usize) -> i32 {
        i32::from(self.bytes[GRID_BASE + index])
    }

    /// The hash table entry `index` (`DATA_006805`).
    fn hash(&self, index: usize) -> i32 {
        dword(self.bytes, HASH + 4 * index) as i32
    }

    /// F853: the number of stars in a sector, times 1024, at detail `quality` (0 is the
    /// game's own count; 16 and over the unhashed density).
    pub fn star_count(&self, x: u32, y: u32, quality: i32) -> i32 {
        if (x & 0xffff) >= 0x2000 || (y & 0xffff) >= 0x2000 {
            return 0;
        }
        if quality == 0 {
            if let Some(k) = self.special(x as u16, y as u16) {
                return ((self.start(k + 1) - self.start(k)) << 10) as i32;
            }
        }
        let ex = ((x & 0xffff) << 3) as i32;
        let ey = ((y & 0xffff) << 3) as i32;
        let index = (((ey & 0xfe00) + (ex >> 7)) >> 2) as usize;
        let c = self.grid(index);
        let below = self.grid(index + 128);
        let diagonal = self.grid(index + 129);
        let right = self.grid(index + 1);
        let fx = (ex & 0x1ff) << 6;
        let fy = (ey & 0x1ff) << 6;
        let mut sum = (right - c)
            .wrapping_mul(fx)
            .wrapping_add((below - c).wrapping_mul(fy));
        let cross = fx.wrapping_mul(fy) >> 15;
        sum = sum.wrapping_add(cross.wrapping_mul(diagonal - below - right + c));
        sum = sum.wrapping_add(sum).wrapping_add(c << 16);
        let value = sum >> 8;
        if quality >= 0x10 {
            return value;
        }
        let slot = ((fx.wrapping_add(value) ^ (fx.wrapping_mul(fy) >> 15)) >> 5) & 0x7f;
        let factor = self.hash(slot as usize);
        if quality == 0 {
            return (value.wrapping_mul(factor) as u32 >> 16) as i32;
        }
        let shrink = (0x10 - quality).wrapping_mul(factor) >> 5;
        let scale = 0xffff - shrink;
        (value.wrapping_mul(scale) as u32 >> 16) as i32
    }

    /// F841: the stars of the sector at (`x`, `y`) (without the render set-up). A generated
    /// sector's stars come from the original's small generator seeded by the sector.
    pub fn stars(&self, x: u16, y: u16) -> Vec<Star> {
        let Some(k) = self.special(x, y) else {
            let count = (self.star_count(u32::from(x), u32::from(y), 0) >> 10).clamp(0, 0x3e);
            let key = u32::from(x) | u32::from(y) << 16;
            let mut generator = Generator::new(key);
            for _ in 0..3 {
                generator.step();
            }
            return (0..count)
                .map(|_| {
                    generator.step();
                    let a = generator.a;
                    let b = generator.b;
                    let z = ((a & 0xff_0000) >> 16) as u8;
                    let y = ((((a & 0xff00) as u16) as i16) >> 9) as u8;
                    let x = (((((a & 0x1fe) >> 1) as u8) as i8) >> 1) as u8;
                    let low = self.bytes[TYPE_HIGH + ((b >> 16) & 0x1f) as usize];
                    let high = self.bytes[TYPE_LOW + (b & 0x1f) as usize];
                    Star {
                        vector: [9, 0, z, y, x, 0],
                        kind: u16::from(high) << 8 | u16::from(low),
                    }
                })
                .collect();
        };
        let (start, end) = (self.start(k) as usize, self.start(k + 1) as usize);
        (start..end)
            .map(|i| Star {
                vector: self.bytes[6 * i..6 * i + 6].try_into().unwrap(),
                kind: u16::from_le_bytes([
                    self.bytes[WORDS + 2 * i],
                    self.bytes[WORDS + 2 * i + 1],
                ]),
            })
            .collect()
    }

    /// The drive record of a class (0 to 14).
    pub fn drive(&self, class: usize) -> Option<Drive> {
        (class < DRIVE_BYTES / 16).then(|| {
            let at = DRIVES + 16 * class;
            Drive {
                power: dword(self.bytes, at) as i32,
                fuel_modifier: dword(self.bytes, at + 4) as i32,
                mass: dword(self.bytes, at + 8) as i32,
                name: dword(self.bytes, at + 12),
            }
        })
    }

    /// F852 for a system code: its position, or `None` when the star number is not in
    /// the sector.
    pub fn system_position(&self, code: u32) -> Option<[i32; 3]> {
        let (x, y) = ((code & 0x1fff) as u16, ((code >> 13) & 0x1fff) as u16);
        let number = ((code >> 26) & 0x3f) as usize;
        let stars = self.stars(x, y);
        let star = stars.get(number)?;
        Some(position(x, y, &star.vector))
    }
}

/// F861's generator: `a' = rotl(a, 3) + (a + b)`, `b' = rotl(a + b, 5)`. It starts with
/// `a` the sector key's halves swapped and `b` the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Generator {
    pub a: u32,
    pub b: u32,
}

impl Generator {
    pub fn new(key: u32) -> Self {
        Self {
            a: key.rotate_left(16),
            b: key,
        }
    }

    pub fn step(&mut self) {
        let sum = self.a.wrapping_add(self.b);
        let a = self.a.rotate_left(3).wrapping_add(sum);
        self.b = sum.rotate_left(5);
        self.a = a;
    }
}

/// F856: the 16.16 position of a star vector in sector (`x`, `y`): x from byte 4, y from
/// byte 3, z from byte 2 (signed, with F856's rounding quirk).
pub fn position(x: u16, y: u16, vector: &Vector) -> [i32; 3] {
    let low = |byte: u8| ((i32::from(byte as i8) << 9).wrapping_add(0x8000)) & 0xffff;
    let px = ((u32::from(x) << 16) as i32).wrapping_add(low(vector[4]));
    let py = ((u32::from(y) << 16) as i32).wrapping_add(low(vector[3]));
    let z = i32::from(vector[2] as i8);
    let fraction = if z > 0 { 0xffff } else { 0 };
    let pz = ((((-z) << 16) | fraction) >> 7).wrapping_neg();
    [px, py, pz]
}

/// F833: the distance between two positions in the units the hyperdrive uses
/// (about 800 to the sector): the differences scaled to 15 bits, the square root of their
/// squares, times `0xc8b4`, shifted back.
pub fn distance(a: [i32; 3], b: [i32; 3]) -> i32 {
    let mut shift = 0xe;
    let mut d: [i32; 3] = std::array::from_fn(|k| {
        let value = b[k].wrapping_sub(a[k]) >> 8;
        if value < 0 {
            value.wrapping_neg()
        } else {
            value
        }
    });
    loop {
        for k in 0..3 {
            if d[k] > 0x7fff {
                d = d.map(|v| v >> 1);
                shift -= 1;
            }
        }
        if d.iter().all(|v| *v <= 0x7fff) {
            break;
        }
    }
    let sum = d
        .iter()
        .fold(0i32, |sum, v| sum.wrapping_add(v.wrapping_mul(*v)));
    // `fild` reads the sum as signed; a negative one has no root and `fistp` leaves zero.
    let root = if sum < 0 {
        0
    } else {
        f64::from(sum).sqrt() as i32
    };
    root.wrapping_mul(0xc8b4) >> (shift & 31)
}

/// Strings in the system string table (`DATA_006796`).
pub const STRING_COUNT: usize = 267;

/// The 267 system strings: 117 hand-made star names, then star, planet and system
/// descriptions (cache v22).
#[derive(Debug, Clone, Copy)]
pub struct Strings<'a> {
    bytes: &'a [u8],
}

impl<'a> Strings<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        let zeros = bytes.iter().filter(|b| **b == 0).count();
        if zeros != STRING_COUNT || bytes.last() != Some(&0) {
            return Err("the system strings are not 267 terminated strings".to_owned());
        }
        Ok(Self { bytes })
    }

    /// String `index` (without its zero).
    pub fn get(&self, index: usize) -> Option<&'a [u8]> {
        self.bytes
            .split(|b| *b == 0)
            .nth(index)
            .filter(|_| index < STRING_COUNT)
    }
}

/// F827: rotate a 16-bit value left by `count`.
fn rotate16(value: u16, count: u32) -> u16 {
    value.rotate_left(count & 15)
}

impl Tables<'_> {
    /// The syllable `index` (0..32) of `DATA_006802`: up to three letters in four bytes.
    fn syllable(&self, index: usize) -> &[u8] {
        let at = SPECIAL + 404 + 4 * index;
        let slot = &self.bytes[at..at + 4];
        &slot[..slot.iter().position(|b| *b == 0).unwrap_or(4)]
    }

    /// F830 for a system code: the hand-made star's name from the string table, the
    /// gateway's, or a pronounceable name made from the sector and star number.
    pub fn system_name(&self, strings: &Strings, code: u32) -> Option<Vec<u8>> {
        let (x, y) = ((code & 0x1fff) as u16, ((code >> 13) & 0x1fff) as u16);
        let number = ((code >> 26) & 0x3f) as usize;
        if let Some(k) = self.special(x, y) {
            let index = self.start(k) as usize + number;
            return strings.get(index).map(<[u8]>::to_vec);
        }
        // The only other special name: the Gateway system.
        if code == 0x1aa5_1717 {
            return Some(b"Gateway".to_vec());
        }
        let mut b = x.wrapping_add(number as u16);
        let mut d = y.wrapping_add(b);
        b = rotate16(b, 3);
        b = b.wrapping_add(d);
        d = rotate16(d, 5);
        d = d.wrapping_add(b);
        d = rotate16(d, 4);
        b = rotate16(b, (number & 0xf) as u32);
        b = b.wrapping_add(d);
        let first = usize::from((b & 0x7c) >> 2);
        let b = rotate16(b, 11);
        let second = usize::from((b & 0x7c) >> 2);
        let b = rotate16(b, 11);
        let third = usize::from((b & 0x7c) >> 2);
        let mut name = Vec::new();
        for index in [first, second, third] {
            name.extend_from_slice(self.syllable(index));
        }
        if let Some(letter) = name.first_mut() {
            *letter = letter.wrapping_add(0xe0);
        }
        Some(name)
    }
}

/// F830's `(x,y)` form (selector `0x1fe`): the sector relative to Sol's, `(0,0)`.
pub fn sector_label(code: u32) -> String {
    let x = i32::try_from(code & 0x1fff).unwrap_or(0) - 0x1718;
    let y = i32::try_from((code >> 13) & 0x1fff).unwrap_or(0) - 0x1524;
    format!("({x},{y})")
}
