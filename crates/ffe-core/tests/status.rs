//! Rank, reputation and legal-status tests: `DATA_005110`/`DATA_005114`'s
//! thresholds, the twelve military words, F307's medals, F311's five legal
//! words and F544/F545's fine payment. Every string and threshold is cited in
//! `docs/notes/status.md`.

use ffe_core::npc::{Career, Legal};
use ffe_core::status::{
    demotion_message, elite_rating, federal_rank, fine_paid_message, fine_refused_message,
    imperial_title, jurisdiction as profile_jurisdiction, legal_status, medal_message, medals,
    promotion_message, ELITE_MESSAGE, FEDERAL_RANKS, IMPERIAL_TITLES, LEGAL_STATUSES, MEDALS,
};

#[test]
fn military_points_map_to_the_source_rank_words() {
    // DATA_005114: 1, 16, 81, ... The rank word is the count of thresholds the
    // points have reached (DATA_005206 + index).
    assert_eq!(federal_rank(0), "None");
    assert_eq!(federal_rank(1), "Private");
    assert_eq!(federal_rank(16), "Corporal");
    assert_eq!(federal_rank(81), "Sergeant");
    assert_eq!(federal_rank(20_736), "Admiral");
    assert_eq!(federal_rank(u32::MAX), FEDERAL_RANKS[12]);
    assert_eq!(imperial_title(0), "Outsider");
    assert_eq!(imperial_title(1), "Serf");
    assert_eq!(imperial_title(65_535), IMPERIAL_TITLES[13]);
}

#[test]
fn elite_points_map_to_the_source_rating_words() {
    // DATA_005110 is walked highest first; below 1024 is Harmless.
    assert_eq!(elite_rating(0), "Harmless");
    assert_eq!(elite_rating(1_023), "Harmless");
    assert_eq!(elite_rating(1_024), "Mostly Harmless");
    assert_eq!(elite_rating(2_048), "Poor");
    assert_eq!(elite_rating(32_768), "Competent");
    assert_eq!(elite_rating(768_000), "Deadly");
    assert_eq!(elite_rating(1_536_000), "ELITE");
}

#[test]
fn legal_status_words_are_f311s_five_bands() {
    // DATA_005109: 100000, 20000, 5500, 1.
    assert_eq!(legal_status(0), LEGAL_STATUSES[0]);
    assert_eq!(legal_status(1), "Offender");
    assert_eq!(legal_status(5_499), "Offender");
    assert_eq!(legal_status(5_500), "Criminal");
    assert_eq!(legal_status(20_000), "Outlaw");
    assert_eq!(legal_status(100_000), "Fugitive");
}

#[test]
fn a_commander_award_adds_signed_points_and_announces_the_rank_word() {
    let mut career = Career::default();
    assert_eq!(career.federal_rank(), "None");
    assert!(career.award_federal(15).is_some(), "crosses to Private");
    assert_eq!(
        promotion_message(career.federal_rank()),
        "Congratulations! Your have earned a promotion to Private."
    );
    let change = career.award_federal(1).expect("crosses to Corporal");
    assert_eq!((change.from, change.to), (1, 2));
    let change = career.award_federal(-16).expect("back to None");
    assert_eq!((change.from, change.to), (2, 0));
    assert!(!change.promoted());
    assert_eq!(
        demotion_message(career.federal_rank()),
        "We are disappointed with you. You have been demoted to None."
    );
    // Negative points never go below zero.
    assert!(career.award_federal(-100).is_none());
    assert_eq!(career.federal, 0);
}

#[test]
fn a_kill_adds_the_statistics_rating_and_reports_a_rating_change() {
    let mut career = Career::default();
    assert_eq!(career.elite_rating(), "Harmless");
    assert!(!career.award_kill(1_023));
    assert_eq!(career.elite, 1_023);
    // The rating word changes, but the high word is still zero, so F942 does
    // not print its message.
    assert!(!career.award_kill(1));
    assert_eq!(career.elite_rating(), "Mostly Harmless");
    for _ in 0..47 {
        career.award_kill(32_767);
    }
    assert_eq!(career.elite_rating(), "ELITE");
    assert!(career.elite >> 16 > 0);
    assert_eq!(
        ELITE_MESSAGE,
        "Message from Elite Federation of Pilots:\rRight on, Commander!"
    );
}

#[test]
fn medals_and_permits_are_data_009091_bits() {
    let mut career = Career::default();
    assert!(career.award_medal(0));
    assert!(!career.award_medal(0));
    assert!(career.award_medal(15));
    assert_eq!(medals(career.medals), vec![MEDALS[0], MEDALS[15]]);
    assert_eq!(
        medal_message(MEDALS[0]),
        "Please take this Certificate of Valour medal in thanks for good service."
    );
}

#[test]
fn f236_records_the_offence_in_the_current_jurisdiction() {
    let mut legal = Legal::default();
    let report = legal
        .player_fired(Some(2))
        .expect("a near starport records the offence");
    assert_eq!(report.charged, 6_000);
    assert_eq!(legal.fines, [0, 0, 6_000, 0, 0, 0]);
    assert_eq!(legal.records[2], 1 << 3, "unlawful discharge bit");
    assert_eq!(legal.status(2), "Criminal");
    assert_eq!(legal.status(1), "Clean");
    assert_eq!(legal.record(2), 1 << 3);
    assert_eq!(legal.record(1), 0);
}

#[test]
fn f544_pays_the_fine_and_f545_refuses_without_enough_cash() {
    let mut legal = Legal::default();
    legal.fines[2] = 6_000;
    legal.records[2] = 1 << 3;
    // F545: cash is below the fine.
    let refused = legal.pay_fine(5_999, 2);
    assert_eq!(refused.amount, 6_000);
    assert_eq!(refused.paid, 0);
    assert!(!refused.ok());
    assert_eq!(legal.fines[2], 6_000);
    assert_eq!(
        fine_refused_message(refused.amount),
        "You've until midnight to pay $600.0 or be put in jail and your ship confiscated."
    );
    // F544: exactly enough.
    let paid = legal.pay_fine(6_000, 2);
    assert_eq!(paid.amount, 6_000);
    assert_eq!(paid.paid, 6_000);
    assert!(paid.ok());
    assert_eq!(legal.fines[2], 0);
    // F544 clears only the ledger; DATA_009093's offence bits are permanent.
    assert_eq!(legal.records[2], 1 << 3);
    assert_eq!(legal.status(2), "Clean");
    assert_eq!(
        fine_paid_message(paid.amount),
        "The $600.0 has been deducted. You may be punished more severely in the future."
    );
    // Paying a zero fine takes nothing.
    let zero = legal.pay_fine(0, 2);
    assert_eq!(zero.amount, 0);
    assert!(zero.ok());
}

#[test]
fn the_profile_uses_the_three_named_slots_and_the_current_heading() {
    let mut legal = Legal {
        fines: [5_500, 100, 0, 25_000, 0, 0],
        ..Legal::default()
    };
    legal.records[3] = 1;
    let career = Career {
        federal: 16,
        imperial: 1,
        elite: 1_024,
        ..Career::default()
    };
    let profile = ffe_core::status::Profile::new("Jameson", "Gateway", 3, &career, &legal);
    assert_eq!(profile.commander, "Jameson");
    assert_eq!(profile.heading(), "ALLIANCE");
    assert_eq!(profile.federal_rank, "Corporal");
    assert_eq!(profile.imperial_title, "Serf");
    assert_eq!(profile.elite_rating, "Mostly Harmless");
    // F312 prints slots 2, 1 and 0 as Federal, Imperial and Interpol.
    assert_eq!(profile.federal_status, "Clean");
    assert_eq!(profile.imperial_status, "Offender");
    assert_eq!(profile.interpol_status, "Criminal");
    assert_eq!(profile.record, vec!["Failure to obey Traffic Control"]);
    assert_eq!(profile_jurisdiction(2), "FEDERAL");
}
