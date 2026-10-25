//! Local-only frame checks for the console's message log (`F271`/`F272`/`F273`).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! messages_view -- --ignored --nocapture`

use ffe_rs::camera::ViewState;
use ffe_rs::console::ConsoleState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::screen::Screen;

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

fn draw(screen: &mut Screen, views: ViewState) {
    let state = flight();
    screen.draw(&state, views, None, false, false, "");
}

fn changed(before: &[u32], after: &[u32], x0: i32, x1: i32, y0: i32, y1: i32) -> usize {
    (y0.max(0)..y1.min(200))
        .flat_map(|y| (x0.max(0)..x1.min(320)).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let at = *y as usize * 320 + *x as usize;
            before[at] != after[at]
        })
        .count()
}

/// The boot frame: the header date/time and one undated entry, two short lines
/// above the right-hand console buttons (x 196, y 159 and y 167).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn boot_log_frame() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    screen.clock = "1-Jan-3250 12:26:56".to_owned();
    screen.messages.push(
        "Landed at Olympus Village.",
        "1-Jan-3250 12:26:56".to_owned(),
    );
    let state = flight();
    screen.draw(&state, ViewState::default(), None, false, false, "");
    let with_log = screen.logical_pixels().to_vec();
    save(&screen, "log-boot.ppm");

    // Nothing in the view or the left/middle console moved.
    screen.messages_visible = false;
    screen.draw(&state, ViewState::default(), None, false, false, "");
    let without = screen.logical_pixels().to_vec();
    assert_eq!(changed(&without, &with_log, 0, 195, 0, 200), 0);
    assert_eq!(changed(&without, &with_log, 195, 320, 0, 158), 0);
    // The two lines are y 159..166 and 167..175 (the font's ninth row); the
    // buttons at y 177+ stay clear at boot.
    assert!(changed(&without, &with_log, 195, 320, 158, 175) > 100);
    assert_eq!(changed(&without, &with_log, 195, 320, 176, 200), 0);
}

/// Ten stored entries: the window is top-anchored (header, newest entry, then
/// each older entry's date line and text) and five 8-pixel rows are visible.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn multi_entry_log_frame() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    screen.clock = "1-Jan-3250 12:30:16".to_owned();
    for (index, text) in [
        "Docked.",
        "Set:900.0 km{ Act:648.0 km{",
        "Autopilot to Olympus Village.",
        "Landed at Olympus Village.",
        "Launch.",
    ]
    .iter()
    .enumerate()
    {
        screen
            .messages
            .push(*text, format!("1-Jan-3250 12:2{}:16", 6 + index));
    }
    draw(&mut screen, ViewState::default());
    save(&screen, "log-multi.ppm");

    // Scroll to the oldest: F273's 0x18 clamp moves the header off the top and
    // the older entries into the window.
    let content = screen
        .messages
        .lines(&screen_font(&cache), &screen.clock)
        .len()
        * ffe_rs::messages::LINE_HEIGHT;
    assert!(screen.messages.scroll_by(10_000, content, 40));
    assert_eq!(screen.messages.scroll(), ffe_rs::messages::MAX_SCROLL);
    draw(&mut screen, ViewState::default());
    save(&screen, "log-multi-scrolled.ppm");
}

/// The places list owns the view columns x 5 and x 230, y 5..145; the log stays
/// in the console's right-hand window and can be hidden independently.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn log_beside_places_frame() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    screen.clock = "1-Jan-3250 12:26:56".to_owned();
    for index in 0..4 {
        screen.messages.push(
            format!("Message {index} with a few words."),
            format!("1-Jan-3250 12:2{index}:16"),
        );
    }
    let views = ViewState {
        console: ConsoleState {
            navigation: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    draw(&mut screen, views);
    screen.draw_places(&["Daedalus".to_owned(), "Titan City".to_owned()]);
    let with_log = screen.logical_pixels().to_vec();
    save(&screen, "log-places.ppm");

    // The same frame with the places list and no log: every list pixel is
    // identical, so the log does not cover it.
    screen.messages_visible = false;
    draw(&mut screen, views);
    screen.draw_places(&["Daedalus".to_owned(), "Titan City".to_owned()]);
    let without = screen.logical_pixels().to_vec();
    assert_eq!(changed(&without, &with_log, 0, 229, 0, 158), 0);
    // The log is still in its own window when the list is open.
    assert!(changed(&without, &with_log, 195, 320, 158, 200) > 100);
}

fn screen_font(cache: &ffe_content::VerifiedCache) -> ffe_rs::indexed::BitmapFont {
    ffe_rs::indexed::BitmapFont::new(cache.asset("base:font/main").unwrap()).unwrap()
}
