//! What the launcher verifies: the original game files (through
//! `ffe_tools::inspect_game`, the fingerprint check the game's own loading
//! runs) and an optional FFED3D resource folder.

use std::path::{Path, PathBuf};

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
    match inspection.source_profile {
        Some(profile) => lines.push(Line::good(format!(
            "FIRSTENC.EXE: {} ({})",
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
        Report::from_lines("Game files verified", "Game files have problems", lines)
    } else {
        let mut report =
            Report::from_lines("Game files verified", "Game files have problems", lines);
        report.level = Level::Bad;
        report.summary = "Game files have problems".to_owned();
        report
    }
}

/// The FFED3D resource folder. Accepts the folder that holds `resources`, or
/// `resources` itself.
pub fn ffed3d_resources(folder: &Path) -> PathBuf {
    let inner = folder.join("resources");
    if inner.is_dir() {
        inner
    } else {
        folder.to_path_buf()
    }
}

/// A child folder of `parent` named `name` in any letter case (an installed
/// FFED3D has `MODELS` and `Textures`, the source tree `models` and
/// `textures`).
fn child_folder(parent: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
                && entry.path().is_dir()
        })
        .map(|entry| entry.path())
}

/// A file in `folder` named `name` in any letter case.
fn has_file(folder: &Path, name: &str) -> bool {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
                    && entry.path().is_file()
            })
        })
        .unwrap_or(false)
}

/// FFED3D's replacement assets: its skybox and star textures, the `texNNN`
/// HD textures, the star, nebula, sphere and atmosphere models and the
/// numbered replacement models (`MODELS/<n>/model.x`). Either an installed
/// FFED3D or its source tree's `resources`, in any letter case. FFED3D's own
/// configuration (`ffewin.cfg`, `ffed3daj.cfg`) is not needed: the port keeps
/// its settings in `saves/config.cfg`. These are optional; the port's
/// Classic graphics never need them.
pub fn verify_ffed3d(folder: &Path) -> Report {
    let resources = ffed3d_resources(folder);
    if !resources.is_dir() {
        return Report {
            level: Level::Bad,
            summary: "FFED3D resources not found".to_owned(),
            lines: vec![Line::bad(format!(
                "{} is not a folder.",
                resources.display()
            ))],
        };
    }
    let mut lines = Vec::new();
    let textures = child_folder(&resources, "textures");
    let models = child_folder(&resources, "models");
    if let Some(textures) = textures {
        let skybox = (0..6)
            .filter(|i| {
                has_file(&textures, &format!("skybox{i}.tga"))
                    || has_file(&textures, &format!("skybox{i}.png"))
            })
            .count();
        lines.push(if skybox == 6 {
            Line::good("Skybox: all six faces")
        } else {
            Line::warning(format!("Skybox: {skybox} of 6 faces"))
        });
        lines.push(if has_file(&textures, "stars.tga") {
            Line::good("Star texture")
        } else {
            Line::warning("stars.tga missing")
        });
        let hd = std::fs::read_dir(&textures)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| is_hd_texture(&entry.file_name().to_string_lossy()))
                    .count()
            })
            .unwrap_or(0);
        lines.push(if hd > 0 {
            Line::good(format!("{hd} replacement textures (texNNN)"))
        } else {
            Line::warning("No texNNN replacement textures")
        });
    } else {
        lines.push(Line::bad("No textures folder"));
    }
    if let Some(models) = models {
        let wanted = ["stars.X", "nebula.X", "sphere.X", "atmo.X"];
        let found = wanted.iter().filter(|name| has_file(&models, name)).count();
        lines.push(if found == wanted.len() {
            Line::good("Star, nebula, sphere and atmosphere models")
        } else {
            Line::warning(format!("{found} of {} core models", wanted.len()))
        });
        let replacements = std::fs::read_dir(&models)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        !name.is_empty()
                            && name.chars().all(|c| c.is_ascii_digit())
                            && has_file(&entry.path(), "model.x")
                    })
                    .count()
            })
            .unwrap_or(0);
        lines.push(if replacements > 0 {
            Line::good(format!(
                "{replacements} replacement models (MODELS/<n>/model.x)"
            ))
        } else {
            Line::warning("No numbered replacement models")
        });
    } else {
        lines.push(Line::bad("No models folder"));
    }
    Report::from_lines(
        "FFED3D resources found",
        "FFED3D resources incomplete",
        lines,
    )
}

fn is_hd_texture(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("tex") else {
        return false;
    };
    let Some((number, extension)) = rest.split_once('.') else {
        return false;
    };
    !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
        && matches!(extension, "png" | "dds" | "tga")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hd_texture_names() {
        assert!(is_hd_texture("tex713.png"));
        assert!(is_hd_texture("TEX94.DDS"));
        assert!(!is_hd_texture("texture.png"));
        assert!(!is_hd_texture("tex.png"));
        assert!(!is_hd_texture("skybox0.tga"));
    }

    #[test]
    fn a_missing_game_folder_is_reported() {
        let report = verify_game(Path::new("/definitely/not/here"));
        assert_eq!(report.level, Level::Bad);
    }

    #[test]
    fn ffed3d_layout_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let resources = dir.path().join("resources");
        std::fs::create_dir_all(resources.join("textures")).unwrap();
        std::fs::create_dir_all(resources.join("models")).unwrap();
        for i in 0..6 {
            std::fs::write(resources.join(format!("textures/skybox{i}.tga")), b"x").unwrap();
        }
        for name in ["stars.tga", "tex713.png"] {
            std::fs::write(resources.join("textures").join(name), b"x").unwrap();
        }
        for name in ["stars.X", "nebula.X", "sphere.X", "atmo.X"] {
            std::fs::write(resources.join("models").join(name), b"x").unwrap();
        }
        std::fs::create_dir_all(resources.join("models/14")).unwrap();
        std::fs::write(resources.join("models/14/model.X"), b"x").unwrap();
        // Either the parent or `resources` itself.
        assert_eq!(verify_ffed3d(dir.path()).level, Level::Good);
        assert_eq!(verify_ffed3d(&resources).level, Level::Good);
        std::fs::remove_file(resources.join("textures/skybox3.tga")).unwrap();
        assert_eq!(verify_ffed3d(dir.path()).level, Level::Warning);
    }

    /// An installed FFED3D: `MODELS` and `Textures`, PNG skyboxes, no
    /// `ffewin.cfg`.
    #[test]
    fn an_installed_ffed3d_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Textures")).unwrap();
        std::fs::create_dir_all(root.join("MODELS/112")).unwrap();
        for i in 0..6 {
            std::fs::write(root.join(format!("Textures/skybox{i}.png")), b"x").unwrap();
        }
        for name in ["stars.tga", "tex0.png"] {
            std::fs::write(root.join("Textures").join(name), b"x").unwrap();
        }
        for name in ["stars.X", "nebula.X", "sphere.X", "atmo.X"] {
            std::fs::write(root.join("MODELS").join(name), b"x").unwrap();
        }
        std::fs::write(root.join("MODELS/112/model.X"), b"x").unwrap();
        let report = verify_ffed3d(root);
        assert_eq!(report.level, Level::Good, "{:?}", report.lines);
    }
}
