//! The original's object motion, ported routine by routine from `cphys.txt`
//! and `ffe.asm`: F601 and F602 (velocity from thrust over a time step) and the
//! orientation update F585 with F584, F611 and F1520 (F1519, F1552) so far.
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
    /// The drift counter (`+0x24`, a signed byte): F611 lowers it as it rotates
    /// and F1520 re-orthogonalises the matrix when it goes negative.
    pub drift: i8,
    /// The angular rates (`+0xb0` pitch, `+0xb2` roll, `+0xb4` yaw).
    pub rate: [i16; 3],
    /// The dword read at `+0x82`: the model number, whose higher bytes F585
    /// reads along with it.
    pub model: u32,
    /// The craft number byte (`+0x86`), compared with the player's.
    pub craft: u8,
    /// The autopilot's turn rates (`+0x110`, `+0x112`, `+0x114`).
    pub autopilot: [i16; 3],
    /// The parent object's index (`+0x56`): zero when the object is in free
    /// space and no frame of reference applies.
    pub parent: u8,
    /// The position (`+0x3e`, `+0x46`, `+0x4e`): three 64-bit coordinates.
    pub position: [i64; 3],
    /// Gun mounts (`+0xd1`, a signed byte): four or more allow the bottom turret.
    pub gun_mounts: i8,
}

/// The scanner state F585 touches: the player's craft number (`DATA_008857`)
/// and the scanner rotation word (`DATA_008933`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scanner {
    pub player: u32,
    pub rotation: u16,
}

/// F611's drift decrements, indexed by the step number minus two (the ten bytes
/// at `DATA_006159`).
const DRIFT: [u8; 10] = [0x12, 3, 3, 3, 5, 2, 1, 1, 1, 1];

/// F584: the largest step size (and its code) not above `value`; below the
/// smallest, 0x14 with code 9. Returns `(code, size)`.
fn step_size(value: i32) -> (u32, i32) {
    const STEPS: [(i32, u32); 7] = [
        (0x9fb, 2),
        (0x511, 3),
        (0x28b, 4),
        (0x146, 5),
        (0xa3, 6),
        (0x52, 7),
        (0x29, 8),
    ];
    for (size, code) in STEPS {
        if value >= size {
            return (code, size);
        }
    }
    (9, 0x14)
}

fn find_msb(value: u32) -> u32 {
    31u32.saturating_sub(value.leading_zeros())
}

/// F1519: a vector scaled to unit length (Q31): the reciprocal of its length
/// through F1523, applied with F1521 and shifted back up.
fn normalise(vector: [i32; 3]) -> [i32; 3] {
    let mut scaled = crate::model_math::length(vector).wrapping_add(1);
    let shift = 31 - find_msb(scaled);
    scaled = scaled.wrapping_shl(shift) >> 1;
    let reciprocal = (crate::model_math::fraction(0x3f80_0000, scaled) >> 1).wrapping_sub(1) as i32;
    vector.map(|c| crate::model::multiply(c, reciprocal).wrapping_shl(shift + 1))
}

/// F1552: the cross product `a x b` with F1521 products.
fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    use crate::model::multiply as m;
    [
        m(a[1], b[2]).wrapping_sub(m(a[2], b[1])),
        m(a[2], b[0]).wrapping_sub(m(a[0], b[2])),
        m(a[0], b[1]).wrapping_sub(m(a[1], b[0])),
    ]
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

/// F1515: `a * b >> 16` with `a` taken as two 16-bit halves (so `a` is
/// unsigned): the low half's product shifted down plus the high half's product.
fn split_multiply(a: u32, b: i32) -> i32 {
    let low = ((a & 0xffff) as i32).wrapping_mul(b) as u32 >> 16;
    let high = ((a >> 16) as i32).wrapping_mul(b);
    (low as i32).wrapping_add(high)
}

/// F1516: twice F1515, with the sign of `b` applied to the result.
fn signed_multiply(a: u32, b: i32) -> i32 {
    if b < 0 {
        split_multiply(a, b.wrapping_neg())
            .wrapping_add(split_multiply(a, b.wrapping_neg()))
            .wrapping_neg()
    } else {
        split_multiply(a, b).wrapping_mul(2)
    }
}

/// F1496: normalise an ffp number (mantissa, exponent): shift the mantissa left,
/// decrementing the exponent, until bit 14 is set. A zero mantissa is returned
/// unchanged.
fn normalise_ffp(mantissa: u16, exponent: u16) -> (u16, u16) {
    let (mut mantissa, mut exponent) = (mantissa, exponent);
    if mantissa == 0 {
        return (mantissa, exponent);
    }
    while mantissa & 0x4000 == 0 {
        mantissa <<= 1;
        exponent = exponent.wrapping_sub(1);
    }
    (mantissa, exponent)
}

/// F1482: ffp multiply: exponents add, mantissas multiply `>> 15`.
fn multiply_ffp(a: (u16, u16), b: (u16, u16)) -> (u16, u16) {
    let product = (i32::from(a.0 as i16) * i32::from(b.0 as i16)) >> 15;
    (product as u16, a.1.wrapping_add(b.1))
}

/// F1483: ffp divide. A zero divisor gives the largest number (mantissa
/// 0x7fff, exponent 0x7f); otherwise the exponents subtract and the mantissa
/// quotient `(a << 14) / b` is doubled unless it reached 0x4000.
fn divide_ffp(a: (u16, u16), b: (u16, u16)) -> (u16, u16) {
    if b.0 == 0 {
        return (0x7fff, 0x7f);
    }
    let exponent = a.1.wrapping_sub(b.1);
    let quotient = ((i32::from(a.0 as i16) << 14) / i32::from(b.0 as i16)) as i16;
    if i32::from(quotient).abs() >= 0x4000 {
        (quotient as u16, exponent.wrapping_add(1))
    } else {
        ((quotient as u16) << 1, exponent)
    }
}

/// A 64-bit value as the original's pair of dwords (`high`, `low`).
fn split64(value: i64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}

fn join64(high: u32, low: u32) -> i64 {
    (u64::from(high) << 32 | u64::from(low)) as i64
}

impl Motion {
    /// F1512/F1511: add the velocity over a 64-bit signed `time` to the
    /// position. The magnitude is shifted right seven and then until it fits
    /// fifteen bits, remembering the shift count; each velocity coordinate
    /// times that value, arithmetically shifted down two and back up by the
    /// count (in the original's 32-bit halves, whose shift counts wrap at 32),
    /// negated for a negative time, is added to the 64-bit coordinate.
    pub fn advance(&mut self, time: i64) {
        let negative = time < 0;
        let magnitude = if negative { time.wrapping_neg() } else { time };
        let shifted = (magnitude as u64) >> 7;
        let (high, mut low) = ((shifted >> 32) as u32, shifted as u32);
        let mut count = 0u32;
        if high != 0 {
            count = find_msb(high) + 1;
            low = (low >> count) | high.wrapping_shl(32 - count);
        }
        let extra = (find_msb(low) as i32 - 14).max(0) as u32;
        count += extra;
        low >>= extra;
        for (coordinate, velocity) in self.position.iter_mut().zip(self.velocity) {
            let product = (i64::from(velocity) * i64::from(low)) >> 2;
            let (mut h, mut l) = split64(product);
            if count != 0 {
                h = h.wrapping_shl(count) | l.wrapping_shr(32u32.wrapping_sub(count));
                l = l.wrapping_shl(count);
            }
            let mut delta = join64(h, l);
            if negative {
                delta = delta.wrapping_neg();
            }
            *coordinate = coordinate.wrapping_add(delta);
        }
    }

    /// F607: the thrust request that brings the object to `speed` along its
    /// forward axis. `speed + speed / 100` (truncating) times the forward row's
    /// high words (F1516) less the current velocity gives the velocity change,
    /// which is stored as the F601 caps (`+0xf2`, `+0xf6`, `+0xfa`) and carried
    /// through the orientation (F1670) and shifted left two.
    pub fn set_speed_request(&mut self, speed: i32) -> [i32; 3] {
        let target = speed.wrapping_add(speed.wrapping_div(100));
        let forward = self.orientation[2];
        let along = |component: i32| {
            if target < 0 {
                signed_multiply(target.wrapping_neg() as u32, component >> 16).wrapping_neg()
            } else {
                signed_multiply(target as u32, component >> 16)
            }
        };
        let change: [i32; 3] =
            std::array::from_fn(|axis| along(forward[axis]).wrapping_sub(self.velocity[axis]));
        self.cap = change;
        let matrix: [i32; 9] = std::array::from_fn(|i| self.orientation[i / 3][i % 3]);
        crate::model_math::transform_rows(change, &matrix).map(|v| v.wrapping_shl(2))
    }

    /// F1520: re-orthogonalise the orientation: right and forward normalised,
    /// up = forward x right normalised, right = up x forward.
    pub fn renormalise(&mut self) {
        let [right, up, forward] = &mut self.orientation;
        *right = normalise(*right);
        *forward = normalise(*forward);
        *up = cross(*forward, *right);
        *up = normalise(*up);
        *right = cross(*up, *forward);
    }

    /// F611: rotate row `a` toward row `b` (or away, for `plus` false) by a step
    /// of 2^-n: for n up to 5 both rows are first shrunk by `2^-(2n+1)`, then
    /// `b' = b +- (a >> n)` and `a' = a -+ (b >> n)` with the shrunk `b`.
    /// The drift counter then falls by the step's byte, and when it goes
    /// negative F1520 runs and the counter restarts at 0x3c.
    fn turn(&mut self, a: usize, b: usize, plus: bool, n: u32) {
        let (mut row_a, mut row_b) = (self.orientation[a], self.orientation[b]);
        if n <= 5 {
            let shift = 2 * n + 1;
            row_a = row_a.map(|v| v.wrapping_sub(v >> shift));
            row_b = row_b.map(|v| v.wrapping_sub(v >> shift));
        }
        let new_b: [i32; 3] = std::array::from_fn(|i| {
            let moved = row_a[i] >> n;
            if plus {
                moved.wrapping_add(row_b[i])
            } else {
                row_b[i].wrapping_sub(moved)
            }
        });
        let moved_b = row_b.map(|v| v >> n);
        let new_a: [i32; 3] = std::array::from_fn(|i| {
            if plus {
                row_a[i].wrapping_sub(moved_b[i])
            } else {
                row_a[i].wrapping_add(moved_b[i])
            }
        });
        self.orientation[a] = new_a;
        self.orientation[b] = new_b;
        self.drift = self.drift.wrapping_sub(DRIFT[n as usize - 2] as i8);
        if self.drift < 0 {
            self.renormalise();
            self.drift = 0x3c;
        }
    }

    /// F585: apply the angular rates over `time`. The time is halved for the
    /// larger craft models (0x35 and up, again from 0x3b); with `autopilot` set
    /// the rates first gain the autopilot's turn rates scaled by `ai_time`
    /// (capped at 0x7fff, `* rate >> 16`). Each rate (pitch, roll, yaw) of at
    /// least 0xf, limited by the time (itself capped at 0x7fff), is spent in the
    /// largest step F584 offers and turns the matching row pair; the yaw also
    /// turns the scanner when this is the player's craft.
    pub fn orient(&mut self, time: i32, autopilot: bool, ai_time: i32, scanner: &mut Scanner) {
        let mut limit = time;
        if self.model >= 0x35 {
            limit >>= 1;
            if self.model >= 0x3b {
                limit >>= 1;
            }
        }
        if autopilot {
            let ai_time = ai_time.min(0x7fff);
            for (rate, turn) in self.rate.iter_mut().zip(self.autopilot) {
                *rate = rate.wrapping_add(((i32::from(turn) * ai_time) >> 16) as i16);
            }
        }
        let limit = limit.min(0x7fff);
        // (rate index, rows turned (a, b), scanner turns with it)
        for (axis, (a, b)) in [(0usize, (2usize, 1usize)), (1, (1, 0)), (2, (0, 2))] {
            let rate = i32::from(self.rate[axis]);
            let positive = rate >= 0;
            let wanted = rate.abs().min(limit);
            if wanted < 0xf {
                continue;
            }
            let (code, size) = step_size(wanted);
            self.rate[axis] = if positive {
                self.rate[axis].wrapping_sub(size as i16)
            } else {
                self.rate[axis].wrapping_add(size as i16)
            };
            self.turn(a, b, positive, code);
            if axis == 2 && u32::from(self.craft) == scanner.player {
                scanner.rotation = if positive {
                    scanner.rotation.wrapping_sub(size as u16)
                } else {
                    scanner.rotation.wrapping_add(size as u16)
                };
            }
        }
    }

    /// F609: the roll the autopilot applies to keep the right axis level
    /// against `up`, the truncated relative position F603 leaves in
    /// `DATA_008708..DATA_008710`: the right row's high words dotted with it,
    /// shifted right 18 and negated into the 16-bit rate at `+0x112`.
    pub fn auto_roll(&mut self, up: [i32; 3]) {
        let dot = self.orientation[0]
            .iter()
            .zip(up)
            .fold(0i32, |sum, (row, up)| {
                sum.wrapping_add((row >> 16).wrapping_mul(up))
            });
        self.autopilot[1] = ((dot >> 0x12) as i16).wrapping_neg();
    }

    /// F603: the pull of the parent body on an object in its frame of
    /// reference. `mass` is the parent's dword at `+0xa4` (an ffp number: a
    /// 16-bit mantissa and exponent; a zero mantissa means no gravity) and
    /// `time` the signed time step. The position relative to the parent is cut
    /// to 13 bits (F1658, remembering the shift), its length found (F1466)
    /// and cubed in ffp (F1482), the mass divided by that (F1483), and the
    /// result times the time and the negated relative position is added to the
    /// velocity: directly when large, through the fraction accumulators when
    /// small. Returns the truncated relative position the original leaves in
    /// `DATA_008708..DATA_008710`, or `None` when it returns early.
    pub fn gravity(&mut self, mass: u32, time: i32) -> Option<[i32; 3]> {
        if self.parent == 0 || mass as u16 == 0 {
            return None;
        }
        let msb = crate::model_animation::vector_msb(self.position);
        let cut = msb as i32 - 0xd;
        let (relative, exponent) = if cut > 0 {
            (
                crate::model_animation::shift_vector(self.position, cut as u32),
                cut,
            )
        } else {
            (self.position, 0)
        };
        let relative = relative.map(|v| v as i32);
        let published = relative;
        let length = crate::model_math::length(relative);
        let near = normalise_ffp(length as u16, exponent as u16);
        if near.0 == 0 {
            return Some(published);
        }
        let square = multiply_ffp(near, near);
        let cube = (
            ((i32::from(near.0) * i32::from(square.0)) >> 15) as u16,
            square.1,
        );
        let cube = normalise_ffp(cube.0, cube.1);
        let pull = divide_ffp(((mass & 0xffff) as u16, (mass >> 16) as u16), cube);
        let (pull_mantissa, mut pull_exponent) = (pull.0, pull.1 as i16);
        let mut time = time;
        if time < 0 {
            while time < -0x7fff {
                pull_exponent = pull_exponent.wrapping_add(1);
                time >>= 1;
            }
        } else {
            while time > 0x7fff {
                pull_exponent = pull_exponent.wrapping_add(1);
                time >>= 1;
            }
        }
        let scale = (i32::from(pull_mantissa as i16).wrapping_mul(time) >> 15).wrapping_neg();
        let mut vector = relative.map(|v| v.wrapping_mul(scale));
        if pull_exponent > 0 {
            let remaining = pull_exponent.wrapping_sub(0x10);
            if remaining < 0 {
                let count = -i32::from(remaining);
                vector = vector.map(|v| v >> (count & 31));
            }
            for (velocity, delta) in self.velocity.iter_mut().zip(vector) {
                *velocity = velocity.wrapping_add(delta);
            }
            return Some(published);
        }
        if pull_exponent <= -0x20 {
            return Some(published);
        }
        if pull_exponent != 0 {
            let count = -i32::from(pull_exponent);
            vector = vector.map(|v| v >> (count & 31));
        }
        for ((fraction, velocity), delta) in
            self.fraction.iter_mut().zip(&mut self.velocity).zip(vector)
        {
            let total = i32::from(*fraction).wrapping_add(delta);
            *fraction = total as u16;
            *velocity = velocity.wrapping_add(total >> 16);
        }
        Some(published)
    }

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

    // Each axis indexes four parallel arrays (thrust, cap, velocity, fraction).
    #[allow(clippy::needless_range_loop)]
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
            for (sum, coordinate) in thrust.iter_mut().zip(row) {
                let part =
                    ((coordinate >> 16).wrapping_mul(time) >> 15).wrapping_mul(i32::from(*word));
                *sum = sum.wrapping_add(part);
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
