//! The open-air starport takeoff's mode word, from `ffe.asm`: F1013's
//! `0x24` → `0x26` hand-off (altitude above `0x30d40` or a set speed) and
//! F1031's `0x26` decisions (the `0x20` starport region clear releases the bay;
//! a landing `0x40` on an open-air starport returns to `0x24`).

use ffe_core::traffic::{state, Takeoff, TakeoffStep, TAKEOFF_ALTITUDE};

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
