//! The original message log (`F271`, `F272`, `F273`; `ffe.asm:20623-21232`).
//!
//! The console's right-hand text window is not a one-line toast: `F271` keeps the
//! last ten messages in a ring at `DATA_008681` (stride 0x40e, one 1025-byte
//! buffer each), each stamped with the game date and time (`DATA_008678/79`) and
//! a flag word (`DATA_008682`).
//!
//! `F272` builds one string: the current date/time header at the top, then the
//! newest entry **without** a date (the header is its date), then each older
//! entry as its own date/time line followed by the text. `F271` has already
//! wrapped each stored entry at 0x78 (120) pixels and inserted the `\r` breaks;
//! `F272` wraps the assembled string at 0x7c (124) pixels. It then draws an
//! 8-pixel-line shadow at x 195, y 158 minus the scroll and the text at x 196,
//! y 159 minus the scroll, clipped to y 158..199. The window is **top-anchored**:
//! line 0 (the header) sits at y 159, and scrolling moves the whole block up to
//! reveal older entries. `F273` moves the scroll with actions 0xe/0xf (clamped at
//! 0x18 pixels) and auto-scrolls back to the newest entry after 0x3cae6 ticks.
//!
//! The port formats the stamp with [`crate::galaxy_map::date_text`] (the same
//! calendar the maps use) and keeps the original's fixed 10-entry ring, 120-pixel
//! entry wrap, 124-pixel display wrap, line height, window position and scroll
//! clamp. It uses the port's text colours (15 for entries, 17 for the header, 4
//! for the shadow) because the original injects `0x01 0x04`/`0x01 0x06` colours
//! through `F1399` and the runtime upper palette is not reproduced
//! (`docs/STATUS.md` item 2).

use std::collections::VecDeque;

/// The original's ten entry buffers (`DATA_008681`, ten records of 0x40e bytes).
pub const ENTRIES: usize = 10;
/// `F271` wraps each stored entry at 0x78 pixels (`ffe.asm:20652-20655`).
pub const ENTRY_WRAP: usize = 0x78;
/// `F272` wraps the assembled header/entries at 0x7c pixels
/// (`ffe.asm:21040-21043`).
pub const WRAP: usize = 0x7c;
/// `F1586`'s line height for the console (8 pixels).
pub const LINE_HEIGHT: usize = 8;
/// The header/text origin: x 196, y 159, with the shadow one pixel left and up.
pub const TEXT_X: usize = 196;
pub const TEXT_Y: usize = 159;
pub const SHADOW_X: usize = 195;
pub const SHADOW_Y: usize = 158;
/// `F272` clips the text to the console panel (`ymin 158`, `ymax -1`).
pub const CLIP_TOP: usize = 158;
pub const CLIP_BOTTOM: usize = 200;
/// The five 8-pixel rows from y 159 to y 199.
pub const VISIBLE_ROWS: usize = (CLIP_BOTTOM - TEXT_Y) / LINE_HEIGHT;
/// `F273` clamps the scroll to 0x18 pixels.
pub const MAX_SCROLL: i32 = 0x18;
/// `F273` auto-scrolls to the newest entry after 0x3cae6 tics.
pub const AUTO_SCROLL_TICS: u32 = 0x3cae6;

/// One stored message: its text and the in-game date/time stamp `F271` keeps
/// beside it (`DATA_008678/79`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub stamp: String,
}

/// One drawn line and the colour group it belongs to: the current date/time
/// header and each older entry's date line use `0x01 0x04`, the entry text
/// `0x01 0x06` (`ffe.asm:20814-21018`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub header: bool,
}

/// The ten-entry ring (`DATA_008681`), the scroll word (`DATA_005096`) and its
/// animation/auto-scroll state (`DATA_005092`/`DATA_008677`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageLog {
    entries: VecDeque<Entry>,
    /// Pixels the content is scrolled up from the newest position.
    scroll: i32,
    /// Tics since the player last scrolled; `F273` returns to the newest at
    /// `0x3cae6`.
    idle: u32,
}

impl MessageLog {
    /// An empty log; the header date/time still draws.
    pub fn new() -> Self {
        Self::default()
    }

    /// `F271` (`flag 0`): append one entry, dropping the oldest beyond ten and
    /// returning to the newest.
    pub fn push(&mut self, text: impl Into<String>, stamp: impl Into<String>) {
        self.insert(text, stamp, false);
    }

    /// `F271` with flag word bit 0 set (`ffe.asm:20776-20792`): overwrite the
    /// newest entry instead of adding one. The original uses this for a message
    /// that is still being updated (an autopilot range, a speed readout), so a
    /// changing value does not fill the ring with near-identical lines.
    pub fn update(&mut self, text: impl Into<String>, stamp: impl Into<String>) {
        self.insert(text, stamp, true);
    }

    fn insert(&mut self, text: impl Into<String>, stamp: impl Into<String>, replace: bool) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let entry = Entry {
            text,
            stamp: stamp.into(),
        };
        if replace {
            if let Some(last) = self.entries.back_mut() {
                *last = entry;
            } else {
                self.entries.push_back(entry);
            }
        } else {
            // A repeated assignment of the same message is not a new event.
            if self
                .entries
                .back()
                .is_some_and(|last| last.text == entry.text)
            {
                return;
            }
            if self.entries.len() == ENTRIES {
                self.entries.pop_front();
            }
            self.entries.push_back(entry);
        }
        self.scroll = 0;
        self.idle = 0;
    }

    pub fn entries(&self) -> &VecDeque<Entry> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The scroll offset in pixels, 0 at the newest (top) position.
    pub fn scroll(&self) -> i32 {
        self.scroll
    }

    /// `F273`: scroll the log up or down by `pixels`, clamped to the content and
    /// the original's 0x18-pixel limit. Any manual scroll restarts the idle
    /// timer. Returns whether the offset changed. Positive values move the text
    /// up and reveal older entries; the window is top-anchored like `F272`.
    pub fn scroll_by(&mut self, pixels: i32, content_height: usize, _window: usize) -> bool {
        let max = self.max_scroll(content_height);
        let next = (self.scroll + pixels).clamp(0, max);
        let changed = next != self.scroll;
        self.scroll = next;
        self.idle = 0;
        changed
    }

    /// The furthest the content can scroll: `F273` allows up to
    /// `line_count * 8 - 8`, capped at its 0x18. `window` is accepted by
    /// `scroll_by` for the callers/tests that still pass it but does not limit
    /// the original's scroll.
    pub fn max_scroll(&self, content_height: usize) -> i32 {
        let last_line = content_height.saturating_sub(LINE_HEIGHT) as i32;
        last_line.min(MAX_SCROLL)
    }

    /// `F273`'s auto-scroll: after 0x3cae6 tics with no manual scroll, return to
    /// the newest entry.
    pub fn tick(&mut self, tics: u32) {
        if self.scroll == 0 {
            return;
        }
        self.idle = self.idle.saturating_add(tics);
        if self.idle >= AUTO_SCROLL_TICS {
            self.scroll = 0;
            self.idle = 0;
        }
    }

    /// `F272`'s assembled lines, top to bottom. The first line is `clock` (or
    /// the newest entry's stamp when the port has no clock yet), the newest entry
    /// follows without a stamp, and every older entry gets its own date/time line
    /// (`ffe.asm:20840-21018`). Entry text is wrapped at 120 pixels as `F271`
    /// stored it; the display wrap is 124.
    pub fn lines(&self, font: &crate::indexed::BitmapFont, clock: &str) -> Vec<Line> {
        let mut lines = Vec::new();
        let header = if clock.is_empty() {
            self.entries.back().map_or("", |entry| entry.stamp.as_str())
        } else {
            clock
        };
        if !header.is_empty() {
            lines.extend(
                wrap(font, header, WRAP)
                    .into_iter()
                    .map(|text| Line { text, header: true }),
            );
        }
        for (index, entry) in self.entries.iter().rev().enumerate() {
            if index == 0 {
                // The current header is this entry's date/time; `F272` skips its
                // stamp (`[ebp-0x4]` starts at 1).
                lines.extend(
                    wrap(font, &entry.text, ENTRY_WRAP)
                        .into_iter()
                        .map(|text| Line {
                            text,
                            header: false,
                        }),
                );
                continue;
            }
            if !entry.stamp.is_empty() {
                lines.extend(
                    wrap(font, &entry.stamp, WRAP)
                        .into_iter()
                        .map(|text| Line { text, header: true }),
                );
            }
            lines.extend(
                wrap(font, &entry.text, ENTRY_WRAP)
                    .into_iter()
                    .map(|text| Line {
                        text,
                        header: false,
                    }),
            );
        }
        lines
    }

    /// The first line index and its pixel offset for the current scroll. `F272`
    /// draws line 0 at `TEXT_Y - scroll`, so the window starts at the top.
    pub fn first_line(&self) -> (usize, i32) {
        let scroll = self.scroll.max(0) as usize;
        (scroll / LINE_HEIGHT, (scroll % LINE_HEIGHT) as i32)
    }
}

/// `F1398`/`StringWrap`: break at spaces to keep each line within `width`
/// pixels. Words longer than the window are hard-broken, as the original's
/// back-up-to-the-last-space loop does.
pub fn wrap(font: &crate::indexed::BitmapFont, text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if font
            .width(&candidate)
            .is_ok_and(|measured| measured <= width)
        {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // A word alone can still be too wide: break it at the width.
        let mut rest = word;
        while font.width(rest).is_ok_and(|measured| measured > width) {
            let mut cut = rest.len();
            while cut > 1 && font.width(&rest[..cut]).is_ok_and(|m| m > width) {
                cut -= 1;
            }
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            if cut == 0 {
                break;
            }
            lines.push(rest[..cut].to_owned());
            rest = &rest[cut..];
        }
        line = rest.to_owned();
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> crate::indexed::BitmapFont {
        // The fixed 4-pixel test font `tests/indexed.rs` uses.
        let mut bytes = vec![0; 1160];
        for record in bytes.chunks_exact_mut(10) {
            record[0] = 0x81;
            record[1] = 0x40;
            record[8] = 0x20;
            record[9] = 4;
        }
        crate::indexed::BitmapFont::new(&bytes).unwrap()
    }

    #[test]
    fn keeps_the_last_ten_entries() {
        let mut log = MessageLog::new();
        for n in 0..12 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        assert_eq!(log.len(), ENTRIES);
        assert_eq!(log.entries()[0].text, "message 2");
        assert_eq!(log.entries()[9].text, "message 11");
    }

    #[test]
    fn a_repeated_assignment_is_one_entry() {
        let mut log = MessageLog::new();
        log.push("Docked.", "1-Jan-3250");
        log.push("Docked.", "1-Jan-3250");
        log.push("Docked.", "1-Jan-3250");
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn a_replacement_updates_the_newest_entry() {
        let mut log = MessageLog::new();
        log.push("Autopilot to Mars.", "1-Jan-3250 12:00:00");
        log.update("Autopilot: Mars 2 Mm.", "1-Jan-3250 12:00:01");
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].text, "Autopilot: Mars 2 Mm.");
        assert_eq!(log.entries()[0].stamp, "1-Jan-3250 12:00:01");
    }

    #[test]
    fn the_boot_log_is_the_header_and_one_undated_entry() {
        let font = font();
        let mut log = MessageLog::new();
        log.push("Landed at Olympus Village.", "1-Jan-3250 12:26:56");
        let lines = log.lines(&font, "1-Jan-3250 12:26:56");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].header);
        assert_eq!(lines[0].text, "1-Jan-3250 12:26:56");
        assert!(!lines[1].header);
        assert_eq!(lines[1].text, "Landed at Olympus Village.");
    }

    #[test]
    fn older_entries_get_their_own_date_line_not_a_prefixed_text() {
        let font = font();
        let mut log = MessageLog::new();
        log.push("Docked.", "1-Jan-3250 12:00:00");
        log.push("Launched.", "1-Jan-3250 12:01:00");
        let lines = log.lines(&font, "1-Jan-3250 12:01:00");
        let text: Vec<_> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(
            text,
            [
                "1-Jan-3250 12:01:00",
                "Launched.",
                "1-Jan-3250 12:00:00",
                "Docked."
            ]
        );
        assert!(lines[2].header);
        assert!(!lines[3].header);
    }

    #[test]
    fn entries_wrap_at_the_originals_120_pixels() {
        let font = font();
        let mut log = MessageLog::new();
        log.push(
            "The quick brown fox jumps over the lazy dog at the station.",
            "1-Jan-3250",
        );
        let lines = log.lines(&font, "1-Jan-3250 12:00:00");
        assert!(lines.len() > 2);
        for line in lines.iter().skip(1) {
            assert!(
                font.width(&line.text).unwrap() <= ENTRY_WRAP,
                "{:?} is too wide",
                line.text
            );
        }
        // The header uses F272's wider display wrap.
        assert!(font.width(&lines[0].text).unwrap() <= WRAP);
    }

    #[test]
    fn the_window_is_top_anchored_at_the_header() {
        let font = font();
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        let lines = log.lines(&font, "1-Jan-3250 12:00:00");
        assert_eq!(log.first_line(), (0, 0));
        assert_eq!(lines[0].text, "1-Jan-3250 12:00:00");
        assert_eq!(lines[1].text, "message 9");
        // A five-row window shows the header, the newest entry, its date line
        // and the two lines of the next older entry.
        assert_eq!(VISIBLE_ROWS, 5);
        assert_eq!(lines[3].text, "message 8");
        assert!(lines[4].header);
    }

    #[test]
    fn scroll_clamps_and_auto_scrolls_after_f273s_ticks() {
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        // Ten entries plus header: 11 lines = 88 pixels; F273 caps at 0x18.
        let content = 11 * LINE_HEIGHT;
        assert_eq!(log.max_scroll(content), MAX_SCROLL);
        assert!(log.scroll_by(10_000, content, VISIBLE_ROWS * LINE_HEIGHT));
        assert_eq!(log.scroll(), MAX_SCROLL);
        log.tick(AUTO_SCROLL_TICS - 1);
        assert_eq!(log.scroll(), MAX_SCROLL);
        log.tick(1);
        assert_eq!(log.scroll(), 0);
    }

    #[test]
    fn scroll_moves_the_line_zero_origin() {
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        let content = 11 * LINE_HEIGHT;
        let window = VISIBLE_ROWS * LINE_HEIGHT;
        assert!(log.scroll_by(10, content, window));
        // Ten pixels is one line and two pixels: the header moves above the
        // clip and the next line starts at 159 - 2.
        assert_eq!(log.first_line(), (1, 2));
    }

    #[test]
    fn f273_allows_a_two_line_log_to_scroll_one_line() {
        let mut log = MessageLog::new();
        log.push("Landed at Olympus Village.", "1-Jan-3250 12:26:56");
        let content = 2 * LINE_HEIGHT;
        assert_eq!(log.max_scroll(content), LINE_HEIGHT as i32);
        assert!(log.scroll_by(8, content, VISIBLE_ROWS * LINE_HEIGHT));
        assert_eq!(log.first_line(), (1, 0));
    }
}
