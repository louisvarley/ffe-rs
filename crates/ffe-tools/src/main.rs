use ffe_tools::{inspect_game, Inspection};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
ffe-tools: local First Encounters installation tools

Usage:
  ffe-tools inspect-game [--game <directory>] [--json]
  ffe-tools export-external --game <directory> --out <new-directory> [--json]
  ffe-tools export-images --game <directory> --out <new-directory> [--json]
  ffe-tools build-data --game <directory> --out <new-file> [--json]
  ffe-tools inspect-data --data <file> [--json]
  ffe-tools verify-reference --vectors <jsonl> --state <jsonl> [--json]
  ffe-tools model-census [--game <directory>] [--json]
  ffe-tools --help
  ffe-tools --version

Defaults to game beside this executable, not the shell working directory.
Inspection is read-only. Exit codes: 0 passed/help, 1 invalid installation,
2 command, fatal directory or output failure. Per-file errors cause exit 1.
A recognized version is not proof of a working game converter.
export-external converts only six cockpit BMPs and 23 PCM effects, with
per-image UHD guidance and a contact sheet. It never overwrites an output.
export-images exports every bitmap the game draws (cabins, 345 embedded
sprites/textures, two raw texture blocks, the main font) as original-size PNGs
with a manifest, UHD sizes.csv, excluded.json and a contact sheet, for local
upscaling. It never overwrites an output and is not a graphics pack.
build-data imports 345 bitmaps, two raw texture blocks, the fixed low palette, cabins and
effects and main font into a deterministic partial cache. V3 adds two original
station vertex programs; v4/v5 add root commands/normals and v6 the original sine table; v7 imports every model that decodes; v8 adds the texture descriptor table and the 12-bit fixed colour map (floppy 1.02).
inspect-data verifies every entry. Bounded native command execution is not yet
complete station rendering. Submodels, gameplay tables, the other 16 songs and full conversion remain absent.
verify-reference compares fixed legacy RNG/timer vectors and bounded new-game
clock transitions against native Rust primitives. It is not full flight validation.
model-census reports, read-only, how far each of the 465 original models gets
through native import, decoding and execution, and what blocks the rest.
";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    InspectGame,
    ExportExternal,
    ExportImages,
    BuildData,
    InspectData,
    VerifyReference,
    ModelCensus,
}

struct Options {
    command: Command,
    game: PathBuf,
    json: bool,
    output: Option<PathBuf>,
    data: Option<PathBuf>,
    vectors: Option<PathBuf>,
    state: Option<PathBuf>,
}

fn parse_options(args: Vec<OsString>) -> Result<Options, String> {
    let mut game = None;
    let mut json = false;
    let mut output = None;
    let mut data = None;
    let mut vectors = None;
    let mut state = None;
    let mut args = args.into_iter();
    let command = args
        .next()
        .ok_or("Expected a command. Use --help for usage.")?;
    let command = if command == "inspect-game" {
        Command::InspectGame
    } else if command == "export-external" {
        Command::ExportExternal
    } else if command == "export-images" {
        Command::ExportImages
    } else if command == "build-data" {
        Command::BuildData
    } else if command == "inspect-data" {
        Command::InspectData
    } else if command == "verify-reference" {
        Command::VerifyReference
    } else if command == "model-census" {
        Command::ModelCensus
    } else {
        return Err("Unknown command. Use --help for usage.".to_owned());
    };
    let exporting = matches!(
        command,
        Command::ExportExternal | Command::ExportImages | Command::BuildData
    );
    while let Some(arg) = args.next() {
        if arg == "--game"
            && matches!(
                command,
                Command::InspectGame
                    | Command::ExportExternal
                    | Command::ExportImages
                    | Command::BuildData
                    | Command::ModelCensus
            )
        {
            if game.is_some() {
                return Err("--game was supplied more than once.".to_owned());
            }
            let value = args.next().ok_or("--game requires a directory.")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("--game requires a directory, not another option.".to_owned());
            }
            game = Some(PathBuf::from(value));
        } else if arg == "--out" && exporting && output.is_none() {
            let value = args.next().ok_or("--out requires a new destination.")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("--out requires a destination, not another option.".to_owned());
            }
            output = Some(PathBuf::from(value));
        } else if arg == "--data" && command == Command::InspectData && data.is_none() {
            let value = args.next().ok_or("--data requires a cache file.")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("--data requires a cache file, not another option.".to_owned());
            }
            data = Some(PathBuf::from(value));
        } else if command == Command::VerifyReference
            && ((arg == "--vectors" && vectors.is_none()) || (arg == "--state" && state.is_none()))
        {
            let value = args
                .next()
                .ok_or("reference option requires a trace path")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("reference option requires a path, not another option".to_owned());
            }
            if arg == "--vectors" {
                vectors = Some(PathBuf::from(value));
            } else {
                state = Some(PathBuf::from(value));
            }
        } else if arg == "--json" && !json {
            json = true;
        } else {
            return Err(format!(
                "Unknown or duplicate argument: {}",
                arg.to_string_lossy()
            ));
        }
    }
    if exporting && output.is_none() {
        return Err("Export/build requires --out <new-destination>.".to_owned());
    }
    if command == Command::InspectData && data.is_none() {
        return Err("inspect-data requires --data <file>.".to_owned());
    }
    if command == Command::VerifyReference && (vectors.is_none() || state.is_none()) {
        return Err("verify-reference requires --vectors <jsonl> and --state <jsonl>.".to_owned());
    }
    let game = match game {
        Some(path) => path,
        None => std::env::current_exe()
            .map_err(|error| format!("Cannot resolve executable location: {error}"))?
            .parent()
            .ok_or("Executable has no parent directory.")?
            .join("game"),
    };
    Ok(Options {
        command,
        game,
        json,
        output,
        data,
        vectors,
        state,
    })
}

fn write_report(output: &mut impl Write, report: &Inspection, json: bool) -> Result<(), String> {
    if json {
        serde_json::to_writer_pretty(&mut *output, report)
            .map_err(|error| format!("Cannot write JSON report: {error}"))?;
        writeln!(output).map_err(|error| error.to_string())?;
    } else {
        writeln!(
            output,
            "Source: {}\nGameplay reference: {}\nInspection: {}\nFiles fingerprinted: {}\nOptional media: {} AVI, {} BMP",
            report.source_profile.map_or("unrecognized", |profile| profile.id),
            report.simulation_reference,
            if report.inspection_passed { "passed" } else { "FAILED" },
            report.files.len(),
            report.optional_media.avi_files,
            report.optional_media.bmp_files,
        ).map_err(|error| error.to_string())?;
        for diagnostic in &report.diagnostics {
            writeln!(
                output,
                "{:?} [{}] {}: {}",
                diagnostic.severity, diagnostic.code, diagnostic.path, diagnostic.message
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn run() -> Result<u8, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut output = io::stdout().lock();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        output
            .write_all(HELP.as_bytes())
            .map_err(|error| error.to_string())?;
        return Ok(0);
    }
    if args.len() == 1 && args[0] == "--version" {
        writeln!(output, "ffe-tools {}", env!("CARGO_PKG_VERSION"))
            .map_err(|error| error.to_string())?;
        return Ok(0);
    }
    let options = parse_options(args)?;
    if options.command == Command::ModelCensus {
        let census = ffe_tools::census::model_census(&options.game)?;
        if options.json {
            serde_json::to_writer_pretty(&mut output, &census)
                .map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        } else {
            writeln!(
                output,
                "Source: {}\nModels executed natively: {} of {}\nBlockers:",
                census.source_profile, census.executed, census.models
            )
            .map_err(|error| error.to_string())?;
            for (blocker, count) in &census.blockers {
                writeln!(output, "  {count:4}  {blocker}").map_err(|error| error.to_string())?;
            }
            writeln!(output, "Synthetic inputs; glyphs/children not executed; not verified against the original.")
                .map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    if options.command == Command::VerifyReference {
        let report = ffe_tools::reference::verify_reference(
            options.vectors.as_deref().ok_or("missing --vectors")?,
            options.state.as_deref().ok_or("missing --state")?,
        )?;
        if options.json {
            serde_json::to_writer_pretty(&mut output, &report)
                .map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        } else {
            writeln!(output, "Rust/reference match: {} RNG vectors, {} timer vectors, {} new-game clock transitions.\n{} state frames; new game reached at frame {}.\nBounded numeric/state coverage only; flight, docking, trading and save/load are not validated.",
                report.rng_vectors, report.timer_vectors, report.game_clock_transitions,
                report.state_frames, report.new_game_frame).map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    if matches!(options.command, Command::InspectData | Command::BuildData) {
        let manifest = if options.command == Command::InspectData {
            ffe_tools::cache::inspect_data(options.data.as_deref().ok_or("missing --data")?)?
        } else {
            ffe_tools::cache::build_data(
                &options.game,
                options.output.as_deref().ok_or("missing --out")?,
            )?
        };
        if options.json {
            serde_json::to_writer_pretty(&mut output, &manifest)
                .map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        } else {
            writeln!(output, "Verified content cache v{}: {}\nAssets: {}; source fingerprints: {}\nPartial content only; not a complete game cache. Runtime upper palette, full model-command execution/submodels, gameplay tables, other fonts, journal and CD media remain unresolved.",
                manifest.format_version, manifest.source_profile, manifest.files.len(), manifest.source_files.len()).map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    if options.command == Command::ExportImages {
        let destination = options.output.as_deref().ok_or("missing --out")?;
        let manifest = ffe_tools::images::export_images(&options.game, destination)?;
        if options.json {
            serde_json::to_writer_pretty(&mut output, &manifest)
                .map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        } else {
            let counts: Vec<_> = manifest
                .counts
                .iter()
                .map(|(category, count)| format!("{count} {category}"))
                .collect();
            let review = manifest
                .images
                .iter()
                .filter(|image| !image.review.is_empty())
                .count();
            writeln!(
                output,
                "Exported {} images to {}: {}.
{review} carry review notes (run-time colours, unfilled slots, upper palette).
Open index.html for previews and sizes.csv for UHD recommendations. No image resizing performed.",
                manifest.images.len(),
                destination.display(),
                counts.join(", ")
            )
            .map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    if let Some(destination) = options.output {
        let manifest = ffe_tools::external::export_external(&options.game, &destination)?;
        if options.json {
            serde_json::to_writer_pretty(&mut output, &manifest)
                .map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        } else {
            writeln!(output, "Exported {} cockpit images and {} PCM effects to {}.\nPartial export only: embedded game images/models, music and CD media are not converted.\nOpen index.html for previews and sizes.csv for UHD recommendations. No image resizing performed.",
                manifest.images.len(), manifest.sound_effects, destination.display()).map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    let report = inspect_game(&options.game)?;
    write_report(&mut output, &report, options.json)?;
    Ok(if report.inspection_passed { 0 } else { 1 })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_options_accept_both_orders_and_require_output_after_parsing() {
        for arguments in [
            vec![
                "export-external",
                "--game",
                "source",
                "--out",
                "destination",
                "--json",
            ],
            vec![
                "export-external",
                "--json",
                "--out",
                "destination",
                "--game",
                "source",
            ],
        ] {
            let options =
                parse_options(arguments.into_iter().map(OsString::from).collect()).unwrap();
            assert_eq!(options.game, PathBuf::from("source"));
            assert_eq!(options.output, Some(PathBuf::from("destination")));
            assert!(options.json);
        }
        for arguments in [
            vec!["export-external"],
            vec!["export-external", "--game", "source"],
            vec!["export-external", "--out", "one", "--out", "two"],
        ] {
            assert!(parse_options(arguments.into_iter().map(OsString::from).collect()).is_err());
        }
    }
}
