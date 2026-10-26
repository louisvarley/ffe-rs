//! Synthetic combat-state tests that need no cache: the loadout, save
//! compatibility, the firing gates, the range conversion, and the ray and
//! mount geometry F931/F1040 use.

use ffe_core::laser;
use ffe_rs::camera::ViewMode;
use ffe_rs::combat::{self, Combat};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::ships::ShipStats;

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
    state.combat.drive = state.loadout.drive;
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
    assert_eq!(
        old.loadout,
        Loadout {
            drive: state.combat.drive,
            ..Loadout::default()
        }
    );
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

/// The new-game Eagle's descriptor words (F998/F1533): one gun mount, the eye
/// at 1500/30, the front gun at 1300/0 and the rear/turret words.
fn eagle_stats() -> ShipStats {
    let mut block = [0u8; 48];
    let word = |block: &mut [u8; 48], offset: usize, value: i16| {
        block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    };
    word(&mut block, 0x04, 1); // gun mounts
    word(&mut block, 0x06, 25); // mass
    word(&mut block, 0x18, 1500); // eye forward
    word(&mut block, 0x1a, 30); // eye up
    word(&mut block, 0x20, 1300); // front gun forward
    word(&mut block, 0x22, 0); // front gun up
    word(&mut block, 0x24, 800); // rear gun
    word(&mut block, 0x26, 0);
    word(&mut block, 0x28, 36); // top turret
    word(&mut block, 0x2a, 30);
    word(&mut block, 0x2c, 64); // bottom turret
    word(&mut block, 0x2e, 34);
    ShipStats::from_block(&block).unwrap()
}

fn straight_flight() -> Flight {
    Flight {
        mode: Mode::Flight,
        position: [0.0; 3],
        axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ..Flight::default()
    }
}

fn gun_mm(state: &Flight, view: ViewMode, turret: [i16; 2]) -> ([i64; 3], [f64; 3]) {
    let stats = eagle_stats();
    let beam = combat::beam(state, Some(&stats), view, turret);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    (beam.origin.map(|v| (v * unit).round() as i64), beam.axes[2])
}

#[test]
fn f931s_ray_starts_at_the_gun_and_follows_the_mount_axis() {
    let state = straight_flight();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    // The front ray is the gun's 1300/8 = 10400 mm, not the eye's 12000 mm.
    let (origin, direction) = gun_mm(&state, ViewMode::Front, [0, 0]);
    assert_eq!(origin, [0, 0, (1300.0 * 8.0 / unit * unit).round() as i64]);
    assert_eq!(origin[2], 10_400);
    assert_eq!(direction, [0.0, 0.0, 1.0]);

    let ahead = laser::Target {
        index: 7,
        centre: [0, 0, 200_000],
        radius: 5_000,
    };
    let shot = laser::hit(origin, direction, &[ahead]).expect("the centre shot misses");
    assert_eq!(shot.index, 7);
    // The 13-bit interaction box starts 5 m before the centre.
    assert_eq!(shot.point, [0, 0, 195_000]);

    // F931 does not aim at the crosshair's screen position: a ship 20 m to the
    // side at 200 m is outside the 5 m box and the nose-parallel ray misses.
    let off_axis = laser::Target {
        centre: [20_000, 0, 200_000],
        ..ahead
    };
    assert!(laser::hit(origin, direction, &[off_axis]).is_none());

    // F1111's bitmap 97 is 61x61 at (129,48), so its centre is (159,78), one
    // pixel up-left of F1479's projection centre (160,79). A target on the
    // crosshair is therefore 0.78 m off the nose at 200 m, still inside a 5 m
    // box; the 1 px offset is the original's art, not a ray error.
    let crosshair = laser::Target {
        centre: [
            (f64::from(-200_000) / 256.0).round() as i64,
            (f64::from(200_000) / 256.0).round() as i64,
            200_000,
        ],
        ..ahead
    };
    assert!(
        laser::hit(origin, direction, &[crosshair]).is_some(),
        "the gun ray does not pass through the crosshair"
    );
    let (sx, sy) = ffe_rs::camera::Camera {
        position: state.position,
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    }
    .project([0.0, 0.0, 200_000.0], true)
    .unwrap();
    assert_eq!((sx, sy), (160, 79));
}

#[test]
fn every_view_fires_its_own_mount_along_its_own_axis() {
    let state = straight_flight();
    // Front: +z from the front gun.
    let (origin, direction) = gun_mm(&state, ViewMode::Front, [0, 0]);
    assert_eq!(origin[2], 10_400);
    assert_eq!(direction, [0.0, 0.0, 1.0]);
    // Rear: F931 negates the forward axis and uses the rear gun word, so the
    // ray leaves from 800/8 = 6400 mm behind the centre towards -z.
    let (origin, direction) = gun_mm(&state, ViewMode::Rear, [0, 0]);
    assert_eq!(origin[2], -6_400);
    assert_eq!(direction, [0.0, 0.0, -1.0]);
    let behind = laser::Target {
        index: 1,
        centre: [0, 0, -200_000],
        radius: 5_000,
    };
    assert_eq!(laser::hit(origin, direction, &[behind]).unwrap().index, 1);
    // The external view is F1040's mount 0, with the ship's axes and gun, not
    // the external camera's: the shot comes from the ship, not the eye.
    assert_eq!(combat::mount(ViewMode::External), Some(0));
    assert_eq!(
        combat::beam(&state, Some(&eagle_stats()), ViewMode::External, [0, 0]),
        combat::beam(&state, Some(&eagle_stats()), ViewMode::Front, [0, 0])
    );
    // Turrets use F1038's yaw/pitch axes: a quarter turn puts the top turret's
    // ray on +x.
    let (origin, direction) = gun_mm(&state, ViewMode::TopTurret, [0x4000, 0]);
    for (actual, expected) in direction.into_iter().zip([1.0, 0.0, 0.0]) {
        assert!((actual - expected).abs() < 1e-12, "{direction:?}");
    }
    assert_eq!(origin, [288, 240, 0]);
    let side = laser::Target {
        index: 2,
        centre: [200_000, 0, 0],
        radius: 5_000,
    };
    assert_eq!(laser::hit(origin, direction, &[side]).unwrap().index, 2);
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
