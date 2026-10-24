use ffe_rs::camera::{key_amount, ExternalKeys, ViewMode, ViewState};
use ffe_rs::console::{acceleration_for_digit, Click, ConsoleState, ACCELERATION_NAMES};
use ffe_rs::flight::{Controls, Flight, STEP};
use ffe_rs::input;
use ffe_rs::model_lab::ModelLab;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{describe, distance, Kind as NavKind, Navigator, View as NavView};
use ffe_rs::original_flight::{Keys as OriginalKeys, OriginalFlight};
use ffe_rs::saves;
use ffe_rs::screen::ViewButton;
use ffe_rs::screen::{Screen, HEIGHT, WIDTH};
use ffe_rs::ships::ShipList;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use minifb::{MouseButton, MouseMode};
use std::collections::HashMap;
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
        "Saved {name}. Shift+F5 reloads it; prior snapshots kept."
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
    let mut views = ViewState::default();
    // The original flight model (key O); None runs the synthetic prototype flight.
    let mut original: Option<OriginalFlight> = None;
    // When each external-view or turret key went down, for the original's
    // held-key ramp.
    let mut held_since: HashMap<Key, Instant> = HashMap::new();
    let mut mouse_was_down = false;
    // The console's time acceleration step (0 = 1x .. 4 = 10000x).
    let mut acceleration = 0usize;
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
    // The original's named craft (cache v11); Page Up/Down changes the player's ship.
    let ships = scene
        .as_ref()
        .map(|scene| ShipList::from_cache(&cache, scene))
        .unwrap_or_default();
    // The systems, their bodies and the autopilot (Tab, G, Backspace).
    let mut nav = match Navigator::new(&cache) {
        Ok(nav) => Some(nav),
        Err(error) => {
            eprintln!("Navigation unavailable: {error}");
            None
        }
    };
    let mut paused = false;
    let mut message = String::from("Welcome. H closes help; F2 selects a playtest.");
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
                && model_viewer.is_none()
                && nav.as_ref().is_none_or(|nav| nav.view == NavView::Closed),
        );
        let pressed = |key| window.is_key_pressed(key, KeyRepeat::No);
        // A console click on the mouse-down edge, in flight (client pixels against
        // the client size: get_mouse_pos divides by the creation scale only, which
        // breaks after a resize).
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let console_click = if active
            && mouse_down
            && !mouse_was_down
            && state.mode == ffe_rs::flight::Mode::Flight
            && !help
            && !scenarios
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| Screen::logical_point(x, y, window.get_size()))
                .and_then(|(x, y)| Screen::console_click(x, y))
        } else {
            None
        };
        mouse_was_down = mouse_down;
        // The acceleration meter and the screen buttons: the 6 7 8 9 0 keys give the
        // meter's steps as the original's FUNC_000208 maps them.
        let digit_step = [
            (Key::Key6, '6'),
            (Key::Key7, '7'),
            (Key::Key8, '8'),
            (Key::Key9, '9'),
            (Key::Key0, '0'),
        ]
        .into_iter()
        .find(|(key, _)| pressed(*key))
        .and_then(|(_, digit)| acceleration_for_digit(digit))
        .filter(|_| active && !help && !scenarios && state.mode == ffe_rs::flight::Mode::Flight);
        let chosen_step = match console_click {
            Some(Click::Acceleration(step)) => Some(step),
            _ => digit_step,
        };
        if let Some(step) = chosen_step {
            acceleration = step;
            if let Some(model) = original.as_mut() {
                model.set_time_acceleration(step);
                message = format!("Time acceleration {}.", ACCELERATION_NAMES[step]);
            } else {
                message = format!(
                    "Time acceleration {} (applies to the original flight model: O).",
                    ACCELERATION_NAMES[step]
                );
            }
        }
        match console_click {
            Some(Click::Pause) => {
                paused = !paused;
                held_controls.disarm(&keys_down);
                message = if paused { "Paused." } else { "Resumed." }.to_owned();
            }
            Some(click) => {
                if let Some(text) = click.unported() {
                    message = text.to_owned();
                }
            }
            None => {}
        }
        if active {
            // F1 is the original's view key; help moves to H.
            if pressed(Key::H) {
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
            // F6 is the original's front view; the model viewer moves to M.
            if pressed(Key::M) {
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
            // Navigation: Tab the system map, G the galaxy, Up/Down choose, Enter
            // flies to the body (or jumps to the system), Backspace stops the autopilot.
            if let Some(nav) = nav.as_mut().filter(|_| {
                state.mode == ffe_rs::flight::Mode::Flight
                    && !help
                    && !scenarios
                    && model_lab.is_none()
                    && model_viewer.is_none()
            }) {
                if pressed(Key::Tab) {
                    nav.open(NavView::System);
                    held_controls.disarm(&keys_down);
                }
                if pressed(Key::G) {
                    nav.open(NavView::Galaxy);
                    held_controls.disarm(&keys_down);
                }
                if nav.view != NavView::Closed {
                    if pressed(Key::Up) {
                        nav.step_selection(-1);
                    }
                    if pressed(Key::Down) {
                        nav.step_selection(1);
                    }
                    if pressed(Key::Enter) {
                        held_controls.disarm(&keys_down);
                        if original.is_none() {
                            message =
                                "Navigation needs the original flight model: press O.".to_owned();
                        } else if nav.view == NavView::Galaxy {
                            let index = nav.selected;
                            match nav.jump(&cache, index) {
                                Ok(position) => {
                                    if let Some(model) = original.as_mut() {
                                        model.teleport(position);
                                    }
                                    message =
                                        format!("Arrived in the {} system.", nav.system_name());
                                    nav.view = NavView::Closed;
                                }
                                Err(error) => message = format!("Jump failed: {error}"),
                            }
                        } else if let Some(body) = nav.engage() {
                            message = format!("Autopilot to {}.", body.name);
                        }
                    }
                }
                if pressed(Key::Backspace) && nav.target.take().is_some() {
                    message = "Autopilot off.".to_owned();
                }
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
                // Engines: off / half / full thrust (global 15) for the glows.
                if pressed(Key::E) {
                    viewer.cycle_thrust();
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
                    message = "Scenario reset. Saves kept. H for controls.".to_owned();
                }
            } else if !help && !paused {
                // The original's view keys (FUNC_001001/FUNC_001056): F1 cycles,
                // F6 front, F7 rear, F9 external; the console icons do the same.
                // The ship's gun mounts (cache v12): more than two offer the turrets.
                let gun_mounts = scene
                    .as_ref()
                    .and_then(|scene| scene.ship_stats(state.ship))
                    .map_or(0, |stats| stats.gun_mounts);
                if pressed(Key::O) && state.mode == ffe_rs::flight::Mode::Flight {
                    if original.take().is_some() {
                        state.axes = None;
                        message = "Prototype flight model.".to_owned();
                    } else {
                        let stats = scene
                            .as_ref()
                            .and_then(|scene| scene.ship_stats(state.ship));
                        original = Some(
                            OriginalFlight::from_flight(&state, stats.as_ref())
                                .with_time_acceleration(acceleration),
                        );
                        if let Some(model) = original.as_mut() {
                            model.set_time_acceleration(acceleration);
                        }
                        message =
                            "Original flight: Enter/Shift set speed; Z/A pitch, ,/. yaw, [ ] roll."
                                .to_owned();
                    }
                }
                if state.mode == ffe_rs::flight::Mode::Flight {
                    let clicked = console_click.and_then(|click| match click {
                        Click::CycleView => Some(ViewButton::Cycle),
                        Click::Front => Some(ViewButton::Select(ViewMode::Front)),
                        Click::Rear => Some(ViewButton::Select(ViewMode::Rear)),
                        Click::External => Some(ViewButton::Select(ViewMode::External)),
                        Click::Turret => Some(ViewButton::Turret),
                        _ => None,
                    });
                    let turret = ViewMode::turret(gun_mounts, views.turret.pitch);
                    let chosen = if pressed(Key::F1) || clicked == Some(ViewButton::Cycle) {
                        Some(views.mode.next(gun_mounts, views.turret.pitch))
                    } else if pressed(Key::F6) {
                        Some(ViewMode::Front)
                    } else if pressed(Key::F7) {
                        Some(ViewMode::Rear)
                    } else if pressed(Key::F8) || clicked == Some(ViewButton::Turret) {
                        turret
                    } else if pressed(Key::F9) {
                        Some(ViewMode::External)
                    } else if let Some(ViewButton::Select(mode)) = clicked {
                        Some(mode)
                    } else {
                        None
                    };
                    if let Some(next) = chosen {
                        views.mode = next;
                        message = format!("{}.", views.mode.name());
                    }
                }
                let now = Instant::now();
                let mut held = |keys: &[Key]| {
                    let down = keys.iter().find(|key| window.is_key_down(**key)).copied();
                    for key in keys {
                        if Some(*key) != down {
                            held_since.remove(key);
                        }
                    }
                    down.map(|key| {
                        let since = *held_since.entry(key).or_insert(now);
                        // 50 Hz ticks held (DATA_KeyAccumTable).
                        (now.duration_since(since).as_secs_f64() * 50.0) as u32
                    })
                };
                let elapsed_ms = (elapsed * 1000.0) as u32;
                if views.mode.is_turret() && original.is_none() {
                    // FUNC_001038: A/Z and comma/period aim the turret (the arrows
                    // too, in either control scheme); the ship holds its course.
                    let amount = |held| key_amount(elapsed_ms, held) as i16;
                    let pitch = amount(held(&[Key::A, Key::Up]))
                        .wrapping_sub(amount(held(&[Key::Z, Key::Down])));
                    let yaw = amount(held(&[Key::Comma, Key::Left]))
                        .wrapping_sub(amount(held(&[Key::Period, Key::Right])));
                    views.mode = views.turret.step(views.mode, pitch, yaw, gun_mounts);
                }
                if views.mode == ViewMode::External {
                    let keys = ExternalKeys {
                        left: held(&[Key::Left]),
                        right: held(&[Key::Right]),
                        up: held(&[Key::Up]),
                        down: held(&[Key::Down]),
                        zoom_out: held(&[Key::NumPadPlus, Key::Equal]),
                        zoom_in: held(&[Key::NumPadMinus, Key::Minus]),
                    };
                    let minimum = scene
                        .as_ref()
                        .and_then(|scene| scene.camera_minimum(state.ship))
                        .unwrap_or(0);
                    views.external.step(keys, elapsed_ms, minimum);
                }
                let ship_step = i32::from(pressed(Key::PageDown)) - i32::from(pressed(Key::PageUp));
                if ship_step != 0 {
                    message = match ships.step(state.ship, ship_step) {
                        Some(ship) => {
                            state.ship = ship;
                            // FUNC_001110 re-applies the closest-camera limit.
                            let minimum = scene
                                .as_ref()
                                .and_then(|scene| scene.camera_minimum(ship))
                                .unwrap_or(0);
                            views.external.distance =
                                views.external.distance.min(minimum.saturating_neg());
                            // ffe-rs: a ship without turrets leaves a turret view.
                            let mounts = scene
                                .as_ref()
                                .and_then(|scene| scene.ship_stats(ship))
                                .map_or(0, |stats| stats.gun_mounts);
                            if views.mode.is_turret() && mounts <= 2 {
                                views.mode = ViewMode::Front;
                            }
                            if original.is_some() {
                                let stats = scene.as_ref().and_then(|scene| scene.ship_stats(ship));
                                original = Some(
                                    OriginalFlight::from_flight(&state, stats.as_ref())
                                        .with_time_acceleration(acceleration),
                                );
                            }
                            format!(
                                "Ship: {} (model {ship}).",
                                ships.name(ship).unwrap_or("unnamed")
                            )
                        }
                        None => "No ship list: rebuild data.bin (cache v11).".to_owned(),
                    };
                }
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
                let shift =
                    window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
                if pressed(Key::F5) && !shift {
                    message = match snapshot(&save_dir, &state) {
                        Ok(text) => text,
                        Err(e) => {
                            eprintln!("SAVE ERROR: {e}");
                            format!("SAVE ERROR: {e}")
                        }
                    };
                }
                // Load moved from F9 (the original's external view) to Shift+F5.
                if pressed(Key::F5) && shift {
                    let loaded = snapshots(&save_dir).and_then(|paths| {
                        paths
                            .last()
                            .ok_or("No prototype snapshots. F5 saves; Shift+F5 loads.".to_owned())
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
            // In the external view the arrows move the camera, so Modern steering
            // falls back to the Classic-like keys there.
            let external_view = views.mode == ViewMode::External;
            let input = if views.mode.is_turret() {
                // The steering keys aim the turret (FUNC_001038).
                Controls {
                    thrust: if modern {
                        axis(Key::W, Key::S)
                    } else {
                        axis(Key::Enter, Key::LeftShift)
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    brake: held_controls.contains(&Key::X),
                }
            } else if modern && !external_view {
                Controls {
                    thrust: axis(Key::W, Key::S),
                    yaw: axis(Key::Right, Key::Left),
                    pitch: axis(Key::Up, Key::Down),
                    brake: held_controls.contains(&Key::X),
                }
            } else if modern {
                Controls {
                    thrust: axis(Key::W, Key::S),
                    yaw: axis(Key::Period, Key::Comma),
                    pitch: axis(Key::A, Key::Z),
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
            if let Some(model) = original.as_mut() {
                let down = |keys: &[Key]| keys.iter().any(|key| held_controls.contains(key));
                let mut keys = OriginalKeys {
                    faster: down(&[Key::Enter]),
                    slower: down(&[Key::LeftShift, Key::RightShift]),
                    pitch_down: down(&[Key::Z]),
                    pitch_up: down(&[Key::A]),
                    left: down(&[Key::Comma]),
                    right: down(&[Key::Period]),
                    roll_left: down(&[Key::LeftBracket]),
                    roll_right: down(&[Key::RightBracket]),
                };
                if let Some(nav) = nav.as_mut() {
                    if let Some((command, body)) = nav.command(&cache, model) {
                        keys = command.keys;
                        if let Some((landing, seconds)) = command.skip {
                            model.teleport(landing);
                            nav.transfer(&cache, seconds);
                            message = format!("Transfer to {}: {:.0} s.", body.name, seconds);
                        } else if command.arrived {
                            message = format!("Arrived at {}.", body.name);
                            nav.target = None;
                        } else {
                            let range = distance(model.player().motion.position, body.position);
                            message = format!("Autopilot: {} {}.", body.name, describe(range));
                        }
                    }
                    let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
                        / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
                    nav.advance(&cache, elapsed * factor);
                    if let Some(moved) = nav.follow(model.player().motion.position) {
                        model.shift(moved);
                    }
                }
                let view = match views.mode {
                    ViewMode::TopTurret => 2,
                    ViewMode::BottomTurret => 3,
                    _ => 4,
                };
                model.set_view(view, [views.turret.yaw, views.turret.pitch]);
                match model.step(elapsed, &keys) {
                    Ok(()) => {
                        model.apply(&mut state);
                        let (view, turret) = model.view();
                        if views.mode.is_turret() {
                            views.mode = if view == 3 {
                                ViewMode::BottomTurret
                            } else {
                                ViewMode::TopTurret
                            };
                            views.turret.yaw = turret[0];
                            views.turret.pitch = turret[1];
                        }
                        if keys.faster || keys.slower {
                            message = format!(
                                "Set speed {:.0} m/s, speed {:.0} m/s.",
                                model.set_speed_metres(),
                                state.speed() * 9.7
                            );
                        }
                        if let Some(text) = state.finish_step() {
                            message = text.to_owned();
                            original = (state.mode == ffe_rs::flight::Mode::Flight).then(|| {
                                let stats = scene
                                    .as_ref()
                                    .and_then(|scene| scene.ship_stats(state.ship));
                                OriginalFlight::from_flight(&state, stats.as_ref())
                                    .with_time_acceleration(acceleration)
                            });
                        }
                    }
                    Err(error) => {
                        eprintln!("ORIGINAL FLIGHT ERROR: {error}");
                        message = format!("ORIGINAL FLIGHT ERROR: {error}");
                        original = None;
                        state.axes = None;
                    }
                }
                accumulator = 0.0;
            } else {
                accumulator += elapsed;
                while accumulator >= STEP {
                    if let Some(text) = state.step(input)? {
                        message = text.to_owned();
                    }
                    accumulator -= STEP;
                }
            }
        } else {
            accumulator = 0.0;
        }
        // FUNC_001110's closest-camera limit for the current ship (applied when the
        // original sets a ship; doing it every frame is equivalent).
        if let Some(minimum) = scene
            .as_ref()
            .and_then(|scene| scene.camera_minimum(state.ship))
        {
            views.external.distance = views.external.distance.min(minimum.saturating_neg());
        }
        // The original returns to the front view when docking (FUNC_001048).
        if state.mode == ffe_rs::flight::Mode::Docked {
            views.mode = ViewMode::Front;
        }
        // A new state (docking, a scenario, a load) ends the original flight model.
        if original.is_some()
            && (state.mode != ffe_rs::flight::Mode::Flight || state.axes.is_none())
        {
            original = None;
            state.axes = None;
        }
        views.console = ConsoleState {
            acceleration,
            paused,
            speed: (state.speed() * 9.7).round() as i32,
            set_speed: original
                .as_ref()
                .map(|model| model.set_speed_metres().round() as i32),
        };
        screen.discs = nav.as_ref().map_or_else(Vec::new, |nav| {
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            nav.bodies
                .iter()
                .filter(|b| matches!(b.kind, NavKind::Star | NavKind::Planet | NavKind::Moon))
                .map(|b| {
                    let length = b.position.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
                    let light = b.position.map(|v| -(v as f64) / length.sqrt().max(1.0));
                    ffe_rs::screen::Disc {
                        centre: b.position.map(|v| v as f64 / unit),
                        radius: b.radius as f64 / unit,
                        light,
                        colours: match b.kind {
                            NavKind::Star => (17, 17),
                            NavKind::Planet => (12, 4),
                            _ => (10, 3),
                        },
                        star: b.kind == NavKind::Star,
                        model: (b.kind != NavKind::Star).then_some((b.model, b.seed)),
                        orientation: b.orientation,
                    }
                })
                .collect()
        });
        screen.draw(&state, views, scene.as_ref(), help, modern, &message);
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
                    "P RESUMES. H HELP. F2 SCENARIOS.",
                    "FOCUS LOSS PAUSES WITHOUT CATCH-UP.",
                ],
            );
        }
        if let Some(nav) = nav.as_ref().filter(|nav| nav.view != NavView::Closed) {
            let ship = original
                .as_ref()
                .map_or([0; 3], |model| model.player().motion.position);
            let title = match nav.view {
                NavView::Galaxy => "GALAXY: ENTER JUMPS".to_owned(),
                _ => format!(
                    "SYSTEM MAP: {}  ENTER FLIES TO",
                    nav.system_name().to_uppercase()
                ),
            };
            screen.draw_map(
                &title,
                &nav.lines(ship),
                nav.selected,
                &nav.points(ship),
                "TAB SYSTEM  G GALAXY  UP/DOWN  ENTER  BKSP AUTOPILOT OFF",
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
