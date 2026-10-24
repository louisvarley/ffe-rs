//! The station markets: prices and availability (F619, F620), their first
//! generation (F622) and daily drift (F633), and the player's buying and
//! selling at a station (F555, F556), ported from `ffe.asm`.
//!
//! The tables (`DATA_006164..6169`) and the 38 commodity names (`DATA_006216..6253`)
//! are game data imported into the content cache (v20, the `MarketTables`
//! asset): 704 bytes of tables followed by the names, each ending in a zero.
//! Prices and the player's cash share one unit: cash starts at 1000, and the
//! prices come out of the formulas below in that unit (a tenth of a credit).
//!
//! Randomness is the game's own generator ([`RandomState`], F1530). The original
//! draws from its single global stream, interleaved with everything else the
//! game does (F633 also generates missions and bulletin entries); a caller that
//! has not ported all of those owns a separate stream.

use crate::RandomState;

/// Bytes of the tables: `DATA_006164` to the end of `DATA_006169`.
pub const TABLE_BYTES: usize = 704;
/// Commodities with a market (`0x21`); the next five names are mission items.
pub const GOODS: usize = 33;
/// Names in the block (`DATA_006216..6253`).
pub const NAMES: usize = 38;

const SUPPLY: usize = 0; // DATA_006164, 8 dwords
const POPULATION_PRICE: usize = 32; // DATA_006165, 10
const DANGER_PRICE: usize = 72; // DATA_006166, 8
const POPULATION_STOCK: usize = 104; // DATA_006167, 10
const DANGER_STOCK: usize = 144; // DATA_006168, 8
const GOOD: usize = 176; // DATA_006169, 33 records of 16 bytes

/// The stock string's flags for a commodity in a system (`DATA_008989`): bits 0-2 the
/// supply class, bit 3 a negative price variance, bit 7 an illegal good.
pub const NEGATIVE: u8 = 0x08;
pub const ILLEGAL: u8 = 0x80;

/// A commodity's own numbers (`DATA_006169`, 16 bytes each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Good {
    pub base_price: i32,
    pub price_variance: i32,
    pub base_stock: i32,
    pub stock_variance: i32,
}

/// A borrowed view of the imported block (tables, then the names).
#[derive(Debug, Clone, Copy)]
pub struct Tables<'a> {
    bytes: &'a [u8],
}

fn dword(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

impl<'a> Tables<'a> {
    /// Check the block's size and that it holds the 38 names after the tables.
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < TABLE_BYTES {
            return Err("market tables are too short".to_owned());
        }
        let names = &bytes[TABLE_BYTES..];
        if names.iter().filter(|b| **b == 0).count() != NAMES
            || names.last() != Some(&0)
            || names.iter().any(|b| *b != 0 && !(0x20..0x7f).contains(b))
        {
            return Err("market names are not 38 printable strings".to_owned());
        }
        Ok(Self { bytes })
    }

    /// The name of commodity `index` (0..38).
    pub fn name(&self, index: usize) -> Option<&'a str> {
        if index >= NAMES {
            return None;
        }
        let name = self.bytes[TABLE_BYTES..].split(|b| *b == 0).nth(index)?;
        std::str::from_utf8(name).ok()
    }

    pub fn good(&self, index: usize) -> Good {
        let at = GOOD + 16 * index;
        Good {
            base_price: dword(self.bytes, at),
            price_variance: dword(self.bytes, at + 4),
            base_stock: dword(self.bytes, at + 8),
            stock_variance: dword(self.bytes, at + 12),
        }
    }

    fn entry(&self, table: usize, count: usize, index: i64, what: &str) -> Result<i32, String> {
        if !(0..count as i64).contains(&index) {
            return Err(format!("{what} {index} is outside the market tables"));
        }
        Ok(dword(self.bytes, table + 4 * index as usize))
    }

    /// F619: the price of commodity `good` in a system of population `population`
    /// (a table index), danger `danger`, with the commodity's `flags` (its supply
    /// class is `flags & 7`). Draws one number.
    pub fn price(
        &self,
        good: usize,
        population: i64,
        danger: i64,
        flags: u8,
        rng: &mut RandomState,
    ) -> Result<i32, String> {
        let record = self.good(good);
        let supply = i64::from(flags & 7);
        let by_population = self.entry(POPULATION_PRICE, 10, population, "population")?;
        let by_supply = self.entry(SUPPLY, 8, supply, "supply")?;
        let by_danger = self.entry(DANGER_PRICE, 8, danger, "danger")?;
        let mut variance = by_population.wrapping_mul(record.price_variance) >> 15;
        variance = by_supply.wrapping_mul(variance) >> 15;
        if flags & NEGATIVE != 0 {
            variance = variance.wrapping_neg();
        }
        let mut value = variance.wrapping_add(record.base_price);
        if flags & ILLEGAL != 0 {
            value = value.wrapping_add(value);
        }
        value = by_danger.wrapping_mul(value) >> 15;
        // A capped price is not doubled, as in the original.
        value = if value > 0x7fff {
            0x7fff
        } else {
            value.wrapping_add(value)
        };
        let noise = i32::from(rng.next_u16() & 0x3ff) + 0x4000 - 0x200;
        let value = noise.wrapping_mul(value);
        let value = if value <= 0x1fff_ffff {
            value.wrapping_shl(2)
        } else if value <= 0x3fff_ffff {
            value.wrapping_add(value)
        } else {
            0x7fff_ffff
        };
        Ok(value >> 16)
    }

    /// F620: how much of commodity `good` there is. Draws two numbers.
    pub fn availability(
        &self,
        good: usize,
        population: i64,
        danger: i64,
        flags: u8,
        rng: &mut RandomState,
    ) -> Result<i32, String> {
        let record = self.good(good);
        let supply = i64::from(flags & 7);
        let by_supply = self.entry(SUPPLY, 8, supply, "supply")?;
        let by_population = self.entry(POPULATION_STOCK, 10, population, "population")?;
        let by_danger = self.entry(DANGER_STOCK, 8, danger, "danger")?;
        let mut variance = by_supply.wrapping_mul(record.stock_variance) >> 15;
        if flags & NEGATIVE == 0 {
            variance = variance.wrapping_neg();
        }
        let mut value = by_population.wrapping_mul(variance.wrapping_add(record.base_stock)) >> 15;
        if flags & ILLEGAL != 0 {
            value >>= 3;
        }
        value = by_danger.wrapping_mul(value) >> 15;
        let spread = |rng: &mut RandomState| {
            ((i32::from(rng.next_u16() & 0x3fff) + 0x4000).wrapping_mul(value)) >> 15
        };
        // The first draw only decides the sign of the correction, the second sizes it.
        let adjust = if spread(rng) >= 0 {
            spread(rng)
        } else {
            spread(rng).wrapping_neg()
        };
        let total = value.wrapping_add(adjust);
        Ok(total.max(0))
    }
}

/// One commodity at one station: how many tonnes there are and what one costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Entry {
    pub stock: i32,
    pub price: i32,
}

/// A system's market conditions: what F870 reports and the stock string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conditions<'a> {
    pub population: i64,
    pub danger: i64,
    /// The per-commodity flags (`DATA_008989`); at least [`GOODS`] bytes.
    pub flags: &'a [u8],
}

/// A station's market: [`GOODS`] entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Market {
    pub entries: [Entry; GOODS],
}

/// What stops a trade (F555, F556).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The hold is full (0x98e3).
    HoldFull,
    /// The station has none (0x98e4).
    OutOfStock,
    /// The price is more than the cash (0x98e1).
    NoCash,
    /// Nothing of it in the hold (0x98e5).
    NothingToSell,
    /// A negative price the cash cannot cover (0x98e2).
    CannotPayToSell,
}

impl Refusal {
    pub fn text(self) -> &'static str {
        match self {
            Refusal::HoldFull => "Your cargo hold is full",
            Refusal::OutOfStock => "None for sale",
            Refusal::NoCash => "You cannot afford it",
            Refusal::NothingToSell => "You have none to sell",
            Refusal::CannotPayToSell => "You cannot afford to dump it",
        }
    }
}

/// The player's cargo: tonnes of each commodity, the hold's size and what is in it
/// (`DATA_008896`, `DATA_008889`, `DATA_008891`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub cargo: [u16; NAMES],
    pub capacity: i32,
    pub used: i32,
}

impl Hold {
    pub fn new(capacity: i32) -> Self {
        Self {
            cargo: [0; NAMES],
            capacity,
            used: 0,
        }
    }
}

impl Market {
    /// F622's loop for one station: every commodity's price, then its stock, in
    /// order (the original also places a market event per commodity).
    pub fn generate(
        tables: &Tables,
        conditions: &Conditions,
        rng: &mut RandomState,
    ) -> Result<Self, String> {
        let mut entries = [Entry::default(); GOODS];
        for (good, entry) in entries.iter_mut().enumerate() {
            let flags = *conditions
                .flags
                .get(good)
                .ok_or("stock string is too short")?;
            entry.price =
                tables.price(good, conditions.population, conditions.danger, flags, rng)?;
            entry.stock =
                tables.availability(good, conditions.population, conditions.danger, flags, rng)?;
        }
        Ok(Self { entries })
    }

    /// F633's market half, once a game day: every price and stock moves a quarter
    /// of the way to a new draw (truncating toward zero).
    pub fn daily(
        &mut self,
        tables: &Tables,
        conditions: &Conditions,
        rng: &mut RandomState,
    ) -> Result<(), String> {
        let quarter = |new: i32, old: i32| {
            let sum = new.wrapping_add(old.wrapping_mul(3));
            (if sum < 0 { sum.wrapping_add(3) } else { sum }) >> 2
        };
        for (good, entry) in self.entries.iter_mut().enumerate() {
            let flags = *conditions
                .flags
                .get(good)
                .ok_or("stock string is too short")?;
            let price = tables.price(good, conditions.population, conditions.danger, flags, rng)?;
            entry.price = quarter(price, entry.price);
            let stock =
                tables.availability(good, conditions.population, conditions.danger, flags, rng)?;
            entry.stock = quarter(stock, entry.stock);
        }
        Ok(())
    }

    /// F555 for one tonne: the cash pays the price, the tonne moves from the
    /// station to the hold.
    pub fn buy(&mut self, good: usize, cash: &mut i32, hold: &mut Hold) -> Result<(), Refusal> {
        let entry = &mut self.entries[good];
        if hold.used >= hold.capacity {
            return Err(Refusal::HoldFull);
        }
        if entry.stock == 0 {
            return Err(Refusal::OutOfStock);
        }
        if entry.price > 0 && entry.price > *cash {
            return Err(Refusal::NoCash);
        }
        *cash = cash.wrapping_sub(entry.price);
        entry.stock = entry.stock.wrapping_sub(1);
        hold.cargo[good] = hold.cargo[good].wrapping_add(1);
        hold.used += 1;
        Ok(())
    }

    /// F556 for one tonne: the station pays the price for a tonne from the hold.
    pub fn sell(&mut self, good: usize, cash: &mut i32, hold: &mut Hold) -> Result<(), Refusal> {
        let entry = &mut self.entries[good];
        if hold.cargo[good] == 0 {
            return Err(Refusal::NothingToSell);
        }
        if entry.price < 0 && entry.price.wrapping_neg() > *cash {
            return Err(Refusal::CannotPayToSell);
        }
        *cash = cash.wrapping_add(entry.price);
        hold.cargo[good] -= 1;
        entry.stock = entry.stock.wrapping_add(1);
        hold.used -= 1;
        Ok(())
    }
}
