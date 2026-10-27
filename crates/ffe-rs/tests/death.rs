//! F1104's death sequence and F1098's escape craft, without a cache.
//!
//! The state tests use synthetic `ShipStats` blocks, as `combat.rs` does; the
//! cached frames are in `death_view.rs`.

use ffe_core::equipment::Loadout;
use ffe_core::laser::Damage;
use ffe_rs::death::{
    self, DeathPhase, EscapeCraft, Outcome, ESCAPE_CAPSULE_BIT, ESCAPE_GENERAL, GAME_OVER_SECONDS,
    GAME_OVER_TICS, STOWMASTER_BIT,
};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::ships::ShipStats;
use ffe_rs::sound::GameAudio;

fn stats(mass: u16, drive: i16) -> ShipStats {
    let mut block = [0u8; 48];
    block[0x06..0x08].copy_from_slice(&mass.to_le_bytes());
    block[0x14..0x16].copy_from_slice(&drive.to_le_bytes());
    ShipStats::from_block(&block).unwrap()
}

fn destroyed() -> Flight {
    Flight {
        mode: Mode::Flight,
        credits: 1234,
        hold: vec![7; ffe_core::market::NAMES],
        position: [10.0, 20.0, -30.0],
        velocity: [4.0, 5.0, -6.0],
        hull: [0, 4608],
        ..Flight::default()
    }
}

#[test]
fn destruction_without_an_escape_craft_starts_the_f1104_hold() {
    let mut state = destroyed();
    state.loadout = Loadout::new_game();
    assert_eq!(death::escape_craft(&state.loadout), None);
    let outcome = death::begin(&mut state, None);
    assert_eq!(outcome, Outcome::Destroyed);
    let death = state.death.expect("F1104's state");
    assert_eq!(death.phase, DeathPhase::Exploding);
    assert!((death.remaining - GAME_OVER_SECONDS).abs() < 1e-9);
    assert_eq!(GAME_OVER_TICS, 248_550);
    // F953's lethal branch leaves the hull word at zero; F1104 does not reset it.
    assert_eq!(state.hull, [0, 4608]);
    // The hold is the source's external-view time, then the game-over page.
    let mut death = death;
    death.tick(GAME_OVER_SECONDS - 0.01);
    assert_eq!(death.phase, DeathPhase::Exploding);
    death.tick(0.02);
    assert_eq!(death.phase, DeathPhase::GameOver);
}

#[test]
fn the_escape_capsule_launches_and_keeps_cash_cargo_and_motion() {
    let mut state = destroyed();
    state.loadout = Loadout {
        general: ESCAPE_CAPSULE_BIT,
        cabins: 4,
        missiles: [0x82, 0x82, 0, 0, 0, 0, 0, 0, 0, 0],
        shield_generators: 1,
        ..Loadout::default()
    };
    state.combat = ffe_rs::combat::Combat::new_game();
    state.shields = [0x40, 0x40];
    let capsule = stats(5, 6);
    let outcome = death::begin(&mut state, Some(&capsule));
    assert_eq!(outcome, Outcome::Escaped(EscapeCraft::Capsule));
    assert_eq!(state.ship, 14, "model 0x0e");
    assert!(state.death.is_none());
    // F1098's reset: autopilot + atmospheric shielding, no gun, no pylons, no
    // shield generator. F1533 takes the capsule's negative drive word.
    assert_eq!(state.loadout.general, ESCAPE_GENERAL);
    assert_eq!(state.loadout.general, 0x4020_0000);
    assert_eq!(state.loadout.missiles, [0; 10]);
    assert_eq!(state.loadout.shield_generators, 0);
    assert_eq!(state.loadout.cabins, 4, "F1098 leaves +0x120 alone");
    assert_eq!(state.combat.lasers, [0; 4]);
    assert_eq!(state.combat.missiles, [0; 10]);
    assert_eq!(state.combat.energy, 0x2000_0000);
    // F1533 only replaces `+0xd0` for a negative model drive word; model 14's
    // positive word leaves the old drive in place.
    assert_eq!(state.combat.drive, 2);
    assert_eq!(state.loadout.drive, 2, "the fitted drive follows the craft");
    assert_eq!(state.shields, [0, 0]);
    assert_eq!(state.hull, [20, 20], "model mass 5 * 4");
    // The commander's cash, hold and motion survive F1098.
    assert_eq!(state.credits, 1234);
    assert_eq!(state.hold, vec![7; ffe_core::market::NAMES]);
    assert_eq!(state.position, [10.0, 20.0, -30.0]);
    assert_eq!(state.velocity, [4.0, 5.0, -6.0]);
}

#[test]
fn the_stowmaster_wins_and_carries_its_laser_drive_and_cabins() {
    let mut state = destroyed();
    state.loadout = Loadout {
        general: ESCAPE_CAPSULE_BIT | STOWMASTER_BIT,
        cabins: 4,
        ..Loadout::default()
    };
    state.combat = ffe_rs::combat::Combat {
        lasers: [0xaa, 0xbb, 0xcc, 0xdd],
        drive: 9,
        ..Default::default()
    };
    let outcome = death::begin(&mut state, Some(&stats(14, -5)));
    assert_eq!(outcome, Outcome::Escaped(EscapeCraft::Stowmaster));
    assert_eq!(state.ship, 16, "model 0x10");
    assert_eq!(state.combat.lasers, [ffe_core::laser::PULSE_1MW, 0, 0, 0]);
    assert_eq!(state.combat.drive, 2, "F1098 writes drive 2");
    assert_eq!(state.loadout.cabins, 2);
    assert_eq!(state.hull, [56, 56]);
}

#[test]
fn a_dead_ship_cannot_be_saved_and_old_saves_load_alive() {
    let mut state = destroyed();
    let bytes = ffe_rs::saves::encode(&state).unwrap();
    let loaded = ffe_rs::saves::decode(&bytes).unwrap();
    assert!(loaded.death.is_none());
    assert!(state.death.is_none());
    death::begin(&mut state, None);
    assert!(state.death.is_some());
    assert!(ffe_rs::saves::encode(&state).is_err());
    // A save from before the field has no `death` key and loads alive.
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["state"].as_object_mut().unwrap().remove("death");
    assert!(ffe_rs::saves::decode(&serde_json::to_vec(&json).unwrap())
        .unwrap()
        .death
        .is_none());
}

#[test]
fn f942s_explosion_and_debris_spawn_at_the_player() {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let effects = death::player_explosion([100, 200, 300], [1.0, 2.0, 3.0], 8, axes, 42.0);
    assert!(effects
        .iter()
        .any(|effect| effect.model == ffe_core::effects::EXPLOSION_MODEL));
    assert!(effects
        .iter()
        .any(|effect| effect.model == ffe_core::effects::DEBRIS_MODEL));
    assert!(effects
        .iter()
        .all(|effect| effect.position == [100, 200, 300]));
}

#[test]
fn the_destruction_and_escape_cues_are_the_sources_samples_and_stop_music() {
    let sink = Recorder::default();
    let mut audio = GameAudio::new(sink.clone());
    death::play_destroyed_audio(&mut audio);
    // F770/F1903's stop, then F942's 0x05/0x1f, F1104's 0x1f before F1098 and
    // its second 0x1f after the failed launch.
    assert_eq!(sink.0.lock().unwrap().stops, 1);
    assert_eq!(
        sink.0.lock().unwrap().samples,
        vec![
            ffe_rs::sound::sample::SHIP_EXPLOSION,
            ffe_rs::sound::sample::BOOM,
            ffe_rs::sound::sample::BOOM,
            ffe_rs::sound::sample::BOOM
        ]
    );
    sink.0.lock().unwrap().samples.clear();
    sink.0.lock().unwrap().stops = 0;
    death::play_escape_audio(&mut audio);
    assert_eq!(
        sink.0.lock().unwrap().samples,
        vec![ffe_rs::sound::sample::LAUNCH]
    );
    assert_eq!(sink.0.lock().unwrap().stops, 0, "F1098 does not stop music");
    // The automatic path: F942's two cues, F1104's 0x1f before F1098, then the
    // launch. No music stop (F1104 returns before the game-over branch).
    sink.0.lock().unwrap().samples.clear();
    death::play_destroyed_escape_audio(&mut audio);
    assert_eq!(
        sink.0.lock().unwrap().samples,
        vec![
            ffe_rs::sound::sample::SHIP_EXPLOSION,
            ffe_rs::sound::sample::BOOM,
            ffe_rs::sound::sample::BOOM,
            ffe_rs::sound::sample::LAUNCH
        ]
    );
    assert_eq!(sink.0.lock().unwrap().stops, 0);
}

#[test]
fn the_death_page_uses_datas_commander_lines() {
    assert_eq!(
        death::died_lines("Jameson"),
        ["COMMANDER", "   Jameson", "     DIED"]
    );
    assert_eq!(
        death::escaped_message("Pirate", EscapeCraft::Capsule),
        "Pirate destroys your ship. Escape Capsule launched."
    );
    assert_eq!(
        death::escaped_message("Pirate", EscapeCraft::Stowmaster),
        "Pirate destroys your ship. \"Stowmaster\" fighter launched."
    );
}

#[derive(Clone, Default)]
struct Recorder(std::sync::Arc<std::sync::Mutex<Recorded>>);

#[derive(Default)]
struct Recorded {
    samples: Vec<u8>,
    stops: usize,
}

impl ffe_rs::audio::Audio for Recorder {
    fn play_song(&mut self, _index: i16) -> Result<(), ffe_rs::audio::AudioError> {
        Ok(())
    }
    fn stop_song(&mut self) {
        self.0.lock().unwrap().stops += 1;
    }
    fn song_done(&self) -> bool {
        true
    }
    fn play_sample(&mut self, code: u8, _volume: u8, _pitch: i32) {
        self.0.lock().unwrap().samples.push(code);
    }
    fn tick(&mut self, _elapsed: std::time::Duration) {}
    fn settings(&self) -> ffe_rs::audio::AudioSettings {
        ffe_rs::audio::AudioSettings::default()
    }
    fn set_settings(&mut self, _settings: ffe_rs::audio::AudioSettings) {}
    fn sound_flags(&self) -> ffe_rs::audio::SoundFlags {
        ffe_rs::audio::SoundFlags::default()
    }
    fn set_sound_flags(&mut self, _flags: ffe_rs::audio::SoundFlags) {}
}

#[test]
fn f953_reaches_the_destroyed_outcome_before_f1104() {
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    assert_eq!(state.take_damage(50, 25), Damage::Hull);
    assert_eq!(state.take_damage(50, 25), Damage::Destroyed);
    assert!(state.destroyed());
    assert_eq!(death::begin(&mut state, None), Outcome::Destroyed);
}

#[test]
fn f1105_external_camera_follows_the_explosion_instead_of_the_old_hull() {
    use ffe_rs::camera::{Camera, ViewMode, ViewState};
    let mut state = Flight::free_flight();
    state.position = [-10.0, 7.0, 100.0];
    assert_eq!(death::begin(&mut state, None), Outcome::Destroyed);
    let effect = ffe_core::effects::Effect {
        kind: ffe_core::effects::Kind::Explosion,
        model: ffe_core::effects::EXPLOSION_MODEL,
        position: [97000, 194000, 291000],
        velocity: [9700.0, 0.0, 0.0],
        forward: [1.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        timer: 0xffff,
        countdown: 0,
        think: 0,
        alive: true,
        shift: 0,
        cargo: None,
        size: 0,
        life: 0,
        exit: false,
    };
    state.death.as_mut().unwrap().track_explosion(&effect);
    let mut views = ViewState {
        mode: ViewMode::External,
        ..ViewState::default()
    };
    views.external.distance = death::DEATH_CAMERA_DISTANCE;
    let camera = Camera::new(&state, &views, None, 9700.0);
    let centre = camera.point([10.0, 20.0, 30.0]);
    assert!(centre[0].abs() < 1.0e-10 && centre[1].abs() < 1.0e-10);
    assert!(centre[2] > 0.0);
    state.death.as_mut().unwrap().tick(0.5);
    assert_eq!(
        state.death.unwrap().reference.unwrap().position,
        [10.5, 20.0, 30.0]
    );
    assert_eq!(state.position, [-10.0, 7.0, 100.0]);
    assert_eq!(death::DEATH_CAMERA_DISTANCE as u32, 0xf9000000);
}
