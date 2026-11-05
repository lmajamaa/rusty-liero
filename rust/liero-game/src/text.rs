//! Text helpers shared by the HUD and menus: strings from C++, the time
//! formats from `text.cpp` and drawing with the game font.

use crate::gfx::{liero_font_draw_char, liero_font_draw_text, liero_font_get_dims, LieroBitmap, LieroFontChar};

/// Text from C++: `len` bytes from `ptr`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl LieroStr {
    /// # Safety
    /// `ptr` must point to `len` bytes that live as long as the result is used.
    pub unsafe fn bytes(&self) -> &[u8] {
        if self.len == 0 || self.ptr.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(self.ptr, self.len)
        }
    }
}

fn digit(v: i32) -> u8 {
    (b'0' as i32).wrapping_add(v) as u8
}

/// "MM:SS" from seconds: `timeToString`.
pub fn time_to_string(sec: i32) -> [u8; 5] {
    [digit(sec / 600), digit((sec % 600) / 60), b':', digit((sec % 60) / 10), digit(sec % 10)]
}

/// "M:SS.CC" style time from milliseconds: `timeToStringEx`.
pub fn time_to_string_ex(mut ms: i32, force_hours: bool, force_minutes: bool) -> Vec<u8> {
    let mut ret = Vec::with_capacity(9);
    if ms >= 6_000_000 || force_hours {
        ret.push(digit(ms / 6_000_000));
        ms %= 6_000_000;
    }
    if ms >= 60_000 || force_minutes {
        ret.push(digit(ms / 600_000));
        ms %= 600_000;
        ret.push(digit(ms / 60_000));
        ms %= 60_000;
        ret.push(b':');
    }
    ret.push(digit(ms / 10_000));
    ms %= 10_000;
    ret.push(digit(ms / 1000));
    ms %= 1000;
    ret.push(b'.');
    ret.push(digit(ms / 100));
    ms %= 100;
    ret.push(digit(ms / 10));
    ret
}

/// Writes `text` and a NUL to `out`, which holds `cap` bytes.
unsafe fn write_c(text: &[u8], out: *mut u8, cap: usize) {
    if cap == 0 {
        return;
    }
    let n = text.len().min(cap - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, n);
    *out.add(n) = 0;
}

/// `timeToString`: writes "MM:SS" and a NUL to `out` (6 bytes).
///
/// # Safety
/// `out` must hold 6 bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_time_to_string(sec: i32, out: *mut u8) {
    write_c(&time_to_string(sec), out, 6);
}

/// `timeToStringEx`: writes the time and a NUL to `out` (10 bytes).
///
/// # Safety
/// `out` must hold 10 bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_time_to_string_ex(ms: i32, force_hours: bool, force_minutes: bool, out: *mut u8) {
    write_c(&time_to_string_ex(ms, force_hours, force_minutes), out, 10);
}

/// The game font, `Font` in C++.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroFont {
    pub chars: *const LieroFontChar,
    pub count: usize,
}

impl LieroFont {
    /// `Font::drawText`.
    pub(crate) unsafe fn text(&self, scr: &LieroBitmap, text: &[u8], x: i32, y: i32, color: i32, size: i32) {
        liero_font_draw_text(scr, self.chars, self.count, text.as_ptr(), text.len(), x, y, color as u8, size);
    }

    /// `Font::drawChar`.
    pub(crate) unsafe fn char(&self, scr: &LieroBitmap, c: u8, x: i32, y: i32, color: i32) {
        liero_font_draw_char(scr, self.chars, self.count, c, x, y, color as u8, 1);
    }

    /// `Font::getDims`: the width, and the height.
    pub(crate) unsafe fn dims(&self, text: &[u8]) -> (i32, i32) {
        let mut height = 0;
        let width = liero_font_get_dims(self.chars, self.count, text.as_ptr(), text.len(), &mut height);
        (width, height)
    }

    /// `Font::drawCenteredText`.
    pub(crate) unsafe fn centered(&self, scr: &LieroBitmap, text: &[u8], x: i32, y: i32, color: i32, size: i32) {
        let len = self.dims(text).0 * size;
        self.text(scr, text, x - len / 2, y, color, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_strings_match_cpp() {
        assert_eq!(&time_to_string(754), b"12:34");
        assert_eq!(&time_to_string(0), b"00:00");
        assert_eq!(time_to_string_ex(83_456, false, true), b"01:23.45");
        assert_eq!(time_to_string_ex(3_456, false, false), b"03.45");
        assert_eq!(time_to_string_ex(7_283_456, false, false), b"121:23.45");
        assert_eq!(time_to_string_ex(1_000, true, false), b"001.00");

        let mut out = [0xffu8; 10];
        unsafe { liero_time_to_string_ex(83_456, false, true, out.as_mut_ptr()) };
        assert_eq!(&out[..9], b"01:23.45\0");
    }
}
