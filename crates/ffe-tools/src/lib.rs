use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub mod cache;
pub mod census;
pub mod external;
pub mod reference;

const MAX_EXECUTABLE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 16 * 1024 * 1024;
const FLOPPY_SHA256: &str = "d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72";
const CD_SHA256: &str = "9957dc34aa9ef93d9a69c5f5be8e13d243046e758ad83c8a35241e31b7e6e616";
const JOURNAL_SHA256: &str = "3cf36755c7420f12281ca8a55f11dbc9787bacaeef49edc4584338f77dc94afc";
const CABIN_SHA256: [&str; 6] = [
    "99bfe4ca6159427a28d9e24ebb70061ea9eb69c93b262d27cd15d0b11c5f3c0a",
    "dfdc9ad2efe49001c8d9e63218f2c32791540296359309fe8ef5673ad6c9be58",
    "64e82260b7e118b480e164bcf3c7926b47449d618a3da8d6fb362cc881055ad0",
    "2acda06a83958ae501ae5e931987deb79feccdff4a54b99bb8dec919e9263d19",
    "b8a9a1515dee8d83e1e95c5b87ce31c408d503a6ae0dd65403392bade59066bb",
    "421ef56f28c4bed69a658c74464a29a4cf16d50cadf0ceb5c2cdc1c1d57e06ae",
];

const EFFECTS: [&str; 23] = [
    "AIROPEN", "BIP", "BONG", "BOOM", "CAMERA", "CHIME", "CLUNKIT", "DAMAGE", "ECM", "HYPE",
    "LASER1", "LASER2", "LASER3", "LASER4", "LAUNCH", "MISSEXPL", "NECM", "NOWT", "ROAR",
    "SHIPEXP", "SIREN", "STATION", "WIND",
];
const SONGS: [&str; 17] = [
    "BABAYAGA", "BAREM", "BLUEDAN", "GGOFKIEV", "HALLKING", "INTRO", "JUPALT", "QQATMOSP",
    "QQDRAMA", "QQESCAPE", "QQFRONT", "QQFRONT2", "QQPARADI", "QQROCK", "QQSUSPEN", "QQTRAVEL",
    "VALKRIES",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceProfile {
    pub id: &'static str,
    pub release: &'static str,
}

pub fn identify_executable(size: u64, sha256: &str) -> Option<SourceProfile> {
    match (size, sha256) {
        (2_097_152, FLOPPY_SHA256) => Some(SourceProfile {
            id: "ffe-floppy-1.02-en",
            release: "English Release 1.02 - 21/04/95",
        }),
        (2_093_056, CD_SHA256) => Some(SourceProfile {
            id: "ffe-cd-1.01-en",
            release: "English Release 1.01 - 13/04/95",
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct MediaInventory {
    pub avi_files: usize,
    pub bmp_files: usize,
}

#[derive(Debug, Serialize)]
pub struct Inspection {
    pub report_version: u32,
    pub simulation_reference: &'static str,
    pub source_profile: Option<SourceProfile>,
    pub inspection_passed: bool,
    pub conversion_implemented: bool,
    pub files: Vec<FileRecord>,
    pub optional_media: MediaInventory,
    pub diagnostics: Vec<Diagnostic>,
}

impl Inspection {
    fn diagnostic(
        &mut self,
        severity: Severity,
        code: &'static str,
        path: &str,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            code,
            path: path.to_owned(),
            message: message.into(),
        });
    }

    fn error(&mut self, code: &'static str, path: &str, message: impl Into<String>) {
        self.diagnostic(Severity::Error, code, path, message);
    }
}

fn contextual_error(path: &Path, error: impl std::fmt::Display) -> String {
    format!("{}: {error}", path.display())
}

fn find_child(directory: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let entries = fs::read_dir(directory).map_err(|error| contextual_error(directory, error))?;
    let mut found = None;
    for entry in entries {
        let entry = entry.map_err(|error| contextual_error(directory, error))?;
        if entry.file_name().eq_ignore_ascii_case(name) {
            if found.is_some() {
                return Err(format!(
                    "{}: multiple entries match {name} ignoring case; remove the ambiguity",
                    directory.display()
                ));
            }
            found = Some(entry.path());
        }
    }
    Ok(found)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| contextual_error(path, error))?;
    if !metadata.file_type().is_file() {
        return Err(contextual_error(
            path,
            "expected a regular file, not a link or directory",
        ));
    }
    if metadata.len() > limit {
        return Err(contextual_error(
            path,
            format!("file exceeds {limit}-byte inspection limit"),
        ));
    }
    let file = fs::File::open(path).map_err(|error| contextual_error(path, error))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| contextual_error(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(contextual_error(
            path,
            "file grew beyond the inspection limit",
        ));
    }
    Ok(bytes)
}

fn read_required(
    report: &mut Inspection,
    directory: &Path,
    name: &str,
    relative: &str,
    limit: u64,
) -> Option<Vec<u8>> {
    let path = match find_child(directory, name) {
        Ok(Some(path)) => path,
        Ok(None) => {
            report.error("missing_file", relative, "Required original file is missing; copy a complete installed game into this directory.");
            return None;
        }
        Err(error) => {
            report.error("read_error", relative, error);
            return None;
        }
    };
    match read_bounded(&path, limit) {
        Ok(bytes) => {
            report.files.push(FileRecord {
                path: relative.to_owned(),
                size: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            });
            Some(bytes)
        }
        Err(error) => {
            report.error("read_error", relative, error);
            None
        }
    }
}

fn directory(report: &mut Inspection, root: &Path, name: &str, required: bool) -> Option<PathBuf> {
    match find_child(root, name) {
        Ok(Some(path)) => match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => Some(path),
            Ok(_) => {
                report.error(
                    "invalid_directory",
                    name,
                    "Expected a directory, not a file or symbolic link.",
                );
                None
            }
            Err(error) => {
                report.error("read_error", name, contextual_error(&path, error));
                None
            }
        },
        Ok(None) => {
            if required {
                report.error(
                    "missing_directory",
                    name,
                    "Required asset directory is missing.",
                );
            }
            None
        }
        Err(error) => {
            report.error("read_error", name, error);
            None
        }
    }
}

fn check_digest(report: &mut Inspection, path: &str, bytes: &[u8], expected: &str) {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        report.error("content_mismatch", path, format!(
            "Original content does not match the verified English releases: expected SHA-256 {expected}, found {actual}. Keep modified art in a separate graphics pack."
        ));
    }
}

pub fn inspect_game(root: &Path) -> Result<Inspection, String> {
    if !root.is_dir() {
        return Err(contextual_error(
            root,
            "game directory does not exist or is not a directory; supply an installed/extracted copy, not a disc image or patch installer",
        ));
    }
    let mut report = Inspection {
        report_version: 1,
        simulation_reference: "jjffe-2.8a7",
        source_profile: None,
        inspection_passed: false,
        conversion_implemented: false,
        files: Vec::new(),
        optional_media: MediaInventory {
            avi_files: 0,
            bmp_files: 0,
        },
        diagnostics: Vec::new(),
    };
    if let Some(bytes) = read_required(
        &mut report,
        root,
        "FIRSTENC.EXE",
        "FIRSTENC.EXE",
        MAX_EXECUTABLE_BYTES,
    ) {
        let digest = format!("{:x}", Sha256::digest(&bytes));
        report.source_profile = identify_executable(bytes.len() as u64, &digest);
        if report.source_profile.is_none() {
            report.error("unsupported_executable", "FIRSTENC.EXE", format!(
                "Unrecognized executable: {} bytes, SHA-256 {digest}. Recognized inputs: English floppy 1.02 and CD 1.01. Patched outputs need separate validation; no offsets will be guessed.",
                bytes.len()
            ));
        }
    }
    if let Some(bytes) = read_required(
        &mut report,
        root,
        "MISSION.DAT",
        "MISSION.DAT",
        MAX_ASSET_BYTES,
    ) {
        check_digest(&mut report, "MISSION.DAT", &bytes, JOURNAL_SHA256);
    }
    for (index, digest) in CABIN_SHA256.iter().enumerate() {
        let name = format!("CABIN{index}.BMP");
        if let Some(bytes) = read_required(&mut report, root, &name, &name, MAX_ASSET_BYTES) {
            check_digest(&mut report, &name, &bytes, digest);
        }
    }
    if let Some(fx) = directory(&mut report, root, "FX", true) {
        for effect in EFFECTS {
            let name = format!("{effect}.RAW");
            let relative = format!("FX/{name}");
            if let Some(bytes) = read_required(&mut report, &fx, &name, &relative, MAX_ASSET_BYTES)
            {
                if bytes.len() % 2 != 0 || (bytes.is_empty() && effect != "NOWT") {
                    report.error("invalid_pcm", &relative, "Expected non-empty 16-bit PCM (an even number of bytes); only NOWT.RAW may be empty.");
                }
            }
        }
    }
    if let Some(music) = directory(&mut report, root, "MUSIC", true) {
        for song in SONGS {
            let name = format!("{song}.HMP");
            let relative = format!("MUSIC/{name}");
            if let Some(bytes) =
                read_required(&mut report, &music, &name, &relative, MAX_ASSET_BYTES)
            {
                if !bytes.starts_with(b"HMIMIDIP") {
                    report.error("invalid_hmp", &relative, "Expected an original HMP file beginning HMIMIDIP. MIDI replacements are not inspected by this initial tool.");
                }
            }
        }
    }
    inspect_optional_media(&mut report, root);
    report.diagnostic(Severity::Warning, "inspection_only", "", "This is installation inspection, not a working game or converter. Audio checks cover file presence, PCM length and HMP signature, not full decoding or original-audio hash equivalence. Optional media is counted, not decoded.");
    report
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    report.diagnostics.sort_by(|left, right| {
        (&left.path, left.code, &left.message).cmp(&(&right.path, right.code, &right.message))
    });
    report.inspection_passed = report.source_profile.is_some()
        && !report
            .diagnostics
            .iter()
            .any(|entry| entry.severity == Severity::Error);
    Ok(report)
}

fn inspect_optional_media(report: &mut Inspection, root: &Path) {
    let Some(data) = directory(report, root, "DATA", false) else {
        report.diagnostic(
            Severity::Warning,
            "no_video_media",
            "DATA",
            "No optional CD media directory; floppy installations normally have no videos.",
        );
        return;
    };
    let result = (|| -> io::Result<()> {
        for entry in fs::read_dir(&data)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if extension.eq_ignore_ascii_case("avi") {
                report.optional_media.avi_files += 1;
            } else if extension.eq_ignore_ascii_case("bmp") {
                report.optional_media.bmp_files += 1;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        report.error("read_error", "DATA", contextual_error(&data, error));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_require_both_size_and_digest() {
        assert_eq!(
            identify_executable(2_097_152, FLOPPY_SHA256).unwrap().id,
            "ffe-floppy-1.02-en"
        );
        assert_eq!(
            identify_executable(2_093_056, CD_SHA256).unwrap().id,
            "ffe-cd-1.01-en"
        );
        assert!(identify_executable(2_097_152, CD_SHA256).is_none());
        assert!(identify_executable(2_093_056, FLOPPY_SHA256).is_none());
        assert!(identify_executable(2_097_152, "unrecognized").is_none());
    }

    #[test]
    fn profile_hashes_are_valid_sha256_strings() {
        for hash in [FLOPPY_SHA256, CD_SHA256, JOURNAL_SHA256]
            .into_iter()
            .chain(CABIN_SHA256)
        {
            assert_eq!(hash.len(), 64);
            assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }
}
