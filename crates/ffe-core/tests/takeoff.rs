//! The open-air starport takeoff's mode word, from `ffe.asm`: F1013's
//! `0x24` → `0x26` hand-off (altitude above `0x30d40` or a set speed) and
//! F1031's `0x26` decisions (the `0x20` starport region clear releases the bay;
//! a landing `0x40` on an open-air starport returns to `0x24`).

use ffe_core::traffic::{state, Landing, Takeoff, TakeoffStep, TAKEOFF_ALTITUDE};

#[test]
fn the_new_game_landed_state_takes_off_into_f1013s_climb() {
    let mut takeoff = Takeoff::landed();
    assert_eq!(takeoff.state, state::LANDED);
    assert_eq!(takeoff.state, 0x30);
    assert!(takeoff.begin());
    assert_eq!(takeoff.state, state::TAKEOFF);
    assert_eq!(takeoff.state, 0x24);
    // The launch action only starts from the pad.
    assert!(!takeoff.begin());
    assert_eq!(takeoff.state, 0x24);
    assert_eq!(Takeoff::taking_off(), takeoff);
}

#[test]
fn f1013_hands_over_above_the_altitude_or_any_set_speed() {
    let mut takeoff = Takeoff::taking_off();
    // At the threshold exactly: F1013's `jg` has not passed it.
    assert_eq!(
        takeoff.step(TAKEOFF_ALTITUDE, 0, 0x20, true),
        None,
        "0x30d40 is not above 0x30d40"
    );
    assert_eq!(takeoff.state, state::TAKEOFF);
    assert_eq!(
        takeoff.step(TAKEOFF_ALTITUDE + 1, 0, 0x20, true),
        Some(TakeoffStep::HandOff)
    );
    assert_eq!(takeoff.state, state::AFTER_TAKEOFF);
    assert_eq!(takeoff.state, 0x26);

    let mut takeoff = Takeoff::taking_off();
    assert_eq!(takeoff.step(0, 1, 0x20, true), Some(TakeoffStep::HandOff));
    assert_eq!(takeoff.state, 0x26);
    let mut takeoff = Takeoff::taking_off();
    assert_eq!(takeoff.step(0, -1, 0x20, true), Some(TakeoffStep::HandOff));
}

#[test]
fn f1031_clears_only_when_the_starport_region_is_behind_the_ship() {
    let mut takeoff = Takeoff {
        state: state::AFTER_TAKEOFF,
    };
    // The 0x20 flag is the starport's effect region: while it is set the mode
    // stays 0x26, with or without a landing.
    assert_eq!(takeoff.step(1_000, 0, 0x20, true), None);
    assert_eq!(takeoff.state, 0x26);
    assert_eq!(takeoff.step(1_000, 0, 0x22, true), None);
    assert_eq!(takeoff.state, 0x26);
    // Region clear: F1031 sets 0 and F1032 releases the bay.
    assert_eq!(
        takeoff.step(1_000, 0, 0x02, true),
        Some(TakeoffStep::Cleared)
    );
    assert_eq!(takeoff.state, state::FREE);
    assert_eq!(takeoff.state, 0x00);
    assert_eq!(takeoff.step(1_000, 0, 0, true), None);
}

#[test]
fn f1031_takes_off_again_after_a_landing_on_the_open_air_pad() {
    let mut takeoff = Takeoff {
        state: state::AFTER_TAKEOFF,
    };
    // 0x20 (region) and 0x40 (landing) on an open-air starport: back to 0x24.
    assert_eq!(
        takeoff.step(1_000, 500, 0x60, true),
        Some(TakeoffStep::Retakeoff)
    );
    assert_eq!(takeoff.state, state::TAKEOFF);
    // An indoor station's pad is not an open-air starport: F1031's second test
    // requires the object's 0x20 class bit, so nothing happens.
    let mut takeoff = Takeoff {
        state: state::AFTER_TAKEOFF,
    };
    assert_eq!(takeoff.step(1_000, 0, 0x60, false), None);
    assert_eq!(takeoff.state, 0x26);
    // The region check comes first: a clear region wins over the landing bit.
    let mut takeoff = Takeoff {
        state: state::AFTER_TAKEOFF,
    };
    assert_eq!(
        takeoff.step(1_000, 0, 0x40, true),
        Some(TakeoffStep::Cleared)
    );
}

/// F1021's surface landing (`0x32`), F1047's view frame (`0x34`), and F1056's
/// launch action into F1013's climb (`0x24`), which F1013/F1031 then take out
/// to free flight exactly as an open-air starport takeoff does. F1033's flags
/// have no `0x20` region on a planet, so F1031 clears on the first `0x26`
/// frame.
#[test]
fn a_terrain_landing_reaches_the_landed_state_and_launches_again() {
    let mut landing = Landing::on_surface();
    assert_eq!(landing.state, state::LANDING);
    assert_eq!(landing.state, 0x32);
    assert!(!landing.landed(), "0x32 is the frame of the touch");
    // Next frame's view function F1047.
    assert_eq!(landing.frame(), Some(state::LANDED_ROUGH));
    assert_eq!(landing.state, 0x34);
    assert!(landing.landed(), "0x34 is the guarded landed state");
    assert_eq!(landing.frame(), None, "0x34 has no further transition");

    // F1056's action 9 from 0x34 goes straight to 0x24.
    let mut takeoff = landing.begin_launch().expect("a landed ship can launch");
    assert_eq!(takeoff.state, state::TAKEOFF);
    assert_eq!(takeoff.state, 0x24);
    assert!(Takeoff::from_landed(state::LANDED_ROUGH).is_some());
    assert!(Takeoff::from_landed(state::LANDED).is_some());
    assert!(Takeoff::from_landed(state::FREE).is_none());
    assert!(!takeoff.begin(), "already climbing");

    // F1013's up-thrust climb hands over at the altitude, then F1031 sees a
    // planet touch (0x40) with no starport region (0x20): free flight.
    assert_eq!(
        takeoff.step(TAKEOFF_ALTITUDE + 1, 0, 0x42, false),
        Some(TakeoffStep::HandOff)
    );
    assert_eq!(takeoff.state, state::AFTER_TAKEOFF);
    assert_eq!(
        takeoff.step(TAKEOFF_ALTITUDE + 2, 0, 0x42, false),
        Some(TakeoffStep::Cleared)
    );
    assert_eq!(takeoff.state, state::FREE);
}

/// F1019's open-air starport landing takes the parallel `0x2e` -> `0x30` pair.
#[test]
fn a_starport_landing_reaches_the_pad_state() {
    let mut landing = Landing::at_starport();
    assert_eq!(landing.state, 0x2e);
    assert!(!landing.landed());
    assert_eq!(landing.frame(), Some(state::LANDED));
    assert_eq!(landing.state, state::LANDED);
    assert!(landing.landed());
    // Its launch request (F381/F654) then starts the climb.
    let takeoff = landing.begin_launch().expect("the pad can launch");
    assert_eq!(takeoff.state, state::TAKEOFF);
}
