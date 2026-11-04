//! The playable's side of the mission scripts and the journals: the cache's
//! script image, stories and journal archive, the [`Game`] hooks the scripts
//! call, and the game events that feed them (new game, each new day, docking).
//! See `docs/notes/mission-scripts.md` and `docs/notes/journal.md`.
//!
//! The hooks answer the scripts' queries from the flight and the navigator
//! and queue what their object statements ask for ([`Action`]), which
//! [`apply`] carries out after each run: the ships and objects they make,
//! place, steer, damage and name, their canisters and contracts.

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

/// The player's object id as the scripts see it.
pub const PLAYER_ID: i32 = 1;
/// A navigator ship's id as the scripts see it: `ship.id + SHIP_IDS`.
pub const SHIP_IDS: i32 = 0x10;
/// A system body's id: `BODY_IDS + index` (the original's are object slots;
/// the port keeps bodies, ships and the scripts' canisters apart).
pub const BODY_IDS: i32 = 0x1000;
/// The first id of the canisters the scripts make (statements 52 and 58).
pub const TAG_IDS: i32 = 0x2000;

/// F702/F1507's packed distance (statements 54 and 50): an ffp number, the
/// signed mantissa in the low word and the exponent in the high word.
pub fn packed_ffp(value: i32) -> ffe_core::ffp::Ffp {
    ffe_core::ffp::Ffp::new(value as i16, (value >> 16) as i16)
}

/// [`packed_ffp`] in millimetres: F1507 multiplies F1506's Q31 direction by
/// the mantissa and shifts by `exponent - 0x2e`, so the distance is
/// `mantissa * 2^(exponent - 15)` mm.
pub fn packed_distance(value: i32) -> f64 {
    let ffp = packed_ffp(value);
    f64::from(ffp.mantissa) * 2f64.powi(i32::from(ffp.exponent) - 15)
}

/// What the scripts asked the game to do, applied in order after each run
/// ([`apply`]). Each is one of F41's object statements or F37's object
/// targets.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// F772 (statement 19): a ship of F772's type `kind` and model `model`,
    /// where the player is until a script places it.
    Create { ship: u32, kind: usize, model: u32 },
    /// F669 (statements 41-43): a ship of type `kind` that attacks `target`,
    /// with `word` kept at `+0x11a`.
    Attacker {
        ship: u32,
        model: u32,
        target: i32,
        word: i32,
    },
    /// F672 (statement 39): a ship of model `model` named from `seed`,
    /// waiting in `starport`'s bay until `leaves` (the scripts' seconds, F21
    /// case 5).
    Departure {
        ship: u32,
        model: u32,
        seed: i32,
        starport: i32,
        leaves: i32,
    },
    /// F702 (statement 54): `object` put on an orbit of the packed
    /// `distance` round `parent` (0 the system's root), F1530 then F1507
    /// with the shared pair as the longitude and the phase.
    Place {
        object: i32,
        parent: i32,
        distance: i32,
    },
    /// Statement 57: the object's velocity (`+0x8c..+0x94`) is zeroed.
    Stop { object: i32 },
    /// Statement 60: AI state 0xb in `leader`'s formation at
    /// `npc::SCRIPT_FORMATIONS[slot]`.
    Formation {
        object: i32,
        leader: i32,
        slot: usize,
    },
    /// F953 (statement 21).
    Damage { object: i32, amount: u32 },
    /// F942 (statement 51): the object is destroyed.
    Explode { object: i32 },
    /// Statements 26 and 47: the object's name (`+0x124`) is message `code`
    /// with `word`.
    Name { object: i32, code: i32, word: i32 },
    /// F926 (statement 52): a canister of commodity `cargo` at `near`.
    Canister { tag: i32, cargo: u16, near: i32 },
    /// F291 (statement 58): the player jettisons one tonne of `commodity`.
    Jettison { tag: i32, commodity: usize },
    /// F37's target 0x3e: the object's AI state (`+0xff`).
    Ai { object: i32, state: u8 },
    /// F37's target 0x41: the object's target (`+0xfe`).
    Target { object: i32, target: i32 },
    /// F37's targets 0x24 and 0x3d (F441/F36): the object's model.
    Model { object: i32, model: u32 },
    /// F37's target 0x38: the object's `+0xa0` word.
    Seed { object: i32, seed: i32 },
    /// F297 (statement 48, a building of `model` on planet `body` at
    /// `DATA_007130/7131`'s place) or F40 (statement 50, an object of
    /// `model` on an orbit round `body`): a fixed object the player can
    /// shoot, with `seed` at `+0xa0`.
    Object {
        ship: u32,
        model: u32,
        body: i32,
        seed: i32,
        place: Place,
    },
    /// Not a script's: [`apply`]'s last step, holding F297/F40's objects to
    /// their bodies.
    Sites,
    /// F37's target 0x42 (F649): a station's price for a commodity.
    Price {
        station: i32,
        commodity: usize,
        price: i32,
    },
    /// F290 (statement 49): a delivery contract.
    Delivery {
        basic_data: u32,
        reward: u32,
        destination: u32,
        deadline_day: u32,
        seed: u32,
    },
}

/// Where a statement 48/50 object goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    /// F879's latitude and longitude words on the planet's surface.
    Surface { latitude: u16, longitude: u16 },
    /// F40's orbit: the packed `radius` (`+0xb0`), the phase `offset` added
    /// to F1510's high dword (`<< 16`), the `rate` (`+0xc2`), `+0xc6`'s
    /// longitude, the `+0xc8` spin and `+0xcc` tilt.
    Orbit {
        radius: i32,
        offset: i32,
        rate: u32,
        longitude: i16,
        spin: u8,
        tilt: i16,
    },
}

/// What a script reads of an object (F1532's record): its id, model
/// (`+0x82`), AI state (`+0xff`), target (`+0xfe`), `+0xa0` word and F1680's
/// distance class (`+0x88`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipView {
    pub id: i32,
    pub model: i32,
    pub ai: i32,
    pub target: i32,
    pub seed: i32,
    pub distance: i32,
}

/// A system body as the scripts' searches see it (F27): its id, model
/// (F29's planets are 0x76..=0x85), `+0xa0` code and `+0x14c` class (F28's
/// starports have 0x10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyView {
    pub id: i32,
    pub model: u32,
    pub seed: u32,
    pub class: u8,
}

/// The player's hold and equipment, for the cargo token (0x25).
pub struct Cargo<'a> {
    pub hold: &'a mut Vec<u16>,
    /// `DATA_008889`: the hold's capacity in tonnes.
    pub capacity: i32,
    pub loadout: &'a mut ffe_core::equipment::Loadout,
    pub combat: &'a mut crate::combat::Combat,
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
    /// Console lines to post (F89's `0xa600`, F236's and F306's notices).
    pub posts: &'a mut Vec<String>,
    /// The player's legal record (`DATA_009093`/`9095`/`8908`); the caller
    /// writes it back.
    pub legal: ffe_core::npc::Legal,
    /// The military points (`DATA_008912`/`8913`) and medals; written back too.
    pub career: ffe_core::npc::Career,
    /// `base:world/system-tables`, for F871's allegiance.
    pub tables: Option<&'a [u8]>,
    /// The id the next created ship gets, and the next canister's.
    pub next_ship: u32,
    pub next_tag: i32,
    /// What the scripts asked for, in order.
    pub actions: Vec<Action>,
    /// The objects the scripts can ask about.
    pub ships: Vec<ShipView>,
    pub bodies: Vec<BodyView>,
    /// F21's player values the port keeps elsewhere: 1 the Elite score, 3
    /// the reputation, 13 the place the player is at and 14 its mode (see
    /// [`player_values`]).
    pub values: [i32; 21],
    /// The system allegiance (`DATA_008996`), for F635's type shift.
    pub allegiance: u32,
    /// The player's hold and equipment (none when the caller has none to lend).
    pub cargo: Option<Cargo<'a>>,
    /// The contracts, for F314/F315's script records.
    pub contracts: Option<&'a mut ffe_core::missions::State>,
    /// F326's own entries for the bulletin board (screen 2): the station's
    /// adverts as [`native_entry`] records.
    pub native: Vec<ffe_core::script::menu::Entry>,
}

/// The mark [`native_entry`] leaves in an entry's second extra word.
const NATIVE: i32 = 0x4e41_5456;

/// The station's advert `index` (message `code`) as an entry of F326's menu,
/// so the scripts' F346 can insert theirs among the station's.
pub fn native_entry(index: usize, code: u16) -> ffe_core::script::menu::Entry {
    ffe_core::script::menu::Entry {
        code: i32::from(code),
        extra: [index as i32, NATIVE],
        ..Default::default()
    }
}

/// The station advert a menu entry stands for ([`native_entry`]).
pub fn native_index(entry: &ffe_core::script::menu::Entry) -> Option<usize> {
    (entry.extra[1] == NATIVE)
        .then(|| usize::try_from(entry.extra[0]).ok())
        .flatten()
}

impl Hooks<'_> {
    fn ship_view(&self, id: i32) -> Option<&ShipView> {
        self.ships.iter().find(|ship| ship.id == id)
    }

    fn ship_view_mut(&mut self, id: i32) -> Option<&mut ShipView> {
        self.ships.iter_mut().find(|ship| ship.id == id)
    }

    /// A new navigator id; later reads in the same run see the ship.
    fn new_ship(&mut self, model: u32, ai: i32, target: i32, seed: i32) -> u32 {
        let ship = self.next_ship;
        self.next_ship += 1;
        self.ships.push(ShipView {
            id: ship as i32 + SHIP_IDS,
            model: model as i32,
            ai,
            target,
            seed,
            distance: 0,
        });
        ship
    }

    /// F658/F635: a model for F772's type `kind`.
    fn type_model(&mut self, kind: usize) -> u32 {
        ffe_core::npc::model_for_type(kind, self.allegiance, self.random)
    }

    /// F37/F31's cargo token: commodity `kind - 1000` in the player's hold,
    /// else equipment record `kind - 100` on the player's ship (F392).
    fn cargo_count(&self, object: i32, kind: i32) -> i32 {
        let Some(cargo) = self.cargo.as_ref().filter(|_| object == PLAYER_ID) else {
            return 0;
        };
        if kind >= 1000 {
            return usize::try_from(kind - 1000)
                .ok()
                .and_then(|index| cargo.hold.get(index))
                .map_or(0, |tonnes| i32::from(*tonnes));
        }
        usize::try_from(kind - 100).map_or(0, |index| {
            cargo.loadout.count(index, &cargo.combat.lasers) as i32
        })
    }

    /// F37's cargo target: the hold is set to `value` tonnes within the free
    /// space (a negative value empties it); equipment is fitted (F390) or
    /// removed (F391) one at a time until the count is `value` or a handler
    /// refuses.
    fn set_cargo(&mut self, object: i32, kind: i32, value: i32) {
        let have = self.cargo_count(object, kind);
        let Some(cargo) = self.cargo.as_mut().filter(|_| object == PLAYER_ID) else {
            return;
        };
        let used: i32 = cargo.hold.iter().map(|tonnes| i32::from(*tonnes)).sum();
        if kind >= 1000 {
            let Some(index) = usize::try_from(kind - 1000).ok().filter(|i| *i < 0x26) else {
                return;
            };
            if cargo.hold.len() <= index {
                cargo.hold.resize(index + 1, 0);
            }
            let mut change = value - have;
            if change > 0 {
                change = change.min(cargo.capacity - used).max(0);
            } else if value < 0 {
                change = -have;
            }
            cargo.hold[index] = (have + change).clamp(0, i32::from(u16::MAX)) as u16;
            return;
        }
        let Ok(index) = usize::try_from(kind - 100) else {
            return;
        };
        let limits = crate::outfitting::Limits {
            free_space: cargo.capacity - used,
            ..crate::outfitting::Limits::default_for_ship()
        };
        // F390/F391 are the category handlers: no price changes hands.
        let mut credits = u32::MAX / 2;
        for _ in 0..(value - have).unsigned_abs().min(64) {
            let done = if value > have {
                crate::outfitting::fit(index, 0, cargo.loadout, cargo.combat, &mut credits, limits)
            } else {
                crate::outfitting::remove(
                    index,
                    0,
                    cargo.loadout,
                    cargo.combat,
                    &mut credits,
                    limits,
                )
            };
            if done.is_err() {
                break;
            }
        }
    }

    /// Statement 53: F305 (Imperial, `side` 1) or F304 (Federal, 2), each
    /// F307's medal bit `medal` and F306's `points`, with their notices.
    fn reward(&mut self, side: i32, points: i32, medal: i32) {
        if !(1..=2).contains(&side) {
            return;
        }
        if let Some(index) = usize::try_from(medal).ok().filter(|index| *index < 32) {
            if self.career.award_medal(index) {
                if let Some(name) = ffe_core::status::medal(index) {
                    self.posts.push(ffe_core::status::medal_message(name));
                }
            }
        }
        let change = if side == 1 {
            self.career.award_imperial(points)
        } else {
            self.career.award_federal(points)
        };
        if let Some(change) = change {
            let rank = if side == 1 {
                self.career.imperial_title()
            } else {
                self.career.federal_rank()
            };
            self.posts.push(if change.promoted() {
                ffe_core::status::promotion_message(rank)
            } else {
                ffe_core::status::demotion_message(rank)
            });
        }
    }
}

/// The player and the navigator's ships as the scripts see them.
pub fn ship_views(
    state: &crate::flight::Flight,
    nav: &crate::navigation::Navigator,
    player: [i64; 3],
) -> Vec<ShipView> {
    let mut views = vec![ShipView {
        id: PLAYER_ID,
        model: state.ship as i32,
        ai: 0,
        target: 0,
        seed: state.script_seed,
        distance: 0,
    }];
    views.extend(
        nav.ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| ShipView {
                id: ship.id as i32 + SHIP_IDS,
                model: ship.model as i32,
                ai: i32::from(ship.ai_state()),
                target: ship.script_target,
                seed: ship.seed,
                distance: ffe_core::dust::distance_class(ship.position, player, 0),
            }),
    );
    views
}

/// The system's bodies as the scripts' searches see them.
pub fn body_views(nav: &crate::navigation::Navigator) -> Vec<BodyView> {
    nav.bodies
        .iter()
        .enumerate()
        .map(|(index, body)| BodyView {
            id: BODY_IDS + index as i32,
            model: body.model,
            seed: body.seed,
            class: body.class,
        })
        .collect()
}

/// F21's cases the game raises event 1 for when they change: the system
/// (4), the docked object (13), the flight mode (14), the cabins (15) and
/// their passengers (16).
pub const WATCHED: [i32; 5] = [4, 13, 14, 15, 16];

impl Hooks<'_> {
    /// The [`WATCHED`] values now.
    pub fn watched(&self) -> [i32; 5] {
        WATCHED.map(|case| match case {
            4 => self.system as i32,
            _ => self.values[case as usize],
        })
    }
}

/// F21's player values the port keeps outside the hooks' other fields.
pub fn player_values(
    state: &crate::flight::Flight,
    nav: &crate::navigation::Navigator,
) -> [i32; 21] {
    let mut values = [0; 21];
    values[1] = nav.career.elite as i32;
    values[3] = i32::from(state.missions.reputation);
    // DATA_008870, the player's mode: 0x2a docked in a station, 0x30 on a
    // starport pad, the landed words, 0x2c dying and 4 in hyperspace.
    let place = nav
        .docked
        .and_then(|index| nav.bodies.get(index).map(|body| (index, body)));
    values[14] = if state.death.is_some() {
        0x2c
    } else if nav.transit.is_some() {
        4
    } else if let Some(word) = nav.landed_state() {
        i32::from(word)
    } else {
        match place {
            Some((_, body)) if body.kind == crate::navigation::Kind::Starport => 0x30,
            Some(_) => 0x2a,
            None => 0,
        }
    };
    // DATA_008890/8892: the cabins fitted and those with passengers.
    values[15] = i32::from(state.loadout.cabins);
    values[16] = i32::from(state.loadout.occupied_cabins);
    // DATA_008862: the object the player is docked or landed at.
    values[13] = place
        .map(|(index, _)| index)
        .or_else(|| nav.landed_body())
        .map_or(0, |index| BODY_IDS + index as i32);
    values
}

impl Host for Hooks<'_> {
    fn native_entries(&mut self, screen: i32) -> Vec<ffe_core::script::menu::Entry> {
        if screen == 2 {
            self.native.clone()
        } else {
            Vec::new()
        }
    }

    fn player(&mut self, case: i32) -> i32 {
        match case {
            // F21 0: the player's object id (`DATA_008861 + 0x86`); the
            // port's player is object 1.
            0 => PLAYER_ID,
            2 => *self.credits as i32,
            4 => self.system as i32,
            5..=11 => {
                // F21: tics into the day / 0xc22e (seconds) plus the day
                // counter from 0x121cf7 in seconds.
                let day = absolute_day(self.clock) as i64;
                let seconds = (self.clock - day as f64 * 86_400.0).max(0.0) as i64;
                ((day - 0x121cf7) * 86_400 + seconds) as i32
            }
            _ => usize::try_from(case)
                .ok()
                .and_then(|case| self.values.get(case))
                .copied()
                .unwrap_or(0),
        }
    }

    fn set_player(&mut self, case: i32, value: i32) {
        // F22 writes the same words F21 reads; the port's cash is the one
        // the game keeps, the elite score and reputation go back through
        // `values` (the caller stores 1 and 3).
        match case {
            2 => *self.credits = value.max(0) as u32,
            1 | 3 => self.values[case as usize] = value,
            _ => {}
        }
    }

    fn random(&mut self) -> u32 {
        self.random.advance()
    }

    fn query(&mut self, token: u8, args: &[i32]) -> i32 {
        let arg = |n: usize| args.get(n).copied().unwrap_or(0);
        let slot = |n: i32| usize::try_from(n).ok().filter(|slot| *slot < 6);
        match token {
            // DATA_009093[x]: the offences each jurisdiction holds.
            0x1f => slot(arg(0)).map_or(0, |s| self.legal.records[s] as i32),
            // 1: DATA_008913 (Imperial), 2: DATA_008912 (Federal) points.
            0x22 => match arg(0) {
                1 => self.career.imperial as i32,
                2 => self.career.federal as i32,
                _ => 0,
            },
            // DATA_009095[x]: the fines.
            0x23 => slot(arg(0)).map_or(0, |s| self.legal.fines[s] as i32),
            // F1532's object: `+0x82` the model (0x24 and 0x3d), `+0xff` the
            // AI state, `+0xa0` its word, `+0x88` the distance class and
            // `+0xfe` the target.
            0x24 | 0x3d | 0x3e | 0x38 | 0x40 | 0x41 => {
                let id = arg(0);
                if let Some(ship) = self.ship_view(id) {
                    return match token {
                        0x24 | 0x3d => ship.model,
                        0x3e => ship.ai,
                        0x38 => ship.seed,
                        0x40 => ship.distance,
                        _ => ship.target,
                    };
                }
                let Some(body) = self.bodies.iter().find(|body| body.id == id) else {
                    return 0;
                };
                match token {
                    0x24 | 0x3d => body.model as i32,
                    0x38 => body.seed as i32,
                    _ => 0,
                }
            }
            // F392 / DATA_008896: the cargo token.
            0x25 => self.cargo_count(arg(0), arg(1)),
            // DATA_008908[x]: the offence points.
            0x30 => slot(arg(0)).map_or(0, |s| self.legal.points[s] as i32),
            // F871: the system's allegiance (`+10 >> 6`).
            0x31 => self
                .tables
                .and_then(|bytes| ffe_core::galaxy::Tables::new(bytes).ok())
                .map_or(0, |tables| {
                    tables
                        .control(arg(0) as u32, [self.career.federal, self.career.imperial])
                        .allegiance as i32
                }),
            // F27 with F28 (a starport, `+0x14c & 0x10`) or F29 (a planet,
            // models 0x76..=0x85) whose `+0xa0` is the code; F27 walks the
            // slots from the top.
            0x33 | 0x3c => self
                .bodies
                .iter()
                .rev()
                .find(|body| {
                    let kind = if token == 0x33 {
                        body.class & 0x10 != 0
                    } else {
                        (0x76..=0x85).contains(&body.model)
                    };
                    kind && body.seed == arg(0) as u32
                })
                .map_or(0, |body| body.id),
            // F658: a model for F772's type.
            0x35 => usize::try_from(arg(0)).map_or(0, |kind| self.type_model(kind) as i32),
            _ => 0,
        }
    }

    fn statement(&mut self, opcode: u8, args: &[i32], _text: &[u8]) -> i32 {
        let arg = |n: usize| args.get(n).copied().unwrap_or(0);
        let kind = |n: i32| usize::try_from(n).ok().filter(|kind| *kind < 0x13);
        match opcode {
            // F772: a ship of type `arg(0)`, unplaced; its id.
            19 => {
                let Some(kind) = kind(arg(0)) else {
                    return 0;
                };
                let model = self.type_model(kind);
                let ship = self.new_ship(model, 0, 0, 0);
                self.actions.push(Action::Create { ship, kind, model });
                ship as i32 + SHIP_IDS
            }
            // F953(object, amount, kind): a negative amount is the most.
            21 => {
                let amount = if arg(1) < 0 { i32::MAX } else { arg(1) } as u32;
                self.actions.push(Action::Damage {
                    object: arg(0),
                    amount,
                });
                0
            }
            26 | 47 => {
                self.actions.push(Action::Name {
                    object: arg(0),
                    code: arg(1),
                    word: if opcode == 47 { arg(2) } else { 0 },
                });
                0
            }
            // F314(7, ..., DATA_008885): a script contract; F315 removes it.
            37 => {
                let system = self.system;
                self.contracts
                    .as_mut()
                    .map_or(-1, |missions| missions.add_script_contract(system))
            }
            38 => {
                if let Some(missions) = self.contracts.as_mut() {
                    missions.remove_script_contract(arg(0));
                }
                0
            }
            // F664 -> F672(type, seed, starport, F24/F23 of the time): AI
            // state 0x24, waiting in the bay.
            39 => {
                let Some(kind) = kind(arg(0)) else {
                    return 0;
                };
                let model = self.type_model(kind);
                let ship = self.new_ship(model, 0x24, arg(2), arg(1));
                self.actions.push(Action::Departure {
                    ship,
                    model,
                    seed: arg(1),
                    starport: arg(2),
                    leaves: arg(3),
                });
                ship as i32 + SHIP_IDS
            }
            // F236(jurisdiction, offence, the player's ship).
            40 => {
                let jurisdiction = usize::try_from(arg(0)).ok().filter(|slot| *slot < 6);
                if let (Some(jurisdiction), Ok(offence)) = (jurisdiction, usize::try_from(arg(1))) {
                    if let Some(report) = self.legal.record(offence, Some(jurisdiction)) {
                        if !report.message.is_empty() {
                            self.posts.push(report.message);
                        }
                    }
                }
                0
            }
            // F665/F666/F668 -> F669: a ship of type 0xc/4/2 that attacks
            // `arg(0)` in AI state 9 (F41 passes the target first, then the
            // word).
            41..=43 => {
                let kind = match opcode {
                    41 => 0xc,
                    42 => 4,
                    _ => 2,
                };
                let model = self.type_model(kind);
                let ship = self.new_ship(model, 9, arg(0), 0);
                self.actions.push(Action::Attacker {
                    ship,
                    model,
                    target: arg(0),
                    word: arg(1),
                });
                ship as i32 + SHIP_IDS
            }
            // F297(name flag, model, planet, place, seed): a building on
            // the planet at DATA_007130/7131's place.
            48 => {
                let Some(slot) = usize::try_from(arg(3)).ok().filter(|slot| *slot < 16) else {
                    return 0;
                };
                let (latitude, longitude) = self
                    .tables
                    .and_then(|bytes| ffe_core::galaxy::Tables::new(bytes).ok())
                    .map_or((0, 0), |tables| {
                        (
                            tables.dword(ffe_core::galaxy::tables::STARPORT_LATITUDE + 8 * slot)
                                as u16,
                            tables.dword(ffe_core::galaxy::tables::STARPORT_LONGITUDE + 8 * slot)
                                as u16,
                        )
                    });
                let model = arg(1) as u32;
                let ship = self.new_ship(model, 0x0e, 0, arg(4));
                self.actions.push(Action::Object {
                    ship,
                    model,
                    body: arg(2),
                    seed: arg(4),
                    place: Place::Surface {
                        latitude,
                        longitude,
                    },
                });
                ship as i32 + SHIP_IDS
            }
            // F290(basic data, reward, destination, F23(deadline), seed).
            49 => {
                self.actions.push(Action::Delivery {
                    basic_data: arg(0) as u32,
                    reward: arg(1) as u32,
                    destination: arg(2) as u32,
                    deadline_day: (arg(3) as u32) / 0x1_5180 + 0x12_1cf7,
                    seed: arg(4) as u32,
                });
                0
            }
            // F40(model, parent, seed, radius, phase, rate, longitude,
            // spin, tilt): an object on an orbit round the parent.
            50 => {
                let model = arg(0) as u32;
                let ship = self.new_ship(model, 0x0e, 0, arg(2));
                self.actions.push(Action::Object {
                    ship,
                    model,
                    body: arg(1),
                    seed: arg(2),
                    place: Place::Orbit {
                        radius: arg(3),
                        offset: arg(4),
                        rate: arg(5) as u32,
                        longitude: arg(6) as i16,
                        spin: arg(7) as u8,
                        tilt: arg(8) as i16,
                    },
                });
                ship as i32 + SHIP_IDS
            }
            51 => {
                self.actions.push(Action::Explode { object: arg(0) });
                0
            }
            // F926(0x8e00 + commodity, object): the canister's id.
            52 => {
                let tag = self.next_tag;
                self.next_tag += 1;
                self.actions.push(Action::Canister {
                    tag,
                    cargo: arg(0) as u16,
                    near: arg(1),
                });
                tag
            }
            53 => {
                self.reward(arg(0), i32::from(arg(1) as i8), arg(2));
                0
            }
            54 => {
                self.actions.push(Action::Place {
                    object: arg(0),
                    parent: arg(1),
                    distance: arg(2),
                });
                0
            }
            57 => {
                self.actions.push(Action::Stop { object: arg(0) });
                0
            }
            // F291(object, commodity): only the player's hold, and only a
            // commodity it carries; the canister's id.
            58 => {
                let Some(commodity) = usize::try_from(arg(1) - 0x3e8).ok().filter(|c| *c < 0x26)
                else {
                    return 0;
                };
                let have = self.cargo_count(PLAYER_ID, arg(1));
                if arg(0) != PLAYER_ID || have == 0 {
                    return 0;
                }
                self.set_cargo(PLAYER_ID, arg(1), have - 1);
                let tag = self.next_tag;
                self.next_tag += 1;
                self.actions.push(Action::Jettison { tag, commodity });
                tag
            }
            // AI state 0xb, the old target to `+0x100`, the leader to
            // `+0xfe` and DATA_001705's place to `+0x102..+0x106`.
            60 => {
                let Some(slot) = usize::try_from(arg(2))
                    .ok()
                    .filter(|slot| *slot < ffe_core::npc::SCRIPT_FORMATIONS.len())
                else {
                    return 0;
                };
                if let Some(view) = self.ship_view_mut(arg(0)) {
                    view.ai = 0xb;
                    view.target = arg(1);
                }
                self.actions.push(Action::Formation {
                    object: arg(0),
                    leader: arg(1),
                    slot,
                });
                0
            }
            _ => 0,
        }
    }

    fn assign(&mut self, token: u8, args: &[i32], value: i32) -> Option<(i32, i32, i32)> {
        let index = args.first().copied().unwrap_or(-1);
        let slot = usize::try_from(index).ok().filter(|slot| *slot < 6);
        match token {
            0x1f => {
                self.legal.records[slot?] = value as u32;
                Some((0x15, 0, index))
            }
            0x23 => {
                self.legal.fines[slot?] = value.max(0) as u32;
                Some((0x0a, 0, index))
            }
            0x30 => {
                self.legal.points[slot?] = value.max(0) as u32;
                Some((0x11, 0, index))
            }
            0x25 => {
                self.set_cargo(index, args.get(1).copied().unwrap_or(0), value);
                None
            }
            0x24 | 0x3d => {
                if let Some(view) = self.ship_view_mut(index) {
                    view.model = value;
                }
                self.actions.push(Action::Model {
                    object: index,
                    model: value as u32,
                });
                None
            }
            0x38 => {
                if let Some(view) = self.ship_view_mut(index) {
                    view.seed = value;
                }
                self.actions.push(Action::Seed {
                    object: index,
                    seed: value,
                });
                None
            }
            // F37 only sets the state of an object with AI (`+0x38`), then
            // raises event 0x17.
            0x3e => {
                let view = self
                    .ships
                    .iter_mut()
                    .find(|ship| ship.id == index && ship.id != PLAYER_ID)?;
                view.ai = value & 0xff;
                self.actions.push(Action::Ai {
                    object: index,
                    state: value as u8,
                });
                Some((0x17, 0, index))
            }
            // F649's market of station `index`: commodity `kind - 1000`'s
            // price word, then event 0x18.
            0x42 => {
                let kind = args.get(1).copied().unwrap_or(0);
                let commodity = usize::try_from(kind - 1000).ok().filter(|c| *c < 0x26)?;
                self.actions.push(Action::Price {
                    station: index,
                    commodity,
                    price: value,
                });
                Some((0x18, index, kind))
            }
            0x41 => {
                if let Some(view) = self.ship_view_mut(index) {
                    view.target = value & 0xff;
                }
                self.actions.push(Action::Target {
                    object: index,
                    target: value,
                });
                None
            }
            _ => None,
        }
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

/// The script globals the game's vetoable actions use.
pub mod veto {
    /// F381: a launch.
    pub const LAUNCH: usize = 4;
    /// F931/F291: a shot (`0x7cf + laser`) or a jettison (`0x3e8 + cargo`).
    pub const SHOT_OR_JETTISON: usize = 5;
    /// F908: a hyperspace jump.
    pub const JUMP: usize = 0xf;
    /// F650: a docking request (the ship's id).
    pub const DOCKING: usize = 0x17;
}

/// The script globals the game signals or reads besides the vetoes.
pub mod signal {
    /// F316: the contract row being drawn (its record id); the scripts
    /// answer with the row's code in [`ROW_CODE`] and its words in 1-3.
    pub const CONTRACT_ROW: usize = 8;
    pub const ROW_CODE: usize = 9;
    /// F383: the Upgrades screen's price multiplier (in tenths), signalled
    /// on [`UPGRADES`] before the list is built.
    pub const PRICE_MULTIPLIER: usize = 0xa;
    pub const UPGRADES: usize = 0xb;
    /// F622/F633: the station's daily signal (the station's id).
    pub const STATION_DAY: usize = 0x10;
}

/// The navigator id the next script ship gets: above every ship and every
/// queued creation.
pub fn next_ship_id(nav: &crate::navigation::Navigator, pending: &[Action]) -> u32 {
    let ships = nav.ships.iter().map(|ship| ship.id + 1).max().unwrap_or(0);
    let queued = pending
        .iter()
        .filter_map(|action| match action {
            Action::Create { ship, .. }
            | Action::Attacker { ship, .. }
            | Action::Departure { ship, .. }
            | Action::Object { ship, .. } => Some(ship + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    ships.max(queued).max(1)
}

/// The id the next script canister gets.
pub fn next_tag(nav: &crate::navigation::Navigator, pending: &[Action]) -> i32 {
    let live = nav.effects.iter().filter_map(|effect| effect.tag);
    let queued = pending.iter().filter_map(|action| match action {
        Action::Canister { tag, .. } | Action::Jettison { tag, .. } => Some(*tag),
        _ => None,
    });
    live.chain(queued).max().map_or(TAG_IDS, |tag| tag + 1)
}

/// Ask the scripts whether a game action may go ahead (`true` when there
/// are no scripts).
pub fn permits(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    assets: Option<&Assets>,
    global: usize,
    value: i32,
) -> bool {
    let Some(assets) = assets else {
        return true;
    };
    match with_hooks(state, nav, cache, |scripts, hooks, today| {
        scripts.permits(&assets.image, hooks, today, global, value)
    }) {
        Some((Ok(allowed), _)) => allowed,
        Some((Err(error), _)) => {
            eprintln!("mission scripts: {error}");
            true
        }
        None => true,
    }
}

/// F953's script signal for damage `amount` of `kind` to the object the
/// scripts know as `object`: the damage to deal (`amount` with no scripts).
pub fn damage(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    assets: Option<&Assets>,
    object: i32,
    kind: i32,
    amount: u32,
) -> u32 {
    let Some(assets) = assets else {
        return amount;
    };
    let value = i32::try_from(amount).unwrap_or(i32::MAX);
    match with_hooks(state, nav, cache, |scripts, hooks, today| {
        scripts.damage(&assets.image, hooks, today, object, kind, value)
    }) {
        Some((Ok(dealt), _)) => dealt.max(0) as u32,
        Some((Err(error), _)) => {
            eprintln!("mission scripts: {error}");
            amount
        }
        None => amount,
    }
}

/// F383's price signal: global 0xa set to the tech multiplier, 0xb pulsed,
/// and the multiplier the scripts leave in 0xa (the tech multiplier with no
/// scripts).
pub fn upgrade_multiplier(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    assets: Option<&Assets>,
    tech_level: u16,
) -> u32 {
    let base = ffe_core::equipment::tech_multiplier(tech_level);
    let Some(assets) = assets else {
        return base;
    };
    let result = with_hooks(state, nav, cache, |scripts, hooks, today| {
        let image = &assets.image;
        scripts.set_global(image, hooks, today, signal::PRICE_MULTIPLIER, base as i32)?;
        scripts.pulse(image, hooks, today, signal::UPGRADES, 1)?;
        Ok::<_, String>(scripts.global(signal::PRICE_MULTIPLIER))
    });
    match result {
        Some((Ok(value), _)) => value.max(0) as u32,
        Some((Err(error), _)) => {
            eprintln!("mission scripts: {error}");
            base
        }
        None => base,
    }
}

/// F316's row for a script contract: global 8 set to the record's id (and
/// back to 0), then the row is the message code in global 9 with globals 1-3
/// as its words.
pub fn contract_row(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    assets: &Assets,
    id: i32,
) -> Option<ffe_core::script::menu::Entry> {
    let result = with_hooks(state, nav, cache, |scripts, hooks, today| {
        scripts.pulse(&assets.image, hooks, today, signal::CONTRACT_ROW, id)?;
        let mut entry = ffe_core::script::menu::Entry {
            code: scripts.global(signal::ROW_CODE),
            ..Default::default()
        };
        for (word, index) in entry.words.iter_mut().zip(1..) {
            *word = scripts.global(index);
        }
        Ok::<_, String>(entry)
    });
    match result {
        Some((Ok(entry), _)) => Some(entry),
        Some((Err(error), _)) => {
            eprintln!("mission scripts: {error}");
            None
        }
        None => None,
    }
}

/// The original's day number (`DATA_008807`) for world seconds `clock`.
pub fn absolute_day(clock: f64) -> u32 {
    (clock.max(0.0) / 86_400.0) as u32
}

/// What the hooks read of the game, taken before a run.
pub struct Snapshot {
    pub system: u32,
    pub clock: f64,
    pub legal: ffe_core::npc::Legal,
    pub career: ffe_core::npc::Career,
    pub next_ship: u32,
    pub next_tag: i32,
    pub ships: Vec<ShipView>,
    pub bodies: Vec<BodyView>,
    pub values: [i32; 21],
    pub allegiance: u32,
    pub capacity: i32,
}

impl Snapshot {
    pub fn take(
        state: &crate::flight::Flight,
        nav: &crate::navigation::Navigator,
        cache: &ffe_content::VerifiedCache,
    ) -> Self {
        let system = nav.current_code();
        let allegiance = cache
            .asset("base:world/system-tables")
            .and_then(|bytes| ffe_core::galaxy::Tables::new(bytes).ok())
            .map_or(0, |tables| tables.details(system).allegiance >> 6);
        Self {
            system,
            clock: nav.clock(),
            legal: nav.legal,
            career: nav.career,
            next_ship: next_ship_id(nav, &state.script_actions),
            next_tag: next_tag(nav, &state.script_actions),
            ships: ship_views(state, nav, nav.player_at),
            bodies: body_views(nav),
            values: player_values(state, nav),
            allegiance,
            capacity: crate::trade::capacity(nav, cache, state),
        }
    }
}

/// What a run leaves for the game.
pub struct Leftover {
    legal: ffe_core::npc::Legal,
    career: ffe_core::npc::Career,
    values: [i32; 21],
    actions: Vec<Action>,
}

impl<'a> Hooks<'a> {
    pub fn new(
        snapshot: Snapshot,
        credits: &'a mut u32,
        random: &'a mut Random,
        posts: &'a mut Vec<String>,
        tables: Option<&'a [u8]>,
        cargo: Option<Cargo<'a>>,
        contracts: Option<&'a mut ffe_core::missions::State>,
    ) -> Self {
        let capacity = snapshot.capacity;
        Self {
            credits,
            random,
            system: snapshot.system,
            clock: snapshot.clock,
            posts,
            legal: snapshot.legal,
            career: snapshot.career,
            tables,
            next_ship: snapshot.next_ship,
            next_tag: snapshot.next_tag,
            actions: Vec::new(),
            ships: snapshot.ships,
            bodies: snapshot.bodies,
            values: snapshot.values,
            allegiance: snapshot.allegiance,
            cargo: cargo.map(|cargo| Cargo { capacity, ..cargo }),
            contracts,
            native: Vec::new(),
        }
    }

    /// What the run asked the game to do (for a caller that keeps the rest
    /// of the state itself).
    pub fn finish_actions(self) -> Vec<Action> {
        self.actions
    }

    pub fn finish(self) -> Leftover {
        Leftover {
            legal: self.legal,
            career: self.career,
            values: self.values,
            actions: self.actions,
        }
    }
}

impl Leftover {
    /// Write the run's changes back: the legal record, the career, the
    /// Elite score and reputation (F22's cases 1 and 3) and the queued
    /// actions.
    pub fn store(self, state: &mut crate::flight::Flight, nav: &mut crate::navigation::Navigator) {
        nav.legal = self.legal;
        nav.career = self.career;
        nav.career.elite = self.values[1].max(0) as u32;
        state.missions.reputation = self.values[3].clamp(0, 255) as u8;
        state.script_actions.extend(self.actions);
    }
}

/// Run `f` on the flight's scripts (`None` before a new game has started
/// them) with hooks over the flight and the navigator, collecting their
/// console posts and queueing what they asked for.
pub fn with_all<R>(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    f: impl FnOnce(&mut Option<Scripts>, &mut Hooks<'_>, u32) -> R,
) -> (R, Vec<String>) {
    let snapshot = Snapshot::take(state, nav, cache);
    let today = absolute_day(snapshot.clock);
    let mut posts = Vec::new();
    let (result, left) = {
        let crate::flight::Flight {
            scripts,
            credits,
            hold,
            loadout,
            combat,
            missions,
            ..
        } = state;
        let cargo = Cargo {
            hold,
            capacity: 0,
            loadout,
            combat,
        };
        let mut hooks = Hooks::new(
            snapshot,
            credits,
            &mut nav.random,
            &mut posts,
            cache.asset("base:world/system-tables"),
            Some(cargo),
            Some(missions),
        );
        let result = f(scripts, &mut hooks, today);
        (result, hooks.finish())
    };
    left.store(state, nav);
    (result, posts)
}

/// [`with_all`] for flights that have scripts: `None` when there are none.
pub fn with_hooks<R>(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    f: impl FnOnce(&mut Scripts, &mut Hooks<'_>, u32) -> R,
) -> Option<(R, Vec<String>)> {
    state.scripts.as_ref()?;
    let (result, posts) = with_all(state, nav, cache, |scripts, hooks, today| {
        scripts.as_mut().map(|scripts| f(scripts, hooks, today))
    });
    result.map(|result| (result, posts))
}

/// The script ids of the objects present: the navigator's live ships and
/// the scripts' canisters (F924's removals are the ones that go).
pub fn present(nav: &crate::navigation::Navigator) -> std::collections::BTreeSet<i32> {
    nav.ships
        .iter()
        .filter(|ship| ship.alive())
        .map(|ship| ship.id as i32 + SHIP_IDS)
        .chain(
            nav.effects
                .iter()
                .filter(|effect| effect.alive)
                .filter_map(|effect| effect.tag),
        )
        .collect()
}

/// F770, AI state 9's control: a ship closing on its target that comes
/// within F1680's distance class 0xa of it takes up the attack (state 1)
/// and raises event 0x17. Returns each such ship's id and whether its target
/// is the player, whom it hails with `0x98d0` and its `+0xa0` word.
pub fn approaches(
    nav: &mut crate::navigation::Navigator,
    player: [i64; 3],
) -> Vec<(i32, i32, bool)> {
    let mut out = Vec::new();
    let positions: Vec<(i32, [i64; 3])> = nav
        .ships
        .iter()
        .filter(|ship| ship.alive())
        .map(|ship| (ship.id as i32 + SHIP_IDS, ship.position))
        .collect();
    for ship in nav.ships.iter_mut().filter(|ship| ship.alive()) {
        if ship.ai_state() != 9 {
            continue;
        }
        let at = if ship.script_target == PLAYER_ID {
            Some(player)
        } else {
            positions
                .iter()
                .find(|(id, _)| *id == ship.script_target)
                .map(|(_, at)| *at)
        };
        let Some(at) = at else {
            continue;
        };
        if ffe_core::dust::distance_class(ship.position, at, 0) >= 0xa {
            continue;
        }
        ship.set_ai(1, target_of(ship.script_target));
        out.push((
            ship.id as i32 + SHIP_IDS,
            ship.seed,
            ship.script_target == PLAYER_ID,
        ));
    }
    out
}

/// Where an object the scripts name is: the player, a ship, a body or a
/// canister.
fn object_position(
    nav: &crate::navigation::Navigator,
    player: [i64; 3],
    object: i32,
) -> Option<[i64; 3]> {
    if object == PLAYER_ID {
        return Some(player);
    }
    if object >= TAG_IDS {
        return nav
            .effects
            .iter()
            .find(|effect| effect.tag == Some(object))
            .map(|effect| effect.position);
    }
    if object >= BODY_IDS {
        return usize::try_from(object - BODY_IDS)
            .ok()
            .and_then(|index| nav.bodies.get(index))
            .map(|body| body.position);
    }
    ship_of(nav, object).map(|ship| ship.position)
}

fn ship_of(nav: &crate::navigation::Navigator, object: i32) -> Option<&ffe_core::npc::NpcShip> {
    let id = u32::try_from(object - SHIP_IDS).ok()?;
    nav.ships.iter().find(|ship| ship.id == id && ship.alive())
}

fn ship_of_mut(
    nav: &mut crate::navigation::Navigator,
    object: i32,
) -> Option<&mut ffe_core::npc::NpcShip> {
    let id = u32::try_from(object - SHIP_IDS).ok()?;
    nav.ships
        .iter_mut()
        .find(|ship| ship.id == id && ship.alive())
}

/// A script target as a ship's target: `Some(None)` the player,
/// `Some(Some(id))` a navigator ship, `None` none.
fn target_of(object: i32) -> Option<Option<u32>> {
    if object == PLAYER_ID {
        Some(None)
    } else if (SHIP_IDS..BODY_IDS).contains(&object) {
        Some(Some((object - SHIP_IDS) as u32))
    } else {
        None
    }
}

/// F1506's random direction from the shared pair.
fn random_direction(random: &mut Random) -> [f64; 3] {
    let yaw = f64::from(random.bounded(0x1_0000)) / 65_536.0 * std::f64::consts::TAU;
    let pitch = (f64::from(random.bounded(0x1_0000)) / 65_536.0 - 0.5) * std::f64::consts::PI;
    [
        pitch.cos() * yaw.cos(),
        pitch.sin(),
        pitch.cos() * yaw.sin(),
    ]
}

/// F669's orbit for its attackers, `0x5000` at exponent `0x31` (about
/// 2.3 AU) round the system's root.
pub const ATTACKER_ORBIT: i32 = 0x0031_5000;

/// F702: F1530 advances the shared pair and F1507/F1508 put `object` on the
/// orbit of the packed `distance` round `origin`, with the pair's words as
/// the longitude and the phase. A ship takes F1507's `+0x8c` words as its
/// velocity and F702 zeroes its yaw rate and side thrust (`+0xb4..+0xb8`).
fn place_on_orbit(
    nav: &mut crate::navigation::Navigator,
    sine: &ffe_core::model_math::SineTable,
    object: i32,
    origin: [i64; 3],
    distance: i32,
) {
    nav.random.advance();
    let (point, words) = ffe_core::objects::orbit_state(
        sine,
        packed_ffp(distance),
        nav.random.a as i32,
        nav.random.b,
    );
    let at: [i64; 3] = std::array::from_fn(|k| origin[k].wrapping_add(point[k]));
    let velocity = words.map(|v| f64::from(v) * ffe_core::npc::MM_PER_VELOCITY_UNIT);
    if let Some(ship) = ship_of_mut(nav, object) {
        ship.position = at;
        ship.anchor = at;
        ship.velocity = velocity;
        ship.speed = velocity.iter().map(|v| v * v).sum::<f64>().sqrt();
        ship.motion.velocity = words;
        ship.motion.rate[2] = 0;
        ship.motion.thrust[0] = 0;
        ship.motion.thrust[1] = 0;
    } else if let Some(effect) = nav.effects.iter_mut().find(|e| e.tag == Some(object)) {
        effect.position = at;
        effect.velocity = velocity;
    }
}

/// The rows of an object as its forward and up axes.
fn set_rows(ship: &mut ffe_core::npc::NpcShip, rows: &[i32; 9]) {
    let axis = |row: usize| {
        let v: [f64; 3] = std::array::from_fn(|k| f64::from(rows[3 * row + k]));
        let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
        if length > 0.0 {
            v.map(|c| c / length)
        } else {
            v
        }
    };
    let (up, forward) = (axis(1), axis(2));
    if forward.iter().any(|c| *c != 0.0) && up.iter().any(|c| *c != 0.0) {
        ship.forward = forward;
        ship.up = up;
    }
}

/// The ship spec for `model`, else another of F772's type list `kind`.
fn spec_for(
    cache: &ffe_content::VerifiedCache,
    model: u32,
    kind: Option<usize>,
) -> Option<ffe_core::npc::Spec> {
    crate::ships::npc_spec(cache, model)
        .map(|spec| ffe_core::npc::Spec { model, ..spec })
        .or_else(|| {
            let table = ffe_core::npc::SHIP_TYPES.get(kind?)?;
            table.iter().find_map(|model| {
                crate::ships::npc_spec(cache, *model).map(|spec| ffe_core::npc::Spec {
                    model: *model,
                    ..spec
                })
            })
        })
}

/// Carry out the actions the scripts queued, in order: create, place and
/// steer their ships, damage and name objects, make their canisters and
/// contracts. `player` is the player's position in the system frame.
/// Returns the ids of the ships F669 made, for its event 0x17.
pub fn apply(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    player: [i64; 3],
) -> Vec<i32> {
    use ffe_core::npc::{self, State};
    let actions = std::mem::take(&mut state.script_actions);
    let mut made_attackers = Vec::new();
    let sine = if actions.is_empty() && nav.script_sites.iter().all(|site| site.orbit.is_none()) {
        None
    } else {
        cache
            .asset("base:math/sine")
            .and_then(|bytes| ffe_core::model_math::SineTable::from_bytes(bytes).ok())
    };
    for action in actions.into_iter().chain(std::iter::once(Action::Sites)) {
        match action {
            Action::Create { ship, kind, model } => {
                if let Some(spec) = spec_for(cache, model, Some(kind)) {
                    let mut made = npc::script_ship(&spec, ship, player, &mut nav.random);
                    // F773's F703: F1530, then `+0xa0` is the pair's first word.
                    nav.random.advance();
                    made.seed = nav.random.a as i32;
                    nav.ships.push(made);
                }
            }
            Action::Attacker {
                ship,
                model,
                target,
                word,
            } => {
                let Some(spec) = spec_for(cache, model, None) else {
                    continue;
                };
                let mut made = npc::script_attacker(&spec, ship, [0; 3], &mut nav.random);
                // F772's F703: F1530, then `+0xa0` is the pair's first word.
                nav.random.advance();
                made.seed = nav.random.a as i32;
                made.script_target = target;
                made.bounty = word.max(0) as u32;
                made.set_ai(9, target_of(target));
                nav.ships.push(made);
                let id = ship as i32 + SHIP_IDS;
                // F669: F702 puts it on its orbit round the root, then F1530
                // gives the `+0x102..+0x106` approach words (read only by
                // F597's accelerated branch, which is not ported).
                if let Some(sine) = sine.as_ref() {
                    place_on_orbit(nav, sine, id, [0; 3], ATTACKER_ORBIT);
                }
                nav.random.advance();
                made_attackers.push(id);
            }
            Action::Departure {
                ship,
                model,
                seed,
                starport,
                leaves,
            } => {
                let Some(mut spec) = spec_for(cache, model, None) else {
                    continue;
                };
                // F672's name: F791 kind 0x12 from the seed.
                if let Some(name) = cache
                    .asset("base:text/names")
                    .and_then(|bytes| ffe_core::names::Names::new(bytes).ok())
                    .and_then(|names| names.generate(0x12, seed as u32))
                {
                    spec.name = String::from_utf8_lossy(&name).into_owned();
                }
                let index = usize::try_from(starport - BODY_IDS)
                    .ok()
                    .filter(|index| nav.bodies.get(*index).is_some_and(|b| b.kind.is_place()));
                let leaves = f64::from(leaves) + f64::from(0x12_1cf7u32) * 86_400.0;
                let wait = (leaves - nav.clock()).max(0.0);
                let mut made = match index {
                    Some(index) => npc::pad_trader(
                        &spec,
                        ship,
                        nav.bodies[index].position,
                        index,
                        wait,
                        &mut nav.random,
                    ),
                    // No bay: F672 puts it far out in state 8.
                    None => {
                        let direction = random_direction(&mut nav.random);
                        let at = std::array::from_fn(|k| player[k] + (direction[k] * 1.0e9) as i64);
                        let mut made = npc::script_ship(&spec, ship, at, &mut nav.random);
                        made.state = State::Transit;
                        made
                    }
                };
                made.name = spec.name.clone();
                made.seed = seed;
                made.script_target = starport;
                nav.ships.push(made);
            }
            Action::Place {
                object,
                parent,
                distance,
            } => {
                // F702: `+0x56` is the parent's frame (0 the root's).
                let origin = if parent == 0 {
                    [0; 3]
                } else {
                    object_position(nav, player, parent).unwrap_or(player)
                };
                let Some(sine) = sine.as_ref() else {
                    continue;
                };
                place_on_orbit(nav, sine, object, origin, distance);
            }
            Action::Stop { object } => {
                if let Some(ship) = ship_of_mut(nav, object) {
                    ship.velocity = [0.0; 3];
                    ship.speed = 0.0;
                } else if let Some(effect) = nav.effects.iter_mut().find(|e| e.tag == Some(object))
                {
                    effect.velocity = [0.0; 3];
                }
            }
            Action::Formation {
                object,
                leader,
                slot,
            } => {
                if let Some(ship) = ship_of_mut(nav, object) {
                    ship.script_target = leader;
                    ship.set_ai(0xb, target_of(leader));
                    let place = npc::SCRIPT_FORMATIONS[slot];
                    ship.formation = place.map(|word| i64::from(word) * npc::FORMATION_SCALE);
                }
            }
            Action::Damage { object, amount } => {
                if object == PLAYER_ID {
                    nav.script_events.push(crate::navigation::ShipEvent::Hit {
                        ship: 0,
                        name: String::new(),
                        code: 0,
                        damage: amount,
                        beam: false,
                    });
                } else if let Ok(id) = u32::try_from(object - SHIP_IDS) {
                    nav.script_damage(id, amount);
                }
            }
            Action::Explode { object } => {
                if let Ok(id) = u32::try_from(object - SHIP_IDS) {
                    nav.script_damage(id, u32::MAX);
                }
                if let Some(effect) = nav.effects.iter_mut().find(|e| e.tag == Some(object)) {
                    effect.alive = false;
                }
            }
            Action::Name { object, code, word } => {
                let Some(messages) = cache
                    .asset("base:text/messages")
                    .and_then(|bytes| ffe_core::message_strings::Messages::new(bytes).ok())
                else {
                    continue;
                };
                let mut entry = ffe_core::script::menu::Entry {
                    code,
                    ..Default::default()
                };
                entry.words[0] = word;
                let station = nav.docked.unwrap_or(0);
                let Ok(text) =
                    nav.with_world(cache, station, |world| entry_text(&messages, world, &entry))
                else {
                    continue;
                };
                if object == PLAYER_ID {
                    state.script_name = Some(text);
                } else if let Some(ship) = ship_of_mut(nav, object) {
                    ship.name = text;
                }
            }
            Action::Canister { tag, cargo, near } => {
                let at = object_position(nav, player, near).unwrap_or(player);
                let velocity = ship_of(nav, near).map_or([0.0; 3], |ship| ship.velocity);
                let mut canister = ffe_core::effects::canister(
                    at,
                    velocity,
                    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    [0.0; 3],
                    cargo,
                    &mut nav.random,
                );
                canister.tag = Some(tag);
                nav.effects.push(canister);
            }
            Action::Jettison { tag, commodity } => {
                let axes = crate::camera::ship_axes(state);
                let at: [i64; 3] =
                    std::array::from_fn(|k| player[k] - (axes[2][k] * 20_000.0) as i64);
                let cargo = ffe_core::effects::JETTISON_CARGO
                    .get(commodity)
                    .map_or(commodity as u16, |code| u16::from(*code));
                let velocity = state
                    .velocity
                    .map(|v| v * crate::original_flight::UNITS_PER_FLIGHT_UNIT);
                let mut canister = ffe_core::effects::canister(
                    at,
                    velocity,
                    axes,
                    axes[2].map(|v| v * 2_500.0),
                    cargo,
                    &mut nav.random,
                );
                canister.tag = Some(tag);
                nav.effects.push(canister);
            }
            Action::Ai {
                object,
                state: code,
            } => {
                if let Some(ship) = ship_of_mut(nav, object) {
                    let target = target_of(ship.script_target);
                    ship.set_ai(code, target);
                }
            }
            Action::Target { object, target } => {
                if let Some(ship) = ship_of_mut(nav, object) {
                    ship.script_target = target;
                    if let Some(code) = ship.ai_code {
                        ship.set_ai(code, target_of(target));
                    }
                }
            }
            Action::Model { object, model } => {
                if object == PLAYER_ID {
                    // F441 on the player: the ship the hold, statistics and
                    // drawing read.
                    if crate::ships::npc_spec(cache, model).is_some() {
                        state.ship = model;
                    }
                } else if let Some(ship) = ship_of_mut(nav, object) {
                    ship.model = model;
                }
            }
            Action::Seed { object, seed } => {
                if object == PLAYER_ID {
                    state.script_seed = seed;
                } else if let Some(ship) = ship_of_mut(nav, object) {
                    ship.seed = seed;
                }
            }
            Action::Object {
                ship,
                model,
                body,
                seed,
                place,
            } => {
                let Some(spec) = crate::ships::object_spec(cache, model, 0x400) else {
                    continue;
                };
                let index = usize::try_from(body - BODY_IDS)
                    .ok()
                    .filter(|index| *index < nav.bodies.len());
                let clock = crate::navigation::clock_words(nav.clock);
                let mut site = crate::navigation::ScriptSite {
                    ship,
                    system: nav.current_code(),
                    body: index.unwrap_or(0),
                    offset: [0.0; 3],
                    surface: false,
                    local: None,
                    orbit: None,
                };
                match place {
                    Place::Surface {
                        latitude,
                        longitude,
                    } => {
                        // F297's F879: on the planet's terrain, in its
                        // rotating frame.
                        let Some(index) = index else {
                            continue;
                        };
                        let Some(placed) =
                            nav.surface_site(cache, index, model, latitude, longitude)
                        else {
                            eprintln!("mission scripts: F879 cannot place model {model}");
                            continue;
                        };
                        site.local = Some(placed);
                        site.surface = true;
                    }
                    Place::Orbit {
                        radius,
                        offset,
                        rate,
                        longitude,
                        spin,
                        tilt,
                    } => {
                        let (Some(sine), Some(class)) =
                            (sine.as_ref(), crate::ships::model_class(cache, model))
                        else {
                            continue;
                        };
                        site.orbit = Some(ffe_core::objects::Orbiter::new(
                            sine,
                            radius as u32,
                            offset,
                            rate,
                            longitude,
                            spin,
                            tilt,
                            class,
                            clock,
                        ));
                    }
                }
                let mut made = npc::script_object(&spec, ship, player, &mut nav.random);
                made.seed = seed;
                nav.ships.push(made);
                if index.is_some() {
                    nav.script_sites.push(site);
                } else if let Some(orbit) = site.orbit {
                    // F40 round the root.
                    if let Some(made) = nav.ships.last_mut() {
                        made.position = orbit.position;
                        made.anchor = orbit.position;
                        set_rows(made, &orbit.rows);
                    }
                }
            }
            Action::Sites => {
                // F297/F40's objects stay with their body.
                let sites = std::mem::take(&mut nav.script_sites);
                let system = nav.current_code();
                let clock = crate::navigation::clock_words(nav.clock);
                let mut kept = Vec::new();
                for mut site in sites.into_iter().filter(|site| site.system == system) {
                    let Some(body) = nav.bodies.get(site.body) else {
                        continue;
                    };
                    let (offset, rows): ([i64; 3], Option<[i32; 9]>) =
                        if let (Some(orbit), Some(sine)) = (site.orbit.as_mut(), sine.as_ref()) {
                            // F866 moves F40's object on its orbit.
                            orbit.advance(sine, clock);
                            (orbit.position, Some(orbit.rows))
                        } else if let Some((local, rows)) = site.local {
                            // F1677: the rotating frame's place turned by
                            // the body's rows, as the starports are.
                            let (vector, shift) = ffe_core::frames::truncate(local, 0x1c);
                            (
                                ffe_core::frames::shift_up(
                                    ffe_core::frames::vec_mat_mul(
                                        vector.map(|v| v as i32),
                                        &body.rows,
                                    ),
                                    shift,
                                ),
                                Some(ffe_core::frames::mat_mat_mul(&rows, &body.rows)),
                            )
                        } else if site.surface {
                            let turned = body.orientation;
                            let offset: [f64; 3] = std::array::from_fn(|k| {
                                (0..3).map(|i| site.offset[i] * turned[i][k]).sum::<f64>()
                            });
                            (offset.map(|v| v as i64), None)
                        } else {
                            (site.offset.map(|v| v as i64), None)
                        };
                    let at: [i64; 3] =
                        std::array::from_fn(|k| body.position[k].wrapping_add(offset[k]));
                    let Some(ship) = nav
                        .ships
                        .iter_mut()
                        .find(|ship| ship.id == site.ship && ship.alive())
                    else {
                        continue;
                    };
                    ship.position = at;
                    ship.anchor = at;
                    if let Some(rows) = rows {
                        set_rows(ship, &rows);
                    }
                    kept.push(site);
                }
                nav.script_sites = kept;
            }
            Action::Price {
                station,
                commodity,
                price,
            } => {
                let Some(index) = usize::try_from(station - BODY_IDS)
                    .ok()
                    .filter(|index| nav.bodies.get(*index).is_some_and(|b| b.kind.is_place()))
                else {
                    continue;
                };
                if let Ok(market) = nav.market(cache, index) {
                    if let Some(entry) = market.market.entries.get_mut(commodity) {
                        entry.price = price;
                    }
                }
            }
            Action::Delivery {
                basic_data,
                reward,
                destination,
                deadline_day,
                seed,
            } => {
                let destination_name = nav
                    .system_name_for(destination)
                    .unwrap_or_default()
                    .to_owned();
                let day = absolute_day(nav.clock());
                let origin = nav.current_code();
                let origin_name = nav.system_name().to_owned();
                state.missions.add_script_delivery(
                    basic_data,
                    reward,
                    destination,
                    &destination_name,
                    deadline_day,
                    seed,
                    day,
                    origin,
                    &origin_name,
                );
            }
        }
    }
    made_attackers
}

/// F57's new game for the scripts: start mission 0 and publish the
/// journals' first issues.
pub fn new_game(assets: &Assets, hooks: &mut Hooks<'_>, docked: bool) -> Result<Scripts, String> {
    let mut scripts = Scripts::new(&assets.image);
    let today = absolute_day(hooks.clock);
    scripts.new_game(&assets.image, &assets.stories, hooks, today, docked)?;
    Ok(scripts)
}

/// The daily message for each day the clock crossed, then F633's daily
/// market signal for each of the system's stations and starports (`places`,
/// their script ids): global 0x10 is the station's id while its prices may
/// be set (target 0x42).
pub fn days(
    scripts: &mut Scripts,
    assets: &Assets,
    hooks: &mut Hooks<'_>,
    crossed: &[u32],
    docked: bool,
    places: &[i32],
) -> Result<(), String> {
    for day in crossed {
        scripts.daily(&assets.image, &assets.stories, hooks, *day, docked)?;
        for place in places {
            scripts.pulse(&assets.image, hooks, *day, signal::STATION_DAY, *place)?;
        }
    }
    Ok(())
}

/// The script ids of the current system's stations and starports.
pub fn places(nav: &crate::navigation::Navigator) -> Vec<i32> {
    nav.bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.kind.is_place())
        .map(|(index, _)| BODY_IDS + index as i32)
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

/// Expand `entries` at station `station` for the playable.
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
    with_texts(
        state,
        nav,
        cache,
        station,
        assets,
        messages,
        commander,
        |scripts, hooks, texts, today| {
            entries
                .iter()
                .map(|entry| expand(scripts, hooks, texts, today, entry))
                .collect()
        },
    )
    .unwrap_or_default()
}

/// One article of a journal's current issue as the viewer shows it (F71's
/// text: the publication, the headline and the body), its `[` macros
/// expanded with the article's slot words (F95).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    pub publication: String,
    pub headline: String,
    pub body: String,
    /// What F78 copies: the raw publication line and the rest through its
    /// copy pass with the macros expanded inline.
    pub source: crate::journal_page::Source,
}

/// The ship library's journals (F81's list): each journal's slot and name,
/// for the journals the scripts have made.
pub fn journals(scripts: &Scripts) -> Vec<(usize, String)> {
    scripts
        .library
        .journals
        .iter()
        .enumerate()
        .filter(|(_, journal)| journal.period != 0)
        .map(|(slot, journal)| {
            let name = journal.name.split(|byte| *byte == 0).next().unwrap_or(&[]);
            (slot, String::from_utf8_lossy(name).into_owned())
        })
        .collect()
}

/// F71/F79: the articles of `journal`'s current issue, expanded.
#[allow(clippy::too_many_arguments)]
pub fn issue(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    station: usize,
    assets: &Assets,
    messages: &ffe_core::message_strings::Messages<'_>,
    commander: &str,
    journal: usize,
) -> Vec<Article> {
    let Ok(archive) = ffe_core::journal::Journal::new(&assets.archive) else {
        return Vec::new();
    };
    with_texts(
        state,
        nav,
        cache,
        station,
        assets,
        messages,
        commander,
        |scripts, hooks, texts, today| {
            let records = scripts.library.shown_records(journal);
            let mut out = Vec::new();
            for (article, slots) in records {
                let Some(text) = usize::try_from(article)
                    .ok()
                    .and_then(|article| archive.article(article))
                else {
                    continue;
                };
                // F95: the article's slot words are DATA_008568 while its
                // macros expand.
                let saved: Vec<i32> = scripts.library.words.iter().take(10).copied().collect();
                for (word, slot) in scripts.library.words.iter_mut().zip(slots) {
                    *word = slot;
                }
                let mut expand_bytes = |bytes: &[u8]| {
                    let mut macros = GameMacros {
                        scripts: &mut *scripts,
                        hooks: &mut *hooks,
                        texts,
                        today,
                        depth: 0,
                    };
                    ffe_core::script::brackets::expand(bytes, &mut macros)
                };
                let string = |bytes: Vec<u8>| bytes.into_iter().map(char::from).collect::<String>();
                // F78: the publication line as it is; the rest through the
                // copy pass, F76 expanding its macros as it goes.
                let raw = usize::try_from(article)
                    .ok()
                    .and_then(|article| archive.raw(article))
                    .unwrap_or(&[]);
                let rest = raw
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(&[][..], |end| &raw[end + 1..]);
                let source = crate::journal_page::Source {
                    publication: text.publication.to_vec(),
                    text: expand_bytes(&crate::journal_page::copy_pass(rest)),
                };
                let article = Article {
                    publication: string(expand_bytes(text.publication)),
                    headline: string(expand_bytes(text.headline)),
                    body: string(expand_bytes(text.body)),
                    source,
                };
                for (word, value) in scripts.library.words.iter_mut().zip(saved) {
                    *word = value;
                }
                out.push(article);
            }
            out
        },
    )
    .unwrap_or_default()
}

/// Run `f` with the scripts, their hooks and the texts' world at station
/// `station`: the shared pair is moved out of the navigator while its world
/// view is borrowed. `None` when the flight has no scripts.
#[allow(clippy::too_many_arguments)]
fn with_texts<R>(
    state: &mut crate::flight::Flight,
    nav: &mut crate::navigation::Navigator,
    cache: &ffe_content::VerifiedCache,
    station: usize,
    assets: &Assets,
    messages: &ffe_core::message_strings::Messages<'_>,
    commander: &str,
    f: impl FnOnce(&mut Scripts, &mut Hooks<'_>, &Texts<'_>, u32) -> R,
) -> Option<R> {
    state.scripts.as_ref()?;
    let snapshot = Snapshot::take(state, nav, cache);
    let today = absolute_day(snapshot.clock);
    let mut random = std::mem::replace(&mut nav.random, Random::start());
    let mut posts = Vec::new();
    let (out, left) = {
        let crate::flight::Flight {
            scripts,
            credits,
            hold,
            loadout,
            combat,
            missions,
            ..
        } = &mut *state;
        let Some(scripts) = scripts.as_mut() else {
            nav.random = random;
            return None;
        };
        let cargo = Cargo {
            hold,
            capacity: 0,
            loadout,
            combat,
        };
        let mut hooks = Hooks::new(
            snapshot,
            credits,
            &mut random,
            &mut posts,
            cache.asset("base:world/system-tables"),
            Some(cargo),
            Some(missions),
        );
        let out = nav
            .with_world(cache, station, |world| {
                let texts = Texts {
                    assets,
                    messages,
                    world,
                    commander,
                };
                f(scripts, &mut hooks, &texts, today)
            })
            .ok();
        (out, hooks.finish())
    };
    nav.random = random;
    left.store(state, nav);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffe_core::equipment::{self, Category, Loadout};

    struct Game {
        credits: u32,
        random: Random,
        posts: Vec<String>,
        hold: Vec<u16>,
        loadout: Loadout,
        combat: crate::combat::Combat,
        missions: ffe_core::missions::State,
    }

    impl Game {
        fn new() -> Self {
            Self {
                credits: 1000,
                random: Random::start(),
                posts: Vec::new(),
                hold: vec![0; 0x26],
                loadout: Loadout::default(),
                combat: crate::combat::Combat::default(),
                missions: ffe_core::missions::State::default(),
            }
        }

        fn hooks(&mut self) -> Hooks<'_> {
            let snapshot = Snapshot {
                system: 0x1234,
                clock: 0x12_1cf7 as f64 * 86_400.0,
                legal: Default::default(),
                career: Default::default(),
                next_ship: 5,
                next_tag: TAG_IDS,
                ships: vec![ShipView {
                    id: PLAYER_ID,
                    model: 30,
                    ai: 0,
                    target: 0,
                    seed: 0,
                    distance: 0,
                }],
                bodies: vec![
                    BodyView {
                        id: BODY_IDS,
                        model: 0x80,
                        seed: 0xdb2f_337a,
                        class: 0,
                    },
                    BodyView {
                        id: BODY_IDS + 1,
                        model: 20,
                        seed: 0x9c80_553e,
                        class: 0x10,
                    },
                ],
                values: [0; 21],
                allegiance: 1,
                capacity: 20,
            };
            Hooks::new(
                snapshot,
                &mut self.credits,
                &mut self.random,
                &mut self.posts,
                None,
                Some(Cargo {
                    hold: &mut self.hold,
                    capacity: 0,
                    loadout: &mut self.loadout,
                    combat: &mut self.combat,
                }),
                Some(&mut self.missions),
            )
        }
    }

    #[test]
    fn f1507s_packed_distance_is_the_mantissa_shifted_by_the_exponent() {
        assert_eq!(packed_distance(0x0017_7fff), 32_767.0 * 256.0);
        assert_eq!(packed_distance(0x0025_7fff), 32_767.0 * f64::from(1 << 22));
        assert_eq!(packed_distance(0x000f_0001), 1.0);
    }

    #[test]
    fn a_created_ship_is_placed_and_flown_in_formation() {
        let mut game = Game::new();
        let mut hooks = game.hooks();
        // F772(0x11): a Thargoid-list ship; its id is the next ship's.
        let ship = hooks.statement(19, &[0x11], &[]);
        assert_eq!(ship, 5 + SHIP_IDS);
        let leader = hooks.statement(19, &[0x11], &[]);
        // Statement 54 at 0x197fff (33.5 km) from the player.
        hooks.statement(54, &[ship, PLAYER_ID, 0x0019_7fff], &[]);
        // F37's targets: the attack target, then AI state 5.
        assert_eq!(hooks.assign(0x41, &[ship], PLAYER_ID), None);
        assert_eq!(hooks.assign(0x3e, &[ship], 5), Some((0x17, 0, ship)));
        assert_eq!(hooks.query(0x3e, &[ship]), 5);
        assert_eq!(hooks.query(0x41, &[ship]), PLAYER_ID);
        // Statement 60: formation place 9 behind the leader.
        hooks.statement(60, &[leader, ship, 9], &[]);
        assert_eq!(hooks.query(0x3e, &[leader]), 0xb);
        let actions = hooks.finish().actions;
        assert!(matches!(
            actions[0],
            Action::Create {
                ship: 5,
                kind: 0x11,
                ..
            }
        ));
        assert_eq!(
            actions[2],
            Action::Place {
                object: ship,
                parent: PLAYER_ID,
                distance: 0x0019_7fff,
            }
        );
        assert_eq!(
            actions[5],
            Action::Formation {
                object: leader,
                leader: ship,
                slot: 9,
            }
        );
    }

    #[test]
    fn the_searches_find_planets_and_starports_by_their_code() {
        let mut game = Game::new();
        let mut hooks = game.hooks();
        assert_eq!(hooks.query(0x3c, &[0xdb2f_337a_u32 as i32]), BODY_IDS);
        assert_eq!(hooks.query(0x33, &[0x9c80_553e_u32 as i32]), BODY_IDS + 1);
        // F29 wants a planet model, F28 a starport class.
        assert_eq!(hooks.query(0x3c, &[0x9c80_553e_u32 as i32]), 0);
        assert_eq!(hooks.query(0x33, &[0xdb2f_337a_u32 as i32]), 0);
        assert_eq!(hooks.query(0x38, &[BODY_IDS + 1]), 0x9c80_553e_u32 as i32);
    }

    #[test]
    fn the_cargo_token_sets_the_hold_within_its_space_and_fits_equipment() {
        let mut game = Game::new();
        let mut hooks = game.hooks();
        // Commodity 3 (1003): 25 t asked, the 20 t hold takes 20.
        hooks.assign(0x25, &[PLAYER_ID, 1003], 25);
        assert_eq!(hooks.query(0x25, &[PLAYER_ID, 1003]), 20);
        hooks.assign(0x25, &[PLAYER_ID, 1003], 4);
        assert_eq!(hooks.query(0x25, &[PLAYER_ID, 1003]), 4);
        // Statement 58 jettisons one tonne and returns the canister's id.
        assert_eq!(hooks.statement(58, &[PLAYER_ID, 1003], &[]), TAG_IDS);
        assert_eq!(hooks.query(0x25, &[PLAYER_ID, 1003]), 3);
        // Two shield generators by F390, then none by F391.
        let shield = equipment::equipment()
            .iter()
            .position(|item| item.category == Category::Shield)
            .unwrap() as i32;
        hooks.assign(0x25, &[PLAYER_ID, 100 + shield], 2);
        assert_eq!(hooks.query(0x25, &[PLAYER_ID, 100 + shield]), 2);
        hooks.assign(0x25, &[PLAYER_ID, 100 + shield], 0);
        assert_eq!(hooks.query(0x25, &[PLAYER_ID, 100 + shield]), 0);
        // Another object's cargo is not the player's.
        assert_eq!(hooks.query(0x25, &[5 + SHIP_IDS, 1003]), 0);
        drop(hooks);
        assert_eq!(game.hold[3], 3);
        assert_eq!(game.credits, 1000);
    }

    #[test]
    fn script_contracts_fines_and_ranks() {
        let mut game = Game::new();
        let mut hooks = game.hooks();
        // F314 then F315.
        let first = hooks.statement(37, &[], &[]);
        let second = hooks.statement(37, &[], &[]);
        assert_eq!((first, second), (1, 2));
        hooks.statement(38, &[first], &[]);
        // F236(Federal, offence 3): its points and fine on slot 2.
        hooks.statement(40, &[2, 3], &[]);
        let offence = ffe_core::npc::OFFENCES[3];
        assert_eq!(hooks.query(0x30, &[2]), offence.points as i32);
        assert_eq!(hooks.query(0x23, &[2]), (offence.fine * 10) as i32);
        // F304: 20 Federal points and no medal.
        hooks.statement(53, &[2, 20, -1], &[]);
        assert_eq!(hooks.query(0x22, &[2]), 20);
        assert!(!hooks.posts.is_empty());
        drop(hooks);
        let ids: Vec<i32> = game
            .missions
            .script_contracts
            .iter()
            .map(|contract| contract.id)
            .collect();
        assert_eq!(ids, [2]);
        assert_eq!(game.missions.script_contracts[0].system, 0x1234);
    }
}
