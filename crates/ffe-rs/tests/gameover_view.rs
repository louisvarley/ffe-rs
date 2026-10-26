//! Local-only game-over tombstone render (F917/model 172). Run with
//! `FFE_GAMEOVER_CACHE=<data.bin> FFE_GAMEOVER_OUT=<dir> cargo test -p ffe-rs
//! --test gameover_view -- --ignored`.
//!
//! This is a smoke test of the assembled screen, not a pixel oracle: it checks
//! that the tombstone and its engraved FF9C text are produced by the real
//! cache's model and glyph data. The executed original's frames are compared in
//! `docs/notes/gameover.md`.
use ffe_rs::gameover::{GameOver, Tombstone, TOMBSTONE_MODEL};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::screen::Screen;

fn cache() -> ffe_content::VerifiedCache {
    let path = std::env::var_os("FFE_GAMEOVER_CACHE").expect("FFE_GAMEOVER_CACHE");
    ffe_content::read(&std::fs::read(path).unwrap()).unwrap()
}

fn write_ppm(screen: &Screen, name: &str) {
    let Some(dir) = std::env::var_os("FFE_GAMEOVER_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut data = b"P6\n320 200\n255\n".to_vec();
    for pixel in screen.logical_pixels().iter().take(320 * 200) {
        data.push((pixel >> 16) as u8);
        data.push((pixel >> 8) as u8);
        data.push(*pixel as u8);
    }
    std::fs::write(dir.join(name), data).unwrap();
}

fn lit(screen: &Screen) -> usize {
    screen
        .logical_pixels()
        .iter()
        .take(320 * 200)
        .filter(|pixel| **pixel != 0)
        .count()
}

fn light_text(screen: &Screen) -> usize {
    screen
        .logical_pixels()
        .iter()
        .take(320 * 200)
        .filter(|pixel| {
            let pixel = **pixel;
            let [r, g, b] = [pixel >> 16, pixel >> 8, pixel].map(|v| v & 0xff);
            r.min(g).min(b) > 110
        })
        .count()
}

#[test]
#[ignore = "Local-only game-over frame render; set FFE_GAMEOVER_CACHE to a v28 data.bin"]
fn the_tombstone_draws_game_over_and_the_obituary() {
    let cache = cache();
    let mut viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let game = GameOver::tombstone("JAMESON", "1-JAN-3250");
    assert_eq!(game.tombstone_kind(), Tombstone::Normal);
    assert_eq!(game.tombstone_kind().model(), TOMBSTONE_MODEL);
    viewer.set_game_over_text(game.commander().as_bytes(), game.date().as_bytes());

    // The `GAME OVER` face at F917's 501st tombstone frame, where the slab is at
    // `TOMBSTONE_NEAREST` and the counter is -272543: the same view as the
    // original's `death-frame-990`.
    screen
        .draw_game_over(&mut viewer, TOMBSTONE_MODEL, -272_543)
        .unwrap();
    write_ppm(&screen, "gameover-game-over.ppm");
    assert!(
        lit(&screen) > 12_000,
        "the tombstone covers only {} pixels",
        lit(&screen)
    );
    assert!(
        light_text(&screen) > 2_000,
        "the engraved text is missing: {} light pixels",
        light_text(&screen)
    );
    let front = screen.logical_pixels()[..320 * 200].to_vec();

    // Half a turn later the other face's obituary is readable. A counter of
    // -74374 is F917's loop 155.7 degrees round with the slab at
    // `TOMBSTONE_NEAREST` (the same frame as the original's `death-frame-800`).
    screen
        .draw_game_over(&mut viewer, TOMBSTONE_MODEL, -74_374)
        .unwrap();
    write_ppm(&screen, "gameover-obituary.ppm");
    assert!(
        light_text(&screen) > 2_000,
        "the obituary is missing: {} light pixels",
        light_text(&screen)
    );
    assert_ne!(front, screen.logical_pixels()[..320 * 200]);
}

#[test]
#[ignore = "Local-only game-over frame render; set FFE_GAMEOVER_CACHE to a v28 data.bin"]
fn the_rare_variant_is_the_other_tombstone_model() {
    let cache = cache();
    let mut viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let game = GameOver::tombstone("JAMESON", "1-JAN-3250").with_variant(Tombstone::Rare);
    viewer.set_game_over_text(game.commander().as_bytes(), game.date().as_bytes());
    screen
        .draw_game_over(&mut viewer, game.tombstone_kind().model(), -272_543)
        .unwrap();
    write_ppm(&screen, "gameover-rare.ppm");
    // Model 175's flowers/centre are visible at F917's pitch-0 spin; the normal
    // tombstone's slab is not part of this model.
    assert!(lit(&screen) > 500);
}
