use crate::settings::{scale_volume, AudioSettings};
use crate::sink::MidiSink;
use ffe_core::hmp::{HmpEventKind, HmpSong};
use std::time::Duration;

/// Whether a finished song stops or starts again. The original game plays a song
/// once and the caller decides what happens next (`F232`/`F919`), so `Once` is
/// the default. `Loop` is provided for the front-end and for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    #[default]
    Once,
    Loop,
}

#[derive(Debug)]
struct PlayingSong {
    index: usize,
    song: HmpSong,
    positions: Vec<usize>,
    elapsed_us: u128,
    finished: bool,
}

impl PlayingSong {
    fn new(index: usize, song: HmpSong) -> Self {
        let positions = vec![0; song.tracks.len()];
        Self {
            index,
            song,
            positions,
            elapsed_us: 0,
            finished: false,
        }
    }

    fn tick_due(&self, tick: u64) -> bool {
        self.elapsed_us * u128::from(self.song.division)
            >= u128::from(tick) * u128::from(self.song.tempo_us_per_quarter)
    }

    fn all_tracks_finished(&self) -> bool {
        self.song
            .tracks
            .iter()
            .zip(&self.positions)
            .all(|(track, position)| *position >= track.events.len())
    }
}

/// A single-song MIDI player driven by the caller's clock.
///
/// `tick` must be called with the elapsed wall time. This keeps the player
/// independent of threads and makes it deterministic in tests.
#[derive(Debug, Default)]
pub struct MusicPlayer {
    playing: Option<PlayingSong>,
    loop_mode: LoopMode,
}

impl MusicPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn loop_mode(&self) -> LoopMode {
        self.loop_mode
    }

    pub fn set_loop_mode(&mut self, mode: LoopMode) {
        self.loop_mode = mode;
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    pub fn current_song(&self) -> Option<usize> {
        self.playing.as_ref().map(|playing| playing.index)
    }

    /// True when there is no song, matching `F1904`: no current song means done.
    pub fn song_done(&self) -> bool {
        self.playing.is_none()
    }

    /// The playing song's duration in seconds, if any.
    pub fn duration_seconds(&self) -> Option<f64> {
        self.playing
            .as_ref()
            .map(|playing| playing.song.duration_seconds())
    }

    /// Start a song from tick zero. Any previous song is stopped first.
    pub fn play(
        &mut self,
        song: HmpSong,
        index: usize,
        sink: &mut dyn MidiSink,
        settings: AudioSettings,
    ) {
        if !settings.music_on {
            self.stop(sink);
            return;
        }
        self.stop(sink);
        let mut playing = PlayingSong::new(index, song);
        setup_channels(sink, settings);
        let finished = pump(&mut playing, sink, settings);
        if finished && self.loop_mode == LoopMode::Once {
            all_notes_off(sink);
            return;
        }
        if finished && playing.song.duration_ticks() == 0 {
            all_notes_off(sink);
            return;
        }
        self.playing = Some(playing);
    }

    /// Stop the current song and silence every channel.
    pub fn stop(&mut self, sink: &mut dyn MidiSink) {
        if self.playing.take().is_some() {
            all_notes_off(sink);
            sink.reset();
        }
    }

    /// Advance the clock and send every due event. Returns true when the song is
    /// done (there is no song).
    pub fn tick(
        &mut self,
        elapsed: Duration,
        sink: &mut dyn MidiSink,
        settings: AudioSettings,
    ) -> bool {
        let Some(playing) = self.playing.as_mut() else {
            return true;
        };
        if !settings.music_on {
            self.playing = None;
            all_notes_off(sink);
            sink.reset();
            return true;
        }
        playing.elapsed_us = playing.elapsed_us.saturating_add(elapsed.as_micros());
        let finished = pump(playing, sink, settings);
        if !finished {
            return false;
        }
        if self.loop_mode == LoopMode::Loop && playing.song.duration_ticks() > 0 {
            all_notes_off(sink);
            playing.positions.fill(0);
            playing.elapsed_us = 0;
            playing.finished = false;
            setup_channels(sink, settings);
            if pump(playing, sink, settings) {
                // A song whose whole content is at tick zero cannot loop.
                self.playing = None;
                all_notes_off(sink);
                sink.reset();
                return true;
            }
            return false;
        }
        self.playing = None;
        all_notes_off(sink);
        sink.reset();
        true
    }

    /// Re-apply the channel volume without restarting the song.
    pub fn set_settings(&mut self, settings: AudioSettings, sink: &mut dyn MidiSink) {
        if !settings.music_on {
            self.stop(sink);
            return;
        }
        if self.playing.is_some() {
            for channel in 0..16u8 {
                sink.send(&[0xb0 | channel, 7, settings.music_midi_volume()]);
            }
        }
    }
}

fn setup_channels(sink: &mut dyn MidiSink, settings: AudioSettings) {
    for channel in 0..16u8 {
        sink.send(&[0xb0 | channel, 123, 0]);
        sink.send(&[0xb0 | channel, 7, settings.music_midi_volume()]);
    }
}

fn all_notes_off(sink: &mut dyn MidiSink) {
    for channel in 0..16u8 {
        sink.send(&[0xb0 | channel, 123, 0]);
        sink.send(&[0xb0 | channel, 64, 0]);
    }
}

/// Send all events whose tick has arrived. Returns true when every track has
/// reached its end.
fn pump(playing: &mut PlayingSong, sink: &mut dyn MidiSink, settings: AudioSettings) -> bool {
    let mut any = false;
    for (track_index, track) in playing.song.tracks.iter().enumerate() {
        while let Some(event) = track.events.get(playing.positions[track_index]) {
            if !playing.tick_due(event.tick) {
                break;
            }
            any = true;
            match &event.kind {
                HmpEventKind::Midi {
                    status,
                    data1,
                    data2,
                } => send_midi(sink, *status, *data1, *data2, settings),
                HmpEventKind::Meta { .. } => {}
            }
            playing.positions[track_index] += 1;
        }
    }
    if any {
        playing.finished = playing.all_tracks_finished();
    }
    playing.finished || playing.all_tracks_finished()
}

fn send_midi(
    sink: &mut dyn MidiSink,
    status: u8,
    data1: u8,
    data2: Option<u8>,
    settings: AudioSettings,
) {
    match status & 0xf0 {
        0x80 | 0x90 | 0xa0 | 0xe0 => {
            let data2 = data2.unwrap_or(0);
            sink.send(&[status, data1, data2]);
        }
        0xb0 => {
            let data2 = data2.unwrap_or(0);
            // Make the user's music volume win over the song's channel volume.
            let value = if data1 == 7 {
                scale_volume(data2, settings.music_volume)
            } else {
                data2
            };
            sink.send(&[status, data1, value]);
        }
        0xc0 | 0xd0 => sink.send(&[status, data1]),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::RecordingMidiSink;
    use ffe_core::hmp::{HmpEvent, HmpEventKind, HmpTrack};

    fn song() -> HmpSong {
        HmpSong {
            header_track_count: 2,
            advertised_duration_seconds: 2,
            division: 192,
            tempo_us_per_quarter: 1_605_632,
            tracks: vec![HmpTrack {
                events: vec![
                    HmpEvent {
                        tick: 0,
                        kind: HmpEventKind::Midi {
                            status: 0x90,
                            data1: 60,
                            data2: Some(100),
                        },
                    },
                    HmpEvent {
                        tick: 96,
                        kind: HmpEventKind::Midi {
                            status: 0x80,
                            data1: 60,
                            data2: Some(0),
                        },
                    },
                    HmpEvent {
                        tick: 192,
                        kind: HmpEventKind::Meta {
                            kind: 0x2f,
                            data: Vec::new(),
                        },
                    },
                ],
                end_tick: 192,
            }],
        }
    }

    #[test]
    fn sends_volume_note_on_and_note_off() {
        let mut sink = RecordingMidiSink::new();
        let mut player = MusicPlayer::new();
        let settings = AudioSettings {
            music_volume: 50,
            ..AudioSettings::default()
        };
        player.play(song(), 0, &mut sink, settings);
        assert!(sink.contains(&[0xb0, 7, 63]));
        assert!(sink.contains(&[0x90, 60, 100]));
        assert!(!sink.contains(&[0x80, 60, 0]));
        assert!(!player.song_done());

        // 96 ticks at 1,605,632 us/quarter and 192 ticks/quarter is 802,816 us.
        player.tick(Duration::from_millis(900), &mut sink, settings);
        assert!(sink.contains(&[0x80, 60, 0]));
        assert!(!player.song_done());

        player.tick(Duration::from_millis(800), &mut sink, settings);
        assert!(player.song_done());
        assert!(sink.contains(&[0xb0, 123, 0]));
    }

    #[test]
    fn loop_mode_restarts_and_stop_silences() {
        let mut sink = RecordingMidiSink::new();
        let mut player = MusicPlayer::new();
        player.set_loop_mode(LoopMode::Loop);
        player.play(song(), 3, &mut sink, AudioSettings::default());
        let note_on = [0x90, 60, 100];
        assert_eq!(sink.count(&note_on), 1);

        // The song ends at 192 ticks (1,605,632 us); go past it.
        player.tick(
            Duration::from_millis(1_700),
            &mut sink,
            AudioSettings::default(),
        );
        assert_eq!(sink.count(&note_on), 2);
        assert_eq!(player.current_song(), Some(3));
        assert!(!player.song_done());

        player.stop(&mut sink);
        assert!(player.song_done());
        assert!(sink.contains(&[0xb0, 123, 0]));
        assert!(sink.contains(&[0xff, 0xff]));
    }

    #[test]
    fn volume_changes_do_not_restart() {
        let mut sink = RecordingMidiSink::new();
        let mut player = MusicPlayer::new();
        let settings = AudioSettings::default();
        player.play(song(), 0, &mut sink, settings);
        let before = sink.messages().len();
        player.set_settings(
            AudioSettings {
                music_volume: 10,
                ..settings
            },
            &mut sink,
        );
        assert!(sink.contains(&[0xb0, 7, 12]));
        assert!(sink.messages().len() > before);
        assert_eq!(sink.count(&[0x90, 60, 100]), 1);
    }
}
