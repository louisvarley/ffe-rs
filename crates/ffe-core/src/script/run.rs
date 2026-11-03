//! The scripts and the journals together: [`Scripts`] owns the interpreter
//! state and the journal library, answers the statements and tokens that
//! belong to the journals (statements 10-18, tokens 0x20, 0x2d, 0x2e) and
//! passes everything else to the [`Game`].

use serde::{Deserialize, Serialize};

use super::engine::{Engine, Host};
use super::Image;
use crate::journal::Story;
use crate::journal_library::{self, Library, Published};

/// The game side: the engine's hooks plus what the journals need.
pub trait Game: Host {
    /// F922: a uniform draw below `bound` from the shared pair.
    fn bounded(&mut self, _bound: u32) -> u32 {
        0
    }
    /// F1530: advance the shared pair, returning its first word.
    fn advance(&mut self) -> i32 {
        0
    }
    /// A console line (F89's `0xa600`).
    fn post(&mut self, _text: &str) {}
}

/// The persisted script and journal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scripts {
    pub engine: Engine,
    pub library: Library,
}

/// The engine's host while scripts run: journal statements are answered
/// here, everything else goes to the game.
struct Bridge<'a> {
    library: &'a mut Library,
    game: &'a mut dyn Game,
    today: u32,
}

struct Draws<'a> {
    game: &'a mut dyn Game,
    globals: &'a [i32],
}

impl journal_library::Context for Draws<'_> {
    fn global(&mut self, index: usize) -> i32 {
        self.globals.get(index).copied().unwrap_or(0)
    }
    fn bounded(&mut self, bound: u32) -> u32 {
        self.game.bounded(bound)
    }
    fn advance(&mut self) -> i32 {
        self.game.advance()
    }
}

impl Host for Bridge<'_> {
    fn player(&mut self, case: i32) -> i32 {
        self.game.player(case)
    }
    fn set_player(&mut self, case: i32, value: i32) {
        self.game.set_player(case, value);
    }
    fn random(&mut self) -> u32 {
        self.game.random()
    }
    fn query(&mut self, token: u8, args: &[i32]) -> i32 {
        let arg = |n: usize| args.get(n).copied().unwrap_or(0);
        let mut draws = Draws {
            game: &mut *self.game,
            globals: &[],
        };
        match token {
            0x20 => i32::from(self.library.in_issue(arg(0), arg(1))),
            0x2d => self.library.random_word(arg(0), &mut draws),
            0x2e => self
                .library
                .article_word(arg(0), arg(1), arg(2), &mut draws),
            _ => self.game.query(token, args),
        }
    }
    fn assign(&mut self, token: u8, args: &[i32], value: i32) -> Option<(i32, i32, i32)> {
        self.game.assign(token, args, value)
    }
    fn native_entries(&mut self, screen: i32) -> Vec<crate::script::menu::Entry> {
        self.game.native_entries(screen)
    }
    fn statement(&mut self, opcode: u8, args: &[i32], text: &[u8]) -> i32 {
        let arg = |n: usize| args.get(n).copied().unwrap_or(0);
        let library = &mut *self.library;
        match opcode {
            10 => library.create(
                text,
                arg(0),
                arg(1) as u32,
                arg(2) as u32,
                arg(3) as u32,
                self.today,
            ),
            11 => {
                library.set_delivered(arg(0), true);
                0
            }
            12 => {
                library.set_delivered(arg(0), false);
                0
            }
            13 => {
                library.end(arg(0));
                0
            }
            14 => {
                library.queue(arg(0), arg(1), arg(2), 0, 0);
                0
            }
            15 => {
                library.remove(arg(0), arg(1));
                0
            }
            16 => {
                library.replace(arg(0), arg(1), arg(2));
                0
            }
            17 => {
                library.subscribe(arg(0), arg(1) as u32, self.today);
                0
            }
            18 => {
                library.unsubscribe(arg(0));
                0
            }
            _ => self.game.statement(opcode, args, text),
        }
    }
}

/// F953's script globals: the damage, the object and the kind.
pub const DAMAGE_AMOUNT: usize = 0xc;
pub const DAMAGE_OBJECT: usize = 0xd;
pub const DAMAGE_KIND: usize = 0xe;

impl Scripts {
    pub fn new(image: &Image) -> Self {
        Self {
            engine: Engine::new(image),
            library: Library::default(),
        }
    }

    fn bridge<'a>(library: &'a mut Library, game: &'a mut dyn Game, today: u32) -> Bridge<'a> {
        Bridge {
            library,
            game,
            today,
        }
    }

    /// Raise the scripts' event 7 for each published article and post its
    /// notice.
    fn announce(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        published: Vec<Published>,
    ) -> Result<(), String> {
        for issue in published {
            let mut bridge = Self::bridge(&mut self.library, game, today);
            self.engine.raise(
                image,
                &mut bridge,
                journal_library::EVENT_PUBLISHED,
                issue.journal as i32,
                issue.article,
            )?;
            if let Some(notice) = issue.notice {
                game.post(&notice);
            }
        }
        Ok(())
    }

    /// F57's new-game message (4): F55 starts mission 0, then F68 publishes
    /// each journal's first issue.
    pub fn new_game(
        &mut self,
        image: &Image,
        stories: &[Story],
        game: &mut dyn Game,
        today: u32,
        docked: bool,
    ) -> Result<(), String> {
        self.engine.clear();
        self.library.reset();
        {
            let mut bridge = Self::bridge(&mut self.library, game, today);
            self.engine.new_game(image, &mut bridge)?;
        }
        let published = {
            let mut draws = Draws {
                game: &mut *game,
                globals: &self.engine.globals,
            };
            self.library.start(stories, today, docked, &mut draws)
        };
        self.announce(image, game, today, published)
    }

    /// The daily message (15): F59's day ticks, then F90's issues.
    pub fn daily(
        &mut self,
        image: &Image,
        stories: &[Story],
        game: &mut dyn Game,
        today: u32,
        docked: bool,
    ) -> Result<(), String> {
        {
            let mut bridge = Self::bridge(&mut self.library, game, today);
            self.engine.day_ticks(image, &mut bridge, today)?;
        }
        let published = {
            let mut draws = Draws {
                game: &mut *game,
                globals: &self.engine.globals,
            };
            self.library.daily(stories, today, docked, &mut draws)
        };
        self.announce(image, game, today, published)
    }

    /// F60 each frame: the time ticks.
    pub fn time(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        tics: u32,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.time_ticks(image, &mut bridge, today, tics)
    }

    /// The player opens station screen `screen` (F326's builder, e.g. 2 the
    /// bulletin board): the station's entries and the scripts' own.
    pub fn open_screen(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        screen: i32,
    ) -> Result<(), String> {
        self.engine.menu.screen = screen;
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.build_screen(image, &mut bridge)
    }

    /// The player chooses `entry` from the menu (F361/F360).
    pub fn select(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        entry: crate::script::menu::Entry,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.select(image, &mut bridge, entry)
    }

    /// F64's query (`[Mi...(name)`): the text records the scripts write.
    pub fn query(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        word: i32,
        id: i32,
    ) -> Result<Vec<crate::script::menu::Entry>, String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.query(image, &mut bridge, word, id)
    }

    /// F924/F61: object `id` left the game.
    pub fn object_removed(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        id: i32,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.object_removed(image, &mut bridge, id)
    }

    /// A game action the scripts may veto (see [`Engine::permits`]).
    pub fn permits(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        index: usize,
        value: i32,
    ) -> Result<bool, String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.permits(image, &mut bridge, index, value)
    }

    /// A game signal on a script global (set, then back to 0).
    pub fn pulse(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        index: usize,
        value: i32,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.pulse(image, &mut bridge, index, value)
    }

    /// F34 from the game: set global `index` and raise its event.
    pub fn set_global(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        index: usize,
        value: i32,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.set_global(image, &mut bridge, index, value)
    }

    /// F35: global `index` (0 outside the 71 words).
    pub fn global(&self, index: usize) -> i32 {
        self.engine.globals.get(index).copied().unwrap_or(0)
    }

    /// F953's signal before any damage: global 0xd the object's id, 0xe the
    /// damage kind and 0xc the amount, each raising its event; the damage
    /// dealt is global 0xc as the scripts leave it.
    pub fn damage(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        object: i32,
        kind: i32,
        amount: i32,
    ) -> Result<i32, String> {
        self.set_global(image, game, today, DAMAGE_OBJECT, object)?;
        self.set_global(image, game, today, DAMAGE_KIND, kind)?;
        self.set_global(image, game, today, DAMAGE_AMOUNT, amount)?;
        Ok(self.global(DAMAGE_AMOUNT))
    }

    /// F90's docking message (5).
    pub fn docked(&mut self) {
        self.library.docked();
    }

    /// F48 from the game: a game event the scripts may be waiting on.
    pub fn raise(
        &mut self,
        image: &Image,
        game: &mut dyn Game,
        today: u32,
        event: i32,
        a: i32,
        b: i32,
    ) -> Result<(), String> {
        let mut bridge = Self::bridge(&mut self.library, game, today);
        self.engine.raise(image, &mut bridge, event, a, b)
    }
}
