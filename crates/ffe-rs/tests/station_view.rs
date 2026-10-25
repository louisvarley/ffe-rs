//! The docked screens drawn from the real cache, checked against the layout
//! read from the DOS release's screens (`docs/notes/station.md`). Local-only:
//! needs `FFE_WORLD_CACHE`; set `FFE_SHOT_DIR` to keep the frames.
use ffe_core::equipment::Loadout;
use ffe_rs::camera::ViewState;
use ffe_rs::combat::Combat;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::Navigator;
use ffe_rs::outfitting::{self, Limits};
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

/// The Upgrades DockView F327/F329 draw from F383's rows.
fn upgrade_view(
    station: &str,
    rows: &[station::UpgradeRow],
    status: station::Status,
    scroll: usize,
) -> DockView {
    let visible = station::layout(StationScreen::Upgrades).visible();
    let first = scroll.min(rows.len().saturating_sub(visible));
    DockView {
        screen: StationScreen::Upgrades,
        title: station::title(StationScreen::Upgrades, station),
        rows: rows
            .iter()
            .skip(first)
            .take(visible)
            .map(|row| DockRow {
                main: row.name.to_owned(),
                detail: None,
                columns: vec![
                    (station::UPGRADE_PRICE_X, format!("{}", row.price), 0x7d),
                    (station::UPGRADE_REMOVAL_X, format!("{}", row.removal), 0x7d),
                    (station::UPGRADE_MASS_X, format!("{}t", row.mass), 0x7d),
                ],
                fitted: row.fitted,
            })
            .collect(),
        status,
        scroll: first,
        selected: 0,
        total: rows.len(),
    }
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
                fitted: false,
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

    // F869/F622's starport tech level: Ross 154's record byte +5 is 0xfd, the
    // value the reference Sirocco frame's Upgrades list reflects (record 0's
    // 0xff is above the station, so XB74 Proximity Mine is the first row).
    let tech_level = nav.tech_level(&cache).expect("system tables (cache v15)");
    assert_eq!(tech_level, 0xfd, "DATA_008995");
    let mut loadout = Loadout::new_game();
    let mut combat = Combat::new_game();
    let rows = station::upgrades(tech_level, &loadout, &combat.lasers);
    assert_eq!(rows[0].name, "XB74 Proximity Mine");
    assert_eq!(rows.len(), 51, "the records at or below tech 0xfd");
    let view = upgrade_view(&name, &rows, status, 0);
    let before = screen.logical_pixels().to_vec();
    draw(&mut screen, Some(view), None, station::UPGRADES_HINT);
    assert_ne!(
        &before[16 * 320 + 152..16 * 320 + 240],
        &screen.logical_pixels()[16 * 320 + 152..16 * 320 + 240],
        "upgrades row"
    );
    assert_ne!(
        &before[16 * 320 + 242..16 * 320 + 260],
        &screen.logical_pixels()[16 * 320 + 242..16 * 320 + 260],
        "upgrades price column"
    );
    save(&screen, "Upgrades-ours.ppm");

    // F386's Fit/Remove on the Upgrades row: remove the new game's front 1MW
    // pulse laser, then buy the 5MW one; the frame shows the new fitted flag
    // (the Remove button on that row) and the Fitting console line.
    let five_mw = rows
        .iter()
        .find(|row| row.name == "5MW Pulse Laser")
        .copied()
        .expect("the reference frame lists the 5MW pulse laser");
    let mut credits = 1_000_000u32;
    let limits = Limits {
        mounts: 1,
        pylons: 2,
        free_space: hold,
    };
    outfitting::remove(21, tech_level, &mut loadout, &mut combat, &mut credits).unwrap();
    let message = outfitting::fit(
        five_mw.index,
        tech_level,
        &mut loadout,
        &mut combat,
        &mut credits,
        limits,
    )
    .unwrap();
    assert!(message.starts_with("Fitting 5MW Pulse Laser"));
    let rows = station::upgrades(tech_level, &loadout, &combat.lasers);
    assert!(
        rows.iter()
            .find(|row| row.name == "5MW Pulse Laser")
            .unwrap()
            .fitted
    );
    assert!(
        !rows
            .iter()
            .find(|row| row.name == "1MW Pulse Laser")
            .unwrap()
            .fitted
    );
    let fitted_at = rows
        .iter()
        .position(|row| row.name == "5MW Pulse Laser")
        .expect("the bought row is still offered");
    draw(
        &mut screen,
        Some(upgrade_view(
            &name,
            &rows,
            status,
            fitted_at.saturating_sub(6),
        )),
        None,
        &message,
    );
    save(&screen, "Upgrades-buy-laser.ppm");

    // The shipyard list has no importable generator in this cache: F437's
    // empty-state message (0x98ed) is the original text for no hulls.
    let ships = DockView {
        screen: StationScreen::Ships,
        title: station::title(StationScreen::Ships, &name),
        rows: Vec::new(),
        status,
        scroll: 0,
        selected: 0,
        total: 0,
    };
    let before = screen.logical_pixels().to_vec();
    draw(&mut screen, Some(ships), None, station::SHIPS_EMPTY);
    assert_ne!(
        &before[2 * 320 + 120..2 * 320 + 200],
        &screen.logical_pixels()[2 * 320 + 120..2 * 320 + 200],
        "ships header"
    );
    assert_eq!(station::hint(StationScreen::Ships, 0), station::SHIPS_EMPTY);
    save(&screen, "Ships-ours.ppm");

    // F444's three service rows for an undamaged, drive-fitted ship: the
    // reference frame's Sirocco costs (mass 38 -> 3040/1900/1900) and dates.
    let repairs = station::repairs(38, true, "1-Jan-3250", "1-Jan-3250");
    assert_eq!(
        repairs.iter().map(|row| row.cost).collect::<Vec<_>>(),
        [3040, 1900, 1900]
    );
    let tiles = DockView {
        screen: StationScreen::Repairs,
        title: station::title(StationScreen::Repairs, &name),
        rows: repairs
            .iter()
            .map(|row| DockRow {
                main: row.title.clone(),
                detail: None,
                columns: vec![(station::REPAIR_COST_X, format!("${}", row.cost), 0x7d)],
                fitted: false,
            })
            .collect(),
        status,
        scroll: 0,
        selected: 0,
        total: repairs.len(),
    };
    let before = screen.logical_pixels().to_vec();
    draw(&mut screen, Some(tiles), None, station::REPAIRS_HINT);
    assert_ne!(
        &before[16 * 320 + 140..16 * 320 + 300],
        &screen.logical_pixels()[16 * 320 + 140..16 * 320 + 300],
        "repairs first service row"
    );
    assert_ne!(
        &before[16 * 320 + 292..16 * 320 + 315],
        &screen.logical_pixels()[16 * 320 + 292..16 * 320 + 315],
        "repairs cost column"
    );
    assert_eq!(
        station::hint(StationScreen::Repairs, 0),
        station::REPAIRS_HINT
    );
    save(&screen, "Repairs-ours.ppm");
}

/// The default DOS start (Gateway/Hope, record 0) is landed at Old Blackelk in
/// state `0x30`. F4 at the pad is F343: the services board is the same screen
/// as a docked station's, titled with the starport. The reference capture
/// `docs/evidence/start-flow/original-services.png` is the executed original's
/// frame after `--game-frame 500 0x3e`.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn default_start_services_board_is_old_blackelk() {
    let cache = cache();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let station = nav.docked.expect("landed at the default start's starport");
    let name = nav.bodies[station].name.clone();
    assert_eq!(name, "Old Blackelk");
    assert_eq!(
        station::title(StationScreen::Services, &name),
        "Old Blackelk Services"
    );
    let state = nav.new_game.expect("a new game state");
    assert_eq!(state.player_state, 0x30, "F994's landed state");
    let status = station::Status {
        cargo_used: 0,
        cargo_capacity: nav.hold(&cache, state.ship),
        cash: state.cash,
        ..station::Status::default()
    };
    assert_eq!(status.cargo_capacity, 1, "the Saker's new-game hold");
    let rows = station::SERVICES
        .iter()
        .map(|entry| DockRow {
            main: entry.title.to_owned(),
            detail: entry.detail.map(str::to_owned),
            columns: Vec::new(),
            fitted: false,
        })
        .collect();
    let view = DockView {
        screen: StationScreen::Services,
        title: station::title(StationScreen::Services, &name),
        rows,
        status,
        scroll: 0,
        selected: 0,
        total: station::SERVICES.len(),
    };
    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    draw(&mut screen, Some(view), None, station::SERVICES_HINT);
    save(&screen, "start-services-ours.ppm");
}
