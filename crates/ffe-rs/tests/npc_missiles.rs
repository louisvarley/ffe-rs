//! Local-only scripted NPC missile encounter: F754/F941's launch, F1054/F1057's
//! chaff, F757/F935's NPC ECM and F718/F720's player hit.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! npc_missiles -- --ignored --nocapture`
//!
//! Frames: `npc-missile-launch.ppm` (the pirate's missile in flight),
//! `npc-missile-chaff.ppm` (the same missile pulled to the chaff object) and
//! `npc-missile-hit.ppm` (a second missile's F953 hit on the player).
use ffe_core::equipment::{CHAFF_BIT, ECM_BIT};
use ffe_core::market::{CHAFF, NAMES};
use ffe_core::missile::{self, PLAYER_TARGET};
use ffe_core::npc::{Role, State};
use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::missile::{MissileEvent, MissileField};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, ShipEvent, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemMissile, SystemShip};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn save(screen: &Screen, name: &str) {
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for pixel in screen.logical_pixels() {
            ppm.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
    }
}

fn changed(before: &[u32], after: &[u32]) -> usize {
    before.iter().zip(after).filter(|(a, b)| a != b).count()
}

/// Pixels changed in the 80x60 centre where an object 40 m ahead draws.
fn centre_changed(before: &[u32], after: &[u32]) -> usize {
    let mut count = 0;
    for y in 70..130 {
        for x in 120..200 {
            if before[y * 320 + x] != after[y * 320 + x] {
                count += 1;
            }
        }
    }
    count
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
    v.map(|x| x / length)
}

fn look_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let forward = normalise(std::array::from_fn(|i| to[i] - from[i]));
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    [right, up, forward]
}

fn draw(screen: &mut Screen, nav: &Navigator, state: &Flight, viewer: &ModelViewer) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.ships = nav
        .ships
        .iter()
        .map(|ship| SystemShip {
            model: ship.model,
            centre: ship.position.map(|v| v as f64 / unit),
            axes: ship.axes(),
            flash: ship.flash > 0.0,
            dead: !ship.alive(),
        })
        .collect();
    screen.missiles = nav
        .missiles
        .missiles
        .iter()
        .filter(|missile| missile.alive())
        .map(|missile| SystemMissile {
            model: missile.spec.model,
            centre: missile.position.map(|v| v as f64 / unit),
            axes: missile.axes,
        })
        .collect();
    screen.effects = nav
        .decoys
        .iter()
        .map(|decoy| ffe_rs::screen::SystemEffect {
            model: missile::DECOY_MODEL,
            centre: decoy.position.map(|v| v as f64 / unit),
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            timer: 0,
            size: 0,
            scale: None,
        })
        .collect();
    screen.draw(state, ViewState::default(), Some(viewer), false, "");
}

fn spec_for(cache: &ffe_content::VerifiedCache, viewer: &ModelViewer, code: u8) -> missile::Spec {
    let model = missile::model(code).unwrap();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let radius =
        ffe_core::objects::ModelInfo::read(records, stats, model).map_or(0, |info| info.radii().1);
    MissileField::spec(code, viewer.ship_stats(model), radius).unwrap()
}

/// The scripted pirate's current list index (despawning traffic compacts).
fn pirate_index(nav: &Navigator, id: u32) -> usize {
    nav.ships
        .iter()
        .position(|ship| ship.id == id)
        .expect("the scripted pirate is still in the system")
}

fn pirate_ref(nav: &Navigator, id: u32) -> &ffe_core::npc::NpcShip {
    &nav.ships[pirate_index(nav, id)]
}

fn pirate_mut(nav: &mut Navigator, id: u32) -> &mut ffe_core::npc::NpcShip {
    let index = pirate_index(nav, id);
    &mut nav.ships[index]
}

fn player_state(at: [i64; 3], look: [i64; 3]) -> Flight {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let from = at.map(|v| v as f64 / unit);
    let mut state = Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some(look_at(from, look.map(|v| v as f64 / unit)));
    state
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn npc_launches_chaff_pulls_and_ecm_destroys() {
    let cache = cache();
    // F1054's ammo source: player+0x174 is cargo[32], and cache good 32 is the
    // source's "Chaff" string (DATA_006248).
    let market = ffe_core::market::Tables::new(cache.asset("base:market/tables").unwrap()).unwrap();
    assert_eq!(market.name(CHAFF), Some("Chaff"));
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let at = nav.station().map(|(_, at)| at).expect("a start station");
    let forward = nav.bodies[nav.docked.expect("docked")].orientation[2];
    nav.respawn_traffic(&cache, at, forward);
    let pirate = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .expect("the capture spawns a pirate");
    let pirate_id = nav.ships[pirate].id;
    // F773 always fits some pylons when the model has mounts and cargo; force
    // one known KL760 so the scripted launch is stable.
    pirate_mut(&mut nav, pirate_id).pylons = [0x82; missile::PYLONS];
    pirate_mut(&mut nav, pirate_id).general = ECM_BIT;

    // 1,000 km: F610 exponent 5..8, F754's launch band.
    let target = pirate_ref(&nav, pirate_id).position;
    let player = [target[0], target[1], target[2] + 1_000_000];
    let mut state: Flight;
    let mut launch = None;
    for _ in 0..600 {
        let events = nav.tick_ships(0.02, 20, player, 10_000);
        for event in events {
            if let ShipEvent::MissileLaunch {
                owner,
                name,
                code,
                pylon,
                target,
                position,
                axes,
                launcher_radius,
            } = event
            {
                assert_eq!(owner, pirate_id);
                assert_eq!(target, PLAYER_TARGET);
                assert_eq!(code, 0x82);
                assert!((1..=8).contains(&pylon));
                let spec = spec_for(&cache, &viewer, code);
                nav.launch_npc_missile(
                    spec,
                    code,
                    owner,
                    target,
                    position,
                    axes,
                    pylon,
                    launcher_radius,
                );
                launch = Some((name, pylon));
                break;
            }
        }
        if launch.is_some() {
            break;
        }
    }
    let (name, pylon) = launch.expect("F754 launched no missile");
    assert_eq!(name, pirate_ref(&nav, pirate_id).name);
    assert_eq!(
        pirate_ref(&nav, pirate_id).pylons[pylon - 1],
        0,
        "F941 cleared it"
    );
    assert_eq!(nav.missiles.missiles.len(), 1);
    assert_eq!(nav.missiles.missiles[0].owner, pirate_id);
    assert_eq!(nav.missiles.missiles[0].target, Some(PLAYER_TARGET));
    // Frame: put the eye 40 m behind the missile so its model is visible, as
    // the missile_view fixture does.
    let missile_at = nav.missiles.missiles[0].position;
    let missile_forward = nav.missiles.missiles[0].axes[2];
    let eye =
        std::array::from_fn(|axis| missile_at[axis] - (missile_forward[axis] * 40_000.0) as i64);
    state = player_state(eye, missile_at);
    draw(&mut screen, &nav, &state, &viewer);
    save(&screen, "npc-missile-launch.ppm");
    let launch_frame = screen.logical_pixels().to_vec();
    screen.missiles.clear();
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    assert!(
        centre_changed(&launch_frame, screen.logical_pixels()) > 0,
        "the NPC missile did not draw"
    );

    // F1054: the chaff dispenser needs general bit 11 and the Chaff cargo word.
    assert!(!state.loadout.chaff_dispenser());
    assert_eq!(state.hold.get(CHAFF).copied().unwrap_or(0), 0);
    state.loadout.general |= CHAFF_BIT;
    let mut hold = vec![0u16; NAMES];
    hold[CHAFF] = 1;
    state.hold = hold;
    assert!(state.loadout.chaff_dispenser());
    assert_eq!(state.hold[CHAFF], 1);
    // Drop the chaff directly in the missile's flight path (the source's F291
    // ejects the object behind the ship; the scripted frame needs a short,
    // deterministic run), then fly clear: the missile must chase the decoy
    // instead of the player.
    let forward = nav.missiles.missiles[0].axes[2];
    let chaff_at = std::array::from_fn(|axis| {
        nav.missiles.missiles[0].position[axis] + (forward[axis] * 2_000_000.0) as i64
    });
    let chaff = nav.deploy_chaff(chaff_at, 200_000);
    assert!(missile::is_decoy(chaff));
    let pulled = nav.retarget_missiles(PLAYER_TARGET, chaff, |_| true);
    assert_eq!(pulled, 1);
    assert_eq!(nav.missiles.missiles[0].target, Some(chaff));
    let clear = [player[0], player[1] + 2_000_000, player[2]];
    // Evidence frame: look at the chaff object (model 0xa4) from 100 m away.
    let eye = std::array::from_fn(|axis| chaff_at[axis] - (forward[axis] * 100_000.0) as i64);
    state = player_state(eye, chaff_at);
    draw(&mut screen, &nav, &state, &viewer);
    save(&screen, "npc-missile-chaff.ppm");
    let chaff_frame = screen.logical_pixels().to_vec();
    screen.effects.clear();
    screen.draw(&state, ViewState::default(), Some(&viewer), false, "");
    assert!(
        centre_changed(&chaff_frame, screen.logical_pixels()) > 0,
        "the chaff object did not draw"
    );
    // Place the missile at the scripted close pass so the outcome is
    // deterministic, then tick F718/F721's contact test. The player is
    // elsewhere, so this must be a decoy hit and not a player hit.
    state = player_state(clear, target);
    nav.missiles.missiles[0].position = chaff_at;
    let events = nav.tick_missiles_at(0.02, clear, [0.0; 3], 10_000);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, MissileEvent::HitDecoy { .. })),
        "the missile did not detonate on the chaff: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, MissileEvent::HitPlayer { .. })),
        "the decoy did not pull the missile"
    );
    assert!(
        nav.missiles.missiles.is_empty(),
        "the missile should have been consumed by the chaff"
    );

    // A second missile straight at the player: F718/F721/F953 on the player.
    let spec = spec_for(&cache, &viewer, 0x82);
    nav.launch_npc_missile(
        spec,
        0x82,
        pirate_id,
        PLAYER_TARGET,
        clear,
        state.axes.unwrap(),
        1,
        10_000,
    );
    state.shields = [400, 400];
    state.hull = [100, 100];
    assert_eq!(state.shields[0], 400);
    let mut hit = None;
    for _ in 0..200 {
        for event in nav.tick_missiles_at(0.02, clear, [0.0; 3], 10_000) {
            if let MissileEvent::HitPlayer { damage, .. } = event {
                hit = Some(damage);
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let damage = hit.expect("the missile did not reach the player");
    assert_eq!(damage, 200, "DATA_006269[6]");
    let outcome = state.take_damage(damage, 25);
    assert_eq!(outcome, ffe_core::laser::Damage::Shields);
    assert_eq!(state.shields[0], 200);
    assert_eq!(state.hull[0], 100);
    draw(&mut screen, &nav, &state, &viewer);
    save(&screen, "npc-missile-hit.ppm");
    let hit_frame = screen.logical_pixels().to_vec();
    assert!(changed(&chaff_frame, &hit_frame) > 0);

    // F757/F935: an evading NPC with the standard E.C.M. bit destroys the
    // player's missile. F941 puts the target into 0x0c when the player launches.
    // The scripted pirate may have been destroyed by the first missile in a
    // different RNG run; restore it for the F757 half of the encounter.
    if !pirate_ref(&nav, pirate_id).alive() {
        pirate_mut(&mut nav, pirate_id).state = State::Attack;
        pirate_mut(&mut nav, pirate_id).damage.hull = 100;
        pirate_mut(&mut nav, pirate_id).damage.shields = 0;
        pirate_mut(&mut nav, pirate_id).damage.shields_max = 0;
    }
    pirate_mut(&mut nav, pirate_id).general = ECM_BIT;
    pirate_mut(&mut nav, pirate_id).pylons = [0; missile::PYLONS];
    let player_missile = spec_for(&cache, &viewer, 0x82);
    nav.launch_missile(
        player_missile,
        0x82,
        0,
        pirate_id,
        clear,
        state.axes.unwrap(),
        1,
        10_000,
    );
    assert_eq!(pirate_ref(&nav, pirate_id).state, State::Evade);
    let player_missile_id = nav
        .missiles
        .missiles
        .last()
        .map(|missile| missile.id)
        .unwrap();
    assert_eq!(
        pirate_ref(&nav, pirate_id).missile_target,
        Some(player_missile_id)
    );
    let mut ecm = None;
    for _ in 0..200 {
        for event in nav.tick_ships(0.02, 20, clear, 10_000) {
            if let ShipEvent::Ecm { naval, .. } = event {
                ecm = Some(naval);
            }
        }
        if ecm.is_some() {
            break;
        }
    }
    let naval = ecm.expect("F757 never rolled the ECM burst");
    assert!(!naval, "the standard E.C.M. bit");
    nav.ecm(naval);
    assert!(nav.missiles.get(player_missile_id).is_none());
    // F780/F781: the missile's death ends the evasive state and returns the
    // ship to its attack.
    assert_eq!(pirate_ref(&nav, pirate_id).state, State::Attack);
    assert_eq!(pirate_ref(&nav, pirate_id).missile_target, None);
}
