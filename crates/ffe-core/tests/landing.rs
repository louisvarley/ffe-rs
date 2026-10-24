//! F575's contact tail: the soft-landing test (`0x258`, the landing word
//! `+0x9e`, the `0x78000000` dot), the crash path, the player's bounce and flag
//! 1. Written from the assembly (`JUMP_002410` onwards).

use ffe_core::landing::{self, Pair, Surface, Touch};
use ffe_core::model::multiply;
use ffe_core::objects::ModelInfo;

const BYTES: usize = 0x152;

fn dword(object: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(object[at..at + 4].try_into().unwrap())
}

fn set_dword(object: &mut [u8], at: usize, value: i32) {
    object[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_qword(object: &mut [u8], at: usize, value: i64) {
    object[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// A body with an orthonormal orientation and the fields the code reads. F575's
/// dot and its placement use the second row (`+0xc`).
fn object() -> Vec<u8> {
    let mut body = vec![0u8; BYTES];
    set_dword(&mut body, 0x0, 0x7fff_ffff);
    set_dword(&mut body, 0x10, 0x7fff_ffff);
    set_dword(&mut body, 0x20, 0x7fff_ffff);
    body[0x86] = 7;
    body[0x9e..0xa0].copy_from_slice(&0xffffu16.to_le_bytes());
    body
}

/// Point the second row at the surface's up, so the dot is as large as it can be.
fn aim(body: &mut [u8]) {
    let up = up();
    for (k, value) in up.iter().enumerate() {
        set_dword(body, 0xc + 4 * k, *value);
    }
}

/// A planet whose radii are equal (flag 1 stays clear).
fn planet() -> Vec<u8> {
    let mut planet = vec![0u8; BYTES];
    planet[0x86] = 3;
    set_qword(&mut planet, 0x138, 0x1000);
    set_qword(&mut planet, 0x140, 0x1000);
    planet
}

fn model() -> ModelInfo {
    ModelInfo {
        radius_dword: 0x100,
        scale: 0,
        scale_offset: 0,
        flags: 0,
        complex: false,
        radius: 0,
        collision_radius: 0,
        stats: None,
    }
}

fn with_stats(mass: i16) -> ModelInfo {
    let mut stats = [0u8; 48];
    stats[6..8].copy_from_slice(&mass.to_le_bytes());
    ModelInfo {
        stats: Some(stats),
        ..model()
    }
}

/// A relative position along x, so the sphere's up vector has no other axis and
/// the dot with row 0 is one product.
const RELPOS: [i64; 3] = [0x7fff_ffff, 0, 0];

fn up() -> [i32; 3] {
    landing::up(RELPOS, Surface::Sphere { squares: 0x1000 })
}

/// A row-0 x coordinate whose F1521 product with `factor` is `wanted`.
fn row_for(wanted: i32, factor: i32) -> i32 {
    let (mut low, mut high) = (0i32, i32::MAX);
    while low < high {
        let middle = low + (high - low) / 2;
        if multiply(middle, factor) < wanted {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    assert_eq!(multiply(low, factor), wanted, "the product is reachable");
    low
}

fn contact(body: &mut Vec<u8>, planet: &[u8], flags: u8, crash: u8, player: u8) -> (u8, Touch) {
    let mut pair = Pair {
        body,
        planet,
        body_model: model(),
        planet_model: model(),
    };
    landing::contact(
        &mut pair,
        RELPOS,
        Surface::Sphere { squares: 0x1000 },
        flags,
        crash,
        player,
    )
}

/// The soft landing needs all three tests: speed at most `0x258`, the landing
/// word `0xffff` and a dot of at least `0x78000000`.
#[test]
fn landing_boundaries() {
    let up = up();
    let dot = |body: &mut Vec<u8>, value: i32| {
        set_dword(body, 0xc, value);
        set_dword(body, 0x10, 0);
        set_dword(body, 0x14, 0);
    };
    // The dot at the boundary, one below and one above.
    for (wanted, landing) in [
        (0x7800_0000, true),
        (0x77ff_ffff, false),
        (0x7800_0001, true),
    ] {
        let mut body = object();
        dot(&mut body, row_for(wanted, up[0]));
        assert_eq!(
            multiply(dword(&body, 0xc), up[0]),
            wanted,
            "the dot is the boundary value"
        );
        let (flags, touch) = contact(&mut body, &planet(), 0, 0xff, 0);
        if landing {
            assert_eq!(touch, Touch::Landing);
            assert_eq!(flags & 0xc0, 0x40);
        } else {
            assert_eq!(touch, Touch::Crash { damage: 0 });
            assert_eq!(flags & 0xc0, 0x80);
        }
    }

    // The speed: |vx| + |vy| + |vz| at most 0x258.
    for (speed, landing) in [(0x258, true), (0x257, true), (0x259, false)] {
        let mut body = object();
        aim(&mut body);
        set_dword(&mut body, 0x8c, speed);
        body[0x9e..0xa0].copy_from_slice(&0xffffu16.to_le_bytes());
        let (flags, touch) = contact(&mut body, &planet(), 0, 0xff, 0);
        if landing {
            assert_eq!(touch, Touch::Landing);
            assert_eq!(flags & 0xc0, 0x40);
        } else {
            assert_eq!(touch, Touch::Crash { damage: 0 });
            assert_eq!(flags & 0xc0, 0x80);
        }
    }

    // The landing word at +0x9e: 0xffff and nothing else.
    for (word, landing) in [(0xffffu16, true), (0xfffe, false)] {
        let mut body = object();
        aim(&mut body);
        body[0x9e..0xa0].copy_from_slice(&word.to_le_bytes());
        let (flags, touch) = contact(&mut body, &planet(), 0, 0xff, 0);
        if landing {
            assert_eq!(touch, Touch::Landing);
        } else {
            assert_eq!(touch, Touch::Crash { damage: 0 });
        }
        // Bit 2 is always set by the tail; landing also clears the crash bit and
        // sets 0x40.
        assert_eq!(flags & 0x2, 0x2);
    }
}

/// A landing sets `0x40` and clears `0x80`; the planet's two radii differ adds
/// flag 1 (`JUMP_002429`).
#[test]
fn landing_flags_and_different_radii() {
    let mut body = object();
    aim(&mut body);
    let (flags, touch) = contact(&mut body, &planet(), 0x80, 0xff, 0);
    assert_eq!(touch, Touch::Landing);
    assert_eq!(flags, 0x42, "2, 0x40 and the crash bit cleared");

    let mut planet = planet();
    set_qword(&mut planet, 0x138, 0x0fff);
    let (flags, _) = contact(&mut body, &planet, 0, 0xff, 0);
    assert_eq!(flags & 0x1, 0x1, "different radii");
    assert_eq!(flags & 0xc0, 0x40);

    // F574's scan has no crash mask: a fast touch still settles the body.
    let mut body = object();
    aim(&mut body);
    set_dword(&mut body, 0x8c, 0x1000);
    let (flags, touch) = contact(&mut body, &planet, 0, 0, 0);
    assert_eq!(touch, Touch::Landing);
    assert_eq!(flags & 0xc0, 0);
}

/// The bounce reflects the velocity as `2 v / 3 - 10 (up . v) up / 12` and
/// turns row 1 to the direction; the damage is the model's mass times four,
/// times the speed lost, over `0x708`, but at least the mass.
#[test]
fn bounce_velocity_and_damage() {
    let up = up();
    let mut body = object();
    let velocity = [600, 0, 600];
    for (k, v) in velocity.iter().enumerate() {
        set_dword(&mut body, 0x8c + 4 * k, *v);
    }
    let dot = multiply(up[0], velocity[0]);
    let expected: [i32; 3] = [
        velocity[0] * 2 / 3 - multiply(up[0], dot) * 10 / 12,
        0,
        velocity[2] * 2 / 3,
    ];
    let speed = velocity[0] + velocity[2];
    let mut planet = planet();
    planet[0x86] = 3;
    let mut pair = Pair {
        body: &mut body,
        planet: &planet,
        body_model: with_stats(100),
        planet_model: model(),
    };
    let damage = landing::bounce(&mut pair, up, speed);
    for (k, value) in expected.iter().enumerate() {
        assert_eq!(dword(&body, 0x8c + 4 * k), *value, "component {k}");
    }
    let new_speed = expected.iter().fold(0i32, |sum, v| sum + v.wrapping_abs());
    let loss = speed - new_speed;
    let expected_damage = ((400i64 * i64::from(loss)) / 0x708) as i32;
    assert_eq!(damage, expected_damage.max(100), "F953's amount");
    // Row 1 is the reflected direction, then F1520's orthogonalisation; F1519
    // rounds, so its length is near (not exactly) Q31.
    let length = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(multiply(
            dword(&body, 0x18 + 4 * k),
            dword(&body, 0x18 + 4 * k),
        ))
    });
    assert!(
        length > 0x7000_0000,
        "row 1 is nearly unit length ({length:#x})"
    );
}

/// A crash of the player's body bounces it; any other ship that is a missile or
/// an escape capsule (`+0x87` 0x0b or 0x0e) is turned to the surface and F953 is
/// given -1.
#[test]
fn crash_branches() {
    let mut body = object();
    set_dword(&mut body, 0xc, 0x7fff_ffff);
    body[0x82..0x86].copy_from_slice(&0x20u32.to_le_bytes());
    body[0x87] = 0xb;
    for (k, v) in [600, 0, 600].iter().enumerate() {
        set_dword(&mut body, 0x8c + 4 * k, *v);
    }
    let (flags, touch) = contact(&mut body, &planet(), 0, 0xff, 7);
    assert_eq!(flags & 0xc0, 0x80);
    match touch {
        Touch::Crash { damage } => {
            // bounce() ran: the velocity was reflected, so it is no longer 600.
            assert_ne!(dword(&body, 0x8c), 600);
            let _ = damage;
        }
        Touch::Landing => panic!("a fast touch is a crash"),
    }

    // The same body when it is not the player: the missile branch.
    let mut body = object();
    set_dword(&mut body, 0xc, 0x7fff_ffff);
    body[0x82..0x86].copy_from_slice(&0x20u32.to_le_bytes());
    body[0x87] = 0xb;
    for (k, v) in [600, 0, 600].iter().enumerate() {
        set_dword(&mut body, 0x8c + 4 * k, *v);
    }
    let (_, touch) = contact(&mut body, &planet(), 0, 0xff, 4);
    assert_eq!(touch, Touch::Crash { damage: -1 });
    for k in 0..3 {
        assert_eq!(dword(&body, 0x8c + 4 * k), 0, "the velocity is cleared");
    }

    // Another kind of body is only flagged.
    let mut body = object();
    set_dword(&mut body, 0xc, 0x7fff_ffff);
    set_dword(&mut body, 0x8c, 0x1000);
    body[0x82..0x86].copy_from_slice(&0x20u32.to_le_bytes());
    body[0x87] = 3;
    let (flags, touch) = contact(&mut body, &planet(), 0, 0xff, 4);
    assert_eq!(touch, Touch::Crash { damage: 0 });
    assert_eq!(flags & 0xc0, 0x80);
}
