//! Recording and replaying a play session's input.
//!
//! Every frame the game takes one [`Frame`]: the frame time, the window size, whether
//! the window is active, the keys down and pressed, and the mouse. `--record FILE`
//! writes each frame as a line of text; `--replay FILE` feeds the recorded frames back
//! in place of the live input (and their frame times in place of the clock), so the
//! same keys, clicks and timings reach the game again.
//!
//! The file is plain text, one frame a line:
//! `dt|width,height|active|down|pressed|repeated|mouse|buttons`, with keys by name
//! (`Enter`, `A`, `LeftShift`), the mouse as `x,y` (or `-`) and the buttons as `L`/`R`.

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

const HEADER: &str = "ffe-rs input v1";

#[rustfmt::skip]
const ALL_KEYS: &[Key] = &[
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Key6, Key::Key7,
    Key::Key8, Key::Key9, Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S,
    Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z, Key::F1, Key::F2, Key::F3,
    Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
    Key::F13, Key::F14, Key::F15, Key::Down, Key::Left, Key::Right, Key::Up, Key::Apostrophe,
    Key::Backquote, Key::Backslash, Key::Comma, Key::Equal, Key::LeftBracket, Key::Minus,
    Key::Period, Key::RightBracket, Key::Semicolon, Key::Slash, Key::Backspace, Key::Delete,
    Key::End, Key::Enter, Key::Escape, Key::Home, Key::Insert, Key::Menu, Key::PageDown,
    Key::PageUp, Key::Pause, Key::Space, Key::Tab, Key::NumLock, Key::CapsLock,
    Key::ScrollLock, Key::LeftShift, Key::RightShift, Key::LeftCtrl, Key::RightCtrl,
    Key::NumPad0, Key::NumPad1, Key::NumPad2, Key::NumPad3, Key::NumPad4, Key::NumPad5,
    Key::NumPad6, Key::NumPad7, Key::NumPad8, Key::NumPad9, Key::NumPadDot,
    Key::NumPadSlash, Key::NumPadAsterisk, Key::NumPadMinus, Key::NumPadPlus,
    Key::NumPadEnter, Key::LeftAlt, Key::RightAlt, Key::LeftSuper, Key::RightSuper,
];

fn key_name(key: Key) -> String {
    format!("{key:?}")
}

fn key_from(name: &str) -> Option<Key> {
    ALL_KEYS.iter().copied().find(|key| key_name(*key) == name)
}

/// One frame's input, in place of the window's own queries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    /// The frame time in seconds (already limited to a tenth of a second).
    pub dt: f64,
    pub size: (usize, usize),
    pub active: bool,
    pub down: Vec<Key>,
    pub pressed: Vec<Key>,
    /// Pressed with the OS key repeat (the model viewer's paging keys).
    pub repeated: Vec<Key>,
    pub mouse: Option<(f32, f32)>,
    pub left: bool,
    pub right: bool,
}

impl Frame {
    /// The live window's input.
    pub fn capture(window: &mut Window, dt: f64) -> Self {
        Frame {
            dt,
            size: window.get_size(),
            active: window.is_active(),
            down: window.get_keys(),
            pressed: window.get_keys_pressed(KeyRepeat::No),
            repeated: window.get_keys_pressed(KeyRepeat::Yes),
            mouse: window.get_unscaled_mouse_pos(MouseMode::Pass),
            left: window.get_mouse_down(MouseButton::Left),
            right: window.get_mouse_down(MouseButton::Right),
        }
    }

    pub fn is_key_down(&self, key: Key) -> bool {
        self.down.contains(&key)
    }

    pub fn is_key_pressed(&self, key: Key, repeat: KeyRepeat) -> bool {
        match repeat {
            KeyRepeat::No => self.pressed.contains(&key),
            KeyRepeat::Yes => self.repeated.contains(&key),
        }
    }

    pub fn get_keys(&self) -> Vec<Key> {
        self.down.clone()
    }

    pub fn get_keys_pressed(&self, repeat: KeyRepeat) -> Vec<Key> {
        match repeat {
            KeyRepeat::No => self.pressed.clone(),
            KeyRepeat::Yes => self.repeated.clone(),
        }
    }

    pub fn get_mouse_down(&self, button: MouseButton) -> bool {
        match button {
            MouseButton::Left => self.left,
            MouseButton::Right => self.right,
            MouseButton::Middle => false,
        }
    }

    pub fn get_unscaled_mouse_pos(&self, _mode: MouseMode) -> Option<(f32, f32)> {
        self.mouse
    }

    pub fn get_size(&self) -> (usize, usize) {
        self.size
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn line(&self) -> String {
        let keys = |list: &[Key]| {
            if list.is_empty() {
                "-".to_owned()
            } else {
                list.iter()
                    .map(|key| key_name(*key))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        };
        let mouse = self
            .mouse
            .map_or_else(|| "-".to_owned(), |(x, y)| format!("{x},{y}"));
        let buttons = format!(
            "{}{}",
            if self.left { "L" } else { "" },
            if self.right { "R" } else { "" }
        );
        format!(
            "{:.6}|{},{}|{}|{}|{}|{}|{}|{}",
            self.dt,
            self.size.0,
            self.size.1,
            u8::from(self.active),
            keys(&self.down),
            keys(&self.pressed),
            keys(&self.repeated),
            mouse,
            if buttons.is_empty() { "-" } else { &buttons }
        )
    }

    fn parse(line: &str) -> Result<Self, String> {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 8 {
            return Err(format!("expected 8 fields, found {}", parts.len()));
        }
        let keys = |text: &str| -> Result<Vec<Key>, String> {
            if text == "-" {
                return Ok(Vec::new());
            }
            text.split(',')
                .map(|name| key_from(name).ok_or_else(|| format!("unknown key {name}")))
                .collect()
        };
        let pair = |text: &str| -> Result<(f64, f64), String> {
            let (a, b) = text.split_once(',').ok_or("expected x,y")?;
            Ok((
                a.parse().map_err(|_| format!("bad number {a}"))?,
                b.parse().map_err(|_| format!("bad number {b}"))?,
            ))
        };
        let size = pair(parts[1])?;
        Ok(Frame {
            dt: parts[0].parse().map_err(|_| "bad frame time".to_owned())?,
            size: (size.0 as usize, size.1 as usize),
            active: parts[2] == "1",
            down: keys(parts[3])?,
            pressed: keys(parts[4])?,
            repeated: keys(parts[5])?,
            mouse: if parts[6] == "-" {
                None
            } else {
                let (x, y) = pair(parts[6])?;
                Some((x as f32, y as f32))
            },
            left: parts[7].contains('L'),
            right: parts[7].contains('R'),
        })
    }
}

/// Where the frames come from and go to.
pub struct Session {
    record: Option<BufWriter<File>>,
    replay: Option<(Vec<Frame>, usize)>,
    /// Seconds of game time so far (the sum of the frame times).
    pub total: f64,
    start: std::time::Instant,
    exit_when_done: bool,
    written: u32,
}

impl Session {
    pub fn new(
        record: Option<&Path>,
        replay: Option<&Path>,
        arguments: &str,
        exit_when_done: bool,
    ) -> Result<Self, String> {
        let record = match record {
            Some(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
                }
                let mut file = BufWriter::new(
                    File::create(path)
                        .map_err(|e| format!("cannot create {}: {e}", path.display()))?,
                );
                writeln!(file, "{HEADER}").map_err(|e| e.to_string())?;
                writeln!(file, "# args: {arguments}").map_err(|e| e.to_string())?;
                Some(file)
            }
            None => None,
        };
        let replay = match replay {
            Some(path) => {
                let file =
                    File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
                let mut frames = Vec::new();
                for (number, line) in BufReader::new(file).lines().enumerate() {
                    let line = line.map_err(|e| e.to_string())?;
                    if number == 0 {
                        if line != HEADER {
                            return Err(format!("{} is not an ffe-rs input file", path.display()));
                        }
                        continue;
                    }
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    frames.push(
                        Frame::parse(&line)
                            .map_err(|e| format!("{}:{}: {e}", path.display(), number + 1))?,
                    );
                }
                Some((frames, 0))
            }
            None => None,
        };
        Ok(Session {
            record,
            replay,
            total: 0.0,
            start: std::time::Instant::now(),
            exit_when_done,
            written: 0,
        })
    }

    /// `--replay-exit`: the recording has ended and the window should close.
    pub fn should_exit(&self) -> bool {
        self.exit_when_done && self.finished()
    }

    pub fn replaying(&self) -> bool {
        self.replay.is_some()
    }

    /// Every recorded frame has been played.
    pub fn finished(&self) -> bool {
        self.replay
            .as_ref()
            .is_some_and(|(frames, next)| *next >= frames.len())
    }

    /// This frame's input: the next recorded frame when replaying, else the window's.
    pub fn frame(&mut self, window: &mut Window, dt: f64) -> Frame {
        let frame = match self.replay.as_mut() {
            Some((frames, next)) => match frames.get(*next) {
                Some(frame) => {
                    *next += 1;
                    frame.clone()
                }
                // Past the end: no input, the live frame time.
                None => Frame {
                    dt,
                    size: window.get_size(),
                    active: true,
                    ..Frame::default()
                },
            },
            None => Frame::capture(window, dt),
        };
        if let Some(file) = self.record.as_mut() {
            let _ = writeln!(file, "{}", frame.line());
            // A crash must not lose the last moments: flush every half second or so.
            self.written += 1;
            if self.written % 30 == 0 {
                let _ = file.flush();
            }
        }
        self.total += frame.dt;
        frame
    }

    /// The game clock as an `Instant`: the start plus the frame times so far, so a held
    /// key lasts the same in a replay as when it was played.
    pub fn now(&self) -> std::time::Instant {
        self.start + std::time::Duration::from_secs_f64(self.total)
    }

    pub fn flush(&mut self) {
        if let Some(file) = self.record.as_mut() {
            let _ = file.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_through_their_text() {
        let frame = Frame {
            dt: 0.0166,
            size: (1280, 960),
            active: true,
            down: vec![Key::W, Key::LeftShift],
            pressed: vec![Key::W],
            repeated: vec![],
            mouse: Some((512.5, 300.0)),
            left: true,
            right: false,
        };
        let again = Frame::parse(&frame.line()).unwrap();
        assert_eq!(again.down, frame.down);
        assert_eq!(again.pressed, frame.pressed);
        assert_eq!(again.size, frame.size);
        assert_eq!(again.mouse, frame.mouse);
        assert!(again.left && !again.right && again.active);
        assert!((again.dt - frame.dt).abs() < 1e-6);
        let idle = Frame::default();
        assert_eq!(Frame::parse(&idle.line()).unwrap().down, Vec::<Key>::new());
    }

    #[test]
    fn every_key_name_parses_back() {
        for key in ALL_KEYS {
            assert_eq!(key_from(&key_name(*key)), Some(*key));
        }
    }
}
