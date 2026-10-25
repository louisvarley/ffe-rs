use crate::flight::Flight;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    format: String,
    version: u32,
    state: Flight,
}

pub fn encode(state: &Flight) -> Result<Vec<u8>, String> {
    state.validate()?;
    serde_json::to_vec_pretty(&Save {
        format: "ffe-rs-experimental".to_owned(),
        version: 1,
        state: state.clone(),
    })
    .map_err(|error| error.to_string())
}

pub fn decode(bytes: &[u8]) -> Result<Flight, String> {
    if bytes.len() > 16384 {
        return Err("Save exceeds 16 KiB.".to_owned());
    }
    let save: Save =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid save: {error}"))?;
    if save.format != "ffe-rs-experimental" || save.version != 1 {
        return Err(
            "Unsupported save format/version; original commanders are not imported yet.".to_owned(),
        );
    }
    let mut state = save.state;
    // Saves from before `Loadout::drive` kept the one drive byte only in
    // `Combat` (the drive was not removable through the Upgrades screen then, so
    // the combat byte is authoritative).
    if state.loadout.drive == 0 {
        state.loadout.drive = state.combat.drive;
    }
    state.validate()?;
    Ok(state)
}

pub fn load(path: &Path) -> Result<Flight, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Save must be a regular file.".to_owned());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(16385).read_to_end(&mut bytes))
        .map_err(|error| format!("Cannot read save: {error}"))?;
    decode(&bytes)
}

/// Immutable numbered snapshots avoid non-atomic replace semantics on Windows.
pub fn save_new(path: &Path, state: &Flight) -> Result<(), String> {
    let bytes = encode(state)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Save write failed (snapshot may be incomplete): {error}"))
}
