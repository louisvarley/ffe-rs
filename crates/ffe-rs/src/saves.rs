//! Native, versioned commander saves and the port's save-file helpers.
//!
//! # Port-native format (not DOS/JJFFE compatible)
//!
//! The original writes a binary commander file through `F65`/`F66` and
//! `F170`/`F171` (a version dword, a 284-byte global block, 105 table blobs, 150
//! object records, their payloads and a zero-run-compressed/XOR-scrambled body).
//! The port writes **JSON**: a bounded, validated, human-auditable document
//! named `quick.json` or `commander-NNNNNNNN.json`. The two formats are
//! deliberately not interchangeable. A real commander file is recognised and
//! left untouched; importing it needs a supplied specimen and a field map, see
//! `docs/notes/persistence.md`.
//!
//! Version 2 is the first full save. It adds the commander (name, rank slots and
//! mission slots), the navigator/system state (system, clock, legal record,
//! docking/landing stage) and the port flags that a plain [`Flight`] does not
//! carry. Version 1 (`ffe-rs-experimental`) snapshots are still accepted
//! read-only: they become a version-2 [`SaveGame`] with no navigator state.
//!
//! Every file is read with a size bound and must be a regular, non-symlinked
//! file. Writes go to a temporary file in the same directory, are synced, and
//! are then renamed over the destination, so a failed write cannot truncate the
//! previous save. Numbered saves are still new-only.

use crate::flight::Flight;
use crate::navigation::NavigatorSave;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The native save format's tag.
pub const FORMAT: &str = "ffe-rs-save";
/// The current native save version.
pub const VERSION: u32 = 2;
/// The tag of the development snapshots this module can still read.
pub const LEGACY_FORMAT: &str = "ffe-rs-experimental";
/// The largest accepted save. The navigator state is small; this is deliberately
/// far below any memory-dump size.
pub const MAX_SAVE_BYTES: usize = 1024 * 1024;
/// The quick-save file inside the `saves` directory.
pub const QUICK_SAVE: &str = "quick.json";
/// The stem of the numbered save files.
pub const NUMBERED_PREFIX: &str = "commander-";
/// The original's commander name (`DATA_004963`) used when none is supplied.
pub const DEFAULT_COMMANDER: &str = "Jameson";

/// The source's commander file name limit (`DATA_009117`, 20 bytes) is not a
/// port limit, but keeps imported names sane.
const MAX_COMMANDER_NAME: usize = 32;
const MAX_MISSIONS: usize = 256;

/// The commander fields that live outside [`Flight`].
///
/// `ranks` are the two words `DATA_008912`/`DATA_008913` the galaxy tables use
/// for a system's control; the port does not change them yet. `missions` is a
/// reserved list: the port has no mission engine, so it is empty in play, but it
/// round-trips so a future mission subsystem can use the same file version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commander {
    #[serde(default = "default_commander")]
    pub name: String,
    #[serde(default)]
    pub ranks: [u32; 2],
    #[serde(default)]
    pub missions: Vec<Mission>,
}

impl Default for Commander {
    fn default() -> Self {
        Self {
            name: default_commander(),
            ranks: [0; 2],
            missions: Vec::new(),
        }
    }
}

fn default_commander() -> String {
    DEFAULT_COMMANDER.to_owned()
}

/// A reserved mission record. The port has no mission engine; this shape keeps
/// the save forward-compatible without claiming a source layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    pub id: u32,
    /// The system code the mission belongs to, when it has one.
    #[serde(default)]
    pub system: Option<u32>,
    #[serde(default)]
    pub completed: bool,
}

/// One complete native save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveGame {
    pub format: String,
    pub version: u32,
    pub commander: Commander,
    /// The flight/player state, kept under the version-1 field name so a v1
    /// snapshot's body can be migrated mechanically.
    pub state: Flight,
    /// The navigator, when the game had one. Version-1 snapshots and the
    /// prototype-only loop have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<NavigatorSave>,
    /// Whether the original flight model (key O) was driving the ship.
    #[serde(default)]
    pub original_flight: bool,
    /// The console time-acceleration step (0 = 1x .. 4 = 10000x).
    #[serde(default)]
    pub acceleration: usize,
}

impl SaveGame {
    /// A v2 save for a state with no navigator (prototype loop or an old body).
    pub fn from_flight(state: &Flight) -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            commander: Commander::default(),
            state: state.clone(),
            navigation: None,
            original_flight: false,
            acceleration: 0,
        }
    }

    /// A complete save.
    pub fn new(commander: Commander, state: Flight, navigation: Option<NavigatorSave>) -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            commander,
            state,
            navigation,
            original_flight: false,
            acceleration: 0,
        }
    }

    /// Check every field this module owns. The caller still refuses a save in
    /// the game-over state, which is a main-loop concern rather than a property
    /// of the document.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!(
                "Unsupported save format {:?}; expected {FORMAT:?}.",
                self.format
            ));
        }
        if self.version != VERSION {
            return Err(format!(
                "Unsupported save version {}; this build writes version {VERSION}.",
                self.version
            ));
        }
        self.commander.validate()?;
        self.state.validate()?;
        if let Some(navigation) = &self.navigation {
            navigation.validate()?;
        }
        if self.acceleration > 4 {
            return Err("Save has an invalid time-acceleration step.".to_owned());
        }
        Ok(())
    }
}

impl Commander {
    fn validate(&self) -> Result<(), String> {
        if self.name.len() > MAX_COMMANDER_NAME {
            return Err(format!(
                "Commander name is longer than {MAX_COMMANDER_NAME} bytes."
            ));
        }
        if self.name.chars().any(char::is_control) {
            return Err("Commander name contains a control character.".to_owned());
        }
        if self.ranks.iter().any(|rank| *rank > 1_000_000) {
            return Err("Commander rank is out of range.".to_owned());
        }
        if self.missions.len() > MAX_MISSIONS {
            return Err("Save has too many mission records.".to_owned());
        }
        let mut ids = std::collections::BTreeSet::new();
        for mission in &self.missions {
            if mission.id > 1_000_000 || !ids.insert(mission.id) {
                return Err("Save has an invalid or duplicate mission id.".to_owned());
            }
        }
        Ok(())
    }
}

/// A version-1 development snapshot, read-only.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveV1 {
    format: String,
    version: u32,
    state: Flight,
}

/// The version/format probe. It deliberately does not reject unknown fields so
/// a newer file still gets a clear "newer version" message instead of a serde
/// shape error.
#[derive(Deserialize)]
struct Header {
    format: String,
    version: u32,
}

/// Encode a full save. Validation happens before serialization.
pub fn encode_game(save: &SaveGame) -> Result<Vec<u8>, String> {
    save.validate()?;
    let mut bytes =
        serde_json::to_vec_pretty(save).map_err(|error| format!("Cannot encode save: {error}"))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(format!(
            "Save is {} bytes, larger than the {MAX_SAVE_BYTES}-byte limit.",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Object +0xd0 (Combat) is authoritative, including zero after F947.
/// Older versions could leave a stale loadout class after damage or servicing.
fn migrate_drive(state: &mut Flight) {
    state.loadout.drive = state.combat.drive;
}

/// Decode a full save, accepting a version-1 snapshot as a migrated v2 document.
pub fn decode_game(bytes: &[u8]) -> Result<SaveGame, String> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(format!("Save exceeds the {MAX_SAVE_BYTES}-byte limit."));
    }
    if bytes.is_empty() {
        return Err("Save is empty.".to_owned());
    }
    if looks_like_legacy_binary(bytes) {
        return Err(legacy_binary_message(bytes.len()));
    }
    let header: Header = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid save (not JSON): {error}"))?;
    match (header.format.as_str(), header.version) {
        (FORMAT, VERSION) => {
            let mut save: SaveGame = serde_json::from_slice(bytes)
                .map_err(|error| format!("Invalid save v{VERSION}: {error}"))?;
            migrate_drive(&mut save.state);
            save.validate()?;
            Ok(save)
        }
        (LEGACY_FORMAT, 1) => {
            let old: SaveV1 = serde_json::from_slice(bytes)
                .map_err(|error| format!("Invalid version-1 snapshot: {error}"))?;
            if old.format != LEGACY_FORMAT || old.version != 1 {
                return Err("Invalid version-1 snapshot header.".to_owned());
            }
            let mut state = old.state;
            migrate_drive(&mut state);
            state.validate()?;
            Ok(SaveGame {
                format: FORMAT.to_owned(),
                version: VERSION,
                commander: Commander::default(),
                state,
                navigation: None,
                original_flight: false,
                acceleration: 0,
            })
        }
        (LEGACY_FORMAT, version) => Err(format!(
            "Unsupported version-1 snapshot version {version}; only 1 is known."
        )),
        (FORMAT, version) => Err(format!(
            "Save version {version} is newer than this build's version {VERSION}."
        )),
        (format, _) => Err(format!(
            "Unsupported save format {format:?}; expected {FORMAT:?}."
        )),
    }
}

/// Backwards-compatible v1-style helper: encode a flight state as a full v2 save
/// with the default commander. The playable uses [`encode_game`].
pub fn encode(state: &Flight) -> Result<Vec<u8>, String> {
    encode_game(&SaveGame::from_flight(state))
}

/// Backwards-compatible helper: decode any supported save and return its flight
/// state. The playable uses [`decode_game`] to keep the commander and navigator.
pub fn decode(bytes: &[u8]) -> Result<Flight, String> {
    decode_game(bytes).map(|save| save.state)
}

/// Read and validate a save file. The path must be a regular, non-symlinked file
/// no larger than [`MAX_SAVE_BYTES`].
pub fn load_game(path: &Path) -> Result<SaveGame, String> {
    let bytes = read_bounded(path)?;
    decode_game(&bytes)
}

/// Backwards-compatible file loader returning only the flight state.
pub fn load(path: &Path) -> Result<Flight, String> {
    load_game(path).map(|save| save.state)
}

/// Read at most `MAX_SAVE_BYTES + 1` bytes from a regular file.
fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: a save must be a regular file, not a link or directory.",
            path.display()
        ));
    }
    if metadata.len() > MAX_SAVE_BYTES as u64 {
        return Err(format!(
            "{}: save is {} bytes, larger than the {MAX_SAVE_BYTES}-byte limit.",
            path.display(),
            metadata.len()
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_SAVE_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("{}: cannot read save: {error}", path.display()))?;
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(format!(
            "{}: save exceeds the {MAX_SAVE_BYTES}-byte limit.",
            path.display()
        ));
    }
    Ok(bytes)
}

/// Write `save` to `path` atomically: a sibling temporary file is created,
/// synced and renamed over the destination. On failure the old file remains.
pub fn save_atomic(path: &Path, save: &SaveGame) -> Result<(), String> {
    let bytes = encode_game(save)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: save has no parent directory", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{}: invalid save name", path.display()))?;
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
                    .write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| format!("{}: save write failed: {error}", temp.display()));
                drop(file);
                if let Err(error) = result {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                match fs::rename(&temp, path) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        let _ = fs::remove_file(&temp);
                        return Err(format!(
                            "{}: cannot replace the save atomically: {error}",
                            path.display()
                        ));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt += 1;
                if attempt > 100 {
                    return Err(format!(
                        "{}: cannot make a temporary save name",
                        path.display()
                    ));
                }
                temp = parent.join(format!(".{name}.{}.{attempt}.tmp", std::process::id()));
            }
            Err(error) => {
                return Err(format!(
                    "{}: cannot create a temporary save: {error}",
                    temp.display()
                ));
            }
        }
    }
}

/// The quick-save path inside `directory`.
pub fn quick_path(directory: &Path) -> PathBuf {
    directory.join(QUICK_SAVE)
}

/// Write the quick save, replacing the previous one atomically.
pub fn save_quick(directory: &Path, save: &SaveGame) -> Result<PathBuf, String> {
    let path = quick_path(directory);
    save_atomic(&path, save)?;
    Ok(path)
}

/// Load the quick save.
pub fn load_quick(directory: &Path) -> Result<(PathBuf, SaveGame), String> {
    let path = quick_path(directory);
    let save = load_game(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((path, save))
}

/// The numbered saves in `directory`, lowest first. Only exact
/// `commander-NNNNNNNN.json` names are considered.
pub fn numbered_saves(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(number) = name
            .strip_prefix(NUMBERED_PREFIX)
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        if number.len() != 8 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

/// Write a new numbered save without overwriting any existing one. The bytes go
/// to a sibling temporary file and are linked into place, so an interrupted
/// writer cannot leave a partial numbered save.
pub fn save_numbered(directory: &Path, save: &SaveGame) -> Result<PathBuf, String> {
    let existing = numbered_saves(directory)?;
    let next = match existing.last() {
        Some(path) => {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.strip_prefix(NUMBERED_PREFIX))
                .ok_or("Invalid numbered save name")?
                .parse::<u32>()
                .map_err(|error| error.to_string())?
                + 1
        }
        None => 1,
    };
    if next > 99_999_999 {
        return Err("Numbered save limit reached.".to_owned());
    }
    let path = directory.join(format!("{NUMBERED_PREFIX}{next:08}.json"));
    write_new_atomic(&path, &encode_game(save)?)?;
    Ok(path)
}

/// Write a file that must not already exist, via a synced temporary file and a
/// hard link. Unlike rename, the link refuses to replace an existing name on
/// every platform.
fn write_new_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: save has no parent directory", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{}: invalid save name", path.display()))?;
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
                    .write_all(bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| format!("{}: save write failed: {error}", temp.display()));
                drop(file);
                if let Err(error) = result {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                let linked = fs::hard_link(&temp, path).map_err(|error| {
                    format!("{}: cannot create the save: {error}", path.display())
                });
                let _ = fs::remove_file(&temp);
                return linked;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt += 1;
                if attempt > 100 {
                    return Err(format!(
                        "{}: cannot make a temporary save name",
                        path.display()
                    ));
                }
                temp = parent.join(format!(".{name}.{}.{attempt}.tmp", std::process::id()));
            }
            Err(error) => {
                return Err(format!(
                    "{}: cannot create a temporary save: {error}",
                    temp.display()
                ));
            }
        }
    }
}

/// Load the quick save, or the highest numbered save when there is no quick
/// save. No silent fallback: a present but invalid quick save is an error.
pub fn load_quick_or_latest(directory: &Path) -> Result<(PathBuf, SaveGame), String> {
    let quick = quick_path(directory);
    if quick.exists() {
        return load_quick(directory);
    }
    let paths = numbered_saves(directory)?;
    let path = paths
        .last()
        .ok_or_else(|| format!("No save in {}; F5 quick-saves.", directory.display()))?;
    let save = load_game(path)?;
    Ok((path.clone(), save))
}

/// Write the old experimental snapshot format's successor: a new-only file at
/// `path` with the default commander. Kept for the prototype loop and callers
/// that only have a [`Flight`].
pub fn save_new(path: &Path, state: &Flight) -> Result<(), String> {
    write_new_atomic(path, &encode_game(&SaveGame::from_flight(state))?)
}

/// The original commander file starts with a version dword 1 and is at least
/// `0x258` bytes (`F185`). We only use this to recognise a legacy file and leave
/// it alone; its tables and compressed body need the source's fixed table sizes.
fn looks_like_legacy_binary(bytes: &[u8]) -> bool {
    bytes.len() >= 0x258
        && bytes.len() <= 0x1_0000
        && bytes
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| *byte != b'{')
        && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 1
}

fn legacy_binary_message(size: usize) -> String {
    format!(
        "Legacy commander file detected ({size} bytes, F65 version 1). The port is \
         read-only for original files and cannot import this one without a supplied \
         reference specimen and field map; the file was not modified. Use a native \
         save (version {VERSION})."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::Mode;
    use std::io::Write;

    #[test]
    fn stale_loadout_cannot_restore_a_destroyed_or_degraded_drive() {
        for class in [0, 1, 2, 10, 14] {
            let mut state = Flight::default();
            state.loadout.drive = 14;
            state.combat.drive = class;
            migrate_drive(&mut state);
            assert_eq!(state.loadout.drive, class);
            assert_eq!(state.combat.drive, class);
        }
    }

    fn sample_navigation() -> NavigatorSave {
        NavigatorSave {
            system_code: 0x1234_5678,
            clock: 1234.5,
            start_clock: 100.0,
            world_epoch: Some(100.0),
            new_game: None,
            page: 2,
            view: crate::navigation::View::Places,
            selected: 3,
            target: Some(2),
            locked: Some(4),
            destination: Some(crate::navigation::Destination {
                code: 0x00ab_cdef,
                position: [1, -2, 3],
            }),
            altitude: 0x00b7_1b00,
            docked: Some(1),
            legal: ffe_core::npc::Legal {
                wanted: true,
                alert: true,
                illegal_fire: true,
                offences: 3,
                fines: [0, 6000, 0, 0, 0, 0],
                records: [0; 6],
            },
            ship_lock: Some(7),
            missile_lock: Some(9),
            missile_view: Some(11),
            sequence: Some(crate::navigation::SequenceSave {
                station: 1,
                state: 0x1a,
                timeout: 0xffff,
                drawn: Some((0x1a, 0x1234)),
                launching: true,
            }),
            takeoff: Some(0x24),
            landed: Some(crate::navigation::LandedSave {
                state: 0x34,
                index: 2,
            }),
            final_leg: true,
            spawned: Some(5),
        }
    }

    fn sample() -> SaveGame {
        let mut state = Flight::free_flight();
        state.credits = 12_345;
        state.hold = vec![0; ffe_core::market::NAMES];
        state.hold[0] = 3;
        state.hold[1] = 2;
        state.combat = crate::combat::Combat::new_game();
        state.loadout = ffe_core::equipment::Loadout::new_game();
        state.shields = [0x40, 0x40];
        state.hull = [4608, 4608];
        state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        state.system_flight = true;
        SaveGame {
            format: FORMAT.to_owned(),
            version: VERSION,
            commander: Commander {
                name: "Jameson".to_owned(),
                ranks: [4, 7],
                missions: vec![Mission {
                    id: 12,
                    system: Some(0x00ab_cdef),
                    completed: false,
                }],
            },
            state,
            navigation: Some(sample_navigation()),
            original_flight: true,
            acceleration: 3,
        }
    }

    #[test]
    fn a_complete_save_round_trips_every_field() {
        let save = sample();
        let bytes = encode_game(&save).unwrap();
        let loaded = decode_game(&bytes).unwrap();
        assert_eq!(loaded, save);
        assert!(loaded.state.validate().is_ok());
        loaded.validate().unwrap();
    }

    #[test]
    fn version_one_snapshots_migrate_and_default_the_new_fields() {
        let v1 = br#"{
            "format": "ffe-rs-experimental",
            "version": 1,
            "state": {
                "mode": "Flight",
                "position": [0.0, 0.0, -180.0],
                "velocity": [0.0, 0.0, 5.0],
                "yaw": 0.0,
                "pitch": 0.0,
                "credits": 1000,
                "cargo": 0,
                "stock": 50,
                "ticks": 0,
                "docking_requested": false
            }
        }"#;
        let loaded = decode_game(v1).unwrap();
        assert_eq!(loaded.version, VERSION);
        assert_eq!(loaded.state.mode, Mode::Flight);
        assert_eq!(loaded.state.credits, 1000);
        assert_eq!(loaded.commander, Commander::default());
        assert_eq!(loaded.navigation, None);
        assert!(!loaded.original_flight);
    }

    #[test]
    fn corrupt_newer_and_unknown_files_are_rejected_clearly() {
        let bytes = encode(&Flight::default()).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap().to_owned();
        let truncated = &bytes[..bytes.len() - 2];
        assert!(decode(truncated).is_err());
        assert!(decode(&[]).is_err());
        assert!(decode(&vec![b' '; MAX_SAVE_BYTES + 1]).is_err());
        for (from, to) in [
            ("\"version\": 2", "\"version\": 99"),
            ("\"version\": 2", "\"version\": 1"),
            ("\"format\": \"ffe-rs-save\"", "\"format\": \"other\""),
            ("\"credits\": 1000", "\"credits\": 1000000001"),
            ("\"cargo\": 0", "\"cargo\": 11"),
            ("\"stock\": 50", "\"stock\": 1001"),
            ("\"ticks\": 0", "\"ticks\": 0, \"surprise\": 0"),
        ] {
            let changed = text.replace(from, to);
            assert_ne!(changed, text);
            assert!(decode(changed.as_bytes()).is_err(), "{from} -> {to}");
        }
    }

    #[test]
    fn invalid_state_is_rejected_on_encode_and_decode() {
        let mut invalid = Flight::default();
        invalid.velocity[0] = 1.0;
        assert!(encode(&invalid).is_err());
        let mut invalid = Flight::free_flight();
        invalid.yaw = f64::INFINITY;
        assert!(encode(&invalid).is_err());
        let mut dead = Flight::free_flight();
        dead.death = Some(crate::death::Death::exploding());
        assert!(encode(&dead).is_err());
        let mut save = sample();
        save.acceleration = 5;
        assert!(encode_game(&save).is_err());
        save.acceleration = 0;
        save.commander.name = "x".repeat(MAX_COMMANDER_NAME + 1);
        assert!(encode_game(&save).is_err());
    }

    #[test]
    fn atomic_writes_replace_and_leave_a_good_file_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("save.json");
        let first = sample();
        save_atomic(&path, &first).unwrap();
        assert_eq!(load_game(&path).unwrap(), first);
        let mut second = first.clone();
        second.state.credits = 54321;
        save_atomic(&path, &second).unwrap();
        assert_eq!(load_game(&path).unwrap(), second);
        // A temporary file from an interrupted writer never replaces the save.
        let temp = directory.path().join(".save.json.999.tmp");
        let mut file = fs::File::create(&temp).unwrap();
        file.write_all(b"{ broken").unwrap();
        assert_eq!(load_game(&path).unwrap(), second);
        drop(file);
        fs::remove_file(temp).unwrap();
    }

    #[test]
    fn quick_and_numbered_paths_are_new_only_and_latest_prefers_quick() {
        let directory = tempfile::tempdir().unwrap();
        assert!(load_quick_or_latest(directory.path()).is_err());
        let save = sample();
        let quick = save_quick(directory.path(), &save).unwrap();
        assert_eq!(quick, quick_path(directory.path()));
        let numbered = save_numbered(directory.path(), &save).unwrap();
        assert!(numbered.ends_with("commander-00000001.json"));
        assert!(save_numbered(directory.path(), &save)
            .unwrap()
            .ends_with("commander-00000002.json"));
        let (path, loaded) = load_quick_or_latest(directory.path()).unwrap();
        assert_eq!(path, quick);
        assert_eq!(loaded, save);
        fs::remove_file(quick).unwrap();
        let (path, loaded) = load_quick_or_latest(directory.path()).unwrap();
        assert!(path.ends_with("commander-00000002.json"));
        assert_eq!(loaded, save);
    }

    #[test]
    fn a_legacy_binary_is_recognised_and_left_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("JAMESON.000");
        let mut bytes = vec![0x41u8; 0x300];
        bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        let error = load_game(&path).unwrap_err();
        assert!(error.contains("Legacy commander file detected"), "{error}");
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "the original was not written"
        );
    }
}
