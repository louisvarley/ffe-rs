//! HMP (HMI MIDI) parsing, following the original `hmp.cpp` converter used by
//! JJFFE/FFED3D.
//!
//! The parser deliberately keeps the original's unusual delta encoding and its
//! fixed playback grid: division 192 and a tempo of `0x18_8000` microseconds per
//! quarter note. `HmpSong::to_smf` reproduces the Standard MIDI File that
//! `hmp.cpp` writes (a type-1 file with a synthesized conductor track).
//!
//! See `docs/notes/sound.md` for the routine references and the format notes.

use std::fmt;

/// The first eight bytes of every HMP file (`HMIMIDIP`).
pub const HMP_SIGNATURE: &[u8; 8] = b"HMIMIDIP";
/// The header byte that holds the number of tracks, including the conductor.
pub const HMP_TRACK_COUNT_OFFSET: usize = 0x30;
/// The header dword that the original format calls the song duration in seconds.
pub const HMP_ADVERTISED_DURATION_OFFSET: usize = 0x3c;
/// The division `hmp.cpp` writes (`0xC0`).
pub const HMP_DIVISION: u16 = 0xc0;
/// The tempo `hmp.cpp` writes: `0x18_8000` microseconds per quarter note.
pub const HMP_TEMPO_US_PER_QUARTER: u32 = 0x18_8000;

/// One event in a parsed HMP track. `tick` is absolute, not a delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HmpEvent {
    pub tick: u64,
    pub kind: HmpEventKind,
}

/// A channel message or a meta event. HMP does not use running status in the
/// input: every channel event carries its status byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HmpEventKind {
    /// A MIDI channel message. `data2` is `None` for program change (`0xC0`) and
    /// channel pressure (`0xD0`).
    Midi {
        status: u8,
        data1: u8,
        data2: Option<u8>,
    },
    /// A meta event (`0xFF`, type, length, payload). End of track is kept as a
    /// `Meta` event with kind `0x2f`; it terminates the track.
    Meta { kind: u8, data: Vec<u8> },
}

impl HmpEventKind {
    /// True for the end-of-track meta event.
    pub fn is_end_of_track(&self) -> bool {
        matches!(self, Self::Meta { kind: 0x2f, .. })
    }
}

/// One data track, without the synthesized conductor track.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HmpTrack {
    /// Events in file order; ticks are non-decreasing.
    pub events: Vec<HmpEvent>,
    /// The tick of the last event (normally the end-of-track marker).
    pub end_tick: u64,
}

impl HmpTrack {
    /// The channel events in file order.
    pub fn midi_events(&self) -> impl Iterator<Item = &HmpEvent> {
        self.events
            .iter()
            .filter(|event| matches!(event.kind, HmpEventKind::Midi { .. }))
    }
}

/// A parsed HMP song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HmpSong {
    /// The count byte at `0x30`, including the conductor track.
    pub header_track_count: u8,
    /// The value at `0x3c`, which the original format documents as duration in
    /// seconds. It is advisory: two of the shipped songs disagree with the event
    /// timeline (see `docs/notes/sound.md`).
    pub advertised_duration_seconds: u32,
    /// Fixed by `hmp.cpp`; HMP does not carry a usable division for the port.
    pub division: u16,
    /// Fixed by `hmp.cpp`.
    pub tempo_us_per_quarter: u32,
    /// The data tracks. `header_track_count == data_tracks.len() + 1` for every
    /// shipped file; the missing track is the conductor `hmp.cpp` synthesizes.
    pub tracks: Vec<HmpTrack>,
}

impl HmpSong {
    /// Parse an HMP image.
    pub fn parse(bytes: &[u8]) -> Result<Self, HmpError> {
        if bytes.len() < HMP_ADVERTISED_DURATION_OFFSET + 4 {
            return Err(HmpError::TooShort);
        }
        if &bytes[..8] != HMP_SIGNATURE {
            return Err(HmpError::BadSignature);
        }

        let header_track_count = bytes[HMP_TRACK_COUNT_OFFSET];
        if header_track_count == 0 {
            return Err(HmpError::NoTracks);
        }
        if header_track_count > 64 {
            return Err(HmpError::TooManyTracks(header_track_count));
        }
        let advertised_duration_seconds = u32::from_le_bytes(
            bytes[HMP_ADVERTISED_DURATION_OFFSET..][..4]
                .try_into()
                .unwrap(),
        );

        // hmp.cpp scans from 0x30 for the first 0xFF 0x2F and skips seven bytes:
        // the marker plus the four-byte track index leaves the chunk size at
        // `marker + 7`.
        let mut position = HMP_TRACK_COUNT_OFFSET;
        let marker = loop {
            if position + 1 >= bytes.len() {
                return Err(HmpError::MissingFirstTrack);
            }
            if bytes[position] == 0xff && bytes[position + 1] == 0x2f {
                break position;
            }
            position += 1;
        };
        let mut chunk_position = marker + 7;

        let mut tracks = Vec::with_capacity(usize::from(header_track_count) - 1);
        for track_number in 1..header_track_count {
            let header_end = chunk_position.checked_add(8).ok_or(HmpError::Truncated {
                track: track_number,
            })?;
            if header_end > bytes.len() {
                return Err(HmpError::Truncated {
                    track: track_number,
                });
            }
            let size = u32::from_le_bytes(
                bytes[chunk_position..chunk_position + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            if size < 12 {
                return Err(HmpError::BadChunkSize {
                    track: track_number,
                    size,
                });
            }
            let data_length = size - 12;
            let data_start = header_end;
            let data_end = data_start
                .checked_add(data_length)
                .ok_or(HmpError::Truncated {
                    track: track_number,
                })?;
            if data_end > bytes.len() {
                return Err(HmpError::Truncated {
                    track: track_number,
                });
            }
            tracks.push(parse_track(&bytes[data_start..data_end], track_number)?);
            chunk_position = chunk_position
                .checked_add(size)
                .ok_or(HmpError::Truncated {
                    track: track_number,
                })?;
        }

        Ok(Self {
            header_track_count,
            advertised_duration_seconds,
            division: HMP_DIVISION,
            tempo_us_per_quarter: HMP_TEMPO_US_PER_QUARTER,
            tracks,
        })
    }

    /// The number of data tracks (`header_track_count - 1`).
    pub fn data_track_count(&self) -> usize {
        self.tracks.len()
    }

    /// The largest event tick, including the end-of-track events.
    pub fn duration_ticks(&self) -> u64 {
        self.tracks
            .iter()
            .map(|track| track.end_tick)
            .max()
            .unwrap_or(0)
    }

    /// The event timeline in seconds, using the original's fixed tempo.
    pub fn duration_seconds(&self) -> f64 {
        self.duration_ticks() as f64 * f64::from(self.tempo_us_per_quarter)
            / (f64::from(self.division) * 1_000_000.0)
    }

    /// The total number of parsed events, including end-of-track markers.
    pub fn event_count(&self) -> usize {
        self.tracks.iter().map(|track| track.events.len()).sum()
    }

    /// Build the Standard MIDI File that `hmp.cpp` would produce.
    ///
    /// The output is a type-1 file: a conductor track with the fixed tempo and
    /// one track per HMP data track. Channel running status is suppressed when
    /// the status byte repeats, matching `hmp.cpp`.
    pub fn to_smf(&self) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(b"MThd");
        output.extend_from_slice(&6u32.to_be_bytes());
        output.extend_from_slice(&1u16.to_be_bytes());
        output.extend_from_slice(&(self.tracks.len() as u16 + 1).to_be_bytes());
        output.extend_from_slice(&self.division.to_be_bytes());

        // Conductor track: tempo, then end of track.
        let tempo = self.tempo_us_per_quarter.to_be_bytes();
        let conductor: [u8; 11] = [
            0x00, 0xff, 0x51, 0x03, tempo[1], tempo[2], tempo[3], 0x00, 0xff, 0x2f, 0x00,
        ];
        write_track(&mut output, &conductor);

        for track in &self.tracks {
            let mut data = Vec::new();
            let mut last_tick = 0u64;
            let mut last_status = 0u8;
            let mut wrote_end = false;
            for event in &track.events {
                let delta = event.tick.saturating_sub(last_tick);
                last_tick = event.tick;
                write_vlq(&mut data, delta as u32);
                match &event.kind {
                    HmpEventKind::Midi {
                        status,
                        data1,
                        data2,
                    } => {
                        if *status != last_status {
                            data.push(*status);
                            last_status = *status;
                        }
                        data.push(*data1);
                        if let Some(value) = data2 {
                            data.push(*value);
                        }
                    }
                    HmpEventKind::Meta {
                        kind,
                        data: payload,
                    } => {
                        data.extend_from_slice(&[0xff, *kind]);
                        write_vlq(&mut data, payload.len() as u32);
                        data.extend_from_slice(payload);
                        wrote_end = *kind == 0x2f;
                    }
                }
            }
            if !wrote_end {
                write_vlq(&mut data, 0);
                data.extend_from_slice(&[0xff, 0x2f, 0x00]);
            }
            write_track(&mut output, &data);
        }
        output
    }
}

fn write_track(output: &mut Vec<u8>, data: &[u8]) {
    output.extend_from_slice(b"MTrk");
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(data);
}

fn parse_track(data: &[u8], track_number: u8) -> Result<HmpTrack, HmpError> {
    let mut events = Vec::new();
    let mut position = 0usize;
    let mut tick = 0u64;
    while position < data.len() {
        let (delta, next) = read_hmp_vlq(data, position, track_number)?;
        position = next;
        tick = tick
            .checked_add(u64::from(delta))
            .ok_or(HmpError::TickOverflow {
                track: track_number,
            })?;

        if position >= data.len() {
            return Err(HmpError::Truncated {
                track: track_number,
            });
        }
        let status = data[position];
        position += 1;
        match status {
            0xff => {
                if position >= data.len() {
                    return Err(HmpError::Truncated {
                        track: track_number,
                    });
                }
                let kind = data[position];
                position += 1;
                let (length, next) = read_midi_vlq(data, position, track_number)?;
                position = next;
                let end = position
                    .checked_add(length as usize)
                    .ok_or(HmpError::Truncated {
                        track: track_number,
                    })?;
                if end > data.len() {
                    return Err(HmpError::Truncated {
                        track: track_number,
                    });
                }
                let payload = data[position..end].to_vec();
                position = end;
                events.push(HmpEvent {
                    tick,
                    kind: HmpEventKind::Meta {
                        kind,
                        data: payload,
                    },
                });
                if kind == 0x2f {
                    break;
                }
            }
            0x80..=0xef => {
                let data_length = match status & 0xf0 {
                    0xc0 | 0xd0 => 1,
                    _ => 2,
                };
                if position + data_length > data.len() {
                    return Err(HmpError::Truncated {
                        track: track_number,
                    });
                }
                let data1 = data[position];
                let data2 = (data_length == 2).then(|| data[position + 1]);
                position += data_length;
                events.push(HmpEvent {
                    tick,
                    kind: HmpEventKind::Midi {
                        status,
                        data1,
                        data2,
                    },
                });
            }
            0xf0..=0xf7 => {
                return Err(HmpError::UnsupportedSysEx {
                    track: track_number,
                })
            }
            other => {
                return Err(HmpError::BadStatus {
                    track: track_number,
                    status: other,
                })
            }
        }
    }
    let end_tick = events.last().map_or(0, |event| event.tick);
    Ok(HmpTrack { events, end_tick })
}

/// Read the HMP variable-length delta: groups are least-significant first and
/// the high bit marks the final byte. `hmp.cpp` allows at most four bytes.
fn read_hmp_vlq(data: &[u8], position: usize, track: u8) -> Result<(u32, usize), HmpError> {
    let mut value = 0u32;
    let mut shift = 0u32;
    let mut index = position;
    for _ in 0..4 {
        let byte = *data
            .get(index)
            .ok_or(HmpError::BadDelta { track, position })?;
        index += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 != 0 {
            return Ok((value, index));
        }
        shift += 7;
    }
    Err(HmpError::BadDelta { track, position })
}

/// Standard MIDI variable-length quantity, used only for meta lengths.
fn read_midi_vlq(data: &[u8], position: usize, track: u8) -> Result<(u32, usize), HmpError> {
    let mut value = 0u32;
    let mut index = position;
    for _ in 0..4 {
        let byte = *data.get(index).ok_or(HmpError::BadMetaLength { track })?;
        index += 1;
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok((value, index));
        }
    }
    Err(HmpError::BadMetaLength { track })
}

fn write_vlq(output: &mut Vec<u8>, value: u32) {
    let mut buffer = [0u8; 4];
    let mut index = 3;
    buffer[index] = (value & 0x7f) as u8;
    let mut rest = value >> 7;
    while rest != 0 {
        index -= 1;
        buffer[index] = ((rest & 0x7f) as u8) | 0x80;
        rest >>= 7;
    }
    output.extend_from_slice(&buffer[index..]);
}

/// Errors from parsing an HMP image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HmpError {
    TooShort,
    BadSignature,
    NoTracks,
    TooManyTracks(u8),
    MissingFirstTrack,
    Truncated { track: u8 },
    BadChunkSize { track: u8, size: usize },
    BadDelta { track: u8, position: usize },
    BadMetaLength { track: u8 },
    BadStatus { track: u8, status: u8 },
    UnsupportedSysEx { track: u8 },
    TickOverflow { track: u8 },
}

impl fmt::Display for HmpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => formatter.write_str("HMP image is shorter than its header"),
            Self::BadSignature => formatter.write_str("HMP image does not start with HMIMIDIP"),
            Self::NoTracks => formatter.write_str("HMP image declares no tracks"),
            Self::TooManyTracks(count) => write!(formatter, "HMP image declares {count} tracks"),
            Self::MissingFirstTrack => formatter.write_str("HMP first track marker is missing"),
            Self::Truncated { track } => write!(formatter, "HMP track {track} is truncated"),
            Self::BadChunkSize { track, size } => {
                write!(formatter, "HMP track {track} has chunk size {size}")
            }
            Self::BadDelta { track, position } => {
                write!(formatter, "HMP track {track} has a bad delta at {position}")
            }
            Self::BadMetaLength { track } => {
                write!(formatter, "HMP track {track} has a bad meta length")
            }
            Self::BadStatus { track, status } => {
                write!(formatter, "HMP track {track} has status {status:#04x}")
            }
            Self::UnsupportedSysEx { track } => {
                write!(
                    formatter,
                    "HMP track {track} contains a system-exclusive event"
                )
            }
            Self::TickOverflow { track } => {
                write!(formatter, "HMP track {track} tick count overflows")
            }
        }
    }
}

impl std::error::Error for HmpError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal 013195 HMP image with the supplied track data blocks.
    fn build_hmp(tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut image = vec![0u8; 0x40];
        image[..14].copy_from_slice(b"HMIMIDIP013195");
        image[HMP_TRACK_COUNT_OFFSET] = tracks.len() as u8 + 1;
        image[HMP_ADVERTISED_DURATION_OFFSET..HMP_ADVERTISED_DURATION_OFFSET + 4]
            .copy_from_slice(&123u32.to_le_bytes());
        for (index, data) in tracks.iter().enumerate() {
            image.extend_from_slice(&[0xff, 0x2f, 0x00]);
            image.extend_from_slice(&((index as u32) + 1).to_le_bytes());
            image.extend_from_slice(&((data.len() as u32) + 12).to_le_bytes());
            image.extend_from_slice(&[0, 0, 0, 0]);
            image.extend_from_slice(data);
            image.extend_from_slice(&[0, 0, 0, 0]);
        }
        image
    }

    fn hmp_delta(value: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut rest = value;
        loop {
            let group = (rest & 0x7f) as u8;
            rest >>= 7;
            if rest == 0 {
                bytes.push(group | 0x80);
                break;
            }
            bytes.push(group);
        }
        bytes
    }

    #[test]
    fn parses_delta_meta_and_channel_events() {
        let mut data = Vec::new();
        data.extend_from_slice(&hmp_delta(0));
        data.extend_from_slice(&[0x90, 60, 100]);
        data.extend_from_slice(&hmp_delta(200));
        data.extend_from_slice(&[0x80, 60, 0]);
        data.extend_from_slice(&hmp_delta(40));
        data.extend_from_slice(&[0xff, 0x2f, 0x00]);

        let image = build_hmp(&[data]);
        let song = HmpSong::parse(&image).unwrap();
        assert_eq!(song.header_track_count, 2);
        assert_eq!(song.data_track_count(), 1);
        assert_eq!(song.advertised_duration_seconds, 123);
        assert_eq!(song.division, HMP_DIVISION);
        assert_eq!(song.duration_ticks(), 240);
        assert_eq!(song.tracks[0].events.len(), 3);
        assert_eq!(
            song.tracks[0].events[0].kind,
            HmpEventKind::Midi {
                status: 0x90,
                data1: 60,
                data2: Some(100)
            }
        );
        assert_eq!(song.tracks[0].events[1].tick, 200);
        assert!(song.tracks[0].events[2].kind.is_end_of_track());
        assert!((song.duration_seconds() - 2.00704).abs() < 1e-9);
    }

    #[test]
    fn writes_a_type_one_smf_with_a_conductor_track() {
        let mut data = vec![
            0x80, 0x90, 60, 100, 0x80, 0x80, 60, 0, 0x80, 0xff, 0x2f, 0x00,
        ];
        let image = build_hmp(&[data.clone()]);
        let song = HmpSong::parse(&image).unwrap();
        let smf = song.to_smf();
        assert_eq!(&smf[..4], b"MThd");
        assert_eq!(u16::from_be_bytes([smf[10], smf[11]]), 2);
        assert_eq!(u16::from_be_bytes([smf[12], smf[13]]), HMP_DIVISION);
        assert_eq!(&smf[14..18], b"MTrk");
        // Conductor: tempo 0x18_8000 and end of track.
        assert!(smf
            .windows(4)
            .any(|window| window == [0xff, 0x51, 0x03, 0x18]));
        let reparsed = HmpSong::parse(&image).unwrap();
        assert_eq!(reparsed.tracks[0].events, song.tracks[0].events);

        // Channel running status is suppressed in the written track.
        data = smf[22..].to_vec();
        assert!(data.windows(3).any(|window| window == [0x90, 60, 100]));
    }

    #[test]
    fn rejects_bad_images() {
        assert_eq!(HmpSong::parse(&[0; 8]), Err(HmpError::TooShort));
        assert_eq!(HmpSong::parse(&[0u8; 0x40]), Err(HmpError::BadSignature));
        let mut image = build_hmp(&[vec![0x80, 0x90, 60, 100]]);
        image[0x30] = 0;
        assert_eq!(HmpSong::parse(&image), Err(HmpError::NoTracks));
        let mut image = build_hmp(&[vec![0x80, 0x90, 60, 100]]);
        image.truncate(0x48);
        assert!(matches!(
            HmpSong::parse(&image),
            Err(HmpError::Truncated { .. })
        ));
    }
}
