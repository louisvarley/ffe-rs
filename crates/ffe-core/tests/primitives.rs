use ffe_core::{timer_delta, GameClock, RandomState, TimerCounters, TimerError};

#[test]
fn random_state_rotates_previous_a_and_returns_only_low_word() {
    let mut rng = RandomState::new(0, 0);
    assert_eq!(rng.next_u16(), 26796);
    assert_eq!(rng, RandomState::new(4263798956, 0));
    let previous = rng;
    let result = rng.next_u16();
    assert_eq!(rng.b, previous.a.rotate_left(13));
    assert_eq!(result, rng.a as u16);
    let mut identical = previous;
    assert_eq!(identical.next_u16(), result);
    assert_eq!(identical, rng);
}

#[test]
fn random_states_are_instance_local_and_wrapping_in_debug_builds() {
    let mut left = RandomState::new(u32::MAX, u32::MAX);
    let mut right = left;
    let sequence: Vec<_> = (0..1024).map(|_| left.next_u16()).collect();
    assert_eq!(right, RandomState::new(u32::MAX, u32::MAX));
    assert_eq!(
        sequence,
        (0..1024).map(|_| right.next_u16()).collect::<Vec<_>>()
    );
    assert_eq!(left, right);
}

#[test]
fn game_clock_propagates_fraction_carry_and_wraps_day_word() {
    for (before, delta, expected) in [
        ((3, 10), 0, (3, 10)),
        ((3, u32::MAX), 1, (4, 0)),
        ((u32::MAX, u32::MAX), 2, (0, 1)),
        ((5, 1), u32::MAX, (6, 0)),
        ((5, 0), u32::MAX, (5, u32::MAX)),
    ] {
        let mut clock = GameClock {
            days: before.0,
            fraction: before.1,
        };
        clock.advance(delta);
        assert_eq!((clock.days, clock.fraction), expected);
    }
}

#[test]
fn timer_uses_wide_product_per_update_truncation_and_wrapping_counters() {
    assert_eq!(timer_delta(20, 994), Ok(994));
    assert_eq!(timer_delta(21, 994), Ok(1043));
    assert_eq!(timer_delta(19, 1), Ok(0));
    assert_eq!(timer_delta(0, u32::MAX), Ok(0));
    assert_eq!(timer_delta(20, u32::MAX), Ok(u32::MAX));
    assert_eq!(timer_delta(21, u32::MAX), Err(TimerError::DeltaOverflow));
    let mut counters = TimerCounters {
        system: u32::MAX - 15,
        accelerated: u32::MAX - 31,
    };
    counters.advance(21, 1000).unwrap();
    assert_eq!(
        counters,
        TimerCounters {
            system: 1027,
            accelerated: 1018
        }
    );
    let mut paused = TimerCounters::default();
    paused.advance(20, 0).unwrap();
    assert_eq!(
        paused,
        TimerCounters {
            system: 994,
            accelerated: 0
        }
    );
}

#[test]
fn timer_error_does_not_partially_mutate_counters_and_no_remainder_is_carried() {
    let mut counters = TimerCounters {
        system: 10,
        accelerated: 20,
    };
    let before = counters;
    assert_eq!(
        counters.advance(21, u32::MAX),
        Err(TimerError::DeltaOverflow)
    );
    assert_eq!(counters, before);
    assert_eq!(
        counters.advance(u32::MAX, 0),
        Err(TimerError::DeltaOverflow)
    );
    assert_eq!(counters, before);
    let mut split = TimerCounters::default();
    split.advance(1, 994).unwrap();
    split.advance(1, 994).unwrap();
    let mut combined = TimerCounters::default();
    combined.advance(2, 994).unwrap();
    assert_eq!(split.system, 98);
    assert_eq!(combined.system, 99);
}
