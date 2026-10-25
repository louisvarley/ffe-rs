//! Digital sound-effect mixing, the port of `w32sound.cpp`'s 16-channel
//! DirectSound path (`SoundPlaySample`, `LoadSample`).
//!
//! The original has no per-frame mixer: each effect is a one-shot DirectSound
//! buffer, and the driver mixes the duplicated channel buffers. This module
//! keeps the same model in software:
//!
//! - the 40-entry `pSoundSampleDescs` table (file name, loop count, pitch
//!   offset and fixed-voice marker),
//! - 16 voices chosen like `SoundPlaySample` (first free channel, reuse of the
//!   same sample, `0x10`'s volume/pitch-only update, and the laser1 steal),
//! - the loop count baked into the buffer (`LoadSample` copies the PCM N times),
//! - the original's centibel curve (`2217 * log10(level)`, rounded to 16 and
//!   offset by -10000).
//!
//! The cache stores every effect as canonical 16-bit mono 22,050 Hz WAV
//! (`base:effect/<name>`), so [`decode_wav`] is deliberately a small PCM reader
//! rather than a general audio container. No new dependency is needed.
//!
//! The mixer is pure and device-free: [`Mixer::mix_into`] renders 16-bit mono
//! samples, and the platform sink in `sink` feeds them to WinMM `waveOut*` (the
//! same system library the MIDI player uses) or to a test recorder.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// The original's fixed mixing rate (`waveformat`, `w32sound.cpp`).
pub const SAMPLE_RATE: u32 = 22_050;

/// `NUMCHANNELS` from `w32sound.cpp`.
pub const VOICE_COUNT: usize = 16;

/// The sample file names in `FX/`, lower-case as the cache stores them.
pub const EFFECT_NAMES: [&str; 23] = [
    "airopen", "bip", "bong", "boom", "camera", "chime", "clunkit", "damage", "ecm", "hype",
    "laser1", "laser2", "laser3", "laser4", "launch", "missexpl", "necm", "nowt", "roar",
    "shipexp", "siren", "station", "wind",
];

/// One `pSoundSampleDescs` entry. `loops` is the count `LoadSample` bakes into
/// the buffer (0 in the source means one copy). `pitch_mod` is added to the
/// requested frequency. `voice` is the original's fixed-voice marker (`-1` for
/// the generic allocator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleDesc {
    pub name: &'static str,
    pub loops: u8,
    pub pitch_mod: i32,
    pub voice: i8,
}

/// `pSoundSampleDescs` (40 entries) from `win32/w32sound.cpp`.
pub const SAMPLE_TABLE: [SampleDesc; 40] = [
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x00
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x04
    SampleDesc {
        name: "shipexp",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "missexpl",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x08
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "laser1",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "laser2",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "airopen",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x0c
    SampleDesc {
        name: "airopen",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "wind",
        loops: 1,
        pitch_mod: -0x4000,
        voice: 0,
    },
    SampleDesc {
        name: "station",
        loops: 50,
        pitch_mod: 0,
        voice: 1,
    },
    SampleDesc {
        name: "station",
        loops: 50,
        pitch_mod: 0x1000,
        voice: 2,
    }, // 0x10
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "airopen",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "airopen",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "bip",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x14
    SampleDesc {
        name: "bip",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "bong",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "chime",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x18
    SampleDesc {
        name: "ecm",
        loops: 2,
        pitch_mod: 0,
        voice: 3,
    },
    SampleDesc {
        name: "necm",
        loops: 2,
        pitch_mod: 0,
        voice: 4,
    },
    SampleDesc {
        name: "shipexp",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "missexpl",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x1c
    SampleDesc {
        name: "damage",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "siren",
        loops: 3,
        pitch_mod: 0,
        voice: 5,
    },
    SampleDesc {
        name: "boom",
        loops: 1,
        pitch_mod: 0,
        voice: 6,
    },
    SampleDesc {
        name: "launch",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x20
    SampleDesc {
        name: "hype",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "laser3",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "laser4",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    }, // 0x24
    SampleDesc {
        name: "wind",
        loops: 1,
        pitch_mod: -0x4000,
        voice: 7,
    },
    SampleDesc {
        name: "nowt",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
    SampleDesc {
        name: "camera",
        loops: 1,
        pitch_mod: 0,
        voice: -1,
    },
];

/// The descriptor for an original sample-table index.
pub fn sample_desc(code: u8) -> Option<SampleDesc> {
    SAMPLE_TABLE.get(usize::from(code)).copied()
}

/// The original's linear level -> DirectSound centibel conversion:
/// `(long)(2217 * log10(level << 9)) + 15 & -16 - 10000`.
///
/// `F1908` shifts the linear level left by nine and caps it at `0x7fff` before
/// calling the platform, so this is the same argument `SoundPlaySample` sees.
/// The result is in hundredths of a decibel relative to full scale (0 = full
/// volume, -10000 = silence).
pub fn level_centibels(level: u8) -> i32 {
    let scaled = (u32::from(level) * 512).min(0x7fff) as f64;
    if scaled <= 0.0 {
        return -10_000;
    }
    let raw = (2217.0 * scaled.log10()) as i32;
    ((raw + 15) & !15) - 10_000
}

/// The amplitude ratio for an original linear level (0..=63 for the game's
/// default range). This is the centibel curve above converted back to a
/// multiplier, clamped to full scale.
pub fn level_gain(level: u8) -> f64 {
    let centibels = level_centibels(level);
    if centibels <= -10_000 {
        return 0.0;
    }
    10f64.powf(f64::from(centibels) / 2000.0).min(1.0)
}

/// Errors from decoding a cache effect WAV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WavError {
    TooShort,
    NotRiff,
    NotWave,
    MissingFormat,
    MissingData,
    UnsupportedFormat(u16),
    UnsupportedChannels(u16),
    UnsupportedBits(u16),
    ZeroSampleRate,
    OddDataLength,
    Truncated(&'static str),
}

impl fmt::Display for WavError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(formatter, "WAV is shorter than a RIFF header"),
            Self::NotRiff => write!(formatter, "WAV does not start with RIFF"),
            Self::NotWave => write!(formatter, "RIFF form is not WAVE"),
            Self::MissingFormat => write!(formatter, "WAV has no fmt chunk"),
            Self::MissingData => write!(formatter, "WAV has no data chunk"),
            Self::UnsupportedFormat(tag) => write!(formatter, "WAV format {tag} is not PCM"),
            Self::UnsupportedChannels(count) => {
                write!(formatter, "WAV has {count} channels; the mixer is mono")
            }
            Self::UnsupportedBits(bits) => {
                write!(
                    formatter,
                    "WAV has {bits}-bit samples; the mixer needs 16-bit"
                )
            }
            Self::ZeroSampleRate => write!(formatter, "WAV sample rate is zero"),
            Self::OddDataLength => {
                write!(formatter, "16-bit WAV data has an odd byte length")
            }
            Self::Truncated(what) => write!(formatter, "WAV {what} is truncated"),
        }
    }
}

impl std::error::Error for WavError {}

/// Decoded 16-bit mono PCM from one effect WAV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSample {
    pub pcm: Vec<i16>,
    pub sample_rate: u32,
}

impl EffectSample {
    pub fn frames(&self) -> usize {
        self.pcm.len()
    }
}

/// Decode the canonical cache WAV (`pcm16_mono_22050_wav` output). Extra RIFF
/// chunks are skipped and the data chunk may be anywhere after `fmt `; the
/// decoder deliberately rejects non-PCM, non-mono and non-16-bit content rather
/// than guessing.
pub fn decode_wav(bytes: &[u8]) -> Result<EffectSample, WavError> {
    if bytes.len() < 12 {
        return Err(WavError::TooShort);
    }
    if &bytes[..4] != b"RIFF" {
        return Err(WavError::NotRiff);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWave);
    }

    let mut position = 12usize;
    let mut format = None;
    let mut data = None;
    while position + 8 <= bytes.len() {
        let id = &bytes[position..position + 4];
        let size = u32::from_le_bytes(
            bytes[position + 4..position + 8]
                .try_into()
                .expect("four-byte chunk length"),
        ) as usize;
        let start = position + 8;
        let end = start
            .checked_add(size)
            .ok_or(WavError::Truncated("chunk length"))?;
        if end > bytes.len() {
            return Err(WavError::Truncated("chunk"));
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(WavError::Truncated("fmt chunk"));
                }
                let header = &bytes[start..start + 16];
                let tag = u16::from_le_bytes([header[0], header[1]]);
                let channels = u16::from_le_bytes([header[2], header[3]]);
                let sample_rate = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
                let bits = u16::from_le_bytes([header[14], header[15]]);
                if tag != 1 {
                    return Err(WavError::UnsupportedFormat(tag));
                }
                if channels != 1 {
                    return Err(WavError::UnsupportedChannels(channels));
                }
                if bits != 16 {
                    return Err(WavError::UnsupportedBits(bits));
                }
                if sample_rate == 0 {
                    return Err(WavError::ZeroSampleRate);
                }
                format = Some(sample_rate);
            }
            b"data" => {
                if size % 2 != 0 {
                    return Err(WavError::OddDataLength);
                }
                let mut pcm = Vec::with_capacity(size / 2);
                for pair in bytes[start..end].chunks_exact(2) {
                    pcm.push(i16::from_le_bytes([pair[0], pair[1]]));
                }
                data = Some(pcm);
            }
            _ => {}
        }
        position = end + (size & 1);
    }

    let sample_rate = format.ok_or(WavError::MissingFormat)?;
    let pcm = data.ok_or(WavError::MissingData)?;
    Ok(EffectSample { pcm, sample_rate })
}

/// Decoded effect samples keyed by the lower-case `FX` file name.
#[derive(Debug, Default, Clone)]
pub struct EffectLibrary {
    samples: BTreeMap<String, Arc<EffectSample>>,
}

impl EffectLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one cache WAV. The name may be `bip`, `BIP`, `FX/bip.raw` or a cache
    /// path; it is normalised to the table's lower-case stem.
    pub fn insert_wav(&mut self, name: &str, bytes: &[u8]) -> Result<(), WavError> {
        let sample = decode_wav(bytes)?;
        self.samples.insert(normalise_name(name), Arc::new(sample));
        Ok(())
    }

    pub fn insert_sample(&mut self, name: &str, sample: EffectSample) {
        self.samples.insert(normalise_name(name), Arc::new(sample));
    }

    pub fn get(&self, name: &str) -> Option<Arc<EffectSample>> {
        self.samples.get(&normalise_name(name)).cloned()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

fn normalise_name(name: &str) -> String {
    let stem = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    stem.strip_suffix(".wav")
        .or_else(|| stem.strip_suffix(".raw"))
        .unwrap_or(&stem)
        .to_owned()
}

#[derive(Debug)]
struct Voice {
    sample: Arc<EffectSample>,
    code: u8,
    /// Fractional frame position, so the pitch/frequency scaling resamples.
    position: f64,
    /// Frames advanced per output frame (`frequency / sample_rate`).
    step: f64,
    /// Linear gain from [`level_gain`].
    gain: f64,
    /// How many more copies of the PCM to play (`LoadSample`'s baked loop).
    loops_remaining: u8,
}

/// The 16-voice software mixer.
#[derive(Debug)]
pub struct Mixer {
    effects: Arc<EffectLibrary>,
    voices: [Option<Voice>; VOICE_COUNT],
}

impl Mixer {
    pub fn new(effects: EffectLibrary) -> Self {
        Self {
            effects: Arc::new(effects),
            voices: std::array::from_fn(|_| None),
        }
    }

    pub fn set_effects(&mut self, effects: Arc<EffectLibrary>) {
        self.effects = effects;
    }

    pub fn effects(&self) -> &EffectLibrary {
        &self.effects
    }

    /// The port of `SoundPlaySample`: choose a voice, apply the original's
    /// same-sample rules and start (or update) it.
    ///
    /// `volume` is the linear level after `AudioSettings::effect_level`
    /// (`0..=63` for the game's calls); `pitch` is the DirectSound frequency,
    /// with `-1` meaning the 22,050 Hz default.
    pub fn play(&mut self, code: u8, volume: u8, pitch: i32) {
        let Some(desc) = sample_desc(code) else {
            return;
        };
        let Some(sample) = self.effects.get(desc.name) else {
            return;
        };
        if sample.pcm.is_empty() {
            return;
        }

        let gain = level_gain(volume);
        if gain <= 0.0 {
            return;
        }
        let step = frequency_step(desc.pitch_mod, pitch, sample.sample_rate);

        // `SoundPlaySample` first looks for a channel already playing this
        // sample. A finished channel is restarted in place; `0x10` only gets a
        // volume/pitch update; other fixed-voice samples ignore the repeat.
        if let Some(index) = self
            .voices
            .iter()
            .position(|voice| voice.as_ref().is_some_and(|voice| voice.code == code))
        {
            if code == 0x10 {
                let voice = self.voices[index]
                    .as_mut()
                    .expect("voice found by the position above");
                voice.gain = gain;
                voice.step = step;
                return;
            }
            if desc.voice >= 0 {
                return;
            }
        }

        let slot = if let Some(index) = self.voices.iter().position(Option::is_none) {
            index
        } else if let Some(index) = self
            .voices
            .iter()
            .position(|voice| voice.as_ref().is_some_and(|voice| voice.code == 0x0b))
        {
            // The original frees a laser1 channel rather than cutting a more
            // important sample; everything else is dropped.
            index
        } else {
            return;
        };

        self.voices[slot] = Some(Voice {
            sample,
            code,
            position: 0.0,
            step,
            gain,
            loops_remaining: desc.loops.saturating_sub(1),
        });
    }

    pub fn stop_all(&mut self) {
        self.voices = std::array::from_fn(|_| None);
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.is_some()).count()
    }

    /// The active voice sample codes, in channel order. Exposed for tests and
    /// the diagnostics pane.
    pub fn active_codes(&self) -> Vec<u8> {
        self.voices
            .iter()
            .filter_map(|voice| voice.as_ref().map(|voice| voice.code))
            .collect()
    }

    /// Render mono 16-bit samples. The caller chooses the block size; the
    /// device sink uses small blocks and the tests render exact lengths.
    pub fn mix_into(&mut self, output: &mut [i16]) {
        for slot in output {
            let mut sum = 0.0;
            for index in 0..self.voices.len() {
                let Some(voice) = self.voices[index].as_mut() else {
                    continue;
                };
                let length = voice.sample.pcm.len();
                if length == 0 {
                    self.voices[index] = None;
                    continue;
                }
                let position = voice.position;
                let frame = (position.floor() as usize).min(length - 1);
                let fraction = position - frame as f64;
                let current = f64::from(voice.sample.pcm[frame]);
                let next = if frame + 1 < length {
                    f64::from(voice.sample.pcm[frame + 1])
                } else if voice.loops_remaining > 0 {
                    f64::from(voice.sample.pcm[0])
                } else {
                    current
                };
                sum += (current + (next - current) * fraction) * voice.gain;

                voice.position += voice.step;
                while voice.position >= length as f64 {
                    if voice.loops_remaining == 0 {
                        self.voices[index] = None;
                        break;
                    }
                    voice.position -= length as f64;
                    voice.loops_remaining -= 1;
                }
            }
            *slot = sum.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        }
    }

    pub fn render(&mut self, frames: usize) -> Vec<i16> {
        let mut output = vec![0i16; frames];
        self.mix_into(&mut output);
        output
    }
}

fn frequency_step(pitch_mod: i32, pitch: i32, sample_rate: u32) -> f64 {
    // DirectSound clamps SetFrequency to DSBFREQUENCY_MIN..=MAX.
    const DSBFREQUENCY_MIN: i64 = 100;
    const DSBFREQUENCY_MAX: i64 = 200_000;
    let requested = if pitch < 0 {
        i64::from(SAMPLE_RATE)
    } else {
        i64::from(pitch)
    };
    let frequency = (requested + i64::from(pitch_mod)).clamp(DSBFREQUENCY_MIN, DSBFREQUENCY_MAX);
    frequency as f64 / f64::from(sample_rate.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(pcm: &[i16], sample_rate: u32) -> Vec<u8> {
        let data_length = (pcm.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + pcm.len() * 2);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_length).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_length.to_le_bytes());
        for sample in pcm {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    fn library(name: &str, pcm: &[i16]) -> EffectLibrary {
        let mut library = EffectLibrary::new();
        library
            .insert_wav(name, &wav(pcm, SAMPLE_RATE))
            .expect("test WAV decodes");
        library
    }

    fn full_library() -> EffectLibrary {
        let mut library = EffectLibrary::new();
        for name in EFFECT_NAMES {
            library.insert_sample(
                name,
                EffectSample {
                    pcm: vec![1_000; 4],
                    sample_rate: SAMPLE_RATE,
                },
            );
        }
        library
    }

    #[test]
    fn decodes_canonical_pcm16_mono_wav() {
        let bytes = wav(&[1, -2, 32_767, -32_768], SAMPLE_RATE);
        let decoded = decode_wav(&bytes).expect("decode");
        assert_eq!(decoded.sample_rate, SAMPLE_RATE);
        assert_eq!(decoded.pcm, vec![1, -2, 32_767, -32_768]);
    }

    #[test]
    fn rejects_non_pcm_and_truncated_wavs() {
        assert_eq!(decode_wav(b"short"), Err(WavError::TooShort));
        assert_eq!(decode_wav(b"RIFX0000WAVE"), Err(WavError::NotRiff));

        let mut bytes = wav(&[1], SAMPLE_RATE);
        bytes[20] = 2; // fmt tag = 2 (ADPCM)
        assert_eq!(decode_wav(&bytes), Err(WavError::UnsupportedFormat(2)));

        let mut bytes = wav(&[1], SAMPLE_RATE);
        bytes[22] = 2; // channels = 2
        assert_eq!(decode_wav(&bytes), Err(WavError::UnsupportedChannels(2)));

        let mut bytes = wav(&[1], SAMPLE_RATE);
        bytes[34] = 8; // bits = 8
        assert_eq!(decode_wav(&bytes), Err(WavError::UnsupportedBits(8)));

        let mut bytes = wav(&[1, 2], SAMPLE_RATE);
        bytes.pop();
        assert_eq!(decode_wav(&bytes), Err(WavError::Truncated("chunk")));
    }

    #[test]
    fn level_curve_matches_the_original_centibel_rounding() {
        assert_eq!(level_centibels(0), -10_000);
        assert_eq!(level_gain(0), 0.0);
        // 63 << 9 = 32256 -> 9995 raw, rounded up to 10000 -> 0 centibels.
        assert_eq!(level_centibels(63), 0);
        assert!((level_gain(63) - 1.0).abs() < 1e-12);
        // 31 << 9 = 15872 -> 9312 raw, already a multiple of 16 -> -688.
        assert_eq!(level_centibels(31), -688);
        assert!(level_gain(31) < level_gain(63));
        assert!(level_gain(15) < level_gain(31));
        assert!(level_gain(1) > 0.0);
    }

    #[test]
    fn mixes_real_pcm_and_stops_at_the_end() {
        let mut mixer = Mixer::new(library("bip", &[1_000, 2_000, 3_000, 4_000]));
        mixer.play(0x14, 63, -1);
        let rendered = mixer.render(8);
        assert_eq!(
            rendered[..4],
            [1_000, 2_000, 3_000, 4_000],
            "one-shot PCM is mixed verbatim at the default level"
        );
        assert!(rendered[4..].iter().all(|sample| *sample == 0));
        assert_eq!(mixer.active_voices(), 0);
    }

    #[test]
    fn loop_count_is_baked_into_the_voice() {
        let mut mixer = Mixer::new(library("ecm", &[100, 200]));
        mixer.play(0x19, 63, -1); // ecm loops twice
        let rendered = mixer.render(6);
        assert_eq!(rendered, [100, 200, 100, 200, 0, 0]);
    }

    #[test]
    fn same_sample_fixed_voice_updates_without_restarting() {
        let mut mixer = Mixer::new(library("station", &[100, 200, 300, 400]));
        mixer.play(0x0f, 63, -1);
        assert_eq!(mixer.render(2), [100, 200]);
        // F1908 repeats 0x0f while it plays: the fixed-voice sample ignores it.
        mixer.play(0x0f, 63, -1);
        assert_eq!(mixer.render(2), [300, 400]);

        // 0x10 is the exception: it updates volume/pitch and keeps the position.
        mixer.stop_all();
        mixer.play(0x10, 63, -1);
        assert_eq!(mixer.render(1), [100]);
        mixer.play(0x10, 31, -1);
        assert!(mixer.active_voices() == 1);
        let next = mixer.render(1);
        // 0x10 also has a +0x1000 pitch offset, so the position is between the
        // second and third frames after one rendered frame.
        let step = f64::from(SAMPLE_RATE + 0x1000) / f64::from(SAMPLE_RATE);
        let frame = step.floor();
        let expected = (200.0 + (300.0 - 200.0) * (step - frame)) * level_gain(31);
        assert!(
            (f64::from(next[0]) - expected).abs() < 2.0,
            "0x10 continues at the new gain: {next:?}, expected {expected}"
        );
    }

    #[test]
    fn full_channels_steal_only_laser1() {
        let mut mixer = Mixer::new(full_library());
        // 0x05, 0x06, 0x0a, 0x0c, 0x0d, 0x12, 0x13, 0x14, 0x15, 0x16,
        // 0x17, 0x1b, 0x1c, 0x1d, 0x20 and 0x21 are generic (no fixed voice).
        for code in [
            0x05, 0x06, 0x0a, 0x0c, 0x0d, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x1b, 0x1c, 0x1d,
            0x20, 0x21,
        ] {
            mixer.play(code, 63, -1);
        }
        assert_eq!(mixer.active_voices(), VOICE_COUNT);
        // No laser1 to steal: the source drops the new sample.
        mixer.play(0x14, 63, -1);
        assert_eq!(mixer.active_voices(), VOICE_COUNT);
        assert_eq!(
            mixer
                .active_codes()
                .iter()
                .filter(|code| **code == 0x14)
                .count(),
            1
        );

        // With a laser1 voice, a new sample takes its channel.
        let mut mixer = Mixer::new(full_library());
        for code in [
            0x05, 0x06, 0x0b, 0x0c, 0x0d, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x1b, 0x1c, 0x1d,
            0x20, 0x21,
        ] {
            mixer.play(code, 63, -1);
        }
        assert_eq!(mixer.active_voices(), VOICE_COUNT);
        mixer.play(0x14, 63, -1);
        assert_eq!(mixer.active_voices(), VOICE_COUNT);
        assert!(!mixer.active_codes().contains(&0x0b));
        assert_eq!(
            mixer
                .active_codes()
                .iter()
                .filter(|code| **code == 0x14)
                .count(),
            2
        );
    }

    #[test]
    fn overlapping_voices_are_summed() {
        let mut mixer = Mixer::new(library("bip", &[1_000, 2_000]));
        mixer.play(0x14, 63, -1);
        mixer.play(0x15, 63, -1);
        assert_eq!(mixer.active_voices(), 2);
        assert_eq!(mixer.render(2), [2_000, 4_000]);
    }

    #[test]
    fn empty_nowt_is_a_silent_no_op() {
        let mut mixer = Mixer::new(library("nowt", &[]));
        mixer.play(0x00, 63, -1);
        assert_eq!(mixer.active_voices(), 0);
        assert_eq!(mixer.render(4), [0, 0, 0, 0]);
    }
}
