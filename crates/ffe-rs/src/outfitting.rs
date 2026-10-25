//! F386's Upgrades row actions: Fit (left button) and Remove (right button).
//!
//! `F386` (`ffe.asm:28708`) checks the ship's cash and free hold against the
//! row's `$ to Fit`/`Wt` values, then dispatches the record's category through
//! the install table at `DATA_005293`; the remove button (`row+0x18 & 0x10`)
//! uses `DATA_005294`. The category handlers are F390/F393 and F391/F395
//! (general bits), F398/F400 (cabins), F403/F405 (lasers; F404/F406 add F428's
//! mounting dialog), F408/F410 (the one drive), F413/F415 (shield generators),
//! F418/F420 (missiles) and F423/F425 (mining machines). The price and refund
//! are `F387`/`F388`: ten times the row's whole-credit price, because the
//! player's cash is in tenths of a credit (`trade::credits`: 1000 is `$100.0`).
//!
//! The port applies every category handler to `Loadout`/`Combat`:
//! F393/F395 (general bits), F398/F400 (cabins), F403/F405 (lasers),
//! F408/F410 (the one drive), F413/F415 (shield generators), F418/F420
//! (missiles) and F423/F425 (mining machines). F410 also clears the energy
//! pool (`ship+0x11e`), and every successful Fit/Remove updates the combat
//! copy of the pylon words and the drive byte so the HUD/laser energy gauge and
//! the missile launch see the same ship the Upgrades screen does.

use ffe_core::equipment::{self, Category, Loadout, MINING_SLOTS};

use crate::combat::Combat;

/// The ship limits F386's checks and the category handlers use: `ship+0xd1`
/// (gun mounts), the statistics' missile mounts, the free hold
/// (`DATA_008889 - DATA_008891`), F394's fuel-scoop mounting bit and the
/// statistics' drive word (a negative word makes the drive part of the hull).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub mounts: usize,
    pub pylons: usize,
    pub free_space: i32,
    /// `word[statistics+4] & 0x100` (`statistics+5` bit 0): F394's
    /// "Your ship does not have a fuel scoop mounting." test.
    pub fuel_scoop_mounting: bool,
    /// `statistics+0x14 >= 0`: F410/F411's removable drive. A negative word
    /// means the drive is an integral part of the hull.
    pub drive_removable: bool,
}

impl Limits {
    /// The limits of a ship with no statistics (the tests' empty hull): four
    /// laser mounts and ten pylons, as the storage arrays allow, with no
    /// fuel-scoop or integral-drive restriction.
    pub fn default_for_ship() -> Self {
        Self {
            mounts: 4,
            pylons: 10,
            free_space: 0,
            fuel_scoop_mounting: true,
            drive_removable: true,
        }
    }
}

fn general_bit(item: &equipment::Equipment) -> u32 {
    1 << ((item.code ^ 0x10) & 0x1f)
}

fn sync_combat(loadout: &Loadout, combat: &mut Combat) {
    combat.drive = loadout.drive;
    combat.missiles = loadout.missiles;
}

/// F386's left button: fit equipment `index` at a starport whose tech level is
/// `tech_level`. On success `credits` loses `fit_price * 10`; failures leave the
/// loadout, the lasers and the cash untouched.
pub fn fit(
    index: usize,
    tech_level: u16,
    loadout: &mut Loadout,
    combat: &mut Combat,
    credits: &mut u32,
    limits: Limits,
) -> Result<String, String> {
    let item = equipment::get(index).ok_or_else(|| "No such equipment.".to_owned())?;
    let price =
        equipment::fit_price(index, tech_level).ok_or_else(|| "No such equipment.".to_owned())?;
    // F387 subtracts ten times the row's price (the cash unit is a tenth).
    let cost = price.saturating_mul(10);
    if *credits < cost {
        return Err(format!("You cannot afford {}.", item.name));
    }
    if item.mass > 0 && i64::from(item.mass) > i64::from(limits.free_space) {
        return Err(format!(
            "You do not have enough space in your ship for a {}.",
            item.name
        ));
    }
    match item.category {
        Category::General => {
            let bit = general_bit(item);
            if loadout.general & bit != 0 {
                return Err(format!("{} already fitted.", item.name));
            }
            // F394's first branch: code 4 is the Fuel Scoop itself, and only a
            // model with the statistics' +5 bit 0 has the mounting.
            if item.code == 0x04 && !limits.fuel_scoop_mounting {
                return Err("Your ship does not have a fuel scoop mounting.".to_owned());
            }
            // F394's code 0x0d branch: Cargo Scoop Conversion needs the Fuel
            // Scoop bit (`ship+0xca & 0x10`, general bit 20).
            if item.code == 0x0d {
                let scoop = equipment::equipment()
                    .iter()
                    .find(|other| other.category == Category::General && other.code == 0x04)
                    .map_or(0, general_bit);
                if loadout.general & scoop == 0 {
                    return Err("You need a fuel scoop to be fitted.".to_owned());
                }
            }
            loadout.general |= bit;
        }
        Category::Cabin => loadout.cabins = loadout.cabins.saturating_add(1),
        Category::Laser => {
            let mounts = limits.mounts.min(combat.lasers.len());
            if mounts == 0 {
                return Err("Your ship does not have a gun mounting.".to_owned());
            }
            let slot = (0..mounts)
                .find(|slot| combat.lasers[*slot] == 0)
                .ok_or_else(|| "You already have a weapon on every gun mounting.".to_owned())?;
            combat.lasers[slot] = item.code;
        }
        Category::Drive => {
            // F408/F409: one drive byte; a second drive is refused.
            if loadout.drive != 0 {
                return Err("You may have only one drive system.".to_owned());
            }
            loadout.drive = item.code;
        }
        Category::Shield => {
            loadout.shield_generators = loadout.shield_generators.saturating_add(1);
        }
        Category::Missile => {
            let pylons = limits.pylons.min(loadout.missiles.len());
            let slot = (0..pylons)
                .rev()
                .find(|slot| loadout.missiles[*slot] == 0)
                .ok_or_else(|| "You have no empty missile positions.".to_owned())?;
            loadout.missiles[slot] = item.code;
        }
        Category::Mining => {
            // F423/F424: ten machine records, then "Mining Machine list full".
            if loadout.mining_count() >= MINING_SLOTS || !loadout.add_mining(item.code) {
                return Err("Mining Machine list full".to_owned());
            }
        }
    }
    sync_combat(loadout, combat);
    *credits -= cost;
    Ok(format!("Fitting {}.", item.name))
}

/// F386's right button: remove equipment `index`, refunding `removal_price *
/// 10`. Fails without changing anything when the ship does not carry it.
pub fn remove(
    index: usize,
    tech_level: u16,
    loadout: &mut Loadout,
    combat: &mut Combat,
    credits: &mut u32,
    limits: Limits,
) -> Result<String, String> {
    let item = equipment::get(index).ok_or_else(|| "No such equipment.".to_owned())?;
    let refund = equipment::removal_price(index, tech_level)
        .ok_or_else(|| "No such equipment.".to_owned())?
        .saturating_mul(10);
    let missing = || format!("You have no {} to remove.", item.name);
    let mut removed_drive = false;
    match item.category {
        Category::General => {
            let bit = general_bit(item);
            if loadout.general & bit == 0 {
                return Err(missing());
            }
            loadout.general &= !bit;
        }
        Category::Cabin => {
            if loadout.cabins == 0 {
                return Err(missing());
            }
            // F401 also refuses while every cabin has a passenger in it.
            if loadout.occupied_cabins >= loadout.cabins {
                return Err("You cannot remove an occupied cabin.".to_owned());
            }
            loadout.cabins -= 1;
        }
        Category::Laser => {
            let mounts = combat.lasers.len();
            let slot = (0..mounts)
                .find(|slot| combat.lasers[*slot] == item.code)
                .ok_or_else(missing)?;
            combat.lasers[slot] = 0;
        }
        Category::Drive => {
            if loadout.drive != item.code {
                return Err(missing());
            }
            // F410/F411: a negative statistics drive word marks the drive as
            // part of the hull.
            if !limits.drive_removable {
                return Err("The drive is an integral part of your ship.".to_owned());
            }
            loadout.drive = 0;
            removed_drive = true;
        }
        Category::Shield => {
            if loadout.shield_generators == 0 {
                return Err(missing());
            }
            loadout.shield_generators -= 1;
        }
        Category::Missile => {
            let pylons = loadout.missiles.len();
            let slot = (0..pylons)
                .rev()
                .find(|slot| loadout.missiles[*slot] == item.code)
                .ok_or_else(missing)?;
            loadout.missiles[slot] = 0;
        }
        Category::Mining => {
            // F426/F425: the row's code is not compared; the last machine in
            // DATA_009104's list is removed.
            if loadout.remove_mining().is_none() {
                return Err(missing());
            }
        }
    }
    sync_combat(loadout, combat);
    if removed_drive {
        // F410 clears `ship+0x11e`, the energy pool.
        combat.energy = 0;
    }
    *credits = credits.saturating_add(refund);
    Ok(format!("Removing {}.", item.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            mounts: 1,
            pylons: 2,
            free_space: 10,
            fuel_scoop_mounting: true,
            drive_removable: true,
        }
    }

    fn roomy() -> Limits {
        Limits {
            free_space: 10_000,
            ..limits()
        }
    }

    #[test]
    fn f387_charges_ten_times_the_whole_credit_fit_price() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 20_000;
        // 1MW Pulse Laser, record 21: 90 * 12 = 1080 whole credits.
        let message = fit(21, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(message, "Fitting 1MW Pulse Laser.");
        assert_eq!(combat.lasers[0], 0x88);
        assert_eq!(credits, 20_000 - 10_800);
    }

    #[test]
    fn f388_refunds_ten_times_the_whole_credit_removal_price() {
        let mut loadout = Loadout::default();
        let mut combat = Combat {
            lasers: [0x88, 0, 0, 0],
            ..Combat::default()
        };
        let mut credits = 5_000;
        // Removal price 70 * 12 = 840 whole credits.
        let message = remove(21, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(message, "Removing 1MW Pulse Laser.");
        assert_eq!(combat.lasers, [0; 4]);
        assert_eq!(credits, 5_000 + 8_400);
    }

    #[test]
    fn general_cabins_shields_and_missiles_use_the_category_handlers() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        // Scanner (record 20) is a general bit; F390/F393 sets it once.
        fit(20, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert!(loadout.fitted(20, &combat.lasers));
        assert_eq!(
            fit(20, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap_err(),
            "Scanner already fitted."
        );
        // Shield Generator (27): F413 adds a generator, F415 removes it.
        fit(27, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.shield_generators, 1);
        remove(27, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.shield_generators, 0);
        // Extra Passenger Cabin (25): F399/F400 count cabins.
        fit(25, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.cabins, 1);
        // KL760 Homing Missile (2): F418 fills the highest empty pylon, and the
        // combat copy the missile launch reads follows it.
        fit(2, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.missiles[1], 0x82);
        assert_eq!(loadout.missiles[0], 0);
        assert_eq!(combat.missiles, loadout.missiles);
    }

    #[test]
    fn f394_requires_a_fuel_scoop_before_cargo_scoop_conversion() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        // Cargo Scoop Conversion (record 30, code 0x0d).
        assert_eq!(
            fit(30, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap_err(),
            "You need a fuel scoop to be fitted."
        );
        // Fuel Scoop (record 29, code 0x04) sets general bit 20 first.
        fit(29, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        fit(30, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert!(loadout.fitted(30, &combat.lasers));
    }

    #[test]
    fn cash_and_hold_failures_leave_the_ship_alone() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 100;
        assert_eq!(
            fit(21, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap_err(),
            "You cannot afford 1MW Pulse Laser."
        );
        assert_eq!(credits, 100);
        assert_eq!(combat.lasers, [0; 4]);
        credits = 1_000_000;
        let mut no_room = limits();
        no_room.free_space = 0;
        assert_eq!(
            fit(21, 0xfd, &mut loadout, &mut combat, &mut credits, no_room).unwrap_err(),
            "You do not have enough space in your ship for a 1MW Pulse Laser."
        );
        assert_eq!(combat.lasers, [0; 4]);
    }

    #[test]
    fn laser_mounts_and_pylons_are_limited_by_the_ship() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        let mut one = limits();
        one.mounts = 0;
        assert_eq!(
            fit(21, 0xfd, &mut loadout, &mut combat, &mut credits, one).unwrap_err(),
            "Your ship does not have a gun mounting."
        );
        let mut full = limits();
        full.pylons = 1;
        loadout.missiles[0] = 0x82;
        assert_eq!(
            fit(2, 0xfd, &mut loadout, &mut combat, &mut credits, full).unwrap_err(),
            "You have no empty missile positions."
        );
    }

    #[test]
    fn removing_something_not_fitted_reports_the_original_message() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 0;
        assert_eq!(
            remove(21, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap_err(),
            "You have no 1MW Pulse Laser to remove."
        );
        assert_eq!(credits, 0);
    }

    #[test]
    fn f409_fits_one_drive_and_f410_removes_it() {
        let mut loadout = Loadout::default();
        let mut combat = Combat {
            energy: 0x2000_0000,
            ..Combat::default()
        };
        let mut credits = 1_000_000;
        let no_drive = Combat::default().energy_gauge();
        // Class 1 Hyperdrive (record 34, code 2): 800 * 12 = 9600 credits.
        let message = fit(34, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(message, "Fitting Class 1 Hyperdrive.");
        assert_eq!(loadout.drive, 2);
        assert_eq!(combat.drive, 2, "F270's energy gauge reads the drive byte");
        assert!(loadout.fitted(34, &combat.lasers));
        assert!(!loadout.fitted(44, &combat.lasers), "no Class 2 drive");
        assert_eq!(loadout.mass(&combat.lasers), 10);
        assert_eq!(
            combat.energy_gauge(),
            ffe_core::laser::energy_gauge(0x2000_0000, 2)
        );
        assert_ne!(combat.energy_gauge(), no_drive);
        assert_eq!(credits, 1_000_000 - 96_000);
        // F409: only one drive system.
        assert_eq!(
            fit(44, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
            "You may have only one drive system."
        );
        // Removing the wrong class is "no ... to remove".
        assert_eq!(
            remove(44, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
            "You have no Class 2 Hyperdrive to remove."
        );
        // F410: the drive goes and the energy pool with it; 750 * 12 refund.
        let message = remove(34, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(message, "Removing Class 1 Hyperdrive.");
        assert_eq!(loadout.drive, 0);
        assert_eq!(combat.drive, 0);
        assert_eq!(combat.energy, 0);
        assert_eq!(combat.energy_gauge(), 0);
        assert_eq!(credits, 1_000_000 - 96_000 + 90_000);
        assert_eq!(loadout.mass(&combat.lasers), 0);
    }

    #[test]
    fn f410_refuses_an_integral_drive() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        fit(34, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        let mut integral = roomy();
        integral.drive_removable = false;
        assert_eq!(
            remove(34, 0xfd, &mut loadout, &mut combat, &mut credits, integral).unwrap_err(),
            "The drive is an integral part of your ship."
        );
        assert_eq!(loadout.drive, 2);
        assert_eq!(combat.drive, 2);
        assert_eq!(credits, 1_000_000 - 96_000);
    }

    #[test]
    fn f423_fits_ten_mining_machines_and_f426_removes_the_last() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 100_000_000;
        // MB4 Mining Machine (record 32, code 0x11, 30 t, 650 * 12 = 7800).
        for _ in 0..ffe_core::equipment::MINING_SLOTS {
            fit(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        }
        assert_eq!(loadout.mining_count(), 10);
        assert_eq!(loadout.mass(&combat.lasers), 300);
        assert!(loadout.fitted(32, &combat.lasers), "MB4 row");
        assert!(
            loadout.fitted(64, &combat.lasers),
            "the shared list flags both"
        );
        // F423/F424: the eleventh machine is refused.
        assert_eq!(
            fit(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
            "Mining Machine list full"
        );
        assert_eq!(loadout.mining_count(), 10);
        // F426 does not compare the row's code: the Vaccine row removes the last
        // (MB4) machine and refunds the Vaccine record's 0 removal price.
        let before = credits;
        let message = remove(64, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(message, "Removing Vaccine Mining Machine.");
        assert_eq!(loadout.mining_count(), 9);
        assert_eq!(credits, before, "the Vaccine record's removal is 0");
        // The MB4 row's own removal price is 600 * 12 = 7200 whole credits.
        let before = credits;
        remove(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(credits, before + 72_000);
        assert_eq!(loadout.mining_count(), 8);
    }

    #[test]
    fn f399_counts_cabins_and_f401_refuses_an_occupied_cabin() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        // Extra Passenger Cabin (record 25): 150 * 12 = 1800 credits, 5 t.
        fit(25, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.cabins, 1);
        assert_eq!(loadout.cabin_capacity(), 1);
        assert_eq!(loadout.free_cabins(), 1);
        assert_eq!(loadout.mass(&combat.lasers), 5);
        assert!(loadout.fitted(25, &combat.lasers));
        // F401: every cabin occupied -> "You cannot remove an occupied cabin."
        loadout.occupied_cabins = 1;
        assert_eq!(loadout.free_cabins(), 0);
        assert_eq!(
            remove(25, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
            "You cannot remove an occupied cabin."
        );
        assert_eq!(loadout.cabins, 1);
        loadout.occupied_cabins = 0;
        let before = credits;
        remove(25, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.cabins, 0);
        assert_eq!(loadout.mass(&combat.lasers), 0);
        assert_eq!(credits, before + 12_000, "100 * 12 * 10");
    }

    #[test]
    fn f413_adds_0x40_shields_per_generator_and_f415_takes_them_away() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        fit(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.shield_generators, 1);
        assert_eq!(loadout.shield_max(), 0x40);
        fit(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.shield_max(), 0x80);
        assert_eq!(loadout.mass(&combat.lasers), 8);
        assert!(loadout.fitted(27, &combat.lasers));
        remove(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.shield_max(), 0x40);
        remove(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.shield_max(), 0);
        assert!(!loadout.fitted(27, &combat.lasers));
        assert_eq!(loadout.mass(&combat.lasers), 0);
        assert_eq!(
            remove(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
            "You have no Shield Generator to remove."
        );
    }

    #[test]
    fn f394_checks_the_fuel_scoop_mounting_and_the_dependency() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000;
        let mut no_mount = roomy();
        no_mount.fuel_scoop_mounting = false;
        // F394's code-4 branch: the model has no mounting (the Saker Mk III's
        // statistics +5 bit 0 is clear), so the Fuel Scoop cannot be fitted.
        assert_eq!(
            fit(29, 0xfd, &mut loadout, &mut combat, &mut credits, no_mount).unwrap_err(),
            "Your ship does not have a fuel scoop mounting."
        );
        assert_eq!(
            fit(30, 0xfd, &mut loadout, &mut combat, &mut credits, no_mount).unwrap_err(),
            "You need a fuel scoop to be fitted."
        );
        // A model with the mounting can take the scoop, then the conversion.
        fit(29, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        fit(30, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert!(loadout.fitted(30, &combat.lasers));
    }

    #[test]
    fn mass_and_capacity_move_with_each_fit_and_remove() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 10_000_000;
        let delivered = Loadout::default();
        let lasers = [0u8; 4];
        assert_eq!(loadout.capacity(23, &delivered, &lasers), 23);
        fit(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.capacity(23, &delivered, &lasers), 19, "4 t shield");
        fit(25, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.capacity(23, &delivered, &lasers), 14, "5 t cabin");
        fit(34, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(
            loadout.capacity(23, &delivered, &lasers),
            4,
            "10 t Class 1 drive"
        );
        remove(34, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        remove(25, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        remove(27, 0xfd, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
        assert_eq!(loadout.capacity(23, &delivered, &lasers), 23);
    }
}
