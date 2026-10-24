//! The lift scenes of docking and launching, ported from `ffe.asm` (F1070-F1088, F1092).
//!
//! While the player's stage machine runs (`traffic::Stages`), each frame the original
//! draws one or two lift models in the player's view: F1070 puts a model at a depth in
//! the ship's frame (less the eye's offset, so in view space), turned about the vertical
//! by an angle, with an animation word (the door), and draws it. The stage's remaining
//! timeout gives the depth, the word and the angle. This module is that arithmetic;
//! drawing is the caller's.

use crate::traffic::state;

/// The model the lift cage and its doors use (`0xea`).
pub const CAGE: u32 = 0xea;
/// The model of the bay beyond the lift (`0xe9`).
pub const BAY: u32 = 0xe9;
/// The model of the lift's interior (`0xec`).
pub const INTERIOR: u32 = 0xec;

/// A depth the cage starts at (`0x30000`) and the bay's depth while the inner door opens.
const CAGE_DEPTH: i32 = 0x3_0000;
const BAY_DEPTH: i32 = 0x2_ea60;

/// One model to draw: its depth in the ship's frame (model units), the animation
/// word (global 1: the door, 0 open), and the turn about the vertical (a binary
/// angle, `0x10000` a full turn).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    pub model: u32,
    pub depth: i32,
    pub word: u16,
    pub angle: u16,
}

fn object(model: u32, depth: i32, word: i32, angle: i32) -> Object {
    Object {
        model,
        depth,
        word: word as u16,
        angle: angle as u16,
    }
}

/// The models a stage draws when `remaining` is what F1067/F1068 leave in `*t`: the
/// timeout still to run (zero on the frame the stage ends). `None` for a state without
/// a scene (the touch, the first stage, and flight).
pub fn objects(stage: u8, remaining: i32) -> Option<Vec<Object>> {
    use state::*;
    let t = remaining;
    Some(match stage {
        // F1069: the cage ahead, door opening.
        OPEN_DOOR => vec![object(CAGE, CAGE_DEPTH, t >> 1, 0x8000)],
        // F1074: the cage comes to the ship.
        INTO_LIFT => vec![object(CAGE, t.wrapping_add(t), 0, 0x8000)],
        // F1075: the ship turns in the cage (the angle is a quarter of the time left).
        TURN => {
            let q = t >> 2;
            vec![object(CAGE, 0, q, q & 0xffff)]
        }
        // F1076: the door closes.
        CLOSE_DOOR => vec![object(CAGE, 0, !((t >> 1) & 0xffff), 0)],
        // F1077 for F1078: waiting inside the lift.
        WAIT_IN_LIFT => vec![object(INTERIOR, -1, 0, 0)],
        // F1081, F1080: the inner door opens on the bay.
        OPEN_INNER => vec![
            object(INTERIOR, 0, t >> 1, 0),
            object(BAY, BAY_DEPTH, t >> 1, 0),
        ],
        // F1082, F1079: into the bay.
        INTO_BAY => {
            let depth = t.wrapping_shl(1);
            vec![object(BAY, depth, depth, 0)]
        }
        // F1083, F1079: out of the bay.
        LAUNCH_INTO_LIFT => {
            let t2 = t.wrapping_shl(1);
            vec![object(BAY, BAY_DEPTH.wrapping_sub(t2), t2, 0)]
        }
        // F1084, F1080: the inner door closes.
        LAUNCH_CLOSE => {
            let word = !(t >> 1);
            vec![
                object(INTERIOR, 0, word, 0),
                object(BAY, BAY_DEPTH, word, 0),
            ]
        }
        // F1085, F1077: waiting.
        LAUNCH_WAIT => vec![object(INTERIOR, -1, 0, 0)],
        // F1087: the cage door opens.
        LAUNCH_OPEN => vec![object(CAGE, 0, t >> 1, 0)],
        // F1086: the cage drops away behind (the timeout is the original's `+0x106` after
        // this frame's decrement; at or below zero the cage is at its far pose).
        LAUNCH_OUT => {
            let ahead = if t > 0 { t.wrapping_add(t) } else { 0 };
            vec![object(CAGE, ahead.wrapping_sub(CAGE_DEPTH), 0, 0)]
        }
        // F1092: docked, the bay behind the cabin.
        DOCKED => vec![object(BAY, 0, 0, 0)],
        _ => return None,
    })
}

/// Where F1070 puts an object in view space: x zero, y below the eye by the eye's
/// height (words from the ship's statistics, times 8), z the depth less the eye's
/// forward offset. The low dwords the original stores, as signed values.
pub fn view_position(object: &Object, eye: [i16; 2]) -> [i32; 3] {
    [
        0,
        0i32.wrapping_sub(i32::from(eye[1]) << 3),
        object.depth.wrapping_sub(i32::from(eye[0]) << 3),
    ]
}
