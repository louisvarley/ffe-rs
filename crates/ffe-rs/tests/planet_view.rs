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
            Shape::TexturedUv(..) => "textured-uv",
            Shape::Glow(..) => "glow",
            Shape::Label(..) => "label",
            Shape::Spans(..) => "spans",
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    eprintln!("{counts:?} skipped {}", frame.skipped);
    // The planet's F1805/F1807 (descriptor) and F1806/F1808 (explicit u/v)
    // entries now draw the real bitmaps from the cache: the surface is solid
    // texture, not a flat stand-in, and every surface bitmap is the original's
    // land/sea (0x40/0x121) or the explicit-coordinate terrain block (0x5e).
    let textured: Vec<&ffe_rs::model_viewer::Textured> = frame
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Textured(textured) | Shape::TexturedUv(textured, _) => Some(textured),
            _ => None,
        })
        .collect();
    assert!(
        !textured.is_empty(),
        "the planet surface must use the texture path"
    );
    assert!(
        textured
            .iter()
            .all(|t| matches!(t.bitmap & 0xfff, 0x40 | 0x5e | 0x121)),
        "invented surface bitmap: {:?}",
        textured
            .iter()
            .map(|t| t.bitmap & 0xfff)
            .collect::<Vec<_>>()
    );
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
    let mut explicit_total = 0usize;
    // 445 is a surface whose entries are all explicit-coordinate (F1806/F1808).
    for model in [129u32, 120, 125, 445] {
        let _ = viewer.render_scene(model, unit, [0.0, 0.0, 1.0e6], [0.0, 0.0, 1.0]);
        let r = viewer.surface_extent.get();
        for k in [1.2, 1.5, 2.0, 2.5, 3.0, 3.3] {
            let Ok(frame) = viewer.render_scene(model, unit, [0.0, 0.0, k * r], [1.0, 0.0, 0.0])
            else {
                continue;
            };
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            let mut widths: Vec<i32> = Vec::new();
            let explicit = frame
                .shapes
                .iter()
                .filter(|s| matches!(s, Shape::TexturedUv(..)))
                .count();
            eprintln!("model {model} k {k}: {explicit} explicit-coordinate textured shapes");
            explicit_total += explicit;
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
    assert!(
        explicit_total > 0,
        "the explicit-coordinate (F1806/F1808) path must draw"
    );
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
                                Shape::TexturedUv(..) => "textured-uv",
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

/// A landed surface renders the same pixels on a fresh viewer and after a
/// hundred frames' worth of palette registrations. The runtime palette builds
/// each frame from that frame's colours; retaining old request cells made the
/// terrain's representatives drift into speckle.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn landed_surface_pixels_are_stable_across_frames() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::Navigator;
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::Disc;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new_game(&cache, 1).unwrap();
    let moon = nav
        .bodies
        .iter()
        .find(|b| b.name == "Merlin")
        .expect("Ross 154's Merlin")
        .clone();
    let up: [f64; 3] = std::array::from_fn(|k| moon.position[k] as f64);
    let length = up.iter().map(|v| v * v).sum::<f64>().sqrt();
    let up = up.map(|v| v / length);
    let right = {
        let c = [up[1], -up[0], 0.0];
        let l = c.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
        c.map(|v| v / l)
    };
    let forward = [
        up[1] * right[2] - up[2] * right[1],
        up[2] * right[0] - up[0] * right[2],
        up[0] * right[1] - up[1] * right[0],
    ];
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        axes: Some([right, up, forward]),
        position: std::array::from_fn(|k| {
            (moon.position[k] as f64 + up[k] * moon.inner_radius as f64) / UNIT
        }),
        ..Flight::default()
    };
    let disc = Disc {
        centre: moon.position.map(|v| v as f64 / UNIT),
        radius: moon.inner_radius as f64 / UNIT,
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((moon.model, moon.seed)),
        orientation: moon.orientation,
    };
    let render = |viewer: &ModelViewer| {
        let mut screen = Screen::new(&cache).unwrap();
        screen.discs = vec![disc];
        screen.sky = ffe_rs::prefs::space_colour_rgb(4);
        screen.draw(&state, ViewState::default(), Some(viewer), false, true, "");
        screen.draw(&state, ViewState::default(), Some(viewer), false, true, "");
        screen.logical_pixels()[..320 * 158].to_vec()
    };
    let fresh = ModelViewer::new(&cache).unwrap();
    let first = render(&fresh);
    let ground = first.iter().filter(|p| **p != 0x0c_0c_4c).count();
    assert!(ground > 3_000, "the surface view drew {ground} pixels");
    // A long warm-up must not change the frame.
    let warmed = ModelViewer::new(&cache).unwrap();
    for _ in 0..100 {
        let _ = render(&warmed);
    }
    assert_eq!(
        first,
        render(&warmed),
        "the landed surface drifts over frames"
    );
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
        radius: ffe_rs::navigation::surface_radius(planet, false),
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
        // Solid faces, not just outlines: some row must hold a run of changed
        // pixels several wide (F1604/F1607/F1560 fill the building's faces).
        let mut longest_run = 0usize;
        for y in 0..158 {
            let mut run = 0usize;
            for x in 0..320 {
                let i = y * 320 + x;
                if with[i] != without[i] {
                    run += 1;
                    longest_run = longest_run.max(run);
                } else {
                    run = 0;
                }
            }
        }
        eprintln!(
            "starport {} model {:#x} at {height} mm: {changed} pixels changed, longest run {longest_run}",
            port.name, port.model
        );
        assert!(changed > 100, "the starport must draw ({changed} pixels)");
        assert!(
            longest_run >= 3,
            "the starport must draw solid faces (longest run {longest_run})"
        );
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

/// The flight view draws the inner (base-sphere) radius, and a settled ship
/// draws the outer radius word (`+0x138`, `Body::radius`) - the same radius
/// `landing_body` hands F575. A rough landing therefore settles on the ground
/// the landed view draws. The approach keeps the inner base so the eye stays
/// outside the shell.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn surface_and_landing_use_one_radius() {
    use ffe_rs::navigation::{Kind, Navigator};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new_game(&cache, 1).unwrap();
    let planet = nav.bodies.iter().find(|b| b.kind == Kind::Planet).unwrap();
    let info = ffe_core::objects::ModelInfo::read(
        cache.asset("base:model/records").unwrap(),
        cache.asset("base:ship/stats").unwrap(),
        planet.model,
    )
    .unwrap();
    let contact = ffe_rs::navigation::landing_body(planet, &info);
    assert_eq!(contact.outer_radius, planet.radius);
    assert_eq!(contact.inner_radius, planet.inner_radius);
    assert!(
        planet.radius > planet.inner_radius,
        "a planet's outer radius (drawn surface) is larger than its collision radius"
    );
    assert_eq!(
        ffe_rs::navigation::surface_radius(planet, false) * UNIT,
        planet.inner_radius as f64,
        "in flight the base sphere is the inner radius"
    );
    assert_eq!(
        ffe_rs::navigation::surface_radius(planet, true) * UNIT,
        planet.radius as f64,
        "a settled ship draws the same outer radius the contact uses"
    );
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
        radius: ffe_rs::navigation::surface_radius(planet, false),
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

/// The whole surface-landing sequence, as the playable runs it: a slow, level
/// touch on a planet's terrain takes F1021's `0x32`; F1047's view frame makes it
/// `0x34`; the surface view is the flight view from the settled ship (terrain,
/// F1877's sky and the starport where one is in range); `L` starts F1013's climb
/// (`0x24`), F1013 hands over to `0x26`, F1031 clears it to `0x00`, and the ship
/// flies again. Frames are saved with `FFE_SHOT_DIR`.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn landing_reaches_the_surface_state_and_launches_again() {
    use ffe_core::traffic::{state, TakeoffStep};
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{landing_body, Library, Navigator};
    use ffe_rs::original_flight::{
        Keys as OriginalKeys, OriginalFlight, UNITS_PER_FLIGHT_UNIT as UNIT,
    };
    use ffe_rs::screen::{Disc, SystemStation};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    let port_index = nav.bodies.iter().position(|b| b.name == "London").unwrap();
    let planet_index = nav.bodies[port_index].parent.unwrap();
    let port = nav.bodies[port_index].clone();
    let planet = nav.bodies[planet_index].clone();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let ship = 14;
    let player_info = ffe_core::objects::ModelInfo::read(records, stats, ship).unwrap();
    let planet_info = ffe_core::objects::ModelInfo::read(records, stats, planet.model).unwrap();
    let parent = landing_body(&planet, &planet_info);
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    let to_port: [f64; 3] = std::array::from_fn(|k| (port.position[k] - planet.position[k]) as f64);
    let radius = to_port.iter().map(|v| v * v).sum::<f64>().sqrt();
    let to_port = to_port.map(|v| v / radius);
    // The starport's own high ground (F879 puts it on the terrain), ten metres
    // inside the collision surface: a level, still ship the terrain contact
    // settles. The playable's F1033 would give this point the starport's effect
    // region (0x20) and F1021's pad branch; the test drives the rough branch the
    // planet is for by naming the planet as the scan's parent.
    let up = to_port;
    let at: [i64; 3] =
        std::array::from_fn(|k| (planet.position[k] as f64 + up[k] * (radius - 10_000.0)) as i64);
    let mut flight = Flight {
        mode: Mode::Flight,
        position: at.map(|v| v as f64 / UNIT),
        system_flight: true,
        ship,
        ..Flight::default()
    };
    let right = {
        let c = [up[1], -up[0], 0.0];
        let l = c.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
        c.map(|v| v / l)
    };
    let forward = [
        up[1] * right[2] - up[2] * right[1],
        up[2] * right[0] - up[0] * right[2],
        up[0] * right[1] - up[1] * right[0],
    ];
    flight.axes = Some([right, up, forward]);
    let mut model = OriginalFlight::from_flight(&flight, None);
    let mut view_state = flight.clone();

    let touch = model
        .step_landing(
            0.02,
            &OriginalKeys::default(),
            &parent,
            planet.position,
            &player_info,
            Some(&tools),
        )
        .unwrap();
    assert_eq!(touch.flags & 0x40, 0x40, "the slow level touch lands");
    assert_eq!(touch.flags & 0x80, 0, "and does not crash");
    assert!(nav.land(planet_index), "the planet is a landing body");
    assert_eq!(nav.landed_state(), Some(state::LANDING));
    assert_eq!(nav.landed_state(), Some(0x32));
    assert_eq!(nav.landing_frame(), Some(state::LANDED_ROUGH));
    assert_eq!(nav.landed_state(), Some(0x34));
    model.settle_landing();
    model.set_mode(nav.landed_state().unwrap());
    assert_eq!(model.player().mode, 0x34);
    model.apply(&mut view_state);
    assert!(view_state.speed() < 0.01, "F1042 zeroed the speed");

    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.discs = vec![Disc {
        centre: planet.position.map(|v| v as f64 / UNIT),
        radius: ffe_rs::navigation::surface_radius(&planet, true),
        light: [1.0, 0.0, 0.0],
        colours: (12, 4),
        star: false,
        model: Some((planet.model, planet.seed)),
        orientation: planet.orientation,
    }];
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| {
            matches!(
                b.kind,
                ffe_rs::navigation::Kind::Station | ffe_rs::navigation::Kind::Starport
            )
        })
        .map(|b| SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            door: 0xffff,
            tics: 0,
        })
        .collect();
    let shots = std::env::var_os("FFE_SHOT_DIR");
    let save = |name: &str, state: &Flight, screen: &mut Screen| {
        if let Some(dir) = &shots {
            screen.draw(state, ViewState::default(), Some(&viewer), false, true, "");
            let mut ppm = b"P6\n320 200\n255\n".to_vec();
            for p in screen.logical_pixels() {
                ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(std::path::Path::new(dir).join(name), ppm).unwrap();
        }
    };
    // F1052 clears with the previous frame's scene colour: the second draw is
    // the settled surface view, with F1877's atmosphere sky over the space one.
    // Draw explicitly so the check does not depend on FFE_SHOT_DIR being set.
    screen.draw(
        &view_state,
        ViewState::default(),
        Some(&viewer),
        false,
        true,
        "",
    );
    screen.draw(
        &view_state,
        ViewState::default(),
        Some(&viewer),
        false,
        true,
        "",
    );
    save("surface-0000-landed.ppm", &view_state, &mut screen);
    let view = &screen.logical_pixels()[..320 * 158];
    // F1052's background at this altitude is the space colour (`DATA_008814`
    // stays 0: the eye is above the haze's surface), and the terrain/starport
    // fill the rest of the upper view.
    assert_eq!(
        view[0], screen.sky,
        "the surface background is the space colour"
    );
    let ground = view.iter().filter(|p| **p != screen.sky).count();
    assert!(ground > 4_000, "the surface view drew {ground} pixels");

    // L: F1056's launch action. F1013's climb begins at 0x24.
    nav.begin_surface_launch().unwrap();
    model.begin_takeoff();
    assert_eq!(nav.takeoff_state(), Some(state::TAKEOFF));
    let mut seen = Vec::new();
    let mut cleared = false;
    let seat = model.absolute_position();
    for _ in 0..6_000 {
        if nav.takeoff.is_none() {
            break;
        }
        // The rough landing's scan is the planet's terrain, not the starport's
        // hull: F1021's non-pad branch.
        let body = nav.bodies[planet_index].clone();
        let info = ffe_core::objects::ModelInfo::read(records, stats, body.model).unwrap();
        let parent = landing_body(&body, &info);
        let climbing = nav.takeoff_state() == Some(state::TAKEOFF);
        let keys = OriginalKeys {
            faster: !climbing && model.set_speed_metres() < 500.0,
            ..OriginalKeys::default()
        };
        let contact = model
            .step_landing(
                0.02,
                &keys,
                &parent,
                body.position,
                &player_info,
                Some(&tools),
            )
            .unwrap();
        let before = nav.takeoff_state().unwrap();
        // A terrain contact reports F1033's altitude directly; a complex shape
        // that leaves it at i32::MAX is measured from the landing seat.
        let altitude = if contact.altitude() == i32::MAX {
            let at = model.absolute_position();
            (0..3)
                .map(|k| (at[k] - seat[k]) as f64 * up[k])
                .sum::<f64>()
                .clamp(0.0, f64::from(i32::MAX)) as i32
        } else {
            contact.altitude()
        };
        let step = nav.takeoff_step(altitude, model.set_speed_units(), contact.flags);
        if nav.takeoff_state() != Some(before) {
            seen.push(before);
        }
        match step {
            Some(TakeoffStep::HandOff) => {
                model.set_mode(nav.takeoff_state().unwrap());
                model.apply(&mut view_state);
                save("surface-0100-climb.ppm", &view_state, &mut screen);
            }
            Some(TakeoffStep::Cleared) => {
                model.set_mode(state::FREE);
                model.apply(&mut view_state);
                save("surface-0200-flight.ppm", &view_state, &mut screen);
                cleared = true;
                break;
            }
            Some(TakeoffStep::Retakeoff) => model.begin_takeoff(),
            None => {}
        }
    }
    eprintln!(
        "surface launch modes {seen:?}, cleared {cleared}, moved {}, mode {:#x}",
        ffe_rs::navigation::describe(
            (0..3)
                .map(|k| (model.absolute_position()[k] - seat[k]) as f64)
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
        ),
        model.player().mode
    );
    assert_eq!(seen.first(), Some(&state::TAKEOFF), "F1013's climb first");
    assert!(
        seen.contains(&state::AFTER_TAKEOFF),
        "F1013 hands over to 0x26"
    );
    assert!(cleared, "F1031 clears the planet into free flight");
    assert_eq!(model.player().mode, 0x00);
    assert!(nav.takeoff.is_none() && nav.landed.is_none());
}

/// The new game's landed-at-starport view (state `0x30`, F1050/F1054): the
/// flight view from F994's pad seat, with the planet and the starport drawn and
/// the flight console below. `FFE_SHOT_DIR/landed-starport.ppm` is the frame
/// `tools/reference/verify-surface.py` compares with the executed original's
/// (`jjffe-smoke --game-frame 500 0`).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_landed_surface_view() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::navigation::{Kind, Navigator};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, SystemStation};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new_game(&cache, 1).unwrap();
    let start = nav.new_game.as_ref().unwrap();
    let index = nav.docked.unwrap();
    let body = nav.bodies[index].clone();
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start.seat.map(|v| v as f64 / UNIT),
        ..Flight::default()
    };
    state.axes = Some(body.orientation);
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.discs = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Star | Kind::Planet | Kind::Moon))
        .map(|b| {
            let length = b.position.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
            Disc {
                centre: b.position.map(|v| v as f64 / UNIT),
                radius: ffe_rs::navigation::surface_radius(b, true),
                light: b.position.map(|v| -(v as f64) / length.sqrt().max(1.0)),
                colours: (12, 4),
                star: b.kind == Kind::Star,
                model: Some((b.model, b.seed)),
                orientation: b.orientation,
            }
        })
        .collect();
    let tics = ffe_core::model_animation::START_TICS;
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Station | Kind::Starport))
        .map(|b| SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            door: 0xffff,
            tics,
        })
        .collect();
    screen.sky = ffe_rs::prefs::space_colour_rgb(4);
    // Two frames: F1052 applies the previous scene's background colour.
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        // Draw again so the saved frame is a settled one.
        screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("landed-starport.ppm"), ppm).unwrap();
    }
    let view = &screen.logical_pixels()[..320 * 158];
    eprintln!(
        "landed at {}: {} non-sky pixels, sky {:#08x}, corner {:#08x}",
        body.name,
        view.iter().filter(|p| **p != screen.sky).count(),
        screen.sky,
        view[0]
    );
    assert_eq!(
        view[0], screen.sky,
        "the landed background is the space colour until the haze"
    );
    assert!(
        view.iter().filter(|p| **p != screen.sky).count() > 2_000,
        "the starport and its terrain are drawn"
    );
    // The ground below the horizon: the lower half of the view is the drawn
    // surface, not the space colour. (The old inner-radius scale left the eye
    // thousands of kilometres above the drawn ground and drew only sky here.)
    let lower = view[320 * 100..320 * 158]
        .iter()
        .filter(|p| **p != screen.sky)
        .count();
    assert!(
        lower > 8_000,
        "the landed view must show ground below the horizon ({lower} pixels)"
    );
}

/// The DOS default new game (F829 record 0): Gateway's starport Old Blackelk on
/// Hope, the Saker Mk III, state 0x30. The boot frame is the landed surface view
/// (F1050/F1054), with Set/Act at (2,148), Alt at (266,139) and the parent name
/// `Hope` beneath it, as the executed original's boot frame shows.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn new_game_hope_landed_surface_view() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::flight::{Flight, Mode};
    use ffe_rs::hud::PlayerHud;
    use ffe_rs::indexed::BitmapFont;
    use ffe_rs::navigation::{Kind, Navigator};
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, SystemStation};
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new_game(&cache, 0).unwrap();
    let start = nav.new_game.as_ref().unwrap();
    assert_eq!(nav.system_name(), "Gateway");
    assert_eq!(start.start_type, 0);
    assert_eq!(start.player_state, 0x30);
    assert_eq!(start.player_index, 87);
    assert_eq!(start.cash, 10_000);
    assert_eq!(start.ship, 0x1f);
    let index = nav.docked.expect("landed at the start starport");
    let body = nav.bodies[index].clone();
    assert_eq!(body.name, "Old Blackelk");
    assert_eq!(body.slot, 0x6a);
    assert!(body.open_air);
    let parent = body
        .parent
        .and_then(|p| nav.bodies.get(p))
        .expect("the pad's planet");
    assert_eq!(parent.name, "Hope");

    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start.seat.map(|v| v as f64 / UNIT),
        ..Flight::default()
    };
    state.axes = Some(body.orientation);
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.discs = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Star | Kind::Planet | Kind::Moon))
        .map(|b| {
            let length = b.position.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
            Disc {
                centre: b.position.map(|v| v as f64 / UNIT),
                radius: if b.kind == Kind::Star {
                    b.radius as f64 / UNIT
                } else {
                    b.inner_radius as f64 / UNIT
                },
                light: b.position.map(|v| -(v as f64) / length.sqrt().max(1.0)),
                colours: (12, 4),
                star: b.kind == Kind::Star,
                model: Some((b.model, b.seed)),
                orientation: b.orientation,
            }
        })
        .collect();
    let tics = ffe_core::model_animation::START_TICS;
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, Kind::Station | Kind::Starport))
        .map(|b| SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            door: 0xffff,
            tics,
        })
        .collect();
    screen.sky = ffe_rs::prefs::space_colour_rgb(4);
    screen.hud = PlayerHud {
        set_speed: Some(0.0),
        actual_speed: 0.0,
        altitude: Some(0),
        registration: parent.name.clone(),
        ..PlayerHud::default()
    };
    let font = BitmapFont::new(cache.asset("base:font/main").unwrap()).unwrap();
    assert_eq!(
        screen.hud.speed_lines(&font),
        ["Set:0.0 km/h", "Act:0.0 km/h", "Alt:0 m", "Hope"]
    );
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    screen.draw(&state, ViewState::default(), Some(&viewer), false, true, "");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join("hope-landed.ppm"), ppm).unwrap();
    }
    let pixels = screen.logical_pixels();
    let view = &pixels[..320 * 158];
    assert!(
        view.iter().filter(|p| **p != screen.sky).count() > 2_000,
        "the Hope terrain and Old Blackelk are drawn"
    );
    // F1099's speed block: Set/Act, then Alt and the parent name below them
    // in the font's HUD colour (15).
    let hud_colour = screen.palette()[usize::from(ffe_rs::hud::HUD_COLOUR)];
    let ink = |x0: usize, x1: usize, y0: usize, y1: usize| {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| pixels[y * 320 + x]))
            .filter(|p| *p == hud_colour)
            .count()
    };
    assert!(
        ink(2, 60, 148, 180) > 20,
        "Set/Act/Alt/Hope are drawn at (2,148)"
    );
    // The landing is not the services board (EXP-014): the docked screen is only
    // opened by the original's docking path (`Traffic::Docked`), and the board
    // itself still renders from the port's `DockState`.
    assert!(
        screen.dock.is_none(),
        "the landed view does not draw services"
    );
    let mut dock = ffe_rs::station::DockState::default();
    dock.open(ffe_rs::station::Screen::Services);
    assert_eq!(dock.screen, ffe_rs::station::Screen::Services);
    assert_eq!(ffe_rs::station::SERVICES.len(), 4);
}
