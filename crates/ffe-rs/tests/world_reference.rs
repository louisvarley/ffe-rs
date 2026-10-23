use ffe_core::galaxy::Tables;
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
        if case.index >= 0 {
            found += 1;
            let d = tables.details(case.code).unwrap();
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
            let m = tables.market(case.code).unwrap();
            let want = case.market.unwrap();
            assert_eq!(
                (m.population, m.danger, m.allegiance, m.stock.to_vec()),
                (want.population, want.danger, want.allegiance, want.stock),
                "market {:#x}",
                case.code
            );
            let want = case.control.unwrap();
            let c = tables.control(case.code, want.ranks).unwrap();
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
        } else {
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
