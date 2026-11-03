//! The playable's side of the mission scripts and the journals: the cache's
//! script image, stories and journal archive, the [`Game`] hooks the scripts
//! call, and the game events that feed them (new game, each new day, docking).
//! See `docs/notes/mission-scripts.md` and `docs/notes/journal.md`.
//!
//! The hooks answer what the port can already answer (the player's cash,
//! ship, system and the date and time, the shared random pair); the
//! station-conversation and object statements are not wired yet and do
//! nothing.

use ffe_core::journal::{decode_stories, Story};
use ffe_core::npc::Random;
use ffe_core::script::engine::Host;
use ffe_core::script::run::{Game, Scripts};
use ffe_core::script::Image;

/// What the scripts need from the cache (v33 or later).
pub struct Assets {
    pub image: Image,
    pub stories: Vec<Story>,
    /// `base:text/journal`, for the viewer.
    pub archive: Vec<u8>,
}

impl Assets {
    /// `None` for a cache from before v33.
    pub fn load(cache: &ffe_content::VerifiedCache) -> Option<Self> {
        let image = Image::new(cache.asset("base:script/missions")?).ok()?;
        let stories = decode_stories(cache.asset("base:text/journal-stories")?).ok()?;
        let archive = cache.asset("base:text/journal")?.to_vec();
        Some(Self {
            image,
            stories,
            archive,
        })
    }
}

/// The original's day number (`DATA_008807`) for world seconds `clock`.
pub fn absolute_day(clock: f64) -> u32 {
    (clock.max(0.0) / 86_400.0) as u32
}

/// The game state the hooks read and write.
pub struct Hooks<'a> {
    /// The player's cash in tenths of a credit (`DATA_008888`).
    pub credits: &'a mut u32,
    /// The shared random pair (`DATA_008818/8820`).
    pub random: &'a mut Random,
    /// The current system's code (`DATA_008885`).
    pub system: u32,
    /// World seconds (F21's cases 5-11).
    pub clock: f64,
    /// Console lines to post (F89's `0xa600`).
    pub posts: &'a mut Vec<String>,
}

impl Host for Hooks<'_> {
    fn player(&mut self, case: i32) -> i32 {
        match case {
            2 => *self.credits as i32,
            4 => self.system as i32,
            5..=11 => {
                // F21: tics into the day / 0xc22e (seconds) plus the day
                // counter from 0x121cf7 in seconds.
                let day = absolute_day(self.clock) as i64;
                let seconds = (self.clock - day as f64 * 86_400.0).max(0.0) as i64;
                ((day - 0x121cf7) * 86_400 + seconds) as i32
            }
            _ => 0,
        }
    }

    fn set_player(&mut self, case: i32, value: i32) {
        if case == 2 {
            *self.credits = value.max(0) as u32;
        }
    }

    fn random(&mut self) -> u32 {
        self.random.advance()
    }
}

impl Game for Hooks<'_> {
    fn bounded(&mut self, bound: u32) -> u32 {
        self.random.bounded(bound)
    }

    fn advance(&mut self) -> i32 {
        self.random.advance() as i32
    }

    fn post(&mut self, text: &str) {
        self.posts.push(text.to_owned());
    }
}

/// F57's new game for the scripts: start mission 0 and publish the
/// journals' first issues.
pub fn new_game(assets: &Assets, hooks: &mut Hooks<'_>, docked: bool) -> Result<Scripts, String> {
    let mut scripts = Scripts::new(&assets.image);
    let today = absolute_day(hooks.clock);
    scripts.new_game(&assets.image, &assets.stories, hooks, today, docked)?;
    Ok(scripts)
}

/// The daily message for each day the clock crossed.
pub fn days(
    scripts: &mut Scripts,
    assets: &Assets,
    hooks: &mut Hooks<'_>,
    crossed: &[u32],
    docked: bool,
) -> Result<(), String> {
    for day in crossed {
        scripts.daily(&assets.image, &assets.stories, hooks, *day, docked)?;
    }
    Ok(())
}

/// The journal viewer's text: each shown article of `journal` as its
/// publication, headline and body (F76's macros are not expanded yet).
pub fn issue_text(
    scripts: &Scripts,
    assets: &Assets,
    journal: usize,
) -> Vec<(String, String, String)> {
    let Ok(archive) = ffe_core::journal::Journal::new(&assets.archive) else {
        return Vec::new();
    };
    scripts
        .library
        .shown(journal)
        .into_iter()
        .filter_map(|article| usize::try_from(article).ok())
        .filter_map(|article| archive.article(article))
        .map(|article| {
            let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
            (
                text(article.publication),
                text(article.headline),
                text(article.body),
            )
        })
        .collect()
}
