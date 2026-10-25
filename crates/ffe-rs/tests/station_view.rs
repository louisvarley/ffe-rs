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
    screen.draw(&state, ViewState::default(), None, true, message);
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
    // The reference list's Pt.Exchg column is the new game's Eagle (model 23,
    // +0xa 38, so 950 * 38 = 36100); the sandbox station's own stock may hold
    // cheaper hulls, which is why the row values can be negative here.
    let player_ship = 23;
    let station = nav.docked.expect("docked at the start");
    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    let name = nav.bodies[station].name.clone();
    let rows = trade::listing(&mut nav, &cache, station, &Flight::default()).unwrap();
    let hold = nav.cargo_space(&cache, player_ship);
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
        draw(&mut screen, Some(view), None, station::hint(id, 0, 0));
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
        black_market: false,
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

    // F622/F633's BBS: posts generated from DATA_006170 and the 0x981a..0x982d
    // templates, four 33-pixel rows a page and the original "More..." arrow.
    let (posts, hulls) = {
        let offers = nav
            .station_offers(&cache, station)
            .expect("F622's BBS and hull stock");
        (offers.posts.clone(), offers.rows(player_ship, None))
    };
    assert!(
        posts.len() > 1,
        "the generated BBS is populated ({} posts)",
        posts.len()
    );
    assert!(
        posts.iter().all(|post| !post.text.is_empty()),
        "every post has the original template's text"
    );
    assert!(
        posts.iter().all(|post| !post.text.contains('[')),
        "every 0xff code in the templates expanded (no [0x....] fallbacks)"
    );
    assert!(
        posts.iter().any(|post| post.code == 0x9829
            || post.code == 0x9827
            || post.code == 0x981a
            || post.code == 0x981b),
        "the reference frame's goods, military and ship-job notices"
    );
    let bulletin_rows: Vec<DockRow> = posts
        .iter()
        .map(|post| DockRow {
            main: post.text.clone(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        })
        .collect();
    let bulletin = DockView {
        screen: StationScreen::Bulletin,
        title: station::title(StationScreen::Bulletin, &name),
        rows: bulletin_rows.clone(),
        status,
        scroll: 0,
        selected: 0,
        total: bulletin_rows.len(),
    };
    draw(
        &mut screen,
        Some(bulletin),
        None,
        station::hint(StationScreen::Bulletin, posts.len(), hulls.len()),
    );
    assert_eq!(
        station::hint(StationScreen::Bulletin, 0, 0),
        "Sorry, no advertisements have been placed."
    );
    assert_ne!(
        screen.logical_pixels()[16 * 320 + 140],
        screen.logical_pixels()[2 * 320 + 120],
        "BBS post text differs from the header"
    );
    // Force the goods/company path with the real name table: DATA_006171's
    // companion post is "GOODS BOUGHT AND SOLD: At <company>..." and the
    // company is F791's name generator, not invented text.
    {
        use ffe_core::bulletin::{self, World};
        use ffe_core::names::Names;
        use ffe_core::npc::Random;
        let names = Names::new(cache.asset("base:text/names").unwrap()).unwrap();
        let world = World {
            code: 1,
            name: nav.system_name(),
            population: 9,
            danger: 0,
            allegiance: 2,
            station_name: &name,
            destinations: &[],
            names: Some(&names),
        };
        let mut rng = Random::new(0x9876, 0x5432);
        let generated = bulletin::generate(&mut rng, &world, 0);
        assert!(
            generated.iter().any(|post| post.code == 0x9829),
            "the goods generator runs over a Federal board"
        );
        assert!(
            generated.iter().all(|post| !post.text.contains('[')),
            "the company/passage expansion handles every code"
        );
    }
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
        fuel_scoop_mounting: true,
        drive_removable: true,
    };
    outfitting::remove(
        21,
        tech_level,
        &mut loadout,
        &mut combat,
        &mut credits,
        limits,
    )
    .unwrap();
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

    // Fitting evidence: the drive, mining-machine and cabin rows F383 offers at
    // Ross 154's tech level. The Saker's F998 drive is Class 1 (code 2), so its
    // row starts fitted; the mining rows share DATA_009104's list.
    for (row_name, record, mass) in [
        ("Interplanetary drive", 31usize, 4u32),
        ("MB4 Mining Machine", 32, 30),
        ("Extra Passenger Cabin", 25, 5),
    ] {
        let row = rows
            .iter()
            .find(|row| row.name == row_name)
            .unwrap_or_else(|| panic!("the reference list has {row_name}"));
        assert_eq!(row.index, record, "{row_name}");
        assert_eq!(row.mass, mass, "{row_name} Wt column");
    }
    assert!(
        rows.iter()
            .find(|row| row.name == "Class 1 Hyperdrive")
            .unwrap()
            .fitted,
        "F998 fits drive class 2"
    );
    for (row_name, shot) in [
        ("Interplanetary drive", "Upgrades-drive.ppm"),
        ("MB4 Mining Machine", "Upgrades-mining.ppm"),
        ("Extra Passenger Cabin", "Upgrades-cabin.ppm"),
    ] {
        let at = rows.iter().position(|row| row.name == row_name).unwrap();
        draw(
            &mut screen,
            Some(upgrade_view(&name, &rows, status, at.saturating_sub(6))),
            None,
            station::UPGRADES_HINT,
        );
        save(&screen, shot);
        assert!(
            screen.logical_pixels().iter().any(|pixel| *pixel != 0),
            "{shot} drew"
        );
    }
    // A scripted fit -> effect -> remove: F410 takes the delivered drive and its
    // energy pool, F409 fits the Class 2 Hyperdrive (ship+0xd0 = 3), and the
    // energy gauge scale follows the class before the drive is put back.
    let sequence = Limits {
        free_space: 1_000,
        ..limits
    };
    let class_one = rows
        .iter()
        .find(|row| row.name == "Class 1 Hyperdrive")
        .unwrap()
        .index;
    let class_two = rows
        .iter()
        .find(|row| row.name == "Class 2 Hyperdrive")
        .unwrap()
        .index;
    combat.energy = 0x2000_0000;
    let old_gauge = ffe_core::laser::energy_gauge(combat.energy, 2);
    let mut log = Vec::new();
    log.push((
        "remove Class 1",
        outfitting::remove(
            class_one,
            tech_level,
            &mut loadout,
            &mut combat,
            &mut credits,
            sequence,
        )
        .unwrap(),
    ));
    assert_eq!(combat.drive, 0);
    assert_eq!(combat.energy, 0, "F410 clears ship+0x11e");
    log.push((
        "fit Class 2",
        outfitting::fit(
            class_two,
            tech_level,
            &mut loadout,
            &mut combat,
            &mut credits,
            sequence,
        )
        .unwrap(),
    ));
    assert_eq!(loadout.drive, 3);
    assert_eq!(combat.drive, 3);
    let new_gauge = ffe_core::laser::energy_gauge(0x2000_0000, 3);
    assert_ne!(new_gauge, old_gauge, "F270 scales the gauge by drive class");
    combat.energy = 0x2000_0000;
    log.push((
        "remove Class 2",
        outfitting::remove(
            class_two,
            tech_level,
            &mut loadout,
            &mut combat,
            &mut credits,
            sequence,
        )
        .unwrap(),
    ));
    log.push((
        "fit Class 1 again",
        outfitting::fit(
            class_one,
            tech_level,
            &mut loadout,
            &mut combat,
            &mut credits,
            sequence,
        )
        .unwrap(),
    ));
    assert_eq!(loadout.drive, 2);
    assert_eq!(combat.drive, 2);
    for (step, message) in &log {
        println!("{step}: {message}");
    }

    // F437's New and Reconditioned Ships list: the generated hull stock with
    // Cost, Pt.Exchg and Cap. The reference Sirocco frame's Federal shipyard
    // hulls (Spar 71000/34900/37t, Hawk 48000/11900/7t, ...) come from the same
    // table; the port's random stock names differ but the prices/capacities do
    // not.
    assert!(
        !hulls.is_empty(),
        "F622's hull stock is populated ({} hulls)",
        hulls.len()
    );
    assert!(
        hulls.len() <= ffe_core::shipyard::MAX_HULLS,
        "F634's thirteen-hull cap"
    );
    let ships_rows: Vec<DockRow> = hulls
        .iter()
        .map(|hull| DockRow {
            main: hull.name.to_owned(),
            detail: None,
            columns: vec![
                (station::SHIP_COST_X, format!("${}", hull.price), 0x7d),
                (
                    station::SHIP_EXCHANGE_X,
                    if hull.net < 0 {
                        format!("-${}", -hull.net)
                    } else {
                        format!("${}", hull.net)
                    },
                    0x7d,
                ),
                (station::SHIP_CAPACITY_X, format!("{}t", hull.cargo), 0x7d),
            ],
            fitted: false,
        })
        .collect();
    let ships = DockView {
        screen: StationScreen::Ships,
        title: station::title(StationScreen::Ships, &name),
        rows: ships_rows.clone(),
        status,
        scroll: 0,
        selected: 0,
        total: ships_rows.len(),
    };
    let before = screen.logical_pixels().to_vec();
    draw(
        &mut screen,
        Some(ships),
        None,
        station::hint(StationScreen::Ships, posts.len(), hulls.len()),
    );
    assert_ne!(
        &before[2 * 320 + 120..2 * 320 + 200],
        &screen.logical_pixels()[2 * 320 + 120..2 * 320 + 200],
        "ships header"
    );
    assert_ne!(
        screen.logical_pixels()[16 * 320 + 228],
        screen.logical_pixels()[2 * 320 + 120],
        "the cost column differs from the header"
    );
    assert_eq!(
        station::hint(StationScreen::Ships, 0, 0),
        station::SHIPS_EMPTY
    );
    save(&screen, "Ships-ours.ppm");

    // F438/F442: buy the first hull with the new game's Eagle part-exchanged.
    // The purchase charges ten times the whole-credit net price, installs the
    // model, clears the fitted items and replaces the row with the old hull.
    let target = hulls[0];
    let cash_before = 1_000_000i64;
    let purchase = {
        let offers = nav.station_offers(&cache, station).unwrap();
        offers.buy(0, player_ship, cash_before).unwrap()
    };
    assert_eq!(purchase.old_model, player_ship);
    assert_eq!(purchase.model, target.model);
    assert_eq!(
        purchase.cash_tenths,
        cash_before - target.net * 10,
        "ten times the net whole-credit price"
    );
    let after_hulls = nav
        .station_offers(&cache, station)
        .unwrap()
        .rows(player_ship, None);
    assert_eq!(
        after_hulls[0].model, player_ship,
        "the bought row now holds the player's old hull"
    );
    let after_rows: Vec<DockRow> = after_hulls
        .iter()
        .map(|hull| DockRow {
            main: hull.name.to_owned(),
            detail: None,
            columns: vec![
                (station::SHIP_COST_X, format!("${}", hull.price), 0x7d),
                (station::SHIP_EXCHANGE_X, format!("${}", hull.net), 0x7d),
                (station::SHIP_CAPACITY_X, format!("{}t", hull.cargo), 0x7d),
            ],
            fitted: false,
        })
        .collect();
    let after = DockView {
        screen: StationScreen::Ships,
        title: station::title(StationScreen::Ships, &name),
        rows: after_rows,
        status: station::Status {
            cash: i32::try_from(purchase.cash_tenths).unwrap(),
            ..status
        },
        scroll: 0,
        selected: 0,
        total: after_hulls.len(),
    };
    draw(
        &mut screen,
        Some(after),
        None,
        "Congratulations on your purchase. Call our upgrade department for the latest in additional equipment.",
    );
    save(&screen, "Ships-buy.ppm");

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
        station::hint(StationScreen::Repairs, 0, 0),
        station::REPAIRS_HINT
    );
    save(&screen, "Repairs-ours.ppm");
}

/// The repairs list with damage and after a full repair (the slice's frame
/// evidence): the two hull rows, the destroyed Main thruster and the damaged
/// Fuel Scoop appear from real damage state, then disappear.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn repair_rows_show_damage_and_clear_after_repair() {
    let cache = cache();
    let name = "Sirocco Station";
    let status = station::Status {
        cargo_used: 0,
        cargo_capacity: 2,
        cash: 100_000,
        cabins_used: 0,
        cabins_total: 0,
    };
    let mut screen = Screen::new(&cache).unwrap();
    for (label, hull, equipment, thrusters) in [
        (
            "Repairs-damaged.ppm",
            [140, 152],
            1 << 20,
            [false, false, false, false, true, false],
        ),
        ("Repairs-repaired.ppm", [152, 152], 0, [false; 6]),
    ] {
        let repairs = station::repairs_for(
            38,
            true,
            hull,
            equipment,
            thrusters,
            "1-Jan-3250",
            "1-Jan-3250",
        );
        let tiles = DockView {
            screen: StationScreen::Repairs,
            title: station::title(StationScreen::Repairs, name),
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
        draw(&mut screen, Some(tiles), None, station::REPAIRS_HINT);
        save(&screen, label);
    }
}

/// A generated mission offer, the journal after accepting it, and the comms
/// page it wrote. Local-only: needs the content cache; set `FFE_SHOT_DIR` to
/// keep `MissionOffer-ours.ppm`, `Journal-ours.ppm` and `Comms-ours.ppm`.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn mission_offer_journal_and_comms_frames() {
    use ffe_core::missions::{self, Kind};

    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let name = nav.bodies[station].name.clone();
    let posts = nav
        .station_offers(&cache, station)
        .expect("F622's BBS")
        .posts
        .clone();
    let (row, post) = posts
        .iter()
        .enumerate()
        .find(|(_, post)| matches!(post.code, 0x981a | 0x981b | 0x981f..=0x9822))
        .map(|(index, post)| (index, post.clone()))
        .expect("the generated board has a contract offer");
    assert!(
        matches!(post.code, 0x981a | 0x981b | 0x981f..=0x9822),
        "{:#06x}",
        post.code
    );
    let offer = nav
        .mission_offer(&cache, station, row)
        .unwrap()
        .expect("a contract offer");
    assert!(offer.reward > 0);
    assert!(!offer.destination_name.is_empty());

    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    let status = station::Status {
        cargo_used: 0,
        cargo_capacity: 1,
        cash: 1000,
        ..station::Status::default()
    };
    let offer_view = DockView {
        screen: StationScreen::Bulletin,
        title: station::title(StationScreen::Bulletin, &name),
        rows: vec![DockRow {
            main: post.text.clone(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        }],
        status,
        scroll: 0,
        selected: 0,
        total: 1,
    };
    let before = screen.logical_pixels().to_vec();
    let mut state = Flight {
        mode: Mode::Docked,
        ..Flight::default()
    };
    screen.dock = Some(offer_view);
    screen.draw(&state, ViewState::default(), None, false, &post.text);
    // The offer's text starts at (140, 16), under the title at (4,6).
    assert_ne!(
        &before[16 * 320 + 140..16 * 320 + 300],
        &screen.logical_pixels()[16 * 320 + 140..16 * 320 + 300],
        "mission offer text"
    );
    save(&screen, "MissionOffer-ours.ppm");

    let accepted = state
        .missions
        .accept(
            &offer,
            nav.day(),
            nav.current_system_code(),
            nav.system_name(),
        )
        .unwrap();
    assert_eq!(
        state.missions.missions[0].kind, offer.kind,
        "the contract keeps the offer's type"
    );
    let journal = state.missions.journal.last().unwrap().clone();
    let journal_view = DockView {
        screen: StationScreen::Journal,
        title: station::title(StationScreen::Journal, &name),
        rows: vec![DockRow {
            main: journal.text.clone(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        }],
        status,
        scroll: 0,
        selected: 0,
        total: 1,
    };
    let before = screen.logical_pixels().to_vec();
    screen.dock = Some(journal_view);
    screen.draw(
        &state,
        ViewState::default(),
        None,
        false,
        station::hint(StationScreen::Journal, 0, 0),
    );
    assert_ne!(
        &before[6 * 320 + 4..6 * 320 + 160],
        &screen.logical_pixels()[6 * 320 + 4..6 * 320 + 160],
        "journal title"
    );
    assert_ne!(
        &before[16 * 320 + 4..16 * 320 + 316],
        &screen.logical_pixels()[16 * 320 + 4..16 * 320 + 316],
        "journal entry"
    );
    save(&screen, "Journal-ours.ppm");

    let page = state.missions.pages.last().unwrap().clone();
    let comms_view = DockView {
        screen: StationScreen::Comms,
        title: station::title(StationScreen::Comms, &name),
        rows: vec![DockRow {
            main: page.text.clone(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        }],
        status,
        scroll: 0,
        selected: 0,
        total: 1,
    };
    let before = screen.logical_pixels().to_vec();
    screen.dock = Some(comms_view);
    screen.draw(
        &state,
        ViewState::default(),
        None,
        false,
        station::hint(StationScreen::Comms, 0, 0),
    );
    assert_ne!(
        &before[6 * 320 + 4..6 * 320 + 160],
        &screen.logical_pixels()[6 * 320 + 4..6 * 320 + 160],
        "comms title"
    );
    // The acceptance page is the same source text the journal just drew, so
    // the row pixels are expected to match; the title above is the change the
    // screen makes.
    assert!(!page.text.is_empty());
    save(&screen, "Comms-ours.ppm");

    // The acceptance page is 0x992a's source text and names the destination.
    assert!(accepted.text.contains(&offer.destination_name));
    assert_eq!(page.kind, missions::PageKind::Message);
    assert_eq!(state.missions.missions[0].status, missions::Status::Active);
    assert_eq!(Kind::Passenger, offer.kind);

    // F493's military list (the 0x9827/0x9828 ad's target page). The tier-0
    // row is F632's 0x9854 description.
    let offers = nav
        .military_offers(&cache, station, 0)
        .expect("F493's list");
    assert_eq!(offers.len(), 1);
    let military = DockView {
        screen: StationScreen::Military,
        title: station::title(StationScreen::Military, &name),
        rows: offers
            .iter()
            .map(|offer| DockRow {
                main: offer.text.clone(),
                detail: None,
                columns: Vec::new(),
                fitted: false,
            })
            .collect(),
        status,
        scroll: 0,
        selected: 0,
        total: offers.len(),
    };
    let before = screen.logical_pixels().to_vec();
    draw(
        &mut screen,
        Some(military),
        None,
        station::hint(StationScreen::Military, offers.len(), 0),
    );
    assert_ne!(
        &before[16 * 320 + 140..16 * 320 + 310],
        &screen.logical_pixels()[16 * 320 + 140..16 * 320 + 310],
        "military mission text"
    );
    save(&screen, "Military-ours.ppm");

    // F306's rank page after a military package delivery: 0x9c39's source text
    // with the new rank name (the medal page is skipped because F509's package
    // basic_data has no medal code).
    let military_offer = offers[0].clone();
    let accepted = state
        .missions
        .accept(
            &military_offer,
            nav.day(),
            nav.current_system_code(),
            nav.system_name(),
        )
        .unwrap();
    let outcome = state
        .missions
        .settle(accepted.id, nav.day() + 1, military_offer.destination)
        .unwrap();
    assert_eq!(outcome.status, missions::Status::Complete);
    let rank = state
        .missions
        .pages
        .iter()
        .find(|page| page.kind == missions::PageKind::Rank)
        .expect("F306's rank page")
        .clone();
    let comms_rank = DockView {
        screen: StationScreen::Comms,
        title: station::title(StationScreen::Comms, &name),
        rows: vec![DockRow {
            main: rank.text.clone(),
            detail: None,
            columns: Vec::new(),
            fitted: false,
        }],
        status,
        scroll: 0,
        selected: 0,
        total: 1,
    };
    let before = screen.logical_pixels().to_vec();
    screen.dock = Some(comms_rank);
    screen.draw(
        &state,
        ViewState::default(),
        None,
        false,
        station::hint(StationScreen::Comms, 0, 0),
    );
    assert_ne!(
        &before[16 * 320 + 4..16 * 320 + 316],
        &screen.logical_pixels()[16 * 320 + 4..16 * 320 + 316],
        "rank page text"
    );
    save(&screen, "Comms-rank.ppm");
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
