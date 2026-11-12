//! Input: SDL scancodes to the DOS scancodes the game and its settings use
//! (formerly `keys.cpp`), the key state the menus and frame skip read, the
//! joystick buttons and axes as extended keys, and the local controller
//! turning keys into worm controls (formerly in `gfx.cpp`,
//! `commonController.cpp`, `localController.cpp` and `Game::findControlForKey`).
//! The SDL event loop stays in C++ and passes each event's fields here.
//!
//! Keys below `LIERO_MAX_DOS_KEY` are DOS scancodes; joystick buttons are
//! extended keys from `LIERO_JOY_KEYS_START`, 32 per joystick: the hat's
//! up, down, left and right, then each axis's positive and negative
//! direction from 4, and the buttons from 16.

/// DOS scancodes are below this.
pub const LIERO_MAX_DOS_KEY: u32 = 177;
pub const LIERO_JOY_KEYS_START: u32 = 512;
pub const LIERO_MAX_JOY_BUTTONS: u32 = 32;
pub const LIERO_DK_ESCAPE: u32 = 1;
/// How far an axis must move from the centre to count as pressed.
pub const LIERO_JOY_AXIS_THRESHOLD: i32 = 10000;
/// What SDL scancodes without a DOS key become. Nothing uses 89.
pub const LIERO_UNKNOWN_DOS_KEY: u32 = 89;

/// SDL2's scancode count (`SDL_NUM_SCANCODES`).
const SDL_NUM_SCANCODES: usize = 512;

// SDL2 scancodes (USB HID usages) the table uses.
mod sc {
    pub(super) const A: u16 = 4;
    pub(super) const B: u16 = 5;
    pub(super) const C: u16 = 6;
    pub(super) const D: u16 = 7;
    pub(super) const E: u16 = 8;
    pub(super) const F: u16 = 9;
    pub(super) const G: u16 = 10;
    pub(super) const H: u16 = 11;
    pub(super) const I: u16 = 12;
    pub(super) const J: u16 = 13;
    pub(super) const K: u16 = 14;
    pub(super) const L: u16 = 15;
    pub(super) const M: u16 = 16;
    pub(super) const N: u16 = 17;
    pub(super) const O: u16 = 18;
    pub(super) const P: u16 = 19;
    pub(super) const Q: u16 = 20;
    pub(super) const R: u16 = 21;
    pub(super) const S: u16 = 22;
    pub(super) const T: u16 = 23;
    pub(super) const U: u16 = 24;
    pub(super) const V: u16 = 25;
    pub(super) const W: u16 = 26;
    pub(super) const X: u16 = 27;
    pub(super) const Y: u16 = 28;
    pub(super) const Z: u16 = 29;
    pub(super) const N1: u16 = 30;
    pub(super) const N0: u16 = 39;
    pub(super) const RETURN: u16 = 40;
    pub(super) const ESCAPE: u16 = 41;
    pub(super) const BACKSPACE: u16 = 42;
    pub(super) const TAB: u16 = 43;
    pub(super) const SPACE: u16 = 44;
    pub(super) const MINUS: u16 = 45;
    pub(super) const EQUALS: u16 = 46;
    pub(super) const LEFTBRACKET: u16 = 47;
    pub(super) const RIGHTBRACKET: u16 = 48;
    pub(super) const BACKSLASH: u16 = 49;
    pub(super) const SEMICOLON: u16 = 51;
    pub(super) const APOSTROPHE: u16 = 52;
    pub(super) const GRAVE: u16 = 53;
    pub(super) const COMMA: u16 = 54;
    pub(super) const PERIOD: u16 = 55;
    pub(super) const SLASH: u16 = 56;
    pub(super) const CAPSLOCK: u16 = 57;
    pub(super) const F1: u16 = 58;
    pub(super) const F11: u16 = 68;
    pub(super) const F12: u16 = 69;
    pub(super) const PRINTSCREEN: u16 = 70;
    pub(super) const SCROLLLOCK: u16 = 71;
    pub(super) const INSERT: u16 = 73;
    pub(super) const HOME: u16 = 74;
    pub(super) const PAGEUP: u16 = 75;
    pub(super) const DELETE: u16 = 76;
    pub(super) const END: u16 = 77;
    pub(super) const PAGEDOWN: u16 = 78;
    pub(super) const RIGHT: u16 = 79;
    pub(super) const LEFT: u16 = 80;
    pub(super) const DOWN: u16 = 81;
    pub(super) const UP: u16 = 82;
    pub(super) const NUMLOCKCLEAR: u16 = 83;
    pub(super) const KP_DIVIDE: u16 = 84;
    pub(super) const KP_MULTIPLY: u16 = 85;
    pub(super) const KP_MINUS: u16 = 86;
    pub(super) const KP_PLUS: u16 = 87;
    pub(super) const KP_ENTER: u16 = 88;
    pub(super) const KP_1: u16 = 89;
    pub(super) const KP_0: u16 = 98;
    pub(super) const KP_PERIOD: u16 = 99;
    pub(super) const NONUSBACKSLASH: u16 = 100;
    pub(super) const LCTRL: u16 = 224;
    pub(super) const LSHIFT: u16 = 225;
    pub(super) const LALT: u16 = 226;
    pub(super) const RCTRL: u16 = 228;
    pub(super) const RSHIFT: u16 = 229;
    pub(super) const RALT: u16 = 230;
}

/// The SDL scancode of each DOS key, as in the original table. Print Screen
/// is there twice (0x82 and 0x8f); the later one wins.
const DOS_KEYS: [(u8, u16); 102] = {
    use sc::*;
    [
        (0x01, ESCAPE),
        (0x02, N1),
        (0x03, N1 + 1),
        (0x04, N1 + 2),
        (0x05, N1 + 3),
        (0x06, N1 + 4),
        (0x07, N1 + 5),
        (0x08, N1 + 6),
        (0x09, N1 + 7),
        (0x0a, N1 + 8),
        (0x0b, N0),
        (0x0c, MINUS),
        (0x0d, EQUALS),
        (0x0e, BACKSPACE),
        (0x0f, TAB),
        (0x10, Q),
        (0x11, W),
        (0x12, E),
        (0x13, R),
        (0x14, T),
        (0x15, Y),
        (0x16, U),
        (0x17, I),
        (0x18, O),
        (0x19, P),
        (0x1a, LEFTBRACKET),
        (0x1b, RIGHTBRACKET),
        (0x1c, RETURN),
        (0x1d, LCTRL),
        (0x1e, A),
        (0x1f, S),
        (0x20, D),
        (0x21, F),
        (0x22, G),
        (0x23, H),
        (0x24, J),
        (0x25, K),
        (0x26, L),
        (0x27, SEMICOLON),
        (0x28, APOSTROPHE),
        (0x29, GRAVE),
        (0x2a, LSHIFT),
        (0x2b, BACKSLASH),
        (0x2c, Z),
        (0x2d, X),
        (0x2e, C),
        (0x2f, V),
        (0x30, B),
        (0x31, N),
        (0x32, M),
        (0x33, COMMA),
        (0x34, PERIOD),
        (0x35, SLASH),
        (0x36, RSHIFT),
        (0x37, KP_MULTIPLY),
        (0x38, LALT),
        (0x39, SPACE),
        (0x3a, CAPSLOCK),
        // F1 to F10.
        (0x3b, F1),
        (0x3c, F1 + 1),
        (0x3d, F1 + 2),
        (0x3e, F1 + 3),
        (0x3f, F1 + 4),
        (0x40, F1 + 5),
        (0x41, F1 + 6),
        (0x42, F1 + 7),
        (0x43, F1 + 8),
        (0x44, F1 + 9),
        (0x45, NUMLOCKCLEAR),
        (0x46, SCROLLLOCK),
        // The keypad's 7 8 9 - 4 5 6 + 1 2 3 0 . in DOS order. SDL has
        // 1 to 9 in order from KP_1.
        (0x47, KP_1 + 6),
        (0x48, KP_1 + 7),
        (0x49, KP_1 + 8),
        (0x4a, KP_MINUS),
        (0x4b, KP_1 + 3),
        (0x4c, KP_1 + 4),
        (0x4d, KP_1 + 5),
        (0x4e, KP_PLUS),
        (0x4f, KP_1),
        (0x50, KP_1 + 1),
        (0x51, KP_1 + 2),
        (0x52, KP_0),
        (0x53, KP_PERIOD),
        (0x56, NONUSBACKSLASH),
        (0x57, F11),
        (0x58, F12),
        (0x74, KP_ENTER),
        (0x75, RCTRL),
        (0x82, PRINTSCREEN),
        (0x8d, KP_DIVIDE),
        (0x8f, PRINTSCREEN),
        (0x90, RALT),
        (0x9f, HOME),
        (0xa0, UP),
        (0xa1, PAGEUP),
        (0xa3, LEFT),
        (0xa5, RIGHT),
        (0xa7, END),
        (0xa8, DOWN),
        (0xa9, PAGEDOWN),
        (0xaa, INSERT),
        (0xab, DELETE),
    ]
};

const SDL_TO_DOS: [u8; SDL_NUM_SCANCODES] = {
    let mut t = [LIERO_UNKNOWN_DOS_KEY as u8; SDL_NUM_SCANCODES];
    let mut i = 0;
    while i < DOS_KEYS.len() {
        t[DOS_KEYS[i].1 as usize] = DOS_KEYS[i].0;
        i += 1;
    }
    t
};

/// The DOS key for an SDL scancode, or 89 for one without.
pub fn sdl_to_dos(scancode: i32) -> u32 {
    usize::try_from(scancode)
        .ok()
        .and_then(|i| SDL_TO_DOS.get(i))
        .map_or(LIERO_UNKNOWN_DOS_KEY, |&k| k as u32)
}

pub fn is_extended(key: u32) -> bool {
    key >= LIERO_MAX_DOS_KEY
}

pub fn joy_key(joy: i32, button: i32) -> u32 {
    LIERO_JOY_KEYS_START
        .wrapping_add((LIERO_MAX_JOY_BUTTONS as i32).wrapping_mul(joy) as u32)
        .wrapping_add(button as u32)
}

/// Which DOS keys are down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LieroKeys {
    pub down: [bool; LIERO_MAX_DOS_KEY as usize],
}

impl Default for LieroKeys {
    fn default() -> Self {
        LieroKeys {
            down: [false; LIERO_MAX_DOS_KEY as usize],
        }
    }
}

impl LieroKeys {
    // Keys past the DOS ones are ignored (C indexed past the array).
    /// Every key up.
    pub fn clear(&mut self) {
        *self = LieroKeys::default();
    }

    /// Whether the DOS key for an SDL key is down.
    pub fn test_sdl(&self, scancode: i32) -> bool {
        let d = sdl_to_dos(scancode);
        d != 0 && self.test(d)
    }

    /// As `test_once`, for the DOS key of an SDL key.
    pub fn test_sdl_once(&mut self, scancode: i32) -> bool {
        let d = sdl_to_dos(scancode);
        d != 0 && self.test_once(d)
    }

    /// Lets the DOS key for an SDL key go.
    pub fn release_sdl(&mut self, scancode: i32) {
        let d = sdl_to_dos(scancode);
        if d != 0 {
            self.set(d, false);
        }
    }

    pub fn test(&self, key: u32) -> bool {
        self.down.get(key as usize).copied().unwrap_or(false)
    }

    pub fn set(&mut self, key: u32, state: bool) {
        if let Some(k) = self.down.get_mut(key as usize) {
            *k = state;
        }
    }

    pub fn test_once(&mut self, key: u32) -> bool {
        let state = self.test(key);
        self.set(key, false);
        state
    }

    pub fn toggle(&mut self, key: u32) {
        self.set(key, !self.test(key));
    }

    /// Records a key event. Returns the DOS key, for the controller.
    pub fn event(&mut self, scancode: i32, down: bool) -> u32 {
        let k = sdl_to_dos(scancode);
        if k != 0 {
            self.set(k, down);
        }
        k
    }

    /// The frame skip the number keys choose, 1 to 512 for 1 to 0, and
    /// whether ctrl was down (every that many frames rather than that many
    /// frames each time), taking the first of them pressed.
    pub fn frame_skip(&mut self, frame_skip: &mut i32, inverse: &mut bool) {
        let mut skip = 0;
        for (i, n) in (sc::N1..=sc::N0).enumerate() {
            if self.test_once(sdl_to_dos(n as i32)) {
                skip = 1 << i;
                break;
            }
        }
        if skip != 0 {
            *inverse = self.test(sdl_to_dos(sc::RCTRL as i32)) || self.test(sdl_to_dos(sc::LCTRL as i32));
            *frame_skip = skip;
        }
    }
}

/// Key presses and releases that a joystick event made.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroKeyChanges {
    pub count: u32,
    pub keys: [u32; 4],
    pub states: [bool; 4],
}

impl LieroKeyChanges {
    fn push(&mut self, key: u32, state: bool) {
        self.keys[self.count as usize] = key;
        self.states[self.count as usize] = state;
        self.count += 1;
    }
}

/// A joystick's button states, the hat, axes and buttons as above.
pub type JoyButtons = [bool; LIERO_MAX_JOY_BUTTONS as usize];

fn change(buttons: &mut JoyButtons, out: &mut LieroKeyChanges, joy: i32, button: usize, state: bool) {
    if buttons[button] != state {
        buttons[button] = state;
        out.push(joy_key(joy, button as i32), state);
    }
}

/// An axis moving: its two directions as buttons 4 + 2 * axis (positive)
/// and the one after (negative).
pub fn joy_axis(buttons: &mut JoyButtons, joy: i32, axis: u8, value: i16) -> LieroKeyChanges {
    let mut out = LieroKeyChanges::default();
    let base = 4 + 2 * axis as usize;
    // Axes past 13 would be past the buttons; C wrote past the array.
    if base + 1 < buttons.len() {
        let value = value as i32;
        change(buttons, &mut out, joy, base, value > LIERO_JOY_AXIS_THRESHOLD);
        change(buttons, &mut out, joy, base + 1, value < -LIERO_JOY_AXIS_THRESHOLD);
    }
    out
}

/// The hat moving: up, down, left and right as buttons 0 to 3.
pub fn joy_hat(buttons: &mut JoyButtons, joy: i32, value: u8) -> LieroKeyChanges {
    // SDL_HAT_UP, DOWN, LEFT and RIGHT.
    const BITS: [u8; 4] = [1, 4, 8, 2];
    let mut out = LieroKeyChanges::default();
    for (b, bit) in BITS.iter().enumerate() {
        change(buttons, &mut out, joy, b, value & bit != 0);
    }
    out
}

/// A button going down or up: button 16 + `button`, reported even when
/// unchanged.
pub fn joy_button(buttons: &mut JoyButtons, joy: i32, button: u8, pressed: bool) -> LieroKeyChanges {
    let mut out = LieroKeyChanges::default();
    let b = 16 + button as usize;
    // Buttons past 15 would be past the array; C wrote past it.
    if let Some(s) = buttons.get_mut(b) {
        *s = pressed;
        out.push(joy_key(joy, b as i32), pressed);
    }
    out
}

/// The key a joystick event gives when waiting for a key to bind, or 0.
pub fn joy_axis_key(joy: i32, axis: u8, value: i16) -> u32 {
    let value = value as i32;
    if value > LIERO_JOY_AXIS_THRESHOLD {
        joy_key(joy, 4 + 2 * axis as i32)
    } else if value < -LIERO_JOY_AXIS_THRESHOLD {
        joy_key(joy, 5 + 2 * axis as i32)
    } else {
        0
    }
}

pub fn joy_hat_key(joy: i32, value: u8) -> u32 {
    match [1u8, 4, 8, 2].iter().position(|bit| value & bit != 0) {
        Some(b) => joy_key(joy, b as i32),
        None => 0,
    }
}

/// One worm's controls and states for `liero_local_controller_key`.
#[derive(Clone, Copy, Debug)]
pub struct LieroWormKeys {
    /// The key for each control: up, down, left, right, fire, change,
    /// jump and dig.
    pub controls: *const u32,
    /// The worm's control states (`Worm::controlStates`), one bit each.
    pub states: *mut u32,
    /// The keys' real states, dig included (`cleanControlStates`).
    pub clean: *mut u32,
}

const LEFT: u32 = 2;
const RIGHT: u32 = 3;
/// Controls below this are the worm's; dig is only a key.
const MAX_CONTROL: u32 = 7;
const DIG: u32 = 7;

fn set_bit(v: &mut u32, n: u32, state: bool) {
    if state {
        *v |= 1 << n;
    } else {
        *v &= !(1 << n);
    }
}

/// The first worm and control with `key`.
pub fn control_for_key(controls: &[&[u32]], key: u32) -> Option<(usize, u32)> {
    controls
        .iter()
        .enumerate()
        .find_map(|(w, c)| c.iter().position(|&k| k == key).map(|c| (w, c as u32)))
}

/// A key's effect on a worm's control states: the control follows the key,
/// and while dig is down left and right are both pressed.
pub fn apply_control(states: &mut u32, clean: &mut u32, control: u32, key_state: bool) {
    set_bit(clean, control, key_state);
    if control < MAX_CONTROL {
        set_bit(states, control, key_state);
    }
    if *clean & (1 << DIG) != 0 {
        set_bit(states, LEFT, true);
        set_bit(states, RIGHT, true);
    } else {
        if *clean & (1 << LEFT) == 0 {
            set_bit(states, LEFT, false);
        }
        if *clean & (1 << RIGHT) == 0 {
            set_bit(states, RIGHT, false);
        }
    }
}

/// The DOS key for an SDL key pressed. (C also turned keys from 177 up into
/// 89, but the table has no such keys.)
pub fn sdl_to_dos_keysym(scancode: i32) -> u32 {
    let k = sdl_to_dos(scancode);
    if k >= LIERO_MAX_DOS_KEY {
        LIERO_UNKNOWN_DOS_KEY
    } else {
        k
    }
}

/// The key for a gamepad button, after the hat's 16 keys.
pub fn joy_button_key(joy: i32, button: u8) -> u32 {
    joy_key(joy, 16 + button as i32)
}

pub const KEY_NAMES: [&[u8]; 177] = [
    b"",
    b"Esc",
    b"1",
    b"2",
    b"3",
    b"4",
    b"5",
    b"6",
    b"7",
    b"8",
    b"9",
    b"0",
    b"+",
    b"`",
    b"Backspace",
    b"Tab",
    b"Q",
    b"W",
    b"E",
    b"R",
    b"T",
    b"Y",
    b"U",
    b"I",
    b"O",
    b"P",
    b"\xC5",
    b"^",
    b"Enter",
    b"Left Crtl",
    b"A",
    b"S",
    b"D",
    b"F",
    b"G",
    b"H",
    b"J",
    b"K",
    b"L",
    b"\xD6",
    b"\xC4",
    b"\xBD",
    b"Left Shift",
    b"'",
    b"Z",
    b"X",
    b"C",
    b"V",
    b"B",
    b"N",
    b"M",
    b",",
    b".",
    b"-",
    b"Right Shift",
    b"* (Pad)",
    b"Left Alt",
    b"",
    b"Caps Lock",
    b"F1",
    b"F2",
    b"F3",
    b"F4",
    b"F5",
    b"F6",
    b"F7",
    b"F8",
    b"F9",
    b"F10",
    b"Num Lock",
    b"Scroll Lock",
    b"7 (Pad)",
    b"8 (Pad)",
    b"9 (Pad)",
    b"- (Pad)",
    b"4 (Pad)",
    b"5 (Pad)",
    b"6 (Pad)",
    b"+ (Pad)",
    b"1 (Pad)",
    b"2 (Pad)",
    b"3 (Pad)",
    b"0 (Pad)",
    b", (Pad)",
    b"",
    b"",
    b"<",
    b"F11",
    b"F12",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"Enter (Pad)",
    b"Right Ctrl",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"Print Screen",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"/ (Pad)",
    b"",
    b"Print Screen",
    b"Right Alt",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"",
    b"Home",
    b"Up",
    b"Page Up",
    b"",
    b"Left",
    b"",
    b"Right",
    b"",
    b"End",
    b"Down",
    b"Page Down",
    b"Insert",
    b"Delete",
    b"",
    b"",
    b"",
    b"",
    b"",
];

/// A key's name, as the player menu shows it (`Gfx::getKeyName`).
pub fn key_name(key: u32) -> Vec<u8> {
    if key < LIERO_MAX_DOS_KEY {
        KEY_NAMES[key as usize].to_vec()
    } else if key >= LIERO_JOY_KEYS_START {
        let k = key - LIERO_JOY_KEYS_START;
        format!("J{}_{}", k / LIERO_MAX_JOY_BUTTONS, k % LIERO_MAX_JOY_BUTTONS).into_bytes()
    } else {
        Vec::new()
    }
}

/// A key going down or up in a local game (`LocalController::onKey`): sets
/// the control of the first worm with that key, and returns true for
/// Escape, which leaves the game.
///
/// # Safety
/// Each worm's `controls` points to `control_count` controls, and its
/// `states` and `clean` are valid.
pub unsafe fn local_controller_key(worms: &[LieroWormKeys], control_count: usize, key: i32, key_state: bool) -> bool {
    unsafe {
        let controls: Vec<&[u32]> = worms
            .iter()
            .map(|w| std::slice::from_raw_parts(w.controls, control_count))
            .collect();
        if let Some((w, c)) = control_for_key(&controls, key as u32) {
            apply_control(&mut *worms[w].states, &mut *worms[w].clean, c, key_state);
        }
        key as u32 == LIERO_DK_ESCAPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        // Every DOS key once, in order, and no SDL key twice but Print
        // Screen.
        let mut seen = [0u32; SDL_NUM_SCANCODES];
        for w in DOS_KEYS.windows(2) {
            assert!(w[0].0 < w[1].0);
        }
        for &(d, s) in DOS_KEYS.iter() {
            assert!((d as u32) < LIERO_MAX_DOS_KEY && d != 0 && d as u32 != LIERO_UNKNOWN_DOS_KEY);
            seen[s as usize] += 1;
        }
        for (s, &n) in seen.iter().enumerate() {
            assert!(n <= 1 || s == sc::PRINTSCREEN as usize, "{s}");
        }
    }

    #[test]
    fn mapping() {
        for (s, d) in [
            (0, 89),
            (sc::ESCAPE, 1),
            (sc::A, 0x1e),
            (sc::Z, 0x2c),
            (sc::N1, 2),
            (sc::N1 + 8, 10),
            (sc::N0, 11),
            (sc::KP_1 + 6, 0x47),
            (sc::KP_0, 0x52),
            (sc::F1 + 9, 0x44),
            (sc::F11, 0x57),
            (sc::F12, 0x58),
            (sc::PRINTSCREEN, 0x8f),
            (sc::DELETE, 0xab),
            (sc::RALT, 0x90),
            (sc::RCTRL, 0x75),
            (72, 89),  // Pause
            (231, 89), // Right GUI
            (511, 89),
        ] {
            assert_eq!(sdl_to_dos(s as i32), d, "{s}");
            assert_eq!(sdl_to_dos_keysym(s as i32), d, "{s}");
        }
        for s in [-1, -1000, 512, 513, i32::MAX, i32::MIN] {
            assert_eq!(sdl_to_dos(s), 89);
        }
    }

    #[test]
    fn key_state() {
        let mut k = LieroKeys::default();
        assert_eq!(k.event(sc::A as i32, true), 0x1e);
        assert!(k.test(0x1e));
        assert!(k.test_sdl(sc::A as i32));
        assert!(k.test_once(0x1e));
        assert!(!k.test(0x1e));
        k.toggle(5);
        assert!(k.test_sdl_once(sc::N1 as i32 + 3));
        assert!(!k.test(5));
        k.set(176, true);
        assert!(k.test(176));
        // Past the DOS keys: ignored.
        k.set(177, true);
        assert!(!k.test(177));
        k.event(sc::LEFT as i32, true);
        k.release_sdl(sc::LEFT as i32);
        assert!(!k.test(0xa3));
        // Unknown keys go to 89.
        k.event(231, true);
        assert!(k.test(89));
        k.clear();
        assert_eq!(k, LieroKeys::default());
    }

    #[test]
    fn frame_skip() {
        let mut k = LieroKeys::default();
        let (mut skip, mut inv) = (1, false);
        k.frame_skip(&mut skip, &mut inv);
        assert_eq!((skip, inv), (1, false));
        // The first of several wins and only it is released.
        k.event(sc::N1 as i32 + 2, true);
        k.event(sc::N0 as i32, true);
        k.event(sc::RCTRL as i32, true);
        k.frame_skip(&mut skip, &mut inv);
        assert_eq!((skip, inv), (4, true));
        k.event(sc::RCTRL as i32, false);
        k.frame_skip(&mut skip, &mut inv);
        assert_eq!((skip, inv), (512, false));
        k.event(sc::LCTRL as i32, true);
        k.frame_skip(&mut skip, &mut inv);
        assert_eq!((skip, inv), (512, false));
        k.event(sc::N1 as i32 + 8, true);
        k.frame_skip(&mut skip, &mut inv);
        assert_eq!((skip, inv), (256, true));
    }

    #[test]
    fn joystick() {
        let mut b = [false; 32];
        let c = joy_axis(&mut b, 1, 0, 10001);
        assert_eq!((c.count, c.keys[0], c.states[0]), (1, 512 + 32 + 4, true));
        // At the threshold isn't pressed.
        let c = joy_axis(&mut b, 1, 0, 10000);
        assert_eq!((c.count, c.keys[0], c.states[0]), (1, 512 + 32 + 4, false));
        let c = joy_axis(&mut b, 1, 0, -10000);
        assert_eq!(c.count, 0);
        let c = joy_axis(&mut b, 0, 2, -32768);
        assert_eq!((c.count, c.keys[0], c.states[0]), (1, 512 + 9, true));
        let c = joy_axis(&mut b, 0, 2, 32767);
        assert_eq!(&c.keys[..2], &[512 + 8, 512 + 9]);
        assert_eq!(&c.states[..2], &[true, false]);
        // Axis 13 is the last one inside the buttons.
        assert_eq!(joy_axis(&mut b, 0, 13, 20000).keys[0], 512 + 30);
        assert_eq!(joy_axis(&mut b, 0, 14, 20000).count, 0);

        let mut b = [false; 32];
        // Up and right, then left.
        let c = joy_hat(&mut b, 0, 1 | 2);
        assert_eq!((c.count, c.keys[0], c.keys[1]), (2, 512, 515));
        let c = joy_hat(&mut b, 0, 8);
        assert_eq!(&c.keys[..3], &[512, 514, 515]);
        assert_eq!(&c.states[..3], &[false, true, false]);

        let c = joy_button(&mut b, 2, 3, true);
        assert_eq!((c.count, c.keys[0], c.states[0]), (1, 512 + 64 + 19, true));
        let c = joy_button(&mut b, 2, 3, true);
        assert_eq!(c.count, 1);
        assert!(b[19]);
        assert_eq!(joy_button(&mut b, 2, 15, true).keys[0], 512 + 64 + 31);
        assert_eq!(joy_button(&mut b, 2, 16, true).count, 0);

        assert_eq!(joy_axis_key(0, 1, 10001), 518);
        assert_eq!(joy_axis_key(0, 1, -10001), 519);
        assert_eq!(joy_axis_key(0, 1, -10000), 0);
        assert_eq!(joy_hat_key(1, 0), 0);
        assert_eq!(joy_hat_key(1, 1 | 4), 544);
        assert_eq!(joy_hat_key(1, 4 | 8), 545);
        assert_eq!(joy_hat_key(1, 8 | 2), 546);
        assert_eq!(joy_hat_key(1, 2), 547);
        assert_eq!(joy_button_key(1, 0), 560);
    }

    #[test]
    fn key_names() {
        assert_eq!(key_name(176), KEY_NAMES[176]);
        assert_eq!(key_name(177), b"");
        assert_eq!(key_name(511), b"");
        assert_eq!(key_name(512), b"J0_0");
        assert_eq!(key_name(512 + 32 * 3 + 31), b"J3_31");
        assert!(!is_extended(176));
        assert!(is_extended(177));
        assert_eq!(joy_key(3, 31), 512 + 96 + 31);
    }

    #[test]
    fn controls() {
        let w0 = [10u32, 11, 12, 13, 14, 15, 16, 17];
        let w1 = [20u32, 21, 22, 23, 24, 25, 16, 27];
        let mut states = [0u32; 2];
        let mut clean = [0u32; 2];
        let key = |k: u32, s: bool, states: &mut [u32; 2], clean: &mut [u32; 2]| unsafe {
            let worms = [
                LieroWormKeys {
                    controls: w0.as_ptr(),
                    states: &mut states[0],
                    clean: &mut clean[0],
                },
                LieroWormKeys {
                    controls: w1.as_ptr(),
                    states: &mut states[1],
                    clean: &mut clean[1],
                },
            ];
            local_controller_key(&worms, 8, k as i32, s)
        };
        // A key two worms share goes to the first.
        key(16, true, &mut states, &mut clean);
        assert_eq!((states, clean), ([1 << 6, 0], [1 << 6, 0]));
        key(24, true, &mut states, &mut clean);
        assert_eq!(states[1], 1 << 4);
        // Dig presses left and right but only sets its own clean bit.
        key(27, true, &mut states, &mut clean);
        assert_eq!((states[1], clean[1]), (1 << 4 | 1 << 2 | 1 << 3, 1 << 4 | 1 << 7));
        key(22, true, &mut states, &mut clean);
        key(27, false, &mut states, &mut clean);
        // Left is still held.
        assert_eq!(states[1], 1 << 4 | 1 << 2);
        key(22, false, &mut states, &mut clean);
        assert_eq!(states[1], 1 << 4);
        // An unknown key changes nothing; only Escape returns true.
        assert!(!key(99, true, &mut states, &mut clean));
        assert!(key(1, true, &mut states, &mut clean));
        assert!(key(1, false, &mut states, &mut clean));
        assert_eq!(states, [1 << 6, 1 << 4]);
        // Left and right released while not held clear the states set
        // elsewhere (an AI, or a replay).
        states[0] |= 1 << 2 | 1 << 3 | 1;
        key(10, false, &mut states, &mut clean);
        assert_eq!(states[0], 1 << 6);
    }
}
