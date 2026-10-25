//! F386's Upgrades row actions: Fit (left button) and Remove (right button).
//!
//! `F386` (`ffe.asm:28708`) checks the ship's cash and free hold against the
//! row's `$ to Fit`/`Wt` values, then dispatches the record's category through
//! the install table at `DATA_005293`; the remove button (`row+0x18 & 0x10`)
//! uses `DATA_005294`. The category handlers are F390/F393 (general bits),
//! F399/F400 (cabins), F428 (lasers), F413/F415 (shield generators) and
//! F418/F421 (missiles). The price and refund are `F387`/`F388`: ten times the
//! row's whole-credit price, because the player's cash is in tenths of a credit
//! (`trade::credits`: 1000 is `$100.0`).
//!
//! The port applies the general, cabin, laser, shield and missile handlers to
//! `Loadout`/`Combat`. Drives and mining machines have no port state, so their
//! rows are shown (F383 offers them) but their buttons report that the category
//! is not modelled instead of inventing a drive word.

use ffe_core::equipment::{self, Category, Loadout};

use crate::combat::Combat;

/// The ship limits F386's checks and the category handlers use: `ship+0xd1`
/// (gun mounts), the statistics' missile mounts, and the free hold
/// (`DATA_008889 - DATA_008891`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub mounts: usize,
    pub pylons: usize,
    pub free_space: i32,
}

impl Limits {
    /// The limits of a ship with no statistics (the tests' empty hull): four
    /// laser mounts and ten pylons, as the storage arrays allow.
    pub fn default_for_ship() -> Self {
        Self {
            mounts: 4,
            pylons: 10,
            free_space: 0,
        }
    }
}

fn general_bit(item: &equipment::Equipment) -> u32 {
    1 << ((item.code ^ 0x10) & 0x1f)
}

fn not_modelled(item: &equipment::Equipment) -> String {
    match item.category {
        Category::Drive => "Drive fitting is not modelled in this port.".to_owned(),
        Category::Mining => "Mining machine fitting is not modelled in this port.".to_owned(),
        _ => "That equipment is not modelled in this port.".to_owned(),
    }
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
        Category::Drive | Category::Mining => return Err(not_modelled(item)),
    }
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
) -> Result<String, String> {
    let item = equipment::get(index).ok_or_else(|| "No such equipment.".to_owned())?;
    let refund = equipment::removal_price(index, tech_level)
        .ok_or_else(|| "No such equipment.".to_owned())?
        .saturating_mul(10);
    let missing = || format!("You have no {} to remove.", item.name);
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
            loadout.cabins -= 1;
        }
        Category::Laser => {
            let mounts = combat.lasers.len();
            let slot = (0..mounts)
                .find(|slot| combat.lasers[*slot] == item.code)
                .ok_or_else(missing)?;
            combat.lasers[slot] = 0;
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
        Category::Drive | Category::Mining => return Err(not_modelled(item)),
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
        let message = remove(21, 0xfd, &mut loadout, &mut combat, &mut credits).unwrap();
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
        remove(27, 0xfd, &mut loadout, &mut combat, &mut credits).unwrap();
        assert_eq!(loadout.shield_generators, 0);
        // Extra Passenger Cabin (25): F399/F400 count cabins.
        fit(25, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.cabins, 1);
        // KL760 Homing Missile (2): F418 fills the highest empty pylon.
        fit(2, 0xfd, &mut loadout, &mut combat, &mut credits, limits()).unwrap();
        assert_eq!(loadout.missiles[1], 0x82);
        assert_eq!(loadout.missiles[0], 0);
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
            remove(21, 0xfd, &mut loadout, &mut combat, &mut credits).unwrap_err(),
            "You have no 1MW Pulse Laser to remove."
        );
        assert_eq!(credits, 0);
    }

    #[test]
    fn drives_and_mining_machines_are_reported_as_unmodelled() {
        let mut loadout = Loadout::default();
        let mut combat = Combat::default();
        let mut credits = 1_000_000_000;
        let drive = equipment::equipment()
            .iter()
            .position(|item| item.category == Category::Drive)
            .expect("the table has drives");
        assert!(fit(
            drive,
            0xff,
            &mut loadout,
            &mut combat,
            &mut credits,
            limits()
        )
        .unwrap_err()
        .starts_with("Drive fitting is not modelled"));
        let mining = equipment::equipment()
            .iter()
            .position(|item| item.category == Category::Mining)
            .expect("the table has mining machines");
        let mut roomy = limits();
        roomy.free_space = 10_000;
        assert!(
            fit(mining, 0xff, &mut loadout, &mut combat, &mut credits, roomy)
                .unwrap_err()
                .starts_with("Mining machine fitting is not modelled")
        );
    }
}
