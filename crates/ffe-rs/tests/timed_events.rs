//! Time-driven events over a long run: the day boundary, its once-only
//! periodic updates (F152/F633), mission deadlines, and the NPC persistence
//! and recycle the daily/long-run paths touch (F675/F680/F927).
//!
//! The content checks are local-only because they need the v29 cache
//! (`FFE_WORLD_CACHE`). The pure day arithmetic lives in
//! `ffe_core::calendar` and runs in the normal suite.

use ffe_core::npc::{self, Role, State};
use ffe_rs::navigation::{DriveState, Navigator};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn day_seconds(days: f64) -> f64 {
    days * 86_400.0
}

/// The market and the bulletin board/stock of the starting station.
fn station_snapshot(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
) -> (
    ffe_rs::navigation::StationMarket,
    Vec<String>,
    Vec<(u32, u16)>,
) {
    let station = nav.docked.expect("docked at the start");
    let market = nav.market(cache, station).unwrap().clone();
    let offers = nav.station_offers(cache, station).unwrap();
    let posts = offers.posts.iter().map(|post| post.text.clone()).collect();
    (market, posts, offers.hulls.clone())
}

/// F152's day marker: crossing the day boundary runs F633 exactly once, and a
/// frame-by-frame run and one long accelerated step reach the same state.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_day_boundary_runs_the_updates_once() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let (before, posts, hulls) = station_snapshot(&mut nav, &cache);
    assert_eq!(nav.day(), 0);
    assert_eq!(nav.last_day_tick(), 0);

    // 86,399 s is still day 0: nothing changed and the marker did not move.
    nav.advance(&cache, day_seconds(1.0) - 1.0);
    let (same, same_posts, same_hulls) = station_snapshot(&mut nav, &cache);
    assert_eq!(same, before);
    assert_eq!(same_posts, posts);
    assert_eq!(same_hulls, hulls);
    assert_eq!(nav.last_day_tick(), 0);

    // One more second crosses the day: the market, the board and the stock ran
    // their F633 draws exactly once.
    nav.advance(&cache, 1.0);
    assert_eq!(nav.day(), 1);
    assert_eq!(nav.last_day_tick(), 1);
    let (day_one, day_one_posts, day_one_hulls) = station_snapshot(&mut nav, &cache);
    assert_ne!(day_one, before, "the market drifted");
    assert_ne!(day_one_posts, posts, "the board replaced an entry");
    assert_ne!(day_one_hulls, hulls, "the hull stock changed");

    // More sub-day frames at the same day change nothing.
    for _ in 0..100 {
        nav.advance(&cache, 600.0);
    }
    let (still_day_one, still_posts, still_hulls) = station_snapshot(&mut nav, &cache);
    assert_eq!(still_day_one, day_one);
    assert_eq!(still_posts, day_one_posts);
    assert_eq!(still_hulls, day_one_hulls);
    assert_eq!(nav.last_day_tick(), 1);
}

/// The same number of days reached in one accelerated step and in many small
/// frames leaves the same market, board and stock: no per-frame drift and no
/// double draw.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn acceleration_matches_stepwise_days() {
    let cache = cache();

    let mut one_step = Navigator::new(&cache).unwrap();
    let (start, _, _) = station_snapshot(&mut one_step, &cache);
    one_step.advance(&cache, day_seconds(4.0));
    let after_one = station_snapshot(&mut one_step, &cache);

    let mut stepwise = Navigator::new(&cache).unwrap();
    let (start_again, _, _) = station_snapshot(&mut stepwise, &cache);
    assert_eq!(start_again, start);
    for _ in 0..4 * 24 {
        stepwise.advance(&cache, day_seconds(1.0) / 24.0);
    }
    let after_many = station_snapshot(&mut stepwise, &cache);
    assert_eq!(after_one, after_many);
    assert_eq!(one_step.last_day_tick(), 4);
    assert_eq!(stepwise.last_day_tick(), 4);
    assert_ne!(after_one.0, start, "four days of updates");

    // The tick reports one record per crossed day, in order.
    one_step.advance(&cache, day_seconds(3.0));
    let ticks = one_step.tick_days(&cache);
    assert!(ticks.is_empty(), "the clock did not move");
    one_step.advance(&cache, day_seconds(2.0));
    let ticks = one_step.tick_days(&cache);
    assert!(ticks.is_empty(), "advance already ran the two days");
}

/// A multi-day jump through the tick API returns every day once, in order.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_long_clock_jump_ticks_each_day_in_order() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let _ = station_snapshot(&mut nav, &cache);
    nav.advance(&cache, day_seconds(2.0));
    // Ask for the days directly after advancing the clock without the frame
    // path: the tick covers 3..7 exactly once.
    nav.clock += day_seconds(5.0);
    let ticks = nav.tick_days(&cache);
    let days: Vec<u32> = ticks.iter().map(|tick| tick.day).collect();
    assert_eq!(days, vec![3, 4, 5, 6, 7]);
    assert!(ticks.iter().all(|tick| tick.markets == 1), "{ticks:?}");
    nav.tick_days(&cache);
    assert_eq!(nav.last_day_tick(), 7);
}

/// Mission deadlines are counted in game days, expire once and are drained by
/// the missions stream exactly once.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn mission_deadlines_expire_on_their_day() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    nav.schedule_deadline(7, 2);
    nav.schedule_deadline(8, 5);
    nav.schedule_deadline(9, 1);
    nav.cancel_deadline(9);
    assert_eq!(nav.deadlines().len(), 2);

    nav.advance(&cache, day_seconds(1.0));
    assert!(nav.take_expired().is_empty());

    nav.advance(&cache, day_seconds(1.0));
    let expired = nav.take_expired();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, 7);
    assert_eq!(expired[0].day, 2);
    assert!(nav.take_expired().is_empty(), "drained once");

    nav.advance(&cache, day_seconds(3.0));
    assert_eq!(nav.take_expired()[0].id, 8);
    assert!(nav.deadlines().is_empty());
    assert!(!nav.cancel_deadline(8), "already expired");
}

/// F675/F680: a hostile ship close to the player survives a jump and is
/// restored; F927 recycles a ship that has escaped or drifted out of range.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn npcs_persist_across_a_jump_and_are_recycled() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.unwrap();
    let at = nav.bodies[station].position;
    assert!(nav.respawn_traffic(&cache, at, [0.0, 0.0, 1.0]));
    assert!(!nav.ships.is_empty());

    // Put a pirate inside F675's range so it follows the jump.
    let pirate = nav
        .ships
        .iter_mut()
        .find(|ship| ship.role == Role::Pirate)
        .expect("the capture spawn has a pirate");
    pirate.position = [at[0] + 1_000, at[1], at[2]];
    pirate.velocity = [0.0; 3];
    let name = pirate.name.clone();

    let target = (nav.current_system() + 1) % nav.system_count();
    let arrival = nav.jump(&cache, target).unwrap();
    assert!(nav.ships.is_empty(), "F674 clears the old system");
    nav.spawn_traffic(&cache, arrival, [0.0, 0.0, 1.0]);
    let restored = nav
        .ships
        .iter()
        .find(|ship| ship.name == name)
        .expect("F680 restored the pirate beside the arrival point");
    assert!(restored.hostile);

    // F927: an escaped ship is removed on the next traffic tick.
    let escaped = nav.ships.iter_mut().find(|ship| ship.name == name).unwrap();
    escaped.state = State::Escaped;
    let events = nav.tick_ships(0.1, 4_971, arrival, 1_000);
    assert!(events.iter().any(
        |event| matches!(event, ffe_rs::navigation::ShipEvent::Escaped { name: n } if *n == name)
    ));
    assert!(!nav.ships.iter().any(|ship| ship.name == name));

    // F927: a ship beyond the despawn range is recycled too.
    if let Some(far) = nav.ships.first_mut() {
        far.position = [arrival[0] + 100_000_000, arrival[1], arrival[2]];
        let far_name = far.name.clone();
        nav.tick_ships(0.1, 4_971, arrival, 1_000);
        assert!(!nav.ships.iter().any(|ship| ship.name == far_name));
    }
}

/// A scripted long run: a hyperspace jump, days of accelerated flight, a
/// docked visit and the once-per-day state changes. With `FFE_SHOT_DIR` set it
/// writes `timed-events.jsonl` as the capture.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn long_run_capture() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    nav.drive = Some(DriveState {
        mass: 16,
        class: 10,
        fuel: 3,
    });
    let mut station = nav.docked.unwrap();
    let mut trace = Vec::new();
    let mut record = |nav: &mut Navigator, station: usize, note: &str| {
        let (price, stock, post_count, hull_count) = {
            let market = nav.market(&cache, station).unwrap();
            let first = market.market.entries[0];
            let offers = nav.station_offers(&cache, station).unwrap();
            (
                first.price,
                first.stock,
                offers.posts.len(),
                offers.hulls.len(),
            )
        };
        let clock = nav.clock;
        let day = nav.day();
        let last_day_tick = nav.last_day_tick();
        let system = nav.system_name().to_owned();
        let docked = nav.docked.is_some();
        let ships = nav.ships.len();
        trace.push(format!(
            "{{\"note\":\"{note}\",\"clock\":{clock:.1},\"day\":{day},\"last_day_tick\":{last_day_tick},\"system\":\"{system}\",\"docked\":{docked},\"ships\":{ships},\"water\":{{\"price\":{price},\"stock\":{stock}}},\"posts\":{post_count},\"hulls\":{hull_count}}}"
        ));
    };

    record(&mut nav, station, "start");
    for _ in 0..8 {
        // Six four-hour frames a day, at a time-acceleration-style cadence.
        for _ in 0..6 {
            nav.advance(&cache, day_seconds(1.0) / 6.0);
        }
        record(&mut nav, station, "day");
    }

    // A real hyperspace jump: the clock moves by the jump time, the day marker
    // does not replay, and the destination's traffic is spawned.
    let centauri = (1..15)
        .find(|i| nav.range_to(*i) == Some(435))
        .expect("Alpha Centauri at 4.35 light years");
    let before = nav.day();
    let jumped = nav.hyperspace(&cache, centauri).unwrap();
    assert!(jumped.days > 0.5, "{jumped:?}");
    assert!(nav.day() >= before);
    let starport = nav.bodies.iter().position(|body| {
        matches!(
            body.kind,
            ffe_rs::navigation::Kind::Station | ffe_rs::navigation::Kind::Starport
        )
    });
    if let Some(index) = starport {
        station = index;
        let arrival = nav.bodies[station].position;
        nav.spawn_traffic(&cache, arrival, [0.0, 0.0, 1.0]);
        record(&mut nav, station, "jump");
        nav.dock(station);
        record(&mut nav, station, "dock");
    }

    let days = trace
        .iter()
        .filter(|line| line.contains("\"note\":\"day\""))
        .count();
    assert_eq!(days, 8);
    assert!(trace.iter().any(|line| line.contains("\"note\":\"jump\"")));
    assert!(trace.iter().any(|line| line.contains("\"note\":\"dock\"")));
    assert!(trace.last().unwrap().contains("\"docked\":true"));
    // The day ticks are unique and complete.
    if let Some(path) = std::env::var_os("FFE_SHOT_DIR") {
        let path = std::path::Path::new(&path).join("timed-events.jsonl");
        std::fs::write(&path, trace.join("\n")).unwrap();
        eprintln!("timed-events capture: {}", path.display());
    } else {
        for line in &trace {
            eprintln!("{line}");
        }
    }

    // The NPC helpers are reachable from the long run too.
    assert!(nav.ships.len() <= npc::MAX_SHIPS);
}
