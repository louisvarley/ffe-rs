//! The two flight presets at the playable's boundary: Classic's right-button
//! mouse steering and Enter thrust fly the ship; Modern ignores both. The test
//! also captures the short input/frame strip in `docs/evidence/controls/`.
use ffe_rs::camera::ViewState;
use ffe_rs::flight::Flight;
use ffe_rs::indexed::IndexedImage;
use ffe_rs::input::{self, MouseSteer, Preset, View};
use ffe_rs::screen::Screen;
use minifb::Key;
use std::fs;
use std::path::Path;

fn classic_input(held: &[Key], mouse: MouseSteer) -> ffe_rs::flight::Controls {
    input::flight_controls(Preset::Classic, View::Ship, held, mouse, [false; 2])
}

fn modern_input(held: &[Key], mouse: MouseSteer) -> ffe_rs::flight::Controls {
    input::flight_controls(Preset::Modern, View::Ship, held, mouse, [false; 2])
}

#[test]
fn classic_right_button_and_enter_fly_while_modern_ignores_them() {
    let steering = MouseSteer {
        held: true,
        dx: 20,
        dy: -10,
    };

    // Classic: Enter raises the prototype thrust and the held right button adds
    // yaw and pitch.
    let mut classic = Flight::free_flight();
    for _ in 0..60 {
        classic
            .step(classic_input(&[Key::Enter], steering))
            .unwrap();
    }
    assert!(classic.speed() > 0.0, "{:?}", classic.velocity);
    assert!(classic.yaw.abs() > 0.01, "yaw {}", classic.yaw);
    assert!(classic.pitch.abs() > 0.01, "pitch {}", classic.pitch);

    // Modern: the same keys and mouse movement do nothing: Enter and the right
    // button are Classic's, and the mouse never steers in Modern.
    let mut modern = Flight::free_flight();
    for _ in 0..60 {
        modern.step(modern_input(&[Key::Enter], steering)).unwrap();
    }
    assert_eq!(modern.speed(), 0.0);
    assert_eq!(modern.yaw, 0.0);
    assert_eq!(modern.pitch, 0.0);

    // Modern's own keys still fly: W raises the thrust and the arrows steer.
    let mut modern = Flight::free_flight();
    for _ in 0..60 {
        modern
            .step(modern_input(&[Key::W, Key::Right], MouseSteer::default()))
            .unwrap();
    }
    assert!(modern.speed() > 0.0);
    assert!(modern.yaw > 0.0);

    // Classic does not respond to Modern's keys.
    let mut classic = Flight::free_flight();
    for _ in 0..60 {
        classic
            .step(classic_input(&[Key::W, Key::Right], MouseSteer::default()))
            .unwrap();
    }
    assert_eq!(classic.speed(), 0.0);
    assert_eq!(classic.yaw, 0.0);
}

/// A one-second, 60-frame capture of the Classic input above. Set
/// `FFE_RS_CONTROLS_CAPTURE` to a directory to write the PPM strip and
/// `input.txt` there; the test itself always checks the simulated state.
#[test]
fn classic_input_strip_capture() {
    let Ok(directory) = std::env::var("FFE_RS_CONTROLS_CAPTURE") else {
        return;
    };
    let directory = Path::new(&directory);
    fs::create_dir_all(directory).unwrap();
    let mut screen = screen();
    let mut state = Flight::free_flight();
    let steering = MouseSteer {
        held: true,
        dx: 20,
        dy: -10,
    };
    let mut log = String::new();
    log.push_str("frame,thrust,yaw,pitch,speed\n");
    for frame in 0..=60 {
        if frame > 0 {
            state.step(classic_input(&[Key::Enter], steering)).unwrap();
        }
        if frame % 15 == 0 {
            let input = classic_input(&[Key::Enter], steering);
            log.push_str(&format!(
                "{frame},{:.3},{:.3},{:.3},{:.3}\n",
                input.thrust,
                input.yaw,
                input.pitch,
                state.speed()
            ));
            screen.draw(
                &state,
                ViewState::default(),
                None,
                false,
                &format!(
                    "CLASSIC frame {frame}: Enter + RMB  thrust {:.1}  yaw {:.2}  pitch {:.2}",
                    input.thrust, state.yaw, state.pitch
                ),
            );
            let mut ppm = format!("P6\n{} {}\n255\n", 320, 200).into_bytes();
            for pixel in screen.logical_pixels() {
                ppm.extend([(*pixel >> 16) as u8, (*pixel >> 8) as u8, *pixel as u8]);
            }
            fs::write(directory.join(format!("classic-{frame:02}.ppm")), ppm).unwrap();
        }
    }
    fs::write(directory.join("input.txt"), log).unwrap();
    // The strip's last frame must show movement, not a stalled press.
    assert!(state.speed() > 0.0);
    assert!(state.yaw.abs() > 0.0);
    assert!(state.pitch.abs() > 0.0);
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
