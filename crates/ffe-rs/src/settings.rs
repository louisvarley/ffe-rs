//! The port's one persisted preference: the Modern/Classic control preset.
//!
//! The original stores its preferences in the commander file; the port has not
//! imported that format. This small sidecar in the `saves` directory keeps the
//! F3 / `Port Options` choice across launches without touching the save
//! snapshots. The escape pages' other values stay session-only for now
//! (`docs/CONTROLS.md`).
use std::fs;
use std::io::Write;
use std::path::Path;

/// The controls file's name inside the `saves` directory.
pub const CONTROLS_FILE: &str = "controls.cfg";

/// Read the stored preset: `Some(true)` is Modern, `Some(false)` Classic.
/// A missing, malformed, oversized or symlinked file falls back to the caller's
/// default instead of failing startup.
pub fn load_controls(path: &Path) -> Option<bool> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    match std::str::from_utf8(&bytes).ok()?.trim() {
        "modern" => Some(true),
        "classic" => Some(false),
        _ => None,
    }
}

/// Store the preset. A failure is reported to the caller (the game logs it) but
/// must not take the session down.
pub fn save_controls(path: &Path, modern: bool) -> Result<(), String> {
    let text = if modern { "modern\n" } else { "classic\n" };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preset_round_trips_and_malformed_files_are_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONTROLS_FILE);
        assert_eq!(load_controls(&path), None, "missing file keeps the default");
        save_controls(&path, false).unwrap();
        assert_eq!(load_controls(&path), Some(false));
        save_controls(&path, true).unwrap();
        assert_eq!(load_controls(&path), Some(true));
        fs::write(&path, "sideways\n").unwrap();
        assert_eq!(load_controls(&path), None);
        fs::write(&path, "x".repeat(17)).unwrap();
        assert_eq!(load_controls(&path), None);
    }
}
