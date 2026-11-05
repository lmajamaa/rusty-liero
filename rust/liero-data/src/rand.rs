/// Multiply-with-carry generator matching `gvl::mwc` with its default seed, so
/// the randomized sprite frames come out identical to the C++ tctool.
pub struct Mwc {
    x: u32,
    c: u32,
}

impl Mwc {
    pub fn new() -> Self {
        Mwc { x: 0x1337, c: 9413207 }
    }

    fn next(&mut self) -> u32 {
        let t = 2083801278u64 * self.x as u64 + self.c as u64;
        self.c = (t >> 32) as u32;
        self.x = t as u32;
        self.x
    }

    /// Number in [0, max).
    pub fn below(&mut self, max: u32) -> u32 {
        ((self.next() as u64 * max as u64) >> 32) as u32
    }
}
