//! The commander's standing, as the F2 profile page prints it (`F312`,
//! `ffe.asm:24749`).
//!
//! The strings are `DATA_005189`-`DATA_005257` in `ffedat.asm`; the page's
//! layout and its selector table are cited in `docs/notes/status.md`. Nothing
//! here invents a rank or a threshold: the eleven military thresholds are
//! `DATA_005114` (`ffedat.asm:5514`), the nine Elite-rating thresholds are
//! `DATA_005110` (`ffedat.asm:5491`) and the five legal-status bands are F311's
//! `DATA_005109` (`ffedat.asm:5488`).
//!
//! `DATA_008911` (the Elite score), `DATA_008912`/`DATA_008913` (the Federal
//! and Imperial military points), `DATA_009091` (medals and permits) and the six
//! `DATA_009095` records are the source's own words; see [`Career`] and
//! [`crate::npc::Legal`].

use crate::npc::{Career, Legal, OFFENCES};

/// The thirteen Federal ranks, `DATA_005206`-`DATA_005218`.
pub const FEDERAL_RANKS: [&str; 13] = [
    "None",
    "Private",
    "Corporal",
    "Sergeant",
    "Sergeant-Major",
    "Major",
    "Colonel",
    "Lieutenant",
    "Lt. Commander",
    "Captain",
    "Commodore",
    "Rear Admiral",
    "Admiral",
];

/// The fourteen Imperial titles, `DATA_005219`-`DATA_005231`. The source's
/// promotion loop can produce index 13 (Prince); it is capped there.
pub const IMPERIAL_TITLES: [&str; 14] = [
    "Outsider", "Serf", "Master", "Sir", "Squire", "Lord", "Baron", "Viscount", "Count", "Earl",
    "Marquis", "Duke", "Prince", "Prince",
];

/// The ten Elite ratings, `DATA_005191`-`DATA_005200`.
pub const ELITE_RATINGS: [&str; 10] = [
    "Harmless",
    "Mostly Harmless",
    "Poor",
    "Below Average",
    "Average",
    "Above Average",
    "Competent",
    "Dangerous",
    "Deadly",
    "ELITE",
];

/// The five legal-status words, `DATA_005201`-`DATA_005205`, in F311's order.
pub const LEGAL_STATUSES: [&str; 5] = ["Clean", "Offender", "Criminal", "Outlaw", "Fugitive"];

/// The six record headings, `DATA_005235`-`DATA_005240`. `DATA_008996` selects
/// one; 0 is Interpol, 1 Imperial, 2 Federal, 3 Alliance and 4 Thargoid.
pub const JURISDICTIONS: [&str; 6] = [
    "INTERPOL", "IMPERIAL", "FEDERAL", "ALLIANCE", "THARGOID", "",
];

/// `DATA_005114`: the military points at which the next rank starts.
pub const MILITARY_THRESHOLDS: [u32; 13] = [
    1, 16, 81, 256, 625, 1_296, 2_401, 4_096, 6_561, 10_000, 14_641, 20_736, 65_535,
];

/// `DATA_005110`: the Elite score thresholds, highest first. F312 walks this
/// table from index 8 down; the rating word is `eax + 1`, so a score below 1024
/// is Harmless.
pub const ELITE_THRESHOLDS: [u32; 9] = [
    1_536_000, 768_000, 256_000, 32_768, 16_384, 8_192, 4_096, 2_048, 1_024,
];

/// `DATA_005109`: F311's fine bands, highest first.
pub const LEGAL_THRESHOLDS: [u32; 4] = [100_000, 20_000, 5_500, 1];

/// `DATA_005242`-`DATA_005257`, the medal and permit names F312 lists from
/// `DATA_009091`'s bits.
pub const MEDALS: [&str; 16] = [
    "Certificate of Valour",
    "Starburst",
    "Purple Omega",
    "Vermillion Crest",
    "Blue Excelsior",
    "Frontier Medal",
    "Crimson Brassard",
    "Black Polygon",
    "Golden Spike",
    "Platinum Cross",
    "Legion of Honour",
    "Celestial Warrior",
    "Supreme Knight of the Shining Arc",
    "Ross 128 Prison Permit",
    "Guardian's Permit",
    "Commune Visitor's Permit",
];

/// F306's rank index for `points`: the number of `DATA_005114` entries not
/// above it. The original caps this at 13; the Federal word list has 13 entries
/// (0..=12), so the caller clamps it there.
pub fn military_rank_index(points: u32) -> usize {
    MILITARY_THRESHOLDS
        .iter()
        .filter(|threshold| points >= **threshold)
        .count()
}

/// The Federal rank word for `points`.
pub fn federal_rank(points: u32) -> &'static str {
    FEDERAL_RANKS[military_rank_index(points).min(FEDERAL_RANKS.len() - 1)]
}

/// The Imperial title word for `points`.
pub fn imperial_title(points: u32) -> &'static str {
    IMPERIAL_TITLES[military_rank_index(points).min(IMPERIAL_TITLES.len() - 1)]
}

/// F312's Elite-rating index for `score`: 0 (Harmless) below 1024, then one
/// step per `DATA_005110` threshold up to 9 (ELITE).
pub fn elite_rating_index(score: u32) -> usize {
    ELITE_THRESHOLDS
        .iter()
        .position(|threshold| score >= *threshold)
        .map_or(0, |index| ELITE_THRESHOLDS.len() - index)
}

/// The Elite-rating word for `score`.
pub fn elite_rating(score: u32) -> &'static str {
    ELITE_RATINGS[elite_rating_index(score)]
}

/// F311's legal-status band for a fine: 0 is Clean; 1..=5499 Offender;
/// 5500..=19,999 Criminal; 20,000..=99,999 Outlaw; 100,000 and above Fugitive.
pub fn legal_status(fine: u32) -> &'static str {
    let index = LEGAL_THRESHOLDS
        .iter()
        .position(|threshold| fine >= *threshold)
        .map_or(0, |index| 4 - index);
    LEGAL_STATUSES[index]
}

/// The heading of `DATA_008996`'s record, clamped to the table.
pub fn jurisdiction(index: usize) -> &'static str {
    JURISDICTIONS[index % JURISDICTIONS.len()]
}

/// The medal or permit at `index` (`DATA_005242` + index).
pub fn medal(index: usize) -> Option<&'static str> {
    MEDALS.get(index).copied()
}

/// The names of the set bits of `DATA_009091`, in the order F312 draws them.
pub fn medals(mask: u32) -> Vec<&'static str> {
    MEDALS
        .iter()
        .enumerate()
        .filter(|(index, _)| mask & (1 << index) != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// F306's promotion and demotion notices (`DATA_005186`/`DATA_005187`).
pub fn promotion_message(rank: &str) -> String {
    format!("Congratulations! Your have earned a promotion to {rank}.")
}

/// F306's demotion notice (`DATA_005187`).
pub fn demotion_message(rank: &str) -> String {
    format!("We are disappointed with you. You have been demoted to {rank}.")
}

/// F307's medal notice (`DATA_005241`).
pub fn medal_message(name: &str) -> String {
    format!("Please take this {name} medal in thanks for good service.")
}

/// F942's rating notice (`DATA_005861`): the original prints the Elite
/// Federation header and "Right on, Commander!".
pub const ELITE_MESSAGE: &str = "Message from Elite Federation of Pilots:\rRight on, Commander!";

/// F544's success row (`DATA_005984`).
pub fn fine_paid_message(amount: u32) -> String {
    format!(
        "The ${} has been deducted. You may be punished more severely in the future.",
        money(amount)
    )
}

/// F545's refusal row (`DATA_005985`).
pub fn fine_refused_message(amount: u32) -> String {
    format!(
        "You've until midnight to pay ${} or be put in jail and your ship confiscated.",
        money(amount)
    )
}

/// F552's police greeting (`DATA_005983`). The source inserts the officer's
/// name (`0xff 0x92 0x04`); the port has no police-person name source, so it
/// uses the rank alone and leaves the name for its owner.
pub fn police_greeting(system: &str, fine: u32) -> String {
    format!(
        "I'm Sergeant of the {system} Police. We don't tolerate crime. Pay ${} before midnight to go free.",
        money(fine)
    )
}

/// The original's money format for a tenth-credit word (`$600.0`).
fn money(tenths: u32) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Everything the F2 profile page (`F312`) prints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub commander: String,
    pub system: String,
    pub jurisdiction: usize,
    pub federal_rank: &'static str,
    pub imperial_title: &'static str,
    pub elite_rating: &'static str,
    /// The three legal-status words, indexed Interpol, Imperial, Federal.
    pub federal_status: &'static str,
    pub imperial_status: &'static str,
    pub interpol_status: &'static str,
    /// The current jurisdiction's offence names, `DATA_009093`.
    pub record: Vec<&'static str>,
    /// The set medals and permits, `DATA_009091` + `DATA_005242`.
    pub medals: Vec<&'static str>,
}

impl Profile {
    /// F312's values: `DATA_009097` (Federal), `DATA_009096` (Imperial) and
    /// `DATA_009095` (Interpol) are the three named slots of the six-record
    /// ledger; the heading is `DATA_005235 + DATA_008996`.
    pub fn new(
        commander: &str,
        system: &str,
        jurisdiction: usize,
        career: &Career,
        legal: &Legal,
    ) -> Self {
        let status = |index: usize| legal_status(legal.owed(Some(index)));
        let record_mask = legal.offence_bits(jurisdiction);
        Self {
            commander: commander.to_owned(),
            system: system.to_owned(),
            jurisdiction,
            federal_rank: federal_rank(career.federal),
            imperial_title: imperial_title(career.imperial),
            elite_rating: elite_rating(career.elite),
            federal_status: status(2),
            imperial_status: status(1),
            interpol_status: status(0),
            record: OFFENCES
                .iter()
                .enumerate()
                .filter(|(index, _)| record_mask & (1 << index) != 0)
                .map(|(_, offence)| offence.name)
                .collect(),
            medals: medals(career.medals),
        }
    }

    /// The current record heading (`DATA_005235 + DATA_008996`).
    pub fn heading(&self) -> &'static str {
        jurisdiction(self.jurisdiction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rank_words_are_data_005206_to_005231() {
        assert_eq!(federal_rank(0), "None");
        assert_eq!(federal_rank(1), "Private");
        assert_eq!(federal_rank(16), "Corporal");
        assert_eq!(federal_rank(20_736), "Admiral");
        assert_eq!(federal_rank(u32::MAX), "Admiral");
        assert_eq!(imperial_title(0), "Outsider");
        assert_eq!(imperial_title(1), "Serf");
        assert_eq!(imperial_title(65_535), "Prince");
    }

    #[test]
    fn the_elite_words_follow_data_005110() {
        assert_eq!(elite_rating(0), "Harmless");
        assert_eq!(elite_rating(1023), "Harmless");
        assert_eq!(elite_rating(1024), "Mostly Harmless");
        assert_eq!(elite_rating(2047), "Mostly Harmless");
        assert_eq!(elite_rating(2048), "Poor");
        assert_eq!(elite_rating(32_768), "Competent");
        assert_eq!(elite_rating(1_536_000), "ELITE");
    }

    #[test]
    fn the_legal_words_follow_f311s_bands() {
        assert_eq!(legal_status(0), "Clean");
        assert_eq!(legal_status(1), "Offender");
        assert_eq!(legal_status(5_499), "Offender");
        assert_eq!(legal_status(5_500), "Criminal");
        assert_eq!(legal_status(19_999), "Criminal");
        assert_eq!(legal_status(20_000), "Outlaw");
        assert_eq!(legal_status(99_999), "Outlaw");
        assert_eq!(legal_status(100_000), "Fugitive");
    }

    #[test]
    fn the_jurisdictions_are_data_005235() {
        assert_eq!(jurisdiction(0), "INTERPOL");
        assert_eq!(jurisdiction(1), "IMPERIAL");
        assert_eq!(jurisdiction(2), "FEDERAL");
        assert_eq!(jurisdiction(3), "ALLIANCE");
        assert_eq!(jurisdiction(4), "THARGOID");
    }

    #[test]
    fn medals_are_listed_in_bit_order() {
        assert_eq!(medals(0b101), vec!["Certificate of Valour", "Purple Omega"]);
        assert_eq!(medal(15), Some("Commune Visitor's Permit"));
        assert_eq!(medal(16), None);
    }
}
