//! NPC ships in system flight: the original's ship-type tables, its traffic
//! spawn counts, and the flight/combat AI states it runs them through.
//!
//! Source: JJFFE 2.8a7 `ffe.asm`/`ffedat.asm` (the DOS release's `FIRSTENC.EXE`
//! has the same tables and routines; the one behavioural difference found is
//! noted at F724 below).
//!
//! - **F635** (`ffe.asm:43576`) picks a ship model: `DATA_006199` is a table of
//!   nineteen `(count, model-list)` records and `RND(count)` selects an entry.
//!   Types 0 and 1 are shifted by `2 * DATA_008996` (the system allegiance
//!   index), so a trader in an Imperial system comes from the Imperial shipyard
//!   list (type 1 + 2*2 = 5).
//! - **F688** (`ffe.asm:45818`) is the system traffic spawner. It reads the
//!   four bytes and two words F869 returns from the galaxy record (`+5..+9`,
//!   `+46`, `+50`) and spawns traders (F689, type 1), Thargoids (F690, type
//!   0x11), ships in station bays (F691), shuttles (F699, type 0xf) and patrol
//!   groups (F698). `DATA_008997` (ships in the cache's ship-data table) is
//!   zero in this port, so the bay branch is absent.
//! - **The Thargoid count is `RND(F35(0x16))`** (`ffe.asm:45858`, a
//!   global-variable read; F34/F35 are `ffe.asm:3507`/`3524`). `F35` is a
//!   limit, not the exact count: F688 passes it to F922 like the trader counts.
//!   It is not a system field: the only setter is the mission-script bytecode's
//!   variable assignment (F37, `ffe.asm:3646`, calling F34), and the 71-dword
//!   block is saved with the commander (`F65`, `ffe.asm:6898`). No ported system
//!   loads a mission script, so the normal limit is zero; [`System::thargoids`]
//!   carries it when a mission hook supplies one. F690 itself is F689 with the
//!   type byte 0x11 and the model list `DATA_006197`; it has no military, system
//!   or random (beyond the count draw) gate.
//! - **F694/F695/F696/F693** (F698's four patrol choices) are the pirate
//!   spawns: a lone type-0xa ship in state 5, a type-0xb leader with two
//!   formation escorts in state 0xb (+0x102..+0x106 hold their offsets from the
//!   leader), and two further variants. F697 fits the pirate equipment and
//!   rolls `+0x11a`, the bounty, from `DATA_006268`.
//! - **F702/F1507** (`ffe.asm:46868`, `89917`) place a spawned ship at a random
//!   phase on an orbit of the raw ffp word `0x5000/0x33`.
//! - **F773** (`ffe.asm:50526`) creates the ship: type byte `+0x118`, equipment,
//!   drive, hull (`+0xe4 = mass * 4`) and shields (`+0xe2 = random * 16`).
//! - **F953/F942** (`ffe.asm:71190`, `70381`) apply damage and destroy the ship;
//!   F1536 (`ffe.asm:91274`) draws model 0x9d (157) at a ship whose shields
//!   absorbed a hit.
//! - **The AI table** is `DATA_006258` (`ffedat.asm:9862`): 0x25 states, four
//!   routine slots each, dispatched by F706/F704/F778/F780 on object `+0xff`.
//!   [`AI_STATES`] is that table. The routines the port models are the common
//!   movement F725/F727 (F1512 aims, F585 applies the limited turn, F587
//!   computes the rates), the arrival test F753, the pirate attack states 5/0xa/
//!   0xb (F759/F751/F757) and the jump-out states 0x1f/0x21 (F787/F747).
//! - **F931** (`ffe.asm:69550`) is the AI's laser too: F724 (state 1) calls it
//!   from the ship's front mount, and F930/F953 give the damage the player
//!   takes. JJFFE patched that call out ("Laser invulnerability", `ffe.asm:
//!   47856`) and only sets the fired flag; the port follows DOS FFE and fires.
//! - **F674/F675/F680** (`ffe.asm:45338`, `45374`, `45536`) persist the ships
//!   that survive a hyperspace jump. F675 keeps a pirate (class 0xfb) only when
//!   it is within 0x10 of the player and keeps escape pods (model 0x9a); F680
//!   restores them and then F688 spawns the new system's traffic.
//! - **F927** (`ffe.asm:69170`) allocates a physics slot from 0x72 down, never
//!   below 0x14, recycling the highest explosion/debris slot when full.
//! - **F1035/F1036** (`ffe.asm:76306`, `76393`) are *surface* spawns (they test
//!   `DATA_008867`, a planet altitude, and spawn at state 0x19 near a building
//!   model 0x72). They are not the orbital traffic and are not ported here.
//!
//! **What this module models and what it leaves out.** The model and type tables,
//! the F688 counts and the spawn roles are the original's. The flight AI is a
//! state machine over the original's own states (approach/attack/pass/hold/flee/
//! dead) with its turn limiter, braking rule and laser gates; it is not a
//! transcription of every state routine. The police response is a reconstruction
//! from `DATA_008646/8647` (the wanted/alert words) because the original's
//! police spawn path is the same F688 count as the traffic. Docking, station
//! bays, missiles and ECM remain. Thargoid traffic exists as the F690 rule and
//! role, but the source's count variable is mission-script driven (see above),
//! so no ordinary system spawns any. Placement uses the original's rule (a
//! random phase on an orbit around an anchor) with a radius in the port's unit
//! system that keeps the ships reachable. See `docs/notes/npc.md`.

use crate::galaxy::Details;
use crate::laser::{self, Damage, DamageState};
use crate::model_animation::TICS_PER_FRAME;
use serde::{Deserialize, Serialize};

/// The original's ship-type table, `DATA_006199`/`DATA_006180..6198`: nineteen
/// records of a count and a model list, in table order.
pub const SHIP_TYPES: [&[u32]; 19] = [
    // 0x0: DATA_006184, nine entries
    &[0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x24, 0x22, 0x2a, 0x28],
    // 0x1: DATA_006185, eighteen (traders)
    &[
        0x1c, 0x1d, 0x1e, 0x22, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2c, 0x35, 0x29, 0x2f, 0x39, 0x3b,
        0x3d, 0x41, 0x44,
    ],
    // 0x2: DATA_006182, ten (Imperial military)
    &[0x19, 0x13, 0x1e, 0x13, 0x22, 0x20, 0x20, 0x22, 0x38, 0x38],
    // 0x3: DATA_006183, eight (Imperial shipyard)
    &[0x17, 0x19, 0x38, 0x38, 0x3a, 0x3a, 0x41, 0x41],
    // 0x4: DATA_006180, twelve (Federal military)
    &[
        0x17, 0x18, 0x1f, 0x14, 0x21, 0x15, 0x16, 0x2b, 0x1e, 0x22, 0x23, 0x27,
    ],
    // 0x5: DATA_006181, sixteen (Federal shipyard)
    &[
        0x1d, 0x1e, 0x22, 0x24, 0x26, 0x28, 0x27, 0x37, 0x35, 0x3c, 0x2d, 0x36, 0x43, 0x2c, 0x3e,
        0x44,
    ],
    // 0x6: DATA_006186, twenty-three
    &[
        0x1f, 0x1a, 0x1d, 0x14, 0x1d, 0x1e, 0x14, 0x17, 0x18, 0x1f, 0x2b, 0x22, 0x23, 0x2a, 0x22,
        0x24, 0x23, 0x2a, 0x30, 0x30, 0x28, 0x29, 0x29,
    ],
    // 0x7: DATA_006187, twenty-seven
    &[
        0x1d, 0x1f, 0x24, 0x1f, 0x2a, 0x2c, 0x2a, 0x26, 0x27, 0x28, 0x2c, 0x29, 0x2f, 0x40, 0x2f,
        0x29, 0x2d, 0x39, 0x2d, 0x3b, 0x43, 0x3d, 0x41, 0x43, 0x44, 0x41, 0x44,
    ],
    // 0x8: DATA_006188, eleven (Thargoids)
    &[
        0x31, 0x31, 0x32, 0x32, 0x33, 0x33, 0x3f, 0x3f, 0x3f, 0x3f, 0x3f,
    ],
    // 0x9: DATA_006189, twelve (Thargoids and a transporter)
    &[
        0x45, 0x31, 0x31, 0x32, 0x32, 0x33, 0x33, 0x3f, 0x3f, 0x3f, 0x3f, 0x3f,
    ],
    // 0xa: DATA_006190, fifteen (single pirates)
    &[
        0x1c, 0x2b, 0x22, 0x1d, 0x24, 0x26, 0x25, 0x2a, 0x30, 0x28, 0x29, 0x2f, 0x27, 0x38, 0x41,
    ],
    // 0xb: DATA_006191, twenty-four (grouped pirates)
    &[
        0x13, 0x14, 0x1f, 0x21, 0x20, 0x15, 0x16, 0x17, 0x18, 0x19, 0x21, 0x1b, 0x2b, 0x1e, 0x1a,
        0x22, 0x23, 0x23, 0x2a, 0x2a, 0x1c, 0x1d, 0x24, 0x30,
    ],
    // 0xc: DATA_006192, ten
    &[0x20, 0x1d, 0x23, 0x24, 0x2a, 0x26, 0x25, 0x27, 0x27, 0x28],
    // 0xd: DATA_006193, two (shuttle and lifter)
    &[0x0f, 0x12],
    // 0xe: DATA_006194, four (police)
    &[0x1e, 0x1e, 0x22, 0x22],
    // 0xf: DATA_006195, three (Lynx, LRC)
    &[0x46, 0x46, 0x47],
    // 0x10: DATA_006196, twenty (assassination targets)
    &[
        0x37, 0x40, 0x2c, 0x2f, 0x35, 0x3c, 0x36, 0x3e, 0x39, 0x2d, 0x3b, 0x3d, 0x43, 0x3e, 0x43,
        0x38, 0x41, 0x3a, 0x41, 0x44,
    ],
    // 0x11: DATA_006197, twelve (Thargoids and a transporter)
    &[
        0x45, 0x31, 0x31, 0x32, 0x32, 0x33, 0x33, 0x3f, 0x3f, 0x3f, 0x3f, 0x3f,
    ],
    // 0x12: DATA_006198, one
    &[0x11],
];

/// One row of the original's AI state table, `DATA_006255` (`ffedat.asm:9854`):
/// four routine slots per state, `0x25` rows.
///
/// The slots are `(control, movement, notify, damage)`: `F706` (event 6, the
/// per-frame controller) indexes `DATA_006255`, `F704` (event 5, the physics
/// step) indexes `DATA_006256`, `F780` (another object's death) indexes
/// `DATA_006257` and `F778` (this object taking a hit) indexes `DATA_006258`.
/// The three labels between `DATA_006255` and `DATA_006258` are the first row's
/// remaining slots, not separate tables: the row data is
/// `F742, F727, 0, F789` for state 0. A `None` slot is a `dd 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiState {
    /// `object+0xff`.
    pub code: u8,
    /// What the original uses the state for.
    pub name: &'static str,
    /// F706's event-6 controller.
    pub control: Option<u16>,
    /// F704's event-5 physics step.
    pub movement: Option<u16>,
    /// F780's death notification for the target this ship wants dead.
    pub notify: Option<u16>,
    /// F778's response to a hit on this ship (F785-F790).
    pub damage: Option<u16>,
}

/// `DATA_006255`: the AI states, in `object+0xff` order.
///
/// The first rows are the flight cycle: F742's navigation (0x00), F752/F753's
/// attack turns (0x01/0x02), F755/F756's evasion (0x03), F750's interception
/// (0x04), F749's reposition (0x05), F759's static hold (0x06), F736's departure
/// (0x07) and F741's new target after a jump (0x08). F770 (0x09) is the trading
/// approach, F743 (0x0a) the patrol/station-keeping offset and F751 (0x0b) the
/// pirate formation. Rows 0x10-0x12 are F760's trading timer, F737's jump
/// countdown and F740's jump exit; rows 0x1d/0x1e are the hyperspace clouds.
///
/// The station cycle is 0x1f F734 (request launch), 0x20 F747 (docked: mark the
/// object and wait for traffic control), 0x21 F735 (launching: wait for the
/// station's door, then state 7), 0x23 F746 (final approach and pad contact,
/// with F745's F655 request) and 0x24 F733 (docked: wait out the object time,
/// then request launch). 0x22 F748 is the player autopilot's launch check.
pub const AI_STATES: [AiState; 0x25] = [
    AiState {
        code: 0x00,
        name: "navigation",
        control: Some(742),
        movement: Some(727),
        notify: None,
        damage: Some(789),
    },
    AiState {
        code: 0x01,
        name: "attack turn",
        control: Some(752),
        movement: Some(725),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x02,
        name: "firing",
        control: Some(752),
        movement: Some(724),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x03,
        name: "evade",
        control: Some(755),
        movement: Some(725),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x04,
        name: "intercept",
        control: Some(750),
        movement: Some(727),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x05,
        name: "reposition",
        control: Some(749),
        movement: Some(727),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x06,
        name: "static",
        control: Some(759),
        movement: Some(725),
        notify: None,
        damage: Some(790),
    },
    AiState {
        code: 0x07,
        name: "depart station",
        control: Some(736),
        movement: Some(727),
        notify: None,
        damage: Some(788),
    },
    AiState {
        code: 0x08,
        name: "new target",
        control: Some(741),
        movement: Some(727),
        notify: None,
        damage: Some(789),
    },
    AiState {
        code: 0x09,
        name: "trading approach",
        control: Some(770),
        movement: Some(727),
        notify: None,
        damage: Some(785),
    },
    AiState {
        code: 0x0a,
        name: "patrol",
        control: Some(743),
        movement: Some(727),
        notify: None,
        damage: Some(789),
    },
    AiState {
        code: 0x0b,
        name: "formation",
        control: Some(751),
        movement: Some(729),
        notify: Some(782),
        damage: Some(785),
    },
    AiState {
        code: 0x0c,
        name: "missile evasion",
        control: Some(757),
        movement: Some(725),
        notify: Some(781),
        damage: None,
    },
    AiState {
        code: 0x0d,
        name: "none",
        control: None,
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x0e,
        name: "parked shuttle",
        control: None,
        movement: None,
        notify: None,
        damage: Some(790),
    },
    AiState {
        code: 0x0f,
        name: "parked police",
        control: None,
        movement: None,
        notify: None,
        damage: Some(790),
    },
    AiState {
        code: 0x10,
        name: "trading timer",
        control: Some(760),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x11,
        name: "jump countdown",
        control: Some(737),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x12,
        name: "jump exit",
        control: Some(740),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x13,
        name: "missile",
        control: Some(758),
        movement: Some(717),
        notify: Some(784),
        damage: None,
    },
    AiState {
        code: 0x14,
        name: "mine",
        control: None,
        movement: Some(716),
        notify: Some(784),
        damage: None,
    },
    AiState {
        code: 0x15,
        name: "missile",
        control: Some(768),
        movement: Some(712),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x16,
        name: "missile",
        control: Some(767),
        movement: Some(713),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x17,
        name: "missile",
        control: Some(732),
        movement: Some(714),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x18,
        name: "cargo",
        control: Some(766),
        movement: Some(710),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x19,
        name: "asteroid",
        control: Some(765),
        movement: Some(711),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1a,
        name: "missile",
        control: None,
        movement: Some(709),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1b,
        name: "space dust",
        control: Some(769),
        movement: Some(715),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1c,
        name: "explosion",
        control: None,
        movement: Some(708),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1d,
        name: "hyperspace cloud",
        control: Some(764),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1e,
        name: "cloud remnant",
        control: Some(763),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x1f,
        name: "request launch",
        control: Some(734),
        movement: Some(727),
        notify: None,
        damage: Some(787),
    },
    AiState {
        code: 0x20,
        name: "docked",
        control: Some(747),
        movement: Some(723),
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x21,
        name: "launching",
        control: Some(735),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x22,
        name: "launch check",
        control: Some(748),
        movement: None,
        notify: None,
        damage: None,
    },
    AiState {
        code: 0x23,
        name: "docking",
        control: Some(746),
        movement: Some(727),
        notify: None,
        damage: Some(790),
    },
    AiState {
        code: 0x24,
        name: "docked countdown",
        control: Some(733),
        movement: None,
        notify: None,
        damage: Some(786),
    },
];

/// `DATA_006255`'s row for a state byte, when the byte is in the table.
pub fn ai_state(code: u8) -> Option<&'static AiState> {
    AI_STATES.get(usize::from(code))
}

/// F922 (`ffe.asm:68974`): the original's bounded random number. `a` and `b`
/// are `DATA_008818`/`DATA_008820`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Random {
    pub a: u32,
    pub b: u32,
}

impl Random {
    pub const fn new(a: u32, b: u32) -> Self {
        Self { a, b }
    }

    /// F922's bounded draw: a new state, then `((a & 0xffff) * limit) >> 16`.
    pub fn bounded(&mut self, limit: u32) -> u32 {
        let b = self.b;
        let rotated = b.rotate_left(5);
        let a = self.a.wrapping_add(b).wrapping_add(rotated);
        self.a = a.rotate_left(16);
        self.b = rotated;
        ((u64::from(self.a & 0xffff) * u64::from(limit)) >> 16) as u32
    }
}

/// Which of the original's traffic roles a ship has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Trader,
    Shuttle,
    Police,
    /// A lone pirate (F694, type 0xa in F772) or a group member (F695, 0xb).
    Pirate,
    Military,
    Thargoid,
}

impl Role {
    /// The F635 type-table index, before the allegiance shift for types 0 and 1.
    pub fn type_code(self) -> usize {
        match self {
            Role::Trader => 0x1,
            Role::Shuttle => 0xd,
            Role::Police => 0xe,
            Role::Pirate => 0xa,
            Role::Military => 0x2,
            Role::Thargoid => 0x11,
        }
    }

    /// F773 writes the role into `+0x118`; the values it uses for the traffic
    /// roles are 1 (traders/station ships), 0xf (shuttles), 0x11 (the F690
    /// branch) and 0xfb (the F697 equipment branch).
    pub fn ai_class(self) -> u8 {
        match self {
            Role::Trader => 0x1,
            Role::Shuttle | Role::Police | Role::Military => 0xf,
            Role::Pirate => 0xfb,
            Role::Thargoid => 0x11,
        }
    }

    pub fn hostile(self) -> bool {
        matches!(self, Role::Pirate | Role::Thargoid)
    }

    /// A ship the law protects: attacking one is a crime (F931's illegal-fire
    /// flag, `DATA_008860` bit 2, is set for any player shot; the wanted state
    /// that makes police react is the port's reconstruction from
    /// `DATA_008646/8647`). Pirates and Thargoids are fair game.
    pub fn clean(self) -> bool {
        matches!(
            self,
            Role::Trader | Role::Shuttle | Role::Police | Role::Military
        )
    }
}

/// F635: the model for a role. `allegiance` is `DATA_008996` (the system
/// record's allegiance word `>> 6`). Out-of-range types fall back to the
/// original's first table so a hand-made system cannot panic.
pub fn model_for(role: Role, allegiance: u32, rng: &mut Random) -> u32 {
    let mut index = role.type_code();
    if index <= 1 {
        index += 2 * (allegiance.min(4) as usize);
    }
    let table = SHIP_TYPES
        .get(index)
        .or_else(|| SHIP_TYPES.first())
        .expect("the table is not empty");
    if table.is_empty() {
        return 0;
    }
    table[rng.bounded(table.len() as u32) as usize]
}

/// The fields of an original ship the NPC needs: its model's radii, its
/// statistics' thrust, mass, shield and cargo words, and its gun mounts. Built
/// by `ffe-rs` from the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub model: u32,
    pub name: String,
    pub radius: i64,
    pub mass: u16,
    pub shield_strength: u16,
    /// The statistics' forward thrust word (`+0x0`, object `+0xc4`).
    pub thrust: i16,
    /// The statistics' drive word's low byte (`+0x14`), the drive class.
    pub drive: u8,
    /// The statistics' gun mounts (`+0x4`).
    pub gun_mounts: i8,
    /// The cargo space (`+0x116`): F942 uses `min(4, cargo >> 1)` extra debris
    /// pods.
    pub cargo: u16,
}

impl Spec {
    /// `+0xe4`: F1533 stores the statistics' mass word times four.
    pub fn hull(&self) -> u16 {
        self.mass.saturating_mul(4)
    }

    /// The shield maximum. F773 gives an AI ship `random * 16` from its equipment
    /// budget; the port uses the model's shield word's low byte times four, which
    /// gives the traffic ships a shield that a 1MW pulse can absorb (see
    /// `docs/notes/npc.md`).
    pub fn shield_max(&self) -> u16 {
        (self.shield_strength & 0xff).saturating_mul(4)
    }

    /// F601's thrust-to-mass ratio sets how a ship accelerates, so the port makes
    /// its top speed proportional to the same ratio. The scale is calibrated on
    /// the new-game Eagle (thrust 13625, mass 25), which keeps the old pirate
    /// speed. Big hulls are slower and turn less sharply, as F585's halving of
    /// the turn limit for models 0x35 and 0x3b says.
    pub fn top_speed(&self) -> f64 {
        let ratio = f64::from(self.thrust.max(0)) / f64::from(self.mass.max(1));
        (SPEED_SCALE * ratio).clamp(MIN_SPEED, MAX_SPEED)
    }

    /// The acceleration that reaches [`Spec::top_speed`] in [`ACCEL_SECONDS`].
    /// The original integrates the thrust word itself (F601); its units are the
    /// original's position units, which do not carry over to the port's scaled
    /// system frame, so the ramp is the port's and the ratio above sets the top.
    pub fn accel(&self) -> f64 {
        self.top_speed() / ACCEL_SECONDS
    }
}

/// Where traffic spawns: a station or planet with its interaction radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub position: [i64; 3],
    pub radius: i64,
    /// The system body's index when the anchor is a place to dock (F689's
    /// `+0x100` destination in `Navigator::bodies` order), so a trader can ask
    /// for a bay there. `None` for a star or a plain orbit.
    pub station: Option<usize>,
}

/// The radius of the orbit a trader or shuttle holds around its anchor. The
/// original's F702 words put a ship on a system-wide orbit; this radius keeps
/// the same "random phase around an anchor" rule but is near enough to see and
/// shoot (the laser's range is [`laser::RANGE`]).
pub const ANCHOR_ORBIT: f64 = 2.5e6;
/// The base distance ahead of the player a pirate group appears at.
pub const PIRATE_DISTANCE: f64 = 3.5e6;
/// Ships farther than this from the player are despawned (F927 recycles slots).
pub const DESPAWN_RANGE: f64 = 30.0e6;
/// F1536's model-0x9d flash stays for this many seconds (the original clears the
/// flag on the next frame; a short visible flash replaces it in the port).
pub const FLASH_SECONDS: f64 = 0.25;
/// How long the port keeps a destroyed ship's marker before removing it.
pub const DEATH_SECONDS: f64 = 0.6;
/// The port caps the original's traffic count so a system stays legible.
pub const MAX_SHIPS: usize = 5;
/// Station traffic's cruise floor. [`Spec::top_speed`]'s thrust/mass ratio
/// clamps a heavy freighter to 2,000 mm/s, which would put F745's five-kilometre
/// approach half an hour away; FFE's own units do not scale to the port's
/// millimetre frame, so traders and shuttles get this floor for the docking
/// cycle. It is a port decision, not a source constant.
pub const STATION_CRUISE_SPEED: f64 = 4.0e4;
/// The acceleration that goes with [`STATION_CRUISE_SPEED`]: a stop from it in
/// 40 m, so the final approach can still brake at the pad.
pub const STATION_ACCEL: f64 = 2.0e4;

/// The original's tics per second at 1x (`DATA_009155` is 994 per 20 ms).
pub const TICS_PER_SECOND: f64 = TICS_PER_FRAME as f64 * 50.0;
/// The base turn rate in radians per second. F585 limits the per-frame turn and
/// halves that limit for models 0x35 and above and quarters it from 0x3b; the
/// base is the port's, calibrated so a Cobra can track a crossing Eagle.
pub const TURN_RATE: f64 = 1.2;
/// The speed scale: `top_speed = 71 * thrust / mass`, which gives the Eagle
/// (13625/25) the 38,800 units/s the traffic slice used.
const SPEED_SCALE: f64 = 71.0;
const MIN_SPEED: f64 = 2_000.0;
const MAX_SPEED: f64 = 80_000.0;
/// Seconds to reach [`Spec::top_speed`].
const ACCEL_SECONDS: f64 = 3.0;
/// The distance a pirate stops closing at (F725/F753's approach distance).
pub const STANDOFF: f64 = 1.2e6;
/// Within this of an orbit or waypoint the ship is there.
const ARRIVE: f64 = 0.35e6;
/// How long a F597 overshoot pass runs before the ship turns back in.
const PASS_SECONDS: f64 = 1.4;
/// How long a ship runs for before F787/F738's jump-out.
const FLEE_SECONDS: f64 = 3.0;
/// A hull at or below this fraction runs (the port's trigger; see the module
/// docs: the original has the jump-out states but no low-hull trigger).
const FLEE_HULL: f64 = 0.25;
/// F675 keeps a pirate within 0x10 (16) of the player across a jump. The
/// original's 0x10 is in its own 32-bit frame; the port's 2,000 km keeps the
/// same "close enough to follow you" intent in its scaled frame.
pub const PERSIST_RANGE: f64 = 2.0e6;

/// F747's docked interval: `[+0xac] = [D8807] + 2` days and
/// `[+0xa8] = [D8804] + [D8818]`, a random fraction of a day in tics. The
/// object time is the original's absolute game clock; the port counts the
/// interval down in game seconds so time acceleration (`Navigator::clock`)
/// shortens the wait exactly as it does in the original.
pub const DOCKED_DAYS: f64 = 2.0;
/// The day fraction F747 adds from `[D8818]` (tics in a day, 49,700 a second).
pub const DOCKED_JITTER_SECONDS: f64 = 86_400.0;
/// How long a trader waits at a station for a bay before F762 picks another
/// station: the source retries F745 every frame for ever, so this give-up is
/// the port's (see `docs/notes/npc.md`, "Owner decisions").
pub const DOCK_WAIT_SECONDS: f64 = 30.0;
/// F745/F734's retry after traffic control refused a request.
pub const DOCK_RETRY_SECONDS: f64 = 1.0;
/// How long F736 flies out of the slot before F761/F762 pick the next station.
/// The original decides in the same frame (it is already at the station); the
/// port gives the departure a short visible run first.
const DEPART_SECONDS: f64 = 1.0;

/// A station as the station-traffic cycle needs it: its centre, docking axis
/// and face (F745's `+0x102..+0x106` target), the bay F650 gave this ship, and
/// the traffic record's clearance and door words. The caller owns the record
/// (`ffe_core::traffic::Starport`) and supplies a snapshot for the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// The system body's index (`Navigator::bodies`).
    pub index: usize,
    pub position: [i64; 3],
    /// The outward docking axis (the object's +z row, F650's frame).
    pub axis: [f64; 3],
    /// Distance from the centre to the docking face (millimetres).
    pub front: f64,
    /// The model's bounding radius (millimetres).
    pub radius: f64,
    /// F650's pad point in the system frame for an open-air starport; indoor
    /// stations have no visible pad, so the port uses the slot instead.
    pub pad: Option<[i64; 3]>,
    /// The bay F650 allocated to this ship (`+0xd0..`), if any.
    pub bay: Option<u16>,
    /// F657: this ship is the record's current one.
    pub cleared: bool,
    /// F652's mode 3: the door is closing behind the ship.
    pub closing: bool,
    /// F865: the door word has reached 0.
    pub door_open: bool,
}

impl Station {
    /// The point F745 aims at: the allocated pad, or just inside the slot of an
    /// indoor station.
    pub fn bay_point(&self) -> [i64; 3] {
        match self.pad {
            Some(pad) => pad,
            None => self.point(self.front * 0.9),
        }
    }

    /// The point `along` from the centre on the docking axis.
    pub fn point(&self, along: f64) -> [i64; 3] {
        std::array::from_fn(|k| self.position[k] + (self.axis[k] * along).round() as i64)
    }

    /// F745/F736's "distance 6" test: the ship asks for a bay at the station's
    /// interaction reach, not at the pad.
    pub fn request_range(&self) -> f64 {
        self.front + self.radius
    }

    /// F1088/F735: where a launch puts the ship, outside the face and hull.
    pub fn launch_point(&self) -> [i64; 3] {
        self.point(self.front + 2.0 * self.radius + 2.0e6)
    }
}

/// A request the station-traffic record must answer for a ship. The caller owns
/// the `Starport` (F649-F657) and applies these; the ship only flies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockRequest {
    /// F655/F654/F650: ask for clearance and a bay.
    Request { ship: u32, station: usize },
    /// F652: the ship has touched the pad (only the current ship may).
    Touch { ship: u32, station: usize },
    /// F747/F653: the door has closed and the handling record is cleared.
    Settle { ship: u32, station: usize },
    /// F654 again: ask traffic control to open the door (F734).
    Launch { ship: u32, station: usize },
    /// F651/F788: free the bay.
    Release { ship: u32, station: usize },
    /// F761/F762: pick the next station, or leave the system when there is none.
    ChooseStation { ship: u32, station: usize },
}

/// The traffic record's ship byte for an NPC id: 0 is an empty bay and 1 is the
/// player (`traffic::PLAYER`), so NPCs start at 2.
pub fn traffic_id(id: u32) -> u8 {
    (id % 250) as u8 + 2
}

/// `DATA_006268` (`ffedat.asm`): the pirate bounty table, in credits. F697 picks
/// an entry with `DATA_006268[(random & 0xf) + offset]`, offset 4 for F694's
/// lone pirate and 2 for F695's group, so a lone pirate can carry 50..150 CR
/// and a group member up to 100.
pub const PIRATE_BOUNTIES: [u32; 22] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 100, 125, 150, 175, 200,
];

/// A ship's AI state in this port. Each variant names the original states it
/// stands in for, so [`State::original`] can be checked against [`AI_STATES`]:
///
/// - `Transit` = 0x00/0x08/0x09 (F742/F741/F770's navigation to the target).
/// - `Approach` = 0x23 (F746's final approach; F745 asks F655 for the bay).
/// - `Docked` = 0x20/0x24 (F747 marks the docked object, F733 waits out F747's
///   two-to-three day object time and requests launch).
/// - `Launch` = 0x1f/0x21 (F734's F654 request, F735 waiting for the door).
/// - `Depart` = 0x07 (F736 flies out, F651 releases the bay, F761/F762 choose
///   the next station or jump out).
/// - `Hold` = 0x06/0x0d/0x0e/0x0f (static holding, F759; the parked shuttle and
///   police states have only their F790 damage response).
/// - `Escort` = 0x0a/0x0b (F743's station-keeping and F751's formation join).
/// - `Attack` = 0x01/0x02/0x04/0x05 (F752/F753 attack, F724 firing, F750
///   interception, F749's transition).
/// - `Pass` = 0x03 (F755/F756's evasion/overshoot).
/// - `Flee` = 0x1d (F761 hides the ship as F738's hyperspace cloud).
/// - `Dead` = 0x1c (F708/F942's explosion object).
/// - `Escaped`: the cloud is gone and F927 frees the slot; no live row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Fly to the station or waypoint, then hold.
    Transit,
    /// Fly the final leg to F650's bay and touch the pad (F745/F746).
    Approach,
    /// In the bay, waiting out F747's docked interval (F733).
    Docked,
    /// Clearance given, waiting for the door to open (F734/F735).
    Launch,
    /// Fly out of the slot and choose the next station (F736/F761).
    Depart,
    /// Circle the spawn anchor (F689/F699's station traffic).
    Hold,
    /// Station-keep in a leader's frame (F695's escorts, F743).
    Escort,
    /// Close on the player (F694/F695's pirates, type 0xa/0xb).
    Attack,
    /// A fast pass through the target: F597's "cannot stop in time" overshoot.
    Pass,
    /// Run from the player and jump out (F787/F738).
    Flee,
    /// Destroyed; the death marker counts down (F942).
    Dead,
    /// Jumped out: the slot is free (F738/F927).
    Escaped,
}

impl State {
    /// The original `+0xff` values this state stands in for.
    pub fn original(self) -> &'static [u8] {
        match self {
            State::Transit => &[0x00, 0x08, 0x09],
            State::Approach => &[0x23],
            State::Docked => &[0x20, 0x24],
            State::Launch => &[0x1f, 0x21],
            State::Depart => &[0x07],
            State::Hold => &[0x06, 0x0d, 0x0e, 0x0f],
            State::Escort => &[0x0a, 0x0b],
            State::Attack => &[0x01, 0x02, 0x04, 0x05],
            State::Pass => &[0x03],
            State::Flee => &[0x1d],
            State::Dead => &[0x1c],
            State::Escaped => &[],
        }
    }

    /// Whether the ship is in the station's docking cycle.
    pub fn docking(self) -> bool {
        matches!(
            self,
            State::Approach | State::Docked | State::Launch | State::Depart
        )
    }
}

/// One NPC ship in the system flight frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcShip {
    /// A stable id for the target lock (the slot index is not stable when a
    /// ship is destroyed and the list is compacted).
    pub id: u32,
    pub model: u32,
    pub role: Role,
    pub name: String,
    /// Position in the system frame, original units (millimetres as the port's
    /// bodies use them).
    pub position: [i64; 3],
    pub velocity: [f64; 3],
    /// Unit forward and up axes; the right axis is `up x forward`.
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub radius: i64,
    pub damage: DamageState,
    /// The statistics' shield word (`+0xe`), which F1099 shows as
    /// "Shield Strength d".
    pub shield_strength: i16,
    /// The ship's cargo space (`+0x116`), which F942 turns into debris pods.
    pub cargo: u16,
    pub state: State,
    pub anchor: [i64; 3],
    pub orbit_radius: f64,
    pub phase: f64,
    /// The top speed from the model's thrust/mass ratio (see [`Spec::top_speed`]).
    pub max_speed: f64,
    /// The acceleration that reaches it (see [`Spec::accel`]).
    pub accel: f64,
    /// The turn limit from F585: [`TURN_RATE`], halved for models 0x35..0x3a and
    /// quartered from 0x3b.
    pub turn_rate: f64,
    /// The current speed along `forward`.
    pub speed: f64,
    /// Whether the ship attacks the player. Pirates and Thargoids start true;
    /// police turn true when the player is wanted.
    pub hostile: bool,
    /// The front laser (`+0xd2`), when the model has a gun mount. The port fits
    /// one 1MW pulse to a combat role; see `docs/notes/npc.md`.
    pub laser: Option<u8>,
    /// F931's mutable words for that laser (`+0xe8` refire, `+0xea` heat,
    /// `+0x11e` energy).
    pub guns: laser::State,
    /// `+0x11a`, the bounty F942 pays when the player kills it.
    pub bounty: u32,
    /// The leader this ship escorts (F695's `+0xfe` points at it) and the offset
    /// in the leader's frame (`+0x102..+0x106`).
    pub leader: Option<u32>,
    pub formation: [i64; 3],
    /// Seconds of a [`State::Pass`] left.
    pub pass: f64,
    /// Seconds of a [`State::Flee`] left before the jump.
    pub flee: f64,
    /// Seconds of the F1536 shield-hit flash left.
    pub flash: f64,
    /// Seconds of the death marker left.
    pub death: f64,
    /// The station body this ship is trading with (F689's `+0x100`, F762's
    /// choice), when it has one.
    pub station: Option<usize>,
    /// F650's bay (`+0xd0..`), held from the request until the departure's F651.
    pub bay: Option<u16>,
    /// The bay's point in the system frame (F650's offset), when the station
    /// supplied one; indoor stations use their slot instead.
    pub pad: Option<[i64; 3]>,
    /// F747's remaining docked object time in game seconds.
    pub dock_timer: f64,
    /// F745/F734's retry delay after traffic control said no.
    pub dock_retry: f64,
    /// Seconds spent waiting for a bay or clearance.
    pub dock_wait: f64,
}

impl NpcShip {
    fn new(
        spec: &Spec,
        role: Role,
        position: [i64; 3],
        anchor: [i64; 3],
        rng: &mut Random,
    ) -> Self {
        let shields = spec.shield_max();
        let phase = (rng.bounded(65_536) as f64) / 65_536.0 * std::f64::consts::TAU;
        let orbit_radius = ANCHOR_ORBIT + f64::from(rng.bounded(1_000_000));
        let hostile = role.hostile();
        Self {
            id: 0,
            model: spec.model,
            role,
            name: spec.name.clone(),
            position,
            velocity: [0.0; 3],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            radius: spec.radius,
            damage: DamageState {
                shields,
                shields_max: shields,
                hull: spec.hull(),
                mass: spec.mass,
            },
            shield_strength: spec.shield_strength as i16,
            cargo: spec.cargo,
            state: if hostile { State::Attack } else { State::Hold },
            anchor,
            orbit_radius,
            phase,
            max_speed: spec.top_speed(),
            accel: spec.accel(),
            turn_rate: TURN_RATE / f64::from(turn_divisor(spec.model)),
            speed: 0.0,
            hostile,
            laser: role.laser_code(spec, rng),
            guns: laser::State {
                refire: 0,
                heat: 0,
                energy: drive_energy(spec.drive),
            },
            bounty: if role == Role::Pirate {
                pirate_bounty(rng, false)
            } else {
                0
            },
            leader: None,
            formation: [0; 3],
            pass: 0.0,
            flee: 0.0,
            flash: 0.0,
            death: 0.0,
            station: None,
            bay: None,
            pad: None,
            dock_timer: 0.0,
            dock_retry: 0.0,
            dock_wait: 0.0,
        }
    }

    pub fn alive(&self) -> bool {
        !matches!(self.state, State::Dead | State::Escaped)
    }

    /// Whether the ship should be drawn, locked or hit this frame. A docked or
    /// just-launched ship is inside the station (F747 hides the object and the
    /// bay is not in the outside view).
    pub fn visible(&self) -> bool {
        self.alive() && !matches!(self.state, State::Docked | State::Launch)
    }

    /// F650/F745: the station gave this ship a bay; fly the final leg to it.
    pub fn begin_approach(&mut self, bay: u16, pad: Option<[i64; 3]>) {
        if matches!(self.state, State::Transit | State::Approach) {
            self.state = State::Approach;
            self.bay = Some(bay);
            self.pad = pad;
            self.dock_retry = 0.0;
            self.dock_wait = 0.0;
        }
    }

    /// F652 refused the touch (another ship is current, or the clearance ran
    /// out): go back to transit and ask again.
    pub fn touch_failed(&mut self) {
        if matches!(self.state, State::Approach | State::Docked) {
            self.state = State::Transit;
            self.bay = None;
            self.pad = None;
            self.dock_timer = 0.0;
            self.dock_retry = DOCK_RETRY_SECONDS;
        }
    }

    /// F747/F733: the ship is in its bay. Hold it at the pad and start the
    /// two-to-three day object time (F747's `+2` days and `[D8818]` fraction).
    /// The `phase` is the ship's spawn-time random draw, so the wait is stable.
    pub fn begin_docked(&mut self, seat: [i64; 3]) {
        self.state = State::Docked;
        self.position = seat;
        self.velocity = [0.0; 3];
        self.speed = 0.0;
        let fraction = self.phase / std::f64::consts::TAU;
        self.dock_timer = DOCKED_DAYS * 86_400.0 + fraction * DOCKED_JITTER_SECONDS;
        self.dock_retry = 0.0;
        self.dock_wait = 0.0;
    }

    /// F735: the door is open and the bay is free; fly out.
    pub fn begin_departure(&mut self, seat: [i64; 3]) {
        self.state = State::Depart;
        self.position = seat;
        self.velocity = [0.0; 3];
        self.speed = 0.0;
        self.bay = None;
        self.pad = None;
        self.dock_timer = 0.0;
    }

    /// F761/F762: head for station `station` and ask for a bay there.
    pub fn begin_transit(&mut self, station: usize, anchor: [i64; 3]) {
        self.state = State::Transit;
        self.station = Some(station);
        self.anchor = anchor;
        self.bay = None;
        self.pad = None;
        self.dock_timer = 0.0;
        self.dock_retry = 0.0;
        self.dock_wait = 0.0;
    }

    /// F925: no station to trade with; leave the system (the HS cloud is the
    /// port's `Escaped` marker).
    pub fn escape(&mut self) {
        self.state = State::Escaped;
        self.velocity = [0.0; 3];
    }

    /// Whether the ship attacks the player. Pirates and Thargoids start true;
    /// police and military turn true when the player is wanted ([`Self::provoke`]).
    pub fn is_hostile(&self) -> bool {
        self.hostile
    }

    /// The player attacked a clean ship: the police response. F783 clears the
    /// original's `DATA_008647` alert when a police ship's target is gone; the
    /// port's counterpart is that police and military ships engage a wanted
    /// player. Traders and shuttles do not fight back.
    pub fn provoke(&mut self) -> bool {
        if !self.alive() || self.hostile || !matches!(self.role, Role::Police | Role::Military) {
            return false;
        }
        self.hostile = true;
        self.state = State::Attack;
        self.pass = 0.0;
        self.flee = 0.0;
        true
    }

    /// The right/up/forward axes, as `screen` and the camera use them.
    pub fn axes(&self) -> [[f64; 3]; 3] {
        [
            normalise(cross(self.up, self.forward)),
            self.up,
            self.forward,
        ]
    }

    /// F953: shields first, then hull. A shield hit sets the F1536 flash flag
    /// (`+0x151 & 1`); F942's destruction removes the ship after its marker.
    pub fn hit(&mut self, damage: u32) -> Damage {
        if !self.alive() {
            return Damage::Hull;
        }
        let result = laser::apply_damage(&mut self.damage, damage);
        match result {
            Damage::Shields => self.flash = FLASH_SECONDS,
            Damage::Hull => {}
            Damage::Destroyed => {
                self.state = State::Dead;
                self.death = DEATH_SECONDS;
                self.flash = 0.0;
            }
        }
        result
    }

    pub fn hull_per_mille(&self) -> i32 {
        laser::hull_per_mille(&self.damage)
    }

    pub fn shield_percent(&self) -> i32 {
        laser::shield_percent(&self.damage)
    }

    /// The remaining hull as a fraction of `mass * 4`.
    pub fn hull_fraction(&self) -> f64 {
        let full = self.damage.mass.saturating_mul(4).max(1);
        f64::from(self.damage.hull.min(full)) / f64::from(full)
    }

    /// One frame of the flight AI: F730/F938's laser upkeep, F585's limited
    /// turn, F1512's aim and F597's "can it stop?" speed rule. `dt` is
    /// accelerated world seconds, `tics` the frame's game tics, `player` the
    /// player's position in the system frame and `escort` an escort's station in
    /// its leader's frame (F695's `+0x102..+0x106`), when it has one.
    pub fn tick(&mut self, dt: f64, tics: u32, player: [i64; 3], escort: Option<[i64; 3]>) {
        let mut requests = Vec::new();
        self.tick_with_station(dt, tics, player, escort, None, &mut requests);
    }

    /// [`Self::tick`] with the station-traffic cycle. `station` is the current
    /// station's snapshot (its face, the bay F650 gave this ship, F657's
    /// clearance and F865's door); requests are appended for the caller to apply
    /// to its `traffic::Starport`.
    pub fn tick_with_station(
        &mut self,
        dt: f64,
        tics: u32,
        player: [i64; 3],
        escort: Option<[i64; 3]>,
        station: Option<&Station>,
        requests: &mut Vec<DockRequest>,
    ) {
        if !self.alive() {
            self.death = (self.death - dt).max(0.0);
            return;
        }
        self.flash = (self.flash - dt).max(0.0);
        laser::tick_refire(&mut self.guns, tics);
        laser::cool(&mut self.guns, dt, false);
        self.dock_retry = (self.dock_retry - dt).max(0.0);
        // The original's jump-out states hide the model and move it out
        // (F738/F787). The low-hull trigger is the port's.
        if self.hostile
            && matches!(self.state, State::Attack | State::Pass)
            && self.hull_fraction() <= FLEE_HULL
        {
            self.state = State::Flee;
            self.flee = FLEE_SECONDS;
        }
        if self.state == State::Flee {
            self.flee -= dt;
            if self.flee <= 0.0 {
                self.state = State::Escaped;
                self.velocity = [0.0; 3];
                return;
            }
        }
        if self.state == State::Pass {
            self.pass -= dt;
            if self.pass <= 0.0 {
                self.state = State::Attack;
            }
        }
        // F747/F733: in the bay. The object stays on its pad while the station's
        // own motion carries it (the pad snapshot is in the system frame), and
        // F733's object time counts down. At zero F734 asks F654 for the door.
        if self.state == State::Docked {
            self.speed = 0.0;
            self.velocity = [0.0; 3];
            let Some(station) = station else {
                self.touch_failed();
                requests.push(DockRequest::ChooseStation {
                    ship: self.id,
                    station: self.station.unwrap_or(0),
                });
                return;
            };
            self.position = station
                .pad
                .or(self.pad)
                .unwrap_or_else(|| station.bay_point());
            // F747 waits for F652's mode 3 to end (the door closed) and then
            // calls F653; only then does F733's object time run.
            if station.closing {
                return;
            }
            if station.cleared && self.bay.is_some() {
                requests.push(DockRequest::Settle {
                    ship: self.id,
                    station: station.index,
                });
                return;
            }
            self.dock_timer = (self.dock_timer - dt).max(0.0);
            if self.dock_timer <= 0.0 {
                self.dock_wait += dt;
                if self.dock_retry <= 0.0 {
                    requests.push(DockRequest::Launch {
                        ship: self.id,
                        station: station.index,
                    });
                    self.dock_retry = DOCK_RETRY_SECONDS;
                }
                // The source retries F654 for ever; after the port's give-up the
                // bay is freed (F651) and F762 picks another station.
                if self.dock_wait >= DOCK_WAIT_SECONDS {
                    requests.push(DockRequest::Release {
                        ship: self.id,
                        station: station.index,
                    });
                    requests.push(DockRequest::ChooseStation {
                        ship: self.id,
                        station: station.index,
                    });
                    self.state = State::Transit;
                    self.bay = None;
                    self.pad = None;
                    self.dock_wait = 0.0;
                }
            }
            return;
        }
        // F734/F735: launch clearance is in the record; wait for the door word
        // (F865) to reach zero, then F651 frees the bay and F736 flies out.
        if self.state == State::Launch {
            self.speed = 0.0;
            self.velocity = [0.0; 3];
            let Some(station) = station else {
                self.touch_failed();
                return;
            };
            self.dock_wait += dt;
            self.position = station
                .pad
                .or(self.pad)
                .unwrap_or_else(|| station.bay_point());
            if station.door_open {
                let seat = station
                    .pad
                    .or(self.pad)
                    .unwrap_or_else(|| station.bay_point());
                requests.push(DockRequest::Release {
                    ship: self.id,
                    station: station.index,
                });
                self.begin_departure(seat);
                self.dock_wait = DEPART_SECONDS;
            } else if self.dock_wait >= DOCK_WAIT_SECONDS {
                // The door never opened: free the bay and pick a new station.
                requests.push(DockRequest::Release {
                    ship: self.id,
                    station: station.index,
                });
                requests.push(DockRequest::ChooseStation {
                    ship: self.id,
                    station: station.index,
                });
                self.state = State::Transit;
                self.bay = None;
                self.pad = None;
                self.dock_wait = 0.0;
            }
            return;
        }
        // F745/F746: the final leg to the pad. A lost clearance sends the ship
        // back to transit (the traffic record timed out in F867).
        if self.state == State::Approach && station.is_some_and(|s| !s.cleared || s.bay.is_none()) {
            self.touch_failed();
        }
        if self.state == State::Transit {
            if let Some(station) = station {
                let range = distance_between(self.position, station.position);
                if range <= station.request_range() {
                    self.dock_wait += dt;
                    if self.dock_retry <= 0.0 {
                        requests.push(DockRequest::Request {
                            ship: self.id,
                            station: station.index,
                        });
                        self.dock_retry = DOCK_RETRY_SECONDS;
                    }
                    if self.dock_wait >= DOCK_WAIT_SECONDS {
                        requests.push(DockRequest::ChooseStation {
                            ship: self.id,
                            station: station.index,
                        });
                        self.dock_wait = 0.0;
                    }
                } else {
                    self.dock_wait = 0.0;
                }
            }
        }
        // The leader's frame gives F695's escorts their station.
        let target = match self.state {
            State::Hold => {
                self.phase += (self.max_speed * 0.35 / self.orbit_radius.max(1.0)) * dt;
                offset(self.anchor, self.phase, self.orbit_radius)
            }
            State::Transit => station.map_or(self.anchor, Station::bay_point),
            State::Approach => station.map_or(self.anchor, |station| {
                station
                    .pad
                    .or(self.pad)
                    .unwrap_or_else(|| station.bay_point())
            }),
            State::Depart => station.map_or(self.anchor, Station::launch_point),
            State::Escort => escort.map_or(player, |point| {
                if distance_between(player, self.position) > 4.0 * STANDOFF {
                    point
                } else {
                    player
                }
            }),
            State::Attack | State::Pass => player,
            State::Flee => {
                let direction = away(self.position, player);
                add(self.position, direction.map(|value| (value * 4.0e6) as i64))
            }
            State::Docked | State::Launch | State::Dead | State::Escaped => self.position,
        };
        let to = sub(target, self.position);
        let distance = length(to);
        let desired = if distance > 1.0 {
            scale(to, 1.0 / distance)
        } else {
            self.forward
        };
        self.forward = turn_toward(self.forward, desired, self.turn_rate * dt);
        self.up = upright(self.forward);
        let wanted = match self.state {
            State::Hold => (self.max_speed * 0.5).min(0.7 * (2.0 * self.accel * distance).sqrt()),
            State::Transit | State::Depart => {
                if distance <= ARRIVE {
                    0.0
                } else {
                    self.max_speed
                        .min(0.7 * (2.0 * self.accel * (distance - ARRIVE)).sqrt())
                }
            }
            State::Approach => self
                .max_speed
                .min(0.7 * (2.0 * self.accel * distance).sqrt()),
            State::Attack => {
                let room = (distance - STANDOFF).max(0.0);
                self.max_speed.min(0.7 * (2.0 * self.accel * room).sqrt())
            }
            State::Escort => {
                let room = (distance - STANDOFF).max(0.0);
                self.max_speed.min(0.7 * (2.0 * self.accel * room).sqrt())
            }
            State::Pass | State::Flee => self.max_speed,
            State::Docked | State::Launch | State::Dead | State::Escaped => 0.0,
        };
        // F597: if the ship cannot stop before the standoff, it commits to the
        // pass (the original's overshoot branch) instead of braking.
        if self.state == State::Attack
            && self.speed > 0.5 * self.max_speed
            && distance < 4.0 * STANDOFF
            && self.speed * self.speed > 2.0 * self.accel * (distance - STANDOFF).max(0.0)
        {
            self.state = State::Pass;
            self.pass = PASS_SECONDS;
        }
        let change = (wanted - self.speed).clamp(-self.accel * dt, self.accel * dt);
        self.speed = (self.speed + change).max(0.0);
        self.velocity = scale(self.forward, self.speed);
        for axis in 0..3 {
            self.position[axis] += (self.velocity[axis] * dt).round() as i64;
        }
        if self.state == State::Transit && distance <= ARRIVE && station.is_none() {
            self.state = State::Hold;
            self.speed = 0.0;
            self.velocity = [0.0; 3];
        }
        // F652: the pad contact. The caller's F652 decides whether this ship is
        // the record's current one, then F747 starts the docked object time.
        if self.state == State::Approach && distance <= ARRIVE {
            if let Some(station) = station {
                requests.push(DockRequest::Touch {
                    ship: self.id,
                    station: station.index,
                });
                self.speed = 0.0;
                self.velocity = [0.0; 3];
            }
        }
        // F736/F761: the departure run is over, so pick the next station or jump
        // out. The original's state 7 is already at the slot and calls F761 at
        // once; the port gives it `DEPART_SECONDS` (or the hull clearance).
        if self.state == State::Depart {
            self.dock_wait -= dt;
            if let Some(station) = station {
                if self.dock_wait <= 0.0
                    || distance_between(self.position, station.position) > station.radius + 2.0e6
                {
                    requests.push(DockRequest::ChooseStation {
                        ship: self.id,
                        station: station.index,
                    });
                    self.station = None;
                    self.state = State::Hold;
                    self.speed = 0.0;
                    self.velocity = [0.0; 3];
                }
            }
        }
    }

    /// F931 for an AI ship: the front laser's gates, then F817's ray against the
    /// player's interaction box. The shot is fired (heat, energy and refire
    /// spent) even when the ray misses, as the original's F931 does. `None` when
    /// the ship has no laser, is not hostile, or F931's gates refuse.
    pub fn fire(&mut self, tics: u32, player: [i64; 3], player_radius: i64) -> Option<Shot> {
        if !self.alive() || !self.is_hostile() {
            return None;
        }
        let code = self.laser?;
        let spec = laser::fire(code, &mut self.guns, tics).ok()?;
        let targets = [laser::Target {
            index: 0,
            centre: player,
            radius: player_radius,
        }];
        let hit = laser::hit(self.position, self.forward, &targets)?;
        Some(Shot {
            ship: self.id,
            name: self.name.clone(),
            code,
            damage: spec.damage,
            beam: spec.beam,
            point: hit.point,
            distance: hit.distance,
        })
    }

    /// F927's slot recycle / the original ship vanishing: beyond
    /// [`DESPAWN_RANGE`] of the player, killed, or jumped out.
    pub fn finished(&self, player: [i64; 3]) -> bool {
        match self.state {
            State::Dead => self.death <= 0.0,
            State::Escaped => true,
            _ => distance_between(self.position, player) > DESPAWN_RANGE,
        }
    }

    /// The hit-test box target for `laser::hit`, in the absolute system frame.
    pub fn target(&self, index: usize) -> laser::Target {
        laser::Target {
            index,
            centre: self.position,
            radius: self.radius,
        }
    }
}

/// An NPC laser hit on the player, from F930/F931 and F817.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// The firing ship's [`NpcShip::id`].
    pub ship: u32,
    pub name: String,
    /// The laser equipment code (`0x88`, ...).
    pub code: u8,
    /// F930's damage, for F953.
    pub damage: u32,
    /// F930's beam branch (the original does not set the refire word for beams).
    pub beam: bool,
    /// Where the ray entered the player's box.
    pub point: [i64; 3],
    pub distance: f64,
}

/// One record of F236's offence table `DATA_005039` (`ffedat.asm:4362-4382`):
/// eleven 14-byte records. `points` is `+0` (added to the per-starport
/// `DATA_008908` word), `fine` is `+4` (the number F236 prints in the message),
/// `witness` is `+8` (the base of F234's starport-witness roll) and `flags`
/// `+0xc`: bit 0 sets the wanted state, bit 1 shows the message and bit 2 asks
/// F234 for a nearby starport before the offence is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offence {
    pub name: &'static str,
    pub points: u32,
    pub fine: u32,
    pub witness: u8,
    pub flags: u8,
}

/// `DATA_005039`, in the original's order. The fines are the `+4` words; F236
/// charges ten times the printed amount into the starport's ledger
/// (`0x50f8 + 4*starport`), while the message shows `fine` itself (the executed
/// original prints "$600" for unlawful discharge and records 6000).
pub const OFFENCES: [Offence; 11] = [
    Offence {
        name: "Failure to obey Traffic Control",
        points: 5,
        fine: 1000,
        witness: 0,
        flags: 3,
    },
    Offence {
        name: "Launching without clearance",
        points: 8,
        fine: 500,
        witness: 0,
        flags: 2,
    },
    Offence {
        name: "Evading payment of fine",
        points: 15,
        fine: 2000,
        witness: 0,
        flags: 3,
    },
    Offence {
        name: "Unlawful discharge of weapon",
        points: 20,
        fine: 600,
        witness: 8,
        flags: 7,
    },
    Offence {
        name: "Illegal Mining",
        points: 20,
        fine: 200,
        witness: 16,
        flags: 7,
    },
    Offence {
        name: "Trading in illegal goods",
        points: 20,
        fine: 0,
        witness: 8,
        flags: 0,
    },
    Offence {
        name: "Attempted bribery",
        points: 25,
        fine: 0,
        witness: 0,
        flags: 0,
    },
    Offence {
        name: "Smuggling illegal goods",
        points: 30,
        fine: 2500,
        witness: 0,
        flags: 2,
    },
    Offence {
        name: "Unlawful dumping",
        points: 40,
        fine: 3000,
        witness: 22,
        flags: 7,
    },
    Offence {
        name: "Permit Violation",
        points: 80,
        fine: 4000,
        witness: 99,
        flags: 7,
    },
    Offence {
        name: "Piracy and murder",
        points: 300,
        fine: 10_000,
        witness: 20,
        flags: 7,
    },
];

/// The index of "Unlawful discharge of weapon" in [`OFFENCES`]: F931/F941's
/// `F237(ship, 3)`.
pub const UNLAWFUL_DISCHARGE: usize = 3;

/// F236's message (`DATA_005040`/`DATA_005041`, `ffedat.asm:4384/4386`). The
/// first offence at a starport uses "is not permitted here"; a later one, when
/// that starport's ledger is already non-zero, uses "can't be tolerated ...
/// Pay an extra".
pub fn offence_message(offence: usize, additional: bool) -> String {
    let record = &OFFENCES[offence];
    if additional {
        format!(
            "{} can't be tolerated. Pay an extra ${} fine.",
            record.name, record.fine
        )
    } else {
        format!(
            "{} is not permitted here. Pay a ${} fine.",
            record.name, record.fine
        )
    }
}

/// What F236 did for one offence, for the caller's message log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffenceReport {
    pub message: String,
    /// The amount printed in the message (`+4`).
    pub fine: u32,
    /// The amount added to the starport's ledger: ten times `fine` (F236's
    /// `lea eax,[eax+eax*4]` after doubling `+4`).
    pub charged: u32,
    /// A later offence at the same starport used the "extra" message.
    pub additional: bool,
}

/// The player's legal standing: the port's reconstruction of `DATA_008646` bit
/// 0 (`wanted`), `DATA_008647` (`alert`, the police chase), `DATA_008860` bit 2
/// (`illegal_fire`, F931/F941's once-only unlawful-discharge report), the
/// offence count, and F236's per-starport fine ledger `DATA_009095` (six
/// dwords indexed by the starport's ordinal, `DATA_008996`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Legal {
    pub wanted: bool,
    pub alert: bool,
    pub illegal_fire: bool,
    pub offences: u32,
    pub fines: [u32; 6],
}

impl Legal {
    /// F931/F941: the player's shot or missile has passed F931's gates. The
    /// first shot in a system (`DATA_008860` bit 2 clear) is reported through
    /// F237(ship, 3); later shots are not. F236 needs a nearby starport (the
    /// caller's `starport` index); without one it returns before the fine and
    /// the wanted bit, leaving the flag set so the system's one report is spent.
    ///
    /// `starport` is the ordinal of the nearest starport in F234's sense, used
    /// as the `DATA_008996` ledger index. `None` means no starport is in range.
    pub fn player_fired(&mut self, starport: Option<usize>) -> Option<OffenceReport> {
        if self.illegal_fire {
            return None;
        }
        self.illegal_fire = true;
        let index = starport?;
        let record = &OFFENCES[UNLAWFUL_DISCHARGE];
        let slot = index % self.fines.len();
        let additional = self.fines[slot] != 0;
        let charged = record.fine.saturating_mul(10);
        self.fines[slot] = self.fines[slot].saturating_add(charged);
        if record.flags & 1 != 0 {
            self.wanted = true;
        }
        self.offences += 1;
        Some(OffenceReport {
            message: offence_message(UNLAWFUL_DISCHARGE, additional),
            fine: record.fine,
            charged,
            additional,
        })
    }

    /// F783: the police object (`+0x118 == 0xe`) has gone. It clears
    /// `DATA_008647` and `DATA_008646` bit 0. The illegal-fire flag
    /// (`DATA_008860` bit 2) is not cleared by F783.
    pub fn clear_alert(&mut self) -> bool {
        let was_set = self.wanted || self.alert;
        self.wanted = false;
        self.alert = false;
        was_set
    }

    /// The fine ledger for a starport, as F655's `owes` argument.
    pub fn owed(&self, starport: Option<usize>) -> u32 {
        starport.map_or(0, |index| self.fines[index % self.fines.len()])
    }
}

/// F675/F680's carried ship: a hostile ship close to the player survives a jump.
/// The offset is from the player, so it can be restored in the new system.
#[derive(Debug, Clone, PartialEq)]
pub struct Persisted {
    pub model: u32,
    pub role: Role,
    pub name: String,
    pub offset: [i64; 3],
    pub velocity: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub radius: i64,
    pub damage: DamageState,
    pub shield_strength: i16,
    /// The statistics' cargo space (`+0x116`), kept for F942's debris on a
    /// later destruction.
    pub cargo: u16,
    pub hostile: bool,
    pub laser: Option<u8>,
    pub guns: laser::State,
    pub bounty: u32,
    /// The model-derived limits, so a restored ship flies and turns like itself.
    pub max_speed: f64,
    pub accel: f64,
    pub turn_rate: f64,
}

/// F675: the ships that follow the player through a jump. The original keeps a
/// pirate (class 0xfb) within 0x10 of the player and escape pods (model 0x9a);
/// the port has no pods and uses [`PERSIST_RANGE`].
pub fn persist(ships: &[NpcShip], player: [i64; 3]) -> Vec<Persisted> {
    ships
        .iter()
        .filter(|ship| ship.alive())
        .filter(|ship| ship.is_hostile())
        .filter(|ship| distance_between(ship.position, player) <= PERSIST_RANGE)
        .map(|ship| Persisted {
            model: ship.model,
            role: ship.role,
            name: ship.name.clone(),
            offset: [
                ship.position[0] - player[0],
                ship.position[1] - player[1],
                ship.position[2] - player[2],
            ],
            velocity: ship.velocity,
            forward: ship.forward,
            up: ship.up,
            radius: ship.radius,
            damage: ship.damage,
            shield_strength: ship.shield_strength,
            cargo: ship.cargo,
            hostile: true,
            laser: ship.laser,
            guns: ship.guns,
            bounty: ship.bounty,
            max_speed: ship.max_speed,
            accel: ship.accel,
            turn_rate: ship.turn_rate,
        })
        .collect()
}

/// F680: restore the carried ships at the player's arrival point, keeping their
/// offset from the player. `first_id` keeps the ids clear of the new system's
/// traffic. A record whose offset is beyond [`DESPAWN_RANGE`] is dropped.
pub fn restore(records: &[Persisted], player: [i64; 3], first_id: u32) -> Vec<NpcShip> {
    records
        .iter()
        .filter(|record| {
            let offset = record.offset.map(|v| v as f64);
            length(offset) <= DESPAWN_RANGE
        })
        .enumerate()
        .map(|(index, record)| NpcShip {
            id: first_id + index as u32,
            model: record.model,
            role: record.role,
            name: record.name.clone(),
            position: add(player, record.offset),
            velocity: record.velocity,
            forward: normalise(record.forward),
            up: normalise(record.up),
            radius: record.radius,
            damage: record.damage,
            shield_strength: record.shield_strength,
            cargo: record.cargo,
            state: State::Attack,
            anchor: player,
            orbit_radius: ANCHOR_ORBIT,
            phase: 0.0,
            max_speed: record.max_speed,
            accel: record.accel,
            turn_rate: record.turn_rate,
            speed: length(record.velocity),
            hostile: record.hostile,
            laser: record.laser,
            guns: record.guns,
            bounty: record.bounty,
            leader: None,
            formation: [0; 3],
            pass: 0.0,
            flee: 0.0,
            flash: 0.0,
            death: 0.0,
            station: None,
            bay: None,
            pad: None,
            dock_timer: 0.0,
            dock_retry: 0.0,
            dock_wait: 0.0,
        })
        .collect()
}

/// F585's turn-limit divisor: the model's size halves the limit from 0x35 and
/// quarters it from 0x3b.
pub fn turn_divisor(model: u32) -> u32 {
    if model >= 0x3b {
        4
    } else if model >= 0x35 {
        2
    } else {
        1
    }
}

/// F773's energy word: `DATA_006266[drive] << 24`.
pub fn drive_energy(drive: u8) -> u32 {
    const DRIVE_FUEL: [u32; 15] = [
        0, 0x20, 0x20, 0x40, 0x60, 0x80, 0xa0, 0xc0, 0xe0, 0xff, 0x20, 0x40, 0x60, 0x80, 0,
    ];
    DRIVE_FUEL.get(usize::from(drive)).copied().unwrap_or(0) << 24
}

/// F697's `DATA_006268` draw: `offset` 4 for F694's lone pirate and 2 for
/// F695's group members; `random & 0xf` is the port's F922 draw.
pub fn pirate_bounty(rng: &mut Random, grouped: bool) -> u32 {
    let offset = if grouped { 2 } else { 4 };
    let index = (rng.bounded(16) as usize + offset).min(PIRATE_BOUNTIES.len() - 1);
    PIRATE_BOUNTIES[index]
}

/// `DATA_006264` (`ffedat.asm:10082`): the per-class laser-capacity proportion
/// F774 adds to its `RND(0x33)` draw. The table has eighteen dwords; class
/// 0x11 (a Thargoid) reads `0xb2` at offset `0x44`.
pub const LASER_PROPORTION: [u16; 18] = [
    0xb2, 0x0c, 0xb2, 0x0c, 0xb2, 0x0c, 0xb2, 0x0c, 0xb2, 0x0c, 0x66, 0x99, 0x99, 0x00, 0xb2, 0x07,
    0x7f, 0xb2,
];

/// `DATA_006265` (`ffedat.asm:10092`): the laser type and tonnage table F774
/// walks. These are the laser bytes F773 writes to the mount, not the
/// equipment-shop codes; the shop's Thargoid Laser is
/// [`crate::equipment::THARGOID_LASER`] (0xd5).
pub const LASER_TONNAGE: [(u8, u32); 9] = [
    (0x00, 0),
    (0x88, 1),
    (0x91, 3),
    (0xa0, 5),
    (0xa9, 20),
    (0xb3, 75),
    (0xbc, 200),
    (0xc6, 500),
    (0xcf, 900),
];

/// F774 (`ffe.asm:50765`): pick the laser a ship of `class` fits from its
/// remaining equipment `budget`. `RND(0x33) + DATA_006264[class]` times the
/// budget, shifted right eight, picks the largest `DATA_006265` tonnage that
/// fits; the chosen tonnage is returned in the tuple with the laser byte.
///
/// The port has no equipment-budget loop, so a caller passes the model's cargo
/// word as the budget. That is the same starting quantity F774 receives for
/// these models (F1533's drive-mass subtraction is not modelled).
pub fn f774_laser(class: u8, budget: u32, rng: &mut Random) -> Option<(u8, u32)> {
    let proportion = u32::from(*LASER_PROPORTION.get(usize::from(class))?);
    let draw = rng.bounded(0x33).saturating_add(proportion);
    let capacity = ((u64::from(draw) * u64::from(budget)) >> 8) as u32;
    let mut index = 1;
    while index < LASER_TONNAGE.len() && capacity >= LASER_TONNAGE[index].1 {
        index += 1;
    }
    // The original decrements after the loop, so a capacity below the first
    // tonnage returns entry 0 (no laser).
    Some(LASER_TONNAGE[index - 1])
}

impl Role {
    /// The front laser an NPC of this role carries. F773/F774 fit it from the
    /// model's equipment budget; the port has no budget loop, so pirates a 1MW
    /// pulse, police and military a 5MW pulse, and Thargoids F774's selection
    /// from their class proportion and cargo. The equipment shop's separate
    /// Thargoid Laser (0xd5) is not one of F774's table entries.
    pub fn laser_code(self, spec: &Spec, rng: &mut Random) -> Option<u8> {
        if spec.gun_mounts <= 0 {
            return None;
        }
        match self {
            Role::Pirate => Some(laser::PULSE_1MW),
            Role::Police | Role::Military => Some(laser::PULSE_5MW),
            Role::Thargoid => f774_laser(self.ai_class(), u32::from(spec.cargo), rng)
                .and_then(|(code, _)| (code != 0).then_some(code)),
            Role::Trader | Role::Shuttle => None,
        }
    }
}

/// The F869 system report plus `F35(0x16)`, the mission-script Thargoid limit
/// F688 passes to F922 before F690. A normal system has `thargoids == 0`: the
/// port has no mission scripts, so nothing writes that global variable, and
/// `RND(0)` spawns none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct System {
    pub details: Details,
    /// `F35(0x16)` (`ffe.asm:45858`): the exclusive limit of F688's Thargoid
    /// draw, not the number of ships.
    pub thargoids: usize,
}

impl System {
    pub const fn new(details: Details, thargoids: usize) -> Self {
        Self { details, thargoids }
    }
}

/// F688's counts and roles with the port's cap. The original's `DATA_008997`
/// (ships in station bays) branch is omitted because the cache's ship-data
/// table is not loaded by the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub traders: usize,
    /// `RND(F35(0x16))`; F690's spawned count, before [`MAX_SHIPS`].
    pub thargoids: usize,
    pub shuttles: usize,
    pub pirates: usize,
    pub police: usize,
}

/// The counts F688 draws from a system record, before [`MAX_SHIPS`]. `details`
/// is F869's report: `bytes[3]` is `+8`, `bytes[1]` is `+6` and `bytes[2]` is
/// `+7`. The Thargoid limit is the separate `F35(0x16)` global, not a system
/// field, and F688 draws `RND(limit)` for it just as it does for traders.
///
/// The source's exact draws are: `RND(+8)` traders (F689), `RND(+8)` F699
/// transport groups (only after F691's station-bay branch fired), `RND(+6*4)`
/// F698 pirate-group attempts (each is a lone pirate, a group, F696, or a
/// no-op) and F690's `RND(F35(0x16))` Thargoids. The port keeps `RND(+8)`
/// traders and shuttles, one pirate group per `+6`, a police group from a
/// non-zero `+7` and the mission-gated Thargoid count; the station-bay branch
/// is omitted. The pirate `+6` count without F698's `RND(*4)` is a port
/// simplification, not the source's count.
pub fn counts(system: &System, rng: &mut Random) -> Counts {
    // F688: RND(byte +8) traders, RND(F35(0x16)) Thargoids (F690), RND(byte
    // +8) shuttles, byte +6 patrol groups, byte +7 police.
    let details = &system.details;
    let thargoid_limit = u32::try_from(system.thargoids).unwrap_or(u32::MAX);
    Counts {
        traders: rng.bounded(details.bytes[3]) as usize,
        thargoids: rng.bounded(thargoid_limit) as usize,
        shuttles: rng.bounded(details.bytes[3]) as usize,
        pirates: details.bytes[1] as usize,
        police: (details.bytes[2] > 0) as usize,
    }
}

/// F688's spawn switches that are the port's rather than the source's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpawnOptions {
    /// Force at least one trader and one pirate group. This is the port's
    /// testability/capture guarantee, not F688: the original can roll zero
    /// traffic for a system. Only a scripted capture or a unit fixture should
    /// set it; a real system entry must not, or a system the source leaves
    /// empty gets a hostile group it should not have.
    pub guarantee: bool,
    /// Place the pirate group `PIRATE_DISTANCE` ahead of the player instead of
    /// on the anchor orbit. This is the port's capture framing (it puts the
    /// pirate in the camera and inside F931's range). F702 places a spawned
    /// ship at a random system-wide point, so a normal entry must not use the
    /// player's position: otherwise a real system's pirate group is forced
    /// beside the pad or the arrival point and attacks immediately.
    pub near_player: bool,
}

impl SpawnOptions {
    /// The scripted traffic capture: something to shoot and a pirate in frame.
    pub const CAPTURE: Self = Self {
        guarantee: true,
        near_player: true,
    };
}

/// Spawn one system's traffic. `spec` and `name` look models up in the cache;
/// models without a statistics block are skipped. The result is capped at
/// [`MAX_SHIPS`] and is deterministic for a given `rng` state. [`SpawnOptions`]
/// carries the port's capture-only guarantee; without it the counts are F688's
/// (including zero) capped for legibility. `system.thargoids` is F35(0x16)'s
/// limit; zero unless a mission script set it.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    player: [i64; 3],
    forward: [f64; 3],
    anchors: &[Anchor],
    system: &System,
    allegiance: u32,
    rng: &mut Random,
    options: SpawnOptions,
    spec: impl Fn(u32) -> Option<Spec>,
) -> Vec<NpcShip> {
    let mut counts = counts(system, rng);
    let anchor = anchors.first().copied().unwrap_or(Anchor {
        position: player,
        radius: 1_000_000,
        station: None,
    });
    if options.guarantee {
        // The capture/test guarantee: a fresh system has something to label and
        // shoot and the F953 damage path is reachable.
        counts.traders = counts.traders.clamp(1, 2);
        counts.pirates = counts.pirates.clamp(1, 2);
    } else {
        // F688 can roll zero of every kind. Keep only the port's per-kind caps.
        counts.traders = counts.traders.min(2);
        counts.pirates = counts.pirates.min(2);
    }
    counts.thargoids = counts.thargoids.min(MAX_SHIPS);
    counts.shuttles = counts.shuttles.min(1);
    counts.police = counts.police.min(1);

    let orbit = (anchor.radius as f64 * 1.2).clamp(ANCHOR_ORBIT, laser::RANGE * 0.75);
    let mut ships: Vec<NpcShip> = Vec::new();
    let mut next_id = 0u32;
    let mut add = |role: Role,
                   kind: usize,
                   position: [i64; 3],
                   anchor,
                   rng: &mut Random|
     -> Option<NpcShip> {
        if next_id as usize >= MAX_SHIPS {
            return None;
        }
        let mut model = model_for(role, allegiance, rng);
        if model == 0 {
            return None;
        }
        let mut picked = spec(model);
        // A model the cache cannot draw or describe is replaced by the first
        // usable entry of the same type list.
        if picked.is_none() {
            let table = SHIP_TYPES.get(kind).copied().unwrap_or(&[]);
            for candidate in table {
                if let Some(found) = spec(*candidate) {
                    model = *candidate;
                    picked = Some(found);
                    break;
                }
            }
        }
        let mut spec = picked?;
        spec.model = model;
        spec.name = format!("{} {}", spec.name, next_id + 1);
        let mut ship = NpcShip::new(&spec, role, position, anchor, rng);
        ship.id = next_id;
        next_id += 1;
        if !role.hostile() {
            // Hold the radius the station spawn used, not the default.
            ship.orbit_radius = orbit;
        }
        if matches!(role, Role::Trader | Role::Shuttle) {
            ship.max_speed = ship.max_speed.max(STATION_CRUISE_SPEED);
            ship.accel = ship.accel.max(STATION_ACCEL);
        }
        Some(ship)
    };

    let spread = |rng: &mut Random, angle: f64, radius: f64| {
        let mut a = angle;
        a += (rng.bounded(16_384) as f64 / 16_384.0 - 0.5) * 0.8;
        let lift = (rng.bounded(1_000_000) as f64) / 1_000_000.0 * 0.4e6;
        [
            anchor.position[0] + (a.cos() * radius) as i64,
            anchor.position[1] + lift as i64,
            anchor.position[2] + (a.sin() * radius) as i64,
        ]
    };

    for i in 0..counts.traders {
        let position = spread(rng, i as f64 * 2.1, orbit);
        if let Some(mut ship) = add(Role::Trader, 1, position, anchor.position, rng) {
            // F689: a trader appears with `+0x100` set to a station and flies to
            // it; the port starts it on that station's docking cycle instead of
            // an endless orbit.
            if let Some(station) = anchor.station {
                ship.begin_transit(station, anchor.position);
            }
            ships.push(ship);
        }
    }
    // F690: F689's setup with type 0x11 (DATA_006197's models). The source uses
    // the same anchor/orbit placement and departure timer, then calls script
    // event 0x17; the port starts them hostile (see `Role::hostile`).
    for i in 0..counts.thargoids {
        let position = spread(rng, 1.0 + i as f64 * 2.1, orbit);
        if let Some(ship) = add(Role::Thargoid, 0x11, position, anchor.position, rng) {
            ships.push(ship);
        }
    }
    for i in 0..counts.shuttles {
        let position = spread(rng, 1.0 + i as f64 * 2.1, orbit * 0.8);
        if let Some(mut ship) = add(Role::Shuttle, 0xd, position, anchor.position, rng) {
            if let Some(station) = anchor.station {
                ship.begin_transit(station, anchor.position);
            }
            ships.push(ship);
        }
    }
    for i in 0..counts.police {
        let position = spread(rng, 2.6 + i as f64 * 2.1, orbit);
        if let Some(ship) = add(Role::Police, 0xe, position, anchor.position, rng) {
            ships.push(ship);
        }
    }
    // F694/F695: the pirate group. The leader is F694's lone type-0xa ship in
    // state 5; the escort uses F695's grouped type 0xb list, state 0xb, spawned
    // at its leader's formation offset and carrying the group bounty (F697's
    // offset 2). A scripted capture puts the group `PIRATE_DISTANCE` ahead of
    // the player; a real system entry uses the anchor orbit, the port's scaled
    // stand-in for F702's system-wide random placement, so the source's pirate
    // count cannot force a group beside the pad/arrival point.
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let mut leader = None;
    for i in 0..counts.pirates {
        let (side, back) = if i == 0 { (1.0, 0.0) } else { (-1.0, 0.8e6) };
        let distance = PIRATE_DISTANCE + f64::from(rng.bounded(1_000_000));
        let position = if options.near_player {
            [
                player[0]
                    + (forward[0] * distance + right[0] * side * 0.8e6 - forward[0] * back) as i64,
                player[1]
                    + (forward[1] * distance + right[1] * side * 0.8e6 - forward[1] * back) as i64,
                player[2]
                    + (forward[2] * distance + right[2] * side * 0.8e6 - forward[2] * back) as i64,
            ]
        } else {
            spread(rng, 3.6 + i as f64 * 2.1, orbit)
        };
        let holder = if options.near_player {
            player
        } else {
            anchor.position
        };
        let Some(mut ship) = add(
            Role::Pirate,
            if i == 0 { 0xa } else { 0xb },
            position,
            holder,
            rng,
        ) else {
            continue;
        };
        let id = ship.id;
        ship.bounty = pirate_bounty(rng, i > 0);
        if i == 0 {
            leader = Some(id);
        } else {
            // F695's escort: +0xfe is the leader, +0x100 the target, and the
            // +0x102..+0x106 words a station in the leader's frame. The words are
            // 500/-500 in the original's position units; the port scales them to
            // its frame (0.4e6/0.6e6) so the escorts are visibly apart.
            ship.state = State::Escort;
            ship.leader = leader;
            ship.formation = if i == 1 {
                [400_000, 0, -400_000]
            } else {
                [-400_000, 0, -600_000]
            };
        }
        ships.push(ship);
    }
    ships
}

fn offset(anchor: [i64; 3], phase: f64, radius: f64) -> [i64; 3] {
    [
        anchor[0] + (phase.cos() * radius) as i64,
        anchor[1] + (phase.sin() * radius * 0.12) as i64,
        anchor[2] + (phase.sin() * radius) as i64,
    ]
}

fn sub(a: [i64; 3], b: [i64; 3]) -> [f64; 3] {
    [
        (a[0] - b[0]) as f64,
        (a[1] - b[1]) as f64,
        (a[2] - b[2]) as f64,
    ]
}

fn add(a: [i64; 3], b: [i64; 3]) -> [i64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The unit direction away from `from`, as seen from `of`.
fn away(of: [i64; 3], from: [i64; 3]) -> [f64; 3] {
    normalise(sub(of, from))
}

fn scale(vector: [f64; 3], factor: f64) -> [f64; 3] {
    vector.map(|v| v * factor)
}

fn length(vector: [f64; 3]) -> f64 {
    vector.iter().map(|v| v * v).sum::<f64>().sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(vector: [f64; 3]) -> [f64; 3] {
    let length = length(vector);
    if length < 1e-9 {
        [1.0, 0.0, 0.0]
    } else {
        scale(vector, 1.0 / length)
    }
}

fn turn_toward(from: [f64; 3], to: [f64; 3], amount: f64) -> [f64; 3] {
    let dot = from[0] * to[0] + from[1] * to[1] + from[2] * to[2];
    let angle = dot.clamp(-1.0, 1.0).acos();
    if angle < 1e-6 {
        return from;
    }
    let t = (amount / angle).clamp(0.0, 1.0);
    // Spherical interpolation of unit vectors.
    let sin = angle.sin();
    if sin.abs() < 1e-9 {
        return to;
    }
    let a = ((1.0 - t) * angle).sin() / sin;
    let b = (t * angle).sin() / sin;
    normalise([
        from[0] * a + to[0] * b,
        from[1] * a + to[1] * b,
        from[2] * a + to[2] * b,
    ])
}

fn upright(forward: [f64; 3]) -> [f64; 3] {
    let world_up = [0.0, 1.0, 0.0];
    let right = cross(world_up, forward);
    if length(right) < 1e-6 {
        return [0.0, 0.0, 1.0];
    }
    normalise(cross(forward, normalise(right)))
}

fn distance_between(a: [i64; 3], b: [i64; 3]) -> f64 {
    length(sub(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::{Reply, Starport, CLEARANCE_SECONDS};

    fn details() -> Details {
        // Ross 154's F869 report: bytes [128, 1, 2, 3, 0].
        Details {
            words: [34032, 32779],
            bytes: [128, 1, 2, 3, 0],
            allegiance: 134,
        }
    }

    fn system(thargoids: usize) -> System {
        System::new(details(), thargoids)
    }

    fn spec(model: u32) -> Option<Spec> {
        let names = ["Eagle", "Viper", "Cobra Mk III", "Shuttle"];
        (14..=0x50).contains(&model).then(|| Spec {
            model,
            name: names[(model as usize) % names.len()].to_owned(),
            radius: 10_000,
            mass: if model >= 0x35 { 800 } else { 25 },
            shield_strength: 0x4014,
            thrust: if model >= 0x35 { 3_270 } else { 13_625 },
            drive: 2,
            gun_mounts: 1,
            cargo: 8,
        })
    }

    fn tick(ship: &mut NpcShip, seconds: f64, player: [i64; 3]) {
        // 20 ms at 1x is 994 tics.
        let tics = (994.0 * seconds / 0.02) as u32;
        ship.tick(seconds, tics, player, None);
    }

    #[test]
    fn the_ai_table_is_data_006255() {
        assert_eq!(AI_STATES.len(), 0x25);
        for (index, row) in AI_STATES.iter().enumerate() {
            assert_eq!(usize::from(row.code), index);
        }
        // State 0 is the plain-navigation row: F742 (event 6), F727 (event 5),
        // no death notification and F789's damage response.
        let row = ai_state(0x00).unwrap();
        assert_eq!(
            (row.control, row.movement, row.notify, row.damage),
            (Some(742), Some(727), None, Some(789))
        );
        // F752/F725/F782/F785: attack turn; F724 is the firing state's event-5
        // routine, not a third "combat" slot.
        let row = ai_state(0x01).unwrap();
        assert_eq!(
            (row.control, row.movement, row.notify, row.damage),
            (Some(752), Some(725), Some(782), Some(785))
        );
        assert_eq!(ai_state(0x02).unwrap().movement, Some(724));
        // F749/F727/F782/F785: F694's pirate state 5 (the transition to F750).
        let row = ai_state(0x05).unwrap();
        assert_eq!(
            (row.control, row.movement, row.notify, row.damage),
            (Some(749), Some(727), Some(782), Some(785))
        );
        // F751/F729/F782/F785: F695's formation state 0xb.
        let row = ai_state(0x0b).unwrap();
        assert_eq!(
            (row.control, row.movement, row.notify, row.damage),
            (Some(751), Some(729), Some(782), Some(785))
        );
        // 0xd is all zero; 0xe/0xf are the parked ships with only F790's
        // damage response; 0x11 is F737's jump countdown and 0x12 F740's exit.
        assert_eq!(
            ai_state(0x0d).unwrap(),
            &AiState {
                code: 0x0d,
                name: "none",
                control: None,
                movement: None,
                notify: None,
                damage: None,
            }
        );
        assert_eq!(ai_state(0x0e).unwrap().damage, Some(790));
        assert_eq!(ai_state(0x11).unwrap().control, Some(737));
        assert_eq!(ai_state(0x12).unwrap().control, Some(740));
        // The station cycle: F734 request, F747 docked, F735 launch, F746
        // final approach, F733's docked object time.
        assert_eq!(ai_state(0x1f).unwrap().control, Some(734));
        assert_eq!(ai_state(0x20).unwrap().control, Some(747));
        assert_eq!(ai_state(0x21).unwrap().control, Some(735));
        assert_eq!(ai_state(0x23).unwrap().control, Some(746));
        assert_eq!(ai_state(0x24).unwrap().control, Some(733));
        assert!(ai_state(0x25).is_none());
        // Every port state names only real rows.
        for state in [
            State::Transit,
            State::Approach,
            State::Docked,
            State::Launch,
            State::Depart,
            State::Hold,
            State::Escort,
            State::Attack,
            State::Pass,
            State::Flee,
            State::Dead,
            State::Escaped,
        ] {
            for code in state.original() {
                assert!(ai_state(*code).is_some(), "{state:?} names {code:#x}");
            }
        }
    }

    #[test]
    fn f585_halves_the_turn_limit_for_big_hulls() {
        assert_eq!(turn_divisor(0x17), 1);
        assert_eq!(turn_divisor(0x35), 2);
        assert_eq!(turn_divisor(0x3b), 4);
        let eagle = NpcShip::new(
            &spec(0x17).unwrap(),
            Role::Pirate,
            [0; 3],
            [0; 3],
            &mut Random::new(1, 2),
        );
        let big = NpcShip::new(
            &spec(0x3b).unwrap(),
            Role::Pirate,
            [0; 3],
            [0; 3],
            &mut Random::new(1, 2),
        );
        assert!((eagle.turn_rate - TURN_RATE).abs() < 1e-12);
        assert!((big.turn_rate - TURN_RATE / 4.0).abs() < 1e-12);
        // F601's thrust/mass ratio: the light Eagle outruns the big hull.
        assert!(eagle.max_speed > big.max_speed);
        assert!(eagle.accel > big.accel);
    }

    #[test]
    fn the_pirate_bounty_comes_from_data_006268() {
        let mut rng = Random::new(7, 9);
        let mut seen = Vec::new();
        for _ in 0..64 {
            seen.push(pirate_bounty(&mut rng, false));
            seen.push(pirate_bounty(&mut rng, true));
        }
        assert!(seen.iter().all(|bounty| PIRATE_BOUNTIES.contains(bounty)));
        assert!(seen.iter().any(|bounty| *bounty > 0));
        // A grouped pirate draws from offset 2, a lone one from offset 4, so the
        // lone range includes the higher entries.
        assert!(PIRATE_BOUNTIES[16..].iter().all(|bounty| *bounty > 0));
    }

    #[test]
    fn a_pirate_holds_its_laser_energy_and_gates() {
        let ship = NpcShip::new(
            &spec(0x17).unwrap(),
            Role::Pirate,
            [0; 3],
            [0; 3],
            &mut Random::new(1, 2),
        );
        assert_eq!(ship.laser, Some(laser::PULSE_1MW));
        // The drive word gives F773's energy: class 2 is 0x20 << 24.
        assert_eq!(ship.guns.energy, 0x2000_0000);
        assert_eq!(drive_energy(2), 0x2000_0000);
        assert_eq!(drive_energy(0), 0);
    }

    fn aimed_ship(role: Role, from: [i64; 3], at: [i64; 3]) -> NpcShip {
        let mut ship = NpcShip::new(
            &spec(0x17).unwrap(),
            role,
            from,
            from,
            &mut Random::new(1, 2),
        );
        ship.forward = normalise(sub(at, from));
        ship.up = upright(ship.forward);
        ship
    }

    #[test]
    fn firing_obeys_f931s_gates_and_applies_f930s_damage() {
        let player = [0, 0, 0];
        let mut ship = aimed_ship(Role::Pirate, [0, 0, 2_000_000], player);
        let shot = ship.fire(994, player, 10_000).expect("a hit");
        assert_eq!(shot.damage, 50);
        assert!(!shot.beam);
        assert_eq!(shot.ship, ship.id);
        assert!(shot.distance > 0.0);
        // F930 set the refire word; the next shot is refused.
        assert_eq!(ship.guns.refire, laser::PULSE_REFIRE);
        assert!(ship.fire(994, player, 10_000).is_none());
        // F730's countdown clears it; then a shot behind the player misses but
        // still spends the refire word.
        ship.guns.refire = 0;
        ship.forward = [0.0, 0.0, 1.0];
        assert!(ship.fire(994, player, 10_000).is_none());
        assert_ne!(ship.guns.refire, 0);
        // Heat above the limit and no energy refuse before the ray is tested,
        // and an unarmed trader never fires.
        ship.guns.refire = 0;
        ship.guns.heat = laser::HEAT_LIMIT + 1;
        assert!(ship.fire(994, player, 10_000).is_none());
        let mut trader = aimed_ship(Role::Trader, [0, 0, 2_000_000], player);
        assert!(trader.fire(994, player, 10_000).is_none());
        // Clean ships do not fire even with a gun.
        let mut police = aimed_ship(Role::Police, [0, 0, 2_000_000], player);
        assert!(police.fire(994, player, 10_000).is_none());
        assert!(police.provoke());
        assert!(police.fire(994, player, 10_000).is_some());
    }

    #[test]
    fn a_damaged_hostile_runs_and_jumps_out() {
        let player = [0, 0, 0];
        let mut ship = aimed_ship(Role::Pirate, [0, 0, 2_000_000], player);
        ship.damage.hull = 1;
        tick(&mut ship, 0.02, player);
        assert_eq!(ship.state, State::Flee);
        let before = distance_between(ship.position, player);
        tick(&mut ship, 1.0, player);
        assert!(distance_between(ship.position, player) > before);
        tick(&mut ship, FLEE_SECONDS, player);
        assert_eq!(ship.state, State::Escaped);
        assert!(!ship.alive());
        assert!(ship.finished(player));
    }

    #[test]
    fn an_attack_pass_brakes_for_the_standoff() {
        let player = [0, 0, 0];
        let mut ship = aimed_ship(Role::Pirate, [0, 0, 3.5e6 as i64], player);
        let start = distance_between(ship.position, player);
        let mut closest = start;
        let mut peak: f64 = 0.0;
        for _ in 0..8000 {
            tick(&mut ship, 0.02, player);
            let distance = distance_between(ship.position, player);
            closest = closest.min(distance);
            peak = peak.max(ship.speed);
        }
        assert!(peak > 0.5 * ship.max_speed, "it never accelerated: {peak}");
        // The braking rule keeps it out of the player's lap, and the ship is
        // still alive and hostile after 8 s.
        assert!(closest > 0.5 * STANDOFF, "closed to {closest}");
        assert!(closest < 2.0 * STANDOFF, "never closed: {closest}");
        assert!(ship.alive());
        assert!(matches!(ship.state, State::Attack | State::Pass));
    }

    #[test]
    fn a_transit_ship_reaches_its_anchor_and_holds() {
        let anchor = [2.0e6 as i64, 0, 0];
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0, 0, 0],
            anchor,
            &mut Random::new(3, 4),
        );
        ship.state = State::Transit;
        let mut closest = f64::MAX;
        for _ in 0..8000 {
            tick(&mut ship, 0.02, [0, 0, 0]);
            closest = closest.min(distance_between(ship.position, anchor));
        }
        assert_eq!(ship.state, State::Hold);
        assert!(closest < 1.5 * ARRIVE, "never arrived: {closest}");
        assert!(distance_between(ship.position, anchor) < 6.0e6);
    }

    #[test]
    fn f675_keeps_a_close_pirate_and_f680_restores_it() {
        let mut rng = Random::new(11, 12);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [0, 0, 0],
                radius: 2_000_000,
                station: None,
            }],
            &system(0),
            0,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        let mut pirate = ships
            .iter()
            .find(|s| s.role == Role::Pirate)
            .unwrap()
            .clone();
        pirate.position = [0, 0, 1_000_000];
        pirate.damage.hull = 5;
        let records = persist(&[pirate.clone()], [0; 3]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].offset, [0, 0, 1_000_000]);
        // A pirate far away is not kept.
        pirate.position = [0, 0, 9_000_000];
        assert!(persist(&[pirate], [0; 3]).is_empty());
        // F680 restores the kept ship at the player's arrival point.
        let restored = restore(&records, [5_000_000, 0, 0], 7);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, 7);
        assert_eq!(restored[0].position, [5_000_000, 0, 1_000_000]);
        assert_eq!(restored[0].damage.hull, 5);
        assert!(restored[0].is_hostile());
        assert_eq!(restored[0].state, State::Attack);
    }

    #[test]
    fn f688_counts_come_from_the_system_record_and_are_capped() {
        let mut rng = Random::new(0x1234_5678, 0x4567_89ab);
        let c = counts(&system(0), &mut rng);
        assert!(c.traders <= 3);
        assert!(c.pirates == 1);
        assert_eq!(c.police, 1);
        assert_eq!(c.thargoids, 0);
        // F35(0x16) is an exclusive limit: F688 calls F922 on it exactly as it
        // does for the trader counts, so the result is below the limit.
        let c = counts(&system(3), &mut rng);
        assert!(c.thargoids < 3);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [1_000_000, 0, 0],
                radius: 4_500_000,
                station: None,
            }],
            &system(0),
            2,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        assert!((1..=MAX_SHIPS).contains(&ships.len()));
        assert!(ships.iter().any(|s| s.role == Role::Trader));
        assert!(ships.iter().any(|s| s.role == Role::Pirate));
        // The pirate group is near the player, within the laser's range.
        let pirate = ships.iter().find(|s| s.role == Role::Pirate).unwrap();
        assert!(distance_between(pirate.position, [0, 0, 0]) < laser::RANGE);
        // The trader orbits the anchor, not the player.
        let trader = ships.iter().find(|s| s.role == Role::Trader).unwrap();
        assert!(distance_between(trader.position, [1_000_000, 0, 0]) < 10.0e6);
        // Every NPC that carries a laser can fire it, and the police do not
        // start hostile.
        let police = ships.iter().find(|s| s.role == Role::Police).unwrap();
        assert!(!police.is_hostile());
        assert!(police.laser.is_some());
    }

    /// Without the port's capture guarantee, F688's own counts decide: a record
    /// whose +6/+8 bytes are zero rolls no traffic at all. This is the rule the
    /// new game and an ordinary system entry use, so a system the source leaves
    /// empty cannot force a pirate group beside the player.
    #[test]
    fn the_system_counts_alone_can_spawn_no_traffic() {
        let mut rng = Random::new(7, 8);
        let empty = System::new(
            Details {
                words: [0, 0],
                bytes: [128, 0, 0, 0, 0],
                allegiance: 0,
            },
            0,
        );
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [1_000_000, 0, 0],
                radius: 4_500_000,
                station: None,
            }],
            &empty,
            0,
            &mut rng,
            SpawnOptions::default(),
            spec,
        );
        assert!(ships.is_empty(), "{ships:?}");
        // The capture guarantee is what adds the one trader/pirate group.
        let mut rng = Random::new(7, 8);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [1_000_000, 0, 0],
                radius: 4_500_000,
                station: None,
            }],
            &empty,
            0,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        assert!(ships.iter().any(|ship| ship.role == Role::Trader));
        assert!(ships.iter().any(|ship| ship.role == Role::Pirate));
    }

    /// A Thargoid model's statistics: DATA_006197's models have 0x31's 232 t
    /// hull/203 t cargo, 1-2 gun mounts and (except 0x45) the shared 0x800e
    /// drive word.
    fn thargoid_spec(model: u32, cargo: u16, guns: i8) -> Spec {
        Spec {
            model,
            name: "Unknown".to_owned(),
            radius: 90_000,
            mass: 232,
            shield_strength: 16_430,
            thrust: 14_715,
            drive: 14,
            gun_mounts: guns,
            cargo,
        }
    }

    #[test]
    fn f690_spawns_rnd_f35_0x16_thargoids_with_data_006197_models() {
        assert_eq!(Role::Thargoid.type_code(), 0x11);
        assert_eq!(Role::Thargoid.ai_class(), 0x11);
        assert!(Role::Thargoid.hostile());
        assert!(!Role::Thargoid.clean());
        assert_eq!(
            SHIP_TYPES[0x11],
            &[0x45, 0x31, 0x31, 0x32, 0x32, 0x33, 0x33, 0x3f, 0x3f, 0x3f, 0x3f, 0x3f][..]
        );

        let anchor = Anchor {
            position: [0, 0, 0],
            radius: 2_000_000,
            station: None,
        };
        // The only source of the count is F35(0x16): zero spawns none.
        let mut rng = Random::new(0x2345_6789, 0x3456_789a);
        let none = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[anchor],
            &system(0),
            0,
            &mut rng,
            SpawnOptions::default(),
            spec,
        );
        assert!(none.iter().all(|ship| ship.role != Role::Thargoid));

        // F688 draws RND(F35(0x16)); use a limit large enough that this seed's
        // draw and the MAX_SHIPS cap still leave a Thargoid in the traffic.
        let limit = 12;
        let mut rng = Random::new(0x2345_6789, 0x3456_789a);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[anchor],
            &system(limit),
            0,
            &mut rng,
            SpawnOptions::default(),
            spec,
        );
        let thargoids: Vec<&NpcShip> = ships
            .iter()
            .filter(|ship| ship.role == Role::Thargoid)
            .collect();
        assert!(!thargoids.is_empty(), "RND({limit}) drew no Thargoids");
        assert!(
            thargoids.len() < limit,
            "the draw must stay below F35(0x16)"
        );
        for ship in thargoids {
            assert!(
                SHIP_TYPES[0x11].contains(&ship.model),
                "model {:#x}",
                ship.model
            );
            assert_eq!(ship.state, State::Attack);
            assert_eq!(ship.role.ai_class(), 0x11);
            assert!(ship.is_hostile());
            assert_eq!(ship.bounty, 0);
            // The test spec's cargo (8 t) reaches F774's 1MW beam entry.
            assert_eq!(ship.laser, Some(laser::BEAM_1MW));
        }
    }

    #[test]
    fn f774_picks_the_thargoid_laser_from_class_and_cargo() {
        let mut rng = Random::new(3, 4);
        // DATA_006264[0x11] is 0xb2. 203 t of cargo always reaches the 75 t
        // 20MW beam entry (0xb3), not the 900 t 0xcf entry.
        for _ in 0..32 {
            assert_eq!(
                f774_laser(0x11, 203, &mut rng),
                Some((laser::BEAM_20MW, 75))
            );
        }
        // The Thargoid Transporter's 13,000 t cargo reaches 0xcf.
        assert_eq!(
            f774_laser(0x11, 13_000, &mut rng),
            Some((laser::THARGOID, 900))
        );
        // No budget is DATA_006265[0], "no laser"; a class outside
        // DATA_006264's eighteen entries has no proportion.
        assert_eq!(f774_laser(0x11, 0, &mut rng), Some((0, 0)));
        assert_eq!(f774_laser(0x1f, 100, &mut rng), None);
        // A gun-less model gets no mount even when its cargo picks a laser.
        let mut rng = Random::new(5, 6);
        let unarmed = Role::Thargoid.laser_code(&thargoid_spec(0x45, 13_000, 0), &mut rng);
        assert_eq!(unarmed, None);
        let armed = Role::Thargoid.laser_code(&thargoid_spec(0x31, 203, 1), &mut rng);
        assert_eq!(armed, Some(laser::BEAM_20MW));
        let unarmed = Role::Thargoid.laser_code(&thargoid_spec(0x31, 0, 1), &mut rng);
        assert_eq!(unarmed, None);
    }

    #[test]
    fn a_thargoid_uses_the_model_shield_hull_and_drive_words() {
        let ship = NpcShip::new(
            &thargoid_spec(0x31, 203, 1),
            Role::Thargoid,
            [0, 0, 2_000_000],
            [0; 3],
            &mut Random::new(7, 8),
        );
        assert_eq!(ship.damage.shields, 0xb8); // 0x2e * 4
        assert_eq!(ship.damage.shields_max, 0xb8);
        assert_eq!(ship.damage.hull, 928); // 232 * 4
        assert_eq!(ship.shield_strength, 16_430);
        assert_eq!(ship.guns.energy, drive_energy(14));
        assert_eq!(ship.guns.energy, 0);
        // The source's 0x800e drive word has no DATA_006266 energy entry; the
        // port keeps that zero rather than inventing a loadout.
        assert_eq!(ship.laser, Some(laser::BEAM_20MW));
    }

    #[test]
    fn an_escort_follows_its_leaders_formation() {
        let mut rng = Random::new(21, 22);
        // Two pirates, so F695's escort is spawned when the cap allows.
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[],
            &System::new(
                Details {
                    words: [0, 0],
                    bytes: [128, 2, 0, 3, 0],
                    allegiance: 0,
                },
                0,
            ),
            0,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        let pirates: Vec<&NpcShip> = ships.iter().filter(|s| s.role == Role::Pirate).collect();
        assert!(pirates.len() >= 2, "no escort spawned");
        assert_eq!(pirates[0].state, State::Attack);
        assert_eq!(pirates[1].state, State::Escort);
        assert_eq!(pirates[1].leader, Some(pirates[0].id));
        assert_ne!(pirates[1].formation, [0; 3]);
    }

    #[test]
    fn damage_is_shields_then_hull_then_death_with_the_flash() {
        let mut rng = Random::new(3, 4);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[],
            &system(0),
            0,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        let mut ship = ships.into_iter().next().unwrap();
        ship.damage = DamageState {
            shields: 100,
            shields_max: 100,
            hull: 100,
            mass: 25,
        };
        assert_eq!(ship.hit(50), Damage::Shields);
        assert!(ship.flash > 0.0);
        assert_eq!(ship.shield_percent(), 50);
        assert_eq!(ship.hit(80), Damage::Hull);
        assert_eq!(ship.damage.hull, 70);
        assert_eq!(ship.hit(70), Damage::Destroyed);
        assert!(!ship.alive());
        assert_eq!(ship.state, State::Dead);
        assert!(!ship.finished([0, 0, 0]));
        ship.tick(DEATH_SECONDS, 0, [0, 0, 0], None);
        assert!(ship.finished([0, 0, 0]));
    }

    #[test]
    fn holding_traffic_stays_near_its_anchor_and_pirates_close_in() {
        let mut rng = Random::new(5, 6);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [0, 0, 0],
                radius: 2_000_000,
                station: None,
            }],
            &system(0),
            0,
            &mut rng,
            SpawnOptions::CAPTURE,
            spec,
        );
        let mut trader = ships
            .iter()
            .find(|s| s.role == Role::Trader)
            .unwrap()
            .clone();
        for _ in 0..600 {
            tick(&mut trader, 0.02, [0, 0, 0]);
        }
        assert!(distance_between(trader.position, [0, 0, 0]) < 6.0e6);
        assert!(distance_between(trader.position, [0, 0, 0]) > 1.0e6);
        let mut pirate = ships
            .iter()
            .find(|s| s.role == Role::Pirate)
            .unwrap()
            .clone();
        let start = distance_between(pirate.position, [0, 0, 0]);
        for _ in 0..600 {
            tick(&mut pirate, 0.02, [0, 0, 0]);
        }
        assert!(distance_between(pirate.position, [0, 0, 0]) < start);
    }

    /// A station with its face at +1 km and a pad 1 km out on +z, for the
    /// docking-cycle tests. The traffic record is the caller's.
    fn test_station(port: &Starport, id: u8) -> Station {
        Station {
            index: 0,
            position: [0; 3],
            axis: [0.0, 0.0, 1.0],
            front: 1_000_000.0,
            radius: 4_000_000.0,
            pad: Some([0, 0, 1_000_000]),
            bay: port.bay_of(id),
            cleared: port.is_docking(id),
            closing: port.mode == crate::traffic::DOCKING_DONE,
            door_open: port.door == 0,
        }
    }

    /// Apply a ship's requests to the traffic record exactly as
    /// `Navigator::apply_dock_request` does, for one ship. Returns how many times
    /// F762 picked a station.
    fn service(
        ship: &mut NpcShip,
        port: &mut Starport,
        requests: &[DockRequest],
        next_station: Option<usize>,
    ) -> usize {
        let id = traffic_id(ship.id);
        let mut chosen = 0;
        for request in requests {
            match *request {
                DockRequest::Request { .. } => {
                    if let Reply::Granted(bay) = port.request_docking(id, false, 0) {
                        ship.begin_approach(bay.index, None);
                    }
                }
                DockRequest::Touch { .. } => {
                    if port.touch(id) {
                        let seat = ship.pad.unwrap_or([0, 0, 1_000_000]);
                        ship.begin_docked(seat);
                    } else {
                        ship.touch_failed();
                    }
                }
                DockRequest::Settle { .. } => port.reset(),
                DockRequest::Launch { .. } => {
                    if port.request(id) {
                        ship.state = State::Launch;
                        ship.dock_wait = 0.0;
                    }
                }
                DockRequest::Release { .. } => port.release(id),
                DockRequest::ChooseStation { .. } => {
                    chosen += 1;
                    match next_station {
                        Some(index) => ship.begin_transit(index, [0; 3]),
                        None => {
                            ship.station = None;
                            ship.state = State::Transit;
                            ship.bay = None;
                            ship.pad = None;
                        }
                    }
                }
            }
        }
        chosen
    }

    /// Drive one ship's station cycle; `stop` ends the run at a state.
    fn run_cycle(
        ship: &mut NpcShip,
        port: &mut Starport,
        frames: usize,
        stop: impl Fn(State) -> bool,
        next_station: Option<usize>,
    ) -> (Vec<State>, usize) {
        let mut states = vec![ship.state];
        let mut chosen = 0;
        for _ in 0..frames {
            // The real frame time is the tick's; the test's 0.5 s frames give the
            // door the matching tics so it opens in the original 1.3 s.
            port.move_door(24_850);
            let station = ship
                .station
                .map(|_| test_station(port, traffic_id(ship.id)));
            let mut requests = Vec::new();
            ship.tick_with_station(0.5, 24_850, [0; 3], None, station.as_ref(), &mut requests);
            chosen += service(ship, port, &requests, next_station);
            if states.last() != Some(&ship.state) {
                states.push(ship.state);
            }
            if stop(ship.state) {
                break;
            }
        }
        (states, chosen)
    }

    #[test]
    fn a_trader_requests_a_bay_approaches_and_docks() {
        let mut port = Starport::new(3);
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0, 0, 4_900_000],
            [0; 3],
            &mut Random::new(3, 4),
        );
        ship.begin_transit(0, [0; 3]);
        // The source's speeds make a 4 km approach minutes long; the test keeps
        // the model's turn and bay arithmetic but accelerates the frame motion.
        ship.max_speed = 400_000.0;
        ship.accel = 2_000_000.0;
        let id = traffic_id(ship.id);
        assert!(
            id > 1,
            "an NPC uses the record's bytes above the player's 1"
        );
        let (states, _) = run_cycle(
            &mut ship,
            &mut port,
            4000,
            |state| state == State::Docked,
            Some(0),
        );
        assert_eq!(states, [State::Transit, State::Approach, State::Docked]);
        // F650's bay is occupied from the request until the departure's F651.
        assert!(port.pads.contains(&id));
        assert_eq!(port.bay_of(id), Some(3));
        // F652 set mode 3 (the door closing). F747 runs F653 once it is over;
        // the bay itself stays allocated.
        assert_eq!(port.mode, crate::traffic::DOCKING_DONE);
        run_cycle(&mut ship, &mut port, 16, |_| false, Some(0));
        assert_eq!((port.mode, port.current, port.timeout), (0, 0, 0));
        assert_eq!(port.bay_of(id), Some(3));
        assert!((ship.position[2] - 1_000_000).abs() < 1);
        assert!(!ship.visible(), "a docked ship is inside the station");
    }

    #[test]
    fn the_docked_interval_is_f747s_two_to_three_days() {
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0; 3],
            [0; 3],
            &mut Random::new(5, 6),
        );
        ship.begin_docked([0, 0, 1_000_000]);
        assert_eq!(ship.state, State::Docked);
        assert!((2.0 * 86_400.0..3.0 * 86_400.0).contains(&ship.dock_timer));
        // F733's object time: the ship asks F654 only after it has run out.
        let mut port = Starport::new(3);
        let id = traffic_id(ship.id);
        port.request_docking(id, false, 0);
        ship.dock_timer = 0.0;
        let station = test_station(&port, id);
        let mut requests = Vec::new();
        ship.tick_with_station(0.5, 24_850, [0; 3], None, Some(&station), &mut requests);
        assert_eq!(
            requests,
            [DockRequest::Launch {
                ship: ship.id,
                station: 0
            }]
        );
    }

    #[test]
    fn a_docked_ship_launches_when_the_door_opens_and_resumes_transit() {
        let mut port = Starport::new(3);
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0, 0, 4_900_000],
            [0; 3],
            &mut Random::new(7, 8),
        );
        ship.begin_transit(0, [0; 3]);
        ship.max_speed = 400_000.0;
        ship.accel = 2_000_000.0;
        let id = traffic_id(ship.id);
        run_cycle(
            &mut ship,
            &mut port,
            4000,
            |state| state == State::Docked,
            Some(0),
        );
        assert!(port.pads.contains(&id));
        // Run the two-to-three day interval down; F654 opens the door and F735
        // waits for F865's word to reach 0 before F651 releases the bay.
        ship.dock_timer = 0.01;
        let (states, chosen) = run_cycle(
            &mut ship,
            &mut port,
            4000,
            |state| state == State::Transit,
            Some(0),
        );
        assert_eq!(
            states,
            [State::Docked, State::Launch, State::Depart, State::Transit]
        );
        assert_eq!(chosen, 1, "F761/F762 picked the next station");
        assert!(port.pads.iter().all(|pad| *pad == 0), "F651 freed the bay");
        assert_eq!(port.bay_of(id), None);
        assert!(ship.visible(), "the launched ship is outside the station");
        assert_eq!(ship.station, Some(0), "it resumed transit to a station");
    }

    #[test]
    fn a_full_station_makes_the_trader_try_another() {
        let mut port = Starport::new(1);
        // Two bays, both taken by other ships.
        port.pads[0] = 20;
        port.pads[1] = 21;
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0, 0, 4_900_000],
            [0; 3],
            &mut Random::new(9, 10),
        );
        ship.begin_transit(0, [0; 3]);
        ship.max_speed = 400_000.0;
        ship.accel = 2_000_000.0;
        let (_, chosen) = run_cycle(&mut ship, &mut port, 400, |_| false, None);
        assert_eq!(chosen, 1, "F761/F762 was asked to pick another station");
        assert_eq!(ship.station, None, "the ship gave up on this station");
        // F655's Full branch resets the handling record but not the occupants.
        assert_eq!((port.mode, port.current, port.timeout), (0, 0, 0));
    }

    #[test]
    fn a_clearance_timeout_frees_the_bay_and_recycles_the_ship() {
        let mut port = Starport::new(3);
        let mut ship = NpcShip::new(
            &spec(0x1c).unwrap(),
            Role::Trader,
            [0, 0, 4_900_000],
            [0; 3],
            &mut Random::new(11, 12),
        );
        ship.begin_transit(0, [0; 3]);
        let id = traffic_id(ship.id);
        let reply = port.request_docking(id, false, 0);
        assert!(matches!(reply, Reply::Granted(_)));
        ship.begin_approach(3, None);
        // F867: the clearance runs out while the ship is still on its way.
        assert_eq!(port.elapse(u32::from(CLEARANCE_SECONDS)), Some(id));
        ship.touch_failed();
        assert_eq!(ship.state, State::Transit);
        assert_eq!(ship.bay, None);
        assert!(port.pads.iter().all(|pad| *pad == 0));
        // The ship asks again after the retry delay.
        assert!(ship.dock_retry > 0.0);
    }

    #[test]
    fn f922_is_bounded_and_deterministic() {
        let mut a = Random::new(0x1234_5678, 0x4567_89ab);
        let mut b = a;
        let first: Vec<u32> = (0..20).map(|_| a.bounded(1000)).collect();
        assert_eq!(first, (0..20).map(|_| b.bounded(1000)).collect::<Vec<_>>());
        assert!(first.iter().all(|value| *value < 1000));
        assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
        assert_eq!(Random::new(0, 0).bounded(10), 0);
    }

    #[test]
    fn the_type_table_has_the_originals_lengths() {
        let counts: Vec<usize> = SHIP_TYPES.iter().map(|table| table.len()).collect();
        assert_eq!(
            counts,
            vec![9, 18, 10, 8, 12, 16, 23, 27, 11, 12, 15, 24, 10, 2, 4, 3, 20, 12, 1]
        );
        // F635's spot values: a Federal trader table and the police/shuttle lists.
        assert_eq!(SHIP_TYPES[1][0], 0x1c);
        assert_eq!(SHIP_TYPES[5][0], 0x1d);
        assert_eq!(SHIP_TYPES[0xd], [0x0f, 0x12]);
        assert_eq!(SHIP_TYPES[0xe], [0x1e, 0x1e, 0x22, 0x22]);
    }

    #[test]
    fn the_allegiance_shifts_traders_to_the_imperial_shipyard() {
        // DATA_008996 is 2 for Ross 154 (allegiance 134 >> 6); type 1 becomes 5.
        let mut rng = Random::new(1, 2);
        let model = model_for(Role::Trader, 2, &mut rng);
        assert!(SHIP_TYPES[5].contains(&model));
        let mut rng = Random::new(1, 2);
        assert!(SHIP_TYPES[1].contains(&model_for(Role::Trader, 0, &mut rng)));
    }
}
