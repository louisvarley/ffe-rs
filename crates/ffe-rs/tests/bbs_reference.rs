//! Local-only: F622's draws before F688 against the executed original.
//!
//! The original's default start (`jjffe-smoke --npc-trace 260 0x39` with
//! `FFE_RNG_TRACE`) makes 1,711 F922 draws before F688 and enters it in the
//! shared state `a3e751f9 1df3aaed`. The port reaches the same state by drawing
//! every place's market steps, advertisements and ship queue, then the news
//! rounds, from the F147 seed. JJFFE's galaxy table has ten more (empty) systems
//! than the floppy's, which the comparison adds with `FFE_REFERENCE_GALAXY`.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test bbs_reference --
//! --ignored --nocapture`

use ffe_rs::navigation::Navigator;

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_default_start_enters_f688_in_the_originals_state() {
    std::env::set_var("FFE_REFERENCE_GALAXY", "1");
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.spawn_traffic(&cache, at, forward);
    let entry = nav.f688_entry.expect("F688 ran");
    assert_eq!((entry.a, entry.b), (0xa3e7_51f9, 0x1df3_aaed));
}

/// F688's own draws (165 more, after F622's 1,711): the default start's traffic
/// is the executed original's, ship for ship, and the pair leaves F688 in the
/// original's state `654ec13e 08573e56`. The original's object table at frame
/// 200 (`--npc-trace 260 0x39`) holds the trader (model 65), six bay ships
/// (59, 64, 45, 44, 44, 65), the aux carrier (71) and nine pirate ships
/// (36, 28, 48, 22, 43 and four of 23).
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_default_starts_traffic_is_the_originals() {
    std::env::set_var("FFE_REFERENCE_GALAXY", "1");
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.spawn_traffic(&cache, at, forward);
    assert_eq!((nav.random.a, nav.random.b), (0x654e_c13e, 0x0857_3e56));
    let mut models: Vec<u32> = nav.ships.iter().map(|ship| ship.model).collect();
    models.sort_unstable();
    let mut expected = vec![
        65, 59, 64, 45, 44, 44, 65, 71, 36, 28, 48, 22, 43, 23, 23, 23, 23,
    ];
    expected.sort_unstable();
    assert_eq!(models, expected);
}

/// Start type 1 (Ross 154, the original's key `1`): F622 and F688 together
/// take the pair from the F147 seed to `8eb9e749 d2cc2b66`, entering F688 in
/// `c1ff509f 1f7a567d`, as the executed original's trace does.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn ross_154_draws_the_originals_ledger() {
    std::env::set_var("FFE_REFERENCE_GALAXY", "1");
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.spawn_traffic(&cache, at, forward);
    let entry = nav.f688_entry.expect("F688 ran");
    assert_eq!((entry.a, entry.b), (0xc1ff_509f, 0x1f7a_567d));
    assert_eq!((nav.random.a, nav.random.b), (0x8eb9_e749, 0xd2cc_2b66));
}
