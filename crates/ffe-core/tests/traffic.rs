use ffe_core::traffic::{
    begin_exit, state, Bay, Reply, Stages, Starport, CLEARANCE_SECONDS, DOCKING_DONE, GRANTED,
};

const PLAYER: u8 = 5;
const OTHER: u8 = 9;

#[test]
fn permission_is_refused_while_another_ship_is_handled() {
    let mut port = Starport::new(3);
    assert!(port.request(OTHER));
    assert_eq!(
        (port.mode, port.current, port.timeout),
        (GRANTED, OTHER, 0x7080)
    );
    // Mode 0xff and a different ship: refused, nothing changes.
    let before = port;
    assert!(!port.request(PLAYER));
    assert_eq!(port, before);
    // The same ship may ask again.
    assert!(port.request(OTHER));
    // Modes 0 and 1 accept anyone.
    for mode in [0, 1] {
        port.mode = mode;
        assert!(port.request(PLAYER), "mode {mode}");
        assert_eq!(port.current, PLAYER);
    }
}

#[test]
fn bays_are_handed_out_from_the_highest_and_kept() {
    let mut port = Starport::new(3);
    assert_eq!(
        port.allocate(PLAYER),
        Some(Bay {
            index: 3,
            fresh: true
        })
    );
    assert_eq!(
        port.allocate(OTHER),
        Some(Bay {
            index: 2,
            fresh: true
        })
    );
    // The same ship gets its own bay back, not a new one.
    assert_eq!(
        port.allocate(PLAYER),
        Some(Bay {
            index: 3,
            fresh: false
        })
    );
    assert_eq!(
        port.allocate(11),
        Some(Bay {
            index: 1,
            fresh: true
        })
    );
    assert_eq!(
        port.allocate(12),
        Some(Bay {
            index: 0,
            fresh: true
        })
    );
    assert_eq!(port.allocate(13), None);
    // Releasing frees it, sets mode 1 and clears the record.
    port.release(OTHER);
    assert_eq!((port.mode, port.current, port.timeout), (1, 0, 0));
    assert_eq!(
        port.allocate(13),
        Some(Bay {
            index: 2,
            fresh: true
        })
    );
}

#[test]
fn the_full_request_answers_as_the_comms_screen_does() {
    let mut port = Starport::new(0);
    // One bay (index 0).
    assert_eq!(
        port.request_docking(PLAYER, true, 0),
        Reply::Granted(Bay {
            index: 0,
            fresh: true
        })
    );
    assert_eq!(
        port.request_docking(PLAYER, true, 0),
        Reply::Granted(Bay {
            index: 0,
            fresh: false
        })
    );
    assert_eq!(port.request_docking(OTHER, false, 0), Reply::Busy);
    // Fines only stop the player, and before anything is touched.
    let mut fresh = Starport::new(0);
    assert_eq!(fresh.request_docking(PLAYER, true, 0x2710), Reply::Fined);
    assert_eq!(fresh, Starport::new(0));
    assert!(matches!(
        fresh.request_docking(OTHER, false, 0x2710),
        Reply::Granted(_)
    ));
    // No free bay: the record is reset (mode 0, nobody).
    let mut full = Starport::new(0);
    full.mode = 1;
    full.pads[0] = OTHER;
    assert_eq!(full.request_docking(PLAYER, true, 0), Reply::Full);
    assert_eq!((full.mode, full.current, full.timeout), (0, 0, 0));
}

#[test]
fn only_the_cleared_ship_can_touch_the_pad() {
    let mut port = Starport::new(3);
    assert!(!port.touch(PLAYER));
    assert!(port.request(PLAYER));
    assert!(!port.touch(OTHER));
    assert!(port.is_docking(PLAYER) && !port.is_docking(OTHER));
    assert!(port.touch(PLAYER));
    assert_eq!((port.mode, port.timeout), (DOCKING_DONE, CLEARANCE_SECONDS));
    port.reset();
    assert_eq!((port.mode, port.current, port.timeout), (0, 0, 0));
}

#[test]
fn clearance_runs_out_in_game_seconds() {
    let mut port = Starport::new(3);
    port.request_docking(PLAYER, true, 0);
    assert_eq!(port.elapse(100), None);
    assert_eq!(port.timeout, 0x7080 - 100);
    assert_eq!(port.elapse(0x7080 - 100 - 1), None);
    assert_eq!(port.timeout, 1);
    // Equal to what is left: expired (unsigned not-above), the bay is released.
    assert_eq!(port.elapse(1), Some(PLAYER));
    assert_eq!((port.mode, port.current, port.timeout), (1, 0, 0));
    assert!(port.pads.iter().all(|p| *p == 0));
    // No timeout, nothing to count.
    assert_eq!(port.elapse(1_000_000), None);
}

#[test]
fn the_door_follows_the_mode() {
    let mut port = Starport::new(3);
    assert_eq!(port.door, 0xffff);
    // Permission: mode 0xff counts the door down to 0 (open), then mode 0xfe.
    port.request(PLAYER);
    let mut frames = 0;
    while port.door != 0 {
        port.move_door(994);
        frames += 1;
        assert!(frames < 1000);
    }
    assert_eq!(port.mode, 0xfe);
    // The touch (mode 3) counts it back up to 0xffff and clears bit 0.
    port.touch(PLAYER);
    while port.door != 0xffff {
        port.move_door(994);
    }
    assert_eq!(port.mode, 2);
    // A launching ship's exit starts the door opening again.
    begin_exit(&mut port);
    assert_eq!((port.mode, port.door), (0xff, 0xffff));
}

fn run(mut stages: Stages, thargoid: bool, door: u16, limit: usize) -> Vec<(u8, usize)> {
    let mut log = Vec::new();
    for frame in 0..limit {
        if let Some(left) = stages.step(994, thargoid, door) {
            log.push((left, frame));
        }
    }
    log
}

#[test]
fn docking_runs_the_stages_in_order_with_the_original_timeouts() {
    let log = run(Stages::touched(false), false, 0xffff, 5000);
    let states: Vec<u8> = log.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        states,
        [0x0a, 0x0c, 0x0e, 0x10, 0x12, 0x14, 0x16, 0x18, 0x28]
    );
    // The first stage lasts 0x70000 tics at 994 a frame, the later ones 0x1ffff
    // (0x18000 and 0x17530 where the original sets them).
    let first = log[0].1;
    assert!(
        (0x7_0000 / 994 - 1..=0x7_0000 / 994).contains(&first),
        "{first}"
    );
    let total: usize = log.last().unwrap().1;
    // 458752 + 131071 + 98304 + 3 * 131071 + 131071 + 95536 + 95536 tics: about 26 s of game time.
    assert!((1310..1330).contains(&total), "{total} frames");
    // And the docked state follows at once.
    let mut stages = Stages {
        state: state::DOCKED_THARGOID,
        timeout: 0,
        drawn: None,
    };
    assert_eq!(stages.step(994, false, 0), Some(state::DOCKED_THARGOID));
    assert_eq!(stages.state, state::DOCKED);
}

#[test]
fn a_thargoid_skips_the_lift() {
    let log = run(Stages::touched(false), true, 0xffff, 1000);
    assert_eq!(
        log.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        [0x0a, 0x28]
    );
}

#[test]
fn launching_waits_for_the_door() {
    let stages = Stages::launch_requested();
    let closed = run(stages, false, 0xffff, 3000);
    let states: Vec<u8> = closed.iter().map(|(s, _)| *s).collect();
    // Stops in state 0x36 while the door word is not zero.
    assert_eq!(states, [0x1a, 0x1c, 0x1e, 0x20, 0x22]);
    let mut stages = Stages::launch_requested();
    let mut left = Vec::new();
    for _ in 0..3000 {
        let door = if stages.state == state::LAUNCHING_STATION {
            0
        } else {
            0xffff
        };
        if let Some(s) = stages.step(994, false, door) {
            left.push(s);
        }
    }
    assert_eq!(left, [0x1a, 0x1c, 0x1e, 0x20, 0x22, 0x36]);
    assert_eq!(stages.state, state::AFTER_TAKEOFF);
}

#[test]
fn a_thargoid_station_launches_straight_out() {
    let mut stages = Stages::launch_requested();
    let mut left = Vec::new();
    for _ in 0..100 {
        if let Some(s) = stages.step(994, true, 0xffff) {
            left.push(s);
        }
    }
    assert_eq!(left, [0x1a]);
    assert_eq!(stages.state, state::LAUNCHING_STATION);
}
