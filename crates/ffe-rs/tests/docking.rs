use ffe_rs::docking::{Contact, Port, Reason, MAX_ENTRY_SPEED};
use ffe_rs::navigation::{Body, Kind};

/// A station 1 km in radius whose docking face stands 400 m out along +z, 2 km
/// from the system origin.
fn station() -> Body {
    Body {
        name: "Test Station".to_owned(),
        kind: Kind::Station,
        position: [2_000_000, 0, 0],
        radius: 1_000_000,
        inner_radius: 1_000_000,
        parent: None,
        model: 72,
        seed: 0,
        slot: 1,
        rows: [0x7fff_ffff, 0, 0, 0, 0x7fff_ffff, 0, 0, 0, 0x7fff_ffff],
        class: 0,
        open_air: false,
        orientation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        spin: 12,
        bays: 3,
    }
}

fn port() -> Port {
    Port::new(&station(), Some(400_000.0), Some(1_000_000.0))
}

#[test]
fn port_geometry_follows_the_docking_axis() {
    let port = port();
    assert_eq!(port.axis, [0.0, 0.0, 1.0]);
    assert_eq!(port.point(500_000.0), [2_000_000, 0, 500_000]);
    // Launch: out on the axis, beyond the face and well clear of the hull.
    let launch = port.launch_point();
    assert_eq!(&launch[..2], &[2_000_000, 0]);
    assert!(launch[2] as f64 > port.front + port.radius);
    assert_eq!(port.contact(launch, 0.0, false), Contact::None);
}

#[test]
fn a_model_without_a_face_in_its_root_vertices_takes_a_quarter_radius() {
    let port = Port::new(&station(), Some(-131_072.0), Some(8_000_000.0));
    assert_eq!(port.front, 2_000_000.0);
    // No model geometry at all: the object's radius.
    let port = Port::new(&station(), None, None);
    assert_eq!(port.radius, 1_000_000.0);
    assert_eq!(port.front, 250_000.0);
}

#[test]
fn the_slot_docks_a_cleared_slow_ship() {
    let port = port();
    // Just inside the face, on the axis, a little off it within the corridor.
    let inside = [2_000_000 + 20_000, 0, 390_000];
    assert_eq!(port.contact(inside, 40.0, true), Contact::Docked);
    // Outside the face: nothing yet.
    assert_eq!(
        port.contact([2_000_000, 0, 410_000], 40.0, true),
        Contact::None
    );
}

#[test]
fn the_slot_refuses_without_clearance_or_too_fast() {
    let port = port();
    let inside = [2_000_000, 0, 300_000];
    let back_out = |contact| match contact {
        Contact::Collision { clear, reason } => {
            assert_eq!(&clear[..2], &[2_000_000, 0], "pushed back along the axis");
            assert!(clear[2] as f64 > port.front);
            assert_eq!(port.contact(clear, 0.0, true), Contact::None);
            reason
        }
        other => panic!("expected a collision, got {other:?}"),
    };
    assert_eq!(
        back_out(port.contact(inside, 10.0, false)),
        Reason::NoClearance
    );
    assert_eq!(
        back_out(port.contact(inside, MAX_ENTRY_SPEED + 1.0, true)),
        Reason::TooFast
    );
}

#[test]
fn the_hull_pushes_the_ship_clear_away_from_the_centre() {
    let port = port();
    // Behind the station (on its -z side) and beside it: hull.
    for ship in [[2_000_000, 0, -300_000], [2_300_000, 0, 50_000]] {
        match port.contact(ship, 10.0, true) {
            Contact::Collision {
                clear,
                reason: Reason::Hull,
            } => {
                let range = (0..3)
                    .map(|k| ((clear[k] - port.centre[k]) as f64).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(range > port.front, "{range}");
                assert_eq!(port.contact(clear, 0.0, true), Contact::None);
            }
            other => panic!("expected a hull collision at {ship:?}, got {other:?}"),
        }
    }
}

#[test]
fn a_turned_station_docks_along_its_own_axis() {
    // Turned so that its +z (docking) axis points along system +x.
    let mut body = station();
    body.orientation = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
    let port = Port::new(&body, Some(400_000.0), Some(1_000_000.0));
    assert_eq!(port.axis, [1.0, 0.0, 0.0]);
    assert_eq!(
        port.contact([2_000_000 + 380_000, 0, 0], 20.0, true),
        Contact::Docked
    );
    assert!(matches!(
        port.contact([2_000_000, 0, 380_000], 20.0, true),
        Contact::Collision {
            reason: Reason::Hull,
            ..
        }
    ));
}
