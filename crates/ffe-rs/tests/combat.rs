//! Synthetic combat-state tests that need no cache: the loadout, save
//! compatibility, the firing gates and the range conversion.

use ffe_rs::combat::Combat;
use ffe_rs::flight::{Flight, Mode};

#[test]
fn new_game_loadout_and_gun_mounts_are_f998s() {
    let combat = Combat::new_game();
    assert_eq!(combat.lasers, [ffe_core::laser::PULSE_1MW, 0, 0, 0]);
    assert_eq!(combat.energy, 0x2000_0000);
    assert_eq!(combat.drive, 2);
    assert_eq!(combat.energy_gauge(), 0xffe0);
}

#[test]
fn combat_state_survives_a_save_and_old_saves_load_empty() {
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.combat = Combat::new_game();
    let bytes = ffe_rs::saves::encode(&state).unwrap();
    let loaded = ffe_rs::saves::decode(&bytes).unwrap();
    assert_eq!(loaded.combat, state.combat);
    // A save from before combat has no field; it loads with no lasers.
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["state"].as_object_mut().unwrap().remove("combat");
    let old = ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(old.combat, Combat::default());
}

#[test]
fn invalid_laser_codes_and_drive_bytes_are_rejected() {
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.combat.lasers[0] = 0xf8; // index 15: past the original's 11-entry table
    assert!(state.validate().is_err());
    state.combat.lasers[0] = ffe_core::laser::PULSE_1MW;
    state.combat.drive = 0x0f;
    assert!(state.validate().is_err());
    state.combat.drive = 2;
    assert!(state.validate().is_ok());
}

#[test]
fn the_pulse_refire_heat_and_energy_gates_match_f931() {
    let mut combat = Combat::new_game();
    // The first shot succeeds; the second is blocked by the refire word.
    assert!(combat.fire(0, 994).is_ok());
    assert_eq!(combat.fire(0, 994), Err(ffe_core::laser::FireError::Refire));
    // F730 subtracts the frame time.
    combat.tick(ffe_core::laser::PULSE_REFIRE as u32, 0.02);
    assert!(combat.fire(0, 994).is_ok());
    // Heat above 0xf000 blocks; cooling brings it back.
    combat.refire = 0;
    combat.heat = 0xf001;
    assert_eq!(
        combat.fire(0, 994),
        Err(ffe_core::laser::FireError::Overheated)
    );
    combat.tick(0, 100.0);
    assert_eq!(combat.heat, 0);
    // No energy blocks.
    combat.energy = 0;
    assert_eq!(
        combat.fire(0, 994),
        Err(ffe_core::laser::FireError::NoEnergy)
    );
}
