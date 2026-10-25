//! Local-only sequence for the NPC flight/combat AI: a pirate fires its 1MW
//! pulse at the player (F931/F930/F953), the player's hull takes the damage, and
//! a clean kill makes the police hostile (the port's `DATA_008647` response).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! npc_ai -- --ignored --nocapture`
//!
//! Frames (when `FFE_SHOT_DIR` is set): `npc-ai-before.ppm` (the pirate in front
//! of the player), `npc-ai-hit.ppm` (the damaged player), `npc-ai-police.ppm`
//! (the wanted player and the hostile police) and `npc-ai-restored.ppm` (a pirate
//! carried through a jump).

use ffe_core::laser;
use ffe_core::npc::Role;
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, ShipEvent, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemShip};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn save(screen: &Screen, name: &str) {
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for pixel in screen.logical_pixels() {
            ppm.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
    v.map(|x| x / length)
}

fn look_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let forward = normalise(std::array::from_fn(|i| to[i] - from[i]));
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    [right, up, forward]
}

fn ships(screen: &mut Screen, nav: &Navigator) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.ships = nav
        .ships
        .iter()
        .map(|ship| SystemShip {
            model: ship.model,
            centre: ship.position.map(|v| v as f64 / unit),
            axes: ship.axes(),
            flash: ship.flash > 0.0,
            dead: !ship.alive(),
        })
        .collect();
}

/// A frame of the player at `player` (system mm) looking at the pirate.
fn frame(
    screen: &mut Screen,
    viewer: &ModelViewer,
    state: &Flight,
    nav: &Navigator,
    message: &str,
) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    ships(screen, nav);
    screen.labels = nav
        .ships
        .iter()
        .map(|ship| ffe_rs::screen::BodyLabel {
            name: ship.name.clone(),
            centre: ship.position.map(|v| v as f64 / unit),
        })
        .collect();
    screen.lasers = [laser::PULSE_1MW, 0, 0, 0];
    let mut views = ViewState::default();
    views.console.labels = true;
    screen.draw(state, views, Some(viewer), false, true, message);
}

fn setup(cache: &ffe_content::VerifiedCache) -> (Navigator, ModelViewer, Screen, Flight) {
    let nav = Navigator::new_game(cache, 1).unwrap();
    let viewer = ModelViewer::new(cache).unwrap();
    let screen = Screen::new(cache).unwrap();
    assert!(!nav.ships.is_empty(), "the new game spawned no traffic");
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    // The player sits 300 km in front of the pirate, looking back at it. The
    // pirate's default forward is +z, so it is already aimed at the player: its
    // first F931 shot lands without a turn. (The port's renderer draws NPC models
    // in flight units, so the evidence frame needs the ship this close.)
    let pirate = nav.ships.iter().find(|s| s.role == Role::Pirate).unwrap();
    let at = pirate.position;
    let player = [at[0], at[1], at[2] + 80_000];
    let mut state = Flight {
        mode: Mode::Flight,
        position: player.map(|v| v as f64 / unit),
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some(look_at(state.position, at.map(|v| v as f64 / unit)));
    (nav, viewer, screen, state)
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_pirate_fires_and_the_player_takes_f953_damage() {
    let cache = cache();
    let (mut nav, viewer, mut screen, mut state) = setup(&cache);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let player = state.position.map(|v| (v * unit).round() as i64);
    let radius = viewer.model_radius(NEW_GAME_SHIP).unwrap();
    let radius = i64::from(radius);
    // The player's hull starts untracked; the first hit records mass * 4.
    let mass = viewer.ship_stats(NEW_GAME_SHIP).unwrap().mass;

    frame(&mut screen, &viewer, &state, &nav, "A pirate closes.");
    save(&screen, "npc-ai-before.ppm");

    let mut first: Option<(String, u32, bool, laser::Damage)> = None;
    'frames: for _ in 0..600 {
        let events = nav.tick_ships(0.02, 994, player, radius);
        for event in events {
            if let ShipEvent::Hit {
                name, damage, beam, ..
            } = event
            {
                let outcome = state.take_damage(damage, mass);
                let message = format!("{name} hits: hull {}%", state.hull[0]);
                screen.hud.hull = state.hull;
                frame(&mut screen, &viewer, &state, &nav, &message);
                save(&screen, "npc-ai-hit.ppm");
                first = Some((name, damage, beam, outcome));
                break 'frames;
            }
        }
    }
    let (name, damage, beam, outcome) = first.expect("the pirate never hit the player");
    assert_eq!(
        damage,
        50,
        "{name} fired a {} laser",
        if beam { "beam" } else { "pulse" }
    );
    assert!(!beam);
    assert_eq!(outcome, laser::Damage::Hull);
    let full = mass * 4;
    assert_eq!(state.hull, [full - 50, full]);
    // A second 1MW pulse uses up a 25 t hull (F953 then F942).
    assert_eq!(state.take_damage(50, mass), laser::Damage::Destroyed);
    assert!(state.destroyed());
    // The hull line the HUD draws is the damaged one, not the model's default.
    screen.hud.hull = [0, full];
    frame(&mut screen, &viewer, &state, &nav, "Hull damaged.");
    let damaged = screen.logical_pixels().to_vec();
    screen.hud.hull = [full, full];
    frame(&mut screen, &viewer, &state, &nav, "Hull damaged.");
    let repaired = screen.logical_pixels().to_vec();
    assert!(
        damaged != repaired,
        "the damaged hull did not reach the HUD"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_clean_kill_makes_the_police_hostile() {
    let cache = cache();
    let (mut nav, viewer, mut screen, mut state) = setup(&cache);
    // A clean trader: the F931 crime flag and the wanted state.
    let trader = nav
        .ships
        .iter()
        .find(|ship| ship.role == Role::Trader)
        .expect("no trader");
    let id = trader.id;
    let name = trader.name.clone();
    assert!(!nav.legal.wanted);
    let attack = nav.player_attack(id, 10_000).expect("the trader");
    assert!(attack.crime);
    assert!(attack.destroyed);
    assert!(nav.legal.wanted);
    assert_eq!(nav.legal.offences, 1);
    // Police and military ships turn hostile; the pirates were already so.
    let police: Vec<_> = nav
        .ships
        .iter()
        .filter(|ship| ship.role == Role::Police)
        .collect();
    assert!(!police.is_empty(), "no police in the system");
    assert!(
        police.iter().all(|ship| ship.is_hostile()),
        "the police did not react to {name}"
    );
    assert!(police
        .iter()
        .all(|ship| ship.state == ffe_core::npc::State::Attack));
    assert!(attack.police);
    // A pirate is not a clean ship: attacking it is not a crime (F931's illegal
    // fire flag is the port's `legal.wanted`, which is already set here).
    let pirate = nav
        .ships
        .iter()
        .find(|ship| ship.role == Role::Pirate)
        .map(|ship| ship.id);
    if let Some(id) = pirate {
        let pirate_hit = nav.player_attack(id, 10_000).unwrap();
        assert!(!pirate_hit.crime, "a pirate is not a clean ship");
    }
    // The police frame: labels show the hostile ships and the HUD the wanted
    // state is reported in the message line.
    // The police frame: put the camera beside a police ship (the traffic spawned
    // around the station, not around the pirate the first test used).
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let police_at = nav
        .ships
        .iter()
        .find(|ship| ship.role == Role::Police)
        .map(|ship| ship.position)
        .unwrap();
    state.position = [police_at[0], police_at[1], police_at[2] + 250_000].map(|v| v as f64 / unit);
    state.axes = Some(look_at(state.position, police_at.map(|v| v as f64 / unit)));
    let message = "WANTED: police are responding.";
    frame(&mut screen, &viewer, &state, &nav, message);
    save(&screen, "npc-ai-police.ppm");
    let with = screen.logical_pixels().to_vec();
    screen.ships.clear();
    screen.labels.clear();
    screen.draw(
        &state,
        ViewState::default(),
        Some(&viewer),
        false,
        true,
        message,
    );
    assert!(
        with != screen.logical_pixels(),
        "the police traffic did not draw"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_pirate_close_to_the_player_follows_a_jump() {
    let cache = cache();
    let (mut nav, viewer, mut screen, mut state) = setup(&cache);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let player = state.position.map(|v| (v * unit).round() as i64);
    // Bring a pirate within F675's carry range.
    let pirate = nav
        .ships
        .iter_mut()
        .find(|ship| ship.role == Role::Pirate)
        .expect("no pirate");
    let name = pirate.name.clone();
    let model = pirate.model;
    pirate.position = [player[0], player[1], player[2] + 1_000_000];
    nav.tick_ships(0.02, 994, player, 1_000);
    // Jump to the next system in the list and enter it (F680).
    assert!(nav.system_count() > 1, "only one system");
    let next = (nav.current_system() + 1) % nav.system_count();
    let arrival = nav.jump(&cache, next).unwrap();
    nav.spawn_traffic(&cache, arrival, [0.0, 0.0, 1.0]);
    let restored = nav
        .ships
        .iter()
        .find(|ship| ship.role == Role::Pirate && ship.name == name);
    assert!(restored.is_some(), "{name} did not follow the jump");
    let restored = restored.unwrap();
    assert_eq!(restored.model, model);
    assert!(restored.is_hostile());
    let at = restored.position;
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    state.position = [at[0], at[1], at[2] + 80_000].map(|v| v as f64 / unit);
    state.axes = Some(look_at(state.position, at.map(|v| v as f64 / unit)));
    let message = "The pirate followed the jump.";
    frame(&mut screen, &viewer, &state, &nav, message);
    save(&screen, "npc-ai-restored.ppm");
}
