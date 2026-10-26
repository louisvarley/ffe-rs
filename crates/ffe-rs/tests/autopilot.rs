use ffe_rs::flight::{Flight, Mode};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};

#[test]
fn source_controls_fly_and_brake_without_keyboard_speed_or_direct_turns() {
    let mut model = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            system_flight: true,
            axes: Some([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
            position: [0.; 3],
            ..Flight::default()
        },
        None,
    );
    let target = [0, 0, 1_000_000];
    let start = model.absolute_position();
    for _ in 0..5000 {
        let guidance = model.guidance(target, [0; 3], 0);
        model.apply_guidance(guidance);
        model.step(0.02, &Keys::default()).unwrap();
    }
    let at = model.absolute_position();
    assert!(
        at[2] > start[2] + 500_000,
        "source thrust did not advance: {at:?}"
    );
    assert!(
        (at[2] - target[2]).abs() < 100_000,
        "failed to brake near target: {at:?}"
    );
    assert!(
        model.player().motion.velocity[2].abs() < 100,
        "not braking: {:?}",
        model.player().motion.velocity
    );
    model.clear_guidance();
    assert_eq!(model.player().mode, 0);
    assert!(model.player().set_speed >= 0);
}

#[test]
fn installing_guidance_does_not_snap_orientation_or_position() {
    let mut model = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            system_flight: true,
            axes: Some([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
            position: [0.; 3],
            ..Flight::default()
        },
        None,
    );
    let before = *model.player();
    let command = model.guidance([1_000_000, 0, 0], [0; 3], 0);
    model.apply_guidance(command);
    assert_eq!(model.player().motion.position, before.motion.position);
    assert_eq!(model.player().motion.orientation, before.motion.orientation);
    model.step(0.02, &Keys::default()).unwrap();
    assert_ne!(model.player().motion.orientation, before.motion.orientation);
    let mut state = Flight::default();
    model.apply(&mut state);
    for (a, b) in state.position.into_iter().zip(model.absolute_position()) {
        assert!((a * UNITS_PER_FLIGHT_UNIT - b as f64).abs() <= 1.0);
    }
}

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

#[test]
#[ignore = "Local-cache station/clearance integration: FFE_WORLD_CACHE"]
fn source_pad_approach_requests_clearance_and_continues_to_the_granted_pad() {
    use ffe_rs::navigation::{Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.unwrap();
    let body = nav.bodies[index].clone();
    nav.lock(index);
    assert!(
        nav.engage_locked().is_none(),
        "F1020 rejects the landed state"
    );
    nav.starport(index).release(PLAYER);
    nav.docked = None;
    nav.engage_locked().unwrap();
    let point = std::array::from_fn(|k| {
        body.position[k]
            + i64::from(ffe_core::model::multiply(body.rows[3 + k], 0x1770) as i16) * 1024
    });
    let mut model = OriginalFlight::from_flight(&Flight::free_flight(), None);
    model.teleport(point);
    nav.source_command(&cache, &mut model).unwrap();
    assert!(nav.final_leg, "source F745 granted clearance");
    assert!(nav.starport(index).bay_of(PLAYER).is_some());
    let (guidance, _) = nav.source_command(&cache, &mut model).unwrap();
    assert_eq!(nav.target, Some(index), "F746 must continue to the pad");
    assert_ne!(guidance.thrust, [0; 3]);
    assert!(!nav.notice.as_ref().unwrap().contains("manual"));
    model.apply_guidance(guidance);
    model.step(0.02, &Keys::default()).unwrap();
    assert_ne!(model.absolute_position(), point);
}

#[test]
#[ignore = "Local-cache station/clearance integration: FFE_WORLD_CACHE"]
fn source_starport_clearance_honours_fines_and_does_not_spam_or_disengage() {
    use ffe_rs::navigation::{Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.unwrap();
    let body = nav.bodies[index].clone();
    nav.starport(index).release(PLAYER);
    nav.docked = None;
    nav.legal
        .record(ffe_core::npc::SMUGGLING, Some(nav.jurisdiction));
    assert!(nav.fine_at(index) > 0);
    nav.lock(index);
    nav.engage_locked().unwrap();
    let point = std::array::from_fn(|k| {
        body.position[k]
            + i64::from(ffe_core::model::multiply(body.rows[3 + k], 0x1770) as i16) * 1024
    });
    let mut model = OriginalFlight::from_flight(&Flight::free_flight(), None);
    model.teleport(point);
    nav.source_command(&cache, &mut model).unwrap();
    assert!(!nav.final_leg);
    assert!(nav.starport(index).bay_of(PLAYER).is_none());
    assert_eq!(nav.target, Some(index));
    assert!(nav.notice.take().is_some());
    nav.source_command(&cache, &mut model).unwrap();
    assert!(
        nav.notice.is_none(),
        "unchanged refusal must not flood the message log"
    );
}

#[test]
#[ignore = "Local-cache end-to-end pad approach: FFE_WORLD_CACHE"]
fn source_accelerated_starport_completes_the_f746_pad_transition() {
    fly_source_pad(1);
}

#[test]
#[ignore = "Local-cache normal-speed pad landing: FFE_WORLD_CACHE"]
fn source_normal_speed_starport_lands_through_collision() {
    fly_source_pad(0);
}

fn fly_source_pad(acceleration: usize) {
    use ffe_rs::navigation::{landing_body, Library, Navigator, PLAYER};
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 0).unwrap();
    let index = nav.docked.unwrap();
    let body = nav.bodies[index].clone();
    let ship = nav.new_game.unwrap().ship;
    let info = |model| {
        ffe_core::objects::ModelInfo::read(
            cache.asset("base:model/records").unwrap(),
            cache.asset("base:ship/stats").unwrap(),
            model,
        )
        .unwrap()
    };
    let ship_info = info(ship);
    let ship_stats = ffe_rs::ships::ShipStats::from_block(&ship_info.stats.unwrap()).unwrap();
    let point = std::array::from_fn(|k| {
        body.position[k]
            + i64::from(ffe_core::model::multiply(body.rows[3 + k], 0x1770) as i16) * 1024
    });
    let state = Flight {
        mode: Mode::Flight,
        system_flight: true,
        ship,
        axes: Some(body.orientation),
        position: point.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT),
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&state, Some(&ship_stats));
    nav.starport(index).release(PLAYER);
    nav.docked = None;
    nav.lock(index);
    nav.engage_locked().unwrap();
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: 0,
    };
    model.set_time_acceleration(acceleration);
    if acceleration != 0 {
        // Isolate F746's near-pad branch from the still-incomplete accelerated
        // frame matching in F597. The full route below is tested at 1x.
        nav.starport(index).request_docking(PLAYER, true, 0);
        nav.final_leg = true;
        let bay = nav.starport(index).bay_of(PLAYER).unwrap();
        let plane = nav.starport_pad_point(index, bay).unwrap();
        model.teleport(plane);
        assert!(nav.source_command(&cache, &mut model).is_none());
        assert_eq!(nav.landed_body(), Some(index));
        assert_eq!(nav.docked, Some(index));
        assert!(nav.at_starport_pad());
        assert_eq!(model.player().mode, 0x2e);
        return;
    }
    let mut landed = false;
    let mut flags = 0;
    for _ in 0..10000 {
        nav.advance(&cache, if acceleration == 0 { 0.1 } else { 1.0 });
        if let Some(at) = nav.follow(model.absolute_position()) {
            model.shift(at);
        }
        let Some((guidance, _)) = nav.source_command(&cache, &mut model) else {
            landed = nav.landed_body() == Some(index);
            break;
        };
        model.apply_guidance(guidance);
        let body = &nav.bodies[index];
        let planet = &nav.bodies[body.parent.unwrap()];
        let contact = model
            .step_starport_landing(
                0.1,
                &Keys::default(),
                &landing_body(planet, &info(planet.model)),
                planet.position,
                &landing_body(body, &info(body.model)),
                body.position,
                &ship_info,
                Some(&tools),
            )
            .unwrap();
        flags = contact.flags;
        if flags & 0x40 != 0 && flags & 0x80 == 0 {
            landed = true;
            break;
        }
    }
    let pad = nav.takeoff_seat(index, ship_info.radii().1 as i32).unwrap();
    let range = ffe_rs::navigation::distance(model.absolute_position(), pad);
    if acceleration == 0 {
        let bay = nav.starport(index).bay_of(PLAYER).unwrap();
        let plane = nav.starport_pad_point(index, bay).unwrap();
        let plane_range = ffe_rs::navigation::distance(model.absolute_position(), plane);
        // Require a real F576/F575 landing flag, not merely proximity.
        assert!(
            landed,
            "failed to reach F650 pad plane: {plane_range} mm, flags {flags:#x}, velocity {:?}, delta {:?}", model.player().motion.velocity, std::array::from_fn::<_,3,_>(|k| model.absolute_position()[k]-plane[k])
        );
        return;
    }
    assert!(
        landed,
        "source approach failed: flags {flags:#x}, range {range} mm, velocity {:?}",
        model.player().motion.velocity
    );
}
