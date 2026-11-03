//! Native simulation primitives: no I/O, wall clock, GPU, or legacy runtime.

pub mod autopilot;
pub mod bulletin;
pub mod calendar;
pub mod collision;
pub mod dust;
pub mod effects;
pub mod equipment;
mod equipment_table;
pub mod ffp;
pub mod frames;
pub mod galaxy;
pub mod handcoded;
pub mod hmp;
pub mod hyperspace;
pub mod landing;
pub mod laser;
pub mod lift;
pub mod market;
pub mod message_strings;
pub mod mining;
pub mod missile;
pub mod missions;
pub mod model;
pub mod model_animation;
pub mod model_commands;
pub mod model_depth;
pub mod model_dot;
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
pub mod names;
pub mod npc;
pub mod objects;
pub mod pads;
pub mod physics;
pub mod player;
pub mod sector;
pub mod shipyard;
mod shipyard_table;
pub mod star_points;
pub mod status;
pub mod surface;
pub mod sysgen;
pub mod terrain;
pub mod terrain_draw;
pub mod terrain_emit;
pub mod terrain_haze;
pub mod thrust;
pub mod traffic;
pub mod x87;

/// JJFFE's two-word generator. The returned low 16 bits are not the full state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// The original's time units per second at 1x (`0x3e2` per 20 ms).
pub const TICS_PER_SECOND: f64 = 49_700.0;
/// The 1x acceleration word (`DATA_007706` = `0x3e2`).
pub const REAL_TIME: u32 = 0x3e2;

/// One frame's time as F1418 hands it to the game (PAR-12): the whole
/// milliseconds measured, the system and accelerated tic deltas (each
/// `ms * word / 20`, truncated per frame as the source's DIV does) and the
/// number of 50 Hz callbacks (`Timer50Hz`) the frame crossed. Every
/// gameplay system in a frame reads this one value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frame {
    pub ms: u32,
    pub real_tics: u32,
    pub tics: u32,
    pub ticks_50hz: u32,
}

impl Frame {
    /// The accelerated tics in seconds of game time.
    pub fn seconds(&self) -> f64 {
        f64::from(self.tics) / TICS_PER_SECOND
    }

    /// The system tics in seconds of real time.
    pub fn real_seconds(&self) -> f64 {
        f64::from(self.real_tics) / TICS_PER_SECOND
    }
}

/// F1418's frame clock: the timestamp in whole milliseconds (the presented
/// frame's fraction of a millisecond carries to the next frame, as the
/// source's `tmLastTime` does), `tmNext50Hz` and the two counters.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameTimer {
    pub counters: TimerCounters,
    now_ms: u64,
    next_50hz_ms: u64,
    carry_ms: f64,
}

impl FrameTimer {
    /// Advance by `seconds` of presented time at the acceleration word
    /// `acceleration` (`DATA_007706`, [`REAL_TIME`] for 1x).
    pub fn advance(&mut self, seconds: f64, acceleration: u32) -> Frame {
        let total = self.carry_ms
            + if seconds.is_finite() {
                seconds.max(0.0) * 1000.0
            } else {
                0.0
            };
        // A whole millisecond lost to float rounding is not lost.
        let ms = (total + 1e-9).floor().min(f64::from(u32::MAX / REAL_TIME));
        self.carry_ms = (total - ms).max(0.0);
        let ms = ms as u32;
        self.now_ms += u64::from(ms);
        let mut ticks_50hz = 0;
        while self.now_ms >= self.next_50hz_ms + 20 {
            self.next_50hz_ms += 20;
            ticks_50hz += 1;
        }
        let real_tics = timer_delta(ms, REAL_TIME).unwrap_or(u32::MAX);
        let tics = timer_delta(ms, acceleration).unwrap_or(u32::MAX);
        let _ = self.counters.advance(ms, acceleration);
        Frame {
            ms,
            real_tics,
            tics,
            ticks_50hz,
        }
    }
}

#[cfg(test)]
mod frame_timer_tests {
    use super::*;

    fn run(fps: f64, seconds: f64, acceleration: u32) -> (u64, u64, u64) {
        let mut timer = FrameTimer::default();
        let frames = (seconds * fps).round() as usize;
        let (mut tics, mut ms, mut ticks) = (0u64, 0u64, 0u64);
        for _ in 0..frames {
            let frame = timer.advance(1.0 / fps, acceleration);
            tics += u64::from(frame.tics);
            ms += u64::from(frame.ms);
            ticks += u64::from(frame.ticks_50hz);
        }
        (ms, tics, ticks)
    }

    #[test]
    fn a_twenty_ms_frame_is_994_tics() {
        let mut timer = FrameTimer::default();
        let frame = timer.advance(0.02, REAL_TIME);
        assert_eq!(
            (frame.ms, frame.real_tics, frame.tics, frame.ticks_50hz),
            (20, 994, 994, 1)
        );
        let fast = timer.advance(0.02, 0x26d6);
        assert_eq!((fast.real_tics, fast.tics), (994, 0x26d6));
        assert!((frame.seconds() - 0.02).abs() < 1e-12);
    }

    #[test]
    fn every_frame_rate_sees_the_same_milliseconds_and_50hz_callbacks() {
        // Ten seconds at 20, 50, 60 and 144 fps: the same whole milliseconds and
        // the same 500 Timer50Hz calls; the tic total differs only by F1418's
        // per-frame truncation (under one tic a frame).
        for fps in [20.0, 50.0, 60.0, 144.0] {
            let (ms, tics, ticks) = run(fps, 10.0, REAL_TIME);
            assert_eq!(ms, 10_000, "{fps} fps");
            assert_eq!(ticks, 500, "{fps} fps");
            let frames = (10.0 * fps) as u64;
            assert!(497_000 - tics <= frames, "{fps} fps: {tics}");
        }
    }
}
