//! An end-to-end trade loop on the public playable APIs: buy hydrogen at the
//! starting starport, hyperspace to Titican, dock at Argent, buy water, launch,
//! jump home and sell the water at Old Blackelk.
//!
//! Local-only (needs `FFE_WORLD_CACHE`). `Titican` is a real neighbour of
//! Gateway in the galaxy tables (4.42 LY away, code `0x12a53716`, within the
//! Saker's 10.71 LY range); `Navigator::new_game` now offers the start system's
//! galaxy-table neighbours through `navigation::galaxy_around` (F852/F860/F832/
//! F878) within `NEIGHBOUR_REACH` (30 LY), so `Navigator::hyperspace(index)` has
//! an index for it and its generated bodies include the station `Argent`. When
//! a name is missing the test prints every system on offer with its stations and
//! starports and fails with that catalogue instead of guessing a substitute.
//! `reports_the_systems_and_stations_on_offer` prints the same catalogue.
//!
//! Run with:
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test trade_loop -- --ignored --nocapture`
use ffe_core::hyperspace::{HYDROGEN, MILITARY_CLASS, MILITARY_FUEL};
use ffe_core::market::NAMES;
use ffe_core::objects::ModelInfo;
use ffe_core::sector::{distance as star_distance, Strings, Tables as GalaxyTables};
use ffe_core::traffic::state as traffic_state;
use ffe_rs::docking::Contact;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{
    bodies, burn_fuel, clock_words, galaxy_around, start_entry, DriveState, Entry, Kind, Navigator,
    Traffic, NEIGHBOUR_REACH, PLAYER, START_SYSTEMS,
};
use ffe_rs::original_flight::{OriginalFlight, MM_PER_SECOND};
use ffe_rs::trade;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// The systems `Navigator::new_game(&cache, 0)` offers, in its own order:
/// `galaxy_around`'s hand-coded, test and Gateway-neighbour entries with F829
/// record 0 (Gateway) replacing one or appended (`navigation.rs` `galaxy_around`
/// and `new_game`).
fn systems(cache: &ffe_content::VerifiedCache) -> Vec<Entry> {
    let gateway = start_entry(cache, START_SYSTEMS[0]).expect("F829 record 0 (Gateway)");
    let mut entries = galaxy_around(cache, gateway.params.code, NEIGHBOUR_REACH)
        .expect("the galaxy list around Gateway");
    match entries
        .iter()
        .position(|entry| entry.params.code == gateway.params.code)
    {
        Some(index) => entries[index] = gateway,
        None => entries.push(gateway),
    }
    entries
}

/// The stations and starports of one system, for the stop message.
fn places_text(cache: &ffe_content::VerifiedCache, entry: &Entry, clock: f64) -> String {
    let list = bodies(cache, entry, clock_words(clock)).expect("the generated bodies");
    let places: Vec<String> = list
        .iter()
        .filter(|body| body.kind.is_place())
        .map(|body| format!("{} ({})", body.name, body.kind.label()))
        .collect();
    if places.is_empty() {
        "<no stations or starports>".to_owned()
    } else {
        places.join(", ")
    }
}

/// The real galaxy-table stars within `within` hundredths of a light year of
/// `start`, sorted by range. Used only to explain a missing system in the stop
/// message (Titican is in the tables but not on offer).
fn galaxy_neighbours(
    cache: &ffe_content::VerifiedCache,
    start: &Entry,
    within: i32,
) -> Vec<(i32, u32, String)> {
    let (Some(tables), Some(strings)) = (
        cache
            .asset("base:world/galaxy-tables")
            .and_then(|bytes| GalaxyTables::new(bytes).ok()),
        cache
            .asset("base:text/system-strings")
            .and_then(|bytes| Strings::new(bytes).ok()),
    ) else {
        return Vec::new();
    };
    let Some(home) = tables.system_position(start.params.code) else {
        return Vec::new();
    };
    let (hx, hy) = (
        (start.params.code & 0x1fff) as i32,
        ((start.params.code >> 13) & 0x1fff) as i32,
    );
    let mut found = Vec::new();
    for dx in -24..=24 {
        for dy in -24..=24 {
            let (x, y) = (hx + dx, hy + dy);
            if !(0..0x2000).contains(&x) || !(0..0x2000).contains(&y) {
                continue;
            }
            let count = tables.star_count(x as u32, y as u32, 0) >> 10;
            for star in 0..count.max(0) as u32 {
                let code = (x as u32 & 0x1fff) | (y as u32 & 0x1fff) << 13 | star << 26;
                let Some(position) = tables.system_position(code) else {
                    continue;
                };
                let away = star_distance(home, position);
                if away > within {
                    continue;
                }
                if let Some(name) = tables.system_name(&strings, code) {
                    found.push((away, code, String::from_utf8_lossy(&name).trim().to_owned()));
                }
            }
        }
    }
    found.sort();
    found
}

/// The index of `wanted` among the systems on offer, or the full catalogue of
/// what is generated (the task's stop rule: report, do not guess).
fn require_system(
    cache: &ffe_content::VerifiedCache,
    entries: &[Entry],
    clock: f64,
    wanted: &str,
) -> usize {
    if let Some(index) = entries.iter().position(|entry| entry.name == wanted) {
        return index;
    }
    let mut report = format!(
        "STOP: {wanted:?} is not one of the {} systems the navigator offers.\n\
         The generated systems and their stations/starports are:\n",
        entries.len()
    );
    for (index, entry) in entries.iter().enumerate() {
        report.push_str(&format!(
            "  {index:2}  {:<18} {}\n",
            entry.name,
            places_text(cache, entry, clock)
        ));
    }
    let generated: Vec<&str> = entries
        .iter()
        .filter(|entry| entry.name.starts_with("Test-"))
        .map(|entry| entry.name.as_str())
        .collect();
    report.push_str(&format!(
        "The generated neighbour systems are: {}.\n",
        generated.join(", ")
    ));
    if let Some(gateway) = entries.iter().find(|entry| entry.name == "Gateway") {
        if let Some((away, code, _)) = galaxy_neighbours(cache, gateway, 1_000)
            .into_iter()
            .find(|(_, _, name)| name.as_str() == wanted)
        {
            report.push_str(&format!(
                "{wanted} is a real neighbour of Gateway in base:world/galaxy-tables \
                 ({:.2} LY away, code {code:#010x}), but it is not a system on offer; \
                 Navigator has no public API to add one, so hyperspace(index) cannot \
                 reach it.\n",
                f64::from(away) / 100.0
            ));
        }
    }
    panic!("{report}Nothing was guessed.");
}

/// The index of a body of the current system by name, or every generated body
/// name (the stop rule for a missing station).
fn require_body(nav: &Navigator, wanted: &str) -> usize {
    match nav.bodies.iter().position(|body| body.name == wanted) {
        Some(index) => index,
        None => {
            let list: Vec<String> = nav
                .bodies
                .iter()
                .map(|body| format!("{} ({})", body.name, body.kind.label()))
                .collect();
            panic!(
                "STOP: {} has no body named {wanted:?}. Its generated bodies are: {}",
                nav.system_name(),
                list.join(", ")
            );
        }
    }
}

/// The commodity names in the market tables, indexed by slot. Water is resolved
/// from these names rather than a hard-coded index (`ffe_core::market::Tables::name`).
fn good_names(nav: &Navigator) -> Vec<String> {
    let tables = nav.market_tables().expect("the market tables");
    (0..NAMES)
        .map(|good| tables.name(good).unwrap_or("?").to_owned())
        .collect()
}

/// The hyperdrive's view of the player, as `main::drive_state` builds it
/// (`main.rs:6347-6374`): the model statistics' mass, the fitted drive class and
/// the fuel in the hold.
fn drive_state(cache: &ffe_content::VerifiedCache, state: &Flight) -> DriveState {
    let info = ModelInfo::read(
        cache.asset("base:model/records").expect("model records"),
        cache.asset("base:ship/stats").expect("ship statistics"),
        state.ship,
    )
    .expect("the player's model");
    let stats = ffe_rs::ships::ShipStats::from_block(&info.stats.expect("the model's statistics"))
        .expect("the statistics block");
    let class = i32::from(state.loadout.drive);
    let slot = if class >= MILITARY_CLASS {
        MILITARY_FUEL
    } else {
        HYDROGEN
    };
    DriveState {
        mass: i32::from(stats.mass as i16),
        class,
        fuel: i32::from(state.hold.get(slot).copied().unwrap_or(0)),
    }
}

/// The original flight model in free flight at `at` (the ship the autopilot
/// flies), with its collision radius so the starport descent touches the pad.
fn new_pilot(cache: &ffe_content::VerifiedCache, ship: u32, at: [i64; 3]) -> OriginalFlight {
    let mut pilot = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            system_flight: true,
            ship,
            ..Flight::default()
        },
        None,
    );
    if let Some(info) = ModelInfo::read(
        cache.asset("base:model/records").expect("model records"),
        cache.asset("base:ship/stats").expect("ship statistics"),
        ship,
    ) {
        pilot.set_collision_radius(info.radii().1);
    }
    pilot.teleport(at);
    pilot
}

/// The speed in metres per second the way the docking tests compute it.
fn speed_metres(pilot: &OriginalFlight) -> f64 {
    pilot
        .player()
        .motion
        .velocity
        .iter()
        .map(|value| (f64::from(*value) * MM_PER_SECOND / 1000.0).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// One frame of `Navigator::command` (`navigation.rs:4416-4622`), applied to the
/// flight model as the playable applies it. Returns false when the autopilot has
/// no target left.
fn autopilot_step(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    pilot: &mut OriginalFlight,
) -> bool {
    let Some((command, _)) = nav.command(cache, pilot) else {
        return false;
    };
    if let Some((landing, seconds)) = command.skip {
        // A leg too long to fly in real time: the autopilot hands back the
        // transfer and the caller carries it out (`navigation.rs:4591-4620`).
        pilot.teleport(landing);
        nav.transfer(cache, seconds);
        return true;
    }
    if let Some(target) = command.face {
        pilot.turn_toward(target, 0.05, 1.0);
    }
    if let Some(rows) = command.align {
        pilot.align_to(rows);
    }
    pilot.set_thrust_override(command.thrust);
    pilot.step(0.05, &command.keys).expect("the flight step");
    nav.advance(cache, 0.05);
    if let Some(moved) = nav.follow(pilot.absolute_position()) {
        pilot.shift(moved);
    }
    true
}

/// Fly to an orbital station and dock: engage it, fly the autopilot's two legs
/// (approach point, then the slot with clearance), take F652's contact and run
/// the timed docking stages until `Traffic::Docked` (`navigation.rs:2710-2724`,
/// `3843-3906`, `3922-3928`).
fn dock_at_station(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    index: usize,
    pilot: &mut OriginalFlight,
) {
    assert_eq!(
        nav.bodies[index].kind,
        Kind::Station,
        "step 4 expects a station, but {} is a {}",
        nav.bodies[index].name,
        nav.bodies[index].kind.label()
    );
    // A station's real docking face if the model viewer can supply it; the port
    // otherwise falls back to the body's radius (`docking.rs:64-77`).
    if let Ok(viewer) = ffe_rs::model_viewer::ModelViewer::new(cache) {
        let model = nav.bodies[index].model;
        if let Some(found) = viewer.find(model) {
            nav.set_port_geometry(model, found.front(), found.radius());
        }
    }
    nav.lock(index);
    nav.engage_locked();
    for step in 0..60_000 {
        if !autopilot_step(nav, cache, pilot) {
            panic!(
                "the autopilot lost {} after {step} steps (range {})",
                nav.bodies[index].name,
                ffe_rs::navigation::describe(ffe_rs::navigation::distance(
                    pilot.absolute_position(),
                    nav.bodies[index].position
                ))
            );
        }
        let at = pilot.absolute_position();
        let speed = speed_metres(pilot);
        if let Some((hit, contact)) = nav.contact(at, speed) {
            match contact {
                Contact::Docked if hit == index => {
                    assert!(
                        nav.begin_docking(index),
                        "F652: the touch at {} was not accepted",
                        nav.bodies[index].name
                    );
                    let mut frames = 0;
                    let mut docked = false;
                    while !docked {
                        frames += 1;
                        assert!(frames < 4_000, "the docking stages never finished");
                        for event in nav.tick_traffic(994, 0.02) {
                            docked |= event == Traffic::Docked(index);
                        }
                    }
                    assert_eq!(
                        nav.docked,
                        Some(index),
                        "F1091: the navigator must be docked after the stages"
                    );
                    return;
                }
                Contact::Collision { clear, .. } => pilot.teleport(clear),
                _ => {}
            }
        }
    }
    panic!(
        "{}: not docked after 60 000 steps (range {})",
        nav.bodies[index].name,
        ffe_rs::navigation::describe(ffe_rs::navigation::distance(
            pilot.absolute_position(),
            nav.bodies[index].position
        ))
    );
}

/// Fly to an open-air starport and land on its pad: the autopilot flies to
/// F741's approach point, asks F655 for clearance and takes F745's hand-off.
/// From there the port's own controller flies the last kilometres; this test
/// places the ship on F650's pad seat and runs one command frame, which is
/// exactly how `starport_autopilot_lands_on_the_pad` exercises F746/F652's touch
/// (`tests/navigation.rs:1318-1350`).
fn land_at_starport(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    index: usize,
    pilot: &mut OriginalFlight,
) {
    assert_eq!(
        nav.bodies[index].kind,
        Kind::Starport,
        "step 7 expects a starport, but {} is a {}",
        nav.bodies[index].name,
        nav.bodies[index].kind.label()
    );
    nav.lock(index);
    nav.engage_locked();
    for _ in 0..60_000 {
        if nav.final_leg && nav.cleared(index) {
            let bay = nav
                .starport(index)
                .bay_of(PLAYER)
                .expect("F650's bay for the player");
            let pad = nav
                .starport_pad_point(index, bay)
                .expect("F650's pad point");
            let up = nav.bodies[index].orientation[1];
            let radius = pilot.collision_radius() as f64;
            let seat: [i64; 3] = std::array::from_fn(|k| pad[k] + (up[k] * radius).round() as i64);
            pilot.teleport(seat);
            pilot.align_to(nav.bodies[index].orientation);
            pilot.set_thrust_override(None);
            let _ = nav.command(cache, pilot);
            assert_eq!(
                nav.landed_body(),
                Some(index),
                "F652's pad touch at {} did not take (range {} mm)",
                nav.bodies[index].name,
                ffe_rs::navigation::distance(pilot.absolute_position(), seat)
            );
            assert_eq!(
                nav.landed_state(),
                Some(traffic_state::LANDED),
                "F1048's 0x30 after the pad touch"
            );
            return;
        }
        if !autopilot_step(nav, cache, pilot) {
            break;
        }
    }
    panic!(
        "{}: the autopilot never reached F745's cleared pad (range {}, final leg {}, cleared {}, notice {:?})",
        nav.bodies[index].name,
        ffe_rs::navigation::describe(ffe_rs::navigation::distance(
            pilot.absolute_position(),
            nav.bodies[index].position
        )),
        nav.final_leg,
        nav.cleared(index),
        nav.notice
    );
}

/// F654's launch from an indoor station and the timed lift stages
/// (`navigation.rs:2728-2757`, `3843-3906`).
fn launch_from_station(nav: &mut Navigator, index: usize) {
    nav.begin_launch().expect("F654's launch permission");
    let mut frames = 0;
    let mut done = false;
    while !done {
        frames += 1;
        assert!(frames < 4_000, "the launch stages never finished");
        for event in nav.tick_traffic(994, 0.02) {
            done |= event == Traffic::LaunchDone(index);
        }
    }
    assert!(nav.sequence.is_none(), "the launch sequence is over");
    assert!(nav.takeoff.is_none(), "the lift put the ship out");
}

/// The 8 steps: buy fuel at Old Blackelk, jump to Titican, dock at Argent, buy
/// water, launch, jump home, land at Old Blackelk and sell the water.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn trade_loop_buys_fuel_warps_docks_and_sells() {
    let cache = cache();

    // Step 1: F998's default new game, docked at Old Blackelk on Hope in Gateway.
    let mut nav = Navigator::new_game(&cache, 0).expect("the default new game");
    assert_eq!(nav.system_name(), "Gateway");
    let new_game = nav.new_game.expect("a new-game record");
    let entries = systems(&cache);
    assert_eq!(
        entries.len(),
        nav.system_count(),
        "the test's system list must match the navigator's"
    );
    assert_eq!(
        entries[nav.current].params.code,
        nav.current_system_code(),
        "the start system's code must match its entry"
    );
    let old_blackelk = nav
        .bodies
        .iter()
        .position(|body| body.kind == Kind::Starport && body.name == "Old Blackelk");
    let old_blackelk = old_blackelk.expect("the starting starport Old Blackelk");
    assert_eq!(nav.docked, Some(old_blackelk));
    assert_eq!(
        nav.bodies[old_blackelk]
            .parent
            .map(|p| nav.bodies[p].name.as_str()),
        Some("Hope"),
        "Old Blackelk stands on Hope"
    );
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: new_game.ship,
        credits: u32::try_from(new_game.cash).expect("the starting cash"),
        hold: vec![0; NAMES],
        loadout: ffe_core::equipment::Loadout::new_game(),
        combat: ffe_rs::combat::Combat::new_game(),
        ..Flight::default()
    };
    assert_eq!(state.credits, 10_000, "1000.0 credits");
    let start_cash = state.credits;
    assert_eq!(nav.hold(&cache, state.ship), 7, "EXP-019's 7 t hold");
    assert_eq!(
        trade::capacity(&nav, &cache, &state),
        7,
        "the new game's free hold"
    );
    let goods = good_names(&nav);
    assert_eq!(
        goods[HYDROGEN], "Hydrogen Fuel",
        "slot {HYDROGEN} is hydrogen fuel"
    );
    let water = goods
        .iter()
        .position(|name| name == "Water")
        .expect("Water in the market tables");
    assert!(
        water < ffe_core::market::GOODS,
        "Water is a market commodity"
    );

    // Step 2: buy 7 t of hydrogen fuel at Old Blackelk.
    let (hydrogen_price, hydrogen_stock) = {
        let market = nav
            .market(&cache, old_blackelk)
            .expect("Old Blackelk's market");
        (
            market.market.entries[HYDROGEN].price,
            market.market.entries[HYDROGEN].stock,
        )
    };
    assert!(
        hydrogen_stock >= 7,
        "Old Blackelk has only {hydrogen_stock} t of hydrogen"
    );
    let text = trade::trade(
        &mut nav,
        &cache,
        &mut state,
        old_blackelk,
        HYDROGEN,
        true,
        7,
    )
    .expect("the fuel purchase");
    assert_eq!(text, "Buying Hydrogen Fuel.");
    assert_eq!(state.hold[HYDROGEN], 7, "7 t of fuel in the hold");
    assert_eq!(
        i64::from(state.credits),
        i64::from(start_cash) - 7 * i64::from(hydrogen_price),
        "the cash paid for 7 t of fuel"
    );
    assert_eq!(
        nav.market(&cache, old_blackelk)
            .expect("the market")
            .market
            .entries[HYDROGEN]
            .stock,
        hydrogen_stock - 7,
        "the station's fuel stock fell by 7 t"
    );
    eprintln!(
        "step 2: 7 t of hydrogen at {hydrogen_price} per tonne; cash {start_cash} -> {} \
         (1000.0 -> {:.1} credits), hold[{HYDROGEN}] = 7, stock {hydrogen_stock} -> {}",
        state.credits,
        f64::from(state.credits) / 10.0,
        hydrogen_stock - 7
    );

    // Step 3: hyperspace to Titican (`navigation.rs:4129-4172`).
    let titican = require_system(&cache, &entries, nav.clock, "Titican");
    nav.drive = Some(drive_state(&cache, &state));
    let (distance, report) = nav
        .jump_report(titican)
        .expect("Titican's jump data from the drive");
    assert!(
        report.result >= 0,
        "Titican is {distance} units away, beyond the drive's {}: {report:?}",
        report.max_range
    );
    let jumped = nav
        .hyperspace(&cache, titican)
        .expect("the jump to Titican");
    assert_eq!(nav.current_system(), titican);
    assert_eq!(nav.system_name(), "Titican");
    assert!(jumped.fuel >= 1, "F901 burns at least a tonne: {jumped:?}");
    // F901: the caller burns the fuel the jump used (`navigation.rs:1114-1127`).
    burn_fuel(&mut state.hold, nav.drive, jumped.fuel);
    assert_eq!(
        state.hold[HYDROGEN],
        7 - u16::try_from(jumped.fuel).expect("a positive burn"),
        "the jump burnt {} t of hydrogen",
        jumped.fuel
    );

    // Step 4: fly to Argent and dock.
    let argent = require_body(&nav, "Argent");
    let mut pilot = new_pilot(&cache, state.ship, jumped.arrival);
    dock_at_station(&mut nav, &cache, argent, &mut pilot);
    assert_eq!(nav.docked, Some(argent));
    assert_eq!(nav.bodies[argent].name, "Argent");
    assert_eq!(
        nav.station().map(|(name, _)| name),
        Some("Argent"),
        "the station services see Argent"
    );

    // Step 5: buy 1 t of water at Argent.
    let used: u16 = state.hold.iter().copied().sum();
    assert!(
        i32::from(used) < trade::capacity(&nav, &cache, &state),
        "the jump must free a tonne of hold for the water (used {used} of {})",
        trade::capacity(&nav, &cache, &state)
    );
    let water_price = nav
        .market(&cache, argent)
        .expect("Argent's market")
        .market
        .entries[water]
        .price;
    let cash_before = state.credits;
    let text = trade::trade(&mut nav, &cache, &mut state, argent, water, true, 1)
        .expect("the water purchase");
    assert_eq!(text, "Buying Water.");
    assert_eq!(state.hold[water], 1, "one tonne of water aboard");
    assert_eq!(
        i64::from(state.credits),
        i64::from(cash_before) - i64::from(water_price),
        "the cash paid for one tonne of water"
    );

    // Step 6: launch and jump back to Gateway.
    launch_from_station(&mut nav, argent);
    nav.drive = Some(drive_state(&cache, &state));
    let gateway = entries
        .iter()
        .position(|entry| entry.name == "Gateway")
        .expect("Gateway among the systems on offer");
    let home = nav.hyperspace(&cache, gateway).expect("the jump home");
    assert_eq!(nav.current_system(), gateway);
    assert_eq!(nav.system_name(), "Gateway");
    burn_fuel(&mut state.hold, nav.drive, home.fuel);
    assert_eq!(
        state.hold[water], 1,
        "the water is still aboard after the jump"
    );

    // Step 7: fly to Hope and land at Old Blackelk.
    let hope = require_body(&nav, "Hope");
    let blackelk = require_body(&nav, "Old Blackelk");
    assert_eq!(
        nav.bodies[blackelk].parent,
        Some(hope),
        "Old Blackelk orbits Hope"
    );
    assert_eq!(nav.bodies[blackelk].kind, Kind::Starport);
    let mut pilot = new_pilot(&cache, state.ship, home.arrival);
    land_at_starport(&mut nav, &cache, blackelk, &mut pilot);
    assert_eq!(nav.landed_body(), Some(blackelk));
    assert_eq!(nav.landed_state(), Some(traffic_state::LANDED));
    // The playable selects a starport the ship has landed at for F4's services
    // board (`main.rs:1777-1794`): state 0x30 has a landed record, not `docked`.
    nav.docked = nav.landed_body();
    assert_eq!(
        nav.station().map(|(name, _)| name),
        Some("Old Blackelk"),
        "the pad services see Old Blackelk"
    );

    // Step 8: sell the tonne of water.
    let sell_price = nav
        .market(&cache, blackelk)
        .expect("Old Blackelk's market")
        .market
        .entries[water]
        .price;
    let cash_before = state.credits;
    let fuel_left = state.hold[HYDROGEN];
    let text = trade::trade(&mut nav, &cache, &mut state, blackelk, water, false, 1)
        .expect("the water sale");
    assert_eq!(text, "Selling Water.");
    assert_eq!(state.hold[water], 0, "the water is sold");
    assert_eq!(state.hold[HYDROGEN], fuel_left, "the fuel is untouched");
    assert_eq!(
        i64::from(state.credits),
        i64::from(cash_before) + i64::from(sell_price),
        "the cash received for one tonne of water"
    );
    eprintln!(
        "trade loop: Gateway -> Titican ({distance} units, {} t of fuel) -> Gateway; \
         water bought at {water_price}, sold at {sell_price}; cash {start_cash} -> {}, \
         fuel {fuel_left} t",
        jumped.fuel, state.credits
    );
}

/// The catalogue the stop message is based on, printed without failing: every
/// system on offer with its stations and starports, the new game's drive and
/// reach, Old Blackelk's water and fuel rows, and Gateway's real galaxy
/// neighbours. Run it after the neighbour systems are added to see the names a
/// trade loop can use.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn reports_the_systems_and_stations_on_offer() {
    let cache = cache();
    let entries = systems(&cache);
    let mut nav = Navigator::new_game(&cache, 0).expect("the default new game");
    assert_eq!(entries.len(), nav.system_count());
    let new_game = nav.new_game.expect("a new-game record");
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: new_game.ship,
        credits: u32::try_from(new_game.cash).expect("cash"),
        hold: vec![0; NAMES],
        loadout: ffe_core::equipment::Loadout::new_game(),
        combat: ffe_rs::combat::Combat::new_game(),
        ..Flight::default()
    };
    nav.drive = Some(drive_state(&cache, &state));
    let drive = nav.drive.unwrap();
    eprintln!(
        "{} systems on offer, start {}, drive mass {} class {} fuel {} t, reach {:?}",
        entries.len(),
        nav.system_name(),
        drive.mass,
        drive.class,
        drive.fuel,
        nav.reach()
    );
    for (index, entry) in entries.iter().enumerate() {
        let report = nav.jump_report(index);
        let fuel = if index == nav.current {
            "here".to_owned()
        } else {
            report.map_or_else(
                || "no jump data".to_owned(),
                |(distance, report)| {
                    if report.result >= 0 {
                        format!("{distance} units, {} t", report.fuel_needed)
                    } else if report.fuel_needed < 0 {
                        format!("{distance} units, OUT OF RANGE (max {})", report.max_range)
                    } else {
                        format!("{distance} units, needs {} t", report.fuel_needed)
                    }
                },
            )
        };
        eprintln!(
            "  {index:2}  {:<18} code {:#010x}  {fuel:<34} {}",
            entry.name,
            entry.params.code,
            places_text(&cache, entry, nav.clock)
        );
    }
    let old_blackelk = nav.docked.expect("the starting starport");
    let goods = good_names(&nav);
    let water = goods
        .iter()
        .position(|name| name == "Water")
        .expect("Water in the market tables");
    let market = nav
        .market(&cache, old_blackelk)
        .expect("Old Blackelk's market");
    eprintln!(
        "Old Blackelk: water slot {water} {:?} {:?}, hydrogen slot {HYDROGEN} {:?}",
        goods[water], market.market.entries[water], market.market.entries[HYDROGEN]
    );
    if let Some(gateway) = entries.iter().find(|entry| entry.name == "Gateway") {
        eprintln!("Gateway's nearest galaxy-table neighbours:");
        for (away, code, name) in galaxy_neighbours(&cache, gateway, 600).into_iter().take(12) {
            eprintln!(
                "  {:>6.2} LY  {code:#010x}  {name}",
                f64::from(away) / 100.0
            );
        }
    }
}
