//! The mission-script interpreter: F31 (expressions), F37 (assignment), F41
//! (statements), F32/F33/F44-F48 (event subscriptions and dispatch) and
//! F51-F57 (instances and the scheduler). Game state comes through
//! [`Host`]; everything the scripts keep for themselves is in [`Engine`].
//! See `docs/notes/mission-scripts.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::menu::{self, Entry, Line, Menu};
use super::{Image, Operand, Pointer, EXPRESSION_SHAPES, TOKENS};

/// `DATA_008561`'s 150 instance slots.
pub const INSTANCES: usize = 0x96;
/// `DATA_008548..8549`: 284 bytes of global words.
pub const GLOBALS: usize = 71;
/// F58's time ticks (F60): `(period in tics, event b)`.
pub const TIME_TICKS: [(u32, i32); 3] = [(0xc22e, 5), (0x2d_82c8, 6), (0xaaa_a6e0, 7)];
/// F59's day ticks: `(period in days, event b)`.
pub const DAY_TICKS: [(u32, i32); 4] = [(1, 8), (7, 9), (0x1c, 10), (0x16d, 11)];

/// Event 1: an F21 player value changed or a time/day tick (`b` = the case).
pub const EVENT_PLAYER: i32 = 1;
/// Event 2: a global word changed (`b` = the word).
pub const EVENT_GLOBAL: i32 = 2;
/// Event 3: a mission's static word changed (`a` = the mission, `b` = word).
pub const EVENT_STATIC: i32 = 3;
/// Event 4: an instance's local word changed (`a` = the instance).
pub const EVENT_LOCAL: i32 = 4;

/// The game side of the interpreter. Every method has a neutral default so
/// a host can grow one hook at a time; `args` are the operands F31 has
/// already evaluated, in stream order.
pub trait Host {
    /// F21: player value `case` (0 ship id ... 5-11 the time in seconds,
    /// 12-20 more player state).
    fn player(&mut self, _case: i32) -> i32 {
        0
    }
    /// F22: set player value `case` (the engine raises event 1 after).
    fn set_player(&mut self, _case: i32, _value: i32) {}
    /// F1476: the next draw of the shared generator (token 0x32).
    fn random(&mut self) -> u32 {
        0
    }
    /// Any other F31 token (ships, systems, cargo, journals, ...).
    fn query(&mut self, _token: u8, _args: &[i32]) -> i32 {
        0
    }
    /// An F37 target other than tokens 3-6. Returns the event F37 raises
    /// for it, if any.
    fn assign(&mut self, _token: u8, _args: &[i32], _value: i32) -> Option<(i32, i32, i32)> {
        None
    }
    /// An F41 statement other than 0-3 and 7-9; `text` is opcode 10's
    /// inline string. The return value goes to the statement's target, if
    /// it has one.
    fn statement(&mut self, _opcode: u8, _args: &[i32], _text: &[u8]) -> i32 {
        0
    }
    /// F326's builder for `screen`: the station's own entries for it (the
    /// bulletin board's adverts, the market's rows, ...).
    fn native_entries(&mut self, _screen: i32) -> Vec<Entry> {
        Vec::new()
    }
}

/// A host that answers nothing (tests and the headless start).
pub struct NullHost;
impl Host for NullHost {}

/// One `DATA_008561` slot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// 0 free, 1 waiting, 2 running a condition, 3 entering a state, 4
    /// starting, 5 being killed, 6 to free.
    pub status: i32,
    /// `+0x06`: bumped on every start and kill.
    pub generation: u16,
    /// `+0x08`/`+0x0c`: the current state and its index.
    pub state: Option<u32>,
    pub state_index: i32,
    /// `+0x10`/`+0x14`: the state F53 enters next.
    pub next: Option<u32>,
    pub next_index: i32,
    /// `+0x18`: the mission number.
    pub mission: usize,
    /// `+0x20`: the local words (`[0]` the id, `[1]` the creator's word).
    pub locals: Vec<i32>,
}

/// A subscription's key: F42's order (event, a, b, instance, condition).
type Key = (i32, i32, i32, usize, usize);

/// Everything the scripts keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engine {
    pub globals: Vec<i32>,
    pub statics: Vec<i32>,
    pub instances: Vec<Instance>,
    /// `DATA_008550` (the day of the last time-tick reset), `8551..8553`
    /// (the time ticks) and `8554..8557` (the day ticks).
    pub timers: [u32; 8],
    /// F44's tree: each key's subscription count (saved as a list, since
    /// JSON maps need string keys).
    #[serde(with = "subscription_list")]
    subscriptions: BTreeMap<Key, u32>,
    /// The event F48 is dispatching (for the event tokens).
    #[serde(skip)]
    trigger: Option<(i32, i32, i32)>,
    /// The station menu the scripts drive (F326-F360).
    #[serde(default)]
    pub menu: Menu,
}

mod subscription_list {
    use super::Key;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer>(map: &BTreeMap<Key, u32>, out: S) -> Result<S::Ok, S::Error> {
        map.iter()
            .map(|(key, count)| (*key, *count))
            .collect::<Vec<_>>()
            .serialize(out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<BTreeMap<Key, u32>, D::Error> {
        Ok(Vec::<(Key, u32)>::deserialize(input)?.into_iter().collect())
    }
}

/// F31's event tokens whose evaluator case is empty: they stand for "this
/// event happened". The original returns an uninitialised word; the port
/// returns 1 while F48 is dispatching the matching event and 0 otherwise.
fn event_token(token: u8) -> bool {
    matches!(
        token,
        0x1c | 0x1d | 0x1e | 0x21 | 0x26 | 0x27 | 0x28 | 0x29 | 0x2c
    )
}

impl Engine {
    pub fn new(image: &Image) -> Self {
        Self {
            globals: vec![0; GLOBALS],
            statics: vec![0; image.static_words()],
            instances: vec![Instance::default(); INSTANCES],
            timers: [0; 8],
            subscriptions: BTreeMap::new(),
            trigger: None,
            menu: Menu::default(),
        }
    }

    /// The instance id dword (`+0x04`): the slot in the low word and the
    /// generation in the high word.
    pub fn id(&self, slot: usize) -> i32 {
        (u32::from(self.instances[slot].generation) << 16 | slot as u32) as i32
    }

    /// Live instances: `(slot, mission, status)`.
    pub fn live(&self) -> impl Iterator<Item = (usize, usize, i32)> + '_ {
        self.instances
            .iter()
            .enumerate()
            .filter(|(_, instance)| instance.status > 0)
            .map(|(slot, instance)| (slot, instance.mission, instance.status))
    }

    pub fn subscription_count(&self) -> u32 {
        self.subscriptions.values().sum()
    }

    // ---- F31 -------------------------------------------------------------

    /// F31: evaluate the expression at `*at` for instance `ctx`.
    pub fn evaluate(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
    ) -> Result<i32, String> {
        let token = image.byte(*at)?;
        *at += 1;
        let Some(&(expressions, dwords)) = EXPRESSION_SHAPES.get(usize::from(token)) else {
            return Ok(0);
        };
        if token < 2 {
            return Ok(0);
        }
        if dwords == 1 {
            let operand = image.dword(*at)?;
            *at += 4;
            return Ok(match token {
                2 => operand,
                3 => host.player(operand),
                4 => self.globals.get(operand as usize).copied().unwrap_or(0),
                5 => self
                    .static_word(image, ctx, operand)?
                    .map_or(0, |i| self.statics[i]),
                _ => self.local(ctx, operand),
            });
        }
        if token == 0x32 {
            // F1476 first, then the bound (`random % bound`, unsigned).
            let draw = host.random();
            let bound = self.evaluate(image, host, at, ctx)? as u32;
            return Ok(draw.checked_rem(bound).unwrap_or(0) as i32);
        }
        let mut args = [0i32; 9];
        for slot in args.iter_mut().take(usize::from(expressions)) {
            *slot = self.evaluate(image, host, at, ctx)?;
        }
        let (x, y) = (args[0], args[1]);
        Ok(match token {
            7 => x.wrapping_neg(),
            8 => !x,
            9 => i32::from(x == 0),
            0x0a => x.wrapping_mul(y),
            0x0b => x.checked_div(y).unwrap_or(0),
            0x0c => x.checked_rem(y).unwrap_or(0),
            0x0d => x.wrapping_add(y),
            0x0e => x.wrapping_sub(y),
            0x0f => x.wrapping_shl(y as u32 & 31),
            0x10 => x.wrapping_shr(y as u32 & 31),
            0x11 => i32::from(x < y),
            0x12 => i32::from(x <= y),
            0x13 => i32::from(x > y),
            0x14 => i32::from(x >= y),
            0x15 => i32::from(x == y),
            0x16 => i32::from(x != y),
            0x17 => x & y,
            0x18 => x ^ y,
            0x19 => x | y,
            0x1a => i32::from(x != 0 && y != 0),
            0x1b => i32::from(x != 0 || y != 0),
            0x2a | 0x2b => 0,
            t if event_token(t) => {
                let (_, event, a_mode, _) = TOKENS[usize::from(t)];
                let key_a = if a_mode == 3 { x } else { 0 };
                let key_b = if a_mode == 3 { y } else { x };
                i32::from(self.trigger == Some((i32::from(event), key_a, key_b)))
            }
            _ => host.query(token, &args[..usize::from(expressions)]),
        })
    }

    fn local(&self, ctx: usize, index: i32) -> i32 {
        self.instances
            .get(ctx)
            .and_then(|instance| instance.locals.get(index as usize))
            .copied()
            .unwrap_or(0)
    }

    /// The static word index of `[ctx's mission]+0x14 [index]`.
    fn static_word(&self, image: &Image, ctx: usize, index: i32) -> Result<Option<usize>, String> {
        let Some(instance) = self.instances.get(ctx) else {
            return Ok(None);
        };
        let mission = image.mission_struct(instance.mission)?;
        Ok(match mission.statics {
            Pointer::Static(base) => usize::try_from(i64::from(base) + i64::from(index))
                .ok()
                .filter(|word| *word < self.statics.len()),
            _ => None,
        })
    }

    /// A mission's identity as an event key (its struct offset).
    fn mission_key(&self, image: &Image, ctx: usize) -> i32 {
        self.instances
            .get(ctx)
            .and_then(|instance| image.mission_struct(instance.mission).ok())
            .map_or(0, |mission| mission.base as i32)
    }

    // ---- F37 -------------------------------------------------------------

    /// F37: assign `value` to the target at `*at`.
    pub fn assign(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
        value: i32,
    ) -> Result<(), String> {
        let token = image.byte(*at)?;
        *at += 1;
        match token {
            3..=6 => {
                let index = image.dword(*at)?;
                *at += 4;
                match token {
                    3 => {
                        host.set_player(index, value);
                        self.raise(image, host, EVENT_PLAYER, 0, index)?;
                    }
                    4 => {
                        if let Some(word) = self.globals.get_mut(index as usize) {
                            *word = value;
                        }
                        self.raise(image, host, EVENT_GLOBAL, 0, index)?;
                    }
                    5 => {
                        if let Some(word) = self.static_word(image, ctx, index)? {
                            self.statics[word] = value;
                        }
                        let mission = self.mission_key(image, ctx);
                        self.raise(image, host, EVENT_STATIC, mission, index)?;
                    }
                    _ => {
                        if let Some(word) = self
                            .instances
                            .get_mut(ctx)
                            .and_then(|instance| instance.locals.get_mut(index as usize))
                        {
                            *word = value;
                        }
                        self.raise(image, host, EVENT_LOCAL, ctx as i32, index)?;
                    }
                }
            }
            _ => {
                let Some(shape) = super::target_shape(token) else {
                    return Ok(());
                };
                let mut args = [0i32; 2];
                for slot in args.iter_mut().take(shape.len()) {
                    *slot = self.evaluate(image, host, at, ctx)?;
                }
                if let Some((event, a, b)) = host.assign(token, &args[..shape.len()], value) {
                    self.raise(image, host, event, a, b)?;
                }
            }
        }
        Ok(())
    }

    // ---- F41 -------------------------------------------------------------

    /// F41: run the statements at `*at` for instance `ctx` until opcode 0
    /// or a statement that stops the stream.
    pub fn execute(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
    ) -> Result<(), String> {
        // F41's 48-byte record is a local: a field a statement does not set
        // keeps what an earlier statement of the same stream left there.
        let mut record = Entry::default();
        for _ in 0..0x10_0000 {
            let opcode = image.byte(*at)?;
            *at += 1;
            if let Some(stop) = self.menu_statement(image, host, at, ctx, opcode, &mut record)? {
                if stop {
                    return Ok(());
                }
                continue;
            }
            match opcode {
                0 => return Ok(()),
                1 => {
                    let mission = image.dword(*at)?;
                    *at += 4;
                    let creator = self.id(ctx);
                    let id = self.start(image, host, at, ctx, mission as usize, creator)?;
                    self.assign(image, host, at, ctx, id)?;
                }
                2 => {
                    if self.evaluate(image, host, at, ctx)? != 0 {
                        *at += 4;
                    } else {
                        let offset = image.dword(*at)?;
                        *at = at.wrapping_add(offset as u32);
                    }
                }
                3 => {
                    let offset = image.dword(*at)?;
                    *at = at.wrapping_add(offset as u32);
                }
                7 => {
                    let value = self.evaluate(image, host, at, ctx)?;
                    self.assign(image, host, at, ctx, value)?;
                }
                8 => {
                    let id = self.evaluate(image, host, at, ctx)?;
                    self.kill(image, host, id)?;
                    if id == self.id(ctx) {
                        return Ok(());
                    }
                }
                9 => {
                    let index = image.dword(*at)?;
                    let mission = image.mission_struct(self.instances[ctx].mission)?;
                    let instance = &mut self.instances[ctx];
                    instance.next_index = index;
                    instance.next = Some(image.state(&mission, index)?);
                    instance.status = 4;
                    return Ok(());
                }
                _ => {
                    let Some(shape) = super::statement_shape(opcode) else {
                        continue;
                    };
                    let mut args = Vec::with_capacity(shape.len());
                    let mut text = Vec::new();
                    let mut target = None;
                    for operand in shape {
                        match operand {
                            Operand::Expression => args.push(self.evaluate(image, host, at, ctx)?),
                            Operand::String => {
                                while image.byte(*at)? != 0 {
                                    text.push(image.byte(*at)?);
                                    *at += 1;
                                }
                                *at += 1;
                            }
                            Operand::Target => target = Some(*at),
                            Operand::Dword | Operand::Parameters => {}
                        }
                    }
                    let result = host.statement(opcode, &args, &text);
                    if let Some(mut target) = target {
                        self.assign(image, host, &mut target, ctx, result)?;
                        *at = target;
                    }
                }
            }
        }
        Err("statement stream does not end".to_owned())
    }

    // ---- F326-F360: the station menu ---------------------------------------

    /// F34: set global `index` and raise its event.
    pub fn set_global(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        index: usize,
        value: i32,
    ) -> Result<(), String> {
        if let Some(word) = self.globals.get_mut(index) {
            *word = value;
        }
        self.raise(image, host, EVENT_GLOBAL, 0, index as i32)
    }

    /// A game signal: global `index` set to `value` and back to 0 (F453's
    /// bulletin-board 1, F493's military 3, F381's launch request, ...).
    pub fn pulse(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        index: usize,
        value: i32,
    ) -> Result<(), String> {
        self.set_global(image, host, index, value)?;
        self.set_global(image, host, index, 0)
    }

    /// F342: signal 2 on global 0, then an empty menu.
    pub fn clear_menu(&mut self, image: &Image, host: &mut dyn Host) -> Result<(), String> {
        self.pulse(image, host, 0, 2)?;
        self.menu.clear();
        Ok(())
    }

    /// F326: show the current screen again. The list screens rebuild their
    /// entries: the bulletin board (2, F453) and the military office (7,
    /// F493) clear the menu (F331/F342's signal 2), take the station's
    /// entries and then signal the scripts (1 and 3) to add theirs; the
    /// market (3), ship lists (5, 6) clear and refill; repairs (4) and the
    /// shipyard (8) refill. Every other screen (1 and 11, F541, and F335)
    /// only redraws what the scripts put there.
    pub fn build_screen(&mut self, image: &Image, host: &mut dyn Host) -> Result<(), String> {
        let screen = self.menu.screen;
        self.menu.rebuild = true;
        if !(2..=8).contains(&screen) {
            return Ok(());
        }
        if matches!(screen, 2 | 3 | 5 | 6 | 7) {
            self.clear_menu(image, host)?;
        }
        let mut entries = host.native_entries(screen);
        entries.truncate(menu::ENTRIES);
        self.menu.entries = entries;
        match screen {
            2 => {
                self.pulse(image, host, 0, 1)?;
                self.menu.header = Entry {
                    code: if self.menu.entries.is_empty() {
                        0x98de
                    } else {
                        0x98dd
                    },
                    ..Entry::default()
                };
            }
            7 => self.pulse(image, host, 0, 3)?,
            _ => {}
        }
        Ok(())
    }

    /// F360: the scripts (or the player) choose `entry`.
    pub fn select(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        entry: Entry,
    ) -> Result<(), String> {
        match menu::selection(&entry) {
            None => self.menu.selected.push(entry),
            Some(writes) => {
                for (index, value) in writes {
                    self.set_global(image, host, index, value)?;
                }
            }
        }
        Ok(())
    }

    /// The menu statements (F41's 4-6, 22-25, 27-34, 44, 46, 55, 56, 59):
    /// `Some(stop)` when `opcode` is one of them.
    fn menu_statement(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
        opcode: u8,
        record: &mut Entry,
    ) -> Result<Option<bool>, String> {
        let mut read = |engine: &mut Self, at: &mut u32, n: usize| -> Result<[i32; 6], String> {
            let mut values = [0; 6];
            for value in values.iter_mut().take(n) {
                *value = engine.evaluate(image, host, at, ctx)?;
            }
            Ok(values)
        };
        let set = |record: &mut Entry, values: &[i32]| {
            record.code = values[0];
            for (word, value) in record.words.iter_mut().zip(&values[1..]) {
                *word = *value;
            }
        };
        match opcode {
            4 => {
                let v = read(self, at, 6)?;
                let menu = &mut self.menu;
                menu.active = true;
                menu.screen = v[0];
                menu.title = v[1];
                menu.words = [v[3], v[4], v[5]];
                if v[2] != 0 {
                    record.code = v[2];
                    menu.lines.push(Line {
                        entry: Entry {
                            flags: -1,
                            ..*record
                        },
                        new: false,
                    });
                }
                self.clear_menu(image, host)?;
            }
            5 => self.clear_menu(image, host)?,
            6 => self.build_screen(image, host)?,
            22 | 28 => {
                let n = if opcode == 22 { 1 } else { 2 };
                let v = read(self, at, n)?;
                record.flags = -1;
                set(record, &v[..n]);
                self.menu.lines.push(Line {
                    entry: *record,
                    new: true,
                });
            }
            23 | 27 | 55 | 56 => {
                let n = match opcode {
                    23 => 1,
                    27 => 2,
                    55 => 3,
                    _ => 4,
                };
                let v = read(self, at, n)?;
                set(record, &v[..n]);
                self.menu.append(*record);
            }
            24 | 44 => {
                let n = if opcode == 24 { 1 } else { 2 };
                let v = read(self, at, n)?;
                set(record, &v[..n]);
                self.menu.insert(*record);
            }
            25 => {
                let v = read(self, at, 1)?;
                record.code = v[0];
                record.flags = 0;
                self.select(image, host, *record)?;
            }
            59 => {
                let v = read(self, at, 5)?;
                set(record, &v[..4]);
                record.extra[0] = v[4];
                record.flags = 0;
                self.select(image, host, *record)?;
            }
            29..=31 => {
                let n = usize::from(opcode - 28);
                let v = read(self, at, n)?;
                record.flags = -1;
                set(record, &v[..n]);
                self.menu.lines.push(Line {
                    entry: *record,
                    new: false,
                });
            }
            32 | 46 => {
                let n = if opcode == 32 { 1 } else { 2 };
                let v = read(self, at, n)?;
                record.flags = -1;
                set(record, &v[..n]);
                self.menu.header = *record;
            }
            33 => {
                let v = read(self, at, 2)?;
                self.close_timed(image, host)?;
                self.menu.timed = true;
                self.menu.timer = v[1].wrapping_mul(0xc22e);
                self.menu.screen = v[0];
                self.menu.title = 0;
                self.clear_menu(image, host)?;
            }
            34 => self.close_timed(image, host)?,
            _ => return Ok(None),
        }
        Ok(Some(false))
    }

    /// F359: a timed screen signals 2 on global 0 when it closes.
    fn close_timed(&mut self, image: &Image, host: &mut dyn Host) -> Result<(), String> {
        if self.menu.timed {
            self.pulse(image, host, 0, 2)?;
        }
        self.menu.timed = false;
        Ok(())
    }

    // ---- F51/F52/F38 -------------------------------------------------------

    /// F51: claim a free slot for `mission`, or `None` with all 150 in use.
    fn claim(&mut self, image: &Image, mission: usize, word: i32) -> Result<Option<usize>, String> {
        let record = image.mission_struct(mission)?;
        let Some(slot) = self
            .instances
            .iter()
            .position(|instance| instance.status == 0)
        else {
            return Ok(None);
        };
        let locals = usize::try_from(record.locals).unwrap_or(0).max(2);
        let instance = &mut self.instances[slot];
        instance.status = 4;
        instance.generation = instance.generation.wrapping_add(1);
        instance.state = None;
        instance.state_index = -1;
        instance.next = None;
        instance.next_index = -1;
        instance.mission = mission;
        instance.locals = vec![0; locals];
        let id = self.id(slot);
        self.instances[slot].locals[0] = id;
        self.instances[slot].locals[1] = word;
        Ok(Some(slot))
    }

    /// F52: start `mission` with its parameters read from `*at` (evaluated
    /// for `ctx`) and the creator's `word`; returns the new instance id.
    pub fn start(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
        mission: usize,
        word: i32,
    ) -> Result<i32, String> {
        let record = image.mission_struct(mission)?;
        let Some(slot) = self.claim(image, mission, word)? else {
            return Ok(0);
        };
        let mut target = match record.parameters {
            Pointer::Blob(stream) => Some(stream),
            _ => None,
        };
        for _ in 0..record.parameter_count.max(0) {
            let value = self.evaluate(image, host, at, ctx)?;
            if let Some(stream) = target.as_mut() {
                self.assign(image, host, stream, slot, value)?;
            }
        }
        let instance = &mut self.instances[slot];
        instance.next = Some(image.state(&record, 0)?);
        instance.next_index = 0;
        Ok(self.id(slot))
    }

    /// F38: kill instance `id` (when it is the live instance in its slot).
    pub fn kill(&mut self, image: &Image, host: &mut dyn Host, id: i32) -> Result<(), String> {
        let slot = (id & 0xffff) as usize;
        if slot >= INSTANCES || self.id(slot) != id {
            return Ok(());
        }
        if !(1..5).contains(&self.instances[slot].status) {
            return Ok(());
        }
        self.unsubscribe(image, host, slot)?;
        let mission = image.mission_struct(self.instances[slot].mission)?;
        let instance = &mut self.instances[slot];
        if mission.kill_state >= 0 {
            instance.status = 5;
        } else {
            instance.generation = instance.generation.wrapping_add(1);
            instance.status = 6;
        }
        Ok(())
    }

    // ---- F32/F33 and F44/F45/F48 -----------------------------------------

    /// F32: walk the condition tree at `*at`, adding (or removing) a
    /// subscription for every event token in it.
    fn walk(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        at: &mut u32,
        ctx: usize,
        condition: usize,
        add: bool,
    ) -> Result<(), String> {
        let token = image.byte(*at)?;
        *at += 1;
        let (children, event, a_mode, b_mode) = TOKENS
            .get(usize::from(token))
            .copied()
            .unwrap_or((0, 0, 0, 0));
        if event > 0 {
            let resume = *at;
            let a = match a_mode {
                1 => self.mission_key(image, ctx),
                2 => ctx as i32,
                3 => self.evaluate(image, host, at, ctx)?,
                _ => 0,
            };
            let b = match b_mode {
                1 => {
                    let value = image.dword(*at)?;
                    *at += 4;
                    value
                }
                2 => self.evaluate(image, host, at, ctx)?,
                _ => 0,
            };
            let key = (i32::from(event), a, b, ctx, condition);
            if add {
                *self.subscriptions.entry(key).or_insert(0) += 1;
            } else if let Some(count) = self.subscriptions.get_mut(&key) {
                *count -= 1;
                if *count == 0 {
                    self.subscriptions.remove(&key);
                }
            }
            *at = resume;
        }
        if children > 0 {
            for _ in 0..children {
                self.walk(image, host, at, ctx, condition, add)?;
            }
        } else {
            *at += 4;
        }
        Ok(())
    }

    /// F33: subscribe (or unsubscribe) every condition of `slot`'s state.
    fn subscribe_state(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        slot: usize,
        add: bool,
    ) -> Result<(), String> {
        let Some(state) = self.instances[slot].state else {
            return Ok(());
        };
        for condition in 0..image.conditions(state)? {
            let mut at = image.condition(state, condition)?;
            self.walk(image, host, &mut at, slot, condition, add)?;
        }
        Ok(())
    }

    fn unsubscribe(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        slot: usize,
    ) -> Result<(), String> {
        if self.instances[slot].state.is_some() {
            self.subscribe_state(image, host, slot, false)?;
            let instance = &mut self.instances[slot];
            instance.state = None;
            instance.state_index = -1;
        }
        Ok(())
    }

    /// F48: run every condition subscribed to `(event, a, b)` (in F42's
    /// order), then the scheduler when any matched.
    pub fn raise(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        event: i32,
        a: i32,
        b: i32,
    ) -> Result<(), String> {
        let low = (event, a, b, 0, 0);
        let high = (event, a, b, usize::MAX, usize::MAX);
        let matched: Vec<(usize, usize, u32)> = self
            .subscriptions
            .range(low..=high)
            .map(|(&(_, _, _, slot, condition), &count)| (slot, condition, count))
            .collect();
        if matched.is_empty() {
            return Ok(());
        }
        let outer = self.trigger.replace((event, a, b));
        for (slot, condition, count) in matched {
            for _ in 0..count {
                self.check(image, host, slot, condition)?;
            }
        }
        self.trigger = outer;
        self.schedule(image, host)
    }

    /// F47: re-check condition `condition` of a waiting instance.
    fn check(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        slot: usize,
        condition: usize,
    ) -> Result<(), String> {
        let Some(state) = self.instances[slot].state else {
            return Ok(());
        };
        if self.instances[slot].status != 1 || condition >= image.conditions(state)? {
            return Ok(());
        }
        let mut at = image.condition(state, condition)?;
        if self.evaluate(image, host, &mut at, slot)? != 0 {
            self.instances[slot].status = 2;
            self.execute(image, host, &mut at, slot)?;
            if self.instances[slot].status == 2 {
                self.instances[slot].status = 1;
            }
        }
        Ok(())
    }

    // ---- F53/F54 ---------------------------------------------------------

    /// F53: enter `slot`'s next state: run its entry stream, then check its
    /// conditions once (after one fires the scan resumes at condition 1),
    /// then subscribe them.
    fn enter(&mut self, image: &Image, host: &mut dyn Host, slot: usize) -> Result<(), String> {
        self.instances[slot].status = 3;
        self.unsubscribe(image, host, slot)?;
        let Some(next) = self.instances[slot].next else {
            self.instances[slot].status = 6;
            return Ok(());
        };
        if let Some(mut at) = image.entry(next)? {
            self.execute(image, host, &mut at, slot)?;
        }
        if self.instances[slot].status != 3 {
            return Ok(());
        }
        let mut condition = 0;
        while condition < image.conditions(next)? {
            let mut at = image.condition(next, condition)?;
            if self.evaluate(image, host, &mut at, slot)? != 0 {
                self.execute(image, host, &mut at, slot)?;
                if self.instances[slot].status != 3 {
                    return Ok(());
                }
                condition = 0;
            }
            condition += 1;
        }
        let instance = &mut self.instances[slot];
        instance.state = instance.next;
        instance.state_index = instance.next_index;
        self.subscribe_state(image, host, slot, true)?;
        self.instances[slot].status = 1;
        Ok(())
    }

    /// F54: run the instances above status 3, highest status first (the
    /// lowest slot among equals), until none is left.
    pub fn schedule(&mut self, image: &Image, host: &mut dyn Host) -> Result<(), String> {
        for _ in 0..0x10000 {
            let mut best = (0, 0);
            for (slot, instance) in self.instances.iter().enumerate() {
                if instance.status > best.0 {
                    best = (instance.status, slot);
                }
            }
            let (status, slot) = best;
            match status {
                4 => self.enter(image, host, slot)?,
                5 => {
                    self.instances[slot].status = 6;
                    let mission = image.mission_struct(self.instances[slot].mission)?;
                    let state = image.state(&mission, mission.kill_state)?;
                    if let Some(mut at) = image.entry(state)? {
                        self.execute(image, host, &mut at, slot)?;
                    }
                }
                6 => {
                    self.instances[slot].status = 0;
                    self.instances[slot].locals.clear();
                }
                _ => return Ok(()),
            }
        }
        Err("the script scheduler does not settle".to_owned())
    }

    // ---- F55-F61 ---------------------------------------------------------

    /// F55 (F57's new-game event 4): clear the timers, start mission 0 and
    /// run the scheduler.
    pub fn new_game(&mut self, image: &Image, host: &mut dyn Host) -> Result<(), String> {
        self.timers = [0; 8];
        let mut nothing = 0;
        self.start(image, host, &mut nothing, 0, 0, 0)?;
        self.schedule(image, host)
    }

    /// F56 (F57's event 1): free every instance and drop the subscriptions.
    pub fn clear(&mut self) {
        for instance in &mut self.instances {
            if instance.status > 0 {
                instance.status = 0;
                instance.locals.clear();
            }
        }
        self.subscriptions.clear();
    }

    /// F58: raise event 1 with `b` when `now` reaches `timer`, then move the
    /// timer to the next multiple of `period`.
    fn tick(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        now: u32,
        timer: usize,
        period: u32,
        b: i32,
    ) -> Result<(), String> {
        if now >= self.timers[timer] {
            self.raise(image, host, EVENT_PLAYER, 0, b)?;
            let next = now.wrapping_add(period);
            self.timers[timer] = next - next % period;
        }
        Ok(())
    }

    /// F60: the time ticks for game day `day` at `tics` into it.
    pub fn time_ticks(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        day: u32,
        tics: u32,
    ) -> Result<(), String> {
        if day > self.timers[0] {
            self.timers[1] = 0;
            self.timers[2] = 0;
            self.timers[3] = 0;
            self.timers[0] = day;
        }
        for (index, (period, b)) in TIME_TICKS.into_iter().enumerate() {
            self.tick(image, host, tics, 1 + index, period, b)?;
        }
        Ok(())
    }

    /// F59 on the daily message (15): the day ticks.
    pub fn day_ticks(
        &mut self,
        image: &Image,
        host: &mut dyn Host,
        day: u32,
    ) -> Result<(), String> {
        for (index, (period, b)) in DAY_TICKS.into_iter().enumerate() {
            self.tick(image, host, day, 4 + index, period, b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::tests::tiny;

    #[derive(Default)]
    struct Recorder {
        statements: Vec<(u8, Vec<i32>)>,
    }

    impl Host for Recorder {
        fn statement(&mut self, opcode: u8, args: &[i32], _text: &[u8]) -> i32 {
            self.statements.push((opcode, args.to_vec()));
            0
        }
    }

    #[test]
    fn a_new_game_enters_state_zero_and_waits_on_its_condition() {
        // Entry: global 3 = 5 + local 1. Condition: global 3 > 6 -> F349
        // page (opcode 22) with 7, then go to state 0 again.
        let entry = [7, 0x0d, 2, 5, 0, 0, 0, 6, 1, 0, 0, 0, 4, 3, 0, 0, 0, 0];
        let condition = [0x13, 4, 3, 0, 0, 0, 2, 6, 0, 0, 0, 22, 2, 7, 0, 0, 0, 0];
        let image = Image::new(&tiny(&entry, &condition)).unwrap();
        let mut engine = Engine::new(&image);
        let mut host = Recorder::default();
        engine.new_game(&image, &mut host).unwrap();
        assert_eq!(engine.globals[3], 5);
        assert_eq!(engine.live().collect::<Vec<_>>(), vec![(0, 0, 1)]);
        assert_eq!(engine.instances[0].locals[0], engine.id(0));
        // The condition reads global 3, so it waits on event 2 for word 3.
        assert_eq!(engine.subscription_count(), 1);
        assert!(host.statements.is_empty());
        // A script (or the game) setting the word re-checks it.
        engine.globals[3] = 9;
        engine.raise(&image, &mut host, EVENT_GLOBAL, 0, 3).unwrap();
        // Opcode 22 is F349: a new console line for code 7.
        assert_eq!(engine.menu.lines.len(), 1);
        assert_eq!(engine.menu.lines[0].entry.code, 7);
        assert!(engine.menu.lines[0].new);
        assert!(host.statements.is_empty());
    }

    #[test]
    fn the_scheduler_frees_a_killed_instance() {
        // Entry: kill local 0 (the instance's own id).
        let entry = [8, 6, 0, 0, 0, 0, 0];
        let image = Image::new(&tiny(&entry, &[2, 0, 0, 0, 0, 0])).unwrap();
        let mut engine = Engine::new(&image);
        engine.new_game(&image, &mut NullHost).unwrap();
        assert_eq!(engine.live().count(), 0);
        assert_eq!(engine.instances[0].generation, 2);
    }

    #[test]
    fn day_and_time_ticks_follow_f58() {
        let image = Image::new(&tiny(&[0], &[2, 0, 0, 0, 0, 0])).unwrap();
        let mut engine = Engine::new(&image);
        engine.day_ticks(&image, &mut NullHost, 10).unwrap();
        // Daily: next day; weekly: the next multiple of 7.
        assert_eq!(&engine.timers[4..8], &[11, 14, 28, 365]);
        engine
            .time_ticks(&image, &mut NullHost, 3, 0x1_0000)
            .unwrap();
        assert_eq!(engine.timers[0], 3);
        assert_eq!(
            engine.timers[1],
            0x1_0000 + 0xc22e - (0x1_0000 + 0xc22e) % 0xc22e
        );
    }
}
