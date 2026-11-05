//! Port of `gvl::gash` and `gvl::hash_accumulator<gvl::gash>`
//! (src/gvl/crypt/gash.hpp), the hash replays use to check that both ends
//! have the same settings.

const BLOCK_SIZE: usize = 4;

/// A gash value: the first four words of the state.
pub type Hash = [u64; BLOCK_SIZE];

struct Gash {
    d: [u64; 8],
}

impl Gash {
    fn new() -> Self {
        let mut d = [0; 8];
        let mut accum: u64 = 1;
        for v in &mut d {
            accum = accum.wrapping_mul(0x579d16a0).wrapping_add(1);
            *v = accum;
        }
        Gash { d }
    }

    fn process(&mut self, n: &[u64; BLOCK_SIZE]) {
        let d = &mut self.d;
        for i in 0..4 {
            d[i] ^= n[i];
        }
        for _ in 0..4 {
            self.round1();
            self.round2();
        }
        let d = &mut self.d;
        for i in 0..4 {
            d[4 + i] = d[4 + i].wrapping_add(n[i]);
        }
    }

    fn round1(&mut self) {
        let d = &mut self.d;
        d[0] = d[0].wrapping_sub(d[5]);
        d[1] = d[1].wrapping_sub(d[6]);
        d[2] = d[2].wrapping_sub(d[7]);
        d[3] ^= d[0];
        d[4] ^= d[1];
        d[5] ^= d[2];
        d[6] = d[6].rotate_left(17);
        d[7] = d[7].rotate_left(37);
        d.swap(0, 4);
        d.swap(2, 5);
        d.swap(3, 7);
    }

    fn round2(&mut self) {
        let d = &mut self.d;
        d[7] = d[7].wrapping_sub(d[2]);
        d[6] = d[6].wrapping_sub(d[1]);
        d[5] = d[5].wrapping_sub(d[0]);
        d[4] ^= d[7];
        d[3] ^= d[6];
        d[2] ^= d[5];
        d[0] = d[0].rotate_left(13);
        d[1] = d[1].rotate_left(23);
        d.swap(1, 2);
        d.swap(3, 5);
        d.swap(7, 0);
    }
}

/// Hashes `data` as `hash_accumulator` does with one `flush` at the end:
/// bytes go into 64-bit words most significant first, four words to a
/// block, and the end is marked with 0x80 and padded with zeroes.
pub fn gash(data: &[u8]) -> Hash {
    let mut hash = Gash::new();
    let mut buf = [0u64; BLOCK_SIZE];
    let mut word_n = 0;
    let mut cur: u64 = 0;
    let mut bit_n = 64;

    for &b in data.iter().chain(std::iter::once(&0x80)) {
        bit_n -= 8;
        cur |= u64::from(b) << bit_n;
        if bit_n == 0 {
            buf[word_n] = cur;
            word_n += 1;
            bit_n = 64;
            cur = 0;
            if word_n == BLOCK_SIZE {
                hash.process(&buf);
                word_n = 0;
            }
        }
    }

    if bit_n < 64 {
        buf[word_n] = cur;
        word_n += 1;
    }
    if word_n > 0 {
        for v in &mut buf[word_n..] {
            *v = 0;
        }
        hash.process(&buf);
    }

    let mut out = [0; BLOCK_SIZE];
    out.copy_from_slice(&hash.d[..BLOCK_SIZE]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_from_the_cpp_state() {
        let g = Gash::new();
        assert_eq!(g.d[0], 0x579d16a1);
        assert_eq!(g.d[1], 0x579d16a1u64.wrapping_mul(0x579d16a0).wrapping_add(1));
    }

    #[test]
    fn every_byte_and_length_changes_the_hash() {
        let base = gash(&[1, 2, 3]);
        assert_ne!(base, gash(&[1, 2, 4]));
        assert_ne!(base, gash(&[1, 2, 3, 0]));
        // A block boundary: 31 bytes plus 0x80 fill exactly one block.
        assert_ne!(gash(&[0; 31]), gash(&[0; 32]));
        assert_ne!(gash(&[]), gash(&[0]));
    }
}
