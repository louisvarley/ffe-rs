use std::sync::{Arc, Mutex};

/// A destination for short MIDI messages (up to three bytes).
///
/// The player never needs long messages: the original HMP files contain only
/// channel and meta events. A backend that cannot open a device may be replaced
/// by [`NullMidiSink`].
pub trait MidiSink: Send {
    fn send(&mut self, message: &[u8]);
    /// Stop all notes and reset the device (called when a song stops).
    fn reset(&mut self);
}

/// A destination for digital sound effects.
pub trait SampleSink: Send {
    fn play_sample(&mut self, code: u8, volume: u8, pitch: i32);
    fn stop_all(&mut self);
}

/// A recorded effect call, for tests and for the diagnostics pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleCall {
    pub code: u8,
    pub volume: u8,
    pub pitch: i32,
}

/// Discards everything. This is the CI/headless backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullMidiSink;

impl MidiSink for NullMidiSink {
    fn send(&mut self, _message: &[u8]) {}
    fn reset(&mut self) {}
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullSampleSink;

impl SampleSink for NullSampleSink {
    fn play_sample(&mut self, _code: u8, _volume: u8, _pitch: i32) {}
    fn stop_all(&mut self) {}
}

/// Keeps every MIDI message in order. Cheap to clone: clones share the log.
#[derive(Debug, Default, Clone)]
pub struct RecordingMidiSink {
    messages: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl RecordingMidiSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn messages(&self) -> Vec<Vec<u8>> {
        lock(&self.messages).clone()
    }

    pub fn clear(&self) {
        lock(&self.messages).clear();
    }

    /// True when a three-byte channel message with this status and data was sent.
    pub fn contains(&self, message: &[u8]) -> bool {
        lock(&self.messages)
            .iter()
            .any(|recorded| recorded.as_slice() == message)
    }

    pub fn count(&self, message: &[u8]) -> usize {
        lock(&self.messages)
            .iter()
            .filter(|recorded| recorded.as_slice() == message)
            .count()
    }
}

impl MidiSink for RecordingMidiSink {
    fn send(&mut self, message: &[u8]) {
        lock(&self.messages).push(message.to_vec());
    }

    fn reset(&mut self) {
        lock(&self.messages).push(vec![0xff, 0xff]);
    }
}

/// Keeps every effect call in order. Cheap to clone: clones share the log.
#[derive(Debug, Default, Clone)]
pub struct RecordingSampleSink {
    calls: Arc<Mutex<Vec<SampleCall>>>,
}

impl RecordingSampleSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<SampleCall> {
        lock(&self.calls).clone()
    }

    pub fn clear(&self) {
        lock(&self.calls).clear();
    }
}

impl SampleSink for RecordingSampleSink {
    fn play_sample(&mut self, code: u8, volume: u8, pitch: i32) {
        lock(&self.calls).push(SampleCall {
            code,
            volume,
            pitch,
        });
    }

    fn stop_all(&mut self) {}
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// WinMM system-MIDI output, the closest dependency-free equivalent of the
/// original's Win32 MIDI path. It sends short messages to the system synth (the
/// Microsoft GS Wavetable Synth on a default Windows install).
#[cfg(target_os = "windows")]
pub struct WindowsMidiSink {
    handle: *mut std::ffi::c_void,
}

#[cfg(target_os = "windows")]
unsafe impl Send for WindowsMidiSink {}

#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn midiOutOpen(
        handle: *mut *mut std::ffi::c_void,
        device: u32,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn midiOutClose(handle: *mut std::ffi::c_void) -> u32;
    fn midiOutShortMsg(handle: *mut std::ffi::c_void, message: u32) -> u32;
    fn midiOutReset(handle: *mut std::ffi::c_void) -> u32;
}

#[cfg(target_os = "windows")]
impl WindowsMidiSink {
    /// `device = None` uses `MIDI_MAPPER`, as MCI/DirectMusic would by default.
    pub fn open(device: Option<u32>) -> Result<Self, String> {
        let mut handle = std::ptr::null_mut();
        let device = device.unwrap_or(u32::MAX);
        // SAFETY: WinMM writes one handle; the callback and instance are null.
        let result = unsafe { midiOutOpen(&mut handle, device, 0, 0, 0) };
        if result != 0 {
            return Err(format!("midiOutOpen failed with MMSYSERR {result}"));
        }
        Ok(Self { handle })
    }
}

#[cfg(target_os = "windows")]
impl MidiSink for WindowsMidiSink {
    fn send(&mut self, message: &[u8]) {
        if message.is_empty() || message.len() > 3 {
            return;
        }
        let mut packed = 0u32;
        for (index, byte) in message.iter().enumerate() {
            packed |= u32::from(*byte) << (8 * index);
        }
        // SAFETY: the handle is valid until Drop, and the message is a short one.
        unsafe {
            midiOutShortMsg(self.handle, packed);
        }
    }

    fn reset(&mut self) {
        // SAFETY: the handle is valid until Drop.
        unsafe {
            midiOutReset(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsMidiSink {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by WindowsMidiSink::open and is closed once.
        unsafe {
            midiOutClose(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl std::fmt::Debug for WindowsMidiSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowsMidiSink")
            .field("handle", &self.handle)
            .finish()
    }
}
