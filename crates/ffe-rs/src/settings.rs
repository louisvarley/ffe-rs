//! Persisted port preferences: the control preset, the presentation choices and
//! the remaining applied preference pages, in one versioned `config.cfg`.
//!
//! The original keeps its preferences in the commander file; the port has not
//! imported that block. `config.cfg` is a port-native text file with an explicit
//! `version` line so future fields can be added without guessing. `controls.cfg`
//! and `display.cfg` are the version-1 sidecars written before this file
//! existed; they are still read as a migration path and are no longer written.
//!
//! Loading never fails startup: a missing, malformed, oversized or symlinked
//! file leaves the caller's defaults. Saving is atomic (temporary file, sync,
//! rename) and reports errors to the caller.
//!
//! The `bind.*` keys are reserved for input remapping. The port has no
//! remapping screen yet, so they are read and written unchanged but do not
//! drive the key tables; see `docs/CONTROLS.md` and the owner decisions in
//! `docs/notes/persistence.md`.

use crate::display::DisplaySettings;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

/// The combined configuration file.
pub const CONFIG_FILE: &str = "config.cfg";
/// The current configuration version.
pub const CONFIG_VERSION: u32 = 2;
/// The legacy controls sidecar (read-only migration).
pub const CONTROLS_FILE: &str = "controls.cfg";
/// The legacy display sidecar (read-only migration).
pub const DISPLAY_FILE: &str = "display.cfg";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// Every persisted preference. `remap` is reserved (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub modern: bool,
    pub display: DisplaySettings,
    pub space_colour: usize,
    pub shape_detail: usize,
    pub others: [bool; 10],
    pub maps: [bool; 5],
    pub sound: [bool; 10],
    /// Port addition: the Docking Options page's indices.
    pub station_approach: usize,
    pub starport_approach: usize,
    /// Port addition: the World Options page's choices.
    pub traffic: usize,
    pub police: bool,
    pub pirates: bool,
    pub advanced_traffic: bool,
    pub combat_improved: bool,
    pub remap: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        let prefs = crate::prefs::Prefs::default();
        Self {
            modern: prefs.modern,
            display: prefs.display,
            space_colour: prefs.space_colour,
            shape_detail: prefs.shape_detail,
            others: prefs.others,
            maps: prefs.maps,
            sound: prefs.sound,
            station_approach: prefs.station_approach,
            starport_approach: prefs.starport_approach,
            traffic: prefs.traffic,
            police: prefs.police,
            pirates: prefs.pirates,
            advanced_traffic: prefs.advanced_traffic,
            combat_improved: prefs.combat_improved,
            remap: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The file's text, in a stable order.
    pub fn to_config(&self) -> String {
        let mut out = String::new();
        out.push_str("# ffe-rs preferences; version 2 adds the preference pages.\n");
        out.push_str(&format!("version={CONFIG_VERSION}\n"));
        out.push_str(&format!(
            "controls={}\n",
            if self.modern { "modern" } else { "classic" }
        ));
        out.push_str(&format!("space_colour={}\n", self.space_colour.min(4)));
        out.push_str(&format!("shape_detail={}\n", self.shape_detail.min(3)));
        for (index, value) in self.others.iter().enumerate() {
            out.push_str(&format!("other_{index}={}\n", on_off(*value)));
        }
        for (index, value) in self.maps.iter().enumerate() {
            out.push_str(&format!("map_{index}={}\n", on_off(*value)));
        }
        for (index, value) in self.sound.iter().enumerate() {
            out.push_str(&format!("sound_{index}={}\n", on_off(*value)));
        }
        out.push_str(&format!("station_approach={}\n", self.station_approach));
        out.push_str(&format!("starport_approach={}\n", self.starport_approach));
        out.push_str(&format!("traffic={}\n", self.traffic));
        out.push_str(&format!("police={}\n", on_off(self.police)));
        out.push_str(&format!("pirates={}\n", on_off(self.pirates)));
        out.push_str(&format!(
            "advanced_traffic={}\n",
            on_off(self.advanced_traffic)
        ));
        out.push_str(&format!(
            "combat={}\n",
            if self.combat_improved {
                "improved"
            } else {
                "classic"
            }
        ));
        out.push_str(&self.display.to_config());
        for (action, key) in &self.remap {
            out.push_str(&format!("bind.{action}={key}\n"));
        }
        out
    }

    /// Parse the file. Unknown keys are ignored for forward compatibility; a
    /// known key with a bad value, a missing/unsupported version or a malformed
    /// line returns `None` so the caller keeps its defaults.
    pub fn from_config(text: &str) -> Option<Self> {
        let mut config = Self::default();
        let mut version = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "version" => {
                    let parsed: u32 = value.parse().ok()?;
                    if !(1..=CONFIG_VERSION).contains(&parsed) {
                        return None;
                    }
                    version = Some(parsed);
                }
                "controls" => match value {
                    "modern" => config.modern = true,
                    "classic" => config.modern = false,
                    _ => return None,
                },
                "space_colour" => {
                    let parsed: usize = value.parse().ok()?;
                    if parsed > 4 {
                        return None;
                    }
                    config.space_colour = parsed;
                }
                "station_approach" => {
                    let parsed: usize = value.parse().ok()?;
                    if parsed >= crate::prefs::STATION_APPROACH_KM.len() {
                        return None;
                    }
                    config.station_approach = parsed;
                }
                "starport_approach" => {
                    let parsed: usize = value.parse().ok()?;
                    if parsed >= crate::prefs::STARPORT_APPROACH_KM.len() {
                        return None;
                    }
                    config.starport_approach = parsed;
                }
                "traffic" => {
                    let parsed: usize = value.parse().ok()?;
                    if parsed >= crate::prefs::TRAFFIC_LABELS.len() {
                        return None;
                    }
                    config.traffic = parsed;
                }
                "police" => config.police = parse_on_off(value)?,
                "pirates" => config.pirates = parse_on_off(value)?,
                "advanced_traffic" => config.advanced_traffic = parse_on_off(value)?,
                // The earlier single choice sets both.
                "police_pirates" => {
                    let on = parse_on_off(value)?;
                    config.police = on;
                    config.pirates = on;
                }
                "combat" => {
                    config.combat_improved = match value {
                        "classic" => false,
                        "improved" => true,
                        _ => return None,
                    }
                }
                "shape_detail" => {
                    let parsed: usize = value.parse().ok()?;
                    if parsed > 3 {
                        return None;
                    }
                    config.shape_detail = parsed;
                }
                _ if key.starts_with("other_") => {
                    let index: usize = key["other_".len()..].parse().ok()?;
                    *config.others.get_mut(index)? = parse_on_off(value)?;
                }
                _ if key.starts_with("map_") => {
                    let index: usize = key["map_".len()..].parse().ok()?;
                    *config.maps.get_mut(index)? = parse_on_off(value)?;
                }
                _ if key.starts_with("sound_") => {
                    let index: usize = key["sound_".len()..].parse().ok()?;
                    *config.sound.get_mut(index)? = parse_on_off(value)?;
                }
                _ if key.starts_with("bind.") => {
                    let action = &key["bind.".len()..];
                    if action.is_empty() || value.is_empty() {
                        return None;
                    }
                    config.remap.insert(action.to_owned(), value.to_owned());
                }
                "scale" | "fit" | "filter" | "aspect" => {}
                _ => {}
            }
        }
        version?;
        // `DisplaySettings::from_config` keeps its own defaults for missing keys
        // and rejects a known key with a bad value. It ignores the version line.
        config.display = DisplaySettings::from_config(text)?;
        Some(config)
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn parse_on_off(value: &str) -> Option<bool> {
    match value {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Read the combined configuration from `directory`.
///
/// `config.cfg` wins. When it is absent, the two version-1 sidecars are read as
/// a migration: `controls.cfg` (`modern`/`classic`) and `display.cfg` (the
/// display keys). A malformed new file is not silently replaced by the old
/// sidecars (that would change the user's last choice); it returns the defaults.
pub fn load_config(directory: &Path) -> Option<Config> {
    let path = directory.join(CONFIG_FILE);
    if path.exists() {
        let text = read_text(&path, MAX_CONFIG_BYTES)?;
        return Config::from_config(&text);
    }
    let mut config = Config::default();
    if let Some(modern) = load_controls(&directory.join(CONTROLS_FILE)) {
        config.modern = modern;
    }
    if let Some(display) = load_display(&directory.join(DISPLAY_FILE)) {
        config.display = display;
    }
    Some(config)
}

/// Write the combined configuration atomically.
pub fn save_config(directory: &Path, config: &Config) -> Result<(), String> {
    save_text(&directory.join(CONFIG_FILE), &config.to_config())
}

/// Read the old controls sidecar: `Some(true)` is Modern, `Some(false)` Classic.
pub fn load_controls(path: &Path) -> Option<bool> {
    let text = read_text(path, 64)?;
    let mut preset = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("preset=") {
            preset = match value.trim() {
                "modern" => Some(true),
                "classic" => Some(false),
                _ => return None,
            };
        } else if line == "modern" {
            preset = Some(true);
        } else if line == "classic" {
            preset = Some(false);
        } else if let Some(value) = line.strip_prefix("version=") {
            let version: u32 = value.trim().parse().ok()?;
            if !(1..=CONFIG_VERSION).contains(&version) {
                return None;
            }
        } else if !line.is_empty() && !line.starts_with('#') {
            return None;
        }
    }
    preset
}

/// Write the legacy controls sidecar in the versioned form. The playable writes
/// `config.cfg`; this is retained for callers that only need the preset.
pub fn save_controls(path: &Path, modern: bool) -> Result<(), String> {
    let text = format!(
        "version={CONFIG_VERSION}\npreset={}\n",
        if modern { "modern" } else { "classic" }
    );
    save_text(path, &text)
}

/// Read the old display sidecar. A missing, malformed, oversized or symlinked
/// file falls back to the caller's default.
pub fn load_display(path: &Path) -> Option<DisplaySettings> {
    let text = read_text(path, 256)?;
    DisplaySettings::from_config(&text)
}

/// Write the legacy display sidecar in the versioned form.
pub fn save_display(path: &Path, settings: DisplaySettings) -> Result<(), String> {
    save_text(
        path,
        &format!("version={CONFIG_VERSION}\n{}", settings.to_config()),
    )
}

fn read_text(path: &Path, limit: u64) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    String::from_utf8(bytes).ok()
}

fn save_text(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: no parent directory", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{}: invalid file name", path.display()))?;
    let mut temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let mut attempt = 0u32;
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(mut file) => {
                let result = file
                    .write_all(text.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|error| format!("{}: write failed: {error}", temp.display()));
                drop(file);
                if let Err(error) = result {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                return match fs::rename(&temp, path) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        let _ = fs::remove_file(&temp);
                        Err(format!(
                            "{}: cannot replace atomically: {error}",
                            path.display()
                        ))
                    }
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt += 1;
                if attempt > 100 {
                    return Err(format!("{}: cannot make a temporary name", path.display()));
                }
                temp = parent.join(format!(".{name}.{}.{attempt}.tmp", std::process::id()));
            }
            Err(error) => {
                return Err(format!(
                    "{}: cannot create a temporary file: {error}",
                    temp.display()
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docking_options_round_trip_and_reject_bad_indices() {
        let config = Config {
            station_approach: 3,
            starport_approach: 2,
            ..Config::default()
        };
        let text = config.to_config();
        assert!(text.contains("station_approach=3\nstarport_approach=2\n"));
        assert_eq!(Config::from_config(&text), Some(config));
        assert_eq!(
            Config::from_config("version=2\nstation_approach=4\n"),
            None,
            "only four station distances"
        );
        assert_eq!(
            Config::default().station_approach,
            0,
            "the original's offset"
        );
    }

    #[test]
    fn world_options_default_to_the_original_and_round_trip() {
        let config = Config::default();
        assert_eq!(
            (
                config.traffic,
                config.police,
                config.pirates,
                config.combat_improved
            ),
            (0, true, true, false)
        );
        let config = Config {
            traffic: 3,
            police: false,
            combat_improved: true,
            ..Config::default()
        };
        let text = config.to_config();
        assert!(text.contains(
            "traffic=3\npolice=off\npirates=on\nadvanced_traffic=off\ncombat=improved\n"
        ));
        assert_eq!(Config::from_config(&text), Some(config));
        assert_eq!(Config::from_config("version=2\ntraffic=4\n"), None);
        assert_eq!(Config::from_config("version=2\ncombat=arcade\n"), None);
        // The earlier single police-and-pirates choice sets both.
        let old = Config::from_config("version=2\npolice_pirates=off\n").unwrap();
        assert_eq!((old.police, old.pirates), (false, false));
    }

    #[test]
    fn the_versioned_config_round_trips_every_page() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(load_config(directory.path()), Some(Config::default()));
        let mut config = Config {
            modern: false,
            space_colour: 1,
            shape_detail: 3,
            ..Config::default()
        };
        config.display.scale = 3;
        config.others[1] = true;
        config.maps[4] = true;
        config.sound[7] = false;
        config
            .remap
            .insert("pitch_up".to_owned(), "KeyZ".to_owned());
        save_config(directory.path(), &config).unwrap();
        assert_eq!(load_config(directory.path()), Some(config));
        let text = fs::read_to_string(directory.path().join(CONFIG_FILE)).unwrap();
        assert!(text.starts_with("#"), "{text}");
        assert!(text.contains("version=2"), "{text}");
        assert!(text.contains("bind.pitch_up=KeyZ"), "{text}");
    }

    #[test]
    fn the_old_sidecars_migrate_and_are_not_required_afterwards() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(CONTROLS_FILE), "classic\n").unwrap();
        fs::write(
            directory.path().join(DISPLAY_FILE),
            "scale=2\nfit=fill-window\n",
        )
        .unwrap();
        let config = load_config(directory.path()).unwrap();
        assert!(!config.modern);
        assert_eq!(config.display.scale, 2);
        assert_eq!(config.display.fit, crate::display::FitMode::FillWindow);
        save_config(directory.path(), &config).unwrap();
        fs::remove_file(directory.path().join(CONTROLS_FILE)).unwrap();
        fs::remove_file(directory.path().join(DISPLAY_FILE)).unwrap();
        assert_eq!(load_config(directory.path()), Some(config));
    }

    #[test]
    fn malformed_and_future_configs_fall_back_to_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE);
        for text in [
            "version=99\ncontrols=modern\n",
            "version=2\ncontrols=sideways\n",
            "version=2\nscale=9\n",
            "no equals sign\n",
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(load_config(directory.path()), None, "{text}");
        }
        fs::write(&path, "x".repeat(MAX_CONFIG_BYTES as usize + 1)).unwrap();
        assert_eq!(load_config(directory.path()), None);
    }

    #[test]
    fn the_preset_sidecar_reads_both_forms_and_rejects_junk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONTROLS_FILE);
        assert_eq!(load_controls(&path), None);
        fs::write(&path, "classic\n").unwrap();
        assert_eq!(load_controls(&path), Some(false));
        save_controls(&path, true).unwrap();
        assert_eq!(load_controls(&path), Some(true));
        fs::write(&path, "sideways\n").unwrap();
        assert_eq!(load_controls(&path), None);
        fs::write(&path, "x".repeat(65)).unwrap();
        assert_eq!(load_controls(&path), None);
    }

    #[test]
    fn the_display_sidecar_round_trips_and_bad_files_are_ignored() {
        use crate::display::{Filter, FitMode, PixelAspect};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(DISPLAY_FILE);
        assert_eq!(load_display(&path), None);
        let settings = DisplaySettings {
            scale: 3,
            fit: FitMode::FillWindow,
            filter: Filter::Linear,
            aspect: PixelAspect::Square,
            graphics: crate::modern::Graphics::Modern,
        };
        save_display(&path, settings).unwrap();
        assert_eq!(load_display(&path), Some(settings));
        fs::write(&path, "scale=9\n").unwrap();
        assert_eq!(load_display(&path), None);
        fs::write(&path, "x".repeat(257)).unwrap();
        assert_eq!(load_display(&path), None);
    }
}
