//! The flyable craft: original model table slots 14..=70, the ship range of the
//! original's model name table (`DATA_004682`, cache v11). Slots named
//! "Unknown", or holding text rather than a name, are left out, as are craft the
//! viewer cannot draw on their own.
use crate::model_viewer::ModelViewer;
use ffe_content::VerifiedCache;

/// First and last model table slots of the original's ship range.
pub const FIRST_SHIP: u32 = 14;
pub const LAST_SHIP: u32 = 70;
/// The Eagle long range fighter (slot 20), used when a save names no ship.
pub const DEFAULT_SHIP: u32 = 20;

/// The fields of a model's 48-byte statistics block (descriptor +0x38, cache v12)
/// that object set-up (the routine at 0x125c0 in floppy 1.02) copies onto a ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipStats {
    /// +0x0, the forward thrust limit (object +0xc4).
    pub forward_thrust: i16,
    /// +0x2, the reverse thrust limit (object +0xc6; negated for +0xbc/+0xc0).
    pub reverse_thrust: i16,
    /// +0x4, gun mounts (object +0xd1, compared as a signed byte); F1 offers the
    /// turret views above 2, and the bottom turret from 4.
    pub gun_mounts: i8,
    /// +0x6, hull mass in tonnes (object +0xe4 holds it times 4).
    pub mass: u16,
    /// +0x8, the cargo space (tonnes, signed word) before the hyperdrive's mass.
    pub cargo: i16,
    /// +0x12, missile mounts.
    pub missiles: u16,
    /// +0x14, the hyperdrive word: negative means a drive of the class in its low byte.
    pub drive: i16,
    /// +0x18/+0x1a, the cockpit eye: along the forward axis, along the up axis
    /// (FUNC_001054 front and rear views).
    pub eye: [i16; 2],
    /// +0x28/+0x2a, the top turret (view 2; the eye is 0x3e above it).
    pub top_turret: [i16; 2],
    /// +0x2c/+0x2e, the bottom turret (view 3; the eye is 0x3e below it).
    pub bottom_turret: [i16; 2],
}

impl ShipStats {
    /// `None` for an all-zero block (a slot without statistics).
    pub fn from_block(block: &[u8]) -> Option<Self> {
        let word = |offset: usize| u16::from_le_bytes([block[offset], block[offset + 1]]);
        (block.len() == 48 && block.iter().any(|b| *b != 0)).then(|| Self {
            forward_thrust: word(0) as i16,
            reverse_thrust: word(2) as i16,
            gun_mounts: block[4] as i8,
            mass: word(6),
            cargo: word(8) as i16,
            missiles: word(0x12),
            drive: word(0x14) as i16,
            eye: [word(0x18) as i16, word(0x1a) as i16],
            top_turret: [word(0x28) as i16, word(0x2a) as i16],
            bottom_turret: [word(0x2c) as i16, word(0x2e) as i16],
        })
    }

    /// The limits object set-up gives the ship (the routine at 0x125c0): z from
    /// the reverse limit up to the forward one, x and y symmetric at the
    /// reverse limit's magnitude.
    pub fn limits(&self) -> ffe_core::thrust::Limits {
        let side = self.reverse_thrust.wrapping_neg();
        ffe_core::thrust::Limits {
            axes: [
                [side, self.reverse_thrust],
                [side, self.reverse_thrust],
                [self.forward_thrust, self.reverse_thrust],
            ],
        }
    }

    /// The object's z thrust word (+0xba, model global 15) for a throttle in
    /// -1..=1. F600 scales the requested thrust to fit the limits, so full
    /// throttle gives the forward limit and full reverse the (negative) reverse
    /// limit; between them the word is proportional.
    pub fn thrust_word(&self, throttle: f64) -> u16 {
        let throttle = throttle.clamp(-1.0, 1.0);
        let limit = if throttle >= 0.0 {
            f64::from(self.forward_thrust)
        } else {
            -f64::from(self.reverse_thrust)
        };
        ((throttle * limit) as i16) as u16
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShipList {
    ships: Vec<(u32, String)>,
}

impl ShipList {
    /// Ships with an original name (cache v11) and a model the viewer can draw.
    /// Empty without the name table.
    pub fn from_cache(cache: &VerifiedCache, models: &ModelViewer) -> Self {
        let Some(table) = cache.asset("base:text/model-names") else {
            return Self::default();
        };
        let names: Vec<String> = table
            .split(|b| *b == 0)
            .map(|name| name.iter().map(|b| char::from(*b)).collect())
            .collect();
        let ships = (FIRST_SHIP..=LAST_SHIP)
            .filter_map(|index| {
                let name = names.get(index as usize)?.trim();
                let readable =
                    name.chars().next().is_some_and(char::is_alphabetic) && name != "Unknown";
                let drawable = models.find(index).and_then(|m| m.radius()).is_some();
                (readable && drawable).then(|| (index, name.to_owned()))
            })
            .collect();
        Self { ships }
    }

    pub fn from_ships(ships: Vec<(u32, String)>) -> Self {
        Self { ships }
    }

    pub fn is_empty(&self) -> bool {
        self.ships.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ships.len()
    }

    pub fn name(&self, index: u32) -> Option<&str> {
        self.ships
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, name)| name.as_str())
    }

    /// The ship `step` places after `current` in table order, wrapping; the first
    /// ship when `current` is not in the list.
    pub fn step(&self, current: u32, step: i32) -> Option<u32> {
        let count = self.ships.len() as i32;
        if count == 0 {
            return None;
        }
        let position = self.ships.iter().position(|(i, _)| *i == current);
        let next = match position {
            Some(position) => (position as i32 + step).rem_euclid(count),
            None => 0,
        };
        Some(self.ships[next as usize].0)
    }
}
