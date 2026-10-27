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
/// `F273` scrolls at most 0x18 pixels at a time toward older lines.
pub const MAX_SCROLL: i32 = 0x18;
/// `F273`'s idle time before the window slides its message away: 0x3cae6
/// `DATA_008799` tics, five game seconds.
pub const AUTO_SCROLL_TICS: u32 = 0x3cae6;
/// `F271`/`F273`'s resting scroll for `lines` CRs of the newest entry: the
/// header and the entry's lines sit at the bottom of the five-row window.
/// With no entry held (`DATA_005094` = -1) only the header is left showing.
fn rest(lines: i32) -> i32 {
    lines * LINE_HEIGHT as i32 - 0x18
}

/// Every producer of a log entry in the port. Port-only UI (unported button
/// notices, view names, pause/status toggles, debug and capture text) has no
/// variant here and never reaches `F271`. The citation is the source routine
/// that writes the entry; `tests/message_log.rs` checks every variant carries
/// one and that `main` has a single writer for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// F982's autopilot actions and `F350`'s replace flag
    /// (`ffe.asm:27076-27156`, `72772-72791`).
    Autopilot,
    /// F982's `t` target lock (`0x8a01`, replace) (`ffe.asm:72772-72791`).
    Target,
    /// F352's queued names, ranges and arrivals
    /// (`ffe.asm:27185-27207`).
    Navigation,
    /// F654/F655 traffic control and F863's docking/launch progress
    /// (`ffe.asm:44601-44656`, `60421-60481`).
    Docking,
    /// F381 clearance and F1088/F1089 launch (`ffe.asm:44601-44656`).
    Launch,
    /// F953/F942 hull collisions (`ffe.asm:70650`, `71190`).
    Collision,
    /// F931/F941/F953 lasers, missiles and ship hits (`ffe.asm:71190`).
    Combat,
    /// F234/F236 fines and police reports (`docs/notes/legal.md`).
    Legal,
    /// F323/F328 station market results.
    Trade,
    /// F1104/F1098 death and escape craft (`ffe.asm:81458`).
    Death,
    /// F290/F303/F349: an accepted or settled BBS contract's page.
    Mission,
    /// F343/F327: a station screen's prompt ("Which service do you require?").
    Station,
}

impl Source {
    pub const ALL: [Source; 12] = [
        Source::Autopilot,
        Source::Target,
        Source::Navigation,
        Source::Docking,
        Source::Launch,
        Source::Collision,
        Source::Combat,
        Source::Legal,
        Source::Trade,
        Source::Death,
        Source::Mission,
        Source::Station,
    ];

    /// The DOS FFE label/line that writes the entry.
    pub fn citation(self) -> &'static str {
        match self {
            Source::Autopilot => "F982 'a' + F350 replace flag (ffe.asm:27076-27156, 72772-72791)",
            Source::Target => "F982 't' 0x8a01 replace (ffe.asm:72772-72791)",
            Source::Navigation => "F352 + F350/F351 (ffe.asm:27185-27207)",
            Source::Docking => "F654/F655 + F863 replace progress (ffe.asm:44601, 60421)",
            Source::Launch => "F381/F654/F655 + F1088/F1089 launch (ffe.asm:44601)",
            Source::Collision => "F953/F942 collision (ffe.asm:70650, 71190)",
            Source::Combat => "F931/F941/F953 lasers, missiles, hits (ffe.asm:71190)",
            Source::Legal => "F234/F236/F783 fines and police (docs/notes/legal.md)",
            Source::Trade => "F323/F328 market (ffe.asm station stream)",
            Source::Death => "F1104/F1098 death and escape (ffe.asm:81458)",
            Source::Mission => "F290 contract, F303 settle, F349/F350 page (ffe.asm:22592-27185)",
            Source::Station => "F343/F327 station screen prompt (docs/notes/station.md)",
        }
    }
}

/// One log write: which source wrote it, the text, and whether it is F271's
/// replace-flag update (bit 0 of `DATA_008682`) rather than a new entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub source: Source,
    pub text: String,
    pub replace: bool,
}

impl Event {
    pub fn new(source: Source, text: impl Into<String>) -> Self {
        Self {
            source,
            text: text.into(),
            replace: false,
        }
    }

    pub fn replace(source: Source, text: impl Into<String>) -> Self {
        Self {
            source,
            text: text.into(),
            replace: true,
        }
    }

    /// `F271` with a NULL string: remove the newest entry when it was written
    /// with the replace flag (`ffe.asm:20760-20783`).
    pub fn clear(source: Source) -> Self {
        Self {
            source,
            text: String::new(),
            replace: true,
        }
    }
}

/// What [`Channel::post`] did, so the caller knows whether F351's bong belongs
/// to the new entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Post {
    /// Nothing changed (no event, a repeat, or a clear with nothing to clear).
    Ignored,
    /// A new F271 entry was appended.
    Added,
    /// The newest F271 entry was overwritten (replace flag).
    Updated,
    /// The newest replace-flagged entry was removed.
    Cleared,
}

/// The port's single route into `F271`. `last` mirrors the port's old
/// `logged` string so a repeated assignment is not a new event.
#[derive(Debug, Clone, Default)]
pub struct Channel {
    last: String,
}

impl Channel {
    /// `F271`: add, replace or clear one entry. The clock stamp is the current
    /// game date/time (`DATA_008807/8804`).
    pub fn post(&mut self, log: &mut MessageLog, event: Event, clock: &str) -> Post {
        if event.text.is_empty() {
            if log.clear_current() {
                self.last.clear();
                return Post::Cleared;
            }
            return Post::Ignored;
        }
        if event.text == self.last {
            return Post::Ignored;
        }
        if event.replace {
            log.record_replace(event.source, event.text.clone(), clock);
            self.last = event.text;
            Post::Updated
        } else {
            log.record(event.source, event.text.clone(), clock);
            self.last = event.text;
            Post::Added
        }
    }
}

/// One stored message: its text and the in-game date/time stamp `F271` keeps
/// beside it (`DATA_008678/79`), the replace flag and the source that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub stamp: String,
    /// `DATA_008682` bit 0: this entry is the current replace-mode message.
    pub replace: bool,
    /// The port's source mapping; `None` for tests and frame fixtures that use
    /// the raw [`MessageLog::push`]/[`MessageLog::update`] helpers.
    pub source: Option<Source>,
}

/// One drawn line and the colour group it belongs to: the current date/time
/// header and each older entry's date line use `0x01 0x04`, the entry text
/// `0x01 0x06` (`ffe.asm:20814-21018`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub header: bool,
}

/// The ten-entry ring (`DATA_008681`) and `F273`'s scroll state.
///
/// The text is drawn from y `TEXT_Y - scroll` (`F272`'s `DATA_008684`), so a
/// negative scroll moves it down the five-row window. A new entry slides the
/// window until the header and the entry's lines fill its bottom; five game
/// seconds later (`AUTO_SCROLL_TICS`) `F273` slides it on until only the
/// date/time header is left, which is how a message leaves the console. A
/// replace-flagged entry (a running readout) is held in view instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLog {
    entries: VecDeque<Entry>,
    /// `DATA_005096`: pixels the text is moved up from y 159.
    scroll: i32,
    /// `DATA_005092`: pixels left to slide, two a frame.
    remaining: i32,
    /// `DATA_008677`: the slide direction, +1 or -1.
    direction: i32,
    /// `DATA_005094`: the CRs of the held entry, -1 when none is held.
    held: i32,
    /// `DATA_008799 - DATA_005093`: tics since the last entry or scroll.
    idle: u32,
    /// A new entry whose wrapped line count the next [`Self::tick`] measures
    /// (`F271` wraps it at 0x78 pixels); `true` when it is replace-flagged.
    fresh: Option<bool>,
}

impl Default for MessageLog {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            scroll: 0,
            remaining: 0,
            direction: 0,
            held: -1,
            idle: 0,
            fresh: None,
        }
    }
}

impl MessageLog {
    /// An empty log; the header date/time still draws.
    pub fn new() -> Self {
        Self::default()
    }

    /// `F271` (`flag 0`): append one entry, dropping the oldest beyond ten.
    /// Untagged; use [`Self::record`] for game events.
    pub fn push(&mut self, text: impl Into<String>, stamp: impl Into<String>) {
        self.insert(text, stamp, false, None);
    }

    /// `F271` with flag word bit 0 set (`ffe.asm:20776-20792`): overwrite the
    /// newest entry instead of adding one. The original uses this for a message
    /// that is still being updated (an autopilot range, a speed readout), so a
    /// changing value does not fill the ring with near-identical lines.
    pub fn update(&mut self, text: impl Into<String>, stamp: impl Into<String>) {
        self.insert(text, stamp, true, None);
    }

    /// A new entry from a source-faithful game event.
    pub fn record(&mut self, source: Source, text: impl Into<String>, stamp: impl Into<String>) {
        self.insert(text, stamp, false, Some(source));
    }

    /// The current replace-flagged entry from a source-faithful game event.
    pub fn record_replace(
        &mut self,
        source: Source,
        text: impl Into<String>,
        stamp: impl Into<String>,
    ) {
        self.insert(text, stamp, true, Some(source));
    }

    /// `F271` with a NULL string: remove the newest entry only when it carries
    /// the replace flag (`ffe.asm:20760-20792`). A normal entry stays. The
    /// window is no longer held, so the idle slide takes the header down.
    pub fn clear_current(&mut self) -> bool {
        if self.entries.back().is_some_and(|entry| entry.replace) {
            self.entries.pop_back();
            self.held = -1;
            if self.fresh.is_some() {
                self.fresh = Some(false);
            }
            true
        } else {
            false
        }
    }

    fn insert(
        &mut self,
        text: impl Into<String>,
        stamp: impl Into<String>,
        replace: bool,
        source: Option<Source>,
    ) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let entry = Entry {
            text,
            stamp: stamp.into(),
            replace,
            source,
        };
        if replace {
            // F271 writes at DATA_005099, the slot after the last new entry.
            // If that slot already holds a replace-mode entry, overwrite it;
            // otherwise the replace flag starts a new entry that stays current
            // until the next new entry advances the slot.
            if let Some(last) = self.entries.back_mut().filter(|entry| entry.replace) {
                *last = entry;
            } else {
                if self.entries.len() == ENTRIES {
                    self.entries.pop_front();
                }
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
        self.fresh = Some(replace);
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

    /// `DATA_005096`: pixels the text is moved up from y 159 (negative moves
    /// it down).
    pub fn scroll(&self) -> i32 {
        self.scroll
    }

    /// Slide toward `target` (`F271`/`F273`: `DATA_005092` and `DATA_008677`).
    fn aim(&mut self, target: i32) {
        self.remaining = (target - self.scroll).abs();
        self.direction = if target < self.scroll { -1 } else { 1 };
    }

    /// `F273`'s action 0xf (Page Up): when no slide is running and older lines
    /// are below the window, slide up toward them, at most 0x18 pixels unless
    /// fewer than 0x20 are left. `content_lines` is `F272`'s CR count
    /// (`DATA_008685`). Any scroll key restarts the idle time.
    pub fn scroll_older(&mut self, content_lines: usize) -> bool {
        let last = content_lines as i32 * LINE_HEIGHT as i32 - LINE_HEIGHT as i32;
        let moved = self.remaining == 0 && last > self.scroll;
        if moved {
            let left = last - self.scroll;
            self.remaining = if left < 0x20 { left } else { MAX_SCROLL };
            self.direction = 1;
        }
        self.idle = 0;
        moved
    }

    /// `F273`'s action 0xe (Page Down): when no slide is running and the text
    /// is above -0x18, slide down by 0x20 (or to -0x20 from a negative scroll).
    pub fn scroll_newer(&mut self) -> bool {
        let moved = self.remaining == 0 && self.scroll > -0x18;
        if moved {
            self.remaining = if self.scroll < 0 {
                self.scroll + 0x20
            } else {
                0x20
            };
            self.direction = -1;
        }
        self.idle = 0;
        moved
    }

    /// One `F273` frame: measure a new entry (`F271`'s target), slide two
    /// pixels, then after `AUTO_SCROLL_TICS` idle tics slide to the held
    /// entry's rest, or below the last entry when none is held.
    pub fn tick(&mut self, tics: u32, font: &crate::indexed::BitmapFont) {
        if let Some(replace) = self.fresh.take() {
            let lines = self
                .entries
                .back()
                .map_or(1, |entry| wrap(font, &entry.text, ENTRY_WRAP).len())
                as i32
                - 1;
            self.held = if replace { lines } else { -1 };
            self.aim(rest(lines));
        }
        for _ in 0..2 {
            if self.remaining != 0 {
                self.remaining -= 1;
                self.scroll += self.direction;
            }
        }
        self.idle = self.idle.saturating_add(tics);
        if self.idle > AUTO_SCROLL_TICS && self.scroll != rest(self.held) {
            self.aim(rest(self.held));
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
    fn a_replacement_updates_the_current_entry() {
        let mut log = MessageLog::new();
        log.record_replace(
            Source::Autopilot,
            "Autopilot to Mars.",
            "1-Jan-3250 12:00:00",
        );
        log.record_replace(
            Source::Autopilot,
            "Autopilot: Mars 2 Mm.",
            "1-Jan-3250 12:00:01",
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].text, "Autopilot: Mars 2 Mm.");
        assert_eq!(log.entries()[0].stamp, "1-Jan-3250 12:00:01");
        // A new entry closes the replace slot; the next replace opens a new one.
        log.push("Docked.", "1-Jan-3250 12:00:02");
        log.update("Autopilot: Mars 1 Mm.", "1-Jan-3250 12:00:03");
        assert_eq!(log.len(), 3);
        assert_eq!(log.entries()[1].text, "Docked.");
        assert!(log.entries()[2].replace);
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

    /// Run `frames` of F273 with `tics` each.
    fn frames(log: &mut MessageLog, font: &crate::indexed::BitmapFont, frames: usize, tics: u32) {
        for _ in 0..frames {
            log.tick(tics, font);
        }
    }

    #[test]
    fn the_header_leads_the_newest_entry_and_older_dated_entries() {
        let font = font();
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        let lines = log.lines(&font, "1-Jan-3250 12:00:00");
        assert_eq!(lines[0].text, "1-Jan-3250 12:00:00");
        assert_eq!(lines[1].text, "message 9");
        assert!(lines[2].header);
        assert_eq!(lines[3].text, "message 8");
        assert_eq!(VISIBLE_ROWS, 5);
    }

    #[test]
    fn a_new_message_slides_in_then_leaves_after_five_game_seconds() {
        let font = font();
        let mut log = MessageLog::new();
        log.push("Landed at Old Blackelk.", "1-Jan-3200 12:00");
        // F271: the one-line entry rests at -0x18, header and text in the
        // window's bottom two rows, sliding two pixels a frame.
        log.tick(0, &font);
        assert_eq!(log.scroll(), -2);
        frames(&mut log, &font, 20, 0);
        assert_eq!(log.scroll(), -0x18);
        // F273: at 0x3cae6 tics it is still shown; past them it slides down to
        // -0x20, leaving only the date/time header.
        frames(&mut log, &font, 1, AUTO_SCROLL_TICS);
        assert_eq!(log.scroll(), -0x18);
        frames(&mut log, &font, 10, 1);
        assert_eq!(log.scroll(), -0x20);
        // The next message brings the window back.
        log.push("Launch permission granted.", "1-Jan-3200 12:01");
        frames(&mut log, &font, 10, 0);
        assert_eq!(log.scroll(), -0x18);
    }

    #[test]
    fn a_replace_flagged_readout_is_held_until_cleared() {
        let font = font();
        let mut log = MessageLog::new();
        log.update("Set: 100 km", "1-Jan-3200 12:00");
        frames(&mut log, &font, 20, 0);
        frames(&mut log, &font, 20, AUTO_SCROLL_TICS);
        assert_eq!(log.scroll(), -0x18);
        assert!(log.clear_current());
        frames(&mut log, &font, 10, 1);
        assert_eq!(log.scroll(), -0x20);
    }

    #[test]
    fn page_up_slides_toward_older_lines_and_page_down_back() {
        let font = font();
        let mut log = MessageLog::new();
        for n in 0..10 {
            log.push(format!("message {n}"), "1-Jan-3250");
        }
        frames(&mut log, &font, 20, 0);
        let lines = log.lines(&font, "1-Jan-3250 12:00:00").len();
        assert!(log.scroll_older(lines));
        // A slide in progress ignores the key.
        assert!(!log.scroll_older(lines));
        frames(&mut log, &font, 12, 0);
        assert_eq!(log.scroll(), 0);
        assert!(log.scroll_older(lines));
        frames(&mut log, &font, 12, 0);
        assert_eq!(log.scroll(), MAX_SCROLL);
        // Page Down: 0x20 down from a non-negative scroll.
        assert!(log.scroll_newer());
        frames(&mut log, &font, 16, 0);
        assert_eq!(log.scroll(), MAX_SCROLL - 0x20);
        // At or below -0x18 it stops.
        assert!(log.scroll_newer());
        frames(&mut log, &font, 16, 0);
        assert_eq!(log.scroll(), -0x20);
        assert!(!log.scroll_newer());
    }

    #[test]
    fn every_source_has_a_dos_citation() {
        assert_eq!(Source::ALL.len(), 12);
        for source in Source::ALL {
            let citation = source.citation();
            assert!(!citation.is_empty(), "{source:?} has no citation");
            assert!(
                citation.contains("F")
                    || citation.contains("ffe.asm")
                    || citation.contains("docs/"),
                "{source:?} does not name a source: {citation}"
            );
        }
    }

    #[test]
    fn the_channel_adds_new_entries_and_replaces_the_current_one() {
        let mut log = MessageLog::new();
        let mut channel = Channel::default();
        assert_eq!(
            channel.post(
                &mut log,
                Event::new(Source::Station, "Which service do you require?"),
                "1-Jan-3250 12:00:00"
            ),
            Post::Added
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].source, Some(Source::Station));
        assert!(!log.entries()[0].replace);

        assert_eq!(
            channel.post(
                &mut log,
                Event::replace(Source::Autopilot, "Autopilot to Hope."),
                "1-Jan-3250 12:00:01"
            ),
            Post::Updated
        );
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries()[1].source, Some(Source::Autopilot));
        assert!(log.entries()[1].replace);
        assert_eq!(
            channel.post(
                &mut log,
                Event::replace(Source::Autopilot, "Autopilot to Hope."),
                "1-Jan-3250 12:00:02"
            ),
            Post::Ignored,
            "a repeated assignment is not a new event"
        );
    }

    #[test]
    fn clear_current_removes_only_the_replace_entry() {
        let mut log = MessageLog::new();
        let mut channel = Channel::default();
        channel.post(
            &mut log,
            Event::new(Source::Station, "Which service do you require?"),
            "1-Jan-3250 12:00:00",
        );
        channel.post(
            &mut log,
            Event::replace(Source::Autopilot, "Autopilot to Hope."),
            "1-Jan-3250 12:00:01",
        );
        assert_eq!(
            channel.post(
                &mut log,
                Event::clear(Source::Autopilot),
                "1-Jan-3250 12:00:02"
            ),
            Post::Cleared
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log.entries()[0].text, "Which service do you require?");

        // A normal entry is not replace-flagged, so a NULL F271 leaves it.
        assert_eq!(
            channel.post(
                &mut log,
                Event::clear(Source::Autopilot),
                "1-Jan-3250 12:00:03"
            ),
            Post::Ignored
        );
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn the_channel_keeps_the_ten_entry_ring_and_resets_scroll() {
        let font = font();
        let mut log = MessageLog::new();
        let mut channel = Channel::default();
        for n in 0..12 {
            channel.post(
                &mut log,
                Event::new(Source::Combat, format!("message {n}")),
                "1-Jan-3250 12:00:00",
            );
        }
        assert_eq!(log.len(), ENTRIES);
        assert_eq!(log.entries()[0].text, "message 2");
        assert_eq!(log.entries()[9].text, "message 11");

        frames(&mut log, &font, 20, 0);
        assert!(log.scroll_older(11 * 3));
        frames(&mut log, &font, 20, 0);
        assert_eq!(log.scroll(), 0);
        channel.post(
            &mut log,
            Event::new(Source::Combat, "message 12"),
            "1-Jan-3250 12:00:01",
        );
        frames(&mut log, &font, 20, 0);
        assert_eq!(log.scroll(), -0x18, "a new entry slides back to the newest");
    }
}
