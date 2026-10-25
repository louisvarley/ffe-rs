use ffe_core::laser;
use ffe_core::traffic::Reply;
use ffe_rs::camera::{key_amount, ship_axes, ExternalKeys, ViewMode, ViewState};
use ffe_rs::combat::{self, Combat};
use ffe_rs::console::{
    acceleration_for_digit, Click, ConsoleState, ShipSystems, ACCELERATION_NAMES,
};
use ffe_rs::docking::Contact;
use ffe_rs::flight::{Controls, Flight, STEP};
use ffe_rs::front_end::{self, Intro, IntroAction, IntroTimeline};
use ffe_rs::galaxy_map::GalaxyMap;
use ffe_rs::hud::PlayerHud;
use ffe_rs::input;
use ffe_rs::model_lab::ModelLab;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{
    describe, distance, refusal, surface_radius, Kind as NavKind, Navigator, Traffic,
    View as NavView,
};
use ffe_rs::original_flight::{Keys as OriginalKeys, OriginalFlight};
use ffe_rs::outfitting::{self, Limits};
use ffe_rs::prefs::{
    self, Activate, Key as PrefsKey, Prefs, GRAPHIC, MAIN, MAPS, OTHERS, PORT, SOUND,
};
use ffe_rs::saves;
use ffe_rs::screen::ViewButton;
use ffe_rs::screen::{Screen, HEIGHT, WIDTH};
use ffe_rs::ships::ShipList;
use ffe_rs::sound::{Cue, GameAudio};
use ffe_rs::station::{self, Action as StationAction, DockState, Screen as StationScreen};
use ffe_rs::trade;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use minifb::{MouseButton, MouseMode};

/// What the open-air launch tells the player: F1013/F1031 need thrust to climb
/// out of the starport's effect region.
const TAKEOFF_HINT: &str = "Lifting off: pitch up and raise the set speed to clear the ground.";
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Which game the boot starts: the original's new game or the development sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boot {
    /// Ross 154, the Eagle (model 23), 1000 CR, on the starport pad (`--start 1` default).
    NewGame(u8),
    /// The existing Sol/Mars High sandbox (`--start sol`).
    Sandbox,
}

struct Options {
    root: PathBuf,
    smoke_frames: Option<usize>,
    /// Directory for a scripted undock capture (starts the launch sequence and
    /// saves every tenth frame, then exits).
    capture: Option<PathBuf>,
    /// Directory for a scripted planet approach: the ship is placed a few radii
    /// out from the nearest planet, the autopilot is engaged, and frames are
    /// saved through the approach.
    capture_planet: Option<PathBuf>,
    /// Directory for a scripted NPC traffic frame: the ship is put in flight
    /// beside a spawned pirate, labels and the target readout are on, and a
    /// shield hit and a kill are applied and saved.
    capture_traffic: Option<PathBuf>,
    /// The boot: the original new game (default) or `--start sol`'s sandbox.
    boot: Boot,
    /// `--skip-intro`: the logo/intro screens are not shown. The front end is
    /// modelled in `ffe_rs::front_end`; its drawing is not wired into the boot
    /// yet, so the new game currently starts directly.
    skip_intro: bool,
    /// Directory for one frame of every preference page plus a flight frame
    /// showing the selected space colour; exits without opening a window.
    capture_prefs: Option<PathBuf>,
    /// Directory for the intro's sample frames at fixed `GameTics`; exits without
    /// opening a window.
    capture_intro: Option<PathBuf>,
}

fn options() -> Result<Options, String> {
    let mut root = None;
    let mut smoke_frames = None;
    let mut capture = None;
    let mut capture_planet = None;
    let mut capture_traffic = None;
    let mut boot = Boot::NewGame(1);
    let mut boot_given = false;
    let mut skip_intro = false;
    let mut capture_prefs = None;
    let mut capture_intro = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(
                args.next().ok_or("--root requires a directory")?,
            ));
        } else if arg == "--start" && !boot_given {
            let value = args
                .next()
                .ok_or("--start requires 1..5 or sol")?
                .to_string_lossy()
                .to_ascii_lowercase();
            boot_given = true;
            boot = if value == "sol" {
                Boot::Sandbox
            } else {
                let start_type = value
                    .parse::<u8>()
                    .map_err(|_| "start type must be 1..5 or sol")?;
                if !(1..=5).contains(&start_type) {
                    return Err("start type must be 1..5 or sol".to_owned());
                }
                if start_type != 1 {
                    eprintln!(
                        "Start type {start_type} is the JJFFE mapping and is not verified; \
                         starting type 1 (Ross 154)."
                    );
                }
                Boot::NewGame(1)
            };
        } else if arg == "--skip-intro" && !skip_intro {
            skip_intro = true;
        } else if arg == "--capture" && capture.is_none() {
            capture = Some(PathBuf::from(
                args.next().ok_or("--capture requires a directory")?,
            ));
        } else if arg == "--capture-planet" && capture_planet.is_none() {
            capture_planet = Some(PathBuf::from(
                args.next().ok_or("--capture-planet requires a directory")?,
            ));
        } else if arg == "--capture-traffic" && capture_traffic.is_none() {
            capture_traffic = Some(PathBuf::from(
                args.next()
                    .ok_or("--capture-traffic requires a directory")?,
            ));
        } else if arg == "--capture-prefs" && capture_prefs.is_none() {
            capture_prefs = Some(PathBuf::from(
                args.next().ok_or("--capture-prefs requires a directory")?,
            ));
        } else if arg == "--capture-intro" && capture_intro.is_none() {
            capture_intro = Some(PathBuf::from(
                args.next().ok_or("--capture-intro requires a directory")?,
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
            return Err(format!("Unknown argument {}. Usage: ffe-rs --root <repo-with-data.bin> [--start 1..5|sol] [--skip-intro] [--smoke-frames 1..300] [--capture DIR] [--capture-planet DIR] [--capture-traffic DIR] [--capture-prefs DIR] [--capture-intro DIR]",arg.to_string_lossy()));
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
        capture_planet,
        capture_traffic,
        boot,
        skip_intro,
        capture_prefs,
        capture_intro,
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

fn write_shot(dir: &Path, name: &str, screen: &Screen) {
    let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in &screen.pixels {
        out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    let _ = fs::write(dir.join(name), out);
}

/// The intro's sample `GameTics`: boot, the bitmap boundary, the model switch,
/// the first credit's end, a mid-credit, the last two credits and the loop point.
fn write_intro_capture(
    screen: &mut Screen,
    scene: Option<&ModelViewer>,
    dir: &Path,
) -> Result<(), String> {
    for tics in [
        front_end::BOOT_TICS,
        0x795cc,
        0x795cd,
        front_end::MODEL_SWITCH,
        0xfbd3a,
        0x181534,
        0x77a7d8,
        front_end::INTRO_END - 1,
    ] {
        screen.draw_intro(scene, tics, false)?;
        write_shot(dir, &format!("intro-{tics:08x}.ppm"), screen);
    }
    // The DOS version line latches on V from 0x719668.
    screen.draw_intro(scene, front_end::INTRO_INPUT_FROM, true)?;
    write_shot(dir, "intro-version.ppm", screen);
    Ok(())
}

fn intro_key(key: Key) -> Option<char> {
    match key {
        Key::Key1 => Some('1'),
        Key::Key2 => Some('2'),
        Key::Key3 => Some('3'),
        Key::Key4 => Some('4'),
        Key::Key5 => Some('5'),
        Key::V => Some('V'),
        // Modifiers alone are not GUI actions in the original.
        Key::LeftShift
        | Key::RightShift
        | Key::LeftCtrl
        | Key::RightCtrl
        | Key::LeftAlt
        | Key::RightAlt
        | Key::LeftSuper
        | Key::RightSuper => None,
        // DOS's "any other key" arm (F147(0)).
        _ => Some('\0'),
    }
}

/// F961/F962 in the original's boot loop: `GameTics` starts at 0x400, advances
/// by the timer's tics, and the screen is redrawn every frame. A start key (1-5)
/// leaves with that type; any other key is DOS `F147(0)` and this port maps it to
/// the verified type 1. V latches the version line from 0x719668.
///
/// F919 runs around the same loop: song 0 (`QQFRONT`) starts with the intro,
/// replays when `F1904` reports done, and stops on the leaving key (`ffe.asm`
/// 68419-68428 and 68484).
fn run_intro(
    window: &mut Window,
    screen: &mut Screen,
    scene: Option<&ModelViewer>,
    sound: &mut GameAudio,
) -> Result<u8, String> {
    let intro = Intro::new();
    let mut tics = front_end::BOOT_TICS;
    let mut version = false;
    let mut last = Instant::now();
    sound.front_end_music();
    loop {
        if !window.is_open() {
            sound.leave_front_end();
            return Ok(1);
        }
        screen.draw_intro(scene, tics, version)?;
        window
            .update_with_buffer(&screen.pixels, WIDTH, HEIGHT)
            .map_err(|e| format!("Window update: {e}"))?;
        for key in window.get_keys_pressed(KeyRepeat::No) {
            let Some(character) = intro_key(key) else {
                continue;
            };
            let Some(action) = intro.key(tics, character) else {
                continue;
            };
            match action {
                IntroAction::Version => version = true,
                IntroAction::Start {
                    requested,
                    chosen,
                    notice,
                } => {
                    if let Some(notice) = notice {
                        eprintln!("Intro start key {requested}: {notice}");
                    }
                    sound.leave_front_end();
                    return Ok(chosen);
                }
                IntroAction::Skip { chosen, notice } => {
                    if let Some(notice) = notice {
                        eprintln!("{notice}");
                    }
                    sound.leave_front_end();
                    return Ok(chosen);
                }
            }
        }
        let now = Instant::now();
        let elapsed = now.duration_since(last).as_millis().min(100) as u32;
        last = now;
        // F1904/F232 and F919's replay: the same tick the game loop uses.
        sound.tick(std::time::Duration::from_millis(u64::from(elapsed)));
        tics = IntroTimeline::advance(tics, elapsed);
    }
}

fn run() -> Result<(), String> {
    let options = options()?;
    let cache = ffe_content::read(&read_cache(&options.root.join("data.bin"))?)?;
    let mut screen = Screen::new(&cache)?;
    // Diagnostic frames of the preference pages and of the space colour, for
    // the port's screenshot comparisons. No window is opened.
    if let Some(dir) = &options.capture_prefs {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let mut shown = Prefs {
            open: true,
            ..Default::default()
        };
        for (page, name) in [
            (MAIN, "prefs-main.ppm"),
            (GRAPHIC, "prefs-graphic.ppm"),
            (SOUND, "prefs-sound.ppm"),
            (OTHERS, "prefs-others.ppm"),
            (MAPS, "prefs-maps.ppm"),
            (PORT, "prefs-port.ppm"),
        ] {
            shown.page = page;
            shown.selected = 0;
            screen.draw_prefs(&shown);
            write_shot(dir, name, &screen);
        }
        let scene = ModelViewer::new(&cache).ok();
        let state = Flight::free_flight();
        for (index, name) in [(4, "space-light-blue.ppm"), (0, "space-black.ppm")] {
            screen.sky = prefs::space_colour_rgb(index);
            screen.draw(
                &state,
                ViewState::default(),
                scene.as_ref(),
                false,
                true,
                &format!("SPACE COLOUR: {}", prefs::SPACE_COLOURS[index]),
            );
            write_shot(dir, name, &screen);
        }
        return Ok(());
    }
    if let Some(dir) = &options.capture_intro {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let scene = ModelViewer::new(&cache).ok();
        write_intro_capture(&mut screen, scene.as_ref(), dir)?;
        return Ok(());
    }
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
    // The Escape pages, including the modern/classic control preset the top
    // bar's button and F3 also toggle.
    let mut prefs = Prefs::default();
    // Sound effects and music. The original's cues (`F1906`/`F1907`) and songs
    // (`F1902`..`F1904`) are posted from the events below; captures and the
    // smoke test never open a device.
    let headless = options.capture.is_some()
        || options.capture_planet.is_some()
        || options.capture_traffic.is_some()
        || options.smoke_frames.is_some();
    let mut sound = if headless {
        GameAudio::silent(&options.root)
    } else {
        GameAudio::from_cache(&options.root, &cache)
    };
    sound.apply_prefs(&prefs);
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
    // F2's status/equipment page (`F1120`, `DATA_008871 & 8`).
    let mut status_page = false;
    // The last message pushed into the original's ten-entry log.
    let mut logged = String::new();
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
    // The original's boot shows the intro until a start key. Scripted captures and
    // the smoke frame count skip it; --start sol is the development sandbox.
    let scripts = options.smoke_frames.is_some()
        || options.capture.is_some()
        || options.capture_planet.is_some()
        || options.capture_traffic.is_some();
    let mut boot = options.boot;
    if !options.skip_intro && !scripts && matches!(options.boot, Boot::NewGame(1)) {
        boot = Boot::NewGame(run_intro(
            &mut window,
            &mut screen,
            scene.as_ref(),
            &mut sound,
        )?);
    }
    // F569's scan needs the model collision streams and the terrain tables. They
    // are borrowed from the cache for the whole run.
    let landing_library = ffe_rs::navigation::Library(&cache);
    let landing_tools = cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(
            |(terrain_table, terrain_maps)| ffe_core::surface::SurfaceTools {
                shapes: &landing_library,
                terrain_table,
                terrain_maps,
                detail: 0,
            },
        );
    let model_records = cache.asset("base:model/records").unwrap_or(&[]);
    let model_stats = cache.asset("base:ship/stats").unwrap_or(&[]);
    // The original's named craft (cache v11); Page Up/Down changes the player's ship.
    let ships = scene
        .as_ref()
        .map(|scene| ShipList::from_cache(&cache, scene))
        .unwrap_or_default();
    // The systems, their bodies and the autopilot (Tab, G, Backspace). The default
    // boot is the original's new game at Ross 154; `--start sol` keeps the
    // development sandbox for the existing tests and screenshots.
    let mut nav = match boot {
        Boot::NewGame(start_type) => Navigator::new_game(&cache, start_type),
        Boot::Sandbox => Navigator::new(&cache),
    }
    .map_err(|error| {
        eprintln!("Navigation unavailable: {error}");
    })
    .ok();
    if let Some(nav) = nav.as_ref() {
        if let Some(new_game) = nav.new_game {
            state.credits = u32::try_from(new_game.cash).unwrap_or(0);
            state.ship = new_game.ship;
            state.mode = ffe_rs::flight::Mode::Docked;
            eprintln!(
                "New game: {} (start type {}), {} CR, ship {}, hold {} t, time accel {}.",
                nav.system_name(),
                new_game.start_type,
                new_game.cash,
                new_game.ship,
                new_game.cargo,
                new_game.acceleration
            );
        }
    }
    if options.skip_intro {
        eprintln!("--skip-intro: the logo sequence and the credits are not shown.");
    }
    // F1910: the initial new game is on a starport pad, so the station bed is
    // requested once. (The original keeps re-sending its volume/pitch.)
    if nav.as_ref().is_some_and(|nav| nav.docked.is_some()) {
        sound.play(Cue::Station);
    }
    // F998's new-game armament: one 1MW pulse laser in the front mount, a class-2
    // drive and 1 t of energy on the start-type 1 hull, with that hull's general
    // equipment bits and two 0x82 missiles (the equipment page's icons). The port
    // has no outfitting yet, so the same original loadout arms the sandbox too.
    state.combat = Combat::new_game();
    state.loadout = ffe_core::equipment::Loadout::new_game();
    // A scripted undock for screenshots: start the launch sequence immediately.
    if let Some(dir) = &options.capture {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        if let Some(nav) = nav.as_mut() {
            let _ = nav.begin_launch();
        }
    }
    // A scripted approach to the nearest planet for screenshots: put the ship a
    // few radii out on the same side as the starting station, flying the same
    // autopilot and original flight model the playable uses.
    let mut captured_planet: Option<String> = None;
    let mut capture_port = false;
    if let Some(dir) = &options.capture_planet {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        if let Some(nav) = nav.as_mut() {
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            let from = nav
                .docked
                .and_then(|index| nav.bodies.get(index))
                .map_or([0; 3], |station| station.position);
            let nearest = nav
                .bodies
                .iter()
                .enumerate()
                .filter(|(_, body)| body.kind == NavKind::Planet)
                .min_by(|a, b| {
                    distance(from, a.1.position).total_cmp(&distance(from, b.1.position))
                })
                .map(|(index, _)| index);
            if let Some(index) = nearest {
                let body = nav.bodies[index].clone();
                // Three radii out, on the station's side of the planet (the face the
                // launch shows), so the local star lights the same hemisphere.
                let out: [f64; 3] = std::array::from_fn(|k| (from[k] - body.position[k]) as f64);
                let length = out.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
                let reach =
                    (3.0 * body.radius as f64).max(ffe_rs::navigation::standoff(&body) * 1.5);
                let start: [f64; 3] = std::array::from_fn(|k| {
                    body.position[k] as f64 / unit + out[k] / length * reach / unit
                });
                state.mode = ffe_rs::flight::Mode::Flight;
                state.system_flight = true;
                state.position = start;
                state.axes = None;
                state.yaw = 0.0;
                state.pitch = 0.0;
                let stats = scene
                    .as_ref()
                    .and_then(|scene| scene.ship_stats(state.ship));
                let mut model = OriginalFlight::from_flight(&state, stats.as_ref())
                    .with_time_acceleration(acceleration);
                model.teleport(start.map(|v| (v * unit) as i64));
                model.face(body.position);
                model.apply(&mut state);
                original = Some(model);
                nav.lock(index);
                nav.engage_locked();
                captured_planet = Some(body.name);
            }
        }
    }
    // A scripted NPC traffic frame: put the ship beside the spawned pirate,
    // labels on, and fire a shield hit and a kill through F953/F942.
    let mut captured_traffic = false;
    let mut capture_placed = false;
    if let Some(dir) = &options.capture_traffic {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        // The camera and the frozen target are placed on the first flight frame
        // (the launch's F688 traffic spawn replaces the ships this block would
        // otherwise select); labels start on.
        captured_traffic = true;
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
    // The original's text-label flag: player struct +0xe9 bit 2 (DATA_008871 in
    // cplayer.txt's older naming), toggled by `l` or the console's labels icon.
    let mut labels = false;
    if captured_traffic {
        labels = true;
    }
    let mut last_mode = state.mode;
    // The scripted capture of the new game starts on the pad: the launch block
    // below sees a docked-to-flight change and places the model there.
    if options.capture.is_some() && nav.as_ref().is_some_and(|nav| nav.takeoff.is_some()) {
        state.mode = ffe_rs::flight::Mode::Flight;
        state.system_flight = true;
    }
    // The station the ship has just left: its hull is not a wall to the ship
    // until it is clear of the slot (F1088 leaves the player at the station).
    let mut launch_grace: Option<usize> = None;
    // The pad seat and its up row, for F1013's altitude while the starport's
    // complex shape reports no distance (DATA_008867 = 0x7fffffff).
    let mut takeoff_climb: Option<([i64; 3], [f64; 3])> = None;
    // The station screen the commander is on (the services root by default).
    let mut dock = DockState::default();
    let mut message = match nav.as_ref().and_then(|nav| {
        nav.docked
            .map(|index| (nav.bodies[index].name.as_str(), nav.bodies[index].kind))
    }) {
        Some((name, NavKind::Starport)) => format!(
            "Landed at {name}, {}. L launches; Up/Down choose goods, B buys, N sells.",
            nav.as_ref().map_or("", |nav| nav.system_name())
        ),
        Some(_) => station::SERVICES_HINT.to_owned(),
        None => "Docked. L launches.".to_owned(),
    };
    if let Some(name) = &captured_planet {
        message = format!("Capture: autopilot to {name} from three radii.");
    }
    let mut last = Instant::now();
    let mut accumulator = 0.0;
    let mut frames = 0;
    // A scripted new-game capture stops a moment after F1031 clears the pad.
    let capture_takeoff =
        options.capture.is_some() && nav.as_ref().is_some_and(|nav| nav.takeoff.is_some());
    let mut takeoff_done_at: Option<usize> = None;
    let mut held_controls = input::HeldControls::default();
    while window.is_open() {
        let now = Instant::now();
        let measured = now.duration_since(last).as_secs_f64().min(0.1);
        last = now;
        // The planet capture is scripted: a fixed 0.1 s (the original model's
        // largest frame) per presented frame, so the approach takes minutes of
        // world time in a minute of wall time.
        let elapsed = if options.capture_planet.is_some() {
            0.1
        } else {
            measured
        };
        // F1904/F232/F919: advance the song and restart the front-end theme or
        // the continuous-music selection when the current song is done.
        sound.tick(std::time::Duration::from_secs_f64(elapsed));
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
        // Escape opens the original's preference pages; Escape in the pages
        // closes them again. The original reaches the same screen through a
        // first Escape that stops time (FUNC_000224); this port opens the
        // pages directly, as the owner asked.
        if active && pressed(Key::Escape) {
            prefs.escape();
            sound.play(Cue::UiBeep);
            held_controls.disarm(&keys_down);
            last = now;
        }
        // While the pages are open the game is frozen: only the page state and
        // the page picture change. The original stops the clock in FUNC_000220.
        if prefs.open {
            let before = prefs.clone();
            if active {
                let key = [
                    (Key::Up, PrefsKey::Up),
                    (Key::Down, PrefsKey::Down),
                    (Key::Left, PrefsKey::Left),
                    (Key::Right, PrefsKey::Right),
                    (Key::Enter, PrefsKey::Enter),
                    (Key::Backspace, PrefsKey::Backspace),
                ]
                .into_iter()
                .find(|(key, _)| pressed(*key))
                .map(|(_, action)| action);
                if let Some(key) = key {
                    if prefs.key(key) == Activate::Quit {
                        return Ok(());
                    }
                }
            }
            if let Some((x, y)) = edge_point {
                match prefs.click(x, y) {
                    Activate::Quit => return Ok(()),
                    Activate::Close => {
                        held_controls.disarm(&keys_down);
                        message = "Returned to the game.".to_owned();
                    }
                    Activate::None => {}
                }
            }
            // F150/F216: a page action beeps and any Sound Effects change is
            // applied to the player immediately.
            if prefs != before {
                sound.apply_prefs(&prefs);
                sound.play(Cue::UiBeep);
            }
            // Consume the mouse-down edge while the pages are open; the page
            // loop returns before the flight loop's edge bookkeeping.
            mouse_was_down = mouse_down;
            screen.draw_prefs(&prefs);
            window
                .update_with_buffer(&screen.pixels, WIDTH, HEIGHT)
                .map_err(|e| format!("Window update: {e}"))?;
            frames += 1;
            last = Instant::now();
            continue;
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
        // A click on a docked screen's button (F328 hot areas) on the mouse-down edge.
        let dock_click = if active
            && mouse_down
            && !mouse_was_down
            && state.mode == ffe_rs::flight::Mode::Docked
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
        if console_click == Some(Click::Pause) {
            paused = !paused;
            held_controls.disarm(&keys_down);
            message = if paused { "Paused." } else { "Resumed." }.to_owned();
        }
        // F2 is the original's status screen: F1120's ship diagram and equipment
        // page on the console. The key and its console button both toggle it.
        if active
            && state.mode == ffe_rs::flight::Mode::Flight
            && !help
            && !scenarios
            && model_lab.is_none()
            && model_viewer.is_none()
            && (pressed(Key::F2) || console_click == Some(Click::Screen(2)))
        {
            status_page = !status_page;
            held_controls.disarm(&keys_down);
            message = if status_page {
                "Status: ship systems (F2 returns)."
            } else {
                "Flight console."
            }
            .to_owned();
        }
        // Every console button the port has no screen for says so in the message
        // log instead of doing nothing.
        if console_click.is_some() {
            sound.play(Cue::UiBeep);
        }
        if let Some(note) = console_click.and_then(Click::unported) {
            message = note.to_owned();
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
                prefs.modern = !prefs.modern;
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
                if pressed(Key::Tab) {
                    toggle(nav, NavView::System);
                    held_controls.disarm(&keys_down);
                }
                // The console's navigation-computer icon (`n`): the original's
                // list of the system's stations and starports. The key is `n`
                // (action 0x6e, ffe.asm 78415/78537).
                if pressed(Key::N) || console_click == Some(Click::Navigation) {
                    toggle(nav, NavView::Places);
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
                    // The ffe-rs system map and galaxy list take the arrow keys; the
                    // original navigation computer is a mouse list (its rows are hot
                    // areas 0xd1..0xe2), so it has no selection keys.
                    if nav.view != NavView::Places {
                        if pressed(Key::Up) && galaxy.is_none() {
                            nav.step_selection(-1);
                        }
                        if pressed(Key::Down) && galaxy.is_none() {
                            nav.step_selection(1);
                        }
                    }
                    if matches!(nav.view, NavView::System | NavView::Places) {
                        if pressed(Key::Right) || pressed(Key::PageDown) {
                            nav.turn_page(1);
                        }
                        if pressed(Key::Left) || pressed(Key::PageUp) {
                            nav.turn_page(-1);
                        }
                    }
                    let mut enter = pressed(Key::Enter) && nav.view != NavView::Places;
                    if let Some((x, y)) = click_point.filter(|_| galaxy.is_none()) {
                        let count = nav.lines(ship_position(&original, &state)).len();
                        let row = match nav.view {
                            NavView::System => Screen::nav_row(x, y).filter(|row| *row < count),
                            NavView::Places => Screen::place_row(x, y).filter(|row| *row < count),
                            _ => Screen::map_row(nav.selected, count, x, y),
                        };
                        if let Some(row) = row {
                            nav.selected = row;
                            // A click on a body of this system engages the autopilot; a
                            // click on a system selects it (H hyperspaces).
                            if matches!(nav.view, NavView::System | NavView::Places) {
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
                    let engage = enter && matches!(nav.view, NavView::System | NavView::Places);
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
                                    let forward = ship_axes(&state)[2];
                                    nav.spawn_traffic(&cache, jumped.arrival, forward);
                                    // F908's `0x21`; F656's song 15 when the
                                    // "Music after hyperspace" switch is on.
                                    sound.play(Cue::Hyperspace);
                                    sound.hyperspace_music();
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
                    let ship = ship_position(&original, &state);
                    if pressed(Key::T) || console_click == Some(Click::Target) {
                        let back = window.is_key_down(Key::LeftShift)
                            || window.is_key_down(Key::RightShift);
                        let combat_id = if back { nav.ship_lock } else { None };
                        if let Some(id) = combat_id {
                            // Shift+T selects the combat target (DATA_008875),
                            // the one F1096/F941 launch a missile at.
                            let name = nav
                                .ships
                                .iter()
                                .find(|ship| ship.id == id)
                                .map_or_else(String::new, |ship| ship.name.clone());
                            nav.missile_lock = if nav.missile_lock == Some(id) {
                                None
                            } else {
                                Some(id)
                            };
                            message = if nav.missile_lock.is_some() {
                                format!("Missile target locked: {name}. M launches.")
                            } else {
                                "Missile target lock off.".to_owned()
                            };
                        } else if nav.ship_lock.is_some() {
                            if let Some(ship) = nav.cycle_ship_lock(if back { -1 } else { 1 }) {
                                message = format!(
                                    "Locked on {} {}. Space fires; a ship cannot be flown to.",
                                    ship.name,
                                    describe(distance(
                                        ship_position(&original, &state),
                                        ship.position
                                    ))
                                );
                            }
                        } else if let Some(body) = nav.cycle_lock(ship, if back { -1 } else { 1 }) {
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
                        let body_count = nav.bodies.len();
                        let mut candidates: Vec<(usize, [f64; 3], f64)> = nav
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
                        candidates.extend(nav.ships.iter().enumerate().filter_map(
                            |(index, ship)| {
                                ship.visible().then_some((
                                    body_count + index,
                                    ship.position.map(|v| v as f64 / unit),
                                    ship.radius as f64 / unit,
                                ))
                            },
                        ));
                        let hit = Screen::pick(&state, &views, scene.as_ref(), &candidates, point);
                        if let Some(index) = hit {
                            if index < body_count {
                                nav.ship_lock = None;
                                if let Some(body) = nav.lock(index) {
                                    sound.play(Cue::UiBeep);
                                    message = format!(
                                        "Locked on {} {}. K or AUTOPILOT flies there.",
                                        body.name,
                                        describe(distance(ship, body.position))
                                    );
                                }
                            } else {
                                nav.locked = None;
                                let id = nav.ships[index - body_count].id;
                                if let Some(ship) = nav.lock_ship(id) {
                                    sound.play(Cue::UiBeep2);
                                    message = format!(
                                        "Locked on {} {}. Space fires.",
                                        ship.name,
                                        describe(distance(
                                            ship_position(&original, &state),
                                            ship.position
                                        ))
                                    );
                                }
                            }
                        }
                    }
                    if pressed(Key::M) {
                        // F1054's 'm': the first non-zero pylon in
                        // ship+0xd6..+0xdf; F1096 requires DATA_008875 (the
                        // combat target) and calls F941.
                        match (state.combat.first_missile(), nav.missile_lock) {
                            (None, _) => message = "No missiles left.".to_owned(),
                            (Some(_), None) => {
                                message =
                                    "No missile target: lock a ship and press Shift+T.".to_owned()
                            }
                            (Some((pylon, code)), Some(id)) => {
                                let target = nav
                                    .ships
                                    .iter()
                                    .find(|ship| ship.id == id && ship.alive())
                                    .map(|ship| ship.name.clone());
                                match target {
                                    None => message = "The missile target is gone.".to_owned(),
                                    Some(name) => {
                                        let model = ffe_core::missile::model(code).unwrap_or(0);
                                        let stats = scene
                                            .as_ref()
                                            .and_then(|scene| scene.ship_stats(model));
                                        let missile_radius = ffe_core::objects::ModelInfo::read(
                                            model_records,
                                            model_stats,
                                            model,
                                        )
                                        .map_or(0, |info| info.radii().1);
                                        let launcher_radius = ffe_core::objects::ModelInfo::read(
                                            model_records,
                                            model_stats,
                                            state.ship,
                                        )
                                        .map_or(0, |info| info.radii().1);
                                        if let Some(spec) = ffe_rs::missile::MissileField::spec(
                                            code,
                                            stats,
                                            missile_radius,
                                        ) {
                                            let axes = ship_axes(&state);
                                            let position = ship_position(&original, &state);
                                            if state.combat.take_missile(pylon).is_some() {
                                                nav.launch_missile(
                                                    spec,
                                                    code,
                                                    0,
                                                    id,
                                                    position,
                                                    axes,
                                                    pylon,
                                                    launcher_radius,
                                                );
                                                message = format!("Missile away at {name}.");
                                            }
                                        } else {
                                            message = "Missile model unavailable.".to_owned();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if pressed(Key::K) || console_click == Some(Click::Autopilot) {
                        if nav.locked.is_none() {
                            message = if nav.ship_lock.is_some() {
                                "A ship cannot be followed by the autopilot.".to_owned()
                            } else {
                                "Lock a target first: click it in the view, or press T.".to_owned()
                            };
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
                    // F1056's launch action (its action 9) from a landed state:
                    // `0x34` goes straight to F1013's climb. In flight `L` is the
                    // original's text-label toggle instead.
                    let landed = nav.as_ref().and_then(|nav| nav.landed_state());
                    if pressed(Key::L) && landed.is_some() {
                        if let Some(nav) = nav.as_mut() {
                            match nav.begin_surface_launch() {
                                Ok(()) => {
                                    acceleration = 0;
                                    if let Some(model) = original.as_mut() {
                                        model.set_time_acceleration(0);
                                        // F1042's zero motion, then F1013's climb:
                                        // the up thrust is F1041's `0x24` request.
                                        model.begin_takeoff();
                                    }
                                    message = TAKEOFF_HINT.to_owned();
                                }
                                Err(why) => message = why,
                            }
                        }
                    } else if pressed(Key::L) || console_click == Some(Click::Labels) {
                        // `l` / the console labels icon: toggle the object name
                        // labels (`DATA_008804+0xe9` bit 2, ffe.asm 78447-78465).
                        labels = !labels;
                        message = if labels {
                            "Text labels on.".to_owned()
                        } else {
                            "Text labels off.".to_owned()
                        };
                    }
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
                        let changed = next != views.mode;
                        views.mode = next;
                        if changed {
                            // F1125: `0x27` camera on a view change.
                            sound.play(Cue::Camera);
                        }
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
                let mut dock_action: Option<StationAction> = None;
                let dock_before = dock;
                if state.mode == ffe_rs::flight::Mode::Docked
                    && !help
                    && !scenarios
                    && model_lab.is_none()
                    && model_viewer.is_none()
                {
                    let market_rows = if dock.screen == StationScreen::Market {
                        match nav
                            .as_mut()
                            .and_then(|nav| nav.docked.map(|station| (nav, station)))
                        {
                            Some((nav, station)) => trade::listing(nav, &cache, station, &state)
                                .map_or(0, |rows| rows.len()),
                            None => 0,
                        }
                    } else {
                        0
                    };
                    // F383's list for this starport: F869/F622's tech level
                    // (DATA_008995) and the current fitted loadout. It is rebuilt
                    // every frame, so a Fit/Remove updates the fitted flags.
                    let tech_level = nav
                        .as_ref()
                        .and_then(|nav| nav.tech_level(&cache))
                        .unwrap_or(0);
                    let upgrade_rows = if dock.screen == StationScreen::Upgrades {
                        station::upgrades(tech_level, &state.loadout, &state.combat.lasers)
                    } else {
                        Vec::new()
                    };
                    let upgrade_fitted: Vec<bool> =
                        upgrade_rows.iter().map(|row| row.fitted).collect();
                    let repair_rows = if dock.screen == StationScreen::Repairs {
                        station_repairs(&state, scene.as_ref())
                    } else {
                        Vec::new()
                    };
                    let rows = station::row_count(
                        dock.screen,
                        0,
                        market_rows,
                        upgrade_rows.len(),
                        repair_rows.len(),
                    );
                    let step = i32::from(pressed(Key::Down)) - i32::from(pressed(Key::Up));
                    if step != 0 {
                        dock.step(step, rows);
                    }
                    // F354: keys 1..4 are actions 0x31..0x34, one per visible row.
                    for (key, number) in [
                        (Key::Key1, 1usize),
                        (Key::Key2, 2),
                        (Key::Key3, 3),
                        (Key::Key4, 4),
                    ] {
                        if pressed(key) {
                            let index = dock.scroll + number - 1;
                            if index < rows {
                                dock.selected = index;
                                if dock.screen == StationScreen::Market {
                                    // A number selects the market line; B/N trade it.
                                } else if dock.screen == StationScreen::Upgrades {
                                    // F354's digit action is the left (Fit) button.
                                    dock_action = Some(StationAction::Fit { row: index });
                                } else if dock.screen == StationScreen::Repairs {
                                    dock_action = Some(StationAction::NotPorted(
                                        "Repairs and servicing actions are not ported.",
                                    ));
                                } else if let Some(entry) =
                                    station::menu_entry(dock.screen, index, 0)
                                {
                                    dock_action = Some(entry.action);
                                }
                            }
                        }
                    }
                    if pressed(Key::Enter)
                        && dock.screen != StationScreen::Market
                        && dock.selected < rows
                    {
                        if dock.screen == StationScreen::Upgrades {
                            dock_action = Some(StationAction::Fit { row: dock.selected });
                        } else if dock.screen == StationScreen::Repairs {
                            dock_action = Some(StationAction::NotPorted(
                                "Repairs and servicing actions are not ported.",
                            ));
                        } else if let Some(entry) =
                            station::menu_entry(dock.screen, dock.selected, 0)
                        {
                            dock_action = Some(entry.action);
                        }
                    }
                    if pressed(Key::Backspace) {
                        dock_action = Some(StationAction::Back);
                    }
                    if let Some((x, y)) = dock_click {
                        if let Some(hot) = station::click_with_fitted(
                            dock.screen,
                            dock.scroll,
                            rows,
                            Some(&upgrade_fitted),
                            x,
                            y,
                        ) {
                            match hot {
                                station::Hot::Row(index) => {
                                    if dock.screen == StationScreen::Repairs {
                                        dock_action = Some(StationAction::NotPorted(
                                            "Repairs and servicing actions are not ported.",
                                        ));
                                    } else if let Some(entry) =
                                        station::menu_entry(dock.screen, index, 0)
                                    {
                                        dock_action = Some(entry.action);
                                    }
                                }
                                station::Hot::Market { buy, row } => {
                                    dock.selected = row;
                                    dock_action = Some(StationAction::Trade { buy, row });
                                }
                                station::Hot::Outfit { remove, row } => {
                                    dock.selected = row;
                                    dock_action = Some(if remove {
                                        StationAction::Remove { row }
                                    } else {
                                        StationAction::Fit { row }
                                    });
                                }
                                station::Hot::More => {
                                    let visible = station::layout(dock.screen).visible();
                                    dock.scroll += visible;
                                    dock.scroll = dock.scroll.min(rows.saturating_sub(visible));
                                    dock.selected = dock.scroll;
                                }
                                station::Hot::Up => {
                                    dock.scroll = dock.scroll.saturating_sub(1);
                                    dock.selected = dock.scroll;
                                }
                                station::Hot::Down => {
                                    let visible = station::layout(dock.screen).visible();
                                    dock.scroll = dock.scroll.saturating_add(1);
                                    dock.scroll = dock.scroll.min(rows.saturating_sub(visible));
                                    dock.selected = dock.scroll;
                                }
                                station::Hot::Back => {
                                    dock_action = Some(StationAction::Back);
                                }
                            }
                        }
                    }
                }
                let ship_step = i32::from(pressed(Key::PageDown)) - i32::from(pressed(Key::PageUp));
                if ship_step != 0 {
                    message = match ships.step(state.ship, ship_step) {
                        Some(ship) => {
                            state.ship = ship;
                            // The port has no outfitting: the new-game front laser
                            // stays fitted while the hull has a gun mount, and is
                            // removed from a hull with none.
                            let mounts = scene
                                .as_ref()
                                .and_then(|scene| scene.ship_stats(ship))
                                .map_or(0, |stats| stats.gun_mounts);
                            if mounts <= 0 {
                                state.combat.lasers = [0; 4];
                            }
                            // FUNC_001110 re-applies the closest-camera limit.
                            let minimum = scene
                                .as_ref()
                                .and_then(|scene| scene.camera_minimum(ship))
                                .unwrap_or(0);
                            views.external.distance =
                                views.external.distance.min(minimum.saturating_neg());
                            // ffe-rs: a ship without turrets leaves a turret view.
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
                if let Some(station_action) = dock_action {
                    match station_action {
                        StationAction::Launch => {
                            if let Some(nav) = nav.as_mut() {
                                match nav.begin_launch() {
                                    Ok(()) if nav.takeoff.is_some() => {
                                        // F381 leaves the open-air starport's
                                        // ship on its pad; the launch button
                                        // takes it to 0x24, so flight starts
                                        // here and the next frame's launch
                                        // block places the model on the pad.
                                        acceleration = 0;
                                        state.mode = ffe_rs::flight::Mode::Flight;
                                        state.system_flight = true;
                                        // F1018's launch-transition beep.
                                        sound.stop_for_sequence();
                                        sound.play(Cue::UiBeep);
                                        message = TAKEOFF_HINT.to_owned();
                                    }
                                    Ok(()) => {
                                        acceleration = 0;
                                        // F1048 stops the song; F1098's launch
                                        // sample comes at the end of the lift
                                        // sequence (`Traffic::LaunchDone`).
                                        sound.stop_for_sequence();
                                        message =
                                            "Launch permission granted: moving into the lift."
                                                .to_owned();
                                    }
                                    Err(error) => message = error,
                                }
                            }
                        }
                        StationAction::Open(screen) => {
                            dock.open(screen);
                            message = station::hint(screen, 0).to_owned();
                        }
                        StationAction::Back => {
                            dock.back();
                            message = station::hint(dock.screen, 0).to_owned();
                        }
                        StationAction::Fit { row } => {
                            if let Some(nav) = nav.as_ref() {
                                message = fit_or_remove(
                                    nav,
                                    &cache,
                                    &mut state,
                                    scene.as_ref(),
                                    row,
                                    false,
                                );
                            }
                        }
                        StationAction::Remove { row } => {
                            if let Some(nav) = nav.as_ref() {
                                message = fit_or_remove(
                                    nav,
                                    &cache,
                                    &mut state,
                                    scene.as_ref(),
                                    row,
                                    true,
                                );
                            }
                        }
                        StationAction::Trade { buy, row } => {
                            let station = nav.as_ref().and_then(|nav| nav.docked);
                            let shift = window.is_key_down(Key::LeftShift)
                                || window.is_key_down(Key::RightShift);
                            if let (Some(nav), Some(station)) = (nav.as_mut(), station) {
                                if let Some(good) = trade::listing(nav, &cache, station, &state)
                                    .ok()
                                    .and_then(|rows| rows.get(row).map(|line| line.good))
                                {
                                    match trade::trade(
                                        nav,
                                        &cache,
                                        &mut state,
                                        station,
                                        good,
                                        buy,
                                        if shift { 10 } else { 1 },
                                    ) {
                                        Ok(text) => message = text,
                                        Err(error) => message = format!("Trade failed: {error}"),
                                    }
                                }
                            }
                        }
                        StationAction::NotPorted(text) => message = text.to_owned(),
                    }
                }
                // F1056: station-screen navigation and buttons beep.
                if dock_action.is_some() || dock != dock_before {
                    sound.play(Cue::UiBeep);
                }
                let action = if pressed(Key::L) && state.mode == ffe_rs::flight::Mode::Docked {
                    match nav.as_mut() {
                        Some(nav) if state.mode == ffe_rs::flight::Mode::Docked => {
                            match nav.begin_launch() {
                                Ok(()) if nav.takeoff.is_some() => {
                                    acceleration = 0;
                                    state.mode = ffe_rs::flight::Mode::Flight;
                                    state.system_flight = true;
                                    sound.stop_for_sequence();
                                    sound.play(Cue::UiBeep);
                                    message = TAKEOFF_HINT.to_owned();
                                    None
                                }
                                Ok(()) => {
                                    acceleration = 0;
                                    sound.stop_for_sequence();
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
                    let ship = ship_position(&original, &state);
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
                    // F1020's docking-computer beep.
                    sound.play(Cue::UiBeep);
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
                                && dock.screen == StationScreen::Market
                                && nav.market_tables().is_some() =>
                        {
                            let rows = trade::listing(nav, &cache, station, &state);
                            match rows.map(|rows| rows.get(dock.selected).map(|row| row.good)) {
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
            // The original's "Reverse left-right controls" and "Reverse up-down
            // controls" (DATA_009021/DATA_009022, toggled by FUNC_000216).
            let reverse_lr = prefs.others[3];
            let reverse_ud = prefs.others[4];
            let flip = |value: f64, reverse: bool| if reverse { -value } else { value };
            // In the external view the arrows move the camera, so Modern steering
            // falls back to the Classic-like keys there.
            let external_view = views.mode == ViewMode::External;
            let input = if views.mode.is_turret() {
                // The steering keys aim the turret (FUNC_001038).
                Controls {
                    thrust: if prefs.modern {
                        axis(Key::W, Key::S)
                    } else {
                        axis(Key::Enter, Key::LeftShift)
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    brake: held_controls.contains(&Key::X),
                }
            } else if prefs.modern && !external_view {
                Controls {
                    thrust: axis(Key::W, Key::S),
                    yaw: flip(axis(Key::Right, Key::Left), reverse_lr),
                    pitch: flip(axis(Key::Up, Key::Down), reverse_ud),
                    brake: held_controls.contains(&Key::X),
                }
            } else if prefs.modern {
                Controls {
                    thrust: axis(Key::W, Key::S),
                    yaw: flip(axis(Key::Period, Key::Comma), reverse_lr),
                    pitch: flip(axis(Key::A, Key::Z), reverse_ud),
                    brake: held_controls.contains(&Key::X),
                }
            } else {
                Controls {
                    thrust: axis(Key::Enter, Key::LeftShift),
                    yaw: flip(axis(Key::Period, Key::Comma), reverse_lr),
                    pitch: flip(axis(Key::A, Key::Z), reverse_ud),
                    brake: held_controls.contains(&Key::X),
                }
            };
            if let Some(model) = original.as_mut() {
                let down = |keys: &[Key]| keys.iter().any(|key| held_controls.contains(key));
                // Classic: the original's keys. Modern adds W/S for the set speed, the arrows
                // for pitch (up is nose up, which is Z) and yaw, Q/E for roll.
                let both =
                    |classic: &[Key], extra: &[Key]| down(classic) || (prefs.modern && down(extra));
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
                if reverse_lr {
                    std::mem::swap(&mut keys.left, &mut keys.right);
                }
                if reverse_ud {
                    std::mem::swap(&mut keys.pitch_down, &mut keys.pitch_up);
                }
                // The scripted launch capture flies the takeoff itself: F1013's
                // fixed up-thrust does the first climb, then it pitches and
                // accelerates through F1031's 0x26 and the short free-flight
                // tail so the captured frames do not fall back to the planet.
                if capture_takeoff {
                    let climbing = nav.as_ref().is_some_and(|nav| {
                        nav.takeoff_state() == Some(ffe_core::traffic::state::TAKEOFF)
                    });
                    keys.pitch_up = !climbing;
                    keys.faster = model.set_speed_metres() < 500.0;
                }
                let mut command_face = None;
                if nav.as_ref().is_some_and(|nav| nav.holding()) {
                    keys = OriginalKeys::default();
                    model.teleport(model.player().motion.position);
                }
                if let Some(nav) = nav.as_mut().filter(|nav| !nav.holding()) {
                    // A landed ship has no autopilot input (F1012 runs no F1037
                    // and the target is cleared by F1021); the clock and the
                    // body's motion still carry it.
                    if nav.landed.is_none() {
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
                                message = format!(
                                    "Autopilot: docking at {} {}.",
                                    body.name,
                                    describe(range)
                                );
                            } else {
                                let range = distance(model.player().motion.position, body.position);
                                message = format!("Autopilot: {} {}.", body.name, describe(range));
                            }
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
                    // The planet capture continues to the nearest open-air
                    // starport once the planet's standoff is reached, so the saved
                    // frames show the buildings on the surface.
                    if options.capture_planet.is_some() && !capture_port && nav.target.is_none() {
                        let from = model.player().motion.position;
                        let port = nav
                            .bodies
                            .iter()
                            .enumerate()
                            .filter(|(_, body)| body.kind == NavKind::Starport)
                            .min_by(|a, b| {
                                distance(from, a.1.position)
                                    .total_cmp(&distance(from, b.1.position))
                            })
                            .map(|(index, _)| index);
                        if let Some(index) = port {
                            nav.locked = Some(index);
                            nav.engage_locked();
                            capture_port = true;
                            message = format!("Capture: starport {}.", nav.bodies[index].name);
                        }
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
                let ship_at = model.absolute_position();
                let player_info =
                    ffe_core::objects::ModelInfo::read(model_records, model_stats, state.ship);
                // The original's landed handlers (F1011/F1012) run no flight
                // step: no F1037/F1041, no F601/F603/F1512, and F1039 only sees
                // the turret's input. The view (F1050/F1054) and F1056's input
                // still run, and the ship's position is the one the contact
                // settled it at.
                let landed_state = nav.as_ref().and_then(|nav| nav.landed_state());
                let landing = if model.on_pad() || landed_state.is_some() {
                    None
                } else {
                    nav.as_ref()
                        .filter(|_| state.system_flight)
                        .and_then(|nav| {
                            let info = player_info?;
                            let index = nav.landing_parent(ship_at, info.radii().0)?;
                            let body = nav.bodies.get(index)?;
                            let parent_info = ffe_core::objects::ModelInfo::read(
                                model_records,
                                model_stats,
                                body.model,
                            )?;
                            Some((
                                index,
                                ffe_rs::navigation::landing_body(body, &parent_info),
                                body.position,
                                info,
                            ))
                        })
                };
                let mut landed_at = None;
                let step_result = if let Some(mode) = landed_state {
                    model.set_mode(mode);
                    Ok(None)
                } else if model.on_pad() {
                    model
                        .step_pad(elapsed, &keys, landing_tools.as_ref())
                        .map(Some)
                } else if let Some((index, parent, at, info)) = landing {
                    model
                        .step_landing(elapsed, &keys, &parent, at, &info, landing_tools.as_ref())
                        .map(|contact| {
                            if contact.flags & 0x40 != 0 {
                                landed_at = Some(index);
                                message = "Landed. The ship is settled on the surface.".to_owned();
                                // F1088's docking/landing transition, then
                                // F1094's docking music.
                                sound.play(Cue::Docking);
                                sound.docking_music();
                            } else if contact.flags & 0x80 != 0 {
                                message = format!(
                                    "Crash: damage {}. The ship bounced off the surface.",
                                    contact.damage.max(0)
                                );
                                sound.play(Cue::HullDamage);
                            }
                            Some(contact)
                        })
                } else {
                    model.step(elapsed, &keys).map(|()| None)
                };
                match step_result {
                    Ok(contact) => {
                        model.apply(&mut state);
                        // F1021's rough landing (`0x32`) then F1047's view frame
                        // (`0x34`): the ship is settled, its motion is F1042's
                        // zero and the surface view is the flight view from there.
                        if let Some(index) = landed_at {
                            if let Some(nav) = nav.as_mut() {
                                if nav.land(index) {
                                    model.settle_landing();
                                    if let Some(mode) = nav.landing_frame() {
                                        model.set_mode(mode);
                                    }
                                    model.apply(&mut state);
                                    message = format!(
                                        "Landed on {}. The surface view is ahead; L launches.",
                                        nav.bodies[index].name
                                    );
                                }
                            }
                        }
                        // F1013/F1031: the open-air starport takeoff's mode
                        // word follows the contact scan and the set speed.
                        if let Some(contact) = contact {
                            if let Some(nav) = nav.as_mut().filter(|nav| nav.takeoff.is_some()) {
                                // F1013 reads DATA_008867. A complex starport
                                // shape leaves it at 0x7fffffff, so measure the
                                // climb from the pad instead (the distance the
                                // ship has risen along the pad's up row).
                                let altitude = if contact.altitude() == i32::MAX {
                                    takeoff_climb.map_or(i32::MAX, |(origin, up)| {
                                        let at = model.absolute_position();
                                        (0..3)
                                            .map(|k| (at[k] - origin[k]) as f64 * up[k])
                                            .sum::<f64>()
                                            .clamp(0.0, f64::from(i32::MAX))
                                            as i32
                                    })
                                } else {
                                    contact.altitude()
                                };
                                match nav.takeoff_step(
                                    altitude,
                                    model.set_speed_units(),
                                    contact.flags,
                                ) {
                                    Some(ffe_core::traffic::TakeoffStep::HandOff) => {
                                        if let Some(mode) = nav.takeoff_state() {
                                            model.set_mode(mode);
                                        }
                                        message = TAKEOFF_HINT.to_owned();
                                    }
                                    Some(ffe_core::traffic::TakeoffStep::Cleared) => {
                                        model.set_mode(ffe_core::traffic::state::FREE);
                                        // F573/F1677: back to the system frame. A
                                        // surface launch never built the pad world.
                                        if model.on_pad() {
                                            model.end_pad_takeoff();
                                        }
                                        model.apply(&mut state);
                                        takeoff_climb = None;
                                        message = "Clear of the surface: free flight.".to_owned();
                                    }
                                    Some(ffe_core::traffic::TakeoffStep::Retakeoff) => {
                                        model.begin_takeoff();
                                        message = "Back on the pad: lifting off again.".to_owned();
                                    }
                                    None => {}
                                }
                                if capture_takeoff
                                    && nav.takeoff.is_none()
                                    && takeoff_done_at.is_none()
                                {
                                    takeoff_done_at = Some(frames);
                                }
                            }
                        }
                        // The stations: the slot docks a cleared ship, the hull pushes it clear.
                        if let Some(nav) = nav.as_mut().filter(|_| state.system_flight) {
                            let ship = model.player().motion.position;
                            if let Some(index) = launch_grace {
                                let body = &nav.bodies[index];
                                let clear = nav
                                    .port(index)
                                    .map_or(body.radius as f64, |port| port.front * 1.1 + 1.0e3);
                                if distance(ship, body.position) >= clear {
                                    launch_grace = None;
                                }
                            }
                            if launch_grace.is_none() && nav.takeoff.is_none() {
                                if let Some((index, contact)) =
                                    nav.contact(ship, state.speed() * 9.7)
                                {
                                    let name = nav.bodies[index].name.clone();
                                    match contact {
                                        Contact::None => {}
                                        Contact::Docked => {
                                            if nav.begin_docking(index) {
                                                acceleration = 0;
                                                model.set_time_acceleration(0);
                                                // F1019/F1048: the airlock cue and
                                                // the sequence stops the song.
                                                sound.stop_for_sequence();
                                                sound.play(Cue::Docking);
                                                message = format!(
                                                    "{name}: docking, aligning with the lift."
                                                );
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
                            // F1026/F1029's airlock `0x12`/`0x13` on the door
                            // stages (the other stages have no cue).
                            if text.contains("door") {
                                sound.play(Cue::Door);
                            }
                        }
                        Traffic::Docked(_) => {
                            state.mode = ffe_rs::flight::Mode::Docked;
                            state.position = [0.0, 0.0, -60.0];
                            state.velocity = [0.0; 3];
                            state.thrust = 0.0;
                            state.docking_requested = false;
                            dock.open(StationScreen::Services);
                            // F1088/F1094: the docking cue, the station bed and
                            // a random docking song.
                            sound.play(Cue::Docking);
                            sound.play(Cue::Station);
                            sound.docking_music();
                            message = station::SERVICES_HINT.to_owned();
                        }
                        Traffic::LaunchOut(index) => {
                            // F1088 sets the station's door opening; the ship is
                            // still in the lift until it is open (LaunchDone). Keep
                            // the docked/inside view, not the flight view.
                            message = format!("Launching: leaving {}.", nav.bodies[index].name);
                            sound.play(Cue::Door);
                        }
                        Traffic::LaunchDone(index) => {
                            // F1088/F1089: the door is open and the ship leaves.
                            state.mode = ffe_rs::flight::Mode::Flight;
                            state.docking_requested = false;
                            state.system_flight = true;
                            launch_grace = Some(index);
                            // F1098's `0x20` launch, F1092's engine bed and
                            // F1094's random launch music.
                            sound.play(Cue::Launch);
                            sound.play(Cue::Engine);
                            sound.docking_music();
                            message = format!("Clear of {}.", nav.bodies[index].name);
                            // F688 spawns the system traffic on entering it;
                            // the sandbox has no new game, so the launch is its
                            // system-entry point.
                            let player = ship_position(&original, &state);
                            let forward = ship_axes(&state)[2];
                            nav.spawn_traffic(&cache, player, forward);
                        }
                    }
                }
            }
        }
        // The system's NPC traffic: the flight/combat AI (F752/F725/F585, F931),
        // ticked in game time like the stations' doors. A hostile hit runs F953 on
        // the player's shields and hull. Ships only fly and fire in flight: inside
        // a station the player's object is not in the system.
        if active
            && !help
            && !paused
            && model_lab.is_none()
            && model_viewer.is_none()
            && state.mode == ffe_rs::flight::Mode::Flight
        {
            if let Some(nav) = nav.as_mut() {
                let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
                    / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
                let player = ship_position(&original, &state);
                let tics = (elapsed * ffe_rs::original_flight::TICS_PER_SECOND * factor) as u32;
                // Inside a docking or launch sequence the player's object is at
                // the station: no NPC shot can reach it (F817's box would have
                // nothing to test).
                let radius = if nav.sequence.is_some() {
                    0
                } else {
                    scene
                        .as_ref()
                        .and_then(|scene| scene.model_radius(state.ship))
                        .map_or(10_000, i64::from)
                };
                let events = nav.tick_ships(elapsed * factor, tics, player, radius);
                let mass = scene
                    .as_ref()
                    .and_then(|scene| scene.ship_stats(state.ship))
                    .map_or(25, |stats| stats.mass);
                for event in events {
                    match event {
                        ffe_rs::navigation::ShipEvent::Hit {
                            name, damage, beam, ..
                        } => {
                            let outcome = state.take_damage(damage, mass);
                            let laser = if beam { "beam laser" } else { "pulse laser" };
                            message = match outcome {
                                ffe_core::laser::Damage::Shields => {
                                    let percent = if state.shields[1] == 0 {
                                        0
                                    } else {
                                        state.shields[0] as i32 * 100 / i32::from(state.shields[1])
                                    };
                                    format!(
                                        "{name} hits with its {laser}: shields {percent}% left."
                                    )
                                }
                                ffe_core::laser::Damage::Hull => {
                                    let per_mille =
                                        state.hull[0] as i32 * 250 / i32::from(mass.max(1));
                                    format!(
                                        "{name} hits with its {laser}: hull {}.{}%.",
                                        per_mille / 10,
                                        per_mille % 10
                                    )
                                }
                                ffe_core::laser::Damage::Destroyed => {
                                    playership_destroyed(&mut state, &mut original, nav, mass);
                                    format!("{name} destroys your ship. Recovered at the starport.")
                                }
                            };
                        }
                        ffe_rs::navigation::ShipEvent::PoliceHostile { name } => {
                            message = format!("{name} is police and turns on you. You are wanted.");
                        }
                        ffe_rs::navigation::ShipEvent::Escaped { name } => {
                            message = format!("{name} jumps out.");
                        }
                    }
                }
                for event in nav.tick_missiles(elapsed * factor, player) {
                    match event {
                        ffe_rs::missile::MissileEvent::Hit { name, outcome, .. } => {
                            message = match outcome {
                                ffe_core::laser::Damage::Shields => {
                                    let left = nav
                                        .ships
                                        .iter()
                                        .find(|ship| ship.name == name)
                                        .map_or(0, |ship| ship.shield_percent());
                                    format!("{name}: missile absorbed by shields ({left}% left).")
                                }
                                ffe_core::laser::Damage::Hull => {
                                    let hull = nav
                                        .ships
                                        .iter()
                                        .find(|ship| ship.name == name)
                                        .map_or(0, |ship| ship.hull_per_mille() / 10);
                                    format!("{name}: missile hit, hull {hull}%.")
                                }
                                ffe_core::laser::Damage::Destroyed => {
                                    format!("{name} destroyed by missile.")
                                }
                            }
                        }
                        ffe_rs::missile::MissileEvent::Expired { .. } => {}
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
            labels,
            navigation: nav.as_ref().is_some_and(|nav| nav.view == NavView::Places),
            status_page,
        };
        // Launching from the station puts the ship out from it in this system, on the
        // original flight model (which the navigation moves).
        if last_mode == ffe_rs::flight::Mode::Docked
            && state.mode == ffe_rs::flight::Mode::Flight
            && nav.as_ref().is_some_and(|nav| nav.takeoff.is_some())
        {
            // An open-air starport: F994's pad seat, the station's rows as the
            // ship's orientation (F994 copies them), and F1042's zero motion
            // before F1013's climb. The ship is not put out 20 km; it leaves
            // the pad under its own thrust and F1031 releases the bay once the
            // starport's effect region is behind it.
            let stats = scene
                .as_ref()
                .and_then(|scene| scene.ship_stats(state.ship));
            let player_info =
                ffe_core::objects::ModelInfo::read(model_records, model_stats, state.ship);
            let radius = player_info.map_or(0, |info| info.radii().1 as i32);
            if let Some((at, rows, station, parent_info)) = nav.as_ref().and_then(|nav| {
                nav.docked.and_then(|index| {
                    let body = &nav.bodies[index];
                    let parent_info =
                        ffe_core::objects::ModelInfo::read(model_records, model_stats, body.model)?;
                    Some((
                        nav.takeoff_seat(index, radius)?,
                        body.orientation,
                        body.clone(),
                        parent_info,
                    ))
                })
            }) {
                state.position =
                    at.map(|v| v as f64 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT);
                state.velocity = [0.0; 3];
                state.thrust = 0.0;
                state.axes = Some(rows);
                state.system_flight = true;
                takeoff_climb = Some((at, rows[1]));
                let mut model = OriginalFlight::from_flight(&state, stats.as_ref())
                    .with_time_acceleration(acceleration);
                if let Some(info) = player_info {
                    model.begin_takeoff();
                    // F570's frame switches persist while the ship is on the
                    // pad: a two-body world of the ship and the starport.
                    let parent_body = ffe_rs::navigation::landing_body(&station, &parent_info);
                    model.begin_pad_takeoff(&parent_body, station.position, &info);
                }
                model.apply(&mut state);
                original = Some(model);
                message = TAKEOFF_HINT.to_owned();
            }
        } else if last_mode == ffe_rs::flight::Mode::Docked
            && state.mode == ffe_rs::flight::Mode::Flight
        {
            let stats = scene
                .as_ref()
                .and_then(|scene| scene.ship_stats(state.ship));
            let mut model = OriginalFlight::from_flight(&state, stats.as_ref())
                .with_time_acceleration(acceleration);
            // F1088 copies the player's docked position (the station) to the ship:
            // the launch starts inside the station, at the open door.
            let at = nav.as_ref().and_then(|nav| {
                nav.docked
                    .and_then(|index| nav.launch_seat(index))
                    .or_else(|| nav.station().map(|(_, at)| at))
            });
            if let Some(at) = at {
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
                model.apply(&mut state);
                state.system_flight = true;
            }
            original = Some(model);
            message = if prefs.modern {
                "Launched. W/S speed, arrows pitch and yaw, Q/E roll; the map icons list systems."
            } else {
                "Launched. Enter/Shift speed, Z/A pitch, comma/period yaw, [ ] roll."
            }
            .to_owned();
        }
        // The scripted traffic capture leaves the pad immediately and spawns
        // F688's traffic at the starport's launch point, so the camera can be
        // placed beside a pirate. The target is held still so the 2 km camera
        // keeps it in frame through the hit and the kill; traffic otherwise
        // closes at PIRATE_SPEED.
        if options.capture_traffic.is_some() && !capture_placed {
            state.mode = ffe_rs::flight::Mode::Flight;
            state.system_flight = true;
            if let Some(nav) = nav.as_mut() {
                let launch = nav.station().map_or([0; 3], |(_, at)| at);
                let forward = ship_axes(&state)[2];
                nav.respawn_traffic(&cache, launch, forward);
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                let found = nav
                    .ships
                    .iter()
                    .position(|ship| ship.role == ffe_core::npc::Role::Pirate)
                    .or_else(|| (!nav.ships.is_empty()).then_some(0));
                if let Some(index) = found {
                    let target = nav.ships[index].position;
                    let ship = &mut nav.ships[index];
                    ship.state = ffe_core::npc::State::Hold;
                    ship.max_speed = 0.0;
                    ship.velocity = [0.0; 3];
                    let from_mm = [target[0], target[1], target[2] - 2_000_000];
                    let from = from_mm.map(|v| v as f64 / unit);
                    state.position = from;
                    state.velocity = [0.0; 3];
                    state.axes = Some(axes_looking_at(from, target.map(|v| v as f64 / unit)));
                    let id = nav.ships[index].id;
                    nav.lock_ship(id);
                    capture_placed = true;
                    original = None;
                }
            }
        }
        if last_mode != ffe_rs::flight::Mode::Docked && state.mode == ffe_rs::flight::Mode::Docked {
            dock.open(StationScreen::Services);
        }
        last_mode = state.mode;
        // Lasers (F1040/F931): Space holds the fire input, the current view picks
        // the mount, and F931's refire/heat/energy gates decide whether a shot
        // happens this frame. The beam is F1776's model 0x9c at the gun.
        let acceleration_factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
            / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
        let frame_tics =
            (elapsed * 50.0 * f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])) as u32;
        if active && !paused && state.mode == ffe_rs::flight::Mode::Flight {
            // F938 reads `ship+0xc8 & 1` (the laser cooling booster) directly.
            state.combat.cooling_booster = state.loadout.cooling_booster();
            state.combat.tick(frame_tics, elapsed * acceleration_factor);
        }
        screen.beam = None;
        let can_fire = active
            && !help
            && !paused
            && !scenarios
            && state.mode == ffe_rs::flight::Mode::Flight
            && model_lab.is_none()
            && model_viewer.is_none()
            && nav
                .as_ref()
                .is_none_or(|nav| nav.view == NavView::Closed && nav.sequence.is_none());
        if can_fire && window.is_key_down(Key::Space) {
            if let Some(mount) = combat::mount(views.mode) {
                match state.combat.fire(mount, frame_tics) {
                    Ok(shot) => {
                        // F930: `laser1` for banks 0..3, `laser2` for 4..15; the
                        // port's four mounts are all in the first bank.
                        sound.play(Cue::Laser { bank: mount as u8 });
                        let stats = scene
                            .as_ref()
                            .and_then(|scene| scene.ship_stats(state.ship));
                        let beam = combat::beam(
                            &state,
                            stats.as_ref(),
                            views.mode,
                            [views.turret.yaw, views.turret.pitch],
                        );
                        let origin = beam.origin.map(|v| {
                            (v * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT).round() as i64
                        });
                        let body_count = nav.as_ref().map_or(0, |nav| nav.bodies.len());
                        let targets: Vec<laser::Target> =
                            nav.as_ref().map_or_else(Vec::new, |nav| {
                                let mut targets: Vec<laser::Target> = nav
                                    .bodies
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, body)| body.radius > 0)
                                    .map(|(index, body)| laser::Target {
                                        index,
                                        centre: body.position,
                                        radius: body.radius,
                                    })
                                    .collect();
                                targets.extend(
                                    nav.ships
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, ship)| ship.visible())
                                        .map(|(index, ship)| ship.target(body_count + index)),
                                );
                                targets
                            });
                        if let Some(hit) = laser::hit(origin, beam.axes[2], &targets) {
                            if hit.index < body_count {
                                if let Some(nav) = nav.as_ref() {
                                    if let Some(body) = nav.bodies.get(hit.index) {
                                        if !shot.beam {
                                            message = format!(
                                                "{} hit: {}.",
                                                body.name,
                                                match shot.code {
                                                    laser::PULSE_1MW => "1MW pulse laser",
                                                    laser::PULSE_5MW => "5MW pulse laser",
                                                    laser::BEAM_1MW => "1MW beam laser",
                                                    laser::BEAM_4MW => "4MW beam laser",
                                                    laser::BEAM_20MW => "20MW beam laser",
                                                    laser::BEAM_100MW => "100MW beam laser",
                                                    laser::MINING_30MW => "30MW mining laser",
                                                    laser::THARGOID => "Thargoid laser",
                                                    _ => "laser",
                                                }
                                            );
                                        }
                                        // F953's hull/impact sample for a
                                        // non-ship target.
                                        sound.play(Cue::HullDamage);
                                    }
                                }
                            } else if let Some(nav) = nav.as_mut() {
                                let index = hit.index - body_count;
                                if let Some(ship) = nav.ships.get(index) {
                                    let id = ship.id;
                                    let name = ship.name.clone();
                                    let model = ship.model;
                                    nav.lock_ship(id);
                                    let outcome =
                                        nav.damage_ship_at(id, shot.damage, Some(hit.point));
                                    if let Some(attack) = outcome {
                                        state.credits = state.credits.saturating_add(attack.bounty);
                                        message = match attack.damage {
                                            ffe_core::laser::Damage::Shields => format!(
                                                "{name}: shields absorb the hit{}.",
                                                if attack.crime { ". You are wanted" } else { "" }
                                            ),
                                            ffe_core::laser::Damage::Hull => format!(
                                                "{name}: hull damaged.{}",
                                                if attack.crime { " You are wanted." } else { "" }
                                            ),
                                            ffe_core::laser::Damage::Destroyed => {
                                                if attack.bounty > 0 {
                                                    format!(
                                                        "{name} destroyed. Bounty {} CR.",
                                                        attack.bounty
                                                    )
                                                } else if attack.crime {
                                                    format!(
                                                        "{name} destroyed. You are wanted for murder."
                                                    )
                                                } else {
                                                    format!("{name} destroyed.")
                                                }
                                            }
                                        };
                                        if attack.police {
                                            message.push_str(" Police are responding.");
                                        }
                                    }
                                    // F750/F236: the first hit starts the battle
                                    // (siren + song 14 if enabled); F953/F942
                                    // then play the shield, hull or explosion
                                    // sample. F351's bong follows the message
                                    // log entry at the end of the frame.
                                    sound.start_battle();
                                    match outcome.map(|attack| attack.damage) {
                                        Some(ffe_core::laser::Damage::Shields) => {
                                            sound.play(Cue::ShieldHit);
                                        }
                                        Some(ffe_core::laser::Damage::Hull) => {
                                            sound.play(Cue::HullDamage);
                                        }
                                        Some(ffe_core::laser::Damage::Destroyed) => {
                                            sound.play(Cue::ShipExplosion);
                                            // F942: the `boom` for a hull whose
                                            // descriptor mass (`+0x6`) is >= 0xa.
                                            let large = scene
                                                .as_ref()
                                                .and_then(|scene| scene.ship_stats(model))
                                                .is_some_and(|stats| stats.mass >= 0x0a);
                                            if large {
                                                sound.play(Cue::LargeExplosion);
                                            }
                                        }
                                        None => {}
                                    }
                                    // F770: the last hostile's death ends the
                                    // battle and stops the song.
                                    if outcome.is_some_and(|attack| attack.destroyed)
                                        && !nav.ships.iter().any(|ship| {
                                            ship.alive() && ship.role == ffe_core::npc::Role::Pirate
                                        })
                                    {
                                        sound.end_battle();
                                    }
                                }
                            }
                        }
                        screen.beam = Some(beam);
                    }
                    Err(laser::FireError::Overheated) => {
                        message = "Lasers overheated.".to_owned();
                    }
                    Err(laser::FireError::NoEnergy) => {
                        message = "No energy for the lasers.".to_owned();
                    }
                    Err(laser::FireError::NoLaser | laser::FireError::Refire) => {}
                }
            }
        }
        screen.lasers = state.combat.lasers;
        screen.energy = state.combat.energy;
        screen.heat = state.combat.heat;
        screen.drive = state.combat.drive;
        // The scripted traffic capture fires on frame 10 (shield hit) and frame
        // 20 (kill) through the same F953/F942 words a player shot uses.
        if options.capture_traffic.is_some() && (frames == 10 || frames == 20) {
            if let Some(nav) = nav.as_mut() {
                if let Some(id) = nav.ship_lock {
                    let damage = if frames == 10 { 50 } else { 1000 };
                    nav.damage_ship(id, damage);
                }
            }
        }
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
        let landed_view = nav.as_ref().is_some_and(|nav| nav.landed.is_some());
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
                        // The approach draws the base sphere (inner); a settled
                        // ship draws the outer radius the landing contact uses,
                        // so it sits on the ground. See `surface_radius`.
                        radius: surface_radius(b, landed_view),
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
        // The system's stations are listed for every view: the launch draws them
        // from inside the station before the ship is in flight.
        screen.stations = match nav.as_ref() {
            Some(nav) => {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                let tics = ffe_core::model_animation::START_TICS.wrapping_add(
                    (nav.clock * ffe_rs::original_flight::TICS_PER_SECOND) as u64 as u32,
                );
                nav.bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| matches!(b.kind, NavKind::Station | NavKind::Starport))
                    .map(|(index, b)| ffe_rs::screen::SystemStation {
                        model: b.model,
                        centre: b.position.map(|v| v as f64 / unit),
                        orientation: b.orientation,
                        speed: b.spin,
                        door: nav.door(index),
                        tics,
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // F688's NPC ships: model, position and axes in flight space; a dead ship
        // keeps only F1536's flash marker (F942's particles are not ported).
        screen.ships = match nav.as_ref() {
            Some(nav) => {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                nav.ships
                    .iter()
                    .filter(|ship| ship.visible() || !ship.alive())
                    .map(|ship| ffe_rs::screen::SystemShip {
                        model: ship.model,
                        centre: ship.position.map(|v| v as f64 / unit),
                        axes: ship.axes(),
                        flash: ship.flash > 0.0,
                        dead: !ship.alive(),
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // F939/F942/F926: the system's hit particles, explosions and debris.
        screen.effects = match nav.as_ref() {
            Some(nav) => {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                nav.effects
                    .iter()
                    .map(|effect| ffe_rs::screen::SystemEffect {
                        model: effect.model,
                        centre: effect.position.map(|v| v as f64 / unit),
                        axes: effect.axes(),
                        mass: if effect.kind == ffe_core::effects::Kind::Explosion {
                            ffe_core::effects::EXPLOSION_MASS
                        } else {
                            0
                        },
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // F941's missile objects: the same renderer draws models 4..13.
        screen.missiles = match nav.as_ref() {
            Some(nav) => {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                nav.missiles
                    .missiles
                    .iter()
                    .filter(|missile| missile.alive())
                    .map(|missile| ffe_rs::screen::SystemMissile {
                        model: missile.spec.model,
                        centre: missile.position.map(|v| v as f64 / unit),
                        axes: missile.axes,
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // DATA_007247 (combat target) and DATA_007252 (F1113's blink).
        screen.missile_lock = nav.as_ref().is_some_and(|nav| nav.missile_lock.is_some());
        screen.missile_blink = nav.as_ref().is_some_and(|nav| {
            ((nav.clock * ffe_rs::original_flight::TICS_PER_SECOND / 49_710.0) as u64) & 1 == 0
        });
        // The labels toggle's object names (the original's F1108 reads the object's
        // +0x124 string): every body of the system, at its camera position.
        screen.labels.clear();
        if labels {
            if let Some(nav) = nav.as_ref() {
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                screen
                    .labels
                    .extend(nav.bodies.iter().map(|body| ffe_rs::screen::BodyLabel {
                        name: body.name.clone(),
                        centre: body.position.map(|v| v as f64 / unit),
                    }));
                screen
                    .labels
                    .extend(nav.ships.iter().filter(|ship| ship.visible()).map(|ship| {
                        ffe_rs::screen::BodyLabel {
                            name: ship.name.clone(),
                            centre: ship.position.map(|v| v as f64 / unit),
                        }
                    }));
            }
        }
        // The lift scenes of the docking and launching stages (the front view).
        screen.lift = None;
        // Draw the inside view until the launch completes (LaunchDone), not
        // only while the current lift stage has objects: F1088 keeps the ship in
        // the station while the outer door opens.
        let launching_scene = nav
            .as_ref()
            .is_some_and(|nav| nav.sequence.is_some_and(|s| s.launching))
            && state.mode == ffe_rs::flight::Mode::Docked;
        screen.station_view = launching_scene;
        if let Some((objects, launching)) = nav.as_ref().and_then(|nav| nav.lift_scene()) {
            views.mode = ViewMode::Front;
            let _ = launching;
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
                        date: nav.date_text(),
                        info: map
                            .describe(viewer, system_details.as_ref())
                            .unwrap_or_default(),
                    });
                }
                Err(error) => message = format!("Map error: {error}"),
            }
        }
        screen.dock = None;
        screen.market = None;
        if state.mode == ffe_rs::flight::Mode::Docked {
            if let Some(nav) = nav.as_mut() {
                if let Some(station) = nav.docked {
                    let hold = trade::hold_of(&state, nav.cargo_space(&cache, state.ship));
                    let status = station::Status {
                        cargo_used: hold.used,
                        cargo_capacity: hold.capacity,
                        cash: i32::try_from(state.credits).unwrap_or(i32::MAX),
                        cabins_used: 0,
                        cabins_total: 0,
                    };
                    let station_name = nav.bodies[station].name.clone();
                    if dock.screen == StationScreen::Market && nav.market_tables().is_some() {
                        if let Ok(rows) = trade::listing(nav, &cache, station, &state) {
                            let visible = station::layout(StationScreen::Market).visible();
                            dock.selected = dock.selected.min(rows.len().saturating_sub(1));
                            dock.scroll = dock.scroll.min(rows.len().saturating_sub(visible));
                            screen.market = Some(ffe_rs::screen::MarketView {
                                station: station_name,
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
                                scroll: dock.scroll,
                                selected: dock.selected,
                                cash: status.cash,
                                used: status.cargo_used,
                                capacity: status.cargo_capacity,
                            });
                        }
                    } else if dock.screen == StationScreen::Upgrades {
                        let tech_level = nav.tech_level(&cache).unwrap_or(0);
                        let upgrades =
                            station::upgrades(tech_level, &state.loadout, &state.combat.lasers);
                        let visible = station::layout(StationScreen::Upgrades).visible();
                        dock.selected = dock.selected.min(upgrades.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(upgrades.len().saturating_sub(visible));
                        let rows = upgrades
                            .iter()
                            .skip(dock.scroll)
                            .map(|row| ffe_rs::station::DockRow {
                                main: row.name.to_owned(),
                                detail: None,
                                columns: vec![
                                    (station::UPGRADE_PRICE_X, format!("{}", row.price), 0x7d),
                                    (station::UPGRADE_REMOVAL_X, format!("{}", row.removal), 0x7d),
                                    (station::UPGRADE_MASS_X, format!("{}t", row.mass), 0x7d),
                                ],
                                fitted: row.fitted,
                            })
                            .collect();
                        screen.dock = Some(ffe_rs::station::DockView {
                            screen: dock.screen,
                            title: station::title(dock.screen, &station_name),
                            rows,
                            status,
                            scroll: dock.scroll,
                            selected: dock.selected,
                            total: upgrades.len(),
                        });
                    } else if dock.screen == StationScreen::Repairs {
                        let repairs = station_repairs(&state, scene.as_ref());
                        let visible = station::layout(StationScreen::Repairs).visible();
                        dock.selected = dock.selected.min(repairs.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(repairs.len().saturating_sub(visible));
                        let rows = repairs
                            .iter()
                            .skip(dock.scroll)
                            .map(|row| ffe_rs::station::DockRow {
                                main: row.title.clone(),
                                detail: None,
                                columns: vec![(
                                    station::REPAIR_COST_X,
                                    format!("${}", row.cost),
                                    0x7d,
                                )],
                                fitted: false,
                            })
                            .collect();
                        screen.dock = Some(ffe_rs::station::DockView {
                            screen: dock.screen,
                            title: station::title(dock.screen, &station_name),
                            rows,
                            status,
                            scroll: dock.scroll,
                            selected: dock.selected,
                            total: repairs.len(),
                        });
                    } else {
                        let entries: &[station::Entry] = match dock.screen {
                            StationScreen::Services => &station::SERVICES,
                            StationScreen::Shipyard => &station::SHIPYARD,
                            _ => &[],
                        };
                        let rows = entries
                            .iter()
                            .skip(dock.scroll)
                            .map(|entry| ffe_rs::station::DockRow {
                                main: entry.title.to_owned(),
                                detail: entry.detail.map(str::to_owned),
                                columns: Vec::new(),
                                fitted: false,
                            })
                            .collect();
                        screen.dock = Some(ffe_rs::station::DockView {
                            screen: dock.screen,
                            title: station::title(dock.screen, &station_name),
                            rows,
                            status,
                            scroll: dock.scroll,
                            selected: dock.selected,
                            total: entries.len(),
                        });
                    }
                }
            }
        }
        // The autopilot's target, else the locked body, else a locked NPC ship:
        // a line of text and brackets in the view.
        let engaged = nav.as_ref().and_then(|nav| nav.target);
        let locked_ship = nav.as_ref().and_then(|nav| nav.locked_ship());
        let marked = if locked_ship.is_none() {
            engaged.or_else(|| nav.as_ref().and_then(|nav| nav.locked))
        } else {
            None
        };
        screen.has_lock =
            locked_ship.is_some() || nav.as_ref().is_some_and(|nav| nav.locked.is_some());
        let player_at = ship_position(&original, &state);
        screen.lock = if let Some(ship) = locked_ship {
            format!(
                "TARGET: {} {}",
                ship.name.to_ascii_uppercase(),
                describe(distance(player_at, ship.position)).to_ascii_uppercase()
            )
        } else {
            match nav.as_ref().zip(marked) {
                Some((nav, i)) => format!(
                    "{}: {} {}",
                    if engaged.is_some() {
                        "AUTOPILOT"
                    } else {
                        "TARGET"
                    },
                    nav.bodies[i].name.to_ascii_uppercase(),
                    describe(distance(player_at, nav.bodies[i].position)).to_ascii_uppercase()
                ),
                None => String::new(),
            }
        };
        screen.target_mark = if let Some(ship) = locked_ship {
            (state.mode == ffe_rs::flight::Mode::Flight)
                .then(|| {
                    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                    Screen::body_screen(
                        &state,
                        &views,
                        scene.as_ref(),
                        ship.position.map(|v| v as f64 / unit),
                        ship.radius as f64 / unit,
                    )
                })
                .flatten()
                .map(|(x, y, radius)| ffe_rs::screen::TargetMark {
                    x,
                    y,
                    radius,
                    engaged: false,
                })
        } else {
            nav.as_ref()
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
                })
        };
        // F272/F274/F275: the scanner's contacts and globe. Bodies are the
        // original's live objects (absolute system millimetres, with their
        // parent frame); NPC traffic is in the player's frame by construction.
        // F275 skips starports (`+0x14c` bit 0x20) and draws each contact's
        // colour from the model statistics' mass word.
        let mass = |model: u32| {
            scene
                .as_ref()
                .and_then(|scene| scene.ship_stats(model))
                .map(|stats| stats.mass)
        };
        screen.scanner = Some(match nav.as_ref() {
            Some(nav) => {
                let mut contacts: Vec<ffe_rs::scanner::Contact> = nav
                    .bodies
                    .iter()
                    .enumerate()
                    .map(|(index, body)| ffe_rs::scanner::Contact {
                        position: body.position,
                        frame: body.parent,
                        body: Some(index),
                        starport: body.kind == NavKind::Starport,
                        mass: mass(body.model),
                    })
                    .collect();
                contacts.extend(nav.ships.iter().filter(|ship| ship.visible()).map(|ship| {
                    ffe_rs::scanner::Contact {
                        position: ship.position,
                        frame: nav.frame(),
                        body: None,
                        starport: false,
                        mass: Some(ship.damage.mass),
                    }
                }));
                ffe_rs::scanner::View {
                    position: player_at,
                    axes: ship_axes(&state),
                    frame: nav.frame(),
                    orientation: ffe_rs::scanner::orientation(state.yaw),
                    contacts,
                }
            }
            // The prototype's station at the origin (the sandbox has no
            // navigator): the same contact F275 would project.
            None => ffe_rs::scanner::View {
                position: player_at,
                axes: ship_axes(&state),
                frame: None,
                orientation: ffe_rs::scanner::orientation(state.yaw),
                contacts: vec![ffe_rs::scanner::Contact {
                    position: [0; 3],
                    frame: None,
                    body: None,
                    starport: false,
                    mass: None,
                }],
            },
        });
        // The Graphic Detail page's space colour and the Others page's stars
        // option (the original's DATA_008815..817 / DATA_009019).
        screen.sky = prefs::space_colour_rgb(prefs.space_colour);
        screen.stars_visible = prefs.others[1];
        screen.lock_external = prefs.others[2];
        // F1100's range readout for the locked target, in the original's km/AU
        // conversion. A target that does not project is behind or off the view
        // and takes the fixed (50,50) text, as F1100 does.
        screen.target_hud = None;
        screen.target_damage = None;
        screen.tunnels = None;
        if state.mode == ffe_rs::flight::Mode::Flight {
            let target = if let Some(ship) = locked_ship {
                Some((ship.position, Some(ship)))
            } else {
                marked.and_then(|index| {
                    nav.as_ref()?
                        .bodies
                        .get(index)
                        .map(|body| (body.position, None))
                })
            };
            if let Some((position, ship)) = target {
                let from = player_at;
                let (mantissa, exponent) = laser::range_words(from, position);
                let (value, au) = laser::range_value(mantissa, exponent);
                let forward = ship_axes(&state)[2];
                let toward = [
                    (position[0] - from[0]) as f64,
                    (position[1] - from[1]) as f64,
                    (position[2] - from[2]) as f64,
                ];
                let behind =
                    forward[0] * toward[0] + forward[1] * toward[1] + forward[2] * toward[2] <= 0.0;
                // The navigation target (player+0xec) takes F1100's (50,50);
                // the combat target (player+0xed) (50,60).
                let navigation = ship.is_none() && locked_ship.is_none() && engaged.is_some();
                let (x, y, offscreen) = match screen.target_mark {
                    Some(mark) => (mark.x, mark.y, false),
                    None => (50, if navigation { 50 } else { 60 }, !behind),
                };
                screen.target_hud = Some(ffe_rs::screen::TargetHud {
                    x,
                    y,
                    value,
                    au,
                    behind,
                    offscreen,
                });
                // F1100/F1113's targeting tunnels: "Show targeting tunnels"
                // (DATA_009024) and, for a ship with a statistics block,
                // "Allow targeting tunnels for ships" (DATA_009025). The nav
                // target (player+0xec) is colour 0x11, the combat target
                // (player+0xed) 0x10. External view (4) and the combat computer
                // (6) skip them; the port has no combat view.
                if prefs.others[6]
                    && (ship.is_none() || prefs.others[7])
                    && views.mode != ViewMode::External
                {
                    if let Some(mark) = screen.target_mark {
                        screen.tunnels = Some(ffe_rs::targeter::Tunnels {
                            x: mark.x,
                            y: mark.y,
                            base: ffe_rs::targeter::tunnel_base(mantissa),
                            colour: if navigation {
                                ffe_rs::targeter::NAVIGATION_COLOUR
                            } else {
                                ffe_rs::targeter::COMBAT_COLOUR
                            },
                        });
                    }
                }
                if let Some(ship) = ship {
                    screen.target_damage = Some(ffe_rs::screen::TargetDamage {
                        hull: ship.hull_per_mille(),
                        shields: (ship.damage.shields_max > 0).then(|| ship.shield_percent()),
                        shield_strength: ship.shield_strength,
                    });
                }
            }
        }
        // DATA_007252: F1023 takes `DATA_008799 / 0xc22e` (accelerated game
        // tics / tics per second) and keeps its low bit, so the targetter cursor
        // blinks once per game second.
        screen.blink = ((nav.as_ref().map_or(elapsed, |nav| nav.clock).max(0.0) as u64) & 1) == 1;
        // F1099's player text and the console's lamp/equipment inputs. The
        // damage words are F1533's hull (mass * 4) and F413's shield maximum
        // (0x40 per generator); the equipment bits and missile pylons come from
        // the saved loadout, so a damaged shield, a fitted ECM or a missile
        // icon is real state rather than a default.
        let stats = scene
            .as_ref()
            .and_then(|scene| scene.ship_stats(state.ship));
        let hull_full = stats.map_or(0, |stats| stats.mass.wrapping_mul(4));
        if stats.is_some() {
            state.ensure_damage(hull_full);
        }
        let autopilot = nav.as_ref().is_some_and(|nav| nav.target.is_some());
        screen.hud = PlayerHud {
            shields: state.shields,
            hull: state.hull,
            set_speed: views.console.set_speed,
            actual_speed: state.speed().round() as i32,
            altitude: Some(0),
            registration: ships.name(state.ship).unwrap_or("").to_owned(),
            autopilot,
            turret: views.mode.is_turret().then(|| {
                (
                    PlayerHud::angle(views.turret.yaw),
                    -PlayerHud::angle(views.turret.pitch),
                )
            }),
        };
        screen.systems = ShipSystems {
            // `player+0xe8 == 2` is the autopilot/engine state F1099 and F1116
            // test; 2 lights 0x39 and draws the Autopilot line.
            e8: if autopilot { 2 } else { 0 },
            e9: if labels { 4 } else { 0 },
            mounts: stats.map_or(1, |stats| stats.gun_mounts),
            slots: state.combat.missiles,
            data_246: screen.target_mark.is_some(),
            // F1120 hides the 0x4f lamp while the missile combat target is set;
            // F1063/F1064 write DATA_008875/7247.
            data_247: nav.as_ref().is_some_and(|nav| nav.missile_lock.is_some()),
            general: state.loadout.general,
            ..screen.systems
        };
        screen.clock = nav.as_ref().map(|nav| nav.date_text()).unwrap_or_default();
        // F271: a changed console message becomes a dated log entry. F351's
        // bong accompanies a newly shown message.
        if message != logged && !message.is_empty() {
            screen.messages.push(message.clone(), screen.clock.clone());
            logged.clone_from(&message);
            sound.play(Cue::Message);
        }
        // F273's auto-scroll timer runs on the world tics.
        let log_tics =
            (elapsed * 50.0 * f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])).max(0.0)
                as u32;
        screen.messages.tick(log_tics);
        if launching_scene {
            // Inside the station, from the cockpit: the flight view with only the lift in it.
            let mut inside = state.clone();
            inside.mode = ffe_rs::flight::Mode::Flight;
            inside.system_flight = true;
            inside.axes = None;
            // The ship waits in the station's lift, at the station's position (the
            // player's docked position, F1088 copies it to the ship), facing out
            // along the docking axis. The station's bay and door are the world seen
            // past the lift's open doors, not empty sky.
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            if let Some(seat) = nav
                .as_ref()
                .and_then(|nav| nav.docked.and_then(|index| nav.launch_seat(index)))
            {
                inside.position = seat.map(|v| v as f64 / unit);
            }
            if let Some(port) = nav
                .as_ref()
                .and_then(|nav| nav.docked.and_then(|index| nav.port(index)))
            {
                let forward = port.axis;
                let up = [0.0, 1.0, 0.0];
                let right = [
                    up[1] * forward[2] - up[2] * forward[1],
                    up[2] * forward[0] - up[0] * forward[2],
                    up[0] * forward[1] - up[1] * forward[0],
                ];
                inside.axes = Some([right, up, forward]);
            } else if let Some(body) = nav
                .as_ref()
                .and_then(|nav| nav.docked.and_then(|index| nav.bodies.get(index)))
            {
                if let Some(world) = nav
                    .as_ref()
                    .and_then(|nav| nav.nearest_world(body.position))
                {
                    let to = [
                        world[0] as f64 / unit - inside.position[0],
                        world[1] as f64 / unit - inside.position[1],
                        world[2] as f64 / unit - inside.position[2],
                    ];
                    let length = to.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
                    inside.yaw = (to[0] / length).atan2(to[2] / length);
                    inside.pitch = (to[1] / length).asin();
                }
            }
            screen.draw(&inside, views, scene.as_ref(), help, prefs.modern, &message);
        } else {
            screen.draw(&state, views, scene.as_ref(), help, prefs.modern, &message);
        }
        if let Some(dir) = &options.capture {
            if frames % 10 == 0 {
                write_shot(dir, &format!("shot-{frames:04}.ppm"), &screen);
            }
        } else if let Some(dir) = &options.capture_planet {
            // Every tenth of a second of screen time (every 100 frames) through
            // the approach, plus the first frame.
            if frames % 100 == 0 {
                write_shot(dir, &format!("planet-{frames:04}.ppm"), &screen);
            }
        } else if let Some(dir) = &options.capture_traffic {
            if matches!(frames, 0 | 10 | 11 | 20 | 21 | 40 | 79) {
                write_shot(dir, &format!("traffic-{frames:02}.ppm"), &screen);
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
                    "P RESUMES. H HELP. F2 STATUS PAGE.",
                    "FOCUS LOSS PAUSES WITHOUT CATCH-UP.",
                ],
            );
        }
        if let Some(nav) = nav.as_ref().filter(|nav| {
            state.mode == ffe_rs::flight::Mode::Flight
                && nav.view != NavView::Closed
                && galaxy.is_none()
        }) {
            let ship = original
                .as_ref()
                .map_or([0; 3], |model| model.player().motion.position);
            let title = match nav.view {
                NavView::Galaxy => "SYSTEMS: H HYPERSPACES TO THE SELECTED".to_owned(),
                NavView::Places => nav.system_name().to_uppercase(),
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
            } else if nav.view == NavView::Places {
                // F1099's own list: markers and names only, over the view. It is
                // drawn after `screen.draw` (F1099's HUD and F272's message
                // window), so no console writing can cover it; its rows end at
                // y 133, above F272's window at y 158.
                screen.draw_places(&nav.lines(ship));
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
        let arrived = options.capture_planet.is_some()
            && nav.as_ref().is_some_and(|nav| nav.target.is_none());
        if options.smoke_frames.is_some_and(|limit| frames >= limit)
            || (options.capture.is_some() && frames >= 1000)
            || takeoff_done_at.is_some_and(|done| frames >= done + 100)
            || (options.capture_planet.is_some() && (frames >= 6000 || arrived))
            || (options.capture_traffic.is_some() && frames >= 80)
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

/// The player's position in system millimetres: the original flight model's when
/// it is running, else the prototype state's (the sandbox and the pre-launch
/// screens have no original model, and the old `[0; 3]` fallback put the ship at
/// the system origin for every lock and range readout).
fn ship_position(original: &Option<OriginalFlight>, state: &Flight) -> [i64; 3] {
    original.as_ref().map_or_else(
        || {
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            state.position.map(|v| (v * unit).round() as i64)
        },
        |model| model.player().motion.position,
    )
}

/// F942 on the player, as far as this sandbox can present it: the hull is used
/// up, so the ship is recovered at the nearest starport or station with a fresh
/// hull. The original runs its game-over/escape-pod sequence (F942 destroys the
/// player object and F924 notifies the flight state); that screen is not ported.
fn playership_destroyed(
    state: &mut Flight,
    original: &mut Option<OriginalFlight>,
    nav: &Navigator,
    mass: u16,
) {
    let full = mass.max(1).saturating_mul(4);
    state.hull = [full, full];
    state.velocity = [0.0; 3];
    state.thrust = 0.0;
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let station = nav.bodies.iter().find(|body| {
        matches!(
            body.kind,
            ffe_rs::navigation::Kind::Starport | ffe_rs::navigation::Kind::Station
        )
    });
    if let Some(body) = station {
        let at = body.position.map(|value| value as f64 / unit);
        let distance = ffe_rs::navigation::standoff(body) / unit;
        state.position = [at[0], at[1], at[2] - distance];
        if let Some(model) = original.as_mut() {
            model.teleport(state.position.map(|value| (value * unit).round() as i64));
        }
    }
}

/// The ship axes looking from `from` at `to` (flight units), for the scripted
/// traffic frame.
fn axes_looking_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let unit = |v: [f64; 3]| {
        let length = v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
        v.map(|x| x / length)
    };
    let forward = unit(std::array::from_fn(|i| to[i] - from[i]));
    let right = unit([forward[2], 0.0, -forward[0]]);
    let up = [
        forward[1] * right[2] - forward[2] * right[1],
        forward[2] * right[0] - forward[0] * right[2],
        forward[0] * right[1] - forward[1] * right[0],
    ];
    [right, up, forward]
}

/// F386's Fit/Remove on the Upgrades row `row`, against the current starport
/// tech level (`DATA_008995`), the ship's gun/missile mounts and its free hold.
/// Returns the console line F387/F388/F389 (or the refusal) would print.
fn fit_or_remove(
    nav: &Navigator,
    cache: &ffe_content::VerifiedCache,
    state: &mut Flight,
    scene: Option<&ModelViewer>,
    row: usize,
    remove: bool,
) -> String {
    let tech_level = nav.tech_level(cache).unwrap_or(0);
    let rows = station::upgrades(tech_level, &state.loadout, &state.combat.lasers);
    let Some(entry) = rows.get(row).copied() else {
        return "No such equipment.".to_owned();
    };
    let stats = scene.and_then(|scene| scene.ship_stats(state.ship));
    let used: i32 = state.hold.iter().map(|tonnes| i32::from(*tonnes)).sum();
    let limits = Limits {
        mounts: stats.map_or(4, |stats| {
            usize::try_from(stats.gun_mounts.max(0)).unwrap_or(0)
        }),
        pylons: stats.map_or(10, |stats| usize::from(stats.missiles)),
        free_space: nav.cargo_space(cache, state.ship) - used,
    };
    let result = if remove {
        outfitting::remove(
            entry.index,
            tech_level,
            &mut state.loadout,
            &mut state.combat,
            &mut state.credits,
        )
    } else {
        outfitting::fit(
            entry.index,
            tech_level,
            &mut state.loadout,
            &mut state.combat,
            &mut state.credits,
            limits,
        )
    };
    result.unwrap_or_else(|why| why)
}

/// F444's three service rows for the player's ship: the statistics' mass, the
/// drive byte (`ship+0xd0`) and the stored service days. The date-only form is
/// what the 0x9816/0x9817 templates print.
fn station_repairs(state: &Flight, scene: Option<&ModelViewer>) -> Vec<station::RepairRow> {
    let mass = scene
        .and_then(|scene| scene.ship_stats(state.ship))
        .map_or(0, |stats| u32::from(stats.mass));
    let date = |day: u32| {
        ffe_rs::galaxy_map::date_text(f64::from(day) * 86_400.0)
            .split(' ')
            .next()
            .unwrap_or("")
            .to_owned()
    };
    station::repairs(
        mass,
        state.combat.drive != 0,
        &date(state.service_days[0]),
        &date(state.service_days[1]),
    )
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
