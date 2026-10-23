//! The player's thrust limits: F598 (request clamp), F599 (per-axis clamp) and
//! F600 (proportional scale to the limits). An object holds a maximum and a
//! minimum word per axis (x +0xbc/+0xbe, y +0xc0/+0xc2, z +0xc4/+0xc6; the
//! maximum is positive thrust) and receives the result as the words
//! +0xb6/+0xb8/+0xba. Verified against the executed original
//! (`docs/notes/thrust.md`).
use crate::model::multiply;
use crate::model_math::fraction;

/// An object's thrust limits: `[maximum, minimum]` per axis (x, y, z).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    pub axes: [[i16; 2]; 3],
}

/// The result: the three dwords the routine returns and the three words it
/// stores in the object (the dwords' low 16 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thrust {
    pub vector: [i32; 3],
    pub words: [u16; 3],
}

impl Thrust {
    fn new(vector: [i32; 3]) -> Self {
        Self {
            vector,
            words: vector.map(|v| v as u16),
        }
    }
}

/// F598: x and y reduced to full deflection in the request's direction (a
/// negative request is -32767, anything else +32767), z shifted left by three,
/// then each clamped to its `[minimum, maximum]` (the maximum is checked first,
/// then the minimum, so a minimum above the maximum wins).
pub fn request_clamp(limits: &Limits, request: [i32; 3]) -> Thrust {
    let full = |v: i32| if v < 0 { -0x7fff } else { 0x7fff };
    let mut out = [
        full(request[0]),
        full(request[1]),
        request[2].wrapping_shl(3),
    ];
    for (value, [max, min]) in out.iter_mut().zip(limits.axes) {
        if i32::from(max) < *value {
            *value = i32::from(max);
        }
        if i32::from(min) > *value {
            *value = i32::from(min);
        }
    }
    Thrust::new(out)
}

/// F599: the request clamped per axis to `[minimum, maximum]`.
pub fn clamp(limits: &Limits, request: [i32; 3]) -> Thrust {
    let mut out = request;
    for ((value, req), [max, min]) in out.iter_mut().zip(request).zip(limits.axes) {
        if i32::from(max) < req {
            *value = i32::from(max);
        }
        if i32::from(min) > req {
            *value = i32::from(min);
        }
    }
    Thrust::new(out)
}

/// F600: one scale for the whole vector. Each axis in turn with a request above
/// its maximum sets the scale to `fraction(max, request)` and, below its
/// minimum, to `fraction(-minimum, -request)`; a later axis replaces an earlier
/// one (the last out-of-range axis wins, as in the original). The scale starts
/// at 0x7fffffff (1.0), and each request component is multiplied by it (F1521).
pub fn scale_to_limits(limits: &Limits, request: [i32; 3]) -> Thrust {
    let mut scale = 0x7fff_ffff_i32;
    for (req, [max, min]) in request.into_iter().zip(limits.axes) {
        let (max, min) = (i32::from(max), i32::from(min));
        if max < req {
            scale = fraction(max as u32, req as u32) as i32;
        }
        if min > req {
            scale = fraction(min.wrapping_neg() as u32, req.wrapping_neg() as u32) as i32;
        }
    }
    Thrust::new(request.map(|req| multiply(scale, req)))
}
