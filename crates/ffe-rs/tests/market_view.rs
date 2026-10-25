use ffe_core::market::{GOODS, ILLEGAL, NAMES};
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::Navigator;
use ffe_rs::screen::{MarketRow, MarketView, Screen};
use ffe_rs::trade;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// Buying and selling at the starting station move the cash, the hold and the stock as the
/// original's F555/F556 do, a day's drift changes the market, and the hold survives a save.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn trading_at_the_first_station() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let mut state = Flight::default();
    assert_eq!(state.credits, 1000);
    let rows = trade::listing(&mut nav, &cache, station, &state).unwrap();
    assert!(
        rows.len() >= 20 && rows.len() < 33,
        "{} legal goods",
        rows.len()
    );
    assert_eq!(rows[0].name, "Water");
    let capacity = nav.cargo_space(&cache, state.ship);
    assert_eq!(capacity, 5, "the Eagle's 5 t hold");
    // Buy a tonne of water (the cheapest).
    let water = rows[0].good;
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, true, 1).unwrap();
    assert_eq!(text, "Buying Water.");
    assert_eq!(state.hold[water], 1);
    assert_eq!(state.credits, 1000 - rows[0].price as u32);
    let after = trade::listing(&mut nav, &cache, station, &state).unwrap();
    assert_eq!(after[0].stock, rows[0].stock - 1);
    assert_eq!(after[0].held, 1);
    // The hold fills: ten buys stop at the capacity.
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, true, 1000).unwrap();
    let held = i32::from(state.hold[water]);
    assert_eq!(held, capacity.min(1 + 1000).min(rows[0].stock), "{text}");
    assert!(
        text == "Your cargo hold is full." || held == rows[0].stock,
        "{text}"
    );
    // Selling returns the cash at the current price and the stock to the station.
    let price = after[0].price;
    let credits = state.credits;
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, false, 2).unwrap();
    assert_eq!(text, "Selling Water.");
    assert_eq!(state.credits, credits + 2 * price as u32);
    // Selling what is not held is refused and changes nothing.
    let before = state.clone();
    let none = rows[1].good;
    let text = trade::trade(&mut nav, &cache, &mut state, station, none, false, 1).unwrap();
    assert_eq!(text, format!("You have no {} to sell.", rows[1].name));
    assert_eq!(state, before);
    // A day later the market has drifted.
    let first = trade::listing(&mut nav, &cache, station, &state).unwrap();
    nav.advance(&cache, 3.0 * 86_400.0);
    let later = trade::listing(&mut nav, &cache, station, &state).unwrap();
    assert_ne!(first, later, "three days of drift");
    // The same market is generated for the same system and station in a new run.
    let mut again = Navigator::new(&cache).unwrap();
    let fresh = trade::listing(&mut again, &cache, station, &Flight::default()).unwrap();
    assert_eq!(fresh[0].price, rows[0].price);
    assert_eq!(fresh[5].stock, rows[5].stock);
    // F633 fires once per game day: no drift within a day, one drift across it.
    let a = trade::listing(&mut again, &cache, station, &Flight::default()).unwrap();
    let b = trade::listing(&mut again, &cache, station, &Flight::default()).unwrap();
    assert_eq!(a, b, "same day, same market");
    again.advance(&cache, 0.5 * 86_400.0);
    assert_eq!(
        trade::listing(&mut again, &cache, station, &Flight::default()).unwrap(),
        a,
        "half a day does not drift"
    );
    again.advance(&cache, 0.5 * 86_400.0);
    assert_ne!(
        trade::listing(&mut again, &cache, station, &Flight::default()).unwrap(),
        a,
        "the first day's drift"
    );
    // The hold is saved and loads back; a hold of the wrong size is refused.
    state.validate().unwrap();
    let json = serde_json::to_string(&state).unwrap();
    let loaded: Flight = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.hold, state.hold);
    let mut bad = state.clone();
    bad.hold = vec![1, 2, 3];
    assert!(bad.validate().is_err());
}

/// F454's black market: the legal list hides eight illegal commodities at Sol,
/// F538's `0x80` mask lists all thirty-three, and a row action at F640's fake
/// advertisement is F542/F543's 500.0-credit police fine with no tonne moved.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_black_market_lists_illegal_goods_and_the_fake_advertisement_fines() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let state = Flight::default();
    let flags = nav.stock_flags(&cache).unwrap();
    let legal = trade::listing(&mut nav, &cache, station, &state).unwrap();
    let all = trade::listing_with(&mut nav, &cache, station, &state, true).unwrap();
    assert_eq!(legal.len(), 25, "Sol's legal goods");
    assert_eq!(all.len(), GOODS, "the black market lists every commodity");
    assert!(trade::illegal_available(&nav, &cache).unwrap());
    assert!(
        all.iter().any(|row| row.name == "Narcotics"),
        "Narcotics is an illegal Sol commodity"
    );
    assert!(
        legal.iter().all(|row| row.name != "Narcotics"),
        "the ordinary list hides it"
    );
    // Real quantity and price, and the buy/sell state effects. Pick an illegal
    // good the new game's 100.0 CR can afford.
    let mut state = Flight::default();
    let mut nav2 = Navigator::new(&cache).unwrap();
    let station2 = nav2.docked.unwrap();
    let before = trade::listing_with(&mut nav2, &cache, station2, &state, true).unwrap();
    let row = before
        .iter()
        .find(|row| row.price <= 1000 && row.stock > 0 && flags[row.good] & ILLEGAL != 0)
        .expect("an affordable illegal Sol commodity");
    let good = row.good;
    let name = row.name.clone();
    let price = row.price;
    let stock = row.stock;
    let text = trade::trade(&mut nav2, &cache, &mut state, station2, good, true, 1).unwrap();
    assert_eq!(text, format!("Buying {name}."));
    assert_eq!(state.hold[good], 1);
    assert_eq!(state.credits, (1000i64 - i64::from(price)) as u32);
    let after = trade::listing_with(&mut nav2, &cache, station2, &state, true).unwrap();
    assert_eq!(after[good].stock, stock - 1);
    assert_eq!(after[good].held, 1);

    // F640's fake advertisement: F542 records offence 5 and F543 charges 5000
    // tenths (500.0 CR) to the starport's ledger; no cash or cargo moves.
    let cash = state.credits;
    let hold = state.hold.clone();
    let ordinal = nav2.starport_ordinal(station2);
    let report = trade::police_sting(&mut nav2, station2);
    assert_eq!(
        report.message,
        "Trading in illegal goods is not permitted here. Pay a $500.0 fine."
    );
    assert_eq!(nav2.legal.offences, 1);
    assert_eq!(nav2.legal.owed(Some(ordinal)), 5_000);
    assert_eq!(state.credits, cash);
    assert_eq!(state.hold, hold);
    assert!(
        !nav2.legal.wanted,
        "offence 5's flags leave the wanted bit clear"
    );
    // F454's 0x98d7 title is the 0x920d company name, from the ad's seed.
    assert!(nav2.company_name(&cache, 0x1234).is_some());
}

/// F233's event-5 smuggling scan on a real system: a clean hold is never
/// caught; a hold of illegal goods is caught on one of the danger rolls and
/// records the 2500/25000 smuggling offence.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_customs_scan_catches_illegal_cargo_and_records_offence_seven() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let flags = nav.stock_flags(&cache).unwrap();
    let illegal = (0..GOODS)
        .find(|good| flags[*good] & ILLEGAL != 0)
        .expect("Sol has illegal goods");
    let mut hold = vec![0u16; NAMES];
    hold[illegal] = 2;
    // A clean hold is never caught, whatever the draw.
    assert!(nav.smuggling_scan(&cache, &[0u16; NAMES]).is_none());
    let mut caught = None;
    for offences in 0..64 {
        nav.legal.offences = offences;
        if let Some(report) = nav.smuggling_scan(&cache, &hold) {
            caught = Some(report);
            break;
        }
    }
    let report = caught.expect("a danger-5 system eventually catches two tonnes");
    assert_eq!(report.fine, 2_500);
    assert_eq!(report.charged, 25_000);
    assert_eq!(
        report.message,
        "Smuggling illegal goods is not permitted here. Pay a $2500 fine."
    );
    let station = nav.docked.expect("docked at the start");
    assert_eq!(nav.legal.owed(Some(nav.starport_ordinal(station))), 25_000);
    assert!(!nav.legal.wanted);
}

/// The docked market screen, as the playable draws it (set `FFE_SHOT_DIR` to keep a frame).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_market_screen_draws() {
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.unwrap();
    let mut state = Flight::default();
    let _ = trade::trade(&mut nav, &cache, &mut state, station, 4, true, 3);
    let rows = trade::listing(&mut nav, &cache, station, &state).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    for selected in [0usize, 14] {
        screen.market = Some(MarketView {
            station: nav.bodies[station].name.clone(),
            system: nav.system_name().to_owned(),
            rows: rows
                .iter()
                .map(|r| MarketRow {
                    name: r.name.clone(),
                    price: r.price,
                    stock: r.stock,
                    held: r.held,
                })
                .collect(),
            scroll: selected.saturating_sub(12),
            selected,
            cash: state.credits as i32,
            used: 3,
            capacity: 5,
            black_market: false,
        });
        let state = Flight {
            mode: Mode::Docked,
            ..Flight::default()
        };
        screen.draw(
            &state,
            ViewState::default(),
            None,
            false,
            true,
            "Bought 3 t.",
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(
                std::path::Path::new(&dir).join(format!("market{selected}.ppm")),
                ppm,
            )
            .unwrap();
        }
    }
    // F454's black market: all thirty-three rows and the 0x98d7 company title.
    let all = trade::listing_with(&mut nav, &cache, station, &state, true).unwrap();
    let title = nav
        .company_name(&cache, 0x1234)
        .unwrap_or_else(|| "Black Market".to_owned());
    screen.market = Some(MarketView {
        station: title,
        system: nav.system_name().to_owned(),
        rows: all
            .iter()
            .map(|r| MarketRow {
                name: r.name.clone(),
                price: r.price,
                stock: r.stock,
                held: r.held,
            })
            .collect(),
        scroll: 0,
        selected: 0,
        cash: state.credits as i32,
        used: 3,
        capacity: 5,
        black_market: true,
    });
    screen.draw(
        &Flight {
            mode: Mode::Docked,
            ..Flight::default()
        },
        ViewState::default(),
        None,
        false,
        true,
        "Choose Buy or Sell to trade one unit.",
    );
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("market-black.ppm"), ppm).unwrap();
    }
}
