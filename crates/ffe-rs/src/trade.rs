//! Buying and selling at a station in the playable: the original's markets
//! (`ffe_core::market`) against the player's cash (`Flight::credits`, the
//! original's units: 1000 is 100.0 credits) and hold (`Flight::hold`).
use crate::flight::Flight;
use crate::navigation::Navigator;
use ffe_content::VerifiedCache;
use ffe_core::market::{Hold, GOODS, ILLEGAL, NAMES};
use ffe_core::npc::{self, OffenceReport};

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

/// The player's hold as the market code sees it.
pub fn hold_of(state: &Flight, capacity: i32) -> Hold {
    let mut hold = Hold::new(capacity);
    for (slot, tonnes) in hold.cargo.iter_mut().zip(&state.hold) {
        *slot = *tonnes;
    }
    hold.used = state.hold.iter().map(|t| i32::from(*t)).sum();
    hold
}

/// The market screen's lines for `station`: the legal goods, in the original's
/// order. The black market (F454's `0x80` mask) uses [`listing_with`] instead.
pub fn listing(
    nav: &mut Navigator,
    cache: &VerifiedCache,
    station: usize,
    state: &Flight,
) -> Result<Vec<Listing>, String> {
    listing_with(nav, cache, station, state, false)
}

/// [`listing`] with F538's illegal-goods mask: `show_illegal` is the `0x80`
/// bit F454 passes to F537, so the fake/real smuggler advertisement's screen
/// lists the bit-7 commodities as well.
pub fn listing_with(
    nav: &mut Navigator,
    cache: &VerifiedCache,
    station: usize,
    state: &Flight,
    show_illegal: bool,
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
        .filter(|good| show_illegal || flags[*good] & ILLEGAL == 0)
        .map(|good| Listing {
            good,
            name: names[good].clone(),
            price: market.market.entries[good].price,
            stock: market.market.entries[good].stock,
            held: state.hold.get(good).copied().unwrap_or(0),
        })
        .collect())
}

/// Whether this system's stock string marks any commodity illegal (F639/F640
/// only place a smuggler advertisement when one exists).
pub fn illegal_available(nav: &Navigator, cache: &VerifiedCache) -> Result<bool, String> {
    Ok(nav
        .stock_flags(cache)?
        .iter()
        .any(|flags| flags & ILLEGAL != 0))
}

/// F542/F543: a row action at F640's fake smuggler advertisement. The offence
/// is F237(5) ("Trading in illegal goods") and F543 charges `0x1388` (500.0
/// credits) to the starport's ledger; F554 returns before F555/F556, so no
/// tonne moves.
pub fn police_sting(nav: &mut Navigator, station: usize) -> OffenceReport {
    let ordinal = nav.starport_ordinal(station);
    let report = nav
        .legal
        .record(npc::TRADING_ILLEGAL, Some(ordinal))
        .unwrap_or(OffenceReport {
            message: String::new(),
            fine: 0,
            charged: 0,
            additional: false,
        });
    nav.legal.charge(npc::POLICE_FINE, Some(ordinal));
    OffenceReport {
        message: format!(
            "Trading in illegal goods is not permitted here. Pay a ${} fine.",
            credits(npc::POLICE_FINE as i32)
        ),
        charged: report.charged.saturating_add(npc::POLICE_FINE),
        ..report
    }
}

/// Buy (or sell) up to `count` tonnes of `good` at `station`, stopping at the first
/// refusal. Returns the original's console message (`0x98e1..0x98e7`): a refusal
/// names the commodity, otherwise a completed trade says "Buying <good>." or
/// "Selling <good>.".
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
    let capacity = nav.cargo_space(cache, state.ship);
    let name = nav
        .market_tables()
        .and_then(|tables| tables.name(good).map(str::to_owned))
        .ok_or("no market tables")?;
    let mut hold = hold_of(state, capacity);
    let mut cash = i32::try_from(state.credits).unwrap_or(i32::MAX);
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
    Ok(match (done, refusal) {
        (_, Some(why)) => why.message(&name),
        (0, None) => String::new(),
        (_, None) if buying => format!("Buying {name}."),
        (_, None) => format!("Selling {name}."),
    })
}
