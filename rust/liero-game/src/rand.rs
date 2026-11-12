//! The game's random number generator: the multiply-with-carry generator
//! `gvl::mwc` from `src/gvl/math/cmwc.hpp`, with the range helpers of
//! `gvl::prng_common`. Its output has to stay bit-identical to the C++
//! version, or level generation, weapons and old replays change.

/// Generator state. C++'s `Rand` inherits from this struct, so the state stays
/// a plain value inside `Game`, `Worm` and the other objects that own one:
/// they are copied for the AI and written into replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LieroRand {
    pub x: u32,
    pub c: u32,
}

const MULTIPLIER: u64 = 2083801278;
const INITIAL_CARRY: u32 = 9413207;
pub const LIERO_RAND_DEFAULT_SEED: u32 = 0x1337;

impl LieroRand {
    pub const fn new(seed: u32) -> Self {
        LieroRand {
            x: seed,
            c: INITIAL_CARRY,
        }
    }

    pub fn seed(&mut self, seed: u32) {
        *self = LieroRand::new(seed);
    }

    pub fn next_u32(&mut self) -> u32 {
        let t = MULTIPLIER * self.x as u64 + self.c as u64;
        self.c = (t >> 32) as u32;
        self.x = t as u32;
        self.x
    }

    /// Number in [0, max).
    pub fn below(&mut self, max: u32) -> u32 {
        ((self.next_u32() as u64 * max as u64) >> 32) as u32
    }

    /// Number in [min, max). Needs min <= max; C++'s `Rand` checks that before
    /// calling in, as `prng_common` did.
    pub fn range(&mut self, min: u32, max: u32) -> u32 {
        self.below(max.wrapping_sub(min)).wrapping_add(min)
    }

    /// Number in [0, max).
    pub fn double_below(&mut self, max: f64) -> f64 {
        self.next_u32() as f64 / 4294967296.0 * max
    }
}

impl Default for LieroRand {
    fn default() -> Self {
        LieroRand::new(LIERO_RAND_DEFAULT_SEED)
    }
}

pub fn liero_rand_seed(rand: &mut LieroRand, seed: u32) {
    rand.seed(seed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::Fnv;

    // The expected hashes are printed by reference/reference_values.cpp,
    // which runs the same loops through the original C++ code.

    fn stream_hash(seed: u32, f: impl Fn(&mut LieroRand, u32) -> u32) -> u64 {
        let mut r = LieroRand::new(seed);
        let mut h = Fnv::new();
        for i in 0..1_000_000u32 {
            h.add(f(&mut r, i));
        }
        h.add(r.x);
        h.add(r.c);
        h.finish()
    }

    #[test]
    fn default_state() {
        assert_eq!(LieroRand::default(), LieroRand { x: 0x1337, c: 9413207 });
    }

    #[test]
    fn next_matches_cpp() {
        assert_eq!(stream_hash(LIERO_RAND_DEFAULT_SEED, |r, _| r.next_u32()), EXPECTED_NEXT);
        assert_eq!(stream_hash(0, |r, _| r.next_u32()), EXPECTED_NEXT_SEED0);
        assert_eq!(stream_hash(0xffff_ffff, |r, _| r.next_u32()), EXPECTED_NEXT_SEEDMAX);
    }

    #[test]
    fn below_matches_cpp() {
        assert_eq!(stream_hash(12345, |r, i| r.below(i % 1000)), EXPECTED_BELOW);
    }

    #[test]
    fn range_matches_cpp() {
        assert_eq!(stream_hash(777, |r, i| r.range(i % 50, i % 50 + i % 7)), EXPECTED_RANGE);
    }

    #[test]
    fn double_below_matches_cpp() {
        let mut r = LieroRand::new(4242);
        let mut h = Fnv::new();
        for i in 0..100_000u32 {
            h.add64(r.double_below(1.0 + i as f64 * 0.37).to_bits());
        }
        assert_eq!(h.finish(), EXPECTED_DOUBLE);
    }

    const EXPECTED_NEXT: u64 = 0x834c_5cdf_79f6_1d4f;
    const EXPECTED_NEXT_SEED0: u64 = 0x1750_21a3_d2bf_2bfd;
    const EXPECTED_NEXT_SEEDMAX: u64 = 0xbd54_7c34_b5f2_740d;
    const EXPECTED_BELOW: u64 = 0x22ad_950f_1b56_8947;
    const EXPECTED_RANGE: u64 = 0x1319_f4de_9046_380f;
    const EXPECTED_DOUBLE: u64 = 0x915b_31f9_894d_0860;
}
