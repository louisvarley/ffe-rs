//! The original's hand-made galaxy table and system-parameter lookups, ported
//! from `csystem.txt` and `ffe.asm` (F868-F871). The table is game data: it is
//! imported from the player's executable into the content cache (v15, the
//! `SystemTables` asset) and read here through a borrowed view.
//! Verified against the executed original (`docs/notes/galaxy.md`).

/// Bytes of the imported block: from `DATA_007106` to the end of `DATA_007138`.
pub const TABLES_BYTES: usize = 20_984;
/// Offset of the galaxy table (`DATA_007110`) in the block.
pub const GALAXY_OFFSET: usize = 432;
/// Bytes per galaxy record.
pub const RECORD_BYTES: usize = 54;
/// Records in the table; a zero code (a 330th, blank record) follows them.
pub const RECORDS: usize = 329;
/// Offset of the four special system codes (`DATA_007116`).
pub const SPECIAL_OFFSET: usize = 18_252;

/// Table offsets in the block of the system generator's data (`DATA_007119..7138`).
pub mod tables {
    pub const TYPE: usize = 18_492; // 7119, 23 dwords
    pub const MODEL: usize = 18_584; // 7120
    pub const MASS: usize = 18_676; // 7121, 24 dwords (thresholds, zero ends)
    pub const SHAPE: usize = 18_772; // 7122
    pub const TEMPERATURE: usize = 18_864; // 7123
    pub const LIGHT: usize = 18_956; // 7124
    pub const HANDLER: usize = 19_048; // 7125, relocated pointers: not used
    pub const SEED: usize = 19_140; // 7126, 7127 and the government 7128
    pub const SPACING: usize = 19_152; // 7129, 256 dwords
    pub const STARPORT_LONGITUDE: usize = 20_176; // 7130, 16 pairs of dwords
    pub const STARPORT_LATITUDE: usize = 20_180; // 7131
    pub const STARPORT_MODEL: usize = 20_304; // 7132
    pub const HANDCODED_INDEX: usize = 20_496; // 7133, pairs of dwords
    pub const TEMPLATES: usize = 20_592; // 7134
    pub const STATION_MODEL: usize = 20_936; // 7136, 7137, 7138: four dwords each
}

/// A borrowed view of the imported block.
#[derive(Debug, Clone, Copy)]
pub struct Tables<'a> {
    bytes: &'a [u8],
}

/// What F869 reports about a system (`DATA_007106 + 0x1b0 + 54 * index + ...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Details {
    /// The dwords at `+46` and `+50`.
    pub words: [u32; 2],
    /// The bytes at `+5..+9`.
    pub bytes: [u32; 5],
    /// The word at `+10` (the allegiance in its upper ten bits).
    pub allegiance: u32,
}

/// What F870 reports: the population and danger bytes, the allegiance word and
/// the stock string it leaves in `DATA_008790`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Market {
    /// The byte at `+4`.
    pub population: u32,
    /// The byte at `+12`.
    pub danger: u32,
    /// The word at `+10`.
    pub allegiance: u32,
    /// The 37 bytes the original leaves from `DATA_008790`: the 31 bytes at `+13`,
    /// zeros, and 0xd at byte 32 (the first byte of the next label, `DATA_008791`).
    pub stock: [u8; 37],
}

/// What F871 reports: the two control nibbles of the byte at `+44`, the
/// allegiance (`+10 >> 6`), the greater control and the two ranks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    pub imperial: u32,
    pub federal: u32,
    pub allegiance: u32,
    pub maximum: u32,
    pub minor_rank: u32,
    pub major_rank: u32,
}

impl<'a> Tables<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() != TABLES_BYTES {
            return Err("The system tables block has the wrong size.".to_owned());
        }
        let tables = Tables { bytes };
        if tables.code(RECORDS) != 0 || (0..RECORDS).any(|i| tables.code(i) == 0) {
            return Err("The galaxy table is not 329 records and a terminator.".to_owned());
        }
        Ok(tables)
    }

    /// A dword of the block.
    pub fn dword(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    /// The 54 bytes at `index` records from `DATA_007110`. The classes -2 to -8
    /// of a system outside the table read the eight records' worth of tables
    /// before it (`DATA_007106..7109`), as the original does.
    fn record(&self, index: i32) -> &[u8] {
        let start = GALAXY_OFFSET as i64 + i64::from(index) * RECORD_BYTES as i64;
        let start = usize::try_from(start).expect("a system class reads inside the block");
        &self.bytes[start..start + RECORD_BYTES]
    }

    /// A record's system code (`+0`); the table ends with a zero code.
    pub fn code(&self, index: usize) -> u32 {
        self.dword(GALAXY_OFFSET + index * RECORD_BYTES)
    }

    /// F868: the index of a system in the table, or a negative class for a
    /// system that is not in it: -1 for one of the four special codes, otherwise
    /// -2 to -8 by the squared distance from the galactic centre.
    pub fn index(&self, code: u32) -> i32 {
        if let Some(index) = (0..RECORDS).find(|i| self.code(*i) == code) {
            return index as i32;
        }
        let masked = code & 0x03ff_ffff;
        if (0..4).any(|i| self.dword(SPECIAL_OFFSET + 4 * i) == masked) {
            return -1;
        }
        let jitter = i64::from((code >> 24 & 0xff) >> 2 & 7);
        let x = i64::from(code & 0x1fff) - 0x1718;
        let y = i64::from(code >> 13 & 0x1fff) - 0x1524;
        let x = x.abs() + jitter;
        let y = y.abs() + jitter;
        let squared = (x * x + y * y) as u32;
        match squared {
            0x4c90.. => -8,
            0x121.. => -7,
            0x64.. => -6,
            0x31.. => -5,
            0x19.. => -4,
            0x9.. => -3,
            _ => -2,
        }
    }

    /// F869: the details of a system (the record the index selects; a negative
    /// index reads the tables before the galaxy, as the original does).
    pub fn details(&self, code: u32) -> Details {
        let r = self.record(self.index(code));
        let dword = |at: usize| u32::from_le_bytes(r[at..at + 4].try_into().unwrap());
        Details {
            words: [dword(46), dword(50)],
            bytes: std::array::from_fn(|i| u32::from(r[5 + i])),
            allegiance: u32::from(u16::from_le_bytes([r[10], r[11]])),
        }
    }

    /// F870: the market parameters of a system.
    pub fn market(&self, code: u32) -> Market {
        let r = self.record(self.index(code));
        let mut stock = [0u8; 37];
        stock[..31].copy_from_slice(&r[13..44]);
        stock[32] = 0x0d;
        Market {
            population: u32::from(r[4]),
            danger: u32::from(r[12]),
            allegiance: u32::from(u16::from_le_bytes([r[10], r[11]])),
            stock,
        }
    }

    /// F871: the control of a system. `ranks` are the player's two values
    /// (`DATA_008912`, `DATA_008913`), assigned to the major and minor rank by
    /// which control is greater.
    pub fn control(&self, code: u32, ranks: [u32; 2]) -> Control {
        let r = self.record(self.index(code));
        let imperial = u32::from(r[44] >> 4);
        let federal = u32::from(r[44] & 0xf);
        let allegiance = u32::from(u16::from_le_bytes([r[10], r[11]])) >> 6;
        let (minor_rank, major_rank, maximum) =
            if imperial > federal || (imperial == federal && allegiance == 1) {
                (ranks[0], ranks[1], imperial)
            } else {
                (ranks[1], ranks[0], federal)
            };
        Control {
            imperial,
            federal,
            allegiance,
            maximum,
            minor_rank,
            major_rank,
        }
    }
}
