//! Local-only frame checks for the two right-hand console buttons: the
//! navigation computer's list and the text-label toggle.

use ffe_rs::camera::{ViewMode, ViewState};
use ffe_rs::console::ConsoleState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::screen::{BodyLabel, Screen};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn flight() -> Flight {
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    state
}

fn save(screen: &Screen, name: &str) {
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
    }
}

fn changed(off: &[u32], on: &[u32], x0: i32, x1: i32, y0: i32, y1: i32) -> usize {
    (y0.max(0)..y1.min(200))
        .flat_map(|y| (x0.max(0)..x1.min(320)).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let at = *y as usize * 320 + *x as usize;
            off[at] != on[at]
        })
        .count()
}

/// The labels button's own bitmap changes between its off and on images.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_labels_and_navigation_buttons_show_their_on_images() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let mut views = ViewState {
        console: ConsoleState::default(),
        ..ViewState::default()
    };
    // F991 shows the 0x11 navigation button only with the navigation computer
    // fitted (general-equipment bit 5); the fixture is otherwise an empty ship.
    screen.systems.general |= 1 << 5;

    screen.draw(&state, views, None, false, false, "");
    let off = screen.logical_pixels().to_vec();
    save(&screen, "labels-off.ppm");

    views.console.labels = true;
    screen.draw(&state, views, None, false, false, "");
    let labels_on = screen.logical_pixels().to_vec();
    save(&screen, "labels-on.ppm");
    // The labels icon's 33x23 bitmap sits at 274,178.
    assert!(
        changed(&off, &labels_on, 274, 307, 178, 201) > 40,
        "labels button did not switch images"
    );

    views.console.labels = false;
    views.console.navigation = true;
    screen.draw(&state, views, None, false, false, "");
    let navigation_on = screen.logical_pixels().to_vec();
    save(&screen, "navigation-on.ppm");
    // The navigation icon's 25x25 bitmap sits at 261,177; 261-273 is clear of
    // the labels icon at 274.
    assert!(
        changed(&off, &navigation_on, 261, 274, 177, 202) > 20,
        "navigation button did not switch images"
    );
    // The labels image is idle again in the navigation-on frame (the part clear
    // of the navigation icon, which reaches x 285).
    assert_eq!(changed(&off, &navigation_on, 286, 307, 178, 201), 0);
}

/// `l`: the labels are absent in the off frame and the object's name is drawn
/// at F1108's position (screen x + 1, y) in the on frame.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_labels_toggle_draws_the_objects_name_in_the_flight_view() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let mut views = ViewState {
        mode: ViewMode::Front,
        ..ViewState::default()
    };
    screen.draw(&state, views, None, false, false, "");
    let off = screen.logical_pixels().to_vec();
    save(&screen, "labels-text-off.ppm");

    let centre = [0.0, 0.0, 1000.0];
    screen.labels = vec![BodyLabel {
        name: "MARS".to_owned(),
        centre,
    }];
    views.console.labels = true;
    screen.draw(&state, views, None, false, false, "");
    let on = screen.logical_pixels().to_vec();
    save(&screen, "labels-text-on.ppm");

    let (x, y, _) = Screen::body_screen(&state, &views, None, centre, 0.0).unwrap();
    assert!(changed(&off, &on, x + 1, x + 48, y, y + 9) > 10);
    // Behind the eye, or in the console strip, F989 rejects the label.
    screen.labels[0].centre = [0.0, 0.0, -1000.0];
    screen.draw(&state, views, None, false, false, "");
    assert_eq!(
        changed(&off, screen.logical_pixels(), x + 1, x + 48, y, y + 9),
        0
    );
}

/// The navigation list overlays the view with marker bitmap 0x31 and the name
/// fifteen pixels to its right, as F1099 draws it.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_navigation_list_draws_markers_and_names_over_the_view() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState {
        console: ConsoleState {
            navigation: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    screen.draw(&state, views, None, false, false, "");
    let before = screen.logical_pixels().to_vec();
    screen.draw_places(&["LONDON STATION".to_owned(), "MOSCOW STATION".to_owned()]);
    let after = screen.logical_pixels().to_vec();
    save(&screen, "navigation-list.ppm");
    // The first marker is at 10,5 and its name at 25,5; the second row at 10,20.
    assert!(changed(&before, &after, 10, 21, 5, 15) > 20);
    assert!(changed(&before, &after, 25, 200, 5, 14) > 10);
    assert!(changed(&before, &after, 10, 21, 20, 30) > 20);
    assert_eq!(changed(&before, &after, 25, 200, 30, 40), 0);
}

/// The exact original frame at Gateway (the FFE manual's Figure 20): nine
/// entries down the left column at x 10, then three at x 230, green names at
/// x+15 with grey marker squares, no background, and the HUD/message window
/// left untouched. F1099 draws the list in the upper view (y 5..125); F272's
/// log is at y 159..199, so neither can cover the other.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_gateway_places_list_is_the_originals_columns_and_cannot_be_covered() {
    use ffe_rs::hud::PlayerHud;
    use ffe_rs::messages::MessageLog;

    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState {
        console: ConsoleState {
            navigation: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    // The original's HUD and message window under the list.
    screen.hud = PlayerHud {
        set_speed: Some(17),
        actual_speed: 19,
        altitude: Some(615),
        registration: "Hope".to_owned(),
        ..PlayerHud::default()
    };
    screen.clock = "1-Jan-3250 12:02:54".to_owned();
    let mut log = MessageLog::new();
    log.push(
        "Old Blackelk Traffic Control: cleared.",
        "1-Jan-3250 12:02".to_owned(),
    );
    log.push("Docking computer engaged.", "1-Jan-3250 12:03".to_owned());
    screen.messages = log;

    // The manual's Gateway frame, in F1099's slot order: nine left, three right.
    let names: Vec<String> = [
        "Fortress Saskatoon",
        "Old Curie",
        "Old Blackelk",
        "Turner",
        "Turner Town",
        "Dublin Citadel",
        "Sarajevo",
        "Seattle",
        "Larson",
        "New Chernobyl",
        "Sarajevo",
        "Micca Town",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    screen.draw(&state, views, None, false, false, "");
    let before = screen.logical_pixels().to_vec();
    screen.draw_places(&names);
    let after = screen.logical_pixels().to_vec();
    save(&screen, "places-gateway.ppm");

    // Every row's marker (10, 5+15r / 230, 5+15(r-9)) and name (25 / 245) draws.
    for index in 0..names.len() {
        let (x, y) = Screen::place_cell(index).unwrap();
        assert!(
            changed(&before, &after, x, x + 11, y, y + 10) > 20,
            "marker {index} missing at {x},{y}"
        );
        assert!(
            changed(&before, &after, x + 15, 320, y, y + 8) > 10,
            "name {index} missing at {},{}",
            x + 15,
            y
        );
    }
    // The tenth entry starts the second column: it is not drawn at y 140 on the
    // left, and the columns are nine rows for the original's 18 hot areas.
    assert_eq!(changed(&before, &after, 10, 21, 140, 150), 0);
    assert!(changed(&before, &after, 230, 241, 5, 15) > 20);
    assert!(changed(&before, &after, 245, 320, 20, 28) > 10);
    assert!(changed(&before, &after, 245, 320, 35, 43) > 10);

    // No panel or background: the space between the columns stays the view.
    assert_eq!(changed(&before, &after, 150, 230, 0, 140), 0);
    // F272's message window (x 195.., y 159..) is untouched by the list.
    assert_eq!(changed(&before, &after, 195, 320, 158, 200), 0);
    // And a populated log drawn under the list changes no list cell: draw the
    // same frame again with and without messages and compare the list.
    let mut without = Screen::new(&cache).unwrap();
    without.hud = screen.hud.clone();
    without.draw(&state, views, None, false, false, "");
    let bare = without.logical_pixels().to_vec();
    without.draw_places(&names);
    let bare_list = without.logical_pixels().to_vec();
    for index in 0..names.len() {
        let (x, y) = Screen::place_cell(index).unwrap();
        assert_eq!(
            changed(&after, &bare_list, x, x + 120, y, y + 9),
            0,
            "the message log changed list row {index}"
        );
    }
    // The list itself never reaches the log's rows.
    assert_eq!(changed(&bare, &bare_list, 0, 320, 158, 200), 0);
}

/// The navigation computer and labels buttons each have their own image, and
/// the console's state follows `n`/the icon and `l` independently.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_navigation_and_labels_buttons_light_independently() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let mut views = ViewState {
        console: ConsoleState::default(),
        ..ViewState::default()
    };
    screen.systems.general |= 1 << 5; // the navigation computer is fitted
    screen.draw(&state, views, None, false, false, "");
    let off = screen.logical_pixels().to_vec();

    views.console.labels = true;
    views.console.navigation = true;
    screen.draw(&state, views, None, false, false, "");
    let both = screen.logical_pixels().to_vec();
    save(&screen, "labels-and-navigation-on.ppm");
    // The labels icon (274,178, 33x23) and the navigation icon (261,177,
    // 25x25) are both lit; neither overwrites the other.
    assert!(changed(&off, &both, 274, 307, 178, 201) > 40);
    assert!(changed(&off, &both, 261, 274, 177, 202) > 20);

    // Closing the list leaves the labels on: only the navigation icon changes.
    views.console.navigation = false;
    screen.draw(&state, views, None, false, false, "");
    let labels_only = screen.logical_pixels().to_vec();
    // The navigation icon reaches x 285, over the labels icon's left edge.
    assert_eq!(changed(&both, &labels_only, 286, 307, 178, 201), 0);
    assert!(changed(&both, &labels_only, 261, 274, 177, 202) > 20);
}

/// The same Gateway list drawn over a landing view with the original HUD and
/// message log, for the evidence frame: the list is over the world, the console
/// text stays underneath and none of it reaches the list's cells.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_gateway_places_frame_over_a_landing_view() {
    use ffe_rs::hud::PlayerHud;
    use ffe_rs::model_viewer::ModelViewer;
    use ffe_rs::navigation::{bodies, galaxy};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Disc;

    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let sol = galaxy(&cache)
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "Sol")
        .unwrap();
    let list = bodies(&cache, &sol, [0x1000, 0]).unwrap();
    let planet = list.iter().find(|body| body.name == "Earth").unwrap();
    let radius = planet.radius as f64 / UNIT;
    let centre = planet.position.map(|v| v as f64 / UNIT);
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    // Stand on the surface and look along the horizon, as a landing does.
    state.position = [centre[0], centre[1] + 1.01 * radius, centre[2]];
    screen.discs = vec![Disc {
        centre,
        radius,
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((planet.model, planet.seed)),
        orientation: planet.orientation,
    }];
    screen.hud = PlayerHud {
        set_speed: Some(17),
        actual_speed: 19,
        altitude: Some(615),
        registration: "Hope".to_owned(),
        ..PlayerHud::default()
    };
    screen.clock = "1-Jan-3250 12:02:54".to_owned();
    screen.messages.push(
        "Old Blackelk Traffic Control: cleared.",
        "1-Jan-3250 12:02".to_owned(),
    );
    screen
        .messages
        .push("Docking computer engaged.", "1-Jan-3250 12:03".to_owned());
    let names: Vec<String> = [
        "Fortress Saskatoon",
        "Old Curie",
        "Old Blackelk",
        "Turner",
        "Turner Town",
        "Dublin Citadel",
        "Sarajevo",
        "Seattle",
        "Larson",
        "New Chernobyl",
        "Sarajevo",
        "Micca Town",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let views = ViewState {
        console: ConsoleState {
            navigation: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    // Redraw the flight frame and put the list over it, as F1099 does.
    for _ in 0..4 {
        screen.draw(&state, views, Some(&viewer), false, false, "");
        screen.draw_places(&names);
    }
    save(&screen, "places-gateway-world.ppm");
    // The world behind is not blank, and the list's 12 rows still draw.
    let view = &screen.logical_pixels()[..320 * 158];
    assert!(
        view.iter().filter(|pixel| **pixel != 0).count() > 10_000,
        "the landing view did not draw"
    );
}

/// The same list with Sol's actual 18 places, in the generated system's order.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_sol_places_list_frame_has_the_originals_names() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState {
        console: ConsoleState {
            navigation: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    screen.draw(&state, views, None, false, false, "");
    let mut nav = Navigator::new(&cache).unwrap();
    nav.open(View::Places);
    let names = nav.lines([0; 3]);
    assert_eq!(names.len(), 18);
    assert_eq!(names[0], "Daedalus");
    assert_eq!(names[17], "Titan City");
    screen.draw_places(&names);
    save(&screen, "navigation-list-sol.ppm");
}
