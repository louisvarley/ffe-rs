//! Getting around: the systems the original's generator builds (the hand-coded
//! neighbourhood and a few generated ones), their bodies with the names F881/F893
//! give them, the positions F880 places them at for a game clock, and an autopilot
//! that flies the original flight model to a chosen body with the same keys a
//! player presses. See `docs/NAVIGATION.md`.
use crate::docking::{Contact, Port, CLEARANCE_RANGE};
use crate::original_flight::{Keys, OriginalFlight, MM_PER_SECOND, UNITS_PER_FLIGHT_UNIT};
use ffe_content::VerifiedCache;
use ffe_core::effects::{self, Effect};
use ffe_core::galaxy::{tables, Tables};
use ffe_core::hyperspace::{plan as jump_plan, Report};
use ffe_core::market::{Conditions, Market, Tables as MarketTables};
use ffe_core::model_math::SineTable;
use ffe_core::names::Names;
use ffe_core::npc::{self, Anchor, NpcShip};
use ffe_core::objects::Models as ShipModels;
use ffe_core::objects::{Models, PhysList, Placement};
use ffe_core::pads::Pads;
use ffe_core::sector::{distance as star_distance, Drive, Strings, Tables as GalaxyTables};
use ffe_core::surface::{Mesh, Shape, Shapes, SurfaceTools};
use ffe_core::sysgen::{Generator, Params, OBJECT_BYTES};
use ffe_core::traffic::{
    begin_exit, finish_touch, state, Landing, Reply, Stages, Starport, Takeoff, TakeoffStep,
};
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

    /// The original navigation computer lists places to dock at: F880 sets object
    /// flag `+0x14c` bit 0x10 only for type codes 0x20-0x23, its stations and
    /// starports (ffe.asm 64695-64699), and F1099 walks the same objects.
    pub fn is_place(self) -> bool {
        matches!(self, Kind::Station | Kind::Starport)
    }
}

/// One body of a system.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub name: String,
    pub kind: Kind,
    /// Position in the system frame (millimetres).
    pub position: [i64; 3],
    /// The body's outer radius (`+0x138`, millimetres): the atmosphere's for a
    /// planet, the hull's for a station.
    pub radius: i64,
    /// The body's inner radius (`+0x140`, millimetres): the solid surface's for
    /// a planet.
    pub inner_radius: i64,
    /// The system-object index of the body this one orbits, when it has one.
    pub parent: Option<usize>,
    /// The object's model and its `+0xa0` (the surface seed).
    pub model: u32,
    pub seed: u32,
    /// The object's slot in the physics list (`+0x18` of its system record): the
    /// index F1531/F1532 use to find the physics object.
    pub slot: usize,
    /// The object's first nine dwords, Q31: its orientation rows as the original
    /// keeps them (F650 turns the pads by these, so the raw values are kept).
    pub rows: [i32; 9],
    /// The object's class byte (`+0x14c`).
    pub class: u8,
    /// Whether the class byte's 0x40 marks an open-air starport (F650's branch).
    pub open_air: bool,
    /// The object's orientation rows (its first nine dwords, Q31), as floats.
    pub orientation: [[f64; 3]; 3],
    /// F866's signed rotation speed byte (`+0xc8`) for a station; 0 otherwise.
    pub spin: i8,
    /// The object's bay count (`+0xce`): F880 gives a station 3 (7 for kind 0x22).
    pub bays: u16,
}

/// The original navigation computer's entries: stations and starports, in system
/// order. F927 allocates object slots from 0x72 downwards, so F1099's walk from
/// 0x72 down (ffe.asm 80518-80527) visits the system list in creation order,
/// which is the order of [`Navigator::bodies`].
pub fn place_indices(bodies: &[Body]) -> Vec<usize> {
    bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.kind.is_place())
        .map(|(index, _)| index)
        .collect()
}

/// A system that can be visited.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub params: Params,
}

/// The models' collision data, for placing starports on planets.
pub struct Library<'a>(pub &'a VerifiedCache);

impl Shapes for Library<'_> {
    fn model(&self, model: u32) -> Option<ffe_core::objects::ModelInfo> {
        ffe_core::objects::ModelInfo::read(
            self.0.asset("base:model/records")?,
            self.0.asset("base:ship/stats")?,
            model,
        )
    }

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
        let inner_radius = ((u64::from(list.dword(slot, 0x144)) << 32
            | u64::from(list.dword(slot, 0x140))) as i64)
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
        let rows: [i32; 9] = std::array::from_fn(|k| list.dword(slot, 4 * k) as i32);
        let class = list.byte(slot, 0x14c);
        out.push(Body {
            name: String::from_utf8_lossy(text).into_owned(),
            kind,
            position,
            radius,
            inner_radius,
            parent: parent.checked_sub(1),
            model: list.dword(slot, 0x82),
            seed: list.dword(slot, 0xa0),
            slot,
            rows,
            class,
            open_air: class & ffe_core::pads::OPEN_AIR_CLASS != 0,
            orientation: std::array::from_fn(|r| {
                std::array::from_fn(|c| f64::from(rows[3 * r + c]) / 2_147_483_648.0)
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

/// A station in low orbit around a planet or moon, so a launch from it puts that
/// world in the sky; the closest orbit wins. `None` when the system has none.
fn orbital_station(bodies: &[Body]) -> Option<usize> {
    bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.kind == Kind::Station)
        .filter_map(|(index, station)| {
            let away = bodies
                .iter()
                .filter(|body| matches!(body.kind, Kind::Planet | Kind::Moon))
                .map(|body| distance(station.position, body.position) / body.radius.max(1) as f64)
                .min_by(f64::total_cmp)?;
            (away < 10.0).then_some((index, away))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
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

/// A unit vector, for the docking axis.
fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    if length < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        v.map(|c| c / length)
    }
}

/// A body as the collision scan's [`ffe_core::frames::Body`]: `+0x138`/`+0x140`
/// radii, the Q31 orientation, the class byte and the terrain seed. Its position
/// is the body's absolute position (the caller passes it as the scan's parent,
/// which has no parent of its own).
pub fn landing_body(body: &Body, model: &ffe_core::objects::ModelInfo) -> ffe_core::frames::Body {
    ffe_core::frames::Body {
        position: body.position,
        parent: 0,
        index: 2,
        kind: if body.kind == Kind::Starport { 1 } else { 0 },
        locked: body.kind == Kind::Starport,
        orientation: std::array::from_fn(|k| {
            (body.orientation[k / 3][k % 3] * 2_147_483_647.0) as i32
        }),
        outer_radius: body.radius,
        inner_radius: body.inner_radius,
        shape: model.class(),
        model: body.model,
        seed: body.seed,
        ..ffe_core::frames::Body::default()
    }
}

impl Navigator {
    /// The body whose frame the ship is in for F569's collision scan: the nearest
    /// starport within its own hull plus the ship's radius, else the nearest
    /// planet or moon whose outer (atmosphere) radius the ship is inside. `None`
    /// in free space.
    pub fn landing_parent(&self, from: [i64; 3], ship_radius: i64) -> Option<usize> {
        let reach =
            |body: &Body| distance(from, body.position) - (body.radius + ship_radius) as f64;
        self.bodies
            .iter()
            .enumerate()
            .filter(|(_, body)| body.kind == Kind::Starport)
            .map(|(index, body)| (reach(body), index))
            .filter(|(gap, _)| *gap <= 0.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, index)| index)
            .or_else(|| {
                self.bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, body)| matches!(body.kind, Kind::Planet | Kind::Moon))
                    .map(|(index, body)| (reach(body), index))
                    .filter(|(gap, _)| *gap <= 0.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, index)| index)
            })
    }
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

/// The radius the flight view draws a body's surface at, in flight units.
///
/// In flight the surface model's base sphere is the inner radius word
/// (`+0x140`); it is what keeps the eye outside the shell on an approach. Once
/// the ship is settled on the ground the view draws the outer radius word
/// (`+0x138`), the same radius F1533 and the terrain contact use, so a rough
/// landing sits on the drawn ground. F879's starports are on the terrain at
/// about 0.9 of the outer radius.
pub fn surface_radius(body: &Body, landed: bool) -> f64 {
    let radius = if landed {
        body.radius
    } else {
        body.inner_radius
    };
    radius as f64 / UNITS_PER_FLIGHT_UNIT
}

/// The standoff the autopilot stops at: just off the body's surface or station.
pub fn standoff(body: &Body) -> f64 {
    match body.kind {
        Kind::Station => 20.0e6,
        // An open-air starport is tens of kilometres across (F880 gives it a
        // 20-32 km hull), so 20 km would be inside it.
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
    /// The ffe-rs system map (Tab): every body, as a navigation aid.
    System,
    /// The original navigation computer (the console's `n` icon): the system's
    /// stations and starports in the original's two columns of nine.
    Places,
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
    /// The clock this game began at: the new game's absolute start, or 0 for the
    /// sandbox. Dates and the market's day use the difference.
    pub start_clock: f64,
    /// The original's new-game fields, when this navigator started one.
    pub new_game: Option<NewGameState>,
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
    /// The NPC traffic of the current system (F688/F635), ticked in flight.
    pub ships: Vec<NpcShip>,
    /// The player's legal standing (`DATA_008646` bit 0 / `DATA_008647`): the
    /// police alert that makes police and military ships hostile.
    pub legal: npc::Legal,
    /// F675's carried ships: the pirates close enough to follow a hyperspace
    /// jump, restored by F680 when the new system is entered.
    persisted: Vec<npc::Persisted>,
    /// The player's position at the last traffic tick, for F675's offsets.
    player_at: [i64; 3],
    /// The system's short-lived visuals: F939 hit particles, F942 destruction
    /// objects and F926 debris (see `ffe_core::effects`). They outlive the ship
    /// that spawned them and are not saved.
    pub effects: Vec<Effect>,
    /// The id of the ship locked as the target (F1020's target byte for ships;
    /// the port keeps bodies in `locked` and ships here).
    pub ship_lock: Option<u32>,
    /// The combat target (`DATA_008875`) the missile lock-on uses. F1096/F941
    /// launch at this target, not at the navigation target `ship_lock`.
    pub missile_lock: Option<u32>,
    /// The missiles in flight (F941 objects, model 4..13).
    pub missiles: crate::missile::MissileField,
    /// The system index the traffic was spawned for: a killed ship is not
    /// restored until the system is entered again.
    spawned: Option<usize>,
    /// The docking or launching stages the player is in, when it is.
    pub sequence: Option<Sequence>,
    /// The open-air starport takeoff (`0x30`/`0x24`/`0x26`/`0x00`), when the
    /// player has launched from a pad.
    pub takeoff: Option<Takeoff>,
    /// The landed state (`0x32`/`0x34`) after a rough landing on a planet or
    /// moon (`0x2e`/`0x30` for an open-air starport pad).
    pub landed: Option<Landed>,
    /// F650's pad tables (cache v26), for placing the ship on a pad (F994).
    pads: Option<Pads>,
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

/// The player's landed state on a surface (`DATA_008870`): F1021's rough
/// landing (`0x32`) or open-air starport touch (`0x2e`), then the view
/// function's guarded state (`0x34`/`0x30`). `index` is the body the ship came
/// down on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landed {
    pub landing: Landing,
    pub index: usize,
}

impl Landed {
    /// The mode word F1011/F1012 (`0x30` or `0x34`) or the frame's touch
    /// (`0x2e`/`0x32`).
    pub fn state(&self) -> u8 {
        self.landing.state
    }
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

/// What a frame of NPC traffic changed.
#[derive(Debug, Clone, PartialEq)]
pub enum ShipEvent {
    /// F931/F930: an NPC laser hit the player for `damage` (F953).
    Hit {
        ship: u32,
        name: String,
        code: u8,
        damage: u32,
        beam: bool,
    },
    /// A police or military ship turned hostile because the player is wanted.
    PoliceHostile { name: String },
    /// F787/F738/F927: a ship jumped out and its slot was freed.
    Escaped { name: String },
}

/// The result of one of the player's laser hits on an NPC ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attack {
    /// What F953 did.
    pub damage: ffe_core::laser::Damage,
    /// The ship was clean: the player is now wanted.
    pub crime: bool,
    /// F942's bounty for a destroyed pirate (`+0x11a`), else 0.
    pub bounty: u32,
    /// A police or military ship turned hostile because of this hit.
    pub police: bool,
    /// F942 destroyed the ship.
    pub destroyed: bool,
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

/// F829's start-system table (`DATA_006820`): five 20-byte records of galaxy x, y
/// and z (16.16), the star's number in its sector, and one dword the original
/// keeps beside them (`0x88` for the playable starts, `8` for the first and last).
/// Record 0 is the DOS default, Gateway [-1,4] (`0x17170200`, `0x15287600`,
/// -56832, star 6, extra 8); record 1 is Ross 154 (`0x17176a00`, `0x15243200`,
/// -0x3400, star 1).
pub const START_SYSTEMS: [(i32, i32, i32, u32, u32); 5] = [
    (0x1717_0200, 0x1528_7600, -56_832, 6, 8),
    (0x1717_6a00, 0x1524_3200, -0x3400, 1, 0x88),
    (0x1718_8000, 0x1524_a000, -0x6c00, 0, 0x88),
    (0x1715_e000, 0x151e_e000, 0, 0, 0x88),
    (0x1717_0200, 0x1528_7600, -56_832, 6, 8),
];

/// The new game's clock: 1 Jan 3250, noon, in absolute GameTics
/// (`0x121cf7.5` days at 86,400 seconds each).
pub const NEW_GAME_CLOCK: f64 = 102_562_286_400.0;
/// F998's starting cash (`DATA_008888`).
pub const NEW_GAME_CASH: i32 = 1000;
/// F998's ship for start type 1 (`+0x17`).
pub const NEW_GAME_SHIP: u32 = 23;
/// The player's state byte after F994 (`DATA_008870`): docked at an open-air starport.
pub const NEW_GAME_PLAYER_STATE: u8 = 0x30;
/// `DATA_008857`: the player's index among the physics objects (F994's `+0x86`).
pub const NEW_GAME_PLAYER_INDEX: u8 = 107;
/// F997's default time acceleration (`DATA_009012`).
pub const NEW_GAME_ACCELERATION: u32 = 994;
/// The tonnes the starting ship's equipment leaves out of its hold: the hull's
/// 20 t statistic less this is the new game's 8 t.
pub const NEW_GAME_HOLD_LOSS: i32 = 0xc;
/// The physics-list slot of the starting starport (`0x6e`): F994's `F1532(0x6e)`
/// is the object the player's ship is placed at and docked with.
pub const NEW_GAME_STARPORT_SLOT: usize = 0x6e;

/// F998's default arm (start types 0, 4, 5 and 6): the Saker Mk III on Old
/// Blackelk, Gateway [-1,4], with 10,000 CR. The DOS boot's "any other key"
/// starts this one (`docs/notes/intro.md`).
pub const DEFAULT_START_TYPE: u8 = 0;
pub const DEFAULT_SHIP: u32 = 0x1f;
pub const DEFAULT_CASH: i32 = 0x2710;
pub const DEFAULT_STARPORT_SLOT: usize = 0x6a;
pub const DEFAULT_PLAYER_INDEX: u8 = 87;
pub const DEFAULT_HOLD_LOSS: i32 = 0x10;

/// F829/F832/F869/F830 for one record of `DATA_006820`: the system F830 names,
/// generated from the record's coordinates and extra word with F878's inputs.
/// Record 0 is the DOS default start (Gateway [-1,4], the planet Hope and the
/// starport Old Blackelk); record 1 is Ross 154, whose hand-coded template the
/// galaxy list already holds.
pub fn start_entry(
    cache: &VerifiedCache,
    record: (i32, i32, i32, u32, u32),
) -> Result<Entry, String> {
    let (x, y, z, star, extra) = record;
    let code = (x >> 16) as u32 & 0x1fff | ((y >> 16) as u32 & 0x1fff) << 13 | (star & 0x3f) << 26;
    let galaxy = cache
        .asset("base:world/galaxy-tables")
        .ok_or("no galaxy tables (cache v21)")?;
    let tables = GalaxyTables::new(galaxy)?;
    let strings = Strings::new(
        cache
            .asset("base:text/system-strings")
            .ok_or("no system strings (cache v22)")?,
    )?;
    let name = tables
        .system_name(&strings, code)
        .ok_or_else(|| format!("F830 has no name for system {code:#x}"))?;
    // F869's first byte is `DATA_008995`: F832 leaves it where F855 reads the
    // generator's population-like parameter.
    let details = Tables::new(
        cache
            .asset("base:world/system-tables")
            .ok_or("no system tables")?,
    )?
    .details(code);
    // F832: `seed = (y >> 16) + (y << 16) + x - z`, `stars` is the high extra
    // byte's low three bits, and the total mass is the `extra & 0xf` row of
    // DATA_006803 scaled by `stars & 0xf8`.
    let seed = (y >> 16)
        .wrapping_add(y << 16)
        .wrapping_add(x)
        .wrapping_sub(z) as u32;
    let stars = extra >> 8;
    let mass = tables.start_mass((extra & 0xf) as usize, stars as u8);
    let mut name_field = [0u8; 20];
    let length = name.len().min(name_field.len());
    name_field[..length].copy_from_slice(&name[..length]);
    Ok(Entry {
        name: String::from_utf8_lossy(&name).trim().to_owned(),
        params: Params {
            seed,
            mass,
            stars: stars & 7,
            code,
            population: details.bytes[0] as i32,
            name: name_field,
        },
    })
}

/// The original's new-game state, kept for the local comparison with `--state`
/// and the new-game oracle. The playable uses the navigator's own fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewGameState {
    /// The start-system record F829 selected (`DATA_008881`).
    pub start_type: u8,
    /// The player's state byte after F994 (`DATA_008870`).
    pub player_state: u8,
    /// The player's object index (`DATA_008857`).
    pub player_index: u8,
    /// Cash on hand (`DATA_008888`).
    pub cash: i32,
    /// The player's ship model (`+0x82`).
    pub ship: u32,
    /// The hold in tonnes (`+0x116`, less the hyperdrive).
    pub cargo: i32,
    /// The starting time acceleration (`DATA_009012`).
    pub acceleration: u32,
    /// The bay F650 allocated and the pad position it gives (system frame, mm).
    pub bay: u16,
    pub seat: [i64; 3],
}

impl Navigator {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let entries = galaxy(cache)?;
        let current = entries.iter().position(|e| e.name == "Sol").unwrap_or(0);
        let bodies = bodies(cache, &entries[current], clock_words(0.0))?;
        // Start at a station in a planet's orbit, so the system's world is in view
        // after launch; deep-space stations only when the system has none.
        let mut nav = Self::assemble(cache, entries, current, bodies)?;
        nav.docked = orbital_station(&nav.bodies).or_else(|| first_station(&nav.bodies));
        Ok(nav)
    }

    /// The navigator's fields, without a start state: the sandbox and a new game
    /// differ only in the clock, the bodies and the docking (F829/F998).
    fn assemble(
        cache: &VerifiedCache,
        entries: Vec<Entry>,
        current: usize,
        bodies: Vec<Body>,
    ) -> Result<Self, String> {
        Ok(Self {
            entries,
            current,
            bodies,
            clock: 0.0,
            start_clock: 0.0,
            new_game: None,
            drift: [0; 3],
            skips: 0,
            page: 0,
            view: View::Closed,
            selected: 0,
            target: None,
            locked: None,
            docked: None,
            traffic: HashMap::new(),
            ships: Vec::new(),
            legal: npc::Legal::default(),
            persisted: Vec::new(),
            player_at: [0; 3],
            effects: Vec::new(),
            ship_lock: None,
            missile_lock: None,
            missiles: crate::missile::MissileField::default(),
            spawned: None,
            sequence: None,
            takeoff: None,
            landed: None,
            pads: cache
                .asset("base:gameplay/pads")
                .and_then(|bytes| Pads::from_bytes(bytes).ok()),
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

    /// The original's new game (F829/F998): the default DOS start is record 0,
    /// Gateway [-1,4] on the planet Hope at the starport Old Blackelk, with the
    /// Saker Mk III (F998's default arm). Record 1 is Ross 154's Eagle; the menu's
    /// types 2-5 are still the verified type 1 in this port. The bodies are placed
    /// at the 3250 clock and the player is F994-positioned on the open-air pad.
    pub fn new_game(cache: &VerifiedCache, start_type: u8) -> Result<Self, String> {
        let (record_index, start_type) = match start_type {
            0 => (0usize, 0u8),
            1 => (1, 1),
            2..=5 => (1, 1),
            _ => return Err(format!("start type {start_type} is not in DATA_006820")),
        };
        let record = START_SYSTEMS[record_index];
        let (default, ship, cash, starport_slot, player_index, hold_loss) = if start_type == 0 {
            (
                true,
                DEFAULT_SHIP,
                DEFAULT_CASH,
                DEFAULT_STARPORT_SLOT,
                DEFAULT_PLAYER_INDEX,
                DEFAULT_HOLD_LOSS,
            )
        } else {
            (
                false,
                NEW_GAME_SHIP,
                NEW_GAME_CASH,
                NEW_GAME_STARPORT_SLOT,
                NEW_GAME_PLAYER_INDEX,
                NEW_GAME_HOLD_LOSS,
            )
        };
        let code = (record.0 >> 16) as u32 & 0x1fff
            | ((record.1 >> 16) as u32 & 0x1fff) << 13
            | (record.3 & 0x3f) << 26;
        let mut entries = galaxy(cache)?;
        let current = if start_type == 0 {
            // Record 0 is not one of the hand-coded systems: F829/F832 generate
            // it from the record's coordinates and extra word.
            let entry = start_entry(cache, record)?;
            if let Some(index) = entries.iter().position(|e| e.params.code == code) {
                entries[index] = entry;
                index
            } else {
                entries.push(entry);
                entries.len() - 1
            }
        } else if let Some(index) = entries.iter().position(|entry| entry.params.code == code) {
            index
        } else {
            return Err(format!("start system {code:#x} is not in the galaxy table"));
        };
        let bodies = bodies(cache, &entries[current], clock_words(NEW_GAME_CLOCK))?;
        let index = bodies
            .iter()
            .position(|body| body.kind == Kind::Starport && body.slot == starport_slot)
            .ok_or_else(|| {
                format!(
                    "{} has no starport in slot {starport_slot:#x}",
                    entries[current].name
                )
            })?;
        let pads = Pads::from_bytes(
            cache
                .asset("base:gameplay/pads")
                .ok_or("no pad tables (cache v26)")?,
        )?;
        let mut port = ffe_core::traffic::Starport::new(bodies[index].bays);
        let bay = port
            .allocate(PLAYER)
            .ok_or("the start starport has no free bay")?;
        let body = &bodies[index];
        let models = Models {
            records: cache
                .asset("base:model/records")
                .ok_or("no model records")?,
            stats: cache.asset("base:ship/stats").ok_or("no ship statistics")?,
            gameplay: cache
                .asset("base:gameplay/tables")
                .ok_or("no gameplay tables")?,
        };
        // The source path places the first system's traffic where F688 does:
        // F687/F686 call it for the flight module's start, so the new game has
        // the record's traders/police and (for Hope) one pirate group. The port
        // puts a pirate group on the anchor orbit, not beside the pad.
        let launch_axis = body.orientation[2];
        // F994 copies the station's orientation to the ship and puts it on the
        // pad: F650's offset plus the ship's collision radius along its up row.
        let up = [body.rows[3], body.rows[4], body.rows[5]];
        let radius = models.info(ship).radii().1 as i32;
        let seat = pads.seat(
            body.position,
            body.model,
            bay.index,
            body.rows,
            body.open_air,
            radius,
            up,
        )?;
        let cargo = if default {
            // F998's default arm leaves the Saker with Cargo Space 1 t on the
            // executed original's F2 status page (`--game-frame 400 0x3c` after
            // the default start); the port uses that observed hold rather than
            // subtracting the Eagle's 0xc.
            1
        } else {
            (models.cargo_space(ship).unwrap_or(0) - hold_loss).max(0)
        };
        let mut nav = Self::assemble(cache, entries, current, bodies)?;
        nav.pads = Some(pads);
        nav.clock = NEW_GAME_CLOCK;
        nav.start_clock = NEW_GAME_CLOCK;
        nav.docked = Some(index);
        nav.traffic.insert(index, port);
        nav.new_game = Some(NewGameState {
            start_type,
            player_state: NEW_GAME_PLAYER_STATE,
            player_index,
            cash,
            ship,
            cargo,
            acceleration: NEW_GAME_ACCELERATION,
            bay: bay.index,
            seat,
        });
        // F687/F686's start entry: the new game's system has its F688 traffic,
        // but no capture guarantee and no pirate placed at the pad.
        nav.spawn_traffic(cache, seat, launch_axis);
        Ok(nav)
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

    /// World seconds since this game began (the sandbox starts at 0, the new game
    /// at `0x121cf7.5` absolute days).
    pub fn elapsed(&self) -> f64 {
        self.clock - self.start_clock
    }

    /// The game day of the clock (the original's day counter, counted from the
    /// new game's start so a new game's market drifts from day 0).
    pub fn day(&self) -> u32 {
        (self.elapsed().max(0.0) / 86_400.0) as u32
    }

    /// The console's date and time for this navigator's clock: the calendar counts
    /// from 1 Jan 3250, noon, not from the absolute day counter (`0x121cf7.5`).
    pub fn date_text(&self) -> String {
        crate::galaxy_map::date_text(self.elapsed())
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

    /// `DATA_008995`: the starport tech level of the current system, from F869's
    /// record byte at `+5`. F622 stores it once per system and F383 uses it for
    /// the Upgrades rows and prices. `None` when the cache has no system tables.
    pub fn tech_level(&self, cache: &VerifiedCache) -> Option<u16> {
        let blob = cache.asset("base:world/system-tables")?;
        let tables = Tables::new(blob).ok()?;
        Some(tables.tech_level(self.entries[self.current].params.code))
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

    /// The player's hold in tonnes: the new game's F998 value when this navigator
    /// started one, else the ship model's cargo statistic.
    pub fn hold(&self, cache: &VerifiedCache, ship: u32) -> i32 {
        self.new_game
            .map_or_else(|| self.cargo_space(cache, ship), |state| state.cargo)
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

    /// The station's door word (`+0x9e`, F865): 0 is open, 0xffff closed.
    pub fn door(&self, index: usize) -> u16 {
        self.traffic.get(&index).map_or(0xffff, |port| port.door)
    }

    /// The position the player occupies while docked at the station: its own
    /// position. F1088 copies it to the ship when the launch's door opens, so the
    /// launch starts inside the station at the open slot, not outside its hull.
    pub fn launch_seat(&self, index: usize) -> Option<[i64; 3]> {
        self.bodies
            .get(index)
            .filter(|body| matches!(body.kind, Kind::Station | Kind::Starport))
            .map(|body| body.position)
    }

    /// Where F994 puts a ship of collision radius `radius` on the bay's pad:
    /// the station's position plus F650's offset, lifted along the ship's up row
    /// (F994 copies the station's rows to the ship first, so that row is the
    /// station's second row). `None` when this station has no pad for the
    /// player or the cache has no pad tables.
    pub fn takeoff_seat(&self, index: usize, radius: i32) -> Option<[i64; 3]> {
        let body = self.bodies.get(index)?;
        let bay = self.traffic.get(&index)?.bay_of(PLAYER)?;
        let up = [body.rows[3], body.rows[4], body.rows[5]];
        self.pads
            .as_ref()?
            .seat(
                body.position,
                body.model,
                bay,
                body.rows,
                body.open_air,
                radius,
                up,
            )
            .ok()
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
        if self.sequence.is_some() || self.takeoff.is_some() {
            return Err("Already launching.".to_owned());
        }
        if !self.starport(index).request(PLAYER) {
            return Err("Launch refused: traffic control is busy.".to_owned());
        }
        self.docked = Some(index);
        // An open-air starport has no lift: F381 asks F654 and leaves the ship
        // on its pad, and the launch button takes the player to 0x24, F1013's
        // climb. The lift stages remain for indoor stations.
        if self
            .bodies
            .get(index)
            .is_some_and(|b| b.kind == Kind::Starport)
        {
            self.takeoff = Some(Takeoff::taking_off());
            return Ok(());
        }
        self.sequence = Some(Sequence {
            station: index,
            stages: Stages::launch_requested(),
            launching: true,
        });
        Ok(())
    }

    /// One frame of the open-air takeoff's mode word: F1013's hand-off from
    /// `0x24` to `0x26`, then F1031's decisions. `altitude` is F1033's
    /// `DATA_008867`, `set_speed` F1013's `DATA_008918` and `flags` F1033's
    /// collision flags. On [`TakeoffStep::Cleared`] the bay is released (F1032
    /// calls F651) and the ship is in free flight.
    pub fn takeoff_step(
        &mut self,
        altitude: i32,
        set_speed: i32,
        flags: u8,
    ) -> Option<TakeoffStep> {
        let mut takeoff = self.takeoff?;
        // F1031's second test is the object's class `0x20` (a starport), not
        // its open-air bit.
        let starport = self
            .docked
            .and_then(|index| self.bodies.get(index))
            .is_some_and(|body| body.kind == Kind::Starport);
        let step = takeoff.step(altitude, set_speed, flags, starport);
        match step {
            Some(TakeoffStep::Cleared) => {
                if let Some(index) = self.docked {
                    self.starport(index).release(PLAYER);
                }
                self.takeoff = None;
                self.docked = None;
            }
            _ => self.takeoff = Some(takeoff),
        }
        step
    }

    /// F1021's rough landing: the ship has settled on planet or moon `index`.
    /// The mode becomes `0x32` for the frame of the touch; [`Self::landing_frame`]
    /// then runs F1047's view function for `0x34`. A starport pad or a station
    /// hull is not a surface landing (F1019/F1021 give those the docking stages
    /// or the open-air `0x2e` state).
    pub fn land(&mut self, index: usize) -> bool {
        if self.landed.is_some() || self.takeoff.is_some() || self.sequence.is_some() {
            return false;
        }
        let Some(body) = self.bodies.get(index) else {
            return false;
        };
        if !matches!(body.kind, Kind::Planet | Kind::Moon) {
            return false;
        }
        self.target = None;
        self.final_leg = false;
        self.landed = Some(Landed {
            landing: Landing::on_surface(),
            index,
        });
        true
    }

    /// The frame after a landing: F1047 turns `0x32` into `0x34` (and F1048
    /// `0x2e` into `0x30`). Returns the guarded state once.
    pub fn landing_frame(&mut self) -> Option<u8> {
        let landed = self.landed.as_mut()?;
        landed.landing.frame()
    }

    /// The landed mode word (`0x34`), or the frame's `0x32`; `None` in flight.
    pub fn landed_state(&self) -> Option<u8> {
        self.landed.as_ref().map(Landed::state)
    }

    /// The body the ship is landed on.
    pub fn landed_body(&self) -> Option<usize> {
        self.landed.as_ref().map(|landed| landed.index)
    }

    /// F994's new-game state `0x30`: the ship is on the starport pad, still in
    /// the flight view, and has not started F1013's climb. The original's
    /// F1012 path runs no flight step here and F829/F998 spawn no traffic.
    pub fn on_pad(&self) -> bool {
        self.new_game.is_some()
            && self.docked.is_some()
            && self.takeoff.is_none()
            && self.sequence.is_none()
            && self.landed.is_none()
    }

    /// The player is in a landed view: a surface `0x34`/`0x32`, the open-air
    /// `0x30`, or the new game's pad state. F1012/F1048 run the landed handlers
    /// instead of the flight step, so the NPC traffic's AI/fire/damage does not
    /// run against the player.
    pub fn landed_view(&self) -> bool {
        self.landed.is_some() || self.on_pad()
    }

    /// F1056's launch action (its action 9) at a landed state: F1013's climb
    /// begins at `0x24`. F1031's contact scan then clears it into `0x00` (a
    /// planet's touch has no starport region `0x20`); see [`Self::takeoff_step`].
    /// `Err` when the ship is not in a landed state.
    pub fn begin_surface_launch(&mut self) -> Result<(), String> {
        let Some(landed) = self.landed else {
            return Err("Not landed on a surface.".to_owned());
        };
        if self.sequence.is_some() || self.takeoff.is_some() {
            return Err("Already launching.".to_owned());
        }
        let takeoff = landed
            .landing
            .begin_launch()
            .ok_or_else(|| "Not landed on a surface.".to_owned())?;
        self.takeoff = Some(takeoff);
        self.landed = None;
        Ok(())
    }

    /// The mode word the takeoff's current state gives the flight model
    /// (`DATA_008870`); `None` when no takeoff is running.
    pub fn takeoff_state(&self) -> Option<u8> {
        self.takeoff.map(|takeoff| takeoff.state)
    }

    /// The takeoff has only just begun and the ship is still on its pad.
    pub fn taking_off(&self) -> bool {
        self.takeoff
            .is_some_and(|takeoff| takeoff.state == state::TAKEOFF)
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

    /// F688/F635: spawn the current system's NPC traffic once. The original
    /// spawns on entering a system and restores the ships from the save; the
    /// port keeps a killed ship killed until the system is entered again. The
    /// ships F675 kept across the last jump are restored first (F680).
    /// Returns whether traffic was spawned.
    ///
    /// This is the source path: F688's own counts decide, so a system can spawn
    /// none. The port's capture/test guarantee belongs to
    /// [`Self::respawn_traffic`] only.
    pub fn spawn_traffic(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
    ) -> bool {
        self.spawn_traffic_with(cache, player, forward, npc::SpawnOptions::default())
    }

    fn spawn_traffic_with(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
        options: npc::SpawnOptions,
    ) -> bool {
        if self.spawned == Some(self.current) {
            return false;
        }
        let Some(blob) = cache.asset("base:world/system-tables") else {
            return false;
        };
        let Ok(tables) = Tables::new(blob) else {
            return false;
        };
        let code = self.entries[self.current].params.code;
        let details = tables.details(code);
        let allegiance = details.allegiance >> 6;
        let anchors: Vec<Anchor> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, body)| body.kind.is_place())
            .map(|(index, body)| Anchor {
                position: body.position,
                radius: body.radius,
                station: Some(index),
            })
            .collect();
        let clock = clock_words(self.clock);
        let seed = code ^ clock[0] ^ (player[0] as u32) ^ (player[2] as u32).rotate_left(11);
        let mut rng = npc::Random::new(seed, clock[1] ^ 0x4567_89ab);
        let mut ships = npc::restore(&self.persisted, player, 0);
        self.persisted.clear();
        let base = ships.len() as u32;
        for mut ship in npc::spawn(
            player,
            forward,
            &anchors,
            &details,
            allegiance,
            &mut rng,
            options,
            |model| crate::ships::npc_spec(cache, model),
        ) {
            ship.id += base;
            ships.push(ship);
        }
        ships.truncate(npc::MAX_SHIPS);
        self.ships = ships;
        self.player_at = player;
        self.ship_lock = None;
        self.effects.clear();
        self.missile_lock = None;
        self.missiles.clear();
        self.spawned = Some(self.current);
        !self.ships.is_empty()
    }

    /// Force the F688 spawner to run again for the current system, even when
    /// traffic was already spawned: the scripted traffic capture needs ships
    /// beside the camera rather than around a station pad. This is also the
    /// only caller that asks for the port's guarantee of one trader and one
    /// pirate group, so the capture always has a target.
    pub fn respawn_traffic(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
    ) -> bool {
        self.spawned = None;
        self.spawn_traffic_with(cache, player, forward, npc::SpawnOptions::CAPTURE)
    }

    /// One frame of NPC traffic: the flight AI, F931's fire and F927's recycle of
    /// ships that are dead, fled or too far from the player. `tics` are the
    /// frame's game tics and `player_radius` the player's interaction radius
    /// (F817's box). The returned events are the shots that hit the player and
    /// the police that turned hostile. F939/F942/F926's effects are ticked even
    /// when there is no traffic left.
    pub fn tick_ships(
        &mut self,
        seconds: f64,
        tics: u32,
        player: [i64; 3],
        player_radius: i64,
    ) -> Vec<ShipEvent> {
        self.player_at = player;
        let mut events = Vec::new();
        for effect in &mut self.effects {
            effect.tick(seconds);
        }
        self.effects.retain(Effect::alive);
        if self.ships.is_empty() {
            return events;
        }
        // F695's escorts hold a station in their leader's frame: right*+0x102 +
        // up*+0x104 + forward*+0x106 (F743's F610 target).
        let leaders: Vec<(u32, [i64; 3], [[f64; 3]; 3])> = self
            .ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| (ship.id, ship.position, ship.axes()))
            .collect();
        // The station-traffic snapshot of every ship's destination, taken before
        // the mutable AI pass (the traffic record and the bodies are read here).
        let stations: Vec<Option<npc::Station>> = self
            .ships
            .iter()
            .map(|ship| self.station_snapshot(ship))
            .collect();
        let mut requests = Vec::new();
        for (ship, station) in self.ships.iter_mut().zip(stations.iter()) {
            let escort = ship.leader.and_then(|id| {
                let (_, position, axes) = leaders.iter().find(|(leader, _, _)| *leader == id)?;
                let [right, up, forward] = ship.formation;
                Some(std::array::from_fn(|axis| {
                    position[axis]
                        + (axes[0][axis] * right as f64
                            + axes[1][axis] * up as f64
                            + axes[2][axis] * forward as f64) as i64
                }))
            });
            ship.tick_with_station(
                seconds,
                tics,
                player,
                escort,
                station.as_ref(),
                &mut requests,
            );
        }
        for request in requests {
            self.apply_dock_request(request);
        }
        // The police respond to a wanted player (the port's reconstruction of
        // DATA_008647; F783 clears the alert when the chase ends).
        if self.legal.wanted {
            for ship in &mut self.ships {
                if ship.provoke() {
                    events.push(ShipEvent::PoliceHostile {
                        name: ship.name.clone(),
                    });
                }
            }
        }
        // F931: every hostile ship with a laser fires at the player's box.
        for ship in &mut self.ships {
            if let Some(shot) = ship.fire(tics, player, player_radius) {
                events.push(ShipEvent::Hit {
                    ship: shot.ship,
                    name: shot.name,
                    code: shot.code,
                    damage: shot.damage,
                    beam: shot.beam,
                });
            }
        }
        for ship in &self.ships {
            if ship.state == ffe_core::npc::State::Escaped {
                events.push(ShipEvent::Escaped {
                    name: ship.name.clone(),
                });
            }
        }
        // F927's recycle also frees a bay the leaving ship still held.
        let leaving: Vec<(usize, u32)> = self
            .ships
            .iter()
            .filter(|ship| ship.finished(player) && ship.bay.is_some())
            .filter_map(|ship| Some((ship.station?, ship.id)))
            .collect();
        for (station, ship) in leaving {
            self.starport(station).release(npc::traffic_id(ship));
        }
        self.ships.retain(|ship| !ship.finished(player));
        if self.ship_lock.is_some_and(|id| {
            !self
                .ships
                .iter()
                .any(|ship| ship.id == id && ship.visible())
        }) {
            self.ship_lock = None;
        }
        if self.missile_lock.is_some_and(|id| {
            !self
                .ships
                .iter()
                .any(|ship| ship.id == id && ship.visible())
        }) {
            self.missile_lock = None;
        }
        events
    }

    /// The station-traffic snapshot for a ship's destination (F745's target, F650's
    /// bay, F657's clearance and F865's door). `None` when the ship has no station
    /// or the destination is not a place to dock.
    fn station_snapshot(&self, ship: &NpcShip) -> Option<npc::Station> {
        let index = ship.station?;
        let body = self.bodies.get(index)?;
        if !body.kind.is_place() {
            return None;
        }
        let id = npc::traffic_id(ship.id);
        let record = self.traffic.get(&index);
        let bay = record.and_then(|port| port.bay_of(id));
        let pad = bay.and_then(|bay| self.pad_point(index, bay));
        let (front, radius, axis) = match self.port(index) {
            Some(port) => (port.front, port.radius, port.axis),
            None => {
                let radius = body.radius as f64;
                (0.25 * radius, radius, unit(body.orientation[2]))
            }
        };
        Some(npc::Station {
            index,
            position: body.position,
            axis,
            front,
            radius,
            pad,
            bay,
            cleared: record.is_some_and(|port| port.is_docking(id)),
            closing: record.is_some_and(|port| port.mode == ffe_core::traffic::DOCKING_DONE),
            door_open: record.is_some_and(|port| port.door == 0),
        })
    }

    /// F650's pad point for an open-air starport (`ffe_core::pads`); indoor
    /// stations have no visible pad, so the traffic uses the slot instead.
    fn pad_point(&self, index: usize, bay: u16) -> Option<[i64; 3]> {
        let body = self.bodies.get(index)?;
        if !body.open_air {
            return None;
        }
        let offset = self
            .pads
            .as_ref()?
            .position(body.model, bay, body.rows, true)
            .ok()?;
        Some(std::array::from_fn(|axis| {
            body.position[axis].wrapping_add(offset[axis])
        }))
    }

    /// Apply a ship's station-traffic request to F649-F657's record and tell the
    /// ship what happened.
    fn apply_dock_request(&mut self, request: npc::DockRequest) {
        match request {
            npc::DockRequest::Request { ship, station } => {
                let id = npc::traffic_id(ship);
                let reply = self.starport(station).request_docking(id, false, 0);
                let pad = match reply {
                    Reply::Granted(bay) => self.pad_point(station, bay.index),
                    _ => None,
                };
                if let Some(record) = self.ships.iter_mut().find(|record| record.id == ship) {
                    match reply {
                        Reply::Granted(bay) => record.begin_approach(bay.index, pad),
                        Reply::Busy | Reply::Full => record.dock_retry = npc::DOCK_RETRY_SECONDS,
                        Reply::Fined => {}
                    }
                }
            }
            npc::DockRequest::Touch { ship, station } => {
                let id = npc::traffic_id(ship);
                let touched = self.starport(station).touch(id);
                let seat = self
                    .bodies
                    .get(station)
                    .map_or([0; 3], |body| body.position);
                if let Some(record) = self.ships.iter_mut().find(|record| record.id == ship) {
                    if touched {
                        record.begin_docked(record.pad.unwrap_or(seat));
                    } else {
                        record.touch_failed();
                    }
                }
            }
            npc::DockRequest::Settle { station, .. } => {
                // F747 (state 0x20) runs F653 once the docking mode is over;
                // the bay itself stays allocated.
                self.starport(station).reset();
            }
            npc::DockRequest::Launch { ship, station } => {
                let id = npc::traffic_id(ship);
                let granted = self.starport(station).request(id);
                if let Some(record) = self.ships.iter_mut().find(|record| record.id == ship) {
                    if granted {
                        record.state = npc::State::Launch;
                        record.dock_wait = 0.0;
                    } else {
                        record.dock_retry = npc::DOCK_RETRY_SECONDS;
                    }
                }
            }
            npc::DockRequest::Release { ship, station } => {
                self.starport(station).release(npc::traffic_id(ship));
            }
            npc::DockRequest::ChooseStation { ship, station } => {
                let next = self.next_place(station);
                if let Some(record) = self.ships.iter_mut().find(|record| record.id == ship) {
                    match next {
                        Some(index) => {
                            let anchor = self
                                .bodies
                                .get(index)
                                .map_or(record.position, |body| body.position);
                            record.begin_transit(index, anchor);
                        }
                        // F762's F700 returned no starport: F925 removes it.
                        None => record.escape(),
                    }
                }
            }
        }
    }

    /// F761/F762: the next station to trade with. The original draws a random
    /// one; the port walks the system's places so a trader visits them in turn
    /// (a single-station system sends it back to the same one).
    fn next_place(&self, current: usize) -> Option<usize> {
        let places = place_indices(&self.bodies);
        if places.is_empty() {
            return None;
        }
        let at = places
            .iter()
            .position(|index| *index == current)
            .unwrap_or(0);
        Some(places[(at + 1) % places.len()])
    }

    /// The player's laser hit on an NPC ship: F953/F942 plus the legal
    /// consequences. A hit on a clean ship (a trader, shuttle, police or
    /// military hull) makes the player wanted and the police hostile; a pirate's
    /// destruction pays its `+0x11a` bounty (F942's F697 roll).
    pub fn player_attack(&mut self, id: u32, damage: u32) -> Option<Attack> {
        let ship = self.ships.iter_mut().find(|ship| ship.id == id)?;
        let was_alive = ship.alive();
        let crime = ship.role.clean() && was_alive;
        let bounty = ship.bounty;
        let station = ship.station;
        let held_bay = ship.bay.is_some();
        let result = ship.hit(damage);
        let destroyed = was_alive && !ship.alive();
        if crime {
            self.legal.wanted = true;
            self.legal.offences += 1;
        }
        let mut police = false;
        if self.legal.wanted {
            for ship in &mut self.ships {
                police |= ship.provoke();
            }
        }
        if destroyed {
            if let (Some(station), true) = (station, held_bay) {
                // F788: a destroyed ship releases its bay.
                self.starport(station).release(npc::traffic_id(id));
            }
        }
        Some(Attack {
            damage: result,
            crime,
            bounty: if destroyed && matches!(result, ffe_core::laser::Damage::Destroyed) {
                bounty
            } else {
                0
            },
            police,
            destroyed,
        })
    }

    /// F941: put a missile in flight at the combat target. The caller has
    /// already taken the pylon word from `Combat`.
    #[allow(clippy::too_many_arguments)]
    pub fn launch_missile(
        &mut self,
        spec: ffe_core::missile::Spec,
        code: u8,
        owner: u32,
        target: u32,
        position: [i64; 3],
        axes: [[f64; 3]; 3],
        pylon: usize,
        launcher_radius: i64,
    ) {
        self.missiles.launch(
            spec,
            code,
            owner,
            Some(target),
            position,
            axes,
            pylon,
            launcher_radius,
        );
    }

    /// One frame of missiles: F758/F717's tracking, the fuse and F953's damage.
    /// The combat target is cleared when its ship is gone.
    pub fn tick_missiles(
        &mut self,
        seconds: f64,
        _player: [i64; 3],
    ) -> Vec<crate::missile::MissileEvent> {
        if self
            .missile_lock
            .is_some_and(|id| !self.ships.iter().any(|ship| ship.id == id && ship.alive()))
        {
            self.missile_lock = None;
        }
        self.missiles.tick(seconds, &mut self.ships)
    }

    /// The locked NPC ship, if one is.
    pub fn locked_ship(&self) -> Option<&NpcShip> {
        let id = self.ship_lock?;
        self.ships.iter().find(|ship| ship.id == id)
    }

    /// Lock an NPC ship by id (a click or a laser hit). Returns it.
    pub fn lock_ship(&mut self, id: u32) -> Option<&NpcShip> {
        self.ship_lock = Some(id);
        self.ships.iter().find(|ship| ship.id == id)
    }

    /// The next live ship after the locked one, wrapping; `None` when there is
    /// no live traffic. `step` is +1 or -1.
    pub fn cycle_ship_lock(&mut self, step: i32) -> Option<&NpcShip> {
        let ids: Vec<u32> = self
            .ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| ship.id)
            .collect();
        if ids.is_empty() {
            self.ship_lock = None;
            return None;
        }
        let at = self
            .ship_lock
            .and_then(|id| ids.iter().position(|candidate| *candidate == id))
            .unwrap_or(0);
        let next = (at as i32 + step).rem_euclid(ids.len() as i32) as usize;
        self.ship_lock = Some(ids[next]);
        self.ships.iter().find(|ship| ship.id == ids[next])
    }

    /// F953/F942 on an NPC ship. `None` when the id is not live. Kept for the
    /// callers that only need the damage result; [`Self::damage_ship_at`] is the
    /// full path that also spawns F939/F942's effects and reports the legal
    /// consequences.
    pub fn damage_ship(&mut self, id: u32, damage: u32) -> Option<ffe_core::laser::Damage> {
        self.damage_ship_at(id, damage, None)
            .map(|attack| attack.damage)
    }

    /// F953/F942 on an NPC ship with the hit point F817 found. The hit spawns
    /// F939's particle at `at` (the ship's centre when the caller has no point)
    /// and, on destruction, F942's explosion and F926's debris at the ship. The
    /// legal/bounty side is [`Self::player_attack`].
    pub fn damage_ship_at(&mut self, id: u32, damage: u32, at: Option<[i64; 3]>) -> Option<Attack> {
        let ship = self.ships.iter().find(|ship| ship.id == id)?;
        let position = ship.position;
        let axes = ship.axes();
        let velocity = ship.velocity;
        let cargo = ship.cargo;
        let speed = velocity.iter().map(|v| v * v).sum::<f64>().sqrt();
        let attack = self.player_attack(id, damage)?;
        let seed = (self.clock * 1000.0) as u32
            ^ id.rotate_left(7)
            ^ (self.effects.len() as u32).wrapping_mul(0x9e37_79b9);
        let mut rng = npc::Random::new(seed, seed.rotate_left(13) ^ 0x4567_89ab);
        // F931 calls F939 once per hit, whether the shields or the hull took it.
        self.effects.push(effects::hit(
            at.unwrap_or(position),
            velocity,
            axes,
            speed,
            &mut rng,
        ));
        if attack.damage == ffe_core::laser::Damage::Destroyed {
            self.effects.extend(effects::explosion(
                position, velocity, cargo, axes, &mut rng,
            ));
        }
        Some(attack)
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
                // F867: the clearance timed out and F651 freed the bay. An NPC
                // that was still on its way goes back to transit and asks again.
                if let Some(released) = port.elapse(whole as u32) {
                    if released != PLAYER {
                        if let Some(ship) = self
                            .ships
                            .iter_mut()
                            .find(|ship| npc::traffic_id(ship.id) == released)
                        {
                            ship.touch_failed();
                        }
                    }
                }
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

    /// How many systems the galaxy list holds.
    pub fn system_count(&self) -> usize {
        self.entries.len()
    }

    /// The index of the system the ship is in.
    pub fn current_system(&self) -> usize {
        self.current
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
            View::System | View::Places => self.listed().len(),
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
        // A transfer is a jump in the system frame; it cannot arrive landed.
        self.landed = None;
        self.takeoff = None;
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
        // F903 only jumps from mode 0 or 0x24; a landed ship must launch first.
        if self.landed.is_some()
            || self
                .takeoff
                .is_some_and(|takeoff| takeoff.state != state::TAKEOFF)
        {
            return Err("Launch before a hyperspace jump.".to_owned());
        }
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
        self.ship_lock = None;
        // F675: the pirates close to the player are carried through the jump.
        self.persisted = npc::persist(&self.ships, self.player_at);
        self.ships.clear();
        self.spawned = None;
        self.docked = None;
        self.traffic.clear();
        self.markets.clear();
        self.sequence = None;
        self.takeoff = None;
        self.landed = None;
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
    /// original's). The original's list (`View::Places`) is its stations and starports
    /// in system order; the ffe-rs system map (`View::System`) also shows the star,
    /// planets and moons, stars and planets first, then the ports and stations.
    pub fn listed(&self) -> Vec<usize> {
        let rank = |k: Kind| match k {
            Kind::Star => 0,
            Kind::Planet => 1,
            Kind::Starport => 2,
            Kind::Station => 3,
            Kind::Moon => 4,
        };
        let mut all: Vec<usize> = match self.view {
            View::Places => place_indices(&self.bodies),
            _ => (0..self.bodies.len()).collect(),
        };
        if self.view != View::Places {
            all.sort_by_key(|i| (rank(self.bodies[*i].kind), *i));
        }
        all.into_iter()
            .skip(18 * self.pages_clamped())
            .take(18)
            .collect()
    }

    fn pages_clamped(&self) -> usize {
        self.page.min(self.pages().saturating_sub(1))
    }

    /// How many pages the current list has (the original has one page of 18; the
    /// generated systems can have more stations, so the port pages them).
    pub fn pages(&self) -> usize {
        let length = match self.view {
            View::Places => place_indices(&self.bodies).len(),
            _ => self.bodies.len(),
        };
        length.div_ceil(18).max(1)
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
    /// the player's target and its autopilot mode, then closes the list). F1099's
    /// action 0xd1+n stores the object's number in the player's target byte `+0xec`
    /// (ffe.asm 78362-78376); the port's `locked` is that target.
    pub fn engage(&mut self) -> Option<&Body> {
        let index = self.selected_body()?;
        self.target = Some(index);
        self.locked = Some(index);
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

    /// The body whose frame the ship is in (F1678's `player+0x56`), as chosen by
    /// the last [`Navigator::follow`]. The scanner's same-frame filter and the
    /// player's frame body use it.
    pub fn frame(&self) -> Option<usize> {
        self.frame
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
            // The original list draws only the marker and the name.
            View::Places => self
                .listed()
                .into_iter()
                .map(|i| self.bodies[i].name.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// F829/F832's default record (Gateway [-1,4], star 6, extra 8) gives the
    /// seed the executed original reports in `--state` (`0x8d17f528`). The z
    /// word is the 16.16 -56832 the DOS start table holds, not the rounded -222
    /// an earlier transcription kept.
    #[test]
    fn the_default_start_record_seed_is_f829s() {
        let (x, y, z, star, extra) = START_SYSTEMS[0];
        assert_eq!(
            (x, y, z, star, extra),
            (0x1717_0200, 0x1528_7600, -56_832, 6, 8)
        );
        let seed = (y >> 16)
            .wrapping_add(y << 16)
            .wrapping_add(x)
            .wrapping_sub(z) as u32;
        assert_eq!(seed, 0x8d17_f528);
        assert_eq!(START_SYSTEMS[4], START_SYSTEMS[0]);
        assert_eq!(DEFAULT_START_TYPE, 0);
        assert_eq!(DEFAULT_STARPORT_SLOT, 0x6a);
        assert_eq!(DEFAULT_PLAYER_INDEX, 87);
    }
}
