//! The bulletin board's generators as the original draws them (F621, F636-F646,
//! F857), call for call, so the shared F922/F1530 pair advances exactly as the
//! executed original's does.
//!
//! The port's earlier generator (`bulletin::generate_one`) made the same kinds of
//! advertisement from independent draws. The executed original (an F922 trace,
//! `FFE_RNG_TRACE`, on five start keys) draws differently: F621 takes
//! `RND(32)` for a slot of `DATA_006170`; most handlers then take their numbers
//! from F1530 (which advances the same pair without a logged draw) and from the
//! destination picker F857 (`RND(12)` twice, then `RND(stars)`). F646 stores
//! at most 18 entries, but every attempt still draws: a place makes
//! `population * 4 + 1` attempts whatever is on its board.
//!
//! An entry is F646's six dwords: the message code and arguments 2..6. The
//! module keeps them raw; `bulletin::post_from_raw` turns one into the
//! advertisement the Bulletin Board screen shows.

use crate::galaxy;
use crate::market::{Entry, GOODS};
use crate::npc::Random;
use crate::sector;

/// F646's cap: `cmp dword [place+0x2e2],0x12`.
pub const MAX_ENTRIES: usize = 18;

/// One F646 entry: the message code and arguments 2..6 (`+0` .. `+0x14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raw {
    pub code: u32,
    pub args: [u32; 5],
}

/// A starport's board: the entries (`+0x2e2` count, `+0x132` array) and its flag
/// byte `+0x131` (bit 0 F639's goods notice, bit 1 F640's fake, bit 2 and 3 the
/// Federal and Imperial recruitment notices, bit 4 the non-police-world mark).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    pub entries: Vec<Raw>,
    pub flags: u8,
    /// The ships F634 queues for the starport (`+0x2e6`, `+0x31e` count, at most
    /// 13): the draw's word and F635's model.
    pub ships: Vec<(u16, u32)>,
}

/// F634's ship list cap: `cmp dword [place+0x31e],0xd`.
pub const MAX_SHIPS: usize = 13;

impl Board {
    /// F646.
    fn store(&mut self, code: u32, args: [u32; 5]) {
        if self.entries.len() < MAX_ENTRIES {
            self.entries.push(Raw { code, args });
        }
    }
}

/// `DATA_009006..DATA_009009`: the destinations F638 remembers for F636's
/// "missing person" notices. Index 0 is `DATA_009006`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Remembered(pub [u32; 4]);

/// What the generators read: the system report F622 copies into
/// `DATA_008991..DATA_008996`, the stock string `DATA_008989`, the day
/// `DATA_008807` and the station's own market and object seed.
pub struct Context<'a> {
    /// `DATA_008885`: the current system's code.
    pub code: u32,
    /// `DATA_008991`.
    pub population: u32,
    /// `DATA_008992` (a signed word).
    pub danger: i32,
    /// `DATA_008996`: the allegiance word `>> 6`.
    pub allegiance: u32,
    /// `DATA_008989`: [`galaxy::Market::stock`], 33 bytes are read.
    pub stock: &'a [u8; 37],
    /// The station's market as F619/F620 just left it (`place+8k`, `+4`).
    pub market: &'a [Entry; GOODS],
    /// `DATA_008807`: the absolute game day.
    pub day: u32,
    /// `[station object + 0xa0]`, the word F639 rotates for its company name.
    pub object_seed: u32,
    /// `DATA_008997`: the number of places in the system.
    pub places: u32,
    /// `DATA_008993`: the system record's `+6` byte.
    pub patrol: u32,
    /// `DATA_008994`: the system record's `+8` byte.
    pub trade: u32,
    /// System codes that are records with population 0 in the reference's
    /// galaxy table but not in the player's (JJFFE 2.8a7 has ten more systems
    /// than the floppy's 329). Empty in the game; the oracle comparisons pass
    /// [`REFERENCE_EMPTY`].
    pub empty: &'a [u32],
    pub galaxy: &'a galaxy::Tables<'a>,
    pub sectors: &'a sector::Tables<'a>,
}

/// `DATA_006816`/`DATA_006817`: the one-sector offsets, `(x, y)` per entry.
const RANGE_1: [[i32; 2]; 12] = [
    [0, 1],
    [1, 0],
    [0, -1],
    [-1, 0],
    [0, 1],
    [1, 0],
    [0, -1],
    [-1, 0],
    [1, 1],
    [1, -1],
    [-1, -1],
    [-1, 1],
];
/// `DATA_006818`/`DATA_006819`: the two-sector offsets.
const RANGE_2: [[i32; 2]; 12] = [
    [0, 2],
    [2, 0],
    [0, -2],
    [-2, 0],
    [1, 2],
    [2, 1],
    [1, -2],
    [-2, 1],
    [-1, 2],
    [2, -1],
    [-1, -2],
    [-2, -1],
];

/// `DATA_006171`/`DATA_006172`: the passage descriptions and multipliers.
const PASSAGE: [(u32, u32); 8] = [
    (0x9918, 2),
    (0x9919, 3),
    (0x991a, 4),
    (0x991a, 5),
    (0x991b, 6),
    (0x991b, 7),
    (0x991b, 8),
    (0x991c, 1),
];

/// `DATA_006173`: extra money by the squared draw's index.
const EXTRA_MONEY: [u32; 16] = [0, 0, 0, 0, 0, 3, 10, 12, 15, 20, 30, 40, 50, 75, 100, 200];

/// `DATA_006174`: the local-danger multiplier.
const DANGER_MULTIPLIER: [u32; 17] = [
    25, 40, 45, 50, 50, 55, 60, 75, 100, 125, 150, 200, 250, 300, 350, 400, 500,
];

/// `DATA_006214` and `DATA_006215`: F643's payment base and title offset.
const REMOVAL: [(u32, u32); 16] = [
    (3000, 0),
    (3500, 0),
    (4000, 0),
    (4500, 0),
    (5000, 0),
    (6000, 1),
    (6500, 1),
    (7000, 1),
    (8000, 2),
    (8500, 2),
    (9000, 2),
    (10000, 3),
    (13000, 4),
    (15000, 5),
    (17500, 6),
    (20000, 7),
];

#[derive(Clone, Copy)]
enum Handler {
    Goods,
    FakeGoods,
    Military,
    History,
    Missing,
    Passage,
    Commodity,
    Charity,
    Removal,
    Crew,
}

/// `DATA_006170`.
const SLOTS: [Handler; 32] = [
    Handler::Goods,
    Handler::Goods,
    Handler::Goods,
    Handler::Goods,
    Handler::Goods,
    Handler::FakeGoods,
    Handler::Military,
    Handler::Military,
    Handler::Military,
    Handler::Military,
    Handler::History,
    Handler::Missing,
    Handler::Missing,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Passage,
    Handler::Commodity,
    Handler::Commodity,
    Handler::Commodity,
    Handler::Charity,
    Handler::Removal,
    Handler::Removal,
    Handler::Crew,
    Handler::Crew,
];

/// F857: a system code near `code`. Mode 0 keeps the sector, 1 and 2 move one
/// or two sectors by independent `RND(12)` draws, then a star of that sector is
/// picked with `RND(count)`. Zero when the sector is empty, the mode is not
/// 0..2 or the pick is `code` itself.
pub fn neighbour(sectors: &sector::Tables<'_>, rng: &mut Random, code: u32, mode: u32) -> u32 {
    let mut value = code;
    match mode {
        0 => value &= 0x03ff_ffff,
        1 | 2 => {
            let table = if mode == 1 { &RANGE_1 } else { &RANGE_2 };
            let dx = table[rng.bounded(12) as usize][0];
            let low = (dx.wrapping_add((value & 0xffff) as i32) as u32 & 0x1fff) | (value & 0xe000);
            value = (value & 0xffff_0000) | low;
            let dy = table[rng.bounded(12) as usize][1];
            let y = (dy.wrapping_add((value >> 13) as i32) as u32) & 0x1fff;
            value = (y << 13) | (value & 0xfc00_1fff);
        }
        _ => return 0,
    }
    let x = value & 0x1fff;
    let y = (value >> 13) & 0x1fff;
    let stars = sectors.star_count(x, y, 0) >> 10;
    if stars == 0 {
        return 0;
    }
    let pick = rng.bounded(stars as u32);
    let byte3 = (((pick & 0x3f) << 2) | ((value >> 24) & 3)) & 0xff;
    value = (value & 0x00ff_ffff) | (byte3 << 24);
    if value == code {
        0
    } else {
        value
    }
}

/// JJFFE 2.8a7's ten galaxy records beyond the floppy's 329 (all population 0).
pub const REFERENCE_EMPTY: [u32; 10] = [
    0x16a5_3717,
    0x16a5_5717,
    0x02a5_5717,
    0x02a5_3716,
    0x26a5_3715,
    0x2aa5_3715,
    0x0aa5_3715,
    0x0ea5_3715,
    0x16a5_3715,
    0x1aa5_3715,
];

fn population_of(ctx: &Context<'_>, code: u32) -> u32 {
    if ctx.empty.contains(&code) {
        return 0;
    }
    ctx.galaxy.market(code).population
}

/// F639: "goods bought and sold", once per board.
fn goods(board: &mut Board, ctx: &Context<'_>) -> bool {
    if board.flags & 1 != 0 {
        return false;
    }
    board.flags |= 1;
    if !ctx.stock[..=32].iter().any(|byte| byte & 0x80 != 0) {
        return false;
    }
    let word = ctx.object_seed.rotate_left(17);
    board.store(0x9829, [0, 0, 0, word, word]);
    true
}

/// F640: the police's fake smuggler notice, once per board.
fn fake_goods(board: &mut Board, ctx: &Context<'_>, rng: &mut Random) -> bool {
    if board.flags & 2 != 0 {
        return false;
    }
    rng.advance();
    if !ctx.stock[..=32].iter().any(|byte| byte & 0x80 != 0) {
        return false;
    }
    board.flags |= 2;
    board.store(0x9829, [u32::MAX, 0, 0, rng.a, rng.a]);
    true
}

/// F642: the recruitment notice of the system's allegiance, once each.
fn military(board: &mut Board, ctx: &Context<'_>) -> bool {
    match ctx.allegiance {
        1 if board.flags & 8 == 0 => {
            board.flags |= 8;
            board.store(0x9828, [0; 5]);
            true
        }
        2 if board.flags & 4 == 0 => {
            board.flags |= 4;
            board.store(0x9827, [0; 5]);
            true
        }
        _ => false,
    }
}

/// F636: a remembered destination, newest first.
fn history(
    board: &mut Board,
    ctx: &Context<'_>,
    remembered: &mut Remembered,
    rng: &mut Random,
) -> bool {
    let mut mode = 2;
    let mut found = 0;
    for slot in (1..4).rev() {
        found = std::mem::take(&mut remembered.0[slot]);
        if found != 0 {
            break;
        }
    }
    if found == 0 {
        found = std::mem::take(&mut remembered.0[0]);
        if found == 0 {
            return false;
        }
        mode = 1;
    }
    wanted(board, ctx, rng, found, mode)
}

/// F637: a missing-person notice from a fresh F1530 word.
fn missing(board: &mut Board, ctx: &Context<'_>, rng: &mut Random) -> bool {
    rng.advance();
    let word = rng.b;
    wanted(board, ctx, rng, word, 0)
}

/// F636's and F637's common tail: a nearby system, the reward from the word's
/// low bits and the sign of the word choosing the wording.
fn wanted(board: &mut Board, ctx: &Context<'_>, rng: &mut Random, word: u32, a4: u32) -> bool {
    let system = neighbour(ctx.sectors, rng, ctx.code, 1);
    if system == 0 {
        return false;
    }
    let code = if word & 0x8000_0000 != 0 {
        0x981e
    } else {
        0x981d
    };
    let reward = (word & 3) * 40 + 0x50;
    board.store(code, [1, system, a4, reward, word]);
    true
}

/// F638: a passenger or parcel job to a system of population three or more.
fn passage(
    board: &mut Board,
    ctx: &Context<'_>,
    remembered: &mut Remembered,
    rng: &mut Random,
) -> bool {
    for _ in 0..4 {
        let draw = rng.bounded(3);
        let system = neighbour(ctx.sectors, rng, ctx.code, draw);
        if system == 0 || system == ctx.code {
            continue;
        }
        if population_of(ctx, system) < 3 {
            continue;
        }
        rng.advance();
        let (a, b) = (rng.a, rng.b);
        let mut kind = a & 7;
        let (description, multiplier) = PASSAGE[kind as usize];
        let mut money = (draw + 3).wrapping_mul(multiplier);
        if kind == 7 {
            kind |= 0x2000;
        }
        let squared = a.wrapping_mul(a);
        let high = squared >> 16;
        let squared = high.wrapping_mul(high);
        let bits = ((squared as i32) >> 19) as u32 & 0x1e00;
        kind = kind.wrapping_add(bits);
        let index = (bits >> 9) & 0xf;
        money = money.wrapping_add(EXTRA_MONEY[index as usize]);
        if index >= 0xf && kind & 0x2000 == 0 {
            remembered.0[0] = b;
        }
        let danger = (b & 7) as i32 + ctx.danger;
        let multiplier = DANGER_MULTIPLIER[(danger.max(0) as usize).min(16)];
        money = multiplier.wrapping_mul(money);
        let code = if b & 0x1000 != 0 { 0x981b } else { 0x981a };
        board.store(code, [description, system, kind, money, b]);
        return true;
    }
    false
}

/// F641: a wanted-commodity notice for a good the station has none of.
fn commodity(board: &mut Board, ctx: &Context<'_>, rng: &mut Random) -> bool {
    let mut skip = rng.bounded(12) as i32;
    for good in (0..GOODS).rev() {
        let entry = ctx.market[good];
        if entry.stock != 0 {
            continue;
        }
        skip -= 1;
        if skip >= 0 {
            continue;
        }
        let doubled = entry.price.wrapping_mul(2);
        if (entry.price ^ doubled) < 0 {
            continue;
        }
        board.store(0x982a, [0x8e00 + good as u32, doubled as u32, 0, 0, rng.a]);
        return true;
    }
    false
}

/// F643: a removal job, to a system of population three or more.
fn removal(board: &mut Board, ctx: &Context<'_>, rng: &mut Random) -> bool {
    for _ in 0..4 {
        let draw = rng.bounded(3);
        let system = neighbour(ctx.sectors, rng, ctx.code, draw);
        if system == 0 {
            continue;
        }
        if population_of(ctx, system) < 3 {
            continue;
        }
        rng.advance();
        let first = rng.a;
        let row = (rng.b & 0xf) as usize;
        rng.advance();
        let date = ctx
            .day
            .wrapping_sub(0x0012_1cf7)
            .wrapping_add(rng.b & 0x3f)
            .wrapping_add(7);
        let (payment, title) = REMOVAL[row];
        let money = draw * 1000 + payment;
        let code = 0x981f + ((rng.a >> 16) & 3);
        board.store(code, [date, system, 0x99b4 + title, money, first]);
        return true;
    }
    false
}

/// F644: a crew notice.
fn crew(board: &mut Board, rng: &mut Random) -> bool {
    rng.advance();
    let word = rng.b;
    board.store(0x9823 + ((word >> 16) & 3), [0, 0, 0, 0, word]);
    true
}

/// F645: a charity notice. The original's code test reads the string id after
/// it has been shifted, so the code is always `0x982c`.
fn charity(board: &mut Board, rng: &mut Random) -> bool {
    rng.advance();
    let (a, b) = (rng.a, rng.b);
    board.store(0x982c, [0x99d7 + (a & 7), 0, 0, b, a]);
    true
}

/// F621: one attempt, `RND(32)` choosing the generator.
pub fn attempt(
    board: &mut Board,
    ctx: &Context<'_>,
    remembered: &mut Remembered,
    rng: &mut Random,
) {
    match SLOTS[rng.bounded(32) as usize] {
        Handler::Goods => goods(board, ctx),
        Handler::FakeGoods => fake_goods(board, ctx, rng),
        Handler::Military => military(board, ctx),
        Handler::History => history(board, ctx, remembered, rng),
        Handler::Missing => missing(board, ctx, rng),
        Handler::Passage => passage(board, ctx, remembered, rng),
        Handler::Commodity => commodity(board, ctx, rng),
        Handler::Charity => charity(board, rng),
        Handler::Removal => removal(board, ctx, rng),
        Handler::Crew => crew(board, rng),
    };
}

/// F622's advertisement loop for one place: `population * 4 + 1` attempts.
pub fn generate(ctx: &Context<'_>, remembered: &mut Remembered, rng: &mut Random) -> Board {
    let mut board = Board::default();
    for _ in 0..=ctx.population.saturating_mul(4) {
        attempt(&mut board, ctx, remembered, rng);
    }
    board
}

/// F635: the model for F634's `mode` and the draw's word. Rows 0 and 1 are
/// shifted by the allegiance; a small population scales the trader count down.
fn ship_pick(mode: usize, ctx: &Context<'_>, rng: &mut Random) -> (u16, u32) {
    let row = if mode <= 1 {
        mode + 2 * ctx.allegiance as usize
    } else {
        mode
    };
    let list = crate::npc::SHIP_TYPES.get(row).copied().unwrap_or(&[]);
    let mut count = list.len() as u32;
    if mode == 1 && ctx.population < 4 {
        count = (ctx.population * count) >> 2;
    }
    let pick = rng.bounded(count) as usize;
    (rng.b as u16, list.get(pick).copied().unwrap_or(0))
}

/// F634: queue one ship for the starport, three times in eight.
fn queue_ship(board: &mut Board, mode: usize, ctx: &Context<'_>, rng: &mut Random) {
    rng.advance();
    if rng.b & 8 == 0 && board.ships.len() < MAX_SHIPS {
        let picked = ship_pick(mode, ctx, rng);
        board.ships.push(picked);
    }
}

/// F622's loop body for one place: the advertisements, then the ship queue
/// (`(places - 1) / 4 + 1` of row 13, `3 * patrol + 1` of row 0 and
/// `3 * trade + 1` of row 1).
pub fn generate_place(ctx: &Context<'_>, remembered: &mut Remembered, rng: &mut Random) -> Board {
    let mut board = generate(ctx, remembered, rng);
    let rows = (ctx.places.saturating_sub(1) / 4) + 1;
    for _ in 0..rows {
        queue_ship(&mut board, 13, ctx, rng);
    }
    for _ in 0..=ctx.patrol * 3 {
        queue_ship(&mut board, 0, ctx, rng);
    }
    for _ in 0..=ctx.trade * 3 {
        queue_ship(&mut board, 1, ctx, rng);
    }
    board
}

/// One record of the job list F624 fills (`DATA_009004`, 0x18 bytes, 25 of
/// them): a news or mission item with its system, date, money and the text id
/// and word the generator drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Job {
    pub code: u32,
    pub system: u32,
    pub date: u32,
    pub money: u32,
    pub text: u32,
    pub word: u32,
}

/// F624's capacity: `cmp edx,0x19`.
pub const MAX_JOBS: usize = 25;

/// `DATA_006210`: the capital F625 picks a neighbour of, by allegiance.
const CAPITALS: [u32; 6] = [
    0x12a4_9718,
    0x12a4_9718,
    0x0aa4_1718,
    0x0aa4_1718,
    0x0aa4_1718,
    0x0aa4_1718,
];
/// `DATA_006211..DATA_006213`: F626-F628's money by the drawn index.
const RANK_MONEY: [[u32; 8]; 3] = [
    [
        0xfa0, 0x1194, 0x1388, 0x157c, 0x1770, 0x1b58, 0x1f40, 0x2710,
    ],
    [
        0x2710, 0x2af8, 0x2ee0, 0x32c8, 0x36b0, 0x3a98, 0x3e80, 0x4268,
    ],
    [
        0x3a98, 0x3e80, 0x4268, 0x4650, 0x4a38, 0x4e20, 0x55f0, 0x61a8,
    ],
];

/// F624: the first free job record takes the item.
fn post_job(jobs: &mut Vec<Job>, job: Job) {
    if jobs.len() < MAX_JOBS {
        jobs.push(job);
    }
}

/// F632: a job to a nearby system of population three or more, with money
/// from `text_base` and the two draws.
fn news_job(jobs: &mut Vec<Job>, ctx: &Context<'_>, rng: &mut Random, count: u32, text_base: u32) {
    let extra = rng.bounded(count);
    let mode = rng.bounded(3);
    let system = neighbour(ctx.sectors, rng, ctx.code, mode);
    if system == 0 || population_of(ctx, system) < 3 {
        return;
    }
    rng.advance();
    let word = rng.b;
    let date = rng.bounded(14).wrapping_add(10).wrapping_add(ctx.day);
    let text = text_base.wrapping_add(extra);
    let money = text
        .wrapping_add(mode.wrapping_mul(2))
        .wrapping_mul(50)
        .wrapping_add(0xffe2_091a);
    post_job(
        jobs,
        Job {
            code: 0x9854,
            system,
            date,
            money,
            text,
            word,
        },
    );
}

/// F625: a neighbour of the allegiance's capital and its population.
fn capital_neighbour(ctx: &Context<'_>, rng: &mut Random) -> (u32, u32) {
    let mode = rng.bounded(3);
    let capital = CAPITALS[(ctx.allegiance as usize).min(5)];
    let system = neighbour(ctx.sectors, rng, capital, mode);
    if system == 0 {
        (0, 0)
    } else {
        (system, population_of(ctx, system))
    }
}

/// F626, F627 and F628: rank-gated jobs. `tier` is 0..2, `limit` the rank
/// difference they need and `code` the item's message.
fn rank_job(
    jobs: &mut Vec<Job>,
    ctx: &Context<'_>,
    rng: &mut Random,
    rank_difference: u32,
    tier: usize,
) {
    let (limit, code, base, least) = [
        (0x100, 0x9857, 0x99bc, 3),
        (0x271, 0x9855, 0x9966, 3),
        (0x510, 0x9856, 0x9966, 1),
    ][tier];
    if rank_difference <= limit {
        return;
    }
    let (system, population) = capital_neighbour(ctx, rng);
    if population < least {
        return;
    }
    rng.advance();
    let index = (rng.a & 7) as usize;
    let word = rng.b;
    rng.advance();
    let date = (rng.a & 0x3f).wrapping_add(ctx.day).wrapping_add(14);
    post_job(
        jobs,
        Job {
            code,
            system,
            date,
            money: RANK_MONEY[tier][index],
            text: base + index as u32,
            word,
        },
    );
}

/// F623: one news round. F1530's low nibble picks F629 (eight slots of
/// sixteen), F630, F631, F626, F627 or F628.
pub fn news_round(jobs: &mut Vec<Job>, ctx: &Context<'_>, rng: &mut Random, rank_difference: u32) {
    rng.advance();
    match rng.a & 0xf {
        0..=7 => news_job(jobs, ctx, rng, 8, 0x996e),
        8 | 9 if rank_difference > 0x10 => news_job(jobs, ctx, rng, 4, 0x9976),
        10 | 11 if rank_difference > 0x51 => news_job(jobs, ctx, rng, 4, 0x997a),
        12 | 13 => rank_job(jobs, ctx, rng, rank_difference, 0),
        14 => rank_job(jobs, ctx, rng, rank_difference, 1),
        15 => rank_job(jobs, ctx, rng, rank_difference, 2),
        _ => {}
    }
}

/// F622's closing loop: `maximum control * 8 + 8` news rounds.
pub fn news(
    ctx: &Context<'_>,
    rng: &mut Random,
    maximum_control: u32,
    rank_difference: u32,
) -> Vec<Job> {
    let mut jobs = Vec::new();
    for _ in 0..maximum_control * 8 + 8 {
        news_round(&mut jobs, ctx, rng, rank_difference);
    }
    jobs
}
