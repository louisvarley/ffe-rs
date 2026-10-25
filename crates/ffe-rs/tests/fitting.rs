//! Equipment fitting beyond the general/laser paths: drives (F409/F411),
//! mining machines (F423-F427), cabins (F399/F401) and shield generators
//! (F413/F415), with F390/F391's mass and the energy-gauge effect.
//!
//! The scripted sequence is the evidence: it removes the Saker's delivered
//! Class 1 drive, fits a Class 2 drive, shows the F270 gauge scale change and
//! the F390 mass/hold move, then puts the Class 1 drive back. Local cache frames
//! for the Upgrades drive/mining/cabin rows are produced by
//! `tests/station_view.rs` (`Upgrades-drive.ppm`, `Upgrades-mining.ppm`,
//! `Upgrades-cabin.ppm`) and kept in `docs/evidence/fitting/`.

use ffe_core::equipment::{self, Loadout, MINING_SLOTS};
use ffe_rs::combat::Combat;
use ffe_rs::outfitting::{self, Limits};

const TECH: u16 = 0xfd;

/// A synthetic roomy hull, so the 25 t Class 2 drive fits: the Saker's real 1 t
/// hold (plus the 10 t its delivered drive frees) would make F386 refuse it, as
/// the source's space check does. F394's fuel-scoop mounting is clear here so the
/// scoop test can turn it on explicitly.
fn roomy() -> Limits {
    Limits {
        mounts: 2,
        pylons: 2,
        free_space: 1_000,
        fuel_scoop_mounting: false,
        drive_removable: true,
    }
}

fn record(name: &str) -> usize {
    equipment::equipment()
        .iter()
        .position(|item| item.name == name)
        .unwrap_or_else(|| panic!("the table has {name}"))
}

#[test]
fn scripted_fit_effect_remove_sequence() {
    let mut loadout = Loadout::new_game();
    let mut combat = Combat::new_game();
    let mut credits = 1_000_000u32;
    let delivered = Loadout::new_game();
    let class_one = record("Class 1 Hyperdrive");
    let class_two = record("Class 2 Hyperdrive");
    assert_eq!(class_one, 34);
    assert_eq!(class_two, 44);

    // F998 delivers the Saker with drive class 2 and 1 t of hold.
    assert_eq!(loadout.drive, 2);
    assert_eq!(loadout.mass(&combat.lasers), 16);
    assert_eq!(loadout.capacity(1, &delivered, &combat.lasers), 1);

    // F410/F411: remove the delivered drive; F410 clears the energy pool.
    combat.energy = 0x2000_0000;
    let message = outfitting::remove(
        class_one,
        TECH,
        &mut loadout,
        &mut combat,
        &mut credits,
        roomy(),
    )
    .unwrap();
    println!("remove Class 1 -> {message}");
    assert_eq!(loadout.drive, 0);
    assert_eq!(combat.drive, 0);
    assert_eq!(combat.energy, 0);
    assert_eq!(loadout.mass(&combat.lasers), 6);
    // F391 adds the delivered drive's 10 t back to DATA_008889.
    assert_eq!(loadout.capacity(1, &delivered, &combat.lasers), 11);

    // F409: fit the Class 2 drive. Its 25 t record takes the hold to 0 and the
    // F270 energy scale changes from class 2's 0x7ff to class 3's 0x3ff.
    let before_gauge = ffe_core::laser::energy_gauge(0x2000_0000, 2);
    let message = outfitting::fit(
        class_two,
        TECH,
        &mut loadout,
        &mut combat,
        &mut credits,
        roomy(),
    )
    .unwrap();
    println!("fit Class 2 -> {message}");
    assert_eq!(loadout.drive, 3);
    assert_eq!(combat.drive, 3);
    assert_eq!(loadout.mass(&combat.lasers), 31, "6 t + the 25 t drive");
    assert_eq!(loadout.capacity(1, &delivered, &combat.lasers), 0);
    let after_gauge = ffe_core::laser::energy_gauge(0x2000_0000, 3);
    assert_ne!(after_gauge, before_gauge);
    println!("effect -> drive 3, mass 31 t, hold 0, energy gauge {before_gauge} -> {after_gauge}");

    // F410 again, then F409 puts the Class 1 drive back: mass and hold return.
    let message = outfitting::remove(
        class_two,
        TECH,
        &mut loadout,
        &mut combat,
        &mut credits,
        roomy(),
    )
    .unwrap();
    println!("remove Class 2 -> {message}");
    let message = outfitting::fit(
        class_one,
        TECH,
        &mut loadout,
        &mut combat,
        &mut credits,
        roomy(),
    )
    .unwrap();
    println!("fit Class 1 -> {message}");
    assert_eq!(loadout.drive, 2);
    assert_eq!(combat.drive, 2);
    assert_eq!(loadout.mass(&combat.lasers), 16);
    assert_eq!(loadout.capacity(1, &delivered, &combat.lasers), 1);
}

#[test]
fn every_unmodelled_category_now_fits_and_removes() {
    let mut loadout = Loadout::default();
    let mut combat = Combat::default();
    let mut credits = 1_000_000;
    let delivered = Loadout::default();
    let lasers = [0u8; 4];

    // Cabin: 5 t, 18,000 tenths in, 12,000 out.
    outfitting::fit(25, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!((loadout.cabins, loadout.mass(&lasers)), (1, 5));
    assert_eq!(credits, 1_000_000 - 18_000);
    // Shield: 4 t each and 0x40 max shields 1,500 tenths in, 1,200 out.
    outfitting::fit(27, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!(loadout.shield_max(), 0x40);
    assert_eq!(loadout.mass(&lasers), 9);
    assert_eq!(credits, 1_000_000 - 18_000 - 30_000);
    // Mining: 30 t, 7,800 tenths in, 7,200 out.
    outfitting::fit(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!((loadout.mining_count(), loadout.mass(&lasers)), (1, 39));
    assert!(
        loadout.fitted(64, &lasers),
        "both mining rows read the list"
    );
    assert_eq!(credits, 1_000_000 - 18_000 - 30_000 - 78_000);
    assert_eq!(loadout.capacity(100, &delivered, &lasers), 100 - 39);

    let before = credits;
    outfitting::remove(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!(credits, before + 72_000);
    outfitting::remove(27, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    outfitting::remove(25, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!(loadout.mass(&lasers), 0);
    assert_eq!(loadout.capacity(100, &delivered, &lasers), 100);
}

#[test]
fn the_ten_machine_cap_and_occupied_cabin_rules_are_the_sources() {
    let mut loadout = Loadout::default();
    let mut combat = Combat::default();
    let mut credits = 100_000_000;
    for _ in 0..MINING_SLOTS {
        outfitting::fit(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    }
    assert_eq!(
        outfitting::fit(32, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
        "Mining Machine list full"
    );
    // The Vaccine row removes the last machine and refunds its 0 removal price.
    let before = credits;
    outfitting::remove(64, 0xff, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!(loadout.mining_count(), MINING_SLOTS - 1);
    assert_eq!(credits, before);

    outfitting::fit(25, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    loadout.occupied_cabins = loadout.cabins;
    assert_eq!(
        outfitting::remove(25, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap_err(),
        "You cannot remove an occupied cabin."
    );
    loadout.occupied_cabins = 0;
    outfitting::remove(25, TECH, &mut loadout, &mut combat, &mut credits, roomy()).unwrap();
    assert_eq!(loadout.cabins, 0);
}

#[test]
fn the_fuel_scoop_mounting_and_dependency_are_f394s() {
    let mut loadout = Loadout::default();
    let mut combat = Combat::default();
    let mut credits = 1_000_000;
    let mut no_mount = roomy();
    no_mount.fuel_scoop_mounting = false;
    assert_eq!(
        outfitting::fit(29, TECH, &mut loadout, &mut combat, &mut credits, no_mount).unwrap_err(),
        "Your ship does not have a fuel scoop mounting."
    );
    assert_eq!(
        outfitting::fit(30, TECH, &mut loadout, &mut combat, &mut credits, no_mount).unwrap_err(),
        "You need a fuel scoop to be fitted."
    );
    let mut mounted = roomy();
    mounted.fuel_scoop_mounting = true;
    outfitting::fit(29, TECH, &mut loadout, &mut combat, &mut credits, mounted).unwrap();
    outfitting::fit(30, TECH, &mut loadout, &mut combat, &mut credits, mounted).unwrap();
    assert!(loadout.fitted(30, &combat.lasers));
}
