//! Local-only scripted sequence for the station traffic cycle: a trader flies
//! its final approach (F745/F746), touches the pad (F652), waits out F747's
//! two-to-three day object time with its bay occupied, asks F654 for the door,
//! waits for F865's word (F735), releases the bay (F651/F736) and picks its next
//! station (F761/F762).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! npc_docking -- --ignored --nocapture`
//!
//! Frames (when `FFE_SHOT_DIR` is set): `npc-dock-approach.ppm` (the trader on
//! its final leg), `npc-dock-docked.ppm` (the bay occupied, the ship inside the
//! station) and `npc-dock-launch.ppm` (the launched ship flying out).
//!
//! The sequence fast-forwards F747's two-to-three day interval after checking it,
//! and gives the trader a test cruise speed so the approach does not take the
//! source's real minutes. The turn, bay, door and station-record arithmetic are
//! the real ones.

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
) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.ships = nav
        .ships
        .iter()
        .filter(|ship| ship.visible())
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
        .filter(|ship| ship.visible())
        .map(|ship| ffe_rs::screen::BodyLabel {
            name: ship.name.clone(),
            centre: ship.position.map(|v| v as f64 / unit),
        })
        .collect();
    screen.lasers = [laser::PULSE_1MW, 0, 0, 0];
    screen.draw(state, ViewState::default(), Some(viewer), true, message);
}

/// Point the camera 500 m from `at` on the far side from `away` (the station),
/// looking at `at` with the station behind it.
fn camera_at(at: [i64; 3], away: [i64; 3]) -> Flight {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let direction = normalise(std::array::from_fn(|k| (at[k] - away[k]) as f64));
    let from_mm: [i64; 3] = std::array::from_fn(|k| at[k] + (direction[k] * 500_000.0) as i64);
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

/// A frame with the camera on the ship, the station behind it.
fn follow(state: &mut Flight, at: [i64; 3], station: [i64; 3]) {
    *state = camera_at(at, station);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_trader_approaches_docks_and_launches() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    // F829/F998's new game has no traffic; this cycle needs F688's capture
    // guarantee (a trader with a station) at the starport's launch point.
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.respawn_traffic(&cache, at, forward);
    let station = nav.docked.expect("the new game is docked");
    let body = nav.bodies[station].clone();
    let trader = nav
        .ships
        .iter()
        .find(|ship| matches!(ship.role, Role::Trader | Role::Shuttle) && ship.station.is_some())
        .expect("no station traffic");
    let id = trader.id;
    let name = trader.name.clone();
    let model = trader.model;
    let traffic = npc::traffic_id(id);
    println!(
        "F688 trader {name} (model {model:#x}) to station {station}: radius {} mm, orbit {} mm",
        body.radius,
        ffe_rs::navigation::distance(trader.position, body.position) as i64
    );
    // The station-traffic cruise floor (STATION_CRUISE_SPEED) keeps the approach
    // inside a play session; the model's turn limit and the station's bay
    // arithmetic are unchanged.
    let max_speed = {
        let ship = nav.ships.iter().find(|ship| ship.id == id).unwrap();
        ship.max_speed
    };
    println!("approach speed {max_speed} mm/s");
    let player = body.position;
    let radius = viewer.model_radius(NEW_GAME_SHIP).unwrap();
    let radius = i64::from(radius);
    let mut state = camera_at(
        nav.ships.iter().find(|s| s.id == id).unwrap().position,
        body.position,
    );
    let mut messages = Vec::new();
    let mut approach = false;
    let mut docked = false;
    let mut dock_frame = None;
    let mut dock_seconds = 0.0;
    let mut docked_bay = None;
    for frame_no in 0..60_000 {
        let _ = nav.tick_ships(0.02, 994, player, radius);
        let _ = nav.tick_traffic(994, 0.02);
        let Some(ship) = nav.ships.iter().find(|ship| ship.id == id) else {
            break;
        };
        let (ship_state, ship_bay, ship_timer, ship_position) =
            (ship.state, ship.bay, ship.dock_timer, ship.position);
        match ship_state {
            State::Approach if !approach => {
                approach = true;
                messages.push(format!(
                    "F745: {name} cleared for bay {} (F655 granted)",
                    ship_bay.unwrap_or(0)
                ));
                follow(&mut state, ship_position, body.position);
                frame(
                    &mut screen,
                    &viewer,
                    &state,
                    &nav,
                    "Traffic on final approach.",
                );
                save(&screen, "npc-dock-approach.ppm");
            }
            State::Docked if !docked => {
                docked = true;
                dock_frame = Some(frame_no);
                dock_seconds = ship_timer;
                docked_bay = ship_bay;
                messages.push(format!(
                    "F652 touch: {name} on the pad; F747 waits {:.1} h with bay {} occupied",
                    ship_timer / 3600.0,
                    ship_bay.unwrap_or(0)
                ));
                let port = nav.starport(station);
                assert_eq!(port.bay_of(traffic), ship_bay);
                assert!(port
                    .pads
                    .iter()
                    .any(|pad| Some(u16::from(*pad)) == ship_bay));

                follow(&mut state, ship_position, body.position);
                frame(
                    &mut screen,
                    &viewer,
                    &state,
                    &nav,
                    "Bay occupied: traffic docked.",
                );
                save(&screen, "npc-dock-docked.ppm");
                break;
            }
            _ => {}
        }
    }
    assert!(approach, "the trader never started its final approach");
    assert!(docked, "the trader never docked");
    assert!(
        (2.0 * 86_400.0..3.0 * 86_400.0).contains(&dock_seconds),
        "F747's interval {dock_seconds}"
    );
    println!(
        "docked on frame {} after {:.1} s; F747 interval {:.2} days",
        dock_frame.unwrap(),
        dock_frame.unwrap() as f64 * 0.02,
        dock_seconds / 86_400.0
    );

    // Fast-forward the two-to-three days (time acceleration does this in play),
    // then run the launch: F734's F654 opens the door, F735 waits for F865 and
    // F651/F736 release the bay and send the ship to F762's next station.
    nav.ships
        .iter_mut()
        .find(|ship| ship.id == id)
        .unwrap()
        .dock_timer = 0.01;
    let mut launched = false;
    let mut departed = false;
    let mut resumed = false;
    for _ in 0..4000 {
        let _ = nav.tick_ships(0.02, 994, player, radius);
        let _ = nav.tick_traffic(994, 0.02);
        let ship = nav.ships.iter().find(|ship| ship.id == id).unwrap();
        let (ship_state, ship_position, ship_station) = (ship.state, ship.position, ship.station);
        match ship_state {
            State::Launch => {
                if !launched {
                    launched = true;
                    messages.push(format!(
                        "F734/F654: {name} cleared to launch; waiting for the door"
                    ));
                }
            }
            State::Depart => {
                if !departed {
                    departed = true;
                    messages.push(format!(
                        "F735/F865: door open; F651 frees bay {}",
                        docked_bay.map_or("?".to_owned(), |bay| bay.to_string())
                    ));
                    follow(&mut state, ship_position, body.position);
                    frame(
                        &mut screen,
                        &viewer,
                        &state,
                        &nav,
                        "Launching: bay released.",
                    );
                    save(&screen, "npc-dock-launch.ppm");
                    let with = screen.logical_pixels().to_vec();
                    screen.ships.clear();
                    screen.labels.clear();
                    screen.draw(
                        &state,
                        ViewState::default(),
                        Some(&viewer),
                        true,
                        "Launching: bay released.",
                    );
                    assert!(
                        with != screen.logical_pixels(),
                        "the launched ship did not draw"
                    );
                    let port = nav.starport(station);
                    assert_eq!(port.bay_of(traffic), None);
                    assert!(!port
                        .pads
                        .iter()
                        .any(|pad| u16::from(*pad) == u16::from(traffic)));
                }
            }
            State::Transit if departed => {
                resumed = true;
                messages.push(format!(
                    "F761/F762: {name} set course for station {ship_station:?}"
                ));
                break;
            }
            _ => {}
        }
    }
    assert!(
        launched,
        "the docked trader never asked for launch clearance"
    );
    assert!(departed, "the launch never left the station");
    assert!(resumed, "the trader never resumed transit");
    assert!(nav.ships.iter().any(|ship| ship.id == id && ship.visible()));
    for message in messages {
        println!("{message}");
    }
}
