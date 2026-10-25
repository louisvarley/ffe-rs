//! The whole-day marker behind the source's once-a-day updates.
//!
//! The original advances a frame's clock fraction in `DATA_008804` and compares
//! it with the last fraction handed to the day handlers (`DATA_008808`,
//! `ffe.asm:15196-15221`). The difference divided by `0xc22e` is the number of
//! whole days that have passed; the marker moves by exactly that many days, so
//! each day is dispatched once no matter how many frames or how much time
//! acceleration produced it. This type is that arithmetic without the
//! fractional word: callers pass the whole day their clock has reached and get
//! the days that are newly due, in order.

/// How many of the original's clock ticks make one day (`0xc22e`).
pub const TICKS_PER_DAY: u32 = 0xc22e;

/// The last whole day whose periodic updates have run (`DATA_008808`'s day
/// equivalent).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Calendar {
    day: u32,
}

impl Calendar {
    /// A marker that has already processed `day`: a new game starts at day 0,
    /// and a loaded commander starts at whatever day the clock says.
    pub const fn at(day: u32) -> Self {
        Self { day }
    }

    /// The last day processed.
    pub const fn day(self) -> u32 {
        self.day
    }

    /// The days newly crossed when the clock reaches `day`, `start..end` with
    /// `end` exclusive. The marker never moves backwards: a clock that has gone
    /// back (a reset) produces no days instead of replaying old ones.
    ///
    /// One call per frame, exactly like F152's `(DATA_008804 - DATA_008808) /
    /// 0xc22e`; time acceleration and a hyperspace jump simply make the range
    /// longer, and it is still each day once.
    pub fn advance(&mut self, day: u32) -> std::ops::Range<u32> {
        if day <= self.day {
            return self.day..self.day;
        }
        let start = self.day.saturating_add(1);
        self.day = day;
        start..day.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_crossed_at_the_boundary_once() {
        let mut calendar = Calendar::at(0);
        // The clock has not reached day 1 yet.
        assert_eq!(calendar.advance(0), 0..0);
        assert_eq!(calendar.day(), 0);
        // 86,399 s is still day 0; 86,400 s is day 1.
        assert_eq!(calendar.advance(0), 0..0);
        assert_eq!(calendar.advance(1), 1..2);
        assert_eq!(calendar.day(), 1);
        // Another call at the same day does not replay it.
        assert_eq!(calendar.advance(1), 1..1);
    }

    #[test]
    fn acceleration_and_long_jumps_return_every_day_in_order() {
        let mut calendar = Calendar::at(3);
        assert_eq!(calendar.advance(6), 4..7);
        assert_eq!(calendar.day(), 6);
        // A jump of several days after a wait: no day is skipped or doubled.
        assert_eq!(calendar.advance(10), 7..11);
        assert_eq!(calendar.advance(10), 10..10);
    }

    #[test]
    fn many_small_frames_equal_one_long_advance() {
        let mut frame_by_frame = Calendar::at(0);
        let mut seen = Vec::new();
        // 600 frames of 288 s each cross the two day boundaries; the same two
        // days are dispatched as one 2-day call.
        for frame in 0..=600u32 {
            let day = (frame * 288) / 86_400;
            seen.extend(frame_by_frame.advance(day));
        }
        let mut at_once = Calendar::at(0);
        assert_eq!(seen, at_once.advance(2).collect::<Vec<_>>());
    }

    #[test]
    fn a_clock_that_goes_back_does_not_replay_days() {
        let mut calendar = Calendar::at(5);
        assert_eq!(calendar.advance(2), 5..5);
        assert_eq!(calendar.day(), 5);
        assert_eq!(calendar.advance(6), 6..7);
    }
}
