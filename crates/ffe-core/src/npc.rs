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
//! bays, missiles and Thargoids remain. Placement uses the original's rule (a
//! random phase on an orbit around an anchor) with a radius in the port's unit
//! system that keeps the ships reachable. See `docs/notes/npc.md`.

use crate::galaxy::Details;
use crate::laser::{self, Damage, DamageState};
use crate::model_animation::TICS_PER_FRAME;

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
    // 0x2: DATA_006182, ten (Federal military)
    &[0x19, 0x13, 0x1e, 0x13, 0x22, 0x20, 0x20, 0x22, 0x38, 0x38],
    // 0x3: DATA_006183, eight (Federal shipyard)
    &[0x1d, 0x1e, 0x22, 0x24, 0x26, 0x28, 0x27, 0x37],
    // 0x4: DATA_006180, twelve (Imperial military)
    &[
        0x17, 0x18, 0x1f, 0x14, 0x21, 0x15, 0x16, 0x2b, 0x1e, 0x22, 0x23, 0x27,
    ],
    // 0x5: DATA_006181, sixteen (Imperial shipyard)
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

/// One row of the original's AI state table, `DATA_006258`
/// (`ffedat.asm:9862`): four routine slots per state.
///
/// The slots are the routines F706 (control/arrival), F704 (movement),
/// F778 (combat, AI ships only) and F780 (notification/collision) dispatch:
/// a row's `None` slot is a `dd 0` in the table. The routine numbers are the
/// original's, so a row can be checked against the disassembly; the port models
/// the states named below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiState {
    /// `object+0xff`.
    pub code: u8,
    /// What the original uses the state for.
    pub name: &'static str,
    /// F706's routine.
    pub control: Option<u16>,
    /// F704's routine.
    pub movement: Option<u16>,
    /// F778's routine.
    pub combat: Option<u16>,
    /// F780's routine.
    pub notify: Option<u16>,
}

/// `DATA_006258`: the AI states, in `object+0xff` order.
///
/// The first ten rows are the approach/docking and dogfight cycle: F752/F753
/// fly to the target, F753's arrival test picks state 3 (close) under 5 units or
/// state 4 (F749's random reposition) over 10; F724 (state 1) is the original's
/// only AI fire call. Rows 5/0xa/0xb are the pirate states F694/F695 set.
pub const AI_STATES: [AiState; 0x25] = [
    AiState {
        code: 0x00,
        name: "approach",
        control: Some(789),
        movement: Some(752),
        combat: Some(725),
        notify: Some(782),
    },
    AiState {
        code: 0x01,
        name: "attack",
        control: Some(785),
        movement: Some(752),
        combat: Some(724),
        notify: Some(782),
    },
    AiState {
        code: 0x02,
        name: "line up",
        control: Some(785),
        movement: Some(755),
        combat: Some(725),
        notify: Some(782),
    },
    AiState {
        code: 0x03,
        name: "close",
        control: Some(785),
        movement: Some(750),
        combat: Some(727),
        notify: Some(782),
    },
    AiState {
        code: 0x04,
        name: "reposition",
        control: Some(785),
        movement: Some(749),
        combat: Some(727),
        notify: Some(782),
    },
    AiState {
        code: 0x05,
        name: "pirate attack",
        control: Some(785),
        movement: Some(759),
        combat: Some(725),
        notify: None,
    },
    AiState {
        code: 0x06,
        name: "hold station",
        control: Some(790),
        movement: Some(736),
        combat: Some(727),
        notify: None,
    },
    AiState {
        code: 0x07,
        name: "depart",
        control: Some(788),
        movement: Some(741),
        combat: Some(727),
        notify: None,
    },
    AiState {
        code: 0x08,
        name: "new destination",
        control: Some(789),
        movement: Some(770),
        combat: Some(727),
        notify: None,
    },
    AiState {
        code: 0x09,
        name: "formation",
        control: Some(785),
        movement: Some(743),
        combat: Some(727),
        notify: None,
    },
    AiState {
        code: 0x0a,
        name: "attack run",
        control: Some(789),
        movement: Some(751),
        combat: Some(729),
        notify: Some(782),
    },
    AiState {
        code: 0x0b,
        name: "escort",
        control: Some(785),
        movement: Some(757),
        combat: Some(725),
        notify: Some(781),
    },
    AiState {
        code: 0x0c,
        name: "none",
        control: None,
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x0d,
        name: "parked shuttle",
        control: Some(790),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x0e,
        name: "parked police",
        control: Some(790),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x0f,
        name: "jump arrival",
        control: Some(790),
        movement: Some(760),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x10,
        name: "jump out",
        control: Some(737),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x11,
        name: "docked",
        control: Some(740),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x12,
        name: "after jump",
        control: Some(758),
        movement: Some(717),
        combat: Some(784),
        notify: None,
    },
    AiState {
        code: 0x13,
        name: "match frame",
        control: None,
        movement: Some(716),
        combat: Some(784),
        notify: None,
    },
    AiState {
        code: 0x14,
        name: "missile",
        control: Some(768),
        movement: Some(712),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x15,
        name: "missile",
        control: Some(767),
        movement: Some(713),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x16,
        name: "missile",
        control: Some(732),
        movement: Some(714),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x17,
        name: "missile",
        control: Some(766),
        movement: Some(710),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x18,
        name: "missile",
        control: Some(765),
        movement: Some(711),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x19,
        name: "missile",
        control: None,
        movement: Some(709),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1a,
        name: "missile",
        control: Some(769),
        movement: Some(715),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1b,
        name: "missile",
        control: None,
        movement: Some(708),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1c,
        name: "explosion",
        control: Some(764),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1d,
        name: "debris",
        control: Some(763),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1e,
        name: "undock check",
        control: Some(734),
        movement: Some(727),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x1f,
        name: "hyperspace",
        control: Some(787),
        movement: Some(747),
        combat: Some(723),
        notify: None,
    },
    AiState {
        code: 0x20,
        name: "leave station",
        control: Some(735),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x21,
        name: "wait clearance",
        control: Some(748),
        movement: None,
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x22,
        name: "dock",
        control: Some(746),
        movement: Some(727),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x23,
        name: "launch",
        control: Some(790),
        movement: Some(733),
        combat: None,
        notify: None,
    },
    AiState {
        code: 0x24,
        name: "spawn near player",
        control: Some(786),
        movement: None,
        combat: None,
        notify: None,
    },
];

/// `DATA_006258`'s row for a state byte, when the byte is in the table.
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
/// - `Transit` = 0x00/0x01/0x07/0x08/0x12 (fly to the target, F752/F753).
/// - `Hold` = 0x06/0x09/0x11/0x0d/0x0e (station keeping, F736/F743/F740).
/// - `Attack` = 0x05/0x0a/0x0b (the pirate states, F759/F751/F757).
/// - `Pass` = 0x03/0x04 (F753's close and F749's reposition).
/// - `Flee` = 0x10/0x1f/0x21 (F737/F787's jump-out).
/// - `Dead` = 0x1c/0x1d (F942's explosion object).
/// - `Escaped` = 0x1f after the jump (F738 hides the model and F927 frees it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Fly to the anchor or waypoint, then hold.
    Transit,
    /// Circle the spawn anchor (F689/F699's station traffic).
    Hold,
    /// Station-keep in a leader's frame (F695's escorts, F757/F743).
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
            State::Transit => &[0x00, 0x01, 0x07, 0x08, 0x12],
            State::Hold => &[0x06, 0x0d, 0x0e, 0x11],
            State::Escort => &[0x09, 0x0b],
            State::Attack => &[0x05, 0x0a],
            State::Pass => &[0x03, 0x04],
            State::Flee => &[0x10, 0x1f, 0x21],
            State::Dead => &[0x1c, 0x1d],
            State::Escaped => &[0x1f],
        }
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
            laser: role.laser_code(spec),
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
        }
    }

    pub fn alive(&self) -> bool {
        !matches!(self.state, State::Dead | State::Escaped)
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
        if !self.alive() {
            self.death = (self.death - dt).max(0.0);
            return;
        }
        self.flash = (self.flash - dt).max(0.0);
        laser::tick_refire(&mut self.guns, tics);
        laser::cool(&mut self.guns, dt, false);
        // The original's jump-out states (0x10/0x1f/0x21) hide the model and
        // move it out (F738/F787). The low-hull trigger is the port's.
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
        // The leader's frame gives F695's escorts their station.
        let target = match self.state {
            State::Hold => {
                self.phase += (self.max_speed * 0.35 / self.orbit_radius.max(1.0)) * dt;
                offset(self.anchor, self.phase, self.orbit_radius)
            }
            State::Transit => self.anchor,
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
            State::Dead | State::Escaped => self.position,
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
            State::Transit => {
                if distance <= ARRIVE {
                    0.0
                } else {
                    self.max_speed
                        .min(0.7 * (2.0 * self.accel * (distance - ARRIVE)).sqrt())
                }
            }
            State::Attack => {
                let room = (distance - STANDOFF).max(0.0);
                self.max_speed.min(0.7 * (2.0 * self.accel * room).sqrt())
            }
            State::Escort => {
                let room = (distance - STANDOFF).max(0.0);
                self.max_speed.min(0.7 * (2.0 * self.accel * room).sqrt())
            }
            State::Pass | State::Flee => self.max_speed,
            State::Dead | State::Escaped => 0.0,
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
        if self.state == State::Transit && distance <= ARRIVE {
            self.state = State::Hold;
            self.speed = 0.0;
            self.velocity = [0.0; 3];
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

/// The player's legal standing, the port's reconstruction of `DATA_008646` bit
/// 0 and `DATA_008647` (F783, F233, F238): `wanted` is the police alert,
/// `offences` the number of clean ships the player has attacked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Legal {
    pub wanted: bool,
    pub offences: u32,
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

impl Role {
    /// The front laser an NPC of this role carries. F773/F774 fit it from the
    /// model's equipment budget; the port has no budget, so combat roles get one
    /// front mount: pirates a 1MW pulse, police and military a 5MW pulse, and
    /// Thargoids the Thargoid laser.
    pub fn laser_code(self, spec: &Spec) -> Option<u8> {
        if spec.gun_mounts <= 0 {
            return None;
        }
        match self {
            Role::Pirate => Some(laser::PULSE_1MW),
            Role::Police | Role::Military => Some(laser::PULSE_5MW),
            Role::Thargoid => Some(laser::THARGOID),
            Role::Trader | Role::Shuttle => None,
        }
    }
}

/// F688's counts and roles with the port's cap. The original's `DATA_008997`
/// (ships in station bays) branch is omitted because the cache's ship-data
/// table is not loaded by the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub traders: usize,
    pub shuttles: usize,
    pub pirates: usize,
    pub police: usize,
}

/// The counts F688 draws from a system record, before [`MAX_SHIPS`]. `details`
/// is F869's report: `bytes[3]` is `+8`, `bytes[1]` is `+6` and `bytes[2]` is
/// `+7`.
pub fn counts(details: &Details, rng: &mut Random) -> Counts {
    // F688: RND(byte +8) traders, RND(byte +8) shuttles, byte +6 patrol groups,
    // byte +7 police (the F690/Thargoid branch is left out; F688's
    // `F35(0x16)` is not ported, so no Thargoid traffic).
    Counts {
        traders: rng.bounded(details.bytes[3]) as usize,
        shuttles: rng.bounded(details.bytes[3]) as usize,
        pirates: details.bytes[1] as usize,
        police: (details.bytes[2] > 0) as usize,
    }
}

/// Spawn one system's traffic. `spec` and `name` look models up in the cache;
/// models without a statistics block are skipped. The result is capped at
/// [`MAX_SHIPS`], always has at least one trader when there is an anchor and at
/// least one pirate group (the port's playability choice), and is deterministic
/// for a given `rng` state.
pub fn spawn(
    player: [i64; 3],
    forward: [f64; 3],
    anchors: &[Anchor],
    details: &Details,
    allegiance: u32,
    rng: &mut Random,
    spec: impl Fn(u32) -> Option<Spec>,
) -> Vec<NpcShip> {
    let mut counts = counts(details, rng);
    let anchor = anchors.first().copied().unwrap_or(Anchor {
        position: player,
        radius: 1_000_000,
    });
    // The original can roll zero traffic; the port keeps one trader so a fresh
    // system has something to label and shoot, and one pirate group so the
    // damage path is reachable.
    counts.traders = counts.traders.clamp(1, 2);
    counts.shuttles = counts.shuttles.min(1);
    counts.pirates = counts.pirates.clamp(1, 2);
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
        if let Some(ship) = add(Role::Trader, 1, position, anchor.position, rng) {
            ships.push(ship);
        }
    }
    for i in 0..counts.shuttles {
        let position = spread(rng, 1.0 + i as f64 * 2.1, orbit * 0.8);
        if let Some(ship) = add(Role::Shuttle, 0xd, position, anchor.position, rng) {
            ships.push(ship);
        }
    }
    for i in 0..counts.police {
        let position = spread(rng, 2.6 + i as f64 * 2.1, orbit);
        if let Some(ship) = add(Role::Police, 0xe, position, anchor.position, rng) {
            ships.push(ship);
        }
    }
    // F694/F695: a pirate group appears near the player. The leader is F694's
    // lone type-0xa ship in state 5; the escort uses F695's grouped type 0xb
    // list, state 0xb, spawned at its leader's formation offset and carrying the
    // group bounty (F697's offset 2).
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let mut leader = None;
    for i in 0..counts.pirates {
        let (side, back) = if i == 0 { (1.0, 0.0) } else { (-1.0, 0.8e6) };
        let distance = PIRATE_DISTANCE + f64::from(rng.bounded(1_000_000));
        let position = [
            player[0]
                + (forward[0] * distance + right[0] * side * 0.8e6 - forward[0] * back) as i64,
            player[1]
                + (forward[1] * distance + right[1] * side * 0.8e6 - forward[1] * back) as i64,
            player[2]
                + (forward[2] * distance + right[2] * side * 0.8e6 - forward[2] * back) as i64,
        ];
        let Some(mut ship) = add(
            Role::Pirate,
            if i == 0 { 0xa } else { 0xb },
            position,
            player,
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

    fn details() -> Details {
        // Ross 154's F869 report: bytes [128, 1, 2, 3, 0].
        Details {
            words: [34032, 32779],
            bytes: [128, 1, 2, 3, 0],
            allegiance: 134,
        }
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
    fn the_ai_table_is_data_006258() {
        assert_eq!(AI_STATES.len(), 0x25);
        for (index, row) in AI_STATES.iter().enumerate() {
            assert_eq!(usize::from(row.code), index);
        }
        // F789/F752/F725/F782: the approach row.
        let row = ai_state(0x00).unwrap();
        assert_eq!(
            (row.control, row.movement, row.combat, row.notify),
            (Some(789), Some(752), Some(725), Some(782))
        );
        // F785/F759/F725: F694's lone pirate state 5 has no notification slot.
        let row = ai_state(0x05).unwrap();
        assert_eq!(
            (row.control, row.movement, row.combat, row.notify),
            (Some(785), Some(759), Some(725), None)
        );
        // F785/F757/F725/F781: F695's escort state.
        let row = ai_state(0x0b).unwrap();
        assert_eq!(
            (row.control, row.movement, row.combat, row.notify),
            (Some(785), Some(757), Some(725), Some(781))
        );
        // 0xc is all zero; F740 alone is the docked state 0x11.
        assert_eq!(
            ai_state(0x0c).unwrap(),
            &AiState {
                code: 0x0c,
                name: "none",
                control: None,
                movement: None,
                combat: None,
                notify: None,
            }
        );
        assert_eq!(ai_state(0x11).unwrap().control, Some(740));
        assert!(ai_state(0x25).is_none());
        // Every port state names only real rows.
        for state in [
            State::Transit,
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
            }],
            &details(),
            0,
            &mut rng,
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
        let counts = counts(&details(), &mut rng);
        assert!(counts.traders <= 3);
        assert!(counts.pirates == 1);
        assert_eq!(counts.police, 1);
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[Anchor {
                position: [1_000_000, 0, 0],
                radius: 4_500_000,
            }],
            &details(),
            2,
            &mut rng,
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

    #[test]
    fn an_escort_follows_its_leaders_formation() {
        let mut rng = Random::new(21, 22);
        // Two pirates, so F695's escort is spawned when the cap allows.
        let ships = spawn(
            [0, 0, 0],
            [0.0, 0.0, 1.0],
            &[],
            &Details {
                words: [0, 0],
                bytes: [128, 2, 0, 3, 0],
                allegiance: 0,
            },
            0,
            &mut rng,
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
            &details(),
            0,
            &mut rng,
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
            }],
            &details(),
            0,
            &mut rng,
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
