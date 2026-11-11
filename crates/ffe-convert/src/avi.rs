//! The CD 1.01 videos (`DATA\*.AVI`): RIFF AVI with one uncompressed 8-bit
//! DIB video stream (120 x 120 in every supplied clip, 15 frames a second, a
//! 256-colour palette in its format block) and one PCM audio stream (22,050 Hz,
//! 16-bit mono), interleaved in `rec ` lists. Only that form is accepted:
//! compressed video, other depths or audio formats are reported, not guessed.
//! Every chunk is bounds-checked; a RIFF size that overstates or understates
//! the file (some clips carry eight bytes past it) is tolerated by walking the
//! bytes that are there.

/// The largest clip the decoder accepts (the supplied ones are under 4 MiB).
pub const MAX_AVI_BYTES: usize = 32 * 1024 * 1024;
/// The largest frame side accepted.
pub const MAX_SIDE: u32 = 640;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    /// The PCM bytes of every audio chunk, in file order.
    pub pcm: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    pub width: u32,
    pub height: u32,
    /// Microseconds per frame (`avih`).
    pub frame_micros: u32,
    /// The DIB palette, red/green/blue.
    pub palette: Vec<[u8; 3]>,
    /// Each frame's colour indices, top row first, `width * height` bytes.
    pub frames: Vec<Vec<u8>>,
    pub audio: Option<Audio>,
}

impl Video {
    /// The clip's length in seconds from its frame count and rate.
    pub fn seconds(&self) -> f64 {
        self.frames.len() as f64 * f64::from(self.frame_micros) / 1_000_000.0
    }
}

struct Chunk<'a> {
    id: [u8; 4],
    data: &'a [u8],
}

fn chunks(mut bytes: &[u8]) -> impl Iterator<Item = Chunk<'_>> {
    std::iter::from_fn(move || {
        if bytes.len() < 8 {
            return None;
        }
        let id: [u8; 4] = bytes[0..4].try_into().ok()?;
        let size = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
        let end = 8usize.checked_add(size)?.min(bytes.len());
        let data = &bytes[8..end];
        let next = (8 + size + (size & 1)).min(bytes.len());
        bytes = &bytes[next..];
        Some(Chunk { id, data })
    })
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| "AVI header is truncated".to_owned())
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| "AVI header is truncated".to_owned())
}

/// Decode one clip.
pub fn decode_avi(bytes: &[u8]) -> Result<Video, String> {
    if bytes.len() > MAX_AVI_BYTES {
        return Err("AVI file is larger than the decoder accepts".to_owned());
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"AVI " {
        return Err("not a RIFF AVI file".to_owned());
    }
    let body = &bytes[12..];
    let mut frame_micros = 0;
    let mut video: Option<(u32, u32, Vec<[u8; 3]>)> = None;
    let mut audio: Option<Audio> = None;
    let mut frames = Vec::new();
    let mut movi_seen = false;
    for chunk in chunks(body) {
        if &chunk.id != b"LIST" || chunk.data.len() < 4 {
            continue;
        }
        match &chunk.data[0..4] {
            b"hdrl" => {
                for header in chunks(&chunk.data[4..]) {
                    match &header.id {
                        b"avih" => frame_micros = u32_at(header.data, 0)?,
                        b"LIST" if header.data.get(0..4) == Some(b"strl") => {
                            stream(&header.data[4..], &mut video, &mut audio)?
                        }
                        _ => {}
                    }
                }
            }
            b"movi" => {
                movi_seen = true;
                let (width, height, _) = video
                    .as_ref()
                    .ok_or("AVI has no video stream before its data")?;
                let frame_bytes = (*width * *height) as usize;
                let mut take = |data: &[u8], id: &[u8; 4]| -> Result<(), String> {
                    match &id[2..4] {
                        b"db" | b"dc" if &id[0..2] == b"00" => {
                            if data.len() < frame_bytes {
                                return Err("AVI video frame is truncated".to_owned());
                            }
                            // Bottom-up DIB rows to top-down.
                            let mut frame = Vec::with_capacity(frame_bytes);
                            for row in (0..*height as usize).rev() {
                                let start = row * *width as usize;
                                frame.extend_from_slice(&data[start..start + *width as usize]);
                            }
                            frames.push(frame);
                        }
                        b"wb" => {
                            if let Some(audio) = audio.as_mut() {
                                audio.pcm.extend_from_slice(data);
                            }
                        }
                        _ => {}
                    }
                    Ok(())
                };
                for item in chunks(&chunk.data[4..]) {
                    if &item.id == b"LIST" && item.data.get(0..4) == Some(b"rec ") {
                        for inner in chunks(&item.data[4..]) {
                            take(inner.data, &inner.id)?;
                        }
                    } else {
                        take(item.data, &item.id)?;
                    }
                }
            }
            _ => {}
        }
    }
    let (width, height, palette) = video.ok_or("AVI has no video stream")?;
    if !movi_seen || frames.is_empty() {
        return Err("AVI has no video frames".to_owned());
    }
    if frame_micros == 0 {
        return Err("AVI has no frame rate".to_owned());
    }
    Ok(Video {
        width,
        height,
        frame_micros,
        palette,
        frames,
        audio,
    })
}

type VideoFormat = Option<(u32, u32, Vec<[u8; 3]>)>;

fn stream(bytes: &[u8], video: &mut VideoFormat, audio: &mut Option<Audio>) -> Result<(), String> {
    let mut kind = None;
    for chunk in chunks(bytes) {
        match &chunk.id {
            b"strh" => kind = chunk.data.get(0..4).map(|k| [k[0], k[1], k[2], k[3]]),
            b"strf" if kind.as_ref() == Some(b"vids") => {
                let data = chunk.data;
                let header = u32_at(data, 0)? as usize;
                let width = u32_at(data, 4)?;
                let height = u32_at(data, 8)? as i32;
                let bits = u16_at(data, 14)?;
                let compression = u32_at(data, 16)?;
                if compression != 0 || bits != 8 {
                    return Err(format!(
                        "AVI video is not uncompressed 8-bit DIB ({bits} bits, compression {compression:#x})"
                    ));
                }
                if height <= 0 {
                    return Err("AVI video is not a bottom-up DIB".to_owned());
                }
                let height = height as u32;
                if width == 0 || width > MAX_SIDE || height > MAX_SIDE {
                    return Err(format!("AVI video size {width}x{height} is out of range"));
                }
                let colours = match u32_at(data, 32)? {
                    0 => 256,
                    used => used.min(256) as usize,
                };
                let table = data
                    .get(header..header + 4 * colours)
                    .ok_or("AVI palette is truncated")?;
                let mut palette: Vec<[u8; 3]> = table
                    .chunks_exact(4)
                    .map(|bgr| [bgr[2], bgr[1], bgr[0]])
                    .collect();
                palette.resize(256, [0; 3]);
                *video = Some((width, height, palette));
            }
            b"strf" if kind.as_ref() == Some(b"auds") => {
                let format = u16_at(chunk.data, 0)?;
                let channels = u16_at(chunk.data, 2)?;
                let rate = u32_at(chunk.data, 4)?;
                let bits = u16_at(chunk.data, 14)?;
                if format != 1 || channels == 0 || !matches!(bits, 8 | 16) {
                    return Err(format!(
                        "AVI audio is not PCM (format {format}, {channels} channels, {bits} bits)"
                    ));
                }
                *audio = Some(Audio {
                    channels,
                    rate,
                    bits,
                    pcm: Vec::new(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8; 4], items: &[Vec<u8>]) -> Vec<u8> {
        let mut data = kind.to_vec();
        for item in items {
            data.extend_from_slice(item);
        }
        chunk(b"LIST", &data)
    }

    /// A 2x2 clip with two frames and two audio chunks, as the CD's are laid out.
    fn clip(bits: u16) -> Vec<u8> {
        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&66_666u32.to_le_bytes());
        let mut vids = b"vidsDIB ".to_vec();
        vids.resize(56, 0);
        let mut bih = vec![0u8; 40];
        bih[0..4].copy_from_slice(&40u32.to_le_bytes());
        bih[4..8].copy_from_slice(&2u32.to_le_bytes());
        bih[8..12].copy_from_slice(&2u32.to_le_bytes());
        bih[12..14].copy_from_slice(&1u16.to_le_bytes());
        bih[14..16].copy_from_slice(&bits.to_le_bytes());
        for colour in 0..256u32 {
            bih.extend_from_slice(&[colour as u8, 0, 255 - colour as u8, 0]);
        }
        let mut auds = b"auds".to_vec();
        auds.resize(56, 0);
        let wave = [1, 0, 1, 0, 0x22, 0x56, 0, 0, 0x44, 0xac, 0, 0, 2, 0, 16, 0];
        let hdrl = list(
            b"hdrl",
            &[
                chunk(b"avih", &avih),
                list(b"strl", &[chunk(b"strh", &vids), chunk(b"strf", &bih)]),
                list(b"strl", &[chunk(b"strh", &auds), chunk(b"strf", &wave)]),
            ],
        );
        // Bottom-up rows: the file's first row is the picture's last.
        let movi = list(
            b"movi",
            &[
                list(b"rec ", &[chunk(b"01wb", &[1, 2])]),
                list(
                    b"rec ",
                    &[chunk(b"00db", &[3, 4, 1, 2]), chunk(b"01wb", &[3, 4])],
                ),
                list(b"rec ", &[chunk(b"00db", &[7, 8, 5, 6])]),
            ],
        );
        let mut body = b"AVI ".to_vec();
        body.extend_from_slice(&hdrl);
        body.extend_from_slice(&movi);
        chunk(b"RIFF", &body)
    }

    #[test]
    fn decodes_frames_top_down_with_palette_and_audio() {
        let video = decode_avi(&clip(8)).unwrap();
        assert_eq!(
            (video.width, video.height, video.frame_micros),
            (2, 2, 66_666)
        );
        assert_eq!(video.frames, [vec![1, 2, 3, 4], vec![5, 6, 7, 8]]);
        // BGR quads to RGB.
        assert_eq!(video.palette[1], [255 - 1, 0, 1]);
        let audio = video.audio.unwrap();
        assert_eq!((audio.channels, audio.rate, audio.bits), (1, 22_050, 16));
        assert_eq!(audio.pcm, [1, 2, 3, 4]);
    }

    #[test]
    fn rejects_other_forms_and_truncation() {
        assert!(decode_avi(&clip(24)).unwrap_err().contains("8-bit"));
        assert!(decode_avi(b"RIFF\0\0\0\0WAVE").is_err());
        let whole = clip(8);
        // Cutting the file inside the last frame drops its chunk length short.
        assert!(decode_avi(&whole[..whole.len() - 3]).is_err());
    }
}
