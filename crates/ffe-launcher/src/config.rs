//! The launcher's own choices (`saves/launcher.cfg`): where the game files
//! are, and how to start the Classic game. The game's settings
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
    pub start: u8,
    pub skip_intro: bool,
    pub record: bool,
}

impl Launcher {
    pub fn defaults(root: &Path) -> Self {
        Self {
            game: root.join("game"),
            start: 0,
            skip_intro: false,
            record: false,
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# ffe-rs launcher\nversion=1\n");
        out.push_str(&format!("game={}\n", self.game.display()));
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
        if detect_game(&launcher.game).is_none() {
            if let Some(game) = detect_game(root) {
                launcher.game = game;
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

/// The folder that holds `saves` and `recordings`: the nearest of the
/// launcher's folder and four parents with saved launcher settings, a
/// `game\FIRSTENC.EXE` (the checkout's layout, `target\release\` below it)
/// or the workspace's `Cargo.toml`, else the launcher's own folder. The game
/// searches the same way.
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
        .find(|dir| dir.join("saves").join(LAUNCHER_FILE).is_file())
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
    fn the_root_is_the_checkout_without_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("target").join("release");
        std::fs::create_dir_all(&release).unwrap();
        // A plain folder: the launcher's own.
        assert_eq!(root_from(&release), release);
        // The checkout's game folder.
        std::fs::create_dir_all(dir.path().join("game")).unwrap();
        std::fs::write(dir.path().join("game").join("FIRSTENC.EXE"), b"x").unwrap();
        assert_eq!(root_from(&release), dir.path());
        // Saved launcher settings win.
        std::fs::create_dir_all(release.join("saves")).unwrap();
        std::fs::write(release.join("saves").join(LAUNCHER_FILE), b"x").unwrap();
        assert_eq!(root_from(&release), release);
    }

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_793_667_723), "20261103-010203");
    }
}

/// Find an installation directly or within a selected extracted release folder.
/// Bounded depth and entry count avoid walking an entire drive or following links.
pub fn detect_game(root: &Path) -> Option<PathBuf> {
    let mut pending = std::collections::VecDeque::from([(root.to_path_buf(), 0)]);
    let mut visited = 0;
    while let Some((folder, depth)) = pending.pop_front() {
        visited += 1;
        if visited > 512 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut children = Vec::new();
        for entry in entries.filter_map(Result::ok).take(2048) {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_file()
                && (entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("FIRSTENC.EXE")
                    || entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("FIRSTENC.DAT"))
            {
                return Some(folder);
            }
            if depth < 3 && kind.is_dir() {
                children.push(entry.path());
            }
        }
        children.sort();
        pending.extend(children.into_iter().map(|path| (path, depth + 1)));
    }
    None
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    #[test]
    fn finds_nested_installations_without_following_unrelated_files() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(detect_game(root.path()), None);
        let install = root.path().join("release").join("FFE");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(install.join("firstenc.exe"), b"fixture").unwrap();
        assert_eq!(detect_game(root.path()), Some(install.clone()));
        assert_eq!(detect_game(&install), Some(install.clone()));
        std::fs::rename(install.join("firstenc.exe"), install.join("FIRSTENC.DAT")).unwrap();
        assert_eq!(detect_game(root.path()), Some(install));
    }
}

/// Invalidate the root asset pack before switching source installations.
/// A failed removal leaves the caller on the previous source directory.
pub fn invalidate_assets(root: &Path, previous: &Path, next: &Path) -> Result<(), String> {
    let identity = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if identity(previous) == identity(next) {
        return Ok(());
    }
    let assets = root.join("assets.bin");
    match std::fs::remove_file(&assets) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Could not remove {}: {error}. Game folder was not changed.",
            assets.display()
        )),
    }
}

#[cfg(test)]
mod asset_invalidation_tests {
    use super::*;
    #[test]
    fn switching_folders_removes_assets_but_rechecking_keeps_them() {
        let root = tempfile::tempdir().unwrap();
        let previous = root.path().join("previous");
        let next = root.path().join("next");
        std::fs::create_dir(&previous).unwrap();
        std::fs::create_dir(&next).unwrap();
        let assets = root.path().join("assets.bin");
        std::fs::write(&assets, b"old pack").unwrap();
        invalidate_assets(root.path(), &previous, &previous.join(".")).unwrap();
        assert!(assets.is_file());
        invalidate_assets(root.path(), &previous, &next).unwrap();
        assert!(!assets.exists());
        invalidate_assets(root.path(), &previous, &next).unwrap();
    }
    #[test]
    fn removal_failure_is_reported() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets.bin")).unwrap();
        assert!(invalidate_assets(root.path(), Path::new("old"), Path::new("new")).is_err());
    }
}
