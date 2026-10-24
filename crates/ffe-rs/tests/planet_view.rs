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
        let radius = body.radius as f64 / UNIT;
        let centre = body.position.map(|v| v as f64 / UNIT);
        // Three radii out along -z of the body, looking at it (forward is +z).
        state.position = [centre[0], centre[1], centre[2] - 3.0 * radius];
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [1.0, 0.0, 0.0],
            colours: (12, 4),
            star: false,
            model: Some((body.model, body.seed)),
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
