//! The CD 1.01 videos, loaded from the `FMVs` folder beside the data root.
//!
//! The launcher (or `ffe-tools export-media`) copies the CD's `DATA\*.AVI` and
//! `*.BMP` into `FMVs` with a `manifest.json` of names, sizes and SHA-256
//! digests. The library is offered only when the game's content was built from
//! the CD 1.01 executable, so a floppy installation never picks up another
//! release's videos. Each clip is checked against the manifest's size before
//! [`ffe_convert::avi::decode_avi`] decodes it.
//!
//! The station screens choose the clip ([`ffe_core::fmv_select`], JJFFE's
//! F333/F382/F384/F442), and [`Player`] shows it as F120 does: the 120 x 120
//! picture at (1, 17) of the 320 x 200 screen at the clip's own rate (15
//! frames a second), its sound beside the effects, until it ends or any key or
//! click ends it early (F120 hands that input on to the screen).

use ffe_convert::avi::{decode_avi, Video, MAX_AVI_BYTES};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The source profile whose installation has videos.
pub const CD_PROFILE: &str = "ffe-cd-1.01-en";
/// The folder name beside the data root.
pub const FOLDER: &str = "FMVs";
/// Where F120 draws the picture (`FUNC_extravideo`'s x 1, y 0x11).
pub const ORIGIN: (usize, usize) = (1, 0x11);
/// The largest manifest read.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    files: Vec<ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    path: String,
    size: u64,
}

/// The CD's videos: clip name (upper case, no extension) to file and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    dir: PathBuf,
    clips: BTreeMap<String, (PathBuf, u64)>,
}

impl Library {
    /// The library for content built from `profile`, if that is the CD release
    /// and `root/FMVs/manifest.json` lists videos that are all present with
    /// their recorded sizes. `Ok(None)`: no videos apply (not the CD, or no
    /// folder). `Err`: the folder is there but does not match its manifest.
    pub fn find(root: &Path, profile: &str) -> Result<Option<Library>, String> {
        if profile != CD_PROFILE {
            return Ok(None);
        }
        let dir = root.join(FOLDER);
        let manifest = dir.join("manifest.json");
        if !manifest.is_file() {
            return Ok(None);
        }
        let size = std::fs::metadata(&manifest)
            .map_err(|error| format!("{}: {error}", manifest.display()))?
            .len();
        if size > MAX_MANIFEST_BYTES {
            return Err(format!("{} is too large", manifest.display()));
        }
        let text = std::fs::read_to_string(&manifest)
            .map_err(|error| format!("{}: {error}", manifest.display()))?;
        let parsed: Manifest = serde_json::from_str(&text)
            .map_err(|error| format!("{}: {error}", manifest.display()))?;
        let mut clips = BTreeMap::new();
        for file in parsed.files {
            let name = Path::new(&file.path);
            let is_video = name
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("avi"));
            if !is_video {
                continue;
            }
            // Manifest entries are bare file names; anything else is refused.
            if name.components().count() != 1 {
                return Err(format!(
                    "{}: {} is not a file name",
                    manifest.display(),
                    file.path
                ));
            }
            let path = dir.join(name);
            let actual = std::fs::metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .len();
            if actual != file.size {
                return Err(format!(
                    "{} is {actual} bytes, not the {} the manifest records",
                    path.display(),
                    file.size
                ));
            }
            let stem = name
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| format!("{}: unreadable name", file.path))?
                .to_ascii_uppercase();
            clips.insert(stem, (path, file.size));
        }
        if clips.is_empty() {
            return Ok(None);
        }
        Ok(Some(Library { dir, clips }))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Whether clip `name` (`BB1`, `ST12`, case-insensitive) is present.
    pub fn contains(&self, name: &str) -> bool {
        self.clips.contains_key(&name.to_ascii_uppercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.clips.keys().map(String::as_str)
    }

    /// Read and decode clip `name`.
    pub fn load(&self, name: &str) -> Result<Video, String> {
        let (path, size) = self
            .clips
            .get(&name.to_ascii_uppercase())
            .ok_or_else(|| format!("no video {name} in {}", self.dir.display()))?;
        if *size > MAX_AVI_BYTES as u64 {
            return Err(format!(
                "{} is larger than the decoder accepts",
                path.display()
            ));
        }
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if bytes.len() as u64 != *size {
            return Err(format!(
                "{} changed size since it was listed",
                path.display()
            ));
        }
        decode_avi(&bytes).map_err(|error| format!("{}: {error}", path.display()))
    }
}

/// A clip being shown, advanced by the game loop's real time.
#[derive(Debug, Clone)]
pub struct Player {
    name: String,
    video: Video,
    seconds: f64,
    /// The palette as the screen's `0x00RRGGBB` words.
    colours: Vec<u32>,
}

impl Player {
    pub fn new(name: &str, video: Video) -> Self {
        let colours = video
            .palette
            .iter()
            .map(|[r, g, b]| u32::from(*r) << 16 | u32::from(*g) << 8 | u32::from(*b))
            .collect();
        Player {
            name: name.to_owned(),
            video,
            seconds: 0.0,
            colours,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The clip's sound as 16-bit mono samples at its rate, if it has any at
    /// the mixer's 22,050 Hz.
    pub fn sound(&self) -> Option<std::sync::Arc<Vec<i16>>> {
        let audio = self.video.audio.as_ref()?;
        if audio.channels != 1 || audio.bits != 16 || audio.rate != 22_050 {
            return None;
        }
        Some(std::sync::Arc::new(
            audio
                .pcm
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect(),
        ))
    }

    /// Move on by `seconds` of real time. False once the last frame's time
    /// has passed.
    pub fn advance(&mut self, seconds: f64) -> bool {
        self.seconds += seconds.max(0.0);
        !self.finished()
    }

    pub fn finished(&self) -> bool {
        self.seconds >= self.video.seconds()
    }

    /// The frame to show now.
    pub fn frame_index(&self) -> usize {
        let per_frame = f64::from(self.video.frame_micros) / 1_000_000.0;
        ((self.seconds / per_frame) as usize).min(self.video.frames.len() - 1)
    }

    /// Draw the current frame into a `width`-wide screen of `0x00RRGGBB`
    /// words with its top-left at [`ORIGIN`], clipped to the screen.
    pub fn draw(&self, screen: &mut [u32], width: usize) {
        let frame = &self.video.frames[self.frame_index()];
        let (w, h) = (self.video.width as usize, self.video.height as usize);
        let height = screen.len() / width.max(1);
        for row in 0..h {
            let y = ORIGIN.1 + row;
            if y >= height {
                break;
            }
            for column in 0..w.min(width.saturating_sub(ORIGIN.0)) {
                let index = frame[row * w + column];
                screen[y * width + ORIGIN.0 + column] = self.colours[usize::from(index)];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(files: &[(&str, &[u8])], manifest: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(FOLDER);
        std::fs::create_dir(&dir).unwrap();
        for (name, bytes) in files {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        root
    }

    #[test]
    fn only_the_cd_profile_with_a_matching_folder_has_videos() {
        let root = folder(
            &[("BB1.AVI", b"abcd"), ("BB1.BMP", b"xy")],
            r#"{"format":"ffe-cd-media","version":1,"files":[
                {"path":"BB1.AVI","size":4,"sha256":""},
                {"path":"BB1.BMP","size":2,"sha256":""}]}"#,
        );
        let library = Library::find(root.path(), CD_PROFILE).unwrap().unwrap();
        assert_eq!(library.len(), 1);
        assert!(library.contains("bb1"));
        // Not a decodable clip: reported, not played.
        assert!(library.load("BB1").is_err());
        assert!(library.load("ST1").is_err());
        // The floppy release never takes the CD's videos.
        assert_eq!(
            Library::find(root.path(), "ffe-floppy-1.02-en").unwrap(),
            None
        );
        // No folder: nothing to play.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(Library::find(empty.path(), CD_PROFILE).unwrap(), None);
    }

    #[test]
    fn a_folder_that_differs_from_its_manifest_is_an_error() {
        let root = folder(
            &[("BB1.AVI", b"abc")],
            r#"{"files":[{"path":"BB1.AVI","size":4,"sha256":""}]}"#,
        );
        assert!(Library::find(root.path(), CD_PROFILE).is_err());
        let escape = folder(
            &[],
            r#"{"files":[{"path":"../X.AVI","size":1,"sha256":""}]}"#,
        );
        assert!(Library::find(escape.path(), CD_PROFILE).is_err());
    }

    fn clip(frames: usize) -> Video {
        Video {
            width: 2,
            height: 2,
            frame_micros: 66_666,
            palette: (0..=255u8).map(|i| [i, 0, 255 - i]).collect(),
            frames: (0..frames).map(|f| vec![f as u8; 4]).collect(),
            audio: Some(ffe_convert::avi::Audio {
                channels: 1,
                rate: 22_050,
                bits: 16,
                pcm: vec![1, 0, 0xff, 0xff],
            }),
        }
    }

    /// The frame follows the real time at 15 a second and the clip ends after
    /// its last frame; the picture lands at (1, 17) in the screen's colours.
    #[test]
    fn the_player_shows_frames_at_the_clips_rate() {
        let mut player = Player::new("BB1", clip(3));
        assert_eq!(player.frame_index(), 0);
        assert!(player.advance(0.07));
        assert_eq!(player.frame_index(), 1);
        assert!(player.advance(0.07));
        assert_eq!(player.frame_index(), 2);
        assert!(!player.advance(0.07), "three frames are 0.2 s");
        assert_eq!(player.sound().unwrap().as_slice(), [1, -1]);
        let mut screen = vec![0u32; 320 * 200];
        player.draw(&mut screen, 320);
        let at = |x: usize, y: usize| screen[y * 320 + x];
        assert_eq!(at(1, 17), 2 << 16 | 253);
        assert_eq!(at(2, 18), 2 << 16 | 253);
        assert_eq!((at(0, 17), at(3, 17), at(1, 16), at(1, 19)), (0, 0, 0, 0));
    }
}
