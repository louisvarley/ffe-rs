//! The original's keyboard accumulators.
//!
//! Three pieces of JJFFE 2.8a7 decide how long the flight code thinks a key has
//! been held:
//!
//! * The platform layer (`win32/w32input.cpp`, `sdl/sdlinput.cpp`,
//!   `InputKeybReadStates`) reports every key as up or down and keeps one *last
//!   key*: the key of an initial press, or of an auto-repeat once a key has been
//!   held past the repeat delay (300 ms, then every 30 ms, `ffewin.cfg`). When
//!   several keys report in one read the highest scancode wins.
//! * `Timer50Hz` (ffe.asm) runs once per 20 ms of real time from
//!   `FUNC_001418_TimerAdvance` and adds one to every non-zero entry of
//!   `DATA_KeyAccumTable` and to the last-key counter `DATA_007690`.
//! * `FUNC_001429_GuiReadAllInput`, once per presented frame, starts the
//!   last-key counter at 1 when the reported key differs from the one it tracks
//!   (`DATA_009181`), clears it (and forgets the key) once that key is up, and
//!   sets each key's accumulator to 1 on a press and 0 on a release.
//!
//! F1038 turns a steering key's accumulator into a turn rate
//! (`FUNC_CalcKeyInput`) and F1121 scales the set-speed step by the last-key
//! counter over 16 once it passes 0x10, so holding Return (or Shift) speeds the
//! set-speed change up after about a third of a second, and pressing any other
//! key restarts that.

/// The platform layer's repeat delay and rate in milliseconds (`ffewin.cfg`
/// `[KEYBOARD]`: `repeatdelay=300`, `repeatrate=30`).
pub const REPEAT_DELAY_MS: u32 = 300;
pub const REPEAT_RATE_MS: u32 = 30;
/// `Timer50Hz`'s period.
pub const TICK_MS: u32 = 20;
/// No key (`DATA_009181`'s and the platform's empty value).
pub const NO_KEY: u8 = 0xff;

/// A set of held scancodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanSet([u64; 4]);

impl ScanSet {
    pub fn insert(&mut self, code: u8) {
        self.0[usize::from(code >> 6)] |= 1 << (code & 63);
    }

    pub fn contains(&self, code: u8) -> bool {
        self.0[usize::from(code >> 6)] & (1 << (code & 63)) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }
}

impl FromIterator<u8> for ScanSet {
    fn from_iter<I: IntoIterator<Item = u8>>(codes: I) -> Self {
        let mut set = ScanSet::default();
        for code in codes {
            set.insert(code);
        }
        set
    }
}

/// The platform's per-key state (`pKeybStates`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Up,
    Held,
    Repeating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    /// The millisecond clock (`TimerGetTimeStamp`).
    now: u32,
    /// The next `Timer50Hz` time (`tmNext50Hz`).
    next_tick: u32,
    platform: [Platform; 256],
    /// `pKeybTimes`: the next repeat time of a held key.
    repeat_at: [u32; 256],
    /// `DATA_KeyAccumTable`.
    held: [u32; 256],
    /// `DATA_007690`.
    last_key_count: u32,
    /// `DATA_009181`.
    last_key: u8,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    /// `FUNC_001407_KeybInit` and `FUNC_001416_TimerInit`: nothing held, the
    /// clock and the 50 Hz schedule at zero.
    pub fn new() -> Self {
        Self {
            now: 0,
            next_tick: 0,
            platform: [Platform::Up; 256],
            repeat_at: [0; 256],
            held: [0; 256],
            last_key_count: 0,
            last_key: NO_KEY,
        }
    }

    /// Set the millisecond clock (`tmLastTime`) and the next 50 Hz time
    /// (`tmNext50Hz`), as a replay aligned with an executed trace needs.
    pub fn set_clock(&mut self, now: u32, next_tick: u32) {
        self.now = now;
        self.next_tick = next_tick;
    }

    /// `FUNC_001418_TimerAdvance`'s 50 Hz loop: move the clock on by `ms` and
    /// run `Timer50Hz` once for every 20 ms boundary reached (the original's
    /// `esi >= tmNext50Hz`). Returns the number of ticks run.
    pub fn advance(&mut self, ms: u32) -> u32 {
        self.now = self.now.wrapping_add(ms);
        let mut ticks = 0;
        while self.now.wrapping_sub(self.next_tick) as i32 >= 0 {
            self.next_tick = self.next_tick.wrapping_add(TICK_MS);
            self.tick();
            ticks += 1;
        }
        ticks
    }

    /// `Timer50Hz`: every running accumulator counts one more tick.
    fn tick(&mut self) {
        for count in &mut self.held {
            if *count != 0 {
                *count = count.wrapping_add(1);
            }
        }
        if self.last_key_count != 0 {
            self.last_key_count = self.last_key_count.wrapping_add(1);
        }
    }

    /// One input read: the platform's `InputKeybReadStates` with `down` the
    /// keys physically held now, then `GuiReadAllInput`'s last-key and
    /// accumulator updates, then `InputKeybSetLastKey(0xff)`.
    pub fn read(&mut self, down: &ScanSet) {
        let now = self.now;
        let mut reported = NO_KEY;
        for code in 0..=255u8 {
            let index = usize::from(code);
            if !down.contains(code) {
                self.platform[index] = Platform::Up;
                continue;
            }
            match self.platform[index] {
                Platform::Up => {
                    reported = code;
                    self.platform[index] = Platform::Held;
                    self.repeat_at[index] = now.wrapping_add(REPEAT_DELAY_MS);
                }
                Platform::Held if self.repeat_at[index] < now => {
                    self.platform[index] = Platform::Repeating;
                }
                _ => {}
            }
            if self.platform[index] == Platform::Repeating && self.repeat_at[index] < now {
                reported = code;
                self.repeat_at[index] = self.repeat_at[index].wrapping_add(REPEAT_RATE_MS);
            }
        }
        if reported != NO_KEY && reported != self.last_key {
            self.last_key_count = 1;
            self.last_key = reported;
        }
        if !down.contains(self.last_key) {
            self.last_key_count = 0;
            self.last_key = NO_KEY;
        }
        for code in 0..=255u8 {
            let count = &mut self.held[usize::from(code)];
            if !down.contains(code) {
                *count = 0;
            } else if *count == 0 {
                *count = 1;
            }
        }
    }

    /// `DATA_KeyAccumTable[code]`: zero while the key is up, otherwise one
    /// plus the 50 Hz ticks since the read that saw it pressed.
    pub fn held(&self, code: u8) -> u32 {
        self.held[usize::from(code)]
    }

    /// `DATA_007690`, the last-key counter F1121 reads.
    pub fn last_key_count(&self) -> u32 {
        self.last_key_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(codes: &[u8]) -> ScanSet {
        codes.iter().copied().collect()
    }

    /// After `FUNC_001416_TimerInit`: the first advance has run its tick.
    fn started() -> Keyboard {
        let mut keyboard = Keyboard::new();
        keyboard.advance(0);
        keyboard
    }

    /// One 20 ms frame in the original's order: the read at the end of the
    /// previous frame, then this frame's timer advance.
    fn frame(keyboard: &mut Keyboard, down: &[u8]) {
        keyboard.read(&set(down));
        keyboard.advance(20);
    }

    #[test]
    fn a_held_key_counts_from_one_plus_a_tick_per_20_ms() {
        let mut keyboard = started();
        frame(&mut keyboard, &[0x1c]);
        assert_eq!(keyboard.held(0x1c), 2);
        assert_eq!(keyboard.last_key_count(), 2);
        for expected in 3..40 {
            frame(&mut keyboard, &[0x1c]);
            assert_eq!(keyboard.held(0x1c), expected);
            // Return's own repeats report the same key: no restart.
            assert_eq!(keyboard.last_key_count(), expected);
        }
        frame(&mut keyboard, &[]);
        assert_eq!((keyboard.held(0x1c), keyboard.last_key_count()), (0, 0));
    }

    #[test]
    fn a_new_key_restarts_the_last_key_counter_and_its_release_clears_it() {
        let mut keyboard = started();
        for _ in 0..10 {
            frame(&mut keyboard, &[0x1c]);
        }
        assert_eq!(keyboard.last_key_count(), 11);
        frame(&mut keyboard, &[0x1c, 0x1e]);
        assert_eq!(keyboard.last_key_count(), 2);
        // Return keeps its own accumulator.
        assert_eq!(keyboard.held(0x1c), 12);
        // A released: Return, still held, is not the last key until it repeats.
        for _ in 0..5 {
            frame(&mut keyboard, &[0x1c]);
            assert_eq!(keyboard.last_key_count(), 0);
        }
        assert_eq!(keyboard.held(0x1c), 17);
        // Its repeat (300 ms after the press at 0 ms) is seen by the read at
        // 320 ms and starts the counter again.
        frame(&mut keyboard, &[0x1c]);
        assert_eq!(keyboard.last_key_count(), 2);
    }

    #[test]
    fn two_repeating_keys_keep_restarting_the_counter() {
        let mut keyboard = started();
        frame(&mut keyboard, &[0x1c]);
        for _ in 0..40 {
            frame(&mut keyboard, &[0x1c, 0x2a]);
        }
        let mut most = 0;
        for _ in 0..20 {
            frame(&mut keyboard, &[0x1c, 0x2a]);
            most = most.max(keyboard.last_key_count());
        }
        assert!(most < 0x10, "{most}");
        assert_eq!(keyboard.held(0x2a), 61);
    }

    #[test]
    fn ticks_follow_20_ms_boundaries_not_frames() {
        let mut keyboard = Keyboard::new();
        assert_eq!(keyboard.advance(0), 1);
        assert_eq!(keyboard.advance(19), 0);
        assert_eq!(keyboard.advance(1), 1);
        assert_eq!(keyboard.advance(45), 2);
        assert_eq!(keyboard.advance(15), 1);
    }
}
