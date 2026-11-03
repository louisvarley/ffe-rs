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
    /// `base:script/queries` (v34): the names `[Mi...(name)` asks for.
    pub queries: Vec<(i32, String)>,
}

impl Assets {
    /// `None` for a cache from before v33.
    pub fn load(cache: &ffe_content::VerifiedCache) -> Option<Self> {
        let image = Image::new(cache.asset("base:script/missions")?).ok()?;
        let stories = decode_stories(cache.asset("base:text/journal-stories")?).ok()?;
        let archive = cache.asset("base:text/journal")?.to_vec();
        let queries = ffe_core::script::decode_queries(cache.asset("base:script/queries")?).ok()?;
        Some(Self {
            image,
            stories,
            archive,
            queries,
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

/// A script record's text: its code from the executable's message strings,
/// with the record's words as the variables (F350/F329 expand them through
/// F1344). Codes the cache does not hold show as their number.
pub fn entry_text(
    messages: &ffe_core::message_strings::Messages<'_>,
    world: &ffe_core::bulletin::World<'_>,
    entry: &ffe_core::script::menu::Entry,
) -> String {
    use ffe_core::bulletin::Var;
    let Some(bytes) = u16::try_from(entry.code)
        .ok()
        .and_then(|code| messages.get(code))
    else {
        return format!("[{:#06x}]", entry.code);
    };
    let vars = entry.vars().map(|value| {
        // An ffcode argument is another message: expand it as text.
        match u16::try_from(value).ok().filter(|code| *code >= 0x8000) {
            Some(code) => messages
                .get(code)
                .map(|text| Var::Text(String::from_utf8_lossy(text).into_owned()))
                .unwrap_or(Var::Int(i64::from(value))),
            None => Var::Int(i64::from(value)),
        }
    });
    ffe_core::bulletin::render_bytes(bytes, &vars, world)
}

/// What a selection left for the game to do.
#[derive(Debug, Default)]
pub struct Outcome {
    /// F349/F352's console lines, expanded.
    pub lines: Vec<String>,
    /// Station codes the scripts chose themselves (F360 on 0x98xx).
    pub native: Vec<i32>,
}

/// Collect the scripts' console lines and native selections.
pub fn drain(
    scripts: &mut Scripts,
    messages: &ffe_core::message_strings::Messages<'_>,
    world: &ffe_core::bulletin::World<'_>,
) -> Outcome {
    let menu = &mut scripts.engine.menu;
    Outcome {
        lines: menu
            .lines
            .drain(..)
            .map(|line| entry_text(messages, world, &line.entry))
            .collect(),
        native: menu.selected.drain(..).map(|entry| entry.code).collect(),
    }
}

/// Run `f` on the flight's scripts with hooks over the flight and the
/// navigator, collecting their console posts. `None` when the flight has no
/// scripts.
pub fn with_hooks<R>(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    f: impl FnOnce(&mut Scripts, &mut Hooks<'_>, u32) -> R,
) -> Option<(R, Vec<String>)> {
    let clock = nav.clock();
    let system = nav.current_code();
    let scripts = state.scripts.as_mut()?;
    let mut posts = Vec::new();
    let mut hooks = Hooks {
        credits: &mut state.credits,
        random: &mut nav.random,
        system,
        clock,
        posts: &mut posts,
    };
    let result = f(scripts, &mut hooks, absolute_day(clock));
    Some((result, posts))
}

/// Everything a script text's expansion reads.
pub struct Texts<'a> {
    pub assets: &'a Assets,
    pub messages: &'a ffe_core::message_strings::Messages<'a>,
    pub world: &'a ffe_core::bulletin::World<'a>,
    pub commander: &'a str,
}

/// The `[` macros over the game and the scripts (F76).
struct GameMacros<'a, 'b, 'c> {
    scripts: &'a mut Scripts,
    hooks: &'a mut Hooks<'b>,
    texts: &'a Texts<'c>,
    today: u32,
    depth: usize,
}

impl ffe_core::script::brackets::Macros for GameMacros<'_, '_, '_> {
    fn seed(&mut self, kind: u8, n: i32) -> i32 {
        match kind {
            b'R' => self.hooks.random.advance() as i32,
            b'G' => {
                let globals = self.scripts.engine.globals.clone();
                let mut draws = Draws {
                    random: &mut *self.hooks.random,
                    globals: &globals,
                };
                self.scripts.library.random_word(n, &mut draws)
            }
            // `L n` outside a journal is F95's article 0: DATA_008568.
            _ => usize::try_from(n)
                .ok()
                .and_then(|word| self.scripts.library.words.get(word))
                .copied()
                .unwrap_or(0),
        }
    }

    fn query(&mut self, word: i32, name: &str) -> Vec<u8> {
        let Some(id) = ffe_core::script::query_id(&self.texts.assets.queries, name) else {
            return Vec::new();
        };
        let entries =
            match self
                .scripts
                .query(&self.texts.assets.image, self.hooks, self.today, word, id)
            {
                Ok(entries) => entries,
                Err(error) => {
                    eprintln!("mission scripts: {error}");
                    return Vec::new();
                }
            };
        let mut out = Vec::new();
        for entry in entries {
            out.extend(
                expand_at(
                    self.scripts,
                    self.hooks,
                    self.texts,
                    self.today,
                    &entry,
                    self.depth + 1,
                )
                .into_bytes(),
            );
        }
        out
    }

    fn commander(&mut self) -> Vec<u8> {
        self.texts.commander.as_bytes().to_vec()
    }

    fn cash(&mut self) -> i64 {
        i64::from(*self.hooks.credits)
    }

    fn today(&mut self) -> u32 {
        self.today
    }

    fn name(&mut self, kind: u32, seed: i32) -> Vec<u8> {
        self.texts
            .world
            .names
            .and_then(|names| names.generate(kind, seed as u32))
            .unwrap_or_default()
    }

    fn date(&mut self, day: u32) -> Vec<u8> {
        ffe_core::bulletin::date_day_text(day.saturating_sub(0x121cf7)).into_bytes()
    }
}

/// F1344's draws: the shared pair and the script globals.
struct Draws<'a> {
    random: &'a mut Random,
    globals: &'a [i32],
}

impl ffe_core::journal_library::Context for Draws<'_> {
    fn global(&mut self, index: usize) -> i32 {
        self.globals.get(index).copied().unwrap_or(0)
    }
    fn bounded(&mut self, bound: u32) -> u32 {
        self.random.bounded(bound)
    }
    fn advance(&mut self) -> i32 {
        self.random.advance() as i32
    }
}

fn expand_at(
    scripts: &mut Scripts,
    hooks: &mut Hooks<'_>,
    texts: &Texts<'_>,
    today: u32,
    entry: &ffe_core::script::menu::Entry,
    depth: usize,
) -> String {
    let base = entry_text(texts.messages, texts.world, entry);
    if depth > 4 || !base.contains('[') {
        return base;
    }
    // F1343: DATA_008568[0] holds the code's first word while its macros
    // expand (`[MiL0(...)` names the record's own word).
    let saved = scripts.library.words[0];
    scripts.library.words[0] = entry.words[0];
    let mut macros = GameMacros {
        scripts,
        hooks,
        texts,
        today,
        depth,
    };
    let bytes: Vec<u8> = base.chars().map(|c| c as u32 as u8).collect();
    let out = ffe_core::script::brackets::expand(&bytes, &mut macros);
    macros.scripts.library.words[0] = saved;
    out.into_iter().map(char::from).collect()
}

/// A script record's full text: the code's message with its words, then its
/// `[` macros (which may ask the scripts, F64).
pub fn expand(
    scripts: &mut Scripts,
    hooks: &mut Hooks<'_>,
    texts: &Texts<'_>,
    today: u32,
    entry: &ffe_core::script::menu::Entry,
) -> String {
    expand_at(scripts, hooks, texts, today, entry, 0)
}

/// Expand `entries` at station `station` for the playable: the shared pair
/// is moved out of the navigator while its world view is borrowed.
#[allow(clippy::too_many_arguments)]
pub fn expand_all(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    station: usize,
    assets: &Assets,
    messages: &ffe_core::message_strings::Messages<'_>,
    commander: &str,
    entries: &[ffe_core::script::menu::Entry],
) -> Vec<String> {
    let clock = nav.clock();
    let system = nav.current_code();
    let mut random = std::mem::replace(&mut nav.random, Random::start());
    let today = absolute_day(clock);
    let out = {
        let Some(scripts) = state.scripts.as_mut() else {
            nav.random = random;
            return Vec::new();
        };
        let credits = &mut state.credits;
        nav.with_world(cache, station, |world| {
            let mut posts = Vec::new();
            let mut hooks = Hooks {
                credits,
                random: &mut random,
                system,
                clock,
                posts: &mut posts,
            };
            let texts = Texts {
                assets,
                messages,
                world,
                commander,
            };
            entries
                .iter()
                .map(|entry| expand(scripts, &mut hooks, &texts, today, entry))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    nav.random = random;
    out
}
