//! `export-images`: every bitmap the game draws, as PNGs for the owner's local
//! upscaler, with a manifest, UHD sizing CSV, exclusions and a contact sheet.
//!
//! Sources: the six cockpit BMPs, the 347 embedded bitmap-table records
//! (`DATA_008240`/`DATA_008241`) and the main font. Embedded sprites hold
//! low-palette indices; model textures hold colour slots that the texture
//! descriptors (`DATA_007811`) fill at draw time (`docs/notes/textures.md`).
//! Nothing here is resized, and the output is not a graphics pack.
use crate::cache::{bitmap_layout, font_layout, texture_layout};
use crate::external::{output_destination, recorded_bytes, write_new_directory};
use crate::{inspect_game, FileRecord, Severity};
use ffe_convert::embedded::{
    decode_bitmaps, decode_colour_map, decode_font, decode_texture_descriptors, EmbeddedBitmap,
};
use ffe_convert::{decode_indexed_bmp, indexed_png, recommended_scale, rgba_png};
use ffe_core::model_texture::{fixed_palette_index, pack_colour, Descriptors};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Classic 320x200 canvas shown at 2880x2160 on a 3840x2160 display.
const CANVAS: [u32; 2] = [320, 200];
const CLASSIC_DISPLAY: [u32; 2] = [2880, 2160];
/// Estimated texture scale until measured scene coverage exists.
const TEXTURE_SCALE: u32 = 8;
/// Primary 64 (land) and secondary 193 (sea): the planet-surface textures F1840
/// queues with run-time colours (`docs/notes/terrain.md`).
const SURFACE_IDS: [u32; 2] = [0x40, 0x121];
/// Font atlas layout: one transparent pixel around each 8x9 glyph.
const GLYPH_CELL: [u32; 2] = [10, 11];
const ATLAS_COLUMNS: u32 = 16;

#[derive(Debug, Serialize)]
pub struct ExportedImage {
    pub id: String,
    pub category: &'static str,
    pub source: String,
    pub source_offset: Option<u64>,
    pub source_size: [u32; 2],
    /// The header's signed draw offset (embedded bitmaps), else zero.
    pub anchor: [i16; 2],
    pub alpha_mode: &'static str,
    pub colours: String,
    pub image: String,
    pub indexed_image: String,
    pub recommended_size: [u32; 2],
    pub recommended_scale: u32,
    pub sizing_basis: &'static str,
    /// Texture descriptor records naming this bitmap (the first colours it).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texture_records: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub review: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Exclusion {
    pub what: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ImageManifest {
    pub format: &'static str,
    pub format_version: u32,
    pub tool_version: &'static str,
    pub source_profile: String,
    pub source_files: Vec<FileRecord>,
    pub display_target: [u32; 2],
    pub classic_display_rectangle: [u32; 2],
    pub counts: BTreeMap<&'static str, usize>,
    pub images: Vec<ExportedImage>,
    pub excluded: Vec<Exclusion>,
    pub files: Vec<FileRecord>,
}

const SPRITE_BASIS: &str = "Screen-space art on the 320x200 canvas shown at 2880x2160 (9 x 10.8 per source pixel): integer 11x.";
const TEXTURE_BASIS: &str = "Estimate: projected model/planet texture, no measured scene coverage yet; 8x. Close-ups can need more; override per asset.";
const FONT_BASIS: &str = "Screen-space glyph atlas on the 320x200 canvas: integer 11x. Upscaled glyphs need manual legibility/metric review.";

fn exclusions() -> Vec<Exclusion> {
    vec![
        Exclusion {
            what: "CD DATA/*.BMP companion stills and *.AVI clips",
            reason: "Optional CD media inside the BIN/CUE image; no disc-image importer, and the installed floppy game/ has none.",
        },
        Exclusion {
            what: "Model geometry, vector text glyph programs, planets, stars, dust and flat-shaded faces",
            reason: "Drawn from model command streams and procedural code, not bitmaps; image upscaling cannot improve them.",
        },
        Exclusion {
            what: "Palettes, the 12-bit colour map, texture descriptors, terrain height maps and shade tables",
            reason: "Lookup data, not displayed images; upscaling them would corrupt the game.",
        },
    ]
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn rgb8(rgb6: &[u8]) -> [u8; 3] {
    [rgb6[0] << 2, rgb6[1] << 2, rgb6[2] << 2]
}

/// Upper entries are assigned at run time (`crate::palette` in the game); a
/// grey ramp keeps them visible and distinct without inventing colours.
fn sprite_palette(low_rgb6: &[u8]) -> Vec<u8> {
    let mut palette = Vec::with_capacity(768);
    for index in 0..256usize {
        if index < 128 {
            palette.extend_from_slice(&rgb8(&low_rgb6[index * 3..index * 3 + 3]));
        } else {
            let grey = ((index - 128) * 2) as u8;
            palette.extend_from_slice(&[grey; 3]);
        }
    }
    palette
}

/// A 12-bit colour as the game shows it: the fixed low entry where the colour
/// map names one (F1327, exact), else the runtime palette's single-cell
/// representative, six-bit `4 * nibble + 1`.
fn colour_rgb(colour: [u8; 3], map: Option<&[u8]>, low_rgb6: &[u8]) -> [u8; 3] {
    if let Some(index) = map.and_then(|map| fixed_palette_index(map, pack_colour(colour))) {
        let start = usize::from(index) * 3;
        if let Some(rgb6) = low_rgb6.get(start..start + 3) {
            return rgb8(rgb6);
        }
    }
    colour.map(|nibble| (nibble.min(15) * 4 + 1) << 2)
}

struct Rendered {
    rgba: Vec<u8>,
    indexed: Vec<u8>,
}

fn render(
    width: u32,
    height: u32,
    indices: &[u8],
    palette: &[u8],
    transparent_zero: bool,
) -> Result<Rendered, String> {
    let mut rgba = Vec::with_capacity(indices.len() * 4);
    for &index in indices {
        let start = usize::from(index) * 3;
        let rgb = palette
            .get(start..start + 3)
            .ok_or("bitmap index outside its export palette")?;
        rgba.extend_from_slice(rgb);
        rgba.push(if transparent_zero && index == 0 {
            0
        } else {
            255
        });
    }
    let alpha = transparent_zero.then_some(&[0u8][..]);
    Ok(Rendered {
        rgba: rgba_png(width, height, &rgba)?,
        indexed: indexed_png(width, height, indices, palette, alpha)?,
    })
}

fn source_scale(source: [u32; 2]) -> Result<u32, String> {
    let displayed = [
        source[0] * CLASSIC_DISPLAY[0] / CANVAS[0],
        (source[1] * CLASSIC_DISPLAY[1]).div_ceil(CANVAS[1]),
    ];
    recommended_scale(source, displayed)
}

struct Embedded<'a> {
    bitmap: &'a EmbeddedBitmap,
    id: u32,
}

impl Embedded<'_> {
    fn name(&self) -> String {
        format!("{}-{:03}", self.bitmap.table, self.bitmap.index)
    }

    fn height(&self) -> u32 {
        if self.bitmap.height == 0 {
            self.bitmap.indices.len() as u32 / self.bitmap.width
        } else {
            self.bitmap.height
        }
    }
}

/// Export every bitmap the game draws. Never overwrites; writes the manifest last.
pub fn export_images(root: &Path, output: &Path) -> Result<ImageManifest, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let destination = output_destination(&root, output)?;
    let inspection = inspect_game(&root)?;
    if !inspection.inspection_passed {
        let errors: Vec<_> = inspection
            .diagnostics
            .iter()
            .filter(|entry| entry.severity == Severity::Error)
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
    let executable = inspection
        .files
        .iter()
        .find(|record| record.path == "FIRSTENC.EXE")
        .ok_or("inspection did not fingerprint FIRSTENC.EXE")?;
    let exe = recorded_bytes(&root, executable)?;
    let decoded = decode_bitmaps(&exe, bitmap_layout(profile.id)?)?;
    let font = decode_font(&exe, font_layout(profile.id)?)?;
    let (descriptors, colour_map) = match texture_layout(profile.id) {
        Some((descriptor_rva, map_rva)) => (
            Some(Descriptors::from_bytes(
                &decode_texture_descriptors(&exe, descriptor_rva)?.bytes,
            )?),
            Some(decode_colour_map(&exe, map_rva)?.bytes),
        ),
        None => (None, None),
    };
    let low = &decoded.low_palette_rgb6;

    // Bitmap id (primary 0..96, then secondary) -> descriptor records naming it.
    let mut records: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    if let Some(table) = &descriptors {
        for record in 1..table.len() {
            let descriptor = table.get(record as u16)?;
            records
                .entry(descriptor.bitmap_id())
                .or_default()
                .push(record);
        }
    }

    let mut images = Vec::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    // Cockpit cabins: each BMP keeps its own 8-bit palette.
    for record in &inspection.files {
        if !(record.path.starts_with("CABIN") && record.path.ends_with(".BMP")) {
            continue;
        }
        let bytes = recorded_bytes(&root, record)?;
        let image =
            decode_indexed_bmp(&bytes).map_err(|error| format!("{}: {error}", record.path))?;
        let name = record.path.trim_end_matches(".BMP").to_ascii_lowercase();
        let path = format!("cockpit/{name}.png");
        let scale = source_scale([image.width, image.height])?;
        files.push((format!("images/{path}"), image.rgba_png()?));
        files.push((format!("indexed/{path}"), image.indexed_png()?));
        images.push(ExportedImage {
            id: format!("base:cockpit/{name}"),
            category: "cockpit",
            source: record.path.clone(),
            source_offset: None,
            source_size: [image.width, image.height],
            anchor: [0, 0],
            alpha_mode: "opaque",
            colours: "the BMP's own 8-bit palette".to_owned(),
            image: format!("images/{path}"),
            indexed_image: format!("indexed/{path}"),
            recommended_size: [image.width * scale, image.height * scale],
            recommended_scale: scale,
            sizing_basis: SPRITE_BASIS,
            texture_records: Vec::new(),
            review: Vec::new(),
        });
    }

    let sprites = sprite_palette(low);
    for (position, bitmap) in decoded.bitmaps.iter().enumerate() {
        let entry = Embedded {
            bitmap,
            id: position as u32,
        };
        let width = bitmap.width;
        let height = entry.height();
        let max = bitmap.indices.iter().copied().max().unwrap_or(0);
        let named = records.get(&entry.id).cloned().unwrap_or_default();
        let block = bitmap.height == 0;
        // Slot textures hold 0..7; an unnamed 64/128-wide one is a texture too.
        let texture = !named.is_empty()
            || SURFACE_IDS.contains(&entry.id)
            || block
            || (matches!(width, 64 | 128) && height == width && max < 8);
        let mut review = Vec::new();
        let (category, palette, colours, basis, scale) = if texture {
            let mut palette = Vec::new();
            // A block's 0..20 values are a terrain ramp, not a record's seven slots.
            let colours = match named
                .first()
                .filter(|_| !block)
                .and_then(|record| descriptors.as_ref()?.get(*record as u16).ok())
            {
                Some(descriptor) => {
                    let slots = descriptor.colours.len();
                    for slot in 0..=usize::from(max) {
                        let rgb = match descriptor.colours.get(slot) {
                            Some(colour) => colour_rgb(*colour, colour_map.as_deref(), low),
                            None => [0; 3],
                        };
                        palette.extend_from_slice(&rgb);
                    }
                    if usize::from(max) >= slots {
                        review.push(format!(
                            "Texels use slot(s) {slots}..={max}, which faces never fill (stale in the original); shown black."
                        ));
                    }
                    let variants = named
                        .iter()
                        .filter_map(|record| descriptors.as_ref()?.get(*record as u16).ok())
                        .filter(|other| other.colours != descriptor.colours)
                        .count();
                    if variants > 0 {
                        review.push(format!(
                            "{} texture records name this bitmap, {variants} with other colours; preview uses record {}.",
                            named.len(),
                            named[0]
                        ));
                    }
                    format!(
                        "texture record {}'s unshaded slot colours (the game adds per-face lighting)",
                        named[0]
                    )
                }
                None => {
                    let top = u32::from(max.max(1));
                    for slot in 0..=u32::from(max) {
                        palette.extend_from_slice(&[(slot * 255 / top) as u8; 3]);
                    }
                    review.push(
                        "Colour slots are filled at run time (planet height/sea colours or an unnamed record); preview is a grey slot ramp, not the game's colours."
                            .to_owned(),
                    );
                    "grey ramp over colour slots (run-time colours)".to_owned()
                }
            };
            if block {
                review.push(
                    "Raw 64x512 texture block (header height 0): eight 64x64 tiles or explicit-coordinate rows; layout unconfirmed."
                        .to_owned(),
                );
            }
            let category = if block {
                "texture_block"
            } else if SURFACE_IDS.contains(&entry.id) {
                "planet_texture"
            } else {
                "texture"
            };
            (category, palette, colours, TEXTURE_BASIS, TEXTURE_SCALE)
        } else {
            if max >= 128 {
                let upper = bitmap.indices.iter().filter(|index| **index >= 128).count();
                review.push(format!(
                    "{upper} pixels use upper palette indices, assigned at run time; shown as a grey ramp."
                ));
            }
            (
                "sprite",
                sprites.clone(),
                "fixed low palette (six-bit, expanded by shift 2)".to_owned(),
                SPRITE_BASIS,
                source_scale([width, height])?,
            )
        };
        let name = entry.name();
        let path = format!("{category}/{name}.png");
        let rendered = render(width, height, &bitmap.indices, &palette, bitmap.masked_zero)?;
        files.push((format!("images/{path}"), rendered.rgba));
        files.push((format!("indexed/{path}"), rendered.indexed));
        images.push(ExportedImage {
            id: format!("base:bitmap/{}/{:03}", bitmap.table, bitmap.index),
            category,
            source: "FIRSTENC.EXE".to_owned(),
            source_offset: Some(bitmap.source_offset as u64),
            source_size: [width, height],
            anchor: [bitmap.offset_x, bitmap.offset_y],
            alpha_mode: if bitmap.masked_zero {
                "index 0 transparent"
            } else {
                "opaque"
            },
            colours,
            image: format!("images/{path}"),
            indexed_image: format!("indexed/{path}"),
            recommended_size: [width * scale, height * scale],
            recommended_scale: scale,
            sizing_basis: basis,
            texture_records: named,
            review,
        });
    }

    // Main font: 116 ten-byte records (nine MSB-left rows, then an advance),
    // codes 32..147, as a white-on-transparent atlas.
    let glyphs = font.bytes.len() / 10;
    let rows = (glyphs as u32).div_ceil(ATLAS_COLUMNS);
    let size = [ATLAS_COLUMNS * GLYPH_CELL[0], rows * GLYPH_CELL[1]];
    let mut atlas = vec![0u8; (size[0] * size[1]) as usize];
    let mut advances = Vec::with_capacity(glyphs);
    for (glyph, record) in font.bytes.chunks_exact(10).enumerate() {
        let cell = [
            glyph as u32 % ATLAS_COLUMNS * GLYPH_CELL[0] + 1,
            glyph as u32 / ATLAS_COLUMNS * GLYPH_CELL[1] + 1,
        ];
        for (row, mask) in record[..9].iter().enumerate() {
            for column in 0..8u32 {
                if mask & (0x80 >> column) != 0 {
                    let at = (cell[1] + row as u32) * size[0] + cell[0] + column;
                    atlas[at as usize] = 1;
                }
            }
        }
        advances.push(record[9]);
    }
    let rendered = render(size[0], size[1], &atlas, &[0, 0, 0, 255, 255, 255], true)?;
    files.push(("images/font/main.png".to_owned(), rendered.rgba));
    files.push(("indexed/font/main.png".to_owned(), rendered.indexed));
    files.push((
        "images/font/main.json".to_owned(),
        serde_json::to_vec_pretty(&serde_json::json!({
            "first_code": 32,
            "glyphs": glyphs,
            "columns": ATLAS_COLUMNS,
            "cell": GLYPH_CELL,
            "glyph_origin_in_cell": [1, 1],
            "glyph_size": [8, 9],
            "advances": advances,
        }))
        .map_err(|error| error.to_string())?,
    ));
    let scale = source_scale(size)?;
    images.push(ExportedImage {
        id: "base:font/main".to_owned(),
        category: "font",
        source: "FIRSTENC.EXE".to_owned(),
        source_offset: Some(font.source_offset as u64),
        source_size: size,
        anchor: [0, 0],
        alpha_mode: "transparent background",
        colours: "white glyphs; the game draws them in any colour".to_owned(),
        image: "images/font/main.png".to_owned(),
        indexed_image: "indexed/font/main.png".to_owned(),
        recommended_size: [size[0] * scale, size[1] * scale],
        recommended_scale: scale,
        sizing_basis: FONT_BASIS,
        texture_records: Vec::new(),
        review: vec![format!(
            "Atlas of {glyphs} glyphs (codes 32..{}), 16 per row in {}x{} cells with the 8x9 glyph at (1,1); advances in images/font/main.json.",
            31 + glyphs,
            GLYPH_CELL[0],
            GLYPH_CELL[1]
        )],
    });

    let mut counts = BTreeMap::new();
    for image in &images {
        *counts.entry(image.category).or_insert(0) += 1;
    }
    if images.len() != 6 + 347 + 1 {
        return Err("image inventory changed unexpectedly; no output written".to_owned());
    }

    files.push(("sizes.csv".to_owned(), sizes_csv(&images).into_bytes()));
    files.push((
        "index.html".to_owned(),
        contact_sheet(profile.id, &images, &counts).into_bytes(),
    ));
    let excluded = exclusions();
    let mut excluded_json =
        serde_json::to_vec_pretty(&excluded).map_err(|error| error.to_string())?;
    excluded_json.push(b'\n');
    files.push(("excluded.json".to_owned(), excluded_json));
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut manifest = ImageManifest {
        format: "ffe-image-export",
        format_version: 1,
        tool_version: env!("CARGO_PKG_VERSION"),
        source_profile: profile.id.to_owned(),
        source_files: inspection.files,
        display_target: [3840, 2160],
        classic_display_rectangle: CLASSIC_DISPLAY,
        counts,
        images,
        excluded,
        files: Vec::new(),
    };
    manifest.files = files
        .iter()
        .map(|(path, bytes)| FileRecord {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: sha256(bytes),
        })
        .collect();
    let mut json = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    json.push(b'\n');
    // The completion manifest is written last; an interrupted output is never a valid export.
    files.push(("manifest.json".to_owned(), json));
    write_new_directory(&destination, files)?;
    Ok(manifest)
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn sizes_csv(images: &[ExportedImage]) -> String {
    let mut csv = String::from("id,category,source,source_width,source_height,anchor_x,anchor_y,alpha_mode,target_width,target_height,integer_scale,image,sizing_basis,review\n");
    for image in images {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            image.id,
            image.category,
            image.source,
            image.source_size[0],
            image.source_size[1],
            image.anchor[0],
            image.anchor[1],
            csv_field(image.alpha_mode),
            image.recommended_size[0],
            image.recommended_size[1],
            image.recommended_scale,
            image.image,
            csv_field(image.sizing_basis),
            csv_field(&image.review.join(" ")),
        ));
    }
    csv
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn contact_sheet(
    profile: &str,
    images: &[ExportedImage],
    counts: &BTreeMap<&'static str, usize>,
) -> String {
    let mut html = String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>FFE image export</title><style>body{font:14px sans-serif;background:#1b1b1f;color:#e8e8ea;margin:16px}h2{margin-top:32px;border-bottom:1px solid #444}figure{display:inline-block;vertical-align:top;margin:6px;padding:6px;background:#2a2a30;border-radius:4px;max-width:340px}img{image-rendering:pixelated;background:repeating-conic-gradient(#3a3a40 0 25%,#2a2a30 0 50%) 0 0/16px 16px;max-width:320px}figcaption{font-size:12px;color:#b8b8c0;margin-top:4px}.review{color:#f0c060}</style>");
    html.push_str(&format!("<h1>First Encounters image export</h1><p>Source profile <code>{profile}</code>. Every bitmap the game draws: cockpit art, embedded sprites/UI, model and planet textures, raw texture blocks and the main font. Images stay at original size and are shown here at 2x; <code>sizes.csv</code> gives the recommended UHD size of each. Do not upload original or derived game art. Model geometry, vector text and procedural planets are not bitmaps; see <code>excluded.json</code>.</p><p>"));
    for (category, count) in counts {
        html.push_str(&format!(
            "<a href=\"#{category}\">{category}</a>: {count} &nbsp; "
        ));
    }
    html.push_str("</p>");
    for category in counts.keys() {
        html.push_str(&format!("<h2 id=\"{category}\">{category}</h2>"));
        for image in images.iter().filter(|image| image.category == *category) {
            let scale = if image.source_size[0] > 160 { 1 } else { 2 };
            html.push_str(&format!(
                "<figure><img src=\"{}\" width=\"{}\" height=\"{}\" alt=\"{}\"><figcaption>{}<br>{}x{} → {}x{} ({}x){}{}</figcaption></figure>",
                image.image,
                image.source_size[0] * scale,
                image.source_size[1] * scale,
                html_escape(&image.id),
                html_escape(&image.id),
                image.source_size[0],
                image.source_size[1],
                image.recommended_size[0],
                image.recommended_size[1],
                image.recommended_scale,
                if image.alpha_mode == "opaque" { String::new() } else { format!("<br>{}", html_escape(image.alpha_mode)) },
                image
                    .review
                    .iter()
                    .map(|note| format!("<br><span class=\"review\">{}</span>", html_escape(note)))
                    .collect::<String>(),
            ));
        }
    }
    html.push_str("</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_use_the_fixed_map_else_the_runtime_representative() {
        let mut low = vec![0u8; 384];
        low[21..24].copy_from_slice(&[10, 20, 30]);
        let mut map = vec![0u8; 4096];
        map[0x777] = 8;
        assert_eq!(colour_rgb([7, 7, 7], Some(&map), &low), [40, 80, 120]);
        assert_eq!(colour_rgb([15, 0, 1], Some(&map), &low), [244, 4, 20]);
        assert_eq!(colour_rgb([7, 7, 7], None, &low), [116; 3]);
    }

    #[test]
    fn sprite_palette_keeps_low_entries_and_greys_the_upper_half() {
        let low: Vec<u8> = (0..384).map(|value| (value % 64) as u8).collect();
        let palette = sprite_palette(&low);
        assert_eq!(palette.len(), 768);
        assert_eq!(palette[3..6], [12, 16, 20]);
        assert_eq!(palette[128 * 3..129 * 3], [0; 3]);
        assert_eq!(palette[255 * 3..], [254; 3]);
    }

    #[test]
    fn masked_index_zero_is_transparent_and_bad_indices_fail() {
        let palette = [1, 2, 3, 4, 5, 6];
        assert!(render(2, 1, &[0, 1], &palette, true).is_ok());
        assert!(render(2, 1, &[0, 2], &palette, false).is_err());
    }

    #[test]
    fn screen_art_scales_to_eleven_times() {
        assert_eq!(source_scale([320, 158]).unwrap(), 11);
        assert_eq!(source_scale([8, 13]).unwrap(), 11);
    }

    #[test]
    fn csv_fields_with_commas_are_quoted() {
        assert_eq!(csv_field("a, \"b\""), "\"a, \"\"b\"\"\"");
        assert_eq!(csv_field("plain"), "plain");
    }
}
