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
