//! WinMM `waveOut*` output for the effect mixer.
//!
//! This is the same system library the MIDI player uses (`winmm.dll`); it adds
//! no dependency and is the closest dependency-free equivalent of the original's
//! DirectSound destination. A small worker thread renders 256-frame blocks
//! (11.6 ms at 22,050 Hz) through four prepared buffers, so the game thread only
//! locks the mixer to start/stop voices.
//!
//! The device is opened on the worker thread. `spawn` does not return until the
//! device is open, so a machine without an output device falls back cleanly to
//! the headless mixer in `sink`.

use crate::mixer::Mixer;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};

const WAVE_MAPPER: u32 = u32::MAX;
const WAVE_FORMAT_PCM: u16 = 1;
const CALLBACK_EVENT: u32 = 0x0005_0000;
const WHDR_DONE: u32 = 1;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 0x102;

const BUFFER_FRAMES: usize = 256;
const BUFFER_COUNT: usize = 4;

type WaveOutHandle = *mut c_void;

#[repr(C)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_second: u32,
    average_bytes_per_second: u32,
    block_align: u16,
    bits_per_sample: u16,
    extra_size: u16,
}

#[repr(C)]
struct WaveHdr {
    data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

#[link(name = "winmm")]
extern "system" {
    fn waveOutOpen(
        device: *mut WaveOutHandle,
        device_id: u32,
        format: *const WaveFormatEx,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn waveOutClose(device: WaveOutHandle) -> u32;
    fn waveOutPrepareHeader(device: WaveOutHandle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutUnprepareHeader(device: WaveOutHandle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutWrite(device: WaveOutHandle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutReset(device: WaveOutHandle) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

fn lock(mixer: &Mutex<Mixer>) -> std::sync::MutexGuard<'_, Mixer> {
    mixer.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Open the default wave output and start the mixing thread.
pub fn spawn(mixer: Arc<Mutex<Mixer>>) -> Result<(Arc<AtomicBool>, JoinHandle<()>), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("ffe-waveout".to_owned())
        .spawn(move || run(mixer, thread_stop, ready_tx))
        .map_err(|error| format!("cannot start the wave output thread: {error}"))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok((stop, handle)),
        Ok(Err(error)) => {
            let _ = handle.join();
            Err(error)
        }
        Err(_) => {
            let _ = handle.join();
            Err("wave output thread stopped before opening the device".to_owned())
        }
    }
}

fn run(mixer: Arc<Mutex<Mixer>>, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    let format = WaveFormatEx {
        format_tag: WAVE_FORMAT_PCM,
        channels: 1,
        samples_per_second: crate::mixer::SAMPLE_RATE,
        average_bytes_per_second: crate::mixer::SAMPLE_RATE * 2,
        block_align: 2,
        bits_per_sample: 16,
        extra_size: 0,
    };

    // SAFETY: `CreateEventW` returns an owned handle or null; the other
    // pointers are to live locals for the duration of the calls.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if event.is_null() {
        let _ = ready.send(Err("CreateEventW failed".to_owned()));
        return;
    }

    let mut device: WaveOutHandle = std::ptr::null_mut();
    // SAFETY: the format and event handle are valid; the event is owned by this
    // thread and closed below.
    let result = unsafe {
        waveOutOpen(
            &mut device,
            WAVE_MAPPER,
            &format,
            event as usize,
            0,
            CALLBACK_EVENT,
        )
    };
    if result != 0 {
        // SAFETY: the event was created above and is not used elsewhere.
        unsafe {
            CloseHandle(event);
        }
        let _ = ready.send(Err(format!("waveOutOpen failed with MMSYSERR {result}")));
        return;
    }

    let mut buffers: Vec<Vec<i16>> = (0..BUFFER_COUNT)
        .map(|_| vec![0i16; BUFFER_FRAMES])
        .collect();
    let mut headers: Vec<WaveHdr> = buffers
        .iter_mut()
        .map(|buffer| WaveHdr {
            data: buffer.as_mut_ptr().cast(),
            buffer_length: (buffer.len() * std::mem::size_of::<i16>()) as u32,
            bytes_recorded: 0,
            user: 0,
            flags: 0,
            loops: 0,
            next: std::ptr::null_mut(),
            reserved: 0,
        })
        .collect();
    let header_size = std::mem::size_of::<WaveHdr>() as u32;

    for header in &mut headers {
        // SAFETY: the header points at a live buffer; prepare is required before
        // the first write.
        let prepared = unsafe { waveOutPrepareHeader(device, header, header_size) };
        if prepared != 0 {
            // SAFETY: the device and event are owned by this thread.
            unsafe {
                waveOutClose(device);
                CloseHandle(event);
            }
            let _ = ready.send(Err(format!(
                "waveOutPrepareHeader failed with MMSYSERR {prepared}"
            )));
            return;
        }
    }

    initial_fill(&mixer, &mut buffers, &mut headers, device, header_size);
    if ready.send(Ok(())).is_err() {
        // The caller went away; fall through to cleanup.
        stop.store(true, Ordering::Relaxed);
    }

    while !stop.load(Ordering::Relaxed) {
        // SAFETY: the event and device are valid until the cleanup below.
        let wait = unsafe { WaitForSingleObject(event, 100) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        if wait != WAIT_OBJECT_0 {
            break;
        }
        for index in 0..headers.len() {
            if headers[index].flags & WHDR_DONE == 0 {
                continue;
            }
            lock(&mixer).mix_into(&mut buffers[index]);
            headers[index].flags &= !WHDR_DONE;
            // SAFETY: the header is prepared and its buffer is alive.
            if unsafe { waveOutWrite(device, &mut headers[index], header_size) } != 0 {
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }
    }

    // SAFETY: reset stops playback and marks the buffers done; all handles were
    // opened by this thread.
    unsafe {
        waveOutReset(device);
        for header in &mut headers {
            waveOutUnprepareHeader(device, header, header_size);
        }
        waveOutClose(device);
        CloseHandle(event);
    }
}

fn initial_fill(
    mixer: &Arc<Mutex<Mixer>>,
    buffers: &mut [Vec<i16>],
    headers: &mut [WaveHdr],
    device: WaveOutHandle,
    header_size: u32,
) {
    for index in 0..headers.len() {
        lock(mixer).mix_into(&mut buffers[index]);
        // SAFETY: the header is prepared and the buffer is alive.
        unsafe {
            waveOutWrite(device, &mut headers[index], header_size);
        }
    }
}
