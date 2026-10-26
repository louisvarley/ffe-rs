//! Space dust: F1035's spawn, F769's lifetime, F715's (empty) movement and the
//! original's "Space dust, clouds etc" preference.
//!
//! Source: JJFFE 2.8a7 `ffe.asm` (the DOS release's `FIRSTENC.EXE` has the same
//! routines; FFED3D's copy of F1035/F715/F769 is identical). The routines:
//!
//! - **F1035** (`ffe.asm:76306`, the flight frame's spawner) runs once per
//!   flight frame when `DATA_008867` (the altitude F1033 measured) is at or
//!   above `0x1312d00` (20,000,000 mm = 20 km). Below that it calls the empty
//!   F1015/F1016/F1017 chain; above it calls F1036 (the surface/asteroid
//!   spawner, not modelled here) and then the dust gate.
//! - The gate order is: `DATA_009018` (the player flag byte at `+0x3f51`,
//!   "Space dust, clouds etc") non-zero, `DATA_008630` negative (the flight
//!   module active; `ffedat.lst:27749` documents `0xff` active / `0x7f`
//!   inactive), one `F1530` draw masked to 16 bits below `0x3332` (13106 of
//!   65536, one frame in five), then `DATA_007706` non-zero (time not stopped:
//!   F217 sets the 1x play value `0x3e2`, F229/F120 set 0 while paused/video).
//! - **F928** (`ffe.asm:69259`) allocates the object from the free slot with
//!   flags `0x2a` and model `0x6d` (109); **F1533** (`ffe.asm:90970`) clones
//!   the player's whole object, so the dust inherits the player's position,
//!   velocity and parent. The spawn then writes `+0xb0 = 0` (age),
//!   `+0x57 = 0` (not a rotating frame), `+0x87 = 0xb` (AI craft) and
//!   `+0xff = 0x1b` (the AI state table's dust row).
//! - **F1675** (`ffe.asm:101537`, `MatBuildYZT`) gives it a random YZT
//!   orientation from one `F1530` word and `DATA_008820 & 0xffff`.
//! - **F1517** (`ffe.asm:90632`) scales the inherited `+0x8c` velocity by
//!   `DATA_009123 + 7` into a 64-bit vector and `Vec64Add` (`ffe.asm:100373`)
//!   adds it to `+0x3e`: the dust appears ahead of the player. The distance
//!   word `+0x7e` is `(F1530 & 3) + DATA_008865 + DATA_009123 - 7`.
//! - **F769** (`ffe.asm:50379`, the AI table's control slot for state `0x1b`)
//!   removes the object when `+0x8b` (hidden) is set, when `+0x88` (F1680's
//!   exponential distance class) exceeds `0x14`, or when the age at `+0xb0`
//!   reaches `DATA_007706`. The age rises by `days << 8` on F152's whole-day
//!   tick (F151 → F706), and `DATA_007706` is the time acceleration, so the
//!   four day ticks it takes are four real seconds at any acceleration.
//! - **F715** (`ffe.asm:47368`, the AI table's movement slot for state `0x1b`)
//!   is an empty routine: dust never integrates its velocity. The motion seen
//!   in flight is the player's own frame motion past a static object (F1678);
//!   the inherited velocity is only used by F1035's spawn offset.
//! - **F1680** (`ffe.asm:101877`) computes `+0x88` from the relative position's
//!   largest-component MSB (F1657, `ffe.asm:100097`):
//!   `max(0, msb - 0xd) - object[+0x7e]`.
//!
//! The port keeps its own deterministic [`Random`] stream so dust never
//! perturbs the NPC traffic's rolls (EXP-020). Rendering is the follow-up: this
//! module stores what the draw will need (absolute position, orientation
//! angles, model and state) but draws nothing.

use crate::model_animation::build_yzt;
use crate::model_math::SineTable;
use crate::npc::Random;

/// The dust object's model (`F1035`'s F928 argument `0x6d`).
pub const MODEL: u32 = 0x6d;
/// F928's flag byte for the dust object: `0x20` (counted/recyclable), `0x8`
/// (AI) and `0x2` (active).
pub const FLAGS: u8 = 0x2a;
/// F928 writes the slot into `+0x86`; F1035 sets `+0x87 = 0xb` (AI craft).
pub const CRAFT: u8 = 0xb;
/// F1035 sets `+0xff` to the AI table's dust row (`DATA_006258` 0x1b:
/// control F769, movement F715, no notify, no damage routine).
pub const AI_STATE: u8 = 0x1b;
/// F1035's altitude gate, `DATA_008867`: 20,000,000 mm (20 km).
pub const MIN_ALTITUDE: i32 = 0x0131_2d00;
/// The one-in-five draw limit (`0x3332` of `0x10000`).
pub const SPAWN_LIMIT: u32 = 0x3332;
/// The 16-bit mask F1035 applies to each `F1530` word.
pub const SPAWN_MASK: u32 = 0xffff;
/// F769's age limit, `DATA_007706` at the 1x play value (`F217`).
pub const AGE_LIMIT: u32 = 0x3e2;
/// F769's age step per whole-day tick (`days << 8`).
pub const AGE_STEP: u32 = 0x100;
/// F769's distance-class cull (`+0x88 > 0x14`).
pub const DISTANCE_LIMIT: i32 = 0x14;
/// F1680's exponent base (`- 0xd`).
pub const DISTANCE_BASE: i32 = 0xd;
/// F1035's `+0x7e` jitter mask (`F1530 & 3`).
pub const DISTANCE_JITTER: u32 = 0x3;
/// F1517's base shift (`DATA_009123 + 7`).
pub const LEAD_SHIFT: u32 = 7;
/// `DATA_008865`: no routine in JJFFE 2.8a7 writes it (it follows the mouse
/// accumulators at `DATA_008863/864`), so it is zero.
pub const DISTANCE_BIAS: i32 = 0;
/// The original frame timer's minimum period (`tmMinFrame`): F1035 is called at
/// most once per 20 ms, i.e. 50 times a second.
pub const FRAME_SECONDS: f64 = 0.02;
/// The playable loop's clamp on one presented frame (`main`'s
/// `duration_since(last).min(0.1)`), used to bound [`Field::advance`] so a
/// pathological frame cannot queue thousands of attempts.
pub const MAX_FRAME_SECONDS: f64 = 0.1;
/// The same period in the integer microseconds the field accumulates, so a
/// 20 ms presentation frame is exactly one attempt.
const FRAME_MICROS: u64 = 20_000;
/// F152's day marker: one whole day per second at the original's 1x clock
/// (`DATA_008804` grows by `0x3e2` per 20 ms and a day is `0xc22e`).
const DAY_MICROS: u64 = 1_000_000;
/// `DATA_006258`'s capacity for dust: F928 never allocates below slot `0x14`,
/// so at most `0x72 + 1 - 0x14 = 0x5f` objects can exist.
pub const MAX_PARTICLES: usize = 0x5f;

/// Why F1035 did not spawn this frame, in the original's gate order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// `DATA_008867 < 0x1312d00`: below 20 km the source takes F1015's branch.
    BelowAltitude,
    /// `DATA_009018 == 0`: the "Space dust, clouds etc" option is off.
    DetailOff,
    /// `DATA_008630 >= 0`: not in the flight module.
    FlightInactive,
    /// The 16-bit `F1530` draw was at or above `0x3332`.
    NoDraw,
    /// `DATA_007706 == 0`: time is stopped (pause or video).
    TimeStopped,
}

/// One flight frame's inputs: the player's state and the three preference /
/// module gates. `position` and `velocity` are the values F1533 clones from the
/// player (the `[i64; 3]` position in millimetres and the `+0x8c` velocity
/// words); `altitude` is F1033's `DATA_008867`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawn {
    pub position: [i64; 3],
    pub velocity: [i64; 3],
    pub altitude: i32,
    pub detail: bool,
    pub flight_active: bool,
    pub time_running: bool,
    /// `DATA_009123`, zero in normal flight; F908 sets `0x12` for the
    /// hyperspace exit effect, which shifts the spawn offset farther and
    /// raises the distance bias.
    pub speed: u32,
}

impl Spawn {
    /// Deep space at the flight module's normal speed with the player's
    /// default "Space dust, clouds etc" setting on.
    pub fn deep_space(position: [i64; 3], velocity: [i64; 3], detail: bool) -> Self {
        Self {
            position,
            velocity,
            altitude: i32::MAX,
            detail,
            flight_active: true,
            time_running: true,
            speed: 0,
        }
    }
}

/// One spawned dust object. The source's fields are noted where they differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Particle {
    /// `+0x82`, always [`MODEL`].
    pub model: u32,
    /// `+0xff`, always [`AI_STATE`].
    pub state: u8,
    /// `+0x87`, always [`CRAFT`].
    pub craft: u8,
    /// `+0x3e`: the 64-bit system position, in millimetres. F715 never changes
    /// it; only the spawn offset does.
    pub position: [i64; 3],
    /// `+0x8c`: inherited from the player by F1533 and used only by F1035's
    /// `F1517` offset. F715 does not integrate it.
    pub velocity: [i64; 3],
    /// F1675's two angles: the first `F1530` word and `DATA_008820 & 0xffff`.
    /// [`Particle::orientation`] builds the matrix when a sine table is at hand.
    pub tilt: i32,
    pub spin: i32,
    /// `+0x7e`: F1035's distance-word jitter, subtracted in F1680.
    pub distance_bias: i32,
    /// `+0xb0`: F769's age accumulator (`days << 8` per day tick).
    pub age: u32,
    /// `+0x88`: F1680's exponential distance class, recomputed by [`Field::tick`].
    pub distance: i32,
    /// `+0x8b`: non-zero removes the object (the source's hidden flag).
    pub hidden: bool,
}

impl Particle {
    /// F1675's orientation matrix, the original's random YZT build.
    pub fn orientation(&self, table: &SineTable) -> [i32; 9] {
        build_yzt(table, self.tilt, self.spin)
    }

    /// F1680's class as of the last [`Field::tick`], for a draw or a capture.
    pub fn distance_class(&self) -> i32 {
        self.distance
    }
}

/// What one presented frame did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tick {
    /// The F1035 attempts this frame: one per whole 20 ms of frame time, with
    /// the remainder carried.
    pub attempts: u32,
    /// How many of those attempts spawned a particle.
    pub spawned: u32,
    /// How many particles were removed by F769 this frame.
    pub removed: u32,
}

/// F1680: `max(0, msb(largest |coordinate|) - 0xd) - bias`, as a signed byte
/// in the source. `F1657` ORs the absolute high dwords (or the low dwords when
/// all highs are zero), so the result is the largest component's bit index.
pub fn distance_class(position: [i64; 3], player: [i64; 3], bias: i32) -> i32 {
    let magnitude = (0..3).fold(0u64, |acc, axis| {
        let value = position[axis].wrapping_sub(player[axis]).unsigned_abs();
        let selected = if value >> 32 != 0 { value >> 32 } else { value };
        acc | selected
    });
    let msb = if magnitude == 0 {
        0
    } else {
        63 - magnitude.leading_zeros() as i32
    };
    (msb - DISTANCE_BASE).max(0) - bias
}

/// The playable's dust field: its own seeded stream plus the live particles.
#[derive(Debug, Clone)]
pub struct Field {
    rng: Random,
    particles: Vec<Particle>,
    /// F152's `DATA_008808` day marker: whole days not yet handed to F769. At
    /// the original's 1x clock one day passes per second (`DATA_008804` grows
    /// by `0x3e2` each 20 ms frame and a day is `0xc22e`), and the acceleration
    /// scales both the growth and F769's limit, so this is real time.
    day_carry: u64,
    /// The 20 ms F1035 cadence, so a presentation rate above the original's
    /// 50 fps cap cannot multiply the spawn attempts.
    frame_carry: u64,
}

impl Field {
    /// A field with its own stream. The seed is the caller's choice: the
    /// playable derives it from the system code.
    pub fn new(a: u32, b: u32) -> Self {
        Self {
            rng: Random::new(a, b),
            particles: Vec::new(),
            day_carry: 0,
            frame_carry: 0,
        }
    }

    /// Clear the particles and reseed, as F674/F909 do when a system is left.
    pub fn reset(&mut self, a: u32, b: u32) {
        self.rng = Random::new(a, b);
        self.particles.clear();
        self.day_carry = 0;
        self.frame_carry = 0;
    }

    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// The stream state, for a deterministic capture or a test.
    pub fn random(&self) -> Random {
        self.rng
    }

    /// F1035's single attempt. `Ok(index)` is a spawned particle; `Err` names
    /// the gate that returned. The draw is consumed before the stopped-time
    /// gate exactly as the source orders it.
    pub fn attempt(&mut self, spawn: &Spawn) -> Result<usize, Refusal> {
        if spawn.altitude < MIN_ALTITUDE {
            return Err(Refusal::BelowAltitude);
        }
        if !spawn.detail {
            return Err(Refusal::DetailOff);
        }
        if !spawn.flight_active {
            return Err(Refusal::FlightInactive);
        }
        if (self.rng.advance() & SPAWN_MASK) >= SPAWN_LIMIT {
            return Err(Refusal::NoDraw);
        }
        if !spawn.time_running {
            return Err(Refusal::TimeStopped);
        }
        let tilt = (self.rng.advance() & SPAWN_MASK) as i32;
        let spin = (self.rng.b & SPAWN_MASK) as i32;
        let jitter = (self.rng.advance() & DISTANCE_JITTER) as i32;
        // F1517 stores `velocity << (7 + DATA_009123)` into the 64-bit vector;
        // x86's `shl` uses the low five bits of the count.
        let shift = (LEAD_SHIFT + spawn.speed) & 31;
        let position = std::array::from_fn(|axis| {
            let offset = spawn.velocity[axis].wrapping_shl(shift);
            spawn.position[axis].wrapping_add(offset)
        });
        if self.particles.len() == MAX_PARTICLES {
            // F928 recycles the highest object flagged 0x20; the port's only
            // flagged objects are dust, so the oldest goes first.
            self.particles.remove(0);
        }
        let index = self.particles.len();
        let distance_bias = jitter + spawn.speed as i32 + DISTANCE_BIAS - 7;
        self.particles.push(Particle {
            model: MODEL,
            state: AI_STATE,
            craft: CRAFT,
            position,
            velocity: spawn.velocity,
            tilt,
            spin,
            distance_bias,
            age: 0,
            distance: distance_class(position, spawn.position, distance_bias),
            hidden: false,
        });
        Ok(index)
    }

    /// F769's whole-day tick and F1680's distance class. `elapsed` is real
    /// seconds (the time acceleration cancels out of F769's limit, see the
    /// module note), and `player` is the camera's absolute position.
    pub fn tick(&mut self, elapsed: f64, player: [i64; 3]) -> u32 {
        if elapsed.is_finite() && elapsed > 0.0 {
            self.day_carry += (elapsed * 1_000_000.0).round() as u64;
        }
        let days = self.day_carry / DAY_MICROS;
        self.day_carry %= DAY_MICROS;
        let days = days.min(u64::from(u32::MAX)) as u32;
        let before = self.particles.len();
        for particle in &mut self.particles {
            particle.age = particle.age.saturating_add(days.saturating_mul(AGE_STEP));
            particle.distance = distance_class(particle.position, player, particle.distance_bias);
        }
        self.particles.retain(|particle| {
            !particle.hidden && particle.age < AGE_LIMIT && particle.distance <= DISTANCE_LIMIT
        });
        (before - self.particles.len()) as u32
    }

    /// One presented flight frame: F1035's attempts, then F152/F769's day tick.
    /// The source's frame handler spawns inside the event-3 dispatch and F151's
    /// day tick follows later in the same F152 call, so a particle can be aged
    /// on its spawn frame. One attempt is made per elapsed 20 ms of the
    /// original's frame timer. The spawn offset and cull use `spawn.position`,
    /// the player's current position.
    pub fn advance(&mut self, elapsed: f64, spawn: &Spawn) -> Tick {
        let mut tick = Tick::default();
        let elapsed = if elapsed.is_finite() {
            elapsed.clamp(0.0, MAX_FRAME_SECONDS)
        } else {
            0.0
        };
        self.frame_carry += (elapsed * 1_000_000.0).round() as u64;
        while self.frame_carry >= FRAME_MICROS {
            self.frame_carry -= FRAME_MICROS;
            tick.attempts += 1;
            if self.attempt(spawn).is_ok() {
                tick.spawned += 1;
            }
        }
        tick.removed = self.tick(elapsed, spawn.position);
        tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::ai_state;

    /// A cache-free sine table: F1914's quarter wave with `0x10000` for 1.0.
    /// A cache-free sine table: F1914's Q31 quarter wave.
    fn sine_table() -> SineTable {
        let mut bytes = Vec::with_capacity(16384 * 4);
        for index in 0..16384 {
            let angle = std::f64::consts::FRAC_PI_2 * f64::from(index) / 16384.0;
            let value = (angle.sin() * 2_147_483_647.0).round() as i32;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SineTable::from_bytes(&bytes).unwrap()
    }

    fn spawn() -> Spawn {
        Spawn::deep_space([1_000_000, -2_000_000, 3_000_000], [0, 0, 0], true)
    }

    /// The first seed at or above `seed` whose first F1035 draw passes, so a
    /// test can start with exactly one particle.
    fn with_one(seed: u32, spawn: &Spawn) -> Field {
        let mut seed = seed;
        loop {
            let mut field = Field::new(seed, !seed);
            if field.attempt(spawn).is_ok() {
                return field;
            }
            seed = seed.wrapping_add(1);
        }
    }

    #[test]
    fn the_ai_table_row_is_the_dust_row() {
        let row = ai_state(AI_STATE).expect("state 0x1b is in DATA_006258");
        assert_eq!(row.name, "space dust");
        assert_eq!(row.control, Some(769));
        assert_eq!(row.movement, Some(715));
        assert_eq!(row.notify, None);
        assert_eq!(row.damage, None);
    }

    #[test]
    fn the_detail_and_module_gates_come_before_the_draw() {
        let mut field = Field::new(1, 2);
        let mut cold = spawn();
        // DATA_008867 must reach 0x1312d00.
        cold.altitude = MIN_ALTITUDE - 1;
        assert_eq!(field.attempt(&cold), Err(Refusal::BelowAltitude));
        cold.altitude = MIN_ALTITUDE;
        // The "Space dust, clouds etc" byte at +0x3f51.
        cold.detail = false;
        assert_eq!(field.attempt(&cold), Err(Refusal::DetailOff));
        cold.detail = true;
        // DATA_008630: 0xff in the flight module, 0x7f elsewhere.
        cold.flight_active = false;
        assert_eq!(field.attempt(&cold), Err(Refusal::FlightInactive));
        assert!(field.particles().is_empty());
        // None of those gates consumed a random draw.
        let untouched = Field::new(1, 2);
        let mut gated = Field::new(1, 2);
        let _ = gated.attempt(&cold);
        assert_eq!(untouched.random(), gated.random());
        // The stopped-time gate is after the draw: the source consumes the word
        // whether or not it goes on to spawn.
        let mut off = spawn();
        off.time_running = false;
        let mut stopped = Field::new(9, 10);
        let mut expected = stopped.random();
        expected.advance();
        let result = stopped.attempt(&off);
        assert!(
            matches!(result, Err(Refusal::TimeStopped | Refusal::NoDraw)),
            "{result:?}"
        );
        // Whichever of the two ran, exactly one F1530 word was drawn.
        assert_eq!(stopped.random(), expected);
    }

    #[test]
    fn the_spawn_is_one_draw_in_five_at_the_original_frame_cadence() {
        let mut field = Field::new(0x1234_5678, 0x4567_89ab);
        let mut attempts = 0;
        let mut spawned = 0;
        for _ in 0..10_000 {
            attempts += 1;
            // The first spawn moves the player's own position only; use a fresh
            // field condition each frame so the count is purely F1035's draw.
            if field.attempt(&spawn()).is_ok() {
                spawned += 1;
            }
        }
        // 0x3332 / 0x10000 = 0.1999; the fixed seed keeps this exact.
        assert_eq!(attempts, 10_000);
        assert!(
            (1_950..=2_050).contains(&spawned),
            "spawned {spawned} of {attempts}"
        );
        assert!(field.particles().len() <= MAX_PARTICLES);
    }

    #[test]
    fn one_frame_attempts_at_most_once_per_twenty_milliseconds() {
        let mut field = Field::new(3, 4);
        let spawn = spawn();
        let tick = field.advance(0.02, &spawn);
        assert_eq!(tick.attempts, 1);
        let tick = field.advance(0.1, &spawn);
        assert_eq!(tick.attempts, 5);
        let tick = field.advance(0.0, &spawn);
        assert_eq!(tick.attempts, 0);
        // A presentation rate below 50 fps keeps the source's cadence: two
        // 40 ms frames make two attempts, not one.
        let mut field = Field::new(5, 6);
        assert_eq!(field.advance(0.04, &spawn).attempts, 2);
        // The playable's frame clamp bounds a pathological delta.
        let mut field = Field::new(5, 6);
        assert_eq!(field.advance(10.0, &spawn).attempts, 5);
    }

    #[test]
    fn the_lifetime_is_four_whole_day_ticks() {
        let spawn = spawn();
        let mut field = with_one(7, &spawn);
        assert_eq!(field.particles().len(), 1);
        let player = spawn.position;
        // Three day ticks leave the 0x100 step at 0x300, below 0x3e2.
        for day in 0..3 {
            assert_eq!(field.tick(1.0, player), 0, "day {day}");
            assert_eq!(field.particles()[0].age, 0x100 * (day + 1));
        }
        // The fourth reaches 0x400 and F769 removes it.
        assert_eq!(field.tick(1.0, player), 1);
        assert!(field.particles().is_empty());
        // Sub-day frames do not age it: two half-second frames make one day.
        let mut field = with_one(7, &spawn);
        assert_eq!(field.tick(0.5, player), 0);
        assert_eq!(field.particles()[0].age, 0);
        assert_eq!(field.tick(0.5, player), 0);
        assert_eq!(field.particles()[0].age, 0x100);
    }

    #[test]
    fn dust_holds_its_position_because_f715_is_empty() {
        let mut spawn = spawn();
        spawn.velocity = [500, -250, 125];
        let mut field = with_one(11, &spawn);
        let before = field.particles()[0];
        // F1035's F1517 offset: velocity << 7 added to the inherited position.
        assert_eq!(
            before.position,
            [
                spawn.position[0] + 500 * 128,
                spawn.position[1] - 250 * 128,
                spawn.position[2] + 125 * 128
            ]
        );
        // F715 is empty: one day of ticks leaves the position and velocity as
        // F1533/F1035 wrote them.
        field.tick(1.0, spawn.position);
        assert_eq!(field.particles()[0].position, before.position);
        assert_eq!(field.particles()[0].velocity, [500, -250, 125]);
    }

    #[test]
    fn the_random_orientation_is_f1675() {
        let table = sine_table();
        let particle = with_one(13, &spawn()).particles()[0];
        assert!((0..=0xffff).contains(&particle.tilt));
        assert!((0..=0xffff).contains(&particle.spin));
        let matrix = particle.orientation(&table);
        assert_eq!(matrix, build_yzt(&table, particle.tilt, particle.spin));
        assert_eq!(matrix[7], 0);
        // Each row is a unit vector within the table's rounding.
        for row in 0..3 {
            let length = (matrix[row * 3] as f64).powi(2)
                + (matrix[row * 3 + 1] as f64).powi(2)
                + (matrix[row * 3 + 2] as f64).powi(2);
            let length = length.sqrt() / 2_147_483_648.0;
            assert!((length - 1.0).abs() < 1e-3, "row {row}: {length}");
        }
    }

    #[test]
    fn the_distance_class_is_f1680_and_culls_at_0x14() {
        // F1680/F1657: max(0, msb - 0xd) - +0x7e. The spawn bias is
        // (F1530 & 3) - 7, so it lies in -7..=-4.
        assert_eq!(distance_class([0; 3], [0; 3], -7), 7);
        assert_eq!(distance_class([1 << 24, 0, 0], [0; 3], -7), 18);
        assert_eq!(distance_class([1 << 26, 0, 0], [0; 3], -7), 20);
        assert_eq!(distance_class([1 << 27, 0, 0], [0; 3], -7), 21);
        // A particle at the player survives; one far behind is removed. The
        // cull distance depends on the spawn bias (class 20 is about 2^28 mm
        // at bias -4), so use a delta far beyond every bias.
        let spawn = spawn();
        let mut field = with_one(15, &spawn);
        let bias = field.particles()[0].distance_bias;
        assert!((-7..=-4).contains(&bias), "bias {bias}");
        assert_eq!(field.tick(0.0, spawn.position), 0);
        let far = [
            spawn.position[0] + (1 << 30),
            spawn.position[1],
            spawn.position[2],
        ];
        assert_eq!(field.tick(0.0, far), 1);
        assert!(field.particles().is_empty());
        // The hidden flag removes it too (`+0x8b`).
        let mut field = with_one(17, &spawn);
        field.particles[0].hidden = true;
        assert_eq!(field.tick(0.0, spawn.position), 1);
    }

    #[test]
    fn two_fields_with_one_seed_spawn_the_same_dust() {
        let mut first = Field::new(0xdead_beef, 0x0123_4567);
        let mut second = Field::new(0xdead_beef, 0x0123_4567);
        let spawn = spawn();
        let mut made = 0;
        for _ in 0..500 {
            if first.attempt(&spawn).is_ok() {
                made += 1;
                assert!(second.attempt(&spawn).is_ok());
            } else {
                assert!(second.attempt(&spawn).is_err());
            }
        }
        assert!(made > 0);
        assert_eq!(first.particles(), second.particles());
        assert_eq!(first.random(), second.random());
    }

    #[test]
    fn a_full_field_recycles_its_oldest_particle() {
        let mut field = Field::new(21, 22);
        while field.particles().len() < MAX_PARTICLES {
            field.attempt(&spawn()).ok();
        }
        let oldest = field.particles()[0];
        // Force one more spawn with a passing draw by reseeding until it lands.
        while field.attempt(&spawn()).is_err() {}
        assert_eq!(field.particles().len(), MAX_PARTICLES);
        assert_ne!(field.particles()[0], oldest);
    }

    #[test]
    fn a_reset_clears_the_system_and_reseeds() {
        let mut field = Field::new(23, 24);
        while field.attempt(&spawn()).is_err() {}
        field.reset(25, 26);
        assert!(field.particles().is_empty());
        assert_eq!(field.random(), Random::new(25, 26));
    }
}
