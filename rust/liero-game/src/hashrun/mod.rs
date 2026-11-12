//! The hash run (step 6 in PLAN.md): fixed-seed bot games, replays, menus,
//! settings, sound, keys and a scripted run of the whole app, each hashed
//! into the lines `tools/hashrun/expected.txt` holds, which were made from
//! the original C++. `src/bin/liero-hashrun.rs` prints them natively.

use std::cell::Cell;

use crate::app::app;
use crate::menus::{liero_menus_new, LieroMenusHooks};

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

/// The sections, in the order a full run prints them.
pub const SECTIONS: [&str; 10] = [
    "tc", "games", "menus", "settings", "replays", "ai", "input", "app", "levels", "randomtc",
];

/// Runs section `name`, giving its lines to `out`, with the bot games
/// `ticks` long at most and hashed every `interval`.
fn run_section(name: &str, ticks: i32, interval: i32, out: &mut Out) {
    match name {
        "app" => app::hash_app(out),
        "games" => games::hash_games(out, ticks, interval),
        "replays" => replays::hash_replays(out),
        "ai" => ai::hash_ai_games(out),
        "input" => input::hash_sound_and_input(out),
        "menus" => menus::hash_menus(out),
        "tc" => tc::hash_tc(out),
        "randomtc" => tc::hash_random_tc(out),
        "levels" => levels::hash_level_files(out),
        "settings" => settings::hash_settings(out),
        _ => unreachable!("no hash section {name}"),
    }
}

thread_local! {
    static DOUBLE_RES: Cell<bool> = const { Cell::new(true) };
}

/// Where `path`, which starts with `/`, is on the disk: in the current
/// folder, as the game reads it.
pub(crate) fn on_disk(path: &str) -> String {
    format!(".{path}")
}

unsafe fn menu_cycles() -> u32 {
    app().menu_cycles
}

unsafe fn double_res() -> *mut bool {
    DOUBLE_RES.with(Cell::as_ptr)
}

unsafe fn set_double_res(on: bool) {
    DOUBLE_RES.set(on);
}

/// The menu has set the setting; there's no window to change.
unsafe fn set_fullscreen(_: bool) {}

/// Runs the sections in `only`, or all of them, in their order, each after
/// a line "hash-section name". It runs in a folder of its own, as `/` is in
/// the browser, with the TC in `TC/openliero`: a path starting with `/` is
/// one in the current folder, as the game reads it (`Node::new`). The
/// sections write files there, and the app section lists what it wrote, so
/// the folder starts with only the TC.
pub fn hash_run(ticks: i32, interval: i32, only: &[String], out: &mut dyn FnMut(&str)) -> Result<(), String> {
    if let Some(bad) = only.iter().find(|n| !SECTIONS.contains(&n.as_str())) {
        return Err(format!(
            "no hash section {bad}; the sections are {}",
            SECTIONS.join(",")
        ));
    }
    let a = app();
    if a.menus.is_null() {
        // The window's hooks, without a window: double size starts on, as
        // in the game.
        a.menus = liero_menus_new(LieroMenusHooks {
            menu_cycles,
            double_res,
            set_double_res,
            set_fullscreen,
        });
    }
    let mut out = Out(out);
    for name in SECTIONS
        .iter()
        .filter(|n| only.is_empty() || only.iter().any(|o| o == *n))
    {
        out.line(&format!("hash-section {name}"));
        run_section(name, ticks, interval, &mut out);
    }
    Ok(())
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
