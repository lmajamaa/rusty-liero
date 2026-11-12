//! The game, ported from C++ to Rust, with its web shell (`web`, built for
//! `wasm32-unknown-unknown` by `tools/web/build.ts`) and the hash run.
//!
//! The modules still talk through the plain C functions (`liero_*`) that C++
//! once called; phase 4 of PLAN.md turns them into Rust ones.

pub mod ai;
pub mod app;
pub mod app_files;
pub mod app_menus;
pub mod app_screens;
pub mod boxes;
pub mod controller;
pub mod controllers;
pub mod cpp_sort;
pub mod deflate;
pub mod disk;
pub mod exp;
pub mod fs;
pub mod game;
pub mod game_box;
pub mod hashrun;
pub mod gfx;
pub mod item;
pub mod keys;
pub mod level;
pub mod material;
pub mod menu;
pub mod menu_screen;
pub mod menus;
pub mod math;
pub mod object;
pub mod objects;
pub mod rand;
pub mod renderer;
pub mod replay;
pub mod replay_file;
pub mod screen;
pub mod selector;
pub mod settings;
pub mod shell;
pub mod shared;
pub mod sfx;
pub mod stats;
pub mod tc;
pub mod text;
pub mod scene;
pub mod viewport;
pub mod weapon;
pub mod weapon_options;
/// The web build's shell (`wasm32-unknown-unknown` only).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub mod web;
pub mod weapsel;
pub mod world;
pub mod worm;

use std::ffi::c_char;

/// Name and version of this library as a NUL-terminated string, e.g.
/// "liero-game 0.1.0". The web shell prints it at startup.
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
