//! The console scanner's contacts: `FUNC_000274`/`FUNC_000275` and
//! `FUNC_001569_DrawScannerStalk` (`ffe.asm:21239-21526`, `93327`).
//!
//! The scanner is the 92x30 ellipse at (101,175) whose ring is the console panel
//! art and whose 8 orientation frames are secondary bitmaps 254..261. F274 walks
//! the live object slots 0x72 down to 1 and F275 projects each contact:
//!
//! 1. the object's position relative to the player (F1678), cut to 28 bits
//!    (F1658) and divided by 0x131;
//! 2. rotated into the ship's frame (F1670, the matrix transpose);
//! 3. its length (F1466) tested against 0x50000 and scaled into the ring's
//!    0x3333..0x10000 radius band;
//! 4. `x = 0x93 + (x*45 >> 16)` and `y = 0xbb - (z*45 >> 16)*10/45`;
//! 5. a vertical stalk of `up/2` pixels in the contact's colour.
//!
//! F275 skips the player's own object, objects from another frame (the original's
//! `+0x56` parent), and starports (`+0x14c` bit 0x20). Contact colours come from
//! `DATA_008683` indexed by the statistics block's mass word (`+0x6 >> 4`); with
//! no statistics block (stars, planets, stations) the colour is 0x0f.
//!
//! The port's bodies carry absolute system positions (millimetres, the original's
//! position units), so the same-frame test is [`Contact::frame`] against
//! [`View::frame`]. Ships are in the player's frame by construction. The panel's
//! range rings and centre dot are the original art, not computed here.

/// `DATA_008683`, F259's contact colours (ffe.asm:20187-20223).
pub const CONTACT_COLOURS: [u8; 19] = [
    0x43, 0x11, 0x73, 0x73, 0x10, 0x10, 0x5f, 0x5f, 0x5f, 0x12, 0x12, 0x12, 0x08, 0x09, 0x0a, 0x0b,
    0x0c, 0x0d, 0x0e,
];

/// The colour for an object without a statistics block (`+0x38` null).
pub const DEFAULT_COLOUR: u8 = 0x0f;

/// F275's contact centre in the console (`x = 0x93 + dx`, `y = 0xbb - dz`).
pub const CENTRE: (i32, i32) = (0x93, 0xbb);

/// F275's radial band: contacts nearer than this are pushed out to it.
pub const NEAR_RADIUS: f64 = 0x3333 as f64;
/// Contacts farther than this are pulled in to the scanner's rim.
pub const FAR_RADIUS: f64 = 0x10000 as f64;
/// Contact beyond this length is not drawn at all (`0x50000`).
pub const MAX_RANGE: f64 = 0x50000 as f64;
/// F275's divisor applied to each relative coordinate before the rotation.
const POSITION_SCALE: f64 = 0x131 as f64;
/// F1658's truncation width: the relative vector's MSB is moved to bit 28.
const TRUNCATE_BITS: u32 = 0x1c;

/// One live object on the scanner. Positions are the system frame's original
/// units (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contact {
    pub position: [i64; 3],
    /// The object's parent frame (the original's `+0x56`); `None` is the root.
    pub frame: Option<usize>,
    /// The object's own system index for the "player's frame body" test.
    pub body: Option<usize>,
    /// The object's `+0x14c` class bit 0x20 (a type-0x23 starport), skipped.
    pub starport: bool,
    /// The statistics block's mass word (`+0x6`), which picks the colour.
    pub mass: Option<u16>,
}

/// The player's frame and the contacts in it, with the scanner orientation.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The player's position in system millimetres (the original's object).
    pub position: [i64; 3],
    /// The ship's right, up and forward axes in system space.
    pub axes: [[f64; 3]; 3],
    /// The body whose frame the player is in (the original's `player+0x56`).
    pub frame: Option<usize>,
    /// F273's `(DATA_008933 >> 9) & 7`, selecting secondary 254..261.
    pub orientation: usize,
    pub contacts: Vec<Contact>,
}

/// A projected contact: its ring position, the stalk's signed length and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub x: i32,
    pub y: i32,
    /// `up/2` in pixels; positive draws above the contact, negative below.
    pub stalk: i32,
    pub colour: u8,
}

/// F275's colour: `DATA_008683[mass >> 4]` while the mass word is below 0x12c,
/// else 0x0f. A missing statistics block is 0x0f too.
pub fn colour(mass: Option<u16>) -> u8 {
    match mass {
        Some(mass) if u32::from(mass) < 0x12c => CONTACT_COLOURS[usize::from(mass >> 4)],
        _ => DEFAULT_COLOUR,
    }
}

/// F1658 `Vec64Truncate`: shift the vector right until its largest component's
/// MSB is at `bits`. The following 32-bit maths assumes the cut.
pub fn truncate(vector: [i64; 3], bits: u32) -> [i64; 3] {
    let msb = vector
        .iter()
        .map(|component| {
            let magnitude = component.unsigned_abs();
            if magnitude == 0 {
                0
            } else {
                63 - magnitude.leading_zeros()
            }
        })
        .max()
        .unwrap_or(0);
    if msb > bits {
        let shift = msb - bits;
        vector.map(|component| component >> shift)
    } else {
        vector
    }
}

/// F275's same-frame filter: the object's parent is the player's frame, or the
/// object is the player's frame body itself. The player's own object is never in
/// the list. Starports are skipped (`+0x14c & 0x20`).
pub fn visible(contact: &Contact, frame: Option<usize>) -> bool {
    if contact.starport {
        return false;
    }
    contact.frame == frame || (frame.is_some() && contact.body == frame)
}

/// F275's projection. `relative` is the contact position minus the player's, in
/// original position units. `None` when F275 would skip it (beyond 0x50000).
pub fn project(relative: [i64; 3], axes: [[f64; 3]; 3]) -> Option<Mark> {
    let cut = truncate(relative, TRUNCATE_BITS).map(|component| component as f64 / POSITION_SCALE);
    // F1670: the matrix transpose times the vector puts it in the ship's frame.
    let rotated = axes.map(|row| row[0] * cut[0] + row[1] * cut[1] + row[2] * cut[2]);
    let distance =
        (rotated[0] * rotated[0] + rotated[1] * rotated[1] + rotated[2] * rotated[2]).sqrt();
    if distance > MAX_RANGE {
        return None;
    }
    // F275's two scaling branches pull far contacts to the rim and push near
    // ones out to the inner ring; the middle band is drawn at its true place.
    let scale = if distance <= 0.0 {
        1.0
    } else if distance > FAR_RADIUS {
        FAR_RADIUS / distance
    } else if distance < NEAR_RADIUS {
        NEAR_RADIUS / distance
    } else {
        1.0
    };
    // `(component * 45) >> 16` with the original's negative-value adjustment,
    // i.e. truncation toward zero.
    let scaled = rotated.map(|component| (component * scale * 45.0 / 65536.0).trunc());
    let x = CENTRE.0 + scaled[0] as i32;
    let y = CENTRE.1 - ((scaled[2] * 10.0) / 45.0) as i32;
    Some(Mark {
        x,
        y,
        // `sar 1` on the up component: floor, not truncation.
        stalk: (scaled[1] as i32) >> 1,
        colour: DEFAULT_COLOUR,
    })
}

impl View {
    /// The mark for one contact, with the contact's colour.
    pub fn mark(&self, contact: &Contact) -> Option<Mark> {
        let relative =
            std::array::from_fn(|axis| contact.position[axis].wrapping_sub(self.position[axis]));
        project(relative, self.axes).map(|mark| Mark {
            colour: colour(contact.mass),
            ..mark
        })
    }

    /// F275's visible marks, in the object list's order.
    pub fn marks(&self) -> Vec<Mark> {
        self.contacts
            .iter()
            .filter(|contact| visible(contact, self.frame))
            .filter_map(|contact| self.mark(contact))
            .collect()
    }
}

/// F273's scanner orientation word: `DATA_008933` is the accumulated yaw word,
/// decreased when the ship yaws toward its right (F611's 0x70 branch,
/// ffe.asm:39180-39234), so it is minus the port's yaw. `(word >> 9) & 7`
/// selects the 8-frame overlay.
pub fn orientation(yaw: f64) -> usize {
    let turn = std::f64::consts::TAU;
    let word = ((yaw.rem_euclid(turn) / turn) * 65536.0).round() as i64 as u16;
    let scanner = word.wrapping_neg();
    usize::from(scanner >> 9) & 7
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    #[test]
    fn colours_follow_the_mass_table() {
        assert_eq!(colour(None), 0x0f);
        assert_eq!(colour(Some(0)), 0x43);
        assert_eq!(colour(Some(16)), 0x11);
        assert_eq!(colour(Some(100)), 0x5f);
        assert_eq!(colour(Some(245)), 0x0b);
        assert_eq!(colour(Some(300)), 0x0f);
        assert_eq!(colour(Some(0xffff)), 0x0f);
    }

    #[test]
    fn truncate_moves_the_msb_to_the_cut() {
        assert_eq!(truncate([1, 2, 3], 0x1c), [1, 2, 3]);
        let cut = truncate([1 << 40, 0, 0], 0x1c);
        assert_eq!(cut[0], 1 << 0x1c);
        // The sign is kept by the arithmetic shift.
        let cut = truncate([-(1 << 40), 0, 0], 0x1c);
        assert_eq!(cut[0], -(1 << 0x1c));
    }

    #[test]
    fn the_centre_is_the_players_position() {
        let mark = project([0, 0, 0], AXES).unwrap();
        assert_eq!((mark.x, mark.y), (147, 187));
        assert_eq!(mark.stalk, 0);
    }

    #[test]
    fn lateral_and_forward_offsets_use_the_screen_axes() {
        // The rim after F275's divide is 0x10000, which is 45 screen pixels.
        let rim = (FAR_RADIUS * 0x131 as f64) as i64;
        let mark = project([rim, 0, 0], AXES).unwrap();
        assert_eq!(mark.x, 147 + 45);
        assert_eq!(mark.y, 187);
        let mark = project([0, 0, rim], AXES).unwrap();
        assert_eq!(mark.x, 147);
        assert_eq!(mark.y, 187 - 10);
        // Up does not move the contact: it sets the stalk (floor of up/2).
        let mark = project([0, rim, 0], AXES).unwrap();
        assert_eq!(mark.stalk, 22);
        let mark = project([0, -rim, 0], AXES).unwrap();
        assert_eq!(mark.stalk, -23, "a signed right shift floors");
        assert!(project([0, 0, -rim], AXES).is_some());
    }

    #[test]
    fn far_contacts_clamp_to_the_rim_and_beyond_is_skipped() {
        let near = project([1_000, 0, 0], AXES).unwrap();
        assert_eq!(
            near.x,
            147 + 8,
            "a contact inside the inner ring is pushed out to it"
        );
        assert_eq!(near.y, 187);
        let rim = (FAR_RADIUS * 0x131 as f64) as i64;
        let far = project([2 * rim, 0, 0], AXES).unwrap();
        assert_eq!(far.x, 147 + 45);
        let beyond = (MAX_RANGE * 0x131 as f64) as i64 + 1;
        assert_eq!(project([0, 0, beyond], AXES), None);
    }

    #[test]
    fn the_rotation_uses_the_ship_axes() {
        // A contact ahead in system space appears at the top of the ring when
        // the nose points at it.
        let axes = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        let mark = project([20_000_000, 0, 0], axes).unwrap();
        assert_eq!((mark.x, mark.y), (147, 187 - 10));
    }

    #[test]
    fn frame_and_starport_filters_match_f275() {
        let mut contact = Contact {
            position: [0; 3],
            frame: None,
            body: Some(2),
            starport: false,
            mass: None,
        };
        assert!(visible(&contact, None));
        assert!(visible(&contact, Some(2)), "the frame body itself");
        contact.frame = Some(2);
        assert!(!visible(&contact, None));
        assert!(visible(&contact, Some(2)));
        contact.body = Some(4);
        contact.frame = Some(3);
        assert!(!visible(&contact, Some(2)), "another frame's body");
        let starport = Contact {
            starport: true,
            ..contact
        };
        assert!(!visible(&starport, Some(2)));
    }

    #[test]
    fn orientation_selects_one_of_the_eight_overlays() {
        assert_eq!(orientation(0.0), 0);
        // F273 takes (word >> 9) & 7: one step is 512 of the 65536 angle units.
        assert_eq!(orientation(std::f64::consts::TAU / 128.0), 7);
        assert_eq!(orientation(std::f64::consts::TAU / 64.0), 6);
        // The word is minus the yaw, so the frame runs the other way.
        assert_eq!(orientation(-std::f64::consts::TAU / 128.0), 1);
        // The eight-frame set repeats every 8 * 512 units (22.5 degrees).
        assert_eq!(orientation(std::f64::consts::TAU / 16.0), orientation(0.0));
    }
}
