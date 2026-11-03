//! The ship library's journals at run time (F85-F106, F68, F87-F91): five
//! journal records (`DATA_008569`, 0x38 bytes each), the pool of 700 article
//! records (`DATA_008579`, 0x44 bytes each) linked into each journal's queue
//! and current issue, the used-story bits (`DATA_008565`) and the two word
//! arrays the scripts read (`DATA_008567`, `DATA_008568`). The mission
//! scripts drive it (statements 10-18, tokens 0x20 and 0x2d/0x2e); the
//! daily event publishes issues. See `docs/notes/journal.md`.
//!
//! The lists are kept as the original keeps them, as prev/next links
//! between pool records, so F99's removal (which never moves a journal's
//! head) and F98's reuse of a dead record behave the same.

use serde::{Deserialize, Serialize};

use crate::journal::Story;

/// `DATA_008569`: five journals.
pub const JOURNALS: usize = 5;
/// `DATA_008579`: 700 article records.
pub const RECORDS: usize = 0x2bc;
/// `DATA_002300`: the script globals holding each slot's journal number.
pub const SLOT_GLOBALS: [usize; JOURNALS] = [0x14, 0x12, 0x11, 0x13, 0x15];
/// F86: stories flagged `+0xe` bit 0 need this script global set.
pub const STORY_GATE_GLOBAL: usize = 0x16;
/// Article flag `0x100`: shown in the issue (F71 copies these).
pub const SHOWN: u32 = 0x100;
/// Article flag `0x400`: waiting for the player to dock.
pub const WAITING: u32 = 0x400;
/// Journal flag bit 0 (F102/F103): waiting articles are shown on docking.
pub const DELIVERED: u32 = 1;
/// Journal flag bit 1 (F104/F105): subscribed until `expiry`.
pub const SUBSCRIBED: u32 = 2;
/// Journal flag bit 2 (F97): the journal has ended.
pub const ENDED: u32 = 4;
/// Event F89 raises into the scripts when it publishes an article.
pub const EVENT_PUBLISHED: i32 = 7;

/// One `DATA_008579` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// `+0x00`: the article (-1 a free record).
    pub article: i32,
    /// `+0x04`: [`SHOWN`], [`WAITING`].
    pub flags: u32,
    /// `+0x08`: F89 publishes the highest first.
    pub priority: i32,
    /// `+0x0c`: the first day it may be published.
    pub release: u32,
    /// `+0x10`: the day it was published.
    pub published: u32,
    /// `+0x14..`: ten words the articles' macros fill on first use (F92).
    pub slots: [i32; 10],
    /// `+0x3c`/`+0x40`.
    pub next: Option<u16>,
    pub prev: Option<u16>,
}

impl Default for Record {
    fn default() -> Self {
        Self {
            article: -1,
            flags: 0,
            priority: 0,
            release: 0,
            published: 0,
            slots: [0; 10],
            next: None,
            prev: None,
        }
    }
}

/// One `DATA_008569` journal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// `+0x00`: the name (up to 19 bytes).
    pub name: Vec<u8>,
    /// `+0x14`: the masthead bitmap.
    pub masthead: i32,
    /// `+0x18`.
    pub read: i32,
    /// `+0x1c`: days between issues (0: no journal).
    pub period: u32,
    /// `+0x20`: the day the next issue is due.
    pub next_issue: u32,
    /// `+0x24`: [`DELIVERED`], [`SUBSCRIBED`], [`ENDED`].
    pub flags: u32,
    /// `+0x28`: days an issue's articles stay.
    pub retention: u32,
    /// `+0x2c`: the subscription's last day.
    pub expiry: u32,
    /// `+0x30`/`+0x34`: list heads.
    pub queue: Option<u16>,
    pub issue: Option<u16>,
}

/// What the library needs from the game and the scripts.
pub trait Context {
    /// Script global `index` (`DATA_008548`).
    fn global(&mut self, index: usize) -> i32;
    /// F922: a uniform draw below `bound` from the shared pair.
    fn bounded(&mut self, bound: u32) -> u32;
    /// F1530: advance the shared pair and return its first word.
    fn advance(&mut self) -> i32;
}

/// An event F89 asks the scripts to see, and its console line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub journal: usize,
    pub article: i32,
    /// `0xa600` with the journal's name, when F89 was asked to post it.
    pub notice: Option<String>,
}

/// The whole journal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub journals: Vec<Journal>,
    pub records: Vec<Record>,
    /// `DATA_008565`: one bit per story already used.
    pub used: Vec<u8>,
    /// `DATA_008566`: F68 runs once.
    pub started: bool,
    /// `DATA_008567`: 100 words, each drawn on first read (F106).
    pub random_words: Vec<i32>,
    /// `DATA_008568`: 100 words (F93/F94).
    pub words: Vec<i32>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            journals: vec![Journal::default(); JOURNALS],
            records: vec![Record::default(); RECORDS],
            used: vec![0; 24],
            started: false,
            random_words: vec![0; 100],
            words: vec![0; 100],
        }
    }
}

fn index(link: Option<u16>) -> Option<usize> {
    link.map(usize::from)
}

impl Library {
    /// F91 (message 1): clear the journals' periods, mastheads and flags,
    /// free every record and the used-story bits.
    pub fn reset(&mut self) {
        for journal in &mut self.journals {
            journal.period = 0;
            journal.masthead = 0;
            journal.flags = 0;
        }
        for record in &mut self.records {
            record.article = -1;
        }
        for byte in self.used.iter_mut().take(0x16) {
            *byte = 0;
        }
        self.started = false;
    }

    /// F101 (statement 10): the first journal slot with no period gets
    /// `name`, `masthead`, its first issue (`first`, or today for 0), the
    /// `period` and `retention`. Returns the slot, or -1 with all five used.
    pub fn create(
        &mut self,
        name: &[u8],
        masthead: i32,
        first: u32,
        period: u32,
        retention: u32,
        today: u32,
    ) -> i32 {
        let Some(slot) = self.journals.iter().position(|journal| journal.period == 0) else {
            return -1;
        };
        self.journals[slot] = Journal {
            name: name.iter().copied().take(19).collect(),
            masthead,
            read: self.journals[slot].read,
            period,
            next_issue: if first == 0 { today } else { first },
            flags: 0,
            retention,
            expiry: 0,
            queue: None,
            issue: None,
        };
        slot as i32
    }

    fn journal_mut(&mut self, journal: i32) -> Option<&mut Journal> {
        usize::try_from(journal)
            .ok()
            .and_then(|j| self.journals.get_mut(j))
    }

    /// F102 / F103 (statements 11, 12).
    pub fn set_delivered(&mut self, journal: i32, on: bool) {
        if let Some(journal) = self.journal_mut(journal) {
            if on {
                journal.flags |= DELIVERED;
            } else {
                journal.flags &= !DELIVERED;
            }
        }
    }

    /// F104 (statement 17): subscribe for `days` more (from today when the
    /// subscription has lapsed).
    pub fn subscribe(&mut self, journal: i32, days: u32, today: u32) {
        if let Some(journal) = self.journal_mut(journal) {
            journal.flags |= SUBSCRIBED;
            if journal.expiry <= today {
                journal.expiry = today.wrapping_add(days);
            } else {
                journal.expiry = journal.expiry.wrapping_add(days);
            }
        }
    }

    /// F105 (statement 18).
    pub fn unsubscribe(&mut self, journal: i32) {
        if let Some(journal) = self.journal_mut(journal) {
            journal.flags &= !SUBSCRIBED;
        }
    }

    /// F97 (statement 13): free the queue and end the journal.
    pub fn end(&mut self, journal: i32) {
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return;
        };
        let mut at = index(self.journals[slot].queue);
        while let Some(record) = at {
            self.records[record].article = -1;
            at = index(self.records[record].next);
        }
        self.journals[slot].queue = None;
        self.journals[slot].flags |= ENDED;
    }

    /// F98 (statement 14 and F88): put `article` at the head of the
    /// journal's queue in the first free record.
    pub fn queue(&mut self, journal: i32, article: i32, priority: i32, flags: u32, release: u32) {
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return;
        };
        let Some(free) = self.records.iter().position(|record| record.article == -1) else {
            return;
        };
        let head = self.journals[slot].queue;
        let record = &mut self.records[free];
        record.article = article;
        record.flags = flags;
        record.priority = priority;
        record.release = release;
        record.next = head;
        record.prev = None;
        record.slots = [0; 10];
        if let Some(head) = index(head) {
            self.records[head].prev = Some(free as u16);
        }
        self.journals[slot].queue = Some(free as u16);
    }

    /// F99 (statement 15): mark every record of `article` in the queue and
    /// the issue free. A record with no predecessor stays the list's head.
    pub fn remove(&mut self, journal: i32, article: i32) {
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return;
        };
        for head in [self.journals[slot].queue, self.journals[slot].issue] {
            let mut at = index(head);
            while let Some(record) = at {
                let (next, prev) = (self.records[record].next, self.records[record].prev);
                if self.records[record].article == article {
                    if let Some(prev) = index(prev) {
                        self.records[prev].next = next;
                    }
                    if let Some(next) = index(next) {
                        self.records[next].prev = prev;
                    }
                    self.records[record].article = -1;
                }
                at = index(next);
            }
        }
    }

    /// F100 (statement 16): the first record of `old` in the queue, and the
    /// first in the issue, become `new`.
    pub fn replace(&mut self, journal: i32, old: i32, new: i32) {
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return;
        };
        for head in [self.journals[slot].queue, self.journals[slot].issue] {
            let mut at = index(head);
            while let Some(record) = at {
                if self.records[record].article == old {
                    self.records[record].article = new;
                    break;
                }
                at = index(self.records[record].next);
            }
        }
    }

    /// F85 (token 0x20): `article` is in the journal's current issue.
    pub fn in_issue(&self, journal: i32, article: i32) -> bool {
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return false;
        };
        let mut at = index(self.journals[slot].issue);
        while let Some(record) = at {
            if self.records[record].article == article {
                return true;
            }
            at = index(self.records[record].next);
        }
        false
    }

    /// F106 (token 0x2d): `DATA_008567[word]`, drawn from F1530 on first read.
    pub fn random_word(&mut self, word: i32, ctx: &mut dyn Context) -> i32 {
        let Some(slot) = usize::try_from(word).ok().filter(|w| *w < 100) else {
            return 0;
        };
        if self.random_words[slot] == 0 {
            self.random_words[slot] = ctx.advance();
        }
        self.random_words[slot]
    }

    /// F95 (token 0x2e): `DATA_008568[word]` for article 0, else record
    /// slot `word` of the first record of `article` in the journal's issue,
    /// then its queue (F92 draws it on first read).
    pub fn article_word(
        &mut self,
        word: i32,
        journal: i32,
        article: i32,
        ctx: &mut dyn Context,
    ) -> i32 {
        if article == 0 {
            return usize::try_from(word)
                .ok()
                .and_then(|w| self.words.get(w))
                .copied()
                .unwrap_or(0);
        }
        let Some(slot) = usize::try_from(journal).ok().filter(|j| *j < JOURNALS) else {
            return 0;
        };
        for head in [self.journals[slot].issue, self.journals[slot].queue] {
            let mut at = index(head);
            while let Some(record) = at {
                if self.records[record].article == article {
                    let Some(cell) = usize::try_from(word).ok().filter(|w| *w < 10) else {
                        return 0;
                    };
                    if self.records[record].slots[cell] == 0 {
                        self.records[record].slots[cell] = ctx.advance();
                    }
                    return self.records[record].slots[cell];
                }
                at = index(self.records[record].next);
            }
        }
        0
    }

    fn used(&self, story: usize) -> bool {
        self.used[story / 8] & (1 << (story % 8)) != 0
    }

    fn set_used(&mut self, story: usize, on: bool) {
        if on {
            self.used[story / 8] |= 1 << (story % 8);
        } else {
            self.used[story / 8] &= !(1 << (story % 8));
        }
    }

    /// F86: story `story` can fill journal slot `slot`.
    fn eligible(
        &self,
        stories: &[Story],
        story: usize,
        slot: usize,
        ctx: &mut dyn Context,
    ) -> bool {
        if self.used(story) {
            return false;
        }
        let entry = &stories[story];
        if entry.flags() & 1 != 0 && ctx.global(STORY_GATE_GLOBAL) == 0 {
            return false;
        }
        match entry {
            Story::Grid { rows, .. } => rows.first().is_some_and(|row| row[slot] != -1),
            Story::Single { journal, .. } => *journal == slot as i32,
        }
    }

    /// F87: clear the used bits of every story for journal slot `slot`.
    fn refresh(&mut self, stories: &[Story], slot: usize) {
        for (story, entry) in stories.iter().enumerate() {
            let fits = match entry {
                Story::Grid { rows, .. } => rows.first().is_some_and(|row| row[slot] != -1),
                Story::Single { journal, .. } => *journal == slot as i32,
            };
            if fits {
                self.set_used(story, false);
            }
        }
    }

    /// F88: refill journal `journal`'s queue from the highest-priority
    /// eligible stories (one picked at random), refreshing its stories once
    /// when none is left.
    pub fn refill(&mut self, stories: &[Story], journal: i32, today: u32, ctx: &mut dyn Context) {
        let ids: Vec<i32> = SLOT_GLOBALS.iter().map(|g| ctx.global(*g)).collect();
        let Some(slot) = ids.iter().position(|id| *id == journal) else {
            return;
        };
        let mut best = 0u16;
        for attempt in 0..2 {
            for story in 0..stories.len() {
                if self.eligible(stories, story, slot, ctx) && best < stories[story].priority() {
                    best = stories[story].priority();
                }
            }
            if best != 0 {
                break;
            }
            if attempt == 0 {
                self.refresh(stories, slot);
            }
        }
        if best == 0 {
            return;
        }
        let candidates: Vec<usize> = (0..stories.len())
            .filter(|story| {
                self.eligible(stories, *story, slot, ctx) && stories[*story].priority() == best
            })
            .collect();
        let pick = ctx.bounded(candidates.len() as u32) as usize;
        let Some(&story) = candidates.get(pick) else {
            return;
        };
        match &stories[story] {
            Story::Grid { rows, .. } => {
                for (row, articles) in rows.iter().enumerate() {
                    for (column, article) in articles.iter().enumerate() {
                        if *article == -1 {
                            continue;
                        }
                        let target = ids[column];
                        let period = usize::try_from(target)
                            .ok()
                            .and_then(|j| self.journals.get(j))
                            .map_or(0, |j| j.period);
                        let release = today.wrapping_add(period.wrapping_mul(row as u32));
                        self.queue(target, *article, 1, 0, release);
                    }
                }
            }
            Story::Single { article, .. } => self.queue(journal, *article, 1, 0, 0),
        }
        self.set_used(story, true);
    }

    /// F89: publish journal `journal`'s next issue: drop the issue's
    /// articles older than the retention, move the highest-priority released
    /// article from the queue to the issue (refilling the queue once when
    /// none is ready) and mark it for the player. `docked` is
    /// `DATA_008870` 0x30 or 0x2a.
    pub fn publish(
        &mut self,
        stories: &[Story],
        journal: usize,
        post: bool,
        today: u32,
        docked: bool,
        ctx: &mut dyn Context,
    ) -> Option<Published> {
        self.started = true;
        let id = journal as i32;
        {
            let entry = self.journals.get_mut(journal)?;
            entry.next_issue = entry.period.wrapping_add(today);
        }
        if self.journals[journal].queue.is_none() {
            self.refill(stories, id, today, ctx);
        }
        self.journals[journal].queue?;
        // Expire the current issue.
        let retention = self.journals[journal].retention;
        let mut at = index(self.journals[journal].issue);
        while let Some(record) = at {
            let (next, prev) = (self.records[record].next, self.records[record].prev);
            if retention.wrapping_add(self.records[record].published) <= today {
                match index(prev) {
                    Some(prev) => self.records[prev].next = next,
                    None => self.journals[journal].issue = next,
                }
                if let Some(next) = index(next) {
                    self.records[next].prev = prev;
                }
                self.records[record].article = -1;
            }
            at = index(next);
        }
        // Pick the highest priority released article (later ties win).
        let mut chosen = None;
        let mut best = -1;
        for attempt in 0..2 {
            let mut at = index(self.journals[journal].queue);
            while let Some(record) = at {
                let entry = &self.records[record];
                if best <= entry.priority && entry.release <= today {
                    best = entry.priority;
                    chosen = Some(record);
                }
                at = index(entry.next);
            }
            if chosen.is_some() {
                break;
            }
            if attempt == 0 {
                self.refill(stories, id, today, ctx);
            }
        }
        let record = chosen?;
        // Unlink from the queue, link at the head of the issue.
        let (next, prev) = (self.records[record].next, self.records[record].prev);
        match index(prev) {
            Some(prev) => self.records[prev].next = next,
            None => self.journals[journal].queue = next,
        }
        if let Some(next) = index(next) {
            self.records[next].prev = prev;
        }
        let head = self.journals[journal].issue;
        self.records[record].next = head;
        self.records[record].prev = None;
        if let Some(head) = index(head) {
            self.records[head].prev = Some(record as u16);
        }
        self.journals[journal].issue = Some(record as u16);
        self.records[record].published = today;
        if self.journals[journal].flags & SUBSCRIBED != 0 {
            self.records[record].flags |= WAITING;
            if docked {
                self.records[record].flags |= SHOWN;
            }
        }
        let name = String::from_utf8_lossy(&self.journals[journal].name).into_owned();
        Some(Published {
            journal,
            article: self.records[record].article,
            notice: post.then(|| {
                format!(
                    "The new issue of {name} is now available. Get it from your local newsagent."
                )
            }),
        })
    }

    /// F68 (new game): publish every journal's first issue once, silently.
    pub fn start(
        &mut self,
        stories: &[Story],
        today: u32,
        docked: bool,
        ctx: &mut dyn Context,
    ) -> Vec<Published> {
        if self.started {
            return Vec::new();
        }
        (0..JOURNALS)
            .filter_map(|journal| self.publish(stories, journal, false, today, docked, ctx))
            .collect()
    }

    /// F90 on the daily message (15): lapse finished subscriptions and
    /// publish every due issue, posting `0xa600`.
    pub fn daily(
        &mut self,
        stories: &[Story],
        today: u32,
        docked: bool,
        ctx: &mut dyn Context,
    ) -> Vec<Published> {
        let mut out = Vec::new();
        for journal in 0..JOURNALS {
            if self.journals[journal].expiry < today {
                self.journals[journal].flags &= !SUBSCRIBED;
            }
            let entry = &self.journals[journal];
            if entry.period != 0 && entry.flags & ENDED == 0 && entry.next_issue <= today {
                out.extend(self.publish(stories, journal, true, today, docked, ctx));
            }
        }
        out
    }

    /// F90 on message 5 (docking): show the waiting articles of every
    /// delivered journal.
    pub fn docked(&mut self) {
        for journal in 0..JOURNALS {
            if self.journals[journal].flags & DELIVERED == 0 {
                continue;
            }
            let mut at = index(self.journals[journal].issue);
            while let Some(record) = at {
                if self.records[record].flags & WAITING != 0 {
                    self.records[record].flags |= SHOWN;
                }
                at = index(self.records[record].next);
            }
        }
    }

    /// F71: the articles F78 shows for `journal` (its issue's shown records,
    /// head first).
    /// The current issue's shown articles of `journal` with their slot words
    /// (F95 puts them in `DATA_008568` while an article's macros expand).
    pub fn shown_records(&self, journal: usize) -> Vec<(i32, [i32; 10])> {
        let mut out = Vec::new();
        let Some(entry) = self.journals.get(journal) else {
            return out;
        };
        let mut at = index(entry.issue);
        while let Some(record) = at {
            if self.records[record].flags & SHOWN != 0 {
                out.push((self.records[record].article, self.records[record].slots));
            }
            at = index(self.records[record].next);
        }
        out
    }

    pub fn shown(&self, journal: usize) -> Vec<i32> {
        let mut out = Vec::new();
        let Some(entry) = self.journals.get(journal) else {
            return out;
        };
        let mut at = index(entry.issue);
        while let Some(record) = at {
            if self.records[record].flags & SHOWN != 0 {
                out.push(self.records[record].article);
            }
            at = index(self.records[record].next);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        globals: [i32; 0x20],
        draws: u32,
    }

    impl Context for Fixed {
        fn global(&mut self, index: usize) -> i32 {
            self.globals[index]
        }
        fn bounded(&mut self, _bound: u32) -> u32 {
            0
        }
        fn advance(&mut self) -> i32 {
            self.draws += 1;
            0x1234 + self.draws as i32
        }
    }

    fn context() -> Fixed {
        let mut globals = [0; 0x20];
        for (slot, global) in SLOT_GLOBALS.iter().enumerate() {
            globals[*global] = slot as i32;
        }
        Fixed { globals, draws: 0 }
    }

    #[test]
    fn issues_publish_the_highest_priority_released_article() {
        let mut library = Library::default();
        let mut ctx = context();
        assert_eq!(library.create(b"Imperial Herald", 1, 0, 28, 600, 100), 0);
        library.queue(0, 261, 20, 0, 0);
        library.queue(0, 256, 20, 0, 0);
        library.queue(0, 300, 99, 0, 500);
        library.subscribe(0, 336, 100);
        let first = library.publish(&[], 0, true, 100, true, &mut ctx).unwrap();
        // 300 is not released yet; of the equal priorities the later wins
        // (the list is newest first, so 261 is met last).
        assert_eq!(first.article, 261);
        assert!(first
            .notice
            .unwrap()
            .starts_with("The new issue of Imperial Herald"));
        assert_eq!(library.journals[0].next_issue, 128);
        assert_eq!(library.shown(0), vec![261]);
        assert!(library.in_issue(0, 261));
        // The daily message publishes when the issue is due.
        assert!(library.daily(&[], 127, false, &mut ctx).is_empty());
        let due = library.daily(&[], 128, false, &mut ctx);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].article, 256);
        // Not docked: waiting, not shown, until the player docks.
        assert_eq!(library.shown(0), vec![261]);
        library.set_delivered(0, true);
        library.docked();
        assert_eq!(library.shown(0), vec![256, 261]);
    }

    #[test]
    fn an_empty_queue_is_refilled_from_the_stories() {
        let stories = vec![
            Story::Grid {
                rows: vec![[23, 22, -1, -1, -1], [24, -1, -1, -1, 25]],
                priority: 15,
                flags: 0,
            },
            Story::Single {
                article: 40,
                journal: 0,
                priority: 5,
                flags: 0,
            },
        ];
        let mut library = Library::default();
        let mut ctx = context();
        for (slot, name) in [b"A", b"B", b"C", b"D", b"E"].iter().enumerate() {
            assert_eq!(library.create(*name, 0, 0, 28, 600, 10), slot as i32);
        }
        let published = library
            .publish(&stories, 0, false, 10, false, &mut ctx)
            .unwrap();
        assert_eq!(published.article, 23);
        // The grid's other cells went to their journals, row 1 a period on.
        assert!(library
            .records
            .iter()
            .any(|r| r.article == 24 && r.release == 38));
        assert!(library
            .records
            .iter()
            .any(|r| r.article == 25 && r.release == 38));
        assert!(library.used(0));
    }

    #[test]
    fn removing_a_queue_head_leaves_it_linked_as_the_original_does() {
        let mut library = Library::default();
        library.create(b"X", 0, 0, 28, 600, 0);
        library.queue(0, 1, 1, 0, 0);
        library.queue(0, 2, 1, 0, 0);
        library.remove(0, 2);
        // The head record is free but still the head (F99 never moves it).
        let head = library.journals[0].queue.unwrap();
        assert_eq!(library.records[usize::from(head)].article, -1);
        library.end(0);
        assert!(library.journals[0].queue.is_none());
        assert_eq!(library.journals[0].flags & ENDED, ENDED);
    }
}
