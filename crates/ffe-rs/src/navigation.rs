//! Getting around: the systems the original's generator builds (the hand-coded
//! neighbourhood and a few generated ones), their bodies with the names F881/F893
//! give them, the positions F880 places them at for a game clock, and an autopilot
//! that flies the original flight model to a chosen body with the same keys a
//! player presses. See `docs/NAVIGATION.md`.
use crate::docking::{Contact, Port, CLEARANCE_RANGE};
use crate::original_flight::{Keys, OriginalFlight, MM_PER_SECOND};
use ffe_content::VerifiedCache;
use ffe_core::galaxy::{tables, Tables};
use ffe_core::hyperspace::{plan as jump_plan, Report};
use ffe_core::market::{Conditions, Market, Tables as MarketTables};
use ffe_core::model_math::SineTable;
use ffe_core::names::Names;
use ffe_core::objects::Models as ShipModels;
use ffe_core::objects::{Models, PhysList, Placement};
use ffe_core::sector::{distance as star_distance, Drive, Tables as GalaxyTables};
use ffe_core::surface::{Mesh, Shape, Shapes, SurfaceTools};
use ffe_core::sysgen::{Generator, Params, OBJECT_BYTES};
use ffe_core::traffic::{begin_exit, finish_touch, state, Reply, Stages, Starport};
use ffe_core::RandomState;
use std::collections::HashMap;

/// The reverse thrust's deceleration (metres per second squared, world time).
const BRAKING: f64 = 90.0;

/// The distance (millimetres) left for the real-time approach after a transfer.
const APPROACH: f64 = 1.5e9;

/// Millimetres in an astronomical unit.
pub const AU_MM: f64 = 1.495_978_707e14;

/// What a body is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Star,
    Planet,
    Moon,
    Station,
    Starport,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Star => "star",
            Kind::Planet => "planet",
            Kind::Moon => "moon",
            Kind::Station => "station",
            Kind::Starport => "starport",
        }
    }
}

/// One body of a system.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub name: String,
    pub kind: Kind,
    /// Position in the system frame (millimetres).
    pub position: [i64; 3],
    /// The body's own radius (millimetres), for the arrival distance.
    pub radius: i64,
    /// The object's model and its `+0xa0` (the surface seed).
    pub model: u32,
    pub seed: u32,
    /// The object's orientation rows (its first nine dwords, Q31).
    pub orientation: [[f64; 3]; 3],
    /// F866's signed rotation speed byte (`+0xc8`) for a station; 0 otherwise.
    pub spin: i8,
    /// The object's bay count (`+0xce`): F880 gives a station 3 (7 for kind 0x22).
    pub bays: u16,
}

/// A system that can be visited.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub params: Params,
}

/// The models' collision data, for placing starports on planets.
struct Library<'a>(&'a VerifiedCache);

impl Shapes for Library<'_> {
    fn shape(&self, model: u32) -> Option<Shape<'_>> {
        let cache = self.0;
        let vertices = cache
            .asset(&format!("base:model-vertices/{model:03}"))
            .unwrap_or(&[]);
        let normals = cache
            .asset(&format!("base:model-normals/{model:03}"))
            .unwrap_or(&[]);
        let records = cache.asset("base:model/records")?;
        let record = &records[20 * model as usize..20 * model as usize + 20];
        let table = cache.asset("base:model/collision")?;
        let entry = &table[12 * model as usize..12 * model as usize + 12];
        let word = |at: usize| u32::from_le_bytes(entry[at..at + 4].try_into().unwrap()) as usize;
        let (offset, length, back) = (word(0), word(4), word(8));
        if length == 0 {
            return None;
        }
        let data = &table[12 * 466 + offset..];
        Some(Shape {
            mesh: Mesh {
                vertices,
                normals,
                scale: i32::from_le_bytes(record[0..4].try_into().unwrap()),
                offset: i32::from_le_bytes(record[4..8].try_into().unwrap()),
            },
            stream: &data[back..back + length],
            back: &data[..back],
        })
    }
}

/// The systems on offer: the original's hand-coded neighbourhood, then a few
/// generated ones.
pub fn galaxy(cache: &VerifiedCache) -> Result<Vec<Entry>, String> {
    let blob = cache
        .asset("base:world/system-tables")
        .ok_or("no system tables")?;
    let name_bytes = cache.asset("base:text/names").ok_or("no name text")?;
    let names = Names::new(name_bytes)?;
    let tables_view = Tables::new(blob)?;
    let mut entries = Vec::new();
    for pair in 0..12 {
        let code = tables_view.dword(tables::HANDCODED_INDEX + 8 * pair);
        if code == 0 {
            break;
        }
        let params = Params {
            seed: 0,
            mass: 0,
            stars: 0,
            code,
            population: 20,
            name: [0; 20],
        };
        let mut generator = Generator::new(Tables::new(blob)?);
        generator.set_names(&names);
        generator.generate(&params)?;
        let name = String::from_utf8_lossy(&generator.name_of(0))
            .trim()
            .to_owned();
        entries.push(Entry { name, params });
    }
    for (n, (seed, mass, stars)) in [
        (0x1234_5678, 0x0400_0000, 0),
        (0x2468_ace1, 0x0600_0000, 1),
        (0x0bad_f00d, 0x0300_0000, 2),
        (0x7e57_ab1e, 0x0900_0000, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut name = [0u8; 20];
        let text = format!("Test-{}", n + 1);
        name[..text.len()].copy_from_slice(text.as_bytes());
        entries.push(Entry {
            name: text,
            params: Params {
                seed,
                mass,
                stars,
                code: test_code(cache, n, entries.first().map_or(0, |e| e.params.code)),
                population: 30,
                name,
            },
        });
    }
    Ok(entries)
}

/// A system code for test system `n`: star 0 of the `n`th nearest populated sector to
/// `home` (Sol), so that it has a real place in the galaxy for the jump distances. Falls back
/// to `home` itself when the cache has no galaxy tables.
fn test_code(cache: &VerifiedCache, n: usize, home: u32) -> u32 {
    let Some(tables) = cache
        .asset("base:world/galaxy-tables")
        .and_then(|bytes| GalaxyTables::new(bytes).ok())
    else {
        return home;
    };
    let (hx, hy) = ((home & 0x1fff) as i32, ((home >> 13) & 0x1fff) as i32);
    let mut found = Vec::new();
    'search: for radius in 1..16i32 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let (x, y) = (hx + dx, hy + dy);
                if (0..0x2000).contains(&x)
                    && (0..0x2000).contains(&y)
                    && tables.star_count(x as u32, y as u32, 0) >> 10 > 0
                    && tables.special_free(x as u16, y as u16)
                {
                    found.push((x as u32) | (y as u32) << 13);
                    if found.len() > n {
                        break 'search;
                    }
                }
            }
        }
    }
    found.get(n).copied().unwrap_or(home)
}

/// A system's bodies at a game clock.
pub fn bodies(cache: &VerifiedCache, entry: &Entry, clock: [u32; 2]) -> Result<Vec<Body>, String> {
    let blob = cache
        .asset("base:world/system-tables")
        .ok_or("no system tables")?;
    let name_bytes = cache.asset("base:text/names").ok_or("no name text")?;
    let names = Names::new(name_bytes)?;
    let mut generator = Generator::new(Tables::new(blob)?);
    generator.set_names(&names);
    generator.generate(&entry.params)?;
    let mut system = generator.list.clone();
    let tables_view = Tables::new(blob)?;
    let models = Models {
        records: cache
            .asset("base:model/records")
            .ok_or("no model records")?,
        stats: cache.asset("base:ship/stats").ok_or("no ship statistics")?,
        gameplay: cache
            .asset("base:gameplay/tables")
            .ok_or("no gameplay tables")?,
    };
    let sine = SineTable::from_bytes(cache.asset("base:math/sine").ok_or("no sine table")?)?;
    let library = Library(cache);
    let surface = SurfaceTools {
        shapes: &library,
        terrain_table: cache
            .asset("base:terrain/tables")
            .ok_or("no terrain tables")?,
        terrain_maps: cache.asset("base:terrain/maps").ok_or("no terrain maps")?,
        detail: 0,
    };
    let placement = Placement {
        tables: &tables_view,
        models: &models,
        sine: &sine,
        surface: Some(&surface),
    };
    let mut list = PhysList {
        stamp: 1,
        clock,
        ..PhysList::default()
    };
    placement.create(&mut list, &mut system, clock);
    let mut out: Vec<Body> = Vec::new();
    let mut offsets: Vec<[i64; 3]> = Vec::new();
    let count = generator.count as usize;
    for index in 0..count {
        let at = index * OBJECT_BYTES;
        let kind_code = u16::from_le_bytes([system[at + 0xa], system[at + 0xb]]);
        let parent = u16::from_le_bytes([system[at + 8], system[at + 9]]) as usize;
        let parent_code = (parent != 0).then(|| {
            let p = (parent - 1) * OBJECT_BYTES;
            u16::from_le_bytes([system[p + 0xa], system[p + 0xb]])
        });
        let kind = match kind_code {
            0x23 => Kind::Starport,
            0x20..=0x22 => Kind::Station,
            0x11..=0x1f => Kind::Star,
            _ if parent_code.is_some_and(|c| c < 0x11 && c != 0) => Kind::Moon,
            _ => Kind::Planet,
        };
        let slot = u32::from_le_bytes(system[at + 0x18..at + 0x1c].try_into().unwrap()) as usize;
        let coordinate = |axis: usize| {
            let low = u64::from(list.dword(slot, 0x3e + 8 * axis));
            let high = u64::from(list.dword(slot, 0x42 + 8 * axis));
            (high << 32 | low) as i64
        };
        let field = &system[at + 0x26..at + 0x3f];
        let text = &field[..field.iter().position(|b| *b == 0).unwrap_or(field.len())];
        let radius = ((u64::from(list.dword(slot, 0x13c)) << 32
            | u64::from(list.dword(slot, 0x138))) as i64)
            .max(1_000_000);
        // Positions are relative to the parent body (the system is a tree).
        let mut position = [coordinate(0), coordinate(1), coordinate(2)];
        if parent != 0 {
            let base = offsets.get(parent - 1).copied().unwrap_or([0; 3]);
            for k in 0..3 {
                position[k] += base[k];
            }
        }
        offsets.push(position);
        if kind_code == 0 {
            continue;
        }
        out.push(Body {
            name: String::from_utf8_lossy(text).into_owned(),
            kind,
            position,
            radius,
            model: list.dword(slot, 0x82),
            seed: list.dword(slot, 0xa0),
            orientation: std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    f64::from(list.dword(slot, 4 * (3 * r + c)) as i32) / 2_147_483_648.0
                })
            }),
            spin: if kind == Kind::Station {
                list.byte(slot, 0xc8) as i8
            } else {
                0
            },
            bays: list.word(slot, 0xce),
        });
    }
    Ok(out)
}

/// The first station of a system (else its first starport).
fn first_station(bodies: &[Body]) -> Option<usize> {
    bodies
        .iter()
        .position(|b| b.kind == Kind::Station)
        .or_else(|| bodies.iter().position(|b| b.kind == Kind::Starport))
}

/// Distance in millimetres between a player position and a body.
pub fn distance(from: [i64; 3], to: [i64; 3]) -> f64 {
    (0..3)
        .map(|k| ((to[k] - from[k]) as f64).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// A hyperdrive distance as the galaxy list shows it (units of a hundredth of a light year).
pub fn range_text(units: i32) -> String {
    format!("{}.{:02} LY", units / 100, units % 100)
}

/// A distance as the map shows it.
pub fn describe(mm: f64) -> String {
    if mm >= AU_MM * 0.05 {
        format!("{:.2} AU", mm / AU_MM)
    } else if mm >= 1.0e9 {
        format!("{:.0} Mm", mm / 1.0e9)
    } else {
        format!("{:.1} km", mm / 1.0e6)
    }
}

/// The standoff the autopilot stops at: just off the body's surface or station.
pub fn standoff(body: &Body) -> f64 {
    match body.kind {
        Kind::Station | Kind::Starport => 20.0e6,
        _ => body.radius as f64 * 1.5 + 50.0e6,
    }
}

/// What the autopilot asks for this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Command {
    pub keys: Keys,
    /// The time acceleration step to run at (0 = 1x .. 4 = 10000x).
    pub acceleration: usize,
    pub arrived: bool,
    /// The cosine of the angle between the nose and the target.
    pub aligned: f64,
    /// The point to turn toward this frame (the original's autopilot sets the turn
    /// rates directly; the caller turns the ship with `OriginalFlight::turn_toward`).
    pub face: Option<[i64; 3]>,
    /// A leg too long to fly in real time: where the ship is when only the final
    /// approach is left, and the world seconds the transfer takes at the ship's
    /// thrust and braking. The caller moves the ship and advances the clock.
    pub skip: Option<([i64; 3], f64)>,
}

/// World seconds to cover `metres` from rest to rest: full thrust (about 300 m/s
/// per second) up to the speed the braking (90 m/s per second) can still stop.
pub fn transfer_seconds(metres: f64) -> f64 {
    let (up, down) = (300.0, BRAKING);
    let limit = f64::from(i32::MAX) * MM_PER_SECOND / 1000.0;
    let peak = (2.0 * metres * up * down / (up + down)).sqrt();
    if peak <= limit {
        peak / up + peak / down
    } else {
        let ramp = limit * limit / (2.0 * up) + limit * limit / (2.0 * down);
        limit / up + limit / down + (metres - ramp) / limit
    }
}

/// What the autopilot remembers between frames: the last aim error, to see how
/// fast the nose drifts at the current time acceleration.
#[derive(Debug, Clone, Copy, Default)]
pub struct Memory {
    angle: Option<f64>,
}

/// One frame of the autopilot: turn toward `target` with the steering keys, hold
/// the set speed the distance allows and the time acceleration it can use.
pub fn autopilot(
    flight: &OriginalFlight,
    target: [i64; 3],
    stop: f64,
    memory: &mut Memory,
) -> Command {
    let motion = &flight.player().motion;
    // The integrated orientation drifts off orthonormal; steer by orthonormalised
    // directions (forward first, then right and up with the forward part removed).
    let raw = motion
        .orientation
        .map(|row| row.map(|v| f64::from(v) / 2_147_483_648.0));
    let unit = |v: [f64; 3]| {
        let length = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-9);
        v.map(|c| c / length)
    };
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let without = |v: [f64; 3], w: [f64; 3]| {
        let d = dot3(v, w);
        [v[0] - d * w[0], v[1] - d * w[1], v[2] - d * w[2]]
    };
    let forward_axis = unit(raw[2]);
    let right_axis = unit(without(raw[0], forward_axis));
    let up_axis = unit(without(without(raw[1], forward_axis), right_axis));
    let rows = [right_axis, up_axis, forward_axis];
    let offset: [f64; 3] = std::array::from_fn(|k| (target[k] - motion.position[k]) as f64);
    let range = offset.iter().map(|v| v * v).sum::<f64>().sqrt();
    let dir = offset.map(|v| v / range.max(1.0));
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (right, up, forward) = (dot(dir, rows[0]), dot(dir, rows[1]), dot(dir, rows[2]));
    let mut keys = Keys::default();
    if range <= stop {
        keys.slower = true;
        return Command {
            keys,
            acceleration: 0,
            arrived: flight.set_speed_metres() < 1.0,
            aligned: forward,
            face: None,
            skip: None,
        };
    }
    // Steering at a time acceleration swings the nose by the same factor: steer only
    // in real time, and fall back to it when the aim slips.
    let fast = flight.acceleration_step() > 0;
    if fast && forward < 0.99998 {
        return Command {
            keys,
            acceleration: 0,
            arrived: false,
            aligned: forward,
            face: Some(target),
            skip: None,
        };
    }
    // Too far to fly in real time: once the nose is on the target, hand back the
    // transfer for the caller to carry out (the final approach is flown).
    let cover = range - stop - APPROACH;
    if cover > 4.0 * APPROACH && (right * right + up * up).sqrt() < 0.01 {
        let landing: [i64; 3] =
            std::array::from_fn(|k| target[k] - (dir[k] * (stop + APPROACH)) as i64);
        return Command {
            keys: Keys::default(),
            acceleration: 0,
            arrived: false,
            aligned: forward,
            face: Some(target),
            skip: Some((landing, transfer_seconds(cover / 1000.0))),
        };
    }
    // Speed: the most that can still be stopped within the distance left (the
    // reverse thrust gives about 90 m/s per second), held while the nose swings.
    let remaining = range - stop;
    let metres = remaining / 1000.0;
    let limit = f64::from(i32::MAX) * MM_PER_SECOND / 1000.0; // the largest set speed
    let envelope = (2.0 * BRAKING * 0.4 * metres).sqrt().min(limit);
    let current = flight.set_speed_metres();
    if forward > 0.9995 {
        if current < envelope * 0.8 {
            keys.faster = true;
        } else if current > envelope {
            keys.slower = true;
        }
    } else if forward < 0.98 && current > envelope {
        keys.slower = true;
    }
    // Time acceleration while the nose is on the target; never so much that one
    // frame covers more than a twentieth of what is left (or the braking is missed).
    let velocity = motion
        .velocity
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt()
        * MM_PER_SECOND
        / 1000.0;
    let per_frame = velocity.max(current) * 0.05;
    let factors = [1.0, 10.0, 100.0, 1000.0, 10000.0];
    let angle = (right * right + up * up).sqrt().atan2(forward);
    let drift = memory.angle.map_or(0.0, |last| (angle - last).abs());
    memory.angle = Some(angle);
    let here = factors[flight.acceleration_step()];
    // The turn rates still running (they die slowly), in radians per world second.
    // The transfers cover the long legs; the approach runs in real time.
    let _ = (per_frame, drift, here);
    let acceleration = 0;
    Command {
        keys,
        acceleration,
        arrived: false,
        aligned: forward,
        face: Some(target),
        skip: None,
    }
}

/// Which map is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Closed,
    System,
    Galaxy,
}

/// The playable's navigation state: the systems on offer, where the player is, the
/// bodies of that system at the game clock, the map showing and the autopilot.
pub struct Navigator {
    entries: Vec<Entry>,
    pub current: usize,
    pub bodies: Vec<Body>,
    /// The game clock in world seconds (it moves the planets).
    pub clock: f64,
    /// How far the engaged target has moved since the ship last followed it.
    drift: [i64; 3],
    /// Transfers in a row that have not brought the ship in: after a few it flies.
    skips: u32,
    /// The page of the system list (18 rows a page).
    pub page: usize,
    pub view: View,
    pub selected: usize,
    /// The body the autopilot is flying to.
    pub target: Option<usize>,
    /// The body the player has locked on (clicked in the view or cycled to): what the
    /// autopilot button flies to.
    pub locked: Option<usize>,
    /// The station the player is docked at (or last docked at).
    pub docked: Option<usize>,
    /// Each station's traffic record (F654 and friends), made when first needed.
    traffic: HashMap<usize, Starport>,
    /// The docking or launching stages the player is in, when it is.
    pub sequence: Option<Sequence>,
    /// A message from the autopilot's own clearance request.
    pub notice: Option<String>,
    /// Game seconds not yet handed to the clearance countdowns.
    carry: f64,
    /// The market tables and names (cache v20), when the cache has them.
    market_tables: Vec<u8>,
    /// The galaxy tables (cache v21): star positions and the hyperdrive records.
    galaxy_tables: Vec<u8>,
    /// The ship and fuel the galaxy list and the jump use.
    pub drive: Option<DriveState>,
    /// The markets of the stations visited in this system.
    markets: HashMap<usize, StationMarket>,
    /// The autopilot has reached a station's approach point and is flying into
    /// the slot.
    pub final_leg: bool,
    /// The body whose motion carries the ship between refreshes (the original's
    /// frame of reference, chosen here by distance): the target on its final
    /// approach, a nearby station, or the planet or moon the ship is near.
    frame: Option<usize>,
    /// Docking face and bounding radius (millimetres) of each station model.
    ports: HashMap<u32, (Option<f64>, Option<f64>)>,
    memory: Memory,
    refreshed: f64,
}

/// The player's ship as the hyperdrive sees it: its mass, drive class and the fuel on
/// board (hydrogen, or military fuel for class 10 and up), set by the caller each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveState {
    pub mass: i32,
    pub class: i32,
    pub fuel: i32,
}

/// A hyperspace jump made: where the ship arrives and what it cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jumped {
    pub arrival: [i64; 3],
    /// The distance in the drive's units (about a hundredth of a light year).
    pub distance: i32,
    /// Tonnes of fuel burnt (military fuel leaves as much radioactive waste).
    pub fuel: i32,
    /// Game days the jump took.
    pub days: f64,
}

/// A station's market with the stream its daily drift draws from and the game day it
/// was last brought up to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationMarket {
    pub market: Market,
    rng: RandomState,
    day: u32,
}

/// The most days of drift applied at once (a long transfer or a wait).
const MAX_DRIFT_DAYS: u32 = 30;

/// The player's index in a station's bay table (the original's `DATA_008857`; 0 is an
/// empty bay).
pub const PLAYER: u8 = 1;

/// The player's docking or launching stages at a station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    pub station: usize,
    pub stages: Stages,
    pub launching: bool,
}

/// What a frame of traffic changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Traffic {
    /// A new stage began (its description).
    Stage(&'static str),
    /// The docking stages are over: the ship is docked at the station.
    Docked(usize),
    /// The launch stages put the ship out of the station: it flies again.
    LaunchOut(usize),
    /// The door is open and the bay released: the launch is complete.
    LaunchDone(usize),
}

/// Why traffic control said no (the original's 0x98b7..0x98b9 messages).
pub fn refusal(reply: Reply) -> &'static str {
    match reply {
        Reply::Fined => "docking refused: outstanding fines",
        Reply::Busy => "traffic control is busy with another ship",
        Reply::Full => "all bays are full",
        Reply::Granted(_) => "clearance granted",
    }
}

/// Distance to a station's approach point on its docking axis, beyond the face.
const STATION_APPROACH: f64 = 20.0e6;
/// How close to the approach point the autopilot's first leg stops.
const APPROACH_STOP: f64 = 1.0e6;
/// Within this of a station, its motion carries the ship (200 km).
const STATION_FRAME: f64 = 2.0e8;
/// Within this many radii of a planet or moon, its motion carries the ship.
const WORLD_FRAME_RADII: f64 = 50.0;

/// World seconds to the original's clock words: whole days, then the fraction of a
/// day in 2^-32ths (the orbits turn once in 365 days).
pub fn clock_words(seconds: f64) -> [u32; 2] {
    let days = seconds.max(0.0) / 86_400.0;
    let whole = days.floor();
    [whole as u32, ((days - whole) * 4_294_967_296.0) as u32]
}

impl Navigator {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let entries = galaxy(cache)?;
        let current = entries.iter().position(|e| e.name == "Sol").unwrap_or(0);
        let bodies = bodies(cache, &entries[current], clock_words(0.0))?;
        let docked = first_station(&bodies);
        Ok(Self {
            entries,
            current,
            bodies,
            clock: 0.0,
            drift: [0; 3],
            skips: 0,
            page: 0,
            view: View::Closed,
            selected: 0,
            target: None,
            locked: None,
            docked,
            traffic: HashMap::new(),
            sequence: None,
            notice: None,
            carry: 0.0,
            market_tables: cache
                .asset("base:market/tables")
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            markets: HashMap::new(),
            galaxy_tables: cache
                .asset("base:world/galaxy-tables")
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            drive: None,
            final_leg: false,
            frame: None,
            ports: HashMap::new(),
            memory: Memory::default(),
            refreshed: 0.0,
        })
    }

    /// The station the player is docked at (at the start, the first station of the
    /// system): its name and the point a launch puts the ship at.
    pub fn station(&self) -> Option<(&str, [i64; 3])> {
        let index = self.docked.or_else(|| first_station(&self.bodies))?;
        let body = &self.bodies[index];
        let at = match self.port(index) {
            Some(port) => port.launch_point(),
            None => {
                let mut at = body.position;
                at[2] -= 3 * standoff(body) as i64;
                at
            }
        };
        Some((body.name.as_str(), at))
    }

    /// Record a station model's docking face and bounding radius (millimetres,
    /// from its vertices), for [`Self::port`].
    pub fn set_port_geometry(&mut self, model: u32, front: Option<f64>, radius: Option<f64>) {
        self.ports.insert(model, (front, radius));
    }

    /// The docking port of a station body (`None` for any other body).
    pub fn port(&self, index: usize) -> Option<Port> {
        let body = self.bodies.get(index).filter(|b| b.kind == Kind::Station)?;
        let (front, radius) = self.ports.get(&body.model).copied().unwrap_or((None, None));
        Some(Port::new(body, front, radius))
    }

    /// The nearest station within clearance range of `ship`.
    pub fn nearest_station(&self, ship: [i64; 3]) -> Option<usize> {
        (0..self.bodies.len())
            .filter(|i| self.bodies[*i].kind == Kind::Station)
            .map(|i| (i, distance(ship, self.bodies[i].position)))
            .filter(|(_, range)| *range <= CLEARANCE_RANGE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The commodity tables and names, when the cache has them.
    pub fn market_tables(&self) -> Option<MarketTables<'_>> {
        MarketTables::new(&self.market_tables).ok()
    }

    /// The game day of the clock (the original's day counter).
    pub fn day(&self) -> u32 {
        (self.clock / 86_400.0) as u32
    }

    /// The per-commodity flags of this system's stock string (F870): supply class, price
    /// variance sign and legality; 37 bytes.
    pub fn stock_flags(&self, cache: &VerifiedCache) -> Result<[u8; 37], String> {
        Ok(self.conditions(cache)?.2)
    }

    /// What this system's stock string, population and danger say (F870).
    fn conditions(&self, cache: &VerifiedCache) -> Result<(i64, i64, [u8; 37]), String> {
        let blob = cache
            .asset("base:world/system-tables")
            .ok_or("no system tables")?;
        let tables = Tables::new(blob)?;
        let market = tables.market(self.entries[self.current].params.code);
        Ok((
            i64::from(market.population),
            i64::from(market.danger),
            market.stock,
        ))
    }

    /// The market of station `index`: generated on the first visit (F622's loop for one
    /// station, from a stream seeded by the system and the station, so a market is the
    /// same whenever it is first seen), and brought up to the day by the daily drift
    /// (F633's market half, at most 30 days at once).
    pub fn market(
        &mut self,
        cache: &VerifiedCache,
        index: usize,
    ) -> Result<&mut StationMarket, String> {
        let day = self.day();
        let (population, danger, flags) = self.conditions(cache)?;
        let conditions = Conditions {
            population,
            danger,
            flags: &flags,
        };
        let tables = MarketTables::new(&self.market_tables)
            .map_err(|_| "this cache has no market tables (cache v20)".to_owned())?;
        if let std::collections::hash_map::Entry::Vacant(slot) = self.markets.entry(index) {
            let mut rng = RandomState::new(
                self.entries[self.current].params.code ^ 0x4d41_524b,
                (index as u32 + 1).wrapping_mul(0x9e37_79b9),
            );
            let market = Market::generate(&tables, &conditions, &mut rng)?;
            slot.insert(StationMarket { market, rng, day });
        }
        let station = self.markets.get_mut(&index).expect("just made");
        let days = day.saturating_sub(station.day).min(MAX_DRIFT_DAYS);
        for _ in 0..days {
            station
                .market
                .daily(&tables, &conditions, &mut station.rng)?;
        }
        station.day = day.max(station.day);
        Ok(station)
    }

    /// The hold of a ship model, tonnes (`ffe_core::objects::Models::cargo_space`).
    pub fn cargo_space(&self, cache: &VerifiedCache, ship: u32) -> i32 {
        let (Some(records), Some(stats), Some(gameplay)) = (
            cache.asset("base:model/records"),
            cache.asset("base:ship/stats"),
            cache.asset("base:gameplay/tables"),
        ) else {
            return 0;
        };
        if 48 * (ship as usize + 1) > stats.len() || 20 * (ship as usize + 1) > records.len() {
            return 0;
        }
        ShipModels {
            records,
            stats,
            gameplay,
        }
        .cargo_space(ship)
        .unwrap_or(0)
        .max(0)
    }

    /// The traffic record of a station, made on first use with its bay count.
    pub fn starport(&mut self, index: usize) -> &mut Starport {
        let bays = self.bodies.get(index).map_or(3, |b| b.bays);
        self.traffic
            .entry(index)
            .or_insert_with(|| Starport::new(bays))
    }

    /// The station has given the player clearance (F657).
    pub fn cleared(&self, index: usize) -> bool {
        self.traffic
            .get(&index)
            .is_some_and(|port| port.is_docking(PLAYER))
    }

    /// Ask the nearest station for docking clearance, as the comms screen does
    /// (F655): traffic control may be busy with another ship, or have no bay.
    pub fn request_clearance(&mut self, ship: [i64; 3]) -> Option<(usize, Reply)> {
        let index = self.nearest_station(ship)?;
        let reply = self.starport(index).request_docking(PLAYER, true, 0);
        Some((index, reply))
    }

    /// Some station holds the player's clearance.
    pub fn cleared_anywhere(&self) -> bool {
        (0..self.bodies.len()).any(|i| self.cleared(i))
    }

    /// Give clearance up again (F651).
    pub fn cancel_clearance(&mut self) -> bool {
        let Some(index) = (0..self.bodies.len()).find(|i| self.cleared(*i)) else {
            return false;
        };
        self.starport(index).release(PLAYER);
        true
    }

    /// The ship has touched the pad of station `index` with clearance (F652): the
    /// timed docking stages begin.
    pub fn begin_docking(&mut self, index: usize) -> bool {
        if !self.starport(index).touch(PLAYER) {
            return false;
        }
        self.target = None;
        self.final_leg = false;
        self.sequence = Some(Sequence {
            station: index,
            stages: Stages::touched(false),
            launching: false,
        });
        true
    }

    /// Ask the station the player is docked at for permission to launch (F654) and
    /// begin the launch stages. `Err` says why not.
    pub fn begin_launch(&mut self) -> Result<(), String> {
        let index = self.docked.or_else(|| first_station(&self.bodies));
        let Some(index) = index else {
            return Err("No station to launch from.".to_owned());
        };
        if self.sequence.is_some() {
            return Err("Already launching.".to_owned());
        }
        if !self.starport(index).request(PLAYER) {
            return Err("Launch refused: traffic control is busy.".to_owned());
        }
        self.docked = Some(index);
        self.sequence = Some(Sequence {
            station: index,
            stages: Stages::launch_requested(),
            launching: true,
        });
        Ok(())
    }

    /// The lift models this frame's stage draws, and whether it is a launch (drawn alone,
    /// inside the station) rather than a docking (over the outside view).
    pub fn lift_scene(&self) -> Option<(Vec<ffe_core::lift::Object>, bool)> {
        let sequence = self.sequence?;
        let (stage, remaining) = sequence.stages.drawn?;
        Some((
            ffe_core::lift::objects(stage, remaining)?,
            sequence.launching,
        ))
    }

    /// The ship is held still: it is in the lift of a docking sequence.
    pub fn holding(&self) -> bool {
        self.sequence.is_some_and(|s| !s.launching)
    }

    /// One frame of traffic: every station's door and clearance countdown, and the
    /// player's stages. `tics` are the game tics of the frame, `seconds` game seconds.
    pub fn tick_traffic(&mut self, tics: u32, seconds: f64) -> Vec<Traffic> {
        let mut events = Vec::new();
        self.carry += seconds;
        let whole = self.carry.floor();
        self.carry -= whole;
        for port in self.traffic.values_mut() {
            port.move_door(tics);
            if whole >= 1.0 {
                port.elapse(whole as u32);
            }
        }
        let Some(mut sequence) = self.sequence else {
            return events;
        };
        let station = sequence.station;
        let model = self.bodies.get(station).map_or(0, |b| b.model);
        let thargoid = model == 0x45 || model == 0x4f;
        let door = self.traffic.get(&station).map_or(0xffff, |p| p.door);
        let left = sequence.stages.step(tics as i32, thargoid, door);
        if let Some(left) = left {
            let now = sequence.stages.state;
            if left == state::ALIGN || left == state::STARPORT_ALIGN {
                finish_touch(self.starport(station));
            }
            match now {
                state::DOCKED => {
                    self.sequence = None;
                    self.dock(station);
                    events.push(Traffic::Docked(station));
                    return events;
                }
                state::LAUNCHING_STATION | state::LAUNCHING_STARPORT => {
                    begin_exit(self.starport(station));
                    events.push(Traffic::LaunchOut(station));
                }
                state::AFTER_TAKEOFF | state::TAKEOFF => {
                    self.starport(station).release(PLAYER);
                    self.sequence = None;
                    events.push(Traffic::LaunchDone(station));
                    return events;
                }
                _ => {
                    let text = state::describe(now);
                    if !text.is_empty() {
                        events.push(Traffic::Stage(text));
                    }
                }
            }
        }
        self.sequence = Some(sequence);
        events
    }

    /// The first station whose slot or hull the ship at `ship`, moving at `speed`
    /// metres per second (relative to the station's frame), is touching.
    pub fn contact(&self, ship: [i64; 3], speed: f64) -> Option<(usize, Contact)> {
        (0..self.bodies.len())
            .filter(|i| self.bodies[*i].kind == Kind::Station)
            .filter(|i| distance(ship, self.bodies[*i].position) < CLEARANCE_RANGE)
            .find_map(|index| {
                let port = self.port(index)?;
                let contact = port.contact(ship, speed, self.cleared(index));
                (contact != Contact::None).then_some((index, contact))
            })
    }

    /// The ship has docked at `index`: the autopilot ends.
    pub fn dock(&mut self, index: usize) {
        self.docked = Some(index);
        self.target = None;
        self.final_leg = false;
    }

    /// The nearest planet or moon to a point (what the ship faces on launch).
    pub fn nearest_world(&self, from: [i64; 3]) -> Option<[i64; 3]> {
        self.bodies
            .iter()
            .filter(|b| matches!(b.kind, Kind::Planet | Kind::Moon))
            .min_by(|a, b| distance(from, a.position).total_cmp(&distance(from, b.position)))
            .map(|b| b.position)
    }

    pub fn system_name(&self) -> &str {
        &self.entries[self.current].name
    }

    pub fn open(&mut self, view: View) {
        self.view = if self.view == view {
            View::Closed
        } else {
            view
        };
        self.page = 0;
        self.selected = match self.view {
            View::Galaxy => self.current,
            _ => 0,
        };
    }

    /// Move the selection within the list showing.
    pub fn step_selection(&mut self, delta: i32) {
        let length = match self.view {
            View::Galaxy => self.entries.len(),
            View::System => self.listed().len(),
            View::Closed => return,
        };
        if length > 0 {
            self.selected = (self.selected as i32 + delta).rem_euclid(length as i32) as usize;
        }
    }

    /// Move the planets: the game clock advances `seconds` of world time.
    pub fn advance(&mut self, cache: &VerifiedCache, seconds: f64) {
        self.clock += seconds;
        // Placing a system is cheap but not free: once a second of world time, and
        // on every real second the autopilot flies.
        if self.clock - self.refreshed >= 1.0 || self.clock < self.refreshed {
            self.refresh(cache);
        }
    }

    /// A transfer: the clock jumps and the ship is placed to match, so the target's
    /// drift over it is not carried along.
    pub fn transfer(&mut self, cache: &VerifiedCache, seconds: f64) {
        self.clock += seconds;
        self.refresh(cache);
        self.drift = [0; 3];
    }

    pub fn refresh(&mut self, cache: &VerifiedCache) {
        if let Ok(list) = bodies(cache, &self.entries[self.current], clock_words(self.clock)) {
            if list.len() == self.bodies.len() {
                if let Some(t) = self.frame {
                    for k in 0..3 {
                        self.drift[k] += list[t].position[k] - self.bodies[t].position[k];
                    }
                }
                self.bodies = list;
            }
        }
        self.refreshed = self.clock;
    }

    /// The position of system `index` (F852: its star in its sector), 16.16 fixed point.
    pub fn system_position(&self, index: usize) -> Option<[i32; 3]> {
        let tables = GalaxyTables::new(&self.galaxy_tables).ok()?;
        tables.system_position(self.entries.get(index)?.params.code)
    }

    /// The position of the system the player is in.
    pub fn current_position(&self) -> Option<[i32; 3]> {
        self.system_position(self.current)
    }

    /// The system on offer with this code, if any.
    pub fn entry_for_code(&self, code: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.params.code == code)
    }

    /// What the hyperdrive says about a jump of `distance` (F899: nothing is burnt).
    pub fn report_for(&self, distance: i32) -> Option<Report> {
        let drive = self.drive?;
        jump_plan(
            &self.drives()?,
            drive.mass,
            drive.class,
            distance,
            drive.fuel,
            false,
            true,
        )
        .ok()
    }

    /// The drive's range for the player's ship (what F900 leaves in `DATA_008907`).
    pub fn reach(&self) -> Option<i32> {
        let drive = self.drive?;
        jump_plan(&self.drives()?, drive.mass, drive.class, 0, 0, false, true)
            .ok()
            .map(|report| report.max_range)
    }

    /// The distance from the current system to system `index`, in the drive's units.
    pub fn range_to(&self, index: usize) -> Option<i32> {
        Some(star_distance(
            self.system_position(self.current)?,
            self.system_position(index)?,
        ))
    }

    fn drives(&self) -> Option<Vec<Drive>> {
        let tables = GalaxyTables::new(&self.galaxy_tables).ok()?;
        (0..15).map(|class| tables.drive(class)).collect()
    }

    /// What the hyperdrive says about a jump to system `index` (F899: nothing is burnt).
    pub fn jump_report(&self, index: usize) -> Option<(i32, Report)> {
        let (drive, distance) = (self.drive?, self.range_to(index)?);
        let report = jump_plan(
            &self.drives()?,
            drive.mass,
            drive.class,
            distance,
            drive.fuel,
            false,
            true,
        )
        .ok()?;
        Some((distance, report))
    }

    /// Jump to system `index` through the hyperdrive (F901): the fuel is checked and the
    /// game clock moves by the jump time; the caller burns `Jumped::fuel`. Refused with a
    /// reason when the system is out of range, the same system, or the hold has too little fuel.
    pub fn hyperspace(&mut self, cache: &VerifiedCache, index: usize) -> Result<Jumped, String> {
        let Some((distance, report)) = self.jump_report(index) else {
            return Err("No hyperdrive data for this ship or system.".to_owned());
        };
        if index == self.current {
            return Err("You are already in this system.".to_owned());
        }
        if report.result < 0 {
            return Err(if report.fuel_needed < 0 {
                format!(
                    "Out of range: {} against the drive's {}.",
                    range_text(distance),
                    range_text(report.max_range)
                )
            } else {
                format!("Not enough fuel: the jump needs {} t.", report.fuel_needed)
            });
        }
        let days = f64::from(report.jump_time.unwrap_or(0)) / 65_536.0;
        self.clock += days * 86_400.0;
        let arrival = self.jump(cache, index)?;
        Ok(Jumped {
            arrival,
            distance,
            fuel: report.fuel_needed,
            days,
        })
    }

    /// Enter system `index` at once (no fuel, no time): the bodies are rebuilt and the arrival
    /// point (just off the first planet, or the star) returned.
    pub fn jump(&mut self, cache: &VerifiedCache, index: usize) -> Result<[i64; 3], String> {
        if index >= self.entries.len() {
            return Err("no such system".to_owned());
        }
        self.current = index;
        self.bodies = bodies(cache, &self.entries[index], clock_words(self.clock))?;
        self.refreshed = self.clock;
        self.target = None;
        self.locked = None;
        self.docked = None;
        self.traffic.clear();
        self.markets.clear();
        self.sequence = None;
        self.final_leg = false;
        self.frame = None;
        self.drift = [0; 3];
        self.memory = Memory::default();
        let anchor = self
            .bodies
            .iter()
            .find(|b| b.kind == Kind::Planet)
            .or_else(|| self.bodies.first())
            .ok_or("empty system")?;
        let off = (standoff(anchor) * 4.0) as i64;
        Ok([
            anchor.position[0],
            anchor.position[1],
            anchor.position[2] - off,
        ])
    }

    /// The bodies the navigation computer lists: 18 a page (two columns of nine, as the
    /// original's), the star and planets first, then the ports and stations, then moons.
    pub fn listed(&self) -> Vec<usize> {
        let rank = |k: Kind| match k {
            Kind::Star => 0,
            Kind::Planet => 1,
            Kind::Starport => 2,
            Kind::Station => 3,
            Kind::Moon => 4,
        };
        let mut all: Vec<usize> = (0..self.bodies.len()).collect();
        all.sort_by_key(|i| (rank(self.bodies[*i].kind), *i));
        all.into_iter()
            .skip(18 * self.pages_clamped())
            .take(18)
            .collect()
    }

    fn pages_clamped(&self) -> usize {
        self.page.min(self.pages().saturating_sub(1))
    }

    pub fn pages(&self) -> usize {
        self.bodies.len().div_ceil(18).max(1)
    }

    pub fn turn_page(&mut self, delta: i32) {
        self.page = (self.page as i32 + delta).rem_euclid(self.pages() as i32) as usize;
        self.selected = 0;
    }

    /// The body under the selected row of the system list.
    pub fn selected_body(&self) -> Option<usize> {
        self.listed().get(self.selected).copied()
    }

    /// The original's row click: lock on to the body and engage the autopilot (it sets
    /// the player's target and its autopilot mode, then closes the list).
    pub fn engage(&mut self) -> Option<&Body> {
        let index = self.selected_body()?;
        self.target = Some(index);
        self.final_leg = false;
        self.memory = Memory::default();
        self.skips = 0;
        self.view = View::Closed;
        self.bodies.get(index)
    }

    /// Lock on to a body of this system (a click on it, or the target button).
    pub fn lock(&mut self, index: usize) -> Option<&Body> {
        let body = self.bodies.get(index)?;
        self.locked = Some(index);
        Some(body)
    }

    /// Lock the next body by range from `ship` (the previous one with `step` -1),
    /// starting from the nearest.
    pub fn cycle_lock(&mut self, ship: [i64; 3], step: i32) -> Option<&Body> {
        let mut order: Vec<usize> = (0..self.bodies.len()).collect();
        order.sort_by(|a, b| {
            distance(ship, self.bodies[*a].position)
                .total_cmp(&distance(ship, self.bodies[*b].position))
        });
        let next = match self.locked.and_then(|l| order.iter().position(|i| *i == l)) {
            Some(at) => (at as i32 + step).rem_euclid(order.len() as i32) as usize,
            None => 0,
        };
        let index = *order.get(next)?;
        self.lock(index)
    }

    /// Engage the autopilot on the locked body: it flies there and, at a station, asks
    /// for clearance and docks.
    pub fn engage_locked(&mut self) -> Option<&Body> {
        let index = self.locked?;
        self.target = Some(index);
        self.final_leg = false;
        self.memory = Memory::default();
        self.skips = 0;
        self.view = View::Closed;
        self.bodies.get(index)
    }

    /// The ship's position carried along with the body whose frame it is in: the
    /// engaged target on its final approach (the autopilot cannot catch a body
    /// moving at 30 km/s), else a station or world close by, so that a station
    /// stays where the ship left it. Also picks the frame for the next refresh.
    pub fn follow(&mut self, ship: [i64; 3]) -> Option<[i64; 3]> {
        let drift = std::mem::take(&mut self.drift);
        self.frame = self.choose_frame(ship);
        (drift != [0; 3]).then(|| std::array::from_fn(|k| ship[k] + drift[k]))
    }

    fn choose_frame(&self, ship: [i64; 3]) -> Option<usize> {
        if let Some(t) = self.target {
            if let Some(body) = self.bodies.get(t) {
                if distance(ship, body.position) <= 10.0 * APPROACH + standoff(body) {
                    return Some(t);
                }
            }
        }
        let nearest = |kinds: &[Kind], reach: &dyn Fn(&Body) -> f64| {
            (0..self.bodies.len())
                .filter(|i| kinds.contains(&self.bodies[*i].kind))
                .map(|i| {
                    let body = &self.bodies[i];
                    (i, distance(ship, body.position) / reach(body))
                })
                .filter(|(_, ratio)| *ratio <= 1.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        nearest(&[Kind::Station, Kind::Starport], &|_| STATION_FRAME).or_else(|| {
            nearest(&[Kind::Planet, Kind::Moon], &|b| {
                b.radius as f64 * WORLD_FRAME_RADII
            })
        })
    }

    pub fn disengage(&mut self) {
        self.target = None;
        self.final_leg = false;
    }

    /// The autopilot's command for this frame, when engaged.
    pub fn command(
        &mut self,
        cache: &VerifiedCache,
        flight: &OriginalFlight,
    ) -> Option<(Command, Body)> {
        let index = self.target?;
        let body = self.bodies.get(index)?.clone();
        // A station is flown to in two legs, as the original's autopilot docks: to
        // a point out on the docking axis, then (with clearance) into the slot.
        let port = self.port(index);
        let (offset, stop) = match port {
            Some(port) if self.final_leg => ([0.0; 3], port.front * 0.92),
            Some(port) => (
                port.axis.map(|a| a * (port.front + STATION_APPROACH)),
                APPROACH_STOP,
            ),
            None => ([0.0; 3], standoff(&body)),
        };
        let aim = |at: [i64; 3]| -> [i64; 3] {
            std::array::from_fn(|k| at[k] + offset[k].round() as i64)
        };
        let mut command = autopilot(flight, aim(body.position), stop, &mut self.memory);
        if port.is_some() && command.arrived && !self.final_leg {
            // The approach point: ask for clearance and fly in.
            self.memory = Memory::default();
            match self.starport(index).request_docking(PLAYER, true, 0) {
                Reply::Granted(_) => {
                    self.final_leg = true;
                    command.arrived = false;
                }
                other => {
                    self.notice = Some(format!("{}: {}.", body.name, refusal(other)));
                    command.arrived = true;
                }
            }
        }
        if let Some((_, seconds)) = command.skip {
            // The body moves while the ship travels: aim for where it will be.
            let ship = flight.player().motion.position;
            let mut seconds = seconds;
            let mut future = body.position;
            for _ in 0..6 {
                let Ok(list) = bodies(
                    cache,
                    &self.entries[self.current],
                    clock_words(self.clock + seconds),
                ) else {
                    break;
                };
                let Some(there) = list.get(index) else { break };
                future = aim(there.position);
                let range = distance(ship, future);
                seconds = transfer_seconds((range - stop - APPROACH).max(0.0) / 1000.0);
            }
            let range = distance(ship, future);
            let landing: [i64; 3] = std::array::from_fn(|k| {
                let unit = (future[k] - ship[k]) as f64 / range.max(1.0);
                future[k] - (unit * (stop + APPROACH)) as i64
            });
            self.skips += 1;
            if self.skips > 3 {
                // A target that outruns the transfers: fly the rest.
                command.skip = None;
            } else {
                command.skip = Some((landing, seconds));
            }
        } else {
            self.skips = 0;
        }
        Some((command, body))
    }

    /// The autopilot's leg into a station's slot is running.
    pub fn docking(&self) -> bool {
        self.final_leg && self.target.is_some()
    }

    /// The lines of the map showing, the selected one marked.
    pub fn lines(&self, ship: [i64; 3]) -> Vec<String> {
        let mark = |i: usize| if i == self.selected { ">" } else { " " };
        match self.view {
            View::Closed => Vec::new(),
            View::Galaxy => self
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let here = if i == self.current { "*" } else { " " };
                    let detail = if i == self.current {
                        String::new()
                    } else {
                        match self.jump_report(i) {
                            Some((distance, report)) if report.result >= 0 => {
                                format!("  {}  {} T", range_text(distance), report.fuel_needed)
                            }
                            Some((distance, report)) if report.fuel_needed < 0 => {
                                format!("  {}  TOO FAR", range_text(distance))
                            }
                            Some((distance, report)) => format!(
                                "  {}  NEEDS {} T",
                                range_text(distance),
                                report.fuel_needed
                            ),
                            None => String::new(),
                        }
                    };
                    format!("{}{} {}{}", mark(i), here, e.name, detail)
                })
                .collect(),
            View::System => self
                .listed()
                .into_iter()
                .enumerate()
                .map(|(row, i)| {
                    let b = &self.bodies[i];
                    let flag = if self.target == Some(i) { "=>" } else { "  " };
                    format!(
                        "{}{} {} ({}) {}",
                        mark(row),
                        flag,
                        b.name,
                        b.kind.label(),
                        describe(distance(ship, b.position))
                    )
                })
                .collect(),
        }
    }

    /// The bodies as map points (x, y in -1..1 on a log scale from the ship, kind).
    pub fn points(&self, ship: [i64; 3]) -> Vec<(f32, f32, Kind, bool)> {
        let reach = self
            .bodies
            .iter()
            .map(|b| distance(ship, b.position))
            .fold(1.0e9, f64::max);
        self.bodies
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let dx = (b.position[0] - ship[0]) as f64;
                let dz = (b.position[2] - ship[2]) as f64;
                let range = (dx * dx + dz * dz).sqrt().max(1.0);
                // Log radius so the moons and the outer planets both show.
                let radius = ((range / 1.0e8).ln_1p() / (reach / 1.0e8).ln_1p()).clamp(0.0, 1.0);
                (
                    (dx / range * radius) as f32,
                    (dz / range * radius) as f32,
                    b.kind,
                    self.selected_body() == Some(i),
                )
            })
            .collect()
    }
}
