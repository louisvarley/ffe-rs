//! Curved-surface helpers (opcode 31, F1874/F1876). F1874's resolution lives in
//! [`crate::model_execute::SurfaceEmission`]; the helpers F1876 uses are ported
//! here as their use is established. F1876's patch rendering itself is not
//! ported yet; see `docs/notes/surfaces.md`.

/// F1875: the sign of the 2D cross product `a.x * b.y - a.y * b.x`, computed in
/// exact 64-bit arithmetic and returned as the original does — `-1` when the
/// difference is negative, `0` otherwise (it never returns `1`). Verified
/// against the executed original on twelve synthetic pairs
/// (`jjffe-commands.exe --surface-sign`), including both `i32::MIN` cases.
pub fn cross_sign(a: [i32; 2], b: [i32; 2]) -> i32 {
    let difference = i64::from(a[0]) * i64::from(b[1]) - i64::from(a[1]) * i64::from(b[0]);
    if difference < 0 {
        -1
    } else {
        0
    }
}

/// F1824's 32-bit mix: rotate/shift/or a value, add it, then fold the halves and
/// add the original again. Wrapping throughout; verified against the executed
/// original on eight words (`jjffe-commands.exe --surface-helpers`).
pub fn mix32(x: u32) -> u32 {
    let mut value = (x >> 27) | x.wrapping_shl(5);
    value = value.wrapping_add(x);
    let folded = (value >> 16) | value.wrapping_shl(16);
    x.wrapping_add(folded)
}

/// F1825: the MSB (floor of log2, zero for zero) of the largest absolute
/// coordinate. A two's-complement `neg` leaves `i32::MIN` at `0x80000000`, whose
/// MSB is 31. Verified against the executed original on nine points.
pub fn max_abs_msb(point: [i32; 3]) -> u32 {
    let mut combined = 0u32;
    for axis in point {
        combined |= axis.unsigned_abs();
    }
    find_msb(combined)
}

/// F1656: floor(log2(value)), and 0 for zero.
fn find_msb(value: u32) -> u32 {
    if value == 0 {
        0
    } else {
        31 - value.leading_zeros()
    }
}

#[cfg(test)]
mod tests {
    use super::{cross_sign, max_abs_msb, mix32};

    /// The probe's twelve pairs and their original results.
    #[test]
    fn cross_sign_matches_the_original_on_the_probe_pairs() {
        let cases: [([i32; 2], [i32; 2], i32); 12] = [
            ([1, 0], [0, 1], 0),
            ([0, 1], [1, 0], -1),
            ([3, 4], [-4, 3], 0),
            ([0, 0], [1, 1], 0),
            ([-1, 0], [0, -1], 0),
            ([0x4000_0000, 1], [1, 0x4000_0000], 0),
            ([-0x4000_0000, 0x7fff_ffff], [0x7fff_ffff, -0x4000_0000], -1),
            ([i32::MIN, 1], [1, i32::MIN], 0),
            ([0x7fff_ffff, -0x7fff_ffff], [-0x7fff_ffff, 0x7fff_ffff], 0),
            ([0x1_0000, 0x1_0000], [-0x1_0000, 0x1_0000], 0),
            ([-0x1_0000, 0x1_0000], [0x1_0000, 0x1_0000], -1),
            ([123_456_789, -987_654_321], [987_654_321, 123_456_789], 0),
        ];
        for (a, b, expected) in cases {
            assert_eq!(cross_sign(a, b), expected, "{a:?} x {b:?}");
        }
    }

    #[test]
    fn mix32_and_max_abs_msb_match_the_original_on_the_probe_values() {
        for (value, expected) in [
            (0, 0),
            (1, 2_162_689),
            (2, 4_325_378),
            (0xffff, 4_292_870_175),
            (0x1_0000, 65_569),
            (0x7fff_ffff, 2_146_336_766),
            (0x8000_0000, 2_148_564_992),
            (0xdead_beef, 2_073_588_564),
        ] {
            assert_eq!(mix32(value), expected, "mix32({value:#x})");
        }
        for (point, expected) in [
            ([0, 0, 0], 0),
            ([1, 0, 0], 0),
            ([-1, 0, 0], 0),
            ([0x100, 0, -0x100], 8),
            ([0x1_0000, 0, 0], 16),
            ([-0x1_0000, -1, 1], 16),
            ([0x7fff_ffff, 0, 0], 30),
            ([i32::MIN, 1, 1], 31),
            ([0xffff, 0x1_0000, 0x100], 16),
        ] {
            assert_eq!(max_abs_msb(point), expected, "{point:?}");
        }
    }
}
