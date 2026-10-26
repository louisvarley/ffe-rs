mod library;

use ffe_core::galaxy::Tables;
use ffe_core::model_math::SineTable;
use ffe_core::objects::{Models, PhysList, Placement, SurfaceTools};
use ffe_core::sysgen::{handcoded, Generator, Params};
use serde::Deserialize;

fn system_name() -> [u8; 20] {
    let mut name = [0u8; 20];
    name[..8].copy_from_slice(b"Testname");
    name
}

fn cache_names() -> Vec<u8> {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    cache.asset("base:text/names").unwrap().to_vec()
}

fn cache_tables() -> Vec<u8> {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    cache.asset("base:world/system-tables").unwrap().to_vec()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Market {
    population: u32,
    danger: u32,
    allegiance: u32,
    stock: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    ranks: [u32; 2],
    federal: u32,
    imperial: u32,
    maximum: u32,
    allegiance: u32,
    minor: u32,
    major: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Galaxy {
    code: u32,
    index: i32,
    details: Option<[u32; 8]>,
    market: Option<Market>,
    control: Option<Control>,
}

/// Original F868-F871 (`--galaxy`): the system index (or class) of every table
/// code, the special codes and random codes near and far from the centre, and
/// for table systems the details, market parameters (stock string included) and
/// control with the player's ranks.
#[test]
#[ignore = "Local-only original world data; use tools/reference/verify-world.py"]
fn executed_reference_galaxy() {
    let bytes = cache_tables();
    let tables = Tables::new(&bytes).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_GALAXY_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let (mut cases, mut found, mut extras, mut classes) = (0, 0, 0, [0usize; 9]);
    for line in input.lines() {
        let case: Galaxy = serde_json::from_str(line).unwrap();
        // JJFFE appends eleven systems after the original's 329; a code that
        // only the reference knows has no counterpart in the original table.
        if case.index >= 329 {
            extras += 1;
            continue;
        }
        cases += 1;
        assert_eq!(tables.index(case.code), case.index, "code {:#x}", case.code);
        found += usize::from(case.index >= 0);
        {
            let d = tables.details(case.code);
            let want = case.details.unwrap();
            assert_eq!(
                (d.words[0], d.words[1], d.bytes, d.allegiance),
                (
                    want[0],
                    want[1],
                    [want[2], want[3], want[4], want[5], want[6]],
                    want[7]
                ),
                "details {:#x}",
                case.code
            );
            let m = tables.market(case.code);
            let want = case.market.unwrap();
            assert_eq!(
                (m.population, m.danger, m.allegiance, m.stock.to_vec()),
                (want.population, want.danger, want.allegiance, want.stock),
                "market {:#x}",
                case.code
            );
            let want = case.control.unwrap();
            let c = tables.control(case.code, want.ranks);
            assert_eq!(
                (
                    c.federal,
                    c.imperial,
                    c.maximum,
                    c.allegiance,
                    c.minor_rank,
                    c.major_rank
                ),
                (
                    want.federal,
                    want.imperial,
                    want.maximum,
                    want.allegiance,
                    want.minor,
                    want.major
                ),
                "control {:#x}",
                case.code
            );
        }
        if case.index < 0 {
            classes[(-case.index) as usize] += 1;
        }
    }
    assert!(
        cases > 4000 && found >= 329,
        "too few cases: {cases}, {found} table systems"
    );
    assert!(
        classes[1..].iter().all(|n| *n > 0),
        "a class was never seen: {classes:?}"
    );
    eprintln!("compared {cases} galaxy lookups ({found} table systems, classes {classes:?}); skipped {extras} codes only the reference knows");
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemCase {
    case: usize,
    seed: u32,
    mass: u32,
    stars: u32,
    code: u32,
    population: i32,
    #[serde(default)]
    hung: bool,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    ports: u8,
    #[serde(default)]
    stations: u8,
    #[serde(default)]
    state: [u32; 2],
    #[serde(default)]
    allegiance: u32,
    #[serde(default)]
    objects: Vec<Vec<u8>>,
}

/// The NUL-terminated name of object `j` in a system list.
fn name_of(list: &[u8], j: usize) -> Vec<u8> {
    let field = &list[j * 68 + 0x26..j * 68 + 0x3f];
    field[..field.iter().position(|b| *b == 0).unwrap_or(field.len())].to_vec()
}

/// Whether object `i`'s name cannot be reproduced: it is built from the
/// uninitialised counters of the original's F877 (or from such a name), or holds
/// the accented letters JJFFE's data spells differently (see `names_reference`).
fn unspecified_name(list: &[u8], uncertain: &[usize], i: usize) -> bool {
    let own = name_of(list, i);
    uncertain
        .iter()
        .any(|&j| j <= i && own.starts_with(&name_of(list, j)))
        || (0..=i).any(|j| {
            let other = name_of(list, j);
            other.iter().any(|b| *b >= 0x80) && own.starts_with(&other)
        })
}

/// The object with the bytes after its name's NUL cleared (the original leaves
/// whatever its string pieces wrote there).
fn terminated(object: &[u8]) -> Vec<u8> {
    let mut copy = object.to_vec();
    if let Some(end) = copy[0x26..0x3f].iter().position(|b| *b == 0) {
        copy[0x26 + end..0x3f].fill(0);
    }
    copy
}

/// Original F875 and F878 (`--sysgen`): the hand-coded objects, then 4096 synthetic
/// systems (seeds, star masses, star counts, codes, populations). Every object's
/// 68 bytes (names zeroed), the counters and the random state must match; the
/// few parameter sets that make the original loop forever are skipped.
#[test]
#[ignore = "Local-only original world generation; use tools/reference/verify-world.py"]
fn executed_reference_sysgen() {
    let bytes = cache_tables();
    let name_bytes = cache_names();
    let names = ffe_core::names::Names::new(&name_bytes).unwrap();
    let tables = Tables::new(&bytes).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_SYSGEN_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 160 << 20);
    let mut lines = input.lines();
    let first: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let expected: Vec<Vec<u8>> = serde_json::from_value(first["handcoded"].clone()).unwrap();
    let built = handcoded(&tables, Some(&names));
    assert_eq!(expected.len(), 146);
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            terminated(&built[i * 68..i * 68 + 68]),
            terminated(want),
            "hand-coded object {i}"
        );
    }
    let (mut cases, mut compared, mut hung, mut objects) = (0, 0, 0, 0);
    for line in lines {
        let case: SystemCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        cases += 1;
        if case.hung {
            hung += 1;
            continue;
        }
        let mut generator = Generator::new(Tables::new(&bytes).unwrap());
        generator.set_names(&names);
        let count = generator
            .generate(&Params {
                seed: case.seed,
                mass: case.mass,
                stars: case.stars,
                code: case.code,
                population: case.population,
                name: system_name(),
            })
            .unwrap();
        assert_eq!(count, case.count, "system case {}: object count", case.case);
        for (i, want) in case.objects.iter().enumerate() {
            let got = generator.list[i * 68..i * 68 + 68].to_vec();
            let unspecified = unspecified_name(&generator.list, &generator.uncertain, i);
            let (mut got, mut want) = (terminated(&got), terminated(want));
            if unspecified {
                got[0x26..0x3f].fill(0);
                want[0x26..0x3f].fill(0);
            }
            assert_eq!(got, want, "system case {} object {i}", case.case);
        }
        // A hand-coded system leaves the random state as it was.
        let state = if case.seed == 0 {
            case.state
        } else {
            generator.state
        };
        assert_eq!(
            (
                generator.starports,
                generator.stations,
                state,
                generator.allegiance
            ),
            (case.ports, case.stations, case.state, case.allegiance),
            "system case {}: counters",
            case.case
        );
        compared += 1;
        objects += case.count;
    }
    assert!(compared > 4100, "too few systems: {compared}");
    eprintln!("compared hand-coded objects and {compared} systems ({objects} objects), {hung} skipped as non-terminating");
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldCase {
    case: usize,
    #[serde(default)]
    hung: bool,
    #[serde(default)]
    seed: u32,
    #[serde(default)]
    mass: u32,
    #[serde(default)]
    stars: u32,
    #[serde(default)]
    code: u32,
    #[serde(default)]
    population: i32,
    #[serde(default)]
    low: u32,
    #[serde(default)]
    high: u32,
    #[serde(default)]
    stamp: u8,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    counter: i32,
    #[serde(default)]
    system: Vec<Vec<u8>>,
    #[serde(default)]
    flags: Vec<u8>,
    #[serde(default)]
    objects: std::collections::BTreeMap<String, Vec<u8>>,
}

/// Original F880 with F927, F1533, F879 and the orbit helpers (`--worldgen`): the
/// physics objects created for synthetic systems at synthetic clocks. Every
/// flag byte and every object's 0x152 bytes (names zeroed) must match, and so
/// must the system objects' physics indexes.
#[test]
#[ignore = "Local-only original world generation; use tools/reference/verify-world.py"]
fn executed_reference_worldgen() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let name_bytes = cache.asset("base:text/names").unwrap().to_vec();
    let names = ffe_core::names::Names::new(&name_bytes).unwrap();
    let blob = cache.asset("base:world/system-tables").unwrap();
    let tables = Tables::new(blob).unwrap();
    let models = Models {
        records: cache.asset("base:model/records").unwrap(),
        stats: cache.asset("base:ship/stats").unwrap(),
        gameplay: cache.asset("base:gameplay/tables").unwrap(),
    };
    let sine = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let library = library::Library::load();
    let surface = SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    let placement = Placement {
        tables: &tables,
        models: &models,
        sine: &sine,
        surface: Some(&surface),
    };
    let input = std::fs::read_to_string(std::env::var_os("FFE_WORLDGEN_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 400 << 20);
    let (mut cases, mut compared, mut bodies) = (0, 0, 0);
    for line in input.lines() {
        let case: WorldCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        cases += 1;
        if case.hung {
            continue;
        }
        let mut generator = Generator::new(Tables::new(blob).unwrap());
        generator.set_names(&names);
        generator
            .generate(&Params {
                seed: case.seed,
                mass: case.mass,
                stars: case.stars,
                code: case.code,
                population: case.population,
                name: system_name(),
            })
            .unwrap();
        let mut system = generator.list.clone();
        let mut list = PhysList {
            stamp: case.stamp,
            clock: [case.low, case.high],
            ..PhysList::default()
        };
        placement.create(&mut list, &mut system, [case.low, case.high]);
        assert_eq!(
            case.system.len() as u32,
            case.count,
            "worldgen case {} count",
            case.case
        );
        let mut loose = Vec::new();
        for (i, want) in case.system.iter().enumerate() {
            let mut got = system[i * 68..i * 68 + 68].to_vec();
            let mut want = want.clone();
            if unspecified_name(&system, &generator.uncertain, i) {
                loose.push(u32::from_le_bytes(
                    system[i * 68 + 0x18..i * 68 + 0x1c].try_into().unwrap(),
                ) as usize);
                got[0x26..0x3f].fill(0);
                want[0x26..0x3f].fill(0);
            } else {
                got = terminated(&got);
                want = terminated(&want);
            }
            assert_eq!(got, want, "worldgen case {} system object {i}", case.case);
        }
        assert_eq!(
            &list.bytes[..0x73],
            &case.flags[..],
            "worldgen case {} flags",
            case.case
        );
        assert_eq!(
            list.counter, case.counter,
            "worldgen case {} counter",
            case.case
        );
        for (index, want) in &case.objects {
            let index: usize = index.parse().unwrap();
            let at = 0x74 + 0x152 * index;
            let mut got = list.bytes[at..at + 0x152].to_vec();
            let mut want = want.clone();
            if loose.contains(&index) {
                got[0x124..0x138].fill(0);
                want[0x124..0x138].fill(0);
            } else {
                let end = got[0x124..0x138].iter().position(|b| *b == 0).unwrap_or(20);
                got[0x124 + end..0x138].fill(0);
                let end = want[0x124..0x138]
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap_or(20);
                want[0x124 + end..0x138].fill(0);
            }
            let differing: Vec<usize> = (0..got.len()).filter(|i| got[*i] != want[*i]).collect();
            assert!(
                differing.is_empty(),
                "worldgen case {} physics object {index}: bytes differ at {differing:x?} (got {:?}, want {:?})",
                case.case,
                differing.iter().map(|i| got[*i]).collect::<Vec<_>>(),
                differing.iter().map(|i| want[*i]).collect::<Vec<_>>()
            );
            bodies += 1;
        }
        compared += 1;
    }
    assert!(compared > 900, "too few systems: {compared}");
    eprintln!("compared {compared} created systems ({bodies} physics objects)");
}

#[test]
#[ignore = "Local-only F866 original vectors and FFE_WORLD_CACHE"]
fn executed_reference_body_update() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let tables = Tables::new(cache.asset("base:world/system-tables").unwrap()).unwrap();
    let models = Models {
        records: cache.asset("base:model/records").unwrap(),
        stats: cache.asset("base:ship/stats").unwrap(),
        gameplay: cache.asset("base:gameplay/tables").unwrap(),
    };
    let sine = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let placement = Placement {
        tables: &tables,
        models: &models,
        sine: &sine,
        surface: None,
    };
    #[derive(Deserialize)]
    struct Case {
        clock: [u32; 2],
        before: Vec<u8>,
        after: Vec<u8>,
    }
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_BODY_UPDATE_VECTORS").unwrap()).unwrap();
    let mut count = 0;
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        let mut list = PhysList::default();
        list.bytes[1] = 0x1d;
        let at = ffe_core::objects::FIRST_OBJECT + ffe_core::objects::OBJECT_BYTES;
        assert_eq!(case.before.len(), ffe_core::objects::OBJECT_BYTES);
        list.bytes[at..at + case.before.len()].copy_from_slice(&case.before);
        placement.advance(&mut list, case.clock);
        assert_eq!(
            &list.bytes[at..at + case.after.len()],
            case.after.as_slice(),
            "F866 case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 256);
}
