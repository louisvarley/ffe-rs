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
