//! PAR-17: the game's one shared random pair (`DATA_008818`/`DATA_008820`).
//! Traffic, dust and the ships' rolls advance the navigator's pair in call
//! order, a save carries it, and two identical runs draw identically.
//! Local-only: needs `FFE_WORLD_CACHE`.
use ffe_core::dust;
use ffe_core::npc::Random;
use ffe_rs::navigation::Navigator;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// A new game with the start port's traffic, run for `frames` 20 ms frames
/// in flight: the pair at the end and the ships' positions.
fn run(cache: &ffe_content::VerifiedCache, frames: u32) -> (Navigator, Random, Vec<[i64; 3]>) {
    let mut nav = Navigator::new_game(cache, 1).unwrap();
    let at = nav.station().map(|(_, at)| at).unwrap();
    let forward = nav.bodies[nav.docked.unwrap()].orientation[2];
    nav.respawn_traffic(cache, at, forward);
    nav.docked = None;
    let player = [at[0], at[1], at[2] + 50_000_000];
    let spawn = dust::Spawn {
        position: player,
        velocity: [0; 3],
        altitude: i32::MAX,
        detail: true,
        flight_active: true,
        time_running: true,
        speed: 0,
    };
    for _ in 0..frames {
        nav.tick_ships(0.02, 994, player, 10_000);
        nav.tick_dust(0.02, &spawn);
        nav.tick_effects(0.02, 994);
    }
    let random = nav.random;
    let ships = nav.ships.iter().map(|ship| ship.position).collect();
    (nav, random, ships)
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_world_draws_from_one_pair_in_call_order() {
    let cache = cache();
    // F147 seeds a new game and the start's set-up draws from it: two new
    // games begin from the same words.
    let fresh = Navigator::new_game(&cache, 1).unwrap();
    assert_eq!(fresh.random, Navigator::new_game(&cache, 1).unwrap().random);

    let (nav, first, ships) = run(&cache, 500);
    assert_ne!(first, Random::start(), "nothing drew from the shared pair");
    // The same input gives the same draws and the same world.
    let (_, second, again) = run(&cache, 500);
    assert_eq!(first, second);
    assert_eq!(ships, again);

    // A save carries the pair.
    let save = nav.save_state();
    assert_eq!(save.random, Some([first.a, first.b]));
    let restored = Navigator::from_save(&cache, &save).unwrap();
    assert_eq!(restored.random, first);
}
