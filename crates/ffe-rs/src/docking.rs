//! Docking at a system's stations in the playable: clearance, the slot on the
//! station's +z face, and the push clear after a bad approach. These are ffe-rs's
//! own rules, not yet the original's (traffic control F654/F655, the pad hit in
//! the collision stream and the docking states 0xa..0x18 are read but not ported:
//! `docs/NAVIGATION.md`). Positions are the original's millimetres.
use crate::navigation::Body;

/// The farthest a station answers a clearance request from (100 km).
pub const CLEARANCE_RANGE: f64 = 1.0e8;
/// The fastest the ship may enter the slot, in metres per second.
pub const MAX_ENTRY_SPEED: f64 = 250.0;
/// The narrowest slot corridor (metres either side of the docking axis).
const MIN_CORRIDOR: f64 = 60_000.0;

/// A station's docking port: its centre, the docking axis (the object's +z row)
/// and how far the face with the slot stands out along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port {
    pub centre: [i64; 3],
    pub axis: [f64; 3],
    /// Distance from the centre to the docking face (millimetres).
    pub front: f64,
    /// The model's bounding radius (millimetres).
    pub radius: f64,
}

/// What the ship's position means for a port this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Contact {
    None,
    Docked,
    /// The ship hit the station: it is put back at `clear` with no velocity.
    Collision {
        clear: [i64; 3],
        reason: Reason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// In the slot without clearance.
    NoClearance,
    /// In the slot faster than [`MAX_ENTRY_SPEED`].
    TooFast,
    /// Against the hull, away from the slot.
    Hull,
}

impl Reason {
    pub fn text(self) -> &'static str {
        match self {
            Reason::NoClearance => "no docking clearance (D requests it)",
            Reason::TooFast => "too fast for the docking slot",
            Reason::Hull => "hit the hull: enter along the docking axis",
        }
    }
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-12);
    v.map(|c| c / length)
}

impl Port {
    /// The port of a station body. `front` and `radius` come from its model's
    /// vertices (millimetres); a model whose root vertices stop short of the face
    /// (the orbital city's hub is a child model) takes a quarter of the radius.
    pub fn new(body: &Body, front: Option<f64>, radius: Option<f64>) -> Self {
        let radius = radius.unwrap_or(body.radius as f64);
        let front = front.filter(|f| *f > 0.1 * radius).unwrap_or(0.25 * radius);
        Self {
            centre: body.position,
            axis: unit(body.orientation[2]),
            front,
            radius,
        }
    }

    /// The point `along` millimetres out from the centre on the docking axis.
    pub fn point(&self, along: f64) -> [i64; 3] {
        std::array::from_fn(|k| self.centre[k] + (self.axis[k] * along).round() as i64)
    }

    /// Where a launch puts the ship: out from the face by twice the radius.
    pub fn launch_point(&self) -> [i64; 3] {
        self.point(self.front + 2.0 * self.radius + 2.0e6)
    }

    /// Half the width of the corridor leading into the slot.
    pub fn corridor(&self) -> f64 {
        (0.15 * self.front).max(MIN_CORRIDOR)
    }

    /// The ship at `ship` moving at `speed` metres per second, cleared or not.
    pub fn contact(&self, ship: [i64; 3], speed: f64, cleared: bool) -> Contact {
        let r: [f64; 3] = std::array::from_fn(|k| (ship[k] - self.centre[k]) as f64);
        let range = r.iter().map(|c| c * c).sum::<f64>().sqrt();
        if range >= self.front.max(self.corridor()) {
            return Contact::None;
        }
        let axial = r[0] * self.axis[0] + r[1] * self.axis[1] + r[2] * self.axis[2];
        let lateral = (range * range - axial * axial).max(0.0).sqrt();
        let back_out = self.point(self.front + 4.0 * self.corridor() + 1.0e6);
        if axial > 0.0 && lateral < self.corridor() {
            return if !cleared {
                Contact::Collision {
                    clear: back_out,
                    reason: Reason::NoClearance,
                }
            } else if speed > MAX_ENTRY_SPEED {
                Contact::Collision {
                    clear: back_out,
                    reason: Reason::TooFast,
                }
            } else {
                Contact::Docked
            };
        }
        let away = unit(if range > 1.0 { r } else { self.axis });
        let out = self.front * 1.5 + 1.0e6;
        Contact::Collision {
            clear: std::array::from_fn(|k| self.centre[k] + (away[k] * out).round() as i64),
            reason: Reason::Hull,
        }
    }
}
