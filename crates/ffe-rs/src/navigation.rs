//! Getting around: the systems the original's generator builds (the hand-coded
//! neighbourhood and a few generated ones), their bodies with the names F881/F893
//! give them, the positions F880 places them at for a game clock, and an autopilot
//! that flies the original flight model to a chosen body with the same keys a
//! player presses. See `docs/NAVIGATION.md`.
use crate::docking::{Contact, Port, CLEARANCE_RANGE};
use crate::original_flight::{Keys, OriginalFlight, MM_PER_SECOND, UNITS_PER_FLIGHT_UNIT};
use ffe_content::VerifiedCache;
use ffe_core::bulletin;
use ffe_core::calendar::Calendar;
use ffe_core::dust;
use ffe_core::effects::{self, Effect};
use ffe_core::galaxy::{tables, Tables};
use ffe_core::hyperspace::{plan as jump_plan, Report};
use ffe_core::market::{Conditions, Market, Tables as MarketTables};
use ffe_core::missions;
use ffe_core::model_math::SineTable;
use ffe_core::names::Names;
use ffe_core::npc::{self, Anchor, NpcShip};
use ffe_core::objects::Models as ShipModels;
use ffe_core::objects::{Models, PhysList, Placement};
use ffe_core::pads::Pads;
use ffe_core::sector::{
    distance as star_distance, position as star_position, Drive, Strings, Tables as GalaxyTables,
};
use ffe_core::shipyard;
use ffe_core::surface::{Mesh, Shape, Shapes, SurfaceTools};
use ffe_core::sysgen::{Generator, Params, OBJECT_BYTES};
use ffe_core::traffic::{
    begin_exit, finish_touch, state, Landing, Reply, Stages, Starport, Takeoff, TakeoffStep,
};
use ffe_core::RandomState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One leader's frame for F695/F751's escorts: its id, system position, axes
/// and AI state.
type LeaderFrame = (u32, [i64; 3], [[f64; 3]; 3], npc::State);

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
    /// F595/F594 native velocity words in the parent frame.
    pub velocity: [i32; 3],
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
    /// The object's gravity word (`+0xa4`): F880/F881's ffp mass, which F603
    /// reads for the body an object is parented to.
    pub mass: u32,
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

/// How far (in F833's hundredths of a light year) a start system's list is
/// extended to its galaxy-table neighbours: 30 light years. The default
/// Saker's class 2 drive reaches 10.71 LY, so this is deliberately generous.
pub const NEIGHBOUR_REACH: i32 = 3_000;

/// F833's units per sector: the distance is in hundredths of a light year and
/// a sector is about eight of them (`docs/notes/hyperspace.md`).
const UNITS_PER_SECTOR: i32 = 800;

/// One star around a system, as [`neighbours_within`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Neighbour {
    code: u32,
    /// F856 position, 16.16.
    position: [i32; 3],
    /// F841's type word: F852 reads its low byte as the start type and its high
    /// byte as the number of stars.
    kind: u16,
    /// F833 distance from the home system.
    distance: i32,
}

/// F840/F852's selection around a system: every star of the sectors within
/// `reach` of `home`, nearest first, without `home` itself. `sector(x, y)`
/// supplies the stars of one sector as `(number, F856 position, F841 type
/// word)`; the original enumerates whole sectors this way (F853's count and
/// F841/F856's vectors), which is why no star is missed at a sector boundary.
fn neighbours_within<F>(home_code: u32, home: [i32; 3], reach: i32, mut sector: F) -> Vec<Neighbour>
where
    F: FnMut(u16, u16) -> Vec<(usize, [i32; 3], u16)>,
{
    if reach <= 0 {
        return Vec::new();
    }
    let (hx, hy) = (
        (home_code & 0x1fff) as i32,
        ((home_code >> 13) & 0x1fff) as i32,
    );
    let radius = reach / UNITS_PER_SECTOR + 1;
    let mut found = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (hx + dx, hy + dy);
            if !(0..0x2000).contains(&x) || !(0..0x2000).contains(&y) {
                continue;
            }
            for (number, position, kind) in sector(x as u16, y as u16) {
                let code =
                    (x as u32 & 0x1fff) | (y as u32 & 0x1fff) << 13 | (number as u32 & 0x3f) << 26;
                if code == home_code {
                    continue;
                }
                let distance = star_distance(home, position);
                if distance <= reach {
                    found.push(Neighbour {
                        code,
                        position,
                        kind,
                        distance,
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.code.cmp(&b.code)));
    found.dedup_by_key(|neighbour| neighbour.code);
    found
}

/// The systems on offer around a start system: [`galaxy`]'s hand-coded and
/// test systems plus the start system's real galaxy-table neighbours within
/// `reach`, each generated from its F841 type word and F856 position exactly as
/// F852/F860/F832 do for a map destination. A neighbour already on offer (a
/// hand-coded system, or one of the tests) keeps its hand-coded entry.
pub fn galaxy_around(
    cache: &VerifiedCache,
    home_code: u32,
    reach: i32,
) -> Result<Vec<Entry>, String> {
    let mut entries = galaxy(cache)?;
    let Some(blob) = cache.asset("base:world/galaxy-tables") else {
        return Ok(entries);
    };
    let galaxy = GalaxyTables::new(blob)?;
    let Some(home) = galaxy.system_position(home_code) else {
        return Ok(entries);
    };
    let system = Tables::new(
        cache
            .asset("base:world/system-tables")
            .ok_or("no system tables")?,
    )?;
    // A pre-v22 cache has the tables but no system strings; the start arm that
    // needs them (`start_entry`) already reports that, so the neighbours are
    // simply not offered here.
    let Some(strings) = cache
        .asset("base:text/system-strings")
        .and_then(|bytes| Strings::new(bytes).ok())
    else {
        return Ok(entries);
    };
    let neighbours = neighbours_within(home_code, home, reach, |x, y| {
        galaxy
            .stars(x, y)
            .iter()
            .enumerate()
            .map(|(number, star)| (number, star_position(x, y, &star.vector), star.kind))
            .collect()
    });
    for neighbour in neighbours {
        if entries
            .iter()
            .any(|entry| entry.params.code == neighbour.code)
        {
            continue;
        }
        entries.push(system_entry(
            neighbour.code,
            neighbour.position,
            u32::from(neighbour.kind),
            &galaxy,
            &system,
            &strings,
        )?);
    }
    Ok(entries)
}

/// A system's bodies at a game clock.
pub fn bodies(cache: &VerifiedCache, entry: &Entry, clock: [u32; 2]) -> Result<Vec<Body>, String> {
    bodies_since(cache, entry, clock, clock)
}

/// Source system creation followed by F866's elapsed-time evolution.
pub fn bodies_since(
    cache: &VerifiedCache,
    entry: &Entry,
    created: [u32; 2],
    clock: [u32; 2],
) -> Result<Vec<Body>, String> {
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
        detail: ffe_core::surface::DEFAULT_DETAIL,
    };
    let placement = Placement {
        tables: &tables_view,
        models: &models,
        sine: &sine,
        surface: Some(&surface),
    };
    let mut list = PhysList {
        stamp: 1,
        clock: created,
        ..PhysList::default()
    };
    placement.create(&mut list, &mut system, created);
    if clock != created {
        placement.advance(&mut list, clock);
    }
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
        // F879 stores surface ports in their planet's rotating frame (+0x57).
        // F1677 turns that local position before adding the parent's origin.
        // Treating it as linear places both the port and its pad over a different
        // terrain point, while the planet is still rendered with its true spin.
        let mut position = [coordinate(0), coordinate(1), coordinate(2)];
        let mut rows: [i32; 9] = std::array::from_fn(|k| list.dword(slot, 4 * k) as i32);
        if list.byte(slot, 0x57) != 0 && parent != 0 {
            let parent_at = (parent - 1) * OBJECT_BYTES;
            let parent_slot = u32::from_le_bytes(
                system[parent_at + 0x18..parent_at + 0x1c]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let matrix = std::array::from_fn(|k| list.dword(parent_slot, 4 * k) as i32);
            let (vector, shift) = ffe_core::frames::truncate(position, 0x1c);
            position = ffe_core::frames::shift_up(
                ffe_core::frames::vec_mat_mul(vector.map(|v| v as i32), &matrix),
                shift,
            );
            rows = ffe_core::frames::mat_mat_mul(&rows, &matrix);
        }
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
        let class = list.byte(slot, 0x14c);
        out.push(Body {
            name: String::from_utf8_lossy(text).into_owned(),
            kind,
            position,
            velocity: std::array::from_fn(|k| list.dword(slot, 0x8c + 4 * k) as i32),
            radius,
            inner_radius,
            parent: parent.checked_sub(1),
            model: list.dword(slot, 0x82),
            seed: list.dword(slot, 0xa0),
            mass: list.dword(slot, 0xa4),
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

/// The source's near-starport threshold: `cmp byte [object+0x88],0x13`
/// (`ffe.asm:26918`, `ffed3d-master/win/ffemisc.cpp` `IsCloseToStarport` uses
/// `dist_cam < 0x13`). `+0x88` is F1680's exponential distance byte, not a
/// linear radius.
pub const JURISDICTION_DISTANCE: u8 = 0x13;

/// F1680's `+0x88` exponential distance byte: `max(0, msb(distance) - 0xd)`
/// minus the object's `+0x7e` scale. F1680 stores `log2`-of-distance in a byte,
/// so each unit is a doubling; `0xa` is "maximum distance about 20km"
/// (`ffed3d-master/ffe.asm:72033`). The body's model scale is omitted here,
/// which makes
/// the byte larger (a stricter test) for a large object.
pub fn distance_byte(distance: f64) -> u8 {
    if distance <= 0.0 || !distance.is_finite() {
        return 0;
    }
    let msb = distance.log2().floor() as i32;
    (msb - 0xd).clamp(0, i32::from(u8::MAX)) as u8
}

/// F234/F235: the nearest starport whose exponential distance byte is below
/// [`JURISDICTION_DISTANCE`], as its ordinal among the system's starports (the
/// `DATA_008996` ledger index). The original's witness roll also uses the
/// system danger and a random draw; this is FFED3D's deterministic
/// `IsCloseToStarport` form.
pub fn starport_witness(bodies: &[Body], at: [i64; 3]) -> Option<usize> {
    let mut best: Option<(u8, f64, usize)> = None;
    let mut ordinal = 0usize;
    for body in bodies {
        if body.kind != Kind::Starport {
            continue;
        }
        let away = distance(at, body.position);
        let byte = distance_byte(away);
        let closer = best.is_none_or(|(best_byte, best_away, _)| {
            byte < best_byte || (byte == best_byte && away < best_away)
        });
        if byte < JURISDICTION_DISTANCE && closer {
            best = Some((byte, away, ordinal));
        }
        ordinal += 1;
    }
    best.map(|(_, _, index)| index)
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
        mass: body.mass,
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

/// F1100's target range text: the km form below its exponent threshold and the
/// AU form above it. The two strings (`0x8604`/`0x8605`) use the original's
/// fixed-point formats `0x3007` (two decimals, value / 100) and `0x300a` (three
/// decimals, value / 1000); the exponent switch is F1100's `0x1d` band, i.e.
/// 2^44 mm (17,592,186 km).
pub fn range_label(from: [i64; 3], to: [i64; 3]) -> String {
    let (mantissa, exponent) = ffe_core::laser::range_words(from, to);
    let (value, au) = ffe_core::laser::range_value(mantissa, exponent);
    crate::targeter::range_spaced(value.max(0), au)
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
    /// F746/F599's thrust request in body axes for this frame (the starport's
    /// final descent), overriding the set speed.
    pub thrust: Option<[i32; 3]>,
    /// F746's copy of the target's orientation onto the ship, for the last
    /// metres of a starport descent (the source copies the port's rows when its
    /// distance reaches zero).
    pub align: Option<[[f64; 3]; 3]>,
}

/// The last few metres of an arrival: the braking curve approaches the standoff
/// asymptotically, and the set-speed step is too coarse to creep the final
/// millimetres. F742's own test is an exponential distance band (about 0.5-1 km),
/// so 10 m is a tighter arrival than the source's.
const ARRIVAL_SLOP: f64 = 1.0e4;

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

/// The time and place of a long leg to a body that is itself moving: the first
/// `t` with `t == transfer_seconds(distance(ship, aim(t)) - stop_cover)`, where
/// `aim(t)` is the destination at `clock + t`.
///
/// The original has no long-leg transfer; the port's instant transfer is its
/// own convenience. A fixed-point loop (`guess from the current position`) is
/// unstable for a body whose orbital displacement during the transfer is large:
/// the guess chases the planet around its orbit and never comes back, which is
/// the autopilot circling a planet instead of homing on it. This solves the
/// rendezvous by bracketing the first crossing and bisecting it, so the landing
/// point is where the body really is at the advanced clock.
pub fn intercept_time(
    ship: [i64; 3],
    stop_cover: f64,
    initial: f64,
    mut aim: impl FnMut(f64) -> Option<[i64; 3]>,
) -> Option<(f64, [i64; 3])> {
    let gap = |at: [i64; 3]| (distance(ship, at) - stop_cover).max(0.0) / 1000.0;
    let early = |t: f64, at: [i64; 3]| t - transfer_seconds(gap(at));
    // g(0) is negative: the leg takes longer than no time at all. Find a time
    // by which the transfer has caught up with the clock (g >= 0).
    let mut high = initial.max(30.0).max(1.0);
    let mut bracketed = false;
    for _ in 0..32 {
        let at = aim(high)?;
        if early(high, at) >= 0.0 {
            bracketed = true;
            break;
        }
        high *= 2.0;
        if high > 1.0e10 {
            return None;
        }
    }
    if !bracketed {
        return None;
    }
    // The body may lap the ship before that bound, so find the *first* crossing
    // rather than bisecting the whole interval.
    let mut low = 0.0;
    let samples = 32;
    for step in 1..=samples {
        let t = high * f64::from(step) / f64::from(samples);
        let at = aim(t)?;
        if early(t, at) >= 0.0 {
            high = t;
            break;
        }
        low = t;
    }
    for _ in 0..24 {
        let mid = 0.5 * (low + high);
        let at = aim(mid)?;
        if early(mid, at) >= 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    let t = high;
    aim(t).map(|at| (t, at))
}

/// The point of an orbital station's slot the final leg flies through this
/// frame, and the standoff to brake for. F745/F746 re-aim at F650's pad vector
/// on every tick, so the approach follows the current slot: a ship that is off
/// the axis first lines up on a gate outside the face, then enters along it.
/// Aiming at the centre alone crosses a spinning station's hull and is pushed
/// clear, which is the port's "cleared but never docks".
fn slot_leg(port: &Port, ship: [i64; 3]) -> ([f64; 3], f64) {
    let offset: [f64; 3] = std::array::from_fn(|k| (ship[k] - port.centre[k]) as f64);
    let range = offset.iter().map(|v| v * v).sum::<f64>().sqrt();
    let axial: f64 = (0..3).map(|k| offset[k] * port.axis[k]).sum();
    let lateral = (range * range - axial * axial).max(0.0).sqrt();
    let gate = port.front + 4.0 * port.corridor() + 1.0e6;
    if axial > port.front && lateral < port.corridor() * 0.25 {
        // Lined up in front of the face: enter along the axis, braking so the
        // face is crossed below the 250 m/s slot limit. The contact scan docks
        // at the face before the standoff is reached.
        (port.axis.map(|a| a * (port.front * 0.2)), port.front * 0.8)
    } else {
        // Outside the corridor or behind the station: line up on the gate.
        (port.axis.map(|a| a * gate), 0.0)
    }
}

/// A safe route to a station's approach point. A straight line from an arbitrary
/// start can cut through the hull (a jump arrival on the far side of the
/// station): if the segment to `aim` passes inside the station's safety sphere,
/// steer to a point on that sphere at a tangent toward the destination instead.
/// Repeated each frame, it works around the hull and then lines up on F741's
/// approach point.
fn station_route(port: &Port, ship: [i64; 3], aim: [i64; 3]) -> [i64; 3] {
    let safe = port.front + 4.0 * port.corridor() + 5.0e5;
    let r: [f64; 3] = std::array::from_fn(|k| (ship[k] - port.centre[k]) as f64);
    let range = r.iter().map(|v| v * v).sum::<f64>().sqrt();
    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| a[k] * b[k]).sum::<f64>();
    let unit = |v: [f64; 3]| {
        let length = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-9);
        v.map(|c| c / length)
    };
    if range < safe {
        // Inside the safety sphere: move radially out first.
        let out = if range > 1.0 { unit(r) } else { port.axis };
        return std::array::from_fn(|k| port.centre[k] + (out[k] * safe * 1.2).round() as i64);
    }
    let d: [f64; 3] = std::array::from_fn(|k| (aim[k] - ship[k]) as f64);
    let dd = dot(d, d);
    if dd <= 1.0 {
        return aim;
    }
    let t = (-dot(r, d) / dd).clamp(0.0, 1.0);
    let closest: [f64; 3] = std::array::from_fn(|k| r[k] + d[k] * t);
    let miss = closest.iter().map(|v| v * v).sum::<f64>().sqrt();
    if miss >= safe {
        return aim;
    }
    let radial = unit(r);
    let mut tangent = std::array::from_fn(|k| d[k] - radial[k] * dot(d, radial));
    if tangent.iter().map(|v| v * v).sum::<f64>() < 1.0 {
        // Dead through the centre: pick a tangent about the docking axis.
        tangent = [
            port.axis[1] * radial[2] - port.axis[2] * radial[1],
            port.axis[2] * radial[0] - port.axis[0] * radial[2],
            port.axis[0] * radial[1] - port.axis[1] * radial[0],
        ];
    }
    let tangent = unit(tangent);
    let step = unit(std::array::from_fn(|k| {
        radial[k] * std::f64::consts::FRAC_1_SQRT_2 + tangent[k] * std::f64::consts::FRAC_1_SQRT_2
    }));
    std::array::from_fn(|k| port.centre[k] + (step[k] * safe).round() as i64)
}

/// What the autopilot remembers between frames: the last aim error, to see how
/// fast the nose drifts at the current time acceleration.
#[derive(Debug, Clone, Copy, Default)]
pub struct Memory {
    angle: Option<f64>,
}

/// F746/F599's final descent to an open-air pad: the thrust request in the
/// ship's body axes that drives the ship's velocity onto the braking curve
/// towards `pad`. The source's F596/F597 compute the vector and F599 clamps it;
/// the same request is clamped by `ffe_core::thrust::clamp` when it is applied.
/// The curve is deliberately below the 0x258-unit (about 58 m/s) F575 landing
/// limit at the last metres, so the touch is a landing rather than a bounce.
pub fn starport_descent(flight: &OriginalFlight, pad: [i64; 3], integral: [f64; 3]) -> [i32; 3] {
    let motion = &flight.player().motion;
    let offset: [f64; 3] = std::array::from_fn(|k| (pad[k] - motion.position[k]) as f64);
    let range = offset.iter().map(|v| v * v).sum::<f64>().sqrt();
    let dir = if range > 1.0 {
        offset.map(|v| v / range)
    } else {
        [0.0; 3]
    };
    let metres = range / 1000.0;
    let speed = if range <= 1.0 {
        0.0
    } else {
        (2.0 * 3.0 * metres).sqrt().max(metres * 0.2).min(15.0)
    };
    let velocity = motion
        .velocity
        .map(|v| f64::from(v) * MM_PER_SECOND / 1000.0);
    let change: [f64; 3] = std::array::from_fn(|k| dir[k] * speed - velocity[k]);
    let acceleration: [f64; 3] =
        std::array::from_fn(|k| (change[k] * 5.0 + integral[k]).clamp(-120.0, 120.0));
    let rows: [[f64; 3]; 3] = motion.orientation.map(|row| {
        let values = row.map(|v| f64::from(v) / 2_147_483_648.0);
        let length = values.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
        values.map(|v| v / length)
    });
    // The original's thrust words saturate at the ship's limits, roughly a few
    // hundred m/s²; 0x7fff stands for that full deflection.
    std::array::from_fn(|k| {
        let component = (0..3)
            .map(|axis| acceleration[axis] * rows[k][axis])
            .sum::<f64>();
        (component / 300.0 * 32767.0).round() as i32
    })
}

/// One frame of the autopilot: turn toward `target` with the steering keys, hold
/// the set speed the distance allows and the time acceleration it can use.
pub fn prototype_autopilot(
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
    let position = flight.absolute_position();
    let offset: [f64; 3] = std::array::from_fn(|k| (target[k] - position[k]) as f64);
    let range = offset.iter().map(|v| v * v).sum::<f64>().sqrt();
    let dir = offset.map(|v| v / range.max(1.0));
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (right, up, forward) = (dot(dir, rows[0]), dot(dir, rows[1]), dot(dir, rows[2]));
    let velocity = motion
        .velocity
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt()
        * MM_PER_SECOND
        / 1000.0;
    let mut keys = Keys::default();
    if range <= stop + ARRIVAL_SLOP {
        keys.slower = true;
        return Command {
            keys,
            acceleration: 0,
            arrived: velocity < 1.0,
            aligned: forward,
            face: None,
            skip: None,
            thrust: None,
            align: None,
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
            thrust: None,
            align: None,
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
            thrust: None,
            align: None,
        };
    }
    // Speed: the most that can still be stopped within the distance left (the
    // reverse thrust gives about 90 m/s per second), held while the nose swings.
    let remaining = range - stop;
    let metres = remaining / 1000.0;
    let limit = f64::from(i32::MAX) * MM_PER_SECOND / 1000.0; // the largest set speed
    let envelope = (2.0 * BRAKING * 0.4 * metres).sqrt().min(limit);
    let current = flight.set_speed_metres();
    let actual = velocity.max(current);
    if actual > envelope {
        // Brake on the real speed, not only on the set speed: the set speed
        // steps at the keys' fixed rate and lags the curve over the last
        // hundred metres.
        keys.slower = true;
    } else if forward > 0.9995 && current < envelope * 0.8 {
        keys.faster = true;
    }
    // Time acceleration while the nose is on the target; never so much that one
    // frame covers more than a twentieth of what is left (or the braking is missed).
    let per_frame = actual * 0.05;
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
        thrust: None,
        align: None,
    }
}

/// Which map is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    world_epoch: f64,
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
    autopilot_notice: Option<String>,
    /// The body the player has locked on (clicked in the view or cycled to): what the
    /// autopilot button flies to.
    pub locked: Option<usize>,
    /// The system the galaxy map has chosen (the original's `+0x268` selection and
    /// `+0x27c` system code). It is a hyperspace destination, not a body lock: `H`
    /// jumps there, even after the map closes.
    pub destination: Option<Destination>,
    /// The player's altitude above the nearest body (`DATA_008867`, millimetres).
    /// F908 refuses a jump below [`HYPERSPACE_ALTITUDE`].
    pub altitude: i32,
    /// The station the player is docked at (or last docked at).
    pub docked: Option<usize>,
    /// Each station's traffic record (F654 and friends), made when first needed.
    traffic: HashMap<usize, Starport>,
    /// F381 has granted launch permission at the open-air starport (the launch action
    /// needs it).
    pub launch_cleared: bool,
    /// F1022 has warned about an uncleared touch at this port (`DATA_008873`
    /// set to 6), so it does not repeat until the next landing.
    warned_at: Option<usize>,
    /// The NPC traffic of the current system (F688/F635), ticked in flight.
    pub ships: Vec<NpcShip>,
    /// The space-dust field (F1035/F769), ticked in flight and drawn by the
    /// follow-up. Its own stream is seeded from the system code (EXP-020).
    pub dust: dust::Field,
    /// The player's legal standing (`DATA_008646` bit 0 / `DATA_008647`): the
    /// police alert that makes police and military ships hostile.
    pub legal: npc::Legal,
    /// The commander's military/Elite standing (`DATA_008911`-`8913`,
    /// `DATA_009091`).
    pub career: npc::Career,
    /// F869's system allegiance (`DATA_008996`, the system word `>> 6`): the
    /// fine-ledger and criminal-record slot F236/F655/F312 read.
    pub jurisdiction: usize,
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
    /// `DATA_008877` (`player+0xef`): the last missile the player launched,
    /// which F10's missile view rides. Cleared when that missile dies (F1102).
    pub missile_view: Option<u32>,
    /// The missiles in flight (F941 objects, model 4..13).
    pub missiles: crate::missile::MissileField,
    /// F1054's chaff objects (`DATA_007253`). F1057 points missiles at them.
    pub decoys: Vec<crate::missile::Decoy>,
    /// The next [`ffe_core::missile::DECOY_BASE`] id.
    next_decoy: u32,
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
    /// F908's transit (mode 4) while a hyperspace jump runs.
    pub transit: Option<Transit>,
    /// The market tables and names (cache v20), when the cache has them.
    market_tables: Vec<u8>,
    /// The galaxy tables (cache v21): star positions and the hyperdrive records.
    galaxy_tables: Vec<u8>,
    /// The ship and fuel the galaxy list and the jump use.
    pub drive: Option<DriveState>,
    /// The markets of the stations visited in this system.
    markets: HashMap<(u32, usize), StationMarket>,
    /// The BBS advertisements and hull stock of each visited starport (F622).
    offers: HashMap<(u32, usize), StationOffers>,
    /// The day marker for the once-a-day updates (F152's `DATA_008808`).
    calendar: Calendar,
    /// The mission/contract deadlines the missions stream has registered. They
    /// are counted in [`Self::day`]s and expire on the day tick.
    deadlines: Vec<Deadline>,
    /// The deadlines that fell due since the last [`Self::take_expired`].
    expired: Vec<Deadline>,
    /// The autopilot has reached a station's approach point and is flying into
    /// the slot (or, at an open-air starport, holds F655's cleared pad hand-off
    /// for the descent).
    pub final_leg: bool,
    /// The game clock of the last F655 request at an open-air starport, so a
    /// refusal is retried at a readable rate rather than every frame.
    clearance_at: Option<f64>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveState {
    pub mass: i32,
    pub class: i32,
    pub fuel: i32,
}

/// The star the galaxy map has selected, as the original stores it: the system code
/// (`+0x27c`) and its F856 position (`+0x25c..0x264`). It is a jump destination, not
/// a body lock. Its distance is measured from the player's current system when asked,
/// as F832 does when the selection changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    /// The system code: sector x/y and the star's number in bits 26..31.
    pub code: u32,
    /// The star's position, 16.16 fixed point (F856).
    pub position: [i32; 3],
}

/// F908's altitude floor (`DATA_008867 >= 0xb71b00`): a ship deeper than this in a
/// planet's atmosphere cannot hyperspace.
pub const HYPERSPACE_ALTITUDE: i32 = 0x00b7_1b00;

/// F908's transit: the player in mode 4 while the clock runs at the hyperspace
/// rate (`DATA_007706` = 0xf880000, 2^18 times 1x) through the jump time.
/// F1066 counts it down and F1094 ends it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transit {
    /// Game seconds of the jump still to run (`+0x106`).
    pub remaining: f64,
    /// The whole jump (`+0x532c`).
    pub total: f64,
}

impl Transit {
    /// F1066's animation word for model 0xa5: the elapsed share times 145, << 8.
    pub fn animation(&self) -> u16 {
        let elapsed = (self.total - self.remaining).clamp(0.0, self.total);
        let share = if self.total > 0.0 {
            elapsed * 145.0 / self.total
        } else {
            145.0
        };
        ((share as u16) << 8) & 0xff00
    }
}

/// The clock rate in transit: F908's `0xf880000` against 1x's `0x3e2`.
pub const HYPERSPACE_RATE: f64 = 262_144.0;

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

/// Burn the fuel of a jump from the hold, as F901 does: hydrogen for a normal
/// drive, military fuel for class 10 and up, which leaves the same tonnes of
/// radioactive waste. The caller passes the hold and the drive the jump used.
pub fn burn_fuel(hold: &mut Vec<u16>, drive: Option<DriveState>, tonnes: i32) {
    let Some(drive) = drive else { return };
    hold.resize(ffe_core::market::NAMES, 0);
    let tonnes = u16::try_from(tonnes).unwrap_or(0);
    if drive.class >= ffe_core::hyperspace::MILITARY_CLASS {
        let slot = &mut hold[ffe_core::hyperspace::MILITARY_FUEL];
        *slot = slot.saturating_sub(tonnes);
        let waste = &mut hold[ffe_core::hyperspace::RADIOACTIVES];
        *waste = waste.saturating_add(tonnes);
    } else {
        let slot = &mut hold[ffe_core::hyperspace::HYDROGEN];
        *slot = slot.saturating_sub(tonnes);
    }
}

/// A station's market with the stream its daily drift draws from and the game day it
/// was last brought up to. The F870 conditions are kept with it so the daily
/// tick can run from the stored state without re-reading the system tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationMarket {
    pub market: Market,
    rng: RandomState,
    day: u32,
    population: i64,
    danger: i64,
    flags: [u8; 37],
}

impl StationMarket {
    /// F633's market half for one day.
    fn daily(&mut self, tables: &MarketTables<'_>) -> Result<(), String> {
        self.market.daily(
            tables,
            &Conditions {
                population: self.population,
                danger: self.danger,
                flags: &self.flags,
            },
            &mut self.rng,
        )
    }
}

/// A starport's generated BBS and hull stock (F622/F633), with the stream and
/// game day the daily update uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationOffers {
    pub posts: Vec<bulletin::Post>,
    /// The `+0x2e6` `{model, random word}` entries, in list order.
    pub hulls: Vec<(u32, u16)>,
    rng: npc::Random,
    day: u32,
    /// The F622/F869 record the daily generation needs: the system and station
    /// names, the neighbourhood the advertisements name, population, danger,
    /// allegiance and the two shipyard bytes.
    code: u32,
    name: String,
    station_name: String,
    destinations: Vec<bulletin::Destination>,
    population: u32,
    danger: u32,
    allegiance: u16,
    military: u32,
    civil: u32,
    starports: u32,
}

impl StationOffers {
    /// F437's rows for the player's current ship and its drive class. The
    /// player's model is the fitted drive's source in this port (there is no
    /// separate drive state), so the trade-in is the reference 950 * mass word.
    pub fn rows(&self, player: u32, fitted_drive: Option<u8>) -> Vec<shipyard::Hull> {
        self.hulls
            .iter()
            .filter_map(|(model, seed)| shipyard::row(*model, player, fitted_drive, *seed))
            .collect()
    }

    /// F442's buy: cash and part exchange, then the bought hull is replaced by
    /// the player's old model in the station list. The caller applies the new
    /// model and clears the fitted items (F441).
    pub fn buy(
        &mut self,
        row: usize,
        player: u32,
        cash_tenths: i64,
    ) -> Result<shipyard::Purchase, String> {
        let hull = self
            .hulls
            .get(row)
            .and_then(|(model, seed)| shipyard::row(*model, player, None, *seed))
            .ok_or_else(|| "That hull is no longer on the list.".to_owned())?;
        let purchase = shipyard::buy(&hull, player, cash_tenths).ok_or_else(|| {
            "You cannot afford that ship, even with the part exchange.".to_owned()
        })?;
        shipyard::part_exchange(&mut self.hulls, row, player);
        Ok(purchase)
    }

    /// F633's BBS and shipyard half for one day. Returns whether the visible
    /// board and stock changed. The name table is optional, as in F622.
    fn daily(&mut self, cache: &VerifiedCache, day: u32) -> (bool, bool) {
        let names = cache
            .asset("base:text/names")
            .and_then(|bytes| Names::new(bytes).ok());
        let world = bulletin::World {
            code: self.code,
            name: &self.name,
            population: self.population,
            danger: self.danger,
            allegiance: self.allegiance,
            station_name: &self.station_name,
            destinations: &self.destinations,
            names: names.as_ref(),
        };
        let posts = self.posts.clone();
        bulletin::daily(&mut self.rng, &mut self.posts, &world, day);
        let hulls = self.hulls.clone();
        shipyard::daily(
            &mut self.rng,
            &mut self.hulls,
            self.allegiance,
            self.population,
            self.military,
            self.civil,
            self.starports,
        );
        (posts != self.posts, hulls != self.hulls)
    }
}

/// One day's periodic updates, the port's record of F633 plus the deadline
/// hook. `day` is the navigator's day counter ([`Navigator::day`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayTick {
    pub day: u32,
    /// Visited markets that ran F633's price/stock drift.
    pub markets: usize,
    /// Bulletin boards whose daily draw replaced an entry.
    pub boards: usize,
    /// Shipyards whose daily draw changed the hull list.
    pub hulls: usize,
    /// Deadline ids that fell due on this day.
    pub expired: Vec<u32>,
}

/// A mission or contract deadline registered through
/// [`Navigator::schedule_deadline`].
/// `day` is counted like [`Navigator::day`]: whole days since the new game
/// (or the sandbox clock's start), so a mission's own day arithmetic can be
/// passed in directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    pub id: u32,
    pub day: u32,
}

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
    /// F783: the police object (`+0x118 == 0xe`) was removed and its alert
    /// cleared the wanted state.
    AlertCleared,
    /// F787/F738/F927: a ship jumped out and its slot was freed.
    Escaped { name: String },
    /// F754/F941: an NPC launched a pylon at the player. The caller resolves the
    /// code to a `Spec` (the core has no cache) and calls F941's launch.
    MissileLaunch {
        owner: u32,
        name: String,
        code: u8,
        pylon: usize,
        target: u32,
        position: [i64; 3],
        axes: [[f64; 3]; 3],
        launcher_radius: i64,
    },
    /// F757/F935: an evading NPC fired an ECM burst. `naval` is the system's
    /// extra `DATA_008835` bit.
    Ecm { name: String, naval: bool },
}

/// F931/F941's unlawful-discharge report: F236's message plus whether a police
/// or military ship answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalReport {
    pub offence: ffe_core::npc::OffenceReport,
    pub police: bool,
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
    /// F942's statistics `+0x16` word for a destroyed ship.
    pub rating: i16,
    /// F942 changed the Elite-rating word (`score >> 16` compared).
    pub rating_changed: bool,
    /// A police or military ship turned hostile because of this hit.
    pub police: bool,
    /// F942 destroyed the ship.
    pub destroyed: bool,
}

/// What a soft touch inside a starport's region (F1033's `0x20` with `0x40`)
/// does in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionTouch {
    /// F1019 (autopilot, mode 2) or F1024 (manual with clearance, mode 6):
    /// the ship lands on the port.
    Land,
    /// F1021 (free flight): the ship is not landed; the mode goes back to
    /// F1013's `0x24` climb. Without clearance F1022 warns once.
    Climb(Option<String>),
    /// F1024: a pad other than the allocated bay; F1022 warns once.
    WrongPad(Option<String>),
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

/// F741's station approach offset: `[0, 0, 0xfa0]` words of `0x400` mm,
/// 4,096,000 mm out along the docking axis from the station's centre.
const STATION_APPROACH: f64 = 0xfa0 as f64 * 1024.0;
/// F741's open-air starport approach: `[0, 0x1770, 0]` in the port's frame, whose
/// words F593 scales by `0x400` (1024 mm), so 6,144,000 mm above its centre.
const STARPORT_APPROACH: f64 = 0x1770 as f64 * 1024.0;
/// How close to the approach point the autopilot's first leg stops: F610's
/// distance exponent 6, the band from 524,288 mm to 1,024,000 mm below which
/// F742 runs F745.
const APPROACH_STOP: f64 = 1.0e6;
/// Transfers in a row before the autopilot flies the remaining leg in real time.
/// The counter is not reset between whole transfers (the old reset let a fast
/// target teleport on every frame); engaging or locking a target clears it.
const MAX_SKIPS: u32 = 4;
/// The port's detour: how far above a blocking body's surface a line must pass
/// (a fraction of its radius, at least the margin in millimetres).
const DETOUR_CLEARANCE: f64 = 0.1;
const DETOUR_MARGIN: f64 = 5.0e6;

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit3(v: [f64; 3]) -> [f64; 3] {
    let length = dot3(v, v).sqrt().max(1e-9);
    v.map(|c| c / length)
}

/// F208's `DATA_009013` for each time acceleration step (1x .. 10000x): the
/// shift F597 compares the destination's distance shift against.
const ACCELERATION_SHIFTS: [i32; 5] = [0, 3, 7, 10, 12];
const STATION_FRAME: f64 = 2.0e8;
/// Within this many radii of a planet or moon, its motion carries the ship.
const WORLD_FRAME_RADII: f64 = 50.0;

/// F869's `DATA_008996`: the system word's `>> 6` allegiance, the six-record
/// ledger and criminal-record slot. A cache without system tables reads slot 0.
fn jurisdiction_of(cache: &VerifiedCache, code: u32) -> usize {
    cache
        .asset("base:world/system-tables")
        .and_then(|bytes| Tables::new(bytes).ok())
        .map_or(0, |tables| (tables.details(code).allegiance >> 6) as usize)
}

/// The dust field's own stream (EXP-020), keyed by the system code so the same
/// system always draws the same dust without touching F922/F1530's traffic
/// rolls. F1035 itself uses the game's shared `DATA_008818`/`DATA_008820`.
fn dust_seed(code: u32) -> (u32, u32) {
    (code ^ 0x6d5f_1b00, code.rotate_left(13) ^ 0x00d0_5700)
}

/// World seconds to the original's low/high clock words: fractional day first,
/// then whole days (F880/F1509/F1510; one day is 2^32 low-word ticks).
pub fn clock_words(seconds: f64) -> [u32; 2] {
    let days = seconds.max(0.0) / 86_400.0;
    let whole = days.floor();
    [((days - whole) * 4_294_967_296.0) as u32, whole as u32]
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
/// F998's starting cash for the type-1 Ross 154 arm (`DATA_008888`).
pub const NEW_GAME_CASH: i32 = 1000;
/// F998's ship for start type 1 (`+0x17`).
pub const NEW_GAME_SHIP: u32 = 23;
/// The player's state byte after F994 (`DATA_008870`): docked at an open-air starport.
pub const NEW_GAME_PLAYER_STATE: u8 = 0x30;
/// `DATA_008857`: the player's index among the physics objects (F994's `+0x86`).
pub const NEW_GAME_PLAYER_INDEX: u8 = 107;
/// F997's default time acceleration (`DATA_009012`).
pub const NEW_GAME_ACCELERATION: u32 = 994;
/// The tonnes F998's type-1 arm leaves out of the Eagle's hold (`0xc` in the
/// ported arm; the type-0 Saker uses [`DEFAULT_HOLD_LOSS`]).
pub const NEW_GAME_HOLD_LOSS: i32 = 0xc;
/// The physics-list slot of the type-1 starting starport (`0x6e`).
pub const NEW_GAME_STARPORT_SLOT: usize = 0x6e;

/// F998's default arm (start types 0, 4, 5 and later): the Saker Mk III
/// (model `0x1f`) at Old Blackelk on Hope in Gateway [-1,4], with 10,000 CR.
/// The DOS boot's "any other key" starts this one (`docs/notes/intro.md`).
pub const DEFAULT_START_TYPE: u8 = 0;
/// The Saker Mk III fighter, model slot `0x1f`.
pub const DEFAULT_SHIP: u32 = 0x1f;
/// F998's `DATA_008888` for the default arm: `0x2710` (10,000 CR, shown as
/// 1000.0 credits).
pub const DEFAULT_CASH: i32 = 10_000;
/// F998's default arm subtracts `0x10` t from the Saker's 23 t statistic,
/// leaving 7 t.
pub const DEFAULT_HOLD_LOSS: i32 = 0x10;
/// The default arm's hold (`DATA_008889`): F998 fits its equipment by setting
/// the object's bytes, so nothing more is taken off the 7 t. The reference
/// game reads 7 t here with 1 t used.
pub const DEFAULT_CARGO: i32 = 23 - DEFAULT_HOLD_LOSS;
/// The tonnes of hydrogen fuel F998's default arm puts in that hold
/// (`DATA_008901 = 1`, `DATA_008891 = 1` cargo used).
pub const DEFAULT_HYDROGEN: u16 = 1;
/// F998's default starport: Old Blackelk, physics slot `0x6a`.
pub const DEFAULT_STARPORT_SLOT: usize = 0x6a;
/// F994's player index for the default arm (`DATA_008857`).
pub const DEFAULT_PLAYER_INDEX: u8 = 87;

/// F832's seed for a system from its F856 position and F841 type word:
/// `(y >> 16) + (y << 16) + x - z`, or zero when the type word's low byte has
/// bit 0x80 (the marker the start table's extra word also uses: a zero seed
/// selects the hand-coded system, `sysgen::Generator::generate`).
fn system_seed(position: [i32; 3], startype: u8) -> u32 {
    if startype & 0x80 != 0 {
        return 0;
    }
    let [x, y, z] = position;
    (y >> 16)
        .wrapping_add(y << 16)
        .wrapping_add(x)
        .wrapping_sub(z) as u32
}

/// F832/F830/F869 for one system: `extra` is the F829 record's extra word (a
/// galaxy-table star's F841 type word plays the same role), and `position` is
/// its 16.16 record position (F856's for a table star).
fn system_entry(
    code: u32,
    position: [i32; 3],
    extra: u32,
    galaxy: &GalaxyTables,
    system: &Tables,
    strings: &Strings,
) -> Result<Entry, String> {
    let name = galaxy
        .system_name(strings, code)
        .ok_or_else(|| format!("F830 has no name for system {code:#x}"))?;
    // F869's first byte is `DATA_008995`: F832 leaves it where F855 reads the
    // generator's population-like parameter.
    let details = system.details(code);
    // F832: `stars` is the high extra byte's low three bits and the total mass
    // is the `extra & 0xf` row of DATA_006803 scaled by `stars & 0xf8`.
    let numstars = extra >> 8;
    let mut name_field = [0u8; 20];
    let length = name.len().min(name_field.len());
    name_field[..length].copy_from_slice(&name[..length]);
    Ok(Entry {
        name: String::from_utf8_lossy(&name).trim().to_owned(),
        params: Params {
            seed: system_seed(position, extra as u8),
            mass: galaxy.start_mass((extra & 0xf) as usize, numstars as u8),
            stars: numstars & 7,
            code,
            population: details.bytes[0] as i32,
            name: name_field,
        },
    })
}

/// F829/F832/F869/F830 for one record of `DATA_006820`: the system F830 names,
/// generated from the record's coordinates and extra word with F878's inputs.
/// Record 0 is the DOS default start (Gateway [-1,4], the planet Hope and the
/// starport Old Blackelk); record 1 is Ross 154, whose hand-coded template the
/// galaxy list already holds (its extra word's 0x80 bit selects that template,
/// F832).
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
    let system = Tables::new(
        cache
            .asset("base:world/system-tables")
            .ok_or("no system tables")?,
    )?;
    system_entry(code, [x, y, z], extra, &tables, &system, &strings)
}

/// The original's new-game state, kept for the local comparison with `--state`
/// and the new-game oracle. The playable uses the navigator's own fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// The navigator's serialisable state: where the commander is, what system,
/// which chart is open, the legal record and the landing/docking stage.
///
/// The bodies themselves are not stored: they are regenerated from the system
/// code and the saved clock, exactly as the original regenerates a system's
/// orbit positions from the game date. `index` fields therefore refer to the
/// regenerated body list, and [`Navigator::from_save`] checks every one.
///
/// Not stored: the NPC traffic, missiles and short-lived effects. The source
/// saves its object array; the port respawns traffic deterministically from the
/// system and clock when the live game first needs it. See
/// `docs/notes/persistence.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigatorSave {
    pub system_code: u32,
    pub clock: f64,
    pub start_clock: f64,
    #[serde(default)]
    pub world_epoch: Option<f64>,
    #[serde(default)]
    pub new_game: Option<NewGameState>,
    pub page: usize,
    pub view: View,
    pub selected: usize,
    #[serde(default)]
    pub target: Option<usize>,
    #[serde(default)]
    pub locked: Option<usize>,
    #[serde(default)]
    pub destination: Option<Destination>,
    pub altitude: i32,
    #[serde(default)]
    pub docked: Option<usize>,
    pub legal: ffe_core::npc::Legal,
    #[serde(default)]
    pub ship_lock: Option<u32>,
    #[serde(default)]
    pub missile_lock: Option<u32>,
    #[serde(default)]
    pub missile_view: Option<u32>,
    #[serde(default)]
    pub sequence: Option<SequenceSave>,
    #[serde(default)]
    pub takeoff: Option<u8>,
    #[serde(default)]
    pub landed: Option<LandedSave>,
    #[serde(default)]
    pub final_leg: bool,
    #[serde(default)]
    pub spawned: Option<usize>,
}

/// A mid-docking or mid-launch stage (F1067-F1088).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceSave {
    pub station: usize,
    pub state: u8,
    pub timeout: i32,
    #[serde(default)]
    pub drawn: Option<(u8, i32)>,
    pub launching: bool,
}

/// A landed state (`0x30`/`0x34`) and the body it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandedSave {
    pub state: u8,
    pub index: usize,
}

impl NavigatorSave {
    /// Scalar checks that do not need the generated body list. The caller
    /// checks indices against the regenerated system.
    pub fn validate(&self) -> Result<(), String> {
        if self
            .world_epoch
            .is_some_and(|epoch| !epoch.is_finite() || !(0.0..=1.0e12).contains(&epoch))
        {
            return Err("Invalid system creation clock.".to_owned());
        }
        if !self.clock.is_finite() || !self.start_clock.is_finite() {
            return Err("Save has a non-finite navigation clock.".to_owned());
        }
        if !(0.0..=1.0e12).contains(&self.clock) || !(0.0..=1.0e12).contains(&self.start_clock) {
            return Err("Save has an out-of-range navigation clock.".to_owned());
        }
        if self.page > 1_000_000
            || self.selected > 1_000_000
            || self
                .target
                .into_iter()
                .chain(self.locked)
                .chain(self.docked)
                .chain(self.spawned)
                .any(|index| index > 1_000_000)
        {
            return Err("Save has an out-of-range navigation index.".to_owned());
        }
        if self.altitude < 0 {
            return Err("Save has a negative altitude.".to_owned());
        }
        if let Some(sequence) = &self.sequence {
            if sequence.station > 1_000_000
                || !matches!(
                    sequence.state,
                    state::ALIGN
                        | state::STARPORT_ALIGN
                        | state::OPEN_DOOR
                        | state::INTO_LIFT
                        | state::TURN
                        | state::CLOSE_DOOR
                        | state::WAIT_IN_LIFT
                        | state::OPEN_INNER
                        | state::INTO_BAY
                        | state::DOCKED
                        | state::DOCKED_THARGOID
                        | state::LAUNCH_INTO_LIFT
                        | state::LAUNCH_CLOSE
                        | state::LAUNCH_WAIT
                        | state::LAUNCH_OPEN
                        | state::LAUNCH_OUT
                        | state::LAUNCHING_STATION
                        | state::LAUNCHING_STARPORT
                )
            {
                return Err("Save has an invalid docking/launch stage.".to_owned());
            }
        }
        if let Some(takeoff) = self.takeoff {
            if !matches!(
                takeoff,
                state::LANDED | state::LANDED_ROUGH | state::TAKEOFF | state::AFTER_TAKEOFF
            ) {
                return Err("Save has an invalid takeoff state.".to_owned());
            }
        }
        if let Some(landed) = self.landed {
            if landed.index > 1_000_000
                || !matches!(
                    landed.state,
                    state::LANDED | state::LANDED_ROUGH | state::LANDING | state::STARPORT_LANDING
                )
            {
                return Err("Save has an invalid landed state.".to_owned());
            }
        }
        if let Some(new_game) = &self.new_game {
            if new_game.start_type > 5 || new_game.cargo < 0 || new_game.cash < 0 {
                return Err("Save has an invalid new-game record.".to_owned());
            }
        }
        Ok(())
    }
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
        let jurisdiction = jurisdiction_of(cache, entries[current].params.code);
        let seed = dust_seed(entries[current].params.code);
        Ok(Self {
            entries,
            current,
            bodies,
            clock: 0.0,
            start_clock: 0.0,
            world_epoch: 0.0,
            new_game: None,
            drift: [0; 3],
            skips: 0,
            page: 0,
            view: View::Closed,
            selected: 0,
            target: None,
            autopilot_notice: None,
            locked: None,
            destination: None,
            altitude: i32::MAX,
            docked: None,
            traffic: HashMap::new(),
            launch_cleared: false,
            warned_at: None,
            ships: Vec::new(),
            dust: dust::Field::new(seed.0, seed.1),
            legal: npc::Legal::default(),
            career: npc::Career::default(),
            jurisdiction,
            persisted: Vec::new(),
            player_at: [0; 3],
            effects: Vec::new(),
            ship_lock: None,
            missile_lock: None,
            missile_view: None,
            missiles: crate::missile::MissileField::default(),
            decoys: Vec::new(),
            next_decoy: 0,
            spawned: None,
            sequence: None,
            takeoff: None,
            landed: None,
            pads: cache
                .asset("base:gameplay/pads")
                .and_then(|bytes| Pads::from_bytes(bytes).ok()),
            notice: None,
            carry: 0.0,
            transit: None,
            market_tables: cache
                .asset("base:market/tables")
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            markets: HashMap::new(),
            offers: HashMap::new(),
            calendar: Calendar::at(0),
            deadlines: Vec::new(),
            expired: Vec::new(),
            galaxy_tables: cache
                .asset("base:world/galaxy-tables")
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            drive: None,
            final_leg: false,
            clearance_at: None,
            frame: None,
            ports: HashMap::new(),
            memory: Memory::default(),
            refreshed: 0.0,
        })
    }

    /// The original's new game (F829/F998): the DOS default is record 0, Gateway
    /// [-1,4] on Hope at Old Blackelk, with F998's type-0 Saker Mk III. Record 1
    /// is Ross 154's Eagle; the menu's types 2-5 map to the checked type 1. The
    /// bodies are placed at the 3250 clock and the player is F994-positioned on
    /// the open-air pad.
    pub fn new_game(cache: &VerifiedCache, start_type: u8) -> Result<Self, String> {
        let (record_index, start_type) = match start_type {
            0 => (0usize, 0u8),
            1 => (1, 1),
            2..=5 => (1, 1),
            _ => return Err(format!("start type {start_type} is not in DATA_006820")),
        };
        let default_arm = start_type == DEFAULT_START_TYPE;
        let record = START_SYSTEMS[record_index];
        let code = (record.0 >> 16) as u32 & 0x1fff
            | ((record.1 >> 16) as u32 & 0x1fff) << 13
            | (record.3 & 0x3f) << 26;
        // The start system's real galaxy-table neighbours are enterable as the
        // original's map destinations are (F852/F860/F832/F878), so the default
        // Gateway start can reach Titican and other systems within the drive's
        // reach. The hand-coded and test entries keep their places.
        let mut entries = galaxy_around(cache, code, NEIGHBOUR_REACH)?;
        let current = if default_arm {
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
        let (ship, cash, starport_slot, player_index) = if default_arm {
            (
                DEFAULT_SHIP,
                DEFAULT_CASH,
                DEFAULT_STARPORT_SLOT,
                DEFAULT_PLAYER_INDEX,
            )
        } else {
            (
                NEW_GAME_SHIP,
                NEW_GAME_CASH,
                NEW_GAME_STARPORT_SLOT,
                NEW_GAME_PLAYER_INDEX,
            )
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
        // F994 temporarily marks station +0xd1 occupied before F650 and
        // clears it afterward. Omitting that sentinel selects bay 1 rather
        // than bay 0 at both original starts, facing a different part of town.
        port.pads[1] = 0xff;
        let bay = port.allocate(PLAYER);
        port.pads[1] = 0;
        let bay = bay.ok_or("the start starport has no free bay")?;
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
        let cargo = if default_arm {
            DEFAULT_CARGO
        } else {
            (models.cargo_space(ship).unwrap_or(0) - NEW_GAME_HOLD_LOSS).max(0)
        };
        let mut nav = Self::assemble(cache, entries, current, bodies)?;
        nav.pads = Some(pads);
        nav.clock = NEW_GAME_CLOCK;
        nav.start_clock = NEW_GAME_CLOCK;
        nav.world_epoch = NEW_GAME_CLOCK;
        nav.calendar = Calendar::at(nav.day());
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

    /// This navigator's serialisable state. The system is identified by its
    /// galaxy code, not by the vector index, and the clock replaces the object
    /// positions.
    pub fn save_state(&self) -> NavigatorSave {
        NavigatorSave {
            system_code: self.entries[self.current].params.code,
            clock: self.clock,
            start_clock: self.start_clock,
            world_epoch: Some(self.world_epoch),
            new_game: self.new_game,
            page: self.page,
            view: self.view,
            selected: self.selected,
            target: self.target,
            locked: self.locked,
            destination: self.destination,
            altitude: self.altitude,
            docked: self.docked,
            legal: self.legal,
            ship_lock: self.ship_lock,
            missile_lock: self.missile_lock,
            missile_view: self.missile_view,
            sequence: self.sequence.map(|sequence| SequenceSave {
                station: sequence.station,
                state: sequence.stages.state,
                timeout: sequence.stages.timeout,
                drawn: sequence.stages.drawn,
                launching: sequence.launching,
            }),
            takeoff: self.takeoff.map(|takeoff| takeoff.state),
            landed: self.landed.map(|landed| LandedSave {
                state: landed.landing.state,
                index: landed.index,
            }),
            final_leg: self.final_leg,
            // Ephemeral: the traffic is regenerated on load, so the save does
            // not claim the system already has its ships.
            spawned: None,
        }
    }

    /// Rebuild a navigator from a save. The system's bodies come from the cache
    /// at the saved clock; every stored index is checked against that list.
    pub fn from_save(cache: &VerifiedCache, save: &NavigatorSave) -> Result<Self, String> {
        save.validate()?;
        let mut nav = if let Some(new_game) = save.new_game {
            Self::new_game(cache, new_game.start_type)?
        } else {
            Self::new(cache)?
        };
        if nav.entries[nav.current].params.code != save.system_code {
            let index = nav
                .entries
                .iter()
                .position(|entry| entry.params.code == save.system_code)
                .ok_or_else(|| {
                    format!(
                        "Save's system code {:#x} is not generated by this cache.",
                        save.system_code
                    )
                })?;
            nav.current = index;
        }
        nav.world_epoch = save.world_epoch.unwrap_or(save.clock);
        nav.bodies = bodies_since(
            cache,
            &nav.entries[nav.current],
            clock_words(nav.world_epoch),
            clock_words(save.clock),
        )?;
        nav.clock = save.clock;
        nav.start_clock = save.start_clock;
        nav.new_game = save.new_game;
        nav.page = save.page;
        nav.view = save.view;
        nav.selected = save.selected;
        nav.target = save.target;
        nav.locked = save.locked;
        nav.destination = save.destination;
        nav.altitude = save.altitude;
        nav.docked = save.docked;
        nav.legal = save.legal;
        nav.ship_lock = save.ship_lock;
        nav.missile_lock = save.missile_lock;
        nav.missile_view = save.missile_view;
        nav.final_leg = save.final_leg;
        nav.spawned = save.spawned;
        nav.sequence = save.sequence.map(|sequence| Sequence {
            station: sequence.station,
            stages: Stages {
                state: sequence.state,
                timeout: sequence.timeout,
                drawn: sequence.drawn,
            },
            launching: sequence.launching,
        });
        nav.takeoff = save.takeoff.map(|state| Takeoff { state });
        nav.landed = save.landed.map(|landed| Landed {
            landing: Landing {
                state: landed.state,
            },
            index: landed.index,
        });
        // Traffic, missiles and effects are regenerated rather than restored.
        nav.ships.clear();
        nav.persisted.clear();
        nav.missiles = crate::missile::MissileField::default();
        nav.effects.clear();
        nav.spawned = None;
        // The bodies above were already placed at `save.clock`, so the next
        // orbital refresh must not run until the clock has actually moved on
        // from it; otherwise the first frame after a load advances the body
        // positions while the restored ship stays behind.
        nav.refreshed = save.clock;
        nav.drift = [0; 3];
        nav.skips = 0;
        nav.carry = 0.0;
        nav.frame = None;
        // A restored docked/sequence station needs its bay record for a launch.
        for index in [nav.docked, nav.sequence.map(|sequence| sequence.station)]
            .into_iter()
            .flatten()
        {
            let Some(body) = nav.bodies.get(index) else {
                return Err(format!(
                    "Save's station index {index} is not in the system."
                ));
            };
            if !matches!(body.kind, Kind::Station | Kind::Starport) {
                return Err(format!("Save's docking body {index} is not a station."));
            }
            let bays = body.bays;
            nav.traffic
                .entry(index)
                .or_insert_with(|| Starport::new(bays));
        }
        // Every stored body index must exist after regeneration.
        if nav.target.is_some_and(|index| index >= nav.bodies.len())
            || nav.locked.is_some_and(|index| index >= nav.bodies.len())
            || nav.docked.is_some_and(|index| index >= nav.bodies.len())
            || nav
                .landed
                .is_some_and(|landed| landed.index >= nav.bodies.len())
            || nav
                .sequence
                .is_some_and(|sequence| sequence.station >= nav.bodies.len())
        {
            return Err("Save refers to a body that is not in its system.".to_owned());
        }
        if nav.page >= nav.pages() || nav.selected >= nav.listed().len() {
            return Err("Save's navigation selection is out of range.".to_owned());
        }
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
        let body = self.bodies.get(index)?;
        self.port_for(body)
    }

    /// The docking port of a body's model (`None` for a non-station).
    fn port_for(&self, body: &Body) -> Option<Port> {
        if body.kind != Kind::Station {
            return None;
        }
        let (front, radius) = self.ports.get(&body.model).copied().unwrap_or((None, None));
        Some(Port::new(body, front, radius))
    }

    /// F741's long-leg destination offset for a body in the system frame: the
    /// station's docking axis or the starport's up row. The offset follows the
    /// body's orientation, which the system generator turns by F866, so a long
    /// approach tracks the moving slot rather than a point fixed at the start.
    fn long_leg_offset(&self, body: &Body) -> [f64; 3] {
        match body.kind {
            Kind::Station => self
                .port_for(body)
                .map(|port| port.axis.map(|a| a * (port.front + STATION_APPROACH)))
                .unwrap_or([0.0; 3]),
            Kind::Starport => {
                let up = unit([body.rows[3], body.rows[4], body.rows[5]].map(f64::from));
                up.map(|v| v * STARPORT_APPROACH)
            }
            _ => [0.0; 3],
        }
    }

    /// F741's autopilot approach point at an open-air starport: `0x1770` words
    /// of `0x400` mm along the port's up row (the face that carries the pads),
    /// 6,144,000 mm above its centre.
    pub fn starport_approach_point(&self, index: usize) -> Option<[i64; 3]> {
        let body = self
            .bodies
            .get(index)
            .filter(|b| b.kind == Kind::Starport)?;
        let up = unit([body.rows[3], body.rows[4], body.rows[5]].map(f64::from));
        Some(std::array::from_fn(|k| {
            body.position[k] + (up[k] * STARPORT_APPROACH).round() as i64
        }))
    }

    /// F650's pad point for the granted bay of an open-air starport, in the
    /// port's frame (`Pads::position`).
    pub fn starport_pad_point(&self, index: usize, bay: u16) -> Option<[i64; 3]> {
        self.pad_point(index, bay)
    }

    /// The nearest station or open-air starport within clearance range of
    /// `ship` (F655 asks either).
    pub fn nearest_station(&self, ship: [i64; 3]) -> Option<usize> {
        (0..self.bodies.len())
            .filter(|i| matches!(self.bodies[*i].kind, Kind::Station | Kind::Starport))
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
    /// (F633's market half). Every day after the first visit is applied once, however
    /// many frames or how much time acceleration produced it.
    pub fn market(
        &mut self,
        cache: &VerifiedCache,
        index: usize,
    ) -> Result<&mut StationMarket, String> {
        let day = self.day();
        let code = self.entries[self.current].params.code;
        let (population, danger, flags) = self.conditions(cache)?;
        let conditions = Conditions {
            population,
            danger,
            flags: &flags,
        };
        let tables = MarketTables::new(&self.market_tables)
            .map_err(|_| "this cache has no market tables (cache v20)".to_owned())?;
        let key = (code, index);
        if let std::collections::hash_map::Entry::Vacant(slot) = self.markets.entry(key) {
            let mut rng = RandomState::new(
                code ^ 0x4d41_524b,
                (index as u32 + 1).wrapping_mul(0x9e37_79b9),
            );
            let market = Market::generate(&tables, &conditions, &mut rng)?;
            slot.insert(StationMarket {
                market,
                rng,
                day,
                population,
                danger,
                flags,
            });
        }
        let station = self.markets.get_mut(&key).expect("just made");
        for _ in (station.day + 1)..=day {
            station.daily(&tables)?;
        }
        station.day = station.day.max(day);
        Ok(station)
    }

    /// The name F454 puts at the black market's title (`0x98d7`'s `0x920d`
    /// name code). F639/F640 store the company seed in the advertisement's
    /// StrVars[3]; the name generator's kind 0x0d is the company form.
    pub fn company_name(&self, cache: &VerifiedCache, seed: u32) -> Option<String> {
        let names = Names::new(cache.asset("base:text/names")?).ok()?;
        let bytes = names.generate(0x0d, seed)?;
        Some(String::from_utf8_lossy(&bytes).trim().to_owned())
    }

    /// F233's event-5 smuggling scan: `random & 7 < DATA_009010` (the system
    /// record's byte `+7`, `Details::bytes[2]`), and then, when the hold has
    /// any commodity whose stock flag is bit 7, `F237(7)` records "Smuggling
    /// illegal goods" against the current starport (2500 printed, 25000 in
    /// the ledger, no wanted bit: `DATA_005039[7]`'s flags are 2).
    ///
    /// The original runs this from its special-slot event whenever F931/F941
    /// or F291 write slot 5; the port gives it a station docked and a stable
    /// stream instead, and the caller runs it at a customs checkpoint.
    pub fn smuggling_scan(
        &mut self,
        cache: &VerifiedCache,
        hold: &[u16],
    ) -> Option<npc::OffenceReport> {
        let flags = self.stock_flags(cache).ok()?;
        if ffe_core::market::illegal_tonnes(hold, &flags) == 0 {
            return None;
        }
        let blob = cache.asset("base:world/system-tables")?;
        let tables = Tables::new(blob).ok()?;
        let details = tables.details(self.entries[self.current].params.code);
        let danger = details.bytes[2];
        let mut rng = RandomState::new(
            self.entries[self.current].params.code ^ 0x534d_5547,
            (self.clock as u32)
                ^ self.legal.offences.wrapping_mul(0x9e37_79b9)
                ^ (hold.iter().map(|t| u32::from(*t)).sum::<u32>() << 16),
        );
        if u32::from(rng.next_u16() & 7) >= danger {
            return None;
        }
        self.docked.or_else(|| self.landed_body())?;
        self.legal.record(npc::SMUGGLING, Some(self.jurisdiction))
    }

    /// The system list as the bulletin's destination picker sees it: the
    /// original F857 neighbourhood, with each entry's population and danger.
    fn offers_destinations(&self, cache: &VerifiedCache) -> Vec<bulletin::Destination> {
        let Some(blob) = cache.asset("base:world/system-tables") else {
            return Vec::new();
        };
        let Ok(tables) = Tables::new(blob) else {
            return Vec::new();
        };
        self.entries
            .iter()
            .map(|entry| {
                let market = tables.market(entry.params.code);
                bulletin::Destination {
                    code: entry.params.code,
                    name: entry.name.clone(),
                    population: market.population,
                    danger: market.danger,
                }
            })
            .collect()
    }

    /// A starport's BBS and hull stock: F622 on first use, then F633's daily
    /// half. The stream is seeded by the system code and station, like the
    /// market's (the original uses its one global stream; the port keeps a
    /// station stream so a board is stable whenever it is revisited).
    pub fn station_offers(
        &mut self,
        cache: &VerifiedCache,
        index: usize,
    ) -> Result<&mut StationOffers, String> {
        let day = self.day();
        let code = self.entries[self.current].params.code;
        let blob = cache
            .asset("base:world/system-tables")
            .ok_or("this cache has no system tables (cache v15)")?;
        let tables = Tables::new(blob)?;
        let details = tables.details(code);
        let market = tables.market(code);
        let name = self.system_name().to_owned();
        let station_name = self
            .bodies
            .get(index)
            .map(|body| body.name.clone())
            .ok_or("no such station")?;
        let starports = self
            .bodies
            .iter()
            .filter(|body| body.kind == Kind::Starport)
            .count() as u32;
        let destinations = self.offers_destinations(cache);
        let names_bytes = cache.asset("base:text/names");
        let names = names_bytes.and_then(|bytes| Names::new(bytes).ok());
        let world = bulletin::World {
            code,
            name: &name,
            population: market.population,
            danger: market.danger,
            allegiance: (details.allegiance >> 6) as u16,
            station_name: &station_name,
            destinations: &destinations,
            names: names.as_ref(),
        };
        let key = (code, index);
        if let std::collections::hash_map::Entry::Vacant(slot) = self.offers.entry(key) {
            let mut rng = npc::Random::new(
                code ^ 0x4253_5342,
                (index as u32 + 1).wrapping_mul(0x9e37_79b9),
            );
            let posts = bulletin::generate(&mut rng, &world, day);
            let hulls = shipyard::stock(
                &mut rng,
                (details.allegiance >> 6) as u16,
                market.population,
                details.bytes[1],
                details.bytes[3],
                starports,
            );
            slot.insert(StationOffers {
                posts,
                hulls,
                rng,
                day,
                code,
                name,
                station_name,
                destinations,
                population: market.population,
                danger: market.danger,
                allegiance: (details.allegiance >> 6) as u16,
                military: details.bytes[1],
                civil: details.bytes[3],
                starports,
            });
        }
        let offers = self.offers.get_mut(&key).expect("just made");
        for d in (offers.day + 1)..=day {
            offers.daily(cache, d);
        }
        offers.day = offers.day.max(day);
        Ok(offers)
    }

    /// The last whole day whose periodic updates ran (`DATA_008808`).
    pub fn last_day_tick(&self) -> u32 {
        self.calendar.day()
    }

    /// Run F633's daily updates for every day the clock has crossed since the
    /// last call: the visited markets' price and stock drift, the bulletin
    /// boards' daily replacement and the shipyards' hull stock. Each day is
    /// processed once, in order, so one long frame, a time-accelerated run and
    /// a jump of several days all leave the same state.
    ///
    /// The returned records are one per day: what changed, and which mission
    /// deadlines fell due. Callers that need those from an [`Self::advance`]
    /// (which runs this itself) drain [`Self::take_expired`] instead.
    pub fn tick_days(&mut self, cache: &VerifiedCache) -> Vec<DayTick> {
        let days = self.calendar.advance(self.day());
        if days.is_empty() {
            return Vec::new();
        }
        // The tables are taken out of `self` so the markets can be visited while
        // the drift draws from them.
        let blob = self.market_tables.clone();
        let tables = MarketTables::new(&blob).ok();
        let mut ticks = Vec::new();
        for day in days {
            let mut tick = DayTick {
                day,
                ..DayTick::default()
            };
            if let Some(tables) = tables.as_ref() {
                for station in self.markets.values_mut() {
                    if station.day < day {
                        if station.daily(tables).is_ok() {
                            tick.markets += 1;
                        }
                        station.day = day;
                    }
                }
            }
            for offers in self.offers.values_mut() {
                if offers.day < day {
                    let (board, hulls) = offers.daily(cache, day);
                    tick.boards += usize::from(board);
                    tick.hulls += usize::from(hulls);
                    offers.day = day;
                }
            }
            self.expire(day, &mut tick);
            ticks.push(tick);
        }
        ticks
    }

    /// Register (or move) a mission/contract deadline. The missions stream can
    /// pass its own "day" arithmetic; [`Self::day`] is the same counter (whole
    /// days since the new game/sandbox start). On the day tick, every deadline
    /// whose day has arrived is queued for [`Self::take_expired`] and reported
    /// in that day's [`DayTick`]. Re-registering an id replaces its day.
    pub fn schedule_deadline(&mut self, id: u32, day: u32) {
        self.deadlines.retain(|deadline| deadline.id != id);
        self.deadlines.push(Deadline { id, day });
    }

    /// Remove a deadline (a mission completed or abandoned). Returns whether it
    /// was registered.
    pub fn cancel_deadline(&mut self, id: u32) -> bool {
        let before = self.deadlines.len();
        self.deadlines.retain(|deadline| deadline.id != id);
        before != self.deadlines.len()
    }

    /// The deadlines still pending, in registration order.
    pub fn deadlines(&self) -> &[Deadline] {
        &self.deadlines
    }

    /// Take the deadlines that fell due since the last call. The missions
    /// stream calls this after advancing time and expires or reports them.
    pub fn take_expired(&mut self) -> Vec<Deadline> {
        std::mem::take(&mut self.expired)
    }

    fn expire(&mut self, day: u32, tick: &mut DayTick) {
        let mut pending = Vec::with_capacity(self.deadlines.len());
        for deadline in self.deadlines.drain(..) {
            if deadline.day <= day {
                tick.expired.push(deadline.id);
                self.expired.push(deadline);
            } else {
                pending.push(deadline);
            }
        }
        self.deadlines = pending;
    }

    /// Read the contract out of a BBS row (F453's StrVars). `None` for the
    /// advertisements that are not offers (goods, crew, charity, military
    /// recruitment).
    pub fn mission_offer(
        &mut self,
        cache: &VerifiedCache,
        index: usize,
        row: usize,
    ) -> Result<Option<missions::Offer>, String> {
        let post = {
            let offers = self.station_offers(cache, index)?;
            offers.posts.get(row).cloned()
        };
        let Some(post) = post else {
            return Ok(None);
        };
        let code = self.entries[self.current].params.code;
        let blob = cache
            .asset("base:world/system-tables")
            .ok_or("this cache has no system tables (cache v15)")?;
        let tables = Tables::new(blob)?;
        let details = tables.details(code);
        let market = tables.market(code);
        let name = self.system_name().to_owned();
        let station_name = self
            .bodies
            .get(index)
            .map(|body| body.name.clone())
            .unwrap_or_default();
        let destinations = self.offers_destinations(cache);
        let names_bytes = cache.asset("base:text/names");
        let names = names_bytes.and_then(|bytes| Names::new(bytes).ok());
        let world = bulletin::World {
            code,
            name: &name,
            population: market.population,
            danger: market.danger,
            allegiance: (details.allegiance >> 6) as u16,
            station_name: &station_name,
            destinations: &destinations,
            names: names.as_ref(),
        };
        Ok(missions::offer(&post, &world))
    }

    /// F493/F494/F623/F629-F632's military package list for a station. The
    /// source stores `DATA_009004`; the port regenerates it deterministically
    /// from the station, system and game day, so the same day shows the same
    /// offers and a later day refreshes them.
    pub fn military_offers(
        &self,
        cache: &VerifiedCache,
        index: usize,
        rank_points: u32,
    ) -> Result<Vec<missions::Offer>, String> {
        let code = self.entries[self.current].params.code;
        let blob = cache
            .asset("base:world/system-tables")
            .ok_or("this cache has no system tables (cache v15)")?;
        let tables = Tables::new(blob)?;
        let details = tables.details(code);
        let market = tables.market(code);
        let name = self.system_name().to_owned();
        let station_name = self
            .bodies
            .get(index)
            .map(|body| body.name.clone())
            .unwrap_or_default();
        let destinations = self.offers_destinations(cache);
        let names_bytes = cache.asset("base:text/names");
        let names = names_bytes.and_then(|bytes| Names::new(bytes).ok());
        let world = bulletin::World {
            code,
            name: &name,
            population: market.population,
            danger: market.danger,
            allegiance: (details.allegiance >> 6) as u16,
            station_name: &station_name,
            destinations: &destinations,
            names: names.as_ref(),
        };
        let day = self.day();
        let mut rng = npc::Random::new(
            code ^ 0x4d49_4c49,
            (index as u32 + 1)
                .wrapping_mul(0x9e37_79b9)
                .wrapping_add(day),
        );
        Ok(missions::military_offers(
            &mut rng,
            &world,
            day,
            rank_points,
        ))
    }

    /// F535: remove the accepted advertisement from the board (the original
    /// clears the 24-byte slot, leaving an empty entry; the port removes it).
    pub fn clear_post(
        &mut self,
        cache: &VerifiedCache,
        index: usize,
        row: usize,
    ) -> Result<(), String> {
        let offers = self.station_offers(cache, index)?;
        if row < offers.posts.len() {
            offers.posts.remove(row);
        }
        Ok(())
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
        // F655: the player's outstanding fine at this starport (0x2710 refuses).
        let owes = self.fine_at(index);
        let reply = self.starport(index).request_docking(PLAYER, true, owes);
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
        self.clearance_at = None;
        self.skips = 0;
        self.sequence = Some(Sequence {
            station: index,
            stages: Stages::touched(false),
            launching: false,
        });
        true
    }

    /// Whether the launch action (action 9) must wait for F381's permission: the ship is
    /// on an open-air starport's pad (state 0x30) and has not been cleared.
    pub fn launch_needs_permission(&self) -> bool {
        let on_starport = self
            .landed_body()
            .or(self.docked)
            .and_then(|index| self.bodies.get(index))
            .is_some_and(|b| b.kind == Kind::Starport);
        on_starport && (self.on_pad() || self.landed.is_some()) && !self.launch_cleared
    }

    /// F381: ask the open-air starport's traffic control (F654) for launch permission.
    /// The ship stays on its pad; the launch action (engines on) starts the climb.
    pub fn request_launch(&mut self) -> Result<String, String> {
        let index = self
            .landed_body()
            .or(self.docked)
            .or_else(|| first_station(&self.bodies))
            .ok_or("No station to launch from.")?;
        if self.takeoff.is_some() {
            return Err("Already launching.".to_owned());
        }
        if !self.starport(index).request(PLAYER) {
            return Err("Launch refused: traffic control is busy.".to_owned());
        }
        self.docked = Some(index);
        self.launch_cleared = true;
        Ok("Launch permission granted. Click the engines to turn them on.".to_owned())
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
            self.launch_cleared = false;
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
        self.clearance_at = None;
        self.skips = 0;
        self.landed = Some(Landed {
            landing: Landing::on_surface(),
            index,
        });
        true
    }

    /// A soft touch inside starport `index`'s region: F1019 with the autopilot
    /// engaged, F1024 with clearance (F657: the port's `+0xca` is the player),
    /// else F1021, whose touch is no landing but F1013's `0x24` climb again.
    /// `pad` is F1033's pad byte (one more than the pad, zero off the pads).
    pub fn region_touch(&mut self, index: usize, pad: u8) -> RegionTouch {
        if self.target.is_some() {
            return RegionTouch::Land;
        }
        let name = self.bodies.get(index).map_or("", |b| b.name.as_str());
        // F1022 posts its message only while DATA_008873 is clear, then sets
        // it; the port keeps that per port until the next landing or launch.
        let warning = (self.warned_at != Some(index)).then(|| {
            // Message 0x98be; the comms strings are not in the cache, so this
            // is the port's wording.
            format!("{name}: you have no clearance to land here.")
        });
        if self.cleared(index) {
            let bay = self
                .traffic
                .get(&index)
                .and_then(|port| port.bay_of(PLAYER))
                .map(i32::from);
            let touched = i32::from(pad) - 1;
            if (0..7).contains(&touched) && Some(touched) != bay {
                if warning.is_some() {
                    self.warned_at = Some(index);
                }
                return RegionTouch::WrongPad(warning);
            }
            return RegionTouch::Land;
        }
        if self.takeoff.is_none() {
            self.takeoff = Some(Takeoff::taking_off());
            self.launch_cleared = false;
        }
        if warning.is_some() {
            self.warned_at = Some(index);
        }
        RegionTouch::Climb(warning)
    }

    /// F1019's open-air starport touch (`0x2e`): the port's pad. F1048's view
    /// function then turns it into `0x30` and resets the traffic record. A station
    /// hull or a planet is not this state.
    pub fn land_at_starport(&mut self, index: usize) -> bool {
        if self.landed.is_some() || self.takeoff.is_some() || self.sequence.is_some() {
            return false;
        }
        let Some(body) = self.bodies.get(index) else {
            return false;
        };
        if body.kind != Kind::Starport {
            return false;
        }
        // F1048 calls F653: the traffic record is reset (the bay stays allocated).
        self.starport(index).reset();
        self.warned_at = None;
        self.docked = Some(index);
        self.target = None;
        self.final_leg = false;
        self.clearance_at = None;
        self.skips = 0;
        self.landed = Some(Landed {
            landing: Landing::at_starport(),
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
            && self
                .docked
                .and_then(|index| self.bodies.get(index))
                .is_some_and(|body| body.kind == Kind::Starport)
            && self.takeoff.is_none()
            && self.sequence.is_none()
            && self.landed.is_none()
    }

    /// F343 services are available at the new-game pad and after a port landing.
    pub fn at_starport_pad(&self) -> bool {
        self.on_pad()
            || self.landed.is_some_and(|landed| {
                self.bodies
                    .get(landed.index)
                    .is_some_and(|body| body.kind == Kind::Starport)
            })
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
        let starport = self
            .bodies
            .get(landed.index)
            .is_some_and(|body| body.kind == Kind::Starport);
        if starport {
            // F381: an open-air starport's launch asks F654 first. The bay is
            // the one the landing's F650 allocated, so mark the port as the
            // docked one for F1032's release.
            if !self.starport(landed.index).request(PLAYER) {
                return Err("Launch refused: traffic control is busy.".to_owned());
            }
            self.docked = Some(landed.index);
        }
        let takeoff = landed
            .landing
            .begin_launch()
            .ok_or_else(|| "Not landed on a surface.".to_owned())?;
        self.takeoff = Some(takeoff);
        self.launch_cleared = false;
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
    /// [`Self::respawn_traffic`] only. `F35(0x16)` is zero here: the mission
    /// scripts that set it are not ported, so normal systems have no Thargoids
    /// (see [`Self::spawn_thargoid_traffic`]).
    pub fn spawn_traffic(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
    ) -> bool {
        self.spawn_traffic_with(cache, player, forward, npc::SpawnOptions::default(), 0)
    }

    /// F690's Thargoid branch with an explicit `F35(0x16)` limit: the hook a
    /// ported mission script would use. The value is not a rate or an exact
    /// count: F688 draws `RND(limit)` before F690, and no system record sets it,
    /// so zero spawns nothing. Forces the F688 spawner to run even when this
    /// system already has traffic.
    pub fn spawn_thargoid_traffic(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
        count: usize,
    ) -> bool {
        self.spawned = None;
        self.spawn_traffic_with(cache, player, forward, npc::SpawnOptions::default(), count)
    }

    fn spawn_traffic_with(
        &mut self,
        cache: &VerifiedCache,
        player: [i64; 3],
        forward: [f64; 3],
        options: npc::SpawnOptions,
        thargoids: usize,
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
        let system = npc::System::new(details, thargoids);
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
            &system,
            allegiance,
            &mut rng,
            options,
            |model| crate::ships::npc_spec(cache, model),
        ) {
            ship.id += base;
            ships.push(ship);
        }
        ships.truncate(npc::MAX_SHIPS);
        // The ships being replaced may still hold bays in a traffic record. The
        // original's F651/F927 release the slot with the object; the port must
        // do the same when a spawn (a jump, or a scripted respawn) replaces the
        // list, or a station fills with bays no visible ship owns.
        let held: Vec<(usize, u8)> = self
            .ships
            .iter()
            .filter(|ship| ship.bay.is_some())
            .filter_map(|ship| Some((ship.station?, npc::traffic_id(ship.id))))
            .collect();
        for (station, id) in held {
            self.starport(station).release(id);
        }
        self.occupy_station_bays(&mut ships);
        self.ships = ships;
        self.player_at = player;
        self.ship_lock = None;
        self.effects.clear();
        self.missile_lock = None;
        self.missile_view = None;
        self.missiles.clear();
        self.decoys.clear();
        self.spawned = Some(self.current);
        !self.ships.is_empty()
    }

    /// F691/F650: give every initial station-bay ship its bay and start F747's
    /// object time. The ships come from `npc::spawn` in `State::Docked` with a
    /// destination but no bay; the traffic record owns F650, so the bay is
    /// allocated here exactly as an arriving trader's request would be. A full
    /// station drops that ship (the source's F940 after F650 fails). The bay
    /// stays occupied while the handling record is cleared (F653).
    fn occupy_station_bays(&mut self, ships: &mut Vec<NpcShip>) {
        let mut keep = Vec::with_capacity(ships.len());
        for mut ship in ships.drain(..) {
            if ship.state != npc::State::Docked || ship.bay.is_some() {
                keep.push(ship);
                continue;
            }
            let Some(index) = ship.station else {
                keep.push(ship);
                continue;
            };
            if !self
                .bodies
                .get(index)
                .is_some_and(|body| body.kind.is_place())
            {
                continue;
            }
            let id = npc::traffic_id(ship.id);
            let Reply::Granted(bay) = self.starport(index).request_docking(id, false, 0) else {
                continue;
            };
            let pad = self.pad_point(index, bay.index);
            ship.bay = Some(bay.index);
            ship.pad = pad;
            {
                // F652/F653: the ship is in the bay and the handling record is
                // reset; F650's allocation remains in `pads`.
                let port = self.starport(index);
                port.touch(id);
                port.reset();
            }
            let seat = self
                .station_snapshot(&ship)
                .map(|station| station.bay_point())
                .unwrap_or_else(|| self.bodies[index].position);
            ship.begin_docked_for(seat, ship.dock_span);
            keep.push(ship);
        }
        *ships = keep;
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
        self.spawn_traffic_with(cache, player, forward, npc::SpawnOptions::CAPTURE, 0)
    }

    /// Advance F939/F942/F926's effects without running the traffic AI. The
    /// death sequence stops the player's ship but still shows its explosion
    /// (F1104's external view), so `main` calls this while `Flight::death` is
    /// set.
    pub fn tick_effects(&mut self, seconds: f64) {
        for effect in &mut self.effects {
            effect.tick(seconds);
        }
        self.effects.retain(Effect::alive);
    }

    /// One flight frame of F1035's spawn and F769's lifetime. `elapsed` is the
    /// presented (unaccelerated) frame time: F1035 attempts at the original's
    /// 20 ms timer cadence, and F769's day ticks are real seconds because the
    /// time acceleration scales both the day rate and F769's limit. The caller
    /// supplies the player's position/velocity, F1033's altitude and the
    /// "Space dust, clouds etc" option.
    pub fn tick_dust(&mut self, elapsed: f64, spawn: &dust::Spawn) {
        self.dust.advance(elapsed, spawn);
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
        self.tick_effects(seconds);
        if self.ships.is_empty() {
            return events;
        }
        // F695's escorts hold a station in their leader's frame: right*+0x102 +
        // up*+0x104 + forward*+0x106 (F743's F610 target). F751 then hands the
        // escort to the attack when the leader is in state 1/2 (the port's
        // Attack/Pass): it drops the formation offsets and targets the leader's
        // target, which is the player for a pirate group.
        let leaders: Vec<LeaderFrame> = self
            .ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| (ship.id, ship.position, ship.axes(), ship.state))
            .collect();
        for ship in &mut self.ships {
            if let Some(leader) = ship.leader {
                if leaders.iter().any(|(id, _, _, state)| {
                    *id == leader && matches!(state, npc::State::Attack | npc::State::Pass)
                }) {
                    ship.join_attack();
                }
            }
        }
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
                let (_, position, axes, _) =
                    leaders.iter().find(|(leader, _, _, _)| *leader == id)?;
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
            if let Some(launch) = ship.take_launch() {
                events.push(ShipEvent::MissileLaunch {
                    owner: launch.owner,
                    name: ship.name.clone(),
                    code: launch.code,
                    pylon: launch.pylon,
                    target: launch.target,
                    position: launch.position,
                    axes: launch.axes,
                    launcher_radius: launch.launcher_radius,
                });
            }
            if let Some(naval) = ship.take_ecm() {
                events.push(ShipEvent::Ecm {
                    name: ship.name.clone(),
                    naval,
                });
            }
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
        // F783: a removed police object (`+0x118 == 0xe`) clears
        // DATA_008647 and DATA_008646 bit 0.
        let police_removed = self
            .ships
            .iter()
            .any(|ship| ship.role == ffe_core::npc::Role::Police && ship.finished(player));
        self.ships.retain(|ship| !ship.finished(player));
        if police_removed && self.legal.clear_alert() {
            events.push(ShipEvent::AlertCleared);
        }
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

    /// F234/F235 as `IsCloseToStarport`: the ordinal of the nearest starport
    /// whose `+0x88` exponential distance is below [`JURISDICTION_DISTANCE`]
    /// (`0x13`), or `None` outside a starport's region. The ordinal is only the
    /// witness position; F236's ledger index is the system allegiance
    /// (`DATA_008996`), see [`Self::jurisdiction`].
    pub fn starport_in_range(&self, at: [i64; 3]) -> Option<usize> {
        starport_witness(&self.bodies, at)
    }

    /// The ordinal of the starport nearest `body` among the system's starports
    /// (`DATA_008996` is F236's ledger index; a station uses its nearest
    /// starport).
    pub fn starport_ordinal(&self, body: usize) -> usize {
        let Some(target) = self.bodies.get(body) else {
            return 0;
        };
        let mut ordinal = 0usize;
        let mut nearest: Option<(f64, usize)> = None;
        for candidate in &self.bodies {
            if candidate.kind == Kind::Starport {
                let away = distance(target.position, candidate.position);
                if nearest.is_none_or(|(best, _)| away < best) {
                    nearest = Some((away, ordinal));
                }
                ordinal += 1;
            }
        }
        nearest.map_or(0, |(_, index)| index)
    }

    /// The fine ledger for the current jurisdiction: F655's `owes` argument,
    /// `DATA_009095[DATA_008996]`. `body` is unused beyond the station check.
    pub fn fine_at(&self, _body: usize) -> u32 {
        self.legal.owed(Some(self.jurisdiction))
    }

    /// F1056's action 9 from the pad without F654's clearance (`player+0xf0`
    /// zero): the launch goes ahead, and F237 reports "Launching without
    /// clearance" here and, when this jurisdiction's ledger (`+0x50f8`) is then
    /// over 0x1388, "Evading payment of fine". Returns the messages to log.
    pub fn unauthorised_launch(&mut self) -> Vec<String> {
        let here = Some(self.jurisdiction);
        let mut messages = Vec::new();
        if let Some(report) = self.legal.record(npc::LAUNCHING_WITHOUT_CLEARANCE, here) {
            messages.push(report.message);
        }
        if self.legal.owed(here) > 0x1388 {
            if let Some(report) = self.legal.record(npc::EVADING_FINE, here) {
                messages.push(report.message);
            }
        }
        messages.retain(|m| !m.is_empty());
        messages
    }

    /// The fine the local police would collect, `DATA_009095[DATA_008996]`.
    pub fn fine_here(&self) -> u32 {
        self.legal.owed(Some(self.jurisdiction))
    }

    /// F544/F545: pay the local fine. The caller owns the cash word and deducts
    /// [`npc::FinePayment::paid`].
    pub fn pay_fine(&mut self, cash: u32) -> npc::FinePayment {
        self.legal.pay_fine(cash, self.jurisdiction)
    }

    /// F312's F2 profile values for this navigator's commander, system and local
    /// jurisdiction. `commander` is the save's commander name (the original's
    /// `DATA_008920`).
    pub fn profile(&self, commander: &str) -> ffe_core::status::Profile {
        ffe_core::status::Profile::new(
            commander,
            self.system_name(),
            self.jurisdiction,
            &self.career,
            &self.legal,
        )
    }

    /// F931/F941's player branch: the first shot in a system reports the
    /// unlawful-discharge offence through F237(ship, 3). F236 records it only
    /// with a nearby starport witness (F234); otherwise the shot spends the
    /// system's one report without a fine or the wanted bit. A recorded offence
    /// makes the police and military ships hostile (the port's `DATA_008647`
    /// response; F783 clears it when the chase ends).
    pub fn player_fired(&mut self, at: [i64; 3]) -> Option<LegalReport> {
        // F234 finds the witness; F237 then records F236 against F869's
        // allegiance (`DATA_008996`), not the starport ordinal.
        let witness = self.starport_in_range(at);
        let offence = self
            .legal
            .player_fired(witness.map(|_| self.jurisdiction))?;
        let mut police = false;
        if self.legal.wanted {
            for ship in &mut self.ships {
                police |= ship.provoke();
            }
        }
        if police {
            self.legal.alert = true;
        }
        Some(LegalReport { offence, police })
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
        let rating = ship.rating;
        let station = ship.station;
        let held_bay = ship.bay.is_some();
        let result = ship.hit(damage);
        let destroyed = was_alive && !ship.alive();
        if crime {
            self.legal.wanted = true;
            self.legal.offences += 1;
        }
        // F942 adds the destroyed ship's statistics +0x16 to DATA_008911 and
        // prints the Elite Federation notice when the rating word changes.
        let rating_changed = destroyed && self.career.award_kill(rating);
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
            rating,
            rating_changed,
            police,
            destroyed,
        })
    }

    /// F941: put the player's missile in flight at the combat target. The
    /// caller has already taken the pylon word from `Combat`. F941 also puts
    /// the target ship into state 0x0c (`F757`'s missile evasion).
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
        // `owner == 0` is the player in the callers. Store the reserved player
        // target id instead so F569's owner test cannot contact the player's own
        // missile with the player (the two ids would otherwise differ).
        let stored_owner = if owner == 0 {
            ffe_core::missile::PLAYER_TARGET
        } else {
            owner
        };
        self.missiles.launch(
            spec,
            code,
            stored_owner,
            Some(target),
            position,
            axes,
            pylon,
            launcher_radius,
        );
        let id = self.missiles.missiles.last().map(|missile| missile.id);
        // F941 stores the new object's id at `DATA_008877` when the launcher is
        // the player (`+0x100 == DATA_008857`). The callers pass 0 for the
        // player; the field stores `PLAYER_TARGET` so F569's owner test matches.
        if owner == 0 {
            self.missile_view = id;
        }
        // F941's `+0xff = 0x0c` branch: a live AI ship the missile is aimed at
        // goes evasive and keeps the missile id in `+0xfe`.
        if let Some(id) = id {
            if let Some(ship) = self.ships.iter_mut().find(|ship| ship.id == target) {
                ship.evade_missile(id);
            }
        }
    }

    /// F941 for an NPC launcher: no `DATA_008877` and no evasion side effect on
    /// the player (there is no player AI state).
    #[allow(clippy::too_many_arguments)]
    pub fn launch_npc_missile(
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
    /// The combat target is cleared when its ship is gone, and F1102 forces the
    /// missile view's object to zero when that missile dies. This legacy entry
    /// point has no player target; `main` uses [`Self::tick_missiles_at`].
    pub fn tick_missiles(
        &mut self,
        seconds: f64,
        _player: [i64; 3],
    ) -> Vec<crate::missile::MissileEvent> {
        self.tick_missiles_inner(seconds, None)
    }

    /// [`Self::tick_missiles`] with the player's interaction target, so an NPC's
    /// missile can reach the player through F718/F721.
    pub fn tick_missiles_at(
        &mut self,
        seconds: f64,
        player: [i64; 3],
        velocity: [f64; 3],
        radius: i64,
    ) -> Vec<crate::missile::MissileEvent> {
        let target = ffe_core::missile::Target {
            id: ffe_core::missile::PLAYER_TARGET,
            position: player,
            velocity,
            radius,
        };
        self.tick_missiles_inner(seconds, Some(target))
    }

    fn tick_missiles_inner(
        &mut self,
        seconds: f64,
        player: Option<ffe_core::missile::Target>,
    ) -> Vec<crate::missile::MissileEvent> {
        if self
            .missile_lock
            .is_some_and(|id| !self.ships.iter().any(|ship| ship.id == id && ship.alive()))
        {
            self.missile_lock = None;
        }
        let events = self
            .missiles
            .tick_with(seconds, &mut self.ships, player, &self.decoys);
        // F780 notifies the ship an evading missile belonged to: it drops the
        // 0x0c state and returns to the attack.
        for event in &events {
            let id = event.id();
            for ship in &mut self.ships {
                ship.missile_gone(id);
            }
        }
        if self
            .missile_view
            .is_some_and(|id| self.missiles.get(id).is_none())
        {
            self.missile_view = None;
        }
        events
    }

    /// F1054's chaff object: spawn one model-0xa4 decoy at `position` and return
    /// its [`ffe_core::missile::DECOY_BASE`] id. The source keeps the object in
    /// the object list (F927 recycles slots); the port caps the list so a long
    /// fight cannot grow it without bound.
    pub fn deploy_chaff(&mut self, position: [i64; 3], radius: i64) -> u32 {
        const MAX_DECOYS: usize = 16;
        let id = ffe_core::missile::DECOY_BASE + self.next_decoy;
        self.next_decoy = self.next_decoy.wrapping_add(1);
        if self.decoys.len() >= MAX_DECOYS {
            self.decoys.remove(0);
        }
        self.decoys.push(crate::missile::Decoy {
            id,
            position,
            radius,
        });
        id
    }

    /// The missile the F10 view rides (`DATA_008877`), when it is still alive.
    pub fn viewed_missile(&self) -> Option<&ffe_core::missile::Missile> {
        self.missiles.get(self.missile_view?)
    }

    /// F718/F719: an ECM burst (F935 already checked the equipment). Every
    /// live missile the model table defeats is destroyed and gets F719's
    /// model-0x9b explosion; the viewed missile clears `DATA_008877`.
    pub fn ecm(&mut self, naval: bool) -> Vec<crate::missile::EcmLoss> {
        let losses = self.missiles.ecm(naval);
        if losses.is_empty() {
            return losses;
        }
        let seed = (self.clock * 1000.0) as u32
            ^ (self.effects.len() as u32).wrapping_mul(0x9e37_79b9)
            ^ if naval { 0x8000_0000 } else { 0x4000_0000 };
        let mut rng = npc::Random::new(seed, seed.rotate_left(13) ^ 0x4567_89ab);
        for loss in &losses {
            self.effects.push(ffe_core::effects::missile_explosion(
                loss.position,
                loss.axes,
                &mut rng,
            ));
        }
        if self
            .missile_view
            .is_some_and(|id| losses.iter().any(|loss| loss.id == id))
        {
            self.missile_view = None;
        }
        // F780/F781: the missiles' deaths notify the ships that were evading
        // them, just as a normal hit or timeout does.
        for loss in &losses {
            for ship in &mut self.ships {
                ship.missile_gone(loss.id);
            }
        }
        losses
    }

    /// F1057's chaff retarget: pull missiles aimed at `from` (the player,
    /// [`ffe_core::missile::PLAYER_TARGET`]) to the chaff object `to`. `allow`
    /// is the per-missile `F1530() & 0x1000` roll. `main`'s `c` handler supplies
    /// both after `deploy_chaff`.
    pub fn retarget_missiles(
        &mut self,
        from: u32,
        to: u32,
        allow: impl FnMut(&ffe_core::missile::Missile) -> bool,
    ) -> usize {
        self.missiles.retarget(from, to, allow)
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
        self.clearance_at = None;
        self.skips = 0;
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

    /// The current system's 32-bit code (the galaxy record's key; F885's
    /// `DATA_008885`). It is what a contract's destination field stores.
    pub fn current_system_code(&self) -> u32 {
        self.entries[self.current].params.code
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

    /// Move the planets: the game clock advances `seconds` of world time, and
    /// any whole day it crosses runs its periodic updates once (F152).
    pub fn advance(&mut self, cache: &VerifiedCache, seconds: f64) {
        self.clock += seconds;
        // Placing a system is cheap but not free: once a second of world time, and
        // on every real second the autopilot flies.
        if self.clock - self.refreshed >= 1.0 || self.clock < self.refreshed {
            self.refresh(cache);
        }
        self.tick_days(cache);
    }

    /// A transfer: the clock jumps and the ship is placed to match, so the target's
    /// drift over it is not carried along.
    pub fn transfer(&mut self, cache: &VerifiedCache, seconds: f64) {
        self.clock += seconds;
        self.refresh(cache);
        self.tick_days(cache);
        self.drift = [0; 3];
        // A transfer is a jump in the system frame; it cannot arrive landed.
        self.landed = None;
        self.takeoff = None;
    }

    pub fn refresh(&mut self, cache: &VerifiedCache) {
        if let Ok(list) = bodies_since(
            cache,
            &self.entries[self.current],
            clock_words(self.world_epoch),
            clock_words(self.clock),
        ) {
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

    /// The system code of an entry (the galaxy list's row as a destination).
    pub fn system_code(&self, index: usize) -> Option<u32> {
        Some(self.entries.get(index)?.params.code)
    }

    /// The galaxy map has chosen a star: store it as F832 stores the destination in
    /// `+0x25c..0x264`/`+0x27c`. This does not lock a body of the current system.
    pub fn set_destination(&mut self, code: u32, position: [i32; 3]) {
        self.destination = Some(Destination { code, position });
    }

    /// The map found no star near its centre (F840 clears `+0x268`): no jump target.
    pub fn clear_destination(&mut self) {
        self.destination = None;
    }

    /// The system the destination code names, when the playable has its bodies.
    pub fn destination_entry(&self) -> Option<usize> {
        self.entry_for_code(self.destination?.code)
    }

    /// The distance to the stored destination in the drive's units (F832's `+0x26c`),
    /// measured from the current system so it is right after a jump.
    pub fn destination_distance(&self) -> Option<i32> {
        let destination = self.destination?;
        Some(star_distance(
            self.system_position(self.current)?,
            destination.position,
        ))
    }

    /// F899 for the stored destination: the map's fuel line.
    pub fn destination_report(&self) -> Option<Report> {
        self.report_for(self.destination_distance()?)
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

    /// F903's "jump possible" (`DATA_008860` bit 0x100, which F1116 reads as
    /// `player+0xc7 & 1` for the jump icon): the stored destination is another
    /// system on offer, within the drive's range, with the fuel for it.
    pub fn jump_possible(&self) -> bool {
        let Some(index) = self
            .destination
            .and_then(|destination| self.entry_for_code(destination.code))
        else {
            return false;
        };
        index != self.current && self.jump_report(index).is_some_and(|(_, r)| r.result >= 0)
    }

    /// Jump to system `index` through the hyperdrive (F901): the fuel is checked and the
    /// game clock moves by the jump time; the caller burns `Jumped::fuel`. Refused with a
    /// reason when the system is out of range, the same system, or the hold has too little fuel.
    pub fn hyperspace(&mut self, cache: &VerifiedCache, index: usize) -> Result<Jumped, String> {
        if self.transit.is_some() {
            return Err("Already in hyperspace.".to_owned());
        }
        // F903 only jumps from mode 0 or 0x24; a landed ship must launch first.
        if self.landed.is_some()
            || self
                .takeoff
                .is_some_and(|takeoff| takeoff.state != state::TAKEOFF)
        {
            return Err("Launch before a hyperspace jump.".to_owned());
        }
        // F908: `DATA_008867` must be at least 0xb71b00; a ship in a planet's
        // atmosphere cannot jump.
        if self.altitude < HYPERSPACE_ALTITUDE {
            return Err("Too low for a hyperspace jump.".to_owned());
        }
        let Some((distance, report)) = self.jump_report(index) else {
            return Err("No hyperdrive data for this ship or system.".to_owned());
        };
        if index == self.current {
            return Err("You are already in this system.".to_owned());
        }
        if report.result < 0 {
            // The galaxy map's own strings (DATA_007005/7006): a fuel shortfall
            // and a distance beyond the drive are different messages.
            return Err(if report.fuel_needed < 0 {
                "Out of Range".to_owned()
            } else {
                format!("Insufficient Fuel ({} required)", report.fuel_needed)
            });
        }
        let days = f64::from(report.jump_time.unwrap_or(0)) / 65_536.0;
        // F909 builds the destination at once (F860/F905) and F912 puts the
        // ship on a random orbit about its root; F908 then leaves the clock to
        // run through the jump time in mode 4 (see `transit_step`).
        self.jump(cache, index)?;
        let arrival = self.hyperspace_arrival(cache)?;
        self.player_at = arrival;
        self.transit = Some(Transit {
            remaining: days * 86_400.0,
            total: days * 86_400.0,
        });
        Ok(Jumped {
            arrival,
            distance,
            fuel: report.fuel_needed,
            days,
        })
    }

    /// F909's arrival point: F922 draws the orbit's ffp mantissa
    /// (`random(0x2000) | 0x4000`, exponent 0x33: about 7.5 to 11 AU), F912
    /// advances F1530 and F1507 places the ship on that orbit about the
    /// system's root with `DATA_008818` as the longitude and `DATA_008820` as
    /// the phase. The original's random state is not reproduced; this one is
    /// seeded from the system and the clock.
    fn hyperspace_arrival(&self, cache: &VerifiedCache) -> Result<[i64; 3], String> {
        let sine = SineTable::from_bytes(cache.asset("base:math/sine").ok_or("no sine table")?)?;
        let code = self.entries[self.current].params.code;
        let clock = clock_words(self.clock);
        let mut rng = npc::Random::new(code ^ clock[0], clock[1].rotate_left(7) ^ 0x2f6b_3c91);
        let mantissa = (rng.bounded(0x2000) | 0x4000) as i16;
        rng.advance();
        let radius = ffe_core::ffp::Ffp {
            mantissa,
            exponent: 0x33,
        };
        Ok(ffe_core::objects::orbit_point(
            &sine,
            radius,
            rng.a as i32,
            rng.b,
        ))
    }

    /// One frame of F908's transit: the clock runs at [`HYPERSPACE_RATE`] for
    /// `seconds` of real time, never past the arrival time (the main loop's
    /// rollback sets the clock to it exactly). `true` when the jump time has
    /// run out and F1094 should end the transit.
    pub fn transit_step(&mut self, cache: &VerifiedCache, seconds: f64) -> bool {
        let Some(mut transit) = self.transit else {
            return false;
        };
        let step = (seconds * HYPERSPACE_RATE).min(transit.remaining).max(0.0);
        self.advance(cache, step);
        transit.remaining -= step;
        self.transit = Some(transit);
        transit.remaining <= 0.0
    }

    /// Run a transit to its arrival time at once (tests and scripted jumps); the
    /// playable steps it frame by frame with [`Self::transit_step`].
    pub fn finish_transit(&mut self, cache: &VerifiedCache) {
        while self.transit.is_some() && !self.transit_step(cache, 1.0) {}
        self.transit = None;
    }

    /// F908/F901 for the galaxy map's stored destination. `Err` carries the map's
    /// own message when the star is not one of the systems on offer.
    pub fn hyperspace_to_destination(&mut self, cache: &VerifiedCache) -> Result<Jumped, String> {
        let destination = self
            .destination
            .ok_or_else(|| "No hyperspace destination: choose a system on the map.".to_owned())?;
        match self.entry_for_code(destination.code) {
            Some(index) => self.hyperspace(cache, index),
            None => Err(
                "Unexplored system: only the systems on offer can be entered (centre one of them)."
                    .to_owned(),
            ),
        }
    }

    /// Enter system `index` at once (no fuel, no time): the bodies are rebuilt and the arrival
    /// point (just off the first planet, or the star) returned.
    pub fn jump(&mut self, cache: &VerifiedCache, index: usize) -> Result<[i64; 3], String> {
        if index >= self.entries.len() {
            return Err("no such system".to_owned());
        }
        self.current = index;
        self.world_epoch = self.clock;
        self.bodies = bodies(cache, &self.entries[index], clock_words(self.clock))?;
        self.jurisdiction = jurisdiction_of(cache, self.entries[index].params.code);
        self.refreshed = self.clock;
        self.target = None;
        self.locked = None;
        self.ship_lock = None;
        // F931/F941's DATA_008860 bit 2 is the *system's* single report. The
        // DOS listing only zeroes the word in F1000's reset; the port resets it
        // on a system entry so the offence can happen once per system, as the
        // FFED3D once-per-system check intends.
        self.legal.illegal_fire = false;
        self.missile_lock = None;
        self.destination = None;
        self.altitude = i32::MAX;
        // F675: the pirates close to the player are carried through the jump.
        self.persisted = npc::persist(&self.ships, self.player_at);
        self.ships.clear();
        // F674/F909 clear the system's objects; dust is a system-local object.
        let (dust_a, dust_b) = dust_seed(self.entries[index].params.code);
        self.dust.reset(dust_a, dust_b);
        self.spawned = None;
        self.docked = None;
        self.traffic.clear();
        self.markets.clear();
        self.offers.clear();
        self.sequence = None;
        self.takeoff = None;
        self.landed = None;
        self.final_leg = false;
        self.frame = None;
        self.drift = [0; 3];
        self.memory = Memory::default();
        // F909 clears the system's objects: ships, missiles, hit particles,
        // explosions and debris belong to the system that was left.
        self.effects.clear();
        self.missiles.clear();
        self.decoys.clear();
        self.notice = None;
        self.carry = 0.0;
        self.skips = 0;
        let anchor = self
            .bodies
            .iter()
            .find(|b| b.kind == Kind::Planet)
            .or_else(|| self.bodies.first())
            .ok_or("empty system")?;
        let off = (standoff(anchor) * 4.0) as i64;
        let arrival = [
            anchor.position[0],
            anchor.position[1],
            anchor.position[2] - off,
        ];
        // F909's F606 places the ship in the new system before its traffic spawns.
        self.player_at = arrival;
        Ok(arrival)
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
        if self.landed_view() || self.sequence.is_some() {
            return None;
        }
        let index = self.selected_body()?;
        self.target = Some(index);
        self.autopilot_notice = None;
        self.locked = Some(index);
        self.final_leg = false;
        self.clearance_at = None;
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
        if self.landed_view() || self.sequence.is_some() {
            return None;
        }
        let index = self.locked?;
        self.bodies.get(index)?;
        self.target = Some(index);
        self.autopilot_notice = None;
        self.final_leg = false;
        self.clearance_at = None;
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
        // A ship inside a world's atmosphere moves with that world (or a port on
        // its surface), never with an engaged target in orbit: carrying it along
        // a station's orbit from just above the ground threw it into the terrain.
        let world = (0..self.bodies.len())
            .filter(|i| matches!(self.bodies[*i].kind, Kind::Planet | Kind::Moon))
            .filter(|i| distance(ship, self.bodies[*i].position) <= self.bodies[*i].radius as f64)
            .min_by(|a, b| {
                distance(ship, self.bodies[*a].position)
                    .total_cmp(&distance(ship, self.bodies[*b].position))
            });
        if let Some(world) = world {
            let port = (0..self.bodies.len())
                .filter(|i| {
                    let body = &self.bodies[*i];
                    body.kind == Kind::Starport
                        && body.parent == Some(world)
                        && distance(ship, body.position) <= STATION_FRAME
                })
                .min_by(|a, b| {
                    distance(ship, self.bodies[*a].position)
                        .total_cmp(&distance(ship, self.bodies[*b].position))
                });
            return Some(port.unwrap_or(world));
        }
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

    /// F1018's cancel: drop the autopilot's destination and every per-leg state.
    /// The caller also zeroes the set speed (F1018 sets `DATA_008918` from
    /// `DATA_008866`, which the flight model starts at 0), otherwise the ship
    /// keeps coasting at the speed the autopilot had set.
    pub fn cancel(&mut self) -> bool {
        let engaged = self.target.is_some() || self.final_leg;
        self.target = None;
        self.final_leg = false;
        self.clearance_at = None;
        self.memory = Memory::default();
        self.skips = 0;
        self.notice = None;
        engaged
    }

    /// Drop the autopilot's target (the pre-F1018 keep-braking form). Prefer
    /// [`Self::cancel`], which also clears the rest of the leg.
    pub fn disengage(&mut self) {
        self.cancel();
    }

    /// F1020 changes takeoff to autopilot; F1032 releases the bay in 0x26.
    pub fn autopilot_takeoff_handoff(&mut self) -> bool {
        let Some(takeoff) = self.takeoff.take() else {
            return false;
        };
        if takeoff.state == state::AFTER_TAKEOFF {
            if let Some(index) = self.docked {
                self.starport(index).release(PLAYER);
            }
        }
        self.docked = None;
        true
    }

    fn autopilot_notice(&mut self, text: String) {
        if self.autopilot_notice.as_ref() != Some(&text) {
            self.autopilot_notice = Some(text.clone());
            self.notice = Some(text);
        }
    }

    /// Player path: F741/F742 approach, F745 clearance, F746 pad guidance.
    /// The older `prototype_command` method below is the developer transfer controller;
    /// it is deliberately not called by the live player loop.
    pub fn source_command(
        &mut self,
        _cache: &VerifiedCache,
        flight: &mut OriginalFlight,
    ) -> Option<(ffe_core::autopilot::Guidance, Body)> {
        if self.landed_view() || self.sequence.is_some() {
            return None;
        }
        let index = self.target?;
        let body = self.bodies.get(index)?.clone();
        let station = matches!(body.kind, Kind::Station | Kind::Starport);
        if self.final_leg
            && self
                .traffic
                .get(&index)
                .and_then(|p| p.bay_of(PLAYER))
                .is_none()
        {
            self.final_leg = false;
        }
        let point = if self.final_leg {
            // F655/F650 gives the pad plane. F745 quantises the offset to
            // signed words; F671 adds the ship radius only after touchdown.
            // Using the launch seat here leaves the ship hovering one radius up.
            let bay = self.traffic.get(&index)?.bay_of(PLAYER)?;
            let offset = self
                .pads
                .as_ref()?
                .position(body.model, bay, body.rows, body.open_air)
                .ok()?;
            std::array::from_fn(|k| {
                body.position[k].wrapping_add(i64::from((offset[k] >> 10) as i16) << 10)
            })
        } else if body.kind == Kind::Starport {
            // F741 stores signed offset words, then F593 shifts them by ten.
            // Use the Q31 row, rather than a re-normalised floating point axis.
            std::array::from_fn(|k| {
                body.position[k]
                    + i64::from((ffe_core::model::multiply(body.rows[3 + k], 0x1770) as i16) as i32)
                        * 1024
            })
        } else if body.kind == Kind::Station {
            // F741's indoor-station offset is the literal z word 0xfa0.
            [
                body.position[0],
                body.position[1],
                body.position[2] + 0xfa0 * 1024,
            ]
        } else {
            body.position
        };
        let velocity = if !station && self.frame == body.parent && self.frame != Some(index) {
            body.velocity
        } else {
            [0; 3]
        };
        let modifier = if self.final_leg { -2 } else { 0 };
        let mut guidance = if station {
            flight.guidance(point, velocity, modifier)
        } else {
            flight.body_guidance(point, velocity, body.radius)
        };
        // F597's accelerated branch (F596 passes F612's relative-velocity shift
        // and F592's distance shift): with time accelerated by `DATA_009013`,
        // once the destination is within a few doublings of what one
        // accelerated frame covers, the ship is put at F741's offset point with
        // the target's velocity and no thrust. Without it a 10000x frame near a
        // station overshoots back and forth and never closes to clearance range.
        // Only targets with F741's offset words (stations and starports) take it.
        let shift = ACCELERATION_SHIFTS[flight.acceleration_step().min(4)];
        let mut arrived = false;
        if station && shift != 0 {
            let motion = flight.player().motion.velocity;
            let mut relative: [i32; 3] =
                std::array::from_fn(|k| velocity[k].wrapping_sub(motion[k]));
            let velocity_shift = ffe_core::autopilot::normalise_velocity(&mut relative);
            let gap = guidance.distance_shift - velocity_shift.max(2) - (shift + (shift >> 2));
            if gap < 6 {
                flight.teleport(point);
                guidance = flight.guidance(point, velocity, modifier);
                arrived = true;
            }
        }
        // Port addition (not in F741/F742/F610): a straight line to the
        // destination that runs through a planet, moon or star flies the ship
        // into it, which is every trip between two ports of one planet. While
        // the line is blocked, climb up and forward instead. F597's accelerated
        // arrival above goes first: the original puts the ship there directly.
        if !arrived {
            if let Some(waypoint) = self.detour(flight.absolute_position(), point, index) {
                let guidance = flight.guidance(waypoint, [0; 3], 0);
                return Some((guidance, body));
            }
        }
        // F746's accelerated near-pad completion, not the prototype's
        // long-distance transfer. F652 must accept the allocated bay first.
        if self.final_leg && guidance.distance_shift <= 0 && flight.acceleration_step() != 0 {
            if body.kind == Kind::Starport && self.starport(index).touch(PLAYER) {
                let info = ffe_core::objects::ModelInfo::read(
                    _cache.asset("base:model/records")?,
                    _cache.asset("base:ship/stats")?,
                    flight.player().motion.model,
                )?;
                let seat = self.takeoff_seat(index, info.radii().1 as i32)?;
                flight.settle_autopilot_pad(seat, body.rows);
                self.land_at_starport(index);
                return None;
            } else if body.kind == Kind::Station && self.begin_docking(index) {
                return None;
            }
        }
        if self.final_leg {
            let rows = body
                .orientation
                .map(|r| r.map(|v| (v * 2_147_483_647.0) as i32));
            guidance.turns = ffe_core::autopilot::match_orientation(
                flight.player().motion.orientation,
                rows,
                true,
            );
        } else if station && guidance.distance_shift <= 6 {
            // A refusal does not silently disengage F742; F745 retries.
            let fine = self.fine_at(index);
            match self.starport(index).request_docking(PLAYER, true, fine) {
                Reply::Granted(_) => {
                    self.final_leg = true;
                    self.autopilot_notice(format!("{}: clearance granted.", body.name));
                    // The original's next accelerated AI tick runs F745/F746 at
                    // once; one accelerated frame of physics in between would
                    // drop the ship from the approach point onto the port.
                    if arrived {
                        return self.source_command(_cache, flight);
                    }
                }
                other => {
                    let text = format!("{}: {}.", body.name, refusal(other));
                    self.autopilot_notice(text);
                }
            }
        }
        Some((guidance, body))
    }

    /// The waypoint while the line from `ship` to `point` runs through a star,
    /// planet or moon other than the target `index` (its closest interior point
    /// below its outer radius plus a tenth of it). Below that clearance shell
    /// the ship climbs at 45 degrees toward the target's side, to a point as
    /// far ahead as the shell is above it (a nearer point, recomputed each
    /// frame, held the ship to a crawl). On or above the shell it aims at the
    /// line's closest point pushed out to the shell: that lies between the ship
    /// and the target, round the world's curve, so the target stays ahead.
    /// `None` when the line is clear.
    fn detour(&self, ship: [i64; 3], point: [i64; 3], index: usize) -> Option<[i64; 3]> {
        let s = ship.map(|v| v as f64);
        let d: [f64; 3] = std::array::from_fn(|k| (point[k] - ship[k]) as f64);
        let length2 = dot3(d, d);
        if length2 < 1.0 {
            return None;
        }
        let (body, clear, offset) = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                *i != index && matches!(b.kind, Kind::Star | Kind::Planet | Kind::Moon)
            })
            .filter_map(|(_, b)| {
                let c = b.position.map(|v| v as f64);
                let to_centre: [f64; 3] = std::array::from_fn(|k| c[k] - s[k]);
                let t = dot3(to_centre, d) / length2;
                if t <= 0.0 || t >= 0.999 {
                    return None;
                }
                let closest: [f64; 3] = std::array::from_fn(|k| s[k] + d[k] * t - c[k]);
                // The outer radius bounds the terrain: `inner_radius` is only its
                // lowest point, and a world's mountains can stand thousands of
                // kilometres above it.
                let surface = b.radius as f64;
                // The lowest point of a dipping line is below both ends, so a
                // generous clearance only holds back lines that skim the world
                // between two high points (gravity sags those into the ground);
                // an approach down to a port bottoms out at the port itself.
                let clear = surface + (surface * DETOUR_CLEARANCE).max(DETOUR_MARGIN);
                (dot3(closest, closest).sqrt() < clear).then_some((b, clear, closest))
            })
            .min_by(|a, b| distance(ship, a.0.position).total_cmp(&distance(ship, b.0.position)))?;
        let away: [f64; 3] = std::array::from_fn(|k| s[k] - body.position[k] as f64);
        let height = dot3(away, away).sqrt();
        if height < clear {
            let up = unit3(away);
            let across = dot3(d, up);
            let mut along: [f64; 3] = std::array::from_fn(|k| d[k] - up[k] * across);
            if dot3(along, along) < 1.0 {
                // Straight through the centre: any direction across the surface.
                along = if up[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let across = dot3(along, up);
                along = std::array::from_fn(|k| along[k] - up[k] * across);
            }
            let forward = unit3(along);
            let step = (clear - height).max(DETOUR_MARGIN);
            return Some(std::array::from_fn(|k| {
                ship[k] + ((up[k] + forward[k]) * step) as i64
            }));
        }
        let mut out = offset;
        if dot3(out, out) < 1.0 {
            // Straight through the centre: any direction across the line.
            out = if d[0].abs() < d[1].abs().max(d[2].abs()) {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let along = dot3(out, d) / length2;
            out = std::array::from_fn(|k| out[k] - d[k] * along);
        }
        let out = unit3(out);
        Some(std::array::from_fn(|k| {
            body.position[k] + (out[k] * clear) as i64
        }))
    }

    /// The autopilot's command for this frame, when engaged.
    pub fn prototype_command(
        &mut self,
        cache: &VerifiedCache,
        flight: &OriginalFlight,
    ) -> Option<(Command, Body)> {
        let index = self.target?;
        let body = self.bodies.get(index)?.clone();
        // A station is flown to in two legs, as the original's autopilot docks: to
        // a point out on the docking axis (F741), then (with clearance, F745)
        // into the slot on the current axis (F746). An open-air starport stops at
        // F741's point above the port and hands F655's cleared pad to the descent.
        let port = self.port(index);
        let starport = body.kind == Kind::Starport;
        if self.final_leg && port.is_none() && !starport {
            // A final leg belongs to a station or starport; a target changed
            // without re-engaging must not leave the autopilot holding for a
            // slot on a planet.
            self.final_leg = false;
            self.clearance_at = None;
        }
        let ship = flight.player().motion.position;
        // a point out on the docking axis, then (with clearance) into the slot.
        // An open-air starport is F741/F745/F746's approach: above the port, then
        // F650's pad point with the clearance F655 gave there.
        let final_starport = starport && self.final_leg;
        // F867: a clearance that ran out releases the bay; go back to the
        // approach and ask again (F745/F655).
        if final_starport {
            let held = self.starport(index).bay_of(PLAYER);
            if held.is_none() || !self.starport(index).is_docking(PLAYER) {
                self.final_leg = false;
                self.memory = Memory::default();
                self.notice = Some(format!("{}: clearance lost; asking again.", body.name));
            }
        }
        let final_starport = starport && self.final_leg;
        let (offset, stop) = if starport {
            let up = unit([body.rows[3], body.rows[4], body.rows[5]].map(f64::from));
            (up.map(|v| v * STARPORT_APPROACH), APPROACH_STOP)
        } else {
            match port {
                Some(port) if self.final_leg => slot_leg(&port, ship),
                Some(_) => (self.long_leg_offset(&body), APPROACH_STOP),
                None => ([0.0; 3], standoff(&body)),
            }
        };
        let aim = |at: [i64; 3]| -> [i64; 3] {
            std::array::from_fn(|k| at[k] + offset[k].round() as i64)
        };
        // F745's target: the pad F650 allocated (`+0x102..+0x106`, the model's
        // pad table turned into the port's frame). No standoff: the contact scan
        // decides where the ship stops.
        let pad = final_starport
            .then(|| {
                let up = unit([body.rows[3], body.rows[4], body.rows[5]].map(f64::from));
                self.starport(index)
                    .bay_of(PLAYER)
                    .and_then(|bay| self.starport_pad_point(index, bay))
                    .map(|point| {
                        // F994/F612: the seat is the pad surface raised by the
                        // ship's collision radius along the port's up row.
                        let lift = flight.collision_radius() as f64;
                        std::array::from_fn(|k| point[k] + (up[k] * lift).round() as i64)
                    })
            })
            .flatten();
        if final_starport && pad.is_none() {
            self.final_leg = false;
            self.notice = Some(format!("{}: the pad could not be found.", body.name));
        }
        let final_starport = starport && self.final_leg;
        let ship = flight.player().motion.position;
        let up = unit([body.rows[3], body.rows[4], body.rows[5]].map(f64::from));
        let mut final_height = 0.0;
        let (target, stop, descend) = if final_starport {
            let seat = pad.unwrap_or_else(|| aim(body.position));
            let offset: [f64; 3] = std::array::from_fn(|k| (ship[k] - seat[k]) as f64);
            final_height = offset.iter().zip(up).map(|(v, u)| v * u).sum::<f64>();
            (seat, 0.0, true)
        } else {
            let at = match port {
                Some(port) if !self.final_leg => station_route(&port, ship, aim(body.position)),
                _ => aim(body.position),
            };
            (at, stop, false)
        };
        let mut command = prototype_autopilot(flight, target, stop, &mut self.memory);
        if starport && command.arrived && !self.final_leg {
            // F742/F745: at the approach the autopilot runs F655 and gets a pad.
            // The source holds here and asks again when traffic control refuses;
            // the request is throttled so the log is not spammed.
            let owed = self.fine_at(index);
            let wait = self.clearance_at.is_some_and(|at| self.clock - at < 2.0);
            if wait {
                command.arrived = false;
                command.keys.slower = true;
            } else {
                self.memory = Memory::default();
                self.clearance_at = Some(self.clock);
                match self.starport(index).request_docking(PLAYER, true, owed) {
                    Reply::Granted(bay) => {
                        // Hand the cleared pad to the descent (F745's state 0x23)
                        // and keep the target, so the leg can still be cancelled.
                        self.final_leg = true;
                        self.notice = Some(format!(
                            "{}: clearance granted (bay {}). Descending to the pad.",
                            body.name,
                            bay.index + 1
                        ));
                        command.arrived = false;
                    }
                    other => {
                        self.notice = Some(format!("{}: {}.", body.name, refusal(other)));
                        command.arrived = false;
                        command.keys.slower = true;
                    }
                }
            }
        }
        if final_starport {
            // The touch (F575/F652) ends the leg; `arrived` would drop the target
            // before the pad contact can happen.
            command.arrived = false;
            command.keys = Keys::default();
            command.thrust = Some(starport_descent(flight, target, [0.0; 3]));
            if descend && final_height <= 250_000.0 {
                command.face = None;
                command.align = Some(body.orientation);
            }
            // F746/F744/F652: when F596's distance to the pad reaches zero (the
            // ship's collision sphere at the pad surface), the touch is taken and
            // the port's record marks the pad. The port's controller flies to
            // the seat; a capture of the ship's radius plus a few metres stands
            // in for F596's exact zero (the source's own controller gets there).
            let speed = flight
                .player()
                .motion
                .velocity
                .iter()
                .fold(0i32, |sum, v| sum.wrapping_add(v.wrapping_abs()));
            if distance(ship, target) <= (flight.collision_radius() * 3).max(3_000) as f64
                && speed <= 0x258
            {
                self.starport(index).touch(PLAYER);
                if self.land_at_starport(index) {
                    self.landing_frame();
                    self.notice = Some(format!("{}: touched down on the pad.", body.name));
                }
            }
        }
        if port.is_some() && command.arrived && !self.final_leg {
            // The approach point: ask for clearance and fly into the slot.
            self.memory = Memory::default();
            let owes = self.fine_at(index);
            match self.starport(index).request_docking(PLAYER, true, owes) {
                Reply::Granted(_) => {
                    self.final_leg = true;
                    command.arrived = false;
                }
                other => {
                    self.notice = Some(format!("{}: {}.", body.name, refusal(other)));
                    command.arrived = false;
                    command.keys.slower = true;
                }
            }
        }
        if self.final_leg {
            // The contact (F652) or the starport descent ends the leg; dropping
            // the target here would leave the ship stopped off the pad. On the
            // starport, hold at F741's point for the descent's hand-off.
            command.arrived = false;
            if starport {
                command.keys.slower = true;
            }
        }
        if let Some((_, initial)) = command.skip {
            // The body moves while the ship travels. Solve the rendezvous rather
            // than iterating from the current position (which diverges for a
            // fast planet and made the autopilot transfer in circles).
            let stop_cover = stop + APPROACH;
            let ship = flight.player().motion.position;
            let entry = &self.entries[self.current];
            let solution = intercept_time(ship, stop_cover, initial, |t| {
                let list = bodies_since(
                    cache,
                    entry,
                    clock_words(self.world_epoch),
                    clock_words(self.clock + t),
                )
                .ok()?;
                let there = list.get(index)?;
                let offset = self.long_leg_offset(there);
                Some(std::array::from_fn(|k| {
                    there.position[k] + offset[k].round() as i64
                }))
            });
            self.skips += 1;
            match solution {
                Some((seconds, future)) if self.skips <= MAX_SKIPS => {
                    let range = distance(ship, future).max(1.0);
                    let landing: [i64; 3] = std::array::from_fn(|k| {
                        let unit = (future[k] - ship[k]) as f64 / range;
                        future[k] - (unit * stop_cover) as i64
                    });
                    command.skip = Some((landing, seconds));
                }
                // A target the transfers cannot catch, or too many in a row:
                // fly the rest of the leg rather than teleporting forever.
                _ => command.skip = None,
            }
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
                        range_label(ship, b.position)
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
    fn clock_words_match_the_source_low_fraction_high_day_order() {
        assert_eq!(clock_words(0.0), [0, 0]);
        assert_eq!(clock_words(43200.0), [0x80000000, 0]);
        assert_eq!(clock_words(86400.0), [0, 1]);
        assert_eq!(clock_words(NEW_GAME_CLOCK), [0x80000000, 0x121cf7]);
    }

    #[test]
    fn the_default_start_record_is_f998s_and_f829s() {
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
        assert_eq!(DEFAULT_SHIP, 0x1f);
        assert_eq!(DEFAULT_CASH, 10_000);
        // The reference game's DATA_008889 at Hope: 7 t, 1 t of it hydrogen.
        assert_eq!(DEFAULT_CARGO, 7);
        assert_eq!(DEFAULT_HOLD_LOSS, 0x10);
        assert_eq!(DEFAULT_HYDROGEN, 1);
        assert_eq!(DEFAULT_PLAYER_INDEX, 87);
    }

    #[test]
    fn range_label_uses_f1100s_km_au_band_and_fixed_point_formats() {
        // 1e6 mm is 1 km; F1100's value is 100 in the two-decimal km form.
        assert_eq!(range_label([0; 3], [1_000_000, 0, 0]), "1.00 km");
        // The band boundary: below 2^44 mm the km form, at it the AU form.
        assert!(range_label([0; 3], [(1i64 << 44) - 1, 0, 0]).ends_with("km"));
        assert!(range_label([0; 3], [1i64 << 44, 0, 0]).ends_with("AU"));
        // 1 AU (1.495978707e14 mm) is the original's 1.000 AU (three decimals).
        let au = range_label([0; 3], [149_597_870_700_000, 0, 0]);
        assert!(au.ends_with("AU"), "{au}");
        let value: f64 = au.trim_end_matches(" AU").parse().unwrap();
        assert!((value - 1.0).abs() < 0.001, "{au}");
    }

    #[test]
    fn starport_approach_is_f741s_1770_words_of_1024_mm() {
        assert_eq!(STARPORT_APPROACH, 6_144_000.0);
    }

    #[test]
    fn the_exponential_distance_byte_is_f1680s_log2() {
        // 8.4e6 mm is the pad's order of distance in the executed original
        // (dist_cam ~0xa, "maximum distance about 20km" at 0xa).
        assert_eq!(distance_byte(8.4e6), 10);
        // Byte 0x12 is inside the source's 0x13 threshold, 0x13 is not.
        assert_eq!(distance_byte(2f64.powi(0x12 + 0xd)), 0x12);
        assert_eq!(distance_byte(2f64.powi(0x13 + 0xd)), 0x13);
        assert!(distance_byte(2f64.powi(0x12 + 0xd)) < JURISDICTION_DISTANCE);
        assert!(!(distance_byte(2f64.powi(0x13 + 0xd)) < JURISDICTION_DISTANCE));
        assert_eq!(distance_byte(0.0), 0);
        // A system-scale distance is far outside the starport's region.
        assert!(distance_byte(1.0e12) > JURISDICTION_DISTANCE);
    }

    fn body(kind: Kind, position: [i64; 3]) -> Body {
        Body {
            name: format!("{kind:?}"),
            kind,
            position,
            velocity: [0; 3],
            radius: 1_000_000,
            inner_radius: 1_000_000,
            parent: None,
            model: 0,
            seed: 0,
            mass: 0x4000_0000,
            slot: 0,
            rows: [0; 9],
            class: 0,
            open_air: false,
            orientation: [[0.0; 3]; 3],
            spin: 0,
            bays: 0,
        }
    }

    #[test]
    fn only_a_starport_inside_the_0x13_byte_is_a_witness() {
        let bodies = [
            body(Kind::Planet, [0; 3]),
            body(Kind::Starport, [1_000_000, 0, 0]),
            body(Kind::Starport, [1_000_000_000_000, 0, 0]),
        ];
        // The first starport (ordinal 0) is the witness; the planet is not.
        assert_eq!(starport_witness(&bodies, [0; 3]), Some(0));
        let far = [body(Kind::Starport, [1_000_000_000_000, 0, 0])];
        assert_eq!(starport_witness(&far, [0; 3]), None);
    }

    /// The long-leg rendezvous: a body moving around a circle is met where it
    /// really is at the advanced clock. The old fixed-point loop oscillated for
    /// a fast planet (Sirius' Lucifer, Test-4 1) and never homed.
    #[test]
    fn intercept_time_meets_a_moving_target_without_orbiting() {
        use std::f64::consts::TAU;
        let radius = 4.0e8_f64;
        let period = 4_000.0_f64;
        let omega = TAU / period;
        let ship = [0_i64; 3];
        let stop_cover = 1.5e9;
        let aim = |t: f64| {
            let angle = omega * t;
            Some([
                (radius * angle.cos()).round() as i64,
                (radius * angle.sin()).round() as i64,
                0,
            ])
        };
        let (seconds, at) =
            intercept_time(ship, stop_cover, 300.0, aim).expect("a rendezvous exists");
        assert!(seconds > 0.0 && seconds < period, "{seconds}");
        let expected = transfer_seconds((distance(ship, at) - stop_cover).max(0.0) / 1000.0);
        assert!(
            (seconds - expected).abs() < 1.0,
            "{seconds} vs {expected}: the landing point is not at the transfer time"
        );
        assert!(
            distance(at, aim(seconds).unwrap()) < 1.0,
            "the destination is not the body's position at that time"
        );
    }

    /// F745/F746's final leg follows the current slot: off the axis it lines up
    /// on a gate outside the face, and once in the corridor it enters along the
    /// axis and brakes for the 250 m/s limit. Aiming at the centre alone crossed
    /// a spinning station's hull (the port's cleared-but-never-docks bug).
    #[test]
    fn the_final_slot_leg_tracks_the_station_axis() {
        let mut station = body(Kind::Station, [100_000_000; 3]);
        station.orientation = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let front = 200_000.0;
        let port = Port::new(&station, Some(front), Some(1_000_000.0));
        let gate = front + 4.0 * port.corridor() + 1.0e6;
        // Off to the side: line up on the outside gate.
        let side = [
            station.position[0] + 5_000_000,
            station.position[1],
            station.position[2],
        ];
        let (offset, stop) = slot_leg(&port, side);
        assert!((offset[2] - gate).abs() < 1.0, "{offset:?} gate {gate}");
        assert_eq!(stop, 0.0);
        // In front of the face: enter along the axis, braking by the face.
        let ahead = [
            station.position[0],
            station.position[1],
            station.position[2] + (front * 2.0) as i64,
        ];
        let (offset, stop) = slot_leg(&port, ahead);
        assert!((offset[2] - front * 0.2).abs() < 1.0, "{offset:?}");
        assert!((stop - front * 0.8).abs() < 1.0, "{stop}");
        // Behind the station is still the gate, not a short cut through the hull.
        let behind = [
            station.position[0],
            station.position[1],
            station.position[2] - 2_000_000,
        ];
        let (offset, _) = slot_leg(&port, behind);
        assert!((offset[2] - gate).abs() < 1.0, "{offset:?}");
    }

    /// Homing and arrival in real time: the nose comes onto a target behind the
    /// ship, the set speed follows the braking curve, and the ship stops at the
    /// standoff without passing it and turning back (the old orbiting symptom).
    #[test]
    fn autopilot_homes_brakes_and_stops_at_the_standoff() {
        use crate::flight::{Flight, Mode};
        let start = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let mut flight = OriginalFlight::from_flight(&start, None);
        let target = [0_i64; 3];
        let stop = 1.0e6;
        flight.teleport([0, 0, 20_000_000]);
        flight.set_time_acceleration(0);
        let mut memory = Memory::default();
        let mut closest = f64::MAX;
        let mut turned_away = false;
        let mut arrived = false;
        for _ in 0..200_000 {
            let command = prototype_autopilot(&flight, target, stop, &mut memory);
            if let Some(face) = command.face {
                flight.turn_toward(face, 0.05, 1.0);
            }
            flight.step(0.05, &command.keys).unwrap();
            let range = distance(flight.player().motion.position, target);
            closest = closest.min(range);
            let velocity = flight.player().motion.velocity;
            let speed = (velocity.iter().map(|c| f64::from(*c).powi(2)).sum::<f64>()).sqrt()
                * MM_PER_SECOND
                / 1000.0;
            if range > closest + 2.0e6 && closest <= stop * 1.2 {
                turned_away = true;
            }
            if command.arrived {
                arrived = true;
                break;
            }
            assert!(speed < 1.0e6, "the model ran away at {speed} m/s");
        }
        assert!(arrived, "did not arrive; closest {}", describe(closest));
        let range = distance(flight.player().motion.position, target);
        assert!(
            (range - stop).abs() < 2.0e6,
            "stopped at {} rather than {}",
            describe(range),
            describe(stop)
        );
        assert!(
            !turned_away,
            "the ship passed the standoff and came back (an orbit)"
        );
    }

    /// F832's seed formula on the executed original's Gateway position and a
    /// galaxy-table star's F856 position/F841 type word. No cache: the inputs are
    /// the values the tables hold, so this pins the derivation synthetically.
    #[test]
    fn the_system_seed_is_f832s_position_formula() {
        let gateway = [0x1717_0200, 0x1528_7600, -56_832];
        assert_eq!(system_seed(gateway, 8), 0x8d17_f528);
        // The 0x80 bit of the start type (Ross 154's extra word 0x88) selects
        // the hand-coded system, so F832 leaves the seed zero.
        assert_eq!(system_seed(gateway, 0x88), 0);
        // Titican, star 4 of sector [0x1716, 0x1529]: F856's position and F841's
        // type word 0x0002 (start type 2, no stars).
        assert_eq!(
            system_seed([387_375_616, 355_008_512, -58_368], 2),
            0x1717_db29
        );
    }

    /// The neighbour listing is a synthetic galaxy: the home star is skipped,
    /// duplicates are dropped, only stars within the reach survive, and the
    /// nearest comes first (F840/F852's whole-sector walk around the centre).
    #[test]
    fn neighbours_are_the_stars_within_reach_nearest_first() {
        let home_code = 100 | 100 << 13;
        let home = [100 << 16, 100 << 16, 0];
        let sectors = |x: u16, y: u16| -> Vec<(usize, [i32; 3], u16)> {
            match (x, y) {
                (100, 100) => vec![
                    (0, home, 0x08),
                    (1, [home[0] + 0x8000, home[1], 0], 0x02),
                    // A sector listing the same star twice must not duplicate it.
                    (1, [home[0] + 0x8000, home[1], 0], 0x02),
                    (2, [home[0], home[1] + 0x1_8000, 0], 0x02),
                ],
                (101, 100) => vec![(0, [home[0] + (1 << 16), home[1], 0], 0x02)],
                // Four sectors away is about 32 LY, beyond the 30 LY reach.
                (104, 100) => vec![(0, [home[0] + (4 << 16), home[1], 0], 0x02)],
                _ => Vec::new(),
            }
        };
        let found = neighbours_within(home_code, home, NEIGHBOUR_REACH, sectors);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].code, 100 | 100 << 13 | 1 << 26);
        assert_eq!(found[0].distance, 401);
        assert_eq!(found[0].kind, 0x02);
        assert_eq!(found[1].code, 101 | 100 << 13);
        assert_eq!(found[1].distance, 802);
        assert_eq!(found[2].code, 100 | 100 << 13 | 2 << 26);
        assert_eq!(found[2].distance, 1_204);
        assert!(
            found
                .iter()
                .all(|neighbour| neighbour.distance <= NEIGHBOUR_REACH),
            "{found:?}"
        );
        assert!(
            found.iter().all(|neighbour| neighbour.code != home_code),
            "the home star is not its own neighbour"
        );
        // A zero reach lists nothing (and a negative one, too).
        assert!(neighbours_within(home_code, home, 0, |_, _| unreachable!()).is_empty());
    }
}
