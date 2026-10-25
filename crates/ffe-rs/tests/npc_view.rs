//! Local-only frames and checks for the NPC traffic: a system with traffic, a
//! shield hit (F1536's model 0x9d) and a kill (F942).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! npc_view -- --ignored --nocapture`

use ffe_core::laser;
use ffe_core::npc::Role;
use ffe_rs::camera::{Camera, ViewState};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{model_scale, BodyLabel, Screen, SystemShip};

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

/// The bounding box `(x0, x1, y0, y1)` of the pixels that differ in the upper
/// view (the label button's own image changes in the console below row 158).
fn bounds(before: &[u32], after: &[u32]) -> Option<(i32, i32, i32, i32)> {
    let mut boxed: Option<(i32, i32, i32, i32)> = None;
    for y in 0..158 {
        for x in 0..320 {
            if before[y * 320 + x] != after[y * 320 + x] {
                let (x, y) = (x as i32, y as i32);
                boxed = Some(match boxed {
                    None => (x, x, y, y),
                    Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
                });
            }
        }
    }
    boxed
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
        .map(|effect| ffe_rs::screen::SystemEffect {
            model: effect.model,
            centre: effect.position.map(|v| v as f64 / unit),
            axes: effect.axes(),
            mass: if effect.kind == ffe_core::effects::Kind::Explosion {
                ffe_core::effects::EXPLOSION_MASS
            } else {
                0
            },
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
fn system_ship_label_sits_at_the_projected_centre_in_the_fixed_font() {
    let cache = cache();
    let (nav, viewer, mut screen, state, views, index) = setup(&cache);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let name = nav.ships[index].name.clone();
    let centre = nav.ships[index].position.map(|v| v as f64 / unit);
    let labels_on = ViewState {
        console: ffe_rs::console::ConsoleState {
            labels: true,
            ..ffe_rs::console::ConsoleState::default()
        },
        ..views
    };

    // The view without ships or labels, as the pixel reference.
    ships(&mut screen, &nav);
    screen.ships.clear();
    screen.labels.clear();
    screen.draw(&state, views, Some(&viewer), false, true, "");
    let empty = screen.logical_pixels().to_vec();

    // The ship's projected rect.
    ships(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), false, true, "");
    let ship_frame = screen.logical_pixels().to_vec();
    let ship_box = bounds(&empty, &ship_frame).expect("the ship drew");
    save(&screen, "label-ship.ppm");

    // The label alone: with the model absent, none of its ink is overwritten.
    screen.ships.clear();
    screen.labels = vec![BodyLabel {
        name: name.clone(),
        centre,
    }];
    screen.draw(&state, labels_on, Some(&viewer), false, true, "");
    let label_frame = screen.logical_pixels().to_vec();
    let label_box = bounds(&empty, &label_frame).expect("the label drew");
    save(&screen, "label-only.ppm");

    let (sx, sy, _) = Screen::body_screen(&state, &views, Some(&viewer), centre, 0.0).unwrap();
    // F989 gives F1108 (x + 3, y - 4) and F1108 adds one to x. The name begins
    // with M, whose ink starts at glyph column 0, row 0.
    assert_eq!((label_box.0, label_box.2), (sx + 4, sy - 4), "{name}");
    let height = label_box.3 - label_box.2 + 1;
    assert!(
        (6..=9).contains(&height),
        "label ink {height} rows is not the 9-row main font: {label_box:?}"
    );
    // The label's pen is at the ship's projected centre and its ink overlaps
    // the ship's projected rect (it is not offset off the model).
    assert!(
        label_box.0 <= ship_box.1
            && label_box.1 >= ship_box.0
            && label_box.2 <= ship_box.3
            && label_box.3 >= ship_box.2,
        "label {label_box:?} does not meet the ship {ship_box:?}"
    );

    // Ten times farther: the label keeps the same fixed font size (F1108 does
    // not scale with distance); only the projected point moves.
    let mut far = state.clone();
    far.position = std::array::from_fn(|k| centre[k] + (state.position[k] - centre[k]) * 10.0);
    screen.labels.clear();
    screen.draw(&far, views, Some(&viewer), false, true, "");
    let far_empty = screen.logical_pixels().to_vec();
    screen.labels = vec![BodyLabel { name, centre }];
    screen.draw(&far, labels_on, Some(&viewer), false, true, "");
    let far_frame = screen.logical_pixels().to_vec();
    let far_box = bounds(&far_empty, &far_frame).expect("the far label drew");
    let (fx, fy, _) = Screen::body_screen(&far, &views, Some(&viewer), centre, 0.0).unwrap();
    assert_eq!((far_box.0, far_box.2), (fx + 4, fy - 4));
    assert_eq!(far_box.3 - far_box.2, label_box.3 - label_box.2);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn several_ship_labels_and_a_label_at_the_view_edge() {
    let cache = cache();
    let (mut nav, viewer, mut screen, state, mut views, page) = setup(&cache);
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let stats = viewer.ship_stats(state.ship);
    let scale = model_scale(&viewer).unwrap();
    let camera = Camera::new(&state, &views, stats.as_ref(), scale);
    // A world point whose projection is exactly (sx, sy) at view depth z.
    let at_screen = |sx: f64, sy: f64, z: f64| -> [f64; 3] {
        let v = [(sx - 160.0) * z / 256.0, (79.0 - sy) * z / 256.0, z];
        std::array::from_fn(|i| {
            camera.position[i]
                + camera.axes[0][i] * v[0]
                + camera.axes[1][i] * v[1]
                + camera.axes[2][i] * v[2]
        })
    };
    // Three traffic ships across the view, one of them the locked target.
    let places = [
        (104.0, 50.0, 40.0),
        (160.0, 100.0, 30.0),
        (232.0, 132.0, 55.0),
    ];
    let mut ships = Vec::new();
    let mut labels = Vec::new();
    for (k, ship) in nav.ships.iter_mut().enumerate().take(places.len()) {
        let (sx, sy, z) = places[k];
        ship.position = at_screen(sx, sy, z).map(|v| (v * unit).round() as i64);
        ships.push(SystemShip {
            model: ship.model,
            centre: ship.position.map(|v| v as f64 / unit),
            axes: ship.axes(),
            flash: false,
            dead: false,
        });
        labels.push(BodyLabel {
            name: ship.name.clone(),
            centre: ship.position.map(|v| v as f64 / unit),
        });
    }
    // A name at the right edge (its pen is x 312, so the last glyphs clip) and
    // one at the top edge (pen y -1: the first rows are clipped).
    for (name, sx, sy) in [("RIGHT EDGE", 308.0, 60.0), ("TOP EDGE", 40.0, 3.0)] {
        labels.push(BodyLabel {
            name: name.to_owned(),
            centre: at_screen(sx, sy, 35.0),
        });
    }
    nav.lock_ship(nav.ships[page].id);
    screen.ships = ships;
    screen.labels = labels;
    screen.target_hud = Some(ffe_rs::screen::TargetHud {
        x: 104,
        y: 50,
        value: 2,
        au: false,
        behind: false,
        offscreen: false,
        colour: ffe_rs::targeter::NAVIGATION_COLOUR,
    });
    screen.target_damage = None;
    views.console.labels = true;
    screen.draw(&state, views, Some(&viewer), false, true, "");
    save(&screen, "labels-several.ppm");

    // Every label's pen is F1108's projected point + (4, -4); the edge labels
    // are accepted by F989's bounds and clipped by the text writer.
    let positions = screen.label_positions(&camera);
    assert_eq!(positions.len(), 5);
    for (name, x, y) in &positions {
        let centre = screen
            .labels
            .iter()
            .find(|label| label.name == *name)
            .map(|label| camera.project(label.centre, true).unwrap())
            .unwrap();
        assert_eq!((*x, *y), (centre.0 + 4, centre.1 - 4));
    }
    let top = positions
        .iter()
        .find(|(name, _, _)| name == "TOP EDGE")
        .unwrap();
    assert!(top.2 < 0, "the top-edge label pen is {}", top.2);
    let right = positions
        .iter()
        .find(|(name, _, _)| name == "RIGHT EDGE")
        .unwrap();
    assert_eq!(right.1, 312);
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
        colour: ffe_rs::targeter::COMBAT_COLOUR,
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

    // F953/F942: a hit at least the remaining hull destroys the ship; F942's
    // explosion object and F926's debris replace it.
    assert_eq!(
        nav.damage_ship(ship_id, 1000),
        Some(laser::Damage::Destroyed)
    );
    assert!(nav
        .effects
        .iter()
        .any(|effect| { effect.kind == ffe_core::effects::Kind::Explosion }));
    assert!(!nav
        .ships
        .iter()
        .find(|ship| ship.id == ship_id)
        .unwrap()
        .alive());
    ships(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), false, true, "");
    save(&screen, "npc-kill.ppm");
    let kill_frame = screen.logical_pixels().to_vec();
    assert!(
        changed(&hit_frame, &kill_frame) > 3,
        "the death frame did not differ from the ship and its shield flash"
    );
    // The kill visuals are the effects, not a marker on the dead ship.
    screen.ships.clear();
    screen.effects.clear();
    screen.draw(&state, views, Some(&viewer), false, true, "");
    assert!(
        changed(&kill_frame, screen.logical_pixels()) > 3,
        "the explosion effect did not draw"
    );
    // The effects outlive the ship's slot and expire on their own lifetime;
    // F927 recycles the slot on the same ticks.
    screen.ships = Vec::new();
    let shortest = ffe_core::effects::DEBRIS_SECONDS;
    for _ in 0..(shortest / 0.02) as usize + 2 {
        nav.tick_ships(0.02, 994, from, 10_000);
    }
    assert!(nav.effects.is_empty(), "the effects did not expire");
}
