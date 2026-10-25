//! Local-only frames and checks for the hit particles, explosion and debris
//! (F939/F942/F926), plus the model-155 fragment gate.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! effects_view -- --ignored --nocapture`

use ffe_core::effects::{self, Kind};
use ffe_core::laser;
use ffe_core::npc::Role;
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::{AnimationInputs, ModelViewer};
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemEffect, SystemShip};

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

fn stage(screen: &mut Screen, nav: &Navigator) {
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
        .map(|effect| SystemEffect {
            model: effect.model,
            centre: effect.position.map(|v| v as f64 / unit),
            axes: effect.axes(),
            mass: if effect.kind == Kind::Explosion {
                effects::EXPLOSION_MASS
            } else {
                0
            },
        })
        .collect();
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn hit_particle_explosion_and_frames() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    // F829/F998's new game has no traffic; the capture asks F688 for the
    // guaranteed pirate at the station's launch point.
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.respawn_traffic(&cache, at, forward);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .unwrap_or(0);
    let target = nav.ships[index].position;
    // 20 m from the target, looking along +z at it.
    let from_mm = [target[0], target[1], target[2] - 20_000];
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
    let views = ViewState::default();
    nav.lock_ship(nav.ships[index].id);
    stage(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    let empty = screen.logical_pixels().to_vec();

    // F939: one model-0x9e particle at the hit point.
    let id = nav.ships[index].id;
    // F939 spawns the particle at F817's hit point on the interaction box, not
    // at the ship's centre: put it a few metres off the hull.
    let hit_point = [target[0], target[1] + 2_000, target[2]];
    assert_eq!(
        nav.damage_ship_at(id, 50, Some(hit_point))
            .map(|attack| attack.damage),
        Some(laser::Damage::Shields)
    );
    assert_eq!(
        nav.effects
            .iter()
            .filter(|effect| effect.kind == Kind::Hit)
            .count(),
        1
    );
    let particle = nav.effects.iter().find(|e| e.kind == Kind::Hit).unwrap();
    assert_eq!(particle.model, effects::HIT_MODEL);
    // The hit particle sits on the ship's hull and the shield flash is on.
    assert!(nav.ships[index].flash > 0.0);
    stage(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "effects-hit.ppm");
    let hit_frame = screen.logical_pixels().to_vec();
    assert!(
        changed(&empty, &hit_frame) > 20,
        "the hit frame did not change"
    );
    // At 20 m model 158 is a small spark: its shapes are one screen pixel, so
    // the frame check is the shield flash around it. The model itself is
    // checked by `model_155_fragments_are_gated_on_global_4`.
    assert!(!screen.effects.is_empty());
    // F942/F926: the kill spawns the model-0x9b explosion and F926 debris.
    let before = nav.effects.len();
    assert_eq!(
        nav.damage_ship_at(id, 1000, Some(hit_point))
            .map(|attack| attack.damage),
        Some(laser::Damage::Destroyed)
    );
    assert!(
        nav.effects[before..]
            .iter()
            .any(|effect| effect.kind == Kind::Explosion),
        "F942 spawned no explosion object"
    );
    assert!(
        nav.effects[before..]
            .iter()
            .any(|effect| effect.kind == Kind::Debris),
        "F926 spawned no debris"
    );
    stage(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "effects-kill.ppm");
    let kill_frame = screen.logical_pixels().to_vec();
    assert!(
        changed(&hit_frame, &kill_frame) > 3,
        "the kill frame did not change"
    );

    // The effects live for their own lifetimes (debris the longest), then the
    // frame is clean again.
    for _ in 0..(effects::DEBRIS_SECONDS / 0.05) as usize + 2 {
        nav.tick_ships(0.05, 2_485, from_mm, 10_000);
    }
    assert!(nav.effects.is_empty(), "the effects did not expire");
    stage(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "effects-kill-after.ppm");
    let after = screen.logical_pixels().to_vec();
    assert!(
        changed(&kill_frame, &after) > 3,
        "the explosion did not disappear"
    );
}

/// A model-155 fragment (model 320, one of F942's twelve child calls) is an
/// F1721 complex polygon. Drawn by the F6 viewer it must reach the F1560/F1559/
/// F1613 span rasteriser and cover pixels, not only its outline. At 2,000 units
/// the fragment is large enough to queue polygons rather than the textured path.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn explosion_fragment_polygons_fill() {
    use ffe_core::model_fill::fill_polygon;
    use ffe_rs::model_viewer::Shape;
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let rows = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let frame = viewer
        .render_scene_with(
            320,
            rows,
            [0.0, 0.0, 2_000.0],
            [0.0, 0.0, -1.0],
            &AnimationInputs::original(0, 0, &viewer.shades()),
        )
        .unwrap();
    let polygons: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Polygon(edges, _) => Some(edges),
            _ => None,
        })
        .collect();
    assert!(
        !polygons.is_empty(),
        "the model-155 fragment must queue polygons, got {:?}",
        frame
            .shapes
            .iter()
            .map(std::mem::discriminant)
            .collect::<Vec<_>>()
    );
    let mut filled = 0usize;
    for edges in polygons {
        let before = filled;
        fill_polygon(edges, |_, _| filled += 1).unwrap();
        assert!(filled > before, "the polygon span fill wrote no pixels");
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn model_155_fragments_are_gated_on_global_4() {
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let rows = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let draw = |mass: u16| {
        let mut inputs = AnimationInputs::original(0, 0, &viewer.shades());
        inputs.globals[effects::EXPLOSION_MASS_GLOBAL] = mass;
        viewer
            .render_scene_with(
                effects::EXPLOSION_MODEL,
                rows,
                [0.0, 0.0, 1.0e6],
                [0.0, 0.0, -1.0],
                &inputs,
            )
            .map(|frame| frame.shapes.len())
    };
    // F942 writes 0xffffffff into +0xa4: the model selects no fragment.
    assert_eq!(draw(0xffff), Ok(0));
    // FFP 1.0 selects the twelve fragment calls.
    assert!(draw(effects::EXPLOSION_MASS).is_ok_and(|shapes| shapes >= 12));
    // The hit particle (model 0x9e) is not gated and draws its six shapes.
    let frame = viewer
        .render_scene(
            effects::HIT_MODEL,
            rows,
            [0.0, 0.0, 1.0e6],
            [0.0, 0.0, -1.0],
        )
        .unwrap();
    assert!(frame.shapes.len() >= 6, "{}", frame.shapes.len());
}
