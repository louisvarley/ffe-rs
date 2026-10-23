//! Native simulation primitives: no I/O, wall clock, GPU, or legacy runtime.

pub mod model;
pub mod model_animation;
pub mod model_commands;
pub mod model_depth;
pub mod model_execute;
pub mod model_fill;
pub mod model_instance;
pub mod model_lighting;
pub mod model_line;
pub mod model_math;
pub mod model_normals;
pub mod model_order;
pub mod model_polygon;
pub mod model_surface;
pub mod model_texture;
pub mod model_thruster;
pub mod thrust;

/// JJFFE's two-word generator. The returned low 16 bits are not the full state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomState {
    pub a: u32,
    pub b: u32,
}

impl RandomState {
    pub const fn new(a: u32, b: u32) -> Self {
        Self { a, b }
    }

    pub fn next_u16(&mut self) -> u16 {
        let previous_a = self.a;
        self.a = self.a.wrapping_add((self.b ^ 0x1234_567f).rotate_right(7));
        self.b = previous_a.rotate_left(13);
        self.a as u16
    }
}

/// The game's day fraction is a complete wrapping 32-bit word, not a count of
/// 49,710 ticks per day. A normal 20 ms timer increment is 994 fraction units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameClock {
    pub days: u32,
    pub fraction: u32,
}

impl GameClock {
    pub fn advance(&mut self, delta: u32) {
        let (fraction, carry) = self.fraction.overflowing_add(delta);
        self.fraction = fraction;
        self.days = self.days.wrapping_add(u32::from(carry));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    /// The legacy unsigned DIV would overflow its 32-bit quotient.
    DeltaOverflow,
}

impl std::fmt::Display for TimerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("timer delta exceeds legacy 32-bit quotient")
    }
}

impl std::error::Error for TimerError {}

/// Counter arithmetic only, not the OS timer, sleep loop, 50 Hz input scan,
/// acceleration policy or frame-rate selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimerCounters {
    pub system: u32,
    pub accelerated: u32,
}

pub fn timer_delta(elapsed_ms: u32, acceleration: u32) -> Result<u32, TimerError> {
    let delta = u64::from(elapsed_ms) * u64::from(acceleration) / 20;
    u32::try_from(delta).map_err(|_| TimerError::DeltaOverflow)
}

impl TimerCounters {
    pub fn advance(&mut self, elapsed_ms: u32, acceleration: u32) -> Result<(), TimerError> {
        let system_delta = timer_delta(elapsed_ms, 994)?;
        let accelerated_delta = timer_delta(elapsed_ms, acceleration)?;
        self.system = self.system.wrapping_add(system_delta);
        self.accelerated = self.accelerated.wrapping_add(accelerated_delta);
        Ok(())
    }
}
