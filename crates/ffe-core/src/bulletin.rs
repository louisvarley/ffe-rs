//! The Bulletin Board's generated advertisements (F622/F633, F636-F646).
//!
//! The starport record's `+0x132` array holds 18 advertisements of 24 bytes
//! each (`+0x2e2` count). F646 writes one entry as six dwords:
//!
//! ```text
//! +0x00  message code (0x981a..0x982d)
//! +0x04  arg2 -> F453's row +0x08 -> StrVars[1]
//! +0x08  arg3 -> row +0x0c       -> StrVars[2]
//! +0x0c  arg4 -> row +0x10       -> StrVars[4]
//! +0x10  arg5 -> row +0x04       -> StrVars[0]
//! +0x14  arg6 -> row +0x24       -> StrVars[3]
//! ```
//!
//! `F622` makes `population * 4 + 1` attempts; each asks `F621` for a slot of
//! `DATA_006170` (`F922(0x20)`), and the generator either writes an entry or
//! reports none. F646 stops at 18 entries. F633's daily half replaces one entry
//! when `random(7) < population`.
//!
//! The templates are the original `0x981a..0x982d` strings with their 0xff
//! escape codes intact; [`render`] expands the codes the advertisements use
//! (`F1343`/`F1348`): `0x30xx` variables, `0x85ff` the destination system name
//! and `0x92xx` a name from `F791`/`F801`. The money, job descriptions and
//! danger multipliers come from `DATA_006171..6174`; the removal payments are
//! listed in [`REMOVAL_PAYMENTS`]. See `docs/notes/station.md`.

use crate::names::Names;
use crate::npc::Random;

#[path = "bulletin_table.rs"]
mod table;

pub use table::{DANGER_MONEY, DANGER_MULTIPLIER, GENERATORS};

/// The 18 slots of the starport's `+0x132` BBS array.
pub const MAX_POSTS: usize = 18;

/// F646's message codes: the advertisements this module generates.
pub const MESSAGES: [u16; 20] = [
    0x981a, 0x981b, 0x981c, 0x981d, 0x981e, 0x981f, 0x9820, 0x9821, 0x9822, 0x9823, 0x9824, 0x9825,
    0x9826, 0x9827, 0x9828, 0x9829, 0x982a, 0x982b, 0x982c, 0x982d,
];

/// The `DATA_006170` generator slots (`F621`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    Goods,
    GoodsRandom,
    HistoryWanted,
    MissingPerson,
    ShipJob,
    Commodity,
    Military,
    Removal,
    Crew,
    Charity,
}

/// F638's `DATA_006171`/`DATA_006172` pairs: the passage/parcel job description
/// and its multiplier.
const PASSAGE: [(u16, u32); 8] = [
    (0x9918, 2),
    (0x9919, 3),
    (0x991a, 4),
    (0x991a, 5),
    (0x991b, 6),
    (0x991b, 7),
    (0x991b, 8),
    (0x991c, 1),
];

/// `DATA_006214`'s payment values for F643's removal jobs.
pub const REMOVAL_PAYMENTS: [u32; 16] = [
    0x0b8b, 0x0acd, 0x0fa0, 0x1194, 0x1388, 0x1770, 0x1964, 0x1b58, 0x1f40, 0x2134, 0x2328, 0x2710,
    0x32c8, 0x3a98, 0x445c, 0x4e20,
];

/// One advertisement variable, in F453's StrVars slots (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Var {
    Int(i64),
    Text(String),
    /// A system code for `0x85ff`.
    System(u32),
    /// A random seed `F801` hands to the name generator (`0x92xx`).
    Seed(u32),
}

/// One generated advertisement: the original message code, its variables, the
/// game day it was placed, and the expanded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub code: u16,
    pub day: u32,
    pub vars: [Var; 5],
    pub text: String,
    /// F640's fake smuggler advertisement (`+8 == -1`): the same `0x9829`
    /// wording as F639's real one, but the police set the trap. The market it
    /// opens applies F542's trading-in-illegal-goods fine on a row action.
    pub fake: bool,
}

/// A system the F857 destination picker may choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub code: u32,
    pub name: String,
    pub population: u32,
    pub danger: u32,
}

/// The system and name data the generators and template expander read.
pub struct World<'a> {
    pub code: u32,
    pub name: &'a str,
    pub population: u32,
    pub danger: u32,
    /// `DATA_008996`: the record's allegiance word `>> 6` (1 Imperial,
    /// 2 Federal).
    pub allegiance: u16,
    pub station_name: &'a str,
    pub destinations: &'a [Destination],
    pub names: Option<&'a Names<'a>>,
}

impl World<'_> {
    /// The system name for a code: the board's own system for 0, otherwise the
    /// matching destination. F1343's `0x85ff` handler.
    pub fn system_name(&self, code: u32) -> String {
        if code == 0 {
            return self.name.to_owned();
        }
        self.destinations
            .iter()
            .find(|destination| destination.code == code)
            .map_or_else(
                || self.name.to_owned(),
                |destination| destination.name.clone(),
            )
    }

    /// F857(1, current): a neighbouring system with a population, avoiding the
    /// one the board is in. The original perturbs the grid coordinates; the
    /// port picks from the navigator's neighbourhood list.
    pub fn neighbour(&self, rng: &mut Random, minimum_population: u32) -> Option<&Destination> {
        let choices: Vec<&Destination> = self
            .destinations
            .iter()
            .filter(|destination| {
                destination.code != self.code && destination.population >= minimum_population
            })
            .collect();
        if choices.is_empty() {
            return None;
        }
        Some(choices[rng.bounded(choices.len() as u32) as usize])
    }
}

/// `DATA_005299`'s raw template for a message code.
pub fn template(code: u16) -> Option<&'static [u8]> {
    table::TEMPLATES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, bytes)| *bytes)
}

fn embedded(code: u16) -> Option<&'static [u8]> {
    table::STRINGS
        .iter()
        .chain(table::COMPLETION.iter())
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, bytes)| *bytes)
}

fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn named(world: &World<'_>, kind: u32, seed: u32) -> String {
    world
        .names
        .and_then(|names| names.generate(kind, seed))
        .map(|bytes| text(&bytes))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "???".to_owned())
}

/// F791/F801 for the `0x92xx` codes: the name kind is the low six bits and the
/// seed is StrVars[3] (the `F801` fallback branch).
fn name_code(world: &World<'_>, code: u16, vars: &[Var; 5], depth: usize) -> String {
    let kind = u32::from(code & 0x3f);
    let seed = match &vars[3] {
        Var::Seed(seed) => *seed,
        Var::Int(seed) => *seed as u32,
        Var::Text(value) => value.bytes().fold(0u32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(u32::from(byte))
        }),
        Var::System(code) => *code,
    };
    let name = named(world, kind, seed);
    // Some of the 0x99xx title strings embed further 0x92xx codes; expand
    // them against the same variables (F1348's recursive branch).
    if name.as_bytes().contains(&0xff) {
        expand(name.as_bytes(), world, vars, depth + 1)
    } else {
        name
    }
}

fn system_code(vars: &[Var; 5]) -> u32 {
    match &vars[2] {
        Var::System(code) => *code,
        Var::Int(code) => *code as u32,
        _ => 0,
    }
}

fn var_index(index: u16) -> usize {
    // FF30WriteIntV1/V2/V3, the three fractional writers' V1..V3 slots, and
    // FF30WriteFFCodeV1..V5.
    match index {
        0x01..=0x03 => usize::from(index - 1),
        0x04..=0x0c => usize::from((index - 4) % 3),
        0x19..=0x1c => usize::from(index - 0x18),
        0x1d..=0x1e => usize::from(index - 0x1a),
        _ => 0,
    }
}

fn var_text(var: &Var, world: &World<'_>) -> String {
    match var {
        Var::Int(value) => value.to_string(),
        Var::Text(value) => value.clone(),
        Var::System(code) => world.system_name(*code),
        Var::Seed(seed) => seed.to_string(),
    }
}

fn expand(bytes: &[u8], world: &World<'_>, vars: &[Var; 5], depth: usize) -> String {
    if depth > 4 {
        return text(bytes);
    }
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        // F1348 reads the 16-bit code after 0xff.
        if byte != 0xff || index + 1 >= bytes.len() {
            out.push(char::from(byte));
            continue;
        }
        let code = u16::from(bytes[index]) << 8 | u16::from(bytes[index + 1]);
        index += 2;
        match code {
            // FF30WriteDateV1..V3/DateYearV1..V3: the variable is a day count
            // (F1358's `StringConvertDate`). The military mission template
            // 0x9854 uses 0x300e for its "by midnight on ..." date.
            0x300d..=0x300f => {
                let slot = usize::from((code & 0xff) - 0x0d);
                let days = match vars.get(slot) {
                    Some(Var::Int(value)) => (*value).max(0) as u32,
                    _ => 0,
                };
                out.push_str(&date_day_text(days));
            }
            0x3001..=0x3006 | 0x3019..=0x301d => {
                let slot = var_index(code & 0x3f);
                let var = vars.get(slot).unwrap_or(&Var::Int(0));
                if matches!(code & 0xff, 0x04..=0x06) {
                    // FF30Write1dpFracV1..V3: the integer is in tenths.
                    let value = match var {
                        Var::Int(value) => *value,
                        _ => 0,
                    };
                    out.push_str(&format!("{}.{}", value / 10, value.abs() % 10));
                } else if (0x19..=0x1c).contains(&(code & 0xff)) {
                    // FF30WriteFFCodeV1..V5: the variable is an ffcode or text.
                    match var {
                        Var::Text(value) => {
                            if value.as_bytes().contains(&0xff) {
                                out.push_str(&expand(value.as_bytes(), world, vars, depth + 1));
                            } else {
                                out.push_str(value);
                            }
                        }
                        Var::Int(value) => out.push_str(&value.to_string()),
                        Var::System(system) => out.push_str(&world.system_name(*system)),
                        Var::Seed(seed) => out.push_str(&named(world, 3, *seed)),
                    }
                } else {
                    out.push_str(&var_text(var, world));
                }
            }
            // 0x85ff is the destination system name. 0x85fe is the same
            // system-name handler with a different table selector; in the
            // templates that use it (0x991d, 0x9854) the name is already
            // written by 0x85ff, so this code contributes no text.
            0x85ff => out.push_str(&world.system_name(system_code(vars))),
            0x85fe => {}
            0x9200..=0x92ff => out.push_str(&name_code(world, code, vars, depth)),
            0x9000..=0x9fff => {
                // A nested 0x99xx string from DATA_005299 (passage/charity
                // descriptions); expand its own escapes.
                match embedded(code) {
                    Some(bytes) => out.push_str(&expand(bytes, world, vars, depth + 1)),
                    None => out.push_str(&format!("[{code:#06x}]")),
                }
            }
            _ => out.push_str(&format!("[{code:#06x}]")),
        }
    }
    out
}

/// Expand a generated advertisement exactly as F1343/F1348 do for the codes it
/// uses. Unknown codes are left visible in brackets rather than invented.
pub fn render(code: u16, vars: &[Var; 5], world: &World<'_>) -> String {
    template(code).map_or_else(String::new, |bytes| expand(bytes, world, vars, 0))
}

/// Expand one raw string of `DATA_005299`/`DATA_005104`, the tables F1343's
/// `ff98`/`ff9c` handlers read. This is the entry point the mission reply and
/// completion pages use (`missions::text`); an unknown code returns `None`.
pub fn render_code(code: u16, vars: &[Var; 5], world: &World<'_>) -> Option<String> {
    embedded(code).map(|bytes| expand(bytes, world, vars, 0))
}

/// Expand a raw string table entry. The mission module keeps its source text in
/// `bulletin_table` and expands it through the same F1348 subset as the ads.
pub fn render_bytes(bytes: &[u8], vars: &[Var; 5], world: &World<'_>) -> String {
    expand(bytes, world, vars, 0)
}

/// F1358's date without the time, for FF30's date codes: day 0 is 1-Jan-3250
/// and the game's calendar uses the fixed `DATA_007650` month lengths (no leap
/// years), matching `galaxy_map::date_text`'s year loop.
pub fn date_day_text(days: u32) -> String {
    const MONTHS: [(&str, u32); 12] = [
        ("Jan", 31),
        ("Feb", 28),
        ("Mar", 31),
        ("Apr", 30),
        ("May", 31),
        ("Jun", 30),
        ("Jul", 31),
        ("Aug", 31),
        ("Sep", 30),
        ("Oct", 31),
        ("Nov", 30),
        ("Dec", 31),
    ];
    let mut rest = days;
    let mut year = 3250u32;
    while rest >= 365 {
        rest -= 365;
        year += 1;
    }
    let mut month = 0usize;
    while rest >= MONTHS[month].1 {
        rest -= MONTHS[month].1;
        month += 1;
    }
    format!("{}-{}-{year}", rest + 1, MONTHS[month].0)
}

/// Whether a code is one of the mission reply/completion pages (0x9833..0x983f,
/// 0x991d..0x9930, 0x9c29..0x9c3a, 0x9c67..0x9c70).
pub fn mission_text(code: u16) -> Option<&'static [u8]> {
    embedded(code)
}

fn post(code: u16, day: u32, vars: [Var; 5], world: &World<'_>) -> Post {
    let text = render(code, &vars, world);
    Post {
        code,
        day,
        vars,
        text,
        fake: false,
    }
}

/// F639: "goods bought and sold at <company>". The original seeds the company
/// generator with the station name pointer; the port uses a fresh draw.
fn goods(rng: &mut Random, world: &World<'_>, day: u32) -> Post {
    let seed = rng.bounded(0x1_0000);
    post(
        0x9829,
        day,
        [
            Var::Int(0),
            Var::Int(0),
            Var::System(world.code),
            Var::Seed(seed),
            Var::Int(0),
        ],
        world,
    )
}

/// F640: the police's fake smuggler advertisement. The same `0x9829` template
/// and company name, with the `+8 == -1` marker F454 turns into a trap.
fn fake_goods(rng: &mut Random, world: &World<'_>, day: u32) -> Post {
    let mut post = goods(rng, world, day);
    post.fake = true;
    post
}

/// F642: the military recruitment notice for the system's allegiance.
fn military(world: &World<'_>, day: u32) -> Option<Post> {
    let code = match world.allegiance {
        2 => 0x9827,
        1 => 0x9828,
        _ => return None,
    };
    Some(post(
        code,
        day,
        [
            Var::Int(0),
            Var::Int(0),
            Var::System(world.code),
            Var::Seed(0),
            Var::Int(0),
        ],
        world,
    ))
}

/// F638: a passenger or parcel job. The reward is `(random(3) + 3) * the
/// DATA_006171 multiplier`, plus `DATA_006173`'s extra and times the
/// `DATA_006174` danger multiplier, exactly as the asm computes it.
///
/// The fifth variable is F646's arg6, the BBS entry's `data2` (StrVars[4]):
/// the low three bits are the passenger count minus one, bits 9..12 the
/// mission additive, and bit 13 marks the "small package" passage (F534's
/// `test ah,0x20`). F638 picks the passage index and the additive from the
/// same stream; the port keeps that layout so [`crate::missions`] can accept
/// the post without inventing a second record.
fn ship_job(rng: &mut Random, world: &World<'_>, day: u32) -> Option<Post> {
    let destination = world.neighbour(rng, 3)?;
    let index = rng.bounded(8) as usize;
    let (description, multiplier) = PASSAGE[index];
    let mut money = (rng.bounded(3) + 3) * multiplier;
    let extra = rng.bounded(16) as usize;
    money = money.saturating_add(DANGER_MONEY[extra]);
    let danger_index = (rng.bounded(8) + world.danger.min(8)) as usize;
    money = money.saturating_mul(DANGER_MULTIPLIER[danger_index.min(15)]);
    let code = if rng.bounded(2) == 0 { 0x981a } else { 0x981b };
    let seed = rng.bounded(0x1_0000);
    let package = index == 7;
    // F638's `edx`: `random & 7` is the passenger count minus one (or the
    // package passage's 7), `| 0x2000` for the package, plus `(bits >> 9) &
    // 0xf` of the squared draw for the mission additive.
    let additive = rng.bounded(16);
    let basic_data = (index as u32 & 7) | (additive << 9) | if package { 0x2000 } else { 0 };
    Some(post(
        code,
        day,
        [
            Var::Int(i64::from(money)),
            Var::Text(text(embedded(description).unwrap_or(b""))),
            Var::System(destination.code),
            Var::Seed(seed),
            Var::Int(i64::from(basic_data)),
        ],
        world,
    ))
}

/// F643: a removal/biographical job. The payment is one of `DATA_006214`'s
/// values plus 1000 credits per `random(3)`.
fn removal(rng: &mut Random, world: &World<'_>, day: u32) -> Option<Post> {
    let destination = world.neighbour(rng, 3)?;
    const TITLES: [u16; 16] = [
        0x99b4, 0x99b5, 0x99b6, 0x99b7, 0x99b8, 0x99b9, 0x99ba, 0x99bb, 0x99bc, 0x99bd, 0x99be,
        0x99bf, 0x99c0, 0x99c1, 0x99c2, 0x99c3,
    ];
    let payment = REMOVAL_PAYMENTS[rng.bounded(16) as usize]
        .saturating_add(rng.bounded(3).saturating_mul(1000));
    let title = TITLES[rng.bounded(16) as usize];
    let seed = rng.bounded(0x1_0000);
    let person = rng.bounded(0x1_0000);
    let title = text(embedded(title).unwrap_or(b"???"));
    let code = match rng.bounded(4) {
        0 => 0x981f,
        1 => 0x9820,
        2 => 0x9821,
        _ => 0x9822,
    };
    Some(post(
        code,
        day,
        [
            Var::Int(i64::from(payment)),
            Var::Seed(seed),
            Var::System(destination.code),
            Var::Seed(person),
            Var::Text(title),
        ],
        world,
    ))
}

/// F644: a crew vacancy/wanted notice (0x9823..0x9826).
fn crew(rng: &mut Random, world: &World<'_>, day: u32) -> Post {
    post(
        0x9823 + rng.bounded(4) as u16,
        day,
        [
            Var::Int(0),
            Var::Int(0),
            Var::System(world.code),
            Var::Seed(0),
            Var::Int(0),
        ],
        world,
    )
}

/// F645: a charity/donation notice (0x982c/0x982d) with one of the eight
/// charity names 0x99d7..0x99de.
fn charity(rng: &mut Random, world: &World<'_>, day: u32) -> Post {
    let name = text(embedded(0x99d7 + rng.bounded(8) as u16).unwrap_or(b"a charity"));
    // The 0x99xx charity strings use `???` for the system name.
    let name = name.replace("???", world.name);
    let code = if rng.bounded(2) == 0 { 0x982c } else { 0x982d };
    post(
        code,
        day,
        [
            Var::Int(0),
            Var::Text(name),
            Var::System(world.code),
            Var::Seed(0),
            Var::Int(0),
        ],
        world,
    )
}

/// Run one `DATA_006170` slot. `None` is F621's "no entry this time".
pub fn generate_one(
    generator: Generator,
    rng: &mut Random,
    world: &World<'_>,
    day: u32,
) -> Option<Post> {
    Some(match generator {
        Generator::Goods => goods(rng, world, day),
        Generator::GoodsRandom => {
            if rng.bounded(2) == 0 {
                fake_goods(rng, world, day)
            } else {
                return None;
            }
        }
        Generator::Military => return military(world, day),
        Generator::ShipJob => return ship_job(rng, world, day),
        Generator::Removal => return removal(rng, world, day),
        Generator::Crew => crew(rng, world, day),
        Generator::Charity => charity(rng, world, day),
        // F636 needs the four remembered systems (`DATA_009006..9`), which the
        // port does not keep; F637/F641 read the market's hidden special-offer
        // table. Both are left out rather than inventing advertisements.
        Generator::HistoryWanted | Generator::MissingPerson | Generator::Commodity => return None,
    })
}

/// F622's BBS generation: `population * 4 + 1` attempts, stored up to the
/// 18-entry cap.
pub fn generate(rng: &mut Random, world: &World<'_>, day: u32) -> Vec<Post> {
    let attempts = world.population.saturating_mul(4).saturating_add(1);
    let mut posts = Vec::new();
    for _ in 0..attempts {
        if posts.len() >= MAX_POSTS {
            break;
        }
        let generator = GENERATORS[rng.bounded(32) as usize];
        if let Some(post) = generate_one(generator, rng, world, day) {
            posts.push(post);
        }
    }
    posts
}

/// F633's daily BBS half: with `random(7) < population`, replace one entry
/// with a fresh draw. The original also clears the two service flags; the
/// visible effect is the replacement.
pub fn daily(rng: &mut Random, posts: &mut [Post], world: &World<'_>, day: u32) {
    if posts.is_empty() || rng.bounded(7) >= world.population {
        return;
    }
    let index = rng.bounded(posts.len() as u32) as usize;
    let generator = GENERATORS[rng.bounded(32) as usize];
    if let Some(post) = generate_one(generator, rng, world, day) {
        posts[index] = post;
    }
}

/// The system a generated post names, for the screen tests.
pub fn post_system(post: &Post) -> Option<u32> {
    match post.vars.get(2) {
        Some(Var::System(code)) => Some(*code),
        _ => None,
    }
}

/// The `F801` seed in a generated post's StrVars[3], used by `0x92xx` names.
pub fn post_seed(post: &Post) -> Option<u32> {
    match post.vars.get(3) {
        Some(Var::Seed(seed)) => Some(*seed),
        Some(Var::Int(seed)) => Some(*seed as u32),
        _ => None,
    }
}

/// The human-readable text a screen draws for a post (already expanded).
pub fn post_text(post: &Post) -> &str {
    &post.text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world<'a>(destinations: &'a [Destination], names: Option<&'a Names<'a>>) -> World<'a> {
        World {
            code: 0x02a4_9718,
            name: "Ross 154",
            population: 9,
            danger: 0,
            allegiance: 2,
            station_name: "Sirocco Station",
            destinations,
            names,
        }
    }

    #[test]
    fn the_templates_are_the_originals_advertisements() {
        let destinations = [];
        let world = world(&destinations, None);
        assert_eq!(template(0x981a).unwrap()[0], b'W');
        assert_eq!(
            render(
                0x9827,
                &[
                    Var::Int(0),
                    Var::Int(0),
                    Var::System(0),
                    Var::Seed(0),
                    Var::Int(0)
                ],
                &world
            ),
            "FEDERAL MILITARY: We want YOU for adventures unlimited."
        );
        assert_eq!(
            render(
                0x9823,
                &[
                    Var::Int(0),
                    Var::Int(0),
                    Var::System(0),
                    Var::Seed(0),
                    Var::Int(0)
                ],
                &world
            ),
            "WANTED: Position on a reputable starship."
        );
    }

    #[test]
    fn the_commodity_template_uses_the_price_slot_and_one_decimal() {
        let destinations = [];
        let world = world(&destinations, None);
        assert_eq!(
            render(
                0x982a,
                &[
                    Var::Int(0),
                    Var::Text("Precious Metals".to_owned()),
                    Var::Int(123),
                    Var::Seed(0),
                    Var::Int(0),
                ],
                &world
            ),
            "WANTED: Precious Metals. Will pay $12.3 per tonne."
        );
    }

    #[test]
    fn ff30_date_codes_use_fixed_day_and_month_lengths() {
        assert_eq!(date_day_text(0), "1-Jan-3250");
        assert_eq!(date_day_text(30), "31-Jan-3250");
        assert_eq!(date_day_text(31), "1-Feb-3250");
        assert_eq!(date_day_text(364), "31-Dec-3250");
        assert_eq!(date_day_text(365), "1-Jan-3251");
        // 0x9854's `by midnight on` slot: 0x0e = FF30WriteDateYearV2, var 1.
        let destinations = [Destination {
            code: 7,
            name: "Hope".to_owned(),
            population: 4,
            danger: 0,
        }];
        let world = world(&destinations, None);
        let rendered = render_code(
            0x9854,
            &[
                Var::Int(350),
                Var::Int(31),
                Var::System(7),
                Var::Seed(0),
                Var::Text("a message".to_owned()),
            ],
            &world,
        )
        .unwrap();
        assert_eq!(
            rendered,
            "Deliver a message to the Hope system by midnight on 1-Feb-3250. You will be paid $350."
        );
    }

    #[test]
    fn the_generator_dispatch_table_is_the_originals_shape() {
        assert_eq!(GENERATORS.len(), 32);
        assert_eq!(GENERATORS[0], Generator::Goods);
        assert_eq!(GENERATORS[5], Generator::GoodsRandom);
        assert_eq!(GENERATORS[10], Generator::HistoryWanted);
        assert_eq!(GENERATORS[15], Generator::ShipJob);
        assert_eq!(GENERATORS[27], Generator::Charity);
        assert_eq!(GENERATORS[31], Generator::Crew);
    }

    #[test]
    fn a_ship_job_renders_the_original_template_and_money() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 1,
        }];
        let world = world(&destinations, None);
        let mut rng = Random::new(0x1234, 0x5678);
        let post = ship_job(&mut rng, &world, 0).unwrap();
        assert!(post.code == 0x981a || post.code == 0x981b);
        assert!(post.text.contains("the Sol system"));
        assert!(post.text.starts_with("WANTED:") || post.text.starts_with("SHIP REQUIRED:"));
        assert_eq!(post_system(&post), Some(0x1234_5678));
        assert!(post.vars[0] != Var::Int(0));
    }

    #[test]
    fn a_ship_job_encodes_the_passenger_count_and_package_bit() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 1,
        }];
        let world = world(&destinations, None);
        let mut rng = Random::new(0x0bad, 0xf00d);
        // Every draw is a valid F638 job; the fifth variable is the contract's
        // basic_data, not part of the rendered advertisement.
        for _ in 0..64 {
            let post = ship_job(&mut rng, &world, 0).unwrap();
            let basic = match post.vars[4] {
                Var::Int(value) => value as u32,
                _ => panic!("F638 must set StrVars[4]"),
            };
            if basic & 0x2000 != 0 {
                assert_eq!(basic & 7, 7, "the package passage's low bits");
            } else {
                assert!((1..=7).contains(&((basic & 7) + 1)));
            }
        }
    }

    #[test]
    fn the_population_attempt_count_and_eighteen_slot_cap_are_the_originals() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 0,
        }];
        let world = world(&destinations, None);
        let mut rng = Random::new(7, 11);
        let posts = generate(&mut rng, &world, 3);
        assert!(posts.len() <= MAX_POSTS);
        assert!(posts.iter().all(|post| post.day == 3));
        assert!(posts.iter().all(|post| !post.text.is_empty()));
        // Nine population gives 37 attempts, enough for the cap with the
        // always-succeeding military/goods/crew slots.
        assert!(posts.len() > 4, "{}", posts.len());
    }

    #[test]
    fn the_fake_smuggler_advertisement_keeps_the_goods_wording_and_marks_the_trap() {
        let destinations = [];
        let world = world(&destinations, None);
        let mut rng = Random::new(7, 11);
        let real = goods(&mut rng, &world, 1);
        assert_eq!(real.code, 0x9829);
        assert!(!real.fake);
        let fake = fake_goods(&mut rng, &world, 1);
        assert_eq!(fake.code, 0x9829);
        assert!(fake.fake);
        assert_eq!(fake.text, real.text, "both use the 0x9829 template");
        assert!(post_seed(&fake).is_some());
    }

    #[test]
    fn the_military_notice_follows_the_allegiance() {
        let destinations = [];
        let federal = world(&destinations, None);
        assert_eq!(military(&federal, 0).unwrap().code, 0x9827);
        let imperial = World {
            allegiance: 1,
            ..world(&destinations, None)
        };
        assert_eq!(military(&imperial, 0).unwrap().code, 0x9828);
        let independent = World {
            allegiance: 0,
            ..world(&destinations, None)
        };
        assert!(military(&independent, 0).is_none());
    }

    #[test]
    fn the_daily_update_replaces_one_post_on_the_population_roll() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 0,
        }];
        let world = world(&destinations, None);
        let mut rng = Random::new(1, 2);
        let mut posts = generate(&mut rng, &world, 0);
        let before = posts.len();
        daily(&mut rng, &mut posts, &world, 1);
        assert_eq!(posts.len(), before);
        assert!(posts.iter().all(|post| post.day == 0 || post.day == 1));
    }

    #[test]
    fn a_full_world_renders_every_template_without_placeholder_escapes() {
        let destinations = [Destination {
            code: 0x1234_5678,
            name: "Sol".to_owned(),
            population: 12,
            danger: 2,
        }];
        // The name table is optional; without it the person/company names are
        // the original "???" placeholder rather than invented text.
        let world = world(&destinations, None);
        for code in MESSAGES {
            let vars = [
                Var::Int(1234),
                Var::Text("passage on a fast ship".to_owned()),
                Var::System(0x1234_5678),
                Var::Seed(42),
                Var::Int(0),
            ];
            let rendered = render(code, &vars, &world);
            assert!(!rendered.contains('\u{ff}'), "{code:#06x}: {rendered}");
            assert!(!rendered.is_empty(), "{code:#06x}");
        }
    }
}
