use ffe_rs::flight::{Controls, Flight, STEP};
use ffe_rs::input;
use ffe_rs::model_lab::ModelLab;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::saves;
use ffe_rs::screen::{Screen, HEIGHT, WIDTH};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Options {
    root: PathBuf,
    smoke_frames: Option<usize>,
}

fn options() -> Result<Options, String> {
    let mut root = None;
    let mut smoke_frames = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(
                args.next().ok_or("--root requires a directory")?,
            ));
        } else if arg == "--smoke-frames" && smoke_frames.is_none() {
            let frames = args
                .next()
                .ok_or("--smoke-frames requires 1..300")?
                .to_string_lossy()
                .parse::<usize>()
                .map_err(|_| "invalid smoke frame count")?;
            if !(1..=300).contains(&frames) {
                return Err("smoke frames must be in 1..300".to_owned());
            }
            smoke_frames = Some(frames);
        } else {
            return Err(format!("Unknown argument {}. Usage: ffe-rs --root <repo-with-data.bin> [--smoke-frames 1..300]",arg.to_string_lossy()));
        }
    }
    let root = match root {
        Some(root) => root,
        // Beside the executable, else the nearest of up to four parent folders with
        // a data.bin (so target\release\ffe-rs.exe finds the checkout's cache).
        None => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let beside = exe.parent().ok_or("executable has no parent")?.to_owned();
            beside
                .ancestors()
                .take(5)
                .find(|dir| dir.join("data.bin").is_file())
                .map_or(beside.clone(), Path::to_path_buf)
        }
    };
    Ok(Options {
        root: root
            .canonicalize()
            .map_err(|e| format!("Cannot resolve root: {e}"))?,
        smoke_frames,
    })
}

fn read_cache(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("{}: {e}. Run ffe-tools build-data first.", path.display()))?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.len() > ffe_content::MAX_CACHE_BYTES as u64
    {
        return Err("Cache must be a bounded regular file.".to_owned());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| {
            f.take(ffe_content::MAX_CACHE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| format!("Cannot read cache: {e}"))?;
    Ok(bytes)
}

fn snapshots(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if let Some(number) = text
            .strip_prefix("prototype-")
            .and_then(|s| s.strip_suffix(".json"))
        {
            if number.len() == 8 && number.bytes().all(|b| b.is_ascii_digit()) {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn snapshot(directory: &Path, state: &Flight) -> Result<String, String> {
    let existing = snapshots(directory)?;
    let next = match existing.last() {
        Some(path) => {
            path.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_prefix("prototype-"))
                .ok_or("Invalid snapshot name")?
                .parse::<u32>()
                .map_err(|e| e.to_string())?
                + 1
        }
        None => 1,
    };
    if next > 99_999_999 {
        return Err("Snapshot number limit reached.".to_owned());
    }
    let name = format!("prototype-{next:08}.json");
    saves::save_new(&directory.join(&name), state)?;
    Ok(format!(
        "Saved {name}. F9 reloads it; prior snapshots kept."
    ))
}

fn run() -> Result<(), String> {
    let options = options()?;
    let cache = ffe_content::read(&read_cache(&options.root.join("data.bin"))?)?;
    let mut screen = Screen::new(&cache)?;
    let save_dir = options.root.join("saves");
    if save_dir.exists() {
        let meta = fs::symlink_metadata(&save_dir).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("saves must be a real directory, not a link.".to_owned());
        }
    } else {
        fs::create_dir(&save_dir).map_err(|e| format!("Cannot create saves directory: {e}"))?;
    }
    let mut window = Window::new(
        "FFE-RS 0.9 - Depth-order helpers, experimental flight",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: true,
            scale: minifb::Scale::X2,
            scale_mode: minifb::ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| format!("Cannot open game window: {e}"))?;
    window.set_target_fps(60);
    let mut state = Flight::default();
    let mut modern = true;
    let mut help = true;
    let mut scenarios = false;
    let mut model_lab = None;
    let mut model_viewer: Option<ModelViewer> = None;
    // The flight view's station is the original model 74 when the cache has it;
    // otherwise the placeholder wireframe stays.
    let scene = match ModelViewer::new(&cache) {
        Ok(scene) => Some(scene),
        Err(error) => {
            eprintln!("Station model unavailable, using the placeholder: {error}");
            None
        }
    };
    let mut paused = false;
    let mut message = String::from("Welcome. F1 closes help; F2 selects a playtest.");
    let mut last = Instant::now();
    let mut accumulator = 0.0;
    let mut frames = 0;
    let mut held_controls = input::HeldControls::default();
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let elapsed = now.duration_since(last).as_secs_f64().min(0.1);
        last = now;
        let active = window.is_active();
        let keys_down = window.get_keys();
        held_controls.update(
            &keys_down,
            &window.get_keys_pressed(KeyRepeat::No),
            active
                && !help
                && !scenarios
                && !paused
                && model_lab.is_none()
                && model_viewer.is_none(),
        );
        let pressed = |key| window.is_key_pressed(key, KeyRepeat::No);
        if active {
            if pressed(Key::F1) {
                help = !help;
                scenarios = false;
                model_lab = None;
                model_viewer = None;
                held_controls.disarm(&keys_down);
            }
            if pressed(Key::F2) {
                scenarios = !scenarios;
                help = false;
                model_lab = None;
                model_viewer = None;
                held_controls.disarm(&keys_down);
            }
            if pressed(Key::F4) {
                held_controls.disarm(&keys_down);
                if model_lab.is_some() {
                    model_lab = None;
                } else {
                    help = false;
                    scenarios = false;
                    model_viewer = None;
                    match ModelLab::new(&cache) {
                        Ok(lab) => model_lab = Some(lab),
                        Err(error) => {
                            eprintln!("MODEL LAB ERROR: {error}");
                            message = format!("MODEL LAB ERROR: {error}");
                        }
                    }
                }
            }
            if pressed(Key::F6) {
                held_controls.disarm(&keys_down);
                if model_viewer.is_some() {
                    model_viewer = None;
                } else {
                    help = false;
                    scenarios = false;
                    model_lab = None;
                    match ModelViewer::new(&cache) {
                        Ok(viewer) => model_viewer = Some(viewer),
                        Err(error) => {
                            eprintln!("MODEL VIEWER ERROR: {error}");
                            message = format!("MODEL VIEWER ERROR: {error}");
                        }
                    }
                }
            }
            if pressed(Key::F3) {
                modern = !modern;
                held_controls.disarm(&keys_down);
            }
            if pressed(Key::P) && model_lab.is_none() && model_viewer.is_none() {
                paused = !paused;
                held_controls.disarm(&keys_down);
            }
            if let Some(viewer) = model_viewer.as_mut() {
                let repeated = |key| i32::from(window.is_key_pressed(key, KeyRepeat::Yes));
                viewer.step(repeated(Key::PageDown) - repeated(Key::PageUp));
                viewer.turn(
                    repeated(Key::Right) - repeated(Key::Left),
                    repeated(Key::Up) - repeated(Key::Down),
                );
                viewer.zoom(
                    repeated(Key::Equal) + repeated(Key::NumPadPlus)
                        - repeated(Key::Minus)
                        - repeated(Key::NumPadMinus),
                );
                if pressed(Key::Key0) {
                    viewer.set_animation(0);
                }
                if pressed(Key::Key5) {
                    viewer.set_animation(32768);
                }
                if pressed(Key::Key9) {
                    viewer.set_animation(65535);
                }
                // Animation: original inputs over the viewer's clock.
                if pressed(Key::C) {
                    viewer.use_clock();
                }
                if pressed(Key::G) {
                    viewer.toggle_door();
                }
                if pressed(Key::T) {
                    viewer.cycle_rate();
                }
                viewer.advance((elapsed * 1000.0).round() as u32);
            } else if let Some(lab) = model_lab.as_mut() {
                if pressed(Key::Tab) {
                    lab.cycle_view();
                }
                lab.step_face(i32::from(pressed(Key::Period)) - i32::from(pressed(Key::Comma)))?;
                if pressed(Key::Key1) {
                    lab.select(0)?;
                }
                if pressed(Key::Key2) {
                    lab.select(1)?;
                }
                if pressed(Key::Key0) {
                    lab.set_animation(0);
                }
                if pressed(Key::Key5) {
                    lab.set_animation(32768);
                }
                if pressed(Key::Key9) {
                    lab.set_animation(65535);
                }
                let repeated = |key| i32::from(window.is_key_pressed(key, KeyRepeat::Yes));
                lab.turn(
                    repeated(Key::Right) - repeated(Key::Left),
                    repeated(Key::Up) - repeated(Key::Down),
                )?;
            } else if scenarios {
                let selected = if pressed(Key::Key1) {
                    Some(Flight::default())
                } else if pressed(Key::Key2) {
                    Some(Flight::approach())
                } else if pressed(Key::Key3) {
                    Some(Flight::free_flight())
                } else {
                    None
                };
                if let Some(next) = selected {
                    state = next;
                    scenarios = false;
                    paused = false;
                    message = "Scenario reset. Saves kept. F1 for controls.".to_owned();
                }
            } else if !help && !paused {
                let action = if pressed(Key::L) {
                    Some(state.launch())
                } else if pressed(Key::D) {
                    Some(state.request_docking())
                } else if pressed(Key::B) {
                    Some(state.buy())
                } else if pressed(Key::N) {
                    Some(state.sell())
                } else {
                    None
                };
                if let Some(result) = action {
                    message = match result {
                        Ok(text) => text.to_owned(),
                        Err(error) => {
                            eprintln!("ERROR: {error}");
                            format!("ERROR: {error}")
                        }
                    };
                }
                if pressed(Key::F5) {
                    message = match snapshot(&save_dir, &state) {
                        Ok(text) => text,
                        Err(e) => {
                            eprintln!("SAVE ERROR: {e}");
                            format!("SAVE ERROR: {e}")
                        }
                    };
                }
                if pressed(Key::F9) {
                    let loaded = snapshots(&save_dir).and_then(|paths| {
                        paths
                            .last()
                            .ok_or("No prototype snapshots. F5 saves.".to_owned())
                            .and_then(|path| saves::load(path))
                    });
                    match loaded {
                        Ok(next) => {
                            state = next;
                            message = "Loaded most recent snapshot.".to_owned();
                        }
                        Err(e) => {
                            eprintln!("LOAD ERROR: {e}");
                            message = format!("LOAD ERROR: {e}")
                        }
                    }
                }
            }
        }
        if active && !help && !scenarios && !paused && model_lab.is_none() && model_viewer.is_none()
        {
            let axis = |positive, negative| {
                f64::from(u8::from(held_controls.contains(&positive)))
                    - f64::from(u8::from(held_controls.contains(&negative)))
            };
            let input = if modern {
                Controls {
                    thrust: axis(Key::W, Key::S),
                    yaw: axis(Key::Right, Key::Left),
                    pitch: axis(Key::Up, Key::Down),
                    brake: held_controls.contains(&Key::X),
                }
            } else {
                Controls {
                    thrust: axis(Key::Enter, Key::LeftShift),
                    yaw: axis(Key::Period, Key::Comma),
                    pitch: axis(Key::A, Key::Z),
                    brake: held_controls.contains(&Key::X),
                }
            };
            accumulator += elapsed;
            while accumulator >= STEP {
                if let Some(text) = state.step(input)? {
                    message = text.to_owned();
                }
                accumulator -= STEP;
            }
        } else {
            accumulator = 0.0;
        }
        screen.draw(&state, scene.as_ref(), help, modern, &message);
        if let Some(viewer) = &model_viewer {
            screen.draw_model_viewer(viewer);
        } else if let Some(lab) = &model_lab {
            screen.draw_model_lab(lab)?;
        } else if scenarios {
            screen.overlay(
                "PLAYTEST SCENARIOS - RESETS UNSAVED PROGRESS",
                &[
                    "1  DOCKED: TRADE / LAUNCH / SAVE",
                    "2  APPROACH: D REQUEST DOCK; COAST ~20 SEC",
                    "3  FREE FLIGHT: STATION 1500 UNITS AHEAD",
                    "F2 CANCELS. SAVES ARE NOT DELETED.",
                ],
            );
        } else if (paused || !active) && !help {
            screen.overlay(
                "PAUSED",
                &[
                    "P RESUMES. F1 HELP. F2 SCENARIOS.",
                    "FOCUS LOSS PAUSES WITHOUT CATCH-UP.",
                ],
            );
        }
        window
            .update_with_buffer(&screen.pixels, WIDTH, HEIGHT)
            .map_err(|e| format!("Window update: {e}"))?;
        frames += 1;
        if options.smoke_frames.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    if options.smoke_frames.is_some() {
        println!("PASS: native window presented {frames} frames using verified cabin artwork.");
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FFE-RS ERROR: {error}");
        // Started by double-click, the console closes on exit: keep the message
        // readable. Piped or scripted runs (no terminal on stdin) exit at once.
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!("Press Enter to close.");
            let _ = std::io::stdin().read_line(&mut String::new());
        }
        std::process::exit(2);
    }
}
