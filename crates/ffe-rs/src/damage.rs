//! Player subsystem damage, the starport's repair table and F938/F929.
//!
//! The original keeps the six thruster limit words at `ship+0xbc..+0xc6`
//! (Left/Right/Bottom/Top/Main/Reverse, from `DATA_008935`'s name table), the
//! damaged general-equipment bits at `ship+0xcc`, the service counters at
//! `DATA_008923`/`DATA_008924` and their dates at `DATA_008925`/`DATA_008926`.
//! `F953` calls `F952` on a big enough hull hit; `F950`'s roll then breaks an
//! item (`F945`/`F933`), the drive (`F947`/`F948`) or one thruster (`F949`).
//! `F938` recharges shields and the hull auto-repair; `F929` spends thrust
//! energy and burns fuel. See `docs/notes/combat.md` §2.3 and
//! `docs/notes/station.md`.
use ffe_core::npc::Random;
use ffe_core::thrust::Limits;
use serde::{Deserialize, Serialize};

use crate::ships::ShipStats;

/// The six damaged-thruster names (`0x99ae..0x99b3`): `DATA_008935`'s words
/// and the `ship+0xbc..+0xc6` limits in this order.
pub const THRUSTER_NAMES: [&str; 6] = ["Left", "Right", "Bottom", "Top", "Main", "Reverse"];

/// F450/F451/F452 reset the service counters to 500 days.
pub const SERVICE_DAYS: u32 = 500;
/// F929 adds this much energy for each tonne of fuel burnt (`0x20000000`).
pub const ENERGY_PER_TONNE: u32 = 0x2000_0000;
/// F444's per-click hull repair adds one hull point (250 kg).
pub const HULL_QUARTER_COST: u32 = 10;
/// F444's thruster repair row (`0x9814`) costs 100 whole credits.
pub const THRUSTER_REPAIR_COST: u32 = 100;
/// F1033's scoop distance (`0x1c9c380` mm = 30 km) and speed (`0x2328`).
pub const SCOOP_DISTANCE: i64 = 0x01c9_c380;
pub const SCOOP_SPEED: i32 = 0x2328;
/// F1033 sets `+0x1e8 = 0x2468a` after a successful scoop (about three
/// seconds of tics). The recovered build has no matching decrement, so the
/// port carries the cooldown itself.
pub const SCOOP_COOLDOWN: f64 = 0x2468a as f64 / crate::original_flight::TICS_PER_SECOND;

/// The damaged-equipment bitfield and the thruster/service words. Saves from
/// before the damage model load with no damage, a full set of thrusters when
/// the ship's statistics are seen, and fresh service counters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Damage {
    /// `ship+0xcc`: the general-equipment bits that are damaged (F933/F446).
    #[serde(default)]
    pub equipment: u32,
    /// `ship+0xbc..+0xc6`, in [`THRUSTER_NAMES`]' order. `None` until the
    /// ship's statistics are known; a zeroed word is a destroyed thruster
    /// (F949).
    #[serde(default)]
    pub thrusters: Option<[i16; 6]>,
    /// F929's engine failure: the six object words are zero, but the repair
    /// list reads `DATA_008935`, so this is separate from a damaged thruster.
    #[serde(default)]
    pub engine_failed: bool,
    /// `player+0xc6` bit 0: F929's "Internal Fuel Tank Empty" warning latch.
    #[serde(default)]
    pub fuel_empty: bool,
    /// `DATA_008923`/`DATA_008924`: days until the next servicing failure roll.
    #[serde(default = "default_service_counters")]
    pub service_counters: [u32; 2],
}

fn default_service_counters() -> [u32; 2] {
    [SERVICE_DAYS; 2]
}

impl Default for Damage {
    fn default() -> Self {
        Self {
            equipment: 0,
            thrusters: None,
            engine_failed: false,
            fuel_empty: false,
            service_counters: default_service_counters(),
        }
    }
}

impl Damage {
    /// Object set-up (the routine at 0x125c0) fills the six limits from the
    /// statistics block: z = forward/reverse, x and y = ±reverse.
    pub fn ensure_thrusters(&mut self, stats: &ShipStats) {
        if self.thrusters.is_none() {
            let limits = stats.limits().axes;
            self.thrusters = Some([
                limits[0][0],
                limits[0][1],
                limits[1][0],
                limits[1][1],
                limits[2][0],
                limits[2][1],
            ]);
        }
    }

    /// The object's limits from its six words (F598/F600 read these).
    pub fn limits(&self) -> Option<Limits> {
        self.thrusters.map(|thrusters| Limits {
            axes: [
                [thrusters[0], thrusters[1]],
                [thrusters[2], thrusters[3]],
                [thrusters[4], thrusters[5]],
            ],
        })
    }

    /// Which of the six rows F444's list shows (a zero word is destroyed).
    /// An uninitialised save has no damage rows; the words are filled from the
    /// statistics on the first flight frame.
    pub fn damaged_thrusters(&self) -> [bool; 6] {
        let Some(words) = self.thrusters else {
            return [false; 6];
        };
        std::array::from_fn(|index| words[index] == 0)
    }

    /// F949: zero one thruster limit word.
    pub fn break_thruster(&mut self, index: usize) -> bool {
        match &mut self.thrusters {
            Some(words) if index < 6 => {
                words[index] = 0;
                true
            }
            _ => false,
        }
    }

    /// F449: restore one thruster from the ship's statistics (`esi`'s
    /// descriptor words +0x0/+0x2).
    pub fn repair_thruster(&mut self, index: usize, stats: &ShipStats) {
        self.ensure_thrusters(stats);
        let limits = stats.limits().axes;
        let words = [
            limits[0][0],
            limits[0][1],
            limits[1][0],
            limits[1][1],
            limits[2][0],
            limits[2][1],
        ];
        if let (Some(current), Some(restored)) = (&mut self.thrusters, words.get(index)) {
            current[index] = *restored;
        }
    }

    /// F933's roll: up to 30 draws of `F922(0x1d)` into the swapped general
    /// dword; the first fitted bit found becomes damaged and un-fitted. Returns
    /// the general bit.
    pub fn break_equipment(&mut self, general: &mut u32, rng: &mut Random) -> Option<u8> {
        if *general == 0 {
            return None;
        }
        for _ in 0..30 {
            let code = rng.bounded(0x1d) as u8;
            let bit = (code.wrapping_add(0x10)) & 0x1f;
            if *general & (1 << bit) != 0 {
                *general &= !(1 << bit);
                self.equipment |= 1 << bit;
                return Some(bit);
            }
        }
        None
    }

    /// F446: clear a damaged bit and fit the item again.
    pub fn repair_equipment(&mut self, bit: u8, general: &mut u32) -> bool {
        let mask = 1u32 << (bit & 0x1f);
        if self.equipment & mask == 0 {
            return false;
        }
        self.equipment &= !mask;
        *general |= mask;
        true
    }
}

/// A subsystem failure, with the original's message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    /// `0x98c9`: `ATTENTION: <item> Damaged` (F933 from F945).
    EquipmentDamaged(u8),
    /// `0x98ca`: `ATTENTION: Drive Damaged` (F947/F948).
    DriveDamaged,
    /// `0x98cb`: `ATTENTION: <thruster> Thruster Damaged` (F949).
    ThrusterDamaged(u8),
    /// `0x98cd`: `<item> Failure due to Lack of Servicing` (F934).
    EquipmentFailure(u8),
    /// `0x98cc`: `ATTENTION: Drive Failure due to Lack of Servicing` (F934).
    DriveFailure,
}

impl SystemEvent {
    pub fn message(self) -> String {
        let item = |bit: u8| {
            ffe_core::equipment::general_for_bit(bit).map_or("Equipment", |item| item.name)
        };
        match self {
            Self::EquipmentDamaged(bit) => {
                format!("ATTENTION: {} Damaged", item(bit))
            }
            Self::DriveDamaged => "ATTENTION: Drive Damaged".to_owned(),
            Self::ThrusterDamaged(index) => format!(
                "ATTENTION: {} Thruster Damaged",
                THRUSTER_NAMES
                    .get(usize::from(index))
                    .copied()
                    .unwrap_or("")
            ),
            Self::EquipmentFailure(bit) => {
                format!("{} Failure due to Lack of Servicing", item(bit))
            }
            Self::DriveFailure => "ATTENTION: Drive Failure due to Lack of Servicing".to_owned(),
        }
    }
}

/// F1033's scoop gate: a star contact within 30 km while moving at least
/// `0x2328` speed units. `distance` is F569's contact distance in millimetres.
pub fn scoop_ready(distance: i64, speed_word: i32, fitted: bool) -> bool {
    fitted && (0..=SCOOP_DISTANCE).contains(&distance) && speed_word >= SCOOP_SPEED
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> ShipStats {
        ShipStats {
            forward_thrust: 0x100,
            reverse_thrust: -0x80,
            gun_mounts: 1,
            mass: 25,
            cargo: 20,
            shield_strength: 1,
            missiles: 2,
            drive: -10,
            eye: [0; 2],
            front_gun: [0; 2],
            rear_gun: [0; 2],
            top_turret: [0; 2],
            bottom_turret: [0; 2],
        }
    }

    #[test]
    fn the_six_words_come_from_the_statistics_in_name_order() {
        let mut damage = Damage::default();
        damage.ensure_thrusters(&stats());
        assert_eq!(
            damage.thrusters,
            Some([0x80, -0x80, 0x80, -0x80, 0x100, -0x80])
        );
        assert_eq!(damage.limits().unwrap().axes[2], [0x100, -0x80]);
        assert!(!damage.damaged_thrusters().iter().any(|damaged| *damaged));
        assert!(damage.break_thruster(4));
        assert_eq!(damage.thrusters.unwrap()[4], 0);
        assert!(damage.damaged_thrusters()[4]);
        damage.repair_thruster(4, &stats());
        assert_eq!(damage.thrusters.unwrap()[4], 0x100);
    }

    #[test]
    fn f933_breaks_a_fitted_general_item_and_the_repair_restores_it() {
        // Bit 0 (Laser Cooling Booster) and bit 20 (Fuel Scoop) fitted.
        let mut general = (1 << 0) | (1 << 20);
        let mut damage = Damage::default();
        let mut rng = Random::new(0x1234_5678, 0x9e37_79b9);
        let bit = loop {
            if let Some(bit) = damage.break_equipment(&mut general, &mut rng) {
                break bit;
            }
            // The draw stream can miss both bits in 30 tries; keep drawing.
            assert_eq!(damage.equipment, 0);
        };
        assert!(bit == 0 || bit == 20);
        assert_eq!(general & (1 << bit), 0);
        assert_ne!(damage.equipment & (1 << bit), 0);
        assert!(damage.repair_equipment(bit, &mut general));
        assert_ne!(general & (1 << bit), 0);
        assert_eq!(damage.equipment, 0);
    }

    #[test]
    fn the_repair_costs_are_two_and_a_half_times_the_price() {
        let scoop = ffe_core::equipment::equipment()
            .iter()
            .find(|item| item.code == 0x04)
            .unwrap();
        assert_eq!(scoop.name, "Fuel Scoop");
        assert_eq!(ffe_core::equipment::repair_cost(scoop), 875);
        let hull = ffe_core::equipment::general_for_bit(31).unwrap();
        assert_eq!(hull.name, "Hull Auto-Repair System");
        assert_eq!(ffe_core::equipment::repair_cost(hull), 4375);
        assert_eq!(ffe_core::equipment::general_for_bit(16), None);
    }

    #[test]
    fn the_scoop_gate_is_thirty_km_and_2328_speed_units() {
        assert!(scoop_ready(SCOOP_DISTANCE, SCOOP_SPEED, true));
        assert!(!scoop_ready(SCOOP_DISTANCE + 1, SCOOP_SPEED, true));
        assert!(!scoop_ready(SCOOP_DISTANCE, SCOOP_SPEED - 1, true));
        assert!(!scoop_ready(SCOOP_DISTANCE, SCOOP_SPEED, false));
        assert!(!scoop_ready(-1, SCOOP_SPEED, true));
    }
}
