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
    // Without the background stars and nebula, which Modern generates instead of drawing, so
    // the frames differ only by the planet.
    screen.stars_visible = false;
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
            "k {k}: {} batches, {faces} faces ({textured} textured), {} bitmaps",
            world.batches.len(),
            world.bitmaps.len()
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

/// A star in view glows in Modern graphics: a halo sprite in its colour goes to the GPU before
/// the star, and the GPU picture is brighter round the star than the sky it was drawn over.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn a_star_glows_on_the_gpu() {
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
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    state.position = [0.0; 3];
    // A star 5 original pixels in radius straight ahead.
    screen.discs = vec![Disc {
        centre: [0.0, 0.0, 1000.0],
        radius: 20.0,
        light: [0.0, 0.0, 1.0],
        colours: (15, 15),
        star: true,
        model: None,
        orientation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    }];
    screen.capture = true;
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    let world = screen.world().cloned().expect("the flight view's world");
    let halo = world
        .batches
        .iter()
        .flatten()
        .find_map(|solid| solid.sprite.as_ref()?.halo);
    assert!(halo.is_some_and(|(size, _)| size >= 16), "a halo sprite");
    let (width, height) = (1280usize, 800usize);
    let rect = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut target = vec![0u32; width * height];
    gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
        .unwrap();
    // Twelve original pixels right of the star's centre (160, 79): sky in the frame, glow on
    // the GPU.
    let sum = |p: u32| (p >> 16 & 0xff) + (p >> 8 & 0xff) + (p & 0xff);
    let sky = world.snapshot[79 * 320 + 172];
    let shown = target[(79 * 4 + 2) * width + 172 * 4 + 2];
    eprintln!("sky {sky:06x}, glow {shown:06x}");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        write_png(
            &std::path::Path::new(&dir).join("modern-star.png"),
            &target,
            width,
            height,
        );
    }
    assert!(sum(shown) > sum(sky) + 30, "the star glows");
}

/// The hyperspace tunnel (model 0xa5) goes to the GPU like the flight view's models, and fills
/// as much of the view there as in the original.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn the_hyperspace_tunnel_is_drawn_on_the_gpu() {
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
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    // The runtime palette is built from the previous frame's colours: one frame first.
    for _ in 0..2 {
        screen.hyperspace = Some(72 << 8);
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    }
    let classic = screen.logical_pixels().to_vec();
    screen.hyperspace = Some(72 << 8);
    screen.capture = true;
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    let world = screen.world().cloned().expect("the tunnel's world");
    let faces: usize = world.batches.iter().map(Vec::len).sum();
    assert!(faces > 10, "the tunnel's faces reach the GPU ({faces})");
    let (width, height) = (1280usize, 800usize);
    let rect = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut target = vec![0u32; width * height];
    ffe_rs::modern::ModernRenderer::new().render(screen.logical_pixels(), &mut target, width, rect);
    gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
        .unwrap();
    // The streaks move every frame (the tunnel's own animation), so the two frames are
    // compared by how much of the view the tunnel fills, not pixel by pixel.
    let (mut drawn, mut covered) = (0, 0);
    for y in 0..158 {
        for x in 0..320 {
            let at = y * 320 + x;
            drawn += usize::from(classic[at] != classic[0]);
            covered += usize::from(target[(y * 4 + 2) * width + x * 4 + 2] != world.snapshot[0]);
        }
    }
    eprintln!("the tunnel fills {covered} view pixels on the GPU, {drawn} in the original");
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        write_png(
            &std::path::Path::new(&dir).join("modern-tunnel.png"),
            &target,
            width,
            height,
        );
        let mut upscaled = vec![0u32; width * height];
        for (at, p) in upscaled.iter_mut().enumerate() {
            *p = classic[(at / width) / 4 * 320 + (at % width) / 4];
        }
        write_png(
            &std::path::Path::new(&dir).join("classic-tunnel.png"),
            &upscaled,
            width,
            height,
        );
    }
    assert!(drawn > 1000 && covered * 10 >= drawn * 7 && covered * 10 <= drawn * 14);
}

/// The player's ship from outside with its main engine at full thrust, from four sides: the
/// engine glows (F1805's quads over bitmaps 0x123 and 0x65) go to the GPU as flames, hidden by
/// the hull where it is nearer.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn engine_glows_are_drawn_on_the_gpu() {
    use ffe_rs::camera::ViewMode;
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
    let mut state = Flight::free_flight();
    state.thrust = 1.0;
    let mut views = ViewState {
        mode: ViewMode::External,
        ..ViewState::default()
    };
    let (width, height) = (1280usize, 800usize);
    let rect = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut glows = 0;
    for yaw in [0u16, 0x4000, 0x8000, 0xc000] {
        views.external.yaw = yaw;
        screen.capture = false;
        screen.draw(&state, views, Some(&viewer), false, "");
        let classic = screen.logical_pixels().to_vec();
        screen.capture = true;
        screen.draw(&state, views, Some(&viewer), false, "");
        let world = screen.world().cloned().expect("the flight view's world");
        let here = world
            .batches
            .iter()
            .flatten()
            .filter(|solid| solid.flame.is_some())
            .count();
        eprintln!("yaw {yaw:#x}: {here} glow faces");

        glows += here;
        let mut target = vec![0u32; width * height];
        ffe_rs::modern::ModernRenderer::new().render(
            screen.logical_pixels(),
            &mut target,
            width,
            rect,
        );
        gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
            .unwrap();
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let dir = std::path::Path::new(&dir);
            write_png(
                &dir.join(format!("modern-engine-{yaw:04x}.png")),
                &target,
                width,
                height,
            );
            let upscaled: Vec<u32> = (0..width * height)
                .map(|at| classic[(at / width) / 4 * 320 + (at % width) / 4])
                .collect();
            write_png(
                &dir.join(format!("classic-engine-{yaw:04x}.png")),
                &upscaled,
                width,
                height,
            );
        }
    }
    assert!(glows > 0, "an engine glow from some side");
}

/// The free-flight station with a planet between it and the star: its batch is eclipsed, and
/// with the planet moved aside it is not.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_station_in_a_planets_shadow_is_eclipsed() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let state = Flight::free_flight();
    let disc = |centre: [f64; 3], radius: f64, star: bool| Disc {
        centre,
        radius,
        light: [-1.0, 0.0, 0.0],
        colours: (12, 4),
        star,
        model: None,
        orientation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let mut darkest = |planet_y: f64| {
        // The star far along +x; the planet on the way to it, or 10 radii off the line.
        screen.discs = vec![
            disc([1.0e9, 0.0, 0.0], 1.0e6, true),
            disc([1.0e5, planet_y, 0.0], 1.0e4, false),
        ];
        screen.capture = true;
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
        let world = screen.world().cloned().expect("the flight view's world");
        world
            .eclipses
            .iter()
            .map(|(_, seen)| *seen)
            .fold(1.0f32, f32::min)
    };
    let shadowed = darkest(0.0);
    let sunlit = darkest(1.0e5);
    eprintln!("in the shadow {shadowed}, beside it {sunlit}");
    assert!(shadowed < 0.1, "the station is eclipsed");
    assert_eq!(sunlit, 1.0, "out of the shadow it is not");
}

/// An explosion ahead (F942's model 155 at full size) gives off light on the GPU: its batch is
/// marked, and the GPU picture is brighter there than the original's colours alone.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn an_explosion_glows_on_the_gpu() {
    use ffe_rs::screen::SystemEffect;
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
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    state.position = [0.0, 0.0, -3000.0];
    screen.effects = vec![SystemEffect {
        model: ffe_core::effects::EXPLOSION_MODEL,
        centre: [0.0, 0.0, -2800.0],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        timer: 0xc000,
        size: 0xffff,
        scale: None,
    }];
    state.system_flight = true;
    screen.capture = true;
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    let world = screen.world().cloned().expect("the flight view's world");
    eprintln!(
        "{} batches ({:?} solids), glowing {:?}",
        world.batches.len(),
        world.batches.iter().map(Vec::len).collect::<Vec<_>>(),
        world.glowing
    );
    assert!(
        !world.glowing.is_empty(),
        "the explosion's batch gives off light"
    );
    let (width, height) = (1280usize, 800usize);
    let rect = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut target = vec![0u32; width * height];
    ffe_rs::modern::ModernRenderer::new().render(screen.logical_pixels(), &mut target, width, rect);
    gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
        .unwrap();
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        write_png(
            &std::path::Path::new(&dir).join("modern-explosion.png"),
            &target,
            width,
            height,
        );
    }
}

/// The stars of Gateway (the new game's system, Hope's star) and Sol: the glow and glare take
/// the star's own colour from its drawn model, not the flat stand-in disc's green.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn a_stars_glow_takes_the_stars_own_colour() {
    use ffe_rs::navigation::{Kind, Navigator};
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
    let gateway = Navigator::new_game(&cache, 0).unwrap().bodies;
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let sol = bodies(&cache, sol, [0x1000, 0]).unwrap();
    let stars: Vec<_> = gateway
        .iter()
        .chain(sol.iter())
        .filter(|b| b.kind == Kind::Star)
        .cloned()
        .collect();
    assert!(!stars.is_empty());
    for (n, star) in stars.iter().enumerate() {
        let radius = star.radius as f64 / UNIT;
        let centre = star.position.map(|v| v as f64 / UNIT);
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        state.position = [centre[0], centre[1], centre[2] - 20.0 * radius];
        state.system_flight = true;
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [0.0, 0.0, 1.0],
            colours: (17, 17),
            star: true,
            model: Some((star.model, star.seed)),
            orientation: star.orientation,
        }];
        screen.capture = true;
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
        let world = screen.world().cloned().expect("the flight view's world");
        let glow = world.star.as_ref().expect("the star in view").rgb;
        eprintln!("{} (model {:#x}): glow {glow:?}", star.name, star.model);
        let [r, g, b] = glow.map(u32::from);
        assert!(
            g <= r.max(b) + 16,
            "{}'s glow is not green: {glow:?}",
            star.name
        );
        // The same colour from any side, off-centre, and too far away for the model to be
        // drawn (from 200 radii the flat stand-in disc is drawn instead), lit as the game
        // lights a star (no light vector: the star is at the light's origin).
        let mut seen = Screen::new(&cache).unwrap();
        seen.discs = screen.discs.clone();
        seen.capture = true;
        for distance in [3.0, 200.0, 5000.0] {
            for k in 0..8 {
                let a = f64::from(k) * std::f64::consts::TAU / 8.0;
                let yaw = a + 0.15;
                let mut view = state.clone();
                view.axes = Some([
                    [yaw.cos(), 0.0, yaw.sin()],
                    [0.0, 1.0, 0.0],
                    [-yaw.sin(), 0.0, yaw.cos()],
                ]);
                view.position = [
                    centre[0] + a.sin() * distance * radius,
                    centre[1],
                    centre[2] - a.cos() * distance * radius,
                ];
                seen.discs[0].light = [0.0; 3];
                seen.draw(&view, ViewState::default(), Some(&viewer), false, "");
                let rgb = seen.world().and_then(|w| w.star.as_ref()).map(|s| s.rgb);
                assert_eq!(
                    rgb,
                    Some(glow),
                    "{} from {distance} radii at {k}/8 of a turn",
                    star.name
                );
            }
        }
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
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
            gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
                .unwrap();
            write_png(
                &std::path::Path::new(&dir).join(format!("modern-star-{n}.png")),
                &target,
                width,
                height,
            );
        }
    }
}

/// The gunsight and the targetter's cursor are drawn crisp over the 3-D view: the original's
/// pixels give way to the 3-D view, and the same design is drawn as fine lines at the output's
/// resolution.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn the_gunsight_is_drawn_crisp() {
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
    let state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    screen.lasers = [1, 0, 0, 0];
    screen.targeter = Some(ffe_rs::targeter::Cursor::Combat);
    screen.blink = true;
    screen.capture = true;
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    let world = screen.world().cloned().expect("the flight view's world");
    // The fixed sight and the cursor's circle (and the velocity marker, when it is ahead).
    assert!(world.hud.len() >= 2, "{}", world.hud.len());
    // Their original pixels are let through to the 3-D view: the top of the sight's
    // vertical line (bitmap 97's column 30, rows 0..20, at (129,48)).
    let logical = screen.logical_pixels();
    for y in 48..68 {
        let at = y * 320 + 159;
        assert_eq!(world.crisp[at], Some(logical[at]), "row {y}");
    }
    let (width, height) = (1280usize, 960usize);
    let rect = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut target = vec![0u32; width * height];
    ffe_rs::modern::ModernRenderer::new().render(logical, &mut target, width, rect);
    gpu.render(logical, &world, &mut target, width, rect)
        .unwrap();
    // The line's centre is the sight's colour; a pixel and a half to the side is the sky.
    let sight = world.hud[0].rgb.map(u32::from);
    let at = |x: f32, y: f32| target[(y * 4.8) as usize * width + (x * 4.0) as usize];
    let rgb = |p: u32| [(p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff];
    let centre = rgb(at(159.5, 55.5));
    for k in 0..3 {
        assert!(
            centre[k].abs_diff(sight[k]) <= 8,
            "{centre:?} against {sight:?}"
        );
    }
    let beside = rgb(at(161.5, 55.5));
    assert!(
        beside.iter().zip(&sight).any(|(a, b)| a.abs_diff(*b) > 40),
        "{beside:?}"
    );
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        write_png(
            &std::path::Path::new(&dir).join("modern-gunsight.png"),
            &target,
            width,
            height,
        );
    }
}

/// The modern engine's speed dust: motes all round the ship, each a streak pointing back towards
/// the way the ship is going (so they rush outward from it), none while paused.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn speed_dust_streams_out_from_the_way_ahead() {
    let mut gpu = Gpu3d::new();
    if !gpu.available() {
        eprintln!("no GPU adapter; skipped");
        return;
    }
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    // Flight units a second (9.7 m each): 300 m/s, 5 km/s, 5 km/s at 1000x and 50,000 km/s at
    // 10000x (past the light speed where the first version's layers ran out).
    for (n, (speed, acceleration)) in [(31.0, 0usize), (515.0, 0), (515.0, 3), (5.15e6, 4)]
        .into_iter()
        .enumerate()
    {
        let mut screen = Screen::new(&cache).unwrap();
        screen.capture = true;
        let state = Flight {
            mode: Mode::Flight,
            velocity: [0.0, 0.0, speed],
            axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
            system_flight: true,
            ..Flight::default()
        };
        let mut views = ViewState::default();
        views.console.acceleration = acceleration;
        for _ in 0..3 {
            screen.dust_step = Some(1.0 / 60.0);
            screen.draw(&state, views, Some(&viewer), false, "");
        }
        let world = screen.world().cloned().expect("the flight view's world");
        let ahead: Vec<_> = world
            .dust
            .iter()
            .filter(|s| s.head[2] > 0.0 && s.tail[2] > 0.0)
            .collect();
        eprintln!("{} streaks, {} ahead", world.dust.len(), ahead.len());
        assert!(ahead.len() > 50, "dust all round at speed {speed}");
        // Projected, each tail is nearer the middle of the view than its head.
        let off = |p: [f32; 3]| (p[0] / p[2]).hypot(p[1] / p[2]);
        let outward = ahead
            .iter()
            .filter(|s| off(s.tail) <= off(s.head) + 1.0e-4)
            .count();
        assert_eq!(
            outward,
            ahead.len(),
            "every streak points back to the way ahead"
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
            let (width, height) = (1280usize, 960usize);
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
            gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
                .unwrap();
            write_png(
                &std::path::Path::new(&dir).join(format!("modern-dust-{n}.png")),
                &target,
                width,
                height,
            );
        }
        // Paused, nothing moves: the motes are points.
        views.console.paused = true;
        screen.dust_step = Some(1.0 / 60.0);
        screen.draw(&state, views, Some(&viewer), false, "");
        let world = screen.world().cloned().unwrap();
        assert!(world.dust.iter().all(|s| s.head == s.tail));
    }
}

/// Each higher time step makes the speed dust look faster: at 5 km/s, the streaks are longer on
/// the screen at 10x than at 1x, at 100x than at 10x, and so on up to 10000x.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn speed_dust_looks_faster_at_each_time_step() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let state = Flight {
        mode: Mode::Flight,
        velocity: [0.0, 0.0, 515.0],
        axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        system_flight: true,
        ..Flight::default()
    };
    let mut previous = 0.0f32;
    for acceleration in 0..5 {
        let mut screen = Screen::new(&cache).unwrap();
        screen.capture = true;
        let mut views = ViewState::default();
        views.console.acceleration = acceleration;
        for _ in 0..3 {
            screen.dust_step = Some(1.0 / 60.0);
            screen.draw(&state, views, Some(&viewer), false, "");
        }
        let world = screen.world().cloned().unwrap();
        // The median angle a streak sweeps across the view (a few motes beside the ship sweep
        // far more, whatever the speed).
        let mut angles: Vec<f32> = world
            .dust
            .iter()
            .filter(|s| s.head[2] > 0.0 && s.tail[2] > 0.0)
            .map(|s| {
                let dot: f32 = (0..3).map(|k| s.head[k] * s.tail[k]).sum();
                let size = |p: [f32; 3]| p.iter().map(|c| c * c).sum::<f32>().sqrt();
                (dot / (size(s.head) * size(s.tail)))
                    .clamp(-1.0, 1.0)
                    .acos()
            })
            .collect();
        angles.sort_by(f32::total_cmp);
        let mean = angles.get(angles.len() / 2).copied().unwrap_or(0.0);
        eprintln!(
            "step {acceleration}: median streak {mean:.5} rad of {}",
            angles.len()
        );
        assert!(
            mean > previous,
            "step {acceleration} looks no faster than the one below"
        );
        previous = mean;
    }
}

/// A station in front of Hope's sun hides it: fully in front, the glare and lens flare go, and
/// half in front they dim; the sun's hot core is drawn behind the station, so what is left of it
/// blooms round the station's edge.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE) and a GPU"]
fn a_station_in_front_of_the_star_hides_it() {
    use ffe_rs::navigation::{Kind, Navigator};
    use ffe_rs::screen::SystemStation;
    let mut gpu = Gpu3d::new();
    if !gpu.available() {
        eprintln!("no GPU adapter; skipped");
        return;
    }
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let bodies = Navigator::new_game(&cache, 0).unwrap().bodies;
    let star = bodies.iter().find(|b| b.kind == Kind::Star).unwrap();
    let radius = star.radius as f64 / UNIT;
    let centre = star.position.map(|v| v as f64 / UNIT);
    let eye = [centre[0], centre[1], centre[2] - 20.0 * radius];
    let state = Flight {
        mode: Mode::Flight,
        axes: Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        position: eye,
        system_flight: true,
        ..Flight::default()
    };
    // The flare's glare (the last batch's first glow) for the station `offset` flight units to
    // the side of the line to the sun, or none.
    let mut glare = |offset: Option<f64>, name: &str| {
        let mut screen = Screen::new(&cache).unwrap();
        screen.capture = true;
        screen.discs = vec![Disc {
            centre,
            radius,
            light: [0.0; 3],
            colours: (17, 17),
            star: true,
            model: Some((star.model, star.seed)),
            orientation: star.orientation,
        }];
        screen.stations = offset
            .map(|x| SystemStation {
                model: 74,
                centre: [eye[0] + x, eye[1], eye[2] + 300.0],
                orientation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                speed: 0,
                door: 0xffff,
                tics: ffe_core::model_animation::START_TICS,
                seed: 0,
            })
            .into_iter()
            .collect();
        screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
        let world = screen.world().cloned().expect("the flight view's world");
        let scene = ffe_rs::modern3d::scene(
            &world,
            1280,
            632,
            &mut ffe_rs::modern3d::MapCache::default(),
            0.0,
        );
        if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
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
            gpu.render(screen.logical_pixels(), &world, &mut target, width, rect)
                .unwrap();
            write_png(
                &std::path::Path::new(&dir).join(format!("modern-sun-{name}.png")),
                &target,
                width,
                height,
            );
        }
        scene
            .batches
            .last()
            .and_then(|batch| batch.sprites.first())
            .map_or(0.0, |sprite| {
                sprite.halo.iter().copied().fold(0.0, f32::max)
            })
    };
    let open = glare(None, "open");
    let behind = glare(Some(0.0), "behind");
    let edge = glare(Some(75.0), "edge");
    eprintln!("glare: open {open}, behind the station {behind}, at its edge {edge}");
    assert!(open > 0.0, "the sun glares in the open");
    assert_eq!(behind, 0.0, "no glare with the station over the whole sun");
    assert!(
        edge > 0.0 && edge < open,
        "dimmer with the station over part of it"
    );
}
