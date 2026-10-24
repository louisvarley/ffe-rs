//! Flight views, as the original handles them (docs/notes/views.md).
//!
//! The view mode is the player's `DATA_008872` (0 front, 1 rear, 2/3 turrets,
//! 4 external; 5 missile and 6 combat computer need equipment not modelled
//! here). F1 cycles front, rear, turret, external; F6-F9 select directly.
//! The external camera keeps the original's yaw and pitch (16-bit angles,
//! `+0x128`/`+0x12c`) and signed distance (`+0x130`, negative = behind), turned
//! by FUNC_001054's two rotations of the ship's axes; the turrets keep theirs at
//! `+0x204`/`+0x206`. The eye sits at offsets from the ship's statistics block
//! (descriptor +0x38). The projection is F1479's.
use crate::flight::Flight;
use crate::ships::ShipStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Front,
    Rear,
    /// Mode 2: the turret looking above the ship (turret pitch < 0).
    TopTurret,
    /// Mode 3: the turret looking below (ships with 4 or more gun mounts).
    BottomTurret,
    External,
}

impl ViewMode {
    pub fn name(self) -> &'static str {
        match self {
            ViewMode::Front => "Front View",
            ViewMode::Rear => "Rear View",
            ViewMode::TopTurret => "Top Turret View",
            ViewMode::BottomTurret => "Bottom Turret View",
            ViewMode::External => "External View",
        }
    }

    pub fn is_turret(self) -> bool {
        matches!(self, ViewMode::TopTurret | ViewMode::BottomTurret)
    }

    /// FUNC_001001 (F1): front -> rear -> turret -> external -> front. The rear
    /// view skips to external unless the ship has more than two gun mounts
    /// (signed byte `ship[+0xd1]`); the turret is the top one while its pitch is
    /// negative, otherwise the bottom one.
    pub fn next(self, gun_mounts: i8, turret_pitch: i16) -> ViewMode {
        match self {
            ViewMode::Front => ViewMode::Rear,
            ViewMode::Rear => {
                ViewMode::turret(gun_mounts, turret_pitch).unwrap_or(ViewMode::External)
            }
            ViewMode::TopTurret | ViewMode::BottomTurret => ViewMode::External,
            ViewMode::External => ViewMode::Front,
        }
    }

    /// FUNC_001056's F8 (and the turret console button): `None`, leaving the view
    /// unchanged, without more than two gun mounts.
    pub fn turret(gun_mounts: i8, turret_pitch: i16) -> Option<ViewMode> {
        (gun_mounts > 2).then_some(if turret_pitch < 0 {
            ViewMode::TopTurret
        } else {
            ViewMode::BottomTurret
        })
    }
}

/// The turret angles (player `+0x204` yaw, `+0x206` pitch, 16-bit). The player
/// structure starts zeroed, as these are taken to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurretCamera {
    pub yaw: i16,
    pub pitch: i16,
}

impl TurretCamera {
    /// FUNC_001038 in a turret view, where the steering keys turn the turret
    /// instead of the ship: `pitch_input` is A minus Z and `yaw_input` comma minus
    /// period, as FUNC_CalcKeyInput amounts. Returns the view the pitch selects:
    /// above the ship (pitch clamped to -0x3fff) or, with 4 or more mounts, below
    /// it (clamped to 0x3fff); with fewer, the pitch is held at -1 (the mode
    /// itself switches on the next step, as in the original).
    pub fn step(
        &mut self,
        mode: ViewMode,
        pitch_input: i16,
        yaw_input: i16,
        gun_mounts: i8,
    ) -> ViewMode {
        self.yaw = self.yaw.wrapping_sub(yaw_input);
        self.pitch = self.pitch.wrapping_add(pitch_input);
        if self.pitch < 0 {
            if self.pitch <= -0x4000 {
                self.pitch = -0x3fff;
            }
            ViewMode::TopTurret
        } else if gun_mounts >= 4 {
            if self.pitch >= 0x4000 {
                self.pitch = 0x3fff;
            }
            ViewMode::BottomTurret
        } else {
            self.pitch = -1;
            mode
        }
    }
}

/// FUNC_CalcKeyInput: the frame's time in 1/49710 s units (`elapsed_ms`), scaled
/// by how long the key has been held in 50 Hz ticks (capped at 70), over 64.
pub fn key_amount(elapsed_ms: u32, held: Option<u32>) -> i32 {
    let Some(held) = held else {
        return 0;
    };
    let elapsed = (u64::from(elapsed_ms) * 0x3e2 / 20) as i64;
    ((elapsed * (i64::from(held.min(70)) + 2)) >> 6) as i32
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
        let amount = |held: u32| key_amount(elapsed_ms, Some(held));
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

/// The player's view: the mode, the external camera and the turret angles (both
/// kept across modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewState {
    pub mode: ViewMode,
    pub external: ExternalCamera,
    pub turret: TurretCamera,
    /// The original's "Lock external view to ship" (`DATA_009020`): the external
    /// camera's offset follows the turned forward axis; off uses the ship's own.
    pub lock_external: bool,
    /// What the console shows besides the views (time acceleration, speeds).
    pub console: crate::console::ConsoleState,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Front,
            external: ExternalCamera::default(),
            turret: TurretCamera::default(),
            lock_external: true,
            console: crate::console::ConsoleState::default(),
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
    if let Some(axes) = state.axes {
        return axes;
    }
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
    /// The camera for `views`. `stats` places the eye (none: the ship's centre);
    /// `model_scale` is model units per flight unit, which converts the eye
    /// offsets and the external distance (model units * 256).
    pub fn new(
        state: &Flight,
        views: &ViewState,
        stats: Option<&ShipStats>,
        model_scale: f64,
    ) -> Self {
        let [right, up, forward] = ship_axes(state);
        // FUNC_001053/FUNC_001054: the eye is `along` the forward axis and `above`
        // along the up axis, each word times the axis' top 16 bits (1.0 = 0x7fff)
        // over 4096, in model units.
        let eye = |forward: [f64; 3], words: [i32; 2]| -> [f64; 3] {
            let [along, above] = words.map(|w| f64::from(w) * 32767.0 / 4096.0 / model_scale);
            std::array::from_fn(|i| state.position[i] + forward[i] * along + up[i] * above)
        };
        let words = |pick: fn(&ShipStats) -> [i16; 2], adjust: i32| {
            stats.map_or([0, 0], |stats| {
                let [along, above] = pick(stats);
                [i32::from(along), i32::from(above) + adjust]
            })
        };
        match views.mode {
            ViewMode::Front => Self {
                position: eye(forward, words(|s| s.eye, 0)),
                axes: [right, up, forward],
            },
            // FUNC_001054 negates A and C (a half turn about the up axis) before
            // placing the eye, which mirrors the front offset.
            ViewMode::Rear => {
                let back = forward.map(|v| -v);
                Self {
                    position: eye(back, words(|s| s.eye, 0)),
                    axes: [right.map(|v| -v), up, back],
                }
            }
            // The eye at the turret's offset (0x3e above or below it), then the
            // axes turned as the external camera's, by the turret angles.
            ViewMode::TopTurret | ViewMode::BottomTurret => {
                let position = if views.mode == ViewMode::TopTurret {
                    eye(forward, words(|s| s.top_turret, 0x3e))
                } else {
                    eye(forward, words(|s| s.bottom_turret, -0x3e))
                };
                let (forward, right) = rotate(forward, right, views.turret.yaw as u16);
                let (up, forward) = rotate(up, forward, views.turret.pitch as u16);
                Self {
                    position,
                    axes: [right, up, forward],
                }
            }
            // rot(C, A, yaw), then rot(B, C, pitch); the camera sits at
            // distance / 256 along the turned forward axis with "Lock external
            // view to ship" on, the ship's own forward axis with it off.
            ViewMode::External => {
                let external = views.external;
                let ship_forward = forward;
                let (forward, right) = rotate(forward, right, external.yaw);
                let (up, forward) = rotate(up, forward, external.pitch);
                let offset = f64::from(external.distance) / 256.0 / model_scale;
                let along = if views.lock_external {
                    forward
                } else {
                    ship_forward
                };
                Self {
                    position: std::array::from_fn(|i| state.position[i] + along[i] * offset),
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
