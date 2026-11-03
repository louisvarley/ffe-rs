//! The launcher's own choices (`saves/launcher.cfg`): where the game files
//! and FFED3D resources are, and how to start the game. The game's settings
//! live in `saves/config.cfg` (`ffe_rs::settings`), which the launcher edits
//! through the game's own reader and writer.

use std::path::{Path, PathBuf};

pub const LAUNCHER_FILE: &str = "launcher.cfg";
const MAX_BYTES: u64 = 16 * 1024;

/// The start points the game accepts (`--start`), with their names.
pub const STARTS: [(u8, &str, &str); 2] = [
    (
        0,
        "Gateway",
        "The default start: on the pad at Old Blackelk, on Hope in the Gateway system.",
    ),
    (
        1,
        "Ross 154",
        "The alternative start in the Ross 154 system.",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launcher {
    pub game: PathBuf,
    pub ffed3d: Option<PathBuf>,
    pub start: u8,
    pub skip_intro: bool,
    pub record: bool,
}

impl Launcher {
    pub fn defaults(root: &Path) -> Self {
        Self {
            game: root.join("game"),
            ffed3d: None,
            start: 0,
            skip_intro: false,
            record: true,
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# ffe-rs launcher\nversion=1\n");
        out.push_str(&format!("game={}\n", self.game.display()));
        if let Some(ffed3d) = &self.ffed3d {
            out.push_str(&format!("ffed3d={}\n", ffed3d.display()));
        }
        out.push_str(&format!("start={}\n", self.start));
        out.push_str(&format!("skip_intro={}\n", on_off(self.skip_intro)));
        out.push_str(&format!("record={}\n", on_off(self.record)));
        out
    }

    /// Read `text` over `self`; unknown or malformed lines are ignored.
    pub fn read(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "game" if !value.is_empty() => self.game = PathBuf::from(value),
                "ffed3d" if !value.is_empty() => self.ffed3d = Some(PathBuf::from(value)),
                "start" => {
                    if let Ok(start) = value.parse::<u8>() {
                        if STARTS.iter().any(|(code, _, _)| *code == start) {
                            self.start = start;
                        }
                    }
                }
                "skip_intro" => self.skip_intro = value == "on",
                "record" => self.record = value == "on",
                _ => {}
            }
        }
    }

    pub fn load(saves: &Path, root: &Path) -> Self {
        let mut launcher = Self::defaults(root);
        let path = saves.join(LAUNCHER_FILE);
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.is_file() && meta.len() <= MAX_BYTES {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    launcher.read(&text);
                }
            }
        }
        launcher
    }

    pub fn save(&self, saves: &Path) -> Result<(), String> {
        std::fs::create_dir_all(saves).map_err(|e| e.to_string())?;
        let path = saves.join(LAUNCHER_FILE);
        let temporary = saves.join(format!("{LAUNCHER_FILE}.tmp"));
        std::fs::write(&temporary, self.to_text()).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

/// The folder that holds (or will hold) `data.bin` and `saves`: beside the
/// launcher, else the nearest of four parent folders with a `data.bin`, as
/// the game itself searches. Before the first build there is no `data.bin`:
/// then the nearest folder with the game files in `game\` (the checkout's
/// layout, `target\release\ffe-launcher.exe` four levels below) or with
/// the workspace's `Cargo.toml`, else the launcher's own folder.
pub fn find_root() -> PathBuf {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    root_from(&beside)
}

pub fn root_from(beside: &Path) -> PathBuf {
    let candidates = || beside.ancestors().take(5);
    candidates()
        .find(|dir| dir.join("data.bin").is_file())
        .or_else(|| candidates().find(|dir| dir.join("game").join("FIRSTENC.EXE").is_file()))
        .or_else(|| {
            candidates().find(|dir| dir.join("Cargo.toml").is_file() && dir.join("crates").is_dir())
        })
        .map_or_else(|| beside.to_path_buf(), Path::to_path_buf)
}

/// The game executable beside the launcher.
pub fn game_executable() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = if cfg!(windows) {
        "ffe-rs.exe"
    } else {
        "ffe-rs"
    };
    let path = dir.join(name);
    path.is_file().then_some(path)
}

/// `yyyymmdd-hhmmss` (UTC) for a recording's file name.
pub fn stamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let root = Path::new("/games/ffe");
        let mut launcher = Launcher::defaults(root);
        launcher.ffed3d = Some(PathBuf::from("/games/ffed3d"));
        launcher.start = 1;
        launcher.skip_intro = true;
        launcher.record = false;
        let mut read = Launcher::defaults(root);
        read.read(&launcher.to_text());
        assert_eq!(read, launcher);
    }

    #[test]
    fn bad_lines_keep_the_defaults() {
        let root = Path::new("/games/ffe");
        let mut read = Launcher::defaults(root);
        read.read("start=9\nnonsense\nrecord=maybe\n");
        assert_eq!(read.start, 0);
        assert!(!read.record);
    }

    #[test]
    fn the_root_is_the_checkout_before_the_first_build() {
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("target").join("release");
        std::fs::create_dir_all(&release).unwrap();
        // A plain folder: the launcher's own.
        assert_eq!(root_from(&release), release);
        // The checkout's game folder.
        std::fs::create_dir_all(dir.path().join("game")).unwrap();
        std::fs::write(dir.path().join("game").join("FIRSTENC.EXE"), b"x").unwrap();
        assert_eq!(root_from(&release), dir.path());
        // A data.bin wins.
        std::fs::write(release.join("data.bin"), b"x").unwrap();
        assert_eq!(root_from(&release), release);
    }

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_793_667_723), "20261103-010203");
    }
}
