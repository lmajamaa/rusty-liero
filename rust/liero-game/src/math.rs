//! Fixed-point helpers, the integer vector length and the cos/sin table from
//! `src/game/math.hpp` and `math.cpp`. Results have to stay bit-identical to
//! the C++ versions, which the game's physics depends on.

/// 16.16 fixed-point number, like C++'s `fixed`.
pub type Fixed = i32;

pub const fn itof(v: i32) -> Fixed {
    v << 16
}

pub const fn ftoi(v: Fixed) -> i32 {
    v >> 16
}

/// Two ints, laid out like C++'s `gvl::ivec2` / `fixedvec`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IVec2 {
    pub x: i32,
    pub y: i32,
}

/// Entry `angle` (0 to 127) of the cos/sin table, which is computed once.
pub fn cos_sin(angle: i32) -> IVec2 {
    static TABLE: std::sync::OnceLock<[IVec2; LIERO_COS_SIN_ENTRIES]> = std::sync::OnceLock::new();
    TABLE.get_or_init(cos_sin_table)[angle as usize]
}

/// Number of entries in the cos/sin table: a full turn in 128 steps.
pub const LIERO_COS_SIN_ENTRIES: usize = 128;

/// Integer square root, rounded down.
pub fn isqrt(mut op: u32) -> u32 {
    let mut res = 0u32;
    // The highest power of four that fits in a u32.
    let mut one = 1u32 << 30;

    // Start at the highest power of four <= op.
    while one > op {
        one >>= 2;
    }

    while one != 0 {
        if op >= res + one {
            op -= res + one;
            res += 2 * one;
        }
        res >>= 1;
        one >>= 2;
    }
    res
}

/// Length of (x, y), rounded down. Like the C++ version, x*x + y*y wraps
/// around in 32 bits for very long vectors.
pub fn vector_length(x: i32, y: i32) -> i32 {
    isqrt(x.wrapping_mul(x).wrapping_add(y.wrapping_mul(y)) as u32) as i32
}

/// A number s / 2^bits, used by the Taylor series below.
struct Fp {
    s: i64,
    bits: i32,
}

impl Fp {
    fn reduce(&mut self, to_bits: i32) {
        let lim = 1i64 << to_bits;
        while self.s < -lim - 1 || self.s > lim {
            self.s >>= 1;
            self.bits -= 1;
        }
    }

    fn reduced_frac(&self, to_bits: i32) -> i64 {
        let mut rs = self.s;
        let mut rbits = self.bits;
        while rbits > 60 {
            rs >>= 1;
            rbits -= 1;
        }
        rs << (to_bits - rbits)
    }
}

/// The 16.16 fixed-point table of directions, one per 1/128 of a turn, in
/// C++'s `cossinTable` layout: x is -sin and y is cos of the angle. Computed with integer maths only, so it comes
/// out the same on every platform.
pub fn cos_sin_table() -> [IVec2; LIERO_COS_SIN_ENTRIES] {
    let mut table = [IVec2::default(); LIERO_COS_SIN_ENTRIES];
    let scale_bits = 28;
    let scale: i32 = 13176795; // (2pi / 128) << scale_bits

    for i in 0..LIERO_COS_SIN_ENTRIES {
        let mut rf: i64 = 0;
        let mut c: i64 = -1;
        let xf = i as i32 * scale;

        // Simple Taylor series. Performance is not important. The sum
        // overflows 64 bits for some angles; it wraps around as in the C++
        // version, and the table depends on that.
        let mut num = Fp {
            s: xf as i64,
            bits: scale_bits,
        };
        let mut t: i64 = 1;
        while t < 26 {
            rf = rf.wrapping_add(c.wrapping_mul(num.reduced_frac(60)));

            for _ in 0..2 {
                t += 1;
                num.s /= t;
                num.reduce(31);
                num.s = num.s.wrapping_mul(xf as i64);
                num.bits += scale_bits;
            }

            c = -c;
        }

        let shift = 60 - 16;
        rf = rf.wrapping_add(1i64 << (shift - 1)); // Correct rounding

        let r = (rf >> shift) as i32;
        table[i].x = r;
        table[(i + 32) & 0x7f].y = r;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::Fnv;

    // The expected values are printed by reference/reference_values.cpp,
    // which runs the same loops through the original C++ code.

    #[test]
    fn fixed_point() {
        assert_eq!(itof(3), 3 << 16);
        assert_eq!(ftoi(itof(-7)), -7);
        assert_eq!(ftoi(-1), -1); // Rounds towards minus infinity, like C++'s >>.
    }

    #[test]
    fn isqrt_exact_squares_and_neighbours() {
        for n in [0u32, 1, 2, 3, 255, 256, 65535, 65536, 0xffff_ffff] {
            let r = isqrt(n) as u64;
            assert!(r * r <= n as u64 && (r + 1) * (r + 1) > n as u64, "{n}");
        }
    }

    #[test]
    fn vector_length_matches_cpp() {
        let mut h = Fnv::new();
        for x in (-2000..=2000).step_by(7) {
            for y in (-2000..=2000).step_by(11) {
                h.add(vector_length(x, y) as u32);
            }
        }
        // Large vectors whose squares wrap around in 32 bits.
        for x in (0..=200_000).step_by(997) {
            h.add(vector_length(x, -x / 3) as u32);
        }
        assert_eq!(h.finish(), EXPECTED_VECTOR_LENGTH);
    }

    #[test]
    fn cos_sin_table_matches_cpp() {
        let table = cos_sin_table();
        // x is -sin and y is cos of the angle.
        assert_eq!(table[0], IVec2 { x: 0, y: 65536 });
        assert_eq!(table[32], IVec2 { x: -65536, y: 0 });
        let mut h = Fnv::new();
        for v in table {
            h.add(v.x as u32);
            h.add(v.y as u32);
        }
        assert_eq!(h.finish(), EXPECTED_COS_SIN);
    }

    const EXPECTED_VECTOR_LENGTH: u64 = 0xf8b0_961b_8dc6_14ca;
    const EXPECTED_COS_SIN: u64 = 0x7542_fa6b_c25c_74dd;
}
