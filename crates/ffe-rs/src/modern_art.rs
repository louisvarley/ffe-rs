//! Upscaled model and planet textures for the modern engine's GPU 3-D view, read from the
//! owner's image export (`img-export/`) and read again whenever a file changes.
//!
//! The original colours a texture per face: its bitmap holds colour indices, and each face looks
//! them up in its own slot colours (lighting, haze, material). The export writes every bitmap
//! twice: `indexed/<category>/<name>.png` with the original indices and one texture record's
//! colours as its palette, and `images/<category>/<name>.png` in those colours, which the owner
//! replaces with an upscaled version. An upscaled picture cannot simply be shown, because it has
//! only that one record's colours, so it is turned back into an index map: each of its pixels
//! takes the index whose palette colour is nearest, among the indices the original has round the
//! same place (so two slots that share a colour in that record stay apart, and the upscaler cannot
//! bring in a colour the bitmap never had there), and keeps how much brighter or darker than that
//! colour it was as a detail (`ffe_render::IndexMap`). Every face then colours the enlarged map
//! with its own slots, as the original does.
//!
//! A picture still at the export's size is the original, so it is left out (the original map is
//! used, enlarged by Scale2x). See `docs/notes/modern-graphics.md`.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// The export's categories that hold textures, searched in this order.
const CATEGORIES: [&str; 4] = ["texture", "planet_texture", "sprite", "texture_block"];
/// How often the files are checked for changes.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// The largest upscaled side accepted.
const MAX_SIDE: u32 = 4096;

/// A bitmap as the GPU view uses it: its id (`& 0xfff`), its stride and rows (at least the
/// stride) and its original indices, row after row; masked bitmaps skip index 0.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBitmap {
    pub bitmap: u32,
    pub width: u32,
    pub height: u32,
    pub indices: std::sync::Arc<[u8]>,
    pub masked: bool,
}

/// The export's file name for a bitmap id (the cache's primary table, then the secondary).
pub fn file_name(bitmap: u32) -> String {
    let id = bitmap & 0xfff;
    if id < 96 {
        format!("primary-{id:03}")
    } else {
        format!("secondary-{:03}", id - 96)
    }
}

struct Entry {
    /// The upscaled picture and its indexed key, when both exist.
    paths: Option<(PathBuf, PathBuf)>,
    /// Their modification times when last read.
    stamp: Option<(SystemTime, SystemTime)>,
    map: Option<ffe_render::IndexMap>,
}

/// The upscaled maps read so far, by bitmap id.
#[derive(Default)]
pub struct Art {
    dir: Option<PathBuf>,
    entries: HashMap<u32, Entry>,
    checked: Option<Instant>,
    next: u64,
    /// What went wrong with a file, once per change (for the log).
    pub problems: Vec<String>,
}

impl Art {
    /// Read upscaled textures from `dir` (the export's root, holding `images/` and `indexed/`).
    pub fn set_dir(&mut self, dir: PathBuf) {
        self.dir = Some(dir);
        self.entries.clear();
    }

    /// The upscaled map for a bitmap, if the export has one bigger than the original. Changed
    /// files are read again (checked once a second).
    pub fn map(&mut self, bitmap: &WorldBitmap) -> Option<ffe_render::IndexMap> {
        let dir = self.dir.clone()?;
        let now = Instant::now();
        if self
            .checked
            .is_none_or(|at| now.duration_since(at) >= CHECK_EVERY)
        {
            self.checked = Some(now);
            // Forget the entries whose files changed (or appeared), so they are read again.
            self.entries.retain(|id, entry| {
                let paths = find(&dir, *id);
                paths == entry.paths && stamps(paths.as_ref()) == entry.stamp
            });
        }
        if !self.entries.contains_key(&bitmap.bitmap) {
            let paths = find(&dir, bitmap.bitmap);
            let stamp = stamps(paths.as_ref());
            let map = match &paths {
                Some((picture, key)) => {
                    self.next += 1;
                    let id = 1 << 48 | self.next;
                    match load(picture, key, bitmap, id) {
                        Ok(map) => map,
                        Err(error) => {
                            self.problems
                                .push(format!("Modern graphics: {}: {error}", picture.display()));
                            None
                        }
                    }
                }
                None => None,
            };
            self.entries
                .insert(bitmap.bitmap, Entry { paths, stamp, map });
        }
        self.entries.get(&bitmap.bitmap)?.map.clone()
    }
}

/// The upscaled picture and its indexed key for a bitmap, in the first category that has both.
fn find(dir: &Path, bitmap: u32) -> Option<(PathBuf, PathBuf)> {
    let name = format!("{}.png", file_name(bitmap));
    CATEGORIES.iter().find_map(|category| {
        let picture = dir.join("images").join(category).join(&name);
        let key = dir.join("indexed").join(category).join(&name);
        (picture.is_file() && key.is_file()).then_some((picture, key))
    })
}

fn stamps(paths: Option<&(PathBuf, PathBuf)>) -> Option<(SystemTime, SystemTime)> {
    let (picture, key) = paths?;
    let time = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Some((time(picture)?, time(key)?))
}

/// The export's indexed key: the original indices, their size and the record's colours.
struct Key {
    indices: Vec<u8>,
    width: u32,
    height: u32,
    palette: Vec<[u8; 3]>,
}

/// An 8-bit indexed PNG.
fn read_indexed(bytes: &[u8]) -> Result<Key, String> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("indexed PNG: {e}"))?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err("the indexed key is not an 8-bit palette PNG".to_owned());
    }
    let palette: Vec<[u8; 3]> = info
        .palette
        .as_ref()
        .ok_or("the indexed key has no palette")?
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let mut indices = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut indices)
        .map_err(|e| format!("indexed PNG pixels: {e}"))?;
    indices.truncate(frame.buffer_size());
    if indices.len() != (width * height) as usize {
        return Err("the indexed key's rows are padded".to_owned());
    }
    Ok(Key {
        indices,
        width,
        height,
        palette,
    })
}

/// Any 8- or 16-bit PNG as RGBA.
fn read_rgba(bytes: &[u8]) -> Result<(Vec<[u8; 4]>, u32, u32), String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("PNG: {e}"))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "{width}x{height} is outside 1..={MAX_SIDE} per side"
        ));
    }
    let mut data = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut data)
        .map_err(|e| format!("PNG pixels: {e}"))?;
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("the palette did not expand".to_owned()),
    };
    let pixels = data[..frame.buffer_size()]
        .chunks_exact(channels)
        .map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            2 => [p[0], p[0], p[0], p[1]],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect();
    Ok((pixels, width, height))
}

/// The index map for an upscaled `picture` with its indexed `key`; `None` when the picture is
/// still at the key's size (the original).
fn load(
    picture: &Path,
    key: &Path,
    bitmap: &WorldBitmap,
    id: u64,
) -> Result<Option<ffe_render::IndexMap>, String> {
    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let key = read_indexed(&read(key)?)?;
    let (pixels, width, height) = read_rgba(&read(picture)?)?;
    if (width, height) == (key.width, key.height) {
        return Ok(None);
    }
    Ok(Some(index_map(
        &key.indices,
        (key.width, key.height),
        &key.palette,
        &pixels,
        (width, height),
        bitmap,
        id,
    )))
}

/// The luminance of a colour (Rec. 601 weights).
fn luma([r, g, b]: [u8; 3]) -> f32 {
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

/// Turn an upscaled picture back into indices with a brightness detail (see the module notes).
/// The map covers the bitmap's whole stride and rows: when the bitmap has more rows than the
/// export (the corner table reaches past short bitmaps), the extra rows are left transparent.
pub fn index_map(
    indices: &[u8],
    (key_width, key_height): (u32, u32),
    palette: &[[u8; 3]],
    pixels: &[[u8; 4]],
    (width, height): (u32, u32),
    bitmap: &WorldBitmap,
    id: u64,
) -> ffe_render::IndexMap {
    let rows = if key_height < bitmap.height && key_height > 0 {
        height * bitmap.height / key_height
    } else {
        height
    };
    let colour = |index: u8| palette.get(usize::from(index)).copied().unwrap_or([0; 3]);
    let mut texels = vec![128u16 << 8; (width * rows) as usize];
    for y in 0..height {
        let ky = (y * key_height / height).min(key_height - 1);
        for x in 0..width {
            let kx = (x * key_width / width).min(key_width - 1);
            let [r, g, b, a] = pixels[(y * width + x) as usize];
            let index = if bitmap.masked && a < 128 {
                0
            } else {
                // The nearest colour among the indices round the original texel; on a tie the
                // texel's own index (so slots sharing a colour keep their places).
                let distance = |candidate: u8| {
                    let [pr, pg, pb] = colour(candidate);
                    [(r, pr), (g, pg), (b, pb)]
                        .iter()
                        .map(|&(c, p)| (i32::from(c) - i32::from(p)).pow(2) as u32)
                        .sum::<u32>()
                };
                let own = indices[(ky * key_width + kx) as usize];
                let mut best = if bitmap.masked && own == 0 {
                    (u32::MAX, own)
                } else {
                    (distance(own), own)
                };
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (kx as i32 + dx, ky as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= key_width as i32 || ny >= key_height as i32 {
                            continue;
                        }
                        let candidate = indices[(ny as u32 * key_width + nx as u32) as usize];
                        if bitmap.masked && candidate == 0 {
                            continue;
                        }
                        let distance = distance(candidate);
                        if distance < best.0 {
                            best = (distance, candidate);
                        }
                    }
                }
                best.1
            };
            let base = luma(colour(index));
            let detail = if base > 8.0 && !(bitmap.masked && index == 0) {
                (luma([r, g, b]) / base).clamp(0.5, 1.99)
            } else {
                1.0
            };
            texels[(y * width + x) as usize] =
                u16::from(index) | ((detail * 128.0).round().min(255.0) as u16) << 8;
        }
    }
    ffe_render::IndexMap {
        id,
        width,
        height: rows,
        texels: texels.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(masked: bool) -> WorldBitmap {
        WorldBitmap {
            bitmap: 7,
            width: 2,
            height: 2,
            indices: vec![1, 2, 2, 1].into(),
            masked,
        }
    }

    #[test]
    fn names_follow_the_cache_tables() {
        assert_eq!(file_name(0x40), "primary-064");
        assert_eq!(file_name(0x121), "secondary-193");
        assert_eq!(file_name(0x5123), "secondary-195");
    }

    #[test]
    fn an_upscaled_picture_becomes_indices_and_detail() {
        // Two slots that share a colour in the preview record (1 and 3 are both green) stay
        // apart: each pixel takes the index the original has round its place.
        let palette = [[0, 0, 0], [0, 200, 0], [200, 0, 0], [0, 200, 0]];
        let indices = [1u8, 2, 2, 3];
        // 4x4: the top-left quarter is a darker green, the bottom-right a brighter one.
        let mut pixels = vec![[200u8, 0, 0, 255]; 16];
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            pixels[y * 4 + x] = [0, 100, 0, 255];
        }
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            pixels[y * 4 + x] = [0, 250, 0, 255];
        }
        let map = index_map(
            &indices,
            (2, 2),
            &palette,
            &pixels,
            (4, 4),
            &bitmap(false),
            9,
        );
        assert_eq!((map.width, map.height, map.id), (4, 4, 9));
        let at = |x: usize, y: usize| map.texels[y * 4 + x];
        assert_eq!(at(0, 0) & 0xff, 1);
        assert_eq!(at(3, 3) & 0xff, 3);
        assert_eq!(at(3, 0) & 0xff, 2);
        // Half as bright as slot 1's green, a quarter brighter than slot 3's, red as it is.
        assert_eq!(at(0, 0) >> 8, 64);
        assert_eq!(at(3, 3) >> 8, 160);
        assert_eq!(at(3, 0) >> 8, 128);
    }

    #[test]
    fn a_masked_bitmap_keeps_its_holes_and_short_bitmaps_are_padded() {
        let palette = [[0, 0, 0], [0, 200, 0], [200, 0, 0]];
        let pixels = vec![
            [0, 200, 0, 0],
            [0, 200, 0, 255],
            [200, 0, 0, 255],
            [0, 0, 0, 0],
        ];
        let mut short = bitmap(true);
        short.height = 4;
        let map = index_map(&[1, 2], (2, 1), &palette, &pixels, (2, 2), &short, 1);
        // The export had one row of a four-row bitmap: the picture's two rows are a quarter.
        assert_eq!((map.width, map.height), (2, 8));
        assert_eq!(map.texels[0] & 0xff, 0, "transparent stays index 0");
        // Green over the red texel takes the green index beside it, and red over green the red.
        assert_eq!(map.texels[1] & 0xff, 1);
        assert_eq!(map.texels[2] & 0xff, 2);
        assert!(map.texels[4..].iter().all(|t| t & 0xff == 0));
    }

    #[test]
    fn files_are_found_read_and_read_again_when_they_change() {
        let dir = tempfile::tempdir().unwrap();
        let write_indexed = |path: &Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 2, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![0, 0, 0, 0, 200, 0, 200, 0, 0]);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 2, 1]).unwrap();
        };
        let write_picture = |path: &Path, side: u32, rgb: [u8; 3]| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), side, side);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let data: Vec<u8> = (0..side * side).flat_map(|_| rgb).collect();
            writer.write_image_data(&data).unwrap();
        };
        let key = dir.path().join("indexed/texture/primary-007.png");
        let picture = dir.path().join("images/texture/primary-007.png");
        write_indexed(&key);
        write_picture(&picture, 2, [0, 200, 0]);
        let mut art = Art::default();
        art.set_dir(dir.path().to_owned());
        // Still the export's size: the original serves.
        assert!(art.map(&bitmap(false)).is_none());
        write_picture(&picture, 8, [0, 200, 0]);
        // Changed: read again at the next check.
        art.checked = None;
        let map = art.map(&bitmap(false)).expect("the upscaled map");
        assert_eq!((map.width, map.height), (8, 8));
        assert!(art.problems.is_empty());
    }
}
