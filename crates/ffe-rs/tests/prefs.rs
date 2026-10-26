//! The Escape preference pages: layout, hot areas, keys, the applied settings
//! and the space colour in the drawn frame.
use ffe_rs::display::{DisplaySettings, Filter, FitMode, PixelAspect};
use ffe_rs::flight::Flight;
use ffe_rs::indexed::IndexedImage;
use ffe_rs::prefs::{
    self, escape_action, Activate, Escape, Key, Prefs, DISPLAY, GRAPHIC, MAIN, MAPS, OTHERS, PORT,
    SOUND,
};
use ffe_rs::screen::Screen;

/// F224 at the default start: Escape 1 stops time (no page), Escape 2 opens
/// the pages, Escape 3 returns to the game with the clock still stopped. The
/// reference `--game-frame` captures `hope-esc1..3.png` show the same order.
#[test]
fn escape_pauses_then_opens_the_pages_then_returns() {
    assert_eq!(escape_action(false, false), Escape::Pause);
    assert_eq!(escape_action(false, true), Escape::Open);
    assert_eq!(escape_action(true, true), Escape::Close);
    // The pages cannot be open while time runs in this port, but the function
    // still closes them if they are.
    assert_eq!(escape_action(true, false), Escape::Close);
}

#[test]
fn main_page_hot_areas_are_the_originals_icon_squares() {
    let mut prefs = Prefs::default();
    prefs.escape();
    // `DATA_004898`: the left column's 8x8 icon squares at x 32, y 28..78,
    // and the right column's at x 112.
    for (x, y, page) in [
        (34, 30, GRAPHIC),
        (34, 40, SOUND),
        (34, 50, OTHERS),
        (34, 60, MAPS),
        (34, 70, PORT),
    ] {
        prefs.page = MAIN;
        assert_eq!(prefs.click(x, y), Activate::None);
        assert_eq!(prefs.page, page);
    }
    // The label itself is not a hot area, as in the original.
    prefs.page = MAIN;
    assert_eq!(prefs.click(70, 30), Activate::None);
    assert_eq!(prefs.page, MAIN);
    // Return to Game closes; Quit reports the port's exit; the rows the port
    // does not implement (Load/Save/Delete/New Game) do nothing.
    assert_eq!(prefs.click(114, 60), Activate::Close);
    assert!(!prefs.open);
    prefs.open = true;
    assert_eq!(prefs.click(34, 80), Activate::Quit);
    for (x, y) in [(114, 30), (114, 40), (114, 50), (114, 80)] {
        assert_eq!(prefs.click(x, y), Activate::None);
    }
}

#[test]
fn graphic_page_sets_the_space_colour_and_the_stored_shape_detail() {
    let mut prefs = Prefs::default();
    prefs.escape();
    prefs.page = GRAPHIC;
    // `DATA_004903`: shape detail at y 24..54, space colour at y 80..120.
    for (y, level) in [(24, 0), (34, 1), (44, 2), (54, 3)] {
        prefs.click(34, y);
        assert_eq!(prefs.shape_detail, level);
    }
    for (y, colour) in [(80, 0), (90, 1), (100, 2), (110, 3), (120, 4)] {
        prefs.click(34, y);
        assert_eq!(prefs.space_colour, colour);
        assert_eq!(
            prefs::space_colour_rgb(colour),
            [0x00_0000, 0x04_0414, 0x08_0828, 0x08_0838, 0x0c_0c4c][colour]
        );
    }
    // `0x9e`: Main Menu.
    assert_eq!(prefs.click(34, 136), Activate::None);
    assert_eq!(prefs.page, MAIN);
}

#[test]
fn others_page_applies_stars_and_reverse_and_keeps_the_rest_inert() {
    let mut prefs = Prefs::default();
    prefs.escape();
    prefs.page = OTHERS;
    // `DATA_004904`: rows at y 28..118. Background stars (index 1) and the two
    // reverse options (3, 4) are the port's applied settings.
    assert!(prefs.others[1]);
    prefs.click(34, 38);
    assert!(!prefs.others[1]);
    prefs.click(34, 38);
    assert!(prefs.others[1]);
    // Lock external view to ship (index 2) is applied through ViewState.
    assert!(prefs.others[2]);
    prefs.click(34, 48);
    assert!(!prefs.others[2]);
    prefs.click(34, 58);
    assert!(prefs.others[3]);
    prefs.click(34, 68);
    assert!(prefs.others[4]);
    // Space dust (index 0) now gates F1035's spawn; elite control (5) and
    // video (9) remain inert.
    assert!(prefs.others[0]);
    prefs.click(34, 28);
    assert!(!prefs.others[0]);
    prefs.click(34, 28);
    assert!(prefs.others[0]);
    for (y, index) in [(78, 5), (118, 9)] {
        let before = prefs.others[index];
        prefs.click(34, y);
        assert_eq!(prefs.others[index], before);
    }
}

#[test]
fn maps_stay_inert_and_the_sound_rows_apply_the_originals_switches() {
    let mut prefs = Prefs::default();
    prefs.escape();
    prefs.page = MAPS;
    for (index, label) in prefs::MAP_LABELS.iter().enumerate() {
        assert_eq!(prefs.rows()[index].label, *label);
        assert!(prefs.rows()[index].mark == prefs::Mark::Toggle(true));
        assert!(!prefs.rows()[index].applied);
    }
    prefs.click(34, 28);
    assert_eq!(prefs.maps, [true; 5]);
    prefs.page = SOUND;
    for (index, label) in prefs::SOUND_LABELS.iter().enumerate() {
        assert_eq!(prefs.rows()[index].label, *label);
        // The port now applies all ten Sound Effects switches.
        assert!(prefs.rows()[index].applied);
    }
    prefs.click(34, 28);
    assert_eq!(
        prefs.sound,
        [false, true, true, true, true, true, true, true, true, true]
    );
    assert_eq!(prefs.texts().iter().filter(|t| t.x == 160).count(), 16);
}

#[test]
fn port_page_switches_modern_and_classic_controls() {
    let mut prefs = Prefs::default();
    prefs.escape();
    prefs.page = PORT;
    assert!(prefs.modern);
    assert_eq!(prefs.click(34, 48), Activate::None);
    assert!(!prefs.modern);
    assert_eq!(prefs.click(34, 38), Activate::None);
    assert!(prefs.modern);
}

#[test]
fn the_port_page_flips_the_preset_and_shows_it() {
    use ffe_rs::input::Preset;
    use ffe_rs::prefs::Mark;

    let mut prefs = Prefs::default();
    assert_eq!(Preset::from_modern(prefs.modern), Preset::Modern);
    prefs.toggle_modern();
    assert_eq!(Preset::from_modern(prefs.modern), Preset::Classic);
    prefs.escape();
    prefs.page = PORT;
    let rows = prefs.rows();
    let modern = rows.iter().find(|row| row.label == "Modern").unwrap();
    let classic = rows.iter().find(|row| row.label == "Classic").unwrap();
    assert_eq!(modern.mark, Mark::Radio(false));
    assert_eq!(classic.mark, Mark::Radio(true));
    // The page switches the same value back immediately.
    assert_eq!(prefs.click(34, 38), Activate::None);
    assert!(prefs.modern);
}

#[test]
fn display_page_sets_the_port_only_presentation_options() {
    let mut prefs = Prefs::default();
    prefs.escape();
    prefs.page = DISPLAY;
    assert_eq!(prefs.display, DisplaySettings::default());
    let labels: Vec<&str> = prefs.rows().iter().map(|row| row.label).collect();
    assert!(labels.contains(&"4x"));
    assert!(labels.contains(&"Pixel-perfect"));
    assert!(labels.contains(&"Nearest"));
    assert!(labels.contains(&"4:3 (DOS)"));
    assert!(labels.contains(&"Fullscreen"));
    // 3x, fill window, linear, square pixels.
    prefs.click(34, 56);
    prefs.click(178, 46);
    prefs.click(178, 80);
    prefs.click(178, 114);
    assert_eq!(
        prefs.display,
        DisplaySettings {
            scale: 3,
            fit: FitMode::FillWindow,
            filter: Filter::Linear,
            aspect: PixelAspect::Square,
        }
    );
    // Out-of-range scale rows do nothing.
    assert_eq!(prefs.apply(prefs::Action::WindowScale(0)), Activate::None);
    assert_eq!(prefs.display.scale, 3);
    // The Main Menu row returns to the original page list.
    assert_eq!(prefs.click(34, 136), Activate::None);
    assert_eq!(prefs.page, MAIN);
    // Port Options opens the display page from its port-only row at y 104.
    prefs.page = PORT;
    assert_eq!(prefs.click(34, 104), Activate::None);
    assert_eq!(prefs.page, DISPLAY);
}

#[test]
fn keyboard_navigation_activates_rows_and_escape_closes() {
    let mut prefs = Prefs::default();
    assert_eq!(prefs.escape(), Activate::None);
    assert!(prefs.open);
    // Six rows down from Graphic Detail reaches Quit; Enter reports the exit.
    for _ in 0..5 {
        assert_eq!(prefs.key(Key::Down), Activate::None);
    }
    assert_eq!(prefs.rows()[prefs.selected].label, "Quit");
    assert_eq!(prefs.key(Key::Enter), Activate::Quit);
    // Backspace returns to the main page; Escape closes the pages.
    prefs.page = GRAPHIC;
    prefs.key(Key::Backspace);
    assert_eq!(prefs.page, MAIN);
    assert_eq!(prefs.key(Key::Escape), Activate::Close);
    assert!(!prefs.open);
}

fn screen() -> Screen {
    let mut font = vec![0xff; 1160];
    for width in font.iter_mut().skip(9).step_by(10) {
        *width = 5;
    }
    let panel = IndexedImage::new(320, 43, [0, 0], false, &vec![2; 320 * 43]).unwrap();
    let palette = std::array::from_fn(|index| index as u32);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 320, 158);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![100; 320 * 158 * 3]).unwrap();
    }
    Screen::from_assets(&bytes, &font, panel, palette).unwrap()
}

#[test]
fn the_preference_frame_uses_the_originals_grey_and_keeps_the_console() {
    let mut screen = screen();
    let prefs = Prefs::default();
    screen.draw_prefs(&prefs);
    let pixels = screen.logical_pixels();
    // The menu background is palette 3, the original's (3,3,3) clear colour.
    assert_eq!(pixels[0], 3);
    assert_eq!(pixels[100 * 320 + 300], 3);
    // The console panel below the 158-row view stays as the flight screen has it.
    assert_eq!(pixels[158 * 320], 2);
    // The title and a label are drawn in the original colours.
    let view = &pixels[..320 * 158];
    assert!(view.contains(&16), "the red title");
    assert!(view.contains(&15), "the white labels");
    assert!(view.contains(&17), "the green commander line");
    assert!(view.contains(&18), "the port-added yellow rows");
}

#[test]
fn the_flight_background_is_the_original_space_colour() {
    let mut screen = screen();
    let state = Flight::free_flight();
    let view = 320 * 158;
    // The captured original: the default "Light Blue" is palette entry 182,
    // six-bit (3,3,19).
    assert_eq!(screen.sky, 0x0c_0c4c);
    for (index, expected) in [
        (4, 0x0c_0c4c),
        (0, 0x00_0000),
        (2, 0x08_0828),
        (3, 0x08_0838),
        (1, 0x04_0414),
    ] {
        screen.sky = prefs::space_colour_rgb(index);
        screen.draw(
            &state,
            ffe_rs::camera::ViewState::default(),
            None,
            true,
            "test",
        );
        let count = screen.logical_pixels()[..view]
            .iter()
            .filter(|pixel| **pixel == expected)
            .count();
        assert!(
            count > view / 2,
            "space colour {index}: {count} of {view} pixels"
        );
    }
}

/// The Graphic Detail page's "Black" still leaves the original's stars drawn
/// over the black sky (the "Background stars" option in Others stays on).
#[test]
fn the_black_space_colour_still_draws_the_stars() {
    let mut screen = screen();
    let state = Flight::free_flight();
    let view = 320 * 158;
    assert_eq!(prefs::space_colour_rgb(0), 0x00_0000);
    screen.sky = prefs::space_colour_rgb(0);
    screen.stars_visible = true;
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        None,
        true,
        "test",
    );
    let with_stars = screen.logical_pixels().to_vec();
    let black = with_stars[..view]
        .iter()
        .filter(|pixel| **pixel == 0)
        .count();
    assert!(black > view / 2, "black sky: {black} of {view} pixels");
    screen.stars_visible = false;
    screen.draw(
        &state,
        ffe_rs::camera::ViewState::default(),
        None,
        true,
        "test",
    );
    let stars = with_stars[..view]
        .iter()
        .zip(&screen.logical_pixels()[..view])
        .filter(|(with, without)| with != without)
        .count();
    assert!(stars > 0, "the stars are still drawn over black space");
}

/// A selection made on the Graphic Detail page survives closing and
/// re-opening the pages in the same session.
#[test]
fn the_space_colour_persists_within_the_session() {
    let mut prefs = Prefs::default();
    assert_eq!(prefs.space_colour, 4);
    prefs.escape();
    prefs.page = GRAPHIC;
    // The existing "Black" row at y 80 (`DATA_004903`'s 0x85).
    assert_eq!(prefs.click(34, 80), Activate::None);
    assert_eq!(prefs.space_colour, 0);
    // Return to Game, then Escape again: the page state and the choice remain.
    assert_eq!(prefs.click(34, 136), Activate::None);
    assert_eq!(prefs.page, MAIN);
    prefs.key(Key::Escape);
    assert!(!prefs.open);
    prefs.escape();
    prefs.page = GRAPHIC;
    assert_eq!(prefs.space_colour, 0);
    let rows = prefs.rows();
    let black = rows.iter().find(|row| row.label == "Black").unwrap();
    let blue = rows.iter().find(|row| row.label == "Light Blue").unwrap();
    assert_eq!(black.mark, prefs::Mark::Radio(true));
    assert_eq!(blue.mark, prefs::Mark::Radio(false));
}
