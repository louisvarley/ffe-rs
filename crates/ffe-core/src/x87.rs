//! The x87 extended-precision arithmetic the original's floating-point code runs
//! in: a 64-bit mantissa, round to nearest even, results stored as 32-bit floats
//! (each `fstp dword` rounds once more) or truncated to integers (`fistp` with the
//! truncating control word F2095 sets). Only what the planet atmosphere (F1877)
//! needs: conversions, `+ - * /` and the integer store.

#![allow(clippy::should_implement_trait)]

use std::cmp::Ordering;

/// An x87 register value: a normalised 64-bit mantissa (top bit set unless the
/// value is zero) times two to `exp - 63`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ext {
    neg: bool,
    exp: i32,
    mant: u64,
    kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Finite,
    Inf,
    Nan,
}

impl Ext {
    pub const ZERO: Ext = Ext {
        neg: false,
        exp: 0,
        mant: 0,
        kind: Kind::Finite,
    };
    const NAN: Ext = Ext {
        neg: true,
        exp: 0,
        mant: 0,
        kind: Kind::Nan,
    };

    fn inf(neg: bool) -> Ext {
        Ext {
            neg,
            exp: 0,
            mant: 0,
            kind: Kind::Inf,
        }
    }

    fn is_zero(&self) -> bool {
        self.kind == Kind::Finite && self.mant == 0
    }

    /// `fild`: an integer, exactly.
    pub fn from_i64(value: i64) -> Ext {
        if value == 0 {
            return Ext::ZERO;
        }
        let mant = value.unsigned_abs();
        let shift = mant.leading_zeros();
        Ext {
            neg: value < 0,
            exp: 63 - shift as i32,
            mant: mant << shift,
            kind: Kind::Finite,
        }
    }

    pub fn from_i32(value: i32) -> Ext {
        Ext::from_i64(i64::from(value))
    }

    /// `fld dword`: a 32-bit float, exactly.
    pub fn from_f32(value: f32) -> Ext {
        if value.is_nan() {
            return Ext::NAN;
        }
        if value.is_infinite() {
            return Ext::inf(value < 0.0);
        }
        let bits = value.to_bits();
        let neg = bits >> 31 != 0;
        let biased = ((bits >> 23) & 0xff) as i32;
        let fraction = u64::from(bits & 0x7f_ffff);
        if biased == 0 {
            if fraction == 0 {
                return Ext { neg, ..Ext::ZERO };
            }
            let shift = fraction.leading_zeros();
            return Ext {
                neg,
                exp: -126 - (shift as i32 - 40),
                mant: fraction << shift,
                kind: Kind::Finite,
            };
        }
        let mant = (fraction | 0x80_0000) << 40;
        Ext {
            neg,
            exp: biased - 127,
            mant,
            kind: Kind::Finite,
        }
    }

    /// Round `value * 2^low` (with `sticky` for bits already lost below) to a
    /// 64-bit mantissa.
    fn pack(neg: bool, value: u128, low: i32, sticky: bool) -> Ext {
        if value == 0 {
            return Ext { neg, ..Ext::ZERO };
        }
        let leading = 127 - value.leading_zeros() as i32;
        if leading <= 63 {
            let shift = 63 - leading;
            return Ext {
                neg,
                exp: leading + low,
                mant: (value << shift) as u64,
                kind: Kind::Finite,
            };
        }
        let shift = (leading - 63) as u32;
        let mut mant = (value >> shift) as u64;
        let rest = value & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        let up = match rest.cmp(&half) {
            Ordering::Greater => true,
            Ordering::Equal => sticky || mant & 1 == 1,
            Ordering::Less => false,
        };
        let mut exp = leading + low;
        if up {
            let (next, carry) = mant.overflowing_add(1);
            if carry {
                mant = 1 << 63;
                exp += 1;
            } else {
                mant = next;
            }
        }
        Ext {
            neg,
            exp,
            mant,
            kind: Kind::Finite,
        }
    }

    /// `fstp dword`: round to a 32-bit float (nearest even).
    pub fn to_f32(self) -> f32 {
        match self.kind {
            Kind::Nan => return f32::NAN,
            Kind::Inf => {
                return if self.neg {
                    f32::NEG_INFINITY
                } else {
                    f32::INFINITY
                }
            }
            Kind::Finite => {}
        }
        if self.mant == 0 {
            return if self.neg { -0.0 } else { 0.0 };
        }
        if self.exp < -126 {
            // A denormal: count units of 2^-149.
            let shift = (-86 - self.exp) as u32;
            if shift > 64 {
                return if self.neg { -0.0 } else { 0.0 };
            }
            let (mut top, rest, half) = if shift == 64 {
                (0u64, self.mant, 1u64 << 63)
            } else {
                (
                    self.mant >> shift,
                    self.mant & ((1u64 << shift) - 1),
                    1u64 << (shift - 1),
                )
            };
            if rest > half || (rest == half && top & 1 == 1) {
                top += 1;
            }
            let magnitude = f32::from_bits(top as u32);
            return if self.neg { -magnitude } else { magnitude };
        }
        let mut top = self.mant >> 40;
        let rest = self.mant & ((1u64 << 40) - 1);
        let half = 1u64 << 39;
        let mut exp = self.exp;
        if rest > half || (rest == half && top & 1 == 1) {
            top += 1;
            if top == 1 << 24 {
                top >>= 1;
                exp += 1;
            }
        }
        if exp > 127 {
            return if self.neg {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            };
        }
        let magnitude = f32::from_bits(((exp + 127) as u32) << 23 | (top as u32 & 0x7f_ffff));
        if self.neg {
            -magnitude
        } else {
            magnitude
        }
    }

    pub fn neg(self) -> Ext {
        Ext {
            neg: !self.neg,
            ..self
        }
    }

    pub fn add(self, other: Ext) -> Ext {
        match (self.kind, other.kind) {
            (Kind::Nan, _) | (_, Kind::Nan) => return Ext::NAN,
            (Kind::Inf, Kind::Inf) => {
                return if self.neg == other.neg {
                    self
                } else {
                    Ext::NAN
                }
            }
            (Kind::Inf, _) => return self,
            (_, Kind::Inf) => return other,
            _ => {}
        }
        if self.is_zero() {
            return if other.is_zero() {
                Ext {
                    neg: self.neg && other.neg,
                    ..Ext::ZERO
                }
            } else {
                other
            };
        }
        if other.is_zero() {
            return self;
        }
        // Make `a` the larger magnitude.
        let (a, b) = if (self.exp, self.mant) >= (other.exp, other.mant) {
            (self, other)
        } else {
            (other, self)
        };
        let shift = (a.exp - b.exp) as u32;
        let big = u128::from(a.mant) << 63;
        let (small, sticky) = {
            let full = u128::from(b.mant) << 63;
            if shift >= 127 {
                (0, true)
            } else {
                let lost = full & ((1u128 << shift) - 1);
                (full >> shift, lost != 0)
            }
        };
        let low = a.exp - 63 - 63;
        if a.neg == b.neg {
            Ext::pack(a.neg, big + small, low, sticky)
        } else {
            // big - small, where the lost bits make the true difference a little
            // smaller: borrow one unit and keep the sticky flag.
            let difference = big - small - u128::from(sticky);
            Ext::pack(a.neg, difference, low, sticky)
        }
    }

    pub fn sub(self, other: Ext) -> Ext {
        self.add(other.neg())
    }

    pub fn mul(self, other: Ext) -> Ext {
        let neg = self.neg != other.neg;
        match (self.kind, other.kind) {
            (Kind::Nan, _) | (_, Kind::Nan) => return Ext::NAN,
            (Kind::Inf, _) | (_, Kind::Inf) => {
                return if self.is_zero() || other.is_zero() {
                    Ext::NAN
                } else {
                    Ext::inf(neg)
                }
            }
            _ => {}
        }
        if self.is_zero() || other.is_zero() {
            return Ext { neg, ..Ext::ZERO };
        }
        let product = u128::from(self.mant) * u128::from(other.mant);
        Ext::pack(neg, product, self.exp + other.exp - 126, false)
    }

    pub fn div(self, other: Ext) -> Ext {
        let neg = self.neg != other.neg;
        match (self.kind, other.kind) {
            (Kind::Nan, _) | (_, Kind::Nan) => return Ext::NAN,
            (Kind::Inf, Kind::Inf) => return Ext::NAN,
            (Kind::Inf, _) => return Ext::inf(neg),
            (_, Kind::Inf) => return Ext { neg, ..Ext::ZERO },
            _ => {}
        }
        if other.is_zero() {
            return if self.is_zero() {
                Ext::NAN
            } else {
                Ext::inf(neg)
            };
        }
        if self.is_zero() {
            return Ext { neg, ..Ext::ZERO };
        }
        let numerator = u128::from(self.mant) << 64;
        let denominator = u128::from(other.mant);
        let quotient = numerator / denominator;
        let remainder = numerator % denominator;
        Ext::pack(neg, quotient, self.exp - other.exp - 64, remainder != 0)
    }

    /// `fistp qword` with truncation, the low 32 bits (what F2095 returns); the
    /// "integer indefinite" for a value that does not fit.
    pub fn truncate_low(self) -> i32 {
        if self.kind != Kind::Finite {
            return 0;
        }
        if self.mant == 0 || self.exp < 0 {
            return 0;
        }
        if self.exp >= 64 {
            return 0;
        }
        let magnitude = self.mant >> (63 - self.exp);
        if self.neg {
            if magnitude > 1 << 63 {
                return 0;
            }
            (magnitude as i64).wrapping_neg() as i32
        } else {
            if magnitude >= 1 << 63 {
                return 0;
            }
            magnitude as i64 as i32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_convert_exactly() {
        for value in [0, 1, -1, 7, 255, 123_456_789, i32::MIN, i32::MAX] {
            assert_eq!(Ext::from_i32(value).truncate_low(), value);
            assert_eq!(Ext::from_i32(value).to_f32(), value as f32);
        }
    }

    #[test]
    fn float_round_trip_and_arithmetic() {
        for (a, b) in [(1.5f32, 2.25f32), (-3.0, 0.1), (1e10, 3.0e-3), (7.0, -7.0)] {
            let (x, y) = (Ext::from_f32(a), Ext::from_f32(b));
            assert_eq!(x.to_f32(), a);
            // One rounding on the way out: the same as f64 for these operands.
            assert_eq!(x.add(y).to_f32(), (f64::from(a) + f64::from(b)) as f32);
            assert_eq!(x.mul(y).to_f32(), (f64::from(a) * f64::from(b)) as f32);
            assert_eq!(x.div(y).to_f32(), (f64::from(a) / f64::from(b)) as f32);
        }
    }

    #[test]
    fn division_by_zero_gives_the_indefinite_integer() {
        let q = Ext::from_i32(5).div(Ext::ZERO);
        assert_eq!(q.truncate_low(), 0);
        assert!(Ext::ZERO.div(Ext::ZERO).to_f32().is_nan());
    }

    #[test]
    fn truncation_goes_toward_zero() {
        let third = Ext::from_i32(1).div(Ext::from_i32(3));
        assert_eq!(third.truncate_low(), 0);
        let x = Ext::from_i32(-7).div(Ext::from_i32(2));
        assert_eq!(x.truncate_low(), -3);
    }
}
