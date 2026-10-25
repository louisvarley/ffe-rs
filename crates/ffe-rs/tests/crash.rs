//! Launch placement and surface-contact reproduction for the crash round.
//!
//! `FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test crash -- --ignored
//! --nocapture`

use ffe_core::objects::ModelInfo;
use ffe_core::surface::SurfaceTools;
use ffe_core::traffic::TakeoffStep;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{landing_body, Library, Navigator};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn distance(a: [i64; 3], b: [i64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let d = (*x as f64) - (y as f64);
            d * d
        })
        .sum::<f64>()
        .sqrt()
}

fn level_axes(up: [f64; 3]) -> [[f64; 3]; 3] {
    let reference = if up[1].abs() > 0.99 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let normalise = |v: [f64; 3]| {
        let l = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-12);
        v.map(|c| c / l)
    };
    let right = normalise(cross(reference, up));
    let forward = normalise(cross(up, right));
    [right, up, forward]
}

struct Launch {
    cache: ffe_content::VerifiedCache,
    nav: Navigator,
    state: Flight,
    model: OriginalFlight,
    info: ModelInfo,
    planet: usize,
    port: usize,
    seat: [i64; 3],
    up: [f64; 3],
}

fn launch() -> Launch {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let port = nav.docked.expect("landed at the start starport");
    let start = nav.new_game.unwrap();
    let body = nav.bodies[port].clone();
    let planet = body.parent.expect("a parent planet");
    nav.begin_launch().unwrap();
    let mut state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship: start.ship,
        ..Flight::default()
    };
    state.position = start.seat.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT);
    state.axes = Some(body.orientation);
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let info = ModelInfo::read(records, stats, start.ship).unwrap();
    let mut model = OriginalFlight::from_flight(&state, None);
    model.begin_takeoff();
    Launch {
        cache,
        nav,
        state,
        model,
        info,
        planet,
        port,
        seat: start.seat,
        up: body.orientation[1],
    }
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

/// The gap between the ship and the planet's outer radius (`+0x138`, the
/// atmosphere) and inner radius (`+0x140`, the collision surface).
fn gaps(l: &Launch, at: [i64; 3]) -> (f64, f64) {
    let planet = &l.nav.bodies[l.planet];
    let from = distance(at, planet.position);
    (
        from - planet.radius as f64,
        from - planet.inner_radius as f64,
    )
}

/// Run the playable's open-air takeoff loop until F1031 clears the pad. Returns
/// the mode words seen and the frames the climb and clear took.
fn take_off(l: &mut Launch, hold_set_speed: bool) -> (Vec<u8>, Option<usize>, Option<usize>) {
    take_off_with(l, hold_set_speed, false)
}

fn take_off_with(
    l: &mut Launch,
    hold_set_speed: bool,
    pitch_up: bool,
) -> (Vec<u8>, Option<usize>, Option<usize>) {
    let library = Library(&l.cache);
    let tools = tools(&l.cache, &library);
    let records = l.cache.asset("base:model/records").unwrap();
    let stats = l.cache.asset("base:ship/stats").unwrap();
    let port = l.nav.bodies[l.port].clone();
    let planet = l.nav.bodies[l.planet].clone();
    let planet_body = landing_body(
        &planet,
        &ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let port_body = landing_body(&port, &ModelInfo::read(records, stats, port.model).unwrap());
    l.model.begin_pad_takeoff(
        &planet_body,
        planet.position,
        &port_body,
        port.position,
        &l.info,
    );
    let mut seen = Vec::new();
    let (mut hand_off, mut cleared) = (None, None);
    for frame in 0..8_000 {
        if l.nav.takeoff.is_none() {
            break;
        }
        let climbing = l.nav.takeoff_state() == Some(ffe_core::traffic::state::TAKEOFF);
        let keys = Keys {
            faster: hold_set_speed && !climbing && l.model.set_speed_metres() < 500.0,
            pitch_up: pitch_up && !climbing,
            ..Keys::default()
        };
        let contact = if l.model.on_pad() {
            l.model.step_pad(0.02, &keys, tools.as_ref())
        } else {
            let at = l.model.absolute_position();
            let index = l.nav.landing_parent(at, l.info.radii().0).unwrap_or(l.port);
            let b = l.nav.bodies[index].clone();
            let bi = ModelInfo::read(records, stats, b.model).unwrap();
            l.model.step_landing(
                0.02,
                &keys,
                &landing_body(&b, &bi),
                b.position,
                &l.info,
                tools.as_ref(),
            )
        }
        .unwrap_or_else(|e| panic!("frame {frame}: {e}"));
        l.model.apply(&mut l.state);
        let at = l.model.absolute_position();
        let altitude = {
            let d: f64 = (0..3).map(|k| (at[k] - l.seat[k]) as f64 * l.up[k]).sum();
            d.clamp(0.0, f64::from(i32::MAX)) as i32
        };
        let before = l.nav.takeoff_state().unwrap();
        let step = l
            .nav
            .takeoff_step(altitude, l.model.set_speed_units(), contact.flags);
        if l.nav.takeoff_state() != Some(before) {
            seen.push(before);
        }
        match step {
            Some(TakeoffStep::HandOff) => {
                hand_off = Some(frame);
                l.model.set_mode(l.nav.takeoff_state().unwrap());
            }
            Some(TakeoffStep::Cleared) => {
                l.model.set_mode(ffe_core::traffic::state::FREE);
                if l.model.on_pad() {
                    l.model.end_pad_takeoff();
                }
                l.model.apply(&mut l.state);
                cleared = Some(frame);
                break;
            }
            Some(TakeoffStep::Retakeoff) => l.model.begin_takeoff(),
            None => {}
        }
    }
    (seen, hand_off, cleared)
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn launch_reaches_free_flight_outside_the_collision_surface() {
    let mut l = launch();
    let port = l.nav.bodies[l.port].clone();
    let planet = l.nav.bodies[l.planet].clone();
    eprintln!(
        "seat {} from the port centre, {} from the planet centre; port radii {}/{} planet radii {}/{}",
        distance(l.seat, port.position),
        distance(l.seat, planet.position),
        port.radius,
        port.inner_radius,
        planet.radius,
        planet.inner_radius
    );
    let (seen, hand_off, cleared) = take_off(&mut l, true);
    let at = l.model.absolute_position();
    let (outer, inner) = gaps(&l, at);
    eprintln!(
        "modes {seen:?} hand-off {hand_off:?} clear {cleared:?} at {at:?} gap outer {outer:.0} inner {inner:.0} \
         speed {:.2} units set {:.0} m/s",
        l.state.speed(),
        l.model.set_speed_metres()
    );
    assert_eq!(seen, vec![0x24, 0x26]);
    assert!(cleared.is_some(), "F1031 cleared the pad");
    assert!(
        inner > 0.0,
        "free flight starts {inner:.0} mm inside the collision surface"
    );
    // The playable's 120-frame grace after F1031's clear: no landing scan while
    // the ship leaves the starport. The planet's scan then runs while the ship is
    // still at pad altitude: a level flight into the local rise is a real
    // surface contact (F1013's fixed up-thrust is the only climb), not a
    // placement that starts the ship inside the surface.
    let library = Library(&l.cache);
    let tools = tools(&l.cache, &library);
    let records = l.cache.asset("base:model/records").unwrap();
    let stats = l.cache.asset("base:ship/stats").unwrap();
    let planet_body = landing_body(
        &planet,
        &ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let mut first = None;
    for frame in 0..400 {
        let contact = if frame < 120 {
            l.model.step(0.02, &Keys::default()).unwrap();
            None
        } else {
            l.model
                .step_landing(
                    0.02,
                    &Keys::default(),
                    &planet_body,
                    planet.position,
                    &l.info,
                    tools.as_ref(),
                )
                .ok()
        };
        l.model.apply(&mut l.state);
        let at = l.model.absolute_position();
        if frame >= 120 {
            assert!(
                gaps(&l, at).1 > 0.0,
                "frame {frame}: inside the collision surface"
            );
        }
        if let Some(contact) = contact.filter(|contact| contact.flags & 0xc0 != 0) {
            first = Some((frame, contact));
            break;
        }
    }
    let (frame, contact) = first.expect("the level flight meets the rise");
    eprintln!(
        "post-launch contact frame {frame} flags {:#04x} touch {:?} damage {}",
        contact.flags, contact.touch, contact.damage
    );
    assert!(frame >= 120, "no scan before the grace ends");
    assert_eq!(contact.flags & 0xc0, 0x80, "a fast level touch is a crash");
    assert!(contact.damage > 0, "the contact carries F953's amount");
    // F953 now takes that amount off the hull (the port used to log it only).
    let mut damaged = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    let ship_stats = ffe_rs::ships::ShipStats::from_block(&l.info.stats.unwrap()).unwrap();
    damaged.ensure_damage(ship_stats.mass * 4);
    let outcome = damaged.take_contact_damage(&contact, ship_stats.mass);
    eprintln!(
        "  F953 -> {outcome:?} hull {}/{}",
        damaged.hull[0], damaged.hull[1]
    );
    assert!(outcome.is_some(), "the crash damages the hull");
    assert!(damaged.hull[0] < damaged.hull[1]);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn steering_during_the_climb_still_hands_off() {
    let mut l = launch();
    let (seen, hand_off, cleared) = take_off_with(&mut l, false, true);
    let at = l.model.absolute_position();
    let (_, inner) = gaps(&l, at);
    eprintln!(
        "pitching climb: modes {seen:?} hand-off {hand_off:?} clear {cleared:?} gap inner {inner:.0} \
         mode {:#04x} altitude {}",
        l.model.player().mode,
        l.model.player().altitude
    );
    assert!(
        hand_off.is_some(),
        "F1013 must hand off even when the player pitches during the climb: {seen:?}"
    );
    assert!(
        seen.contains(&0x26) || cleared.is_some(),
        "the climb reaches 0x26: {seen:?}"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn surface_impacts_land_bounce_and_destroy() {
    use ffe_core::frames::{Bodies, Body, Words};
    use ffe_core::laser::Damage;

    let l = launch();
    let library = Library(&l.cache);
    let tools = tools(&l.cache, &library);
    let tools = tools.as_ref().expect("terrain tools");
    let records = l.cache.asset("base:model/records").unwrap();
    let stats = l.cache.asset("base:ship/stats").unwrap();
    let planet = l.nav.bodies[l.planet].clone();
    let planet_body = landing_body(
        &planet,
        &ModelInfo::read(records, stats, planet.model).unwrap(),
    );
    let ship_stats = ffe_rs::ships::ShipStats::from_block(&l.info.stats.unwrap()).unwrap();
    let port = l.nav.bodies[l.port].clone();
    let up = port.orientation[1];
    let (outer, inner) = l.info.radii();
    // The ship's row 1 is the surface up for a level touch (dot 0x7fffffff).
    let axes = level_axes(up);
    let orientation: [i32; 9] = std::array::from_fn(|k| to_q31(axes[k / 3][k % 3]));
    let world = |height: f64, velocity: [i32; 3], orientation: [i32; 9]| {
        let mut world = Bodies {
            stamp: 1,
            frame_tics: 994,
            player: 1,
            ..Bodies::default()
        };
        world.live[1] = true;
        world.physical[1] = true;
        let mut words = Words::default();
        words.0[1] = 0xffff;
        let at: [i64; 3] = std::array::from_fn(|k| l.seat[k] + (up[k] * height) as i64);
        world.body[1] = Body {
            position: std::array::from_fn(|k| at[k].wrapping_sub(planet.position[k])),
            velocity,
            parent: 2,
            index: 1,
            kind: 0,
            orientation,
            outer_radius: outer,
            inner_radius: inner,
            shape: l.info.class(),
            model: l.state.ship,
            words,
            ..Body::default()
        };
        let mut parent = planet_body;
        parent.position = planet.position;
        parent.parent = 0;
        parent.index = 2;
        world.body[2] = parent;
        world.live[2] = true;
        world.physical[2] = true;
        world
    };
    // Find the local terrain under the starport's radial line: descend in 1 m
    // steps until the moving ship's sphere touches.
    let mut surface = None;
    for step in 1..=2000 {
        let height = -f64::from(step) * 1000.0;
        let mut world = world(height, up.map(|v| (-v * 300.0) as i32), orientation);
        let contact = world.collide_with(1, Some(tools), 0xff).expect("scan");
        if contact.flags & 0xc0 != 0 {
            surface = Some(height);
            break;
        }
    }
    let surface = surface.expect("the terrain below the starport");
    eprintln!("the terrain under the port is {surface:.0} mm above its centre");
    for (units, tilted, expect) in [
        (300, false, "land"),
        (300, true, "bounce"),
        (900, true, "bounce"),
        (2400, false, "crash"),
        (4000, false, "crash"),
    ] {
        let mut orientation = orientation;
        if tilted {
            // Pitch the ship's up row 30 degrees off the surface: the soft-landing
            // dot test fails and the touch is a crash (F575's bounce amount is
            // then the mass floor).
            let (sin, cos) = (0.5f64, 0.866f64);
            let right = axes[0];
            orientation[3] = to_q31(axes[1][0] * cos + right[0] * sin);
            orientation[4] = to_q31(axes[1][1] * cos + right[1] * sin);
            orientation[5] = to_q31(axes[1][2] * cos + right[2] * sin);
        }
        let velocity = up.map(|v| (-v * units as f64) as i32);
        let mut world = world(surface, velocity, orientation);
        let contact = world.collide_with(1, Some(tools), 0xff).expect("contact");
        let total = contact.damage;
        eprintln!(
            "impact at {units} units (tilted {tilted}): flags {:#04x} touch {:?} damage {total}",
            contact.flags, contact.touch
        );
        // A slow level touch lands (0x40); a tilted or fast one crashes (0x80).
        match expect {
            "land" => assert_eq!(contact.flags & 0xc0, 0x40, "{units} units must land"),
            _ => {
                assert_eq!(contact.flags & 0xc0, 0x80, "{units} units must crash");
                assert!(total > 0, "{units}: F953's amount");
            }
        }
        // F953 on the Saker's words: shields (none), then the 112 t hull.
        let mut damaged = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        damaged.ensure_damage(ship_stats.mass * 4);
        let result = damaged.take_contact_damage(&contact, ship_stats.mass);
        eprintln!(
            "  F953 -> {result:?} hull {}/{} destroyed {}",
            damaged.hull[0],
            damaged.hull[1],
            damaged.destroyed()
        );
        if total < i64::from(ship_stats.mass * 4) {
            assert_ne!(result, Some(Damage::Destroyed), "{units}: not lethal");
            assert!(!damaged.destroyed());
        } else {
            assert_eq!(result, Some(Damage::Destroyed), "{units}: lethal");
            assert!(damaged.destroyed());
            // F942/F1104: the default Saker has no escape craft, so the death
            // hold starts and reaches F917's game-over tombstone.
            let outcome = ffe_rs::death::begin(&mut damaged, None);
            assert_eq!(outcome, ffe_rs::death::Outcome::Destroyed);
            let death = damaged.death.expect("F1104's hold");
            assert!(!death.is_game_over());
            let mut death = death;
            death.tick(ffe_rs::death::GAME_OVER_SECONDS + 0.001);
            assert!(death.is_game_over(), "F917's tombstone follows the hold");
        }
    }
}

fn to_q31(value: f64) -> i32 {
    (value * 2_147_483_647.0)
        .round()
        .clamp(-2_147_483_648.0, 2_147_483_647.0) as i32
}
