//! The docked screens drawn from the real cache, checked against the layout
//! read from the DOS release's screens (`docs/notes/station.md`). Local-only:
//! needs `FFE_WORLD_CACHE`; set `FFE_SHOT_DIR` to keep the frames.
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::Navigator;
use ffe_rs::screen::{MarketRow, MarketView, Screen};
use ffe_rs::station::{self, DockRow, DockView, Screen as StationScreen};
use ffe_rs::trade;

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

fn draw(screen: &mut Screen, dock: Option<DockView>, market: Option<MarketView>, message: &str) {
    screen.dock = dock;
    screen.market = market;
    let state = Flight {
        mode: Mode::Docked,
        ..Flight::default()
    };
    screen.draw(&state, ViewState::default(), None, false, true, message);
}

/// The services root, shipyard, bulletin board and market at the first station
/// draw at the original's positions and save a frame for the owner.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn docked_screens_draw_at_the_original_layout() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    let name = nav.bodies[station].name.clone();
    let rows = trade::listing(&mut nav, &cache, station, &Flight::default()).unwrap();
    let hold = nav.cargo_space(&cache, Flight::default().ship);
    let status = station::Status {
        cargo_used: 0,
        cargo_capacity: hold,
        cash: 1000,
        ..station::Status::default()
    };

    for (id, title) in [
        (
            StationScreen::Services,
            station::title(StationScreen::Services, &name),
        ),
        (
            StationScreen::Shipyard,
            station::title(StationScreen::Shipyard, &name),
        ),
        (
            StationScreen::Bulletin,
            station::title(StationScreen::Bulletin, &name),
        ),
    ] {
        let entries: &[station::Entry] = match id {
            StationScreen::Services => &station::SERVICES,
            StationScreen::Shipyard => &station::SHIPYARD,
            _ => &[],
        };
        let rows = entries
            .iter()
            .map(|entry| DockRow {
                main: entry.title.to_owned(),
                detail: entry.detail.map(str::to_owned),
                columns: Vec::new(),
            })
            .collect();
        let view = DockView {
            screen: id,
            title,
            rows,
            status,
            scroll: 0,
            selected: 0,
            total: entries.len(),
        };
        let before = screen.logical_pixels().to_vec();
        draw(&mut screen, Some(view), None, station::hint(id, 0));
        // The title and entry text differ from the untouched cabin at the same
        // coordinates (x 4..140, y 15..148).
        assert_ne!(
            &before[6 * 320 + 4..6 * 320 + 120],
            &screen.logical_pixels()[6 * 320 + 4..6 * 320 + 120],
            "{id:?} title"
        );
        if id == StationScreen::Services {
            // The first row's button (124, 15), its text (140, 16) and the
            // status label (1, 138) leave the untouched cabin.
            for (start, end, what) in [
                (15 * 320 + 124, 15 * 320 + 132, "row button"),
                (16 * 320 + 140, 16 * 320 + 220, "row text"),
                (138 * 320 + 1, 138 * 320 + 20, "status"),
            ] {
                assert_ne!(
                    &before[start..end],
                    &screen.logical_pixels()[start..end],
                    "{what}"
                );
            }
        }
        save(&screen, &format!("{id:?}-ours.ppm"));
    }

    // The stockmarket's header at y 2, rows from y 16, price at x 214.
    let market = MarketView {
        station: name.clone(),
        system: nav.system_name().to_owned(),
        rows: rows
            .iter()
            .map(|row| MarketRow {
                name: row.name.clone(),
                price: row.price,
                stock: row.stock,
                held: row.held,
            })
            .collect(),
        scroll: 0,
        selected: 0,
        cash: 1000,
        used: 0,
        capacity: hold,
    };
    let before = screen.logical_pixels().to_vec();
    draw(&mut screen, None, Some(market), station::MARKET_HINT);
    assert_ne!(
        &before[2 * 320 + 120..2 * 320 + 200],
        &screen.logical_pixels()[2 * 320 + 120..2 * 320 + 200],
        "market header"
    );
    assert_ne!(
        screen.logical_pixels()[16 * 320 + 152],
        screen.logical_pixels()[2 * 320 + 120],
        "row text and header differ"
    );
    save(&screen, "Market-ours.ppm");

    // The BBS with no generated entries draws the original empty state's title
    // and no rows; the console line is the original's 0x98de.
    let bulletin = DockView {
        screen: StationScreen::Bulletin,
        title: station::title(StationScreen::Bulletin, &name),
        rows: Vec::new(),
        status,
        scroll: 0,
        selected: 0,
        total: 0,
    };
    draw(&mut screen, Some(bulletin), None, station::BULLETIN_EMPTY);
    assert_eq!(
        station::hint(StationScreen::Bulletin, 0),
        "Sorry, no advertisements have been placed."
    );
    save(&screen, "Bulletin-ours.ppm");
}
