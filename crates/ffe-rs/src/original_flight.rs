//! The original flight model in the playable: the verified free-flight tick
//! (`ffe_core::player`) driven by real time and the original's own controls,
//! with the result copied back into [`Flight`] for the views, docking checks and
//! engine glows. Positions are the original's millimetres (about 1 per model
//! unit); one flight unit is 9,700 of them, the scale the station is drawn at.
//! Mouse steering is not wired (the mouse is the console's); the keys are the
//! original's. See `docs/notes/player.md`.
use crate::flight::Flight;
use crate::ships::ShipStats;
use ffe_core::collision::Contact;
use ffe_core::frames::{Bodies, Body, Words};
use ffe_core::objects::ModelInfo;
use ffe_core::physics::Motion;
use ffe_core::player::{Controls, Player, SteerKeys, ThrusterKeys};
use ffe_core::surface::SurfaceTools;
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
                craft: 1,
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

    /// Turn toward a point by at most `rate` radians per second over `seconds`: the
    /// original's autopilot sets the turn rates itself rather than pressing keys.
    pub fn turn_toward(&mut self, target: [i64; 3], seconds: f64, rate: f64) {
        let at = self.player.motion.position;
        let want: [f64; 3] = std::array::from_fn(|k| (target[k] - at[k]) as f64);
        let length = want.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length < 1.0 {
            return;
        }
        let want = want.map(|v| v / length);
        let rows: [[f64; 3]; 3] = self
            .player
            .motion
            .orientation
            .map(|row| row.map(|v| f64::from(v) / 2_147_483_648.0));
        let norm = |v: [f64; 3]| {
            let l = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-12);
            v.map(|c| c / l)
        };
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let forward = norm(rows[2]);
        let cosine = dot(forward, want).clamp(-1.0, 1.0);
        let angle = cosine.acos();
        if angle < 1e-9 {
            return;
        }
        let mut axis = cross(forward, want);
        if axis.iter().map(|c| c * c).sum::<f64>() < 1e-18 {
            // Dead astern: turn about the ship's up axis.
            axis = norm(rows[1]);
        }
        let axis = norm(axis);
        let step = angle.min(rate * seconds);
        let (sin, cos) = step.sin_cos();
        // Rodrigues' rotation of each row about the axis.
        let turned = rows.map(|v| {
            let c = cross(axis, v);
            let d = dot(axis, v);
            std::array::from_fn(|k| v[k] * cos + c[k] * sin + axis[k] * d * (1.0 - cos))
        });
        // Re-orthonormalise: forward first, then right and up from it.
        let forward = norm(turned[2]);
        let right = norm(cross(turned[1], forward));
        let up = cross(forward, right);
        let q = |v: f64| (v * 2_147_483_647.0) as i32;
        self.player.motion.orientation = [right, up, forward].map(|row| row.map(q));
    }

    /// Turn the ship to face a point (keeping it level against the system's y axis).
    pub fn face(&mut self, target: [i64; 3]) {
        let at = self.player.motion.position;
        let mut forward: [f64; 3] = std::array::from_fn(|k| (target[k] - at[k]) as f64);
        let length = forward.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length < 1.0 {
            return;
        }
        forward = forward.map(|v| v / length);
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let unit = |v: [f64; 3]| {
            let l = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-12);
            v.map(|c| c / l)
        };
        let reference = if forward[1].abs() > 0.99 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let right = unit(cross(reference, forward));
        let up = cross(forward, right);
        let q = |v: f64| (v * 2_147_483_647.0) as i32;
        self.player.motion.orientation = [right, up, forward].map(|row| row.map(q));
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
        let (_, world_tics) = self.prepare(elapsed, keys);
        self.player
            .step(&ThrusterKeys::default(), i64::from(world_tics))
    }

    /// One frame with F569's collision scan against `parent` (a planet, moon or
    /// starport) as the player's frame of reference: the original's F570/F575
    /// contact code runs, so a soft touch settles the ship on the terrain or the
    /// starport and a crash bounces it. `parent.position` is the body's absolute
    /// position. The player is body 1 and the parent body 2 in a two-body world;
    /// the scan's object index in the returned [`Contact`] is 2.
    pub fn step_landing(
        &mut self,
        elapsed: f64,
        keys: &Keys,
        parent: &Body,
        parent_position: [i64; 3],
        player_model: &ModelInfo,
        tools: Option<&SurfaceTools>,
    ) -> Result<Contact, String> {
        let (_, world_tics) = self.prepare(elapsed, keys);
        let (outer_radius, inner_radius) = player_model.radii();
        let vector: [i64; 3] = std::array::from_fn(|k| {
            self.player.motion.position[k].wrapping_sub(parent_position[k])
        });
        // The playable keeps the ship's absolute position between frames; the scan
        // wants it in the parent's linear frame.
        self.player.motion.parent = 2;
        self.player.motion.rotating = 0;
        self.player.motion.updated = 0;
        self.player.motion.switch_delay = 0;
        self.player.motion.craft = 1;
        self.player.motion.position = vector;
        // The two-body world is rebuilt every frame; do not let the original's
        // 13-tick frame selector run against it.
        self.player.reference_counter = 0xd;
        // A mathematically exact Q31 row set makes the landing dot product of two
        // unit vectors round past 0x7fffffff and wrap negative; the original's
        // integrated rows are a hair under unit length. Keep the same margin.
        for row in &mut self.player.motion.orientation {
            for value in row {
                *value = (f64::from(*value) * 0.999_99) as i32;
            }
        }

        let mut world = Bodies {
            stamp: 1,
            frame_tics: world_tics,
            player: 1,
            ..Bodies::default()
        };
        world.live[1] = true;
        world.physical[1] = true;
        world.live[2] = true;
        world.physical[2] = true;
        let motion = self.player.motion;
        let mut words = Words::default();
        // The landing gear is down (`+0x9e` word 1 = 0xffff): a slow, level touch
        // lands instead of crashing.
        words.0[1] = 0xffff;
        world.body[1] = Body {
            position: vector,
            velocity: motion.velocity,
            parent: 2,
            index: 1,
            kind: 0,
            orientation: std::array::from_fn(|k| motion.orientation[k / 3][k % 3]),
            outer_radius,
            inner_radius,
            shape: player_model.class(),
            model: motion.model,
            words,
            ..Body::default()
        };
        let mut planet = *parent;
        planet.position = parent_position;
        planet.parent = 0;
        planet.rotating = 0;
        planet.index = 2;
        world.body[2] = planet;

        let contact = self.player.tick_with(
            &ThrusterKeys::default(),
            i64::from(world_tics),
            &mut world,
            1,
            tools,
        )?;
        // Undo the per-frame landing margin so it does not accumulate.
        self.player.motion.renormalise();
        let absolute = world.absolute(1).unwrap_or(vector);
        self.player.motion.position = absolute;
        self.player.motion.parent = 0;
        self.player.motion.rotating = 0;
        self.player.motion.updated = 0;
        self.player.motion.switch_delay = 0;
        Ok(contact)
    }

    /// The key state F1037/F1038 and set speed for one frame; returns the input
    /// tics and the accelerated world tics.
    fn prepare(&mut self, elapsed: f64, keys: &Keys) -> (u32, u32) {
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
        (tics, world_tics)
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
