use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::galaxy_map::{date_text, GalaxyMap};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::screen::{MapScene, Screen};

/// The map as the original shows it at a new game (Ross 154, sector -1,0); frames are kept when
/// `FFE_SHOT_DIR` is set.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_map_draws() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let bytes = cache.asset("base:world/galaxy-tables").unwrap();
    let tables = ffe_core::sector::Tables::new(bytes).unwrap();
    let home = [
        0x1717_6a00u32 as i32,
        0x1524_3200u32 as i32,
        0xffff_cc00u32 as i32,
    ];
    let mut map = GalaxyMap::new(home, 1000);
    map.update_selection(&tables);
    assert!(map.selected.is_some());
    let shapes = map.render(&viewer, &tables).unwrap();
    let details_bytes = cache.asset("base:world/system-tables").unwrap();
    let details = ffe_core::galaxy::Tables::new(details_bytes).unwrap();
    let text = map.describe(&viewer, Some(&details)).unwrap();
    // What the original's console says of the star at the centre of a new game's map.
    assert_eq!(
        text,
        "Ross 154 Dist. 0.00 light years\rType'M'red star: Outdoor ice and water world. Tourism and fishing."
    );
    assert!(shapes.len() > 100, "{} shapes", shapes.len());
    let mut screen = Screen::new(&cache).unwrap();
    screen.galaxy = Some(MapScene {
        shapes,
        sector: map.sector_text(),
        fuel: None,
        date: date_text(5.0),
        info: "Ross 154 Dist. 0.00 light years\rType M red star: Outdoor ice and water world. Tourism and fishing.".into(),
    });
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("galaxy-map.ppm"), ppm).unwrap();
    }
}

/// The full cached map route: choose a reachable star, store it as the destination,
/// render the map with the fuel line, jump, and render the arrival. Frames and the
/// state series are kept when `FFE_SHOT_DIR` is set; the same state renders twice
/// byte for byte.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_map_destination_and_jump_frames() {
    use ffe_rs::navigation::{DriveState, Navigator};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let tables =
        ffe_core::sector::Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let details =
        ffe_core::galaxy::Tables::new(cache.asset("base:world/system-tables").unwrap()).unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    let target = (1..15)
        .find(|i| nav.range_to(*i) == Some(435))
        .expect("Alpha Centauri");
    let position = nav.system_position(target).unwrap();
    let code = ffe_rs::navigation::galaxy(&cache).unwrap()[target]
        .params
        .code;
    let mut map = GalaxyMap::new(nav.current_position().unwrap(), nav.reach().unwrap_or(0));
    map.centre = [position[0], position[1]];
    map.update_selection(&tables);
    let selected = map.selected.clone().expect("the target star");
    assert_eq!(selected.code, code);
    nav.set_destination(selected.code, selected.position);
    nav.drive = Some(DriveState {
        mass: 16,
        class: 10,
        fuel: 3,
    });
    let report = nav.destination_report().unwrap();
    let fuel = map.fuel_text(Some(&report));
    assert_eq!(fuel.as_deref(), Some("Fuel Required: 1"));
    let shapes = map.render(&viewer, &tables).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    let save = |screen: &Screen, name: &str| {
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut out = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(std::path::Path::new(&dir).join(name), out).unwrap();
        }
    };
    screen.galaxy = Some(MapScene {
        shapes: shapes.clone(),
        sector: map.sector_text(),
        fuel: fuel.clone(),
        date: nav.date_text(),
        info: map.describe(&viewer, Some(&details)).unwrap_or_default(),
    });
    // The scene is deterministic: the same map state renders the same shapes.
    assert_eq!(shapes, map.render(&viewer, &tables).unwrap());
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    save(&screen, "galaxy-map-destination.ppm");
    let before_system = nav.system_name().to_owned();
    let before_clock = nav.clock;
    let jumped = nav.hyperspace_to_destination(&cache).unwrap();
    assert!(nav.system_name().starts_with("Alpha Centauri"));
    assert_eq!(nav.locked, None);
    assert_eq!(nav.destination, None);
    assert!(nav.clock > before_clock);
    // The new system's map frame.
    let home = nav.current_position().unwrap();
    let mut arrival_map = GalaxyMap::new(home, nav.reach().unwrap_or(0));
    arrival_map.update_selection(&tables);
    screen.galaxy = Some(MapScene {
        shapes: arrival_map.render(&viewer, &tables).unwrap(),
        sector: arrival_map.sector_text(),
        fuel: None,
        date: nav.date_text(),
        info: arrival_map
            .describe(&viewer, Some(&details))
            .unwrap_or_default(),
    });
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    save(&screen, "hyperspace-arrival.ppm");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("hyperspace-series.txt"),
            format!(
                "from={before_system}\nto={}\ndistance={}\nfuel={}\ndays={:.6}\nclock_before={before_clock:.3}\nclock_after={:.3}\nfuel_line={}\n",
                nav.system_name(),
                jumped.distance,
                jumped.fuel,
                jumped.days,
                nav.clock,
                fuel.as_deref().unwrap_or("")
            ),
        )
        .unwrap();
    }
}

/// Scrolling picks the star nearest the centre; a long way from any star picks none.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_centre_selects_the_nearest_star() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let tables =
        ffe_core::sector::Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let home = [
        0x1717_6a00u32 as i32,
        0x1524_3200u32 as i32,
        0xffff_cc00u32 as i32,
    ];
    let mut map = GalaxyMap::new(home, 1000);
    map.update_selection(&tables);
    let here = map.selected.clone().unwrap();
    assert_eq!(viewer.system_name(here.code).unwrap(), b"Ross 154");
    // The selection holds until the centre moves.
    map.centre[0] += 0x300;
    map.update_selection(&tables);
    assert_eq!(map.selected.as_ref(), Some(&here));
    // Into the empty part of the sector (a quarter of a sector from every star).
    map.scroll(0x4000, 0x4000);
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..40 {
        map.scroll(0x1800, 0x0900);
        map.update_selection(&tables);
        seen.insert(map.selected.as_ref().map(|s| s.code));
        let _ = step;
    }
    assert!(seen.len() > 2, "{seen:?}");
    assert!(seen.contains(&None), "some centres are far from every star");
    // The corner text follows the centre, in sectors from Sol's.
    map.recentre();
    assert_eq!(map.sector_text(), "Sector: -1,0");
    assert_eq!(map.centre, [home[0], home[1]]);
    // Fuel lines: the same star needs none; a far one is out of range.
    assert_eq!(map.fuel_text(None), None);
}
