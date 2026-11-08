//! Game code ported from C++ to Rust. Built as a static library and linked
//! into the web build of openliero (`wasm32-unknown-emscripten`).
//!
//! C++ calls in through plain C functions (`liero_*`) in each module. `cbindgen`
//! turns them into `include/liero_game.h`; regenerate it after changing them
//! (see README).

pub mod ai;
pub mod app_screens;
pub mod boxes;
pub mod controller;
pub mod cpp_sort;
pub mod deflate;
pub mod fs;
pub mod game;
pub mod gfx;
pub mod item;
pub mod keys;
pub mod level;
pub mod material;
pub mod menu;
pub mod menu_screen;
pub mod math;
pub mod object;
pub mod objects;
pub mod rand;
pub mod replay;
pub mod screen;
pub mod selector;
pub mod settings;
pub mod sfx;
pub mod stats;
pub mod tc;
pub mod text;
pub mod viewport;
pub mod weapon;
pub mod weapon_options;
pub mod weapsel;
pub mod world;
pub mod worm;

use std::ffi::c_char;

/// Name and version of this library as a NUL-terminated string, e.g.
/// "liero-game 0.1.0". openliero prints it at startup to show the Rust side
/// is linked in.
#[no_mangle]
pub extern "C" fn liero_game_version() -> *const c_char {
    concat!("liero-game ", env!("CARGO_PKG_VERSION"), "\0")
        .as_ptr()
        .cast()
}

#[cfg(test)]
mod test_util {
    /// 64-bit FNV-1a over 32- or 64-bit words, used to compare long streams of
    /// results against the C++ reference values in one number.
    pub struct Fnv(u64);

    impl Fnv {
        pub fn new() -> Self {
            Fnv(0xcbf2_9ce4_8422_2325)
        }

        pub fn add(&mut self, v: u32) {
            self.add64(v as u64);
        }

        pub fn add64(&mut self, v: u64) {
            self.0 = (self.0 ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }

        pub fn finish(&self) -> u64 {
            self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_is_nul_terminated() {
        let s = unsafe { CStr::from_ptr(liero_game_version()) };
        assert_eq!(s.to_str().unwrap(), concat!("liero-game ", env!("CARGO_PKG_VERSION")));
    }
}
