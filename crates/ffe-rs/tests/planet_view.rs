use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::screen::Screen;

/// The planet models through the viewer: curved surfaces now give triangles.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_models_draw_triangles() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    for wanted in [113u32, 118, 130, 445] {
        let mut guard = 0;
        while viewer.model().index != wanted && guard < 1000 {
            viewer.step(1);
            guard += 1;
        }
        if viewer.model().index != wanted {
            continue;
        }
        let Ok(frame) = viewer.render() else {
            eprintln!("model {wanted}: too large for the viewer");
            continue;
        };
        eprintln!(
            "model {wanted}: {} shapes, {} skipped",
            frame.shapes.len(),
            frame.skipped
        );
        screen.draw_model_viewer(&viewer);
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(
                std::path::Path::new(&dir).join(format!("model{wanted}.ppm")),
                ppm,
            )
            .unwrap();
        }
    }
}

/// The boot scenario: how far the station the player starts at is from the
/// nearest planet, and whether that planet draws in the flight view.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn boot_station_has_a_planet_in_view() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{distance, galaxy, Kind, Navigator};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Disc;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let nav = Navigator::new(&cache).unwrap();
    let orbit = &nav.bodies[nav.docked.expect("boot station")];
    let station = orbit.name.as_str();
    let entry = galaxy(&cache)
        .unwrap()
        .into_iter()
        .nth(nav.current)
        .unwrap();
    // The navigator's own bodies (its clock), not a fresh list at another clock.
    let list = &nav.bodies;
    let mut planets: Vec<_> = list.iter().filter(|b| b.kind == Kind::Planet).collect();
    planets.sort_by(|a, b| {
        distance(orbit.position, a.position).total_cmp(&distance(orbit.position, b.position))
    });
    let body = planets[0];
    let radii = distance(orbit.position, body.position) / body.radius as f64;
    eprintln!(
        "{station} in {}; nearest planet {} (model {}) is {radii:.2} radii from the station",
        entry.name, body.name, body.model
    );
    // Which stations of Sol actually orbit a planet? That is where a launch
    // would put one in view.
    for b in list.iter().filter(|b| b.kind == Kind::Station) {
        if let Some((near, away)) = planets
            .iter()
            .map(|p| (p, distance(b.position, p.position) / p.radius as f64))
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            eprintln!(
                "  station {:<22} nearest {:<10} {:>9.2} radii",
                b.name, near.name, away
            );
        }
    }
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let radius = body.radius as f64 / UNIT;
    let centre = body.position.map(|v| v as f64 / UNIT);
    state.position = [centre[0], centre[1], centre[2] - 8.0 * radius];
    // Just the world the station orbits: the draw path for the others is the
    // same and is covered by `flight_view_shows_planet_surfaces`.
    screen.discs = vec![Disc {
        centre,
        radius,
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((body.model, body.seed)),
        orientation: body.orientation,
    }];
    screen.draw(
        &state,
        ViewState::default(),
        Some(&viewer),
        false,
        false,
        "",
    );
    let extent = viewer.surface_extent.get();
    eprintln!(
        "surface extent {extent}, {} discs, {} distinct colours on screen",
        screen.discs.len(),
        screen
            .logical_pixels()
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    assert!(extent > 0.0, "the planet's surface must draw");
    assert!(
        radii < 10.0,
        "nearest planet is {radii:.1} radii from the station"
    );
}

/// Earth and the Moon from near orbit, in the flight view's own camera.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn flight_view_shows_planet_surfaces() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy, Kind};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Disc;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    for name in ["Earth", "Mars", "Moon"] {
        let body = list.iter().find(|b| b.name == name).unwrap();
        eprintln!(
            "{name}: model {} seed {:#x} radius {}",
            body.model, body.seed, body.radius
        );
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        eprintln!("   orientation {:?}", body.orientation);
        let radius = body.radius as f64 / UNIT;
        let centre = body.position.map(|v| v as f64 / UNIT);
        // Three radii out along -z of the body, looking at it (forward is +z).
        state.position = [centre[0], centre[1], centre[2] - 8.0 * radius];
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [1.0, 0.0, 0.0],
            colours: (12, 4),
            star: false,
            model: Some((body.model, body.seed)),
            orientation: body.orientation,
        }];
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            false,
            "",
        );
        eprintln!(
            "   extent {} model radius {:?} disc radius {radius}",
            viewer.surface_extent.get(),
            viewer.find(body.model).and_then(|m| m.radius())
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(
                std::path::Path::new(&dir).join(format!("flight_{name}.ppm")),
                ppm,
            )
            .unwrap();
        }
    }
    let _ = Kind::Planet;
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_model_radii() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    for model in [113u32, 120, 125, 129, 445] {
        eprintln!(
            "model {model}: radius {:?}",
            viewer.find(model).and_then(|m| m.radius())
        );
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_screen_size() {
    use ffe_rs::model_viewer::Shape;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let unit = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for model in [129u32, 120] {
        let _ = viewer.render_scene(model, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
        let r = viewer.surface_extent.get();
        for k in [3.0, 3.3] {
            let frame = viewer
                .render_scene(model, unit, [0.0, 0.0, k * r], [1.0, 0.0, 0.0])
                .unwrap();
            let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
            for shape in &frame.shapes {
                if let Shape::Fill(points, _) = shape {
                    for (x, y) in points {
                        lo = (lo.0.min(*x), lo.1.min(*y));
                        hi = (hi.0.max(*x), hi.1.max(*y));
                    }
                }
            }
            eprintln!(
                "model {model} at {k} radii: {} shapes, bbox {lo:?} {hi:?}",
                frame.shapes.len()
            );
        }
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn earth_shape_kinds() {
    use ffe_rs::model_viewer::Shape;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let unit = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let _ = viewer.render_scene(129, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
    let r = viewer.surface_extent.get();
    let frame = viewer
        .render_scene(129, unit, [0.0, 0.0, 3.0 * r], [1.0, 0.0, 0.0])
        .unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for shape in &frame.shapes {
        let name = match shape {
            Shape::Fill(..) => "fill",
            Shape::Gouraud(..) => "gouraud",
            Shape::Flat(..) => "flat",
            Shape::Polygon(..) => "polygon",
            Shape::Line(..) => "line",
            Shape::Curve(..) => "curve",
            Shape::Point(..) => "point",
            Shape::Textured(..) => "textured",
            Shape::Glow(..) => "glow",
            Shape::Label(..) => "label",
            Shape::Spans(..) => "spans",
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    eprintln!("{counts:?} skipped {}", frame.skipped);
    let mut sizes: Vec<i32> = Vec::new();
    for shape in &frame.shapes {
        if let Shape::Fill(points, _) = shape {
            let xs = points.iter().map(|p| p.0);
            let ys = points.iter().map(|p| p.1);
            let w = xs.clone().max().unwrap() - xs.min().unwrap();
            let h = ys.clone().max().unwrap() - ys.min().unwrap();
            sizes.push(w.max(h));
        }
    }
    sizes.sort();
    eprintln!(
        "triangle sizes: median {} p90 {} p99 {} max {}",
        sizes[sizes.len() / 2],
        sizes[sizes.len() * 9 / 10],
        sizes[sizes.len() * 99 / 100],
        sizes[sizes.len() - 1]
    );
    let inside = frame
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Fill(p, _) => Some(p),
            _ => None,
        })
        .filter(|p| {
            p.iter()
                .all(|(x, y)| (-60..380).contains(x) && (-60..220).contains(y))
        })
        .count();
    eprintln!("{inside} triangles within the view");
    for model in [129u32, 120, 125] {
        let _ = viewer.render_scene(model, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
        let r = viewer.surface_extent.get();
        for k in [1.2, 1.5, 2.0, 2.5, 3.0, 3.3] {
            let Ok(frame) = viewer.render_scene(model, unit, [0.0, 0.0, k * r], [1.0, 0.0, 0.0])
            else {
                continue;
            };
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            let mut widths: Vec<i32> = Vec::new();
            for shape in &frame.shapes {
                if let Shape::Fill(points, _) = shape {
                    let xs: Vec<i32> = points.iter().map(|p| p.0).collect();
                    let (a, b) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
                    if b - a < 120 {
                        lo = lo.min(a);
                        hi = hi.max(b);
                    }
                    widths.push(b - a);
                }
            }
            eprintln!(
                "model {model} k {k}: small-triangle x span {lo}..{hi}, {} shapes",
                widths.len()
            );
        }
    }
}

/// Which models the stars of each system use, and what drawing them yields.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn star_models() {
    use ffe_rs::model_viewer::Shape;
    use ffe_rs::navigation::{bodies, galaxy, Kind};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let unit = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut seen = std::collections::BTreeSet::new();
    for entry in galaxy(&cache).unwrap() {
        for body in bodies(&cache, &entry, [0x1000, 0]).unwrap() {
            if body.kind == Kind::Star && seen.insert(body.model) {
                eprintln!(
                    "{}: star {} model {} radius {} seed {:#x}",
                    entry.name, body.name, body.model, body.radius, body.seed
                );
                let _ = viewer.render_scene(body.model, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
                let r = viewer.surface_extent.get();
                match viewer.render_scene(
                    body.model,
                    unit,
                    [0.0, 0.0, 4.0 * r.max(1.0)],
                    [1.0, 0.0, 0.0],
                ) {
                    Ok(frame) => {
                        let mut kinds = std::collections::BTreeMap::new();
                        for shape in &frame.shapes {
                            let name = match shape {
                                Shape::Fill(..) => "fill",
                                Shape::Gouraud(..) => "gouraud",
                                Shape::Flat(..) => "flat",
                                Shape::Polygon(..) => "polygon",
                                Shape::Line(..) => "line",
                                Shape::Curve(..) => "curve",
                                Shape::Point(..) => "point",
                                Shape::Textured(..) => "textured",
                                Shape::Glow(..) => "glow",
                                Shape::Label(..) => "label",
                                Shape::Spans(..) => "spans",
                            };
                            *kinds.entry(name).or_insert(0) += 1;
                        }
                        eprintln!("   extent {r}: {kinds:?} skipped {}", frame.skipped);
                    }
                    Err(e) => eprintln!("   render failed: {e}"),
                }
            }
        }
    }
}

/// Stars through the same surface path as the planets.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn flight_view_shows_stars() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy, Kind};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Disc;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    for entry in galaxy(&cache).unwrap() {
        if !["Sol", "Sirius, Sirius B", "Zaonce"].contains(&entry.name.as_str()) {
            continue;
        }
        for body in bodies(&cache, &entry, [0x1000, 0]).unwrap() {
            if body.kind != Kind::Star {
                continue;
            }
            let mut state = Flight {
                mode: Mode::Flight,
                ..Flight::default()
            };
            state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            let radius = body.radius as f64 / UNIT;
            let centre = body.position.map(|v| v as f64 / UNIT);
            state.position = [centre[0], centre[1], centre[2] - 8.0 * radius];
            screen.discs = vec![Disc {
                centre,
                radius,
                light: [1.0, 0.0, 0.0],
                colours: (17, 17),
                star: true,
                model: Some((body.model, body.seed)),
                orientation: body.orientation,
            }];
            screen.draw(
                &state,
                ViewState::default(),
                Some(&viewer),
                false,
                false,
                "",
            );
            if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
                let mut ppm = b"P6\n320 200\n255\n".to_vec();
                for p in screen.logical_pixels() {
                    ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
                }
                let name = body.name.replace(' ', "_");
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("star_{name}.ppm")),
                    ppm,
                )
                .unwrap();
            }
        }
    }
}

/// The navigation list's layout.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn nav_list_layout() {
    use ffe_rs::navigation::{Navigator, View};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    nav.open(View::System);
    nav.selected = 3;
    nav.target = nav.listed().get(1).copied();
    let rows: Vec<(String, bool, bool)> = nav
        .listed()
        .into_iter()
        .enumerate()
        .map(|(row, i)| {
            (
                nav.bodies[i].name.clone(),
                row == nav.selected,
                nav.target == Some(i),
            )
        })
        .collect();
    screen.draw_nav_list(&rows, "EARTH PLANET 1.00 AU PAGE 1/3  LEFT/RIGHT: PAGES");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("nav_list.ppm"), ppm).unwrap();
    }
}
