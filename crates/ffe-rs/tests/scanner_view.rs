//! Local-only frames and checks for the scanner (`F274`/`F275`/`F1569`) and the
//! targeter (`F1113`/`F1114`/`F1111`).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! scanner_view -- --ignored --nocapture`

use ffe_core::laser;
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Kind as NavKind, Navigator, NEW_GAME_SHIP};
use ffe_rs::scanner;
use ffe_rs::screen::Screen;
use ffe_rs::targeter;

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

fn changed(before: &[u32], after: &[u32], area: (usize, usize, usize, usize)) -> usize {
    let (x0, x1, y0, y1) = area;
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if before[y * 320 + x] != after[y * 320 + x] {
                count += 1;
            }
        }
    }
    count
}

/// The scanner area the panel/globe and contacts occupy (F272/F275).
const SCANNER_AREA: (usize, usize, usize, usize) = (101, 194, 154, 200);
/// F1111's cursor area: the box (156..165, 75..84) and circle (153..166, 73..86).
const CURSOR_AREA: (usize, usize, usize, usize) = (150, 172, 70, 90);

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

/// The scanner input main builds from the navigator (bodies in their frames,
/// traffic in the player's), with the model statistics' mass word for colours.
fn scanner_view(
    nav: &Navigator,
    viewer: &ModelViewer,
    player: [i64; 3],
    state: &Flight,
) -> scanner::View {
    let mass = |model: u32| viewer.ship_stats(model).map(|stats| stats.mass);
    let mut contacts: Vec<scanner::Contact> = nav
        .bodies
        .iter()
        .enumerate()
        .map(|(index, body)| scanner::Contact {
            position: body.position,
            frame: body.parent,
            body: Some(index),
            starport: body.kind == NavKind::Starport,
            mass: mass(body.model),
        })
        .collect();
    contacts.extend(
        nav.ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| scanner::Contact {
                position: ship.position,
                frame: nav.frame(),
                body: None,
                starport: false,
                mass: Some(ship.damage.mass),
            }),
    );
    scanner::View {
        position: player,
        axes: ffe_rs::camera::ship_axes(state),
        frame: nav.frame(),
        orientation: scanner::orientation(state.yaw),
        contacts,
    }
}

/// The pirate is the traffic ship away from the station, so the frame is the
/// ship rather than the starport behind a trader (the `npc_view` setup).
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
        .position(|ship| ship.role == ffe_core::npc::Role::Pirate)
        .unwrap_or(0);
    let target = nav.ships[index].position;
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
fn system_traffic_contacts_tunnels_and_targeter_blink() {
    let cache = cache();
    let (nav, viewer, mut screen, state, views, index) = setup(&cache);
    assert!(!nav.bodies.is_empty());
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let player = state.position.map(|v| (v * unit).round() as i64);
    let view = scanner_view(&nav, &viewer, player, &state);
    screen.systems.ca = 0x04;
    screen.lasers = [laser::PULSE_1MW, 0, 0, 0];

    // Contacts: the pirate's mass word picks its colour from DATA_008683.
    let ship = nav.ships[index].clone();
    let mark = view
        .contacts
        .iter()
        .find(|contact| contact.position == ship.position)
        .and_then(|contact| view.mark(contact))
        .expect("the pirate projects on the scanner");
    assert!(
        (102..=192).contains(&mark.x) && (177..=197).contains(&mark.y),
        "contact {mark:?} outside the ring"
    );
    assert_eq!(mark.colour, scanner::colour(Some(ship.damage.mass)));
    assert!(scanner::CONTACT_COLOURS.contains(&mark.colour));

    screen.scanner = Some(view.clone());
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-system.ppm");
    let with_contacts = screen.logical_pixels().to_vec();
    screen.scanner = Some(scanner::View {
        contacts: Vec::new(),
        ..view.clone()
    });
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-system-empty.ppm");
    let no_contacts = screen.logical_pixels().to_vec();
    assert!(
        changed(&with_contacts, &no_contacts, SCANNER_AREA) > 20,
        "the traffic contact did not draw on the scanner"
    );

    // The globe/rings: the orientation overlay changes the scanner art.
    screen.scanner = Some(scanner::View {
        orientation: 0,
        contacts: Vec::new(),
        ..view.clone()
    });
    screen.draw(&state, views, Some(&viewer), true, "");
    let orientation_0 = screen.logical_pixels().to_vec();
    screen.scanner = Some(scanner::View {
        orientation: 4,
        contacts: Vec::new(),
        ..view.clone()
    });
    screen.draw(&state, views, Some(&viewer), true, "");
    assert!(
        changed(&orientation_0, screen.logical_pixels(), SCANNER_AREA) > 50,
        "the F273 orientation overlay did not change"
    );

    // F1100/F1113: the tunnels from the view centre to the locked ship.
    let (mark_x, mark_y) = Screen::body_screen(
        &state,
        &views,
        Some(&viewer),
        ship.position.map(|v| v as f64 / unit),
        ship.radius as f64 / unit,
    )
    .map(|(x, y, _)| (x, y))
    .expect("the locked ship projects");
    let (mantissa, _) = laser::range_words(player, ship.position);
    screen.scanner = Some(view);
    screen.targeter = Some(targeter::Cursor::Combat);
    screen.blink = true;
    screen.tunnels = None;
    screen.draw(&state, views, Some(&viewer), true, "");
    let no_tunnels = screen.logical_pixels().to_vec();
    save(&screen, "scanner-blink-on.ppm");
    screen.tunnels = Some(targeter::Tunnels {
        x: mark_x,
        y: mark_y,
        base: targeter::tunnel_base(mantissa),
        colour: targeter::COMBAT_COLOUR,
    });
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-tunnels.ppm");
    let blink_on = screen.logical_pixels().to_vec();
    assert!(
        changed(&no_tunnels, &blink_on, (0, 320, 0, 158)) > 20,
        "the targeting tunnels did not draw"
    );

    // F1111/F1023: the combat cursor blinks on the game second.
    screen.blink = false;
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-blink-off.ppm");
    assert!(
        changed(&blink_on, screen.logical_pixels(), CURSOR_AREA) > 10,
        "the targetter cursor did not blink"
    );
    let blink_off = screen.logical_pixels().to_vec();
    // The navigation targetter is the box at (160,79), colour 0x11.
    screen.targeter = Some(targeter::Cursor::Navigation);
    screen.blink = true;
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-nav-cursor.ppm");
    assert!(
        changed(&blink_off, screen.logical_pixels(), CURSOR_AREA) > 10,
        "the navigation cursor is not the box"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn contacts_clip_to_the_ring_and_the_stalk_band() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let state = Flight {
        mode: Mode::Flight,
        position: [0.0; 3],
        system_flight: true,
        ..Flight::default()
    };
    let views = ViewState::default();
    let rim = (scanner::FAR_RADIUS * 305.0) as i64;

    // Four rim contacts and one beyond the maximum range. The up contact has
    // the longest stalk; the down one is the masked branch.
    let contacts = vec![
        scanner::Contact {
            position: [-rim, 0, 0],
            frame: None,
            body: None,
            starport: false,
            mass: Some(16),
        },
        scanner::Contact {
            position: [rim, 0, 0],
            frame: None,
            body: None,
            starport: false,
            mass: Some(32),
        },
        scanner::Contact {
            position: [0, rim, 0],
            frame: None,
            body: None,
            starport: false,
            mass: Some(48),
        },
        scanner::Contact {
            position: [0, -rim, 0],
            frame: None,
            body: None,
            starport: false,
            mass: Some(64),
        },
        scanner::Contact {
            position: [0, 0, (scanner::MAX_RANGE * 305.0) as i64 + rim],
            frame: None,
            body: None,
            starport: false,
            mass: Some(80),
        },
    ];
    let view = scanner::View {
        position: [0; 3],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        frame: None,
        orientation: 0,
        contacts,
    };
    let marks = view.marks();
    assert_eq!(marks.len(), 4, "the out-of-range contact is skipped");
    assert!(marks.iter().all(|mark| (102..=192).contains(&mark.x)));
    screen.scanner = Some(view.clone());
    screen.systems.ca = 0x04;
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "scanner-clipped.ppm");
    let clipped = screen.logical_pixels().to_vec();

    // Compare against a frame without contacts: every changed pixel is in the
    // scanner band, from the deepest up stalk (187 - 22 - 1 = 164) to row 198.
    screen.scanner = Some(scanner::View {
        contacts: Vec::new(),
        ..view
    });
    screen.draw(&state, views, Some(&viewer), true, "");
    let bare = screen.logical_pixels().to_vec();
    let mut differences = 0;
    for y in 0..200 {
        for x in 0..320 {
            if clipped[y * 320 + x] == bare[y * 320 + x] {
                continue;
            }
            differences += 1;
            assert!(
                (101..195).contains(&x),
                "contact pixel {x},{y} outside the scanner's x band"
            );
            assert!(
                (164..=198).contains(&y),
                "contact pixel {x},{y} outside the stalk band"
            );
        }
    }
    assert!(differences > 40, "the synthetic contacts did not draw");
}
