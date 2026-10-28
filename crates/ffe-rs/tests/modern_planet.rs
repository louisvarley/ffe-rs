//! Modern graphics' GPU 3-D view of a planet (`ffe_rs::modern3d`): the surface's triangles are
//! set aside with their view-space corners, drawn by the GPU at the window's resolution, and the
//! classic frame keeps everything else. Local-only: needs the content cache and a GPU.

use ffe_rs::camera::ViewState;
use ffe_rs::display::Rect;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::modern3d::Gpu3d;
use ffe_rs::navigation::{bodies, galaxy};
use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
use ffe_rs::screen::{Disc, Screen};

fn write_png(path: &std::path::Path, pixels: &[u32], width: usize, height: usize) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let bytes: Vec<u8> = pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    writer.write_image_data(&bytes).unwrap();
}

/// Earth from three radii down to the ground: the planet's triangles reach the GPU as one
/// batch, textured where the original textures them, and the GPU picture covers every pixel
/// where the original drew the planet.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn the_planet_is_drawn_on_the_gpu_from_orbit_to_the_ground() {
    let mut gpu = Gpu3d::new();
    if !gpu.available() {
        eprintln!("no GPU adapter; skipped");
        return;
    }
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
    for (k, pitch) in [
        (3.0, 0.0),
        (1.5, -0.84),
        (1.2, -0.59),
        (1.05, -0.31),
        (1.0, -0.1),
    ] {
        // Above the +y pole looking along +z, tipped down by `pitch` to put the horizon in view.
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let (s, c) = (f64::sin(pitch), f64::cos(pitch));
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]);
        state.position = [centre[0], centre[1] + k * radius, centre[2]];
        if k == 3.0 {
            // From the side, the whole disc in view.
            state.axes = Some([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]);
        }
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [1.0, 0.0, 0.0],
            colours: (12, 4),
            star: false,
            model: Some((earth.model, earth.seed)),
            orientation: earth.orientation,
        }];
        let start = std::time::Instant::now();
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
        let drawn = start.elapsed();
        let classic = screen.logical_pixels().to_vec();
        screen.capture = true;
        let start = std::time::Instant::now();
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
        eprintln!(
            "k {k}: the game's frame {:.1} ms classic, {:.1} ms setting the planet aside",
            drawn.as_secs_f64() * 1000.0,
            start.elapsed().as_secs_f64() * 1000.0
        );
        screen.capture = false;
        let world = screen.world().cloned().expect("the flight view's world");
        let faces: usize = world.batches.iter().map(Vec::len).sum();
        let textured = world
            .batches
            .iter()
            .flatten()
            .filter(|solid| solid.texture.is_some())
            .count();
        eprintln!(
            "k {k}: {} batches, {faces} faces ({textured} textured), {} textures",
            world.batches.len(),
            world.textures.len()
        );
        // Every corner the GPU gets projects where the original put a point of the surface.
        let (width, height) = (1280usize, 800usize);
        let rect = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let mut target = vec![0u32; width * height];
        ffe_rs::modern::ModernRenderer::new().render(
            screen.logical_pixels(),
            &mut target,
            width,
            rect,
        );
        let start = std::time::Instant::now();
        gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
            .unwrap();
        let first = start.elapsed();
        let start = std::time::Instant::now();
        gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
            .unwrap();
        eprintln!(
            "k {k}: GPU frame {:.1} ms with its new textures, {:.1} ms without",
            first.as_secs_f64() * 1000.0,
            start.elapsed().as_secs_f64() * 1000.0
        );
        // Where the classic frame shows the planet or its atmosphere ring, so does the GPU
        // picture (not the background they were taken out of), allowing for the edges.
        let background = screen.world().unwrap().snapshot.clone();
        let (mut ground, mut covered) = (0, 0);
        for y in 2..156 {
            for x in 2..318 {
                let at = y * 320 + x;
                let around = [at - 321, at - 319, at + 319, at + 321];
                if classic[at] != background[at]
                    && around.iter().all(|&n| classic[n] != background[n])
                {
                    ground += 1;
                    if target[(y * 4 + 2) * width + x * 4 + 2] != background[at] {
                        covered += 1;
                    }
                }
            }
        }
        eprintln!("k {k}: GPU covers {covered} of {ground} classic ground pixels");
        // At one radius the eye is under the ground and the original draws only the
        // atmosphere ring: no surface to set aside then, only the atmosphere.
        assert!(
            k <= 1.0 || faces > 50,
            "the surface reaches the GPU at {k} radii"
        );
        assert!(
            !world.atmospheres.is_empty(),
            "Earth's atmosphere reaches the GPU at {k} radii"
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let dir = std::path::Path::new(&dir);
            write_png(
                &dir.join(format!("modern-k{k:.2}.png")),
                &target,
                width,
                height,
            );
            let mut upscaled = vec![0u32; width * height];
            for (at, p) in upscaled.iter_mut().enumerate() {
                *p = classic[(at / width) / 4 * 320 + (at % width) / 4];
            }
            write_png(
                &dir.join(format!("classic-k{k:.2}.png")),
                &upscaled,
                width,
                height,
            );
        }
        assert!(
            covered * 100 >= ground * 97,
            "the GPU surface covers the classic one at {k} radii ({covered} of {ground})"
        );
    }
}
