//! Liero game data, shared by the game (`liero-game`) and `liero-tctool`.
//!
//! - `exe` reads the original LIERO.EXE, LIERO.CHR and LIERO.SND.
//! - `save` writes an OpenLiero TC folder.
//! - `load` reads a TC folder the way the C++ game's `Common::load` did.
//!
//! All three use the same `model::Common`.
//!
//! `settings` holds the game settings and their formats: liero.cfg,
//! LIERO.DAT and the replay and profile archives, hashed with `gash`.

// No unsafe code, and none may be added (docs/001-rust-port.md, phase 4).
#![forbid(unsafe_code)]

pub mod exe;
pub mod gash;
pub mod load;
pub mod model;
mod rand;
mod reader;
pub mod save;
pub mod settings;
mod toml_reader;
mod toml_writer;

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}
