//! The original's object motion, ported routine by routine from `cphys.txt`
//! and `ffe.asm`: F601 and F602 (velocity from thrust over a time step) so far.
//! Verified against the executed original (`docs/notes/physics.md`).

/// The fields of a physics object (`PhysObj`) the motion routines read and
/// write, at their original offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Motion {
    /// The orientation (`+0x0..+0x23`): rows right, up, forward; Q31 dwords.
    pub orientation: [[i32; 3]; 3],
    /// Thrust words (`+0xb6`, `+0xb8`, `+0xba`): along right, up, forward.
    pub thrust: [i16; 3],
    /// Velocity (`+0x8c`, `+0x90`, `+0x94`) in world units.
    pub velocity: [i32; 3],
    /// The sub-unit velocity accumulators (`+0xec`, `+0xee`, `+0xf0`).
    pub fraction: [u16; 3],
    /// The velocity-change caps (`+0xf2`, `+0xf6`, `+0xfa`; each unaligned
    /// dword): a non-negative cap limits the change to `0..=cap`, a negative
    /// one to `cap..=0`.
    pub cap: [i32; 3],
}

/// One axis of F601's cap: the change `value` limited by `cap`.
fn capped(cap: i32, value: i32) -> i32 {
    let mut value = value;
    if cap >= 0 {
        if cap < value {
            value = cap;
        }
        if value < 0 {
            value = 0;
        }
    }
    if cap < 0 {
        if cap > value {
            value = cap;
        }
        if value > 0 {
            value = 0;
        }
    }
    value
}

impl Motion {
    /// F601: add the thrust over `tics` to the velocity. `tics >> 3` is cut to
    /// at most 0x7fff by halving (remembering the shifts); the thrust words
    /// are carried through the orientation rows (each coordinate's high 16
    /// bits times the time, `>> 15`, times the word), doubled, and then:
    /// below 0x4014 added whole to the fractional accumulators; otherwise
    /// capped by `cap`, with the accumulators for a time up to 0x7fff and
    /// without them (the value shifted right by the halvings) beyond.
    pub fn accelerate(&mut self, tics: u32) {
        self.integrate(tics, true, 0x10);
    }

    /// F602: F601 without the caps and without the medium regime's extra
    /// test: the accumulators for every time up to 0x7fff, the shifted value
    /// beyond it. It starts the halving count at 0xf, not 0x10, and shifts
    /// `tics` arithmetically (a signed time).
    pub fn accelerate_uncapped(&mut self, tics: u32) {
        self.integrate(tics, false, 0xf);
    }

    fn integrate(&mut self, tics: u32, caps: bool, halvings: i32) {
        // F601 shifts `tics` logically, F602 arithmetically.
        let mut time = if caps {
            (tics >> 3) as i32
        } else {
            (tics as i32) >> 3
        };
        let mut shift = -1;
        if time > 0x7fff {
            shift = halvings;
            while time > 0x7fff {
                time >>= 1;
                shift -= 1;
            }
        }
        let mut thrust = [0i32; 3];
        for (word, row) in self.thrust.iter().zip(self.orientation) {
            if *word == 0 {
                continue;
            }
            for axis in 0..3 {
                let part =
                    ((row[axis] >> 16).wrapping_mul(time) >> 15).wrapping_mul(i32::from(*word));
                thrust[axis] = thrust[axis].wrapping_add(part);
            }
        }
        let thrust = thrust.map(|v| v.wrapping_shl(1));
        if shift >= 0 {
            for axis in 0..3 {
                let mut change = thrust[axis] >> shift;
                if caps {
                    change = capped(self.cap[axis], change);
                }
                self.velocity[axis] = self.velocity[axis].wrapping_add(change);
            }
            return;
        }
        for axis in 0..3 {
            let total = i32::from(self.fraction[axis]).wrapping_add(thrust[axis]);
            self.fraction[axis] = total as u16;
            let mut change = total >> 16;
            if caps && time >= 0x4014 {
                change = capped(self.cap[axis], change);
            }
            self.velocity[axis] = self.velocity[axis].wrapping_add(change);
        }
    }
}
