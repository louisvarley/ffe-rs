use ffe_core::galaxy::Tables;
use ffe_core::sysgen::{handcoded, Generator, Params};
use serde::Deserialize;

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

fn without_names(object: &[u8]) -> Vec<u8> {
    let mut copy = object.to_vec();
    copy[0x26..0x3a].fill(0);
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
    let tables = Tables::new(&bytes).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_SYSGEN_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 160 << 20);
    let mut lines = input.lines();
    let first: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let expected: Vec<Vec<u8>> = serde_json::from_value(first["handcoded"].clone()).unwrap();
    let built = handcoded(&tables);
    assert_eq!(expected.len(), 146);
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            without_names(&built[i * 68..i * 68 + 68]),
            *want,
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
        let count = generator
            .generate(&Params {
                seed: case.seed,
                mass: case.mass,
                stars: case.stars,
                code: case.code,
                population: case.population,
            })
            .unwrap();
        assert_eq!(count, case.count, "system case {}: object count", case.case);
        for (i, want) in case.objects.iter().enumerate() {
            let got = without_names(&generator.list[i * 68..i * 68 + 68]);
            assert_eq!(&got, want, "system case {} object {i}", case.case);
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
