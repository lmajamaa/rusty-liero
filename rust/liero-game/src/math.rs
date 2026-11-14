//! Fixed-point helpers, the integer vector length and the cos/sin table from
//! `src/game/math.hpp` and `math.cpp`. Results have to stay bit-identical to
//! the C++ versions, which the game's physics depends on.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Shr, Sub, SubAssign};

/// A 16.16 fixed-point number, like C++'s `fixed`. Its arithmetic wraps,
/// as C++'s `int` did on the platforms the game ran on, so that physics
/// gives the same numbers in debug builds too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Fixed(pub i32);

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);

    /// The whole number `v` (`itof`).
    pub const fn from_int(v: i32) -> Fixed {
        Fixed(itof(v))
    }

    /// The whole part, rounded down (`ftoi`).
    pub const fn to_int(self) -> i32 {
        ftoi(self.0)
    }

    pub const fn abs(self) -> Fixed {
        Fixed(self.0.wrapping_abs())
    }
}

/// A whole number as a raw 16.16 one.
pub const fn itof(v: i32) -> i32 {
    v << 16
}

/// The whole part of a raw 16.16 number, rounded down.
pub const fn ftoi(v: i32) -> i32 {
    v >> 16
}

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, o: Fixed) -> Fixed {
        Fixed(self.0.wrapping_add(o.0))
    }
}

impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, o: Fixed) -> Fixed {
        Fixed(self.0.wrapping_sub(o.0))
    }
}

impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        Fixed(self.0.wrapping_neg())
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, o: Fixed) {
        *self = *self + o;
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, o: Fixed) {
        *self = *self - o;
    }
}

/// Scaling by a whole number.
impl Mul<i32> for Fixed {
    type Output = Fixed;
    fn mul(self, k: i32) -> Fixed {
        Fixed(self.0.wrapping_mul(k))
    }
}

/// Division by a whole number, rounding towards zero.
impl Div<i32> for Fixed {
    type Output = Fixed;
    fn div(self, k: i32) -> Fixed {
        Fixed(self.0.wrapping_div(k))
    }
}

impl Shr<i32> for Fixed {
    type Output = Fixed;
    fn shr(self, n: i32) -> Fixed {
        Fixed(self.0 >> n)
    }
}

/// Two fixed-point numbers: a position or a velocity (C++'s `fixedvec`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FVec2 {
    pub x: Fixed,
    pub y: Fixed,
}

impl FVec2 {
    pub const fn new(x: Fixed, y: Fixed) -> FVec2 {
        FVec2 { x, y }
    }

    /// The raw numbers, as replays, the bots and the hashes store them.
    pub const fn raw(self) -> [i32; 2] {
        [self.x.0, self.y.0]
    }

    pub const fn from_raw([x, y]: [i32; 2]) -> FVec2 {
        FVec2 {
            x: Fixed(x),
            y: Fixed(y),
        }
    }

    /// The whole-pixel position (`ftoi` of each).
    pub const fn to_int(self) -> IVec2 {
        IVec2 {
            x: self.x.to_int(),
            y: self.y.to_int(),
        }
    }
}

impl Add for FVec2 {
    type Output = FVec2;
    fn add(self, o: FVec2) -> FVec2 {
        FVec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for FVec2 {
    type Output = FVec2;
    fn sub(self, o: FVec2) -> FVec2 {
        FVec2::new(self.x - o.x, self.y - o.y)
    }
}

impl AddAssign for FVec2 {
    fn add_assign(&mut self, o: FVec2) {
        *self = *self + o;
    }
}

impl SubAssign for FVec2 {
    fn sub_assign(&mut self, o: FVec2) {
        *self = *self - o;
    }
}

impl Mul<i32> for FVec2 {
    type Output = FVec2;
    fn mul(self, k: i32) -> FVec2 {
        FVec2::new(self.x * k, self.y * k)
    }
}

impl Div<i32> for FVec2 {
    type Output = FVec2;
    fn div(self, k: i32) -> FVec2 {
        FVec2::new(self.x / k, self.y / k)
    }
}

/// Two ints, laid out like C++'s `gvl::ivec2` / `fixedvec`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IVec2 {
    pub x: i32,
    pub y: i32,
}

/// Entry `angle` (0 to 127) of the cos/sin table, which is computed once.
pub fn cos_sin(angle: i32) -> FVec2 {
    static TABLE: std::sync::OnceLock<[FVec2; COS_SIN_ENTRIES]> = std::sync::OnceLock::new();
    TABLE.get_or_init(cos_sin_table)[angle as usize]
}

/// Number of entries in the cos/sin table: a full turn in 128 steps.
pub const COS_SIN_ENTRIES: usize = 128;

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
pub fn cos_sin_table() -> [FVec2; COS_SIN_ENTRIES] {
    let mut table = [FVec2::default(); COS_SIN_ENTRIES];
    let scale_bits = 28;
    let scale: i32 = 13176795; // (2pi / 128) << scale_bits

    for i in 0..COS_SIN_ENTRIES {
        let mut rf: i64 = 0;
        let mut c: i64 = -1;
        let xf = i as i32 * scale;

        // Simple Taylor series. Performance is not important. The sum
        // overflows 64 bits for some angles; it wraps around as in the C++
        // version, and the table depends on that.
        let mut num = Fp {
            s: i64::from(xf),
            bits: scale_bits,
        };
        let mut t: i64 = 1;
        while t < 26 {
            rf = rf.wrapping_add(c.wrapping_mul(num.reduced_frac(60)));

            for _ in 0..2 {
                t += 1;
                num.s /= t;
                num.reduce(31);
                num.s = num.s.wrapping_mul(i64::from(xf));
                num.bits += scale_bits;
            }

            c = -c;
        }

        let shift = 60 - 16;
        rf = rf.wrapping_add(1i64 << (shift - 1)); // Correct rounding

        let r = Fixed((rf >> shift) as i32);
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
            let r = u64::from(isqrt(n));
            assert!(r * r <= u64::from(n) && (r + 1) * (r + 1) > u64::from(n), "{n}");
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
        assert_eq!(table[0], FVec2::new(Fixed(0), Fixed(65536)));
        assert_eq!(table[32], FVec2::new(Fixed(-65536), Fixed(0)));
        let mut h = Fnv::new();
        for v in table {
            h.add(v.x.0 as u32);
            h.add(v.y.0 as u32);
        }
        assert_eq!(h.finish(), EXPECTED_COS_SIN);
    }

    const EXPECTED_VECTOR_LENGTH: u64 = 0xf8b0_961b_8dc6_14ca;
    const EXPECTED_COS_SIN: u64 = 0x7542_fa6b_c25c_74dd;
}
