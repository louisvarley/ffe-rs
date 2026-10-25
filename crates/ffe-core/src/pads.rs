//! F650's open-air starport pads (`DATA_006175..6179` and the start of
//! `DATA_006180`).
//!
//! At an open-air starport (the station object's class byte `+0x14c` has bit
//! 0x40) F650 turns the allocated bay into an offset in the station's frame:
//! the model chooses one of five tables of three-dword pads, the bay indexes
//! the table, the pad is turned by the station's 3x3 Q31 rows through F1669 and
//! each component is shifted left ten. At an indoor station F650 returns the
//! fixed `(0, 0, -1)` and never touches the tables; the caller supplies that
//! branch because it owns the class byte.
//!
//! The original indexes the table with the bay and no bounds check. F880 bounds
//! `+0xce` to at most 7, but the last table (`DATA_006179`, the default models)
//! holds only six pads, so its bays 6 and 7 read the first six dwords of the
//! ship index tables at `DATA_006180`. [`Pads`] keeps those six dwords and reads
//! past exactly, rather than diagnosing an overrun the original does not have.
//! The cache imports the 192 table bytes plus that 24-byte window.

/// Bytes of the five pad tables: 16 pads of three dwords (`DATA_006175..6179`).
pub const TABLE_BYTES: usize = 0xc0;
/// The first six dwords of `DATA_006180`, which the documented overrun reads.
pub const OVERRUN_BYTES: usize = 0x18;
/// The whole imported block.
pub const PADS_BYTES: usize = TABLE_BYTES + OVERRUN_BYTES;
/// F880's highest bay number: `+0xce` is at most 7.
pub const MAX_BAY: u16 = 7;
/// F650's position at an indoor station (not open-air): `(0, 0, -1)`.
pub const INDOOR_OFFSET: [i64; 3] = [0, 0, -1];
/// The station class byte `+0x14c` bit that marks an open-air starport.
pub const OPEN_AIR_CLASS: u8 = 0x40;

/// The five `DATA_006175..6179` tables and the `DATA_006180` overrun window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pads {
    /// The 16 pads of the five tables, in declaration order from
    /// `DATA_006175`: 3 dwords per pad.
    table: [i32; 48],
    /// `DATA_006180` dwords 0..5: Eagle, Eagle2, Saker, Falcon, Merlin, Hawk.
    overrun: [i32; 6],
}

impl Pads {
    /// Decode the imported block. Exactly [`PADS_BYTES`] bytes are required:
    /// five tables (192) and the overrun window (24).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != PADS_BYTES {
            return Err(format!(
                "pad tables must be {PADS_BYTES} bytes, not {}",
                bytes.len()
            ));
        }
        let mut words = [0i32; PADS_BYTES / 4];
        for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(4)) {
            *word = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let mut table = [0i32; 48];
        table.copy_from_slice(&words[..48]);
        let mut overrun = [0i32; 6];
        overrun.copy_from_slice(&words[48..]);
        Ok(Pads { table, overrun })
    }

    /// F650's model-to-table switch (`ffe.asm:44444-44471`), as the dword offset
    /// of the table in the imported block.
    ///
    /// | Model | Table |
    /// |---|---|
    /// | 0x56 | `DATA_006175` |
    /// | 0x59 | `DATA_006176` |
    /// | 0x55 | `DATA_006177` |
    /// | < 0x53 | `DATA_006178` |
    /// | anything else (0x53, 0x54, 0x57, 0x58, 0x5a, ...) | `DATA_006179` |
    pub fn table(model: u32) -> usize {
        match model {
            0x56 => 0,
            0x59 => 6,
            0x55 => 12,
            model if model < 0x53 => 18,
            _ => 30,
        }
    }

    /// The selected pad's three dwords, reading past the last table into the
    /// `DATA_006180` window exactly as the original does.
    pub fn pad(&self, model: u32, bay: u16) -> Result<[i32; 3], String> {
        if bay > MAX_BAY {
            return Err(format!("bay {bay} exceeds F880's maximum of {MAX_BAY}"));
        }
        let first = Self::table(model) + usize::from(bay) * 3;
        Ok(std::array::from_fn(|i| {
            let index = first + i;
            if index < self.table.len() {
                self.table[index]
            } else {
                self.overrun[index - self.table.len()]
            }
        }))
    }

    /// F650's open-air offset: the selected pad through F1669 (the vector times
    /// the station rows) and then each 32-bit component shifted left ten. The
    /// original's `shl dword` wraps, so the shift is wrapping; the components
    /// are widened to `i64` so a caller can add its own 64-bit origin.
    pub fn offset(&self, model: u32, bay: u16, station_rows: [i32; 9]) -> Result<[i64; 3], String> {
        let pad = self.pad(model, bay)?;
        Ok(crate::frames::vec_mat_mul(pad, &station_rows).map(|v| i64::from(v.wrapping_shl(10))))
    }

    /// F994's placement of a ship on a pad: F650's offset, then F994 adds the
    /// ship's collision radius (`+0x140`) along its second row (`+0xc..+0x14`,
    /// the up row). `base` is the station's position and `up` the row the ship
    /// was given (F994 copies the station's orientation to it first). The
    /// product is F1521's, widened to `i64` by F994's `Int64Add32`.
    #[allow(clippy::too_many_arguments)]
    pub fn seat(
        &self,
        base: [i64; 3],
        model: u32,
        bay: u16,
        station_rows: [i32; 9],
        open_air: bool,
        radius: i32,
        up: [i32; 3],
    ) -> Result<[i64; 3], String> {
        let offset = self.position(model, bay, station_rows, open_air)?;
        Ok(std::array::from_fn(|k| {
            base[k]
                .wrapping_add(offset[k])
                .wrapping_add(i64::from(crate::model::multiply(radius, up[k])))
        }))
    }

    /// F650's whole position branch. `open_air` is the station class byte
    /// `+0x14c`'s bit 0x40, which the caller supplies: without it F650 returns
    /// [`INDOOR_OFFSET`] without looking at the model or bay.
    pub fn position(
        &self,
        model: u32,
        bay: u16,
        station_rows: [i32; 9],
        open_air: bool,
    ) -> Result<[i64; 3], String> {
        if open_air {
            self.offset(model, bay, station_rows)
        } else {
            Ok(INDOOR_OFFSET)
        }
    }
}
