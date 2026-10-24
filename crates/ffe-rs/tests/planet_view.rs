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

/// A planet's surface shades by the sun: the same body lit from opposite sides
/// gives different triangle colours (the terminator follows the light).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_surface_has_a_day_night_terminator() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let unit = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // The setup the existing planet tests use: Earth, probe at 1e6, render at
    // three surface extents (model 125 culls at the distances tried before).
    let _ = viewer.render_scene(129, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
    let extent = viewer.surface_extent.get();
    assert!(extent > 0.0, "Earth must have a surface");
    let frame = |light: [f64; 3]| {
        viewer
            .render_scene(129, unit, [0.0, 0.0, 3.0 * extent], light)
            .unwrap()
    };
    let day = frame([1.0, 0.0, 0.0]);
    let night = frame([-1.0, 0.0, 0.0]);
    eprintln!(
        "Earth: {} shapes lit +x, {} lit -x",
        day.shapes.len(),
        night.shapes.len()
    );
    assert!(!day.shapes.is_empty(), "the day side must draw");
    assert_ne!(
        format!("{:?}", day.shapes),
        format!("{:?}", night.shapes),
        "the terminator must follow the sun"
    );
}

/// The local star lights the station and the ship: two sun directions give
/// different pictures (the fixed default light is the fallback without a sun).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn local_star_light_changes_the_scene() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    let draw = |screen: &mut Screen| {
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            false,
            "",
        );
        let mut hash = 14695981039346656037u64;
        for pixel in screen.logical_pixels() {
            hash = (hash ^ u64::from(*pixel)).wrapping_mul(1099511628211);
        }
        hash
    };
    screen.sun = None;
    let fixed = draw(&mut screen);
    screen.sun = Some([1.0, 0.0, 0.0]);
    let starboard = draw(&mut screen);
    screen.sun = Some([-1.0, 0.0, 0.0]);
    let port = draw(&mut screen);
    eprintln!("fixed {fixed:016x} sun +x {starboard:016x} sun -x {port:016x}");
    assert_ne!(starboard, fixed, "the sun must light the scene");
    assert_ne!(starboard, port, "opposite suns must differ");
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

/// The approach to a planet fills its disc. At three and two radii the surface
/// used to leave large black wedges: the original's 50,000-word order queue
/// overflows there and the drawing stops (the original exits), so the flight
/// view now gives the surface queue room to spare (EXP-009). The clear part of
/// the view (between the top bar and the status block) must have no background
/// pixels inside the disc but a handful of genuinely black surface colours.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_approach_fills_the_disc() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy};
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
    let body = list.iter().find(|b| b.name == "Mars").unwrap();
    let radius = body.radius as f64 / UNIT;
    let centre = body.position.map(|v| v as f64 / UNIT);
    for d in [3.0f64, 2.0, 1.6] {
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        state.position = [centre[0], centre[1], centre[2] - d * radius];
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
        assert!(extent > 0.0, "the surface must draw at {d} radii");
        // The disc's expected screen radius (tan of the angular radius).
        let px = 256.0 / (d * d - 1.0).sqrt();
        let mut inside = 0usize;
        let mut background = 0usize;
        for y in 12..119i32 {
            for x in 0..320i32 {
                let (dx, dy) = (f64::from(x) + 0.5 - 160.0, f64::from(y) + 0.5 - 79.0);
                if dx * dx + dy * dy <= (px * 0.9) * (px * 0.9) {
                    inside += 1;
                    if screen.logical_pixels()[y as usize * 320 + x as usize] == 0 {
                        background += 1;
                    }
                }
            }
        }
        eprintln!("Mars at {d} radii: {background} background pixels of {inside}");
        assert!(
            background < 100,
            "the surface left {background} background pixels of {inside} at {d} radii"
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(
                std::path::Path::new(&dir).join(format!("approach_{d:.1}.ppm")),
                ppm,
            )
            .unwrap();
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

/// The planet is still drawn when the eye is at its surface radius: the old
/// `z > radius` filter dropped the whole body as soon as the centre was nearer
/// than one radius along the view axis, which is where a landing approach ends
/// up. The ground under the eye and the horizon must stay in the frame.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_is_drawn_at_and_inside_one_radius() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy};
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
    let earth = list.iter().find(|b| b.name == "Earth").unwrap();
    let radius = earth.radius as f64 / UNIT;
    let centre = earth.position.map(|v| v as f64 / UNIT);

    for k in [1.5, 1.2, 1.05, 1.0] {
        // Stand on the +y side and look along +z: the horizon is ahead and the
        // ground is below the eye, the view a landing approach ends in.
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        state.position = [centre[0], centre[1] + k * radius, centre[2]];
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [1.0, 0.0, 0.0],
            colours: (12, 4),
            star: false,
            model: Some((earth.model, earth.seed)),
            orientation: earth.orientation,
        }];
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            false,
            "",
        );
        let ground = screen.logical_pixels()[..320 * 158]
            .iter()
            .filter(|p| **p != 0)
            .count();
        eprintln!("k {k}: {ground} non-black view pixels");
        assert!(
            ground > 2_000,
            "the surface must stay drawn at {k} radii ({ground} pixels)"
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(
                std::path::Path::new(&dir).join(format!("surface-k{k:.2}.ppm")),
                ppm,
            )
            .unwrap();
        }
    }
}

/// A starport of a planet is one of the system's `screen.stations` and draws
/// from the surface: the near-side building is visible against its world.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn surface_frame_shows_a_starport() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy, Kind};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, SystemStation};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    let port = list
        .iter()
        .find(|b| b.kind == Kind::Starport && b.name == "London")
        .unwrap();
    let planet = &list[port.parent.unwrap()];
    let up: [f64; 3] = std::array::from_fn(|k| {
        (port.position[k] as f64 - planet.position[k] as f64)
            / (0..3)
                .map(|i| (port.position[i] as f64 - planet.position[i] as f64).powi(2))
                .sum::<f64>()
                .sqrt()
    });
    // 100 km above the port, looking down at it.
    let forward = up.map(|v| -v);
    let right = {
        let c = [
            forward[1] * 0.0 - forward[2] * 1.0,
            forward[2] * 0.0 - forward[0] * 0.0,
            forward[0] * 1.0 - forward[1] * 0.0,
        ];
        let l = c.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
        c.map(|v| v / l)
    };
    let up_cam = [
        forward[1] * right[2] - forward[2] * right[1],
        forward[2] * right[0] - forward[0] * right[2],
        forward[0] * right[1] - forward[1] * right[0],
    ];
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ..Flight::default()
    };
    state.axes = Some([right, up_cam, forward]);
    let disc = Disc {
        centre: planet.position.map(|v| v as f64 / UNIT),
        radius: planet.inner_radius as f64 / UNIT,
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((planet.model, planet.seed)),
        orientation: planet.orientation,
    };
    screen.discs = vec![disc];
    for (height, name) in [
        (100_000_000.0, "surface-starport.ppm"),
        (500_000.0, "surface-starport-near.ppm"),
    ] {
        state.position =
            std::array::from_fn(|k| port.position[k] as f64 / UNIT + up[k] * height / UNIT);
        screen.stations.clear();
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            false,
            "",
        );
        let without = screen.logical_pixels().to_vec();
        screen.stations = vec![SystemStation {
            model: port.model,
            centre: port.position.map(|v| v as f64 / UNIT),
            orientation: port.orientation,
            speed: 0,
            door: 0xffff,
            tics: 0,
        }];
        screen.draw(
            &state,
            ViewState::default(),
            Some(&viewer),
            false,
            false,
            "",
        );
        let with = screen.logical_pixels().to_vec();
        let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
        eprintln!(
            "starport {} model {:#x} at {height} mm: {changed} pixels changed",
            port.name, port.model
        );
        assert!(changed > 100, "the starport must draw ({changed} pixels)");
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in &with {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
        }
    }
}

/// F575's contact tail in the playable: a slow, level touch of the terrain
/// settles the ship (flag 0x40, `Touch::Landing`); a fast one bounces (0x80,
/// damage). `step_landing` is the playable's `Player::tick_with` path.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn soft_touch_lands_and_fast_touch_bounces() {
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy, landing_body, Library};
    use ffe_rs::original_flight::{OriginalFlight, UNITS_PER_FLIGHT_UNIT as UNIT};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    let port = list.iter().find(|b| b.name == "London").unwrap();
    let planet = &list[port.parent.unwrap()];
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let player_info = ffe_core::objects::ModelInfo::read(records, stats, 14).unwrap();
    let planet_info = ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap();
    let parent = landing_body(planet, &planet_info);
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    let up: [f64; 3] = std::array::from_fn(|k| (port.position[k] - planet.position[k]) as f64);
    let length = up.iter().map(|v| v * v).sum::<f64>().sqrt();
    let up = up.map(|v| v / length);
    let make = |height: f64, velocity: f64| {
        let at: [i64; 3] =
            std::array::from_fn(|k| (port.position[k] as f64 + up[k] * height) as i64);
        let mut flight = Flight {
            mode: Mode::Flight,
            position: at.map(|v| v as f64 / UNIT),
            velocity: up.map(|v| -v * velocity),
            ..Flight::default()
        };
        flight.ship = 14;
        flight.axes = Some([[1.0, 0.0, 0.0], up, [up[2], 0.0, -up[0]]]);
        OriginalFlight::from_flight(&flight, None)
    };
    // The starport's anchor is on the terrain; 10 m below it is inside the
    // surface and the ship is level and slow.
    let mut slow = make(-10_000.0, 0.0);
    let landed = slow
        .step_landing(
            0.02,
            &Default::default(),
            &parent,
            planet.position,
            &player_info,
            Some(&tools),
        )
        .unwrap();
    eprintln!("slow touch: flags {:#x} {:?}", landed.flags, landed.touch);
    assert_eq!(landed.flags & 0x40, 0x40, "a slow level touch lands");
    assert_eq!(landed.flags & 0x80, 0, "a landing is not a crash");
    assert!(matches!(
        landed.touch,
        Some(ffe_core::landing::Touch::Landing)
    ));

    // A fast touch bounces: flag 0x80 and the reflected velocity.
    let mut fast = make(-10_000.0, 200.0);
    let crashed = fast
        .step_landing(
            0.02,
            &Default::default(),
            &parent,
            planet.position,
            &player_info,
            Some(&tools),
        )
        .unwrap();
    eprintln!("fast touch: flags {:#x} {:?}", crashed.flags, crashed.touch);
    assert_eq!(crashed.flags & 0x80, 0x80, "a fast touch crashes");
    assert!(crashed.damage > 0, "the crash deals damage");
    assert!(matches!(
        crashed.touch,
        Some(ffe_core::landing::Touch::Crash { .. })
    ));
}

/// The playable's parent choice: a ship at a starport is in that starport's
/// frame, one inside a planet's outer radius is in the planet's, and free space
/// has none.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn landing_parent_chooses_the_surface_body() {
    use ffe_rs::navigation::{distance, Navigator};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new(&cache).unwrap();
    let port = nav.bodies.iter().position(|b| b.name == "London").unwrap();
    let port_body = &nav.bodies[port];
    let planet_index = port_body.parent.unwrap();
    let planet = &nav.bodies[planet_index];
    assert_eq!(nav.landing_parent(port_body.position, 3_000), Some(port));
    // Just inside the atmosphere radius, on the planet's outward axis.
    let away: [i64; 3] = std::array::from_fn(|k| {
        planet.position[k] + (port_body.position[k] - planet.position[k]) * 9 / 10
    });
    assert_eq!(nav.landing_parent(away, 3_000), Some(planet_index));
    assert_eq!(
        nav.landing_parent([0, 0, 0], 3_000),
        None,
        "Sol's centre is not a landing body"
    );
    assert!(distance(away, planet.position) < planet.radius as f64);
}

/// Level flight 100 km above a starport: the horizon and the ground under the
/// eye stay drawn (the same surface path the flight view uses).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn level_flight_near_the_surface_shows_the_horizon() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{bodies, galaxy, Kind};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, SystemStation};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    let port = list
        .iter()
        .find(|b| b.kind == Kind::Starport && b.name == "London")
        .unwrap();
    let planet = &list[port.parent.unwrap()];
    let up: [f64; 3] = std::array::from_fn(|k| (port.position[k] - planet.position[k]) as f64);
    let length = up.iter().map(|v| v * v).sum::<f64>().sqrt();
    let up = up.map(|v| v / length);
    let mut forward = [up[1], -up[0], 0.0];
    let l = forward.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
    forward = forward.map(|v| v / l);
    // Ten degrees nose-down, the attitude a landing approach holds.
    let (sin, cos) = 10f64.to_radians().sin_cos();
    let forward = std::array::from_fn(|k| forward[k] * cos - up[k] * sin);
    let right = [
        up[1] * forward[2] - up[2] * forward[1],
        up[2] * forward[0] - up[0] * forward[2],
        up[0] * forward[1] - up[1] * forward[0],
    ];
    let l = right.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
    let right = right.map(|v| v / l);
    let up_cam = [
        forward[1] * right[2] - forward[2] * right[1],
        forward[2] * right[0] - forward[0] * right[2],
        forward[0] * right[1] - forward[1] * right[0],
    ];
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        position: std::array::from_fn(|k| {
            port.position[k] as f64 / UNIT + up[k] * 100_000_000.0 / UNIT
        }),
        ..Flight::default()
    };
    state.axes = Some([right, up_cam, forward]);
    screen.discs = vec![Disc {
        centre: planet.position.map(|v| v as f64 / UNIT),
        radius: planet.inner_radius as f64 / UNIT,
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((planet.model, planet.seed)),
        orientation: planet.orientation,
    }];
    screen.stations = vec![SystemStation {
        model: port.model,
        centre: port.position.map(|v| v as f64 / UNIT),
        orientation: port.orientation,
        speed: 0,
        door: 0xffff,
        tics: 0,
    }];
    screen.draw(
        &state,
        ViewState::default(),
        Some(&viewer),
        false,
        false,
        "",
    );
    let with = screen.logical_pixels().to_vec();
    screen.discs.clear();
    screen.stations.clear();
    screen.draw(
        &state,
        ViewState::default(),
        Some(&viewer),
        false,
        false,
        "",
    );
    let without = screen.logical_pixels().to_vec();
    let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
    let lower = with[320 * 79..320 * 158]
        .iter()
        .zip(&without[320 * 79..320 * 158])
        .filter(|(a, b)| a != b)
        .count();
    let ground = changed;
    eprintln!("level at 100 km: {ground} world pixels, {lower} in the lower half");
    assert!(
        lower > 2_000,
        "the ground under the eye must draw ({lower})"
    );
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in &with {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(
            std::path::Path::new(&dir).join("surface-horizon-100km.ppm"),
            ppm,
        )
        .unwrap();
    }
}
