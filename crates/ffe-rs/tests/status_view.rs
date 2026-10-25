//! Local-only status screens: F312's F2 profile page and F552/F544/F545's
//! police contact, with the fine -> pay -> clean sequence.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! status_view -- --ignored --nocapture`
//!
//! Frames: `status-profile-clean.ppm`, `status-profile-offender.ppm`,
//! `status-police-fine.ppm`, `status-police-clean.ppm`.

use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::Navigator;
use ffe_rs::screen::{Screen, SystemShip};
use ffe_rs::station::{self, DockRow, DockView, Screen as StationScreen, Status};

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

fn flight_frame(screen: &mut Screen, viewer: &ModelViewer, nav: &Navigator, state: &Flight) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.ships = nav
        .ships
        .iter()
        .map(|ship| SystemShip {
            model: ship.model,
            centre: ship.position.map(|v| v as f64 / unit),
            axes: ship.axes(),
            flash: false,
            dead: false,
        })
        .collect();
    let views = ViewState::default();
    screen.profile = Some(nav.profile("Jameson"));
    screen.draw(
        state,
        views,
        Some(viewer),
        false,
        "Status: commander profile.",
    );
}

fn police_frame(screen: &mut Screen, nav: &Navigator, message: &str) {
    let fine = nav.fine_here();
    let row = if fine > 0 {
        DockRow {
            main: "Pay fine".to_owned(),
            detail: Some(format!(
                "${} fine",
                ffe_rs::trade::credits(fine.min(i32::MAX as u32) as i32)
            )),
            columns: Vec::new(),
            fitted: false,
        }
    } else {
        DockRow {
            main: "Hang up.".to_owned(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        }
    };
    screen.dock = Some(DockView {
        screen: StationScreen::Police,
        title: station::title(
            StationScreen::Police,
            nav.station().map_or("Station", |(n, _)| n),
        ),
        rows: vec![row],
        status: Status::default(),
        scroll: 0,
        selected: 0,
        total: 1,
    });
    let state = Flight {
        mode: Mode::Docked,
        ship: ffe_rs::navigation::NEW_GAME_SHIP,
        ..Flight::default()
    };
    let views = ViewState::default();
    screen.draw(&state, views, None, false, message);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_profile_and_fines_office_follow_the_source() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let seat = nav.new_game.expect("new game").seat;
    let state = Flight {
        mode: Mode::Flight,
        position: seat.map(|v| v as f64 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT),
        system_flight: true,
        ship: ffe_rs::navigation::NEW_GAME_SHIP,
        credits: 1_000,
        ..Flight::default()
    };

    // Ross 154 is Federal (`DATA_008996 = 2`), the reference F2 frame's start.
    assert_eq!(nav.jurisdiction, 2);
    let profile = nav.profile("Jameson");
    assert_eq!(profile.heading(), "FEDERAL");
    assert_eq!(profile.federal_rank, "None");
    assert_eq!(profile.imperial_title, "Outsider");
    assert_eq!(profile.elite_rating, "Harmless");
    assert_eq!(
        (
            profile.federal_status,
            profile.imperial_status,
            profile.interpol_status
        ),
        ("Clean", "Clean", "Clean")
    );
    assert!(profile.record.is_empty());
    flight_frame(&mut screen, &viewer, &nav, &state);
    save(&screen, "status-profile-clean.ppm");

    // F931/F941 + F236: the first shot at the Ross 154 pad records the 600 CR
    // offence as a 6000 tenth-credit debt in the Federal slot.
    let report = nav.player_fired(seat).expect("recorded at Ross 154");
    assert_eq!(report.offence.charged, 6_000);
    assert_eq!(nav.legal.fines, [0, 0, 6_000, 0, 0, 0]);
    assert!(nav.legal.wanted);
    let profile = nav.profile("Jameson");
    assert_eq!(profile.federal_status, "Criminal");
    assert_eq!(profile.imperial_status, "Clean");
    assert_eq!(profile.interpol_status, "Clean");
    assert_eq!(profile.record, vec!["Unlawful discharge of weapon"]);
    flight_frame(&mut screen, &viewer, &nav, &state);
    save(&screen, "status-profile-offender.ppm");

    // F552 opens the police contact with the fine outstanding.
    assert_eq!(nav.fine_here(), 6_000);
    police_frame(
        &mut screen,
        &nav,
        &ffe_core::status::police_greeting(nav.system_name(), nav.fine_here()),
    );
    save(&screen, "status-police-fine.ppm");

    // F545: 1000 tenths is less than the 6000 debt, so nothing is taken.
    let refused = nav.pay_fine(1_000);
    assert!(!refused.ok());
    assert_eq!(refused.amount, 6_000);
    assert_eq!(nav.legal.fines[2], 6_000);
    assert_eq!(
        ffe_core::status::fine_refused_message(refused.amount),
        "You've until midnight to pay $600.0 or be put in jail and your ship confiscated."
    );

    // F544: enough cash pays it and the record returns to Clean.
    let paid = nav.pay_fine(10_000);
    assert!(paid.ok());
    assert_eq!(paid.paid, 6_000);
    assert_eq!(nav.legal.fines, [0; 6]);
    let profile = nav.profile("Jameson");
    assert_eq!(
        (
            profile.federal_status,
            profile.imperial_status,
            profile.interpol_status
        ),
        ("Clean", "Clean", "Clean")
    );
    assert_eq!(profile.record, vec!["Unlawful discharge of weapon"]);
    police_frame(
        &mut screen,
        &nav,
        &ffe_core::status::fine_paid_message(paid.paid),
    );
    save(&screen, "status-police-clean.ppm");
}
