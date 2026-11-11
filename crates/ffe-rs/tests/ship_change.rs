//! After a ship purchase: the hull's own drive (F1533/F441), the hold net of
//! it, and fitting or selling a drive on the new hull.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test ship_change --
//! --ignored --nocapture`

use ffe_core::equipment::{self, Loadout};
use ffe_rs::combat::Combat;
use ffe_rs::flight::Flight;
use ffe_rs::navigation::Navigator;
use ffe_rs::outfitting::{self, Limits};
use ffe_rs::trade;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn record(name: &str) -> usize {
    equipment::equipment()
        .iter()
        .position(|item| item.name == name)
        .unwrap()
}

/// The player in `ship` as `buy_ship` leaves them: the hull's own drive, no
/// start-pad state.
fn bought(cache: &ffe_content::VerifiedCache, nav: &mut Navigator, ship: u32) -> Flight {
    nav.new_game.as_mut().unwrap().bought = true;
    let (drive, _) = nav.hull_drive(cache, ship);
    let mut state = Flight {
        ship,
        credits: 100_000_000,
        loadout: Loadout {
            drive,
            ..Loadout::default()
        },
        combat: Combat::default(),
        ..Flight::default()
    };
    state.combat.drive = drive;
    state
}

fn limits(free: i32, removable: bool) -> Limits {
    Limits {
        mounts: 2,
        pylons: 2,
        free_space: free,
        fuel_scoop_mounting: false,
        drive_removable: removable,
        mount: None,
        price_multiplier: None,
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_bought_hull_keeps_its_drive_and_a_removable_one_can_be_swapped() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    // Model 25 (Eagle Mk3): a removable Class 1 hyperdrive, 22 t of cargo.
    assert!(nav.on_pad(), "the new game starts on the pad");
    let mut state = bought(&cache, &mut nav, 25);
    // F4 still opens the services: the start pad is not forgotten.
    assert!(nav.on_pad());
    assert_eq!(nav.hull_drive(&cache, 25), (2, false));
    assert_eq!(state.loadout.drive, 2);
    // The delivered drive's 10 t is out of the 22 t hold.
    assert_eq!(trade::capacity(&nav, &cache, &state), 12);

    let class_one = record("Class 1 Hyperdrive");
    // A second drive is refused while one is fitted.
    let free = trade::capacity(&nav, &cache, &state);
    let refused = outfitting::fit(
        class_one,
        0xfd,
        &mut state.loadout,
        &mut state.combat,
        &mut state.credits,
        limits(free, true),
    );
    assert_eq!(
        refused,
        Err("You may have only one drive system.".to_owned())
    );

    // Selling the drive gives its 10 t back; then a Class 1 fits again.
    outfitting::remove(
        class_one,
        0xfd,
        &mut state.loadout,
        &mut state.combat,
        &mut state.credits,
        limits(0, true),
    )
    .unwrap();
    assert_eq!((state.loadout.drive, state.combat.drive), (0, 0));
    assert_eq!(trade::capacity(&nav, &cache, &state), 22);
    let free = trade::capacity(&nav, &cache, &state);
    outfitting::fit(
        class_one,
        0xfd,
        &mut state.loadout,
        &mut state.combat,
        &mut state.credits,
        limits(free, true),
    )
    .unwrap();
    assert_eq!(state.loadout.drive, 2);
    assert_eq!(trade::capacity(&nav, &cache, &state), 12);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn an_integral_drive_is_already_out_of_the_hold() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    // Model 46 (Turner Class): a negative drive word, class 0x0d built in.
    let state = bought(&cache, &mut nav, 46);
    assert_eq!(nav.hull_drive(&cache, 46), (0x0d, true));
    assert_eq!(state.loadout.drive, 0x0d);
    assert_eq!(trade::capacity(&nav, &cache, &state), 1207);
}
