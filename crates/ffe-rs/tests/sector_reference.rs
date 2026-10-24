//! Local-only comparison of `ffe_core::sector` with the executed original: F841 (the stars of a
//! sector, with F861), F853 (star counts), F856 (positions) and F833 (distances) (`--sectors`).
//! Run through `tools/reference/verify-sectors.py`.
use ffe_core::sector::{distance, position, Tables};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

#[test]
#[ignore = "Local-only sector comparison; use tools/reference/verify-sectors.py"]
fn executed_reference_sectors() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_SECTOR_CACHE").expect("FFE_SECTOR_CACHE")).unwrap(),
    )
    .unwrap();
    let tables = Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let path = std::env::var_os("FFE_SECTOR_VECTORS").expect("FFE_SECTOR_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 64 << 20, "trace too large");
    let (mut sectors, mut stars, mut specials, mut counts, mut positions, mut distances) =
        (0, 0, 0, 0, 0, 0);
    let (mut capped, mut hashed, mut far) = (0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        match case["k"].as_str().unwrap() {
            "sector" => {
                let (x, y) = (int(&case["x"]), int(&case["y"]));
                let got = if x < 0x2000 && y < 0x2000 {
                    tables.stars(x as u16, y as u16)
                } else {
                    Vec::new()
                };
                let want = case["stars"].as_array().unwrap();
                assert_eq!(int(&case["n"]) as usize, want.len().min(63), "{line}");
                assert_eq!(got.len(), want.len(), "star count: {line}");
                for (star, want) in got.iter().zip(want) {
                    let vector: Vec<u8> = want["v"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| int(v) as u8)
                        .collect();
                    assert_eq!(star.vector.to_vec(), vector, "vector: {line}");
                    assert_eq!(i64::from(star.kind), int(&want["kind"]), "type: {line}");
                }
                sectors += 1;
                stars += got.len();
                specials += usize::from(sectors <= 49);
            }
            "count" => {
                let (x, y, quality) = (int(&case["x"]), int(&case["y"]), int(&case["quality"]));
                let got = tables.star_count(x as u32, y as u32, quality as i32);
                assert_eq!(i64::from(got), int(&case["value"]), "star count: {line}");
                counts += 1;
                hashed += usize::from(quality == 0 && x < 0x2000 && y < 0x2000);
            }
            "position" => {
                let vector: Vec<u8> = case["v"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| int(v) as u8)
                    .collect();
                let out = position(
                    int(&case["x"]) as u16,
                    int(&case["y"]) as u16,
                    &vector.try_into().unwrap(),
                );
                let want: Vec<i64> = case["out"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| int(v) as i32 as i64)
                    .collect();
                assert_eq!(out.map(i64::from).to_vec(), want, "position: {line}");
                positions += 1;
            }
            "distance" => {
                let p: Vec<i32> = case["p"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| int(v) as i32)
                    .collect();
                let got = distance([p[0], p[1], p[2]], [p[3], p[4], p[5]]);
                assert_eq!(i64::from(got), int(&case["value"]), "distance: {line}");
                distances += 1;
                capped += usize::from(p.iter().any(|v| v.abs() > 0x4000_0000));
                far += usize::from(got > 100_000);
            }
            other => panic!("unknown case {other}"),
        }
    }
    eprintln!(
        "{sectors} sectors ({specials} hand-made, {stars} stars), {counts} counts ({hashed} hashed), {positions} positions, {distances} distances ({far} far, {capped} huge)"
    );
    assert_eq!((specials, sectors), (49, 3049));
    assert_eq!((counts, positions, distances), (4000, 3000, 6000));
    assert!(stars > 10_000 && hashed > 1000 && capped > 500);
}
