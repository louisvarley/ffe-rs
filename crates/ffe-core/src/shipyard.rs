//! The station's New and Reconditioned Ships list (F437/F438, `ffe.asm:30441`)
//! and the hull stock F634/F635 generate (`ffe.asm:43543`).
//!
//! The list is the starport record's `+0x2e6` array of 13 `{model, seed}` words
//! (`+0x31e` count). F437 draws each row from the model's statistics block:
//!
//! ```text
//! Cost      = word[+0xa] * 1000
//! Pt.Exchg  = Cost - 950 * (drive_mass[ship+0xd0] + word[+0xa]
//!                           - drive_mass[word[+0x14] & 0xff])
//! Cap.      = word[+0x8]
//! name      = expand(0x4000 | word[+0xe]) through DATA_004682 (F1345, F1348)
//! ```
//!
//! Buying (F438/F442/F441) checks the cash after the part exchange, replaces
//! the bought hull in the list with the player's old model, clears every fitted
//! item (the screen's own hint: "a ship with no extra equipment"), sets the new
//! model, and charges ten times the whole-credit Pt.Exchg because the player's
//! cash is in tenths.
//!
//! The table is generated: `tools/gen-shipyard-table.py`. See
//! `docs/notes/station.md` for the source map and the reference frame.

use crate::npc::{Random, SHIP_TYPES};

pub use crate::shipyard_table::{Model, DRIVE_MASS, MODELS};

/// The original's hull list cap: `F634` compares the count with 0xd.
pub const MAX_HULLS: usize = 13;
/// The `+0x2e6` array F622 generates has room for 13 `{model, seed}` words
/// (`+0x31e`).
pub const STOCK_SLOTS: usize = 13;

/// The model table entry for a shipyard hull model.
pub fn model(index: u32) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.model == index)
}

/// F437's trade-in: `950 * (drive_mass[fitted] + word[+0xa] - drive_mass[required])`.
/// The port has no separate drive state, so the player's model's own required
/// drive is the fitted one (the new game's Eagle 0x17 therefore gives
/// `950 * 38 = 36100`, the reference frame's constant).
pub fn trade_in(player: u32, fitted_drive: Option<u8>) -> u32 {
    let Some(ship) = model(player) else {
        return 0;
    };
    let drive = fitted_drive.unwrap_or(ship.drive) as usize;
    let fitted = DRIVE_MASS.get(drive).copied().unwrap_or(0);
    let required = DRIVE_MASS.get(ship.drive as usize).copied().unwrap_or(0);
    let value = u64::from(fitted) + u64::from(ship.price_mass);
    let value = value.saturating_sub(u64::from(required));
    u32::try_from(value.saturating_mul(950)).unwrap_or(u32::MAX)
}

/// One F437 row: the bought hull, its Cost, the net price after part exchange
/// and the player's trade-in value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hull {
    pub model: u32,
    pub name: &'static str,
    pub price: u32,
    pub trade_in: u32,
    /// F437's `+0x8`: `price - trade_in`, the amount the buy charges.
    pub net: i64,
    pub cargo: u16,
    pub mass: u16,
    pub drive: u8,
    pub gun_mounts: i8,
    pub missiles: u16,
    /// The random word F635 leaves beside the model (`+0x2e8`).
    pub seed: u16,
}

/// F437's row for a hull model and the player's current ship.
pub fn row(model_index: u32, player: u32, fitted_drive: Option<u8>, seed: u16) -> Option<Hull> {
    let ship = model(model_index)?;
    let value = trade_in(player, fitted_drive);
    Some(Hull {
        model: ship.model,
        name: ship.name,
        price: ship.price,
        trade_in: value,
        net: i64::from(ship.price) - i64::from(value),
        cargo: u16::try_from(ship.cargo.max(0)).unwrap_or(0),
        mass: ship.mass,
        drive: ship.drive,
        gun_mounts: ship.gun_mounts,
        missiles: ship.missiles,
        seed,
    })
}

/// F635's type-list index: kinds 0 and 1 are the allegiance's military and
/// shipyard lists (`type + 2 * allegiance`), 0xd is the shuttle pair.
fn type_list(kind: u8, allegiance: u16) -> Option<&'static [u32]> {
    let index = match kind {
        0 | 1 => usize::from(kind) + 2 * usize::from(allegiance.min(4)),
        0x0d => 0x0d,
        _ => return None,
    };
    SHIP_TYPES.get(index).copied()
}

/// F635: one random model from `kind`'s list. The type-1 list is scoped by a
/// population below four (`count * population / 4`).
pub fn pick(rng: &mut Random, kind: u8, allegiance: u16, population: u32) -> Option<u32> {
    let list = type_list(kind, allegiance)?;
    if list.is_empty() {
        return None;
    }
    let mut count = list.len() as u32;
    if kind == 1 && population < 4 {
        count = count.saturating_mul(population) / 4;
    }
    if count == 0 {
        return None;
    }
    Some(list[rng.bounded(count) as usize])
}

/// F634: append one hull when `F1530() & 8` is clear and fewer than 13 are
/// already listed. The port's [`Random`] is F922's generator; one draw below 8
/// stands in for the low bit F634 reads.
fn append(rng: &mut Random, list: &mut Vec<(u32, u16)>, model: u32) {
    if rng.bounded(8) == 7 {
        return;
    }
    if list.len() >= MAX_HULLS {
        return;
    }
    list.push((model, (rng.bounded(0x1_0000)) as u16));
}

/// F622's initial stock for one starport: the system's population and the
/// record's shipyard bytes (`DATA_008993` = record+6, `DATA_008994` =
/// record+8), plus one call per four starports of the shuttle list.
pub fn stock(
    rng: &mut Random,
    allegiance: u16,
    population: u32,
    shipyard_military: u32,
    shipyard_civil: u32,
    starports: u32,
) -> Vec<(u32, u16)> {
    let mut list = Vec::new();
    let shuttles = starports.saturating_sub(1) / 4 + 1;
    for _ in 0..shuttles {
        if let Some(model) = pick(rng, 0x0d, allegiance, population) {
            append(rng, &mut list, model);
        }
    }
    for _ in 0..shipyard_military.saturating_mul(3) + 1 {
        if let Some(model) = pick(rng, 0, allegiance, population) {
            append(rng, &mut list, model);
        }
    }
    for _ in 0..shipyard_civil.saturating_mul(3) + 1 {
        if let Some(model) = pick(rng, 1, allegiance, population) {
            append(rng, &mut list, model);
        }
    }
    list
}

/// F633's daily half for the hull stock: retire one hull when the draw says so,
/// then the three chance additions (`random(9) < record+6`, `< record+8`, and
/// `random(0x1b) < starports`).
pub fn daily(
    rng: &mut Random,
    list: &mut Vec<(u32, u16)>,
    allegiance: u16,
    population: u32,
    shipyard_military: u32,
    shipyard_civil: u32,
    starports: u32,
) {
    let retire = rng.bounded(0xe) as usize;
    if retire < list.len() {
        list.remove(0);
    }
    for (kind, chance) in [(0u8, shipyard_military), (1, shipyard_civil)] {
        if rng.bounded(9) < chance {
            if let Some(model) = pick(rng, kind, allegiance, population) {
                append(rng, list, model);
            }
        }
    }
    if rng.bounded(0x1b) < starports {
        if let Some(model) = pick(rng, 0x0d, allegiance, population) {
            append(rng, list, model);
        }
    }
}

/// What a buy changes. F442/F441 clears the fitted items, installs the new
/// model and subtracts ten times the whole-credit net price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purchase {
    pub model: u32,
    pub cash_tenths: i64,
    pub old_model: u32,
}

/// F442's buy of `hull` for the player's `old_model` with `cash_tenths`:
/// `None` when the part exchange would overdraw the account (F440 restores the
/// cash and reports it). The caller applies the ship/loadout reset.
pub fn buy(hull: &Hull, old_model: u32, cash_tenths: i64) -> Option<Purchase> {
    let charge = hull.net.checked_mul(10)?;
    let remaining = cash_tenths.checked_sub(charge)?;
    if remaining < 0 {
        return None;
    }
    Some(Purchase {
        model: hull.model,
        cash_tenths: remaining,
        old_model,
    })
}

/// F442's list update: the bought row's model is replaced by the player's old
/// hull (the original also stores the old value in `+0x2e8`).
pub fn part_exchange(list: &mut [(u32, u16)], row: usize, old_model: u32) {
    if let Some(entry) = list.get_mut(row) {
        entry.0 = old_model;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_frames_ships_have_their_table_prices() {
        // docs/evidence/station/ships.png at Sirocco (Federal shipyard).
        for (index, name, price, cargo) in [
            (0x2b, "Spar attack fighter", 71_000, 37),
            (0x15, "Hawk airfighter", 48_000, 7),
            (0x27, "Constrictor", 143_000, 90),
            (0x36, "Lion Transport", 348_000, 235),
            (0x26, "Cobra Mk III", 124_000, 80),
            (0x3c, "Puma Clipper", 1_265_000, 825),
            (0x24, "Cobra Mk I", 97_000, 60),
            (0x37, "Tiger Trader", 472_000, 320),
        ] {
            let ship = model(index).expect("the reference hull");
            assert_eq!(ship.name, name);
            assert_eq!(ship.price, price);
            assert_eq!(ship.cargo, cargo);
        }
    }

    #[test]
    fn the_new_games_eagle_is_worth_36100_in_part_exchange() {
        // F437 over the new game's Eagle (model 0x17): drive class 2, +0xa 38.
        assert_eq!(trade_in(0x17, None), 36_100);
        let hull = row(0x2b, 0x17, None, 0).unwrap();
        assert_eq!(hull.price, 71_000);
        assert_eq!(hull.net, 34_900);
        assert_eq!(hull.trade_in, 36_100);
        // A Cobra Mk III costs 124000, so 124000 - 36100 = 87900.
        assert_eq!(row(0x26, 0x17, None, 0).unwrap().net, 87_900);
    }

    #[test]
    fn the_buy_charges_ten_times_the_net_credit_price() {
        let hull = row(0x2b, 0x17, None, 0).unwrap();
        assert!(buy(&hull, 0x17, 349_000).is_some());
        let purchase = buy(&hull, 0x17, 400_000).unwrap();
        assert_eq!(purchase.model, 0x2b);
        assert_eq!(purchase.cash_tenths, 400_000 - 349_000);
        assert!(buy(&hull, 0x17, 348_999).is_none());
        let mut list = [(0x2b, 7), (0x26, 8)];
        part_exchange(&mut list, 0, 0x17);
        assert_eq!(list[0].0, 0x17);
        assert_eq!(list[1].0, 0x26);
    }

    #[test]
    fn the_stock_uses_the_allegiance_lists_and_the_thirteen_hull_cap() {
        // Ross 154 is Federal: kinds 0/1 are DATA_006180 (military) and
        // DATA_006181 (shipyard). Four hundred draws always fill the cap.
        let mut rng = Random::new(0x1234, 0x5678);
        let list = stock(&mut rng, 2, 9, 0, 9, 1);
        assert_eq!(list.len(), MAX_HULLS);
        let federal = |kind: u8| type_list(kind, 2).unwrap();
        let shuttle = type_list(0x0d, 2).unwrap();
        for (model, _) in &list {
            assert!(
                federal(0).contains(model) || federal(1).contains(model) || shuttle.contains(model),
                "{model:#x} is in a Federal list or the shuttle list"
            );
        }
        // The shuttle list is DATA_006193 (0xf/0x12) and the model picker only
        // returns entries from the named list.
        for kind in [0u8, 1, 0x0d] {
            let mut rng = Random::new(1, 2);
            for _ in 0..50 {
                let model = pick(&mut rng, kind, 2, 9).unwrap();
                assert!(type_list(kind, 2).unwrap().contains(&model));
            }
        }
    }

    #[test]
    fn the_daily_update_retires_one_hull_and_respects_the_shipyard_chances() {
        let mut rng = Random::new(9, 10);
        let mut list = stock(&mut rng, 2, 9, 0, 9, 1);
        let before = list.len();
        // record+6 = 0 (never adds military), record+8 = 9 (always adds a
        // shipyard hull when there is room), one starport.
        daily(&mut rng, &mut list, 2, 9, 0, 9, 1);
        assert!(list.len() <= before);
        assert!(!list.is_empty());
    }

    #[test]
    fn a_low_population_narrows_the_type_one_list() {
        // F635's `count * population / 4` for type 1 below population 4.
        let full = type_list(1, 2).unwrap().len();
        let mut rng = Random::new(3, 4);
        for _ in 0..100 {
            let model = pick(&mut rng, 1, 2, 1).unwrap();
            let first_quarter = &type_list(1, 2).unwrap()[..full / 4];
            assert!(first_quarter.contains(&model));
        }
    }
}
