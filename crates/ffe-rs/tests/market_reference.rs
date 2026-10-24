//! Local-only comparison of `ffe_core::market` with the executed original: F619 (price) and
//! F620 (stock) with the game's own tables (`--market`). Run through
//! `tools/reference/verify-market.py`, which needs the Linux reference build.
use ffe_core::market::Tables;
use ffe_core::RandomState;
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

#[test]
#[ignore = "Local-only market comparison; use tools/reference/verify-market.py"]
fn executed_reference_market() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_MARKET_CACHE").expect("FFE_MARKET_CACHE")).unwrap(),
    )
    .unwrap();
    let tables = Tables::new(cache.asset("base:market/tables").unwrap()).unwrap();
    let path = std::env::var_os("FFE_MARKET_VECTORS").expect("FFE_MARKET_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 32 << 20, "trace too large");
    let (mut cases, mut negative, mut illegal, mut capped) = (0, 0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let good = int(&case["good"]) as usize;
        let (population, danger) = (int(&case["population"]), int(&case["danger"]));
        let flags = int(&case["flags"]) as u8;
        let seed = (int(&case["a"]) as u32, int(&case["b"]) as u32);
        let mut rng = RandomState::new(seed.0, seed.1);
        let price = tables
            .price(good, population, danger, flags, &mut rng)
            .unwrap();
        assert_eq!(i64::from(price), int(&case["price"]), "price: {line}");
        assert_eq!(
            (i64::from(rng.a), i64::from(rng.b)),
            (int(&case["price_a"]), int(&case["price_b"])),
            "generator after the price: {line}"
        );
        let mut rng = RandomState::new(seed.0, seed.1);
        let stock = tables
            .availability(good, population, danger, flags, &mut rng)
            .unwrap();
        assert_eq!(i64::from(stock), int(&case["stock"]), "stock: {line}");
        assert_eq!(
            (i64::from(rng.a), i64::from(rng.b)),
            (int(&case["stock_a"]), int(&case["stock_b"])),
            "generator after the stock: {line}"
        );
        cases += 1;
        negative += usize::from(flags & 8 != 0);
        illegal += usize::from(flags & 0x80 != 0);
        capped += usize::from(price >= 0x7fff);
    }
    eprintln!("{cases} market cases ({negative} negative-variance, {illegal} illegal, {capped} high prices)");
    assert_eq!(cases, 8192);
    assert!(negative > 1000 && illegal > 1000);
}
