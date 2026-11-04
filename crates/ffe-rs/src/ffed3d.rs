//! A player's installed FFED3D, whose art the Modern engine draws with
//! (`docs/notes/ffed3d-modern.md`). Nothing here is part of the game: the
//! files stay in the player's folder and are only read.
//!
//! So far: the `texNNN` textures. FFED3D gives a textured face of the
//! original's models `tex(400 + record)`, `record` being the material's
//! texture record (`material & 0xfff`, `DATA_007811`), and shows it in full
//! colour (`ffed3d-master/win/gfx.cpp` `setMaterial`,
//! `win/w32video.cpp`'s loader: `.png`, then `.dds`, then `.tga`; the port
//! reads PNG). FFED3D's cabin backdrops (`Textures/CABIN<n>.png`) are not used: the
//! owner keeps the port's own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// FFED3D's texture number for a textured face's record (`400 + record`).
pub fn texture_number(record: u32) -> u32 {
    400 + record
}

/// A child of `dir` named `name` in any letter case (FFED3D installs keep
/// `MODELS`, `Textures`, `CABIN0.png` and the like).
pub fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
        .map(|entry| entry.path())
}

/// The install's texture folder (`Textures`, or `resources/textures` in the
/// source tree's layout).
pub fn textures_dir(root: &Path) -> Option<PathBuf> {
    find(root, "textures").or_else(|| find(&find(root, "resources")?, "textures"))
}

/// A PNG decoded to RGBA bytes: its width, height and pixels.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let (width, height) = (info.width, info.height);
    let pixels = (width * height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer[..pixels * 4].to_vec(),
        png::ColorType::Rgb => buffer[..pixels * 3]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer[..pixels * 2]
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buffer[..pixels]
            .iter()
            .flat_map(|g| [*g, *g, *g, 255])
            .collect(),
        png::ColorType::Indexed => return Err("an indexed PNG was not expanded".to_owned()),
    };
    Ok((width, height, rgba))
}

/// FFED3D's `texNNN` textures, read when first asked for and kept.
#[derive(Default)]
pub struct Textures {
    dir: Option<PathBuf>,
    loaded: HashMap<u32, Option<ffe_render::ColourMap>>,
    /// What could not be read, for the log.
    pub problems: Vec<String>,
}

impl Textures {
    /// Read the textures of the FFED3D installed at `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            dir: textures_dir(root),
            ..Self::default()
        }
    }

    /// Whether the install has a texture folder.
    pub fn found(&self) -> bool {
        self.dir.is_some()
    }

    /// `texN.png`, when the install has it.
    pub fn texture(&mut self, number: u32) -> Option<ffe_render::ColourMap> {
        if let Some(known) = self.loaded.get(&number) {
            return known.clone();
        }
        let map = self.dir.as_ref().and_then(|dir| {
            let path = find(dir, &format!("tex{number}.png"))?;
            let bytes = std::fs::read(&path)
                .map_err(|e| self.problems.push(format!("{}: {e}", path.display())))
                .ok()?;
            match decode_png(&bytes) {
                Ok((width, height, rgba)) if width > 0 && height > 0 => {
                    Some(ffe_render::ColourMap {
                        id: 2 << 40 | u64::from(number),
                        width,
                        height,
                        rgba: rgba.into(),
                    })
                }
                Ok(_) => None,
                Err(error) => {
                    self.problems.push(format!("{}: {error}", path.display()));
                    None
                }
            }
        });
        self.loaded.insert(number, map.clone());
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_number_from_400() {
        assert_eq!(texture_number(0), 400);
        assert_eq!(texture_number(147), 547);
    }

    #[test]
    fn files_are_found_in_any_case_and_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let textures = dir.path().join("Textures");
        std::fs::create_dir_all(&textures).unwrap();
        // A 2x1 RGB PNG: red, then blue.
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
        }
        std::fs::write(textures.join("TEX412.png"), &bytes).unwrap();
        let mut store = Textures::new(dir.path());
        assert!(store.found());
        let map = store.texture(412).expect("found in another case");
        assert_eq!((map.width, map.height), (2, 1));
        assert_eq!(&map.rgba[..], &[255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(store.texture(413).is_none());
        assert!(store.problems.is_empty());
    }
}
