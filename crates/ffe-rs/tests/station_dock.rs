//! Docking at an orbital station through the original's collision scan
//! (F569/F570/F575 in the station's rotating frame), the contact F1024/F1019
//! read. Local-only: needs `FFE_WORLD_CACHE`.
use ffe_core::objects::ModelInfo;
use ffe_rs::flight::{Flight, Mode};
use ffe_rs::navigation::{landing_body, Kind, Library, Navigator};
use ffe_rs::original_flight::{Keys, OriginalFlight, UNITS_PER_FLIGHT_UNIT};

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The contacts a ship flying into the station's slot along its axis meets:
/// `(frame, flags, pad, distance)` for each frame with a contact.
fn fly_in(speed: f64, lateral: f64, frames: usize) -> Vec<(usize, u8, u8, i32)> {
    let cache = cache();
    let nav = Navigator::new(&cache).unwrap();
    let index = nav
        .bodies
        .iter()
        .position(|body| body.kind == Kind::Station)
        .expect("a station");
    let station = &nav.bodies[index];
    let port = nav.port(index).expect("the station's port");
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let station_info = ModelInfo::read(records, stats, station.model).unwrap();
    let ship_info = ModelInfo::read(records, stats, 20).unwrap();
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: ffe_core::surface::DEFAULT_DETAIL,
    };
    let up = station.orientation[1];
    let side = port.point(port.front + 600_000.0);
    let start: [i64; 3] = std::array::from_fn(|k| side[k] + (up[k] * lateral) as i64);
    let forward = port.axis.map(|v| -v);
    let right = cross(up, forward);
    let flight = Flight {
        mode: Mode::Flight,
        ship: 20,
        system_flight: true,
        position: start.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT),
        velocity: forward.map(|v| v * speed / UNITS_PER_FLIGHT_UNIT * 1000.0),
        axes: Some([right, up, forward]),
        set_speed: None,
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&flight, None);
    let body = landing_body(station, &station_info);
    let mut contacts = Vec::new();
    for frame in 0..frames {
        let contact = model
            .step_station(
                0.02,
                &Keys::default(),
                &body,
                station.position,
                &ship_info,
                Some(&tools),
            )
            .expect("the scan runs");
        if contact.flags != 0 {
            contacts.push((frame, contact.flags, contact.pad, contact.distance));
        }
    }
    contacts
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_ship_entering_the_slot_touches_the_bay_in_the_station_region() {
    // 20 m/s along the docking axis from 600 m out: inside the region
    // (`0x20`) all the way, then F575's soft touch (`0x40`) inside the slot,
    // never the crash bit, so F1024 docks a cleared ship there.
    let contacts = fly_in(20.0, 0.0, 2000);
    assert!(
        contacts.iter().all(|c| c.1 & 0x20 != 0 && c.1 & 0x80 == 0),
        "{contacts:?}"
    );
    assert!(
        contacts.iter().any(|c| c.1 & 0x40 != 0),
        "no touch: {:?}",
        contacts.last()
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_fast_ship_off_the_slot_strikes_the_hull() {
    // 120 m off the axis the ship meets the docking face, not the slot:
    // F575's crash (`0x80`), outside the station region.
    let contacts = fly_in(200.0, 120_000.0, 800);
    let crash = contacts
        .iter()
        .find(|c| c.1 & 0x80 != 0)
        .expect("a hull strike");
    assert_eq!(crash.1 & 0x20, 0, "{crash:?}");
    // Straight down the axis at the same speed: report what the bay does.
    let fast = fly_in(200.0, 0.0, 800);
    let flags = fast.iter().fold(0u8, |acc, c| acc | c.1);
    eprintln!(
        "fast on-axis entry: flags seen {flags:#x}, first touch {:?}",
        fast.iter().find(|c| c.1 & 0xc0 != 0)
    );
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn a_launch_from_f1088s_seat_flies_out_of_the_slot() {
    let cache = cache();
    let nav = Navigator::new(&cache).unwrap();
    let index = nav
        .bodies
        .iter()
        .position(|body| body.kind == Kind::Station)
        .unwrap();
    let station = &nav.bodies[index];
    let port = nav.port(index).unwrap();
    let records = cache.asset("base:model/records").unwrap();
    let stats = cache.asset("base:ship/stats").unwrap();
    let station_info = ModelInfo::read(records, stats, station.model).unwrap();
    let ship_info = ModelInfo::read(records, stats, 20).unwrap();
    let library = Library(&cache);
    let tools = ffe_core::surface::SurfaceTools {
        shapes: &library,
        terrain_table: cache.asset("base:terrain/tables").unwrap(),
        terrain_maps: cache.asset("base:terrain/maps").unwrap(),
        detail: ffe_core::surface::DEFAULT_DETAIL,
    };
    let seat = nav.launch_seat(index).unwrap();
    let forward = port.axis;
    let up = station.orientation[1];
    let right = cross(up, forward);
    let speed = 30.0;
    let flight = Flight {
        mode: Mode::Flight,
        ship: 20,
        system_flight: true,
        position: seat.map(|v| v as f64 / UNITS_PER_FLIGHT_UNIT),
        velocity: forward.map(|v| v * speed / UNITS_PER_FLIGHT_UNIT * 1000.0),
        axes: Some([right, up, forward]),
        set_speed: None,
        ..Flight::default()
    };
    let mut model = OriginalFlight::from_flight(&flight, None);
    let body = landing_body(station, &station_info);
    let mut flags = Vec::new();
    // F1031 holds mode 0x26 while the scan reports the region; run until it
    // does not (the region reaches well beyond the docking face).
    for _ in 0..30_000 {
        let contact = model
            .step_station(
                0.02,
                &Keys::default(),
                &body,
                station.position,
                &ship_info,
                Some(&tools),
            )
            .unwrap();
        flags.push(contact.flags);
        if contact.flags & 0x20 == 0 {
            break;
        }
    }
    let at = model.absolute_position();
    let along: f64 = (0..3)
        .map(|k| (at[k] - station.position[k]) as f64 * port.axis[k])
        .sum();
    eprintln!(
        "seat along {:.0} m; region left at along {:.0} m (front {:.0} m); flags seen {:#x}",
        (0..3)
            .map(|k| (seat[k] - station.position[k]) as f64 * port.axis[k])
            .sum::<f64>()
            / 1000.0,
        along / 1000.0,
        port.front / 1000.0,
        flags.iter().fold(0u8, |a, f| a | f)
    );
    assert!(
        flags.iter().all(|f| f & 0x80 == 0),
        "a hull strike on the way out"
    );
    assert!(along > port.front, "still inside");
    assert_eq!(*flags.last().unwrap() & 0x20, 0, "still in the region");
}
