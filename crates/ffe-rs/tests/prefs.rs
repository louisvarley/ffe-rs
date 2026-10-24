//! The Escape preference pages: layout, hot areas, keys, the applied settings
//! and the space colour in the drawn frame.
use ffe_rs::flight::Flight;
use ffe_rs::indexed::IndexedImage;
use ffe_rs::prefs::{self, Activate, Key, Prefs, GRAPHIC, MAIN, MAPS, OTHERS, PORT, SOUND};
use ffe_rs::screen::Screen;

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
        assert_eq!(prefs::space_colour_rgb(colour), 0x11 * colour as u32);
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
    // Space dust (0), elite control (5) and video (9) are not implemented.
    for (y, index) in [(28, 0), (78, 5), (118, 9)] {
        let before = prefs.others[index];
        prefs.click(34, y);
        assert_eq!(prefs.others[index], before);
    }
}

#[test]
fn maps_and_sound_pages_show_the_originals_defaults_without_applying_them() {
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
        assert!(!prefs.rows()[index].applied);
    }
    prefs.click(34, 28);
    assert_eq!(prefs.sound, [true; 10]);
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
    for (index, expected) in [(4, 0x00_0044), (0, 0x00_0000), (2, 0x00_0022)] {
        screen.sky = prefs::space_colour_rgb(index);
        screen.draw(
            &state,
            ffe_rs::camera::ViewState::default(),
            None,
            false,
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
