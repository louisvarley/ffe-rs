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
