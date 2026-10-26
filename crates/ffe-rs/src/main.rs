use ffe_core::laser;
use ffe_core::traffic::Reply;
use ffe_rs::camera::{key_amount, ship_axes, ExternalKeys, MissileCamera, ViewMode, ViewState};
use ffe_rs::combat::{self, Combat};
use ffe_rs::console::{
    acceleration_for_digit, Click, ConsoleState, ShipSystems, ACCELERATION_NAMES,
};
use ffe_rs::death::{self, Outcome};
use ffe_rs::display::{self, DisplaySettings, Presenter};
use ffe_rs::docking::Contact;
use ffe_rs::flight::{Flight, STEP};
use ffe_rs::front_end::{self, Intro, IntroAction, IntroTimeline};
use ffe_rs::galaxy_map::GalaxyMap;
use ffe_rs::gameover::GameOver;
use ffe_rs::hud::PlayerHud;
use ffe_rs::input;
use ffe_rs::map_keys::{self, MapAction};
use ffe_rs::messages::{Channel, Event as LogEvent, Post, Source};
use ffe_rs::model_lab::ModelLab;
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{
    distance, range_label, refusal, surface_radius, Kind as NavKind, Navigator, Traffic,
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
use ffe_rs::settings;
use ffe_rs::ships::ShipList;
use ffe_rs::sound::{Cue, GameAudio};
use ffe_rs::station::{self, Action as StationAction, DockState, Screen as StationScreen};
use ffe_rs::trade;
use minifb::{Key, KeyRepeat, Window};
use minifb::{MouseButton, MouseMode};

/// The window title, shared by every reopen.
const WINDOW_TITLE: &str = "FFE-RS 0.9 - Depth-order helpers, experimental flight";

/// What the open-air launch tells the player: F1013/F1031 need thrust to climb
/// out of the starport's effect region.
const TAKEOFF_HINT: &str =
    "Lifting off: raise the set speed (W or Enter) and pitch up (Up or A) to clear the starport.";
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// One frame's F271 write. `main` only puts source-faithful game events here;
/// port-only notices stay in `message` (the docked/port note) and never get a
/// date stamp. The last event of a frame wins, as the old `message = ...` chain
/// did.
#[derive(Default)]
struct PendingLog(Option<LogEvent>);

impl PendingLog {
    fn add(&mut self, source: Source, text: impl Into<String>) {
        self.0 = Some(LogEvent::new(source, text));
    }

    fn replace(&mut self, source: Source, text: impl Into<String>) {
        self.0 = Some(LogEvent::replace(source, text));
    }

    fn clear(&mut self, source: Source) {
        self.0 = Some(LogEvent::clear(source));
    }

    fn take(&mut self) -> Option<LogEvent> {
        self.0.take()
    }
}

/// Which game the boot starts: the original's new game or the development sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boot {
    /// The DOS default: Hope in Gateway with F998's type-0 Saker Mk III
    /// (model 0x1f), 10,000 CR. `--start 1` keeps Ross 154's Eagle.
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
    /// Directory for the scripted default-start flow: Escape (pause), Escape
    /// (pages), Escape (back), P (resume), F4 (services), F1 (back) and L with
    /// the set speed held (takeoff to free flight), saving every tenth frame.
    capture_start: Option<PathBuf>,
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
    /// Directory for the presentation evidence: one game frame and the Display
    /// Options page scaled with the real presenter maths at 2x, 3x, square
    /// pixels and a simulated 1920x1080 full-screen fit. No window is opened.
    capture_display: Option<PathBuf>,
    /// Directory for the F917 tombstone screen: starts the game-over state with
    /// the new game's commander/date and saves frames through the spin.
    capture_gameover: Option<PathBuf>,
    /// `--dev` or `FFE_DEV=1`: enables the developer-only Ctrl-chord model
    /// viewer/lab, screenshot and the prototype-flight toggle. Never set by the
    /// player launch path.
    dev: bool,
    /// A native save to load instead of starting a new game.
    load: Option<PathBuf>,
    /// Directory for the save -> unload -> load -> resume evidence.
    capture_save: Option<PathBuf>,
}

fn options() -> Result<Options, String> {
    let mut root = None;
    let mut smoke_frames = None;
    let mut capture = None;
    let mut capture_planet = None;
    let mut capture_traffic = None;
    let mut capture_start = None;
    let mut boot = Boot::NewGame(ffe_rs::navigation::DEFAULT_START_TYPE);
    let mut boot_given = false;
    let mut skip_intro = false;
    let mut capture_prefs = None;
    let mut capture_intro = None;
    let mut capture_display = None;
    let mut capture_gameover = None;
    let mut dev = std::env::var_os("FFE_DEV").is_some_and(|value| value == "1");
    let mut load = None;
    let mut capture_save = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(
                args.next().ok_or("--root requires a directory")?,
            ));
        } else if arg == "--start" && !boot_given {
            let value = args
                .next()
                .ok_or("--start requires 0..5 or sol")?
                .to_string_lossy()
                .to_ascii_lowercase();
            boot_given = true;
            boot = if value == "sol" {
                Boot::Sandbox
            } else {
                let start_type = value
                    .parse::<u8>()
                    .map_err(|_| "start type must be 0..5 or sol")?;
                if start_type > 5 {
                    return Err("start type must be 0..5 or sol".to_owned());
                }
                if (2..=5).contains(&start_type) {
                    eprintln!(
                        "Start type {start_type} is the JJFFE mapping and is not verified; \
                         starting type 1 (Ross 154)."
                    );
                }
                Boot::NewGame(if (2..=5).contains(&start_type) {
                    1
                } else {
                    start_type
                })
            };
        } else if arg == "--skip-intro" && !skip_intro {
            skip_intro = true;
        } else if arg == "--dev" && !dev {
            dev = true;
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
        } else if arg == "--capture-start" && capture_start.is_none() {
            capture_start = Some(PathBuf::from(
                args.next().ok_or("--capture-start requires a directory")?,
            ));
        } else if arg == "--capture-prefs" && capture_prefs.is_none() {
            capture_prefs = Some(PathBuf::from(
                args.next().ok_or("--capture-prefs requires a directory")?,
            ));
        } else if arg == "--capture-intro" && capture_intro.is_none() {
            capture_intro = Some(PathBuf::from(
                args.next().ok_or("--capture-intro requires a directory")?,
            ));
        } else if arg == "--capture-display" && capture_display.is_none() {
            capture_display = Some(PathBuf::from(
                args.next()
                    .ok_or("--capture-display requires a directory")?,
            ));
        } else if arg == "--capture-gameover" && capture_gameover.is_none() {
            capture_gameover = Some(PathBuf::from(
                args.next()
                    .ok_or("--capture-gameover requires a directory")?,
            ));
        } else if arg == "--load" && load.is_none() {
            load = Some(PathBuf::from(
                args.next().ok_or("--load requires a save file")?,
            ));
        } else if arg == "--capture-save" && capture_save.is_none() {
            capture_save = Some(PathBuf::from(
                args.next().ok_or("--capture-save requires a directory")?,
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
            return Err(format!("Unknown argument {}. Usage: ffe-rs --root <repo-with-data.bin> [--start 0..5|sol] [--skip-intro] [--dev] [--load SAVE.json] [--smoke-frames 1..300] [--capture DIR] [--capture-planet DIR] [--capture-traffic DIR] [--capture-start DIR] [--capture-prefs DIR] [--capture-intro DIR] [--capture-display DIR] [--capture-gameover DIR] [--capture-save DIR]",arg.to_string_lossy()));
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
        capture_start,
        boot,
        skip_intro,
        capture_prefs,
        capture_intro,
        capture_display,
        capture_gameover,
        dev,
        load,
        capture_save,
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

/// Build one native save from the live state. `nav` is `None` in the prototype
/// loop. Validation happens in [`saves::save_atomic`]/[`saves::save_numbered`].
fn build_save(
    commander: &saves::Commander,
    state: &Flight,
    nav: Option<&Navigator>,
    original_flight: bool,
    acceleration: usize,
) -> saves::SaveGame {
    let mut save = saves::SaveGame::new(commander.clone(), state.clone(), None);
    save.navigation = nav.map(Navigator::save_state);
    save.original_flight = original_flight;
    save.acceleration = acceleration;
    save
}

/// The persisted view of the live preference pages. The remap table is not
/// edited by this build, so it is carried through unchanged.
fn config_from_prefs(prefs: &Prefs, previous: &settings::Config) -> settings::Config {
    settings::Config {
        modern: prefs.modern,
        display: prefs.display,
        space_colour: prefs.space_colour,
        shape_detail: prefs.shape_detail,
        others: prefs.others,
        maps: prefs.maps,
        sound: prefs.sound,
        remap: previous.remap.clone(),
    }
}

/// Apply a loaded save to the running game. The caller supplies the cache so the
/// navigator's bodies can be regenerated; the ship model and the original-flight
/// flag decide whether the original model resumes.
#[allow(clippy::too_many_arguments)]
fn apply_save(
    cache: &ffe_content::VerifiedCache,
    scene: Option<&ModelViewer>,
    loaded: saves::SaveGame,
    state: &mut Flight,
    nav: &mut Option<Navigator>,
    original: &mut Option<OriginalFlight>,
    dock: &mut DockState,
    acceleration: &mut usize,
    commander: &mut saves::Commander,
    message: &mut String,
) -> Result<(), String> {
    let new_nav = match &loaded.navigation {
        Some(navigation) => Some(Navigator::from_save(cache, navigation)?),
        None => None,
    };
    *state = loaded.state;
    *nav = new_nav;
    *commander = loaded.commander;
    *acceleration = loaded.acceleration;
    *original = if loaded.original_flight && state.mode == ffe_rs::flight::Mode::Flight {
        let stats = scene.and_then(|scene| scene.ship_stats(state.ship));
        Some(
            OriginalFlight::from_flight(state, stats.as_ref())
                .with_time_acceleration(*acceleration),
        )
    } else {
        None
    };
    *dock = DockState::default();
    if state.mode == ffe_rs::flight::Mode::Docked
        && nav.as_ref().is_some_and(|nav| nav.docked.is_some())
    {
        dock.open(StationScreen::Services);
    }
    *message = format!("Loaded commander {}.", commander.name);
    Ok(())
}

fn write_shot(dir: &Path, name: &str, screen: &Screen) {
    let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in &screen.pixels {
        out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    let _ = fs::write(dir.join(name), out);
}

/// Developer screenshot (`--dev` Ctrl+F11): the presented 640x480 frame as a
/// binary PPM under `shots/`. The player build has no screenshot key; the
/// scripted `--capture*` modes remain the reproducible capture path.
fn save_screenshot(root: &Path, screen: &Screen) -> Result<String, String> {
    let dir = root.join("shots");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut number = 1;
    while dir.join(format!("shot-{number:04}.ppm")).exists() {
        number += 1;
    }
    let name = format!("shot-{number:04}.ppm");
    let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in &screen.pixels {
        out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    fs::write(dir.join(&name), out).map_err(|error| error.to_string())?;
    Ok(format!("shots/{name}"))
}

/// Write an RGB PNG from packed 0x00RRGGBB pixels.
fn write_png(path: &Path, pixels: &[u32], width: usize, height: usize) -> Result<(), String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for pixel in pixels.iter().take(width * height) {
            rgb.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        writer
            .write_image_data(&rgb)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// The presentation evidence: the same 320x200 frame through the real display
/// scaler at the supported sizes. `display.cfg` and the window's live size do
/// not affect these images, and no capture (`--capture*`) is touched.
fn write_display_capture(dir: &Path, cache: &ffe_content::VerifiedCache) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("Cannot create {}: {error}", dir.display()))?;
    let mut screen = Screen::new(cache)?;
    let scene = ModelViewer::new(cache).ok();
    let state = Flight::free_flight();
    screen.sky = prefs::space_colour_rgb(4);
    screen.draw(
        &state,
        ViewState::default(),
        scene.as_ref(),
        true,
        "DISPLAY EVIDENCE: THE 320x200 FRAME IS UNCHANGED",
    );
    let logical = screen.logical_pixels().to_vec();
    let default = DisplaySettings::default();
    let mut target = Vec::new();
    for (name, width, height, settings) in [
        ("display-default-4x.png", 1280, 960, default),
        (
            "display-2x.png",
            640,
            480,
            DisplaySettings {
                scale: 2,
                ..default
            },
        ),
        (
            "display-3x.png",
            960,
            720,
            DisplaySettings {
                scale: 3,
                ..default
            },
        ),
        ("display-fullscreen-1080p.png", 1920, 1080, default),
        (
            "display-square-3x.png",
            960,
            600,
            DisplaySettings {
                scale: 3,
                aspect: ffe_rs::display::PixelAspect::Square,
                ..default
            },
        ),
        (
            "display-linear-fit.png",
            1000,
            600,
            DisplaySettings {
                fit: ffe_rs::display::FitMode::FillWindow,
                filter: ffe_rs::display::Filter::Linear,
                ..default
            },
        ),
    ] {
        display::compose(&logical, &settings, width, height, &mut target);
        write_png(&dir.join(name), &target, width, height)?;
    }
    // The port-only page itself, at 2x.
    let shown = Prefs {
        open: true,
        page: ffe_rs::prefs::DISPLAY,
        ..Default::default()
    };
    screen.draw_prefs(&shown);
    display::compose(
        screen.logical_pixels(),
        &DisplaySettings {
            scale: 2,
            ..default
        },
        640,
        480,
        &mut target,
    );
    write_png(&dir.join("display-options-2x.png"), &target, 640, 480)
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

/// How the front end left: a new-game start type, or the commander-load choice.
enum IntroExit {
    Start(u8),
    Load(PathBuf),
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
    presenter: &mut Presenter,
    display: &DisplaySettings,
    screen: &mut Screen,
    scene: Option<&ModelViewer>,
    sound: &mut GameAudio,
    save_dir: &Path,
) -> Result<IntroExit, String> {
    let intro = Intro::new();
    let mut tics = front_end::BOOT_TICS;
    let mut version = false;
    let mut last = Instant::now();
    sound.front_end_music();
    loop {
        if !window.is_open() {
            sound.leave_front_end();
            return Ok(IntroExit::Start(1));
        }
        screen.draw_intro(scene, tics, version)?;
        presenter.present(window, screen.logical_pixels(), display)?;
        for key in window.get_keys_pressed(KeyRepeat::No) {
            // The original's title has a Commander Load choice (F220/F222); the
            // port's front end is the intro, so L is its Load Commander key.
            if key == Key::L {
                match saves::load_quick_or_latest(save_dir) {
                    Ok((path, _)) => {
                        sound.leave_front_end();
                        return Ok(IntroExit::Load(path));
                    }
                    Err(error) => {
                        eprintln!("FRONT END LOAD: {error}");
                        continue;
                    }
                }
            }
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
                    return Ok(IntroExit::Start(chosen));
                }
                IntroAction::Skip { chosen, notice } => {
                    if let Some(notice) = notice {
                        eprintln!("{notice}");
                    }
                    sound.leave_front_end();
                    return Ok(IntroExit::Start(chosen));
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

/// The scripted save evidence: make a complete commander, quick-save it, drop
/// every live object (the "quit"), load the bytes back, rebuild the navigator and
/// compare the resumed state with the original. The two frames and a text log
/// land in `dir`.
fn write_save_capture(cache: &ffe_content::VerifiedCache, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("Cannot create {}: {error}", dir.display()))?;
    let mut nav = Navigator::new_game(cache, 1)?;
    let mut hold = vec![0u16; ffe_core::market::NAMES];
    hold[0] = 7;
    hold[ffe_core::hyperspace::HYDROGEN] = 3;
    let state = Flight {
        mode: ffe_rs::flight::Mode::Flight,
        system_flight: true,
        position: [0.0, 0.0, -500.0],
        velocity: [0.0, 0.0, 5.0],
        yaw: 0.25,
        pitch: -0.1,
        credits: 12_345,
        hold,
        combat: Combat::new_game(),
        loadout: ffe_core::equipment::Loadout::new_game(),
        shields: [0x40, 0x40],
        hull: [4608, 4608],
        ..Flight::default()
    };
    nav.legal.wanted = true;
    nav.legal.alert = true;
    nav.legal.offences = 2;
    nav.legal.fines[1] = 6000;
    nav.clock += 4_000.0;
    nav.altitude = 0x00b7_1b00;
    let commander = saves::Commander {
        name: "Jameson".to_owned(),
        ranks: [4, 7],
        missions: vec![saves::Mission {
            id: 3,
            system: Some(nav.system_code(nav.current).unwrap_or(0)),
            completed: false,
        }],
    };
    let mut save = saves::SaveGame::new(commander, state.clone(), Some(nav.save_state()));
    save.acceleration = 2;
    let path = dir.join("roundtrip.json");
    let _ = fs::remove_file(&path);
    saves::save_atomic(&path, &save)?;
    let mut screen = Screen::new(cache)?;
    let scene = ModelViewer::new(cache).ok();
    screen.draw(
        &state,
        ViewState::default(),
        scene.as_ref(),
        false,
        "SAVE EVIDENCE: BEFORE QUICK-SAVE",
    );
    write_shot(dir, "save-before.ppm", &screen);

    // Quit: drop the live navigator and state completely, then load the file.
    let expected_navigation = save.navigation.clone().unwrap();
    let expected_state = save.state.clone();
    drop(nav);
    drop(save);
    drop(state);
    let loaded = saves::load_game(&path)?;
    let resumed_nav = Navigator::from_save(cache, loaded.navigation.as_ref().unwrap())?;
    if resumed_nav.save_state() != expected_navigation {
        return Err("Save capture: navigator did not resume byte-for-byte.".to_owned());
    }
    if loaded.state != expected_state {
        return Err("Save capture: flight state did not resume byte-for-byte.".to_owned());
    }
    screen.draw(
        &loaded.state,
        ViewState::default(),
        scene.as_ref(),
        false,
        "SAVE EVIDENCE: RESUMED FROM DISK",
    );
    write_shot(dir, "save-after.ppm", &screen);
    let text = format!(
        "native save format: {} version {}\n\
         file: {}\n\
         bytes: {}\n\
         commander: {}\n\
         system: {} ({:#x})\n\
         flight state: round-trip equal\n\
         navigator state: round-trip equal\n\
         original-flight flag: {}\n\
         time acceleration: {}\n\
         legal: wanted={} alert={} offences={} fines={:?}\n\
         ranks: {:?}\n\
         missions: {:?}\n",
        saves::FORMAT,
        saves::VERSION,
        path.display(),
        fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0),
        loaded.commander.name,
        resumed_nav.system_name(),
        expected_navigation.system_code,
        loaded.original_flight,
        loaded.acceleration,
        loaded.navigation.as_ref().unwrap().legal.wanted,
        loaded.navigation.as_ref().unwrap().legal.alert,
        loaded.navigation.as_ref().unwrap().legal.offences,
        loaded.navigation.as_ref().unwrap().legal.fines,
        loaded.commander.ranks,
        loaded.commander.missions,
    );
    fs::write(dir.join("save-roundtrip.txt"), text)
        .map_err(|error| format!("Cannot write save evidence: {error}"))
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
            (ffe_rs::prefs::DISPLAY, "prefs-display.ppm"),
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
    if let Some(dir) = &options.capture_display {
        write_display_capture(dir, &cache)?;
        return Ok(());
    }
    if let Some(dir) = &options.capture_save {
        write_save_capture(&cache, dir)?;
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
    let mut state = Flight::default();
    // The Escape pages, including the Modern/Classic control preset that F3 and
    // the `Port Options` rows toggle, and the port-only display choices. Both
    // sidecars live in `saves` and are loaded before the window so the display
    // options apply from the first frame.
    let mut prefs = Prefs::default();
    // `config.cfg` is the version-2 form; the old `controls.cfg`/`display.cfg`
    // sidecars are read once as a migration when it is absent.
    let config = settings::load_config(&save_dir).unwrap_or_default();
    prefs.modern = config.modern;
    prefs.display = config.display;
    prefs.space_colour = config.space_colour;
    prefs.shape_detail = config.shape_detail;
    prefs.others = config.others;
    prefs.maps = config.maps;
    prefs.sound = config.sound;
    let mut stored_config = config;
    let mut request = display::window_request(&prefs.display, prefs.fullscreen);
    let mut window = display::open_window(WINDOW_TITLE, request)?;
    let mut presenter = Presenter::new();
    // Sound effects and music. The original's cues (`F1906`/`F1907`) and songs
    // (`F1902`..`F1904`) are posted from the events below; captures and the
    // smoke test never open a device.
    let headless = options.capture.is_some()
        || options.capture_planet.is_some()
        || options.capture_traffic.is_some()
        || options.capture_prefs.is_some()
        || options.capture_intro.is_some()
        || options.capture_start.is_some()
        || options.capture_gameover.is_some()
        || options.smoke_frames.is_some();
    let mut sound = if headless {
        GameAudio::silent(&options.root)
    } else {
        GameAudio::from_cache(&options.root, &cache)
    };
    sound.apply_prefs(&prefs);
    let mut model_lab = None;
    // The galaxy map's state and the tables it reads (cache v21 to v25).
    let mut galaxy: Option<GalaxyMap> = None;
    let mut map_drag: Option<(i32, i32)> = None;
    // The map's `DATA_005100` page selection (F834/F840). Reset when the map opens.
    let mut map_console = ffe_rs::console::MapConsole::default();
    let sector_tables = cache
        .asset("base:world/galaxy-tables")
        .and_then(|bytes| ffe_core::sector::Tables::new(bytes).ok());
    let system_details = cache
        .asset("base:world/system-tables")
        .and_then(|bytes| ffe_core::galaxy::Tables::new(bytes).ok());
    let mut views = ViewState::default();
    // `F1054`'s auto-targetter arming (`DATA_007246`/`DATA_007247`) and the
    // skip targets (`DATA_007248`/`DATA_007249`); only the fitted Auto targetter
    // lets `T`/`Shift+T` touch them.
    let mut targeter = ffe_rs::targeter::Arming::default();
    // The original flight model; None runs the synthetic prototype flight. Only
    // `--dev`'s O chord toggles it; a player build always flies the original
    // model once the ship is in free flight.
    let mut original: Option<OriginalFlight> = None;
    // When each external-view or turret key went down, for the original's
    // held-key ramp.
    let mut held_since: HashMap<Key, Instant> = HashMap::new();
    let mut mouse_was_down = false;
    // The Classic preset's right-button steering. The point is the logical
    // 320x200 pointer, kept even while the button is up so the press itself
    // cannot produce a jump; the accumulator is `DATA_009188`.
    let mut mouse_point: Option<(i32, i32)> = None;
    let mut right_was_down = false;
    let mut mouse_accumulator = input::MouseAccumulator::default();
    // The console's time acceleration step (0 = 1x .. 4 = 10000x).
    let mut acceleration = 0usize;
    // F2's status/equipment page (`F1120`, `DATA_008871 & 8`).
    let mut status_page = false;
    // F2's sub-page: 0 is `F1120`'s ship/equipment page, 1 is F312's profile.
    let mut status_section = 0u8;
    // The single route into the original's ten-entry log. Port-only notices
    // (`message` below) never enter it.
    let mut log_channel = Channel::default();
    let mut pending_log = PendingLog::default();
    let mut model_viewer: Option<ModelViewer> = None;
    // F1104/F917: the game-over screen. The death stream starts it from its own
    // destruction event; the current F953 path below uses the same API.
    let mut gameover: Option<GameOver> = None;
    let mut gameover_viewer: Option<ModelViewer> = None;
    // The flight view's station is the original model 74 when the cache has it;
    // otherwise the placeholder wireframe stays.
    let scene = match ModelViewer::new(&cache) {
        Ok(scene) => Some(scene),
        Err(error) => {
            eprintln!("Station model unavailable, using the placeholder: {error}");
            None
        }
    };
    if let Some(dir) = &options.capture_gameover {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        gameover = Some(GameOver::tombstone(
            "JAMESON",
            ffe_rs::galaxy_map::date_short_text(0.0),
        ));
        gameover_viewer = ModelViewer::new(&cache).ok();
    }
    // The original's boot shows the intro until a start key. Scripted captures and
    // the smoke frame count skip it; --start sol is the development sandbox.
    let scripts = options.smoke_frames.is_some()
        || options.capture.is_some()
        || options.capture_planet.is_some()
        || options.capture_traffic.is_some()
        || options.capture_start.is_some()
        || options.capture_gameover.is_some()
        || options.capture_save.is_some();
    let mut boot = options.boot;
    let mut load_path = options.load.clone();
    if !options.skip_intro
        && !scripts
        && load_path.is_none()
        && matches!(options.boot, Boot::NewGame(_))
    {
        match run_intro(
            &mut window,
            &mut presenter,
            &prefs.display,
            &mut screen,
            scene.as_ref(),
            &mut sound,
            &save_dir,
        )? {
            IntroExit::Start(start_type) => boot = Boot::NewGame(start_type),
            IntroExit::Load(path) => load_path = Some(path),
        }
    }
    let mut loaded = match &load_path {
        Some(path) => Some(
            saves::load_game(path)
                .map_err(|error| format!("Cannot load {}: {error}", path.display()))?,
        ),
        None => None,
    };
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
    // The original's named craft (cache v11 names + v12 statistics); Page Up/Down
    // changes the player's ship.
    let ships = scene
        .as_ref()
        .map(|scene| ShipList::from_cache(&cache, scene))
        .unwrap_or_default();
    // The systems, their bodies and the autopilot (Tab, G, Backspace). The default
    // boot is F998's type 0 at Hope in Gateway; `--start 1` keeps Ross 154 and
    // `--start sol` the development sandbox for the existing screenshots.
    let mut nav = if let Some(save) = loaded.as_ref() {
        match &save.navigation {
            Some(navigation) => Some(Navigator::from_save(&cache, navigation)?),
            // A version-1 snapshot has no navigator; keep the boot's system.
            None => match boot {
                Boot::NewGame(start_type) => Navigator::new_game(&cache, start_type),
                Boot::Sandbox => Navigator::new(&cache),
            }
            .map_err(|error| {
                eprintln!("Navigation unavailable: {error}");
            })
            .ok(),
        }
    } else {
        match boot {
            Boot::NewGame(start_type) => Navigator::new_game(&cache, start_type),
            Boot::Sandbox => Navigator::new(&cache),
        }
        .map_err(|error| {
            eprintln!("Navigation unavailable: {error}");
        })
        .ok()
    };
    if loaded.is_none() {
        if let Some(nav) = nav.as_ref() {
            if let Some(new_game) = nav.new_game {
                state.credits = u32::try_from(new_game.cash).unwrap_or(0);
                state.ship = new_game.ship;
                // F994's state 0x30 is the landed surface view (F1050/F1054), not
                // the docked services board: the ship sits on the open-air pad in
                // the flight view. F1050 draws it from the cockpit with the console
                // below.
                state.mode = ffe_rs::flight::Mode::Flight;
                state.system_flight = nav.docked.is_some();
                state.position = new_game
                    .seat
                    .map(|v| v as f64 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT);
                state.velocity = [0.0; 3];
                if let Some(index) = nav.docked {
                    state.axes = Some(nav.bodies[index].orientation);
                }
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
            // F994/F1011: the new game's open-air pad already has its flight
            // model. F1011 calls F1014 at a starport, and F1014 runs F1037
            // (set speed) and F1041 (thrust), so Enter/Shift/W/S and F1099's
            // Set work before the launch action. Without this the pad ran the
            // synthetic prototype (no Set word) and every speed key looked dead.
            if let Some(index) = nav.docked {
                let station = nav.bodies[index].clone();
                let planet = station
                    .parent
                    .and_then(|parent| nav.bodies.get(parent))
                    .cloned()
                    .unwrap_or_else(|| station.clone());
                let player_info =
                    ffe_core::objects::ModelInfo::read(model_records, model_stats, state.ship);
                let station_info =
                    ffe_core::objects::ModelInfo::read(model_records, model_stats, station.model);
                let planet_info =
                    ffe_core::objects::ModelInfo::read(model_records, model_stats, planet.model);
                if let (Some(info), Some(port_info), Some(planet_info)) =
                    (player_info, station_info, planet_info)
                {
                    let stats = scene
                        .as_ref()
                        .and_then(|scene| scene.ship_stats(state.ship));
                    let mut model = OriginalFlight::from_flight(&state, stats.as_ref());
                    model.begin_pad_takeoff(
                        &ffe_rs::navigation::landing_body(&planet, &planet_info),
                        planet.position,
                        &ffe_rs::navigation::landing_body(&station, &port_info),
                        station.position,
                        &info,
                    );
                    model.apply(&mut state);
                    original = Some(model);
                }
            }
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
    // F998's new-game armament (the same four combat words in the type-0 and
    // type-1 arms): one 1MW pulse laser in the front mount, a class-2 drive and
    // 1 t of energy, plus the general equipment bits and two 0x82 missiles (the
    // equipment page's icons). The port has no outfitting yet, so the same
    // original loadout arms the sandbox too. A loaded commander keeps its save's
    // fitted equipment and damage words.
    if loaded.is_none() {
        state.combat = Combat::new_game();
        state.loadout = ffe_core::equipment::Loadout::new_game();
    }
    // A scripted undock for screenshots: start the launch sequence immediately.
    if let Some(dir) = &options.capture {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        if let Some(nav) = nav.as_mut() {
            let _ = nav.begin_launch();
        }
    }
    // The default-start flow: the pad stays the landed view first, then the
    // script presses Escape/Escape/Escape/P/F4/F1/L and holds the set speed.
    let capture_start = options.capture_start.is_some();
    if let Some(dir) = &options.capture_start {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
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
    // The station the ship has just left: its hull is not a wall to the ship
    // until it is clear of the slot (F1088 leaves the player at the station).
    let mut launch_grace: Option<usize> = None;
    // The starport whose pad F1031 has just cleared: its hull and the planet's
    // landing scan must not settle the ship back down for a short grace, while
    // the takeoff's own F1033 contact scan says the pad is behind it.
    let mut pad_grace = false;
    let mut pad_grace_until = 0usize;
    // The starport the current open-air takeoff started from (F994's pad).
    let mut pad_station: Option<usize> = None;
    // The pad seat and its up row, for F1013's altitude: the port's terrain
    // contact reports the terrain's elevation rather than the height above it,
    // so the climb from the seat is the source's 200,000 mm above the pad.
    let mut takeoff_climb: Option<([i64; 3], [f64; 3])> = None;
    // The station screen the commander is on (the services root by default).
    let mut dock = DockState::default();
    // The services board opened from a starport pad's landed view (`0x30`):
    // F4/F343 shows it over the view, and F1 (the console screen button) or the
    // port's Backspace returns to `0x30` without launching.
    let mut pad_board = false;
    // The port's transient note (docked screens draw it under the menu). It is
    // deliberately not the F271 log: port-only hints must not get a date stamp.
    let mut message = match nav.as_ref().and_then(|nav| {
        nav.docked
            .map(|index| (nav.bodies[index].name.as_str(), nav.bodies[index].kind))
    }) {
        Some((_, NavKind::Starport)) => String::new(),
        Some(_) => station::SERVICES_HINT.to_owned(),
        None => String::new(),
    };
    let mut commander = saves::Commander::default();
    if let Some(save) = loaded.take() {
        apply_save(
            &cache,
            scene.as_ref(),
            save,
            &mut state,
            &mut nav,
            &mut original,
            &mut dock,
            &mut acceleration,
            &mut commander,
            &mut message,
        )?;
    }
    let mut last_mode = state.mode;
    // The scripted capture of the new game starts on the pad: the launch block
    // below sees a docked-to-flight change and places the model there.
    if options.capture.is_some() && nav.as_ref().is_some_and(|nav| nav.takeoff.is_some()) {
        state.mode = ffe_rs::flight::Mode::Flight;
        state.system_flight = true;
        last_mode = ffe_rs::flight::Mode::Docked;
    }
    // F994's start state names the body the commander is standing on. This is
    // the original's start message; the boot log has no other entry.
    if let Some(name) = nav
        .as_ref()
        .and_then(|nav| nav.docked.map(|index| nav.bodies[index].name.clone()))
    {
        pending_log.add(Source::Start, format!("Landed at {name}."));
    }
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
    // The default-start capture stops a moment after F1031 clears the pad.
    let mut start_clear_at: Option<usize> = None;
    let mut held_controls = input::HeldControls::default();
    // Consume the intro/start key until it has been released.
    held_controls.disarm(&window.get_keys());
    while window.is_open() {
        let now = Instant::now();
        let measured = now.duration_since(last).as_secs_f64().min(0.1);
        last = now;
        // The planet capture is scripted: a fixed 0.1 s (the original model's
        // largest frame) per presented frame, so the approach takes minutes of
        // world time in a minute of wall time.
        let elapsed = if options.capture_planet.is_some() {
            0.1
        } else if options.capture_gameover.is_some() || options.capture_start.is_some() {
            1.0 / 50.0
        } else {
            measured
        };
        // F917's game-over screen owns the frame until its loop ends. The death
        // stream starts it; the F953 path below is the current trigger. Input,
        // flight and traffic stay frozen, as in the original's state 0x2c.
        if let Some(mut game) = gameover.take() {
            let tics = (elapsed * ffe_rs::original_flight::TICS_PER_SECOND) as i64;
            game.tick(tics);
            if game.is_finished() {
                if options.capture_gameover.is_some() {
                    break;
                }
                // F918(1)/F917 longjmps to the front end, which is the intro.
                let exit = if !options.skip_intro && !scripts {
                    run_intro(
                        &mut window,
                        &mut presenter,
                        &prefs.display,
                        &mut screen,
                        scene.as_ref(),
                        &mut sound,
                        &save_dir,
                    )?
                } else {
                    IntroExit::Start(ffe_rs::navigation::DEFAULT_START_TYPE)
                };
                match exit {
                    IntroExit::Start(start_type) => {
                        boot = Boot::NewGame(start_type);
                        state = Flight::default();
                        nav = match boot {
                            Boot::NewGame(start_type) => Navigator::new_game(&cache, start_type),
                            Boot::Sandbox => Navigator::new(&cache),
                        }
                        .map_err(|error| eprintln!("Navigation unavailable: {error}"))
                        .ok();
                        if let Some(nav) = nav.as_mut() {
                            if let Some(new_game) = nav.new_game {
                                state.credits = u32::try_from(new_game.cash).unwrap_or(0);
                                state.ship = new_game.ship;
                                state.mode = ffe_rs::flight::Mode::Docked;
                            }
                        }
                        state.combat = Combat::new_game();
                        state.loadout = ffe_core::equipment::Loadout::new_game();
                        original = None;
                        views = ViewState::default();
                        dock = DockState::default();
                        acceleration = 0;
                        message = "New game.".to_owned();
                    }
                    IntroExit::Load(path) => {
                        let loaded = saves::load_game(&path)?;
                        apply_save(
                            &cache,
                            scene.as_ref(),
                            loaded,
                            &mut state,
                            &mut nav,
                            &mut original,
                            &mut dock,
                            &mut acceleration,
                            &mut commander,
                            &mut message,
                        )?;
                        views = ViewState::default();
                    }
                }
                gameover_viewer = None;
                status_page = false;
                launch_grace = None;
                takeoff_climb = None;
                galaxy = None;
                paused = false;
                labels = false;
                held_controls.disarm(&window.get_keys());
                last_mode = state.mode;
                if nav.as_ref().is_some_and(|nav| nav.docked.is_some()) {
                    sound.play(Cue::Station);
                }
            } else {
                if let Some(viewer) = gameover_viewer.as_mut() {
                    game.draw(&mut screen, viewer)?;
                }
                if let Some(dir) = &options.capture_gameover {
                    if frames % 20 == 0 {
                        write_shot(dir, &format!("gameover-{frames:04}.ppm"), &screen);
                    }
                }
                gameover = Some(game);
            }
            window
                .update_with_buffer(&screen.pixels, WIDTH, HEIGHT)
                .map_err(|e| format!("Window update: {e}"))?;
            frames += 1;
            if options.smoke_frames.is_some_and(|limit| frames >= limit)
                || (options.capture_gameover.is_some() && frames >= 700)
            {
                break;
            }
            continue;
        }
        // F1904/F232/F919: advance the song and restart the front-end theme or
        // the continuous-music selection when the current song is done.
        sound.tick(std::time::Duration::from_secs_f64(elapsed));
        // Port extension: a display change (a page row, a saved value or
        // Alt+Enter) reopens the window because minifb 0.28 has no live resize
        // or full-screen call. The old window is dropped only after the new one
        // opens, so a failure leaves the game running.
        let wanted = display::window_request(&prefs.display, prefs.fullscreen);
        if wanted != request {
            let fresh = display::open_window(WINDOW_TITLE, wanted)?;
            drop(window);
            window = fresh;
            request = wanted;
            presenter.reset();
            held_controls.disarm(&window.get_keys());
            mouse_point = None;
            mouse_was_down = false;
            right_was_down = false;
        }
        let active = window.is_active();
        let keys_down = window.get_keys();
        let controls_enabled = active
            && !prefs.open
            && !paused
            && !pad_board
            && state.death.is_none()
            && model_lab.is_none()
            && model_viewer.is_none()
            && nav.as_ref().is_none_or(|nav| nav.view == NavView::Closed);
        held_controls.update(
            &keys_down,
            &window.get_keys_pressed(KeyRepeat::No),
            controls_enabled,
        );
        // Mouse steering: only the Classic preset's right-button hold moves the
        // ship (`docs/CONTROLS.md`); the left button stays the console pointer.
        let raw_point = if active {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| display::logical_point(&prefs.display, x, y, window.get_size()))
        } else {
            None
        };
        let (mouse_dx, mouse_dy) = match (raw_point, mouse_point) {
            (Some(now), Some(before)) => (now.0 - before.0, now.1 - before.1),
            _ => (0, 0),
        };
        // A pointer on the letterbox or outside the window restarts the delta so
        // re-entering cannot snap the ship.
        mouse_point = raw_point;
        let right_down = controls_enabled && window.get_mouse_down(MouseButton::Right);
        let mouse_pressed = right_down && !right_was_down;
        if mouse_pressed {
            // The press frame's delta is discarded: a fresh hold starts at zero.
            mouse_accumulator.reset();
        }
        right_was_down = right_down;
        let mouse_steer = input::MouseSteer {
            held: right_down && !prefs.modern,
            dx: if mouse_pressed { 0 } else { mouse_dx },
            dy: if mouse_pressed { 0 } else { mouse_dy },
        };
        mouse_accumulator.update(mouse_steer);
        // Persist every applied preference page in one atomic versioned file.
        // Full-screen stays session-only and is deliberately not written.
        let next_config = config_from_prefs(&prefs, &stored_config);
        if next_config != stored_config {
            if next_config.modern != stored_config.modern {
                mouse_accumulator.reset();
                held_controls.disarm(&keys_down);
            }
            stored_config = next_config;
            if let Err(error) = settings::save_config(&save_dir, &stored_config) {
                eprintln!("SETTINGS: {error}");
            }
        }
        let pressed = |key| window.is_key_pressed(key, KeyRepeat::No);
        // F1104's death sequence: the external-view hold, then F917's spinning
        // tombstone. Flight controls and traffic stay held throughout; F1107's
        // timeout starts the screen. There are no game-over options here: the
        // tombstone ends by returning to the front end (the intro's choices).
        if let Some(value) = state.death.as_mut() {
            value.tick(elapsed);
        }
        let death_control = state.death.is_some();
        // F152/F224: the game clock runs every frame the simulation is live, in
        // every module (flight, the pad, the landed view, stations and the
        // services board). It used to be advanced only inside the original
        // flight model's branch, so the green date froze whenever there was no
        // model to tick. Time acceleration scales it; pause, the preference
        // pages, focus loss and the death/game-over hold stop it.
        if clock_runs(
            active,
            paused,
            prefs.open,
            death_control,
            model_lab.is_some(),
            model_viewer.is_some(),
        ) {
            if let Some(nav) = nav.as_mut() {
                let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
                    / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
                nav.advance(&cache, elapsed * factor);
            }
        }
        // The pad world is planet-relative: keep its planet/starport in step with
        // the navigator even while the services board is open (the flight step is
        // paused there), so F1 returns to the same pad instead of snapping the
        // ship across the body's orbital motion.
        if let Some(model) = original.as_mut().filter(|model| model.on_pad()) {
            if let Some((planet, station)) = nav.as_ref().and_then(pad_bodies) {
                model.set_pad_bodies(planet, station);
            }
        }
        if state
            .death
            .as_ref()
            .is_some_and(|death| death.is_game_over())
            && gameover.is_none()
        {
            let date = nav.as_ref().map_or_else(
                || ffe_rs::galaxy_map::date_short_text(0.0),
                |nav| ffe_rs::galaxy_map::date_short_text(nav.elapsed()),
            );
            gameover = Some(GameOver::tombstone(
                commander.name.to_ascii_uppercase(),
                date,
            ));
            gameover_viewer = ModelViewer::new(&cache).ok();
        }
        // Port extension: Alt+Enter toggles the borderless full-screen window.
        // The reopen happens at the top of the next frame; this frame's input is
        // skipped so Enter does not also activate a page row, thrust or launch.
        if active
            && pressed(Key::Enter)
            && (display::alt_held()
                || window.is_key_down(Key::LeftAlt)
                || window.is_key_down(Key::RightAlt)
                || window.is_key_down(Key::Menu))
        {
            prefs.fullscreen = !prefs.fullscreen;
            sound.play(Cue::UiBeep);
            message = if prefs.fullscreen {
                "Fullscreen (Alt+Enter returns).".to_owned()
            } else {
                "Windowed (Alt+Enter is fullscreen).".to_owned()
            };
            continue;
        }
        // A console click on the mouse-down edge, in flight (client pixels against
        // the client size: get_mouse_pos divides by the creation scale only, which
        // breaks after a resize).
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let edge_point = if active && mouse_down && !mouse_was_down {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| display::logical_point(&prefs.display, x, y, window.get_size()))
        } else {
            None
        };
        // F224: the first Escape stops time, the next opens the original's
        // preference pages, and Escape in the pages returns to the game with
        // the clock still stopped. This is the DOS order (the port used to open
        // the pages on the first press).
        if active
            && !death_control
            && (pressed(Key::Escape) || (capture_start && matches!(frames, 10 | 20 | 30)))
        {
            match ffe_rs::prefs::escape_action(prefs.open, paused) {
                ffe_rs::prefs::Escape::Pause => {
                    paused = true;
                    message = "Paused. Escape again for the preference pages.".to_owned();
                }
                ffe_rs::prefs::Escape::Open => {
                    prefs.escape();
                }
                ffe_rs::prefs::Escape::Close => {
                    prefs.escape();
                    message = if paused {
                        "Returned to the game (paused). P or the meter resumes.".to_owned()
                    } else {
                        "Returned to the game.".to_owned()
                    };
                }
            }
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
            if let Some(dir) = &options.capture_start {
                if frames % 10 == 0 {
                    write_shot(dir, &format!("start-{frames:04}.ppm"), &screen);
                }
            }
            presenter.present(&mut window, screen.logical_pixels(), &prefs.display)?;
            frames += 1;
            last = Instant::now();
            continue;
        }
        let click_point = if active
            && !death_control
            && mouse_down
            && !mouse_was_down
            && state.mode == ffe_rs::flight::Mode::Flight
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| display::logical_point(&prefs.display, x, y, window.get_size()))
        } else {
            None
        };
        let console_click = if active
            && !death_control
            && mouse_down
            && !mouse_was_down
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| display::logical_point(&prefs.display, x, y, window.get_size()))
                .and_then(|(x, y)| Screen::console_click(x, y))
        } else {
            None
        };
        // A click on a docked screen's button (F328 hot areas) on the mouse-down edge.
        let dock_click = if active
            && !death_control
            && mouse_down
            && !mouse_was_down
            && state.mode == ffe_rs::flight::Mode::Docked
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|(x, y)| display::logical_point(&prefs.display, x, y, window.get_size()))
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
        .filter(|_| active && !death_control && state.mode == ffe_rs::flight::Mode::Flight);
        let chosen_step = if capture_start && frames == 75 {
            // The scripted start flow exercises the dashboard's fastest meter
            // step (the `0` key) once the launch has begun: the green clock and
            // the navigator must leap with it while the flight model still ticks.
            Some(4)
        } else {
            match console_click {
                Some(Click::Acceleration(step)) => Some(step),
                _ => digit_step,
            }
        };
        if let Some(step) = chosen_step {
            acceleration = step;
            if let Some(model) = original.as_mut() {
                model.set_time_acceleration(step);
                message = format!("Time acceleration {}.", ACCELERATION_NAMES[step]);
            } else {
                message = format!(
                    "Time acceleration {} (applies to the original flight model).",
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
        // The original's page keys are F6 (ship systems) and F7 (this profile):
        // `F292` maps action 0xf6 to page 0 and 0xf7 to page 1. They select the
        // status page only while it is open.
        if active
            && state.mode == ffe_rs::flight::Mode::Flight
            && !pad_board
            && model_lab.is_none()
            && model_viewer.is_none()
            && (pressed(Key::F2) || console_click == Some(Click::Screen(2)))
        {
            if status_page {
                status_page = false;
                status_section = 0;
                message = "Flight console.".to_owned();
            } else {
                status_page = true;
                status_section = 0;
                message = "Status: ship systems (F2 returns, F7 commander profile).".to_owned();
            }
            held_controls.disarm(&keys_down);
        }
        if active
            && status_page
            && state.mode == ffe_rs::flight::Mode::Flight
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            if pressed(Key::F7) {
                status_section = 1;
                message = "Status: commander profile (F6 ship systems).".to_owned();
            } else if pressed(Key::F6) {
                status_section = 0;
                message = "Status: ship systems (F7 commander profile).".to_owned();
            }
        }
        // Every console click beeps (F1056). A click whose screen is not ported
        // gets only the port's transient note: the source writes no F271 entry
        // for F4/F5, the missile or combat-computer views.
        if console_click.is_some() {
            sound.play(Cue::UiBeep);
        }
        // The original galaxy map page owns F5-F12 (F840). While it is
        // showing, those keys and the overlapping console hot areas are
        // map-local and the port's global save, view, screenshot and viewer
        // keys are not run; everywhere else they are unchanged. See `map_keys`
        // and docs/notes/galaxy-map.md.
        let map_page_open = nav
            .as_ref()
            .is_some_and(|nav| map_keys::owns_keys(nav.view, galaxy.is_some()))
            && model_lab.is_none()
            && model_viewer.is_none();
        if let Some(click) = console_click {
            if let Some(note) = click.unported() {
                // The map row's hot areas are the original's function-key
                // actions (F5-F12), not the flight screens; the map block
                // handles them.
                if !map_page_open || map_keys::click(click).is_none() {
                    message = note.to_owned();
                }
            }
        }
        // F4 is the original's station/comms action (F1056's action 0xf4,
        // F343): from state 0x2a (docked) or 0x30 (an open-air starport pad, the
        // new game's start or a landing after flight) it opens the services
        // board. It is drawn over the landed view, not as a docked state:
        // F1003's return (F1) only closes it, so the flight model, HUD and
        // console are never torn down.
        if active
            && !death_control
            && model_lab.is_none()
            && model_viewer.is_none()
            && (pressed(Key::F4)
                || console_click == Some(Click::Screen(4))
                || (capture_start && frames == 50))
        {
            let pad_services = nav.as_ref().is_some_and(|nav| {
                nav.on_pad()
                    || nav.landed_body().is_some_and(|index| {
                        nav.bodies
                            .get(index)
                            .is_some_and(|body| body.kind == NavKind::Starport)
                    })
            });
            match state.mode {
                ffe_rs::flight::Mode::Flight if pad_services => {
                    // A landed starport after flight is also source state 0x30,
                    // but keeps a landed record; select its station for the
                    // board. The new game's pad already has it.
                    if let Some(nav) = nav.as_mut() {
                        if let Some(index) = nav.landed_body() {
                            nav.docked = Some(index);
                        }
                    }
                    dock.open(StationScreen::Services);
                    pad_board = true;
                    message = station::SERVICES_HINT.to_owned();
                }
                ffe_rs::flight::Mode::Docked => {
                    // F343 re-displays the root from any station screen.
                    dock.open(StationScreen::Services);
                    message = station::SERVICES_HINT.to_owned();
                }
                ffe_rs::flight::Mode::Flight => {
                    message = "No station services here.".to_owned();
                }
            }
        }
        if active && !death_control {
            let shift_down =
                window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
            let ctrl_down = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
            // Developer-only commands. A player build binds none of these:
            // `--dev` (or `FFE_DEV=1`) enables Ctrl+F11 screenshot,
            // Ctrl+F12 model viewer, Ctrl+Shift+F12 model lab and O prototype
            // flight. Plain F11/F12 stay free for the source combat computer
            // and escape capsule.
            let debug_key = [Key::F11, Key::F12, Key::O]
                .into_iter()
                .find(|key| pressed(*key));
            if let Some(command) = debug_key
                .and_then(|key| input::dev_command(options.dev, key, ctrl_down, shift_down))
                .filter(|_| !map_page_open)
            {
                match command {
                    input::DevCommand::Screenshot => {
                        message = match save_screenshot(&options.root, &screen) {
                            Ok(name) => format!("Saved {name}"),
                            Err(error) => format!("Screenshot failed: {error}"),
                        };
                    }
                    input::DevCommand::ModelViewer => {
                        held_controls.disarm(&keys_down);
                        if model_viewer.is_some() {
                            model_viewer = None;
                        } else {
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
                    input::DevCommand::ModelLab => {
                        held_controls.disarm(&keys_down);
                        if model_lab.is_some() {
                            model_lab = None;
                        } else {
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
                    input::DevCommand::PrototypeFlight
                        if state.mode == ffe_rs::flight::Mode::Flight =>
                    {
                        if original.take().is_some() {
                            state.axes = None;
                            message = "Prototype flight model (developer).".to_owned();
                        } else {
                            let stats = scene
                                .as_ref()
                                .and_then(|scene| scene.ship_stats(state.ship));
                            original = Some(
                                OriginalFlight::from_flight(&state, stats.as_ref())
                                    .with_time_acceleration(acceleration),
                            );
                            message = "Original flight model.".to_owned();
                        }
                    }
                    input::DevCommand::PrototypeFlight => {}
                }
            }
            // F3 is the source galaxy map (the console's map icon): handled with
            // G and the icon below. It no longer toggles the control preset; that
            // choice lives on `Escape` -> `Port Options` only.
            // The hyperdrive's view of the ship and its fuel (the galaxy list and the jump).
            if let Some(nav) = nav.as_mut() {
                nav.drive = drive_state(scene.as_ref(), &state);
            }
            // Navigation: Tab the system map, G the galaxy, Up/Down choose, Enter
            // flies to the body (or jumps to the system), Backspace stops the autopilot.
            if let Some(nav) = nav.as_mut().filter(|_| {
                state.mode == ffe_rs::flight::Mode::Flight
                    && !pad_board
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
                if pressed(Key::Tab) && !nav.landed_view() {
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
                if [Key::F3, Key::G]
                    .into_iter()
                    .any(|key| input::galaxy_map_key(key) && pressed(key))
                    || console_click == Some(Click::Screen(3))
                {
                    toggle(nav, NavView::Galaxy);
                    held_controls.disarm(&keys_down);
                }
                // The galaxy map (the original's F3 screen) stands in for the system list.
                if nav.view == NavView::Galaxy {
                    if galaxy.is_none() && sector_tables.is_some() {
                        if let Some(home) = nav.current_position() {
                            galaxy = Some(GalaxyMap::new(home, nav.reach().unwrap_or(0)));
                            // F834: a fresh map page with the map icon lit.
                            map_console = ffe_rs::console::MapConsole::default();
                        }
                    }
                } else {
                    galaxy = None;
                }
                let mut hyperspace = false;
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
                    // F840 owns F5-F12 while this page is showing (map_keys):
                    // F5 the controls page, F6 select, F7 details, F8 picture,
                    // F9/F10 details with next/previous, F11/F12 zoom. The
                    // details and picture screens are later slices; their keys
                    // light the original button and report that state. F969's
                    // console hot areas send the same function-key actions, so
                    // a click on the map row is routed the same way.
                    let mut actions = Vec::new();
                    if let Some(action) = console_click.and_then(map_keys::click) {
                        actions.push(action);
                    }
                    let mut map_key = false;
                    for (offset, key) in [
                        Key::F5,
                        Key::F6,
                        Key::F7,
                        Key::F8,
                        Key::F9,
                        Key::F10,
                        Key::F11,
                        Key::F12,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if pressed(key) {
                            map_key = true;
                            if let Some(action) = map_keys::action(5 + offset as u8) {
                                actions.push(action);
                            }
                        }
                    }
                    for action in actions {
                        match action {
                            MapAction::ZoomIn => map.zoom_by(-zoom),
                            MapAction::ZoomOut => map.zoom_by(zoom),
                            MapAction::Page => {
                                map_console = ffe_rs::console::MapConsole::default();
                                message = "Map controls: F6 select, F7 details, F8 picture, \
                                           F9/F10 next/previous, F11/F12 zoom."
                                    .to_owned();
                            }
                            action => {
                                if let Some(page) = map_keys::page(action) {
                                    map_console = page;
                                }
                                message = match action {
                                    MapAction::Select => "Map controls selected.".to_owned(),
                                    MapAction::Picture => {
                                        "The map picture mode (F844-F849) is not ported yet."
                                            .to_owned()
                                    }
                                    _ => {
                                        "The map details page (F842) is not ported yet.".to_owned()
                                    }
                                };
                            }
                        }
                    }
                    if map_key {
                        held_controls.disarm(&keys_down);
                    }
                    map.zoom = map.zoom.max(0x2000);
                    if pressed(Key::C) {
                        map.recentre();
                    }
                    let point =
                        window
                            .get_unscaled_mouse_pos(MouseMode::Pass)
                            .and_then(|(x, y)| {
                                display::logical_point(&prefs.display, x, y, window.get_size())
                            });
                    if active && mouse_down {
                        if let (Some(before), Some(now)) = (map_drag, point) {
                            map.rotate((now.0 - before.0) as i16, (now.1 - before.1) as i16);
                        }
                        map_drag = point;
                    } else {
                        map_drag = None;
                    }
                }
                // F840/F832: the star nearest the map's centre is the jump
                // destination. It is stored in the navigator (the original's
                // `+0x25c..0x264`/`+0x27c`), not as a body lock, so `H` still jumps
                // after the map closes. A centre with no star clears it (F837).
                if let (Some(map), Some(tables)) = (galaxy.as_mut(), sector_tables.as_ref()) {
                    map.update_selection(tables);
                    match map.selected.as_ref() {
                        Some(selected) => nav.set_destination(selected.code, selected.position),
                        None => nav.clear_destination(),
                    }
                } else if nav.view == NavView::Galaxy {
                    // No 3D map (a cache without the sector tables): the systems
                    // list's selected row is the destination.
                    match (
                        nav.system_code(nav.selected),
                        nav.system_position(nav.selected),
                    ) {
                        (Some(code), Some(position)) => nav.set_destination(code, position),
                        _ => nav.clear_destination(),
                    }
                }
                if console_click == Some(Click::Hyperspace) {
                    if nav.destination.is_some() {
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
                    // F908's event 1 / console record 0x0f: `H` jumps to the stored
                    // destination at any flight time, with or without the map open.
                    if pressed(Key::H) {
                        if nav.destination.is_some() {
                            hyperspace = true;
                        } else {
                            message =
                                "No hyperspace destination: open the map (G) and choose a system."
                                    .to_owned();
                        }
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
                        if hyperspace {
                            match nav.hyperspace_to_destination(&cache) {
                                Ok(jumped) => {
                                    if let Some(model) = original.as_mut() {
                                        model.teleport(jumped.arrival);
                                    }
                                    ffe_rs::navigation::burn_fuel(
                                        &mut state.hold,
                                        nav.drive,
                                        jumped.fuel,
                                    );
                                    let forward = ship_axes(&state)[2];
                                    nav.spawn_traffic(&cache, jumped.arrival, forward);
                                    // F908's `0x21`; F656's song 15 when the
                                    // "Music after hyperspace" switch is on.
                                    sound.play(Cue::Hyperspace);
                                    sound.hyperspace_music();
                                    pending_log.add(
                                        Source::Navigation,
                                        format!(
                                            "Hyperspace: {} in {:.1} days, {} t of fuel. Arrived in the {} system.",
                                            ffe_rs::navigation::range_text(jumped.distance),
                                            jumped.days,
                                            jumped.fuel,
                                            nav.system_name()
                                        ),
                                    );
                                    nav.view = NavView::Closed;
                                }
                                Err(error) => message = format!("Jump refused: {error}"),
                            }
                        } else if let Some(body) = nav.engage() {
                            pending_log
                                .replace(Source::Autopilot, format!("Autopilot to {}.", body.name));
                            nav.view = NavView::Closed;
                        }
                    }
                }
                if (pressed(Key::Backspace) || console_click == Some(Click::AutopilotOff))
                    && nav.target.take().is_some()
                {
                    nav.disengage();
                    // F864 clears the current replace-mode message when the
                    // autopilot ends instead of printing another line.
                    pending_log.clear(Source::Autopilot);
                }
                // Targets: a click on a body in the view, T (Shift+T the previous) or the
                // target buttons lock on to it; K or the AUTOPILOT button flies there, and
                // at a station asks for clearance and docks.
                if nav.view == NavView::Closed {
                    let ship = ship_position(&original, &state);
                    if pressed(Key::T) || console_click == Some(Click::Target) {
                        let back = window.is_key_down(Key::LeftShift)
                            || window.is_key_down(Key::RightShift);
                        if state.loadout.auto_targetter() {
                            // F1054's 't' (action 0x74): with the Auto targetter
                            // (`ship+0xc8 & 0x80`) T toggles `DATA_007246` and
                            // Shift+T `DATA_007247`; arming stores the current
                            // target as the one to skip (`DATA_007248/7249`).
                            if back {
                                let armed = targeter.toggle_combat(nav.ship_lock);
                                message = if armed {
                                    "Combat targetter armed: put a ship under the crosshair."
                                        .to_owned()
                                } else {
                                    "Combat targetter off.".to_owned()
                                };
                            } else {
                                let armed = targeter
                                    .toggle_navigation(nav.locked.map(|index| index as u32));
                                message = if armed {
                                    "Navigation targetter armed: put an object under the crosshair."
                                        .to_owned()
                                } else {
                                    "Navigation targetter off.".to_owned()
                                };
                            }
                        } else if let Some(id) = if back { nav.ship_lock } else { None } {
                            // The port's older gesture (no Auto targetter): Shift+T
                            // locks the currently locked ship as the missile
                            // target. Kept as a documented deviation so the
                            // new-game Eagle can still fire its missiles.
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
                            if nav.missile_lock.is_some() {
                                pending_log.replace(
                                    Source::Target,
                                    format!("Missile target locked: {name}. M launches."),
                                );
                            } else {
                                pending_log.replace(Source::Target, "Missile target lock off.");
                            }
                        } else if nav.ship_lock.is_some() {
                            if let Some(ship) = nav.cycle_ship_lock(if back { -1 } else { 1 }) {
                                pending_log.replace(
                                    Source::Target,
                                    format!(
                                        "Locked on {} {}. Space fires; a ship cannot be flown to.",
                                        ship.name,
                                        range_label(
                                            ship_position(&original, &state),
                                            ship.position
                                        )
                                    ),
                                );
                            }
                        } else if let Some(body) = nav.cycle_lock(ship, if back { -1 } else { 1 }) {
                            pending_log.replace(
                                Source::Target,
                                format!(
                                    "Locked on {} {}. K or AUTOPILOT flies there.",
                                    body.name,
                                    range_label(ship, body.position)
                                ),
                            );
                        }
                    }
                    // F1054's 'e' (0x65), ';' (0x3b) and Insert (0x2): F935
                    // lights the ECM bits; F718's missile update then destroys
                    // the models in `DATA_000029`. There is no energy or ammo
                    // cost in the source.
                    if pressed(Key::E) || pressed(Key::Semicolon) || pressed(Key::Insert) {
                        match state.loadout.ecm() {
                            Some(ecm) => {
                                let naval = ecm == ffe_core::equipment::Ecm::Naval;
                                let losses = nav.ecm(naval);
                                sound.play(if naval { Cue::NavalEcm } else { Cue::Ecm });
                                message = if losses.is_empty() {
                                    "ECM: no missiles in range.".to_owned()
                                } else {
                                    format!(
                                        "ECM destroys {} missile{}.",
                                        losses.len(),
                                        if losses.len() == 1 { "" } else { "s" }
                                    )
                                };
                            }
                            None => message = "No E.C.M. system fitted.".to_owned(),
                        }
                    }
                    // F1054's 'c' (0x63): F1054 tests the Chaff Dispenser
                    // (general bit 11) and the chaff cargo word (`player+0x174`,
                    // the cargo array's commodity 32), spawns the model-0xa4
                    // object (`DATA_007253`) and calls F1057 once for every
                    // missile. The source never decrements the cargo word, so
                    // the port keeps the same free-launch behaviour (reported).
                    if pressed(Key::C) {
                        let fitted = state.loadout.chaff_dispenser();
                        let chaff = state
                            .hold
                            .get(ffe_core::market::CHAFF)
                            .copied()
                            .unwrap_or(0);
                        if !fitted {
                            message = "No chaff dispenser fitted.".to_owned();
                        } else if chaff == 0 {
                            message = "No chaff in the hold.".to_owned();
                        } else if nav.sequence.is_some() {
                            message = "Chaff is only for flight.".to_owned();
                        } else {
                            let position = ship_position(&original, &state);
                            let radius = ffe_core::objects::ModelInfo::read(
                                model_records,
                                model_stats,
                                ffe_core::missile::DECOY_MODEL,
                            )
                            .map_or(0, |info| info.radii().1);
                            let decoy = nav.deploy_chaff(position, radius);
                            let seed = (nav.clock * 1000.0) as u32
                                ^ (nav.decoys.len() as u32).wrapping_mul(0x9e37_79b9);
                            let mut rng = ffe_core::RandomState::new(
                                seed,
                                seed.rotate_left(13) ^ 0x4567_89ab,
                            );
                            let pulled = nav.retarget_missiles(
                                ffe_core::missile::PLAYER_TARGET,
                                decoy,
                                |_| rng.next_u16() & 0x1000 != 0,
                            );
                            sound.play(Cue::Chaff);
                            message = if pulled == 0 {
                                "Chaff deployed.".to_owned()
                            } else {
                                format!(
                                    "Chaff deployed: {pulled} missile{} decoyed.",
                                    if pulled == 1 { "" } else { "s" }
                                )
                            };
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
                                    pending_log.replace(
                                        Source::Target,
                                        format!(
                                            "Locked on {} {}. K or AUTOPILOT flies there.",
                                            body.name,
                                            range_label(ship, body.position)
                                        ),
                                    );
                                }
                            } else {
                                nav.locked = None;
                                let id = nav.ships[index - body_count].id;
                                let locked = nav
                                    .lock_ship(id)
                                    .map(|ship| (ship.name.clone(), ship.position));
                                if let Some((name, at)) = locked {
                                    // A clicked ship is F1099's combat target
                                    // (`player+0xed`), the same word F1096 launches
                                    // a missile at.
                                    nav.missile_lock = Some(id);
                                    sound.play(Cue::UiBeep2);
                                    pending_log.replace(
                                        Source::Target,
                                        format!(
                                            "Locked on {} {}. Space fires.",
                                            name,
                                            range_label(ship_position(&original, &state), at)
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    // F1010/F1062/F1064: while an auto-targetter cursor is
                    // armed, F989's first live object within six pixels of the
                    // view centre becomes the target. An acquisition disarms
                    // the cursor; an empty crosshair only clears the target.
                    if targeter.navigation || targeter.combat {
                        let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                        let project = |position: [i64; 3]| {
                            Screen::body_projection(
                                &state,
                                &views,
                                scene.as_ref(),
                                position.map(|v| v as f64 / unit),
                            )
                        };
                        let ship_entries: Vec<(u32, i32, i32)> = nav
                            .ships
                            .iter()
                            .filter(|ship| ship.visible())
                            .filter_map(|ship| project(ship.position).map(|(x, y)| (ship.id, x, y)))
                            .collect();
                        let body_entries: Vec<(u32, i32, i32)> = nav
                            .bodies
                            .iter()
                            .enumerate()
                            .filter_map(|(index, body)| {
                                project(body.position).map(|(x, y)| (index as u32, x, y))
                            })
                            .collect();
                        if targeter.navigation {
                            match ffe_rs::targeter::acquire(
                                &body_entries,
                                nav.locked.map(|index| index as u32),
                                targeter.previous_navigation,
                            ) {
                                Some(id) => {
                                    nav.locked = Some(id as usize);
                                    targeter.acquired_navigation();
                                    sound.play(Cue::UiBeep2);
                                    if let Some(body) = nav.bodies.get(id as usize) {
                                        message = format!("Navigation target: {}.", body.name);
                                    }
                                }
                                None => nav.locked = None,
                            }
                        }
                        if targeter.combat {
                            match ffe_rs::targeter::acquire(
                                &ship_entries,
                                nav.ship_lock,
                                targeter.previous_combat,
                            ) {
                                Some(id) => {
                                    nav.missile_lock = Some(id);
                                    let name = nav.lock_ship(id).map(|ship| ship.name.clone());
                                    targeter.acquired_combat();
                                    sound.play(Cue::UiBeep2);
                                    if let Some(name) = name {
                                        message = format!("Combat target: {name}. M launches.");
                                    }
                                }
                                None => {
                                    nav.ship_lock = None;
                                    nav.missile_lock = None;
                                }
                            }
                        }
                    }
                    if pressed(Key::M) {
                        // F1054's 'm': the first non-zero pylon in
                        // ship+0xd6..+0xdf; F1096 requires DATA_008875 (the
                        // combat target) and calls F941.
                        match (state.combat.first_missile(), nav.missile_lock) {
                            (None, _) => {
                                pending_log.add(Source::Combat, "No missiles left.");
                            }
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
                                    None => {
                                        pending_log
                                            .add(Source::Combat, "The missile target is gone.");
                                    }
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
                                                sound.play(Cue::MissileLaunch);
                                                // F941 repeats F931's F237(ship, 3)
                                                // for the player's missile.
                                                match nav.player_fired(position) {
                                                    Some(report) => {
                                                        let mut text = report.offence.message;
                                                        if report.police {
                                                            text.push_str(
                                                                " Police are responding.",
                                                            );
                                                        }
                                                        pending_log.add(Source::Legal, text);
                                                    }
                                                    None => pending_log.add(
                                                        Source::Combat,
                                                        format!("Missile away at {name}."),
                                                    ),
                                                }
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
                                if matches!(body.kind, NavKind::Station) {
                                    pending_log.replace(
                                        Source::Autopilot,
                                        format!("Autopilot: docking at {}.", body.name),
                                    );
                                } else {
                                    pending_log.replace(
                                        Source::Autopilot,
                                        format!("Autopilot to {}.", body.name),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if (pressed(Key::P) || (capture_start && frames == 40))
                && model_lab.is_none()
                && model_viewer.is_none()
            {
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
            } else if !paused {
                // The original's view keys (FUNC_001001/FUNC_001056): F1 cycles,
                // F6 front, F7 rear, F9 external; the console icons do the same.
                // The ship's gun mounts (cache v12): more than two offer the turrets.
                let gun_mounts = scene
                    .as_ref()
                    .and_then(|scene| scene.ship_stats(state.ship))
                    .map_or(0, |stats| stats.gun_mounts);
                if state.mode == ffe_rs::flight::Mode::Flight && !pad_board {
                    // F1056's launch action (its action 9, Tab: scancode 0x0f
                    // maps to action 0x09) from a landed state: `0x34` goes
                    // straight to F1013's climb. The new game's `0x30` open-air
                    // pad is the same view; the action asks F654 and starts the
                    // climb through the docked-to-flight setup below. L is the
                    // port's long-standing convenience for the same action; in
                    // flight `L` is the original's text-label toggle instead.
                    let landed = nav.as_ref().and_then(|nav| nav.landed_state());
                    let pad_landed = nav.as_ref().is_some_and(|nav| nav.on_pad());
                    let launch =
                        pressed(Key::L) || pressed(Key::Tab) || (capture_start && frames == 70);
                    if launch && (landed.is_some() || pad_landed) {
                        if let Some(nav) = nav.as_mut() {
                            if landed.is_some() {
                                // F1013's altitude comes from F1033; a complex
                                // starport shape leaves it at 0x7fffffff, so measure
                                // the climb from the pad's seat and up row instead.
                                let climb = nav
                                    .landed_body()
                                    .and_then(|index| nav.bodies.get(index))
                                    .map(|body| {
                                        (
                                            original
                                                .as_mut()
                                                .map_or([0; 3], |model| model.absolute_position()),
                                            body.orientation[1],
                                        )
                                    });
                                match nav.begin_surface_launch() {
                                    Ok(()) => {
                                        acceleration = 0;
                                        takeoff_climb = climb;
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
                            } else {
                                match nav.begin_launch() {
                                    Ok(()) => {
                                        acceleration = 0;
                                        // The docked-to-flight block below builds
                                        // F994's pad model and F1013's climb.
                                        last_mode = ffe_rs::flight::Mode::Docked;
                                    }
                                    Err(why) => message = why,
                                }
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
                    // F12's console icon is F1056's F1098 call (`78528`): the
                    // escape capsule or Stowmaster launches when fitted, and
                    // nothing happens (F1098 returns 0) when it is not. The key
                    // is the source's F12 (action 0xfc), as the console icon is;
                    // Ctrl+F12 is the developer model viewer instead, and F840
                    // owns F12 as zoom-out while the map page is open.
                    let ctrl_down =
                        window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
                    if (pressed(Key::F12) && !ctrl_down && !map_page_open)
                        || (console_click == Some(Click::Capsule) && !map_page_open)
                    {
                        match death::escape_craft(&state.loadout) {
                            Some(craft) => {
                                let stats = scene
                                    .as_ref()
                                    .and_then(|scene| scene.ship_stats(craft.model()));
                                death::board(&mut state, craft, stats.as_ref());
                                if original.is_some() {
                                    original = Some(
                                        OriginalFlight::from_flight(&state, stats.as_ref())
                                            .with_time_acceleration(acceleration),
                                    );
                                }
                                death::play_escape_audio(&mut sound);
                                pending_log
                                    .add(Source::Death, format!("{} launched.", craft.name()));
                            }
                            None => {
                                // F1098 returns 0 without a message when no
                                // escape craft is fitted (ffe.asm:78528).
                                message = "No escape capsule fitted.".to_owned();
                            }
                        }
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
                    let clicked = if map_page_open { None } else { clicked };
                    // F1054's F10 (action 0xfb): the missile view needs the
                    // Missile Viewer (general bit 9) and a missile in flight
                    // (`player+0xef`); otherwise the key does nothing.
                    let mut view_note: Option<String> = None;
                    let view_command = [Key::F1, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10]
                        .into_iter()
                        .find(|key| pressed(*key))
                        .and_then(|key| {
                            let command = input::view_command(key)?;
                            // F6/F7 are the status page's profile keys while it
                            // is open; they must not also change the view.
                            (!status_page
                                || !matches!(
                                    command,
                                    input::ViewCommand::Front | input::ViewCommand::Rear
                                ))
                            .then_some(command)
                        });
                    let chosen = if let Some(command) = view_command {
                        match command {
                            input::ViewCommand::Cycle => {
                                Some(views.mode.next(gun_mounts, views.turret.pitch))
                            }
                            input::ViewCommand::Front => Some(ViewMode::Front),
                            input::ViewCommand::Rear => Some(ViewMode::Rear),
                            input::ViewCommand::Turret => turret,
                            input::ViewCommand::External => Some(ViewMode::External),
                            input::ViewCommand::Missile => {
                                if views.mode == ViewMode::Missile {
                                    Some(ViewMode::Missile)
                                } else if !state.loadout.missile_viewer() {
                                    view_note = Some(
                                        "F10: fit the Missile Viewer at a starport.".to_owned(),
                                    );
                                    None
                                } else if nav.as_ref().is_some_and(|nav| nav.missile_view.is_some())
                                {
                                    Some(ViewMode::Missile)
                                } else {
                                    view_note =
                                        Some("F10: no missile of yours is in flight.".to_owned());
                                    None
                                }
                            }
                        }
                    } else {
                        match clicked {
                            Some(ViewButton::Cycle) => {
                                Some(views.mode.next(gun_mounts, views.turret.pitch))
                            }
                            Some(ViewButton::Turret) => turret,
                            Some(ViewButton::Select(mode)) => Some(mode),
                            None => None,
                        }
                    };
                    // F11 is the source combat-computer key. View 6 is not
                    // ported, so say so instead of leaving a silent dead key.
                    if view_command.is_none() && clicked.is_none() && pressed(Key::F11) {
                        view_note =
                            Some("The combat computer needs equipment (not ported).".to_owned());
                    }
                    if let Some(next) = chosen {
                        let changed = next != views.mode;
                        views.mode = next;
                        if changed {
                            // F1125: `0x27` camera on a view change. The source
                            // changes the view silently; the port must not write
                            // a "Front View." entry into F271.
                            sound.play(Cue::Camera);
                        }
                    }
                    if let Some(note) = view_note {
                        message = note;
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
                if views.mode.is_turret() && original.is_none() && !pad_board {
                    // FUNC_001038: the preset's steering keys aim the turret;
                    // Classic also aims with the right-button mouse. The ship
                    // holds its course.
                    let preset = input::Preset::from_modern(prefs.modern);
                    let [up, down, left, right] = input::turret_keys(preset);
                    let amount = |held| key_amount(elapsed_ms, held) as i16;
                    let (mouse_pitch, mouse_yaw) = input::turret_mouse(preset, mouse_steer);
                    let pitch = amount(held(&[up]))
                        .wrapping_sub(amount(held(&[down])))
                        .wrapping_add(mouse_pitch as i16);
                    let yaw = amount(held(&[left]))
                        .wrapping_sub(amount(held(&[right])))
                        .wrapping_add(mouse_yaw as i16);
                    views.mode = views.turret.step(views.mode, pitch, yaw, gun_mounts);
                }
                if views.mode == ViewMode::External && !pad_board {
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
                // The landed view's way out of the services board: F1 is the
                // console's screen button (action 0xf1, module 3's F1003 which
                // returns to the flight view); Backspace is the port's
                // convenience and only leaves from the root, so sub-screens
                // still go back one level. The flight mode never changed, so
                // closing the board simply reveals the landed view again.
                if pad_board
                    && (pressed(Key::F1)
                        || console_click == Some(Click::CycleView)
                        || (pressed(Key::Backspace) && dock.screen == StationScreen::Services)
                        || (capture_start && frames == 60))
                {
                    pad_board = false;
                    if let Some(nav) = nav.as_ref() {
                        let name = nav
                            .docked
                            .and_then(|index| nav.bodies.get(index))
                            .map_or("the starport", |body| body.name.as_str());
                        message = format!("Landed at {name}. F4 for services, L to launch.");
                    }
                    held_controls.disarm(&keys_down);
                }
                if (state.mode == ffe_rs::flight::Mode::Docked || pad_board)
                    && model_lab.is_none()
                    && model_viewer.is_none()
                {
                    let market_rows = if dock.screen == StationScreen::Market {
                        match nav
                            .as_mut()
                            .and_then(|nav| nav.docked.map(|station| (nav, station)))
                        {
                            Some((nav, station)) => {
                                trade::listing_with(nav, &cache, station, &state, dock.black_market)
                                    .map_or(0, |rows| rows.len())
                            }
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
                    // F622/F633's BBS and hull stock. Both are generated once
                    // per station and brought up to the day; the buy updates
                    // the same list.
                    let (bulletin_rows, ship_rows) = match nav
                        .as_mut()
                        .and_then(|nav| nav.docked.map(|station| (nav, station)))
                    {
                        Some((nav, station)) => match nav.station_offers(&cache, station) {
                            Ok(offers) => (offers.posts.len(), offers.rows(state.ship, None).len()),
                            Err(error) => {
                                if dock.screen == StationScreen::Bulletin
                                    || dock.screen == StationScreen::Ships
                                {
                                    message = format!("Station offers unavailable: {error}");
                                }
                                (0, 0)
                            }
                        },
                        None => (0, 0),
                    };
                    let journal_rows = state.missions.journal.len();
                    let comms_rows = state.missions.pages.len();
                    let military_rows = if dock.screen == StationScreen::Military {
                        match nav
                            .as_ref()
                            .and_then(|nav| nav.docked.map(|station| (nav, station)))
                        {
                            Some((nav, station)) => {
                                let day = nav.day();
                                nav.military_offers(&cache, station, state.missions.military_points)
                                    .map_or(0, |offers| {
                                        offers
                                            .iter()
                                            .filter(|offer| {
                                                !state.missions.is_offer_known(offer, day)
                                            })
                                            .count()
                                    })
                            }
                            None => 0,
                        }
                    } else {
                        0
                    };
                    let rows = station::row_count(
                        dock.screen,
                        bulletin_rows,
                        market_rows,
                        upgrade_rows.len(),
                        repair_rows.len(),
                        ship_rows,
                        journal_rows,
                        comms_rows,
                        military_rows,
                    );
                    // The port's journal/comms pages. The original journal is
                    // the ship library's viewer (F76/F78/F81); F4's comms list
                    // is the source of the messages. J/C are the port's keys
                    // while the station board has focus.
                    if pressed(Key::J) {
                        dock.open(StationScreen::Journal);
                        message = station::hint(StationScreen::Journal, 0, 0).to_owned();
                    }
                    if pressed(Key::C) {
                        dock.open(StationScreen::Comms);
                        message = station::hint(StationScreen::Comms, 0, 0).to_owned();
                    }
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
                                    // F354's digit action is the row button.
                                    dock_action = Some(StationAction::Repair { row: index });
                                } else if dock.screen == StationScreen::Ships {
                                    // F354's digit action is the left (Buy) button.
                                    dock_action = Some(StationAction::BuyShip { row: index });
                                } else if dock.screen == StationScreen::Police {
                                    dock_action = Some(police_action(nav.as_ref()));
                                } else if dock.screen == StationScreen::Bulletin {
                                    dock_action = Some(StationAction::Post { row: index });
                                } else if dock.screen == StationScreen::Comms {
                                    dock_action = Some(StationAction::ReadPage { row: index });
                                } else if dock.screen == StationScreen::Military {
                                    dock_action = Some(StationAction::MilitaryPost { row: index });
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
                            dock_action = Some(StationAction::Repair { row: dock.selected });
                        } else if dock.screen == StationScreen::Ships {
                            dock_action = Some(StationAction::BuyShip { row: dock.selected });
                        } else if dock.screen == StationScreen::Bulletin {
                            dock_action = Some(StationAction::Post { row: dock.selected });
                        } else if dock.screen == StationScreen::Comms {
                            dock_action = Some(StationAction::ReadPage { row: dock.selected });
                        } else if dock.screen == StationScreen::Military {
                            dock_action = Some(StationAction::MilitaryPost { row: dock.selected });
                        } else if dock.screen == StationScreen::Police {
                            dock_action = Some(police_action(nav.as_ref()));
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
                                        dock_action = Some(StationAction::Repair { row: index });
                                    } else if dock.screen == StationScreen::Police {
                                        dock_action = Some(police_action(nav.as_ref()));
                                    } else if dock.screen == StationScreen::Bulletin {
                                        dock.selected = index;
                                        dock_action = Some(StationAction::Post { row: index });
                                    } else if dock.screen == StationScreen::Military {
                                        dock.selected = index;
                                        dock_action =
                                            Some(StationAction::MilitaryPost { row: index });
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
                                station::Hot::Ship { view, row } => {
                                    dock.selected = row;
                                    dock_action = Some(if view {
                                        StationAction::ViewShip { row }
                                    } else {
                                        StationAction::BuyShip { row }
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
                        None => "No ship list: rebuild data.bin (cache v12).".to_owned(),
                    };
                }
                if let Some(station_action) = dock_action {
                    match station_action {
                        StationAction::Launch => {
                            if let Some(nav) = nav.as_mut() {
                                // F233's event-5 customs scan runs before the
                                // launch request (the original triggers it from
                                // the special-slot write that enters flight).
                                if let Some(report) = nav.smuggling_scan(&cache, &state.hold) {
                                    if !report.message.is_empty() {
                                        pending_log.add(Source::Legal, report.message);
                                    }
                                }
                                let surface = nav.landed_body();
                                let climb =
                                    surface.and_then(|index| nav.bodies.get(index)).map(|body| {
                                        (
                                            original
                                                .as_mut()
                                                .map_or([0; 3], |model| model.absolute_position()),
                                            body.orientation[1],
                                        )
                                    });
                                let launch = if surface.is_some() {
                                    nav.begin_surface_launch()
                                } else {
                                    nav.begin_launch()
                                };
                                match launch {
                                    Ok(()) if nav.takeoff.is_some() => {
                                        // F381 leaves the open-air starport's
                                        // ship on its pad; the launch button
                                        // takes it to 0x24, so flight starts here
                                        // and this frame's docked-to-flight block
                                        // below places the model on the pad.
                                        acceleration = 0;
                                        if surface.is_some() {
                                            // F1013 from a landed starport: the
                                            // model already exists and keeps its
                                            // own landed/contact state.
                                            takeoff_climb = climb;
                                            if let Some(model) = original.as_mut() {
                                                model.set_time_acceleration(0);
                                                model.begin_takeoff();
                                            } else {
                                                last_mode = ffe_rs::flight::Mode::Docked;
                                            }
                                        } else {
                                            // F994's pad hand-off: the docked-to-
                                            // flight block below builds the
                                            // original model and F1013's climb.
                                            last_mode = ffe_rs::flight::Mode::Docked;
                                        }
                                        state.mode = ffe_rs::flight::Mode::Flight;
                                        state.system_flight = true;
                                        pad_board = false;
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
                                        pending_log.add(
                                            Source::Launch,
                                            "Launch permission granted: moving into the lift.",
                                        );
                                    }
                                    Err(error) => message = error,
                                }
                            }
                        }
                        StationAction::Open(screen) => {
                            dock.open(screen);
                            let (bbs, hulls) =
                                station_offer_counts(nav.as_mut(), &cache, state.ship);
                            message = if screen == StationScreen::Police {
                                police_message(nav.as_ref())
                            } else {
                                station::hint(screen, bbs, hulls).to_owned()
                            };
                        }
                        StationAction::Back => {
                            dock.back();
                            let (bbs, hulls) =
                                station_offer_counts(nav.as_mut(), &cache, state.ship);
                            message = station::hint(dock.screen, bbs, hulls).to_owned();
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
                        StationAction::BuyShip { row } => {
                            if let Some(nav) = nav.as_mut() {
                                message = buy_ship(nav, &cache, &mut state, row);
                            }
                        }
                        StationAction::ViewShip { row } => {
                            if let Some(nav) = nav.as_mut() {
                                message = view_ship(nav, &cache, &state, row);
                            }
                        }
                        StationAction::Repair { row } => {
                            let rows = station_repairs(&state, scene.as_ref());
                            match rows.get(row) {
                                Some(repair) => {
                                    let repair = repair.clone();
                                    let day = nav.as_ref().map_or(0, |nav| nav.day());
                                    let stats = scene
                                        .as_ref()
                                        .and_then(|scene| scene.ship_stats(state.ship));
                                    message = match station::apply_repair(
                                        &mut state,
                                        stats.as_ref(),
                                        &repair,
                                        day,
                                    ) {
                                        Ok(text) => text.to_owned(),
                                        Err(text) => text,
                                    };
                                }
                                None => message = "No such repair.".to_owned(),
                            }
                        }
                        StationAction::PayFine => {
                            if let Some(nav) = nav.as_mut() {
                                let payment = nav.pay_fine(state.credits);
                                if payment.ok() {
                                    state.credits = state.credits.saturating_sub(payment.paid);
                                    message = ffe_core::status::fine_paid_message(payment.amount);
                                } else {
                                    message =
                                        ffe_core::status::fine_refused_message(payment.amount);
                                }
                            }
                        }
                        StationAction::Post { row } => {
                            // F453/F523/F534/F535: a contract offer is accepted,
                            // the advertisement is cleared and F290/F349's page
                            // is written; any other advertisement is shown.
                            let station = nav.as_ref().and_then(|nav| nav.docked);
                            if let (Some(nav), Some(station)) = (nav.as_mut(), station) {
                                let day = nav.day();
                                let code = nav.current_system_code();
                                let system = nav.system_name().to_owned();
                                match nav.mission_offer(&cache, station, row) {
                                    Ok(Some(offer)) => {
                                        match state.missions.accept(&offer, day, code, &system) {
                                            Ok(accepted) => {
                                                let _ = nav.clear_post(&cache, station, row);
                                                message = accepted.text.clone();
                                                pending_log.add(Source::Mission, accepted.text);
                                                // F349/F351's message bell.
                                                sound.play(Cue::Message);
                                            }
                                            Err(error) => message = error,
                                        }
                                    }
                                    Ok(None) => {
                                        let post = nav
                                            .station_offers(&cache, station)
                                            .ok()
                                            .and_then(|offers| offers.posts.get(row).cloned());
                                        if let Some(post) = post {
                                            if matches!(post.code, 0x9827 | 0x9828) {
                                                // F490/F491/F493: the military
                                                // recruitment notice opens the
                                                // mercenary mission list.
                                                let rows = nav
                                                    .military_offers(
                                                        &cache,
                                                        station,
                                                        state.missions.military_points,
                                                    )
                                                    .map_or(0, |offers| {
                                                        offers
                                                            .iter()
                                                            .filter(|offer| {
                                                                !state
                                                                    .missions
                                                                    .is_offer_known(offer, day)
                                                            })
                                                            .count()
                                                    });
                                                dock.open(StationScreen::Military);
                                                message =
                                                    station::hint(StationScreen::Military, rows, 0)
                                                        .to_owned();
                                            } else if let Some(text) = open_bulletin_post(
                                                nav, &cache, station, row, &mut dock,
                                            ) {
                                                message = text;
                                            }
                                        }
                                    }
                                    Err(error) => message = error,
                                }
                            }
                        }
                        StationAction::Trade { buy, row } => {
                            let station = nav.as_ref().and_then(|nav| nav.docked);
                            let shift = window.is_key_down(Key::LeftShift)
                                || window.is_key_down(Key::RightShift);
                            if let (Some(nav), Some(station)) = (nav.as_mut(), station) {
                                if dock.fake_market {
                                    // F554: on F640's fake advertisement a row
                                    // action is F542/F543, not F555/F556.
                                    let report = trade::police_sting(nav, station);
                                    pending_log.add(Source::Legal, report.message);
                                } else if let Some(good) = trade::listing_with(
                                    nav,
                                    &cache,
                                    station,
                                    &state,
                                    dock.black_market,
                                )
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
                                        Ok(text) => pending_log.add(Source::Trade, text),
                                        Err(error) => message = format!("Trade failed: {error}"),
                                    }
                                }
                            }
                        }
                        StationAction::MilitaryPost { row } => {
                            // F509: accept a military package offer. The list
                            // is regenerated for the day; the offer already
                            // active is filtered from the rows.
                            let station = nav.as_ref().and_then(|nav| nav.docked);
                            if let (Some(nav), Some(station)) = (nav.as_mut(), station) {
                                let day = nav.day();
                                let code = nav.current_system_code();
                                let system = nav.system_name().to_owned();
                                let offers = nav
                                    .military_offers(
                                        &cache,
                                        station,
                                        state.missions.military_points,
                                    )
                                    .unwrap_or_default();
                                let offer = offers
                                    .into_iter()
                                    .filter(|offer| !state.missions.is_offer_known(offer, day))
                                    .nth(row);
                                if let Some(offer) = offer {
                                    match state.missions.accept(&offer, day, code, &system) {
                                        Ok(accepted) => {
                                            message = accepted.text.clone();
                                            pending_log.add(Source::Mission, accepted.text);
                                            sound.play(Cue::Message);
                                        }
                                        Err(error) => message = error,
                                    }
                                }
                            }
                        }
                        StationAction::ReadPage { row } => {
                            // F306/F307: the page's own song (7 or 10). The
                            // list is newest first, matching the comms view.
                            if let Some(page) = state.missions.pages.iter_mut().rev().nth(row) {
                                page.unread = false;
                                message = page.text.clone();
                                match page.kind.song() {
                                    Some(7) => sound.message_page_music(),
                                    Some(10) => sound.message_video_music(),
                                    _ => {}
                                }
                            }
                        }
                        // The port has no screen for this service. This is a
                        // port-only note, not a source F271 entry.
                        StationAction::NotPorted(text) => message = text.to_owned(),
                    }
                }
                // F1056: station-screen navigation and buttons beep.
                if dock_action.is_some() || dock != dock_before {
                    sound.play(Cue::UiBeep);
                }
                let action = if pressed(Key::L) && state.mode == ffe_rs::flight::Mode::Docked {
                    if let Some(nav) = nav.as_mut() {
                        if let Some(report) = nav.smuggling_scan(&cache, &state.hold) {
                            if !report.message.is_empty() {
                                pending_log.add(Source::Legal, report.message);
                            }
                        }
                    }
                    match nav.as_mut() {
                        Some(nav) if state.mode == ffe_rs::flight::Mode::Docked => {
                            match nav.begin_launch() {
                                Ok(()) if nav.takeoff.is_some() => {
                                    acceleration = 0;
                                    state.mode = ffe_rs::flight::Mode::Flight;
                                    state.system_flight = true;
                                    pad_board = false;
                                    sound.stop_for_sequence();
                                    sound.play(Cue::UiBeep);
                                    message = TAKEOFF_HINT.to_owned();
                                    None
                                }
                                Ok(()) => {
                                    acceleration = 0;
                                    sound.stop_for_sequence();
                                    pending_log.add(
                                        Source::Launch,
                                        "Launch permission granted: moving into the lift.",
                                    );
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
                            if dock.fake_market {
                                let report = trade::police_sting(nav, station);
                                pending_log.add(Source::Legal, report.message);
                                None
                            } else {
                                let rows = trade::listing_with(
                                    nav,
                                    &cache,
                                    station,
                                    &state,
                                    dock.black_market,
                                );
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
                                            Ok(text) => pending_log.add(Source::Trade, text),
                                            Err(error) => {
                                                message = format!("Trade failed: {error}");
                                            }
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
                        }
                        _ if buying => Some(state.buy()),
                        _ => Some(state.sell()),
                    }
                } else {
                    None
                };
                if let Some(result) = action {
                    match result {
                        Ok(text) => pending_log.add(Source::Trade, text),
                        Err(error) => {
                            eprintln!("ERROR: {error}");
                            message = format!("ERROR: {error}");
                        }
                    }
                }
                let shift =
                    window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
                let ctrl = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
                // Shift+F5 loads the quick save (or the newest numbered save when
                // there is none). A load never falls back silently: a corrupt file
                // leaves the live state alone and reports why.
                if pressed(Key::F5) && shift && !map_page_open {
                    match saves::load_quick_or_latest(&save_dir).and_then(|(path, loaded)| {
                        apply_save(
                            &cache,
                            scene.as_ref(),
                            loaded,
                            &mut state,
                            &mut nav,
                            &mut original,
                            &mut dock,
                            &mut acceleration,
                            &mut commander,
                            &mut message,
                        )
                        .map(|()| path)
                    }) {
                        Ok(path) => {
                            last_mode = state.mode;
                            launch_grace = None;
                            pad_grace = false;
                            pad_station = None;
                            takeoff_climb = None;
                            gameover = None;
                            message = format!(
                                "Loaded {}. Commander {}.",
                                path.file_name()
                                    .map(|name| name.to_string_lossy())
                                    .unwrap_or_default(),
                                commander.name
                            );
                        }
                        Err(error) => {
                            eprintln!("LOAD ERROR: {error}");
                            message = format!("LOAD ERROR: {error}");
                        }
                    }
                } else if pressed(Key::F5) && !map_page_open {
                    // F5 quick-saves; Ctrl+F5 writes a new numbered save. The
                    // source refuses no in-flight save, so only death and the
                    // post-death tombstone are invalid states here.
                    if gameover.is_some() || state.death.is_some() {
                        message = "SAVE REFUSED: the commander is dead.".to_owned();
                    } else {
                        let save = build_save(
                            &commander,
                            &state,
                            nav.as_ref(),
                            original.is_some(),
                            acceleration,
                        );
                        let result = if ctrl {
                            saves::save_numbered(&save_dir, &save)
                        } else {
                            saves::save_quick(&save_dir, &save)
                        };
                        message = match result {
                            Ok(path) => format!(
                                "Saved {}.{}",
                                path.file_name()
                                    .map(|name| name.to_string_lossy())
                                    .unwrap_or_default(),
                                if ctrl { " Previous saves kept." } else { "" }
                            ),
                            Err(error) => {
                                eprintln!("SAVE ERROR: {error}");
                                format!("SAVE ERROR: {error}")
                            }
                        };
                    }
                }
            }
        }
        if active
            && !death_control
            && !paused
            && !pad_board
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            // The original's "Reverse left-right controls" and "Reverse up-down
            // controls" (DATA_009021/DATA_009022, toggled by FUNC_000216).
            let reverse = [prefs.others[3], prefs.others[4]];
            let preset = input::Preset::from_modern(prefs.modern);
            let input = input::flight_controls(
                preset,
                input::View::of(views.mode),
                held_controls.keys(),
                mouse_steer,
                reverse,
            );
            if let Some(model) = original.as_mut() {
                // F598/F600 clamp against the object's six limit words; a
                // damaged thruster or F929's engine failure changes them.
                model.set_limits(
                    state.limits(
                        scene
                            .as_ref()
                            .and_then(|scene| scene.ship_stats(state.ship))
                            .as_ref(),
                    ),
                );
                let mut keys = input::original_keys(
                    preset,
                    input::View::of(views.mode),
                    held_controls.keys(),
                    mouse_accumulator,
                    mouse_steer.held,
                    reverse,
                );
                // The default-start capture flies the manual takeoff: after the
                // launch action (frame 70) it holds the set speed, as W does.
                if capture_start && frames >= 80 {
                    keys.faster = true;
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
                    model.teleport(model.absolute_position());
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
                                // Capture-only teleport note; not a source line.
                                message = format!("Transfer to {}: {:.0} s.", body.name, seconds);
                            } else if command.arrived {
                                pending_log
                                    .add(Source::Navigation, format!("Arrived at {}.", body.name));
                                nav.target = None;
                            } else if nav.docking() {
                                pending_log.replace(
                                    Source::Autopilot,
                                    format!("Autopilot: docking at {}.", body.name),
                                );
                            } else {
                                pending_log.replace(
                                    Source::Autopilot,
                                    format!("Autopilot to {}.", body.name),
                                );
                            }
                        }
                    }
                    if let Some(notice) = nav.notice.take() {
                        // F654/F655's traffic-control refusal (`0x98b7..0x98b9`).
                        pending_log.add(Source::Docking, notice);
                    }
                    let moved = nav.follow(model.absolute_position());
                    // The pad world is planet-relative; its bodies are moved below
                    // instead, because shifting the relative position by an
                    // absolute drift would double the planet's motion.
                    if !model.on_pad() {
                        if let Some(moved) = moved {
                            let before = model.absolute_position();
                            model.shift(moved);
                            // The climb origin is in system coordinates too.
                            if let Some((origin, _)) = takeoff_climb.as_mut() {
                                for k in 0..3 {
                                    origin[k] =
                                        origin[k].wrapping_add(moved[k].wrapping_sub(before[k]));
                                }
                            }
                        }
                    }
                    // The planet capture continues to the nearest open-air
                    // starport once the planet's standoff is reached, so the saved
                    // frames show the buildings on the surface.
                    if options.capture_planet.is_some() && !capture_port && nav.target.is_none() {
                        let from = model.absolute_position();
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
                // The pad takeoff's own F1031 test has just put the starport's
                // effect region behind the ship; give the port's approach scan a
                // short grace so its terrain contact cannot re-settle the ship
                // on the pad's frame in the same second.
                let pad_takeoff_grace = pad_grace && frames < pad_grace_until;
                if pad_grace && !pad_takeoff_grace {
                    pad_grace = false;
                }
                let landing = if model.on_pad() || landed_state.is_some() || pad_takeoff_grace {
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
                // F1012/F1047's landed handlers (0x32/0x34) and F1011's pad view
                // (0x30) still read the keyboard and run F1041/F1037: the source
                // F1014/F1015 call F1037, F1041 and the contact scan, so Enter /
                // Shift / W / S set the speed and the engine thrust applies even
                // while the ship sits on the surface. The old branch only copied
                // the mode word, which froze the HUD's Set and ignored every
                // speed key until the player launched.
                let landed_parent = nav.as_ref().and_then(|nav| {
                    let index = nav.landed_body()?;
                    let body = nav.bodies.get(index)?;
                    let info =
                        ffe_core::objects::ModelInfo::read(model_records, model_stats, body.model)?;
                    Some((
                        index,
                        ffe_rs::navigation::landing_body(body, &info),
                        body.position,
                        info,
                    ))
                });
                let mut landed_at = None;
                let step_result = if let Some(mode) = landed_state {
                    model.set_mode(mode);
                    if let Some((index, parent, at, info)) = landed_parent {
                        model
                            .step_landing(
                                elapsed,
                                &keys,
                                &parent,
                                at,
                                &info,
                                landing_tools.as_ref(),
                            )
                            .map(|contact| {
                                if contact.flags & 0x40 != 0 {
                                    landed_at = Some(index);
                                } else if contact.flags & 0x80 != 0 {
                                    pending_log.add(
                                        Source::Collision,
                                        format!(
                                            "Crash: damage {}. The ship bounced off the surface.",
                                            contact.damage.max(0)
                                        ),
                                    );
                                    sound.play(Cue::HullDamage);
                                }
                                Some(contact)
                            })
                    } else {
                        Ok(None)
                    }
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
                            } else if contact.flags & 0x80 != 0 {
                                pending_log.add(
                                    Source::Collision,
                                    format!(
                                        "Crash: damage {}. The ship bounced off the surface.",
                                        contact.damage.max(0)
                                    ),
                                );
                                sound.play(Cue::HullDamage);
                                // F575's tail hands F953 the crash damage
                                // (`mass * 4 * speed lost / 0x708`, at least
                                // mass / 4), which the port previously only
                                // logged.
                                if contact.damage > 0 {
                                    let mass = scene
                                        .as_ref()
                                        .and_then(|scene| scene.ship_stats(state.ship))
                                        .map_or(25, |stats| stats.mass);
                                    let (_, system) = state.take_damage_with_systems(
                                        u32::try_from(contact.damage).unwrap_or(u32::MAX),
                                        mass,
                                    );
                                    if let Some(event) = system {
                                        pending_log.add(Source::Collision, event.message());
                                    }
                                }
                            }
                            Some(contact)
                        })
                } else {
                    model.step(elapsed, &keys).map(|()| None)
                };
                match step_result {
                    Ok(contact) => {
                        model.apply(&mut state);
                        // F1033's `DATA_008867`: the altitude F908 tests before a
                        // hyperspace jump. With no contact (free space) the port
                        // leaves it at the original's "no surface" maximum.
                        if let Some(nav) = nav.as_mut() {
                            nav.altitude = contact.as_ref().map_or(i32::MAX, |c| c.altitude());
                        }
                        // F1021's rough landing (`0x32`) then F1047's view frame
                        // (`0x34`): the ship is settled, its motion is F1042's
                        // zero and the surface view is the flight view from there.
                        if let Some(index) = landed_at {
                            let starport = nav
                                .as_ref()
                                .and_then(|nav| nav.bodies.get(index))
                                .is_some_and(|body| body.kind == NavKind::Starport);
                            if let Some(nav) = nav.as_mut() {
                                let landed = if starport {
                                    // F1019/F1048: the open-air pad's `0x2e`
                                    // then the view function's `0x30`. F1048
                                    // also stops the song (F1903); the rough
                                    // landing F1047 changes no music.
                                    let landed = nav.land_at_starport(index);
                                    if landed {
                                        sound.landed(true);
                                    }
                                    landed
                                } else {
                                    nav.land(index)
                                };
                                if landed {
                                    if starport {
                                        model.settle_starport_landing();
                                    } else {
                                        model.settle_landing();
                                    }
                                    if let Some(mode) = nav.landing_frame() {
                                        model.set_mode(mode);
                                    }
                                    model.apply(&mut state);
                                    if starport {
                                        pending_log.add(
                                            Source::Landing,
                                            format!("Landed at {}.", nav.bodies[index].name),
                                        );
                                    } else {
                                        pending_log.add(
                                            Source::Landing,
                                            format!("Landed on {}.", nav.bodies[index].name),
                                        );
                                    }
                                }
                            }
                        }
                        // F1013/F1031: the open-air starport takeoff's mode
                        // word follows the contact scan and the set speed.
                        if let Some(contact) = contact {
                            if let Some(nav) = nav.as_mut().filter(|nav| nav.takeoff.is_some()) {
                                // F1013 reads DATA_008867. The port's terrain
                                // contact reports the terrain's elevation, not
                                // the ship's height above it, so the pad's own
                                // altitude cannot drive the hand-off; measure the
                                // climb from the pad seat along its up row (the
                                // source's 200,000 mm above the pad).
                                let altitude =
                                    takeoff_climb.map_or(contact.altitude(), |(origin, up)| {
                                        let at = model.absolute_position();
                                        (0..3)
                                            .map(|k| (at[k] - origin[k]) as f64 * up[k])
                                            .sum::<f64>()
                                            .clamp(0.0, f64::from(i32::MAX))
                                            as i32
                                    });
                                let station_index = nav.docked;
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
                                        // The starport is still below the ship;
                                        // keep the approach/landing scan off it
                                        // until the hull is behind (the pad
                                        // contact is F1033's, not a re-landing).
                                        pad_grace =
                                            station_index.is_some() || pad_station.is_some();
                                        pad_grace_until = frames + 120;
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
                                if capture_start
                                    && nav.takeoff.is_none()
                                    && start_clear_at.is_none()
                                {
                                    start_clear_at = Some(frames);
                                }
                            }
                        }
                        // The stations: the slot docks a cleared ship, the hull pushes it clear.
                        if let Some(nav) = nav.as_mut().filter(|_| state.system_flight) {
                            let ship = model.absolute_position();
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
                                                // F1019/F1024: the airlock cue
                                                // (`0x13`). The song stops at the
                                                // docked state (F1091).
                                                sound.play(Cue::Docking);
                                                pending_log.replace(
                                                    Source::Docking,
                                                    format!(
                                                        "{name}: docking, aligning with the lift."
                                                    ),
                                                );
                                            }
                                        }
                                        Contact::Collision { clear, reason } => {
                                            model.teleport(clear);
                                            model.face(nav.bodies[index].position);
                                            model.apply(&mut state);
                                            pending_log.add(
                                                Source::Collision,
                                                format!("{name}: {}. Pushed clear.", reason.text()),
                                            );
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
                        // Set:/Act: are F1099's HUD lines (`DATA_007310/312`),
                        // not F271 entries: the HUD reads them from the state.
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
        if active && !death_control && !paused && model_lab.is_none() && model_viewer.is_none() {
            if let Some(nav) = nav.as_mut() {
                let factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
                    / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
                let tics = (elapsed * ffe_rs::original_flight::TICS_PER_SECOND * factor) as u32;
                for event in nav.tick_traffic(tics, elapsed * factor) {
                    match event {
                        Traffic::Stage(text) => {
                            // F863's progress line uses F271's replace flag, so
                            // the stages update one entry instead of filling the
                            // ring with near-identical dates.
                            pending_log.replace(
                                Source::Docking,
                                format!(
                                    "{}: {text}.",
                                    if nav.sequence.is_some_and(|s| s.launching) {
                                        "Launching"
                                    } else {
                                        "Docking"
                                    }
                                ),
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
                            // F1091: the docked state stops the song; F1092's
                            // station bed follows.
                            sound.play(Cue::Station);
                            sound.stop_song();
                            message = station::SERVICES_HINT.to_owned();
                        }
                        Traffic::LaunchOut(index) => {
                            // F1088 sets the station's door opening; the ship is
                            // still in the lift until it is open (LaunchDone). Keep
                            // the docked/inside view, not the flight view.
                            pending_log.add(
                                Source::Launch,
                                format!("Launching: leaving {}.", nav.bodies[index].name),
                            );
                            sound.play(Cue::Door);
                        }
                        Traffic::LaunchDone(index) => {
                            // F1088/F1089: the door is open and the ship leaves.
                            state.mode = ffe_rs::flight::Mode::Flight;
                            state.docking_requested = false;
                            state.system_flight = true;
                            launch_grace = Some(index);
                            // F1098's `0x20` launch, F1092's engine bed. F1088/
                            // F1089 change no song: the random song is F1094's,
                            // after a hyperspace exit.
                            sound.play(Cue::Launch);
                            sound.play(Cue::Engine);
                            pending_log.add(
                                Source::Launch,
                                format!("Clear of {}.", nav.bodies[index].name),
                            );
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
        // a station the player's object is not in the system, and F1012/F1048's
        // landed handlers run no flight step, so a landed (or new-game pad)
        // player is not a target.
        let landed_view = nav.as_ref().is_some_and(|nav| nav.landed_view());
        if active
            && !death_control
            && !paused
            && !landed_view
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
                            let (outcome, system) = state.take_damage_with_systems(damage, mass);
                            if let Some(event) = system {
                                pending_log.add(Source::Combat, event.message());
                            }
                            let laser = if beam { "beam laser" } else { "pulse laser" };
                            match outcome {
                                ffe_core::laser::Damage::Shields => {
                                    let percent = if state.shields[1] == 0 {
                                        0
                                    } else {
                                        state.shields[0] as i32 * 100 / i32::from(state.shields[1])
                                    };
                                    pending_log.add(
                                        Source::Combat,
                                        format!(
                                            "{name} hits with its {laser}: shields {percent}% left."
                                        ),
                                    );
                                }
                                ffe_core::laser::Damage::Hull => {
                                    let per_mille =
                                        state.hull[0] as i32 * 250 / i32::from(mass.max(1));
                                    pending_log.add(
                                        Source::Combat,
                                        format!(
                                            "{name} hits with its {laser}: hull {}.{}%.",
                                            per_mille / 10,
                                            per_mille % 10
                                        ),
                                    );
                                }
                                ffe_core::laser::Damage::Destroyed => {
                                    if state.death.is_some() {
                                        // F1104's 81466 guard: further hits on a
                                        // ship already in the death hold do nothing.
                                        continue;
                                    }
                                    let outcome = playership_destroyed(
                                        &mut state,
                                        &mut original,
                                        nav,
                                        scene.as_ref(),
                                        &mut views,
                                        acceleration,
                                    );
                                    match outcome {
                                        Outcome::Escaped(craft) => {
                                            death::play_destroyed_escape_audio(&mut sound);
                                            pending_log.add(
                                                Source::Death,
                                                death::escaped_message(&name, craft),
                                            );
                                        }
                                        Outcome::Destroyed => {
                                            death::play_destroyed_audio(&mut sound);
                                            pending_log.add(
                                                Source::Death,
                                                death::destroyed_message(&name),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        ffe_rs::navigation::ShipEvent::MissileLaunch {
                            owner,
                            name,
                            code,
                            pylon,
                            target,
                            position,
                            axes,
                            launcher_radius,
                        } => {
                            // F941: resolve the pylon code through the missile
                            // model table and the cache's statistics, then add
                            // the object to the same field the player's missiles
                            // use.
                            let model = ffe_core::missile::model(code).unwrap_or(0);
                            let stats = scene.as_ref().and_then(|scene| scene.ship_stats(model));
                            let missile_radius = ffe_core::objects::ModelInfo::read(
                                model_records,
                                model_stats,
                                model,
                            )
                            .map_or(0, |info| info.radii().1);
                            if let Some(spec) =
                                ffe_rs::missile::MissileField::spec(code, stats, missile_radius)
                            {
                                nav.launch_npc_missile(
                                    spec,
                                    code,
                                    owner,
                                    target,
                                    position,
                                    axes,
                                    pylon,
                                    launcher_radius,
                                );
                                sound.play(Cue::MissileLaunch);
                                pending_log.add(
                                    Source::Combat,
                                    format!("{name} launches a missile at you."),
                                );
                            }
                        }
                        ffe_rs::navigation::ShipEvent::Ecm { name, naval } => {
                            // F757's F935: the NPC's burst is the same global
                            // DATA_008835 flag the player's key sets, so F718
                            // destroys the models in DATA_000029 system-wide.
                            let losses = nav.ecm(naval);
                            sound.play(if naval { Cue::NavalEcm } else { Cue::Ecm });
                            if losses.is_empty() {
                                pending_log.add(Source::Combat, format!("{name} fires E.C.M."));
                            } else {
                                pending_log.add(
                                    Source::Combat,
                                    format!(
                                        "{name}'s E.C.M. destroys {} missile{}.",
                                        losses.len(),
                                        if losses.len() == 1 { "" } else { "s" }
                                    ),
                                );
                            }
                        }
                        ffe_rs::navigation::ShipEvent::PoliceHostile { name } => {
                            pending_log.add(
                                Source::Legal,
                                format!("{name} is police and turns on you. You are wanted."),
                            );
                        }
                        ffe_rs::navigation::ShipEvent::AlertCleared => {
                            pending_log.add(Source::Legal, "The police have called off the chase.");
                        }
                        ffe_rs::navigation::ShipEvent::Escaped { name } => {
                            pending_log.add(Source::Navigation, format!("{name} jumps out."));
                        }
                    }
                }
                let player_velocity = state
                    .velocity
                    .map(|value| value * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT);
                for event in nav.tick_missiles_at(elapsed * factor, player, player_velocity, radius)
                {
                    match event {
                        ffe_rs::missile::MissileEvent::Hit { name, outcome, .. } => {
                            pending_log.add(
                                Source::Combat,
                                match outcome {
                                    ffe_core::laser::Damage::Shields => {
                                        let left = nav
                                            .ships
                                            .iter()
                                            .find(|ship| ship.name == name)
                                            .map_or(0, |ship| ship.shield_percent());
                                        format!(
                                            "{name}: missile absorbed by shields ({left}% left)."
                                        )
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
                                },
                            );
                        }
                        ffe_rs::missile::MissileEvent::HitPlayer { damage, .. } => {
                            // F720/F721/F953 on the player: the same shield ->
                            // hull path as a laser hit, with the missile's
                            // `DATA_006269` damage.
                            let outcome = state.take_damage(damage, mass);
                            match outcome {
                                ffe_core::laser::Damage::Shields => {
                                    let percent = if state.shields[1] == 0 {
                                        0
                                    } else {
                                        state.shields[0] as i32 * 100 / i32::from(state.shields[1])
                                    };
                                    pending_log.add(
                                        Source::Combat,
                                        format!(
                                            "A missile is absorbed by your shields ({percent}% left)."
                                        ),
                                    );
                                    sound.play(Cue::ShieldHit);
                                }
                                ffe_core::laser::Damage::Hull => {
                                    let per_mille =
                                        state.hull[0] as i32 * 1000 / i32::from(mass.max(1) * 4);
                                    pending_log.add(
                                        Source::Combat,
                                        format!(
                                            "A missile hits your hull: {}.{}%.",
                                            per_mille / 10,
                                            per_mille % 10
                                        ),
                                    );
                                    sound.play(Cue::HullDamage);
                                }
                                ffe_core::laser::Damage::Destroyed => {
                                    if state.death.is_none() {
                                        let outcome = playership_destroyed(
                                            &mut state,
                                            &mut original,
                                            nav,
                                            scene.as_ref(),
                                            &mut views,
                                            acceleration,
                                        );
                                        match outcome {
                                            Outcome::Escaped(craft) => {
                                                death::play_destroyed_escape_audio(&mut sound);
                                                pending_log.add(
                                                    Source::Death,
                                                    death::escaped_message("A missile", craft),
                                                );
                                            }
                                            Outcome::Destroyed => {
                                                death::play_destroyed_audio(&mut sound);
                                                pending_log.add(
                                                    Source::Death,
                                                    death::destroyed_message("A missile"),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ffe_rs::missile::MissileEvent::HitDecoy { .. } => {
                            pending_log.add(Source::Combat, "A missile detonates on the chaff.");
                        }
                        ffe_rs::missile::MissileEvent::Expired { .. } => {}
                    }
                }
            }
        } else if death_control {
            // F1104's external view: the traffic AI is off, but F942's explosion
            // (model 0x9b) and F926's debris still age and draw.
            if let Some(nav) = nav.as_mut() {
                nav.tick_effects(elapsed);
            }
        }
        // A new state (docking, a scenario, a load) ends the original flight model.
        if original.is_some()
            && (state.mode != ffe_rs::flight::Mode::Flight || state.axes.is_none())
        {
            original = None;
            state.axes = None;
        }
        // F1054's JUMP_004340 and F1102: the missile view camera is the
        // `DATA_008877` missile object (its position, axes and model eye). When
        // that missile dies, F1102 clears the view to the front.
        views.missile = nav.as_ref().and_then(|nav| {
            let missile = nav.viewed_missile()?;
            let stats = scene
                .as_ref()
                .and_then(|scene| scene.ship_stats(missile.spec.model));
            let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
            Some(MissileCamera {
                position: missile.position.map(|value| value as f64 / unit),
                axes: missile.axes,
                eye: stats.map_or([0, 0], |stats| stats.eye),
            })
        });
        if state.mode == ffe_rs::flight::Mode::Flight
            && views.mode == ViewMode::Missile
            && views.missile.is_none()
        {
            views.mode = ViewMode::Front;
            message = "Missile view ended.".to_owned();
        }
        screen.profile =
            if state.mode == ffe_rs::flight::Mode::Flight && status_page && status_section == 1 {
                nav.as_ref().map(|nav| nav.profile(death::COMMANDER_NAME))
            } else {
                None
            };
        views.console = ConsoleState {
            acceleration,
            paused,
            speed: (state.speed() * 9.7).round() as i32,
            set_speed: original
                .as_ref()
                .map(|model| model.set_speed_metres().round() as i32),
            labels,
            navigation: nav.as_ref().is_some_and(|nav| nav.view == NavView::Places),
            status_page: status_page && status_section == 0,
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
            if let Some((at, rows, station, station_info, planet, planet_info)) =
                nav.as_ref().and_then(|nav| {
                    nav.docked.and_then(|index| {
                        let body = &nav.bodies[index];
                        let station_info = ffe_core::objects::ModelInfo::read(
                            model_records,
                            model_stats,
                            body.model,
                        )?;
                        // F994's parent: the starport's own parent, the planet it
                        // stands on (`+0x56`). F603's gravity and F1033's altitude
                        // come from that frame; the starport is the sibling whose
                        // `0x20` region F1031 waits to clear.
                        let planet = body
                            .parent
                            .and_then(|parent| nav.bodies.get(parent))
                            .cloned()
                            .unwrap_or_else(|| body.clone());
                        let planet_info = ffe_core::objects::ModelInfo::read(
                            model_records,
                            model_stats,
                            planet.model,
                        )?;
                        Some((
                            nav.takeoff_seat(index, radius)?,
                            body.orientation,
                            body.clone(),
                            station_info,
                            planet,
                            planet_info,
                        ))
                    })
                })
            {
                state.position =
                    at.map(|v| v as f64 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT);
                state.velocity = [0.0; 3];
                state.thrust = 0.0;
                state.axes = Some(rows);
                state.system_flight = true;
                takeoff_climb = Some((at, rows[1]));
                pad_station = nav.as_ref().and_then(|nav| nav.docked);
                let mut model = OriginalFlight::from_flight(&state, stats.as_ref())
                    .with_time_acceleration(acceleration);
                if let Some(info) = player_info {
                    model.begin_takeoff();
                    // F570's frame switches persist while the ship is on the
                    // pad: a three-body world of the planet, the starport and
                    // the ship, as F994 parents the player to the planet.
                    let planet_body = ffe_rs::navigation::landing_body(&planet, &planet_info);
                    let station_body = ffe_rs::navigation::landing_body(&station, &station_info);
                    model.begin_pad_takeoff(
                        &planet_body,
                        planet.position,
                        &station_body,
                        station.position,
                        &info,
                    );
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
                "Launched. Hold right mouse to steer; Enter/Shift speed, Z/A pitch, comma/period yaw, [ ] roll."
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
            // F964's leave-flight path clears the current replace-mode message
            // (the autopilot/docking progress line) instead of dating it.
            pending_log.clear(Source::Autopilot);
        }
        last_mode = state.mode;
        // Lasers (F1040/F931): Space holds the fire input, the current view picks
        // the mount, and F931's refire/heat/energy gates decide whether a shot
        // happens this frame. The beam is F1776's model 0x9c at the gun.
        let acceleration_factor = f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])
            / f64::from(ffe_rs::console::TIME_ACCELERATION[0]);
        // DATA_009155 is elapsed time in the original's 1/49710 s tics, not a
        // 50 Hz frame count: F730's refire word (0x795c) and F930's beam branch
        // are both in those tics.
        let frame_tics = combat::frame_tics(elapsed, acceleration_factor);
        let player_stats = scene
            .as_ref()
            .and_then(|scene| scene.ship_stats(state.ship));
        if active && !death_control && !paused && state.mode == ffe_rs::flight::Mode::Flight {
            // F938 reads `ship+0xc8 & 1` (the laser cooling booster) directly.
            state.combat.cooling_booster = state.loadout.cooling_booster();
            state.combat.tick(frame_tics, elapsed * acceleration_factor);
            if let Some(stats) = player_stats {
                state.ensure_thrusters(&stats);
                // F938: shield recharge and the Hull Auto-Repair System.
                state.recharge(frame_tics, stats.mass.saturating_mul(4));
                // F938's engine load through F929: the sum of the three thrust
                // words times the mass, scaled and charged per tic.
                let sum = original
                    .as_ref()
                    .map_or(0, |model| state.thrust_sum(model.thrust_words()));
                let warned = state.damage.fuel_empty;
                if state.spend_thrust_energy(frame_tics, stats.mass, sum) && !warned {
                    pending_log.add(Source::Combat, "WARNING: Internal Fuel Tank Empty.");
                }
            }
            // F933/F934: the daily servicing roll from the game clock.
            if let Some(nav) = nav.as_ref() {
                for event in state.service_day(nav.day()) {
                    pending_log.add(Source::Combat, event.message());
                }
            }
            // F1033's fuel scoop: a star contact within 30 km at speed, one
            // tonne of hydrogen per about three seconds.
            state.scoop_cooldown = (state.scoop_cooldown - elapsed * acceleration_factor).max(0.0);
            if state.loadout.fuel_scoop()
                && state.scoop_cooldown == 0.0
                && nav.as_ref().is_some_and(|nav| nav.sequence.is_none())
            {
                let player = ship_position(&original, &state);
                let surface = nav.as_ref().and_then(|nav| {
                    nav.bodies
                        .iter()
                        .filter(|body| body.kind == NavKind::Star)
                        .map(|body| (distance(player, body.position) - body.radius as f64) as i64)
                        .min()
                });
                let speed_word = (state.speed() * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT
                    / ffe_rs::original_flight::MM_PER_SECOND)
                    as i32;
                if let Some(distance) = surface {
                    if ffe_rs::damage::scoop_ready(distance, speed_word, true) {
                        let capacity = nav
                            .as_ref()
                            .map_or(0, |nav| nav.cargo_space(&cache, state.ship).max(0) as u32);
                        if let Some(tonnes) = state.scoop_hydrogen(capacity) {
                            state.scoop_cooldown = ffe_rs::damage::SCOOP_COOLDOWN;
                            pending_log.add(
                                Source::Combat,
                                format!("Fuel Scoops Working: {tonnes}t Hydrogen in Hold."),
                            );
                        }
                    }
                }
            }
        }
        screen.beam = None;
        let can_fire = active
            && !paused
            && !pad_board
            && state.mode == ffe_rs::flight::Mode::Flight
            && model_lab.is_none()
            && model_viewer.is_none()
            && nav
                .as_ref()
                .is_none_or(|nav| nav.view == NavView::Closed && nav.sequence.is_none());
        // F236's unlawful-discharge message is the frame's report when the
        // shot is the system's first; keep it over the hit text below.
        let mut legal_message: Option<String> = None;
        if can_fire && held_controls.contains(&Key::Space) {
            if let Some(mount) = combat::mount(views.mode) {
                match state.combat.fire(mount, frame_tics) {
                    Ok(shot) => {
                        // F929: a shot that empties the pool refuels from the
                        // hold when the Auto Refueller is fitted.
                        state.refuel_after_energy();
                        if let Some(nav) = nav.as_mut() {
                            if let Some(report) = nav.player_fired(ship_position(&original, &state))
                            {
                                let mut text = report.offence.message;
                                if report.police {
                                    text.push_str(" Police are responding.");
                                }
                                legal_message = Some(text);
                            }
                        }
                        // F930: the code's `(DATA_008836 & 0x78) >> 3` index
                        // selects `laser1` for 0..3 and `laser2` for 4..15 (the
                        // beam codes), not the mount number.
                        sound.play(Cue::Laser {
                            bank: shot.index as u8,
                        });
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
                                            pending_log.add(
                                                Source::Combat,
                                                format!(
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
                                                    },
                                                ),
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
                                        let mut text = match attack.damage {
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
                                            text.push_str(" Police are responding.");
                                        }
                                        pending_log.add(Source::Combat, text);
                                        if attack.rating_changed {
                                            pending_log.add(
                                                Source::Combat,
                                                ffe_core::status::ELITE_MESSAGE,
                                            );
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
                        pending_log.add(Source::Combat, "Lasers overheated.");
                    }
                    Err(laser::FireError::NoEnergy) => {
                        pending_log.add(Source::Combat, "No energy for the lasers.");
                    }
                    Err(laser::FireError::NoLaser | laser::FireError::Refire) => {}
                }
            }
        }
        if let Some(text) = legal_message {
            // F236's unlawful-discharge report is the frame's final F271 line.
            pending_log.add(Source::Legal, text);
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
                let mut effects: Vec<ffe_rs::screen::SystemEffect> = nav
                    .effects
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
                    .collect();
                // F1054's chaff object (model 0xa4) is an ordinary object in
                // the source's list; the port draws it through the effect path.
                effects.extend(nav.decoys.iter().map(|decoy| ffe_rs::screen::SystemEffect {
                    model: ffe_core::missile::DECOY_MODEL,
                    centre: decoy.position.map(|v| v as f64 / unit),
                    axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    mass: 0,
                }));
                effects
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
        // F1111's cursor: DATA_007246's navigation box and DATA_007247's combat
        // circle, both while DATA_007252's blink is on. The armed flags are the
        // auto-targetter state, not the lock itself.
        screen.missile_lock = targeter.combat;
        screen.targeter = targeter
            .navigation
            .then_some(ffe_rs::targeter::Cursor::Navigation);
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
        screen.map_console = map_console;
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
        // F301/F302: settle contracts due at this system and mark overdue
        // ones. The original checks `DATA_008885` (the current system) each
        // event tick, not the docking state, so this runs in flight too. The
        // reward is whole credits and `DATA_008888` is in tenths, so it is
        // multiplied by ten (see `docs/notes/missions.md`).
        if let Some(nav) = nav.as_ref() {
            let day = nav.day();
            let code = nav.current_system_code();
            for outcome in state.missions.tick(day, code) {
                if outcome.reward > 0 {
                    state.credits = state
                        .credits
                        .saturating_add(outcome.reward.saturating_mul(10));
                }
                pending_log.add(Source::Mission, outcome.journal.text.clone());
                message = outcome.journal.text.clone();
                let military = state.missions.missions.iter().any(|mission| {
                    mission.id == outcome.id
                        && mission.kind == ffe_core::missions::Kind::MilitaryDelivery
                });
                if military {
                    // F304/F305/F306: the rank page's song 7. A package
                    // delivery has no medal code, so F307 (song 10) is
                    // skipped; `ReadPage` still plays it for a medal page.
                    sound.message_page_music();
                } else {
                    // F349/F351's bell.
                    sound.play(Cue::Message);
                }
            }
        }
        screen.pad_services = pad_board;
        if state.mode == ffe_rs::flight::Mode::Docked || pad_board {
            if let Some(nav) = nav.as_mut() {
                if let Some(station) = nav.docked {
                    let hold = trade::hold_of(&state, trade::capacity(nav, &cache, &state));
                    let status = station::Status {
                        cargo_used: hold.used,
                        cargo_capacity: hold.capacity,
                        cash: i32::try_from(state.credits).unwrap_or(i32::MAX),
                        cabins_used: i32::from(state.loadout.occupied_cabins),
                        cabins_total: i32::from(state.loadout.cabin_capacity()),
                    };
                    let station_name = nav.bodies[station].name.clone();
                    if dock.screen == StationScreen::Market && nav.market_tables().is_some() {
                        if let Ok(rows) =
                            trade::listing_with(nav, &cache, station, &state, dock.black_market)
                        {
                            let visible = station::layout(StationScreen::Market).visible();
                            dock.selected = dock.selected.min(rows.len().saturating_sub(1));
                            dock.scroll = dock.scroll.min(rows.len().saturating_sub(visible));
                            // F454's title is 0x98d7 (the 0x920d company name),
                            // not "Starport Stockmarket".
                            let title = if dock.black_market {
                                dock.company_seed
                                    .and_then(|seed| nav.company_name(&cache, seed))
                                    .filter(|name| !name.is_empty())
                                    .unwrap_or_else(|| station_name.clone())
                            } else {
                                station_name.clone()
                            };
                            screen.market = Some(ffe_rs::screen::MarketView {
                                station: title,
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
                                black_market: dock.black_market,
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
                    } else if dock.screen == StationScreen::Bulletin {
                        let posts = nav
                            .station_offers(&cache, station)
                            .map(|offers| offers.posts.clone())
                            .unwrap_or_default();
                        let visible = station::layout(StationScreen::Bulletin).visible();
                        dock.selected = dock.selected.min(posts.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(posts.len().saturating_sub(visible));
                        let rows = posts
                            .iter()
                            .skip(dock.scroll)
                            .map(|post| ffe_rs::station::DockRow {
                                main: post.text.clone(),
                                detail: None,
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
                            total: posts.len(),
                        });
                    } else if dock.screen == StationScreen::Military {
                        let day = nav.day();
                        let offers: Vec<ffe_core::missions::Offer> = nav
                            .military_offers(&cache, station, state.missions.military_points)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|offer| !state.missions.is_offer_known(offer, day))
                            .collect();
                        let visible = station::layout(StationScreen::Military).visible();
                        dock.selected = dock.selected.min(offers.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(offers.len().saturating_sub(visible));
                        let rows = offers
                            .iter()
                            .skip(dock.scroll)
                            .map(|offer| ffe_rs::station::DockRow {
                                main: offer.text.clone(),
                                detail: None,
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
                            total: offers.len(),
                        });
                    } else if matches!(dock.screen, StationScreen::Journal | StationScreen::Comms) {
                        // F76/F78/F81's journal text and F349's message pages.
                        // The date line is the port's calendar for the entry's
                        // game day (the original shows the MISSION.DAT issue's
                        // date).
                        let lines: Vec<(u32, String)> = if dock.screen == StationScreen::Journal {
                            state
                                .missions
                                .journal
                                .iter()
                                .rev()
                                .map(|entry| (entry.day, entry.text.clone()))
                                .collect()
                        } else {
                            state
                                .missions
                                .pages
                                .iter()
                                .rev()
                                .map(|page| (page.day, page.text.clone()))
                                .collect()
                        };
                        let visible = station::layout(dock.screen).visible();
                        dock.selected = dock.selected.min(lines.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(lines.len().saturating_sub(visible));
                        let rows = lines
                            .iter()
                            .skip(dock.scroll)
                            .map(|(day, text)| ffe_rs::station::DockRow {
                                main: format!(
                                    "{}: {}",
                                    ffe_rs::galaxy_map::date_short_text(f64::from(*day) * 86_400.0),
                                    text
                                ),
                                detail: None,
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
                            total: lines.len(),
                        });
                    } else if dock.screen == StationScreen::Ships {
                        let hulls = nav
                            .station_offers(&cache, station)
                            .map(|offers| offers.rows(state.ship, None))
                            .unwrap_or_default();
                        let visible = station::layout(StationScreen::Ships).visible();
                        dock.selected = dock.selected.min(hulls.len().saturating_sub(1));
                        dock.scroll = dock.scroll.min(hulls.len().saturating_sub(visible));
                        let rows = hulls
                            .iter()
                            .skip(dock.scroll)
                            .map(|hull| ffe_rs::station::DockRow {
                                main: hull.name.to_owned(),
                                detail: None,
                                columns: vec![
                                    (station::SHIP_COST_X, format!("${}", hull.price), 0x7d),
                                    (
                                        station::SHIP_EXCHANGE_X,
                                        if hull.net < 0 {
                                            format!("-${}", -hull.net)
                                        } else {
                                            format!("${}", hull.net)
                                        },
                                        0x7d,
                                    ),
                                    (station::SHIP_CAPACITY_X, format!("{}t", hull.cargo), 0x7d),
                                ],
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
                            total: hulls.len(),
                        });
                    } else if dock.screen == StationScreen::Police {
                        let fine = nav.fine_here();
                        let row = if fine > 0 {
                            ffe_rs::station::DockRow {
                                main: "Pay fine".to_owned(),
                                detail: Some(format!(
                                    "${} fine",
                                    crate::trade::credits(fine.min(i32::MAX as u32) as i32)
                                )),
                                columns: Vec::new(),
                                fitted: false,
                            }
                        } else {
                            ffe_rs::station::DockRow {
                                main: "Hang up.".to_owned(),
                                detail: None,
                                columns: Vec::new(),
                                fitted: false,
                            }
                        };
                        screen.dock = Some(ffe_rs::station::DockView {
                            screen: dock.screen,
                            title: station::title(dock.screen, &station_name),
                            rows: vec![row],
                            status,
                            scroll: 0,
                            selected: 0,
                            total: 1,
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
                range_label(player_at, ship.position).to_ascii_uppercase()
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
                    range_label(player_at, nav.bodies[i].position).to_ascii_uppercase()
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
        // The Graphic Detail page's "Level of Shape Detail" row is stored one
        // less than the original's `DATA_008812` (High, the default, is 1);
        // F917 clamps its negative values to 0.
        screen.surface_detail = (prefs.shape_detail as i32 - 1).max(0);
        // F1100's range readout for the locked target, in the original's km/AU
        // conversion. A target that does not project is behind or off the view
        // and takes the fixed (50,50) text, as F1100 does.
        screen.target_hud = None;
        screen.target_damage = None;
        screen.tunnels = None;
        if state.mode == ffe_rs::flight::Mode::Flight {
            let missile_view = views.mode == ViewMode::Missile;
            // F1099's missile view reads the combat target only (the navigation
            // target call is after its `cmp ship+0xea,5; jz`).
            let target = if let Some(ship) = locked_ship {
                Some((ship.position, Some(ship)))
            } else if missile_view {
                None
            } else {
                marked.and_then(|index| {
                    nav.as_ref()?
                        .bodies
                        .get(index)
                        .map(|body| (body.position, None))
                })
            };
            if let Some((position, ship)) = target {
                // F1100 measures from the missile (`DATA_008877`) in view 5,
                // otherwise from the player's ship.
                let from = match (missile_view, views.missile) {
                    (true, Some(camera)) => camera.position.map(|value| {
                        (value * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT).round() as i64
                    }),
                    _ => player_at,
                };
                let (mantissa, exponent) = laser::range_words(from, position);
                let (value, au) = laser::range_value(mantissa, exponent);
                let forward = if missile_view {
                    views
                        .missile
                        .map_or(ship_axes(&state)[2], |camera| camera.axes[2])
                } else {
                    ship_axes(&state)[2]
                };
                let toward = [
                    (position[0] - from[0]) as f64,
                    (position[1] - from[1]) as f64,
                    (position[2] - from[2]) as f64,
                ];
                let behind =
                    forward[0] * toward[0] + forward[1] * toward[1] + forward[2] * toward[2] <= 0.0;
                // F1100 reads the target's unclipped screen words
                // (`object+0x98/+0x9a`), not just the pixels inside the view: an
                // off-view target still picks an arrow form from them.
                let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
                let projection = ffe_rs::screen::Screen::body_projection(
                    &state,
                    &views,
                    scene.as_ref(),
                    position.map(|component| component as f64 / unit),
                );
                // The navigation target (player+0xec, a body) takes F1100's
                // (50,50) and colour 0x11; the combat target (player+0xed, a
                // ship) (50,60) and colour 0x10; in the missile view F1099
                // passes 0x5f (amber), not 0x10.
                let navigation = ship.is_none();
                let colour = if navigation {
                    ffe_rs::targeter::NAVIGATION_COLOUR
                } else if missile_view {
                    ffe_rs::targeter::MISSILE_VIEW_COLOUR
                } else {
                    ffe_rs::targeter::COMBAT_COLOUR
                };
                let (x, y, offscreen) = match projection {
                    Some((sx, sy)) => (sx, sy, false),
                    None => (50, if navigation { 50 } else { 60 }, !behind),
                };
                screen.target_hud = Some(ffe_rs::screen::TargetHud {
                    x,
                    y,
                    value,
                    au,
                    behind,
                    offscreen,
                    colour,
                });
                // F1100/F1113's targeting tunnels: "Show targeting tunnels"
                // (DATA_009024), "Allow targeting tunnels for ships"
                // (DATA_009025), and the target's screen words within ±1000
                // (`ffe.asm:81165-81250`). The navigation target draws them in
                // every view but external (4)/combat computer (6); the combat
                // target only in the missile view (5), which the port does not
                // have.
                // The navigation target draws them in every view but external
                // (4)/combat computer (6); the combat target only in the
                // missile view (5).
                if prefs.others[6]
                    && (ship.is_none() || prefs.others[7])
                    && views.mode != ViewMode::External
                    && (navigation || missile_view)
                {
                    if let Some((sx, sy)) = projection
                        .filter(|(sx, sy)| (-1000..1000).contains(sx) && (-1000..1000).contains(sy))
                    {
                        screen.tunnels = Some(ffe_rs::targeter::Tunnels {
                            x: sx,
                            y: sy,
                            base: ffe_rs::targeter::tunnel_base(mantissa),
                            colour,
                        });
                    }
                }
                if let Some(ship) = ship.filter(|_| !missile_view) {
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
        // A save or damage path that left the hull at zero without running F953's
        // lethal branch still enters F1104 once.
        if state.destroyed() && state.death.is_none() {
            if let Some(nav) = nav.as_mut() {
                let outcome = playership_destroyed(
                    &mut state,
                    &mut original,
                    nav,
                    scene.as_ref(),
                    &mut views,
                    acceleration,
                );
                match outcome {
                    Outcome::Escaped(craft) => {
                        death::play_destroyed_escape_audio(&mut sound);
                        pending_log.add(Source::Death, format!("{} launched.", craft.name()));
                    }
                    Outcome::Destroyed => {
                        death::play_destroyed_audio(&mut sound);
                    }
                }
            }
        }
        let autopilot = nav.as_ref().is_some_and(|nav| nav.target.is_some());
        // F1099 prints DATA_009124, the parent object's name: at the new game it
        // is Hope, the planet under Old Blackelk. A surface landing names the
        // body itself; in free flight the port falls back to the system.
        let parent_name = nav
            .as_ref()
            .and_then(|nav| {
                let index = nav.docked.or_else(|| nav.landed_body())?;
                let body = &nav.bodies[index];
                Some(
                    body.parent
                        .and_then(|p| nav.bodies.get(p))
                        .map_or_else(|| body.name.clone(), |parent| parent.name.clone()),
                )
            })
            .unwrap_or_else(|| {
                nav.as_ref()
                    .map_or(String::new(), |nav| nav.system_name().to_owned())
            });
        // F1099's speed words are the original's ~0.1 m/s units; the flight model
        // reports m/s, so scale by `1000 / MM_PER_SECOND` before F1393.
        let source_speed =
            |metres: f64| (metres * 1000.0 / ffe_rs::original_flight::MM_PER_SECOND).round() as i32;
        screen.hud = PlayerHud {
            shields: state.shields,
            hull: state.hull,
            set_speed: views
                .console
                .set_speed
                .map(|speed| source_speed(f64::from(speed))),
            actual_speed: source_speed(state.speed()),
            altitude: Some(0),
            registration: parent_name,
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
            // DATA_007246/7247 are the armed targetters (F1054's T/Shift+T);
            // DATA_008877 (`player+0xef`) lights the missile-view icon while a
            // missile of the player's is in flight.
            data_246: targeter.navigation,
            data_247: targeter.combat,
            ef: u8::from(nav.as_ref().is_some_and(|nav| nav.missile_view.is_some())),
            general: state.loadout.general,
            // `player+0x174` is the cargo array's Chaff word (0x134 + 2*32);
            // F1120's 0x51 lamp and F1054's `c` read the same count.
            p174: state
                .hold
                .get(ffe_core::market::CHAFF)
                .copied()
                .unwrap_or(0),
            ..screen.systems
        };
        screen.clock = nav.as_ref().map(|nav| nav.date_text()).unwrap_or_default();
        // F271: one source-tagged game event per frame becomes a dated log
        // entry. `pending_log` never receives port-only notices (unported
        // buttons, view names, pause/status toggles, debug text); those stay in
        // `message` and are not dated. F351's bong accompanies a newly shown
        // message, not a replace-flag update (`ffe.asm:20776-20792`).
        if let Some(event) = pending_log.take() {
            if log_channel.post(&mut screen.messages, event, &screen.clock) == Post::Added {
                sound.play(Cue::Message);
            }
        }
        // F273's auto-scroll timer runs on the world tics.
        let log_tics =
            (elapsed * 50.0 * f64::from(ffe_rs::console::TIME_ACCELERATION[acceleration])).max(0.0)
                as u32;
        screen.messages.tick(log_tics);
        // F273's Page Up/Down scroll the message log while flying (the original's
        // keyboard map: "PgUp/Dn Scroll the Message Log").
        if active
            && !death_control
            && !paused
            && state.mode == ffe_rs::flight::Mode::Flight
            && nav.as_ref().is_none_or(|nav| nav.view == NavView::Closed)
            && model_lab.is_none()
            && model_viewer.is_none()
        {
            let older = i32::from(window.is_key_pressed(Key::PageUp, KeyRepeat::Yes)) * 8;
            let newer = i32::from(window.is_key_pressed(Key::PageDown, KeyRepeat::Yes)) * 8;
            if older != newer {
                screen.scroll_messages(older - newer);
            }
        }
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
            screen.draw(&inside, views, scene.as_ref(), prefs.modern, &message);
        } else {
            screen.draw(&state, views, scene.as_ref(), prefs.modern, &message);
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
        } else if let Some(dir) = &options.capture_start {
            if frames % 10 == 0 {
                write_shot(dir, &format!("start-{frames:04}.ppm"), &screen);
            }
        }
        if let Some(viewer) = &model_viewer {
            screen.draw_model_viewer(viewer);
        } else if let Some(lab) = &model_lab {
            screen.draw_model_lab(lab)?;
        }
        if let Some(nav) = nav.as_ref().filter(|nav| {
            !death_control
                && state.mode == ffe_rs::flight::Mode::Flight
                && nav.view != NavView::Closed
                && galaxy.is_none()
        }) {
            let ship = original
                .as_ref()
                .map_or([0; 3], |model| model.absolute_position());
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
                        range_label(ship, body.position).to_ascii_uppercase(),
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
        presenter.present(&mut window, screen.logical_pixels(), &prefs.display)?;
        frames += 1;
        let arrived = options.capture_planet.is_some()
            && nav.as_ref().is_some_and(|nav| nav.target.is_none());
        if options.smoke_frames.is_some_and(|limit| frames >= limit)
            || (options.capture.is_some() && frames >= 1000)
            || takeoff_done_at.is_some_and(|done| frames >= done + 100)
            || (options.capture_planet.is_some() && (frames >= 6000 || arrived))
            || (options.capture_traffic.is_some() && frames >= 80)
            || (capture_start
                && (start_clear_at.is_some_and(|done| frames >= done + 40) || frames >= 1_500))
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
    // F408/F410 store the fitted drive at `ship+0xd0`; a model's statistics word
    // only supplies the delivered class (F1533/F441), and old saves before the
    // Loadout drive field kept it in the combat word.
    let class = if state.loadout.drive != 0 {
        i32::from(state.loadout.drive)
    } else if state.combat.drive != 0 {
        i32::from(state.combat.drive)
    } else if stats.drive < 0 {
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

/// The models a generated station can have (F892's tables): their docking faces
/// are read from their vertices at start-up.
const STATION_MODELS: [u32; 10] = [69, 72, 73, 74, 75, 76, 77, 78, 79, 80];

/// Whether F152's game clock runs this frame: the simulation is live, no
/// preference page or death/game-over screen owns it, and the developer model
/// tools are closed. Pausing (F224 or the meter) stops time; losing focus is the
/// port's pause-without-catch-up rule. The clock is not tied to the flight model:
/// it also runs on the pad, the landed view and the station/services screens.
fn clock_runs(
    active: bool,
    paused: bool,
    prefs_open: bool,
    death: bool,
    model_lab: bool,
    model_viewer: bool,
) -> bool {
    active && !paused && !prefs_open && !death && !model_lab && !model_viewer
}

/// The current absolute positions of the planet and starport a new-game pad
/// belongs to, for the pad world that follows the navigator's clock.
fn pad_bodies(nav: &ffe_rs::navigation::Navigator) -> Option<([i64; 3], [i64; 3])> {
    let station = nav.bodies.get(nav.docked?)?;
    let planet = station.parent.and_then(|parent| nav.bodies.get(parent))?;
    Some((planet.position, station.position))
}

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
        |model| model.absolute_position(),
    )
}

/// F942/F1104 on the player: spawn the destruction object at the old position,
/// then run F1104's F1098 test. With an escape craft fitted the player is moved
/// into it (F1098) and keeps flying; otherwise the external-view death hold
/// starts and [`Flight::death`] carries it to the game-over tombstone.
///
/// This replaces the earlier substitute that recovered the player at the
/// nearest starport.
fn playership_destroyed(
    state: &mut Flight,
    original: &mut Option<OriginalFlight>,
    nav: &mut Navigator,
    scene: Option<&ModelViewer>,
    views: &mut ViewState,
    acceleration: usize,
) -> Outcome {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let position = ship_position(original, state);
    let velocity = state.velocity.map(|value| value * unit);
    let axes = ship_axes(state);
    let stats = scene.and_then(|scene| scene.ship_stats(state.ship));
    let cargo = stats.map_or(0, |stats| stats.cargo.max(0) as u16);
    nav.effects.extend(death::player_explosion(
        position,
        velocity,
        cargo,
        axes,
        state.ticks as f64 * STEP,
    ));
    let craft = death::escape_craft(&state.loadout);
    let craft_stats =
        craft.and_then(|craft| scene.and_then(|scene| scene.ship_stats(craft.model())));
    let outcome = death::begin(state, craft_stats.as_ref());
    match outcome {
        Outcome::Escaped(_) => {
            // The new object is the player's ship now: rebuild the original
            // flight model around the capsule/fighter's statistics, keeping the
            // old position and velocity F1098 copied.
            if original.is_some() {
                *original = Some(
                    OriginalFlight::from_flight(state, craft_stats.as_ref())
                        .with_time_acceleration(acceleration),
                );
            }
            views.console.status_page = false;
        }
        Outcome::Destroyed => {
            // F1104's `DATA_008872 = 4`: watch the explosion from outside.
            views.mode = ViewMode::External;
        }
    }
    outcome
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
        free_space: trade::capacity(nav, cache, state) - used,
        // F394 reads the statistics' +5 bit 0; no statistics means no refusal.
        fuel_scoop_mounting: stats.is_none_or(|stats| stats.fuel_scoop_mounting),
        // F410/F411 refuse to remove a drive whose statistics word is negative
        // (the integral part of the hull).
        drive_removable: stats.is_none_or(|stats| stats.drive >= 0),
    };
    let result = if remove {
        outfitting::remove(
            entry.index,
            tech_level,
            &mut state.loadout,
            &mut state.combat,
            &mut state.credits,
            limits,
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

/// The BBS and hull-list counts of the station the player is docked at, for the
/// console hint (0/0 when there is no navigator).
/// F453's bulletin-board row action, for a mouse click or Enter: F454 opens
/// F536's black market when the advertisement is 0x9829 and the system has an
/// illegal commodity; otherwise the advertisement's full text goes to the
/// console. Returns the console line.
fn open_bulletin_post(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    station: usize,
    index: usize,
    dock: &mut DockState,
) -> Option<String> {
    let illegal = trade::illegal_available(nav, cache).unwrap_or(false);
    let post = nav
        .station_offers(cache, station)
        .ok()?
        .posts
        .get(index)?
        .clone();
    if post.code == 0x9829 && illegal {
        dock.open_black_market(post.fake, ffe_core::bulletin::post_seed(&post));
        Some(station::MARKET_HINT.to_owned())
    } else {
        Some(post.text)
    }
}

fn police_action(nav: Option<&Navigator>) -> StationAction {
    if nav.is_some_and(|nav| nav.fine_here() > 0) {
        StationAction::PayFine
    } else {
        StationAction::Back
    }
}

fn police_message(nav: Option<&Navigator>) -> String {
    let Some(nav) = nav else {
        return station::POLICE_HINT.to_owned();
    };
    let fine = nav.fine_here();
    if fine > 0 {
        ffe_core::status::police_greeting(nav.system_name(), fine)
    } else {
        "You have no outstanding fine in this jurisdiction.".to_owned()
    }
}

fn station_offer_counts(
    nav: Option<&mut Navigator>,
    cache: &ffe_content::VerifiedCache,
    ship: u32,
) -> (usize, usize) {
    let Some(nav) = nav else {
        return (0, 0);
    };
    let Some(station) = nav.docked else {
        return (0, 0);
    };
    match nav.station_offers(cache, station) {
        Ok(offers) => (offers.posts.len(), offers.rows(ship, None).len()),
        Err(_) => (0, 0),
    }
}

/// F438/F442/F441's buy: part exchange the current ship for row `row`, charge
/// ten times the net whole-credit price, install the new model and clear the
/// fitted items (the screen's hint: "a ship with no extra equipment"). F442
/// leaves the old hull in the station list; F441 resets the ship's damage and
/// equipment, which the port mirrors here.
fn buy_ship(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    state: &mut Flight,
    row: usize,
) -> String {
    let Some(station) = nav.docked else {
        return "Not docked.".to_owned();
    };
    let result = match nav.station_offers(cache, station) {
        Ok(offers) => offers.buy(row, state.ship, i64::from(state.credits)),
        Err(error) => return error,
    };
    match result {
        Ok(purchase) => {
            state.credits = u32::try_from(purchase.cash_tenths).unwrap_or(0);
            state.ship = purchase.model;
            state.loadout = ffe_core::equipment::Loadout::default();
            // The new hull's hold is no longer F998's new-game 1 t: F441 clears
            // the equipment and F2 quotes the new model's cargo statistic.
            nav.new_game = None;
            state.combat = Combat::default();
            state.hull = [0, 0];
            state.shields = [0, 0];
            "Congratulations on your purchase. Call our upgrade department for the latest in additional equipment.".to_owned()
        }
        Err(text) => text,
    }
}

/// F438's right button (F439's View page): the port shows the statistics the
/// page quotes on the console rather than adding a second screen. The values
/// are the shipyard table's, not invented.
fn view_ship(
    nav: &mut Navigator,
    cache: &ffe_content::VerifiedCache,
    state: &Flight,
    row: usize,
) -> String {
    let Some(station) = nav.docked else {
        return "Not docked.".to_owned();
    };
    match nav.station_offers(cache, station) {
        Ok(offers) => {
            let hulls = offers.rows(state.ship, None);
            hulls.get(row).map_or_else(
                || "No such ship.".to_owned(),
                |hull| {
                    format!(
                        "{}: {}t mass, {}t capacity, {} gun mount(s), {} missile pylon(s), drive class {}.",
                        hull.name, hull.mass, hull.cargo, hull.gun_mounts, hull.missiles, hull.drive
                    )
                },
            )
        }
        Err(error) => error,
    }
}

/// F444's rows for the player's ship: the statistics' mass, the drive byte
/// (`ship+0xd0`), the hull words and the damage state (thruster words and
/// damaged general bits). The date-only form is what the 0x9816/0x9817
/// templates print.
fn station_repairs(state: &Flight, scene: Option<&ModelViewer>) -> Vec<station::RepairRow> {
    let mass = scene
        .and_then(|scene| scene.ship_stats(state.ship))
        .map_or(0, |stats| u32::from(stats.price_mass));
    let date = |day: u32| {
        ffe_rs::galaxy_map::date_text(f64::from(day) * 86_400.0)
            .split(' ')
            .next()
            .unwrap_or("")
            .to_owned()
    };
    station::repairs_for(
        mass,
        state.combat.drive != 0,
        state.hull,
        state.damage.equipment,
        state.damage.damaged_thrusters(),
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
