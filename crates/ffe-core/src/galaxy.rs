//! The original's hand-made galaxy table and system-parameter lookups, ported
//! from `csystem.txt` and `ffe.asm` (F868-F871). The table is game data: it is
//! imported from the player's executable into the content cache (v15, the
//! `SystemTables` asset) and read here through a borrowed view.
//! Verified against the executed original (`docs/notes/galaxy-table.md`).

/// Bytes of the imported block: from `DATA_007106` to the end of `DATA_007138`,
/// with floppy 1.02's 329 galaxy records. A release with more records (Elite
/// Club shareware 1.06 has 339) has a block longer by [`RECORD_BYTES`] each;
/// everything after the table moves down with it ([`Tables::new`]).
pub const TABLES_BYTES: usize = 20_984;
/// The most galaxy records a block may hold.
pub const MAX_RECORDS: usize = 1_024;
/// Offset of the galaxy table (`DATA_007110`) in the block.
pub const GALAXY_OFFSET: usize = 432;
/// Bytes per galaxy record.
pub const RECORD_BYTES: usize = 54;
/// Records in floppy 1.02's table; a zero code (a 330th, blank record) follows
/// them.
pub const RECORDS: usize = 329;
/// Offset of the four special system codes (`DATA_007116`) after 329 records.
pub const SPECIAL_OFFSET: usize = 18_252;

/// Table offsets in the block of the system generator's data (`DATA_007119..7138`),
/// for 329 records: [`Tables::dword`] and [`Tables::bytes`] move any offset
/// from [`SPECIAL_OFFSET`] on past a longer galaxy table.
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
    /// The galaxy records before the zero code.
    records: usize,
}

/// What F869 reports about a system (`DATA_007106 + 0x1b0 + 54 * index + ...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Details {
    /// The dwords at `+46` and `+50`.
    pub words: [u32; 2],
    /// The bytes at `+5..+9`, which `F622` copies to `DATA_008995`, `+0x1b6`,
    /// `+0x1b7`, `+0x1b8` and the byte before the allegiance. `bytes[0]` is the
    /// starport tech level (`DATA_008995`) `F383` compares with each equipment
    /// record's `+0x13`.
    pub bytes: [u32; 5],
    /// The word at `+10` (the allegiance in its upper ten bits).
    pub allegiance: u32,
}

impl Details {
    /// `DATA_008995`: the starport's technology level, as `F869` copies it from
    /// the system record's `+5` and `F622` stores for `F383`.
    pub fn tech_level(&self) -> u16 {
        self.bytes[0] as u16
    }
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
        let records = Self::count(bytes)
            .ok_or("The galaxy table has no terminating zero code.".to_owned())?;
        if records < RECORDS {
            return Err(format!(
                "The galaxy table has {records} records, fewer than the original's {RECORDS}."
            ));
        }
        if bytes.len() != Self::block_bytes(records) {
            return Err("The system tables block has the wrong size.".to_owned());
        }
        Ok(Tables { bytes, records })
    }

    /// The galaxy records before the first zero code in `bytes`, if one is
    /// there within [`MAX_RECORDS`].
    pub fn count(bytes: &[u8]) -> Option<usize> {
        (0..=MAX_RECORDS).find_map(|index| {
            let at = GALAXY_OFFSET + index * RECORD_BYTES;
            let code = bytes.get(at..at + 4)?;
            (code == [0; 4]).then_some(index)
        })
    }

    /// The block's size with `records` galaxy records.
    pub fn block_bytes(records: usize) -> usize {
        TABLES_BYTES + (records - RECORDS) * RECORD_BYTES
    }

    /// The galaxy records (329 in floppy 1.02 and CD 1.01).
    pub fn records(&self) -> usize {
        self.records
    }

    /// Where a 329-record offset is in this block: offsets from the special
    /// codes on follow the longer table.
    fn at(&self, offset: usize) -> usize {
        if offset >= SPECIAL_OFFSET {
            offset + (self.records - RECORDS) * RECORD_BYTES
        } else {
            offset
        }
    }

    /// A slice of the block, at a 329-record offset.
    pub fn bytes(&self, offset: usize, length: usize) -> &[u8] {
        let at = self.at(offset);
        &self.bytes[at..at + length]
    }

    /// A dword of the block, at a 329-record offset.
    pub fn dword(&self, offset: usize) -> u32 {
        let at = self.at(offset);
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
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
        let at = GALAXY_OFFSET + index * RECORD_BYTES;
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }

    /// F868: the index of a system in the table, or a negative class for a
    /// system that is not in it: -1 for one of the four special codes, otherwise
    /// -2 to -8 by the squared distance from the galactic centre.
    pub fn index(&self, code: u32) -> i32 {
        if let Some(index) = (0..self.records).find(|i| self.code(*i) == code) {
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

    /// `DATA_008995`: the tech level F383 reads for the starport of `code`.
    pub fn tech_level(&self, code: u32) -> u16 {
        self.details(code).tech_level()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A block with `records` galaxy records (codes 1..), the special codes and
    /// a marker dword at the generator tables' `SEED`.
    fn block(records: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; Tables::block_bytes(records)];
        for index in 0..records {
            let at = GALAXY_OFFSET + index * RECORD_BYTES;
            bytes[at..at + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
        }
        let shift = (records - RECORDS) * RECORD_BYTES;
        bytes[SPECIAL_OFFSET + shift..SPECIAL_OFFSET + shift + 4]
            .copy_from_slice(&0x0123_4567u32.to_le_bytes());
        bytes[tables::SEED + shift..tables::SEED + shift + 4]
            .copy_from_slice(&0xdead_beefu32.to_le_bytes());
        bytes
    }

    /// Elite Club shareware 1.06 (and JJFFE 2.8a7's data) has 339 records:
    /// the generator tables follow the longer galaxy table, and the records
    /// after the 329th are found.
    #[test]
    fn a_longer_galaxy_table_moves_the_tables_after_it() {
        for records in [RECORDS, 339] {
            let bytes = block(records);
            let tables = Tables::new(&bytes).unwrap();
            assert_eq!(tables.records(), records);
            assert_eq!(tables.dword(tables::SEED), 0xdead_beef, "{records}");
            assert_eq!(tables.dword(SPECIAL_OFFSET), 0x0123_4567, "{records}");
            assert_eq!(tables.index(records as u32), records as i32 - 1);
        }
        // A wrong length for the record count, or too few records, is refused.
        let mut long = block(339);
        long.pop();
        assert!(Tables::new(&long).is_err());
        let mut short = block(RECORDS);
        let at = GALAXY_OFFSET + 10 * RECORD_BYTES;
        short[at..at + 4].fill(0);
        assert!(Tables::new(&short).is_err());
    }
}
