//! Buying and selling at a station in the playable: the original's markets
//! (`ffe_core::market`) against the player's cash (`Flight::credits`, the
//! original's units: 1000 is 100.0 credits) and hold (`Flight::hold`).
use crate::flight::Flight;
use crate::navigation::Navigator;
use ffe_content::VerifiedCache;
use ffe_core::market::{Hold, Refusal, GOODS, ILLEGAL, NAMES};

/// One line of the market screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub good: usize,
    pub name: String,
    pub price: i32,
    pub stock: i32,
    pub held: u16,
}

/// A price or cash as the screen shows it: tenths of a credit.
pub fn credits(value: i32) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = i64::from(value).abs();
    format!("{sign}{}.{}", value / 10, value % 10)
}

/// F390/F391's hold capacity: the ship's delivered capacity less the mass of
/// the equipment fitted after delivery. A new game's `Navigator::hold` is
/// F998's 1 t with the delivered loadout already priced in (`F998` does not run
/// F390 for its own equipment); a hull bought later uses the cargo statistic and
/// all of the fitted mass.
pub fn capacity(nav: &Navigator, cache: &VerifiedCache, state: &Flight) -> i32 {
    let base = nav.hold(cache, state.ship);
    let delivered = if nav.new_game.is_some() {
        ffe_core::equipment::Loadout::new_game()
    } else {
        ffe_core::equipment::Loadout::default()
    };
    state
        .loadout
        .capacity(base, &delivered, &state.combat.lasers)
        .max(0)
}

/// The player's hold as the market code sees it.
pub fn hold_of(state: &Flight, capacity: i32) -> Hold {
    let mut hold = Hold::new(capacity);
    for (slot, tonnes) in hold.cargo.iter_mut().zip(&state.hold) {
        *slot = *tonnes;
    }
    hold.used = state.hold.iter().map(|t| i32::from(*t)).sum();
    hold
}

/// The market screen's lines for `station`: the legal goods, in the original's order.
pub fn listing(
    nav: &mut Navigator,
    cache: &VerifiedCache,
    station: usize,
    state: &Flight,
) -> Result<Vec<Listing>, String> {
    let flags = nav.stock_flags(cache)?;
    let names: Vec<String> = {
        let tables = nav.market_tables().ok_or("no market tables")?;
        (0..GOODS)
            .map(|good| tables.name(good).unwrap_or("?").to_owned())
            .collect()
    };
    let market = nav.market(cache, station)?;
    Ok((0..GOODS)
        .filter(|good| flags[*good] & ILLEGAL == 0)
        .map(|good| Listing {
            good,
            name: names[good].clone(),
            price: market.market.entries[good].price,
            stock: market.market.entries[good].stock,
            held: state.hold.get(good).copied().unwrap_or(0),
        })
        .collect())
}

/// Buy (or sell) up to `count` tonnes of `good` at `station`, stopping at the first
/// refusal. Returns what to tell the player.
pub fn trade(
    nav: &mut Navigator,
    cache: &VerifiedCache,
    state: &mut Flight,
    station: usize,
    good: usize,
    buying: bool,
    count: u32,
) -> Result<String, String> {
    if good >= GOODS {
        return Err("no such commodity".to_owned());
    }
    let space = capacity(nav, cache, state);
    let name = nav
        .market_tables()
        .and_then(|tables| tables.name(good).map(str::to_owned))
        .ok_or("no market tables")?;
    let mut hold = hold_of(state, space);
    let mut cash = i32::try_from(state.credits).unwrap_or(i32::MAX);
    let before = cash;
    let market = nav.market(cache, station)?;
    let mut done = 0;
    let mut refusal = None;
    for _ in 0..count.max(1) {
        let result = if buying {
            market.market.buy(good, &mut cash, &mut hold)
        } else {
            market.market.sell(good, &mut cash, &mut hold)
        };
        match result {
            Ok(()) => done += 1,
            Err(why) => {
                refusal = Some(why);
                break;
            }
        }
    }
    state.credits = u32::try_from(cash.clamp(0, 1_000_000_000)).unwrap_or(0);
    state.hold = hold.cargo[..NAMES].to_vec();
    let paid = (i64::from(before) - i64::from(cash)).abs();
    let paid = credits(paid as i32);
    Ok(match (done, refusal) {
        (0, Some(why)) => format!("{}: {}.", name, why.text()),
        (done, why) => {
            let verb = if buying { "Bought" } else { "Sold" };
            let tail = why.map_or(String::new(), |why: Refusal| {
                format!(" Then: {}.", why.text())
            });
            format!(
                "{verb} {done} t of {name} for {paid} CR (price {}).{tail}",
                credits(nav.market(cache, station)?.market.entries[good].price)
            )
        }
    })
}
