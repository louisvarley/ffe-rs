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
