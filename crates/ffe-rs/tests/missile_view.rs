//! Local-only frames for the missile path: lock, launch, F953 shield hit, F942
//! kill, and F1114's combat-target circle.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! missile_view -- --ignored --nocapture`
use ffe_core::laser;
use ffe_core::missile::PLAYER_MISSILE;
use ffe_core::npc::Role;
use ffe_rs::camera::ViewState;
use ffe_rs::combat::Combat;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::missile::{MissileEvent, MissileField};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemMissile, SystemShip};

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

/// The player's axes looking from `from` at `to` (flight units).
fn look_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let forward = normalise(std::array::from_fn(|i| to[i] - from[i]));
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    [right, up, forward]
}

fn draw(screen: &mut Screen, nav: &Navigator, state: &Flight, viewer: &ModelViewer, lock: bool) {
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
    screen.missiles = nav
        .missiles
        .missiles
        .iter()
        .filter(|missile| missile.alive())
        .map(|missile| SystemMissile {
            model: missile.spec.model,
            centre: missile.position.map(|v| v as f64 / unit),
            axes: missile.axes,
        })
        .collect();
    screen.lasers = state.combat.lasers;
    screen.missile_lock = lock && nav.missile_lock.is_some();
    screen.missile_blink = true;
    screen.draw(state, ViewState::default(), Some(viewer), false, true, "");
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn lock_launch_hit_and_kill_with_frames() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .unwrap_or(0);
    let target = nav.ships[index].position;
    let target_id = nav.ships[index].id;
    // Inside F758's 131 m proximity fuse, 120 m behind the ship.
    let from_mm = [target[0], target[1], target[2] - 120_000];
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
    state.combat = Combat::new_game();

    // Lock and arm (the M key uses DATA_008875; Shift+T sets it).
    nav.lock_ship(target_id);
    nav.missile_lock = Some(target_id);
    // Make the first hit a shield hit so the second one can be the kill.
    nav.ships[index].damage.shields = 300;
    nav.ships[index].damage.shields_max = 300;
    nav.ships[index].damage.hull = 100;

    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let launcher_radius = ffe_core::objects::ModelInfo::read(records, stats, state.ship)
        .map_or(0, |info| info.radii().1);
    let launch = |nav: &mut Navigator, state: &mut Flight| {
        let (pylon, code) = state.combat.first_missile().expect("a missile");
        let model = ffe_core::missile::model(code).unwrap();
        let missile_radius = ffe_core::objects::ModelInfo::read(records, stats, model)
            .map_or(0, |info| info.radii().1);
        let spec = MissileField::spec(code, viewer.ship_stats(model), missile_radius).unwrap();
        assert_eq!(code, PLAYER_MISSILE);
        assert_eq!(spec.model, 6);
        assert_eq!(spec.damage, 200);
        assert!(state.combat.take_missile(pylon).is_some());
        nav.launch_missile(
            spec,
            code,
            0,
            target_id,
            from_mm,
            state.axes.unwrap(),
            pylon,
            launcher_radius,
        );
    };

    launch(&mut nav, &mut state);
    assert_eq!(state.combat.missile_count(), 1);
    assert_eq!(nav.missiles.missiles.len(), 1);
    // One flight step after launch, for the frame: the missile is still inside
    // the fuse radius but far enough from the eye to draw.
    nav.missiles.missiles[0].position[2] += 60_000;
    draw(&mut screen, &nav, &state, &viewer, true);
    save(&screen, "missile-launch.ppm");
    let launch_frame = screen.logical_pixels().to_vec();
    // The missile object draws through the same renderer as the ships: without
    // it the frame is different.
    screen.missiles.clear();
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    assert!(
        changed(&launch_frame, screen.logical_pixels()) > 0,
        "the missile object did not draw"
    );
    draw(&mut screen, &nav, &state, &viewer, true);

    // F1114: the combat-target circle at the view centre.
    draw(&mut screen, &nav, &state, &viewer, false);
    let no_lock = screen.logical_pixels().to_vec();
    draw(&mut screen, &nav, &state, &viewer, true);
    assert!(
        changed(&launch_frame, &no_lock) > 0,
        "the missile lock circle did not draw"
    );

    // The first F758 guidance update (0.4 s) reaches the 131 m fuse: F953's
    // shields first.
    let events = nav.tick_missiles(ffe_core::missile::GUIDANCE_INTERVAL, from_mm);
    assert!(matches!(
        events.as_slice(),
        [MissileEvent::Hit {
            outcome: laser::Damage::Shields,
            ..
        }]
    ));
    assert!(nav.missiles.missiles.is_empty(), "the missile was consumed");
    assert_eq!(nav.ships[index].damage.shields, 100);
    assert_eq!(nav.ships[index].damage.hull, 100);
    draw(&mut screen, &nav, &state, &viewer, true);
    save(&screen, "missile-hit.ppm");
    let hit_frame = screen.logical_pixels().to_vec();
    assert!(changed(&launch_frame, &hit_frame) > 0);

    // The second missile: 200 > the 100 shields and the 100 hull, so F942.
    launch(&mut nav, &mut state);
    assert_eq!(state.combat.missile_count(), 0);
    let events = nav.tick_missiles(ffe_core::missile::GUIDANCE_INTERVAL, from_mm);
    assert!(matches!(
        events.as_slice(),
        [MissileEvent::Hit {
            outcome: laser::Damage::Destroyed,
            ..
        }]
    ));
    assert!(!nav.ships[index].alive());
    draw(&mut screen, &nav, &state, &viewer, true);
    save(&screen, "missile-kill.ppm");
    let kill_frame = screen.logical_pixels().to_vec();
    assert!(changed(&hit_frame, &kill_frame) > 0);
    assert_eq!(state.combat.first_missile(), None);
}
