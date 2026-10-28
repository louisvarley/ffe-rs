//! Port addition: the World Options page's "Improved" combat aids. The original
//! (Classic, the default) marks the combat target with F1111's circle and
//! F1100's range text, and an off-view target only with a fixed "Behind" or
//! arrow string; it has no list of the ships around the player.
//!
//! Improved adds three aids, all drawn over the flight view:
//! - brackets round the locked ship with its name (F1100 already writes the
//!   range beside it), which turn
//!   bright when the gun's ray would hit it now (the same F931/F817 test a shot
//!   makes) and amber when it is within laser range but off the sight;
//! - an arrow on the edge of the view pointing the way to turn when the
//!   locked ship is off the view or behind;
//! - a contacts list down the right of the view, hostiles first, each with a
//!   marker box that locks the ship when clicked (as clicking the ship does).
//!
//! The lasers are hitscan (F931 tests the ray when the trigger is pressed and
//! nothing travels), so there is no lead to show: a ship under the sight is a
//! ship that is hit. The on-target bracket is that test.

/// The contacts list's column: the right column of the navigation computer's
/// places list (x 230), below F1099's target hull/shields text (y 1..31).
pub const CONTACT_X: i32 = 230;
pub const CONTACT_TOP: i32 = 36;
/// Row pitch: the name and, a text line below it, the range.
pub const CONTACT_STEP: i32 = 20;
/// Rows that fit above the view's foot (y 158).
pub const CONTACT_ROWS: usize = 6;

/// Colours (COLPAL indices): F1099's combat red, the navigation green, and the
/// list's plain white.
pub const HOSTILE_COLOUR: u8 = 0x10;
pub const FRIENDLY_COLOUR: u8 = 0x11;
pub const PLAIN_COLOUR: u8 = 15;
/// The bright yellow the menus use for a selected row: "on target".
pub const ON_TARGET_COLOUR: u8 = 18;

/// The view rectangle the arrow stays inside (the 320x158 flight view, inset).
const EDGE_LEFT: f64 = 10.0;
const EDGE_RIGHT: f64 = 309.0;
const EDGE_TOP: f64 = 14.0;
const EDGE_BOTTOM: f64 = 146.0;
/// The view centre the projection uses (`160 + 256x/z`, `79 - 256y/z`).
const CENTRE: (f64, f64) = (160.0, 79.0);

/// One ship the list can show.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub id: u32,
    pub name: String,
    pub range: String,
    pub hostile: bool,
    pub locked: bool,
    /// Distance in position units, for the order.
    pub distance: f64,
}

/// The list in its drawn order: hostiles first, then the nearest, at most
/// [`CONTACT_ROWS`].
pub fn order(mut contacts: Vec<Contact>) -> Vec<Contact> {
    contacts.sort_by(|a, b| {
        b.hostile
            .cmp(&a.hostile)
            .then(a.distance.total_cmp(&b.distance))
            .then(a.id.cmp(&b.id))
    });
    contacts.truncate(CONTACT_ROWS);
    contacts
}

/// The top-left of row `index`'s marker box.
pub fn contact_cell(index: usize) -> Option<(i32, i32)> {
    (index < CONTACT_ROWS).then(|| (CONTACT_X, CONTACT_TOP + CONTACT_STEP * index as i32))
}

/// The row whose marker or text is under (x, y): the box and the two text
/// lines to the view's right edge.
pub fn contact_row(x: i32, y: i32) -> Option<usize> {
    (0..CONTACT_ROWS).find(|index| {
        contact_cell(*index)
            .is_some_and(|(cx, cy)| (cx..320).contains(&x) && (cy..cy + 18).contains(&y))
    })
}

/// The locked ship's brackets.
#[derive(Debug, Clone, PartialEq)]
pub struct Bracket {
    pub x: i32,
    pub y: i32,
    /// Half the bracket's side in pixels.
    pub half: i32,
    pub name: String,
    /// The gun's ray hits the ship now.
    pub on_target: bool,
    /// Within F931's laser range ([`ffe_core::laser::RANGE`]).
    pub in_range: bool,
}

impl Bracket {
    pub fn colour(&self) -> u8 {
        if self.on_target {
            ON_TARGET_COLOUR
        } else if self.in_range {
            0x5f
        } else {
            HOSTILE_COLOUR
        }
    }
}

/// The bracket's half side for a ship of `radius` pixels: never smaller than
/// the gunsight's six-pixel circle, never larger than a quarter of the view.
pub fn bracket_half(radius: i32) -> i32 {
    (radius + 3).clamp(8, 40)
}

/// The off-view arrow: where it sits on the view's edge and the unit screen
/// direction it points (x right, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrow {
    pub x: i32,
    pub y: i32,
    pub dx: f64,
    pub dy: f64,
}

/// The arrow for a target at `point` in the camera's frame (x right, y up,
/// z forward), or `None` when it is on the view. A target behind takes the
/// direction of its sideways offset (turning that way brings it round); one
/// dead astern points down.
pub fn edge_arrow(point: [f64; 3]) -> Option<Arrow> {
    let [x, y, z] = point;
    if z > 2.0 {
        let (sx, sy) = (CENTRE.0 + 256.0 * x / z, CENTRE.1 - 256.0 * y / z);
        if (EDGE_LEFT..=EDGE_RIGHT).contains(&sx) && (EDGE_TOP..=EDGE_BOTTOM).contains(&sy) {
            return None;
        }
    }
    let (mut dx, mut dy) = (x, -y);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        (dx, dy) = (0.0, 1.0);
    } else {
        (dx, dy) = (dx / length, dy / length);
    }
    // Run from the centre to the inset rectangle's edge.
    let reach_x = if dx > 0.0 {
        (EDGE_RIGHT - CENTRE.0) / dx
    } else if dx < 0.0 {
        (EDGE_LEFT - CENTRE.0) / dx
    } else {
        f64::INFINITY
    };
    let reach_y = if dy > 0.0 {
        (EDGE_BOTTOM - CENTRE.1) / dy
    } else if dy < 0.0 {
        (EDGE_TOP - CENTRE.1) / dy
    } else {
        f64::INFINITY
    };
    let reach = reach_x.min(reach_y);
    Some(Arrow {
        x: (CENTRE.0 + dx * reach).round() as i32,
        y: (CENTRE.1 + dy * reach).round() as i32,
        dx,
        dy,
    })
}

/// The arrow's three corners: a tip on the edge point and a base eight pixels
/// back along the direction, six wide.
pub fn arrow_points(arrow: Arrow) -> [(i32, i32); 3] {
    let (tx, ty) = (arrow.x as f64, arrow.y as f64);
    let (bx, by) = (tx - arrow.dx * 8.0, ty - arrow.dy * 8.0);
    let (px, py) = (-arrow.dy * 4.0, arrow.dx * 4.0);
    [
        (arrow.x, arrow.y),
        ((bx + px).round() as i32, (by + py).round() as i32),
        ((bx - px).round() as i32, (by - py).round() as i32),
    ]
}

/// Everything the Improved aids draw this frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overlay {
    pub bracket: Option<Bracket>,
    pub arrow: Option<Arrow>,
    pub contacts: Vec<Contact>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: u32, hostile: bool, distance: f64) -> Contact {
        Contact {
            id,
            name: format!("Ship {id}"),
            range: String::new(),
            hostile,
            locked: false,
            distance,
        }
    }

    #[test]
    fn hostiles_list_first_then_nearest_and_the_list_is_capped() {
        let list = order(vec![
            contact(1, false, 10.0),
            contact(2, true, 50.0),
            contact(3, false, 5.0),
            contact(4, true, 20.0),
        ]);
        let ids: Vec<u32> = list.iter().map(|c| c.id).collect();
        assert_eq!(ids, [4, 2, 3, 1]);
        let many = order((0..20).map(|id| contact(id, false, id as f64)).collect());
        assert_eq!(many.len(), CONTACT_ROWS);
        assert_eq!(many[0].id, 0);
    }

    #[test]
    fn contact_rows_sit_in_the_right_column_below_the_target_readout() {
        assert_eq!(contact_cell(0), Some((230, 36)));
        assert_eq!(contact_cell(5), Some((230, 136)));
        assert_eq!(contact_cell(6), None);
        assert_eq!(contact_row(232, 38), Some(0));
        assert_eq!(contact_row(300, 60), Some(1));
        assert_eq!(contact_row(229, 38), None);
        assert_eq!(contact_row(232, 30), None);
        // The last row's two lines end above the view's foot.
        assert!(contact_cell(CONTACT_ROWS - 1).unwrap().1 + 18 <= 158);
    }

    #[test]
    fn a_target_on_the_view_has_no_arrow() {
        assert_eq!(edge_arrow([0.0, 0.0, 100.0]), None);
        assert_eq!(edge_arrow([10.0, 5.0, 100.0]), None);
    }

    #[test]
    fn off_view_targets_point_from_the_edge_toward_them() {
        // Far right, in front: the arrow is on the right edge pointing right.
        let right = edge_arrow([1000.0, 0.0, 10.0]).unwrap();
        assert_eq!((right.x, right.y), (309, 79));
        assert!(right.dx > 0.99);
        // Above and behind: the top edge, pointing up (screen y negative).
        let up = edge_arrow([0.0, 50.0, -100.0]).unwrap();
        assert_eq!((up.x, up.y), (160, 14));
        assert!(up.dy < -0.99);
        // Dead astern points down.
        let astern = edge_arrow([0.0, 0.0, -100.0]).unwrap();
        assert_eq!((astern.x, astern.y), (160, 146));
    }

    #[test]
    fn the_arrow_tip_is_on_the_edge_and_its_base_is_inside() {
        let arrow = edge_arrow([-1000.0, 0.0, 10.0]).unwrap();
        let [tip, a, b] = arrow_points(arrow);
        assert_eq!(tip, (10, 79));
        assert!(a.0 > tip.0 && b.0 > tip.0);
        assert_eq!(a.1 + b.1, 2 * tip.1);
    }

    #[test]
    fn bracket_colour_reads_on_target_then_in_range() {
        let mut bracket = Bracket {
            x: 0,
            y: 0,
            half: bracket_half(2),
            name: String::new(),
            on_target: false,
            in_range: false,
        };
        assert_eq!(bracket.half, 8);
        assert_eq!(bracket.colour(), HOSTILE_COLOUR);
        bracket.in_range = true;
        assert_eq!(bracket.colour(), 0x5f);
        bracket.on_target = true;
        assert_eq!(bracket.colour(), ON_TARGET_COLOUR);
        assert_eq!(bracket_half(100), 40);
    }
}
