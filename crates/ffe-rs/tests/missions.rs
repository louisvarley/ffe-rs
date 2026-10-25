//! The mission slice's scripted run: an offer is accepted, the contract is
//! settled at its destination, a late one is failed, and the whole state
//! round-trips through the save format. The cache-gated test generates a real
//! BBS post and reads it back through `Navigator::mission_offer`.
use ffe_core::bulletin::{Destination, Post, Var, World};
use ffe_core::missions::{self, Kind, PageKind, Status};
use ffe_rs::flight::Flight;
use ffe_rs::navigation::Navigator;

fn world<'a>(destinations: &'a [Destination]) -> World<'a> {
    World {
        code: 1,
        name: "Ross 154",
        population: 9,
        danger: 0,
        allegiance: 2,
        station_name: "Sirocco Station",
        destinations,
        names: None,
    }
}

fn passenger_post(money: i64, destination: u32) -> Post {
    Post {
        code: 0x981a,
        fake: false,
        day: 0,
        vars: [
            Var::Int(money),
            Var::Text("passage for two people on a fast ship".to_owned()),
            Var::System(destination),
            Var::Seed(77),
            // Two passengers, additive 3, no package bit.
            Var::Int((3 << 9) | 1),
        ],
        text: "WANTED: passage".to_owned(),
    }
}

#[test]
fn a_scripted_passenger_contract_pays_and_journals() {
    let destinations = [Destination {
        code: 0x1234_5678,
        name: "Sol".to_owned(),
        population: 12,
        danger: 1,
    }];
    let offer = missions::offer(&passenger_post(1500, 0x1234_5678), &world(&destinations))
        .expect("a contract offer");
    assert_eq!(offer.kind, Kind::Passenger);
    assert_eq!(offer.passengers, 2);

    let mut state = missions::State::default();
    let mut cash_tenths = 1000u32;
    let accepted = state.accept(&offer, 4, 1, "Ross 154").unwrap();
    assert_eq!(
        accepted.text,
        "Excellent. $1500 will be paid on arrival at the Sol system."
    );
    assert_eq!(state.journal.len(), 1);
    assert_eq!(state.pages.len(), 1);

    // Arrival at Sol three days later: F303 pays the whole-credit reward into
    // the tenths cash field and writes the "thanks for bringing us" page.
    for outcome in state.tick(7, 0x1234_5678) {
        cash_tenths = cash_tenths.saturating_add(outcome.reward.saturating_mul(10));
        assert_eq!(outcome.status, Status::Complete);
    }
    assert_eq!(cash_tenths, 1000 + 1500 * 10);
    assert_eq!(state.reputation, 1);
    assert_eq!(state.missions[0].status, Status::Complete);
    assert!(state
        .journal
        .iter()
        .any(|entry| entry.text.contains("Thanks for bringing us here")));
    assert_eq!(state.pages.len(), 2, "acceptance and completion pages");
    assert_eq!(state.pages[1].kind, PageKind::Message);
}

#[test]
fn a_late_contract_is_expired_and_an_abandoned_one_loses_reputation() {
    let destinations = [Destination {
        code: 0x1234_5678,
        name: "Sol".to_owned(),
        population: 12,
        danger: 1,
    }];
    let mut state = missions::State::default();
    let offer = missions::offer(&passenger_post(1500, 0x1234_5678), &world(&destinations)).unwrap();
    state.accept(&offer, 0, 1, "Ross 154").unwrap();
    state.reputation = 3;
    // The deadline is accept day + 0x1c.
    let outcomes = state.tick(missions::ACCEPT_DAYS + 1, 0x1234_5678);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].status, Status::Expired);
    assert_eq!(outcomes[0].reward, 0);
    assert_eq!(state.reputation, 2);
    assert!(outcomes[0].journal.text.contains("too late"));

    let mut state = missions::State::default();
    state.accept(&offer, 0, 1, "Ross 154").unwrap();
    state.reputation = 4;
    let outcome = state.abandon(1, 1).unwrap();
    assert_eq!(outcome.status, Status::Abandoned);
    assert_eq!(state.reputation, 3);
    assert!(outcome
        .journal
        .text
        .contains("reporting you to the authorities"));
}

#[test]
fn the_flight_save_keeps_contracts_and_old_saves_default_them() {
    let destinations = [Destination {
        code: 0x1234_5678,
        name: "Sol".to_owned(),
        population: 12,
        danger: 1,
    }];
    let mut flight = Flight::default();
    let offer = missions::offer(&passenger_post(1500, 0x1234_5678), &world(&destinations)).unwrap();
    flight.missions.accept(&offer, 2, 1, "Ross 154").unwrap();
    let json = ffe_rs::saves::encode(&flight).unwrap();
    let loaded = ffe_rs::saves::decode(&json).unwrap();
    assert_eq!(loaded.missions, flight.missions);
    assert_eq!(loaded.missions.missions.len(), 1);

    // A save from before the mission slice has no `missions` member; the
    // `#[serde(default)]` on the field gives it an empty state.
    let old = serde_json::json!({
        "format": "ffe-rs-experimental",
        "version": 1,
        "state": {
            "mode": "Docked",
            "position": [0.0, 0.0, -60.0],
            "velocity": [0.0, 0.0, 0.0],
            "yaw": 0.0,
            "pitch": 0.0,
            "credits": 1000,
            "cargo": 0,
            "stock": 50,
            "ticks": 0,
            "docking_requested": false
        }
    });
    let old = serde_json::to_vec(&old).unwrap();
    let loaded = ffe_rs::saves::decode(&old).unwrap();
    assert!(loaded.missions.missions.is_empty());
    assert!(loaded.missions.journal.is_empty());
}

/// Local-only: the military package list is a source-derived offer, and its
/// completion produces F307's medal page (song 10) and F306's rank page
/// (song 7).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_military_package_offer_is_reachable_and_completes() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let offers = nav
        .military_offers(&cache, station, 0)
        .expect("F493's list");
    assert_eq!(offers.len(), 1, "only the first tier at rank 0");
    let offer = offers[0].clone();
    assert!(offer.text.contains("Deliver"));
    assert!(offer.reward >= 150);
    let mut state = missions::State::default();
    state
        .accept(
            &offer,
            nav.day(),
            nav.current_system_code(),
            nav.system_name(),
        )
        .unwrap();
    // Jump the clock past the deadline? No: settle at the destination in time.
    let outcome = state
        .settle(1, nav.day() + 1, offer.destination)
        .expect("the delivery completes");
    assert_eq!(outcome.status, Status::Complete);
    assert_eq!(outcome.reward, offer.reward);
    let rank = state
        .pages
        .iter()
        .find(|page| page.kind == PageKind::Rank)
        .expect("F306's rank page");
    assert!(rank.text.contains("promotion"));
    assert!(
        !state.pages.iter().any(|page| page.kind == PageKind::Medal),
        "a package delivery has no medal (DATA_005115[0] is 0xff)"
    );
}

/// Local-only: the generated board must contain a contract offer that reads
/// back with its destination/reward/deadline fields from the real tables.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_generated_bbs_post_becomes_a_contract() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    let station = nav.docked.expect("docked at the start");
    let posts = nav
        .station_offers(&cache, station)
        .expect("F622's BBS")
        .posts
        .clone();
    let (row, _) = posts
        .iter()
        .enumerate()
        .find(|(_, post)| matches!(post.code, 0x981a | 0x981b | 0x981f..=0x9822))
        .expect("a contract offer on the board");
    let offer = nav
        .mission_offer(&cache, station, row)
        .expect("the offer reads")
        .expect("a contract");
    assert!(offer.reward > 0, "{offer:?}");
    assert_ne!(offer.destination_name, "");
    let mut state = missions::State::default();
    let accepted = state
        .accept(
            &offer,
            nav.day(),
            nav.current_system_code(),
            nav.system_name(),
        )
        .unwrap();
    assert!(accepted.text.contains(&offer.destination_name));
    // F535 clears the advertisement; the board shrinks and the row is gone.
    let before = posts.len();
    nav.clear_post(&cache, station, row).unwrap();
    let after = nav.station_offers(&cache, station).unwrap().posts.len();
    assert_eq!(after, before - 1);
}
