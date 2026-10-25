//! The original message log (`F271`, `F272`, `F273`; `ffe.asm:20623-21232`).
//!
//! The console's right-hand text window is not a one-line toast: `F271` keeps the
//! last ten messages in a ring at `DATA_008681` (stride 0x40e, one 1025-byte
//! buffer each), each stamped with the game date and time (`DATA_008678/79`) and
//! a flag word (`DATA_008682`). `F272` writes the date/time header and then the
//! stored entries into the window at x 196, y 159 minus the scroll, wrapping at
//! 124 pixels and drawing an 8-pixel-line shadow at x 195, y 158 minus the scroll.
//! `F273` moves the scroll with actions 0xe/0xf (clamped at 0x18 pixels) and
//! auto-scrolls back to the newest entry after 0x3cae6 ticks.
//!
//! The port formats the stamp with [`crate::galaxy_map::date_text`] (the same
//! calendar the maps use) and keeps the original's fixed 10-entry ring, 124-pixel
//! wrap, line height, window position and scroll clamp. It uses the port's text
//! colours (15 for entries, 17 for the header, 4 for the shadow) because the
//! original injects `0x01 0x04`/`0x01 0x06` colours through `F1399` and the
//! runtime upper palette is not reproduced (`docs/STATUS.md` item 2).

use std::collections::VecDeque;

/// The original's ten entry buffers (`DATA_008681`, ten records of 0x40e bytes).
pub const ENTRIES: usize = 10;
/// `F272` wraps the log at 124 columns (`ffe.asm:21075-21088`): the window is
/// x 196..319, so this is a pixel width through the original's width table.
pub const WRAP: usize = 124;
/// `F1586`'s line height for the console (8 pixels).
pub const LINE_HEIGHT: usize = 8;
/// The header/text origin: x 196, y 159, with the shadow one pixel left and up.
pub const TEXT_X: usize = 196;
pub const TEXT_Y: usize = 159;
pub const SHADOW_X: usize = 195;
pub const SHADOW_Y: usize = 158;
/// `F273` clamps the scroll to 0x18 pixels.
pub const MAX_SCROLL: i32 = 0x18;
/// `F273` auto-scrolls to the newest entry after 0x3cae6 tics.
pub const AUTO_SCROLL_TICS: u32 = 0x3cae6;
/// The text window's first and last rows (clip ymin 158, height 8 per line).
pub const CLIP_TOP: usize = 158;
pub const CLIP_BOTTOM: usize = 200;

/// One stored message: its text and the in-game date/time stamp `F271` keeps
/// beside it (`DATA_008678/79`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub stamp: String,
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

    /// `F271`: append one entry, dropping the oldest beyond ten and returning to
    /// the newest.
    pub fn push(&mut self, text: impl Into<String>, stamp: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        // A repeated assignment of the same message is not a new event.
        if self.entries.back().is_some_and(|entry| entry.text == text) {
            return;
        }
        if self.entries.len() == ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            text,
            stamp: stamp.into(),
        });
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

    /// The scroll offset in pixels, 0 at the newest entry.
    pub fn scroll(&self) -> i32 {
        self.scroll
    }

    /// `F273`: scroll the log up or down by `pixels`, clamped to the content and
    /// the original's 0x18-pixel limit. Any manual scroll restarts the idle
    /// timer. Returns whether the offset changed.
    pub fn scroll_by(&mut self, pixels: i32, content_height: usize, window: usize) -> bool {
        let max = self.max_scroll(content_height, window);
        let next = (self.scroll + pixels).clamp(0, max);
        let changed = next != self.scroll;
        self.scroll = next;
        self.idle = 0;
        changed
    }

    /// The furthest the content can scroll without exposing blank rows below it,
    /// capped at `F273`'s 0x18.
    pub fn max_scroll(&self, content_height: usize, window: usize) -> i32 {
        let overflow = content_height.saturating_sub(window) as i32;
        overflow.min(MAX_SCROLL)
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

    /// The lines the window shows, newest position first, each wrapped at
    /// [`WRAP`] pixels with [`crate::indexed::BitmapFont::width`]. `header` is the
    /// date and time line `F272` writes before the entries.
    pub fn lines(&self, font: &crate::indexed::BitmapFont, header: &str) -> Vec<String> {
        let mut lines = Vec::new();
        if !header.is_empty() {
            lines.extend(wrap(font, header, WRAP));
        }
        for entry in &self.entries {
            let text = if entry.stamp.is_empty() {
                entry.text.clone()
            } else {
                format!("{} {}", entry.stamp, entry.text)
            };
            lines.extend(wrap(font, &text, WRAP));
        }
        lines
    }

    /// The first visible line and its pixel offset for a window `visible_rows`
    /// high, scrolled up from the newest line (the newest is at the bottom, as
    /// `F272` writes it below the header).
    pub fn window_start(&self, line_count: usize, visible_rows: usize) -> (usize, i32) {
        let content = line_count * LINE_HEIGHT;
        let window = visible_rows * LINE_HEIGHT;
        let bottom = content.saturating_sub(window) as i32;
        let offset = (bottom - self.scroll).max(0) as usize;
        (offset / LINE_HEIGHT, (offset % LINE_HEIGHT) as i32)
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
    fn wraps_at_the_original_124_pixels() {
        let font = font();
        let log = MessageLog::new();
        let line = wrap(
            &font,
            "The quick brown fox jumps over the lazy dog at the station.",
            WRAP,
        );
        assert!(line.len() > 1);
        for line in &line {
            assert!(font.width(line).unwrap() <= WRAP, "{line:?} is too wide");
        }
        // The header and entries share the window.
        assert!(log.lines(&font, "1-Jan-3250 12:00:00").len() == 1);
    }

    #[test]
    fn scroll_clamps_and_auto_scrolls_after_f273s_ticks() {
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        // Ten entries plus header: 11 lines = 88 pixels; the window is 41.
        let content = 11 * LINE_HEIGHT;
        let window = 41;
        assert_eq!(log.max_scroll(content, window), MAX_SCROLL);
        assert!(log.scroll_by(10_000, content, window));
        assert_eq!(log.scroll(), MAX_SCROLL);
        log.tick(AUTO_SCROLL_TICS - 1);
        assert_eq!(log.scroll(), MAX_SCROLL);
        log.tick(1);
        assert_eq!(log.scroll(), 0);
    }

    #[test]
    fn the_window_starts_at_the_newest_lines() {
        let log = MessageLog::new();
        // One line in a five-row window: it sits at the top, no offset.
        assert_eq!(log.window_start(1, 5), (0, 0));
        // Eleven lines in a five-row window: the newest five are shown.
        assert_eq!(log.window_start(11, 5), (6, 0));
    }
}
