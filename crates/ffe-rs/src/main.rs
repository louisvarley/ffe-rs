use ffe_core::traffic::Reply;
use ffe_rs::camera::{key_amount, ExternalKeys, ViewMode, ViewState};
use ffe_rs::console::{acceleration_for_digit, Click, ConsoleState, ACCELERATION_NAMES};
use ffe_rs::docking::Contact;
use ffe_rs::flight::{Controls, Flight, STEP};
use ffe_rs::galaxy_map::{date_text, GalaxyMap};
use ffe_rs::input;
use ffe_rs::model_lab::ModelLab;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{
    describe, distance, refusal, Kind as NavKind, Navigator, Traffic, View as NavView,
};
use ffe_rs::original_flight::{Keys as OriginalKeys, OriginalFlight};
use ffe_rs::saves;
use ffe_rs::screen::ViewButton;
use ffe_rs::screen::{Screen, HEIGHT, WIDTH};
use ffe_rs::ships::ShipList;
use ffe_rs::trade;
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
    /// Directory for a scripted undock capture (starts the launch sequence and
    /// saves every tenth frame, then exits).
    capture: Option<PathBuf>,
}

fn options() -> Result<Options, String> {
    let mut root = None;
    let mut smoke_frames = None;
    let mut capture = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(
                args.next().ok_or("--root requires a directory")?,
            ));
        } else if arg == "--capture" && capture.is_none() {
            capture = Some(PathBuf::from(
                args.next().ok_or("--capture requires a directory")?,
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
        capture,
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
    let mut help = false;
    let mut scenarios = false;
    let mut model_lab = None;
    // The galaxy map's state and the tables it reads (cache v21 to v25).
    let mut galaxy: Option<GalaxyMap> = None;
    let mut map_drag: Option<(i32, i32)> = None;
    let sector_tables = cache
        .asset("base:world/galaxy-tables")
        .and_then(|bytes| ffe_core::sector::Tables::new(bytes).ok());
    let system_details = cache
        .asset("base:world/system-tables")
        .and_then(|bytes| ffe_core::galaxy::Tables::new(bytes).ok());
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
    // A scripted undock for screenshots: start the launch sequence immediately.
    if let Some(dir) = &options.capture {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        if let Some(nav) = nav.as_mut() {
            let _ = nav.begin_launch();
        }
    }
    // The station models' docking faces (their +z ends) and sizes, from the vertices.
    if let (Some(nav), Some(scene)) = (nav.as_mut(), scene.as_ref()) {
        for model in STATION_MODELS {
            let found = scene.find(model);
            nav.set_port_geometry(
                model,
                found.and_then(|m| m.front()),
                found.and_then(|m| m.radius()),
            );
        }
    }
    let mut paused = false;
    // The selected line of the station's market.
    let mut trade_row = 0usize;
    let mut last_mode = state.mode;
    let mut message = match nav
        .as_ref()
        .and_then(|nav| nav.station().map(|(name, _)| name))
    {
        Some(name) => format!(
            "Docked at {name}, {}. Up/Down choose goods, B buys, N sells; L launches.",
            nav.as_ref().map_or("", |nav| nav.system_name())
        ),
        None => "Docked. L launches.".to_owned(),
    };
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
        let edge_point = if active && mouse_down && !mouse_was_down {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| Screen::logical_point(x, y, window.get_size()))
        } else {
            None
        };
        if let Some((x, y)) = edge_point {
            if y < 11
                && (ffe_rs::screen::CONTROLS_BUTTON.0..ffe_rs::screen::CONTROLS_BUTTON.1)
                    .contains(&x)
            {
                modern = !modern;
                held_controls.disarm(&keys_down);
                message = if modern {
                    "Modern controls."
                } else {
                    "Classic controls."
                }
                .to_owned();
            }
        }
        let click_point = if active
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
        } else {
            None
        };
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
        // The other console buttons are not in this build.
        if console_click == Some(Click::Pause) {
            paused = !paused;
            held_controls.disarm(&keys_down);
            message = if paused { "Paused." } else { "Resumed." }.to_owned();
        }
        if active {
            // F1 is the original's view key; help moves to H.
            if pressed(Key::H) && nav.as_ref().is_none_or(|nav| nav.view != NavView::Galaxy) {
                help = !help;
                scenarios = false;
                model_lab = None;
                model_viewer = None;
                held_controls.disarm(&keys_down);
            }
            let shift_down =
                window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
            // F12 is the model viewer (Shift+F12 the model lab).
            // F11 saves the presented frame for bug reports (a binary PPM).
            if pressed(Key::F11) {
                let dir = options.root.join("shots");
                let _ = fs::create_dir_all(&dir);
                let mut number = 1;
                while dir.join(format!("shot-{number:04}.ppm")).exists() {
                    number += 1;
                }
                let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
                for pixel in &screen.pixels {
                    out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
                }
                message = match fs::write(dir.join(format!("shot-{number:04}.ppm")), out) {
                    Ok(()) => format!("Saved shots/shot-{number:04}.ppm"),
                    Err(error) => format!("Screenshot failed: {error}"),
                };
            }
            if pressed(Key::F12) && shift_down {
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
            if pressed(Key::F12) && !shift_down {
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
            // The hyperdrive's view of the ship and its fuel (the galaxy list and the jump).
            if let Some(nav) = nav.as_mut() {
                nav.drive = drive_state(scene.as_ref(), &state);
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
                // The console icons: the map button (F3) lists the systems, the
                // navigation computer lists this system, the H icon hyperspaces.
                let toggle = |nav: &mut ffe_rs::navigation::Navigator, view| {
                    if nav.view == view {
                        nav.view = NavView::Closed;
                    } else {
                        nav.open(view);
                    }
                };
                if pressed(Key::Tab) || console_click == Some(Click::Navigation) {
                    toggle(nav, NavView::System);
                    held_controls.disarm(&keys_down);
                }
                if pressed(Key::G) || console_click == Some(Click::Screen(3)) {
                    toggle(nav, NavView::Galaxy);
                    held_controls.disarm(&keys_down);
                }
                // The galaxy map (the original's F3 screen) stands in for the system list.
                if nav.view == NavView::Galaxy {
                    if galaxy.is_none() && sector_tables.is_some() {
                        if let Some(home) = nav.current_position() {
                            galaxy = Some(GalaxyMap::new(home, nav.reach().unwrap_or(0)));
                        }
                    }
                } else {
                    galaxy = None;
                }
                if let Some(map) = galaxy.as_mut() {
                    let held = |key| window.is_key_down(key);
                    let slow = held(Key::LeftShift) || held(Key::RightShift);
                    let rate = if slow { 0.25 } else { 1.0 };
                    // Arrows move the centre (three quarters of a sector a second), Page Up/Down
                    // or +/- zoom (the original's 0x1f40 a frame), dragging turns the view.
                    let step =
                        (f64::from(ffe_rs::galaxy_map::SECTOR) * 0.75 * rate * elapsed) as i32;
                    let (mut dx, mut dy) = (0, 0);
                    if held(Key::Left) {
                        dx -= step;
                    }
                    if held(Key::Right) {
                        dx += step;
                    }
                    if held(Key::Up) {
                        dy += step;
                    }
                    if held(Key::Down) {
                        dy -= step;
                    }
                    if dx != 0 || dy != 0 {
                        map.scroll(dx, dy);
                    }
                    let zoom = (f64::from(0x1f40) * rate * elapsed / 0.02) as i32;
                    if held(Key::PageUp) || held(Key::Equal) || held(Key::NumPadPlus) {
                        map.zoom_by(-zoom);
                    }
                    if held(Key::PageDown) || held(Key::Minus) || held(Key::NumPadMinus) {
                        map.zoom_by(zoom);
                    }
                    map.zoom = map.zoom.max(0x2000);
                    if pressed(Key::C) {
                        map.recentre();
                    }
                    let point = window
                        .get_unscaled_mouse_pos(MouseMode::Pass)
                        .and_then(|(x, y)| Screen::logical_point(x, y, window.get_size()));
                    if active && mouse_down {
                        if let (Some(before), Some(now)) = (map_drag, point) {
                            map.rotate((now.0 - before.0) as i16, (now.1 - before.1) as i16);
                        }
                        map_drag = point;
                    } else {
                        map_drag = None;
                    }
                }
                let mut hyperspace = false;
                if console_click == Some(Click::Hyperspace) {
                    if nav.view == NavView::Galaxy {
                        hyperspace = true;
                    } else {
                        message =
                            "Open the map, choose a system, then press H or this icon.".to_owned();
                    }
                }
                if nav.view != NavView::Closed {
                    if pressed(Key::Up) && galaxy.is_none() {
                        nav.step_selection(-1);
                    }
                    if pressed(Key::Down) && galaxy.is_none() {
                        nav.step_selection(1);
                    }
                    if nav.view == NavView::System {
                        if pressed(Key::Right) || pressed(Key::PageDown) {
                            nav.turn_page(1);
                        }
                        if pressed(Key::Left) || pressed(Key::PageUp) {
                            nav.turn_page(-1);
                        }
                    }
                    let mut enter = pressed(Key::Enter);
                    if let Some((x, y)) = click_point.filter(|_| galaxy.is_none()) {
                        let count = nav.lines(ship_position(&original)).len();
                        let row = if nav.view == NavView::System {
                            Screen::nav_row(x, y).filter(|row| *row < count)
                        } else {
                            Screen::map_row(nav.selected, count, x, y)
                        };
                        if let Some(row) = row {
                            nav.selected = row;
                            // A click on a body of this system engages the autopilot; a
                            // click on a system selects it (H hyperspaces).
                            if nav.view == NavView::System {
                                enter = true;
                            } else {
                                message = "H hyperspaces to the selected system.".to_owned();
                            }
                        }
                    }
                    if pressed(Key::H) && nav.view == NavView::Galaxy {
                        hyperspace = true;
                    }
                    if nav.view == NavView::Galaxy && enter && click_point.is_none() {
                        hyperspace = true;
                    }
                    let engage = enter && nav.view == NavView::System;
                    if hyperspace || engage {
                        held_controls.disarm(&keys_down);
                        if original.is_none() {
                            // Navigation moves the original flight model's ship: start it,
                            // as O does.
                            let stats = scene
                                .as_ref()
                                .and_then(|scene| scene.ship_stats(state.ship));
                            original = Some(
                                OriginalFlight::from_flight(&state, stats.as_ref())
                                    .with_time_acceleration(acceleration),
                            );
                        }
                        if hyperspace && galaxy.is_some() {
                            // The map's selection must be a system the game has bodies for.
                            let entry = galaxy
                                .as_ref()
                                .and_then(|map| map.selected.as_ref())
                                .and_then(|selected| nav.entry_for_code(selected.code));
                            match entry {
                                Some(index) => nav.selected = index,
                                None => {
                                    hyperspace = false;
                                    message = "Unexplored system: only the systems on offer can be entered (centre one of them)."
                                        .to_owned();
                                }
                            }
                        }
                        if hyperspace {
                            let index = nav.selected;
                            match nav.hyperspace(&cache, index) {
                                Ok(jumped) => {
                                    if let Some(model) = original.as_mut() {
                                        model.teleport(jumped.arrival);
                                    }
                                    burn_fuel(&mut state, nav.drive, jumped.fuel);
                                    message = format!(
                                        "Hyperspace: {} in {:.1} days, {} t of fuel. Arrived in the {} system.",
                                        ffe_rs::navigation::range_text(jumped.distance),
                                        jumped.days,
                                        jumped.fuel,
                                        nav.system_name()
                                    );
                                    nav.view = NavView::Closed;
                                }
                                Err(error) => message = format!("Jump refused: {error}"),
                            }
                        } else if let Some(body) = nav.engage() {
                            message = format!("Autopilot to {}.", body.name);
                            nav.view = NavView::Closed;
                        }
                    }
                }
                if (pressed(Key::Backspace) || console_click == Some(Click::AutopilotOff))
                    && nav.target.take().is_some()
                {
                    nav.disengage();
                    message = "Autopilot off.".to_owned();
                }
                // Targets: a click on a body in the view, T (Shift+T the previous) or the
                // target buttons lock on to it; K or the AUTOPILOT button flies there, and
                // at a station asks for clearance and docks.
                if nav.view == NavView::Closed {
                    let ship = ship_position(&original);
                    if pressed(Key::T) || console_click == Some(Click::Target) {
                        let back = window.is_key_down(Key::LeftShift)
                            || window.is_key_down(Key::RightShift);
                        if let Some(body) = nav.cycle_lock(ship, if back { -1 } else { 1 }) {
                            message = format!(
                                "Locked on {} {}. K or AUTOPILOT flies there.",
                                body.name,
                                describe(distance(ship, body.position))
                            );
                        }
                    }
                    if let Some(point) = click_point
                        .filter(|(_, y)| console_click.is_none() && (12..118).contains(y))
                    {
                        let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                        let candidates: Vec<(usize, [f64; 3], f64)> = nav
                            .bodies
                            .iter()
                            .enumerate()
                            .map(|(index, body)| {
                                (
                                    index,
                                    body.position.map(|v| v as f64 / unit),
                                    body.radius as f64 / unit,
                                )
                            })
                            .collect();
                        let hit = Screen::pick(&state, &views, scene.as_ref(), &candidates, point);
                        if let Some(body) = hit.and_then(|index| nav.lock(index)) {
                            message = format!(
                                "Locked on {} {}. K or AUTOPILOT flies there.",
                                body.name,
                                describe(distance(ship, body.position))
                            );
                        }
                    }
                    if pressed(Key::K) || console_click == Some(Click::Autopilot) {
                        if nav.locked.is_none() {
                            message =
                                "Lock a target first: click it in the view, or press T.".to_owned();
                        } else {
                            held_controls.disarm(&keys_down);
                            if original.is_none() {
                                // The autopilot flies the original flight model: start it, as
                                // O does.
                                let stats = scene
                                    .as_ref()
                                    .and_then(|scene| scene.ship_stats(state.ship));
                                original = Some(
                                    OriginalFlight::from_flight(&state, stats.as_ref())
                                        .with_time_acceleration(acceleration),
                                );
                            }
                            if let Some(body) = nav.engage_locked() {
                                message = if matches!(body.kind, NavKind::Station) {
                                    format!("Autopilot: docking at {}.", body.name)
                                } else {
                                    format!("Autopilot to {}.", body.name)
                                };
                            }
                        }
                    }
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
                if state.mode == ffe_rs::flight::Mode::Docked {
                    let step = i32::from(pressed(Key::Down)) - i32::from(pressed(Key::Up));
                    if step != 0 {
                        let docked = nav.as_ref().and_then(|nav| nav.docked);
                        let rows = match (nav.as_mut(), docked) {
                            (Some(nav), Some(station)) => {
                                trade::listing(nav, &cache, station, &state).map_or(0, |r| r.len())
                            }
                            _ => 0,
                        };
                        if rows > 0 {
                            trade_row = (trade_row as i32 + step).rem_euclid(rows as i32) as usize;
                        }
                    }
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
                    match nav.as_mut() {
                        Some(nav) if state.mode == ffe_rs::flight::Mode::Docked => {
                            match nav.begin_launch() {
                                Ok(()) => {
                                    acceleration = 0;
                                    message = "Launch permission granted: moving into the lift."
                                        .to_owned();
                                    None
                                }
                                Err(error) => Some(Err(error)),
                            }
                        }
                        _ => Some(state.launch()),
                    }
                } else if pressed(Key::D)
                    && state.system_flight
                    && state.mode == ffe_rs::flight::Mode::Flight
                {
                    let ship = ship_position(&original);
                    message = match nav.as_mut() {
                        Some(nav) if nav.sequence.is_some() => {
                            "Docking or launching is under way.".to_owned()
                        }
                        Some(nav) if nav.cleared_anywhere() => {
                            nav.cancel_clearance();
                            "Docking clearance cancelled.".to_owned()
                        }
                        Some(nav) => match nav.request_clearance(ship) {
                            Some((index, Reply::Granted(bay))) => format!(
                                "{}: docking clearance {} (bay {}). Enter the slot on the docking face.",
                                nav.bodies[index].name,
                                if bay.fresh { "granted" } else { "already granted" },
                                bay.index + 1
                            ),
                            Some((index, other)) => {
                                format!("{}: {}.", nav.bodies[index].name, refusal(other))
                            }
                            None => "No station within 100 km to ask for clearance.".to_owned(),
                        },
                        None => "No stations: navigation is unavailable.".to_owned(),
                    };
                    None
                } else if pressed(Key::D) {
                    Some(state.request_docking())
                } else if pressed(Key::B) || pressed(Key::N) {
                    // At a station of a system the original's market; the prototype's
                    // one-item trade otherwise.
                    let buying = pressed(Key::B);
                    let station = nav.as_ref().and_then(|nav| nav.docked);
                    let shift =
                        window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
                    match (nav.as_mut(), station) {
                        (Some(nav), Some(station))
                            if state.mode == ffe_rs::flight::Mode::Docked
                                && nav.market_tables().is_some() =>
                        {
                            let rows = trade::listing(nav, &cache, station, &state);
                            match rows.map(|rows| rows.get(trade_row).map(|row| row.good)) {
                                Ok(Some(good)) => {
                                    match trade::trade(
                                        nav,
                                        &cache,
                                        &mut state,
                                        station,
                                        good,
                                        buying,
                                        if shift { 10 } else { 1 },
                                    ) {
                                        Ok(text) => message = text,
                                        Err(error) => message = format!("Trade failed: {error}"),
                                    }
                                    None
                                }
                                Ok(None) => None,
                                Err(error) => {
                                    message = format!("Market unavailable: {error}");
                                    None
                                }
                            }
                        }
                        _ if buying => Some(state.buy()),
                        _ => Some(state.sell()),
                    }
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
                // Classic: the original's keys. Modern adds W/S for the set speed, the arrows
                // for pitch (up is nose up, which is Z) and yaw, Q/E for roll.
                let both =
                    |classic: &[Key], extra: &[Key]| down(classic) || (modern && down(extra));
                let mut keys = OriginalKeys {
                    faster: both(&[Key::Enter], &[Key::W]),
                    slower: both(&[Key::LeftShift, Key::RightShift], &[Key::S]),
                    pitch_down: both(&[Key::Z], &[Key::Up]),
                    pitch_up: both(&[Key::A], &[Key::Down]),
                    left: both(&[Key::Comma], &[Key::Left]),
                    right: both(&[Key::Period], &[Key::Right]),
                    roll_left: both(&[Key::LeftBracket], &[Key::Q]),
                    roll_right: both(&[Key::RightBracket], &[Key::E]),
                };
                let mut command_face = None;
                if nav.as_ref().is_some_and(|nav| nav.holding()) {
                    keys = OriginalKeys::default();
                    model.teleport(model.player().motion.position);
                }
                if let Some(nav) = nav.as_mut().filter(|nav| !nav.holding()) {
                    if let Some((command, body)) = nav.command(&cache, model) {
                        keys = command.keys;
                        command_face = command.face;
                        if let Some((landing, seconds)) = command.skip {
                            model.teleport(landing);
                            nav.transfer(&cache, seconds);
                            message = format!("Transfer to {}: {:.0} s.", body.name, seconds);
                        } else if command.arrived {
                            message = format!("Arrived at {}.", body.name);
                            nav.target = None;
                        } else if nav.docking() {
                            let range = distance(model.player().motion.position, body.position);
                            message =
                                format!("Autopilot: docking at {} {}.", body.name, describe(range));
                        } else {
                            let range = distance(model.player().motion.position, body.position);
                            message = format!("Autopilot: {} {}.", body.name, describe(range));
                        }
                    }
                    if let Some(notice) = nav.notice.take() {
                        message = notice;
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
                if let Some(target) = command_face {
                    model.turn_toward(target, elapsed, 1.0);
                }
                match model.step(elapsed, &keys) {
                    Ok(()) => {
                        model.apply(&mut state);
                        // The stations: the slot docks a cleared ship, the hull pushes it clear.
                        if let Some(nav) = nav.as_mut().filter(|_| state.system_flight) {
                            let ship = model.player().motion.position;
                            if let Some((index, contact)) = nav.contact(ship, state.speed() * 9.7) {
                                let name = nav.bodies[index].name.clone();
                                match contact {
                                    Contact::None => {}
                                    Contact::Docked => {
                                        if nav.begin_docking(index) {
                                            acceleration = 0;
                                            model.set_time_acceleration(0);
                                            message =
                                                format!("{name}: docking, aligning with the lift.");
                                        }
                                    }
                                    Contact::Collision { clear, reason } => {
                                        model.teleport(clear);
                                        model.face(nav.bodies[index].position);
                                        model.apply(&mut state);
                                        message =
                                            format!("{name}: {}. Pushed clear.", reason.text());
                                    }
                                }
                            }
                        }
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
        // Traffic control: doors, clearance timeouts and the docking and launch stages.
        if active && !help && !paused && model_lab.is_none() && model_viewer.is_none() {
            if let Some(nav) = nav.as_mut() {
                let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
                    / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
                let tics = (elapsed * ffe_rs::original_flight::TICS_PER_SECOND * factor) as u32;
                for event in nav.tick_traffic(tics, elapsed * factor) {
                    match event {
                        Traffic::Stage(text) => {
                            message = format!(
                                "{}: {text}.",
                                if nav.sequence.is_some_and(|s| s.launching) {
                                    "Launching"
                                } else {
                                    "Docking"
                                }
                            );
                        }
                        Traffic::Docked(index) => {
                            state.mode = ffe_rs::flight::Mode::Docked;
                            state.position = [0.0, 0.0, -60.0];
                            state.velocity = [0.0; 3];
                            state.thrust = 0.0;
                            state.docking_requested = false;
                            message = format!(
                                "Docked at {}. Up/Down choose goods, B buys, N sells (Shift: 10); L launches.",
                                nav.bodies[index].name
                            );
                        }
                        Traffic::LaunchOut(_) => {
                            if let Err(error) = state.launch() {
                                message = error;
                            }
                        }
                        Traffic::LaunchDone(index) => {
                            message = format!("Clear of {}.", nav.bodies[index].name);
                        }
                    }
                }
            }
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
        // Launching from the station puts the ship out from it in this system, on the
        // original flight model (which the navigation moves).
        if last_mode == ffe_rs::flight::Mode::Docked && state.mode == ffe_rs::flight::Mode::Flight {
            let stats = scene
                .as_ref()
                .and_then(|scene| scene.ship_stats(state.ship));
            let mut model = OriginalFlight::from_flight(&state, stats.as_ref())
                .with_time_acceleration(acceleration);
            if let Some((_, at)) = nav.as_ref().and_then(|nav| nav.station()) {
                model.teleport(at);
                // Out of the slot along the docking axis, else toward the nearest world.
                let port = nav
                    .as_ref()
                    .and_then(|nav| nav.docked.and_then(|index| nav.port(index)));
                if let Some(port) = port {
                    model.face(port.point(port.front + 1.0e12));
                } else if let Some(world) = nav.as_ref().and_then(|nav| nav.nearest_world(at)) {
                    model.face(world);
                }
                state.system_flight = true;
            }
            original = Some(model);
            message = if modern {
                "Launched. W/S speed, arrows pitch and yaw, Q/E roll; the map icons list systems."
            } else {
                "Launched. Enter/Shift speed, Z/A pitch, comma/period yaw, [ ] roll."
            }
            .to_owned();
        }
        last_mode = state.mode;
        if let Some(nav) = nav.as_ref() {
            screen.system = nav.system_name().to_owned();
            screen.station = nav
                .station()
                .map_or_else(String::new, |(n, _)| n.to_owned());
        }
        // The local star lights ships, stations and the lift scene in a system.
        screen.sun = nav.as_ref().and_then(|nav| {
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            let star = nav.bodies.iter().find(|b| b.kind == NavKind::Star)?;
            let toward = [
                star.position[0] as f64 / unit - state.position[0],
                star.position[1] as f64 / unit - state.position[1],
                star.position[2] as f64 / unit - state.position[2],
            ];
            let length = toward.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
            Some(toward.map(|v| v / length))
        });
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
                        model: Some((b.model, b.seed)),
                        orientation: b.orientation,
                    }
                })
                .collect()
        });
        screen.stations = match nav.as_ref().filter(|_| state.system_flight) {
            Some(nav) => {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                let tics = ffe_core::model_animation::START_TICS.wrapping_add(
                    (nav.clock * ffe_rs::original_flight::TICS_PER_SECOND) as u64 as u32,
                );
                nav.bodies
                    .iter()
                    .filter(|b| b.kind == NavKind::Station)
                    .map(|b| ffe_rs::screen::SystemStation {
                        model: b.model,
                        centre: b.position.map(|v| v as f64 / unit),
                        orientation: b.orientation,
                        speed: b.spin,
                        tics,
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // The lift scenes of the docking and launching stages (the front view).
        screen.lift = None;
        let mut launching_scene = false;
        if let Some((objects, launching)) = nav.as_ref().and_then(|nav| nav.lift_scene()) {
            views.mode = ViewMode::Front;
            launching_scene = launching && state.mode == ffe_rs::flight::Mode::Docked;
            let station = nav
                .as_ref()
                .and_then(|nav| nav.sequence.map(|s| s.station).map(|i| &nav.bodies[i]));
            screen.lift = Some(ffe_rs::screen::LiftView {
                objects,
                seed: station.map_or(0, |body| body.seed),
                starport: station.is_some_and(|body| body.kind == NavKind::Starport),
                world: true,
                tics: ffe_core::model_animation::START_TICS.wrapping_add(
                    (nav.as_ref().map_or(0.0, |nav| nav.clock) * 49_700.0) as u64 as u32,
                ),
            });
        }
        screen.galaxy = None;
        if let (Some(map), Some(tables), Some(viewer), Some(nav)) = (
            galaxy.as_mut(),
            sector_tables.as_ref(),
            scene.as_ref(),
            nav.as_ref(),
        ) {
            map.update_selection(tables);
            match map.render(viewer, tables) {
                Ok(shapes) => {
                    let report = map.selected.as_ref().and_then(|selected| {
                        nav.report_for(ffe_core::sector::distance(map.home, selected.position))
                    });
                    screen.galaxy = Some(ffe_rs::screen::MapScene {
                        shapes,
                        sector: map.sector_text(),
                        fuel: map.fuel_text(report.as_ref()),
                        date: date_text(nav.clock),
                        info: map
                            .describe(viewer, system_details.as_ref())
                            .unwrap_or_default(),
                    });
                }
                Err(error) => message = format!("Map error: {error}"),
            }
        }
        screen.market = None;
        if state.mode == ffe_rs::flight::Mode::Docked {
            if let Some(nav) = nav.as_mut() {
                if let Some(station) = nav.docked.filter(|_| nav.market_tables().is_some()) {
                    if let Ok(rows) = trade::listing(nav, &cache, station, &state) {
                        trade_row = trade_row.min(rows.len().saturating_sub(1));
                        let hold = trade::hold_of(&state, nav.cargo_space(&cache, state.ship));
                        screen.market = Some(ffe_rs::screen::MarketView {
                            station: nav.bodies[station].name.clone(),
                            system: nav.system_name().to_owned(),
                            rows: rows
                                .into_iter()
                                .map(|row| ffe_rs::screen::MarketRow {
                                    name: row.name,
                                    price: row.price,
                                    stock: row.stock,
                                    held: row.held,
                                })
                                .collect(),
                            selected: trade_row,
                            cash: i32::try_from(state.credits).unwrap_or(i32::MAX),
                            used: hold.used,
                            capacity: hold.capacity,
                        });
                    }
                }
            }
        }
        // The autopilot's target, else the locked body: a line of text and brackets in the view.
        let engaged = nav.as_ref().and_then(|nav| nav.target);
        let marked = engaged.or_else(|| nav.as_ref().and_then(|nav| nav.locked));
        screen.has_lock = nav.as_ref().is_some_and(|nav| nav.locked.is_some());
        screen.lock = match nav.as_ref().zip(marked) {
            Some((nav, i)) => format!(
                "{}: {} {}",
                if engaged.is_some() {
                    "AUTOPILOT"
                } else {
                    "TARGET"
                },
                nav.bodies[i].name.to_ascii_uppercase(),
                describe(distance(ship_position(&original), nav.bodies[i].position))
                    .to_ascii_uppercase()
            ),
            None => String::new(),
        };
        screen.target_mark = nav
            .as_ref()
            .zip(marked)
            .filter(|_| state.mode == ffe_rs::flight::Mode::Flight)
            .and_then(|(nav, i)| {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                let body = nav.bodies.get(i)?;
                let (x, y, radius) = Screen::body_screen(
                    &state,
                    &views,
                    scene.as_ref(),
                    body.position.map(|v| v as f64 / unit),
                    body.radius as f64 / unit,
                )?;
                Some(ffe_rs::screen::TargetMark {
                    x,
                    y,
                    radius,
                    engaged: engaged.is_some(),
                })
            });
        if launching_scene {
            // Inside the station, from the cockpit: the flight view with only the lift in it.
            let mut inside = state.clone();
            inside.mode = ffe_rs::flight::Mode::Flight;
            inside.system_flight = true;
            inside.axes = None;
            // The ship waits in the docking slot facing out, so the world (the stars, the
            // planet and the station seen from its slot) shows past the lift's open doors.
            if let Some(port) = nav
                .as_ref()
                .and_then(|nav| nav.docked.and_then(|index| nav.port(index)))
            {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                inside.position = port.point(port.front * 0.9).map(|v| v as f64 / unit);
                let forward = port.axis;
                let up = [0.0, 1.0, 0.0];
                let right = [
                    up[1] * forward[2] - up[2] * forward[1],
                    up[2] * forward[0] - up[0] * forward[2],
                    up[0] * forward[1] - up[1] * forward[0],
                ];
                inside.axes = Some([right, up, forward]);
            }
            screen.draw(&inside, views, scene.as_ref(), help, modern, &message);
        } else {
            screen.draw(&state, views, scene.as_ref(), help, modern, &message);
        }
        if let Some(dir) = &options.capture {
            if frames % 10 == 0 {
                let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
                for pixel in &screen.pixels {
                    out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
                }
                let _ = fs::write(dir.join(format!("shot-{frames:04}.ppm")), out);
            }
        }
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
        if let Some(nav) = nav
            .as_ref()
            .filter(|nav| nav.view != NavView::Closed && galaxy.is_none())
        {
            let ship = original
                .as_ref()
                .map_or([0; 3], |model| model.player().motion.position);
            let title = match nav.view {
                NavView::Galaxy => "SYSTEMS: H HYPERSPACES TO THE SELECTED".to_owned(),
                _ => format!(
                    "{}: CLICK ONE TO AUTOPILOT",
                    nav.system_name().to_uppercase()
                ),
            };
            if nav.view == NavView::System {
                let rows: Vec<(String, bool, bool)> = nav
                    .listed()
                    .into_iter()
                    .enumerate()
                    .map(|(row, i)| {
                        (
                            nav.bodies[i].name.clone(),
                            row == nav.selected,
                            nav.target == Some(i),
                        )
                    })
                    .collect();
                let detail = match nav.selected_body().map(|i| &nav.bodies[i]) {
                    Some(body) => format!(
                        "{} {} {} PAGE {}/{}  LEFT/RIGHT: PAGES",
                        body.name.to_ascii_uppercase(),
                        body.kind.label().to_ascii_uppercase(),
                        describe(distance(ship, body.position)).to_ascii_uppercase(),
                        nav.page + 1,
                        nav.pages()
                    ),
                    None => String::new(),
                };
                screen.draw_nav_list(&rows, &detail);
            } else {
                screen.draw_map(
                    &title,
                    &nav.lines(ship),
                    nav.selected,
                    &nav.points(ship),
                    if message.is_empty() {
                        "MAP ICON/G SYSTEMS  H HYPERSPACE  ENTER"
                    } else {
                        &message
                    },
                );
            }
        }
        window
            .update_with_buffer(&screen.pixels, WIDTH, HEIGHT)
            .map_err(|e| format!("Window update: {e}"))?;
        frames += 1;
        if options.smoke_frames.is_some_and(|limit| frames >= limit)
            || (options.capture.is_some() && frames >= 400)
        {
            break;
        }
    }
    if options.smoke_frames.is_some() {
        println!("PASS: native window presented {frames} frames using verified cabin artwork.");
    }
    Ok(())
}

/// The ship's hyperdrive as the jump sees it: the statistics' mass, the drive class (the low
/// byte of a negative drive word) and the fuel in the hold (military fuel from class 10).
fn drive_state(
    scene: Option<&ModelViewer>,
    state: &ffe_rs::flight::Flight,
) -> Option<ffe_rs::navigation::DriveState> {
    let stats = scene?.ship_stats(state.ship)?;
    let class = if stats.drive < 0 {
        i32::from(stats.drive as u16 & 0xff)
    } else {
        0
    };
    let slot = if class >= ffe_core::hyperspace::MILITARY_CLASS {
        ffe_core::hyperspace::MILITARY_FUEL
    } else {
        ffe_core::hyperspace::HYDROGEN
    };
    Some(ffe_rs::navigation::DriveState {
        mass: i32::from(stats.mass as i16),
        class,
        fuel: i32::from(state.hold.get(slot).copied().unwrap_or(0)),
    })
}

/// Burn the fuel of a jump from the hold: hydrogen, or military fuel which leaves the same
/// tonnes of radioactive waste (F903).
fn burn_fuel(
    state: &mut ffe_rs::flight::Flight,
    drive: Option<ffe_rs::navigation::DriveState>,
    tonnes: i32,
) {
    let Some(drive) = drive else { return };
    state.hold.resize(ffe_core::market::NAMES, 0);
    let tonnes = u16::try_from(tonnes).unwrap_or(0);
    if drive.class >= ffe_core::hyperspace::MILITARY_CLASS {
        let slot = &mut state.hold[ffe_core::hyperspace::MILITARY_FUEL];
        *slot = slot.saturating_sub(tonnes);
        let waste = &mut state.hold[ffe_core::hyperspace::RADIOACTIVES];
        *waste = waste.saturating_add(tonnes);
    } else {
        let slot = &mut state.hold[ffe_core::hyperspace::HYDROGEN];
        *slot = slot.saturating_sub(tonnes);
    }
}

/// The models a generated station can have (F892's tables): their docking faces
/// are read from their vertices at start-up.
const STATION_MODELS: [u32; 10] = [69, 72, 73, 74, 75, 76, 77, 78, 79, 80];

fn ship_position(original: &Option<OriginalFlight>) -> [i64; 3] {
    original
        .as_ref()
        .map_or([0; 3], |model| model.player().motion.position)
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
