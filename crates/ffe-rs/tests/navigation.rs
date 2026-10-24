use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{autopilot, bodies, describe, distance, galaxy, standoff, Kind, Memory};
use ffe_rs::original_flight::OriginalFlight;

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

/// The systems on offer, their bodies by name, and an autopilot run to the first
/// planet of Sol and to a station.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn systems_and_autopilot() {
    let cache = cache();
    let entries = galaxy(&cache).unwrap();
    assert!(entries.len() >= 12, "{} systems", entries.len());
    for entry in &entries {
        let list = bodies(&cache, entry, [0x1000, 0]).unwrap();
        eprintln!("{}: {} bodies", entry.name, list.len());
        for body in list.iter().take(6) {
            eprintln!(
                "   {:?} {} {}",
                body.kind,
                body.name,
                describe(distance([0; 3], body.position))
            );
        }
        assert!(!list.is_empty(), "{}", entry.name);
    }
    let sol = entries.iter().find(|e| e.name == "Sol").expect("Sol");
    let list = bodies(&cache, sol, [0x1000, 0]).unwrap();
    // A long leg: from the first planet out to the farthest one.
    let far = list
        .iter()
        .filter(|b| b.kind == Kind::Planet)
        .max_by(|a, b| distance([0; 3], a.position).total_cmp(&distance([0; 3], b.position)))
        .expect("a planet");
    {
        let start = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let mut flight = OriginalFlight::from_flight(&start, None);
        flight.teleport([0, 0, -50_000_000_000]);
        let stop = standoff(far);
        let mut elapsed = 0.0_f64;
        let mut memory = Memory::default();
        let mut arrived = false;
        while elapsed < 3_000.0 {
            let command = autopilot(&flight, far.position, stop, &mut memory);
            if let Some((landing, seconds)) = command.skip {
                eprintln!("transfer of {seconds:.0} world seconds");
                flight.teleport(landing);
                continue;
            }
            flight.set_time_acceleration(command.acceleration);
            flight.step(0.05, &command.keys).unwrap();
            elapsed += 0.05;
            if command.arrived {
                arrived = true;
                break;
            }
        }
        let range = distance(flight.player().motion.position, far.position);
        eprintln!(
            "{}: arrived {arrived} after {elapsed:.0} real s at {}",
            far.name,
            describe(range)
        );
        assert!(
            arrived,
            "{} not reached; range {}",
            far.name,
            describe(range)
        );
    }
    for wanted in [Kind::Planet, Kind::Station] {
        let target = list.iter().find(|b| b.kind == wanted).expect("a body");
        let start = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        let mut flight = OriginalFlight::from_flight(&start, None);
        let at = [
            target.position[0] + 3_000_000_000,
            target.position[1],
            target.position[2] - 9_000_000_000,
        ];
        flight.teleport(at);
        let stop = standoff(target);
        eprintln!(
            "{}: start range {} stop {}",
            target.name,
            describe(distance(at, target.position)),
            describe(stop)
        );
        let mut elapsed = 0.0_f64;
        let mut memory = Memory::default();
        let mut arrived = false;
        while elapsed < 20_000.0 {
            let command = autopilot(&flight, target.position, stop, &mut memory);
            if let Some((landing, seconds)) = command.skip {
                eprintln!("transfer of {seconds:.0} world seconds");
                flight.teleport(landing);
                continue;
            }
            flight.set_time_acceleration(command.acceleration);
            flight.step(0.05, &command.keys).unwrap();
            elapsed += 0.05;
            if command.arrived {
                arrived = true;
                break;
            }
        }
        let range = distance(flight.player().motion.position, target.position);
        eprintln!(
            "{}: arrived {arrived} after {elapsed:.0} s at {}",
            target.name,
            describe(range)
        );
        assert!(
            arrived,
            "{} not reached; range {}",
            target.name,
            describe(range)
        );
    }
}

/// The map state the keys drive: open, choose, jump, fly.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn navigator_flow() {
    use ffe_rs::navigation::{Navigator, View};
    let cache = cache();
    let mut nav = Navigator::new(&cache).unwrap();
    assert_eq!(nav.system_name(), "Lave");
    nav.open(View::Galaxy);
    assert_eq!(nav.view, View::Galaxy);
    let lines = nav.lines([0; 3]);
    assert!(lines.iter().any(|l| l.contains("Sol")), "{lines:?}");
    // Jump to Sol.
    nav.selected = lines.iter().position(|l| l.contains("Sol")).unwrap();
    let arrival = nav.jump(&cache, nav.selected).unwrap();
    assert_eq!(nav.system_name(), "Sol");
    nav.open(View::System);
    let lines = nav.lines(arrival);
    assert!(lines.iter().any(|l| l.contains("Earth")), "{lines:?}");
    nav.selected = lines.iter().position(|l| l.contains("Earth")).unwrap();
    assert_eq!(nav.engage().map(|b| b.name.as_str()), Some("Earth"));
    let mut flight = OriginalFlight::from_flight(
        &Flight {
            mode: Mode::Flight,
            ..Flight::default()
        },
        None,
    );
    flight.teleport(arrival);
    let mut arrived = false;
    for _ in 0..100_000 {
        let (command, _) = nav.command(&cache, &flight).expect("engaged");
        if let Some((landing, seconds)) = command.skip {
            assert!(seconds > 0.0);
            flight.teleport(landing);
            nav.transfer(&cache, seconds);
            continue;
        }
        flight.step(0.05, &command.keys).unwrap();
        nav.advance(&cache, 0.05);
        if let Some(moved) = nav.follow(flight.player().motion.position) {
            flight.shift(moved);
        }
        if command.arrived {
            arrived = true;
            break;
        }
    }
    let (last, body) = nav.command(&cache, &flight).unwrap();
    eprintln!(
        "stop {} set speed {} m/s aligned {} accel {} step {}",
        describe(standoff(&body)),
        flight.set_speed_metres(),
        last.aligned,
        last.acceleration,
        flight.acceleration_step()
    );
    assert!(
        arrived,
        "Earth not reached; range {} clock {:.0}",
        describe(distance(
            flight.player().motion.position,
            nav.bodies[nav.target.unwrap()].position
        )),
        nav.clock
    );
    assert!(nav.clock > 0.0);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn orbit_rates() {
    use ffe_rs::navigation::clock_words;
    let cache = cache();
    let entries = galaxy(&cache).unwrap();
    let sol = entries.iter().find(|e| e.name == "Sol").unwrap();
    let mut previous: Option<[i64; 3]> = None;
    for seconds in [
        0.0,
        3600.0,
        86_400.0,
        30.0 * 86_400.0,
        182.6 * 86_400.0,
        365.25 * 86_400.0,
    ] {
        let list = bodies(&cache, sol, clock_words(seconds)).unwrap();
        let earth = list.iter().find(|b| b.name == "Earth").unwrap();
        let moon = list.iter().find(|b| b.name == "Moon").unwrap();
        eprintln!(
            "t={seconds:.0} s Earth {} from Sol, Moon {} from Earth",
            describe(distance([0; 3], earth.position)),
            describe(distance(earth.position, moon.position))
        );
        if let Some(last) = previous {
            eprintln!("   moved {}", describe(distance(last, earth.position)));
        }
        previous = Some(earth.position);
    }
}

/// Earth seen from 40,000 km out fills a disc of the right size, lit toward Sol.
#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn planet_discs_are_drawn() {
    use ffe_rs::camera::ViewState;
    use ffe_rs::original_flight::UNITS_PER_FLIGHT_UNIT as UNIT;
    use ffe_rs::screen::{Disc, Screen};
    let cache = cache();
    let mut screen = Screen::new(&cache).unwrap();
    let mut state = Flight {
        mode: Mode::Flight,
        ..Flight::default()
    };
    state.position = [0.0; 3];
    state.axes = Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let radius = 6.371e9 / UNIT; // Earth, in flight units (mm / 9,700)
    let range = 4.0e10 / UNIT;
    screen.discs = vec![Disc {
        centre: [0.0, 0.0, range],
        radius,
        light: [1.0, 0.0, 0.0],
        colours: (100, 101),
        star: false,
        model: None,
    }];
    let discs = std::mem::take(&mut screen.discs);
    screen.draw(&state, ViewState::default(), None, false, false, "");
    let before = screen.logical_pixels().to_vec();
    screen.discs = discs;
    screen.draw(&state, ViewState::default(), None, false, false, "");
    let changed = screen
        .logical_pixels()
        .iter()
        .zip(&before)
        .filter(|(a, b)| a != b)
        .count();
    // Disc area: pi * (256 * radius / range)^2 pixels.
    let expected = std::f64::consts::PI * (256.0 * radius / range).powi(2);
    eprintln!("expected {expected:.0}, changed {changed}");
    assert!((changed as f64 - expected).abs() < expected * 0.1);
}
