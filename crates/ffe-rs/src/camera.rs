//! Flight views, as the original handles them (docs/notes/views.md).
//!
//! The view mode is the player's `DATA_008872` (0 front, 1 rear, 2/3 turrets,
//! 4 external; 5 missile and 6 combat computer need equipment not modelled
//! here). F1 cycles front, rear, (turret), external; F6/F7/F9 select directly.
//! The external camera keeps the original's yaw and pitch (16-bit angles,
//! `+0x128`/`+0x12c`) and signed distance (`+0x130`, negative = behind), turned
//! by FUNC_001054's two rotations of the ship's axes. The projection is F1479's.
use crate::flight::Flight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Front,
    Rear,
    External,
}

impl ViewMode {
    pub fn name(self) -> &'static str {
        match self {
            ViewMode::Front => "Front View",
            ViewMode::Rear => "Rear View",
            ViewMode::External => "External View",
        }
    }

    /// FUNC_001001 (F1): front -> rear -> turret -> external -> front. The turret
    /// views are skipped, as for a ship with no more than two gun mounts.
    pub fn next(self) -> ViewMode {
        match self {
            ViewMode::Front => ViewMode::Rear,
            ViewMode::Rear => ViewMode::External,
            ViewMode::External => ViewMode::Front,
        }
    }
}

/// The original external camera: angles where 0x10000 is a full turn, and the
/// distance in model units * 256 (negative: behind the ship).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalCamera {
    pub yaw: u16,
    pub pitch: u16,
    pub distance: i32,
}

/// FUNC_001000's values at game start; the opposite-key reset uses pitch 0.
impl Default for ExternalCamera {
    fn default() -> Self {
        Self {
            yaw: 0,
            pitch: 0x200,
            distance: DEFAULT_DISTANCE,
        }
    }
}

/// `0xff400000`.
pub const DEFAULT_DISTANCE: i32 = -0x00c0_0000;

/// One frame of external-view keys: for each held key, the number of 50 Hz ticks
/// it has been held (`DATA_KeyAccumTable`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExternalKeys {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub up: Option<u32>,
    pub down: Option<u32>,
    /// Numpad + or `=`.
    pub zoom_out: Option<u32>,
    /// Numpad - or `-`.
    pub zoom_in: Option<u32>,
}

impl ExternalCamera {
    /// FUNC_001056's external-view keys for one frame of `elapsed_ms`.
    /// `minimum` is the closest the camera may come: the ship model's descriptor
    /// radius word shifted by its scale + 8 (FUNC_001110).
    pub fn step(&mut self, keys: ExternalKeys, elapsed_ms: u32, minimum: i32) {
        // FUNC_CalcKeyInputNoPause: the frame's time in 1/49710 s units, scaled by
        // how long the key has been held (capped at 70 ticks).
        let elapsed = (u64::from(elapsed_ms) * 0x3e2 / 20) as i64;
        let amount = |held: u32| ((elapsed * (i64::from(held.min(70)) + 2)) >> 6) as i32;
        match (keys.left, keys.right) {
            (Some(_), Some(_)) => self.yaw = 0,
            (Some(held), None) => self.yaw = self.yaw.wrapping_add(amount(held) as u16),
            (None, Some(held)) => self.yaw = self.yaw.wrapping_sub(amount(held) as u16),
            (None, None) => {}
        }
        match (keys.up, keys.down) {
            (Some(_), Some(_)) => self.pitch = 0,
            (Some(held), None) => self.pitch = self.pitch.wrapping_add(amount(held) as u16),
            (None, Some(held)) => self.pitch = self.pitch.wrapping_sub(amount(held) as u16),
            (None, None) => {}
        }
        match (keys.zoom_out, keys.zoom_in) {
            (Some(_), Some(_)) => self.distance = DEFAULT_DISTANCE,
            (Some(held), None) => {
                self.distance = self.distance.wrapping_sub(amount(held).wrapping_shl(10));
                if self.distance >= 0 {
                    self.distance = i32::MIN + 1;
                }
            }
            (None, Some(held)) => {
                self.distance = self.distance.wrapping_add(amount(held).wrapping_shl(10));
                self.distance = self.distance.min(minimum.saturating_neg());
            }
            (None, None) => {}
        }
    }
}

/// The player's view: the mode and the external camera (kept across modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewState {
    pub mode: ViewMode,
    pub external: ExternalCamera,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Front,
            external: ExternalCamera::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: [f64; 3],
    /// Right, up and forward unit vectors in flight space.
    pub axes: [[f64; 3]; 3],
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The ship's own axes (right, up, forward) from the flight yaw and pitch.
pub fn ship_axes(state: &Flight) -> [[f64; 3]; 3] {
    let (sy, cy) = state.yaw.sin_cos();
    let (sp, cp) = state.pitch.sin_cos();
    [
        [cy, 0.0, -sy],
        [-sy * sp, cp, -cy * sp],
        [sy * cp, sp, cy * cp],
    ]
}

/// FUNC_001513: `v1' = v1 cos a + v2 sin a`, `v2' = v2 cos a - v1 sin a`.
fn rotate(v1: [f64; 3], v2: [f64; 3], angle: u16) -> ([f64; 3], [f64; 3]) {
    let radians = f64::from(angle) / 65536.0 * std::f64::consts::TAU;
    let (s, c) = radians.sin_cos();
    (
        std::array::from_fn(|i| v1[i] * c + v2[i] * s),
        std::array::from_fn(|i| v2[i] * c - v1[i] * s),
    )
}

impl Camera {
    /// The camera for `mode`. `model_scale` is model units per flight unit, which
    /// converts the external distance (model units * 256).
    pub fn new(state: &Flight, mode: ViewMode, external: ExternalCamera, model_scale: f64) -> Self {
        let [right, up, forward] = ship_axes(state);
        match mode {
            ViewMode::Front => Self {
                position: state.position,
                axes: [right, up, forward],
            },
            // FUNC_001054 negates A and C: a half turn about the up axis.
            ViewMode::Rear => Self {
                position: state.position,
                axes: [right.map(|v| -v), up, forward.map(|v| -v)],
            },
            // rot(C, A, yaw), then rot(B, C, pitch); the camera sits at
            // distance / 256 along the turned forward axis ("lock to ship" on).
            ViewMode::External => {
                let (forward, right) = rotate(forward, right, external.yaw);
                let (up, forward) = rotate(up, forward, external.pitch);
                let offset = f64::from(external.distance) / 256.0 / model_scale;
                Self {
                    position: std::array::from_fn(|i| state.position[i] + forward[i] * offset),
                    axes: [right, up, forward],
                }
            }
        }
    }

    /// A flight-space direction in view space.
    pub fn direction(&self, v: [f64; 3]) -> [f64; 3] {
        self.axes.map(|axis| dot(axis, v))
    }

    /// A flight-space point in view space.
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        self.direction(std::array::from_fn(|i| p[i] - self.position[i]))
    }

    /// Screen position of a point (`translated`) or a direction at infinity.
    pub fn project(&self, p: [f64; 3], translated: bool) -> Option<(i32, i32)> {
        let [x, y, z] = if translated {
            self.point(p)
        } else {
            self.direction(p)
        };
        if z <= 2.0 {
            return None;
        }
        let sx = 160.0 + 256.0 * x / z;
        let sy = 79.0 - 256.0 * y / z;
        if sx.abs() > 2000.0 || sy.abs() > 2000.0 {
            return None;
        }
        Some((sx as i32, sy as i32))
    }
}
