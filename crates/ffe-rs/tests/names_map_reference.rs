//! Local-only comparison of `ffe_core::sector`'s system names with the executed original:
//! F830 (`--system-names`). Run through `tools/reference/verify-system-names.py`.
use ffe_core::sector::{sector_label, Strings, Tables};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

#[test]
#[ignore = "Local-only system name comparison; use tools/reference/verify-system-names.py"]
fn executed_reference_system_names() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_NAME_CACHE").expect("FFE_NAME_CACHE")).unwrap(),
    )
    .unwrap();
    let tables = Tables::new(cache.asset("base:world/galaxy-tables").unwrap()).unwrap();
    let strings = Strings::new(cache.asset("base:text/system-strings").unwrap()).unwrap();
    let path = std::env::var_os("FFE_NAME_VECTORS").expect("FFE_NAME_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 16 << 20, "trace too large");
    let (mut made, mut generated, mut listed, mut sectors, mut templates) = (0, 0, 0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let selector = int(&case["selector"]);
        let code = int(&case["code"]) as u32;
        let want: Vec<u8> = case["name"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as u8)
            .collect();
        let got: Vec<u8> = match selector & 0x1ff {
            0x1fe => {
                sectors += 1;
                sector_label(code).into_bytes()
            }
            0 => {
                let name = tables.system_name(&strings, code).unwrap();
                if tables.special_free((code & 0x1fff) as u16, ((code >> 13) & 0x1fff) as u16) {
                    generated += 1;
                } else {
                    made += 1;
                }
                name
            }
            index => {
                // A few table strings are templates with FD/FE/FF codes (numbers, "light years"):
                // the original expands them through the string engine, which is not ported.
                let raw = strings.get(index as usize - 1).unwrap();
                if raw.iter().any(|b| *b >= 0xfd) {
                    templates += 1;
                    continue;
                }
                listed += 1;
                raw.to_vec()
            }
        };
        assert_eq!(got, want, "{}: {line}", String::from_utf8_lossy(&got));
    }
    eprintln!(
        "{made} hand-made, {generated} generated, {listed} listed (+{templates} templates skipped) and {sectors} sector names"
    );
    assert!(made >= 117 && generated >= 8000 && listed >= 100 && sectors == 40);
}
