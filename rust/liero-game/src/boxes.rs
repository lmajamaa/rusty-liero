//! The message box, text box and key choice, as screens (they were loops in
//! `Gfx`).

use std::ffi::c_void;

use crate::keys::liero_key_is_extended;
use crate::menu::{liero_menu_draw_info_box, liero_menu_draw_input_box};
use crate::screen::{into_ffi, Host, LieroScreen, Screen, Tick};
use crate::text::LieroStr;

// SDL's scancodes.
const SCANCODE_RETURN: u32 = 40;
const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_BACKSPACE: u32 = 42;
const SCANCODE_KP_ENTER: u32 = 88;

/// The sound a menu choice makes.
const SOUND_CHOICE: i32 = 27;

fn lstr(b: &[u8]) -> LieroStr {
    LieroStr {
        ptr: b.as_ptr(),
        len: b.len(),
    }
}

/// A message over the screen, until a key is pressed.
struct InfoBox {
    text: Vec<u8>,
    x: i32,
    y: i32,
    clear_screen: bool,
    shown: bool,
    /// The events that came with the box are read.
    waiting: bool,
}

const BG_COLOR: u8 = 0;

impl Screen for InfoBox {
    fn tick(&mut self, host: &Host) -> Tick {
        if !self.shown {
            if self.clear_screen {
                host.menu_palette();
                host.fill_screen(BG_COLOR);
            }
            if let (Some(scr), Some(font)) = (host.screen(), host.font()) {
                unsafe { liero_menu_draw_info_box(&scr, &font, lstr(&self.text), self.x, self.y) };
            }
            self.shown = true;
            return Tick::stay(true);
        }

        if !self.waiting {
            host.process();
            self.waiting = true;
        }

        while let Some(ev) = host.poll_event() {
            if ev.key_down {
                host.clear_keys();
                if self.clear_screen {
                    host.fill_screen(BG_COLOR);
                }
                return Tick::close(0);
            }
        }
        Tick::stay(false)
    }
}

/// The key the next key press, joystick button or stick move gives. Unless
/// `extended`, keys only the extended controls have are passed over.
struct KeyChoice {
    extended: bool,
}

impl Screen for KeyChoice {
    fn tick(&mut self, host: &Host) -> Tick {
        while let Some(ev) = host.poll_event() {
            if ev.choice && (self.extended || !liero_key_is_extended(ev.key)) {
                return Tick::close(ev.key as i32);
            }
        }
        Tick::stay(false)
    }
}

/// Called with the text when Enter closes a text box.
pub type LieroInputDone = unsafe fn(ctx: *mut c_void, text: LieroStr);

/// Typing a line into a box over the frozen screen. It shows a frame for
/// each event. Enter hands the text to `done` and closes with 1, Escape
/// closes with 0.
struct InputBox {
    buffer: Vec<u8>,
    max_len: usize,
    x: i32,
    y: i32,
    digits_only: bool,
    done: Option<LieroInputDone>,
    ctx: *mut c_void,
    shown: bool,
}

impl InputBox {
    /// What a typed character adds, if anything.
    fn accepts(&self, c: u8) -> bool {
        c != 0 && self.buffer.len() < self.max_len && (!self.digits_only || c.is_ascii_digit())
    }
}

impl Screen for InputBox {
    fn tick(&mut self, host: &Host) -> Tick {
        if self.shown {
            let Some(ev) = host.poll_event() else {
                return Tick::stay(false);
            };
            if ev.key_down {
                match ev.scancode {
                    SCANCODE_BACKSPACE => {
                        self.buffer.pop();
                    }
                    SCANCODE_RETURN | SCANCODE_KP_ENTER => {
                        if let Some(done) = self.done {
                            unsafe { done(self.ctx, lstr(&self.buffer)) };
                        }
                        host.play_sound(SOUND_CHOICE);
                        host.clear_keys();
                        return Tick::close(1);
                    }
                    SCANCODE_ESCAPE => {
                        host.clear_keys();
                        return Tick::close(0);
                    }
                    _ => {}
                }
            } else if self.accepts(ev.text) {
                self.buffer.push(ev.text);
            }
        }

        let mut shown = self.buffer.clone();
        shown.push(b'_');
        if let (Some(scr), Some(frozen), Some(font)) = (host.screen(), host.frozen(), host.font()) {
            unsafe { liero_menu_draw_input_box(&scr, &frozen, &font, lstr(&shown), self.x, self.y, true) };
        }
        host.text_input(true);
        self.shown = true;
        Tick::stay(true)
    }

    fn close(&mut self, host: &Host) {
        // Text input stays off outside the box, so that the browser doesn't
        // act on the keys the game uses.
        host.text_input(false);
    }
}

/// A message centred on (x, y) until a key is pressed. With `clear_screen`
/// it's shown alone on the screen, in the TC's palette.
///
/// # Safety
/// `text` must be valid.
pub unsafe fn liero_info_box_screen(text: LieroStr, x: i32, y: i32, clear_screen: bool) -> LieroScreen {
    unsafe {
        into_ffi(Box::new(InfoBox {
            text: text.bytes().to_vec(),
            x,
            y,
            clear_screen,
            shown: false,
            waiting: false,
        }))
    }
}

/// Waits for a key, joystick button or stick move and closes with its DOS
/// key. Unless `extended`, keys only the extended controls have are passed
/// over.
pub fn liero_key_choice_screen(extended: bool) -> LieroScreen {
    into_ffi(Box::new(KeyChoice { extended }))
}

/// Typing at most `max_len` characters, starting from `text`, in a box
/// centred on x at height y. Enter calls `done(ctx, typed)` and closes with
/// 1; Escape closes with 0.
///
/// # Safety
/// `text` must be valid; `done` is called with `ctx`.
pub unsafe fn liero_input_box_screen(
    text: LieroStr,
    max_len: usize,
    x: i32,
    y: i32,
    digits_only: bool,
    done: Option<unsafe fn(ctx: *mut c_void, text: LieroStr)>,
    ctx: *mut c_void,
) -> LieroScreen {
    unsafe {
        into_ffi(Box::new(InputBox {
            buffer: text.bytes().to_vec(),
            max_len,
            x,
            y,
            digits_only,
            done,
            ctx,
            shown: false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{LieroEvent, LieroScreenHost, LieroTickKind};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    thread_local! {
        static EVENTS: RefCell<VecDeque<LieroEvent>> = const { RefCell::new(VecDeque::new()) };
        static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    unsafe fn poll(out: *mut LieroEvent) -> bool {
        unsafe {
            EVENTS
                .with(|e| e.borrow_mut().pop_front())
                .map(|ev| *out = ev)
                .is_some()
        }
    }
    unsafe fn sound(id: i32) {
        LOG.with(|l| l.borrow_mut().push(format!("sound {id}")));
    }
    unsafe fn process() {
        LOG.with(|l| l.borrow_mut().push("process".into()));
    }
    unsafe fn clear() {
        LOG.with(|l| l.borrow_mut().push("clear".into()));
    }
    unsafe fn text_input(on: bool) {
        LOG.with(|l| l.borrow_mut().push(format!("text {on}")));
    }

    fn host(events: &[LieroEvent]) -> Host {
        EVENTS.with(|e| *e.borrow_mut() = events.iter().copied().collect());
        LOG.with(|l| l.borrow_mut().clear());
        Host(LieroScreenHost {
            poll_event: Some(poll),
            process: Some(process),
            clear_keys: Some(clear),
            play_sound: Some(sound),
            screen: None,
            frozen: None,
            font: None,
            fill_screen: None,
            menu_palette: None,
            text_input: Some(text_input),
            ..Default::default()
        })
    }

    fn typed(c: u8) -> LieroEvent {
        LieroEvent {
            text: c,
            ..Default::default()
        }
    }

    fn down(scancode: u32) -> LieroEvent {
        LieroEvent {
            key_down: true,
            scancode,
            ..Default::default()
        }
    }

    unsafe fn keep(ctx: *mut c_void, text: LieroStr) {
        unsafe {
            *(ctx as *mut Vec<u8>) = text.bytes().to_vec();
        }
    }

    fn input(start: &[u8], max_len: usize, digits_only: bool, out: &mut Vec<u8>) -> InputBox {
        InputBox {
            buffer: start.to_vec(),
            max_len,
            x: 0,
            y: 0,
            digits_only,
            done: Some(keep),
            ctx: (out as *mut Vec<u8>).cast(),
            shown: false,
        }
    }

    // Runs ticks until the screen closes; returns the result and the frames shown.
    fn run(s: &mut dyn Screen, host: &Host) -> (i32, usize) {
        let mut frames = 0;
        for _ in 0..100 {
            let t = s.tick(host);
            frames += t.shown as usize;
            if t.kind == LieroTickKind::TickClose {
                return (t.result, frames);
            }
        }
        panic!("never closed");
    }

    #[test]
    fn typing_editing_and_enter() {
        let h = host(&[
            typed(b'A'),
            typed(b'B'),
            down(SCANCODE_BACKSPACE),
            typed(b'C'),
            typed(b'D'),
            down(SCANCODE_RETURN),
        ]);
        let mut out = Vec::new();
        let mut b = input(b"X", 3, false, &mut out);
        // A frame first, then one per event before Enter.
        assert_eq!(run(&mut b, &h), (1, 6));
        assert_eq!(out, b"XAC");
        assert_eq!(
            LOG.with(|l| l.borrow().clone()),
            ["text true"; 6]
                .iter()
                .map(|s| s.to_string())
                .chain(["sound 27".into(), "clear".into()])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn digits_only_and_escape() {
        let h = host(&[typed(b'1'), typed(b'x'), typed(b'2'), down(SCANCODE_ESCAPE)]);
        let mut out = b"unchanged".to_vec();
        let mut b = input(b"", 10, true, &mut out);
        assert_eq!(run(&mut b, &h).0, 0);
        assert_eq!(b.buffer, b"12");
        assert_eq!(out, b"unchanged");
    }

    #[test]
    fn key_choice_skips_extended_keys_unless_asked() {
        let ext = (1..400).find(|&k| liero_key_is_extended(k)).unwrap();
        let plain = LieroEvent {
            choice: true,
            key: 0x11,
            ..Default::default()
        };
        let extended = LieroEvent {
            choice: true,
            key: ext,
            ..Default::default()
        };
        let not_a_key = LieroEvent {
            key: 0x22,
            ..Default::default()
        };

        let h = host(&[not_a_key, extended, plain]);
        assert_eq!(run(&mut KeyChoice { extended: false }, &h).0, 0x11);
        let h = host(&[extended, plain]);
        assert_eq!(run(&mut KeyChoice { extended: true }, &h).0, ext as i32);
    }

    #[test]
    fn info_box_shows_once_and_closes_on_a_key() {
        let h = host(&[typed(b'a'), down(57)]);
        let mut b = InfoBox {
            text: b"HI".to_vec(),
            x: 0,
            y: 0,
            clear_screen: false,
            shown: false,
            waiting: false,
        };
        // The events that came before the box is shown are read once, then
        // a key closes it.
        assert_eq!(run(&mut b, &h), (0, 1));
        assert_eq!(LOG.with(|l| l.borrow().clone()), ["process", "clear"]);
    }
}
