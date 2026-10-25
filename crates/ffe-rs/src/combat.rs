//! The player's laser installation and its firing state (guide: `docs/notes/combat.md`).
//!
//! The original keeps the four mount lasers at `ship+0xd2..+0xd5`, the refire
//! countdown at `+0xe8`, the heat at `+0xea` and the energy at `+0x11e`. The
//! new game's start type 1 (Ross 154, model `0x17`) fits one 1MW pulse laser
//! (`0x88`) in the front mount, two missiles, a class-2 drive and 1 t of
//! energy; `FUNC_000998` at `ffe.asm:74099` is the source. There is no
//! outfitting or save equipment in this port, so that new-game loadout is kept
//! when the development ship list changes the hull.
use ffe_core::laser::{self, FireError, LaserSpec, State};
use serde::{Deserialize, Serialize};

use crate::camera::{self, ViewMode};
use crate::flight::Flight;
use crate::original_flight::{TICS_PER_SECOND, UNITS_PER_FLIGHT_UNIT};
use crate::ships::ShipStats;

/// The player's laser installation and the three words F931 gates on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Combat {
    /// The front, rear, top and bottom laser equipment codes (`ship+0xd2..+0xd5`).
    #[serde(default)]
    pub lasers: [u8; 4],
    /// The ten missile pylon words (`ship+0xd6..+0xdf`); the low nibble is the
    /// type (`DATA_007220`), bit 0x40 a mine. Saves from before missiles load
    /// with none.
    #[serde(default)]
    pub missiles: [u8; ffe_core::missile::PYLONS],
    /// The energy pool (`+0x11e`); F929 removes each shot's cost.
    #[serde(default)]
    pub energy: u32,
    /// The heat (`+0xea`); F931 refuses above 0xf000 and F938 cools it.
    #[serde(default)]
    pub heat: u16,
    /// The pulse refire countdown (`+0xe8`).
    #[serde(default)]
    pub refire: u16,
    /// The drive byte (`+0xd0`) F270's energy bar scales by.
    #[serde(default)]
    pub drive: u8,
    /// `ship+0xc8 & 1`, the laser cooling booster (equipment is not otherwise
    /// modelled; always false in the port for now).
    #[serde(default)]
    pub cooling_booster: bool,
    /// F938 applies whole seconds; the fraction is carried for the next frame.
    #[serde(default)]
    pub cool_carry: f64,
}

impl Combat {
    /// F998's start type 1 record: a 1MW pulse laser in the front mount, a
    /// class-2 drive and `0x20000000` energy (one tonne).
    pub fn new_game() -> Self {
        let mut missiles = [0; ffe_core::missile::PYLONS];
        missiles[0] = ffe_core::missile::PLAYER_MISSILE;
        missiles[1] = ffe_core::missile::PLAYER_MISSILE;
        Self {
            lasers: [laser::PULSE_1MW, 0, 0, 0],
            missiles,
            energy: 0x2000_0000,
            heat: 0,
            refire: 0,
            drive: 2,
            cooling_booster: false,
            cool_carry: 0.0,
        }
    }

    /// F1054's `M` scan: the first non-zero pylon word and its 1-based index
    /// (`ship+0xc4`). `None` when the ship has no missiles.
    pub fn first_missile(&self) -> Option<(usize, u8)> {
        self.missiles
            .iter()
            .position(|code| *code != 0)
            .map(|index| (index + 1, self.missiles[index]))
    }

    /// F941: clear the pylon word after a successful launch. `pylon` is the
    /// 1-based index [`Combat::first_missile`] returns.
    pub fn take_missile(&mut self, pylon: usize) -> Option<u8> {
        let slot = self.missiles.get_mut(pylon.checked_sub(1)?)?;
        let code = *slot;
        if code == 0 {
            return None;
        }
        *slot = 0;
        Some(code)
    }

    /// The number of missiles still on the pylons.
    pub fn missile_count(&self) -> usize {
        self.missiles.iter().filter(|code| **code != 0).count()
    }

    /// The original's three mutable words.
    pub fn state(&self) -> State {
        State {
            refire: self.refire,
            heat: self.heat,
            energy: self.energy,
        }
    }

    fn store(&mut self, state: State) {
        self.refire = state.refire;
        self.heat = state.heat;
        self.energy = state.energy;
    }

    /// F931/F930 for one mount. On success the heat, energy and refire words are
    /// updated and the shot's cost and damage returned.
    pub fn fire(&mut self, mount: usize, frame_tics: u32) -> Result<LaserSpec, FireError> {
        let code = *self.lasers.get(mount).unwrap_or(&0);
        let mut state = self.state();
        let result = laser::fire(code, &mut state, frame_tics);
        self.store(state);
        result
    }

    /// F730 and F938 for one frame: the refire countdown and the heat cooling.
    /// `seconds` is accelerated world time; F938 takes whole seconds, so the
    /// fraction is carried until it makes one.
    pub fn tick(&mut self, frame_tics: u32, seconds: f64) {
        let mut state = self.state();
        laser::tick_refire(&mut state, frame_tics);
        self.cool_carry += seconds.max(0.0);
        let whole = self.cool_carry.floor();
        if whole >= 1.0 {
            laser::cool(&mut state, whole, self.cooling_booster);
            self.cool_carry -= whole;
        }
        self.store(state);
    }

    /// F270's first gauge value.
    pub fn energy_gauge(&self) -> u32 {
        laser::energy_gauge(self.energy, self.drive)
    }
}

/// `DATA_009155` for one frame: the elapsed second converted to the original's
/// 1/49710 s tics and scaled by the time-acceleration factor. At 1x a 50 Hz
/// frame is 994 tics and a 60 Hz frame 828. F730 subtracts this word from the
/// refire countdown (`+0xe8`) and F930's beam branch scales by `tics >> 7`, so a
/// frame count or a 50 Hz tick is not a substitute: 0x795c tics is 0.63 s, and
/// 0x795c frames would be ten minutes.
pub fn frame_tics(elapsed: f64, acceleration_factor: f64) -> u32 {
    (elapsed.max(0.0) * TICS_PER_SECOND * acceleration_factor.max(0.0)) as u32
}

/// One frame's visible laser beam: model `0x9c` (156) drawn at the gun, with the
/// ship's (or turret's) axes, exactly as F1776 sets up the duplicate object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserBeam {
    /// Gun position in flight units.
    pub origin: [f64; 3],
    /// The axes the beam is drawn along (right, up, forward), flight space.
    pub axes: [[f64; 3]; 3],
}

/// The mount a view fires, F1040: the view itself for 0..3, the front mount in
/// the external and combat views (4 and 6), and no firing in the missile view
/// (5). Returns the mount index into `Combat::lasers`.
pub fn mount(view: ViewMode) -> Option<usize> {
    match view {
        ViewMode::Front => Some(0),
        ViewMode::Rear => Some(1),
        ViewMode::TopTurret => Some(2),
        ViewMode::BottomTurret => Some(3),
        ViewMode::External => Some(0),
        // F1040's first check: the missile view (5) fires nothing.
        ViewMode::Missile => None,
    }
}

/// FUNC_001513 as the camera uses it.
fn rotate(v1: [f64; 3], v2: [f64; 3], angle: u16) -> ([f64; 3], [f64; 3]) {
    let radians = f64::from(angle) / 65536.0 * std::f64::consts::TAU;
    let (s, c) = radians.sin_cos();
    (
        std::array::from_fn(|i| v1[i] * c + v2[i] * s),
        std::array::from_fn(|i| v2[i] * c - v1[i] * s),
    )
}

/// The axes of a mount: the ship's for front and rear (rear is a half turn
/// about up), the turret's for the two turret mounts (FUNC_001513 by
/// `+0x204`/`+0x206`, as the camera and F1776 do).
pub fn mount_axes(state: &Flight, view_mode: ViewMode, turret: [i16; 2]) -> [[f64; 3]; 3] {
    let [right, up, forward] = camera::ship_axes(state);
    match view_mode {
        ViewMode::Rear => [right.map(|v| -v), up, forward.map(|v| -v)],
        ViewMode::TopTurret | ViewMode::BottomTurret => {
            let (forward, right) = rotate(forward, right, turret[0] as u16);
            let (up, forward) = rotate(up, forward, turret[1] as u16);
            [right, up, forward]
        }
        _ => [right, up, forward],
    }
}

/// The gun's position in flight units. F931/F1776 place it at
/// `forward * zoff + up * yoff`, each word in eighths of an original position
/// unit; the ship statistics give the mount's two words.
pub fn gun_origin(
    state: &Flight,
    stats: Option<&ShipStats>,
    view_mode: ViewMode,
    axes: [[f64; 3]; 3],
) -> [f64; 3] {
    let words = stats.map_or([0, 0], |stats| match view_mode {
        // The missile view fires nothing (F1040); the front gun is only a
        // harmless placeholder for a caller that asks for a gun origin here.
        ViewMode::Front | ViewMode::External | ViewMode::Missile => stats.front_gun,
        ViewMode::Rear => stats.rear_gun,
        ViewMode::TopTurret => stats.top_turret,
        ViewMode::BottomTurret => stats.bottom_turret,
    });
    // The original's `axis * word * 8 >> 12` (the Q31 row's high word is Q15).
    let scale = 8.0 / UNITS_PER_FLIGHT_UNIT;
    let [along, above] = words.map(|word| f64::from(word) * scale);
    std::array::from_fn(|i| state.position[i] + axes[2][i] * along + axes[1][i] * above)
}

/// The beam for a shot, ready for `Screen`.
pub fn beam(
    state: &Flight,
    stats: Option<&ShipStats>,
    view_mode: ViewMode,
    turret: [i16; 2],
) -> LaserBeam {
    let axes = mount_axes(state, view_mode, turret);
    LaserBeam {
        origin: gun_origin(state, stats, view_mode, axes),
        axes,
    }
}

/// Re-export the core laser errors for callers that match on them.
pub use ffe_core::laser::FireError as LaserError;

/// F270's three bar bitmap ids (combined table ids; 96 and up are secondary).
pub const GAUGE_BITMAPS: [u32; 3] = [275, 274, 273];

/// The bitmap id of the fixed gunsight, `F1111`'s `0x61` (97).
pub const GUNSIGHT_BITMAP: u32 = 0x61;
/// The bitmap id of the velocity marker, `F1112`'s `0x62` (98).
pub const VELOCITY_MARKER_BITMAP: u32 = 0x62;
/// F1776's laser-beam model, `0x9c` (156).
pub const BEAM_MODEL: u32 = 0x9c;
/// F1536's shield-hit flash model, `0x9d` (157), drawn at a ship whose shields
/// absorbed a hit (`ship+0x151 & 1`).
pub const SHIELD_FLASH_MODEL: u32 = 0x9d;
/// F942's death object model, `0x9b` (155), created with type 0xb and AI state
/// 0x1c at the destroyed ship. The screen draws it from a
/// `screen::SystemEffect` with global word 4 = `EXPLOSION_MASS`
/// (`ffe_core::effects`); the constant stays here for callers and tests.
pub const DEATH_MODEL: u32 = 0x9b;

/// Screen position of the velocity marker in the front view, F1112: the
/// velocity direction projected as an F1479 point with a small +z offset. A
/// zero velocity sits at the view centre.
pub fn velocity_marker(camera: &camera::Camera, velocity: [f64; 3]) -> Option<(i32, i32)> {
    let [x, y, z] = camera.direction(velocity);
    if x.abs() + y.abs() + z.abs() < 1e-12 {
        return Some((160, 79));
    }
    if z <= 0.0 {
        return None;
    }
    let sx = 160.0 + 256.0 * x / z;
    let sy = 79.0 - 256.0 * y / z;
    // F1112's own bounds, before the -19/-18 draw offset.
    ((15..=0x131).contains(&(sx as i32)) && (15..=0x8f).contains(&(sy as i32)))
        .then_some((sx as i32, sy as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight() -> Flight {
        Flight {
            mode: crate::flight::Mode::Flight,
            position: [0.0; 3],
            axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
            ..Flight::default()
        }
    }

    #[test]
    fn new_game_has_f998s_front_pulse_laser_and_two_missiles() {
        let combat = Combat::new_game();
        assert_eq!(combat.lasers, [laser::PULSE_1MW, 0, 0, 0]);
        assert_eq!(
            combat.missiles,
            [
                ffe_core::missile::PLAYER_MISSILE,
                ffe_core::missile::PLAYER_MISSILE,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0
            ]
        );
        assert_eq!(combat.missile_count(), 2);
        assert_eq!(
            combat.first_missile(),
            Some((1, ffe_core::missile::PLAYER_MISSILE))
        );
        assert_eq!(combat.energy, 0x2000_0000);
        assert_eq!(combat.drive, 2);
        assert_eq!(combat.energy_gauge(), 0xffe0);
    }

    #[test]
    fn frame_tics_are_data_009155_and_clear_the_pulse_refire() {
        // DATA_009155 at 1x: elapsed seconds in 1/49710 s tics.
        assert_eq!(frame_tics(0.02, 1.0), 994);
        assert_eq!(frame_tics(1.0 / 60.0, 1.0), 828);
        assert_eq!(frame_tics(1.0 / 144.0, 1.0), 345);
        // The time-acceleration factor multiplies the frame time (1x = 994).
        assert_eq!(frame_tics(1.0 / 60.0, 10.0), 8283);
        assert_eq!(frame_tics(0.0, 1.0), 0);

        // A 1MW pulse writes 0x795c tics to the refire word. At 60 Hz that is
        // 38 frames (0.63 s), not 0x795c frames (ten minutes); the second shot
        // then succeeds.
        let tics = frame_tics(1.0 / 60.0, 1.0);
        let mut combat = Combat::new_game();
        assert!(combat.fire(0, tics).is_ok());
        let mut frames = 0;
        while combat.refire != 0 {
            combat.tick(tics, 1.0 / 60.0);
            frames += 1;
            assert!(frames <= 39, "the refire word did not clear");
        }
        assert_eq!(frames, 38);
        assert_eq!(combat.fire(0, tics).unwrap().damage, 50);

        // The same wall time at 10x clears the word ten times sooner: the
        // original's countdown is in accelerated tics.
        let tics = frame_tics(1.0 / 60.0, 10.0);
        combat.refire = ffe_core::laser::PULSE_REFIRE;
        let mut frames = 0;
        while combat.refire != 0 {
            combat.tick(tics, 10.0 / 60.0);
            frames += 1;
            assert!(frames < 10);
        }
        assert_eq!(frames, 4);
    }

    #[test]
    fn the_missile_scan_finds_the_first_pylon_and_clears_it() {
        let mut combat = Combat::new_game();
        assert_eq!(
            combat.take_missile(1),
            Some(ffe_core::missile::PLAYER_MISSILE)
        );
        assert_eq!(combat.missiles[0], 0);
        assert_eq!(combat.missile_count(), 1);
        assert_eq!(
            combat.first_missile(),
            Some((2, ffe_core::missile::PLAYER_MISSILE))
        );
        assert_eq!(combat.take_missile(1), None);
        assert_eq!(
            combat.take_missile(2),
            Some(ffe_core::missile::PLAYER_MISSILE)
        );
        assert_eq!(combat.first_missile(), None);
        assert_eq!(combat.take_missile(0), None);
        assert_eq!(combat.take_missile(99), None);
    }

    #[test]
    fn firing_uses_the_mount_and_updates_the_words() {
        let mut combat = Combat::new_game();
        let shot = combat.fire(0, 994).unwrap();
        assert_eq!((shot.damage, shot.heat, shot.energy), (50, 400, 800));
        assert_eq!(combat.heat, 400);
        assert_eq!(combat.energy, 0x2000_0000 - 800);
        assert_eq!(combat.refire, laser::PULSE_REFIRE);
        assert_eq!(combat.fire(0, 994), Err(ffe_core::laser::FireError::Refire));
        // No rear, turret or bottom laser on the new-game hull.
        for mount in 1..4 {
            assert_eq!(
                combat.fire(mount, 994),
                Err(ffe_core::laser::FireError::NoLaser)
            );
        }
        combat.tick(laser::PULSE_REFIRE as u32, 0.02);
        assert_eq!(combat.refire, 0);
        assert!(combat.fire(0, 994).is_ok());
    }

    #[test]
    fn mounts_fire_from_the_view_that_is_showing() {
        assert_eq!(mount(ViewMode::Front), Some(0));
        assert_eq!(mount(ViewMode::Rear), Some(1));
        assert_eq!(mount(ViewMode::TopTurret), Some(2));
        assert_eq!(mount(ViewMode::BottomTurret), Some(3));
        assert_eq!(mount(ViewMode::External), Some(0));
        // F1040: view 5 does nothing.
        assert_eq!(mount(ViewMode::Missile), None);
    }

    #[test]
    fn gun_offsets_follow_f931s_word_scale() {
        let state = flight();
        let mut block = [0u8; 48];
        let word = |block: &mut [u8; 48], offset: usize, value: i16| {
            block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        };
        word(&mut block, 0x04, 1);
        word(&mut block, 0x06, 25);
        word(&mut block, 0x18, 1500);
        word(&mut block, 0x1a, 30);
        word(&mut block, 0x20, 1300);
        word(&mut block, 0x24, 800);
        word(&mut block, 0x28, 36);
        word(&mut block, 0x2a, 30);
        word(&mut block, 0x2c, 64);
        word(&mut block, 0x2e, 34);
        let stats = ShipStats::from_block(&block).unwrap();
        let axes = mount_axes(&state, ViewMode::Front, [0, 0]);
        let origin = gun_origin(&state, Some(&stats), ViewMode::Front, axes);
        let along = 1300.0 * 8.0 / UNITS_PER_FLIGHT_UNIT;
        assert!((origin[2] - along).abs() < 1e-12, "{origin:?}");
        assert!((origin[0]).abs() < 1e-12 && (origin[1]).abs() < 1e-12);
        // Rear is behind, front of a half-turned ship.
        let rear = mount_axes(&state, ViewMode::Rear, [0, 0]);
        let behind = gun_origin(&state, Some(&stats), ViewMode::Rear, rear);
        assert!((behind[2] + 800.0 * 8.0 / UNITS_PER_FLIGHT_UNIT).abs() < 1e-12);
    }

    #[test]
    fn velocity_marker_is_the_prograde_direction_only() {
        let camera = camera::Camera {
            position: [0.0; 3],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        assert_eq!(velocity_marker(&camera, [0.0, 0.0, 100.0]), Some((160, 79)));
        assert_eq!(
            velocity_marker(&camera, [20.0, 0.0, 100.0]),
            Some((211, 79))
        );
        assert_eq!(velocity_marker(&camera, [0.0, 0.0, -100.0]), None);
        assert_eq!(velocity_marker(&camera, [0.0, 0.0, 0.0]), Some((160, 79)));
    }
}
