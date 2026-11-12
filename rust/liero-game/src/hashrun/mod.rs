//! The hash run in Rust (`--hash-run`, step 6 in PLAN.md). Each section
//! prints the lines its C++ version in `src/game/hash*.cpp` printed, which
//! `tools/hashrun/expected.txt` holds. While the rest is C++, C++ calls a
//! section by name (`liero_hash_section`) and prints its lines in order.

use std::ffi::c_void;

use crate::text::LieroStr;

mod ai;
mod app;
mod game;
mod games;
mod input;
mod levels;
mod menus;
mod replays;
mod settings;
mod tc;

/// FNV-1a, 64-bit, as the C++ `Fnv`.
#[derive(Clone, Copy)]
pub struct Fnv(pub u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= u64::from(x);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// A 32-bit value, low byte first (`Fnv::add`).
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    pub fn bool(&mut self, v: bool) {
        self.u32(u32::from(v));
    }

    /// Hashes `b` as a fresh `Fnv`.
    pub fn of(b: &[u8]) -> Fnv {
        let mut h = Fnv::default();
        h.bytes(b);
        h
    }
}

/// Where a section's lines go.
pub struct Out<'a>(&'a mut dyn FnMut(&str));

impl Out<'_> {
    /// A line as it is.
    pub fn line(&mut self, s: &str) {
        (self.0)(s);
    }

    /// `name index hash`, as the C++ `print`.
    pub fn hash(&mut self, name: &str, index: i64, h: Fnv) {
        (self.0)(&format!("{name} {index} {:016x}", h.0));
    }
}

/// Runs section `name`, giving its lines to `out`, with the bot games
/// `ticks` long at most and hashed every `interval`. False if it isn't one
/// Rust has.
pub fn run_section(name: &str, ticks: i32, interval: i32, out: &mut dyn FnMut(&str)) -> bool {
    let mut out = Out(out);
    match name {
        "app" => app::hash_app(&mut out),
        "games" => games::hash_games(&mut out, ticks, interval),
        "replays" => replays::hash_replays(&mut out),
        "ai" => ai::hash_ai_games(&mut out),
        "input" => input::hash_sound_and_input(&mut out),
        "menus" => menus::hash_menus(&mut out),
        "tc" => tc::hash_tc(&mut out),
        "randomtc" => tc::hash_random_tc(&mut out),
        "levels" => levels::hash_level_files(&mut out),
        "settings" => settings::hash_settings(&mut out),
        _ => return false,
    }
    true
}

/// Runs section `name`, calling `print(ctx, line)` for each line, with the
/// bot games `ticks` long at most and hashed every `interval`. False if it
/// isn't one Rust has.
///
/// # Safety
/// `name` is valid, and `print` can be called with `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_hash_section(
    name: LieroStr,
    ticks: i32,
    interval: i32,
    ctx: *mut c_void,
    print: unsafe extern "C" fn(ctx: *mut c_void, line: LieroStr),
) -> bool {
    let name = String::from_utf8_lossy(name.bytes()).into_owned();
    run_section(&name, ticks, interval, &mut |line| print(ctx, LieroStr::of(line.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_matches_the_cpp() {
        // FNV-1a of "a", and 1 as four bytes.
        assert_eq!(Fnv::of(b"a").0, 0xaf63_dc4c_8601_ec8c);
        let mut h = Fnv::default();
        h.u32(1);
        assert_eq!(h.0, Fnv::of(&[1, 0, 0, 0]).0);
    }
}
