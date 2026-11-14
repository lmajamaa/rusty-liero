//! The game, ported from C++ to Rust, with its web shell (`web`, built for
//! `wasm32-unknown-unknown` by `tools/web/build.ts`) and the hash run.
//!
//! The modules call each other as Rust; only the wasm exports keep the
//! `liero_` names, as the web shell's API.

// The game is safe Rust (docs/001-rust-port.md, phase 4, step 12).
#![forbid(unsafe_code)]
// The pedantic lints that earn their keep: lossless casts are `From`, and
// pointer casts keep their pointee type in view. The rest of
// `clippy::pedantic` would fight the casts the game's arithmetic needs.
#![warn(clippy::cast_lossless, clippy::ptr_as_ptr)]

pub mod ai;
pub mod app;
pub mod app_files;
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
pub mod gfx;
pub mod hashrun;
pub mod keys;
pub mod level;
pub mod material;
pub mod math;
pub mod menu;
pub mod object;
pub mod objects;
pub mod rand;
pub mod renderer;
pub mod replay;
pub mod replay_file;
pub mod scene;
pub mod screen;
pub mod selector;
pub mod settings;
pub mod sfx;
pub mod shell;
pub mod stats;
pub mod tc;
pub mod text;
pub mod viewport;
pub mod weapon;
pub mod weapon_options;
pub mod weapsel;
/// The web build's shell (`wasm32-unknown-unknown` only).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub mod web;
pub mod world;
pub mod worm;

/// Name and version of this library, e.g. "liero-game 0.1.0". The web
/// shell prints it at startup.
pub const VERSION: &str = concat!("liero-game ", env!("CARGO_PKG_VERSION"));

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
            self.add64(u64::from(v));
        }

        pub fn add64(&mut self, v: u64) {
            self.0 = (self.0 ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }

        pub fn finish(&self) -> u64 {
            self.0
        }
    }
}
