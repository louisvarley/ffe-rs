use crate::read_bounded;
use ffe_core::{timer_delta, GameClock, RandomState, TimerCounters};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const STATE_FRAMES: usize = 400;
const MAX_TRACE_BYTES: u64 = 2 * 1024 * 1024;
const SEEDS: [(u32, u32); 6] = [
    (0, 0),
    (0x12345678, 0x456789ab),
    (0x12345678, 0x89abcdef),
    (u32::MAX, u32::MAX),
    (0x80000000, 1),
    (1, 0x80000000),
];
const ELAPSED: [u32; 10] = [0, 1, 5, 19, 20, 21, 39, 40, 100, 1000];
const ACCELERATIONS: [u32; 6] = [0, 1, 994, 1000, 10000, 1000000];

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NumericRow {
    Rng {
        scenario: usize,
        step: usize,
        before_a: u32,
        before_b: u32,
        after_a: u32,
        after_b: u32,
        value: u16,
    },
    Timer {
        elapsed_ms: u32,
        acceleration: u32,
        before_system: u32,
        before_accel: u32,
        after_system: u32,
        after_accel: u32,
        last_ms: u32,
        next_50hz_ms: u32,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateRow {
    frame: usize,
    clock_ms: u32,
    game_tics: u32,
    game_days: u32,
    rng_a: u32,
    rng_b: u32,
    player_state: u8,
    player_index: u32,
    cash: u32,
    start_type: u32,
    time_accel: u32,
    system_counter: u32,
    accel_counter: u32,
    pixels_fnv1a64: String,
    palette_fnv1a64: String,
}

#[derive(Debug, Serialize)]
pub struct ReferenceReport {
    pub reference: &'static str,
    pub numeric_adapter: &'static str,
    pub state_adapter: &'static str,
    pub rng_vectors: usize,
    pub timer_vectors: usize,
    pub state_frames: usize,
    pub game_clock_transitions: usize,
    pub new_game_frame: usize,
    pub vectors_sha256: String,
    pub state_sha256: String,
    pub full_gameplay_validated: bool,
}

fn parse_rows<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    expected: usize,
) -> Result<Vec<T>, String> {
    if bytes.len() as u64 > MAX_TRACE_BYTES {
        return Err("reference trace exceeds 2 MiB limit".to_owned());
    }
    let text = std::str::from_utf8(bytes).map_err(|error| format!("trace UTF-8: {error}"))?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != expected {
        return Err(format!(
            "expected {expected} trace rows, found {}",
            lines.len()
        ));
    }
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| format!("trace row {}: {error}", index + 1))
        })
        .collect()
}

fn verify_vectors(bytes: &[u8]) -> Result<(), String> {
    let rows: Vec<NumericRow> = parse_rows(bytes, SEEDS.len() * 256 + 60)?;
    for (scenario_index, (a, b)) in SEEDS.into_iter().enumerate() {
        let mut rng = RandomState::new(a, b);
        for step_index in 0..256 {
            let before = rng;
            let expected = rng.next_u16();
            match rows[scenario_index * 256 + step_index] {
                NumericRow::Rng { scenario, step, before_a, before_b, after_a, after_b, value }
                    if scenario == scenario_index && step == step_index
                        && RandomState::new(before_a, before_b) == before
                        && RandomState::new(after_a, after_b) == rng && value == expected => {}
                _ => return Err(format!("RNG mismatch or coverage error at scenario {scenario_index}, step {step_index}")),
            }
        }
    }
    for (accel_index, expected_accel) in ACCELERATIONS.into_iter().enumerate() {
        for (elapsed_index, expected_elapsed) in ELAPSED.into_iter().enumerate() {
            let initial_system = if elapsed_index % 2 == 0 {
                0
            } else {
                0xfffffff0
            };
            let initial_accel = if elapsed_index % 2 == 0 {
                0
            } else {
                0xffffffe0
            };
            let mut counters = TimerCounters {
                system: initial_system,
                accelerated: initial_accel,
            };
            counters
                .advance(expected_elapsed, expected_accel)
                .map_err(|error| error.to_string())?;
            match rows[1536 + accel_index * ELAPSED.len() + elapsed_index] {
                NumericRow::Timer { elapsed_ms, acceleration, before_system, before_accel,
                    after_system, after_accel, last_ms, next_50hz_ms }
                    if elapsed_ms == expected_elapsed && acceleration == expected_accel
                        && before_system == initial_system && before_accel == initial_accel
                        && after_system == counters.system && after_accel == counters.accelerated
                        && last_ms == 100 + elapsed_ms
                        && next_50hz_ms == 100 + (elapsed_ms / 20 + 1) * 20 => {}
                _ => return Err(format!("timer mismatch or coverage error at acceleration {expected_accel}, elapsed {expected_elapsed}")),
            }
        }
    }
    Ok(())
}

fn hash16(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_state(bytes: &[u8]) -> Result<(usize, usize), String> {
    let rows: Vec<StateRow> = parse_rows(bytes, STATE_FRAMES)?;
    let first = &rows[0];
    if first.game_tics != 0 || first.game_days != 0 || first.start_type != 0 {
        return Err("state trace did not begin at zeroed startup".to_owned());
    }
    let mut new_game_frame = None;
    let mut checked = 0;
    for (index, row) in rows.iter().enumerate() {
        if row.frame != index || !hash16(&row.pixels_fnv1a64) || !hash16(&row.palette_fnv1a64) {
            return Err(format!("state row {index}: invalid frame/hash"));
        }
        if row.start_type != 0 && row.start_type != 1 {
            return Err(format!("state row {index}: unsupported scripted start"));
        }
        if row.start_type == 1 {
            if row.game_days != 0x121cf7
                || row.player_state != 0x30
                || row.player_index != 107
                || row.cash != 1000
                || row.time_accel != 994
            {
                return Err(format!("state row {index}: unexpected new-game state"));
            }
            if new_game_frame.is_none() {
                if index != 161 || row.game_tics != 0x80000000 {
                    return Err("new-game transition does not match bounded-new-game-v1".to_owned());
                }
                new_game_frame = Some(index);
            } else if row.accel_counter != 0 || row.system_counter != 0 {
                return Err(format!(
                    "state row {index}: counters were not consumed by main loop"
                ));
            }
        } else if new_game_frame.is_some() {
            return Err("state unexpectedly returned to intro".to_owned());
        }
        if index > 0 {
            let previous = &rows[index - 1];
            let elapsed = row
                .clock_ms
                .checked_sub(previous.clock_ms)
                .filter(|elapsed| *elapsed > 0)
                .ok_or("trace clock did not advance")?;
            if previous.start_type == 1 && row.start_type == 1 {
                let mut clock = GameClock {
                    days: previous.game_days,
                    fraction: previous.game_tics,
                };
                clock.advance(
                    timer_delta(elapsed, previous.time_accel).map_err(|error| error.to_string())?,
                );
                if clock.days != row.game_days || clock.fraction != row.game_tics {
                    return Err(format!("Rust game-clock mismatch at frame {index}"));
                }
                checked += 1;
            }
        }
    }
    if rows
        .iter()
        .all(|row| row.pixels_fnv1a64 == first.pixels_fnv1a64)
        || rows
            .last()
            .is_some_and(|row| row.rng_a == 0 && row.rng_b == 0)
    {
        return Err("state trace has no framebuffer/random-state activity".to_owned());
    }
    let new_game_frame = new_game_frame.ok_or("trace never entered the new game")?;
    if checked != 238 {
        return Err(format!(
            "expected 238 in-game clock transitions, found {checked}"
        ));
    }
    Ok((new_game_frame, checked))
}

/// Verifies the fixed scenarios; this cannot authenticate that an arbitrary
/// supplied trace really came from the legacy executable.
pub fn verify_trace_bytes(vectors: &[u8], state: &[u8]) -> Result<ReferenceReport, String> {
    verify_vectors(vectors)?;
    let (new_game_frame, game_clock_transitions) = verify_state(state)?;
    Ok(ReferenceReport {
        reference: "jjffe-2.8a7",
        numeric_adapter: "rng-timer-v1",
        state_adapter: "bounded-new-game-v1",
        rng_vectors: 1536,
        timer_vectors: 60,
        state_frames: STATE_FRAMES,
        game_clock_transitions,
        new_game_frame,
        vectors_sha256: format!("{:x}", Sha256::digest(vectors)),
        state_sha256: format!("{:x}", Sha256::digest(state)),
        full_gameplay_validated: false,
    })
}

pub fn verify_reference(vectors: &Path, state: &Path) -> Result<ReferenceReport, String> {
    verify_trace_bytes(
        &read_bounded(vectors, MAX_TRACE_BYTES)?,
        &read_bounded(state, MAX_TRACE_BYTES)?,
    )
}
