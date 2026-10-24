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
    // The first marker is at 5,5 and its name at 20,5; the second row at 5,20.
    assert!(changed(&before, &after, 5, 16, 5, 15) > 20);
    assert!(changed(&before, &after, 20, 200, 5, 14) > 10);
    assert!(changed(&before, &after, 5, 16, 20, 30) > 20);
    assert_eq!(changed(&before, &after, 20, 200, 30, 40), 0);
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
