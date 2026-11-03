//! The original's message strings (cache v30): F1343's codes resolve to the
//! executable's own text. Local-only: needs `FFE_WORLD_CACHE`.
use ffe_core::message_strings::Messages;

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn message_codes_give_the_executables_text() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let messages = Messages::new(cache.asset("base:text/messages").unwrap()).unwrap();
    for (code, text) in [
        (
            0x98be,
            "VACATE THE AREA IMMEDIATELY.\nYou do not have clearance.",
        ),
        (0x98c5, "ATTENTION: Mis-Jump"),
        (0x98c7, "ATTENTION: Mis-Jump - Drive Destroyed"),
        (0x9928, "You do not have enough cabin space available."),
        (0xa212, "too hot for mining with an MB4"),
    ] {
        assert_eq!(messages.text(code).as_deref(), Some(text), "{code:#x}");
    }
    // The ten imported arrays and their sizes on floppy 1.02.
    assert_eq!(
        messages.counts(),
        vec![
            (0, 44),
            (3, 59),
            (5, 2),
            (10, 46),
            (12, 486),
            (14, 129),
            (15, 13),
            (17, 30),
            (19, 7),
            (20, 468)
        ]
    );
}

/// The port's fixed console and contract texts are the executable's strings
/// (`…` marks an F1399 expansion).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_ports_fixed_texts_are_the_executables() {
    use ffe_core::missions::{Kind, Mission, Status};
    use ffe_core::traffic::Reply;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let messages = Messages::new(cache.asset("base:text/messages").unwrap()).unwrap();
    for (code, reply) in [
        (0x98b7, Reply::Busy),
        (0x98b8, Reply::Full),
        (0x98b9, Reply::Fined),
    ] {
        assert_eq!(
            messages.text(code).as_deref(),
            Some(ffe_rs::navigation::refusal(reply)),
            "{code:#x}"
        );
    }
    let mut mission = Mission {
        id: 1,
        kind: Kind::MilitaryStrike,
        code: 0x9857,
        origin: 0,
        origin_name: "…".to_owned(),
        destination: 0,
        destination_name: String::new(),
        reward: 0,
        accepted_day: 0,
        deadline_day: 0,
        passengers: 0,
        cargo: "…".to_owned(),
        seed: 0,
        basic_data: 0,
        client: String::new(),
        status: Status::Active,
        settled_day: None,
        done: false,
        side: 1,
    };
    for (code, strike, side) in [
        (0x9c1f, 0x9857, 1),
        (0x9c20, 0x9857, 2),
        (0x9c24, 0x9855, 1),
        (0x9c25, 0x9855, 2),
        (0x9c26, 0x9856, 1),
        (0x9c27, 0x9856, 2),
    ] {
        mission.code = strike;
        mission.side = side;
        assert_eq!(mission.row_code(), Some(code));
        // The original's "……" is the return point's two expansions.
        let text = messages.text(code).unwrap().replace("……", "…");
        assert_eq!(mission.row_text(), text, "{code:#x}");
    }
}

/// The ship library's journal archive (cache v31): MISSION.DAT's articles
/// through the executable's offset table, F78-unscrambled.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_journal_archive_is_mission_dats_articles() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let journal =
        ffe_core::journal::Journal::new(cache.asset("base:text/journal").unwrap()).unwrap();
    assert_eq!(journal.len(), ffe_core::journal::ARTICLES);
    let first = journal.article(0).unwrap();
    assert_eq!(first.publication, b"S.L.A.M");
    assert_eq!(
        first.headline,
        b"THE THARGOIDS - UNLIKELY RUMOURS FROM [SyL1(I)".as_slice()
    );
    assert!(first.body.starts_with(b"A series of wild rumours"));
    // Article 28 is one of the archive's empty one-byte slots.
    assert_eq!(journal.raw(28), Some(&b""[..]));
}
