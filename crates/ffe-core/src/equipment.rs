//! The equipment table and the player's fitted loadout.
//!
//! The original keeps the 66 equipment definitions at `DATA_005289`
//! (`ffedat.asm:6425`, 22 bytes each) and lists them at a starport with `F383`
//! (`ffe.asm:28564`). `F392` (`ffe.asm:28998`) dispatches a record to a
//! category handler, and `F393` (`ffe.asm:29030`) reads its `+0x12` selector
//! as a bit in the ship's general-equipment dword (`ship+0xc8`):
//!
//! ```text
//! bit = (record[0x12] ^ 0x10) & 0x1f
//! ```
//!
//! The same dword is what `F991` (`ffe.asm:73469`), `F1116` (`82223`) and
//! `F1120` (`82429`) test for the view-row icons and the engines-off page's
//! system lamps. Its four bytes are `ship+0xc8..+0xcb`; the category handlers
//! for lasers (2), missiles (5) and shield generators (4) use `+0xd2..+0xd5`,
//! `+0xd6..+0xdf` and `+0xe0/+0xe2` instead, so [`Loadout`] keeps those fields
//! separately.
//!
//! The table itself is generated from `ffedat.asm` into
//! `crate::equipment_table` by `tools/gen-equipment-table.py`; it is static
//! game data with no cache dependency, so no cache version changes.

use serde::{Deserialize, Serialize};

use crate::equipment_table::EQUIPMENT;

/// The original's 66 equipment records.
pub const EQUIPMENT_COUNT: usize = 66;
/// The equipment code of the Thargoid Laser (`DATA_005289` record 65,
/// `ffedat.asm:8502`, `+0x12 = 0xd5`). This is the shop/fitted equipment code;
/// F774's AI laser table uses the combat bytes 0x88..0xcf instead
/// (`crate::npc::LASER_TONNAGE`).
pub const THARGOID_LASER: u8 = 0xd5;
/// Ten missile pylons: `ship+0xd6..+0xdf` (`F1120` shows one icon each).
pub const MISSILE_SLOTS: usize = 10;
/// Ten mining machine slots: `DATA_009104` is the list count and `F423` refuses
/// an eleventh ("Mining Machine list full", `0x99d6`). Each record in the list is
/// 52 bytes (`FUNC_000248`, `ffedat.asm` `DATA_009106`); the port keeps only the
/// fitted equipment codes and the count.
pub const MINING_SLOTS: usize = 10;
/// `F1120`'s ten missile icons are at console records `0x3b..=0x44`; the low
/// nibble of a slot byte selects the image (`slot & 0xf`).
pub const ICON_RECORDS: std::ops::RangeInclusive<usize> = 0x3b..=0x44;

/// `DATA_005289`'s record category, the index into `DATA_005297`'s handler
/// pointers (`record[0x10]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// General equipment; the bitfield at `ship+0xc8`.
    General = 0,
    /// Passenger cabins, counted at `player+0x120`.
    Cabin = 1,
    /// Lasers, fitted to `ship+0xd2..+0xd5`.
    Laser = 2,
    /// Hyperdrives/military drives.
    Drive = 3,
    /// Shield generators; each adds 0x40 to `ship+0xe2` (`F413`).
    Shield = 4,
    /// Missiles, fitted to `ship+0xd6..+0xdf` (`F418`).
    Missile = 5,
    /// Mining machines.
    Mining = 6,
}

/// One 22-byte record of `DATA_005289`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equipment {
    /// `DATA_005299[0x9875 + index]`, the original's name string.
    pub name: &'static str,
    /// `+0x00`, the price before `F383`'s tech multiplier.
    pub price: u32,
    /// `+0x04`, the removal price before the multiplier (the "for Removal"
    /// column of template 0x98f5).
    pub removal: u32,
    /// `+0x08`, the weight in tonnes (the "Wt" column).
    pub mass: u32,
    /// `+0x10`, the category/handler index.
    pub category: Category,
    /// `+0x12`: the general-equipment bit selector, laser code, missile code,
    /// shield/cabin/drive selector, depending on the category.
    pub code: u8,
    /// `+0x13`, the minimum starport tech level (`DATA_008995`) F383 tests.
    pub tech_level: u8,
    /// `+0x14`, F383's second (used/second-hand) price branch.
    pub used: bool,
    /// `+0x11`, 0x80 in most records, 0x00 or 0x83 in three. No routine this
    /// slice ports reads it; kept so the decoded record is complete.
    pub flags: u8,
}

/// The decoded table.
pub fn equipment() -> &'static [Equipment; EQUIPMENT_COUNT] {
    &EQUIPMENT
}

/// One record by index, if in range.
pub fn get(index: usize) -> Option<&'static Equipment> {
    EQUIPMENT.get(index)
}

/// The general-equipment bits any `Category::General` record can set. Bits 14,
/// 15 and 16 have no record; a fitted loadout must leave them clear.
pub const GENERAL_MASK: u32 = 0xff_fe_3f_ff;

/// `ship+0xc8` bit 7: the Auto targetter (record code `0x17`). `F1054`'s `T`
/// and `Shift+T` (action `0x74`) only toggle `DATA_007246`/`DATA_007247` when
/// this is fitted (`ffe.asm:78737-78768`).
pub const AUTO_TARGETTER_BIT: u32 = 1 << 7;
/// `ship+0xc8` bit 9: the Missile Viewer (record code `0x19`). `F1054`'s `F10`
/// (action `0xfb`) needs it **and** a missile in flight (`player+0xef`).
pub const MISSILE_VIEWER_BIT: u32 = 1 << 9;
/// `ship+0xc8` bit 11: the Chaff Dispenser (record code `0x1b`).
pub const CHAFF_BIT: u32 = 1 << 11;
/// `ship+0xc8` bit 19: the E.C.M. System (record code `0x03`).
pub const ECM_BIT: u32 = 1 << 19;
/// `ship+0xc8` bit 23: the Naval E.C.M. System (record code `0x07`).
pub const NAVAL_ECM_BIT: u32 = 1 << 23;

/// The ECM equipment `F935` finds, in its test order: Naval first (`+0xca`
/// bit 7, `DATA_008835 |= 0xc`), then the standard E.C.M. (`+0xca` bit 3,
/// `DATA_008835 |= 0x4`). The source has no energy, ammo or range cost: the
/// only gate is the fitted bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecm {
    /// `ship+0xc8` bit 19.
    Standard,
    /// `ship+0xc8` bit 23, which also sets `DATA_008835` bit 3.
    Naval,
}

/// The missile record whose code is `code` (`ship+0xd6..` stores this byte).
pub fn missile_by_code(code: u8) -> Option<&'static Equipment> {
    EQUIPMENT
        .iter()
        .find(|item| item.category == Category::Missile && item.code == code)
}

/// True for the ten missile codes (0xc0, 0xc1, 0x82..0x89).
pub fn is_missile_code(code: u8) -> bool {
    missile_by_code(code).is_some()
}

/// True for a laser code in the equipment table (`F403` copies `record[0x12]`
/// straight into `ship+0xd2..+0xd5`).
pub fn is_laser_code(code: u8) -> bool {
    EQUIPMENT
        .iter()
        .any(|item| item.category == Category::Laser && item.code == code)
}

/// `F383`'s tech-dependent price multiplier: 12 at tech >= 0xcc, 11 at 0xb3,
/// 10 at 0x17, otherwise 9.
pub fn tech_multiplier(tech_level: u16) -> u32 {
    if tech_level >= 0xcc {
        0xc
    } else if tech_level >= 0xb3 {
        0xb
    } else if tech_level >= 0x17 {
        0xa
    } else {
        9
    }
}

/// The price to fit `index` at a starport of `tech_level` (`record.price *
/// tech_multiplier`). The original's used branch uses a fresh random value from
/// `F34`/`F35`; the port uses the same tech multiplier for both branches, so a
/// second-hand price is deterministic (documented deviation).
pub fn fit_price(index: usize, tech_level: u16) -> Option<u32> {
    get(index).map(|item| item.price.saturating_mul(tech_multiplier(tech_level)))
}

/// The removal price, computed the same way as [`fit_price`] (`F383` multiplies
/// both words before storing them).
pub fn removal_price(index: usize, tech_level: u16) -> Option<u32> {
    get(index).map(|item| item.removal.saturating_mul(tech_multiplier(tech_level)))
}

/// One row of `F383`'s Upgrades list at a starport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutfittingRow {
    /// Index into [`equipment`].
    pub index: usize,
    pub price: u32,
    pub removal: u32,
    pub mass: u32,
    /// F392/F393 says this record is already fitted.
    pub fitted: bool,
}

/// What a starport at `tech_level` offers, in `DATA_005289` order: `F383` keeps
/// a record when the station's tech level is at least the record's (`station <
/// record` skips). `lasers` is `ship+0xd2..+0xd5`, which is where the laser
/// category's fitted check reads.
pub fn outfitting(tech_level: u16, loadout: &Loadout, lasers: &[u8; 4]) -> Vec<OutfittingRow> {
    EQUIPMENT
        .iter()
        .enumerate()
        .filter(|(_, item)| tech_level >= u16::from(item.tech_level))
        .map(|(index, item)| OutfittingRow {
            index,
            price: item.price.saturating_mul(tech_multiplier(tech_level)),
            removal: item.removal.saturating_mul(tech_multiplier(tech_level)),
            mass: item.mass,
            fitted: loadout.fitted(index, lasers),
        })
        .collect()
}

/// The player's fitted equipment: the words the HUD, the equipment page and the
/// starport's fitted flags read.
///
/// `general` is `ship+0xc8` (the `Category::General` bitfield), `missiles` is
/// `ship+0xd6..+0xdf`, `shield_generators` is the count behind `F413`'s
/// `ship+0xe2`, `cabins`/`occupied_cabins` are `DATA_008890`/`DATA_008892`,
/// `drive` is `ship+0xd0` and `mining` is `DATA_009104`'s ten machine records.
/// Lasers live in `ffe-rs`'s `Combat` (`ship+0xd2..+0xd5`) so there is one
/// source for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Loadout {
    #[serde(default)]
    pub general: u32,
    #[serde(default)]
    pub missiles: [u8; MISSILE_SLOTS],
    #[serde(default)]
    pub shield_generators: u8,
    #[serde(default)]
    pub cabins: u16,
    /// `DATA_008892`: the cabins carrying passengers. `F401` refuses to remove a
    /// cabin while every one is occupied ("You cannot remove an occupied
    /// cabin."). The port has no passenger missions yet, so saves load with 0.
    #[serde(default)]
    pub occupied_cabins: u16,
    /// `ship+0xd0`: one drive class byte, 0 = no drive. `F409` refuses a second
    /// drive and `F410`/`F411` remove the one fitted.
    #[serde(default)]
    pub drive: u8,
    /// `DATA_009104`'s ten machine records; a zero slot is empty. `F423` appends
    /// and `FUNC_000251`/`FUNC_000249` remove the last idle machine.
    #[serde(default)]
    pub mining: [u8; MINING_SLOTS],
}

impl Loadout {
    /// The DOS default start (`F998`'s type-0 arm, the Saker Mk III):
    /// `ship+0xc8 = 0x40240020` - scanner (bit 18), automatic pilot (bit 21),
    /// atmospheric shielding (bit 30) and the navigation computer (bit 5) -
    /// two 0x82 missiles (`KL760 Homing Missile`), drive class 2 (`ship+0xd0`,
    /// `F998`'s `mov byte [ebx+0xd0],0x2`), no shield generator (`+0xe2 = 0`),
    /// no passenger cabins and no mining machines. The front laser is
    /// `Combat::new_game`'s 0x88.
    pub fn new_game() -> Self {
        let mut missiles = [0; MISSILE_SLOTS];
        missiles[0] = 0x82;
        missiles[1] = 0x82;
        Self {
            general: 0x4024_0020,
            missiles,
            shield_generators: 0,
            cabins: 0,
            occupied_cabins: 0,
            drive: 2,
            mining: [0; MINING_SLOTS],
        }
    }

    /// The general-equipment dword as its four bytes (`ship+0xc8..+0xcb`).
    pub fn bytes(&self) -> [u8; 4] {
        self.general.to_le_bytes()
    }

    /// `F392`: is record `index` fitted? Lasers are checked against the passed
    /// mount words. `F412` compares the one drive byte, and `F427` returns the
    /// mining machine count for **every** mining record (the two records share
    /// the list), so any machine makes both rows read as fitted.
    pub fn fitted(&self, index: usize, lasers: &[u8; 4]) -> bool {
        let Some(item) = get(index) else {
            return false;
        };
        match item.category {
            Category::General => self.general & (1 << ((item.code ^ 0x10) & 0x1f)) != 0,
            Category::Cabin => self.cabins != 0,
            Category::Laser => lasers.contains(&item.code),
            Category::Shield => self.shield_generators != 0,
            Category::Missile => self.missiles.contains(&item.code),
            Category::Drive => self.drive != 0 && self.drive == item.code,
            Category::Mining => self.mining_count() != 0,
        }
    }

    /// The fitted drive record (`ship+0xd0`), when a drive is fitted.
    pub fn drive_record(&self) -> Option<&'static Equipment> {
        equipment()
            .iter()
            .find(|item| item.category == Category::Drive && item.code == self.drive)
    }

    /// The fitted drive class, 0 when there is no drive.
    pub fn drive_class(&self) -> u8 {
        self.drive
    }

    /// `F903`'s military branch: class 10 and up burn military fuel and use the
    /// military tank (`crate::hyperspace::MILITARY_CLASS`).
    pub fn military_drive(&self) -> bool {
        i32::from(self.drive) >= crate::hyperspace::MILITARY_CLASS
    }

    /// The number of fitted mining machines (`DATA_009104`/`DATA_009105`).
    pub fn mining_count(&self) -> usize {
        self.mining.iter().filter(|code| **code != 0).count()
    }

    /// The fitted machine codes, oldest first.
    pub fn mining_machines(&self) -> impl Iterator<Item = u8> + '_ {
        self.mining.iter().copied().filter(|code| *code != 0)
    }

    /// `FUNC_000248`: append one machine to `DATA_009104`'s list. False when the
    /// ten-entry list is full (`F423`/`F424`'s "Mining Machine list full").
    pub fn add_mining(&mut self, code: u8) -> bool {
        let Some(slot) = self.mining.iter_mut().find(|slot| **slot == 0) else {
            return false;
        };
        *slot = code;
        true
    }

    /// `FUNC_000251`/`FUNC_000249`: remove the last idle machine. The source
    /// scans the list from the end and does not test the record's code, so a
    /// removal row takes whichever machine is last.
    pub fn remove_mining(&mut self) -> Option<u8> {
        let slot = self.mining.iter().rposition(|code| *code != 0)?;
        let code = self.mining[slot];
        self.mining[slot] = 0;
        Some(code)
    }

    /// The passenger capacity (`DATA_008890`): one per fitted Extra Passenger
    /// Cabin.
    pub fn cabin_capacity(&self) -> u16 {
        self.cabins
    }

    /// The cabins without passengers (`DATA_008890 - DATA_008892`).
    pub fn free_cabins(&self) -> u16 {
        self.cabins.saturating_sub(self.occupied_cabins)
    }

    /// The mass of every fitted item in tonnes, the quantity `F390`/`F391` move
    /// between the hold and `DATA_008889`. `lasers` is `ship+0xd2..+0xd5`; the
    /// drive is included (fitting one subtracts its record's `+0x8` mass).
    pub fn mass(&self, lasers: &[u8; 4]) -> u32 {
        let mut total = 0u32;
        for item in equipment()
            .iter()
            .filter(|item| item.category == Category::General)
        {
            if self.general & (1 << ((item.code ^ 0x10) & 0x1f)) != 0 {
                total = total.saturating_add(item.mass);
            }
        }
        if let Some(cabin) = equipment()
            .iter()
            .find(|item| item.category == Category::Cabin)
        {
            total = total.saturating_add(cabin.mass.saturating_mul(u32::from(self.cabins)));
        }
        for code in lasers.iter().copied().filter(|code| *code != 0) {
            if let Some(item) = equipment()
                .iter()
                .find(|item| item.category == Category::Laser && item.code == code)
            {
                total = total.saturating_add(item.mass);
            }
        }
        if let Some(shield) = equipment()
            .iter()
            .find(|item| item.category == Category::Shield)
        {
            total = total.saturating_add(
                shield
                    .mass
                    .saturating_mul(u32::from(self.shield_generators)),
            );
        }
        for code in self.missiles.iter().copied().filter(|code| *code != 0) {
            if let Some(item) = missile_by_code(code) {
                total = total.saturating_add(item.mass);
            }
        }
        if let Some(drive) = self.drive_record() {
            total = total.saturating_add(drive.mass);
        }
        for code in self.mining.iter().copied().filter(|code| *code != 0) {
            if let Some(item) = equipment()
                .iter()
                .find(|item| item.category == Category::Mining && item.code == code)
            {
                total = total.saturating_add(item.mass);
            }
        }
        total
    }

    /// `F386`'s free hold as F390/F391 leave it: `base` is the ship's delivered
    /// capacity (a new game's `Navigator::hold`, or the cargo statistic of a
    /// hull bought later) and `delivered` is the loadout already priced into it.
    /// Fitting subtracts the item's mass; removing gives it back.
    pub fn capacity(&self, base: i32, delivered: &Loadout, lasers: &[u8; 4]) -> i32 {
        let extra = i64::from(self.mass(lasers)) - i64::from(delivered.mass(lasers));
        (i64::from(base) - extra).clamp(0, i64::from(i32::MAX)) as i32
    }

    /// `ship+0xc8 & 1`: the laser cooling booster (`F938` doubles the cooling).
    pub fn cooling_booster(&self) -> bool {
        self.general & 1 != 0
    }

    /// `ship+0xc8` bit 7: the Auto targetter, which arms the blinking
    /// auto-targetter cursor and its crosshair acquisition.
    pub fn auto_targetter(&self) -> bool {
        self.general & AUTO_TARGETTER_BIT != 0
    }

    /// `ship+0xc8` bit 9: the Missile Viewer, needed for `F10`.
    pub fn missile_viewer(&self) -> bool {
        self.general & MISSILE_VIEWER_BIT != 0
    }

    /// `ship+0xc8` bit 11: the Chaff Dispenser (`F1054`'s `c` key).
    pub fn chaff_dispenser(&self) -> bool {
        self.general & CHAFF_BIT != 0
    }

    /// `F935`'s equipment test: Naval E.C.M. first, then the standard system.
    pub fn ecm(&self) -> Option<Ecm> {
        if self.general & NAVAL_ECM_BIT != 0 {
            Some(Ecm::Naval)
        } else if self.general & ECM_BIT != 0 {
            Some(Ecm::Standard)
        } else {
            None
        }
    }

    /// `ship+0xe2`: 0x40 per fitted shield generator (`F413` adds, `F415`
    /// subtracts).
    pub fn shield_max(&self) -> u16 {
        u16::from(self.shield_generators) * 0x40
    }

    /// The missile record in pylon `slot`, when the slot is occupied.
    pub fn missile(&self, slot: usize) -> Option<&'static Equipment> {
        self.missiles.get(slot).copied().and_then(missile_by_code)
    }

    /// `F1120`'s icon image for pylon `slot`: `slot_byte & 0xf`, or `None` when
    /// the pylon is empty. Console records 0x3b-0x44 list the ten bitmaps.
    pub fn missile_icon(&self, slot: usize) -> Option<u8> {
        let code = *self.missiles.get(slot)?;
        (code != 0).then_some(code & 0xf)
    }

    /// A loadout the save validator accepts: only real general bits, missile
    /// codes, drive/mining codes the table has, and consistent cabin counts.
    pub fn validate(&self) -> bool {
        self.general & !GENERAL_MASK == 0
            && self
                .missiles
                .iter()
                .all(|code| *code == 0 || is_missile_code(*code))
            && (self.drive == 0
                || equipment()
                    .iter()
                    .any(|item| item.category == Category::Drive && item.code == self.drive))
            && self.mining.iter().all(|code| {
                *code == 0
                    || equipment()
                        .iter()
                        .any(|item| item.category == Category::Mining && item.code == *code)
            })
            && self.shield_generators <= 10
            && self.cabins <= 100
            && self.occupied_cabins <= self.cabins
    }

    /// True when record `index` is a general record whose bit is set.
    pub fn has_general(&self, index: usize) -> bool {
        get(index).is_some_and(|item| {
            item.category == Category::General
                && self.general & (1 << ((item.code ^ 0x10) & 0x1f)) != 0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_the_original_66_records_and_names() {
        assert_eq!(equipment().len(), 66);
        // The name code is 0x9875 + index, which is DATA_005299[0x75 + index].
        assert_eq!(equipment()[0].name, "XB13 Dummy Prox. Mine");
        assert_eq!(equipment()[10].name, "Missile Viewer");
        assert_eq!(equipment()[16].name, "Navigation computer");
        assert_eq!(equipment()[21].name, "1MW Pulse Laser");
        assert_eq!(equipment()[27].name, "Shield Generator");
        assert_eq!(equipment()[65].name, "Thargoid Laser");
    }

    #[test]
    fn the_thargoid_laser_is_equipment_code_0xd5() {
        let item = get(65).unwrap();
        assert_eq!(item.name, "Thargoid Laser");
        assert_eq!(item.category, Category::Laser);
        assert_eq!(item.code, THARGOID_LASER);
        assert_eq!(item.code, 0xd5);
        assert_eq!(item.mass, 50);
        assert_eq!(item.tech_level, 255);
        assert!(is_laser_code(THARGOID_LASER));
        // The code is the shop/fitted byte; F774's AI table ends at 0xcf, a
        // different (Large Plasma Accelerator) record.
        assert!(is_laser_code(0xcf));
        assert_ne!(THARGOID_LASER, 0xcf);
    }

    #[test]
    fn records_decode_price_mass_category_and_tech() {
        // The station template 0x98f5's three numeric columns.
        let laser = get(21).unwrap();
        assert_eq!((laser.price, laser.removal, laser.mass), (90, 70, 1));
        assert_eq!(laser.category, Category::Laser);
        assert_eq!(laser.code, 0x88);
        assert_eq!(laser.tech_level, 78);
        assert!(laser.used);
        let shield = get(27).unwrap();
        assert_eq!((shield.price, shield.mass), (250, 4));
        assert_eq!(shield.category, Category::Shield);
        assert_eq!(shield.code, 0x00);
        assert_eq!(shield.tech_level, 156);
        let hyperdrive = get(58).unwrap();
        assert_eq!((hyperdrive.price, hyperdrive.mass), (96000, 600));
        assert_eq!(hyperdrive.category, Category::Drive);
        // The DOS release's byte: the JJFFE listing has 0xd7 here.
        assert_eq!(get(53).unwrap().tech_level, 0xeb);
    }

    #[test]
    fn the_general_mask_is_exactly_the_bits_the_table_uses() {
        let used = equipment()
            .iter()
            .filter(|item| item.category == Category::General)
            .fold(0u32, |mask, item| mask | (1 << ((item.code ^ 0x10) & 0x1f)));
        assert_eq!(used, GENERAL_MASK);
        // The default start's four bits are all real general bits.
        assert_eq!(Loadout::new_game().general & !GENERAL_MASK, 0);
    }

    #[test]
    fn the_default_start_is_f998s_type_0_saker_loadout() {
        let loadout = Loadout::new_game();
        assert_eq!(loadout.general, 0x4024_0020);
        assert_eq!(loadout.bytes(), [0x20, 0x00, 0x24, 0x40]);
        assert!(loadout.has_general(16), "Navigation computer (bit 5)");
        assert!(loadout.has_general(20), "Scanner (bit 18)");
        assert!(loadout.has_general(24), "Automatic Pilot (bit 21)");
        assert!(loadout.has_general(15), "Atmospheric Shielding (bit 30)");
        assert!(!loadout.has_general(17), "no Combat Computer (bit 6)");
        assert!(!loadout.has_general(22), "no Radar Mapper (bit 22)");
        assert_eq!(loadout.missiles, [0x82, 0x82, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(loadout.shield_generators, 0);
        assert_eq!(loadout.shield_max(), 0);
        assert_eq!(loadout.cabins, 0);
        assert_eq!(loadout.occupied_cabins, 0);
        assert_eq!(loadout.drive, 2, "F998's ship+0xd0 = 2");
        assert_eq!(loadout.drive_class(), 2);
        assert!(!loadout.military_drive());
        assert_eq!(loadout.mining_count(), 0);
        assert!(!loadout.cooling_booster());
        assert!(loadout.validate());
        // F390/F391's mass: the four general bits (Scanner, Autopilot,
        // Atmospheric Shielding; the navigation computer is 0 t), the two
        // missiles and the delivered Class 1 drive.
        assert_eq!(loadout.mass(&[0x88, 0, 0, 0]), 1 + 1 + 1 + 1 + 2 + 10);
        // The F998 hold letter is already the delivered loadout's capacity.
        assert_eq!(
            loadout.capacity(1, &Loadout::new_game(), &[0x88, 0, 0, 0]),
            1
        );
    }

    #[test]
    fn f393s_bit_formula_maps_the_documented_items() {
        let loadout = Loadout {
            general: (1 << 5) | (1 << 6) | (1 << 9) | (1 << 27),
            ..Loadout::default()
        };
        assert!(loadout.has_general(16), "Navigation computer is bit 5");
        assert!(loadout.has_general(17), "Combat Computer is bit 6");
        assert!(loadout.has_general(10), "Missile Viewer is bit 9");
        assert!(loadout.has_general(42), "Escape Capsule is bit 27");
        assert!(!loadout.has_general(39), "Naval ECM is bit 23");
        assert!(loadout.fitted(42, &[0; 4]));
        assert!(!loadout.fitted(27, &[0; 4]), "no shield generator fitted");
        assert!(!loadout.cooling_booster());
    }

    #[test]
    fn the_combat_equipment_bits_match_their_records() {
        // F393: (record[0x12] ^ 0x10) & 0x1f. Auto targetter 0x17 -> 7,
        // Missile Viewer 0x19 -> 9, Chaff Dispenser 0x1b -> 11, E.C.M. 0x03 ->
        // 19, Naval E.C.M. 0x07 -> 23.
        assert_eq!(AUTO_TARGETTER_BIT, 0x80);
        assert_eq!(MISSILE_VIEWER_BIT, 0x200);
        assert_eq!(CHAFF_BIT, 0x800);
        assert_eq!(ECM_BIT, 0x8_0000);
        assert_eq!(NAVAL_ECM_BIT, 0x80_0000);
        let loadout = Loadout {
            general: AUTO_TARGETTER_BIT | MISSILE_VIEWER_BIT | CHAFF_BIT,
            ..Loadout::default()
        };
        assert!(loadout.auto_targetter());
        assert!(loadout.missile_viewer());
        assert!(loadout.chaff_dispenser());
        assert_eq!(loadout.ecm(), None);
        // F935 tests the naval bit first and lights both DATA_008835 bits.
        let naval = Loadout {
            general: ECM_BIT | NAVAL_ECM_BIT,
            ..Loadout::default()
        };
        assert_eq!(naval.ecm(), Some(Ecm::Naval));
        let standard = Loadout {
            general: ECM_BIT,
            ..Loadout::default()
        };
        assert_eq!(standard.ecm(), Some(Ecm::Standard));
        // The new-game Eagle has none of the five.
        let new_game = Loadout::new_game();
        assert!(!new_game.auto_targetter());
        assert!(!new_game.missile_viewer());
        assert!(!new_game.chaff_dispenser());
        assert_eq!(new_game.ecm(), None);
    }

    #[test]
    fn f383_tech_multiplier_and_availability_follow_the_station() {
        assert_eq!(tech_multiplier(0), 9);
        assert_eq!(tech_multiplier(0x16), 9);
        assert_eq!(tech_multiplier(0x17), 10);
        assert_eq!(tech_multiplier(0xb2), 10);
        assert_eq!(tech_multiplier(0xb3), 11);
        assert_eq!(tech_multiplier(0xcb), 11);
        assert_eq!(tech_multiplier(0xcc), 12);

        let empty = Loadout::default();
        // Tech 0x16: only the tech <= 0x16 records. The Extra Passenger Cabin
        // (tech 19 = 0x13) is the only one.
        let low = outfitting(0x16, &empty, &[0; 4]);
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].index, 25);
        assert_eq!(low[0].price, 150 * 9);
        // A high-tech station offers everything; the 1MW pulse laser is 90*12.
        let all = outfitting(0xff, &empty, &[0; 4]);
        assert_eq!(all.len(), 66);
        assert_eq!(all[21].price, 90 * 12);
        assert_eq!(all[21].removal, 70 * 12);
        assert_eq!(all[21].mass, 1);
        assert!(!all[21].fitted);
        // A fitted laser is flagged through the mount words.
        let fitted = outfitting(0xff, &empty, &[0x88, 0, 0, 0]);
        assert!(fitted[21].fitted);
        assert!(!fitted[28].fitted);
    }

    #[test]
    fn fitted_uses_the_category_handler_storage() {
        let mut loadout = Loadout::new_game();
        // Default start: navigation computer (bit 5), scanner (18), autopilot
        // (21), atmospheric shielding (30), two 0x82 homing missiles, no shield
        // generator and no cabins.
        assert!(loadout.fitted(16, &[0; 4]), "Navigation computer");
        assert!(loadout.fitted(20, &[0; 4]), "Scanner");
        assert!(loadout.fitted(24, &[0; 4]), "Automatic Pilot");
        assert!(loadout.fitted(15, &[0; 4]), "Atmospheric Shielding");
        assert!(loadout.fitted(2, &[0; 4]), "KL760 Homing Missile");
        assert!(!loadout.fitted(10, &[0; 4]), "Missile Viewer");
        assert!(!loadout.fitted(25, &[0; 4]), "no passenger cabin");
        assert!(!loadout.fitted(27, &[0; 4]), "Shield Generator");
        assert_eq!(loadout.shield_max(), 0);
        loadout.shield_generators = 2;
        assert_eq!(loadout.shield_max(), 0x80);
        assert!(loadout.fitted(27, &[0; 4]));
        loadout.cabins = 1;
        assert!(loadout.fitted(25, &[0; 4]), "Extra Passenger Cabin");
        assert!(loadout.fitted(21, &[0x88, 0, 0, 0]), "front 1MW pulse");
        // F412: only the one drive byte's class is fitted.
        assert!(loadout.fitted(34, &[0; 4]), "Class 1 Hyperdrive (code 2)");
        assert!(!loadout.fitted(44, &[0; 4]), "Class 2 Hyperdrive (code 3)");
        loadout.drive = 3;
        assert!(loadout.fitted(44, &[0; 4]));
        // F427 returns the mining count for both records.
        assert!(!loadout.fitted(32, &[0; 4]), "no MB4 machine");
        assert!(!loadout.fitted(64, &[0; 4]), "no Vaccine machine");
        assert!(loadout.add_mining(0x11));
        assert_eq!(loadout.mining_count(), 1);
        assert!(loadout.fitted(32, &[0; 4]), "MB4 row sees the shared list");
        assert!(
            loadout.fitted(64, &[0; 4]),
            "Vaccine row sees the same list"
        );
        assert_eq!(loadout.remove_mining(), Some(0x11));
        assert_eq!(loadout.mining_count(), 0);
    }

    #[test]
    fn icon_images_are_the_missile_slot_low_nibble() {
        let loadout = Loadout::new_game();
        assert_eq!(loadout.missile_icon(0), Some(2));
        assert_eq!(loadout.missile_icon(1), Some(2));
        assert_eq!(loadout.missile_icon(2), None);
        assert_eq!(loadout.missile(0).unwrap().name, "KL760 Homing Missile");
        assert_eq!(loadout.missile(2), None);
    }

    #[test]
    fn validation_rejects_bits_and_codes_the_original_never_sets() {
        assert!(Loadout::default().validate());
        assert!(Loadout::new_game().validate());
        assert!(!Loadout {
            general: 1 << 14, // no record uses bit 14
            ..Loadout::default()
        }
        .validate());
        assert!(!Loadout {
            missiles: [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0], // not one of the ten codes
            ..Loadout::default()
        }
        .validate());
        assert!(!Loadout {
            shield_generators: 11,
            ..Loadout::default()
        }
        .validate());
        assert!(!Loadout {
            drive: 0x7f, // no drive record
            ..Loadout::default()
        }
        .validate());
        assert!(!Loadout {
            mining: [0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0], // no mining record
            ..Loadout::default()
        }
        .validate());
        assert!(!Loadout {
            cabins: 1,
            occupied_cabins: 2,
            ..Loadout::default()
        }
        .validate());
    }

    #[test]
    fn f423s_mining_list_is_ten_machines_and_removes_the_last() {
        let mut loadout = Loadout::default();
        for _ in 0..MINING_SLOTS {
            assert!(loadout.add_mining(0x11));
        }
        assert_eq!(loadout.mining_count(), MINING_SLOTS);
        // F423/F424: the eleventh record is refused ("Mining Machine list full").
        assert!(!loadout.add_mining(0x12));
        assert_eq!(loadout.mining_count(), MINING_SLOTS);
        // A Vaccine machine behind the MB4 machines: FUNC_000251 removes the
        // last list entry, not the row's code.
        loadout.mining[MINING_SLOTS - 1] = 0x12;
        assert_eq!(loadout.remove_mining(), Some(0x12));
        assert_eq!(loadout.mining_count(), MINING_SLOTS - 1);
        assert!(loadout.add_mining(0x11));
        assert_eq!(loadout.mining_count(), MINING_SLOTS);
        assert_eq!(
            loadout.mining_machines().collect::<Vec<_>>(),
            vec![0x11; MINING_SLOTS]
        );
    }

    #[test]
    fn mass_and_capacity_follow_f390_and_f391() {
        let mut loadout = Loadout::new_game();
        let lasers = [0x88, 0, 0, 0];
        let delivered = Loadout::new_game();
        assert_eq!(loadout.mass(&lasers), 16);
        // A new game's 1 t hold is the delivered loadout's capacity.
        assert_eq!(loadout.capacity(1, &delivered, &lasers), 1);
        // F390: a 4 t shield generator, a 5 t cabin and a 30 t mining machine.
        loadout.shield_generators = 1;
        loadout.cabins = 1;
        loadout.occupied_cabins = 1;
        assert_eq!(loadout.free_cabins(), 0);
        loadout.add_mining(0x12);
        assert_eq!(loadout.mass(&lasers), 16 + 4 + 5 + 20);
        assert_eq!(loadout.capacity(1, &delivered, &lasers), 0);
        // F391: removing restores exactly what fitting took.
        loadout.remove_mining();
        loadout.shield_generators = 0;
        loadout.cabins = 0;
        loadout.occupied_cabins = 0;
        assert_eq!(loadout.mass(&lasers), 16);
        assert_eq!(loadout.capacity(1, &delivered, &lasers), 1);
        // A hull bought later has no delivered equipment in its cargo word.
        let empty = Loadout::default();
        assert_eq!(empty.capacity(23, &Loadout::default(), &[0; 4]), 23);
        let mut fitted = empty;
        fitted.drive = 2;
        assert_eq!(fitted.capacity(23, &Loadout::default(), &[0; 4]), 13);
    }

    #[test]
    fn a_fitted_drive_is_one_class_and_counts_as_military_from_ten() {
        let mut loadout = Loadout::default();
        assert_eq!(loadout.drive_record(), None);
        assert!(!loadout.military_drive());
        loadout.drive = 2;
        assert_eq!(loadout.drive_record().unwrap().name, "Class 1 Hyperdrive");
        assert_eq!(loadout.drive_record().unwrap().mass, 10);
        assert!(!loadout.military_drive());
        loadout.drive = 0x0a;
        assert!(loadout.military_drive());
        assert_eq!(
            loadout.drive_record().unwrap().name,
            "Class 1 Military Drive"
        );
        assert_eq!(loadout.drive_record().unwrap().mass, 6);
    }
}
