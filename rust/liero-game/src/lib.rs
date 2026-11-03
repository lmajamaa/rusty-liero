//! Game code ported from C++ to Rust. Built as a static library and linked
//! into the web build of openliero (`wasm32-unknown-emscripten`).
//!
//! C++ calls in through the plain C functions below. `cbindgen` turns them
//! into `include/liero_game.h`; regenerate it after changing them (see README).

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
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_is_nul_terminated() {
        let s = unsafe { CStr::from_ptr(liero_game_version()) };
        assert_eq!(s.to_str().unwrap(), concat!("liero-game ", env!("CARGO_PKG_VERSION")));
    }
}
