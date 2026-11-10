//! F152's whole-second events (`ffe.asm:15176-15221`).
//!
//! Every frame of F919's loop, F152 divides the day fraction's advance since
//! the last dispatch, `DATA_008804 - DATA_008808` (unsigned, wrapping), by
//! `0xc22e`. That many events are due: the marker moves on by exactly
//! `count * 0xc22e`, module event `0xa` and F151's object event 6 (F1007, which
//! runs F938's recharge, repair and thrust energy) receive the count, and the
//! remainder waits for a later frame. The day fraction itself advances by the
//! accelerated tics (`DATA_009122`, `0x3e2` per 20 ms at 1x), so `0xc22e`
//! (49,710) is one game second of the 2^32-unit day.
//!
//! The source's quirks are kept:
//! - the new-game set-up (`ffe.asm:14928-14932`) puts the fraction at
//!   `0x80000000` (noon) and the marker at 0, so the first F152 dispatches
//!   43,200 seconds of events at once ([`SecondEvents::new_game`]);
//! - every selection of a 10x..10000x step in F208 (`JUMP_001333..1336`) pulls
//!   the marker back by one event ([`SecondEvents::select_acceleration`]);
//! - the marker is part of the 0x556a-byte player block F166/F167 save, so a
//!   saved game keeps its pending fraction.
//!
//! The type stores the pending advance `fraction - marker` rather than the two
//! words: the port's clock is kept elsewhere, and the difference is all F152
//! reads.

/// The day-fraction units per event (`0xc22e`, one game second).
pub const TICS_PER_EVENT: u32 = 0xc22e;

/// The pending advance after the new-game set-up: fraction `0x80000000`,
/// marker 0.
pub const NEW_GAME_PENDING: u32 = 0x8000_0000;

/// `DATA_008804 - DATA_008808`: the day-fraction advance not yet dispatched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SecondEvents {
    pending: u32,
}

impl SecondEvents {
    /// The state the new-game set-up leaves (43,200 events due).
    pub const fn new_game() -> Self {
        Self {
            pending: NEW_GAME_PENDING,
        }
    }

    /// A state with `pending` fraction units not yet dispatched.
    pub const fn with_pending(pending: u32) -> Self {
        Self { pending }
    }

    /// The fraction units not yet dispatched.
    pub const fn pending(self) -> u32 {
        self.pending
    }

    /// One F152 call after the day fraction advanced by `tics`: the number of
    /// whole-second events now due. The remainder is kept.
    pub fn advance(&mut self, tics: u32) -> u32 {
        self.pending = self.pending.wrapping_add(tics);
        let count = self.pending / TICS_PER_EVENT;
        self.pending -= count * TICS_PER_EVENT;
        count
    }

    /// F208 selecting a 10x, 100x, 1000x or 10000x step: `DATA_008808 -=
    /// 0xc22e`, so one more event is due at the next F152. Selecting 1x does
    /// not move the marker.
    pub fn select_acceleration(&mut self, step: usize) {
        if (1..=4).contains(&step) {
            self.pending = self.pending.wrapping_add(TICS_PER_EVENT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_game_dispatches_twelve_hours_on_the_first_call() {
        let mut events = SecondEvents::new_game();
        // 0x80000000 / 0xc22e = 43,200, leaving 0x2d80.
        assert_eq!(events.advance(0), 43_200);
        assert_eq!(events.pending(), 0x8000_0000 - 43_200 * TICS_PER_EVENT);
        assert_eq!(events.advance(0), 0);
    }

    #[test]
    fn one_event_per_0xc22e_units_not_per_49700_tics() {
        // Fifty 20 ms frames at 1x are 49,700 units: one short of an event.
        let mut events = SecondEvents::default();
        let mut total = 0;
        for _ in 0..50 {
            total += events.advance(0x3e2);
        }
        assert_eq!(total, 0);
        assert_eq!(events.pending(), 49_700);
        assert_eq!(events.advance(10), 1);
        assert_eq!(events.pending(), 0);
        // A whole minute of frames: 60 * 49,700 = 2,982,000 units, 59 events.
        let mut events = SecondEvents::default();
        let total: u32 = (0..3_000).map(|_| events.advance(0x3e2)).sum();
        assert_eq!(total, 2_982_000 / TICS_PER_EVENT);
        assert_eq!(total, 59);
    }

    #[test]
    fn many_frames_dispatch_what_one_long_frame_does() {
        let mut stepped = SecondEvents::with_pending(123);
        let mut sum = 0;
        for _ in 0..1_000 {
            sum += stepped.advance(0x26d6 * 7);
        }
        let mut once = SecondEvents::with_pending(123);
        assert_eq!(once.advance(0x26d6 * 7 * 1_000), sum);
        assert_eq!(once, stepped);
    }

    #[test]
    fn the_pending_advance_wraps_like_the_source_subtraction() {
        let mut events = SecondEvents::with_pending(u32::MAX - 5);
        // The wrapped sum is small: no spurious events.
        assert_eq!(events.advance(10), 0);
        assert_eq!(events.pending(), 4);
    }

    #[test]
    fn selecting_a_faster_step_brings_one_event_forward() {
        for step in 1..=4 {
            let mut events = SecondEvents::with_pending(5);
            events.select_acceleration(step);
            assert_eq!(events.advance(0), 1, "step {step}");
            assert_eq!(events.pending(), 5);
        }
        let mut events = SecondEvents::with_pending(5);
        events.select_acceleration(0);
        events.select_acceleration(5);
        assert_eq!(events.advance(0), 0);
    }
}
