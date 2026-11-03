//! Whole screens through Modern graphics (the intro and F917's tombstone): the classic frame
//! and the Modern picture (upscale, GPU models, real font). Local-only: needs the content cache
//! and a GPU.
//!
//! `FFE_INTRO_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test modern_screens --
//! --ignored --nocapture`

use ffe_rs::display::DisplaySettings;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::modern::{Graphics, ModernRenderer};
use ffe_rs::modern3d::Gpu3d;
use ffe_rs::screen::Screen;

/// The Modern picture of the screen's last frame at `width` x `height`.
fn modern_picture(
    screen: &mut Screen,
    modern: &mut ModernRenderer,
    gpu: &mut Gpu3d,
    width: usize,
    height: usize,
) -> Vec<u32> {
    let settings = DisplaySettings {
        graphics: Graphics::Modern,
        ..DisplaySettings::default()
    };
    let rect = settings.rect((width, height));
    let mut target = vec![0; width * height];
    modern.set_text(screen.take_text());
    modern.render(screen.logical_pixels(), &mut target, width, rect);
    if let Some(world) = screen.world() {
        let frame = modern.frame(screen.logical_pixels()).to_vec();
        gpu.render(&frame, world, &mut target, width, rect).unwrap();
    }
    modern.finish(&mut target, width, rect);
    target
}

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

#[test]
#[ignore = "Local-only: needs the content cache (FFE_INTRO_CACHE) and a GPU"]
fn intro_frames_through_modern_graphics() {
    let mut gpu = Gpu3d::new();
    if !gpu.available() {
        eprintln!("no GPU adapter; skipped");
        return;
    }
    let path = std::env::var_os("FFE_INTRO_CACHE").expect("FFE_INTRO_CACHE");
    let cache = ffe_content::read(&std::fs::read(path).unwrap()).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let out = std::env::var_os("FFE_SHOT_DIR").map(std::path::PathBuf::from);
    let (width, height) = (1280, 960);
    let mut modern = ModernRenderer::new();
    for tics in [0x10000u32, 0x40000, 0x7a000, 0x90000, 0x200000, 0x400000] {
        screen.take_text();
        screen.draw_intro(Some(&viewer), tics, false).unwrap();
        let classic = screen.logical_pixels()[..320 * 200].to_vec();
        screen.take_text();
        screen.capture = true;
        screen.draw_intro(Some(&viewer), tics, false).unwrap();
        screen.capture = false;
        let target = modern_picture(&mut screen, &mut modern, &mut gpu, width, height);
        if let Some(dir) = &out {
            write_png(
                &dir.join(format!("intro-{tics:06x}-classic.png")),
                &classic,
                320,
                200,
            );
            write_png(
                &dir.join(format!("intro-{tics:06x}-modern.png")),
                &target,
                width,
                height,
            );
        }
    }
}

/// F917's tombstone in the full-screen GPU view: its faces are set aside, the view covers all
/// 200 rows with F1479's centre on row 79, and the classic frame is unchanged.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_INTRO_CACHE) and a GPU"]
fn the_tombstone_is_drawn_on_the_gpu() {
    use ffe_rs::gameover::{GameOver, RARE_TOMBSTONE_MODEL, TOMBSTONE_MODEL};
    let mut gpu = Gpu3d::new();
    if !gpu.available() {
        eprintln!("no GPU adapter; skipped");
        return;
    }
    let path = std::env::var_os("FFE_INTRO_CACHE").expect("FFE_INTRO_CACHE");
    let cache = ffe_content::read(&std::fs::read(path).unwrap()).unwrap();
    let mut viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let out = std::env::var_os("FFE_SHOT_DIR").map(std::path::PathBuf::from);
    let game = GameOver::tombstone("JAMESON", "1-JAN-3250");
    viewer.set_game_over_text(game.commander().as_bytes(), game.date().as_bytes());
    let mut modern = ModernRenderer::new();
    // F917's start, part way in, the GAME OVER face and the obituary face, then the rare one.
    for (model, remaining) in [
        (TOMBSTONE_MODEL, 200_000i64),
        (TOMBSTONE_MODEL, 60_000),
        (TOMBSTONE_MODEL, -272_543),
        (TOMBSTONE_MODEL, -74_374),
        (RARE_TOMBSTONE_MODEL, -272_543),
    ] {
        screen
            .draw_game_over(&mut viewer, model, remaining)
            .unwrap();
        let classic = screen.logical_pixels()[..320 * 200].to_vec();
        screen.take_text();
        screen.capture = true;
        screen
            .draw_game_over(&mut viewer, model, remaining)
            .unwrap();
        screen.capture = false;
        let world = screen.world().expect("the tombstone is set aside");
        assert!(world.full_screen && world.rows() == 200);
        assert!(world.batches.iter().flatten().count() > 0);
        let target = modern_picture(&mut screen, &mut modern, &mut gpu, 1280, 960);
        // A steady frame of the GPU pass alone (the first opens pipelines and uploads textures).
        let settings = DisplaySettings {
            graphics: Graphics::Modern,
            ..DisplaySettings::default()
        };
        let mut again = target.clone();
        let world = screen.world().unwrap().clone();
        let start = std::time::Instant::now();
        gpu.render(
            screen.logical_pixels(),
            &world,
            &mut again,
            1280,
            settings.rect((1280, 960)),
        )
        .unwrap();
        println!(
            "tombstone {model} at {remaining}: {} batches, {:.1} ms",
            screen.world().map_or(0, |world| world.batches.len()),
            start.elapsed().as_secs_f64() * 1000.0
        );
        // Drawing again without `capture` gives the classic frame back.
        screen
            .draw_game_over(&mut viewer, model, remaining)
            .unwrap();
        assert_eq!(screen.logical_pixels()[..320 * 200], classic[..]);
        assert!(screen.world().is_none());
        if let Some(dir) = &out {
            let name = format!("tombstone-{model}-{remaining}");
            write_png(&dir.join(format!("{name}-classic.png")), &classic, 320, 200);
            write_png(&dir.join(format!("{name}-modern.png")), &target, 1280, 960);
        }
    }
}
