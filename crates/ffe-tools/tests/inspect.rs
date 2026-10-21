use ffe_tools::{inspect_game, Severity};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ffe-tools"))
}

#[test]
fn absent_root_is_an_actionable_error() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("not-installed");
    let error = inspect_game(&root).unwrap_err();
    assert!(error.contains("installed/extracted copy"));
    assert!(error.contains("not-installed"));
}

#[test]
fn empty_installation_reports_every_required_group() {
    let temp = TempDir::new().unwrap();
    let report = inspect_game(temp.path()).unwrap();
    assert!(!report.inspection_passed);
    assert!(!report.conversion_implemented);
    assert!(report.source_profile.is_none());
    let missing: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.severity == Severity::Error)
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(
        missing,
        [
            "CABIN0.BMP",
            "CABIN1.BMP",
            "CABIN2.BMP",
            "CABIN3.BMP",
            "CABIN4.BMP",
            "CABIN5.BMP",
            "FIRSTENC.EXE",
            "FX",
            "MISSION.DAT",
            "MUSIC",
        ]
    );
}

#[test]
fn executable_release_text_is_not_trusted_as_a_profile() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("firstenc.exe"),
        b"English Release 1.02 - 21/04/95",
    )
    .unwrap();
    let report = inspect_game(temp.path()).unwrap();
    assert!(report.source_profile.is_none());
    let error = report
        .diagnostics
        .iter()
        .find(|entry| entry.code == "unsupported_executable")
        .unwrap();
    assert!(error.message.contains("SHA-256"));
    assert_eq!(report.files[0].path, "FIRSTENC.EXE");
    assert_eq!(report.files[0].size, 31);
}

#[test]
fn synthetic_audio_and_optional_media_receive_explicit_checks() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("fx")).unwrap();
    fs::create_dir(temp.path().join("music")).unwrap();
    fs::create_dir(temp.path().join("data")).unwrap();
    for (name, bytes) in [
        ("fx/nowt.raw", b"".as_slice()),
        ("fx/bip.raw", b"\x01".as_slice()),
        ("fx/wind.raw", b"\x00\x00".as_slice()),
        ("music/intro.hmp", b"HMIMIDIPsynthetic".as_slice()),
        ("music/qqfront.hmp", b"invalid".as_slice()),
        ("data/BB1.AVI", b"synthetic".as_slice()),
        ("data/BB1.bMp", b"synthetic".as_slice()),
    ] {
        fs::write(temp.path().join(name), bytes).unwrap();
    }
    let report = inspect_game(temp.path()).unwrap();
    assert_eq!(report.optional_media.avi_files, 1);
    assert_eq!(report.optional_media.bmp_files, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|entry| entry.code == "invalid_pcm" && entry.path == "FX/BIP.RAW"));
    assert!(report
        .diagnostics
        .iter()
        .any(|entry| entry.code == "invalid_hmp" && entry.path == "MUSIC/QQFRONT.HMP"));
    for valid in ["FX/NOWT.RAW", "FX/WIND.RAW", "MUSIC/INTRO.HMP"] {
        assert!(!report
            .diagnostics
            .iter()
            .any(|entry| entry.path == valid && entry.severity == Severity::Error));
    }
}

#[test]
fn oversized_inputs_are_rejected_without_loading_their_contents() {
    let temp = TempDir::new().unwrap();
    let file = fs::File::create(temp.path().join("FIRSTENC.EXE")).unwrap();
    file.set_len(8 * 1024 * 1024 + 1).unwrap();
    let report = inspect_game(temp.path()).unwrap();
    assert!(report.files.is_empty());
    assert!(report.diagnostics.iter().any(|entry| {
        entry.code == "read_error"
            && entry.path == "FIRSTENC.EXE"
            && entry.message.contains("inspection limit")
    }));
}

#[test]
fn malformed_required_file_type_is_not_opened_as_a_file() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("FIRSTENC.EXE")).unwrap();
    fs::write(temp.path().join("FX"), b"not a directory").unwrap();
    let report = inspect_game(temp.path()).unwrap();
    assert!(report
        .diagnostics
        .iter()
        .any(|entry| entry.code == "read_error" && entry.path == "FIRSTENC.EXE"));
    assert!(report
        .diagnostics
        .iter()
        .any(|entry| entry.code == "invalid_directory" && entry.path == "FX"));
}

#[test]
fn reports_are_deterministic_and_inspection_does_not_modify_inputs() {
    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("FIRSTENC.EXE");
    fs::write(&executable, b"synthetic executable").unwrap();
    let before = fs::read(&executable).unwrap();
    let first = serde_json::to_vec(&inspect_game(temp.path()).unwrap()).unwrap();
    let second = serde_json::to_vec(&inspect_game(temp.path()).unwrap()).unwrap();
    assert_eq!(first, second);
    assert_eq!(fs::read(&executable).unwrap(), before);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn cli_invalid_installation_produces_json_and_exit_one() {
    let temp = TempDir::new().unwrap();
    let output = tool()
        .args(["inspect-game", "--game"])
        .arg(temp.path())
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["inspection_passed"], false);
    assert_eq!(report["conversion_implemented"], false);
    assert_eq!(report["simulation_reference"], "jjffe-2.8a7");
    assert!(report["diagnostics"].as_array().unwrap().len() >= 10);
}

#[test]
fn cli_argument_errors_and_missing_roots_exit_two() {
    for arguments in [
        vec![],
        vec!["unknown"],
        vec!["inspect-game", "--game"],
        vec!["inspect-game", "--game", "--json"],
        vec!["inspect-game", "--json", "--json"],
        vec!["inspect-game", "--game", ".", "--game", "."],
        vec!["inspect-game", "--unexpected"],
    ] {
        let output = tool().args(&arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
    }
    let temp = TempDir::new().unwrap();
    let output = tool()
        .args(["inspect-game", "--game"])
        .arg(temp.path().join("missing"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("installed/extracted"));
}

#[test]
fn cli_help_and_version_do_not_need_game_files() {
    for argument in ["--help", "-h", "--version"] {
        let output = tool().arg(argument).output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn external_export_requires_explicit_output_and_never_overwrites() {
    for arguments in [
        vec!["export-external"],
        vec!["export-external", "--out"],
        vec!["export-external", "--out", "--json"],
        vec!["inspect-game", "--out", "unexpected"],
    ] {
        assert_eq!(
            tool().args(arguments).output().unwrap().status.code(),
            Some(2)
        );
    }
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("game");
    fs::create_dir(&source).unwrap();
    let output = temp.path().join("export");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep.txt"), b"must stay").unwrap();
    let error = ffe_tools::external::export_external(&source, &output).unwrap_err();
    assert!(error.contains("already exists"));
    assert_eq!(fs::read(output.join("keep.txt")).unwrap(), b"must stay");
}

#[test]
fn external_export_blocks_source_outputs_and_invalid_sources_without_writes() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("game");
    fs::create_dir(&source).unwrap();
    let error = ffe_tools::external::export_external(&source, &source.join("art")).unwrap_err();
    assert!(error.contains("must not be inside"));
    assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
    let output = temp.path().join("export");
    let error = ffe_tools::external::export_external(&source, &output).unwrap_err();
    assert!(error.contains("inspection failed"));
    assert!(error.contains("FIRSTENC.EXE"));
    assert!(!output.exists());
    let missing_parent = temp.path().join("missing").join("export");
    let error = ffe_tools::external::export_external(&source, &missing_parent).unwrap_err();
    assert!(error.contains("parent must already exist"));
    assert!(!missing_parent.exists());
}

#[test]
fn cli_default_directory_is_beside_executable_not_working_directory() {
    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("bin").join(
        std::path::Path::new(env!("CARGO_BIN_EXE_ffe-tools"))
            .file_name()
            .unwrap(),
    );
    fs::create_dir(executable.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_ffe-tools"), &executable).unwrap();
    fs::create_dir(temp.path().join("game")).unwrap();
    // On Linux, a test thread forking while the copy's write handle is open can make
    // the fresh executable briefly "Text file busy"; retry only that error.
    let mut attempts = 0;
    let output = loop {
        match Command::new(&executable)
            .arg("inspect-game")
            .current_dir(temp.path())
            .output()
        {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && attempts < 100 =>
            {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => break result.unwrap(),
        }
    };
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    let expected = executable.parent().unwrap().join("game");
    assert!(error.contains(&expected.display().to_string()));
}

#[cfg(unix)]
#[test]
fn case_ambiguous_files_and_symbolic_links_are_rejected() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("FIRSTENC.EXE"), b"one").unwrap();
    fs::write(temp.path().join("firstenc.exe"), b"two").unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("journal"), b"synthetic").unwrap();
    symlink(
        outside.path().join("journal"),
        temp.path().join("MISSION.DAT"),
    )
    .unwrap();
    let report = inspect_game(temp.path()).unwrap();
    assert!(report.diagnostics.iter().any(|entry| {
        entry.path == "FIRSTENC.EXE" && entry.message.contains("multiple entries")
    }));
    assert!(report
        .diagnostics
        .iter()
        .any(|entry| { entry.path == "MISSION.DAT" && entry.message.contains("not a link") }));
}

#[test]
fn cache_build_and_inspection_fail_safely_for_invalid_inputs() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("game");
    fs::create_dir(&source).unwrap();
    let output = temp.path().join("data.bin");
    assert!(ffe_tools::cache::build_data(&source, &output)
        .unwrap_err()
        .contains("inspection failed"));
    assert!(!output.exists());
    assert!(
        ffe_tools::cache::build_data(&source, &source.join("data.bin"))
            .unwrap_err()
            .contains("must not be inside")
    );
    fs::write(&output, b"existing data must stay").unwrap();
    assert!(ffe_tools::cache::build_data(&source, &output)
        .unwrap_err()
        .contains("already exists"));
    assert_eq!(fs::read(&output).unwrap(), b"existing data must stay");
    assert!(ffe_tools::cache::inspect_data(&output).is_err());
    let result = tool()
        .arg("inspect-data")
        .arg("--data")
        .arg(&output)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cache ZIP"));
}

#[test]
fn cache_commands_require_explicit_correct_arguments() {
    for arguments in [
        vec!["build-data"],
        vec!["build-data", "--data", "cache.bin"],
        vec!["inspect-data"],
        vec!["inspect-data", "--data"],
        vec!["inspect-data", "--data", "a", "--data", "b"],
        vec!["inspect-data", "--game", "game", "--data", "cache.bin"],
        vec!["inspect-data", "--out", "cache.bin"],
    ] {
        let result = tool().args(arguments).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn inspect_data_json_loads_a_synthetic_cache_without_game_files() {
    let temp = TempDir::new().unwrap();
    let manifest = ffe_content::Manifest {
        format: "ffe-content-cache".to_owned(),
        format_version: 1,
        tool_version: "synthetic".to_owned(),
        source_profile: "synthetic".to_owned(),
        source_files: vec![ffe_content::SourceFile {
            path: "FIRSTENC.EXE".to_owned(),
            size: 1,
            sha256: "a".repeat(64),
        }],
        complete_game_assets: false,
        unresolved: vec!["synthetic palette-only fixture".to_owned()],
        files: Vec::new(),
    };
    let bytes = ffe_content::build(
        manifest,
        vec![ffe_content::Asset {
            id: "base:palette/low".to_owned(),
            path: "palettes/low.rgb6".to_owned(),
            kind: ffe_content::AssetKind::LowPaletteRgb6,
            bytes: vec![1; 384],
        }],
    )
    .unwrap();
    let path = temp.path().join("data.bin");
    fs::write(&path, bytes).unwrap();
    let output = tool()
        .arg("inspect-data")
        .arg("--json")
        .arg("--data")
        .arg(&path)
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source_profile"], "synthetic");
    assert_eq!(report["complete_game_assets"], false);
    assert_eq!(report["files"].as_array().unwrap().len(), 1);
}
