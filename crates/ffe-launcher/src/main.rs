//! The ffe-rs launcher: find and verify the game files and the optional
//! FFED3D resources, edit the game's settings and start the game. The look
//! follows the approved launcher mockup (`assets/`): a 1536x1024 design
//! canvas, scaled to the window, over the cleaned mockup artwork.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod checks;
mod config;
mod draw;

use checks::{Level, Report};
use config::Launcher;
use draw::{Icon, CREAM, CYAN, DIM, OLIVE, PANEL, VERIFIED};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, TextureHandle,
    UiBuilder, Vec2,
};
use ffe_rs::display::{Filter, FitMode, PixelAspect};
use ffe_rs::modern::Graphics;
use ffe_rs::prefs;
use ffe_rs::settings::{self, Config};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver};

/// The design canvas (the mockup's size).
const CANVAS: Vec2 = vec2(1536.0, 1024.0);
const WARNING: Color32 = Color32::from_rgb(240, 190, 70);
const FAILED: Color32 = Color32::from_rgb(255, 96, 80);

/// The game's console output from the last launch.
const GAME_LOG: &str = "last-run.log";
/// The launcher's own errors (a Windows GUI program has no console).
const LAUNCHER_LOG: &str = "launcher.log";

fn main() {
    // A GUI program on Windows has no console: a panic would vanish without
    // a word. Log it beside the saves and show it.
    std::panic::set_hook(Box::new(|info| {
        let message = format!("The launcher stopped unexpectedly:\n\n{info}");
        report_fatal(&message);
    }));
    // OpenGL first; Windows falls back to Direct3D 12 / Vulkan (wgpu) where
    // no OpenGL 2 driver exists (virtual machines, Remote Desktop, basic
    // display adapters).
    #[cfg(windows)]
    let renderers = [eframe::Renderer::Glow, eframe::Renderer::Wgpu];
    #[cfg(not(windows))]
    let renderers = [eframe::Renderer::Glow];
    let mut failures = Vec::new();
    for renderer in renderers {
        let options = eframe::NativeOptions {
            renderer,
            viewport: egui::ViewportBuilder::default()
                .with_title("Frontier: First Encounters (Elite 3) Launcher")
                .with_inner_size([1152.0, 768.0])
                .with_min_inner_size([768.0, 512.0])
                .with_icon(draw::window_icon()),
            ..Default::default()
        };
        match eframe::run_native(
            "ffe-rs Launcher",
            options,
            Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
        ) {
            Ok(()) => return,
            Err(error) => failures.push(format!("{renderer}: {error}")),
        }
    }
    report_fatal(&format!(
        "The launcher could not open its window. Updating the graphics driver usually fixes this.\n\n{}",
        failures.join("\n")
    ));
    std::process::exit(1);
}

/// Write `message` to `saves/launcher.log` and show it (a message box on
/// Windows and macOS, standard error elsewhere).
fn report_fatal(message: &str) {
    let saves = config::find_root().join("saves");
    let _ = std::fs::create_dir_all(&saves);
    let _ = std::fs::write(saves.join(LAUNCHER_LOG), message);
    eprintln!("{message}");
    #[cfg(any(windows, target_os = "macos"))]
    {
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Frontier: First Encounters Launcher")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Play,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Port,
    Display,
    Game,
    Sound,
    Files,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Game,
    Data,
    Ffed3d,
    Build,
}

struct App {
    root: PathBuf,
    saves: PathBuf,
    launcher: Launcher,
    config: Config,
    page: Page,
    tab: Tab,
    game_text: String,
    ffed3d_text: String,
    game_report: Option<Report>,
    data_report: Option<Report>,
    ffed3d_report: Option<Report>,
    jobs: Vec<(Job, Receiver<Report>)>,
    /// Build `data.bin` once the next game verification passes.
    build_after_verify: bool,
    child: Option<Child>,
    notice: Option<(Level, String)>,
    background: TextureHandle,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        draw::fonts(ctx);
        draw::style(ctx);
        let background = draw::background(ctx);
        let root = config::find_root();
        let saves = root.join("saves");
        let launcher = Launcher::load(&saves, &root);
        let config = settings::load_config(&saves).unwrap_or_default();
        let mut app = Self {
            game_text: launcher.game.display().to_string(),
            ffed3d_text: launcher
                .ffed3d
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            root,
            saves,
            launcher,
            config,
            page: Page::Play,
            tab: Tab::Port,
            game_report: None,
            data_report: None,
            ffed3d_report: None,
            jobs: Vec::new(),
            build_after_verify: true,
            child: None,
            notice: None,
            background,
        };
        app.start(Job::Data);
        app.start(Job::Game);
        if app.launcher.ffed3d.is_some() {
            app.start(Job::Ffed3d);
        }
        app
    }

    fn data_path(&self) -> PathBuf {
        self.root.join("data.bin")
    }

    fn busy(&self, job: Job) -> bool {
        self.jobs.iter().any(|(j, _)| *j == job)
    }

    fn start(&mut self, job: Job) {
        if self.busy(job) {
            return;
        }
        let (send, receive) = mpsc::channel();
        let game = self.launcher.game.clone();
        let data = self.data_path();
        let ffed3d = self.launcher.ffed3d.clone();
        std::thread::spawn(move || {
            let report = match job {
                Job::Game => checks::verify_game(&game),
                Job::Data => checks::verify_data(&data),
                Job::Ffed3d => match ffed3d {
                    Some(folder) => checks::verify_ffed3d(&folder),
                    None => return,
                },
                Job::Build => checks::build_data(&game, &data),
            };
            let _ = send.send(report);
        });
        self.jobs.push((job, receive));
    }

    fn data_ready(&self) -> bool {
        self.data_report
            .as_ref()
            .is_some_and(|report| report.level == Level::Good)
    }

    fn game_ok(&self) -> bool {
        self.game_report
            .as_ref()
            .is_some_and(|report| report.level != Level::Bad)
    }

    fn poll(&mut self) {
        let mut finished = Vec::new();
        self.jobs.retain(|(job, receive)| match receive.try_recv() {
            Ok(report) => {
                finished.push((*job, report));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (job, report) in finished {
            match job {
                Job::Game => self.game_report = Some(report),
                Job::Data => self.data_report = Some(report),
                Job::Ffed3d => self.ffed3d_report = Some(report),
                Job::Build => {
                    self.notice = Some((report.level, report.summary.clone()));
                    self.data_report = None;
                    self.start(Job::Data);
                }
            }
        }
        // Verified game files and a missing or outdated cache: build it.
        if self.build_after_verify
            && self.game_ok()
            && !self.busy(Job::Data)
            && !self.busy(Job::Game)
            && self.data_report.is_some()
        {
            self.build_after_verify = false;
            if !self.data_ready() {
                self.start(Job::Build);
            }
        }
        if let Some(child) = &mut self.child {
            if let Ok(Some(status)) = child.try_wait() {
                self.notice = Some(if status.success() {
                    (Level::Good, "The game has closed.".to_owned())
                } else {
                    // The game prints its reason last (see About > game log).
                    let reason = std::fs::read_to_string(self.saves.join(GAME_LOG))
                        .ok()
                        .and_then(|log| {
                            log.lines()
                                .map(str::trim)
                                .rfind(|line| !line.is_empty() && !line.starts_with("Press Enter"))
                                .map(str::to_owned)
                        });
                    match reason {
                        Some(reason) => (Level::Bad, format!("The game stopped: {reason}")),
                        None => (Level::Bad, format!("The game stopped with {status}.")),
                    }
                });
                self.child = None;
                // The game saves its own options; pick up what it changed.
                if let Some(config) = settings::load_config(&self.saves) {
                    self.config = config;
                }
            }
        }
    }

    fn verify_game_folder(&mut self, path: PathBuf) {
        self.launcher.game = path;
        self.game_text = self.launcher.game.display().to_string();
        self.save_launcher();
        self.game_report = None;
        self.build_after_verify = true;
        self.start(Job::Game);
    }

    fn verify_ffed3d_folder(&mut self, path: Option<PathBuf>) {
        self.launcher.ffed3d = path;
        self.ffed3d_text = self
            .launcher
            .ffed3d
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.save_launcher();
        self.ffed3d_report = None;
        if self.launcher.ffed3d.is_some() {
            self.start(Job::Ffed3d);
        }
    }

    fn save_launcher(&mut self) {
        if let Err(error) = self.launcher.save(&self.saves) {
            self.notice = Some((Level::Bad, format!("Could not save launcher.cfg: {error}")));
        }
    }

    fn save_config(&mut self) {
        let result = std::fs::create_dir_all(&self.saves)
            .map_err(|e| e.to_string())
            .and_then(|()| settings::save_config(&self.saves, &self.config));
        if let Err(error) = result {
            self.notice = Some((Level::Bad, format!("Could not save config.cfg: {error}")));
        }
    }

    fn launch(&mut self) {
        let Some(game) = config::game_executable() else {
            self.notice = Some((
                Level::Bad,
                "ffe-rs was not found beside the launcher.".to_owned(),
            ));
            return;
        };
        let mut command = Command::new(game);
        command.arg("--root").arg(&self.root);
        command.arg("--start").arg(self.launcher.start.to_string());
        if self.launcher.skip_intro {
            command.arg("--skip-intro");
        }
        if self.launcher.record {
            let dir = self.root.join("recordings");
            let _ = std::fs::create_dir_all(&dir);
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            command
                .arg("--record")
                .arg(dir.join(format!("{}.ffe-input", config::stamp(seconds))));
        }
        command.current_dir(&self.root);
        // No console window: the game's messages go to saves/last-run.log,
        // and with no terminal on stdin it exits at once on an error instead
        // of waiting for Enter.
        let _ = std::fs::create_dir_all(&self.saves);
        command.stdin(std::process::Stdio::null());
        if let Ok(log) = std::fs::File::create(self.saves.join(GAME_LOG)) {
            if let Ok(copy) = log.try_clone() {
                command.stdout(copy);
            }
            command.stderr(log);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        match command.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.notice = Some((Level::Good, "The game is running.".to_owned()));
            }
            Err(error) => {
                self.notice = Some((Level::Bad, format!("Could not start the game: {error}")));
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if !self.jobs.is_empty() || self.child.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // Scale the design canvas to the window (letterboxed).
        let native = ctx.input(|i| i.screen_rect().size()) * ctx.zoom_factor();
        let zoom = (native.x / CANVAS.x).min(native.y / CANVAS.y).max(0.3);
        if (zoom - ctx.zoom_factor()).abs() > 0.001 {
            ctx.set_zoom_factor(zoom);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let screen = ui.max_rect();
                let canvas = Rect::from_center_size(screen.center(), CANVAS);
                let at = |x: f32, y: f32| canvas.min + vec2(x, y);
                let rect =
                    |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
                ui.painter().image(
                    self.background.id(),
                    canvas,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                self.sidebar(ui, &rect);
                match self.page {
                    Page::Play => self.play(ui, &rect),
                    Page::Settings => self.settings(ui, rect(309.0, 372.0, 1515.0, 888.0)),
                    Page::About => self.about(ui, rect(309.0, 372.0, 1515.0, 888.0)),
                }
                self.footer(ui, &at);
            });
    }
}

// --- Frame: sidebar and footer -------------------------------------------

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui, rect: &dyn Fn(f32, f32, f32, f32) -> Rect) {
        let painter = ui.painter().clone();
        // The sidebar's own translucent olive panel.
        draw::vertical_gradient(
            &painter,
            rect(22.0, 372.0, 287.0, 945.0),
            &[
                (0.0, Color32::from_rgba_unmultiplied(26, 34, 8, 210)),
                (0.6, Color32::from_rgba_unmultiplied(10, 16, 8, 200)),
                (1.0, Color32::from_rgba_unmultiplied(30, 40, 8, 120)),
            ],
        );
        let frame = rect(22.0, 372.0, 287.0, 945.0);
        painter.line_segment(
            [frame.left_top(), frame.left_bottom()],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(220, 225, 200, 70)),
        );
        painter.line_segment(
            [frame.right_top(), frame.right_bottom()],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(220, 225, 200, 70)),
        );
        for (index, (page, label, icon)) in [
            (Page::Play, "Play", Icon::Play),
            (Page::Settings, "Settings", Icon::Settings),
            (Page::About, "About", Icon::Info),
        ]
        .into_iter()
        .enumerate()
        {
            let top = 374.0 + 62.0 * index as f32;
            let item = rect(24.0, top, 285.0, top + 60.0);
            let response = ui.interact(item, ui.id().with(("nav", index)), Sense::click());
            let selected = self.page == page;
            if selected {
                draw::vertical_gradient(
                    &painter,
                    item,
                    &[
                        (0.0, Color32::from_rgb(150, 164, 20)),
                        (0.5, Color32::from_rgb(98, 112, 6)),
                        (1.0, Color32::from_rgb(44, 54, 2)),
                    ],
                );
                painter.line_segment(
                    [item.left_top(), item.right_top()],
                    Stroke::new(1.5_f32, CREAM),
                );
                painter.line_segment(
                    [item.left_bottom(), item.right_bottom()],
                    Stroke::new(1.5_f32, CREAM),
                );
            } else {
                if response.hovered() {
                    painter.rect_filled(
                        item,
                        CornerRadius::ZERO,
                        Color32::from_rgba_unmultiplied(120, 130, 30, 40),
                    );
                }
                painter.line_segment(
                    [
                        pos2(item.left() + 16.0, item.bottom()),
                        pos2(item.right() - 6.0, item.bottom()),
                    ],
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(220, 225, 200, 60)),
                );
            }
            let centre = pos2(item.left() + 44.0, item.center().y);
            draw::icon(
                &painter,
                icon,
                Rect::from_center_size(centre, Vec2::splat(36.0)),
                CREAM,
            );
            painter.text(
                pos2(item.left() + 86.0, item.center().y),
                Align2::LEFT_CENTER,
                label,
                draw::font(26.0, draw::BOLD),
                CREAM,
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.page = page;
            }
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, at: &dyn Fn(f32, f32) -> egui::Pos2) {
        let painter = ui.painter();
        painter.text(
            at(1508.0, 978.0),
            Align2::RIGHT_CENTER,
            format!(
                "Frontier: First Encounters (Elite 3) Launcher v{}",
                env!("CARGO_PKG_VERSION")
            ),
            draw::font(19.0, draw::ITALIC),
            CREAM,
        );
        if let Some((level, text)) = &self.notice {
            painter.text(
                at(330.0, 978.0),
                Align2::LEFT_CENTER,
                text,
                draw::font(19.0, draw::ITALIC),
                level_colour(*level),
            );
        }
    }
}

fn level_colour(level: Level) -> Color32 {
    match level {
        Level::Good => VERIFIED,
        Level::Warning => WARNING,
        Level::Bad => FAILED,
    }
}

// --- Play ----------------------------------------------------------------

/// The status column of a file row.
struct Status {
    icon: Icon,
    text: String,
    colour: Color32,
    busy: bool,
}

impl App {
    fn game_status(&self) -> Status {
        let busy = |text: &str| Status {
            icon: Icon::Circle,
            text: text.to_owned(),
            colour: CREAM,
            busy: true,
        };
        if self.busy(Job::Game) {
            return busy("Verifying...");
        }
        if self.busy(Job::Build) {
            return busy("Preparing data...");
        }
        match &self.game_report {
            None if !self.launcher.game.is_dir() => Status {
                icon: Icon::Circle,
                text: "Not configured".to_owned(),
                colour: CREAM,
                busy: false,
            },
            None => busy("Verifying..."),
            Some(report) if report.level == Level::Bad => Status {
                icon: Icon::Failed,
                text: if self.launcher.game.join("FIRSTENC.EXE").is_file() {
                    "Not recognised"
                } else {
                    "Not found"
                }
                .to_owned(),
                colour: FAILED,
                busy: false,
            },
            Some(_) if self.busy(Job::Data) || self.data_report.is_none() => {
                busy("Checking data...")
            }
            Some(_) if !self.data_ready() => Status {
                icon: Icon::Warning,
                text: "Build data".to_owned(),
                colour: WARNING,
                busy: false,
            },
            Some(_) => Status {
                icon: Icon::Verified,
                text: "Verified".to_owned(),
                colour: VERIFIED,
                busy: false,
            },
        }
    }

    fn ffed3d_status(&self) -> Status {
        if self.busy(Job::Ffed3d) {
            return Status {
                icon: Icon::Circle,
                text: "Verifying...".to_owned(),
                colour: CREAM,
                busy: true,
            };
        }
        match (&self.launcher.ffed3d, &self.ffed3d_report) {
            (None, _) => Status {
                icon: Icon::Circle,
                text: "Not configured".to_owned(),
                colour: CREAM,
                busy: false,
            },
            (Some(_), None) => Status {
                icon: Icon::Circle,
                text: "Not verified".to_owned(),
                colour: CREAM,
                busy: false,
            },
            (Some(_), Some(report)) => match report.level {
                Level::Good => Status {
                    icon: Icon::Verified,
                    text: "Verified".to_owned(),
                    colour: VERIFIED,
                    busy: false,
                },
                Level::Warning => Status {
                    icon: Icon::Warning,
                    text: "Incomplete".to_owned(),
                    colour: WARNING,
                    busy: false,
                },
                Level::Bad => Status {
                    icon: Icon::Failed,
                    text: "Not found".to_owned(),
                    colour: FAILED,
                    busy: false,
                },
            },
        }
    }

    fn play(&mut self, ui: &mut egui::Ui, rect: &dyn Fn(f32, f32, f32, f32) -> Rect) {
        // The required game files.
        let status = self.game_status();
        let action = self.file_row(
            ui,
            rect(309.0, 578.0, 1515.0, 660.0),
            Icon::Disc,
            ("Frontier: First Encounters (Elite 3)", ""),
            "Original game files (required)",
            &status,
            &self.launcher.game.clone(),
            "game",
        );
        match action {
            RowAction::Status if status.text == "Build data" && self.child.is_none() => {
                self.start(Job::Build)
            }
            RowAction::Status => self.page = Page::About,
            RowAction::Browse(path) => self.verify_game_folder(path),
            RowAction::None => {}
        }
        // The optional FFED3D resources.
        let status = self.ffed3d_status();
        let folder = self.launcher.ffed3d.clone().unwrap_or_default();
        let action = self.file_row(
            ui,
            rect(309.0, 676.0, 1515.0, 758.0),
            Icon::Cube,
            ("FFED3D Enhancement", " (Optional)"),
            "High resolution models, textures and additions",
            &status,
            &folder,
            "ffed3d",
        );
        match action {
            RowAction::Status => self.page = Page::About,
            RowAction::Browse(path) => self.verify_ffed3d_folder(Some(path)),
            RowAction::None => {}
        }
        // LAUNCH.
        let ready = self.data_ready() && self.game_ok() && self.child.is_none();
        let button = rect(320.0, 782.0, 1508.0, 888.0);
        let response = ui.interact(
            button,
            ui.id().with("launch"),
            if ready {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        draw::launch_button(
            ui.painter(),
            button,
            ready,
            response.hovered() && ready,
            response.is_pointer_button_down_on(),
        );
        let response = if ready {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            let reason = if self.child.is_some() {
                "The game is running."
            } else if !self.game_ok() {
                "Choose and verify your game files first."
            } else {
                "The content cache is not ready yet."
            };
            response.on_hover_text(reason)
        };
        if response.clicked() {
            self.launch();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn file_row(
        &self,
        ui: &mut egui::Ui,
        row: Rect,
        icon: Icon,
        title: (&str, &str),
        subtitle: &str,
        status: &Status,
        folder: &Path,
        id: &str,
    ) -> RowAction {
        let painter = ui.painter().clone();
        draw::chamfer_panel(
            &painter,
            row,
            14.0,
            PANEL,
            Stroke::new(1.5_f32, Color32::from_rgb(215, 222, 210)),
        );
        let mid = row.center().y;
        draw::icon(
            &painter,
            icon,
            Rect::from_center_size(pos2(row.left() + 69.0, mid), Vec2::splat(58.0)),
            CREAM,
        );
        // Title (bold) with an optional regular suffix, then the italic subtitle.
        let title_pos = pos2(row.left() + 132.0, mid - 15.0);
        let galley =
            painter.layout_no_wrap(title.0.to_owned(), draw::font(25.0, draw::BOLD), CREAM);
        let width = galley.size().x;
        painter.galley(title_pos - vec2(0.0, galley.size().y / 2.0), galley, CREAM);
        if !title.1.is_empty() {
            painter.text(
                pos2(title_pos.x + width, title_pos.y),
                Align2::LEFT_CENTER,
                title.1,
                draw::font(25.0, draw::REGULAR),
                CREAM,
            );
        }
        painter.text(
            pos2(row.left() + 132.0, mid + 15.0),
            Align2::LEFT_CENTER,
            subtitle,
            draw::font(20.0, draw::ITALIC),
            CREAM,
        );
        let mut action = RowAction::None;
        // Status: icon and text; click for the details (or to build data).
        let status_rect = Rect::from_min_max(
            pos2(row.left() + 722.0, mid - 22.0),
            pos2(row.left() + 930.0, mid + 22.0),
        );
        let response = ui.interact(status_rect, ui.id().with((id, "status")), Sense::click());
        let icon_rect = Rect::from_center_size(pos2(row.left() + 743.0, mid), Vec2::splat(34.0));
        if status.busy {
            let t = ui.input(|i| i.time) as f32;
            draw::spinner(&painter, icon_rect, t, CYAN);
            ui.ctx().request_repaint();
        } else {
            draw::icon(&painter, status.icon, icon_rect, status.colour);
        }
        painter.text(
            pos2(row.left() + 775.0, mid),
            Align2::LEFT_CENTER,
            &status.text,
            draw::font(21.0, draw::REGULAR),
            status.colour,
        );
        if !status.busy
            && response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Details")
                .clicked()
        {
            action = RowAction::Status;
        }
        // Open the folder, and Browse.
        let open = Rect::from_min_max(
            pos2(row.left() + 946.0, mid - 21.0),
            pos2(row.left() + 1012.0, mid + 21.0),
        );
        let response = ui.interact(open, ui.id().with((id, "open")), Sense::click());
        draw::small_button(&painter, open, response.hovered());
        draw::icon(
            &painter,
            Icon::Folder,
            Rect::from_center_size(open.center(), Vec2::splat(30.0)),
            CREAM,
        );
        let tooltip = if folder.as_os_str().is_empty() {
            "No folder chosen".to_owned()
        } else {
            format!("Open {}", folder.display())
        };
        if response.on_hover_text(tooltip).clicked() && folder.is_dir() {
            open_folder(folder);
        }
        let browse = Rect::from_min_max(
            pos2(row.left() + 1036.0, mid - 21.0),
            pos2(row.left() + 1181.0, mid + 21.0),
        );
        let response = ui.interact(browse, ui.id().with((id, "browse")), Sense::click());
        draw::small_button(&painter, browse, response.hovered());
        painter.text(
            browse.center(),
            Align2::CENTER_CENTER,
            "Browse...",
            draw::font(21.0, draw::REGULAR),
            CREAM,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            if let Some(path) = pick_folder(
                if id == "game" {
                    "Choose the folder with FIRSTENC.EXE"
                } else {
                    "Choose the FFED3D folder"
                },
                folder,
            ) {
                action = RowAction::Browse(path);
            }
        }
        action
    }
}

enum RowAction {
    None,
    Status,
    Browse(PathBuf),
}

// --- Settings ------------------------------------------------------------

impl App {
    fn settings(&mut self, ui: &mut egui::Ui, panel: Rect) {
        draw::chamfer_panel(
            ui.painter(),
            panel,
            16.0,
            Color32::from_rgba_unmultiplied(7, 18, 15, 236),
            Stroke::new(1.5_f32, Color32::from_rgb(215, 222, 210)),
        );
        let inner = panel.shrink2(vec2(30.0, 22.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Settings")
                        .font(draw::font(30.0, draw::BOLD))
                        .color(CREAM),
                );
                ui.add_space(20.0);
                for (tab, label) in [
                    (Tab::Port, "Port options"),
                    (Tab::Display, "Display"),
                    (Tab::Game, "Game options"),
                    (Tab::Sound, "Sound"),
                    (Tab::Files, "Files & start"),
                ] {
                    if ui.add(pill(label, self.tab == tab)).clicked() {
                        self.tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            let running = self.child.is_some();
            ui.label(
                RichText::new(if running {
                    "The game is running: it saves its own options when it closes."
                } else {
                    "Saved to saves/config.cfg, the file the in-game options pages use."
                })
                .italics()
                .color(if running { WARNING } else { DIM }),
            );
            ui.add_space(6.0);
            let before = self.config.clone();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_enabled_ui(!running, |ui| match self.tab {
                        Tab::Port => self.port_tab(ui),
                        Tab::Display => self.display_tab(ui),
                        Tab::Game => self.game_tab(ui),
                        Tab::Sound => self.sound_tab(ui),
                        Tab::Files => self.files_tab(ui),
                    });
                    if self.tab != Tab::Files {
                        ui.add_space(10.0);
                        if ui
                            .add_enabled(!running, egui::Button::new("Restore defaults"))
                            .on_hover_text("Key bindings (bind.*) are kept.")
                            .clicked()
                        {
                            let remap = std::mem::take(&mut self.config.remap);
                            self.config = Config {
                                remap,
                                ..Config::default()
                            };
                        }
                    }
                });
            if self.config != before {
                self.save_config();
            }
        });
    }

    fn port_tab(&mut self, ui: &mut egui::Ui) {
        let config = &mut self.config;
        section(ui, "Combat");
        option_row(
            ui,
            "Combat aids",
            "Improved adds a lead indicator, target brackets, edge arrows and a contact list.",
            |ui| {
                choice(
                    ui,
                    &mut config.combat_improved,
                    &[(false, "Classic"), (true, "Improved")],
                );
            },
        );
        section(ui, "World");
        option_row(
            ui,
            "Space traffic",
            "How many ships fly in each system.",
            |ui| {
                combo(
                    ui,
                    "traffic",
                    &mut config.traffic,
                    &prefs::TRAFFIC_LABELS.map(str::to_owned),
                );
            },
        );
        option_row(
            ui,
            "Police",
            "Police patrols and their response to crimes.",
            |ui| {
                toggle(ui, &mut config.police);
            },
        );
        option_row(ui, "Pirates", "Pirate attacks.", |ui| {
            toggle(ui, &mut config.pirates);
        });
        option_row(
            ui,
            "Advanced traffic",
            "Approach lanes, convoys, hyperspace arrivals, NPC fights and busier rich systems.",
            |ui| {
                toggle(ui, &mut config.advanced_traffic);
            },
        );
        section(ui, "Docking");
        let km = |values: &[u32]| -> Vec<String> {
            values
                .iter()
                .enumerate()
                .map(|(i, km)| {
                    if i == 0 {
                        format!("{km} km (original)")
                    } else {
                        format!("{km} km")
                    }
                })
                .collect()
        };
        option_row(
            ui,
            "Station approach",
            "Where the autopilot asks for clearance at an orbital station.",
            |ui| {
                combo(
                    ui,
                    "station",
                    &mut config.station_approach,
                    &km(&prefs::STATION_APPROACH_KM),
                );
            },
        );
        option_row(
            ui,
            "Starport approach",
            "The same for a surface starport.",
            |ui| {
                combo(
                    ui,
                    "starport",
                    &mut config.starport_approach,
                    &km(&prefs::STARPORT_APPROACH_KM),
                );
            },
        );
    }

    fn display_tab(&mut self, ui: &mut egui::Ui) {
        let display = &mut self.config.display;
        section(ui, "Graphics");
        option_row(
            ui,
            "Engine",
            "Classic draws the original's pixels; Modern is the high-resolution engine.",
            |ui| {
                choice(
                    ui,
                    &mut display.graphics,
                    &[(Graphics::Classic, "Classic"), (Graphics::Modern, "Modern")],
                );
            },
        );
        option_row(
            ui,
            "Window size",
            "The starting window, in multiples of 320x240.",
            |ui| {
                let mut index = display.scale.clamp(1, 6) as usize - 1;
                if combo(
                    ui,
                    "scale",
                    &mut index,
                    &prefs::WINDOW_SCALE_LABELS.map(str::to_owned),
                ) {
                    display.scale = index as u32 + 1;
                }
            },
        );
        option_row(
            ui,
            "Fit",
            "Whole-pixel scaling with borders, or fill the window.",
            |ui| {
                choice(
                    ui,
                    &mut display.fit,
                    &[
                        (FitMode::PixelPerfect, prefs::FIT_LABELS[0]),
                        (FitMode::FillWindow, prefs::FIT_LABELS[1]),
                    ],
                );
            },
        );
        option_row(
            ui,
            "Filter",
            "Smoothing when the scale is not a whole number.",
            |ui| {
                choice(
                    ui,
                    &mut display.filter,
                    &[
                        (Filter::Nearest, prefs::FILTER_LABELS[0]),
                        (Filter::Linear, prefs::FILTER_LABELS[1]),
                    ],
                );
            },
        );
        option_row(
            ui,
            "Pixel aspect",
            "The original's 4:3 monitor, or square pixels.",
            |ui| {
                choice(
                    ui,
                    &mut display.aspect,
                    &[
                        (PixelAspect::Dos43, prefs::ASPECT_LABELS[0]),
                        (PixelAspect::Square, prefs::ASPECT_LABELS[1]),
                    ],
                );
            },
        );
        let config = &mut self.config;
        section(ui, "Picture");
        option_row(ui, "Shape detail", "Model detail level.", |ui| {
            combo(
                ui,
                "detail",
                &mut config.shape_detail,
                &prefs::SHAPE_DETAIL.map(str::to_owned),
            );
        });
        option_row(
            ui,
            "Space colour",
            "The background colour of space.",
            |ui| {
                combo(
                    ui,
                    "space",
                    &mut config.space_colour,
                    &prefs::SPACE_COLOURS.map(str::to_owned),
                );
            },
        );
    }

    fn game_tab(&mut self, ui: &mut egui::Ui) {
        let config = &mut self.config;
        section(ui, "Controls");
        option_row(
            ui,
            "Control keys",
            "Classic is the original's keys; Modern uses the arrows and WASD.",
            |ui| {
                choice(
                    ui,
                    &mut config.modern,
                    &[(false, "Classic"), (true, "Modern")],
                );
            },
        );
        section(ui, "Other options");
        toggle_grid(ui, "others", &mut config.others, &prefs::OTHER_LABELS);
        section(ui, "Galaxy map");
        toggle_grid(ui, "maps", &mut config.maps, &prefs::MAP_LABELS);
    }

    fn sound_tab(&mut self, ui: &mut egui::Ui) {
        section(ui, "Sound effects and music");
        toggle_grid(ui, "sound", &mut self.config.sound, &prefs::SOUND_LABELS);
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        section(ui, "New game");
        let mut changed = false;
        option_row(ui, "Start", "Where a new commander begins.", |ui| {
            for (code, name, _) in config::STARTS.iter().rev() {
                if ui.add(pill(name, self.launcher.start == *code)).clicked() {
                    self.launcher.start = *code;
                    changed = true;
                }
            }
        });
        option_row(ui, "Skip the intro", "Go straight to the game.", |ui| {
            changed |= toggle(ui, &mut self.launcher.skip_intro);
        });
        option_row(
            ui,
            "Record sessions",
            "Keep each session's input in recordings/, to replay a bug exactly.",
            |ui| {
                changed |= toggle(ui, &mut self.launcher.record);
            },
        );
        if changed {
            self.save_launcher();
        }
        section(ui, "Folders");
        ui.label(
            RichText::new("Type a path and press Enter, or use Browse on the Play page.")
                .color(DIM)
                .italics(),
        );
        ui.add_space(4.0);
        ui.label(RichText::new("Game files (FIRSTENC.EXE)").color(CREAM));
        if path_field(ui, &mut self.game_text) {
            self.verify_game_folder(PathBuf::from(self.game_text.trim()));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("FFED3D folder (optional)").color(CREAM));
        if path_field(ui, &mut self.ffed3d_text) {
            let text = self.ffed3d_text.trim();
            self.verify_ffed3d_folder((!text.is_empty()).then(|| PathBuf::from(text)));
        }
    }
}

// --- About ---------------------------------------------------------------

impl App {
    fn about(&mut self, ui: &mut egui::Ui, panel: Rect) {
        draw::chamfer_panel(
            ui.painter(),
            panel,
            16.0,
            Color32::from_rgba_unmultiplied(7, 18, 15, 236),
            Stroke::new(1.5_f32, Color32::from_rgb(215, 222, 210)),
        );
        let inner = panel.shrink2(vec2(30.0, 22.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
            ui.label(RichText::new("About").font(draw::font(30.0, draw::BOLD)).color(CREAM));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new(
                    "ffe-rs is a native Rust port of Frontier: First Encounters, checked against the JJFFE \
                     reconstruction of the original. It needs your own copy of the game: the launcher \
                     fingerprints the files, builds the content cache (data.bin) from them and starts the game.",
                ).color(CREAM));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Data folder: {}", self.root.display())).color(DIM));
                section(ui, "Game files");
                report(ui, self.game_report.as_ref(), self.busy(Job::Game));
                section(ui, "Content cache");
                report(ui, self.data_report.as_ref(), self.busy(Job::Data) || self.busy(Job::Build));
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.game_ok() && !self.busy(Job::Build) && self.child.is_none(), egui::Button::new("Rebuild data.bin")).on_disabled_hover_text("Close the game first: it has data.bin open.").clicked() {
                        self.start(Job::Build);
                    }
                    if ui.button("Open data folder").clicked() {
                        open_folder(&self.root);
                    }
                    let log = self.saves.join(GAME_LOG);
                    if ui.add_enabled(log.is_file(), egui::Button::new("Open last game log")).clicked() {
                        open_folder(&log);
                    }
                });
                section(ui, "FFED3D");
                if self.launcher.ffed3d.is_none() {
                    ui.label(RichText::new("No folder chosen.").color(DIM));
                } else {
                    report(ui, self.ffed3d_report.as_ref(), self.busy(Job::Ffed3d));
                }
                ui.label(RichText::new(
                    "The game does not load FFED3D's files yet: the Classic graphics never need them, and \
                     the Modern engine will use them for replacement textures and the skybox when that \
                     support lands.",
                ).italics().color(DIM));
                section(ui, "Notes");
                ui.label(RichText::new(
                    "Never share data.bin, recordings of copyrighted content or the game files. The \
                     launcher artwork is concept art, not original game assets.",
                ).color(DIM));
            });
        });
    }
}

// --- Widgets -------------------------------------------------------------

fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .font(draw::font(16.0, draw::BOLD))
            .color(CYAN)
            .extra_letter_spacing(1.5),
    );
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0, 169, 230, 90));
    ui.add_space(2.0);
}

fn option_row(ui: &mut egui::Ui, title: &str, about: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 420.0).max(300.0));
            ui.label(
                RichText::new(title)
                    .font(draw::font(19.0, draw::BOLD))
                    .color(CREAM),
            );
            ui.label(RichText::new(about).color(DIM).italics());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(4.0);
}

fn pill(label: &str, selected: bool) -> egui::Button<'static> {
    let text = RichText::new(label.to_owned())
        .font(draw::font(
            17.0,
            if selected { draw::BOLD } else { draw::REGULAR },
        ))
        .color(CREAM);
    egui::Button::new(text)
        .fill(if selected {
            OLIVE
        } else {
            Color32::from_rgb(16, 30, 24)
        })
        .stroke(Stroke::new(
            1.0_f32,
            if selected {
                CREAM
            } else {
                Color32::from_rgba_unmultiplied(215, 222, 210, 80)
            },
        ))
        .corner_radius(CornerRadius::same(4))
}

fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) {
    // Right-to-left layout: add in reverse so they read left to right.
    for (option, label) in options.iter().rev() {
        if ui.add(pill(label, *value == *option)).clicked() {
            *value = *option;
        }
    }
}

fn combo(ui: &mut egui::Ui, id: &str, index: &mut usize, labels: &[String]) -> bool {
    let before = *index;
    *index = (*index).min(labels.len().saturating_sub(1));
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .selected_text(labels.get(*index).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.selectable_value(index, i, label);
            }
        });
    *index != before
}

fn toggle(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (rect, mut response) = ui.allocate_exact_size(vec2(52.0, 26.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let off = Color32::from_rgb(30, 44, 38);
    let fill = off.lerp_to_gamma(OLIVE, t);
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(13),
        fill,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(215, 222, 210, 120)),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp(rect.left() + 13.0..=rect.right() - 13.0, t);
    painter.circle_filled(pos2(x, rect.center().y), 9.5, CREAM);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .changed()
}

fn toggle_grid(ui: &mut egui::Ui, id: &str, values: &mut [bool], labels: &[&str]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([60.0, 12.0])
        .show(ui, |ui| {
            for (i, (value, label)) in values.iter_mut().zip(labels).enumerate() {
                ui.horizontal(|ui| {
                    toggle(ui, value);
                    ui.label(RichText::new(*label).color(CREAM));
                });
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

fn path_field(ui: &mut egui::Ui, text: &mut String) -> bool {
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(ui.available_width() - 10.0)
            .hint_text("Folder path"),
    );
    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

fn report(ui: &mut egui::Ui, report: Option<&Report>, busy: bool) {
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Working...").color(DIM));
        });
        return;
    }
    let Some(report) = report else {
        ui.label(RichText::new("Not checked yet.").color(DIM));
        return;
    };
    ui.label(
        RichText::new(&report.summary)
            .font(draw::font(19.0, draw::BOLD))
            .color(level_colour(report.level)),
    );
    for line in report.lines.iter().take(30) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(match line.level {
                    Level::Good => "✔",
                    Level::Warning => "!",
                    Level::Bad => "✖",
                })
                .color(level_colour(line.level)),
            );
            ui.label(RichText::new(&line.text).color(CREAM));
        });
    }
}

// --- Platform ------------------------------------------------------------

fn open_folder(folder: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(folder).spawn();
}

#[cfg(any(windows, target_os = "macos"))]
fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if start.is_dir() {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    // zenity (GNOME) or kdialog (KDE); without either, type the path in
    // Settings > Files & start.
    let start = if start.is_dir() {
        start.display().to_string()
    } else {
        String::from(".")
    };
    let attempts: [(&str, Vec<String>); 2] = [
        (
            "zenity",
            vec![
                "--file-selection".into(),
                "--directory".into(),
                format!("--title={title}"),
                format!("--filename={start}/"),
            ],
        ),
        (
            "kdialog",
            vec![
                "--getexistingdirectory".into(),
                start.clone(),
                "--title".into(),
                title.into(),
            ],
        ),
    ];
    for (program, args) in attempts {
        if let Ok(output) = Command::new(program).args(&args).output() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            return (output.status.success() && !path.is_empty()).then(|| PathBuf::from(path));
        }
    }
    None
}
