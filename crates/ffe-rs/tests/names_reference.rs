use ffe_core::names::Names;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: u32,
    kind: u32,
    seed: u32,
    length: usize,
    text: Vec<u8>,
}

/// The original F791 (`--names`): names of types 1 to 0x14 for random seeds.
#[test]
#[ignore = "Local-only original name generator; use tools/reference/verify-world.py"]
fn executed_reference_names() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let names = Names::new(cache.asset("base:text/names").unwrap()).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_NAME_VECTORS").unwrap()).unwrap();
    let (mut cases, mut accented) = (0, 0);
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        let mut got = names
            .generate(case.kind, case.seed)
            .unwrap_or_else(|| panic!("unsupported kind {}", case.kind));
        // JJFFE spells some of the five accented names with plain letters (the
        // game's font has the accents at codes 0x82, 0x83 and 0x8a): those names
        // cannot be compared.
        if got.iter().any(|b| matches!(*b, 0x82 | 0x83 | 0x8a)) {
            accented += 1;
            continue;
        }
        got.push(0);
        assert_eq!(got, case.text, "case {} kind {:#x}", case.case, case.kind);
        assert_eq!(got.len(), case.length, "case {} length", case.case);
        cases += 1;
    }
    assert!(cases >= 7800);
    eprintln!("compared {cases} generated names ({accented} with accented letters skipped)");
}
