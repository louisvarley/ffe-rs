//! The original flight model in the playable: the verified free-flight tick
//! (`ffe_core::player`) driven by real time and the original's own controls,
//! with the result copied back into [`Flight`] for the views, docking checks and
//! engine glows. Positions are the original's millimetres (about 1 per model
//! unit); one flight unit is 9,700 of them, the scale the station is drawn at.
//! Mouse steering is not wired (the mouse is the console's); the keys are the
//! original's. See `docs/notes/player.md`.
use crate::flight::Flight;
use crate::ships::ShipStats;
use ffe_core::physics::Motion;
use ffe_core::player::{Controls, Player, SteerKeys, ThrusterKeys};
use ffe_core::thrust::Limits;

/// Position units (about a millimetre) per flight unit.
pub const UNITS_PER_FLIGHT_UNIT: f64 = 9_700.0;
/// The original's time units per second (`ms * 0x3e2 / 20` per millisecond).
pub const TICS_PER_SECOND: f64 = 49_700.0;
/// Millimetres per second per velocity unit (F1511: `v * t / 512`).
pub const MM_PER_SECOND: f64 = TICS_PER_SECOND / 512.0;
/// The Eagle's limits, for a cache without ship statistics.
const FALLBACK_LIMITS: Limits = Limits {
    axes: [[4905, -4905], [4905, -4905], [16350, -4905]],
};

/// The keys the original flight model reads, true while held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Keys {
    /// Return: raise the set speed.
    pub faster: bool,
    /// Shift: lower it.
    pub slower: bool,
    pub pitch_down: bool,
    pub pitch_up: bool,
    pub left: bool,
    pub right: bool,
    pub roll_left: bool,
    pub roll_right: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginalFlight {
    player: Player,
    /// Seconds each steering key has been held, in `SteerKeys` order.
    held: [f64; 6],
    forward_limit: i16,
    /// The time acceleration (`DATA_009012`, per 20 ms): 0x3e2 is real time.
    acceleration: u32,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn to_q31(value: f64) -> i32 {
    (value * 2_147_483_647.0)
        .round()
        .clamp(-2_147_483_648.0, 2_147_483_647.0) as i32
}

impl OriginalFlight {
    /// Start from a flight state: its position, velocity and orientation, the
    /// set speed at the current speed along the nose (F1018's switch to manual),
    /// and the ship's limits and gun mounts when the cache has them.
    pub fn from_flight(flight: &Flight, stats: Option<&ShipStats>) -> Self {
        let axes = crate::camera::ship_axes(flight);
        let to_units = |v: f64| v * UNITS_PER_FLIGHT_UNIT / MM_PER_SECOND;
        let velocity = flight.velocity.map(|v| to_units(v) as i32);
        let forward_speed = dot(flight.velocity, axes[2]);
        let limits = stats.map_or(FALLBACK_LIMITS, ShipStats::limits);
        let mut player = Player {
            motion: Motion {
                orientation: axes.map(|row| row.map(to_q31)),
                velocity,
                position: flight
                    .position
                    .map(|p| (p * UNITS_PER_FLIGHT_UNIT).round() as i64),
                model: flight.ship,
                gun_mounts: stats.map_or(0, |s| s.gun_mounts),
                drift: 0x3c,
                ..Motion::default()
            },
            limits,
            set_speed: to_units(forward_speed).round().max(0.0) as i32,
            altitude: i32::MAX,
            reference_counter: 0xd,
            active: true,
            keys_enabled: true,
            view: 4,
            ..Player::default()
        };
        player.scanner.player = 0;
        Self {
            player,
            held: [0.0; 6],
            forward_limit: limits.axes[2][0],
            acceleration: crate::console::TIME_ACCELERATION[0],
        }
    }

    /// The same model at a time acceleration step.
    pub fn with_time_acceleration(mut self, step: usize) -> Self {
        self.set_time_acceleration(step);
        self
    }

    /// Choose the time acceleration step (0 = 1x .. 4 = 10000x): the world
    /// advances by the real time times it, while key amounts stay in real time.
    pub fn set_time_acceleration(&mut self, step: usize) {
        self.acceleration = crate::console::TIME_ACCELERATION[step.min(4)];
    }

    /// Put the ship at a position (millimetres) with its velocity cleared and the
    /// set speed zero: a jump arrival.
    pub fn teleport(&mut self, position: [i64; 3]) {
        self.player.motion.position = position;
        self.player.motion.velocity = [0; 3];
        self.player.set_speed = 0;
    }

    /// Move the ship without touching its velocity or set speed (a change of frame).
    pub fn shift(&mut self, position: [i64; 3]) {
        self.player.motion.position = position;
    }

    pub fn acceleration_step(&self) -> usize {
        crate::console::TIME_ACCELERATION
            .iter()
            .position(|a| *a == self.acceleration)
            .unwrap_or(0)
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The set speed in metres per second.
    pub fn set_speed_metres(&self) -> f64 {
        f64::from(self.player.set_speed) * MM_PER_SECOND / 1000.0
    }

    /// Tell the model which view is showing (2 and 3 are the turrets, whose
    /// angles the steering keys then turn) and the turret angles.
    pub fn set_view(&mut self, view: u8, turret: [i16; 2]) {
        self.player.view = view;
        self.player.turret = turret;
    }

    pub fn view(&self) -> (u8, [i16; 2]) {
        (self.player.view, self.player.turret)
    }

    /// One frame of `elapsed` seconds with `keys` held: F1037/F1038 then F1041,
    /// F601/F602 and F1512 over the elapsed time in the original's units.
    pub fn step(&mut self, elapsed: f64, keys: &Keys) -> Result<(), String> {
        // The input time is real time; the world time is that scaled by the
        // acceleration (`ms * acceleration / 20`, 994 per 20 ms at 1x).
        let tics = (elapsed.clamp(0.0, 0.1) * TICS_PER_SECOND) as u32;
        let world_tics = (elapsed.clamp(0.0, 0.1) * 50.0 * f64::from(self.acceleration)) as u32;
        let down = [
            keys.pitch_down,
            keys.pitch_up,
            keys.left,
            keys.right,
            keys.roll_left,
            keys.roll_right,
        ];
        let mut ticks = [0u32; 6];
        for (index, is_down) in down.into_iter().enumerate() {
            if is_down {
                self.held[index] += elapsed;
                // 50 Hz ticks held, at least one while the key is down.
                ticks[index] = (self.held[index] * 50.0) as u32 + 1;
            } else {
                self.held[index] = 0.0;
            }
        }
        let controls = Controls {
            mouse: 0,
            faster: keys.faster,
            slower: keys.slower,
            steer: SteerKeys {
                pitch_down: ticks[0],
                pitch_up: ticks[1],
                left: ticks[2],
                right: ticks[3],
                roll_left: ticks[4],
                roll_right: ticks[5],
            },
        };
        self.player.input_time = tics;
        self.player.frame_tics = world_tics;
        self.player.update_set_speed(&controls);
        self.player
            .step(&ThrusterKeys::default(), i64::from(world_tics))
    }

    /// Copy the state into `flight`: position and velocity in flight units, the
    /// orientation, `yaw`/`pitch` for saves, and the forward thrust for the glows.
    pub fn apply(&self, flight: &mut Flight) {
        let motion = &self.player.motion;
        let axes: [[f64; 3]; 3] = motion
            .orientation
            .map(|row| row.map(|v| f64::from(v) / 2_147_483_648.0));
        flight.position = motion.position.map(|p| p as f64 / UNITS_PER_FLIGHT_UNIT);
        flight.velocity = motion
            .velocity
            .map(|v| f64::from(v) * MM_PER_SECOND / UNITS_PER_FLIGHT_UNIT);
        flight.axes = Some(axes);
        let forward = axes[2];
        flight.yaw = forward[0]
            .atan2(forward[2])
            .rem_euclid(std::f64::consts::TAU);
        flight.pitch = forward[1].clamp(-1.0, 1.0).asin().clamp(-1.4, 1.4);
        let limit = f64::from(self.forward_limit.max(1));
        flight.thrust = (f64::from(motion.thrust[2]) / limit).clamp(-1.0, 1.0);
        flight.ticks = flight.ticks.wrapping_add(1);
    }
}
