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
pub const FALLBACK_LIMITS: Limits = Limits {
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
    /// The mouse accumulator (`DATA_009188`) while [`Keys::mouse_enabled`] is
    /// set: x in the low word, y in the high word.
    pub mouse: u32,
    /// The Classic preset's right button is held. When false the accumulator is
    /// ignored and its previous value cleared, so a later press cannot jump.
    pub mouse_enabled: bool,
    /// Invert the mouse x and y axes (the original's Reverse controls options).
    pub mouse_flip: [bool; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginalFlight {
    player: Player,
    /// Seconds each steering key has been held, in `SteerKeys` order.
    held: [f64; 6],
    forward_limit: i16,
    /// The time acceleration (`DATA_009012`, per 20 ms): 0x3e2 is real time.
    acceleration: u32,
    /// The two-body world of an open-air takeoff: the ship is body 1 and the
    /// starport body 2, so F570's frame switches (and their position, velocity
    /// and orientation changes) persist from frame to frame while the ship is
    /// on the pad. Dropped when the takeoff clears.
    pad_world: Option<Bodies>,
    guidance: Option<ffe_core::autopilot::Guidance>,
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
        let limits = flight.limits(stats);
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
            pad_world: None,
            guidance: None,
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

    /// F1042 and the launch button: zero the velocity, the turn rates, the
    /// thrust words and the set speed, and put the ship in F1013's climb
    /// (`DATA_008870` `0x24`). The caller has already placed it on the pad.
    pub fn begin_takeoff(&mut self) {
        self.player.mode = ffe_core::traffic::state::TAKEOFF;
        self.player.set_speed = 0;
        self.player.motion.velocity = [0; 3];
        self.player.motion.rate = [0; 3];
        self.player.motion.thrust = [0; 3];
    }

    /// Set the player's mode word (`DATA_008870`): the takeoff machine's
    /// states are copied here each frame so F1041's `0x24` up-thrust and the
    /// ordinary flight paths apply.
    pub fn set_mode(&mut self, mode: u8) {
        self.player.mode = mode;
    }

    /// F1021's landing: F1042 zeroes the velocity, the turn rates, the thrust
    /// words and the set speed, and `DATA_008870` becomes the frame's `0x32`
    /// (the caller's `Navigator::landing_frame` then sets `0x34`). The contact
    /// scan has already settled the ship on the surface.
    pub fn settle_landing(&mut self) {
        self.player.mode = ffe_core::traffic::state::LANDING;
        self.player.set_speed = 0;
        self.player.motion.velocity = [0; 3];
        self.player.motion.rate = [0; 3];
        self.player.motion.thrust = [0; 3];
    }

    /// F1019/F1048's open-air starport touch: F1042 zeroes the motion and the
    /// mode is the frame's `0x2e` (the caller's `Navigator::landing_frame` then
    /// sets `0x30`).
    pub fn settle_starport_landing(&mut self) {
        self.player.mode = ffe_core::traffic::state::STARPORT_LANDING;
        self.player.set_speed = 0;
        self.player.motion.velocity = [0; 3];
        self.player.motion.rate = [0; 3];
        self.player.motion.thrust = [0; 3];
    }

    /// The set speed (`DATA_008918`) in the original's velocity units.
    pub fn set_speed_units(&self) -> i32 {
        self.player.set_speed
    }

    /// Replace the six thrust limits the flight step clamps against
    /// (`ship+0xbc..+0xc6`): a damaged or failed thruster changes how much of
    /// the request F598/F600 can deliver.
    pub fn set_limits(&mut self, limits: Limits) {
        self.player.limits = limits;
        self.forward_limit = limits.axes[2][0];
    }

    /// The three thrust words F938 sums for the energy cost (`+0xb6`/`+0xb8`/
    /// `+0xba`, the clamp's result written each step).
    pub fn thrust_words(&self) -> [i16; 3] {
        self.player.motion.thrust
    }

    /// Start the persistent three-body world of an open-air takeoff. The
    /// original's F994 parents the player to the starport's parent (the planet)
    /// — `+0x56` is the object two slots below the starport — and the starport
    /// is a sibling. That is what gives F603 its gravity and F1033 its altitude
    /// above the terrain. Body 1 is the ship, body 2 the planet it is parented
    /// to and body 3 the starport, so F570's frame switches (and their position,
    /// velocity and orientation changes) persist from frame to frame while the
    /// ship is on the pad. Dropped when the takeoff clears.
    pub fn begin_pad_takeoff(
        &mut self,
        planet: &Body,
        planet_position: [i64; 3],
        station: &Body,
        station_position: [i64; 3],
        player_model: &ModelInfo,
    ) {
        let motion = self.player.motion;
        let vector: [i64; 3] =
            std::array::from_fn(|k| motion.position[k].wrapping_sub(planet_position[k]));
        let (outer_radius, inner_radius) = player_model.radii();
        let mut world = Bodies {
            stamp: 1,
            frame_tics: self.player.frame_tics,
            player: 1,
            ..Bodies::default()
        };
        world.live[1] = true;
        world.physical[1] = true;
        world.live[2] = true;
        world.physical[2] = true;
        let mut words = Words::default();
        // The landing gear is down (`+0x9e` word 1 = 0xffff).
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
        // F994's parent: the planet the starport orbits, with F880's gravity
        // word (`+0xa4`) for F603.
        let mut parent = *planet;
        parent.position = planet_position;
        parent.parent = 0;
        parent.rotating = 0;
        parent.index = 2;
        parent.delay = 0;
        parent.stamp = 0;
        world.body[2] = parent;
        // The starport is a sibling in the planet's frame, so F576's pad contact
        // and its `0x20` effect region still report against the ship.
        if station_position != planet_position || station.model != planet.model {
            let mut port = *station;
            port.position =
                std::array::from_fn(|k| station_position[k].wrapping_sub(planet_position[k]));
            port.parent = 2;
            port.rotating = 0;
            port.index = 3;
            port.delay = 0;
            port.stamp = 0;
            world.body[3] = port;
            world.live[3] = true;
            world.physical[3] = true;
        }
        self.player.motion.position = vector;
        self.player.motion.parent = 2;
        self.player.motion.rotating = 0;
        // The port's planet orientation is fixed; keep the ship in the linear
        // frame so F572/F573's fixed-point round trip cannot move it when the
        // 13-tick selector runs during the climb.
        self.player.motion.switch_delay = 0x7f;
        self.player.motion.updated = 0;
        self.pad_world = Some(world);
    }

    /// One frame on the pad in that world: the same F1041/F601/F1512/F1033
    /// tick as [`OriginalFlight::step_landing`], with the planet as the
    /// player's parent (F603's gravity and F1033's altitude) and the starport a
    /// sibling (F1031's `0x20` effect region and the pad contact).
    pub fn step_pad(
        &mut self,
        elapsed: f64,
        keys: &Keys,
        tools: Option<&SurfaceTools>,
    ) -> Result<Contact, String> {
        let (_, world_tics) = self.prepare(elapsed, keys);
        self.player.reference_counter = 0xd;
        self.player.motion.switch_delay = 0x7f;
        let world = self
            .pad_world
            .as_mut()
            .ok_or("No pad takeoff world to step.")?;
        // F1677's absolute-position cache is keyed by DATA_008811, the frame
        // stamp, so it must change every frame.
        world.stamp = world.stamp.wrapping_add(1);
        world.frame_tics = world_tics;
        tick_guided(&mut self.player, self.guidance, world_tics, world, tools)
    }

    /// The ship is still in the pad world.
    pub fn on_pad(&self) -> bool {
        self.pad_world.is_some()
    }

    /// F1677's absolute position in millimetres (the player's own position
    /// outside the pad world, where it is already absolute).
    pub fn absolute_position(&self) -> [i64; 3] {
        match &self.pad_world {
            Some(world) => {
                // Resolve the live motion without depending on a previous
                // collision pass's cached body/stamp.
                let mut world = world.clone();
                world.stamp = world.stamp.wrapping_add(1);
                world.body[1].take_motion(&self.player.motion);
                world.absolute(1).unwrap_or(self.player.motion.position)
            }
            None => self.player.motion.position,
        }
    }

    /// Leave the pad world: F573 turns the ship back into the linear frame and
    /// F1677 gives its absolute position, so ordinary free flight continues.
    pub fn end_pad_takeoff(&mut self) {
        let Some(mut world) = self.pad_world.take() else {
            return;
        };
        let motion = self.player.motion;
        world.stamp = world.stamp.wrapping_add(1);
        world.body[1].position = motion.position;
        world.body[1].velocity = motion.velocity;
        world.body[1].orientation = std::array::from_fn(|k| motion.orientation[k / 3][k % 3]);
        // F1677's absolute position first: it already inverts a rotating frame,
        // so the one-frame hand-over to the system frame does not move the ship
        // by a frame-rotation-sized step.
        let absolute = world.absolute(1).unwrap_or(motion.position);
        if world.body[1].rotating != 0 {
            world.to_linear(1, 2);
        }
        let body = world.body[1];
        self.player.motion.position = absolute;
        self.player.motion.velocity = body.velocity;
        self.player.motion.orientation =
            std::array::from_fn(|k| std::array::from_fn(|c| body.orientation[3 * k + c]));
        self.player.motion.parent = 0;
        self.player.motion.rotating = 0;
        self.player.motion.switch_delay = 0;
        self.player.motion.updated = 0;
    }

    /// Put the ship at a position (millimetres) with its velocity cleared and the
    /// set speed zero: a jump arrival.
    pub fn teleport(&mut self, position: [i64; 3]) {
        self.shift(position);
        self.player.motion.velocity = [0; 3];
        self.player.set_speed = 0;
    }

    /// Turn toward a point by at most `rate` radians per second over `seconds`: the
    /// original's autopilot sets the turn rates itself rather than pressing keys.
    pub fn turn_toward(&mut self, target: [i64; 3], seconds: f64, rate: f64) {
        let at = self.absolute_position();
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
        let at = self.absolute_position();
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
        let before = self.absolute_position();
        if let Some(world) = self.pad_world.as_mut() {
            // Navigator follows the moving planet in system coordinates. Carry
            // the local world's origin with it, retaining the ship/pad contact.
            for (axis, value) in position.iter().enumerate() {
                world.body[2].position[axis] =
                    world.body[2].position[axis].wrapping_add(value.wrapping_sub(before[axis]));
            }
            world.stamp = world.stamp.wrapping_add(1);
        } else {
            self.player.motion.position = position;
        }
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

    /// Install the source AI's thrust, velocity caps and F587/F589 turn words.
    pub fn apply_guidance(&mut self, guidance: ffe_core::autopilot::Guidance) {
        self.guidance = Some(guidance);
        self.player.mode = 2;
    }

    pub fn clear_guidance(&mut self) {
        if self.guidance.take().is_some() && self.player.mode == 2 {
            // F1018: resume manual flight with actual speed as the set speed.
            self.player.set_speed = ffe_core::model_math::length(self.player.motion.velocity).min(i32::MAX as u32) as i32;
            self.player.mode = 0;
        }
    }

    pub fn guidance(&self, target: [i64; 3], velocity: [i32; 3], modifier: i32) -> ffe_core::autopilot::Guidance {
        ffe_core::autopilot::guide(&self.player.motion, self.absolute_position(), target, velocity, &self.player.limits, modifier)
    }

    pub fn body_guidance(&self, target: [i64;3], velocity: [i32;3], radius: i64) -> ffe_core::autopilot::Guidance {
        ffe_core::autopilot::guide_to_radius(&self.player.motion, self.absolute_position(), target,
            velocity, &self.player.limits, 0, radius)
    }

    /// One frame of `elapsed` seconds with `keys` held: F1037/F1038 then F1041,
    /// F601/F602 and F1512 over the elapsed time in the original's units.
    pub fn step(&mut self, elapsed: f64, keys: &Keys) -> Result<(), String> {
        let (_, world_tics) = self.prepare(elapsed, keys);
        if let Some(guidance) = self.guidance {
            integrate_guidance(&mut self.player, guidance, world_tics, 0);
            Ok(())
        } else {
            self.player.step(&ThrusterKeys::default(), i64::from(world_tics))
        }
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

        let contact = tick_guided(&mut self.player, self.guidance, world_tics, &mut world, tools);
        // Undo the per-frame landing margin so it does not accumulate, and put
        // the player back in the system frame even when the scan fails (an
        // undecodable starport pad must not leave it parent-relative).
        self.player.motion.renormalise();
        let absolute = world.absolute(1).unwrap_or(vector);
        self.player.motion.position = absolute;
        self.player.motion.parent = 0;
        self.player.motion.rotating = 0;
        self.player.motion.updated = 0;
        self.player.motion.switch_delay = 0;
        contact
    }

    /// The key state F1037/F1038 and set speed for one frame; returns the input
    /// tics and the accelerated world tics.
    fn prepare(&mut self, elapsed: f64, keys: &Keys) -> (u32, u32) {
        // The input time is real time; the world time is that scaled by the
        // acceleration (`ms * acceleration / 20`, 994 per 20 ms at 1x).
        let tics = (elapsed.clamp(0.0, 0.1) * TICS_PER_SECOND) as u32;
        let world_tics = (elapsed.clamp(0.0, 0.1) * 50.0 * f64::from(self.acceleration)) as u32;
        if self.guidance.is_some() {
            self.player.input_time = tics;
            self.player.frame_tics = world_tics;
            return (tics, world_tics);
        }
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
        // The mouse is the Classic preset's right-button hold. With it up the
        // accumulator is cleared rather than merely ignored, so pressing the
        // button again starts from a zero delta instead of snapping the ship.
        let (mouse, mouse_flip) = if keys.mouse_enabled {
            (keys.mouse, keys.mouse_flip)
        } else {
            self.player.mouse_previous = 0;
            (0, [false; 2])
        };
        let controls = Controls {
            mouse,
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
        self.player.mouse_flip = mouse_flip;
        self.player.input_time = tics;
        self.player.frame_tics = world_tics;
        self.player.update_set_speed(&controls);
        (tics, world_tics)
    }

    /// Copy the state into `flight`: position and velocity in flight units, the
    /// orientation, `yaw`/`pitch` for saves, and the forward thrust for the glows.
    /// In the pad world the player's position is relative to the starport, so
    /// F1677's absolute position is what the views and navigation see.
    pub fn apply(&mut self, flight: &mut Flight) {
        let position = self.absolute_position();
        let motion = &self.player.motion;
        let axes: [[f64; 3]; 3] = motion
            .orientation
            .map(|row| row.map(|v| f64::from(v) / 2_147_483_648.0));
        flight.position = position.map(|p| p as f64 / UNITS_PER_FLIGHT_UNIT);
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

/// F727: gravity, capped acceleration, F585 with the AI flag, then F1512.
fn integrate_guidance(player: &mut Player, guidance: ffe_core::autopilot::Guidance, tics: u32, mass: u32) {
    player.motion.thrust = guidance.thrust;
    player.motion.cap = guidance.cap;
    player.motion.autopilot = guidance.turns;
    player.motion.gravity(mass, tics as i32);
    player.motion.accelerate(tics);
    player.motion.orient(tics as i32, true, tics as i32, &mut player.scanner);
    player.motion.advance(i64::from(tics));
}

fn tick_guided(player: &mut Player, guidance: Option<ffe_core::autopilot::Guidance>, tics: u32,
    world: &mut Bodies, tools: Option<&SurfaceTools>) -> Result<Contact, String> {
    let Some(guidance) = guidance else {
        return player.tick_with(&ThrusterKeys::default(), i64::from(tics), world, 1, tools);
    };
    let mass = world.body[usize::from(player.motion.parent)].mass;
    integrate_guidance(player, guidance, tics, mass);
    world.body[1].take_motion(&player.motion);
    let contact = world.collide_with(1, tools, 0xff).ok_or("The autopilot contact needs the models' collision records.")?;
    world.body[1].give_motion(&mut player.motion);
    player.altitude = contact.altitude();
    player.contact_flags = contact.flags;
    Ok(contact)
}
