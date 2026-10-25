//! Mission contracts, the journal and the message pages.
//!
//! The BBS's reply page (`F521`) ends with the "OK - agreed." reply (`F534`),
//! which clears the advertisement (`F535`) and adds a contract (`F290`). The
//! contract is the 20-byte record F301/F302/F303 read:
//!
//! ```text
//! +0x00  reward (whole credits, from the row's StrVars[0])
//! +0x04  basic_data (passenger/parcel/additive flags)
//! +0x08  destination system code
//! +0x0c  deadline: game day (F534: accept day + 0x1c)
//! +0x10  name seed (F791's `0x92xx` names)
//! ```
//!
//! F301 marks an overdue contract, F302 completes one when the current system
//! is its destination, and F303 pays/removes it, writing a completion page
//! (0x9c31/0x9c32/0x9c33/0x9c34, or 0x9c29/0x9c2c on failure). The military
//! completion also shows F307's medal and F306's promotion/demotion pages,
//! which start songs 10 and 7. The strings are the original 0xff-coded text
//! from `DATA_005299`/`DATA_005104`; [`text`] expands them through the same
//! F1348 subset as the advertisements.
//!
//! The port keeps the contract fields, the journal entries the player sees and
//! the message pages. Its cash is the original's tenths (`$10.0` is 100), so a
//! completion returns whole credits and the caller adds `reward * 10`; the
//! equivalence is checked in `docs/notes/missions.md`.

use crate::bulletin::{self, Destination, Post, Var, World};
use serde::{Deserialize, Serialize};

/// F534's `0x3b` contract cap (`DATA_008804+0x4a18` / `DATA_009073`).
pub const MAX_MISSIONS: usize = 0x3b;
/// The journal keeps the last entries; the original journal page shows one
/// issue at a time and the port's screen scrolls a ring.
pub const MAX_JOURNAL: usize = 256;
/// The message pages the comms screen can list.
pub const MAX_PAGES: usize = 64;
/// F534's acceptance deadline: the current day + 0x1c days.
pub const ACCEPT_DAYS: u32 = 0x1c;
/// F303/DATA_008922's reputation ceiling (`cmp ...,0x5`).
pub const MAX_REPUTATION: u8 = 5;

/// The contract kinds the BBS can generate. F638's 0x981a/0x981b posts are the
/// passenger/parcel jobs (the passage string picks which); F643's 0x981f..
/// 0x9822 posts are the removal/assassination jobs. The military delivery
/// (F509's 0x9854 page) uses the same contract record but has no BBS generator
/// in this port; see `docs/notes/missions.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Passenger,
    Parcel,
    Assassination,
    MilitaryDelivery,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Passenger => "passenger",
            Kind::Parcel => "parcel",
            Kind::Assassination => "removal",
            Kind::MilitaryDelivery => "military delivery",
        }
    }
}

/// The contract's state. Completed/failed contracts stay in the list (so the
/// journal has their details) and an active one is removed by [`State::sweep`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Active,
    Complete,
    Failed,
    Expired,
    Abandoned,
}

/// One accepted contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mission {
    pub id: u32,
    pub kind: Kind,
    /// The BBS advertisement's message code (0x981a/0x981b/0x981f..0x9822).
    pub code: u16,
    /// The system and station the contract was accepted at.
    pub origin: u32,
    pub origin_name: String,
    pub destination: u32,
    pub destination_name: String,
    /// Whole credits, as the BBS row's money slot stores them.
    pub reward: u32,
    pub accepted_day: u32,
    pub deadline_day: u32,
    /// F638's basic_data low three bits + one; 0 for a parcel/removal.
    pub passengers: u8,
    /// The passenger passage text, parcel description or removal title.
    pub cargo: String,
    /// F791's `0x92xx` name seed (the passenger or the removal target).
    pub seed: u32,
    /// The raw F638/F643 data word, kept for the source record.
    pub basic_data: u32,
    pub status: Status,
    pub settled_day: Option<u32>,
}

impl Mission {
    /// The day F303/F301's comparison uses: the contract's `+0xc` deadline.
    pub fn overdue(&self, day: u32) -> bool {
        day > self.deadline_day
    }

    /// `true` when the contract is a parcel (no cabin count).
    pub fn is_parcel(&self) -> bool {
        self.kind == Kind::Parcel
    }
}

/// What a journal entry records. The text is the source page's expanded string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Accepted,
    Completed,
    Failed,
    Expired,
    Abandoned,
    Rank,
}

/// One journal entry (`MISSION.DAT`'s text, but for the contract events the
/// port can generate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub day: u32,
    pub kind: EntryKind,
    pub text: String,
}

/// The F306/F307 message pages: a normal comm message (F349's bong), the rank
/// page (song 7) or the medal page (song 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageKind {
    Message,
    Rank,
    Medal,
}

impl PageKind {
    /// The song `F306`/`F307` starts for this page, if any.
    pub fn song(self) -> Option<i16> {
        match self {
            PageKind::Message => None,
            PageKind::Rank => Some(7),
            PageKind::Medal => Some(10),
        }
    }

    /// The source routine that shows the page.
    pub fn citation(self) -> &'static str {
        match self {
            PageKind::Message => "F349/F350 (ffe.asm:27059-27185)",
            PageKind::Rank => "F306 (ffe.asm:24542)",
            PageKind::Medal => "F307 (ffe.asm:24622)",
        }
    }
}

/// One message page the comms screen lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub kind: PageKind,
    pub day: u32,
    pub text: String,
    /// The original's F307 bit (`DATA_009091`) / F349 bell: the page is new.
    pub unread: bool,
}

/// The mission, journal and message state. All fields default so a save from
/// before this slice loads with no missions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub missions: Vec<Mission>,
    #[serde(default)]
    pub journal: Vec<Entry>,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub next_id: u32,
    /// `DATA_008922`: 0..=5. F303 raises it on a completed civilian contract,
    /// F534 refuses passengers below its random roll, and F485 lowers it on a
    /// betrayal.
    #[serde(default)]
    pub reputation: u8,
    /// The military rank points F306's `DATA_005114` thresholds compare.
    #[serde(default)]
    pub military_points: u32,
    /// The rank index derived from `military_points` (0..=8). Cached so the
    /// screens do not need the thresholds.
    #[serde(default)]
    pub rank: u8,
}

impl Default for State {
    fn default() -> Self {
        Self {
            missions: Vec::new(),
            journal: Vec::new(),
            pages: Vec::new(),
            next_id: 1,
            reputation: 0,
            military_points: 0,
            rank: 0,
        }
    }
}

/// The `DATA_005114` thresholds: 1, 16, 81, 256, 729, 4096, 6561, 14641, then
/// the end marker. They are the fourth powers the original uses (`F306`).
pub const RANK_THRESHOLDS: [u32; 8] = [1, 16, 81, 256, 729, 4096, 6561, 14641];

/// The military rank names for [`RANK_THRESHOLDS`]' nine bands. These are the
/// first nine Federal rank strings (`DATA_005104`'s 0x9c4e..0x9c56: Private,
/// Corporal, Sergeant, ...); the Imperial titles (0x9c5b..0x9c66) are the
/// other ladder, which needs an allegiance field the port's 20-byte contract
/// does not carry.
pub const RANK_NAMES: [&str; 9] = [
    "Private",
    "Corporal",
    "Sergeant",
    "Sergeant-Major",
    "Major",
    "Colonel",
    "Lieutenant",
    "Lt. Commander",
    "Captain",
];

/// The rank index for a points total (`F306`'s loop over `DATA_005114`).
pub fn rank_for(points: u32) -> u8 {
    RANK_THRESHOLDS
        .iter()
        .position(|threshold| points < *threshold)
        .map_or(8, |index| index as u8)
}

/// An offer read out of a BBS post. `None` for the advertisements that are not
/// contracts (goods, crew, charity, military recruitment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub kind: Kind,
    pub code: u16,
    pub destination: u32,
    pub destination_name: String,
    pub reward: u32,
    pub passengers: u8,
    pub cargo: String,
    pub seed: u32,
    pub basic_data: u32,
    /// The contract's `+0xc` deadline in days from acceptance. Civilian
    /// offers use F534's 0x1c; F632's military package uses `+0xe` plus
    /// `random(0x3f)` (14..77).
    pub deadline_days: u32,
    /// The row/page text: the BBS post for a civilian offer, F493's 0x9854
    /// mission description for a military one.
    pub text: String,
}

/// Read F453's row variables back into a contract. The post's variables are
/// F646's five args: 0 money, 1 passage description/title, 2 destination,
/// 3 name seed, 4 basic_data (F638) or the removal title (F643).
pub fn offer(post: &Post, world: &World<'_>) -> Option<Offer> {
    let code = post.code;
    let kind = match code {
        0x981a | 0x981b => {
            let basic = match post.vars[4] {
                Var::Int(value) => value as u32,
                _ => 0,
            };
            if basic & 0x2000 != 0 {
                Kind::Parcel
            } else {
                Kind::Passenger
            }
        }
        0x981f..=0x9822 => Kind::Assassination,
        _ => return None,
    };
    let reward = match post.vars[0] {
        Var::Int(value) if value > 0 => u32::try_from(value).ok()?,
        _ => return None,
    };
    let destination = match post.vars[2] {
        Var::System(code) => code,
        _ => return None,
    };
    // F643's removal title is StrVars[4] (`0x301c`); F638's passage
    // description is StrVars[1]. The source's date slot (StrVars[1] for
    // removal) is not the contract deadline, which F534 rewrites.
    let cargo = match (kind, &post.vars[1], &post.vars[4]) {
        (Kind::Assassination, _, Var::Text(title)) => title.clone(),
        (_, Var::Text(text), _) => text.clone(),
        _ => String::new(),
    };
    let seed = match post.vars[3] {
        Var::Seed(seed) => seed,
        Var::Int(seed) => seed as u32,
        Var::System(code) => code,
        Var::Text(_) => 0,
    };
    let basic_data = match post.vars[4] {
        Var::Int(value) => value as u32,
        _ => 0,
    };
    let passengers = if kind == Kind::Passenger {
        ((basic_data & 7) as u8).saturating_add(1)
    } else {
        0
    };
    Some(Offer {
        kind,
        code,
        destination,
        destination_name: world.system_name(destination),
        reward,
        passengers,
        cargo,
        seed,
        basic_data,
        deadline_days: ACCEPT_DAYS,
        text: post.text.clone(),
    })
}

/// F629/F630/F631/F632: the military package (delivery) offers. The three
/// tiers are the source's base strings 0x996e (8 variants), 0x9976 (4) and
/// 0x997a (4); F630/F631 refuse the higher two below `DATA_005114`'s 0x10/0x51
/// rank points. The reward is F632's arithmetic, decoded from the asm's
/// `50 * (base_string + additive + 2 * random(4)) - 0x1df6e6`: the constants
/// give 150, 550 and 750 credits before the additive/random term. The deadline
/// is `day + 0xe + random(0x40)`. F623's other constructors need target
/// ships/military bases and are not generated.
pub fn military_offers(
    rng: &mut crate::npc::Random,
    world: &World<'_>,
    day: u32,
    rank_points: u32,
) -> Vec<Offer> {
    let tiers = [
        (0x996e_u16, 8_u32, 150_u32, 0_u32),
        (0x9976, 4, 550, 0x10),
        (0x997a, 4, 750, 0x51),
    ];
    let mut offers = Vec::new();
    for (base_code, variants, base_reward, minimum_rank) in tiers {
        if rank_points <= minimum_rank && minimum_rank != 0 {
            continue;
        }
        let additive = rng.bounded(variants);
        let Some(destination) = world.neighbour(rng, 3) else {
            continue;
        };
        // F632's `esi` is F922(3), reused by F857's system pick and the
        // reward's 2*esi term; the port draws it after the destination.
        let esi = rng.bounded(4);
        let reward = base_reward
            .saturating_add(50 * additive)
            .saturating_add(100 * esi);
        let deadline_days = 14 + rng.bounded(0x40);
        let seed = rng.bounded(0x1_0000);
        let cargo_code = base_code + additive as u16;
        let cargo = bulletin::render_code(
            cargo_code,
            &[
                Var::Int(0),
                Var::Int(0),
                Var::System(destination.code),
                Var::Seed(seed),
                Var::Int(0),
            ],
            world,
        )
        .unwrap_or_else(|| format!("[{cargo_code:#06x}]"));
        let vars = [
            Var::Int(i64::from(reward)),
            Var::Int(i64::from(day.saturating_add(deadline_days))),
            Var::System(destination.code),
            Var::Seed(seed),
            Var::Text(cargo.clone()),
        ];
        let text = bulletin::render_code(0x9854, &vars, world).unwrap_or_default();
        let tier = if base_code == 0x997a {
            2
        } else if base_code == 0x9976 {
            1
        } else {
            0
        };
        offers.push(Offer {
            kind: Kind::MilitaryDelivery,
            code: 0x9854,
            destination: destination.code,
            destination_name: destination.name.clone(),
            reward,
            passengers: 0,
            cargo,
            seed,
            // F509: 0x2000 plus 0x100 for each rank tier above the first.
            basic_data: 0x2000 | (tier * 0x100),
            deadline_days,
            text,
        });
    }
    offers
}

fn same_offer(mission: &Mission, offer: &Offer) -> bool {
    mission.kind == offer.kind
        && mission.code == offer.code
        && mission.destination == offer.destination
        && mission.reward == offer.reward
        && mission.seed == offer.seed
}

/// A successful acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub id: u32,
    pub text: String,
}

/// A settled contract: what the caller pays and which page to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub id: u32,
    pub status: Status,
    /// Whole credits the contract pays (`0` on a failure).
    pub reward: u32,
    pub page: Page,
    pub journal: Entry,
}

fn text_world<'a>(origin: u32, origin_name: &'a str, destinations: &'a [Destination]) -> World<'a> {
    World {
        code: origin,
        name: origin_name,
        population: 0,
        danger: 0,
        allegiance: 0,
        station_name: "",
        destinations,
        names: None,
    }
}

/// Expand one source page. `vars` are F646's StrVars slots.
fn page_text(
    code: u16,
    vars: &[Var; 5],
    origin: u32,
    origin_name: &str,
    destination: u32,
    destination_name: &str,
) -> String {
    let list = [Destination {
        code: destination,
        name: destination_name.to_owned(),
        population: 0,
        danger: 0,
    }];
    let world = text_world(origin, origin_name, &list);
    bulletin::render_code(code, vars, &world)
        .unwrap_or_else(|| format!("[mission page {code:#06x}]"))
}

/// F534's acceptance page: 0x992a ($reward on arrival at the destination).
fn accepted_text(offer: &Offer, origin: u32, origin_name: &str) -> String {
    let vars = [
        Var::Int(i64::from(offer.reward)),
        Var::Text(offer.cargo.clone()),
        Var::System(offer.destination),
        Var::Seed(offer.seed),
        Var::Int(i64::from(offer.basic_data)),
    ];
    page_text(
        0x992a,
        &vars,
        origin,
        origin_name,
        offer.destination,
        &offer.destination_name,
    )
}

impl State {
    /// F534: accept an offer. The caller removes the advertisement (`F535`) and
    /// pays nothing; the reward is paid by [`Self::settle`] on arrival.
    pub fn accept(
        &mut self,
        offer: &Offer,
        day: u32,
        origin: u32,
        origin_name: &str,
    ) -> Result<Accepted, String> {
        if self
            .missions
            .iter()
            .filter(|m| m.status == Status::Active)
            .count()
            >= MAX_MISSIONS
        {
            return Err("There's no room on your carriage roster.".to_owned());
        }
        let id = self.next_id.max(1);
        let text = accepted_text(offer, origin, origin_name);
        let mission = Mission {
            id,
            kind: offer.kind,
            code: offer.code,
            origin,
            origin_name: origin_name.to_owned(),
            destination: offer.destination,
            destination_name: offer.destination_name.clone(),
            reward: offer.reward,
            accepted_day: day,
            deadline_day: day.saturating_add(offer.deadline_days),
            passengers: offer.passengers,
            cargo: offer.cargo.clone(),
            seed: offer.seed,
            basic_data: offer.basic_data,
            status: Status::Active,
            settled_day: None,
        };
        self.missions.push(mission);
        self.next_id = id.saturating_add(1);
        self.record(day, EntryKind::Accepted, text.clone());
        self.push_page(PageKind::Message, day, text.clone());
        Ok(Accepted { id, text })
    }

    /// F303: settle a contract that is due at `system`. A contract delivered
    /// after its deadline pays nothing and is 0x9c2c; one completed in time
    /// pays its reward and raises reputation.
    pub fn settle(&mut self, id: u32, day: u32, system: u32) -> Result<Outcome, String> {
        let index = self
            .missions
            .iter()
            .position(|mission| mission.id == id)
            .ok_or_else(|| "No such mission.".to_owned())?;
        let (
            kind,
            origin,
            origin_name,
            destination,
            destination_name,
            reward,
            passengers,
            seed,
            basic_data,
            status,
        ) = {
            let mission = &self.missions[index];
            (
                mission.kind,
                mission.origin,
                mission.origin_name.clone(),
                mission.destination,
                mission.destination_name.clone(),
                mission.reward,
                mission.passengers,
                mission.seed,
                mission.basic_data,
                mission.status,
            )
        };
        if status != Status::Active {
            return Err("That mission is no longer active.".to_owned());
        }
        if destination != system {
            return Err("That mission is not for this system.".to_owned());
        }
        let late = day > self.missions[index].deadline_day;
        let (status, text, reward) = if late {
            self.missions[index].status = Status::Expired;
            self.missions[index].settled_day = Some(day);
            self.reputation = self.reputation.saturating_sub(1);
            (
                Status::Expired,
                page_text(
                    0x9c2c,
                    &[
                        Var::Int(0),
                        Var::Int(0),
                        Var::System(system),
                        Var::Seed(seed),
                        Var::Int(0),
                    ],
                    origin,
                    &origin_name,
                    destination,
                    &destination_name,
                ),
                0,
            )
        } else {
            self.missions[index].status = Status::Complete;
            self.missions[index].settled_day = Some(day);
            let code = match kind {
                Kind::Parcel => 0x9c34,
                Kind::MilitaryDelivery => 0x9c33,
                Kind::Passenger if passengers <= 1 => 0x9c31,
                Kind::Passenger => 0x9c32,
                Kind::Assassination => 0x9c34,
            };
            if kind != Kind::MilitaryDelivery {
                self.reputation = self.reputation.saturating_add(1).min(MAX_REPUTATION);
            } else {
                // F303's seed==0 branch awards the military rank points. The
                // contract's `basic_data` high bits are F509's tier flags; a
                // delivery gives the 0x9c33 page and the rank page follows.
                self.military_points = self.military_points.saturating_add(500);
                self.rank = rank_for(self.military_points);
            }
            let text = page_text(
                code,
                &[
                    Var::Int(i64::from(reward)),
                    Var::Int(i64::from(passengers)),
                    Var::System(destination),
                    Var::Seed(seed),
                    Var::Int(i64::from(basic_data)),
                ],
                origin,
                &origin_name,
                destination,
                &destination_name,
            );
            (Status::Complete, text, reward)
        };
        let journal_kind = if status == Status::Complete {
            EntryKind::Completed
        } else {
            EntryKind::Expired
        };
        self.record(day, journal_kind, text.clone());
        self.push_page(PageKind::Message, day, text.clone());
        if status == Status::Complete && kind == Kind::MilitaryDelivery {
            // F304/F305's rank page (F306, song 7). The package constructor
            // does not award a medal: F509's basic_data low bits are 0, so
            // F303 selects `DATA_005115[0]`/`DATA_005116[0]`, both 0xff, and
            // F307's medal page is skipped (`test ebx,ebx; jl`). The medal
            // path exists in `PageKind::Medal` for the medal-bearing military
            // constructors, which are not ported.
            let rank_name = RANK_NAMES[usize::from(self.rank.min(8))].to_owned();
            let rank = page_text(
                0x9c39,
                &[
                    Var::Int(0),
                    // 0x9c39's `0xff 0x19` is FF30WriteFFCodeV1, slot 1.
                    Var::Text(rank_name),
                    Var::System(destination),
                    Var::Seed(0),
                    Var::Int(0),
                ],
                origin,
                &origin_name,
                destination,
                &destination_name,
            );
            self.push_page(PageKind::Rank, day, rank.clone());
            self.record(day, EntryKind::Rank, rank);
        }
        Ok(Outcome {
            id,
            status,
            reward,
            page: self.pages.last().cloned().unwrap_or_else(|| Page {
                kind: PageKind::Message,
                day,
                text: text.clone(),
                unread: true,
            }),
            journal: self.journal.last().cloned().unwrap_or(Entry {
                day,
                kind: journal_kind,
                text,
            }),
        })
    }

    /// F301/F302's periodic check: complete a contract at `system`, or mark
    /// one that has run past its deadline. The caller passes the current day
    /// and system, and is responsible for the cash payment (`reward * 10`).
    pub fn tick(&mut self, day: u32, system: u32) -> Vec<Outcome> {
        let due: Vec<u32> = self
            .missions
            .iter()
            .filter(|mission| mission.status == Status::Active)
            .filter(|mission| mission.destination == system || mission.overdue(day))
            .map(|mission| mission.id)
            .collect();
        let mut outcomes = Vec::new();
        for id in due {
            let at_destination = self
                .missions
                .iter()
                .find(|mission| mission.id == id)
                .is_some_and(|mission| mission.destination == system);
            if at_destination {
                if let Ok(outcome) = self.settle(id, day, system) {
                    outcomes.push(outcome);
                }
            } else if let Ok(outcome) = self.expire(id, day) {
                outcomes.push(outcome);
            }
        }
        outcomes
    }

    /// F301's overdue passenger page (0x9c36) / F303's 0x9c29, and the
    /// reputation loss. The contract stays in the list as `Expired`.
    pub fn expire(&mut self, id: u32, day: u32) -> Result<Outcome, String> {
        let index = self
            .missions
            .iter()
            .position(|mission| mission.id == id)
            .ok_or_else(|| "No such mission.".to_owned())?;
        if self.missions[index].status != Status::Active {
            return Err("That mission is no longer active.".to_owned());
        }
        let mission = &mut self.missions[index];
        mission.status = Status::Expired;
        mission.settled_day = Some(day);
        let code = if mission.kind == Kind::Passenger {
            0x9c36
        } else {
            0x9c29
        };
        let text = page_text(
            code,
            &[
                Var::Int(i64::from(mission.reward)),
                Var::Int(i64::from(mission.passengers)),
                Var::System(mission.destination),
                Var::Seed(mission.seed),
                Var::Int(i64::from(mission.basic_data)),
            ],
            mission.origin,
            &mission.origin_name,
            mission.destination,
            &mission.destination_name,
        );
        self.reputation = self.reputation.saturating_sub(1);
        self.record(day, EntryKind::Expired, text.clone());
        self.push_page(PageKind::Message, day, text.clone());
        Ok(Outcome {
            id,
            status: Status::Expired,
            reward: 0,
            page: self.pages.last().cloned().unwrap(),
            journal: self.journal.last().cloned().unwrap(),
        })
    }

    /// A deliberate abandonment. F485/0x9c37's text is the source page; the
    /// reputation loss is the port's single-step equivalent of its `-8`.
    pub fn abandon(&mut self, id: u32, day: u32) -> Result<Outcome, String> {
        let index = self
            .missions
            .iter()
            .position(|mission| mission.id == id)
            .ok_or_else(|| "No such mission.".to_owned())?;
        if self.missions[index].status != Status::Active {
            return Err("That mission is no longer active.".to_owned());
        }
        let mission = &mut self.missions[index];
        mission.status = Status::Abandoned;
        mission.settled_day = Some(day);
        let text = page_text(
            0x9c37,
            &[
                Var::Int(i64::from(mission.reward)),
                Var::Int(i64::from(mission.passengers)),
                Var::System(mission.destination),
                Var::Seed(mission.seed),
                Var::Int(i64::from(mission.basic_data)),
            ],
            mission.origin,
            &mission.origin_name,
            mission.destination,
            &mission.destination_name,
        );
        self.reputation = self.reputation.saturating_sub(1);
        self.record(day, EntryKind::Abandoned, text.clone());
        self.push_page(PageKind::Message, day, text.clone());
        Ok(Outcome {
            id,
            status: Status::Abandoned,
            reward: 0,
            page: self.pages.last().cloned().unwrap(),
            journal: self.journal.last().cloned().unwrap(),
        })
    }

    /// The active contracts, in acceptance order.
    pub fn active(&self) -> impl Iterator<Item = &Mission> {
        self.missions
            .iter()
            .filter(|mission| mission.status == Status::Active)
    }

    /// Whether an offer is already an active contract. The military list is
    /// regenerated from the day's stream, so this is how the screen removes an
    /// accepted row (F509's `DATA_009004` slot).
    pub fn is_offer_active(&self, offer: &Offer) -> bool {
        self.active().any(|mission| same_offer(mission, offer))
    }

    /// Whether the offer was taken today, active or settled. The regenerated
    /// military list must not offer the same contract twice in one day; the
    /// next day's stream may offer it again.
    pub fn is_offer_known(&self, offer: &Offer, day: u32) -> bool {
        self.missions
            .iter()
            .any(|mission| mission.accepted_day == day && same_offer(mission, offer))
    }

    /// Drop settled contracts beyond the journal's record (the original
    /// decrements the count on completion; keeping them is the port's journal).
    pub fn sweep(&mut self) {
        if self.missions.len() > MAX_MISSIONS.saturating_mul(2) {
            let keep = self
                .missions
                .iter()
                .rev()
                .take(MAX_MISSIONS)
                .cloned()
                .collect::<Vec<_>>();
            self.missions = keep.into_iter().rev().collect();
        }
        if self.journal.len() > MAX_JOURNAL {
            let drop = self.journal.len() - MAX_JOURNAL;
            self.journal.drain(..drop);
        }
        if self.pages.len() > MAX_PAGES {
            let drop = self.pages.len() - MAX_PAGES;
            self.pages.drain(..drop);
        }
    }

    fn record(&mut self, day: u32, kind: EntryKind, text: String) {
        self.journal.push(Entry { day, kind, text });
        self.sweep();
    }

    fn push_page(&mut self, kind: PageKind, day: u32, text: String) {
        self.pages.push(Page {
            kind,
            day,
            text,
            unread: true,
        });
        self.sweep();
    }

    /// The validation `Flight::validate` runs. Bounds only, no invented values.
    pub fn validate(&self) -> bool {
        self.missions.len() <= MAX_MISSIONS * 2
            && self.journal.len() <= MAX_JOURNAL
            && self.pages.len() <= MAX_PAGES
            && self.reputation <= MAX_REPUTATION
            && self.rank <= 8
            && self.missions.iter().all(|mission| {
                mission.reward <= 100_000_000
                    && mission.passengers <= 7
                    && mission.accepted_day <= mission.deadline_day
                    && mission.cargo.len() <= 256
                    && mission.origin_name.len() <= 64
                    && mission.destination_name.len() <= 64
            })
            && self.journal.iter().all(|entry| entry.text.len() <= 1024)
            && self.pages.iter().all(|page| page.text.len() <= 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world<'a>(destinations: &'a [Destination]) -> World<'a> {
        World {
            code: 1,
            name: "Ross 154",
            population: 9,
            danger: 0,
            allegiance: 2,
            station_name: "Sirocco Station",
            destinations,
            names: None,
        }
    }

    fn post(code: u16, money: i64, destination: u32, basic: i64) -> Post {
        Post {
            code,
            fake: false,
            day: 0,
            vars: [
                Var::Int(money),
                Var::Text("passage for two people on a fast ship".to_owned()),
                Var::System(destination),
                Var::Seed(42),
                Var::Int(basic),
            ],
            text: "advert".to_owned(),
        }
    }

    #[test]
    fn a_passenger_post_reads_its_f638_fields() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 1,
        }];
        let world = world(&destinations);
        // basic_data: two passengers (`1` in the low bits), additive 3, no
        // package bit.
        let offer = offer(&post(0x981a, 1234, 0x1234_5678, (3 << 9) | 1), &world).unwrap();
        assert_eq!(offer.kind, Kind::Passenger);
        assert_eq!(offer.destination_name, "Sol");
        assert_eq!(offer.reward, 1234);
        assert_eq!(offer.passengers, 2);
    }

    #[test]
    fn the_package_bit_makes_a_parcel_with_no_cabins() {
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981b, 700, 7, 0x2000 | 7), &world(&destinations)).unwrap();
        assert_eq!(offer.kind, Kind::Parcel);
        assert_eq!(offer.passengers, 0);
    }

    #[test]
    fn acceptance_records_the_contract_and_the_992a_page() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 1,
        }];
        let offer = offer(&post(0x981a, 1234, 0x1234_5678, 1), &world(&destinations)).unwrap();
        let accepted = state.accept(&offer, 10, 1, "Ross 154").unwrap();
        assert_eq!(accepted.id, 1);
        assert_eq!(state.missions.len(), 1);
        let mission = &state.missions[0];
        assert_eq!(mission.accepted_day, 10);
        assert_eq!(mission.deadline_day, 10 + ACCEPT_DAYS);
        assert_eq!(mission.status, Status::Active);
        assert!(accepted.text.contains("$1234"));
        assert!(accepted.text.contains("Sol"));
        assert_eq!(state.journal.len(), 1);
        assert_eq!(state.pages.len(), 1);
        assert_eq!(state.pages[0].kind, PageKind::Message);
    }

    #[test]
    fn completion_pays_the_reward_and_raises_reputation() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981a, 900, 7, 1), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let outcome = state.settle(1, 20, 7).unwrap();
        assert_eq!(outcome.status, Status::Complete);
        assert_eq!(outcome.reward, 900);
        assert_eq!(state.reputation, 1);
        assert_eq!(state.missions[0].status, Status::Complete);
        assert!(outcome.journal.text.contains("paid in full"));
    }

    #[test]
    fn a_late_delivery_pays_nothing_and_is_0x9c2c() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981a, 900, 7, 1), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let outcome = state.settle(1, ACCEPT_DAYS + 1, 7).unwrap();
        assert_eq!(outcome.status, Status::Expired);
        assert_eq!(outcome.reward, 0);
        assert!(outcome.journal.text.contains("too late"));
    }

    #[test]
    fn expiry_writes_f301s_page_and_lowers_reputation() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981a, 900, 7, 1), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        state.reputation = 3;
        let outcomes = state.tick(ACCEPT_DAYS + 1, 1);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].status, Status::Expired);
        assert_eq!(state.reputation, 2);
        assert!(outcomes[0].journal.text.contains("waited a month"));
    }

    #[test]
    fn an_arrival_completes_the_contract_on_tick() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981a, 900, 7, 1), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let outcomes = state.tick(2, 7);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(state.missions[0].status, Status::Complete);
        assert_eq!(state.reputation, 1);
    }

    #[test]
    fn a_parcel_completes_with_the_parcel_page() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981b, 700, 7, 0x2000 | 7), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let outcome = state.settle(1, 1, 7).unwrap();
        assert!(outcome.journal.text.contains("collected the parcel"));
        assert_eq!(state.missions[0].passengers, 0);
    }

    #[test]
    fn an_assassination_is_settled_like_a_parcel() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let mut post = post(0x981f, 5000, 7, 0);
        post.vars[4] = Var::Text("Mafia boss".to_owned());
        let offer = offer(&post, &world(&destinations)).unwrap();
        assert_eq!(offer.kind, Kind::Assassination);
        assert_eq!(offer.cargo, "Mafia boss", "F643's title is StrVars[4]");
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let outcome = state.settle(1, 1, 7).unwrap();
        assert_eq!(outcome.status, Status::Complete);
        assert_eq!(outcome.reward, 5000);
    }

    #[test]
    fn military_completion_adds_f306s_rank_page_and_song() {
        let mut state = State::default();
        let offer = Offer {
            kind: Kind::MilitaryDelivery,
            code: 0x9854,
            destination: 7,
            destination_name: "Hope".to_owned(),
            reward: 2000,
            passengers: 0,
            cargo: String::new(),
            seed: 0,
            basic_data: 0x2000,
            deadline_days: ACCEPT_DAYS,
            text: "Deliver a message to the Hope system.".to_owned(),
        };
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        assert!(state.is_offer_active(&offer));
        let outcome = state.settle(1, 1, 7).unwrap();
        assert_eq!(outcome.reward, 2000);
        assert_eq!(state.rank, rank_for(500));
        assert!(!state.is_offer_active(&offer));
        assert!(state.is_offer_known(&offer, 0));
        assert!(!state.is_offer_known(&offer, 1));
        let rank = state
            .pages
            .iter()
            .find(|page| page.kind == PageKind::Rank)
            .unwrap();
        assert!(rank.text.contains("promotion"));
        assert!(rank.text.contains(RANK_NAMES[usize::from(state.rank)]));
        assert_eq!(PageKind::Rank.song(), Some(7));
        // F307's medal page/song 10 is implemented but a package delivery has
        // no medal (`DATA_005115[0]` is 0xff); the path is kept for the
        // medal-bearing constructors.
        assert_eq!(PageKind::Medal.song(), Some(10));
        assert!(!state.pages.iter().any(|page| page.kind == PageKind::Medal));
    }

    #[test]
    fn military_package_offers_use_f632s_reward_and_deadline() {
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let world = world(&destinations);
        let mut rng = crate::npc::Random::new(0x1234, 0x5678);
        let offers = military_offers(&mut rng, &world, 100, 0);
        assert_eq!(offers.len(), 1, "only the first tier is open at rank 0");
        let offer = &offers[0];
        assert_eq!(offer.kind, Kind::MilitaryDelivery);
        assert!(offer.reward >= 150);
        assert!(offer.deadline_days >= 14 && offer.deadline_days <= 77);
        assert!(offer.text.contains("Deliver"));
        assert!(offer.text.contains("Hope"));
        // F630/F631's rank gates.
        assert_eq!(military_offers(&mut rng, &world, 0, 0x10).len(), 1);
        assert_eq!(military_offers(&mut rng, &world, 0, 0x11).len(), 2);
        assert_eq!(military_offers(&mut rng, &world, 0, 0x52).len(), 3);
    }

    #[test]
    fn the_state_round_trips_through_serde_and_old_json_defaults() {
        let mut state = State::default();
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let offer = offer(&post(0x981a, 900, 7, 1), &world(&destinations)).unwrap();
        state.accept(&offer, 0, 1, "Ross 154").unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
        // A save from before this slice has no `missions` field at all.
        let old: State = serde_json::from_str("{}").unwrap();
        assert!(old.missions.is_empty());
        assert!(old.journal.is_empty());
        assert_eq!(old.reputation, 0);
    }

    #[test]
    fn every_message_page_names_its_source_routine() {
        for kind in [PageKind::Message, PageKind::Rank, PageKind::Medal] {
            assert!(kind.citation().contains('F'));
        }
    }
}
