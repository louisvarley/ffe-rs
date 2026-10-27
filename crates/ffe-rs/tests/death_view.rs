//! Local-only frames for F1104's death sequence and F1098's escape capsule.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! death_view -- --ignored --nocapture`

use ffe_core::effects;
use ffe_core::equipment::Loadout;
use ffe_rs::camera::{ViewMode, ViewState};
use ffe_rs::death::{self, DeathPhase, Outcome, ESCAPE_CAPSULE_BIT};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::gameover::{self, GameOver};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Navigator, NEW_GAME_SHIP};
use ffe_rs::screen::{Screen, SystemEffect};

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

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
    v.map(|x| x / length)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn look_at(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 3] {
    let forward = normalise(std::array::from_fn(|i| to[i] - from[i]));
    let right = normalise(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    [right, up, forward]
}

/// The system position the frames use: 2 km from the new game's starport, on the
/// same side as the launch, looking at it.
fn start(cache: &ffe_content::VerifiedCache) -> (Navigator, Flight, [i64; 3]) {
    let nav = Navigator::new_game(cache, 1).unwrap();
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    let station = nav
        .docked
        .and_then(|index| nav.bodies.get(index))
        .expect("the new game starts docked");
    let position = [
        station.position[0],
        station.position[1],
        station.position[2] - 2_000_000,
    ];
    let from = position.map(|v| v as f64 / unit);
    let state = Flight {
        mode: Mode::Flight,
        position: from,
        velocity: [0.0, 0.0, 0.0],
        system_flight: true,
        ship: NEW_GAME_SHIP,
        ..Flight::default()
    };
    let axes = look_at(from, station.position.map(|v| v as f64 / unit));
    (
        nav,
        Flight {
            axes: Some(axes),
            ..state
        },
        position,
    )
}

fn stage_effects(screen: &mut Screen, nav: &Navigator) {
    let unit = ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    screen.effects = nav
        .effects
        .iter()
        .map(|effect| SystemEffect {
            model: effect.model,
            centre: effect.position.map(|v| v as f64 / unit),
            axes: effect.axes(),
            timer: effect.timer,
            size: if effect.kind == effects::Kind::Explosion {
                ffe_core::effects::EXPLOSION_SIZE
            } else {
                0
            },
        })
        .collect();
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn f1104s_death_hold_and_tombstone() {
    let cache = cache();
    let (mut nav, mut state, at) = start(&cache);
    let mut viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let views = ViewState {
        mode: ViewMode::External,
        ..ViewState::default()
    };
    state.hull = [0, 4608];
    state.loadout = Loadout::new_game();
    let axes = state.axes.unwrap();
    nav.effects.extend(death::player_explosion(
        at,
        state
            .velocity
            .map(|v| v * ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT),
        10,
        axes,
        1.0,
    ));
    let outcome = death::begin(&mut state, None);
    assert_eq!(outcome, Outcome::Destroyed);
    assert_eq!(state.death.unwrap().phase, DeathPhase::Exploding);
    stage_effects(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "death-explosion.ppm");
    let explosion = screen.logical_pixels().to_vec();
    let blank = {
        let mut blank = state.clone();
        blank.axes = Some(axes);
        let mut screen = Screen::new(&cache).unwrap();
        screen.draw(&blank, views, Some(&viewer), true, "");
        screen.logical_pixels().to_vec()
    };
    assert!(
        changed(&blank, &explosion) > 20,
        "F942's model-155 explosion is not visible in the external view"
    );
    let model = nav
        .effects
        .iter()
        .find(|effect| effect.kind == effects::Kind::Explosion)
        .map(|effect| effect.model);
    assert_eq!(model, Some(effects::EXPLOSION_MODEL));

    // F1104's 0x3cae6 hold ends at F917's spinning tombstone (model 172), caught
    // here 501 tombstone frames in: the slab is at TOMBSTONE_NEAREST and shows
    // its GAME OVER face (the original's `death-frame-990`).
    state.death.as_mut().unwrap().tick(death::GAME_OVER_SECONDS);
    assert!(state.death.unwrap().is_game_over());
    let game = GameOver::tombstone(death::COMMANDER_NAME.to_ascii_uppercase(), "1-JAN-3250");
    viewer.set_game_over_text(game.commander().as_bytes(), game.date().as_bytes());
    screen
        .draw_game_over(&mut viewer, gameover::TOMBSTONE_MODEL, -272_543)
        .unwrap();
    save(&screen, "death-gameover.ppm");
    let game_over = screen.logical_pixels().to_vec();
    let ink = game_over.iter().filter(|pixel| **pixel != 0).count();
    assert!(changed(&explosion, &game_over) > 20);
    assert!(ink > 500, "the GAME OVER tombstone did not draw");
    println!(
        "death frame: F942 model {} + debris; hold {:.3} s; game over at {} changed pixels",
        effects::EXPLOSION_MODEL,
        death::GAME_OVER_SECONDS,
        changed(&explosion, &game_over)
    );
    // F1104 selects `DATA_008872 = 4`, the external view.
    assert_eq!(views.mode, ViewMode::External);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn f1098s_capsule_escape_frame() {
    let cache = cache();
    let (mut nav, mut state, _at) = start(&cache);
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    let views = ViewState {
        mode: ViewMode::External,
        ..ViewState::default()
    };
    state.hull = [0, 4608];
    state.loadout = Loadout {
        general: ESCAPE_CAPSULE_BIT,
        ..Loadout::new_game()
    };
    let stats = viewer.ship_stats(14).expect("model 14 statistics");
    let outcome = death::begin(&mut state, Some(&stats));
    assert_eq!(outcome, Outcome::Escaped(death::EscapeCraft::Capsule));
    assert_eq!(state.ship, 14);
    assert_eq!(state.loadout.general, death::ESCAPE_GENERAL);
    assert_eq!(state.combat.lasers, [0; 4]);
    assert_eq!(state.combat.missiles, [0; 10]);
    assert_eq!(state.hull, [stats.mass * 4; 2]);
    nav.tick_effects(0.1, 4_970);
    stage_effects(&mut screen, &nav);
    screen.draw(&state, views, Some(&viewer), true, "");
    save(&screen, "death-capsule.ppm");
    let capsule = screen.logical_pixels().to_vec();
    // The old hull and the capsule differ on screen (the model is the player's
    // ship, so the view is from behind model 14).
    let mut old = state.clone();
    old.ship = NEW_GAME_SHIP;
    let mut old_screen = Screen::new(&cache).unwrap();
    old_screen.draw(&old, views, Some(&viewer), true, "");
    assert!(changed(old_screen.logical_pixels(), &capsule) > 0);
    println!(
        "escape frame: model {} hull {:?} general {:#010x}",
        state.ship, state.hull, state.loadout.general
    );
    assert_eq!(death::EscapeCraft::Capsule.model(), 14);
    assert_eq!(death::EscapeCraft::Stowmaster.model(), 16);
}
