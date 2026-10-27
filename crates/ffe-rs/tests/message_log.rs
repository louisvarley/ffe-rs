//! The F271 message log's source mapping.
//!
//! The port used one `message` string for both the original's dated log and
//! port-only notices, so unported buttons, view names and debug/status lines
//! appeared as in-game messages. These tests pin the split:
//!
//! * `main.rs` has one writer (`log_channel.post`) and every event it writes is
//!   tagged with a [`Source`] that carries a DOS FFE citation;
//! * every `Source` is actually used (the mapping is not just documentation);
//! * the port-only notices (unported F4/F5, missile/combat views, "Front View",
//!   pause/status toggles, debug and capture text) never enter that writer.

use ffe_rs::messages::Source;

fn main_rs() -> &'static str {
    include_str!("../src/main.rs")
}

/// Every `needle` call up to its matching `)`. The calls in `main` are at most
/// a few lines; the parenthesis depth keeps a following `message = ...` out.
fn calls<'a>(text: &'a str, needle: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find(needle) {
        let start = at + offset;
        let rest = &text[start..];
        // Find the index where the parenthesis depth first returns to zero.
        let mut depth = 0i32;
        let mut end = rest.len();
        for (index, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push(&rest[..end]);
        at = start + needle.len();
    }
    found
}

#[test]
fn main_has_one_log_writer_and_no_raw_message_log_calls() {
    let source = main_rs();
    assert_eq!(
        source.matches(".messages.push(").count(),
        0,
        "a raw MessageLog::push bypasses the source-tagged channel"
    );
    assert_eq!(
        source.matches(".messages.update(").count(),
        0,
        "a raw MessageLog::update bypasses the source-tagged channel"
    );
    assert_eq!(
        source.matches("log_channel.post(").count(),
        1,
        "all entries must go through the single F271 dispatcher"
    );
}

#[test]
fn every_log_call_site_is_tagged_with_a_cited_source() {
    let source = main_rs();
    let sites = calls(source, "pending_log.").into_iter().filter(|call| {
        call.starts_with("pending_log.add(")
            || call.starts_with("pending_log.replace(")
            || call.starts_with("pending_log.clear(")
    });
    let mut seen = Vec::new();
    let mut count = 0;
    for call in sites {
        count += 1;
        let Some(variant) = call.split("Source::").nth(1) else {
            panic!("log call has no source tag: {call}");
        };
        let variant = variant
            .split(|character: char| !character.is_ascii_alphanumeric())
            .next()
            .unwrap_or_default();
        let source = Source::ALL
            .into_iter()
            .find(|source| format!("{source:?}") == variant)
            .unwrap_or_else(|| panic!("unknown log source {variant:?} in {call}"));
        assert!(
            source.citation().contains('F') || source.citation().contains("docs/"),
            "{} has no DOS citation: {}",
            variant,
            source.citation()
        );
        seen.push(source);
    }
    assert!(
        count >= 20,
        "the scan found only {count} log calls; the integration points moved"
    );
    for source in Source::ALL {
        assert!(
            seen.contains(&source),
            "{source:?} is never written by main.rs; its mapping is stale"
        );
    }
}

#[test]
fn port_only_notices_never_enter_the_log_channel() {
    let source = main_rs();
    // Each of these is a port-only note: an unported screen, a view name, a
    // pause/status toggle, a debug/error line, or capture/help text. The source
    // writes no F271 entry for them.
    const PORT_ONLY: &[&str] = &[
        "Front View",
        "Rear View",
        "Turret View",
        "External View",
        "not ported",
        "not implemented",
        "Time acceleration",
        "Paused.",
        "Resumed.",
        "Status: ship systems",
        "Flight console.",
        "Text labels on.",
        "Text labels off.",
        "Prototype flight model.",
        "Original flight:",
        "Modern controls:",
        "Classic controls:",
        "MODEL LAB ERROR",
        "MODEL VIEWER ERROR",
        "Screenshot failed",
        "Saved shots/",
        "Loaded most recent snapshot.",
        "SAVE ERROR",
        "LOAD ERROR",
        "MARKET UNAVAILABLE",
        "Market unavailable",
        "Map error",
        "ORIGINAL FLIGHT ERROR",
        "Scenario reset",
        "New game.",
        "Returned to the game.",
        "Capture:",
        "Transfer to",
        "Lifting off:",
        "Clear of the surface:",
        "Back on the pad:",
        "No escape capsule fitted.",
        "No missile target:",
        "A ship cannot be followed",
        "Lock a target first",
        "This is a port-only",
    ];
    for call in calls(source, "pending_log.") {
        for notice in PORT_ONLY {
            assert!(
                !call.contains(notice),
                "port-only notice {notice:?} reaches the log channel: {call}"
            );
        }
    }
    // The unported-button text lives in console.rs and must not be copied here.
    assert!(
        !source.contains("F2/F5 (comms) are not ported yet."),
        "the unported screen notice is in main"
    );
    // The view-change line was the reported "Front View" entry.
    assert!(
        !source.contains("views.mode.name()"),
        "the view name is still written somewhere"
    );
}

#[test]
fn no_landed_at_line_reaches_the_log() {
    // F994 (the start) and F1047/F1048 (landings) post no message; the
    // console shows the clock and F1099's altitude and body text instead.
    let source = main_rs();
    for call in calls(source, "pending_log.") {
        assert!(
            !call.contains("Landed at"),
            "a landing line reaches the log: {call}"
        );
        assert!(
            !call.contains("Landed on"),
            "a landing line reaches the log: {call}"
        );
    }
}
