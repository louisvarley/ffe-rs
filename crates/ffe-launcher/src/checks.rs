//! What the launcher verifies: the original game files (through
//! `ffe_tools::inspect_game`, the fingerprint check the game's own loading
//! runs), with optional CD video presence reported separately.

use std::path::Path;

/// How a check came out, for the badge colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Good,
    Warning,
    Bad,
}

/// One line of a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub level: Level,
    pub text: String,
}

impl Line {
    fn good(text: impl Into<String>) -> Self {
        Self {
            level: Level::Good,
            text: text.into(),
        }
    }
    fn warning(text: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            text: text.into(),
        }
    }
    fn bad(text: impl Into<String>) -> Self {
        Self {
            level: Level::Bad,
            text: text.into(),
        }
    }
}

/// A finished check: the headline and the details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub level: Level,
    pub summary: String,
    pub lines: Vec<Line>,
}

impl Report {
    fn from_lines(summary_good: &str, summary_bad: &str, lines: Vec<Line>) -> Self {
        let level = if lines.iter().any(|line| line.level == Level::Bad) {
            Level::Bad
        } else if lines.iter().any(|line| line.level == Level::Warning) {
            Level::Warning
        } else {
            Level::Good
        };
        let summary = if level == Level::Bad {
            summary_bad
        } else {
            summary_good
        };
        Self {
            level,
            summary: summary.to_owned(),
            lines,
        }
    }
}

/// The original game files: `FIRSTENC.EXE` must be one of the recognised
/// English releases and the other files must match it.
pub fn verify_game(folder: &Path) -> Report {
    let inspection = match ffe_tools::inspect_game(folder) {
        Ok(inspection) => inspection,
        Err(error) => {
            return Report {
                level: Level::Bad,
                summary: "Game files not found".to_owned(),
                lines: vec![Line::bad(error)],
            }
        }
    };
    let mut lines = Vec::new();
    let shareware = inspection
        .source_profile
        .is_some_and(|p| p.id == "ffe-shareware-1.06-en");
    if shareware {
        lines.push(Line::warning("Shareware identified; experimental launch is enabled. Its embedded data layout is not supported by the game importer yet; startup errors appear in the game log."));
    }
    if inspection.optional_media.avi_files > 0 {
        lines.push(Line::good(format!(
            "CD videos present: {} AVI files (presence checked)",
            inspection.optional_media.avi_files
        )));
    }
    match inspection.source_profile {
        Some(profile) => lines.push(Line::good(format!(
            "Game executable: {} ({})",
            profile.release, profile.id
        ))),
        None => lines.push(Line::bad("FIRSTENC.EXE is not a recognised release")),
    }
    lines.push(Line::good(format!(
        "{} files fingerprinted",
        inspection.files.len()
    )));
    for diagnostic in &inspection.diagnostics {
        match diagnostic.code {
            // The tool's own disclaimer, not a finding about these files.
            "inspection_only" => continue,
            // Floppy installations have no CD videos; that is normal.
            "no_video_media" => {
                lines.push(Line::good("No CD videos (normal for the floppy release)"));
                continue;
            }
            _ => {}
        }
        let text = if diagnostic.path.is_empty() {
            diagnostic.message.clone()
        } else {
            format!("{}: {}", diagnostic.path, diagnostic.message)
        };
        lines.push(match diagnostic.severity {
            ffe_tools::Severity::Error => Line::bad(text),
            ffe_tools::Severity::Warning => Line::warning(text),
        });
    }
    if inspection.inspection_passed && !lines.iter().any(|l| l.level == Level::Bad) {
        Report::from_lines(
            if shareware {
                "Shareware identified (experimental launch)"
            } else {
                "Game files verified"
            },
            "Game files have problems",
            lines,
        )
    } else {
        let mut report =
            Report::from_lines("Game files verified", "Game files have problems", lines);
        report.level = Level::Bad;
        report.summary = "Game files have problems".to_owned();
        report
    }
}
