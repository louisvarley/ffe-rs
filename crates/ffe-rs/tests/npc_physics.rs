//! NPC ships flying by the original's object physics (F597 guidance, F601
//! thrust within the statistics' limits, F585 turns, F1512 moves). Local-only:
//! needs `FFE_WORLD_CACHE`.
use ffe_core::npc::{self, Random, State};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// A hostile ship of `model` 20 km from a still player: the seconds it takes to
/// come within 5 km, its top speed (m/s) and whether it was ever in the pass.
fn intercept(model: u32) -> (f64, f64, bool) {
    let cache = cache();
    let spec = ffe_rs::ships::npc_spec(&cache, model).expect("a ship");
    assert!(spec.limits.is_some(), "statistics give the limits");
    let mut rng = Random::new(1, 2);
    let mut ship = npc::hyperspace_arrival(&spec, 0, [0, 0, 20_000_000], [0; 3], 0, &mut rng);
    ship.station = None;
    ship.state = State::Attack;
    ship.hostile = true;
    ship.forward = [0.0, 0.0, -1.0];
    let player = [0i64; 3];
    let (mut closed, mut top, mut passed) = (None, 0.0f64, false);
    let mut requests = Vec::new();
    for frame in 0..6000 {
        ship.tick_with_station(0.02, 994, player, None, None, &mut requests);
        top = top.max(ship.speed / 1000.0);
        passed |= ship.state == State::Pass;
        let range = ship
            .position
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        if closed.is_none() && range < 5.0e6 {
            closed = Some(f64::from(frame) * 0.02);
        }
    }
    (closed.unwrap_or(f64::INFINITY), top, passed)
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn pirates_close_under_their_own_thrust() {
    // A Cobra Mk III (0x24) and a heavier freighter-class hull (0x2c).
    for model in [0x24, 0x2c] {
        let (closed, top, passed) = intercept(model);
        eprintln!("model {model:#x}: within 5 km after {closed:.1} s, top speed {top:.0} m/s, pass {passed}");
        assert!(closed.is_finite(), "model {model:#x} never closed");
        assert!(passed, "model {model:#x} never broke off");
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn f750s_alert_fires_as_a_hostile_enters_combat_range() {
    let cache = cache();
    let spec = ffe_rs::ships::npc_spec(&cache, 0x24).expect("a ship");
    let mut rng = Random::new(1, 2);
    let mut ship = npc::hyperspace_arrival(&spec, 0, [0, 0, 20_000_000], [0; 3], 0, &mut rng);
    ship.station = None;
    ship.state = State::Attack;
    ship.hostile = true;
    ship.forward = [0.0, 0.0, -1.0];
    let mut requests = Vec::new();
    let mut first = None;
    for _ in 0..6000 {
        ship.tick_with_station(0.02, 994, [0; 3], None, None, &mut requests);
        let range = ship
            .position
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        if ship.take_alert() && first.is_none() {
            first = Some(range);
        }
    }
    let first = first.expect("F750's alert never fired");
    assert!(
        first < npc::COMBAT_RANGE,
        "alert at {first} mm, outside 2^23"
    );
}
