//! Local-only: a deterministic 60 Hz fire sequence through F931/F930/F953 at
//! an NPC, with the crosshair, the hit flash and F1099's target readout.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! laser_view -- --ignored --nocapture`

use ffe_core::laser;
use ffe_core::npc::Role;
use ffe_rs::camera::ViewState;
use ffe_rs::combat::{self, Combat};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemShip, TargetDamage};

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
    screen.effects = nav
        .effects
        .iter()
        .map(|effect| ffe_rs::screen::SystemEffect::from_effect(None, effect, unit))
        .collect();
}

fn target_readout(screen: &mut Screen, nav: &Navigator) {
    if let Some(ship) = nav.locked_ship() {
        screen.target_damage = Some(TargetDamage {
            hull: ship.hull_per_mille(),
            shields: Some(ship.shield_percent()),
            shield_strength: ship.shield_strength,
        });
    }
}

/// Same target list main builds: the nav bodies first, then the live ships.
fn targets(nav: &Navigator) -> Vec<laser::Target> {
    let body_count = nav.bodies.len();
    let mut targets: Vec<laser::Target> = nav
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.radius > 0)
        .map(|(index, body)| laser::Target {
            index,
            centre: body.position,
            radius: body.radius,
        })
        .collect();
    targets.extend(
        nav.ships
            .iter()
            .enumerate()
            .filter(|(_, ship)| ship.visible())
            .map(|(index, ship)| ship.target(body_count + index)),
    );
    targets
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_60hz_pulse_sequence_aims_fires_and_damages_an_npc() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .expect("the new game spawned no pirate");
    let target = nav.ships[index].position;
    let id = nav.ships[index].id;
    // 200 m behind the pirate, looking at it: the pirated ship fills the
    // crosshair like the live `--capture-traffic` view but close enough that the
    // model and the shield flash are visible.
    let from = [target[0], target[1], target[2] - 200_000].map(|v| v as f64 / unit);
    let mut state = Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some(look_at(from, target.map(|v| v as f64 / unit)));
    let views = ViewState::default();
    // F953's words: 100 shields (max 100) over a 25 t hull (100), so the three
    // 50-damage pulse shots read shields -> shields -> hull.
    let ship = nav.ships.get_mut(index).unwrap();
    ship.damage = laser::DamageState {
        shields: 100,
        shields_max: 100,
        hull: 100,
        mass: 25,
    };
    nav.lock_ship(id);
    screen.lasers = Combat::new_game().lasers;
    ships(&mut screen, &nav);
    target_readout(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "laser-aim-before.ppm");
    let before = screen.logical_pixels().to_vec();

    // 60 Hz, 1x: DATA_009155 is 828 tics a frame. The 1MW pulse's 0x795c refire
    // clears after 38 frames, so 80 frames carry three shots (0, 38, 76).
    let dt = 1.0 / 60.0;
    let tics = combat::frame_tics(dt, 1.0);
    assert_eq!(tics, 828);
    let mut combat = Combat::new_game();
    let stats = viewer.ship_stats(state.ship);
    let mut shot_frames: Vec<usize> = Vec::new();
    let mut outcomes = Vec::new();
    let mut ray_changed = 0;
    for frame in 0..80 {
        combat.tick(tics, dt);
        let Ok(shot) = combat.fire(0, tics) else {
            continue;
        };
        let beam = combat::beam(
            &state,
            stats.as_ref(),
            views.mode,
            [views.turret.yaw, views.turret.pitch],
        );
        let origin = beam.origin.map(|v| (v * unit).round() as i64);
        let hit = laser::hit(origin, beam.axes[2], &targets(&nav))
            .unwrap_or_else(|| panic!("shot {frame} missed the pirate"));
        assert_eq!(
            hit.index,
            nav.bodies.len() + index,
            "shot {frame} hit something else"
        );
        // The ray leaves along the view's mount axis (the ship's nose here), not
        // at the crosshair's 1 px art offset.
        assert_eq!(beam.axes[2], state.axes.unwrap()[2]);
        nav.lock_ship(id);
        let attack = nav
            .damage_ship_at(id, shot.damage, Some(hit.point))
            .expect("no F953 outcome");
        outcomes.push(attack.damage);
        shot_frames.push(frame);

        ships(&mut screen, &nav);
        target_readout(&mut screen, &nav);
        // F1776's model 0x9c is at the gun behind the front eye: the front frame
        // must be the same with or without it (the original's near-plane cull).
        screen.beam = Some(beam);
        screen.draw(&state, views, Some(&viewer), true, "");
        save(&screen, &format!("laser-shot-{}.ppm", shot_frames.len()));
        let with_beam = screen.logical_pixels().to_vec();
        screen.beam = None;
        screen.draw(&state, views, Some(&viewer), true, "");
        ray_changed += changed(&with_beam, screen.logical_pixels());
        save(&screen, &format!("laser-hit-{}.ppm", shot_frames.len()));
        if shot_frames.len() == 1 {
            assert!(
                changed(&before, screen.logical_pixels()) > 20,
                "the first hit frame did not change the view"
            );
        }
    }
    assert_eq!(shot_frames, [0, 38, 76]);
    assert_eq!(
        outcomes,
        [
            laser::Damage::Shields,
            laser::Damage::Shields,
            laser::Damage::Hull
        ]
    );
    // F1776's model 0x9c draws the ray from the gun to the target (global 37,
    // F931's length) in the front view; its muzzle sits behind the eye.
    assert!(ray_changed > 0, "the front view did not draw the ray");
    let ship = nav.ships.iter().find(|ship| ship.id == id).unwrap();
    assert_eq!(ship.damage.shields, 0);
    assert_eq!(ship.damage.hull, 50);
    assert_eq!(ship.shield_percent(), 0);
    assert_eq!(ship.hull_per_mille(), 500);
    assert!(ship.flash > 0.0, "the shield hit did not set F1536's flash");
}
