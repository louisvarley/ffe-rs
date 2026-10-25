//! Synthetic combat-state tests that need no cache: the loadout, save
//! compatibility, the firing gates and the range conversion.

use ffe_rs::combat::Combat;
use ffe_rs::flight::{Flight, Mode};

#[test]
fn new_game_loadout_and_gun_mounts_are_f998s() {
    let combat = Combat::new_game();
    assert_eq!(combat.lasers, [ffe_core::laser::PULSE_1MW, 0, 0, 0]);
    assert_eq!(combat.missile_count(), 2);
    assert_eq!(combat.missiles[0], ffe_core::missile::PLAYER_MISSILE);
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

#[test]
fn the_equipment_loadout_survives_a_save_and_old_saves_load_empty() {
    use ffe_core::equipment::Loadout;

    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.loadout = Loadout::new_game();
    state.shields = [64, 64];
    state.hull = [4608, 4608];
    let bytes = ffe_rs::saves::encode(&state).unwrap();
    let loaded = ffe_rs::saves::decode(&bytes).unwrap();
    assert_eq!(loaded.loadout, state.loadout);
    assert_eq!(loaded.loadout.missiles[0..2], [0x82, 0x82]);
    // A save from before equipment has no field; it loads with nothing fitted.
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["state"].as_object_mut().unwrap().remove("loadout");
    let old = ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(old.loadout, Loadout::default());
    assert_eq!(old.shields, [64, 64], "the damage words still round-trip");
    assert_eq!(old.hull, [4608, 4608]);
    // A save from before the damage model has neither field; the defaults load
    // and `ensure_damage` fills them from the ship statistics.
    json["state"].as_object_mut().unwrap().remove("shields");
    json["state"].as_object_mut().unwrap().remove("hull");
    let mut older = ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(older.shields, [0, 0]);
    assert_eq!(older.hull, [0, 0]);
    older.ensure_damage(4608);
    assert_eq!(older.hull, [4608, 4608]);
    // A loadout with a bit or missile code the original never sets is rejected.
    state.loadout.general = 1 << 14;
    assert!(ffe_rs::saves::encode(&state).is_err());
    state.loadout = Loadout::new_game();
    state.loadout.missiles[0] = 0x80;
    assert!(ffe_rs::saves::encode(&state).is_err());
}

#[test]
fn f953_damages_the_players_shields_then_hull() {
    use ffe_core::equipment::Loadout;
    use ffe_core::laser::Damage;

    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    // One shield generator: `+0xe2 = 0x40`; a 100 t hull is 400 in `+0xe4`.
    state.loadout = Loadout {
        shield_generators: 1,
        ..Loadout::default()
    };
    state.ensure_damage(400);
    assert_eq!(state.shields, [0x40, 0x40]);
    assert_eq!(state.hull, [400, 400]);
    assert!(!state.destroyed());
    // Shields absorb the hit first (F953's first branch).
    assert_eq!(state.hit(10), Damage::Shields);
    assert_eq!(state.shields, [0x36, 0x40]);
    assert_eq!(state.hull, [400, 400]);
    // A larger hit breaks the shields and takes the rest from the hull.
    assert_eq!(state.hit(60), Damage::Hull);
    assert_eq!(state.shields, [0, 0x40]);
    assert_eq!(state.hull, [394, 400]);
    // ensure_damage keeps the damage while the full words match.
    state.ensure_damage(400);
    assert_eq!(state.hull, [394, 400]);
    // A hit at least the remaining hull destroys the ship (F942).
    assert_eq!(state.hit(1000), Damage::Destroyed);
    assert_eq!(state.hull, [0, 400]);
    assert!(state.destroyed());
    // The HUD then reads the real words, not a default full hull.
    let hud = ffe_rs::hud::PlayerHud {
        shields: state.shields,
        hull: state.hull,
        ..Default::default()
    };
    assert_eq!(hud.shield_text().as_deref(), Some("Shields: 0%"));
    assert_eq!(hud.hull_text().as_deref(), Some("Hull: 0.0%"));
}

#[test]
fn a_new_game_hull_has_no_shields_until_a_generator_is_fitted() {
    use ffe_core::equipment::Loadout;
    use ffe_core::laser::Damage;

    let mut state = Flight {
        mode: Mode::Flight,
        loadout: Loadout::new_game(),
        ..Flight::default()
    };
    state.ensure_damage(4608);
    assert_eq!(state.shields, [0, 0], "F998 leaves +0xe2 zero");
    assert_eq!(state.hull, [4608, 4608]);
    // Without shields every point goes to the hull.
    assert_eq!(state.hit(50), Damage::Hull);
    assert_eq!(state.hull, [4558, 4608]);
    // Fitting a generator gives 0x40 points (F413) on the next frame.
    state.loadout.shield_generators = 1;
    state.ensure_damage(4608);
    assert_eq!(state.shields, [0x40, 0x40]);
    assert_eq!(state.hull, [4558, 4608], "damage survives the fit");
}

#[test]
fn f953_damages_the_players_hull_and_records_the_full_value() {
    use ffe_core::laser::Damage;
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    // The new-game Eagle has no shield generator and an untracked hull, so the
    // first hit records `+0xe4 = mass * 4` and takes the damage from it.
    assert_eq!(state.take_damage(50, 25), Damage::Hull);
    assert_eq!(state.hull, [50, 100]);
    assert!(!state.destroyed());
    assert_eq!(state.take_damage(50, 25), Damage::Destroyed);
    assert!(state.destroyed());
    // With shields the hit is absorbed first (F953's `+0xe0` branch).
    let mut shielded = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    shielded.shields = [100, 100];
    assert_eq!(shielded.take_damage(50, 25), Damage::Shields);
    assert_eq!(shielded.shields, [50, 100]);
    assert_eq!(shielded.hull, [100, 100]);
    assert_eq!(shielded.take_damage(50, 25), Damage::Shields);
    assert_eq!(shielded.shields, [0, 100]);
    assert_eq!(shielded.take_damage(60, 25), Damage::Hull);
    assert_eq!(shielded.hull, [40, 100]);
}

#[test]
fn player_damage_survives_a_save_and_old_saves_default() {
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.take_damage(50, 25);
    state.shields = [10, 20];
    let bytes = ffe_rs::saves::encode(&state).unwrap();
    let loaded = ffe_rs::saves::decode(&bytes).unwrap();
    assert_eq!(loaded.hull, state.hull);
    assert_eq!(loaded.shields, state.shields);
    // A save from before the damage model has no words: it loads untracked.
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let object = json["state"].as_object_mut().unwrap();
    object.remove("hull");
    object.remove("shields");
    let old = ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(old.hull, [0, 0]);
    assert_eq!(old.shields, [0, 0]);
}
