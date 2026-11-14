//! Text helpers shared by the HUD and menus: the time
//! formats from `text.cpp` and drawing with the game font.

use crate::gfx::{draw_char, draw_text, text_dims, Bitmap};
use liero_data::model::FontChar;

fn digit(v: i32) -> u8 {
    i32::from(b'0').wrapping_add(v) as u8
}

/// "MM:SS" from seconds: `timeToString`.
pub fn time_to_string(sec: i32) -> [u8; 5] {
    [
        digit(sec / 600),
        digit((sec % 600) / 60),
        b':',
        digit((sec % 60) / 10),
        digit(sec % 10),
    ]
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

/// The game font, `Font` in C++.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    pub chars: &'a [FontChar],
}

impl Font<'_> {
    /// `Font::drawText`.
    pub(crate) fn text(&self, scr: &Bitmap, text: &[u8], x: i32, y: i32, color: i32, size: i32) {
        draw_text(scr, self.chars, text, x, y, color as u8, size);
    }

    /// `Font::drawChar`.
    pub(crate) fn char(&self, scr: &Bitmap, c: u8, x: i32, y: i32, color: i32) {
        draw_char(scr, self.chars, c, x, y, color as u8, 1);
    }

    /// `Font::getDims`: the width, and the height.
    pub(crate) fn dims(&self, text: &[u8]) -> (i32, i32) {
        text_dims(self.chars, text)
    }

    /// `Font::drawCenteredText`.
    pub(crate) fn centered(&self, scr: &Bitmap, text: &[u8], x: i32, y: i32, color: i32, size: i32) {
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
    }
}
