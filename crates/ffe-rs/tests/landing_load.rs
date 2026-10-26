//! Reproduction: land on a planet, save, load, and check the ship stays on the
//! surface instead of falling through.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test landing_load -- --ignored --nocapture`

use ffe_core::objects::ModelInfo;
use ffe_core::surface::SurfaceTools;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{landing_body, Library, Navigator};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};
use ffe_rs::saves::{self, Commander, SaveGame};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn distance(a: [i64; 3], b: [i64; 3]) -> f64 {
    (0..3)
        .map(|k| ((a[k] - b[k]) as f64).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn tools<'a>(
    cache: &'a ffe_content::VerifiedCache,
    library: &'a Library<'a>,
) -> Option<SurfaceTools<'a>> {
    cache
        .asset("base:terrain/tables")
        .zip(cache.asset("base:terrain/maps"))
        .map(|(terrain_table, terrain_maps)| SurfaceTools {
            shapes: library,
            terrain_table,
            terrain_maps,
            detail: 0,
        })
}

/// Descend onto a planet with the real flight model until the ship lands, then
/// return the settled absolute position.
fn land_on_planet(
    cache: &ffe_content::VerifiedCache,
    nav: &Navigator,
    state: &Flight,
    planet: usize,
) -> [i64; 3] {
    let library = Library(cache);
    let tools = tools(cache, &library);
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let planet_body = nav.bodies[planet].clone();
    let planet_model = ModelInfo::read(records, stats, planet_body.model).unwrap();
    let ship_model = ModelInfo::read(records, stats, state.ship).unwrap();
    let mut model = OriginalFlight::from_flight(state, None);
    model.face(planet_body.position);
    for _ in 0..40_000 {
        let keys = Keys {
            slower: true,
            ..Keys::default()
        };
        let contact = model
            .step_landing(
                0.02,
                &keys,
                &landing_body(&planet_body, &planet_model),
                planet_body.position,
                &ship_model,
                tools.as_ref(),
            )
            .unwrap();
        if contact.flags & 0x80 == 0 && contact.flags & 0x40 != 0 {
            break;
        }
    }
    model.absolute_position()
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn landed_position_survives_save_and_load() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let port = nav.docked.unwrap();
    let planet = nav.bodies[port].parent.expect("a parent planet");
    let start = nav.new_game.unwrap();
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        position: start.seat.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT),
        axes: Some(nav.bodies[port].orientation),
        ..Flight::default()
    };

    let landed_at = land_on_planet(&cache, &nav, &state, planet);
    assert!(nav.land(planet), "a planet is a surface landing");
    let mut landed_state = state.clone();
    landed_state.position = landed_at.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT);
    landed_state.velocity = [0.0; 3];

    let planet_body = nav.bodies[planet].clone();
    let from_centre = distance(landed_at, planet_body.position);
    eprintln!(
        "landed {:.0} mm from centre; radii outer {} inner {}",
        from_centre,
        planet_body.radius,
        planet_body.inner_radius
    );

    // Save and reload the landed state.
    let save = SaveGame::new(
        Commander::default(),
        landed_state.clone(),
        Some(nav.save_state()),
    );
    let bytes = saves::encode_game(&save).expect("a landed save encodes");
    let loaded = saves::decode_game(&bytes).unwrap();
    let resumed = Navigator::from_save(&cache, loaded.navigation.as_ref().unwrap()).unwrap();
    assert_eq!(resumed.landed_body(), Some(planet));

    let ship_abs: [i64; 3] = std::array::from_fn(|k| {
        (loaded.state.position[k] * UNITS_PER_FLIGHT_UNIT).round() as i64
    });
    let resumed_planet = resumed.bodies[planet].clone();
    let resumed_from_centre = distance(ship_abs, resumed_planet.position);
    eprintln!(
        "loaded {resumed_from_centre:.0} mm from centre (planet moved {:.0} mm)",
        distance(planet_body.position, resumed_planet.position)
    );
    // The ship must still sit on the surface, between the inner and outer radii.
    assert!(
        resumed_from_centre > resumed_planet.inner_radius as f64 * 0.5,
        "ship fell to {resumed_from_centre:.0} mm from the centre (inner {})",
        resumed_planet.inner_radius
    );
    assert!(
        resumed_from_centre < resumed_planet.radius as f64 * 1.05 + 1_000_000.0,
        "ship drifted to {resumed_from_centre:.0} mm (outer {})",
        resumed_planet.radius
    );

    // Now reproduce the save-time staleness: the clock advances (and the body
    // positions move with it) after the last follow, but the ship's saved
    // position is the frozen one. A load regenerates the bodies at the saved
    // clock, so the ship is displaced by the planet's motion over that gap.
    let mut stale_nav = nav;
    let stale_ship = landed_at;
    let original_planet = stale_nav.bodies[planet].position;
    stale_nav.advance(&cache, 0.5);
    let moved_planet = stale_nav.bodies[planet].position;
    let planet_motion = distance(original_planet, moved_planet);
    let ship_original = distance(stale_ship, original_planet);
    let ship_stale = distance(stale_ship, moved_planet);
    eprintln!(
        "planet motion over 0.5 s: {planet_motion:.0} mm; \
         ship-to-centre was {ship_original:.0} mm, now {ship_stale:.0} mm \
         (delta {:.0} mm)",
        ship_stale - ship_original
    );
}
