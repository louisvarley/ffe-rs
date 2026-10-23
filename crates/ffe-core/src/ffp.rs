//! The original's fast floating point ("ffp"): a dword holding a 16-bit signed
//! mantissa (low word) and a 16-bit exponent (high word), ported from F1463,
//! F1482, F1483, F1486, F1489, F1491 and F1496. A normalised mantissa has bit 14
//! set (magnitude between 0x3fff and 0x7fff). Each function reproduces the
//! original's word truncations and quirks; see `docs/notes/physics.md`.

/// An ffp number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ffp {
    pub mantissa: i16,
    pub exponent: i16,
}

impl Ffp {
    pub const fn new(mantissa: i16, exponent: i16) -> Self {
        Ffp { mantissa, exponent }
    }

    /// The dword the original passes around (mantissa low, exponent high).
    pub fn to_dword(self) -> u32 {
        u32::from(self.mantissa as u16) | u32::from(self.exponent as u16) << 16
    }

    pub fn from_dword(value: u32) -> Self {
        Ffp {
            mantissa: value as i16,
            exponent: (value >> 16) as i16,
        }
    }
}

/// F1496: shift the mantissa left, lowering the exponent, until bit 14 is set. A
/// zero mantissa is returned unchanged.
pub fn normalise(value: Ffp) -> Ffp {
    let (mut mantissa, mut exponent) = (value.mantissa as u16, value.exponent);
    if mantissa == 0 {
        return value;
    }
    while mantissa & 0x4000 == 0 {
        mantissa <<= 1;
        exponent = exponent.wrapping_sub(1);
    }
    Ffp::new(mantissa as i16, exponent)
}

/// F1482: exponents add, mantissas multiply `>> 15`.
pub fn multiply(a: Ffp, b: Ffp) -> Ffp {
    let product = (i32::from(a.mantissa) * i32::from(b.mantissa)) >> 15;
    Ffp::new(product as i16, a.exponent.wrapping_add(b.exponent))
}

/// F1483: a zero divisor gives mantissa 0x7fff, exponent 0x7f; otherwise the
/// exponents subtract and the 16-bit quotient `(a << 14) / b` is doubled unless
/// its magnitude reached 0x4000 (then the exponent grows by one).
pub fn divide(a: Ffp, b: Ffp) -> Ffp {
    if b.mantissa == 0 {
        return Ffp::new(0x7fff, 0x7f);
    }
    let exponent = a.exponent.wrapping_sub(b.exponent);
    let quotient = ((i32::from(a.mantissa) << 14) / i32::from(b.mantissa)) as i16;
    if i32::from(quotient).abs() >= 0x4000 {
        Ffp::new(quotient, exponent.wrapping_add(1))
    } else {
        Ffp::new(quotient << 1, exponent)
    }
}

/// F1489: an integer to ffp. Zero is mantissa 0, exponent -7; a positive value
/// is doubled until bit 31 is set, counting the exponent down from 0x20; a
/// negative one is not shifted at all. The mantissa is the top 15 bits.
pub fn from_int(value: i32) -> Ffp {
    if value == 0 {
        return Ffp::new(0, -7);
    }
    let (mut value, mut exponent) = (value, 0x20i16);
    if value > 0 {
        loop {
            value = value.wrapping_add(value);
            exponent -= 1;
            if value <= 0 {
                break;
            }
        }
    }
    Ffp::new(((value as u32) >> 17) as i16, exponent)
}

/// F1491: ffp to a 32-bit integer: the mantissa in the top word, shifted right
/// by `0x1f - exponent` (the count wraps at 32 as `sar` does). Above exponent
/// 0x1f the original's "saturation" gives 0x80000000 for a negative mantissa
/// and 0 for a positive one.
pub fn to_int(value: Ffp) -> i32 {
    if value.exponent <= 0x1f {
        let count = (0x1f - i32::from(value.exponent)) as u32 & 31;
        (i32::from(value.mantissa) << 16) >> count
    } else if value.mantissa < 0 {
        i32::MIN
    } else {
        0
    }
}

/// F1486: addition. Operands more than 31 binades apart return the larger; the
/// smaller is shifted right by the exponent difference and added, a result over
/// 0x7fff in magnitude is halved (exponent + 1) and one under 0x3fff is
/// shifted left until it is not.
pub fn add(a: Ffp, b: Ffp) -> Ffp {
    let (high, low) = if a.exponent > b.exponent {
        (a, b)
    } else {
        (b, a)
    };
    if a.exponent > b.exponent {
        if i32::from(a.exponent) > i32::from(b.exponent) + 0x1f {
            return a;
        }
    } else if i32::from(b.exponent) > i32::from(a.exponent) + 0x1f {
        return b;
    }
    let shift = (i32::from(high.exponent) - i32::from(low.exponent)) as u32 & 31;
    let sum = (i32::from(low.mantissa) >> shift) + i32::from(high.mantissa);
    let mut exponent = high.exponent;
    if sum.abs() > 0x7fff {
        return Ffp::new((sum >> 1) as i16, exponent.wrapping_add(1));
    }
    let mut mantissa = sum as i16;
    if mantissa == 0 {
        return Ffp::new(0, exponent);
    }
    while i32::from(mantissa).abs() < 0x3fff {
        mantissa = mantissa.wrapping_shl(1);
        exponent = exponent.wrapping_sub(1);
    }
    Ffp::new(mantissa, exponent)
}

/// F1463: square root. The exponent is halved (rounding up when odd; an even
/// exponent doubles the mantissa instead) and the mantissa, in the top word,
/// goes through F1467's table root.
pub fn sqrt(value: Ffp) -> Ffp {
    let mut scaled = i32::from(value.mantissa) << 16;
    let exponent = if value.exponent & 1 != 0 {
        (value.exponent >> 1).wrapping_add(1)
    } else {
        scaled = scaled.wrapping_add(scaled);
        value.exponent >> 1
    };
    let root = crate::model_math::table_root(scaled as u32) >> 1;
    Ffp::new(root as i16, exponent)
}
