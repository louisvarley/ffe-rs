//! Local-only: F690's Thargoid spawn, its DATA_006197 model statistics, and a
//! player pulse-laser hit/kill sequence with F1099's readout and F942's death.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! thargoid_view -- --ignored --nocapture`
//!
//! The frame pins model 0x31 (the first DATA_006197 warship) so the evidence is
//! stable; the spawn itself goes through `spawn_thargoid_traffic`, which is
//! F690 with F35(0x16) = 12 (F688 draws RND(12), so the system gets one or
//! more of the twelve possible).

use ffe_core::laser;
use ffe_core::npc::{self, Role};
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
fn a_thargoid_spawns_from_f690_and_dies_to_the_players_laser() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();

    // F690's count is RND(F35(0x16)); the mission scripts that set the limit are
    // not ported, so this hook is the only way an ordinary system gets one.
    assert!(nav.spawn_thargoid_traffic(&cache, [0, 0, 0], [0.0, 0.0, 1.0], 12));
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Thargoid)
        .expect("F690 spawned no Thargoid");
    let id = nav.ships[index].id;
    // DATA_006197's first entry, model 0x31: pin it so the frame is stable.
    let spec = ffe_rs::ships::npc_spec(&cache, 0x31).expect("model 0x31 has no statistics");
    assert_eq!(spec.name, "Unknown");
    assert_eq!(spec.mass, 232);
    // The statistics' +0xc shield word (the +0xe word is the name code
    // 0x402e, which the old port misread as 16,430).
    assert_eq!(spec.shield_strength, 180);
    assert_eq!(spec.thrust, 14_715);
    assert_eq!(spec.drive, 14);
    assert_eq!(spec.gun_mounts, 1);
    assert_eq!(spec.cargo, 203);
    assert_eq!(spec.radius, 90_000);
    assert_eq!(spec.hull(), 928);
    // Spec::shield_max is the port's approximation of F773's random*16: the
    // statistics shield word's low byte times four, now read from +0xc.
    assert_eq!(spec.shield_max(), 720);
    // F774 with class 0x11's 0xb2 proportion and 203 t of cargo picks the 75 t
    // 20MW beam (0xb3), not the 900 t 0xcf entry.
    assert_eq!(
        npc::f774_laser(
            Role::Thargoid.ai_class(),
            u32::from(spec.cargo),
            &mut npc::Random::new(1, 2)
        )
        .unwrap()
        .0,
        laser::BEAM_20MW
    );

    nav.ships.retain(|ship| ship.role == Role::Thargoid);
    let target = {
        let ship = nav.ships.iter_mut().find(|ship| ship.id == id).unwrap();
        ship.model = spec.model;
        ship.radius = spec.radius;
        ship.name = format!("{} 1", spec.name);
        ship.shield_strength = spec.shield_strength as i16;
        ship.position = [0, 0, 500_000];
        ship.velocity = [0.0; 3];
        ship.damage = laser::DamageState {
            shields: spec.shield_max(),
            shields_max: spec.shield_max(),
            hull: spec.hull(),
            mass: spec.mass,
        };
        ship.position
    };
    nav.lock_ship(id);

    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let from = [0.0, 0.0, 0.0];
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
    screen.lasers = Combat::new_game().lasers;
    ships(&mut screen, &nav);
    target_readout(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "thargoid-before.ppm");
    let before = screen.logical_pixels().to_vec();

    let dt = 1.0 / 60.0;
    let tics = combat::frame_tics(dt, 1.0);
    assert_eq!(tics, 828);
    let mut combat = Combat::new_game();
    let mut shots = 0usize;
    let mut shield_hit = false;
    let mut hull_hit = false;
    for frame in 0..1600 {
        combat.tick(tics, dt);
        let Ok(shot) = combat.fire(0, tics) else {
            continue;
        };
        let beam = combat::beam(
            &state,
            viewer.ship_stats(state.ship).as_ref(),
            views.mode,
            [views.turret.yaw, views.turret.pitch],
        );
        let origin = beam.origin.map(|v| (v * unit).round() as i64);
        let hit = laser::hit(origin, beam.axes[2], &targets(&nav))
            .unwrap_or_else(|| panic!("shot {frame} missed the Thargoid"));
        assert_eq!(
            hit.index,
            nav.bodies.len(),
            "shot {frame} hit something else"
        );
        nav.lock_ship(id);
        let attack = nav
            .damage_ship_at(id, shot.damage, Some(hit.point))
            .expect("no F953 outcome");
        shots += 1;
        ships(&mut screen, &nav);
        target_readout(&mut screen, &nav);
        screen.draw(&state, views, Some(&viewer), true, "");
        if !shield_hit && attack.damage == laser::Damage::Shields {
            shield_hit = true;
            save(&screen, "thargoid-hit-shields.ppm");
            assert!(
                before
                    .iter()
                    .zip(screen.logical_pixels())
                    .filter(|(a, b)| a != b)
                    .count()
                    > 20,
                "the Thargoid did not draw"
            );
        } else if !hull_hit && attack.damage == laser::Damage::Hull {
            hull_hit = true;
            save(&screen, "thargoid-hit-hull.ppm");
        }
        if attack.damage == laser::Damage::Destroyed {
            save(&screen, "thargoid-kill.ppm");
            break;
        }
    }
    assert!(shield_hit && hull_hit, "the kill skipped a damage stage");
    // 720 shields and a 928 hull at 50 damage a shot: fourteen shield hits, a
    // fifteenth that spills thirty into the hull, then eighteen more hull hits,
    // thirty-eight frames apart.
    assert_eq!(shots, 33);
    let ship = nav.ships.iter().find(|ship| ship.id == id).unwrap();
    assert!(!ship.alive());
    assert_eq!(ship.state, ffe_core::npc::State::Dead);
    assert_eq!(ship.damage.shields, 0);
    assert_eq!(ship.damage.hull, 0);
    assert_eq!(ship.hull_per_mille(), 0);
    assert!(!nav.effects.is_empty(), "F942 drew no explosion");
    // F942's explosion is tiny at the firing range; show it from about 40 m.
    let centre = target.map(|v| v as f64 / unit);
    let death_from = [centre[0], centre[1], centre[2] - 2.0];
    let mut death_state = Flight {
        mode: Mode::Flight,
        position: death_from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    death_state.axes = Some(look_at(death_from, centre));
    ships(&mut screen, &nav);
    target_readout(&mut screen, &nav);
    screen.draw(&death_state, views, Some(&viewer), true, "");
    save(&screen, "thargoid-death.ppm");
}
