//! Hit particles, ship explosions and their debris (F939, F942, F926).
//!
//! Source: JJFFE 2.8a7 `ffe.asm` (the gameplay reference), with the object
//! fields and tables in `ffedat.asm`. The routines:
//!
//! - **F939** (`ffe.asm:70291`) spawns the laser hit effect. F931 calls it once
//!   per hit as `F939(player, hit_position, 1)` (`ffe.asm:69804-69808`). It
//!   creates one object with **model 0x9e (158)**, control byte `9 | 0x20`
//!   (`+0x29`; the 0x20 bit is F928's "counted explosion" flag), AI state
//!   `+0xff = 0x1a`, AI class `+0x118 = 0xf8`, and clones the source object
//!   (`F1533`): its position, velocity, matrix rows (`+0x3e..+0x55`) and model
//!   statistics. It then
//!   - builds a random YZT matrix (`F1675`), and
//!   - keeps the cloned velocity only when the *player's* speed
//!     (`DATA_008867`) is at least `0x9c40` or the source's `+0x14c` has bit
//!     0x10; otherwise it zeroes `+0x8c/+0x90/+0x94`.
//!
//!   The flags in the third argument change the object's distance shift
//!   (`+0x7e`): bit 1 adds 2 and bit 2 subtracts 1 (`ffe.asm:70345-70351`).
//!   The laser path passes 1. The port does not model the per-object distance
//!   shift, so the particle draws at its model's own scale.
//!
//! - **F942** (`ffe.asm:70650`) is the destruction effect F953 calls when the
//!   hull is used up (`ffe.asm:71281-71287`). It creates one object with
//!   **model 0x9b (155)**, control byte `0xb`, AI state `+0xff = 0x1c`, a
//!   random YZT matrix and the destroyed ship's distance word (`+0x7e`)
//!   decremented by one. Two adjustments follow (`ffe.asm:70747-70786`):
//!   - with statistics, an object count at `+0x6` of 10 or more plays sample
//!     0x1f and re-increments the distance word; at 0x190 or more the distance
//!     word becomes `(ship_scale + ship_collision_radius) - (explosion_scale +
//!     explosion_collision_radius) + 4`, which the port does not model (it has
//!     no separate descriptor radii here);
//!   - for ship models 0x0f..0x3f it also spawns debris with **F926**
//!     (`ffe.asm:69099`): `F922(6) - 2` pods (0..4) with string `0x8e0f`, and
//!     `min(4, cargo_space >> 1)` more with a random cargo string
//!     (`0x8e00 + DATA_007212[rand & 0x1f]`). F926 makes a **model 0x99 (153)**
//!     object, control 0xb, AI state 0x15, and adds a random 0..1023 to each
//!     velocity word (`+0x8c..+0x94`).
//!
//! - **F952** (`ffe.asm:71154`) is not a visual effect: on a hull hit that
//!   does not destroy the ship it rolls for subsystem damage (`F945`/`F947`/
//!   `F948`/`F949` messages and sounds). The port leaves it out with the rest
//!   of the equipment damage model.
//!
//! Lifetimes are the AI table's (`DATA_006255..58`, four slots per `+0xff`
//! state; the movement slot runs every update with the frame's game tics
//! `DATA_009155`, 49,700 a second at 1x):
//!
//! - **F708** (state `0x1c`, the explosion): object global word 4 (`+0xa4`)
//!   starts at `0xffff` and loses `tics >> 2` an update; the object goes when
//!   that would pass zero, about 5.3 s at 1x. While the word is in
//!   `0xbc00..0xc800` (about 1.15 to 1.4 s in) each update also throws an F939
//!   spark from the explosion's centre.
//! - **F709** (state `0x1a`, the hit particle): the word starts at
//!   `F1530 | 0x8000` and loses `tics >> 1`, so a particle lives 1.3 to 2.6 s.
//! - **F712** (state `0x15`, a debris pod) throws an F939 spark every update
//!   and F768, on each event-6 tick, counts `+0x14e` down from 6; below zero
//!   the pod becomes state `0x16`, where F713 only moves it and F767 removes
//!   it once its distance class (`+0x88`) reaches `0xd`, about 67 km away.
//!
//! F764 (state `0x1d`) is not the explosion's: its `and` of the two clock
//! words (the original executable has `23 c2` there too) always writes
//! `0xffffffff`.
//!
//! The explosion model's own command stream gates its twelve fragment calls on
//! object global word 4 (`+0xa4`): the arithmetic and bit branches in model 155
//! select no fragment for the `0xffffffff` F942 writes there, but select all
//! twelve for FFP 1.0 (`0x4000`). [`EXPLOSION_MASS_GLOBAL`] and
//! [`EXPLOSION_MASS`] carry the value the renderer passes so the original's
//! fragment set is visible; the gate and both values are asserted in the
//! cached-frame test (`crates/ffe-rs/tests/effects_view.rs`).

use crate::npc::Random;

/// F939's hit-particle model, `0x9e` (158).
pub const HIT_MODEL: u32 = 0x9e;
/// F942's destruction model, `0x9b` (155).
pub const EXPLOSION_MODEL: u32 = 0x9b;
/// F926's debris model, `0x99` (153).
pub const DEBRIS_MODEL: u32 = 0x99;

/// Game tics a second at 1x (`DATA_009155` per 20 ms is 994).
pub const TICS_PER_SECOND: f64 = 49_700.0;
/// F942's starting timer word (`+0xa4`).
pub const EXPLOSION_TIMER: u16 = 0xffff;
/// F708's spark window on the explosion's timer word.
pub const SPARK_WINDOW: std::ops::Range<u16> = 0xbc00..0xc800;
/// F926's `+0x14e`: the event-6 ticks a pod sparks for (it changes state once
/// the count goes below zero).
pub const DEBRIS_SPARK_TICKS: i8 = 6;
/// The event-6 interval the port runs F768 at (the missile guidance cadence).
pub const THINK_SECONDS: f64 = 0.4;
/// F767's removal distance: distance class `0xd`, `2^(13 + 13)` mm.
pub const DEBRIS_RANGE: i64 = 1 << 26;
/// Seconds an explosion lives at 1x: F708's `0xffff` at `tics >> 2`.
pub const EXPLOSION_SECONDS: f64 = 0x1_0000 as f64 * 4.0 / TICS_PER_SECOND;

/// F939's speed cutoff (`DATA_008867` vs `0x9c40`): below it the particle does
/// not inherit the ship's velocity.
pub const PARTICLE_SPEED: f64 = 0x9c40 as f64;

/// Object global word 4 (`+0xa4`): the explosion's and particle's timer,
/// which model 155's fragment branches read.
pub const EXPLOSION_MASS_GLOBAL: usize = 4;

/// F942's `+0x9e` (object global word 1) on the explosion, `0xffff`
/// (`ffe.asm:70730`). Model 155's first instruction is `local5 = global1 >> 7`,
/// the fragments' size.
pub const EXPLOSION_SIZE: u16 = 0xffff;

/// Which spawn created an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// F939's one model-0x9e object at the hit point.
    Hit,
    /// F942's one model-0x9b destruction object.
    Explosion,
    /// F926's model-0x99 pod.
    Debris,
}

/// One short-lived visual object in the system frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub kind: Kind,
    pub model: u32,
    /// Position in the system frame, original units (millimetres).
    pub position: [i64; 3],
    /// Original units per second; F939 inherits the ship's, F926 adds 0..1023.
    pub velocity: [f64; 3],
    /// The object's forward and up axes (right is `up x forward`).
    pub forward: [f64; 3],
    pub up: [f64; 3],
    /// Global word 4 (`+0xa4`): F708/F709's countdown. Unused by debris.
    pub timer: u16,
    /// F926's `+0x14e`: a pod's remaining event-6 spark ticks.
    pub countdown: i8,
    /// Seconds towards the next event-6 tick.
    pub think: f64,
    pub alive: bool,
}

/// What an update asks the caller for besides the object itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Nothing,
    /// F939 from the object's centre (F708's window, F712's every update).
    Spark,
}

impl Effect {
    /// The right/up/forward axes, as `screen` and the camera take them.
    pub fn axes(&self) -> [[f64; 3]; 3] {
        [
            normalise(cross(self.up, self.forward)),
            self.up,
            self.forward,
        ]
    }

    /// One movement-slot update of `seconds` game seconds and `tics` game
    /// tics: F1512 carries the object on its velocity, then F708 (explosion),
    /// F709 (particle) or F712/F768 (debris) run their timers.
    pub fn tick(&mut self, seconds: f64, tics: u32) -> Update {
        if seconds <= 0.0 || !self.alive {
            return Update::Nothing;
        }
        for axis in 0..3 {
            self.position[axis] += (self.velocity[axis] * seconds).round() as i64;
        }
        let step = |shift: u32| (tics.wrapping_shl(16 - shift) >> 16) as u16;
        match self.kind {
            Kind::Explosion => {
                let step = step(2);
                if step > self.timer {
                    self.alive = false;
                    return Update::Nothing;
                }
                self.timer -= step;
                if SPARK_WINDOW.contains(&self.timer) {
                    return Update::Spark;
                }
                Update::Nothing
            }
            Kind::Hit => {
                let step = step(1);
                if step > self.timer {
                    self.alive = false;
                } else {
                    self.timer -= step;
                }
                Update::Nothing
            }
            Kind::Debris => {
                self.think += seconds;
                while self.think >= THINK_SECONDS && self.countdown >= 0 {
                    self.think -= THINK_SECONDS;
                    self.countdown -= 1;
                }
                if self.countdown >= 0 {
                    Update::Spark
                } else {
                    Update::Nothing
                }
            }
        }
    }

    /// F767: a drifting pod goes once it is `DEBRIS_RANGE` from `player`.
    pub fn out_of_range(&self, player: [i64; 3]) -> bool {
        self.kind == Kind::Debris
            && (0..3).any(|axis| (self.position[axis] - player[axis]).abs() >= DEBRIS_RANGE)
    }

    pub fn alive(&self) -> bool {
        self.alive
    }
}

/// F939: one hit particle (model 0x9e) at `position`, with the source's axes
/// and velocity. `speed` is the source's speed used for the `0x9c40` cutoff.
pub fn hit(
    position: [i64; 3],
    velocity: [f64; 3],
    axes: [[f64; 3]; 3],
    speed: f64,
    rng: &mut Random,
) -> Effect {
    let (forward, up) = random_axes(axes[2], rng);
    // F939: `+0xa4 = F1530 | 0x8000`.
    let timer = rng.bounded(0x1_0000) as u16 | 0x8000;
    Effect {
        kind: Kind::Hit,
        model: HIT_MODEL,
        position,
        velocity: if speed >= PARTICLE_SPEED {
            velocity
        } else {
            [0.0; 3]
        },
        forward,
        up,
        timer,
        countdown: 0,
        think: 0.0,
        alive: true,
    }
}

/// F719: the missile's own explosion (model 0x9b) after ECM or F721. F719
/// writes `+0xa4 = 0xffffffff` and `+0x9e = 0xffff` and calls F925 to remove
/// the missile; it does not spawn F926's debris pods the way F942 does, and
/// its velocity word is left zero.
pub fn missile_explosion(position: [i64; 3], axes: [[f64; 3]; 3], rng: &mut Random) -> Effect {
    let (forward, up) = random_axes(axes[2], rng);
    Effect {
        kind: Kind::Explosion,
        model: EXPLOSION_MODEL,
        position,
        velocity: [0.0; 3],
        forward,
        up,
        timer: EXPLOSION_TIMER,
        countdown: 0,
        think: 0.0,
        alive: true,
    }
}

/// F942 and F926: the destruction object plus its debris pods.
///
/// `cargo` is the ship's cargo space (`+0x116`, F942's second debris count).
/// The result always starts with the model-0x9b object.
pub fn explosion(
    position: [i64; 3],
    velocity: [f64; 3],
    cargo: u16,
    axes: [[f64; 3]; 3],
    rng: &mut Random,
) -> Vec<Effect> {
    let (forward, up) = random_axes(axes[2], rng);
    let mut effects = vec![Effect {
        kind: Kind::Explosion,
        model: EXPLOSION_MODEL,
        position,
        velocity,
        forward,
        up,
        timer: EXPLOSION_TIMER,
        countdown: 0,
        think: 0.0,
        alive: true,
    }];
    // F942: (F922(6) - 2) pods, then min(4, cargo >> 1) cargo pods.
    let pods = (rng.bounded(6) as i32 - 2).max(0) as usize;
    let cargo_pods = ((cargo >> 1) as usize).min(4);
    for _ in 0..pods + cargo_pods {
        let (forward, up) = random_axes(axes[2], rng);
        // F926 adds DATA_008818 >> 22 per axis: 0..1023 original units.
        let drift = [
            f64::from(rng.bounded(1024)),
            f64::from(rng.bounded(1024)),
            f64::from(rng.bounded(1024)),
        ];
        effects.push(Effect {
            kind: Kind::Debris,
            model: DEBRIS_MODEL,
            position,
            velocity: [
                velocity[0] + drift[0],
                velocity[1] + drift[1],
                velocity[2] + drift[2],
            ],
            forward,
            up,
            timer: 0,
            countdown: DEBRIS_SPARK_TICKS,
            think: 0.0,
            alive: true,
        });
    }
    effects
}

/// A random orientation like F1675_MatBuildYZT's two random binary angles: the
/// given forward tilted away from `base` by a random yaw and pitch.
fn random_axes(base: [f64; 3], rng: &mut Random) -> ([f64; 3], [f64; 3]) {
    let yaw = f64::from(rng.bounded(65_536)) / 65_536.0 * std::f64::consts::TAU;
    let pitch = f64::from(rng.bounded(65_536)) / 65_536.0 * std::f64::consts::PI;
    // A helper axis perpendicular to `base`, so the tilt is not degenerate for
    // a ship pointing straight up.
    let helper = if base[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let right = normalise(cross(helper, base));
    let up = normalise(cross(base, right));
    let forward = normalise([
        base[0] * pitch.cos() + (right[0] * yaw.cos() + up[0] * yaw.sin()) * pitch.sin(),
        base[1] * pitch.cos() + (right[1] * yaw.cos() + up[1] * yaw.sin()) * pitch.sin(),
        base[2] * pitch.cos() + (right[2] * yaw.cos() + up[2] * yaw.sin()) * pitch.sin(),
    ]);
    let right = normalise(cross(helper, forward));
    (forward, normalise(cross(forward, right)))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        v.map(|x| x / length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axes() -> [[f64; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    #[test]
    fn f939_spawns_one_particle_with_the_ships_velocity() {
        let mut rng = Random::new(0x1234_5678, 0x4567_89ab);
        let particle = hit(
            [10, 20, 30],
            [100.0, 200.0, 300.0],
            axes(),
            PARTICLE_SPEED,
            &mut rng,
        );
        assert_eq!(particle.model, HIT_MODEL);
        assert_eq!(particle.kind, Kind::Hit);
        assert_eq!(particle.position, [10, 20, 30]);
        assert_eq!(particle.velocity, [100.0, 200.0, 300.0]);
        assert!(particle.timer >= 0x8000);
        // A slow source leaves the particle where it was hit (F939 zeroes it).
        let mut rng = Random::new(1, 2);
        let slow = hit([0; 3], [1.0, 2.0, 3.0], axes(), 0.0, &mut rng);
        assert_eq!(slow.velocity, [0.0; 3]);
    }

    #[test]
    fn f942_spawns_the_explosion_and_f926_debris_with_its_counts() {
        let mut rng = Random::new(0x1234_5678, 0x4567_89ab);
        let effects = explosion([100, 200, 300], [10.0; 3], 8, axes(), &mut rng);
        let burst = &effects[0];
        assert_eq!(burst.model, EXPLOSION_MODEL);
        assert_eq!(burst.kind, Kind::Explosion);
        assert_eq!(burst.timer, EXPLOSION_TIMER);
        // F922(6) - 2 pods plus min(4, 8 >> 1) cargo pods.
        let debris = effects.iter().filter(|e| e.kind == Kind::Debris).count();
        assert!((4..=7).contains(&debris), "{debris}");
        // No cargo, and F922(6) can still give 0..3 pods.
        let mut rng = Random::new(3, 4);
        let light = explosion([0; 3], [0.0; 3], 0, axes(), &mut rng);
        assert!(light.len() <= 4);
        // A pod keeps the ship's velocity and adds its own 0..1023 drift.
        let mut rng = Random::new(5, 6);
        let pods = explosion([0; 3], [10.0; 3], 16, axes(), &mut rng);
        for pod in pods.iter().skip(1) {
            assert_eq!(pod.model, DEBRIS_MODEL);
            for axis in 0..3 {
                assert!((10.0..=1033.0).contains(&pod.velocity[axis]));
            }
        }
    }

    #[test]
    fn f719_spawns_one_explosion_and_no_debris() {
        let mut rng = Random::new(7, 8);
        let burst = missile_explosion([1, 2, 3], axes(), &mut rng);
        assert_eq!(burst.model, EXPLOSION_MODEL);
        assert_eq!(burst.kind, Kind::Explosion);
        assert_eq!(burst.position, [1, 2, 3]);
        assert_eq!(burst.velocity, [0.0; 3], "F719 leaves the velocity zero");
        assert_eq!(burst.timer, EXPLOSION_TIMER);
    }

    #[test]
    fn effects_move_and_expire() {
        let mut rng = Random::new(5, 6);
        let mut particle = hit([0; 3], [1000.0, 0.0, 0.0], axes(), PARTICLE_SPEED, &mut rng);
        particle.tick(0.1, 4970);
        assert_eq!(particle.position, [100, 0, 0]);
        assert!(particle.alive());
        // F709: at most 0xffff / (994 >> 1) updates of 20 ms, 2.6 s.
        for _ in 0..132 {
            particle.tick(0.02, 994);
        }
        assert!(!particle.alive());
        // A zero-length frame does nothing.
        let before = particle.position;
        particle.tick(0.0, 0);
        assert_eq!(particle.position, before);
    }

    #[test]
    fn f708_runs_the_explosion_timer_and_its_spark_window() {
        let mut rng = Random::new(7, 8);
        let mut burst = explosion([0; 3], [0.0; 3], 0, axes(), &mut rng).remove(0);
        let mut sparks = Vec::new();
        let mut updates = 0;
        while burst.alive() {
            if burst.tick(0.02, 994) == Update::Spark {
                sparks.push(updates);
            }
            updates += 1;
        }
        // 0xffff / (994 >> 2 = 248) is 264 updates of 20 ms: 5.3 s.
        assert_eq!(updates, 265);
        // 0xffff - 0xc800 .. 0xffff - 0xbc00 at 248 a step.
        assert_eq!(sparks.first(), Some(&57));
        assert_eq!(sparks.last(), Some(&69));
    }

    #[test]
    fn f926_pods_spark_for_seven_think_ticks_then_drift() {
        let mut rng = Random::new(5, 6);
        let mut pods = explosion([0; 3], [0.0; 3], 16, axes(), &mut rng);
        let mut pod = pods.pop().unwrap();
        assert_eq!(pod.kind, Kind::Debris);
        let mut sparking = 0.0;
        while pod.tick(0.02, 994) == Update::Spark {
            sparking += 0.02;
        }
        assert!((sparking - 7.0 * THINK_SECONDS).abs() < 0.05, "{sparking}");
        assert!(pod.alive());
        assert!(!pod.out_of_range([0; 3]));
        assert!(pod.out_of_range([2 * DEBRIS_RANGE, 0, 0]));
    }
}
