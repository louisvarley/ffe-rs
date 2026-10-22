//! Animation inputs of a top-level model instance, and orbital rotation.
//!
//! A model program animates through its instance: the global words it reads
//! (selector `0b10`) are the *game object's* words from +0x9c, because the
//! top-level instance's +0x14c points at the object record itself (F1680,
//! F1683, F1684); locals live at instance +0x54. This module ports the routines
//! that build that instance and the object fields the original advances over
//! time. Routine numbers, offsets and what is verified are recorded in
//! `docs/notes/animation.md`.
//!
//! - [`time_word`]: F1534 / F1536 / F1537 write global word 0 before each draw.
//! - [`door_step`]: F865, the starport/station animation of global word 1.
//! - [`build_yzt`]: F1675 (`MatBuildYZT`).
//! - [`OrbitalRotation`]: F866's rotation step for orbital objects.
//! - [`setup_display`] (F1683) and [`setup_object`] (F1680, objects in the
//!   camera's own frame of reference): the instance an object's model runs in.
use crate::model::{multiply, VertexContext, IDENTITY};
use crate::model_execute::{child_culled, InstanceContext};
use crate::model_instance::project;
use crate::model_math::{shift_down, transform_rows, SineTable};

/// `GameTics` (`DATA_008804`) at the start of a new game.
pub const START_TICS: u32 = 0x8000_0000;
/// Day-fraction units per 20 ms frame at acceleration 1 (`timer_delta(20, 994)`).
pub const TICS_PER_FRAME: u32 = 994;

/// Global word indices (object word `+0x9c + 2 * index`). Only the first two
/// are written per frame by code this module ports; the others are listed
/// where `docs/notes/animation.md` names their source.
pub mod global {
    /// +0x9c: `GameTics >> 10`, written before every draw (F1534).
    pub const TIME: usize = 0;
    /// +0x9e: landing gear (ships) or docking door (stations, F865).
    pub const STATE: usize = 1;
    /// +0xa0 (dword, words 2 and 3): the object's random seed.
    pub const SEED: usize = 2;
    /// +0xa4 (word 4): light-point count of opcode 30 `0x801e`; the low word
    /// of the planet mass field.
    pub const MASS: usize = 4;
    /// +0xa8 (dword, words 6 and 7): creation time; orbital objects store the
    /// tics of their last F866 update here.
    pub const UPDATED_TICS: usize = 6;
    /// +0xac (dword, words 8 and 9): creation day; F866's last update day.
    pub const UPDATED_DAYS: usize = 8;
}

/// F1534 as F1536/F1537 use it: global word 0 of every object drawn this frame
/// is `GameTics >> 10`, truncated to 16 bits (a full cycle every 2^26 tics,
/// about 22.5 minutes of game time).
pub fn time_word(tics: u32) -> u16 {
    (tics >> 10) as u16
}

/// F865: one frame of the starport/station animation of word +0x9e (global 1),
/// directed by the signed byte +0xc9. `frame_time` is the frame's accelerated
/// tics (`DATA_009155`); the word moves by `frame_time >> 1`. Opening (+0xc9 > 0)
/// stops at 0xffff and clears bit 0 of +0xc9; closing (< 0) stops at 0 and sets
/// +0xc9 to -2. Signed 32-bit compares, as the original.
pub fn door_step(word: u16, state: i8, frame_time: u32) -> (u16, i8) {
    let step = (frame_time >> 1) as i32;
    if state > 0 {
        let sum = i32::from(word).wrapping_add(step);
        if sum > 0xffff {
            (0xffff, ((state as u8) & 0xfe) as i8)
        } else {
            (sum as u16, state)
        }
    } else if state < 0 {
        let difference = i32::from(word).wrapping_sub(step);
        if difference < 0 {
            (0, -2)
        } else {
            (difference as u16, state)
        }
    } else {
        (word, state)
    }
}

/// F1675 (`MatBuildYZT`): with sine/cosine of `a` and `b` (16-bit binary angles
/// through Sin32) and F1521 products, rows
/// `[ca·cb, sa, -(sb·ca)]`, `[-(sa·cb), ca, sa·sb]`, `[sb, 0, cb]`.
/// Row k is object axis k in the parent frame, so with `b` fixed `a` turns the
/// object about its own z axis, which points along `(sin b, 0, cos b)`.
pub fn build_yzt(table: &SineTable, a: i32, b: i32) -> [i32; 9] {
    let sa = table.sin32(a);
    let sb = table.sin32(b);
    let ca = table.sin32(a.wrapping_add(0x4000));
    let cb = table.sin32(b.wrapping_add(0x4000));
    [
        multiply(cb, ca),
        sa,
        multiply(ca, sb).wrapping_neg(),
        multiply(cb, sa).wrapping_neg(),
        ca,
        multiply(sb, sa),
        sb,
        0,
        cb,
    ]
}

/// Rotation fields of an orbital object (`ship_type` 1): the signed speed byte
/// +0xc8, the orientation word +0xcc and the station flag (+0x14c bit 0x10).
/// F866 runs for every orbital each update; starports (+0x14c bit 0x20) return
/// before the rotation and never turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrbitalRotation {
    pub speed: i8,
    pub orientation: i16,
    pub station: bool,
}

/// The F892 station templates: the generated station copies hand-coded Sol
/// object 2 (Daedalus, F873 rotation speed 12), 17 (Galileo, 11) or 12
/// (Abraham Lincoln, 10), and F880 copies that byte to +0xc8.
pub const TRADING_POST_SPEED: i8 = 12;
pub const ORBITAL_STATION_SPEED: i8 = 11;
pub const ORBITAL_CITY_SPEED: i8 = 10;

/// The rotation speeds a generated station with this model can have (F892
/// cases 0..4 with their model tables `DATA_007136..7138`). Models 0x4c and
/// 0x4e appear in two cases; any other model is not a generated station.
pub fn station_speeds(model: u16) -> &'static [i8] {
    match model {
        0x48 | 0x4b | 0x45 => &[TRADING_POST_SPEED],
        0x49 | 0x4d => &[ORBITAL_STATION_SPEED],
        0x4c | 0x4e => &[TRADING_POST_SPEED, ORBITAL_STATION_SPEED],
        0x4a | 0x50 | 0x4f => &[ORBITAL_CITY_SPEED],
        _ => &[],
    }
}

impl OrbitalRotation {
    /// A generated station: orientation 0 (F873 stores 0 in the template).
    pub fn station(speed: i8) -> Self {
        Self {
            speed,
            orientation: 0,
            station: true,
        }
    }

    /// The tics value F866 compares and builds from: `tics << (speed - 1)` for a
    /// positive speed (shift count masked to 5 bits), unshifted for a negative
    /// one; `None` for speed 0, which does not rotate.
    fn shifted(&self, tics: u32) -> Option<u32> {
        if self.speed == 0 {
            return None;
        }
        let shift = i32::from(self.speed) - 1;
        Some(if shift >= 0 {
            tics.wrapping_shl(shift as u32 & 31)
        } else {
            tics
        })
    }

    /// The 16-bit angle F866 builds the matrix from at these tics.
    pub fn angle(&self, tics: u32) -> Option<u16> {
        self.shifted(tics).map(|value| (value >> 16) as u16)
    }

    /// The matrix F866 builds at these tics: F1675 with the angle first for a
    /// station (spin about its own z axis), second otherwise.
    pub fn matrix(&self, table: &SineTable, tics: u32) -> Option<[i32; 9]> {
        let angle = i32::from(self.angle(tics)?);
        let orientation = i32::from(self.orientation);
        Some(if self.station {
            build_yzt(table, angle, orientation)
        } else {
            build_yzt(table, orientation, angle)
        })
    }

    /// F866's rotation step between the previous update's tics (+0xa8 before the
    /// update) and this one's: a new matrix only when bits 20 and up of the
    /// shifted value changed, otherwise the object keeps its matrix.
    pub fn update(&self, table: &SineTable, previous: u32, tics: u32) -> Option<[i32; 9]> {
        let before = self.shifted(previous)?;
        let now = self.shifted(tics)?;
        if before >> 20 == now >> 20 {
            return None;
        }
        self.matrix(table, tics)
    }
}

/// The parts of a game object record (`PhysObj`) the instance set-up reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectState {
    /// +0x00: orientation relative to the parent (rows = object axes).
    pub matrix: [i32; 9],
    /// Low dwords of +0x26/+0x2e/+0x36, the position F1683 copies.
    pub display_position: [i32; 3],
    /// +0x3e: 64-bit position relative to the parent.
    pub position: [i64; 3],
    /// +0x7e: the object's exponential scale.
    pub scale: i32,
    /// +0x9c..+0x11b: the 64 words a model reads as globals.
    pub words: [u16; 64],
}

impl Default for ObjectState {
    fn default() -> Self {
        Self {
            matrix: IDENTITY,
            display_position: [0; 3],
            position: [0; 3],
            scale: 0,
            words: [0; 64],
        }
    }
}

/// The camera object F1680 draws from: its orientation (+0x00) and relative
/// position (+0x3e), in the same frame of reference as the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub matrix: [i32; 9],
    pub position: [i64; 3],
}

/// Model descriptor dwords (`F1538` entry) the set-up reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// +0x14: vertex scale, the instance's +0x148.
    pub scale: i32,
    /// +0x18: the instance's depth-key scale +0x130.
    pub scale_offset: i32,
    /// +0x1c: cull radius (F1686).
    pub radius: i32,
    /// +0x20: distance class beyond which F1680 does not draw at all.
    pub far: i32,
    /// +0x28: distance class from which F1680 draws a dot (F1700) instead.
    pub dot: i32,
}

/// A top-level instance as the original holds it when it calls F1773.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLevel {
    /// Frame (+0x14), scale (+0x148), globals (the object's words), locals
    /// (+0x54: local 0 = 0, locals 12..17 the shades at +0x84), colour 1,1,1
    /// (+0xf4), base handedness 0 (+0xf8) and the +0x38 light as `direction`.
    /// Locals 1..11 and the basis (+0x9c, +0xfc) hold stale stack data in the
    /// original: `undefined_locals` marks 1..11, and `basis`/`handedness` are
    /// placeholders a program must write (opcode 28/25) before reading.
    pub context: VertexContext,
    /// Position (+0x4), key scale (+0x130), operand scale (+0x148), generation
    /// 2 (F1774 sets +0xf0).
    pub instance: InstanceContext,
    /// +0x134: the light vector the caller passed (`DATA_008821..8823`).
    pub light: [i32; 3],
    /// F1685 observer scale (+0x44) and position (+0x48), before F1774's raise.
    pub observer_scale: u32,
    pub observer: [i32; 3],
}

impl TopLevel {
    /// The observer F1774 hands to normal evaluation: +0x44 raised to
    /// `scale - 8` when lower, shifting the position left by the difference.
    pub fn execution_observer(&self) -> (u32, [i32; 3]) {
        let mut scale = self.observer_scale as i32;
        let mut point = self.observer;
        let raise = (self.context.scale as i32)
            .wrapping_sub(8)
            .wrapping_sub(scale);
        if raise > 0 {
            scale += raise;
            point = point.map(|value| value.wrapping_shl(raise as u32));
        }
        (scale as u32, point)
    }
}

/// What F1680 does with an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Draw {
    /// Distance class at or beyond descriptor +0x20: nothing drawn; +0x8a holds
    /// the "outside view" byte, the screen position is 0x8000.
    Far { distance: u8, outside: u8 },
    /// F1686 culled the bounding sphere.
    Culled {
        distance: u8,
        screen: Option<[i16; 2]>,
    },
    /// Distance class at or beyond descriptor +0x28 (and in front): an F1700 dot.
    Dot {
        distance: u8,
        screen: Option<[i16; 2]>,
    },
    /// The model runs (F1685, F1773, F1780).
    Model {
        distance: u8,
        screen: Option<[i16; 2]>,
        instance: Box<TopLevel>,
    },
}

/// F1657: the bit length class of a 64-bit vector (see the note for the
/// high-dword and sign rules).
pub fn vector_msb(vector: [i64; 3]) -> u32 {
    let high = vector.map(|value| (value >> 32) as i32);
    let low = vector.map(|value| value as u32);
    let wide = high.iter().any(|h| *h != 0 && *h != -1);
    let (mut bits, mut base) = if wide {
        (
            high.iter().fold(0u32, |acc, h| {
                acc | if *h < 0 { h.wrapping_neg() } else { *h } as u32
            }),
            32,
        )
    } else {
        (
            (0..3).fold(0u32, |acc, i| {
                acc | if high[i] < 0 {
                    low[i].wrapping_neg()
                } else {
                    low[i]
                }
            }),
            0,
        )
    };
    for (limit, shift) in [(0xffff, 16), (0xff, 8), (0xf, 4), (3, 2)] {
        if bits > limit {
            bits >>= shift;
            base += shift;
        }
    }
    if bits > 1 {
        base += 1;
    }
    base
}

/// F1660: arithmetic right shift of each 64-bit component, with the original's
/// three count ranges (count masked to 5 bits within each).
pub fn shift_vector(vector: [i64; 3], count: u32) -> [i64; 3] {
    vector.map(|value| {
        let (low, high) = (value as u32, (value >> 32) as i32);
        let (low, high) = match count {
            0 => (low, high),
            1..=31 => (
                (high as u32).wrapping_shl(32 - count) | low >> count,
                high >> count,
            ),
            32 => (high as u32, high >> 31),
            _ => ((high >> ((count - 32) & 31)) as u32, high >> 31),
        };
        (i64::from(high) << 32) | i64::from(low)
    })
}

/// F1673: `result[i][j]` = row i of `a` dotted with row j of `b` (F1521 sums).
pub fn rows_product(a: &[i32; 9], b: &[i32; 9]) -> [i32; 9] {
    std::array::from_fn(|k| {
        let (i, j) = (k / 3, k % 3);
        (0..3).fold(0i32, |sum, c| {
            sum.wrapping_add(multiply(a[3 * i + c], b[3 * j + c]))
        })
    })
}

/// Locals of a top-level instance: local 0 = 0 (F1680/F1683 write it), locals
/// 12..17 the shade bytes copied to +0x84; the mask marks 1..11, which the
/// original leaves as stack data.
pub fn top_level_locals(shades: &[[u8; 3]; 8]) -> ([i32; 64], u32) {
    let mut locals = [0i32; 64];
    let bytes: Vec<u8> = shades.iter().flatten().copied().collect();
    for (k, chunk) in bytes.chunks_exact(4).enumerate() {
        locals[12 + k] = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    (locals, 0b1111_1111_1110)
}

fn instance(
    matrix: [i32; 9],
    position: [i32; 3],
    scale: i32,
    key_scale: i32,
    words: [u16; 64],
    light: [i32; 3],
    shades: &[[u8; 3]; 8],
) -> TopLevel {
    let (locals, undefined_locals) = top_level_locals(shades);
    // F1685: observer = -(position shifted below 0x4000) in object axes; the
    // light (+0x134) in object axes at +0x38.
    let (mut shifted, shift) = shift_down(position);
    let mut observer_scale = shift as i32;
    let difference = observer_scale.wrapping_add(7).wrapping_sub(scale);
    if difference < 0 {
        observer_scale = observer_scale.wrapping_sub(difference);
        let by = difference.wrapping_neg() as u32 & 31;
        shifted = shifted.map(|value| value >> by);
    }
    let observer = transform_rows(shifted.map(i32::wrapping_neg), &matrix);
    let direction = transform_rows(light, &matrix);
    TopLevel {
        context: VertexContext {
            matrix,
            scale: scale as u32,
            globals: words,
            locals,
            colour: [1; 3],
            basis: IDENTITY,
            base_handedness: 0,
            handedness: 0,
            direction,
            undefined_locals,
        },
        instance: InstanceContext {
            position,
            generation: 2,
            key_scale,
            operand_scale: scale,
            textures: true,
        },
        light,
        observer_scale: observer_scale as u32,
        observer,
    }
}

/// F1683 (and F1684 before its own F1774 call): a stand-alone display object.
/// The frame is the object's +0x00 matrix, the position the low dwords of
/// +0x26/+0x2e/+0x36, the scale descriptor +0x14 and the key scale 0. `None`
/// when F1686 culls it.
pub fn setup_display(
    object: &ObjectState,
    descriptor: &Descriptor,
    light: [i32; 3],
    shades: &[[u8; 3]; 8],
) -> Option<TopLevel> {
    let position = object.display_position;
    let radius = descriptor.radius.wrapping_shl(descriptor.scale as u32 & 31);
    if child_culled(position, radius) {
        return None;
    }
    Some(instance(
        object.matrix,
        position,
        descriptor.scale,
        0,
        object.words,
        light,
        shades,
    ))
}

/// F1680 for an object in the camera's own frame of reference (same parent
/// +0x56, neither in a rotating frame +0x57): relative position, distance
/// class (+0x88), the far/cull/dot decisions and the instance it runs.
pub fn setup_object(
    object: &ObjectState,
    camera: &Camera,
    descriptor: &Descriptor,
    light: [i32; 3],
    shades: &[[u8; 3]; 8],
) -> Draw {
    // F1678: object minus camera (Vec64Sub).
    let mut relative: [i64; 3] =
        std::array::from_fn(|i| object.position[i].wrapping_sub(camera.position[i]));
    let msb = vector_msb(relative) as i32;
    let mut shift = msb - 0x1c;
    if shift > 0 {
        relative = shift_vector(relative, shift as u32);
    } else {
        shift = 0;
    }
    let distance = (msb - 0xd).max(0).wrapping_sub(object.scale);
    if distance >= descriptor.far {
        let outside = distance.wrapping_sub(descriptor.far);
        return Draw::Far {
            distance: distance as u8,
            outside: if outside > 5 {
                0x3f
            } else {
                1u8 << (outside & 7)
            },
        };
    }
    let mut position = transform_rows(relative.map(|value| value as i32), &camera.matrix);
    let mut operand_scale = descriptor.scale;
    let mut key_scale = descriptor.scale_offset;
    let excess = key_scale.wrapping_add(object.scale).wrapping_sub(shift);
    if excess < 0 {
        key_scale = key_scale.wrapping_sub(excess);
        operand_scale = operand_scale.wrapping_add(excess).max(0);
    } else {
        let by = excess as u32 & 31;
        position = position.map(|value| value >> by);
    }
    let screen = project(position);
    let radius = descriptor.radius.wrapping_shl(operand_scale as u32 & 31);
    if child_culled(position, radius) {
        return Draw::Culled {
            distance: distance as u8,
            screen,
        };
    }
    if position[2] >= 0x40 && distance >= descriptor.dot {
        return Draw::Dot {
            distance: distance as u8,
            screen,
        };
    }
    Draw::Model {
        distance: distance as u8,
        screen,
        instance: Box::new(instance(
            rows_product(&object.matrix, &camera.matrix),
            position,
            operand_scale,
            key_scale,
            object.words,
            light,
            shades,
        )),
    }
}
