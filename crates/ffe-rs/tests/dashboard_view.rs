//! Local-only frame checks for the original in-flight dashboard: F272's
//! message log, F1099's player shields/hull/Set/Act/Alt/turret text, F1116/F991
//! lamps and F1120's engines-off equipment page.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! dashboard_view -- --ignored --nocapture`

use ffe_rs::camera::{TurretCamera, ViewMode, ViewState};
use ffe_rs::console::{ConsoleState, ShipSystems};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::hud::PlayerHud;
use ffe_rs::screen::Screen;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn flight() -> Flight {
    let mut state = Flight {
        mode: Mode::Flight,
        position: [0.0; 3],
        velocity: [0.0, 0.0, -100.0],
        system_flight: true,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    state
}

fn draw(screen: &mut Screen, state: &Flight, views: ViewState) {
    screen.draw(state, views, None, false, true, "");
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

fn changed(before: &[u32], after: &[u32], x0: i32, x1: i32, y0: i32, y1: i32) -> usize {
    (y0.max(0)..y1.min(200))
        .flat_map(|y| (x0.max(0)..x1.min(320)).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let at = *y as usize * 320 + *x as usize;
            before[at] != after[at]
        })
        .count()
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_message_log_draws_only_in_the_original_right_hand_window() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    draw(&mut screen, &state, ViewState::default());
    let without = screen.logical_pixels().to_vec();

    screen.clock = "1-Jan-3250 12:00:00".to_owned();
    screen.messages.push(
        "Docked. L launches; F2 shows the ship systems.",
        "1-Jan-3250 12:00".to_owned(),
    );
    screen
        .messages
        .push("Time acceleration 10X.", "1-Jan-3250 12:01".to_owned());
    draw(&mut screen, &state, ViewState::default());
    let with_log = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-messages.ppm");

    // F271/F272: the text is at x 196 (shadow 195), y 159, not the port's old
    // two-line strip at (3,139).
    assert!(changed(&without, &with_log, 195, 320, 158, 200) > 200);
    assert_eq!(changed(&without, &with_log, 0, 195, 0, 200), 0);
    // Ten entries keep the newest; the header is still the first line.
    for n in 0..12 {
        screen
            .messages
            .push(format!("message {n}"), "1-Jan-3250 12:02".to_owned());
    }
    assert_eq!(screen.messages.len(), 10);
    draw(&mut screen, &state, ViewState::default());
    let full = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-messages-full.ppm");

    // F273's scroll: 0x18 pixels up, clamped even when asked for more.
    assert!(screen.messages.scroll_by(10_000, 1000, 41));
    assert_eq!(screen.messages.scroll(), 0x18);
    draw(&mut screen, &state, ViewState::default());
    save(&screen, "dashboard-messages-scrolled.ppm");
    assert!(changed(&full, screen.logical_pixels(), 195, 320, 158, 200) > 40);
    // The auto-scroll returns to the newest entry after 0x3cae6 tics.
    screen.messages.tick(0x3cae6);
    assert_eq!(screen.messages.scroll(), 0);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_player_hud_texts_are_at_f1099s_positions() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    draw(&mut screen, &state, ViewState::default());
    let without = screen.logical_pixels().to_vec();

    screen.hud = PlayerHud {
        shields: [128, 256],
        hull: [2304, 4608],
        set_speed: Some(250.0),
        actual_speed: 180.0,
        altitude: Some(1200),
        registration: "EAGLE".to_owned(),
        autopilot: false,
        turret: None,
    };
    draw(&mut screen, &state, ViewState::default());
    let with_hud = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-hud.ppm");

    // Shields (2,1), hull (2,11), Set/Act (2,148) and its Alt/registration lines.
    assert!(changed(&without, &with_hud, 2, 150, 1, 10) > 20);
    assert!(changed(&without, &with_hud, 2, 150, 11, 20) > 20);
    assert!(changed(&without, &with_hud, 2, 150, 148, 176) > 20);
    // The port's old SPEED line at (3,13) and status block at (3,120) stay clear.
    assert_eq!(changed(&without, &with_hud, 0, 195, 118, 147), 0);

    // Autopilot at (2,139).
    screen.hud.autopilot = true;
    draw(&mut screen, &state, ViewState::default());
    let autopilot = screen.logical_pixels().to_vec();
    assert!(changed(&with_hud, &autopilot, 2, 100, 139, 147) > 10);
    save(&screen, "dashboard-autopilot.ppm");

    // Turret Decn./Rotn. at (5,76), only in a turret view.
    let mut views = ViewState {
        mode: ViewMode::TopTurret,
        ..ViewState::default()
    };
    views.turret = TurretCamera {
        yaw: 0x2000,
        pitch: 0x1000,
    };
    screen.hud.turret = Some((450, -225));
    draw(&mut screen, &state, views);
    let turret = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-turret.ppm");
    assert!(changed(&autopilot, &turret, 5, 120, 76, 92) > 10);
    views.mode = ViewMode::Front;
    draw(&mut screen, &state, views);
    assert_eq!(
        changed(&autopilot, screen.logical_pixels(), 5, 120, 76, 92),
        0
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_lamps_and_the_equipment_page_are_the_originals() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState::default();
    draw(&mut screen, &state, views);
    let normal = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-lamps.ppm");
    // F1116 shows 0x49 at (24,158) while DATA_007246 is clear; setting the
    // targeter word hides it.
    screen.systems.data_246 = true;
    draw(&mut screen, &state, views);
    let no_49 = screen.logical_pixels().to_vec();
    assert!(changed(&normal, &no_49, 24, 39, 158, 168) > 0);
    // e9 bit 1 switches both 0x38 (52,185) and 0x46 (3,170) between images.
    screen.systems.data_246 = false;
    screen.systems.e9 = 2;
    draw(&mut screen, &state, views);
    let lamp_off = screen.logical_pixels().to_vec();
    assert!(changed(&normal, &lamp_off, 3, 30, 170, 180) > 0);
    // 0x38 is the lamp at (71,189).
    assert!(changed(&normal, &lamp_off, 71, 90, 189, 200) > 0);
    screen.systems.e9 = 0;
    // F2: F1120 hides 0x45-0x49, shows the 60x45 diagram at (0,157) and the
    // ten equipment icons for the slots that are set.
    screen.systems = ShipSystems {
        slots: [1, 2, 0, 0, 0, 0, 0, 0, 0, 3],
        general: (1 << 2) | (1 << 19) | (1 << 23) | (1 << 24) | (1 << 26),
        e8: 0x34,
        ..ShipSystems::default()
    };
    let mut page_views = views;
    page_views.console = ConsoleState {
        status_page: true,
        ..ConsoleState::default()
    };
    draw(&mut screen, &state, page_views);
    let page = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-equipment.ppm");
    assert!(
        changed(&normal, &page, 0, 60, 157, 200) > 100,
        "the ship diagram did not draw"
    );
    assert!(changed(&normal, &page, 0, 60, 164, 200) > 40);
    // F1120 hides the F1116 lamps: 0x49 at (24,158), 0x47 at (5,175).
    assert!(changed(&normal, &page, 24, 39, 158, 168) > 0);
    assert!(changed(&normal, &page, 5, 40, 175, 185) > 0);
    // Slot icons 0x3c (33,164) and 0x3d (9,167) draw their image 1/2 bitmaps.
    assert!(changed(&normal, &page, 33, 60, 164, 180) > 10);
    assert!(changed(&normal, &page, 9, 40, 167, 180) > 10);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_default_start_draws_the_sakers_hull_and_equipment_page() {
    use ffe_core::equipment::Loadout;
    use ffe_rs::navigation::{DEFAULT_SHIP, DEFAULT_START_TYPE};

    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = flight();
    assert_eq!(DEFAULT_START_TYPE, 0);
    state.ship = DEFAULT_SHIP;
    state.loadout = Loadout::new_game();
    state.combat = ffe_rs::combat::Combat::new_game();
    // F1533: Saker's 28 t mass * 4; F998 leaves +0xe2 zero.
    state.ensure_damage(28 * 4);
    assert_eq!(state.hull, [112, 112]);
    assert_eq!(state.shields, [0, 0]);
    screen.hud = PlayerHud {
        shields: state.shields,
        hull: state.hull,
        set_speed: Some(0.0),
        actual_speed: 0.0,
        altitude: Some(0),
        registration: "Hope".to_owned(),
        autopilot: false,
        turret: None,
    };
    screen.systems = ShipSystems::default().with_loadout(&state.loadout);
    assert_eq!(screen.systems.general, 0x4024_0020);
    let views = ViewState {
        console: ConsoleState {
            status_page: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    // The empty page/HUD baseline for the two checks.
    let mut empty = Screen::new(&cache).unwrap();
    empty.hud = PlayerHud {
        set_speed: Some(0.0),
        ..PlayerHud::default()
    };
    draw(&mut empty, &state, views);
    let before = empty.logical_pixels().to_vec();

    draw(&mut screen, &state, views);
    let after = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-saker-default.ppm");

    // F1099's hull row draws 100.0% from 112/112; the shields row stays clear
    // because +0xe0 == +0xe2 (F1099 suppresses a pristine/absent shield).
    assert!(
        changed(&before, &after, 2, 150, 11, 20) > 20,
        "the Saker's hull line did not draw"
    );
    assert_eq!(changed(&before, &after, 2, 150, 1, 10), 0);
    // F1120's two missiles are KL760 0x82 words: image 2 on records 0x3b/0x3c.
    let with_empty = {
        let mut screen = Screen::new(&cache).unwrap();
        screen.hud = PlayerHud {
            shields: [0, 0],
            hull: [112, 112],
            set_speed: Some(0.0),
            actual_speed: 0.0,
            altitude: Some(0),
            registration: "Hope".to_owned(),
            autopilot: false,
            turret: None,
        };
        screen.systems = ShipSystems::default();
        draw(&mut screen, &state, views);
        screen.logical_pixels().to_vec()
    };
    assert!(changed(&with_empty, &after, 14, 30, 165, 176) > 20);
    assert!(changed(&with_empty, &after, 33, 50, 164, 175) > 20);
    assert_eq!(changed(&with_empty, &after, 0, 10, 168, 178), 0);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_equipment_page_draws_the_new_games_real_missile_icons() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState {
        console: ConsoleState {
            status_page: true,
            ..ConsoleState::default()
        },
        ..ViewState::default()
    };
    // The page with no equipment: only the 60x45 diagram.
    screen.systems = ShipSystems::default();
    draw(&mut screen, &state, views);
    let empty = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-equipment-empty.ppm");
    // F998's default loadout: general bits 0x40240020 and two 0x82 missiles,
    // which F1120 draws as image 2 (bitmap 346) on records 0x3b and 0x3c.
    screen.systems = ShipSystems::default().with_loadout(&ffe_core::equipment::Loadout::new_game());
    draw(&mut screen, &state, views);
    let loaded = screen.logical_pixels().to_vec();
    save(&screen, "dashboard-equipment-newgame.ppm");
    assert!(
        changed(&empty, &loaded, 14, 30, 165, 176) > 20,
        "0x3b at (14,165) did not draw the first missile icon"
    );
    assert!(
        changed(&empty, &loaded, 33, 50, 164, 175) > 20,
        "0x3c at (33,164) did not draw the second missile icon"
    );
    assert_eq!(
        changed(&empty, &loaded, 0, 10, 168, 178),
        0,
        "0x3f at (1,168) has no missile"
    );
}
