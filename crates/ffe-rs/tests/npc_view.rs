//! Local-only frames and checks for the NPC traffic: a system with traffic, a
//! shield hit (F1536's model 0x9d) and a kill (F942).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! npc_view -- --ignored --nocapture`

use ffe_core::laser;
use ffe_core::npc::Role;
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
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

fn changed(before: &[u32], after: &[u32]) -> usize {
    before.iter().zip(after).filter(|(a, b)| a != b).count()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / length)
}

/// The ship axes that look from `from` at `to` (flight units).
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

/// The pirate is the traffic ship away from the station, so the frame is the
/// ship rather than the starport behind a trader.
fn setup(
    cache: &ffe_content::VerifiedCache,
) -> (Navigator, ModelViewer, Screen, Flight, ViewState, usize) {
    let nav = Navigator::new_game(cache, 1).unwrap();
    let viewer = ModelViewer::new(cache).unwrap();
    let screen = Screen::new(cache).unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .unwrap_or(0);
    let target = nav.ships[index].position;
    // Close enough for the model to be more than a pixel: 200 m behind it,
    // looking along +z at it.
    let from_mm = [target[0], target[1], target[2] - 200_000];
    let from = from_mm.map(|v| v as f64 / unit);
    let mut state = Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some(look_at(from, target.map(|v| v as f64 / unit)));
    (nav, viewer, screen, state, ViewState::default(), index)
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn system_traffic_draws_ships_names_hit_and_kill() {
    let cache = cache();
    let (mut nav, viewer, mut screen, state, views, index) = setup(&cache);
    assert!(!nav.ships.is_empty(), "new game spawned no traffic");
    assert!(nav.ships.len() <= ffe_core::npc::MAX_SHIPS);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;

    // The name label of the target ship (F1108) and the target readout.
    let ship_name = nav.ships[index].name.clone();
    let ship_id = nav.ships[index].id;
    nav.lock_ship(ship_id);
    {
        let ship = nav.locked_ship().unwrap();
        screen.labels = vec![ffe_rs::screen::BodyLabel {
            name: ship_name.clone(),
            centre: ship.position.map(|v| v as f64 / unit),
        }];
        screen.target_damage = Some(ffe_rs::screen::TargetDamage {
            hull: ship.hull_per_mille(),
            shields: Some(ship.shield_percent()),
            shield_strength: ship.shield_strength,
        });
    }
    screen.target_hud = Some(ffe_rs::screen::TargetHud {
        x: 160,
        y: 79,
        value: 1,
        au: false,
        behind: false,
        offscreen: false,
    });
    screen.lasers = [laser::PULSE_1MW, 0, 0, 0];
    ships(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), false, true, "");
    save(&screen, "npc-system.ppm");
    let with_ships = screen.logical_pixels().to_vec();

    // Without the ships the view is empty sky plus labels/HUD; the ships add a
    // lot of pixels.
    screen.ships.clear();
    screen.draw(&state, views, Some(&viewer), false, true, "");
    let empty = screen.logical_pixels().to_vec();
    assert!(
        changed(&with_ships, &empty) > 50,
        "the traffic did not draw"
    );
    ships(&mut screen, &nav);

    // F1536: a shield hit draws model 0x9d over the ship.
    let ship = nav.ships.get_mut(index).unwrap();
    ship.damage.shields = 100;
    ship.damage.shields_max = 100;
    ship.damage.hull = 100;
    assert_eq!(ship.hit(50), laser::Damage::Shields);
    assert!(ship.flash > 0.0);
    ships(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), false, true, "");
    save(&screen, "npc-hit.ppm");
    let hit_frame = screen.logical_pixels().to_vec();
    screen.ships[index].flash = false;
    screen.draw(&state, views, Some(&viewer), false, true, "");
    let no_flash = screen.logical_pixels().to_vec();
    assert!(
        changed(&hit_frame, &no_flash) > 3,
        "the model-157 shield flash did not draw"
    );
    screen.ships[index].flash = true;

    // The laser path reaches the ship and F953's words take the damage.
    let target = nav.ships[index].position;
    let from = [target[0], target[1], target[2] - 200_000];
    let targets = vec![laser::Target {
        index: 0,
        centre: target,
        radius: nav.ships[index].radius,
    }];
    let ray = laser::hit(from, [0.0, 0.0, 1.0], &targets).expect("ray hit");
    assert_eq!(ray.index, 0);
    assert_eq!(nav.damage_ship(ship_id, 50), Some(laser::Damage::Shields));

    // F953/F942: a hit at least the remaining hull destroys the ship; the port
    // keeps a short death marker (F942's particles are not ported).
    let ship = nav.ships.get_mut(index).unwrap();
    assert_eq!(ship.hit(1000), laser::Damage::Destroyed);
    assert!(!ship.alive());
    ships(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), false, true, "");
    save(&screen, "npc-kill.ppm");
    let kill_frame = screen.logical_pixels().to_vec();
    assert!(
        changed(&hit_frame, &kill_frame) > 3,
        "the death frame did not differ from the ship and its shield flash"
    );
    // The death marker is still F1536's model drawn where the ship was.
    screen.ships.clear();
    screen.draw(&state, views, Some(&viewer), false, true, "");
    assert!(
        changed(&kill_frame, screen.logical_pixels()) > 3,
        "the death marker did not draw"
    );
    // The marker's timer is what removes the slot (F927's recycle).
    screen.ships = Vec::new();
    for _ in 0..40 {
        nav.tick_ships(0.02, from);
    }
    assert!(nav.ships.iter().all(|ship| ship.id != ship_id));
}
