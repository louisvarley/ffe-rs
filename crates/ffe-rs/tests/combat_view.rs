//! Local-only frame checks for the combat HUD: F1111's gunsight, F1112's
//! velocity marker, F270's gauges, F1100's range text and F1776's beam.
//!
//! `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test
//! combat_view -- --ignored --nocapture`

use ffe_rs::camera::{ViewMode, ViewState};
use ffe_rs::combat::{self, Combat};
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::screen::{Screen, TargetHud};
use ffe_rs::targeter;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn flight() -> Flight {
    let mut state = Flight {
        mode: Mode::Flight,
        position: [0.0; 3],
        velocity: [0.0, 0.0, 100.0],
        system_flight: true,
        ship: ffe_rs::navigation::NEW_GAME_SHIP,
        ..Flight::default()
    };
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    state
}

fn save(screen: &Screen, name: &str) {
    if let Some(dir) = std::env::var_os("FFE_SHOT_DIR") {
        let mut ppm = b"P6\n320 200\n255\n".to_vec();
        for p in screen.logical_pixels() {
            ppm.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(std::path::Path::new(&dir).join(name), ppm).unwrap();
    }
}

fn changed(before: &[u32], after: &[u32], x0: i32, x1: i32, y0: i32, y1: i32) -> usize {
    (y0.max(0)..y1.min(200))
        .flat_map(|y| (x0.max(0)..x1.min(320)).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let at = *y as usize * 320 + *x as usize;
            before[at] != after[at]
        })
        .count()
}

fn draw(screen: &mut Screen, state: &Flight, views: ViewState, scene: Option<&ModelViewer>) {
    screen.draw(state, views, scene, false, true, "");
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn gunsight_is_f1111s_bitmap_97_in_laser_views_only() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = flight();
    // No velocity marker over the gunsight; F1112 draws after F1111 otherwise.
    state.velocity = [0.0, 0.0, -100.0];
    let views = ViewState::default();

    screen.lasers = [0; 4];
    draw(&mut screen, &state, views, None);
    let none = screen.logical_pixels().to_vec();

    let mut front = views;
    front.mode = ViewMode::Front;
    screen.lasers = [ffe_core::laser::PULSE_1MW, 0, 0, 0];
    draw(&mut screen, &state, front, None);
    let with_sight = screen.logical_pixels().to_vec();
    save(&screen, "combat-gunsight-front.ppm");
    // F1111 blits 61x61 at (129,48).
    assert!(
        changed(&none, &with_sight, 129, 190, 48, 109) > 20,
        "the front gunsight did not draw"
    );
    // Compare every non-masked bitmap pixel with the frame.
    let record = cache
        .manifest()
        .files
        .iter()
        .find(|record| record.id == "base:bitmap/secondary/001")
        .unwrap();
    let ffe_content::AssetKind::IndexedBitmap {
        width,
        height,
        offset_x,
        offset_y,
        masked_zero,
        ..
    } = record.asset
    else {
        panic!("secondary 1 is not an indexed bitmap");
    };
    let bitmap = ffe_rs::indexed::IndexedImage::new(
        width,
        height,
        [offset_x, offset_y],
        masked_zero,
        cache.asset("base:bitmap/secondary/001").unwrap(),
    )
    .unwrap();
    let palette = ffe_rs::indexed::palette_rgb6(cache.asset("base:palette/low").unwrap()).unwrap();
    let mut expected = 0;
    for (index, value) in bitmap.indices().iter().enumerate() {
        if *value == 0 {
            continue;
        }
        let (row, col) = (index / width as usize, index % width as usize);
        let pixel = with_sight[(48 + row) * 320 + 129 + col];
        assert_eq!(
            pixel,
            palette[usize::from(*value)],
            "gunsight pixel {col},{row} value {value} pixel {pixel} expected {}",
            palette[usize::from(*value)]
        );
        expected += 1;
    }
    assert!(expected > 200, "only {expected} opaque gunsight pixels");

    // Rear view with no rear laser: no gunsight. With one: the same bitmap.
    let mut rear = views;
    rear.mode = ViewMode::Rear;
    screen.lasers = [0; 4];
    draw(&mut screen, &state, rear, None);
    let rear_base = screen.logical_pixels().to_vec();
    screen.lasers = [ffe_core::laser::PULSE_1MW, 0, 0, 0];
    draw(&mut screen, &state, rear, None);
    let rear_none = screen.logical_pixels().to_vec();
    assert_eq!(changed(&rear_base, &rear_none, 129, 190, 48, 109), 0);
    screen.lasers = [ffe_core::laser::PULSE_1MW, ffe_core::laser::PULSE_1MW, 0, 0];
    draw(&mut screen, &state, rear, None);
    let rear_sight = screen.logical_pixels().to_vec();
    assert!(changed(&rear_none, &rear_sight, 129, 190, 48, 109) > 20);
    save(&screen, "combat-gunsight-rear.ppm");

    // External view never draws it (F1099's view 4).
    let mut external = views;
    external.mode = ViewMode::External;
    screen.lasers = [0; 4];
    draw(&mut screen, &state, external, None);
    let external_base = screen.logical_pixels().to_vec();
    screen.lasers = [ffe_core::laser::PULSE_1MW; 4];
    draw(&mut screen, &state, external, None);
    let external_sight = screen.logical_pixels().to_vec();
    assert_eq!(
        changed(&external_base, &external_sight, 129, 190, 48, 109),
        0,
        "the external view must not draw the gunsight"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn velocity_marker_and_range_text_follow_f1112_f1100() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = flight();
    let views = ViewState::default();

    state.velocity = [0.0, 0.0, 100.0];
    draw(&mut screen, &state, views, None);
    let centre = screen.logical_pixels().to_vec();
    state.velocity = [20.0, 0.0, 100.0];
    draw(&mut screen, &state, views, None);
    let right = screen.logical_pixels().to_vec();
    save(&screen, "combat-velocity-marker.ppm");
    // The marker is 35x35 at (x-19,y-18): the prograde marker moved right.
    assert!(
        changed(&centre, &right, 141, 176, 61, 96) > 10,
        "the velocity marker did not follow the velocity"
    );

    // F1100's on-view range text sits at the target screen position minus 5.
    screen.target_hud = Some(TargetHud {
        x: 160,
        y: 79,
        value: 540,
        au: true,
        behind: false,
        offscreen: false,
        colour: ffe_rs::targeter::NAVIGATION_COLOUR,
    });
    draw(&mut screen, &state, views, None);
    let ranged = screen.logical_pixels().to_vec();
    save(&screen, "combat-range.ppm");
    // The original's leading newline puts "0.540AU" one 10-row line below the
    // target's -5 origin: x=155, y=84.
    assert!(
        changed(&right, &ranged, 150, 190, 82, 94) > 5,
        "the range text did not draw under the target"
    );
    // Off-screen: F1100's fixed (50,50) with the Out of Range string.
    screen.target_hud = Some(TargetHud {
        x: 50,
        y: 50,
        value: 100,
        au: false,
        behind: false,
        offscreen: true,
        colour: ffe_rs::targeter::NAVIGATION_COLOUR,
    });
    draw(&mut screen, &state, views, None);
    let offscreen = screen.logical_pixels().to_vec();
    save(&screen, "combat-range-offscreen.ppm");
    assert!(changed(&ranged, &offscreen, 50, 200, 45, 70) > 20);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn target_lock_uses_f1100s_green_and_three_decimal_au() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = flight();
    // F1112's velocity marker would sit over the lock; keep it out of this frame.
    state.velocity = [0.0, 0.0, -100.0];
    let views = ViewState::default();
    screen.lasers = [0; 4];

    let palette = ffe_rs::indexed::palette_rgb6(cache.asset("base:palette/low").unwrap()).unwrap();
    let green = palette[usize::from(targeter::NAVIGATION_COLOUR)];
    let red = palette[usize::from(targeter::COMBAT_COLOUR)];
    let orange = palette[usize::from(targeter::MISSILE_VIEW_COLOUR)];
    // The original's fixed low palette: 0x11 is green, 0x10 red, 0x5f the
    // missile-view amber. The target display must not use the latter here.
    assert_eq!(green, 0xf0 << 8);
    assert_eq!(red, 0xf0 << 16);
    assert_eq!(orange, (0xf0 << 16) | (0xa8 << 8) | 0x10);
    assert_ne!(green, orange);

    // F1327's fixed path reaches the same low entries from the colour words
    // (F1326's cube is not involved; these are not dynamic colours).
    let runtime = ffe_rs::palette::RuntimePalette::new(
        cache.asset("base:palette/fixed-map").unwrap().to_vec(),
        cache.asset("base:palette/low").unwrap().to_vec(),
    );
    assert_eq!(
        runtime.resolve(targeter::NAVIGATION_WORD),
        Some([0, 240, 0])
    );
    assert_eq!(runtime.resolve(targeter::COMBAT_WORD), Some([240, 0, 0]));

    draw(&mut screen, &state, views, None);
    let base = screen.logical_pixels().to_vec();

    // F1100/F1113 for a navigation target: the four tunnel boxes and the range
    // text share colour 0x11. The text is the original's "0.540AU", one line
    // under the target (x 155, y 84).
    screen.target_hud = Some(TargetHud {
        x: 160,
        y: 79,
        value: 540,
        au: true,
        behind: false,
        offscreen: false,
        colour: targeter::NAVIGATION_COLOUR,
    });
    screen.tunnels = Some(targeter::Tunnels {
        x: 200,
        y: 119,
        base: 127,
        colour: targeter::NAVIGATION_COLOUR,
    });
    draw(&mut screen, &state, views, None);
    let nav = screen.logical_pixels().to_vec();
    save(&screen, "combat-lock-nav-green.ppm");
    assert!(
        changed(&base, &nav, 150, 195, 82, 94) > 5,
        "the green 0.540AU text did not draw under the target"
    );
    for y in 82..94 {
        for x in 150..195 {
            let at = y * 320 + x;
            if base[at] != nav[at] {
                assert_eq!(nav[at], green, "range pixel ({x},{y}) is not green");
            }
        }
    }
    assert!(
        changed(&base, &nav, 0, 320, 0, 158) > 200,
        "the navigation target's four tunnels did not draw"
    );

    // The navigation auto-targetter box is the same green (F1111's outer box).
    screen.lasers = [ffe_core::laser::PULSE_1MW, 0, 0, 0];
    screen.targeter = Some(targeter::Cursor::Navigation);
    screen.blink = true;
    draw(&mut screen, &state, views, None);
    save(&screen, "combat-lock-nav-cursor.ppm");
    let cursor = screen.logical_pixels().to_vec();
    assert!(
        changed(&nav, &cursor, 156, 165, 75, 84) > 5,
        "F1111's navigation cursor box did not draw"
    );
    for y in 75..84 {
        for x in 156..165 {
            let at = y * 320 + x;
            if nav[at] != cursor[at] {
                assert_eq!(cursor[at], green, "cursor pixel ({x},{y}) is not green");
            }
        }
    }

    // The combat target's range text is 0x10 red in normal views. F1100 draws
    // the combat tunnels only in the missile view, which the port has none of.
    screen.lasers = [0; 4];
    screen.tunnels = None;
    screen.targeter = None;
    screen.target_hud.as_mut().unwrap().colour = targeter::COMBAT_COLOUR;
    draw(&mut screen, &state, views, None);
    save(&screen, "combat-lock-combat-red.ppm");
    let combat = screen.logical_pixels().to_vec();
    for y in 82..94 {
        for x in 150..195 {
            let at = y * 320 + x;
            if nav[at] != combat[at] {
                assert_eq!(combat[at], red, "combat range pixel ({x},{y}) is not red");
            }
        }
    }
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn gauges_show_f270s_energy_and_heat_bars() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState::default();
    screen.lasers = [0; 4];

    screen.energy = 0;
    screen.heat = 0;
    draw(&mut screen, &state, views, None);
    let empty = screen.logical_pixels().to_vec();
    screen.energy = 0x2000_0000;
    screen.drive = 2;
    draw(&mut screen, &state, views, None);
    let full = screen.logical_pixels().to_vec();
    save(&screen, "combat-gauges-full.ppm");
    // Gauge 1 (energy) is primary 275 at x249, y161, 17x15.
    assert!(
        changed(&empty, &full, 249, 267, 160, 176) > 20,
        "the energy bar did not fill"
    );
    // Gauge 2 (heat) is primary 274 at x242.
    screen.energy = 0;
    screen.heat = 0xffff;
    draw(&mut screen, &state, views, None);
    let hot = screen.logical_pixels().to_vec();
    save(&screen, "combat-gauges-heat.ppm");
    assert!(
        changed(&empty, &hot, 242, 260, 160, 176) > 20,
        "the heat bar did not fill"
    );
    // The clip top: h = value * 12 >> 16 and ymin = 0xad - h. A full bar starts
    // at ymin 0xa2; an empty one at 0xad.
    assert_eq!(ffe_core::laser::gauge_height(0xffff), 11);
    assert_eq!(ffe_core::laser::gauge_height(0), 0);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_default_start_is_the_saker_with_f998s_front_pulse_laser() {
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    // F998's default arm (`ffe.asm:74159`) creates model 0x1f. The cache's
    // statistics block is F1533's DATA_002655: 2 gun mounts, 28 t hull, 23 t
    // cargo, eye 1800/400, front gun 2200/0, rear 1900/-200.
    let ship = ffe_rs::navigation::DEFAULT_SHIP;
    assert_eq!(ship, 0x1f);
    let stats = viewer.ship_stats(ship).unwrap();
    assert_eq!(stats.gun_mounts, 2);
    assert_eq!(stats.mass, 28);
    assert_eq!(stats.cargo, 23);
    assert_eq!(stats.eye, [1800, 400]);
    assert_eq!(stats.front_gun, [2200, 0]);
    assert_eq!(stats.rear_gun, [1900, -200]);
    assert_eq!(stats.missiles, 2);
    // Two mounts, but F998 fits only the front 0x88; the rear stays empty.
    let combat = Combat::new_game();
    assert_eq!(combat.lasers, [ffe_core::laser::PULSE_1MW, 0, 0, 0]);
    assert_eq!(combat.drive, 2);
    assert_eq!(combat.energy, 0x2000_0000);
    assert_eq!(combat.missile_count(), 2);
    // F1533's hull word is mass * 4 and F998 leaves +0xe2 zero: 112 and no
    // shield line until a generator is fitted.
    let loadout = ffe_core::equipment::Loadout::new_game();
    let mut state = Flight {
        loadout,
        ..Flight::default()
    };
    state.ensure_damage(stats.mass * 4);
    assert_eq!(state.hull, [112, 112]);
    assert_eq!(state.shields, [0, 0]);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_type_1_ross_154_hull_still_carries_its_front_pulse_laser() {
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let stats = viewer
        .ship_stats(ffe_rs::navigation::NEW_GAME_SHIP)
        .unwrap();
    assert_eq!(stats.gun_mounts, 1);
    assert_eq!(stats.front_gun, [1300, 0]);
    assert_eq!(stats.rear_gun, [800, 0]);
    assert_eq!(stats.missiles, 2);
    // The start-type 1 record (`FUNC_000998`, ffe.asm:74099) fits 0x88 in front.
    let combat = Combat::new_game();
    assert_eq!(combat.lasers, [ffe_core::laser::PULSE_1MW, 0, 0, 0]);
    assert_eq!(ffe_rs::navigation::NEW_GAME_SHIP, 23);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn target_damage_readout_draws_f1099s_text() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let state = flight();
    let views = ViewState::default();
    draw(&mut screen, &state, views, None);
    let before = screen.logical_pixels().to_vec();
    screen.target_damage = Some(ffe_rs::screen::TargetDamage {
        hull: 500,
        shields: Some(75),
        shield_strength: 42,
    });
    draw(&mut screen, &state, views, None);
    save(&screen, "combat-target-damage.ppm");
    assert!(
        changed(&before, screen.logical_pixels(), 235, 310, 0, 32) > 20,
        "the target hull/shields text did not draw at 235,1"
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn beam_is_model_156_at_the_gun_only_where_it_is_in_front() {
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let state = flight();
    let mut views = ViewState::default();
    let stats = viewer.ship_stats(state.ship);
    screen.lasers = Combat::new_game().lasers;
    let scale = ffe_rs::screen::model_scale(&viewer).unwrap();

    // F1776 places the duplicate at the front gun, 1300/8 units ahead of the
    // ship centre; the cockpit eye is 1500/8 ahead. The cone (model 0x9c, 64
    // model units) is therefore 1600 units behind the front camera's near
    // plane and F1479/F1680 cull it. The port must not clamp z to invent a
    // flash below the crosshair.
    let beam = combat::beam(&state, stats.as_ref(), views.mode, [0, 0]);
    let camera = ffe_rs::camera::Camera::new(&state, &views, stats.as_ref(), scale);
    let origin = camera.point(beam.origin).map(|v| v * scale);
    assert!(
        origin[2] < 0.0,
        "the front gun is not behind the eye: {origin:?}"
    );
    let rows = beam.axes.map(|axis| camera.direction(axis));
    let light = camera.direction(viewer.light_direction());
    let rendered = viewer
        .render_scene(combat::BEAM_MODEL, rows, origin, light)
        .unwrap();
    assert!(
        rendered.shapes.is_empty(),
        "model 0x9c rendered behind the front eye"
    );

    draw(&mut screen, &state, views, Some(&viewer));
    let before = screen.logical_pixels().to_vec();
    screen.beam = Some(beam);
    draw(&mut screen, &state, views, Some(&viewer));
    save(&screen, "combat-beam-front.ppm");
    assert_eq!(
        changed(&before, screen.logical_pixels(), 0, 320, 0, 158),
        0,
        "the front view invented a muzzle flash"
    );

    // The external camera is 49 m behind the ship, so the gun is in front of it
    // and F1776's model path runs; the cone is one pixel at that distance.
    views.mode = ViewMode::External;
    views.external.distance = -0x00c0_0000;
    screen.beam = None;
    draw(&mut screen, &state, views, Some(&viewer));
    let external_before = screen.logical_pixels().to_vec();
    let beam = combat::beam(&state, stats.as_ref(), views.mode, [0, 0]);
    screen.beam = Some(beam);
    draw(&mut screen, &state, views, Some(&viewer));
    save(&screen, "combat-beam-external.ppm");
    let external_after = screen.logical_pixels().to_vec();
    let external = changed(&external_before, &external_after, 0, 320, 0, 158);
    // The cone is a pixel at the original 49 m camera distance; the model still
    // renders in front of the camera, unlike the front view's cull.
    let camera = ffe_rs::camera::Camera::new(&state, &views, stats.as_ref(), scale);
    let origin = camera.point(beam.origin).map(|v| v * scale);
    assert!(origin[2] > 0.0, "the external camera is not behind the gun");
    let rows = beam.axes.map(|axis| camera.direction(axis));
    let light = camera.direction(viewer.light_direction());
    let rendered = viewer
        .render_scene(combat::BEAM_MODEL, rows, origin, light)
        .unwrap();
    assert!(
        !rendered.shapes.is_empty(),
        "model 0x9c did not render in front of the external camera"
    );
    assert_eq!(external, 1, "the external muzzle is one pixel");

    // Model 0x9c is the original's small muzzle cone: its record coordinates
    // divide by 2^17, so its 8,388,607 record units are 64 model units.
    assert_eq!(viewer.find(combat::BEAM_MODEL).unwrap().scale, 17);
    let front = stats.unwrap().front_gun[0];
    assert_eq!(front, 1300);
    let ahead = f64::from(front) * 8.0 / ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT;
    assert!((beam.origin[2] - ahead).abs() < 1e-9, "{:?}", beam.origin);

    // A new-game combat state fires a pulse on the front mount only.
    let mut combat = Combat::new_game();
    assert!(combat.fire(0, 994).is_ok());
    assert!(
        combat.fire(0, 994).is_err(),
        "the refire gate did not block"
    );
}
