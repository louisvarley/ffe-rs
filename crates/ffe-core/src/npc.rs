//! NPC ships in system flight: the original's ship-type tables and its traffic
//! spawn counts, with a reduced traffic flight state machine.
//!
//! Source: JJFFE 2.8a7 `ffe.asm`/`ffedat.asm`.
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
//! - **F702/F1507** (`ffe.asm:46868`, `89917`) place a spawned ship at a random
//!   phase on an orbit of the raw ffp word `0x5000/0x33`.
//! - **F773** (`ffe.asm:50526`) creates the ship: type byte `+0x118`, equipment,
//!   drive, hull (`+0xe4 = mass * 4`) and shields (`+0xe2 = random * 16`).
//! - **F953/F942** (`ffe.asm:71190`, `70381`) apply damage and destroy the ship;
//!   F1536 (`ffe.asm:91274`) draws model 0x9d (157) at a ship whose shields
//!   absorbed a hit.
//! - The AI tables are `DATA_006258` (`ffedat.asm:9862`); F752/F725/F727 are the
//!   common movement and physics steps and F740/F790/F789 are the station,
//!   depart and attack states.
//!
//! **What this module models and what it leaves out.** The model and type tables,
//! the F688 counts and the spawn roles are the original's. The flight AI is a
//! traffic-level state machine (hold near an anchor, cruise, attack) using the
//! original's idea but not its per-state routines, and the ships do not fire,
//! dock or use hyperspace. Placement uses the original's rule (a random phase
//! on an orbit around an anchor) with a radius in the port's unit system that
//! keeps the ships reachable: the original's raw F702 word is a system-wide
//! orbit, not a point near the player. See `docs/notes/npc.md`.

use crate::galaxy::Details;
use crate::laser::{self, Damage, DamageState};

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

/// The fields of an original ship the NPC needs: its model's two radii, its
/// statistics' mass and shield word. Built by `ffe-rs` from the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub model: u32,
    pub name: String,
    pub radius: i64,
    pub mass: u16,
    pub shield_strength: u16,
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

const TURN_RATE: f64 = 1.2; // radians per second
const STANDOFF: f64 = 1.2e6;
const TRAFFIC_SPEED: f64 = 24_250.0; // original units per second
const PIRATE_SPEED: f64 = 38_800.0;

/// A ship's reduced AI state. The original's states 0x1e/0xd (leaving a station),
/// 0x11 (docked/holding), 0x5/0xb (attack) and the F927 recycle give these
/// traffic-level equivalents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Circle the spawn anchor (F689/F699's station traffic).
    Hold,
    /// Fly to a waypoint.
    Cruise,
    /// Close on the player (F694/F695's pirates, type 0xa/0xb).
    Attack,
    /// Destroyed; the death marker counts down.
    Dead,
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
    pub max_speed: f64,
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
            state: if role.hostile() {
                State::Attack
            } else {
                State::Hold
            },
            anchor,
            orbit_radius,
            phase,
            max_speed: if role.hostile() {
                PIRATE_SPEED
            } else {
                TRAFFIC_SPEED
            },
            flash: 0.0,
            death: 0.0,
        }
    }

    pub fn alive(&self) -> bool {
        self.state != State::Dead
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

    /// One frame of the reduced traffic AI. `dt` is accelerated world seconds.
    /// The ship turns toward its target at [`TURN_RATE`] and moves along its
    /// forward axis; shields and hull do not regenerate.
    pub fn tick(&mut self, dt: f64, player: [i64; 3]) {
        if !self.alive() {
            self.death = (self.death - dt).max(0.0);
            return;
        }
        self.flash = (self.flash - dt).max(0.0);
        let target = match self.state {
            State::Hold => {
                self.phase += (self.max_speed / self.orbit_radius.max(1.0)) * dt;
                offset(self.anchor, self.phase, self.orbit_radius)
            }
            State::Attack => player,
            State::Cruise | State::Dead => self.anchor,
        };
        let to = sub(target, self.position);
        let distance = length(to);
        let desired = if distance > 1.0 {
            scale(to, 1.0 / distance)
        } else {
            self.forward
        };
        self.forward = turn_toward(self.forward, desired, TURN_RATE * dt);
        self.up = upright(self.forward);
        let wanted = match self.state {
            State::Attack if distance < STANDOFF => 0.0,
            _ => self.max_speed,
        };
        self.velocity = scale(self.forward, wanted);
        for axis in 0..3 {
            self.position[axis] += (self.velocity[axis] * dt).round() as i64;
        }
    }

    /// F927's slot recycle / the original ship vanishing: beyond
    /// [`DESPAWN_RANGE`] of the player, or the death marker has run.
    pub fn finished(&self, player: [i64; 3]) -> bool {
        if !self.alive() {
            return self.death <= 0.0;
        }
        distance_between(self.position, player) > DESPAWN_RANGE
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
    let mut ships = Vec::new();
    let mut add = |role: Role, kind: usize, position: [i64; 3], anchor, rng: &mut Random| {
        if ships.len() >= MAX_SHIPS {
            return;
        }
        let mut model = model_for(role, allegiance, rng);
        if model == 0 {
            return;
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
        let Some(mut spec) = picked else { return };
        spec.model = model;
        spec.name = format!("{} {}", spec.name, ships.len() + 1);
        let mut ship = NpcShip::new(&spec, role, position, anchor, rng);
        ship.id = ships.len() as u32;
        if !role.hostile() {
            // Hold the radius the station spawn used, not the default.
            ship.orbit_radius = orbit;
        }
        ships.push(ship);
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
        add(Role::Trader, 1, position, anchor.position, rng);
    }
    for i in 0..counts.shuttles {
        let position = spread(rng, 1.0 + i as f64 * 2.1, orbit * 0.8);
        add(Role::Shuttle, 0xd, position, anchor.position, rng);
    }
    for i in 0..counts.police {
        let position = spread(rng, 2.6 + i as f64 * 2.1, orbit);
        add(Role::Police, 0xe, position, anchor.position, rng);
    }
    // F694/F695: a pirate group appears near the player. The group leader is a
    // type-0xa model and a second ship (when the cap allows) uses the grouped
    // type 0xb list, as F695's escorts do.
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    for i in 0..counts.pirates {
        let side = if i == 0 { 1.0 } else { -1.0 };
        let distance = PIRATE_DISTANCE + f64::from(rng.bounded(1_000_000));
        let position = [
            player[0] + (forward[0] * distance + right[0] * side * 0.8e6) as i64,
            player[1] + (forward[1] * distance + right[1] * side * 0.8e6) as i64,
            player[2] + (forward[2] * distance + right[2] * side * 0.8e6) as i64,
        ];
        add(
            Role::Pirate,
            if i == 0 { 0xa } else { 0xb },
            position,
            player,
            rng,
        );
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
            mass: 25,
            shield_strength: 0x4014,
            cargo: 8,
        })
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
        ship.tick(DEATH_SECONDS, [0, 0, 0]);
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
            trader.tick(0.02, [0, 0, 0]);
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
            pirate.tick(0.02, [0, 0, 0]);
        }
        assert!(distance_between(pirate.position, [0, 0, 0]) < start);
    }
}
