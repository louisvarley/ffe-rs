use ffe_core::market::{
    Conditions, Entry, Hold, Market, Refusal, Tables, GOODS, ILLEGAL, NAMES, NEGATIVE, TABLE_BYTES,
};
use ffe_core::RandomState;

/// A synthetic block: the multipliers are Q15 fractions chosen to make the arithmetic
/// easy to follow, every commodity has the same numbers, and the names are `g0`, `g1`...
fn block() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut put = |values: &[i32]| {
        for v in values {
            bytes.extend(v.to_le_bytes());
        }
    };
    // Supply -> variance: 0, 1/4, 1/2, 3/4, 1, then the top.
    put(&[0, 0x2000, 0x4000, 0x6000, 0x7fff, 0x7fff, 0x7fff, 0x7fff]);
    // Population -> price variance (10).
    put(&[
        0x7fff, 0x6000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000,
    ]);
    // Danger -> price multiplier: 1/2, 1, ...
    put(&[
        0x4000, 0x7fff, 0x7fff, 0x7fff, 0x7fff, 0x7fff, 0x7fff, 0x7fff,
    ]);
    // Population -> stock multiplier (10): 1/2 everywhere but the first.
    put(&[
        0x2000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000,
    ]);
    // Danger -> stock multiplier: 1, 1/2, ...
    put(&[
        0x7fff, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000,
    ]);
    for _ in 0..GOODS {
        // base price 100, price variance 0x1000, base stock 1000, stock variance 0x2000.
        put(&[100, 0x1000, 1000, 0x2000]);
    }
    assert_eq!(bytes.len(), TABLE_BYTES);
    for n in 0..NAMES {
        bytes.extend(format!("g{n}").bytes());
        bytes.push(0);
    }
    bytes
}

fn rng() -> RandomState {
    RandomState::new(0x1357_9bdf, 0x2468_ace0)
}

#[test]
fn the_block_is_checked_and_the_names_read() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    assert_eq!(tables.name(0), Some("g0"));
    assert_eq!(tables.name(37), Some("g37"));
    assert_eq!(tables.name(38), None);
    let good = tables.good(5);
    assert_eq!(
        (
            good.base_price,
            good.price_variance,
            good.base_stock,
            good.stock_variance
        ),
        (100, 0x1000, 1000, 0x2000)
    );
    assert!(Tables::new(&bytes[..TABLE_BYTES - 1]).is_err());
    assert!(
        Tables::new(&bytes[..bytes.len() - 1]).is_err(),
        "last name unterminated"
    );
    let mut bad = bytes.clone();
    bad[TABLE_BYTES] = 0x01;
    assert!(Tables::new(&bad).is_err(), "unprintable name");
}

/// The price with supply class 0 (no variance): base 100, danger 0 halves it, the
/// result is doubled, then scaled by the noise: `(draw & 0x3ff) + 0x3e00`, times four, over 65,536.
#[test]
fn the_price_follows_the_formula() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    let draw = i64::from(rng().next_u16() & 0x3ff) + 0x4000 - 0x200;
    let mut r = rng();
    // 100 * 0x4000 >> 15 = 50, doubled = 100.
    assert_eq!(
        tables.price(0, 2, 0, 0, &mut r).unwrap(),
        ((draw * 100 * 4) >> 16) as i32
    );
    // Illegal goods cost double before the danger factor: 200 -> 100 -> 200.
    let mut r = rng();
    assert_eq!(
        tables.price(0, 2, 0, ILLEGAL, &mut r).unwrap(),
        ((draw * 200 * 4) >> 16) as i32
    );
    // Danger 1 keeps nearly the whole value (0x7fff / 0x8000): 100 -> 99 -> 198.
    let mut r = rng();
    assert_eq!(
        tables.price(0, 2, 1, 0, &mut r).unwrap(),
        ((draw * 198 * 4) >> 16) as i32
    );
    // One draw only.
    let mut once = rng();
    once.next_u16();
    assert_eq!(r, once);
}

#[test]
fn supply_and_its_sign_move_the_price() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    // Population 2 (0x4000 of 0x1000 = 0x800), supply class 4 (almost full): 0x7fff * 0x800 >> 15.
    let variance = (0x7fff_i64 * ((0x4000_i64 * 0x1000) >> 15)) >> 15;
    assert_eq!(variance, 0x7ff);
    let draw = i64::from(rng().next_u16() & 0x3ff) + 0x4000 - 0x200;
    let expect = |base: i64| {
        // danger 1 keeps 0x7fff / 0x8000 of it; doubled; noise; times four; >> 16.
        let value = ((0x7fff * base) >> 15) * 2;
        ((draw * value * 4) >> 16) as i32
    };
    assert_eq!(
        tables.price(0, 2, 1, 4, &mut rng()).unwrap(),
        expect(100 + variance)
    );
    assert_eq!(
        tables.price(0, 2, 1, 4 | NEGATIVE, &mut rng()).unwrap(),
        expect(100 - variance)
    );
}

#[test]
fn a_huge_value_is_capped_and_not_doubled() {
    let mut bytes = block();
    // Base price 40000 for commodity 0: danger 1 keeps 40000, over 0x7fff.
    bytes[176..180].copy_from_slice(&40_000i32.to_le_bytes());
    let tables = Tables::new(&bytes).unwrap();
    let draw = i64::from(rng().next_u16() & 0x3ff) + 0x4000 - 0x200;
    // The value is capped at 0x7fff (not doubled); the noise product is over 0x1fffffff, so
    // only doubled: draw * 0x7fff * 2 >> 16 once more over 0x3fffffff would cap at 0x7fffffff.
    let product = draw * 0x7fff;
    let scaled = if product <= 0x1fff_ffff {
        product * 4
    } else if product <= 0x3fff_ffff {
        product * 2
    } else {
        0x7fff_ffff
    };
    assert_eq!(
        tables.price(0, 2, 1, 0, &mut rng()).unwrap(),
        (scaled >> 16) as i32
    );
}

#[test]
fn the_tables_refuse_out_of_range_indexes() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    assert!(tables.price(0, 10, 0, 0, &mut rng()).is_err());
    assert!(tables.price(0, -1, 0, 0, &mut rng()).is_err());
    assert!(tables.price(0, 0, 8, 0, &mut rng()).is_err());
    assert!(tables.availability(0, 10, 0, 0, &mut rng()).is_err());
    assert!(tables.availability(0, 0, 8, 0, &mut rng()).is_err());
}

/// Stock with supply class 0 and a positive-variance flag clear: no variance, base 1000,
/// population 2 halves it, danger 0 keeps it; then the two draws.
#[test]
fn availability_draws_twice_and_never_goes_negative() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    let mut probe = rng();
    let first = probe.next_u16();
    let second = probe.next_u16();
    assert!(first as i32 & 0x3fff >= 0);
    // 1000 * 0x4000 >> 15 = 500; danger 0 (0x7fff): 0x7fff * 500 >> 15 = 499.
    let value = 499_i64;
    let adjust = (((i64::from(second & 0x3fff)) + 0x4000) * value) >> 15;
    let mut r = rng();
    assert_eq!(
        tables.availability(0, 2, 0, 0, &mut r).unwrap(),
        (value + adjust) as i32
    );
    assert_eq!(r, probe, "two draws");
    // Illegal goods have an eighth.
    let value = ((1000_i64 * 0x4000) >> 15) >> 3;
    let value = (0x7fff_i64 * value) >> 15;
    let adjust = (((i64::from(second & 0x3fff)) + 0x4000) * value) >> 15;
    assert_eq!(
        tables.availability(0, 2, 0, ILLEGAL, &mut rng()).unwrap(),
        (value + adjust) as i32
    );
}

#[test]
fn a_market_is_generated_good_by_good_and_drifts_a_quarter_a_day() {
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    let flags = [0u8; 38];
    let conditions = Conditions {
        population: 2,
        danger: 1,
        flags: &flags,
    };
    let mut r = rng();
    let mut market = Market::generate(&tables, &conditions, &mut r).unwrap();
    // Price (one draw) then stock (two) for each of 33 goods.
    let mut check = rng();
    for _ in 0..GOODS * 3 {
        check.next_u16();
    }
    assert_eq!(r, check);
    let mut again = rng();
    for good in 0..GOODS {
        let entry = market.entries[good];
        assert_eq!(
            entry.price,
            tables.price(good, 2, 1, 0, &mut again).unwrap()
        );
        assert_eq!(
            entry.stock,
            tables.availability(good, 2, 1, 0, &mut again).unwrap()
        );
    }
    // A day: new = old at each step would not move; here the draws differ, so each value
    // is (new + 3 old) / 4.
    let before = market.clone();
    let mut day = RandomState::new(1, 2);
    market.daily(&tables, &conditions, &mut day).unwrap();
    let mut replay = RandomState::new(1, 2);
    for good in 0..GOODS {
        let price = tables.price(good, 2, 1, 0, &mut replay).unwrap();
        let stock = tables.availability(good, 2, 1, 0, &mut replay).unwrap();
        let q = |new: i32, old: i32| (new + 3 * old) / 4;
        assert_eq!(
            market.entries[good].price,
            q(price, before.entries[good].price)
        );
        assert_eq!(
            market.entries[good].stock,
            q(stock, before.entries[good].stock)
        );
    }
    // A short stock string is an error, not a guess.
    let short = Conditions {
        flags: &flags[..10],
        ..conditions
    };
    assert!(Market::generate(&tables, &short, &mut rng()).is_err());
}

fn market_with(price: i32, stock: i32) -> Market {
    Market {
        entries: [Entry { stock, price }; GOODS],
    }
}

#[test]
fn buying_moves_a_tonne_and_the_cash() {
    let mut market = market_with(30, 2);
    let mut hold = Hold::new(3);
    let mut cash = 100;
    market.buy(4, &mut cash, &mut hold).unwrap();
    assert_eq!(
        (cash, hold.cargo[4], hold.used, market.entries[4].stock),
        (70, 1, 1, 1)
    );
    market.buy(4, &mut cash, &mut hold).unwrap();
    assert_eq!(
        market.buy(4, &mut cash, &mut hold),
        Err(Refusal::OutOfStock)
    );
    // Other goods: the hold fills, then cash runs out.
    market.buy(5, &mut cash, &mut hold).unwrap();
    assert_eq!(hold.used, 3);
    assert_eq!(market.buy(5, &mut cash, &mut hold), Err(Refusal::HoldFull));
    let mut hold = Hold::new(10);
    let mut poor = 29;
    assert_eq!(market.buy(6, &mut poor, &mut hold), Err(Refusal::NoCash));
    assert_eq!((poor, hold.used, market.entries[6].stock), (29, 0, 2));
}

#[test]
fn a_free_commodity_ignores_the_cash_check() {
    let mut market = market_with(0, 5);
    let mut hold = Hold::new(10);
    let mut cash = 0;
    market.buy(1, &mut cash, &mut hold).unwrap();
    assert_eq!(cash, 0);
}

#[test]
fn chaff_is_the_last_market_good_before_the_mission_items() {
    // DATA_006248 is the thirty-third pointer, so its cargo word is
    // player+0x174 (0x134 + 2*32). The five names after it are mission items.
    assert_eq!(ffe_core::market::CHAFF, 32);
    assert_eq!(GOODS, 33);
    assert_eq!(NAMES - ffe_core::market::CHAFF, 6);
    let bytes = block();
    let tables = Tables::new(&bytes).unwrap();
    assert_eq!(tables.name(ffe_core::market::CHAFF), Some("g32"));
}

#[test]
fn selling_pays_the_price_and_returns_the_tonne() {
    let mut market = market_with(40, 1);
    let mut hold = Hold::new(5);
    hold.cargo[2] = 2;
    hold.used = 2;
    let mut cash = 10;
    market.sell(2, &mut cash, &mut hold).unwrap();
    assert_eq!(
        (cash, hold.cargo[2], hold.used, market.entries[2].stock),
        (50, 1, 1, 2)
    );
    assert_eq!(
        market.sell(3, &mut cash, &mut hold),
        Err(Refusal::NothingToSell)
    );
    // A negative price costs money, which the cash must cover.
    let mut market = market_with(-30, 1);
    let mut hold = Hold::new(5);
    hold.cargo[2] = 1;
    hold.used = 1;
    let mut poor = 29;
    assert_eq!(
        market.sell(2, &mut poor, &mut hold),
        Err(Refusal::CannotPayToSell)
    );
    let mut cash = 30;
    market.sell(2, &mut cash, &mut hold).unwrap();
    assert_eq!(cash, 0);
}
