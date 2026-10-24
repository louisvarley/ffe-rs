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
    assert!(capacity > 0, "capacity {capacity}");
    // Buy a tonne of water (the cheapest).
    let water = rows[0].good;
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, true, 1).unwrap();
    assert!(text.starts_with("Bought 1 t of Water"), "{text}");
    assert_eq!(state.hold[water], 1);
    assert_eq!(state.credits, 1000 - rows[0].price as u32);
    let after = trade::listing(&mut nav, &cache, station, &state).unwrap();
    assert_eq!(after[0].stock, rows[0].stock - 1);
    assert_eq!(after[0].held, 1);
    // The hold fills: ten buys stop at the capacity.
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, true, 1000).unwrap();
    let held = i32::from(state.hold[water]);
    assert_eq!(held, capacity.min(1 + 1000).min(rows[0].stock), "{text}");
    assert!(text.contains("Your cargo hold is full") || held == rows[0].stock);
    // Selling returns the cash at the current price and the stock to the station.
    let price = after[0].price;
    let credits = state.credits;
    let text = trade::trade(&mut nav, &cache, &mut state, station, water, false, 2).unwrap();
    assert!(text.starts_with("Sold 2 t of Water"), "{text}");
    assert_eq!(state.credits, credits + 2 * price as u32);
    // Selling what is not held is refused and changes nothing.
    let before = state.clone();
    let none = rows[1].good;
    let text = trade::trade(&mut nav, &cache, &mut state, station, none, false, 1).unwrap();
    assert!(text.contains("none to sell"), "{text}");
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
    // The hold is saved and loads back; a hold of the wrong size is refused.
    state.validate().unwrap();
    let json = serde_json::to_string(&state).unwrap();
    let loaded: Flight = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.hold, state.hold);
    let mut bad = state.clone();
    bad.hold = vec![1, 2, 3];
    assert!(bad.validate().is_err());
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
            selected,
            cash: state.credits as i32,
            used: 3,
            capacity: 5,
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
}
