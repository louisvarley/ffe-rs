use crate::{find_child, inspect_game, read_bounded, FileRecord, MAX_ASSET_BYTES};
use ffe_convert::{decode_indexed_bmp, pcm16_mono_22050_wav, recommended_scale};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct ImageAsset {
    pub id: String,
    pub source: String,
    pub source_sha256: String,
    pub source_size: [u32; 2],
    pub source_palette_bits: u8,
    pub alpha_mode: &'static str,
    pub logical_rectangle: [u32; 4],
    pub preview: String,
    pub indexed_image: String,
    pub palette: String,
    pub recommended_size: [u32; 2],
    pub recommended_scale: u32,
    pub sizing_basis: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExternalManifest {
    pub format: &'static str,
    pub format_version: u32,
    pub tool_version: &'static str,
    pub source_profile: String,
    pub source_files: Vec<FileRecord>,
    pub complete_game_assets: bool,
    pub display_target: [u32; 2],
    pub classic_display_rectangle: [u32; 2],
    pub images: Vec<ImageAsset>,
    pub sound_effects: usize,
    pub unresolved: Vec<&'static str>,
    pub files: Vec<FileRecord>,
}

pub(crate) fn recorded_bytes(root: &Path, record: &FileRecord) -> Result<Vec<u8>, String> {
    let mut path = root.to_path_buf();
    for component in record.path.split('/') {
        path = find_child(&path, component)?
            .ok_or_else(|| format!("{}: source disappeared after inspection", record.path))?;
    }
    let bytes = read_bounded(&path, MAX_ASSET_BYTES)?;
    if bytes.len() as u64 != record.size || format!("{:x}", Sha256::digest(&bytes)) != record.sha256
    {
        return Err(format!(
            "{}: source changed after inspection; retry with a stable installation",
            record.path
        ));
    }
    Ok(bytes)
}

pub(crate) fn output_destination(root: &Path, output: &Path) -> Result<PathBuf, String> {
    let name = output
        .file_name()
        .ok_or("output must name a new file or directory")?;
    if !matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
    ) {
        return Err("output must name a normal file or directory".to_owned());
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|error| {
        format!(
            "{}: output parent must already exist: {error}",
            parent.display()
        )
    })?;
    if parent.starts_with(root) {
        return Err("output must not be inside the player's game directory".to_owned());
    }
    let destination = parent.join(name);
    match destination.symlink_metadata() {
        Ok(_) => {
            return Err(format!(
                "{}: output already exists; choose a new destination (nothing will be overwritten)",
                destination.display()
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "{}: cannot inspect output: {error}",
                destination.display()
            ))
        }
    }
    Ok(destination)
}

fn push_file(files: &mut Vec<(String, Vec<u8>)>, path: String, bytes: Vec<u8>) {
    files.push((path, bytes));
}

/// Export only verified external cabins and PCM effects, never a full game cache.
pub fn export_external(root: &Path, output: &Path) -> Result<ExternalManifest, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let destination = output_destination(&root, output)?;
    let inspection = inspect_game(&root)?;
    if !inspection.inspection_passed {
        let errors: Vec<_> = inspection
            .diagnostics
            .iter()
            .filter(|entry| entry.severity == crate::Severity::Error)
            .map(|entry| format!("{} [{}]: {}", entry.path, entry.code, entry.message))
            .collect();
        return Err(format!(
            "source inspection failed; no output written:\n{}",
            errors.join("\n")
        ));
    }
    let profile = inspection
        .source_profile
        .ok_or("inspection passed without a source profile")?;
    let mut manifest = ExternalManifest {
        format: "ffe-external-assets",
        format_version: 1,
        tool_version: env!("CARGO_PKG_VERSION"),
        source_profile: profile.id.to_owned(),
        source_files: Vec::new(),
        complete_game_assets: false,
        display_target: [3840, 2160],
        classic_display_rectangle: [2880, 2160],
        images: Vec::new(),
        sound_effects: 0,
        unresolved: vec![
            "Embedded executable images, fonts, palettes, models and gameplay tables are not exported.",
            "MISSION.DAT decoding and HMP-to-MIDI conversion are not implemented.",
            "Optional CD companion BMPs and AVI clips are not exported.",
            "This is not data.bin, a complete upscale export, or an installable graphics pack.",
        ],
        files: Vec::new(),
    };
    let mut files = Vec::new();
    for record in &inspection.files {
        if record.path.starts_with("CABIN") && record.path.ends_with(".BMP") {
            let bytes = recorded_bytes(&root, record)?;
            let image =
                decode_indexed_bmp(&bytes).map_err(|error| format!("{}: {error}", record.path))?;
            if [image.width, image.height] != [320, 158] {
                return Err(format!(
                    "{}: expected verified 320 x 158 cockpit artwork",
                    record.path
                ));
            }
            let name = record.path.trim_end_matches(".BMP").to_ascii_lowercase();
            let preview = format!("images/{name}.png");
            let indexed = format!("classic/{name}.png");
            let palette = format!("palettes/{name}.json");
            push_file(&mut files, preview.clone(), image.rgba_png()?);
            push_file(&mut files, indexed.clone(), image.indexed_png()?);
            push_file(
                &mut files,
                palette.clone(),
                serde_json::to_vec(&image.palette_rgb).map_err(|error| error.to_string())?,
            );
            let displayed_height = (u64::from(image.height) * 2160).div_ceil(200) as u32;
            let scale = recommended_scale([image.width, image.height], [2880, displayed_height])?;
            manifest.images.push(ImageAsset {
                id: format!("base:cockpit/{name}"),
                source: record.path.clone(),
                source_sha256: record.sha256.clone(),
                source_size: [image.width, image.height],
                source_palette_bits: 8,
                alpha_mode: "opaque",
                logical_rectangle: [0, 0, image.width, image.height],
                preview,
                indexed_image: indexed,
                palette,
                recommended_size: [image.width * scale, image.height * scale],
                recommended_scale: scale,
                sizing_basis: "Conservative integer source-aspect scale for 2880x2160 corrected 4:3 canvas on a 3840x2160 display; cockpit covers logical 320x158 of 320x200. No resizing performed.",
            });
        } else if record.path.starts_with("FX/") && record.path.ends_with(".RAW") {
            let bytes = recorded_bytes(&root, record)?;
            let wav = pcm16_mono_22050_wav(&bytes)
                .map_err(|error| format!("{}: {error}", record.path))?;
            let name = record
                .path
                .trim_start_matches("FX/")
                .trim_end_matches(".RAW")
                .to_ascii_lowercase();
            push_file(&mut files, format!("audio/fx/{name}.wav"), wav);
            manifest.sound_effects += 1;
        }
    }
    if manifest.images.len() != 6 || manifest.sound_effects != 23 {
        return Err("external asset inventory changed unexpectedly; no output written".to_owned());
    }
    manifest.source_files = inspection.files;
    let mut csv = String::from("id,source_sha256,source_width,source_height,target_width,target_height,integer_scale,alpha_mode,preview\n");
    let mut html = String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>FFE external cockpit export (partial)</title><style>body{font:16px sans-serif;background:#222;color:#eee}figure{display:inline-block}img{width:320px;height:158px;image-rendering:pixelated}</style><h1>External cockpit export only</h1><p>Partial inventory: 6 cabins, not all game textures. No embedded assets or CD stills. Recommended dimensions are for a UHD display; images here remain original size. Do not upload original or derived game art.</p>");
    for image in &manifest.images {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            image.id,
            image.source_sha256,
            image.source_size[0],
            image.source_size[1],
            image.recommended_size[0],
            image.recommended_size[1],
            image.recommended_scale,
            image.alpha_mode,
            image.preview
        ));
        html.push_str(&format!("<figure><img src=\"{}\" alt=\"{}\"><figcaption>{}: {}x{}; recommended {}x{} ({}x)</figcaption></figure>",
            image.preview, image.id, image.id, image.source_size[0], image.source_size[1],
            image.recommended_size[0], image.recommended_size[1], image.recommended_scale));
    }
    html.push_str("</html>\n");
    push_file(&mut files, "sizes.csv".to_owned(), csv.into_bytes());
    push_file(&mut files, "index.html".to_owned(), html.into_bytes());
    files.sort_by(|left, right| left.0.cmp(&right.0));
    manifest.files = files
        .iter()
        .map(|(path, bytes)| FileRecord {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        })
        .collect();
    let mut json = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    json.push(b'\n');
    // The completion manifest is written last; an interrupted output is never a valid export.
    push_file(&mut files, "manifest.json".to_owned(), json);
    write_new_directory(&destination, files)?;
    Ok(manifest)
}

/// Create `destination` and write every file with `create_new`, in order; the
/// caller puts its completion manifest last.
pub(crate) fn write_new_directory(
    destination: &Path,
    files: Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| {
        format!(
            "{}: cannot create new output: {error}",
            destination.display()
        )
    })?;
    for (relative, bytes) in files {
        let path = destination.join(relative);
        let result = (|| -> Result<(), std::io::Error> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(format!("{}: export is incomplete: {error}. No existing files were overwritten; choose a fresh output directory before retrying.", path.display()));
        }
    }
    Ok(())
}
