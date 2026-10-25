//! Local-only scripted sequence for F688's station traffic: F691's initial
//! station-bay ships. The new game's F688 pass draws `RND(4 * places)` docked
//! traders; each one asks the station's traffic record (F650) for a bay, starts
//! F691's object time (F1530's word, without F747's `+2` days) and later runs
//! the ordinary F733/F734/F735/F651/F736 launch and F761/F762 next-station
//! choice that the station cycle already implements.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! station_traffic -- --ignored --nocapture`
//!
//! Frames (when `FFE_SHOT_DIR` is set): `station-traffic.ppm` (the station and
//! its visible traffic), `station-bay-docked.ppm` (the bay ship is inside the
//! station, so it is not drawn; the record still holds the bay) and
//! `station-bay-launch.ppm` (the F691 ship out of the slot after F651).

use ffe_core::laser;
use ffe_core::npc::{self, Role, State};
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

fn frame(
    screen: &mut Screen,
    viewer: &ModelViewer,
    state: &Flight,
    nav: &Navigator,
    message: &str,
    draw_docked: Option<u32>,
) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.ships = nav
        .ships
        .iter()
        .filter(|ship| ship.visible() || Some(ship.id) == draw_docked)
        .map(|ship| SystemShip {
            model: ship.model,
            centre: ship.position.map(|v| v as f64 / unit),
            axes: ship.axes(),
            flash: ship.flash > 0.0,
            dead: false,
        })
        .collect();
    let tics = ffe_core::model_animation::START_TICS
        .wrapping_add((nav.clock * ffe_rs::original_flight::TICS_PER_SECOND) as u64 as u32);
    screen.stations = nav
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| {
            matches!(
                body.kind,
                ffe_rs::navigation::Kind::Station | ffe_rs::navigation::Kind::Starport
            )
        })
        .map(|(index, body)| ffe_rs::screen::SystemStation {
            model: body.model,
            centre: body.position.map(|v| v as f64 / unit),
            orientation: body.orientation,
            speed: body.spin,
            door: nav.door(index),
            tics,
        })
        .collect();
    screen.labels = nav
        .ships
        .iter()
        .filter(|ship| ship.visible() || Some(ship.id) == draw_docked)
        .map(|ship| ffe_rs::screen::BodyLabel {
            name: ship.name.clone(),
            centre: ship.position.map(|v| v as f64 / unit),
        })
        .collect();
    screen.lasers = [laser::PULSE_1MW, 0, 0, 0];
    screen.draw(state, ViewState::default(), Some(viewer), false, message);
}

/// Point the camera at `from` (millimetres) looking at `target` (millimetres).
fn camera_looking(from_mm: [i64; 3], target: [i64; 3]) -> Flight {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let from = from_mm.map(|v| v as f64 / unit);
    Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        axes: Some(look_at(from, target.map(|v| v as f64 / unit))),
        ..Flight::default()
    }
}

/// Point the camera `distance` mm from `at` on the far side from `away`,
/// looking at `at` with the station behind it.
fn camera_at(at: [i64; 3], away: [i64; 3], distance: f64) -> Flight {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let direction = normalise(std::array::from_fn(|k| (at[k] - away[k]) as f64));
    let from_mm: [i64; 3] = std::array::from_fn(|k| at[k] + (direction[k] * distance) as i64);
    let from = from_mm.map(|v| v as f64 / unit);
    Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        axes: Some(look_at(from, at.map(|v| v as f64 / unit))),
        ..Flight::default()
    }
}

/// The new game's F688 pass is deterministic per start record. Pick a start
/// whose F691 roll actually placed a ship in a bay (the source's roll can be
/// zero, as can the port's); prefer the record with the most visible traffic so
/// the station frame has something else to show.
fn new_game_with_bay(cache: &ffe_content::VerifiedCache) -> Navigator {
    let mut best: Option<(usize, Navigator)> = None;
    for start_type in 0..=5u8 {
        let Ok(nav) = Navigator::new_game(cache, start_type) else {
            continue;
        };
        if !nav
            .ships
            .iter()
            .any(|ship| ship.state == State::Docked && ship.bay.is_some())
        {
            continue;
        }
        let visible = nav.ships.iter().filter(|ship| ship.visible()).count();
        if best.as_ref().is_none_or(|(most, _)| visible > *most) {
            best = Some((visible, nav));
        }
    }
    best.map(|(_, nav)| nav)
        .expect("no start system drew an F691 station-bay ship")
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn station_bay_ships_are_docked_and_launch_through_f691s_cycle() {
    let cache = cache();
    let mut nav = new_game_with_bay(&cache);
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();

    let (id, name, station, bay, span, model) = {
        let ship = nav
            .ships
            .iter()
            .find(|ship| ship.state == State::Docked && ship.bay.is_some())
            .expect("an F691 ship");
        (
            ship.id,
            ship.name.clone(),
            ship.station.expect("the bay's station"),
            ship.bay.expect("the granted bay"),
            ship.dock_span,
            ship.model,
        )
    };
    let traffic = npc::traffic_id(id);
    let body = nav.bodies[station].clone();
    let port = nav.starport(station);
    assert_eq!(port.bay_of(traffic), Some(bay));
    assert!(port
        .pads
        .iter()
        .any(|pad| u16::from(*pad) == u16::from(traffic)));
    println!(
        "F688 pass in {}: {} place(s), {} ship(s); F700 placed {name} (model {model:#x}) in {} bay {bay}; F691 object time {:.2} h ({:.2} days)",
        nav.system_name(),
        nav.bodies.iter().filter(|body| body.kind.is_place()).count(),
        nav.ships.len(),
        body.name,
        span / 3600.0,
        span / 86_400.0
    );
    assert!(
        (0.0..16.0 * 86_400.0).contains(&span),
        "F691's object time {span}"
    );
    assert!(
        ffe_rs::navigation::distance(
            nav.ships.iter().find(|s| s.id == id).unwrap().position,
            body.position
        ) < 2.0 * body.radius as f64 + 10_000_000.0,
        "the docked ship is not at the station"
    );

    // The station and whatever visible F688 traffic the record spawned.
    let at = nav
        .port(station)
        .map(|port| port.point(port.front + 2.0 * port.radius + 2.0e6))
        .or_else(|| nav.starport_approach_point(station))
        .unwrap_or(body.position);
    let mut state = camera_looking(at, body.position);
    frame(
        &mut screen,
        &viewer,
        &state,
        &nav,
        "Station traffic: F691 ships are in the bays.",
        None,
    );
    save(&screen, "station-traffic.ppm");

    // The bay ship is inside the station: F747 hides it. Draw it for this
    // evidence frame only, from 200 m outside the pad, so the bay placement is
    // visible; the live view keeps `NpcShip::visible` false.
    let ship = nav.ships.iter().find(|s| s.id == id).unwrap();
    assert!(!ship.visible());
    let seat = ship.position;
    state = camera_at(seat, body.position, 200_000.0);
    frame(
        &mut screen,
        &viewer,
        &state,
        &nav,
        "Bay occupied: F691 ship docked (drawn for the evidence frame).",
        Some(id),
    );
    save(&screen, "station-bay-docked.ppm");

    // Time acceleration fast-forwards F691's object time in play.
    nav.ships
        .iter_mut()
        .find(|ship| ship.id == id)
        .unwrap()
        .dock_timer = 0.01;
    let player = body.position;
    let radius = i64::from(viewer.model_radius(NEW_GAME_SHIP).unwrap());
    let mut launched = false;
    let mut departed = false;
    let mut resumed = false;
    let mut next_station = None;
    for _ in 0..6000 {
        let _ = nav.tick_ships(0.02, 994, player, radius);
        let _ = nav.tick_traffic(994, 0.02);
        let Some(ship) = nav.ships.iter().find(|ship| ship.id == id) else {
            break;
        };
        match ship.state {
            State::Launch if !launched => {
                launched = true;
                println!("F733/F734/F654: {name} cleared to launch; waiting for the door");
            }
            State::Depart if !departed => {
                departed = true;
                let position = ship.position;
                let port = nav.starport(station);
                assert_eq!(port.bay_of(traffic), None);
                assert!(!port
                    .pads
                    .iter()
                    .any(|pad| u16::from(*pad) == u16::from(traffic)));
                println!("F735/F865/F651: door open; bay {bay} freed");
                state = camera_at(position, body.position, 150_000.0);
                frame(
                    &mut screen,
                    &viewer,
                    &state,
                    &nav,
                    "Launching: station-bay ship out of the slot.",
                    None,
                );
                save(&screen, "station-bay-launch.ppm");
                let with = screen.logical_pixels().to_vec();
                screen.ships.clear();
                screen.labels.clear();
                screen.draw(
                    &state,
                    ViewState::default(),
                    Some(&viewer),
                    false,
                    "Launching: station-bay ship out of the slot.",
                );
                assert!(
                    with != screen.logical_pixels(),
                    "the launched station-bay ship did not draw"
                );
            }
            State::Transit if departed => {
                resumed = true;
                next_station = ship.station;
                println!("F761/F762: {name} set course for station {next_station:?}");
                break;
            }
            _ => {}
        }
    }
    assert!(launched, "the F691 ship never asked for launch clearance");
    assert!(departed, "the F691 ship never left the station");
    assert!(resumed, "the F691 ship never resumed transit");
    assert!(next_station.is_some());
    assert!(nav.ships.iter().any(|ship| ship.id == id && ship.visible()));

    // The station-bay branch is the source's type-1 trader, not F699's Lynx.
    let ship = nav.ships.iter().find(|s| s.id == id).unwrap();
    assert_eq!(ship.role, Role::Trader);
    assert!(ffe_core::npc::SHIP_TYPES[1].contains(&ship.model));
    println!(
        "visible traffic: {} ship(s), {} at the station",
        nav.ships.iter().filter(|ship| ship.visible()).count(),
        nav.ships
            .iter()
            .filter(|ship| ship.visible() && ship.station == Some(station))
            .count()
    );
}
