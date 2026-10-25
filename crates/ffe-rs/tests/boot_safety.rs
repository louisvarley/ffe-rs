//! Local-only boot-safety checks for the new game and the landed state:
//! F829/F998/F994 leave the player on the starport pad in state `0x30`.
//! F687/F686's start entry does run F688, but no pirate group may be forced
//! beside the pad, and F1012/F1048's landed handler suspends the NPC AI/fire/
//! damage while the player is stationary.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test boot_safety -- --ignored --nocapture`

use ffe_core::npc::{Role, State};
use ffe_rs::flight::Mode;
use ffe_rs::navigation::{Kind, Navigator, ShipEvent};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn distance(a: [i64; 3], b: [i64; 3]) -> f64 {
    let d: [f64; 3] = std::array::from_fn(|axis| (a[axis] - b[axis]) as f64);
    d.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// Run the main loop's traffic step for `frames` 20 ms frames. The guard is
/// main's: F1012/F1048's landed handler runs no NPC tick, so `tick_ships` is not
/// called while [`Navigator::landed_view`] is true. `radius` is F817's box, as
/// `ffe-rs` supplies it.
fn run_frames(nav: &mut Navigator, player: [i64; 3], radius: i64, frames: usize) -> Vec<ShipEvent> {
    let mut out = Vec::new();
    for _ in 0..frames {
        if nav.landed_view() {
            continue;
        }
        out.extend(nav.tick_ships(0.02, 994, player, radius));
    }
    out
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn hope_system_record_details() {
    let cache = cache();
    let (x, y, _, star, _) = ffe_rs::navigation::START_SYSTEMS[0];
    let code = (x >> 16) as u32 & 0x1fff | ((y >> 16) as u32 & 0x1fff) << 13 | (star & 0x3f) << 26;
    let tables =
        ffe_core::galaxy::Tables::new(cache.asset("base:world/system-tables").unwrap()).unwrap();
    let details = tables.details(code);
    println!("Gateway/Hope details: {details:?}");
    let mut rng = ffe_core::npc::Random::new(1, 2);
    println!(
        "F688 counts: {:?}",
        ffe_core::npc::counts(&details, &mut rng)
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_new_game_pad_is_not_attacked() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let new_game = nav.new_game.expect("a new-game state");
    let seat = new_game.seat;
    let station = nav.docked.expect("docked at the start starport");
    // F829/F998/F994: the new game is on the pad and does not call F680/F688.
    assert_eq!(nav.system_name(), "Gateway");
    assert_eq!(new_game.player_state, 0x30);
    // F687/F686's start entry spawns F688's traffic; the port's capture
    // guarantee and near-player pirate are not part of it. Hope's record has a
    // pirate group, but it belongs on the anchor orbit, not beside the pad.
    assert!(!nav.ships.is_empty(), "F688 traffic exists at the start");
    if let Some(pirate) = nav.ships.iter().find(|ship| ship.role == Role::Pirate) {
        println!(
            "start pirate {} at {:.0} mm from the pad",
            pirate.name,
            distance(pirate.position, seat)
        );
        assert!(
            distance(pirate.position, seat) > ffe_core::laser::RANGE,
            "the source's pirate group is not forced beside the pad"
        );
    }
    assert!(nav.on_pad(), "the new game is on the starport pad");
    assert!(nav.landed_view(), "0x30 is F1050/F1054's landed view");
    // The executed original's pad: `--state-combat` reports hull 100 and shields
    // [0,0] from frame 161 to 899 with no attacker.
    let hits = run_frames(&mut nav, seat, 10_000, 3_000);
    assert!(
        hits.is_empty(),
        "the pad was attacked for 60 s: {hits:?} (this is the owner's boot bug)"
    );
    assert_eq!(
        nav.new_game.expect("still the new game").player_state,
        0x30,
        "the start state persists"
    );
    // The traffic spawner itself still works when F688 is entered for real:
    // the capture path asks for the port's guaranteed trader/pirate group.
    let forward = nav.bodies[station].orientation[2];
    nav.respawn_traffic(&cache, seat, forward);
    assert!(!nav.ships.is_empty(), "F688 spawns traffic on system entry");
    assert!(nav.ships.iter().any(|ship| ship.role == Role::Trader));
    assert!(nav.ships.iter().any(|ship| ship.role == Role::Pirate));
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_landed_ship_is_not_a_target() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    // Put the player on a planet's surface: F1021's rough touch is 0x32 and
    // F1047's guarded state is 0x34.
    let planet = nav
        .bodies
        .iter()
        .position(|body| matches!(body.kind, Kind::Planet))
        .expect("Hope");
    assert!(nav.land(planet));
    assert!(nav.landed_view(), "0x32/0x34 is the landed view");
    assert!(!nav.on_pad(), "a surface landing is not the new-game pad");

    // Spawn F688's capture traffic with a pirate right beside the landed ship
    // and run the same 60 s. The landed guard skips the tick entirely.
    let seat = nav.new_game.unwrap().seat;
    nav.respawn_traffic(&cache, seat, [0.0, 0.0, 1.0]);
    let pirate = nav
        .ships
        .iter_mut()
        .find(|ship| ship.role == Role::Pirate)
        .expect("the capture guarantee spawned a pirate");
    pirate.position = seat.map(|v| v + 200_000);
    pirate.state = State::Attack;
    let hits = run_frames(&mut nav, seat, 10_000, 3_000);
    assert!(hits.is_empty(), "a landed ship took fire: {hits:?}");
}

/// A pirate spawned for a real flight entry is inside F931's range and fires
/// once the AI turns; this is the playable combat path that must keep working
/// (only the boot and landed cases are excluded).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn flight_traffic_still_closes_and_fires() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let seat = nav.new_game.unwrap().seat;
    let forward = nav.bodies[nav.docked.unwrap()].orientation[2];
    nav.respawn_traffic(&cache, seat, forward);
    let pirate = nav
        .ships
        .iter()
        .find(|ship| ship.role == Role::Pirate)
        .expect("the capture guarantee spawned a pirate");
    assert!(distance(pirate.position, seat) < ffe_core::laser::RANGE);
    // The player has launched: the port clears `docked` when F1031 releases the
    // bay, so F994's pad state is no longer active.
    nav.docked = None;
    assert!(!nav.landed_view());
    let hits = run_frames(&mut nav, seat, 10_000, 600);
    assert!(
        hits.iter()
            .any(|event| matches!(event, ShipEvent::Hit { .. })),
        "the in-flight pirate never fired: {hits:?}"
    );
}

/// The boot frames and the message log: F1050/F1054's landed view on Hope with
/// the start line only, before and after a minute of game time. The log is the
/// port's ring; main pushes the "Landed at Old Blackelk." line on the first
/// frame and nothing else unless a ship hits.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn boot_frame_and_log_stay_clean() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::Flight;
    use ffe_rs::hud::PlayerHud;
    use ffe_rs::model_viewer::ModelViewer;
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, Screen, SystemStation};

    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let start = nav.new_game.expect("the new game");
    let index = nav.docked.expect("the pad");
    let body = nav.bodies[index].clone();
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start.seat.map(|v| v as f64 / UNIT),
        ..Flight::default()
    };
    state.axes = Some(body.orientation);
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.discs = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Star | Kind::Planet | Kind::Moon))
        .map(|b| {
            let length = b.position.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
            Disc {
                centre: b.position.map(|v| v as f64 / UNIT),
                radius: if b.kind == Kind::Star {
                    b.radius as f64 / UNIT
                } else {
                    b.inner_radius as f64 / UNIT
                },
                light: b.position.map(|v| -(v as f64) / length.sqrt().max(1.0)),
                colours: (12, 4),
                star: b.kind == Kind::Star,
                model: Some((b.model, b.seed)),
                orientation: b.orientation,
            }
        })
        .collect();
    let tics = ffe_core::model_animation::START_TICS;
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Station | Kind::Starport))
        .map(|b| SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            door: 0xffff,
            tics,
        })
        .collect();
    screen.sky = ffe_rs::prefs::space_colour_rgb(4);
    let parent = body.parent.and_then(|p| nav.bodies.get(p)).unwrap();
    screen.hud = PlayerHud {
        set_speed: Some(0.0),
        actual_speed: 0.0,
        altitude: Some(0),
        registration: parent.name.clone(),
        ..PlayerHud::default()
    };
    screen.clock = nav.date_text();
    screen
        .messages
        .push("Landed at Old Blackelk.", screen.clock.clone());
    let draw = |screen: &mut Screen, message: &str| {
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            true,
            message,
        );
    };
    draw(&mut screen, "");
    save(&screen, "port-hope-boot-0000.ppm");
    let hits = run_frames(&mut nav, start.seat, 10_000, 3_000);
    assert!(hits.is_empty(), "the boot pad was attacked: {hits:?}");
    screen.clock = nav.date_text();
    draw(&mut screen, "");
    save(&screen, "port-hope-boot-60s.ppm");
    let entries = screen.messages.entries();
    assert_eq!(entries.len(), 1, "the log gained an entry: {entries:?}");
    assert_eq!(entries[0].text, "Landed at Old Blackelk.");
    assert_eq!(entries[0].stamp, "1-Jan-3250 12:00:00");
    println!(
        "boot log after 60 s: {:?}",
        entries
            .iter()
            .map(|entry| (&entry.stamp, &entry.text))
            .collect::<Vec<_>>()
    );
}

fn save(screen: &ffe_rs::screen::Screen, name: &str) {
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for pixel in screen.logical_pixels() {
            ppm.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
    }
}
