//! Local-only frames for the ECM burst, the armed auto-targetter and the
//! missile view (F718/F719, F1054/F1010/F1111 and F1054's JUMP_004340).
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! ecm_view -- --ignored --nocapture`
use ffe_core::equipment::{Loadout, ECM_BIT};
use ffe_core::missile::PLAYER_MISSILE;
use ffe_core::npc::Role;
use ffe_rs::camera::{MissileCamera, ViewMode, ViewState};
use ffe_rs::combat::Combat;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::missile::{EcmLoss, MissileField};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemMissile, SystemShip};
use ffe_rs::targeter;

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

fn changed_in(before: &[u32], after: &[u32], area: (usize, usize, usize, usize)) -> usize {
    let (x0, x1, y0, y1) = area;
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
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
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / length)
}

fn look_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let forward = normalise(std::array::from_fn(|i| to[i] - from[i]));
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    [right, up, forward]
}

/// The frame's missile view element mismatch area (the F1111 combat circle and
/// the F1100 range text's line at the target).
fn draw(
    screen: &mut Screen,
    nav: &Navigator,
    state: &Flight,
    viewer: &ModelViewer,
    views: ViewState,
) {
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
        .effects
        .iter()
        .map(|effect| ffe_rs::screen::SystemEffect::from_effect(None, effect, unit))
        .collect();
    screen.lasers = state.combat.lasers;
    screen.draw(state, views, Some(viewer), true, "");
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn ecm_destroys_a_missile_and_the_targeter_and_missile_view_draw() {
    let cache = cache();
    let mut nav = Navigator::new_game(&cache, 1).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let index = nav
        .ships
        .iter()
        .position(|ship| ship.role == Role::Pirate)
        .unwrap_or(0);
    let target = nav.ships[index].position;
    let target_id = nav.ships[index].id;
    let from_mm = [target[0], target[1], target[2] - 120_000];
    let from = from_mm.map(|v| v as f64 / unit);
    let mut state = Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0; 3],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some(look_at(from, target.map(|v| v as f64 / unit)));
    state.combat = Combat::new_game();
    // F1054's 'e' needs F935's equipment: the new-game Eagle has no ECM.
    assert_eq!(state.loadout.ecm(), None);
    state.loadout.general |= ECM_BIT;
    assert_eq!(
        state.loadout.ecm(),
        Some(ffe_core::equipment::Ecm::Standard)
    );

    nav.lock_ship(target_id);
    nav.missile_lock = Some(target_id);
    // Keep the missile alive for the ECM frame: the target is 120 m away, so a
    // guidance tick would fuse immediately; don't tick before the burst.
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let launcher_radius = ffe_core::objects::ModelInfo::read(records, stats, state.ship)
        .map_or(0, |info| info.radii().1);
    let launch = |nav: &mut Navigator, state: &mut Flight| {
        let (pylon, code) = state.combat.first_missile().expect("a missile");
        assert_eq!(code, PLAYER_MISSILE);
        let model = ffe_core::missile::model(code).unwrap();
        let missile_radius = ffe_core::objects::ModelInfo::read(records, stats, model)
            .map_or(0, |info| info.radii().1);
        let spec = MissileField::spec(code, viewer.ship_stats(model), missile_radius).unwrap();
        assert!(state.combat.take_missile(pylon).is_some());
        nav.launch_missile(
            spec,
            code,
            0,
            target_id,
            from_mm,
            state.axes.unwrap(),
            pylon,
            launcher_radius,
        );
    };
    launch(&mut nav, &mut state);
    assert_eq!(nav.missiles.missiles.len(), 1);
    // F941 stores the object id in DATA_008877 for the player's launch.
    let missile_id = nav.missile_view.expect("DATA_008877");
    assert_eq!(nav.missiles.get(missile_id).unwrap().spec.model, 6);
    // Put the missile 20 m dead ahead of the camera so its model draws and
    // F719's model-0x9b spark is more than a single pixel (F941's separation
    // impulse would otherwise put it far off-axis this close).
    nav.missiles.missiles[0].position = [from_mm[0], from_mm[1], from_mm[2] + 20_000];
    draw(&mut screen, &nav, &state, &viewer, ViewState::default());
    save(&screen, "ecm-before.ppm");
    let before = screen.logical_pixels().to_vec();

    // F718: the standard E.C.M. defeats model 6 (and 5), then F719 leaves the
    // model-0x9b explosion.
    let losses: Vec<EcmLoss> = nav.ecm(false);
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].model, 6);
    assert_eq!(losses[0].id, missile_id);
    assert!(nav.missiles.missiles.is_empty());
    assert_eq!(nav.missile_view, None, "F1102 clears DATA_008877");
    assert_eq!(nav.effects.len(), 1);
    assert_eq!(nav.effects[0].model, ffe_core::effects::EXPLOSION_MODEL);
    // F719's model-0x9b object is spawned (checked above); its command stream
    // collapses to single-pixel flats in this port's model renderer, exactly as
    // the F942 explosion does in `effects_view`, where the visible burst is
    // F926's debris — which F719 does not create. The frame below therefore
    // shows the missile before the burst and its absence after it.
    draw(&mut screen, &nav, &state, &viewer, ViewState::default());
    save(&screen, "ecm-after.ppm");
    let after = screen.logical_pixels().to_vec();
    assert!(
        changed(&before, &after) > 0,
        "the missile and its explosion frame did not change"
    );

    // F1111's armed navigation targetter box: the gunsight's own cursor area
    // changes when the flag is set (both use the game-second blink).
    screen.systems.data_246 = false;
    screen.targeter = None;
    screen.blink = true;
    screen.missile_blink = true;
    draw(&mut screen, &nav, &state, &viewer, ViewState::default());
    let off = screen.logical_pixels().to_vec();
    screen.targeter = Some(targeter::Cursor::Navigation);
    draw(&mut screen, &nav, &state, &viewer, ViewState::default());
    save(&screen, "targeter-armed.ppm");
    let armed = screen.logical_pixels().to_vec();
    const CURSOR_AREA: (usize, usize, usize, usize) = (150, 172, 70, 90);
    assert!(
        changed_in(&off, &armed, CURSOR_AREA) > 0,
        "the armed navigation box did not draw"
    );
    screen.targeter = None;

    // F10: the missile view rides the missile object. Launch the second missile
    // and point the camera from it at the pirate.
    launch(&mut nav, &mut state);
    let missile = nav.viewed_missile().copied().expect("the second missile");
    let missile_camera = MissileCamera {
        position: missile.position.map(|v| v as f64 / unit),
        axes: missile.axes,
        eye: viewer
            .ship_stats(missile.spec.model)
            .map_or([0, 0], |stats| stats.eye),
    };
    let front_views = ViewState::default();
    draw(&mut screen, &nav, &state, &viewer, front_views);
    save(&screen, "missile-front.ppm");
    let front = screen.logical_pixels().to_vec();
    let missile_views = ViewState {
        mode: ViewMode::Missile,
        missile: Some(missile_camera),
        ..ViewState::default()
    };
    // F1099's view-5 colour and the missile as the range source.
    let (mantissa, exponent) = ffe_core::laser::range_words(missile.position, target);
    let (value, au) = ffe_core::laser::range_value(mantissa, exponent);
    screen.target_hud = Some(ffe_rs::screen::TargetHud {
        x: 160,
        y: 79,
        value,
        au,
        behind: false,
        offscreen: false,
        colour: targeter::MISSILE_VIEW_COLOUR,
    });
    draw(&mut screen, &nav, &state, &viewer, missile_views);
    save(&screen, "missile-view.ppm");
    let viewed = screen.logical_pixels().to_vec();
    assert!(
        changed(&front, &viewed) > 0,
        "the missile view camera did not move the frame"
    );
    // The missile camera is the missile object's own eye, not the ship's.
    let camera = ffe_rs::camera::Camera::new(
        &state,
        &missile_views,
        viewer.ship_stats(state.ship).as_ref(),
        32767.0,
    );
    assert!(
        (camera.position[0] - missile_camera.position[0]).abs() < 1.0
            && (camera.position[2] - missile_camera.position[2]).abs() < 1.0,
        "the missile view camera is not at the missile"
    );
    assert_eq!(ffe_rs::combat::mount(ViewMode::Missile), None);
    // The F10 icon is lit only with the viewer and a missile.
    let loadout = Loadout {
        general: ffe_core::equipment::MISSILE_VIEWER_BIT,
        ..Loadout::new_game()
    };
    assert!(loadout.missile_viewer());
}
