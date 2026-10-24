use ffe_rs::camera::ViewState;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::navigation::{Kind, Navigator};
use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
use ffe_rs::screen::{LiftView, Screen, SystemStation};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
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

/// The launch stages draw a lift scene every frame, the docking stages draw one after the
/// touch; frames are kept when `FFE_SHOT_DIR` is set.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_lift_scenes_draw() {
    let cache = cache();
    let viewer = ModelViewer::new(&cache).unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    for model in [69, 72, 73, 74, 75, 76, 77, 78, 79, 80] {
        let found = viewer.find(model);
        nav.set_port_geometry(
            model,
            found.and_then(|m| m.front()),
            found.and_then(|m| m.radius()),
        );
    }
    let station = nav.docked.unwrap();
    let mut screen = Screen::new(&cache).unwrap();
    screen.system = nav.system_name().to_owned();
    let state = Flight {
        ship: 20,
        ..Flight::default()
    };
    // Launching: the lift over the world seen from the slot, facing out.
    let slot = nav.port(station).unwrap();
    nav.begin_launch().unwrap();
    let mut last = 0;
    let mut frames = 0;
    let mut drawn_stages = std::collections::BTreeSet::new();
    while nav.sequence.is_some() && frames < 3000 {
        let events = nav.tick_traffic(994, 0.02);
        frames += 1;
        let Some((objects, launching)) = nav.lift_scene() else {
            continue;
        };
        assert!(launching);
        let stage = nav.sequence.unwrap().stages.state;
        drawn_stages.insert(stage);
        if stage != last || frames % 120 == 0 {
            last = stage;
            let mut inside = state.clone();
            inside.mode = Mode::Flight;
            inside.system_flight = true;
            inside.position = slot.point(slot.front * 0.9).map(|v| v as f64 / UNIT);
            let forward = slot.axis;
            let up = [0.0, 1.0, 0.0];
            let right = [
                up[1] * forward[2] - up[2] * forward[1],
                up[2] * forward[0] - up[0] * forward[2],
                up[0] * forward[1] - up[1] * forward[0],
            ];
            inside.axes = Some([right, up, forward]);
            screen.stations = nav
                .bodies
                .iter()
                .filter(|b| b.kind == Kind::Station)
                .map(|b| SystemStation {
                    model: b.model,
                    centre: b.position.map(|v| v as f64 / UNIT),
                    orientation: b.orientation,
                    speed: b.spin,
                    tics: 0x8000_0000,
                })
                .collect();
            screen.lift = Some(LiftView {
                objects,
                seed: 0x2468_ace0,
                starport: false,
                world: true,
                tics: 0x8000_0000 + frames * 994,
            });
            screen.draw(
                &inside,
                ViewState::default(),
                Some(&viewer),
                false,
                true,
                "",
            );
            // The lift fills the 320x158 view: the port's status block must not
            // clear its bottom rows while a lift scene is drawn.
            // Only the closed/opening door stages fill the bottom rows; others
            // legitimately show dark geometry there.
            if stage == 0x1c {
                let pixels = screen.logical_pixels();
                assert!(
                    (0..320).any(|x| pixels[119 * 320 + x] != 0),
                    "lift row 119 was cleared to black at stage {stage:#04x}"
                );
            }
            save(&screen, &format!("lift-launch-{frames:04}-{stage:02x}.ppm"));
        }
        if events
            .iter()
            .any(|e| matches!(e, ffe_rs::navigation::Traffic::LaunchOut(_)))
        {
            break;
        }
    }
    assert!(drawn_stages.len() >= 4, "{drawn_stages:?}");
    // Docking: the lift over the outside view, ship in the slot.
    let port = nav.port(station).unwrap();
    let mut nav = Navigator::new(&cache).unwrap();
    for model in [69, 72, 73, 74, 75, 76, 77, 78, 79, 80] {
        let found = viewer.find(model);
        nav.set_port_geometry(
            model,
            found.and_then(|m| m.front()),
            found.and_then(|m| m.radius()),
        );
    }
    let at = port.point(port.front * 0.9);
    let forward = port.axis.map(|v| -v);
    let up = [0.0, 1.0, 0.0];
    let right = [
        up[1] * forward[2] - up[2] * forward[1],
        up[2] * forward[0] - up[0] * forward[2],
        up[0] * forward[1] - up[1] * forward[0],
    ];
    let mut docking = Flight {
        mode: Mode::Flight,
        ship: 20,
        system_flight: true,
        ..Flight::default()
    };
    docking.position = at.map(|v| v as f64 / UNIT);
    docking.axes = Some([right, up, forward]);
    assert!(nav.request_clearance(at).is_some());
    assert!(nav.begin_docking(station));
    screen.stations = nav
        .bodies
        .iter()
        .filter(|b| b.kind == Kind::Station)
        .map(|b| SystemStation {
            model: b.model,
            centre: b.position.map(|v| v as f64 / UNIT),
            orientation: b.orientation,
            speed: b.spin,
            tics: 0x8000_0000,
        })
        .collect();
    let (mut last, mut frames, mut stages) = (0, 0u32, std::collections::BTreeSet::new());
    while nav.sequence.is_some() && frames < 3000 {
        nav.tick_traffic(994, 0.02);
        frames += 1;
        let Some((objects, launching)) = nav.lift_scene() else {
            continue;
        };
        assert!(!launching);
        let stage = nav.sequence.unwrap().stages.state;
        stages.insert(stage);
        if stage != last || frames % 120 == 0 {
            last = stage;
            screen.lift = Some(LiftView {
                objects,
                seed: 0x2468_ace0,
                starport: false,
                world: true,
                tics: 0x8000_0000 + frames * 994,
            });
            screen.draw(
                &docking,
                ViewState::default(),
                Some(&viewer),
                false,
                true,
                "",
            );
            save(&screen, &format!("lift-dock-{frames:04}-{stage:02x}.ppm"));
        }
    }
    assert!(stages.len() >= 6, "{stages:?}");
}
