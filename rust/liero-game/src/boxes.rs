//! The message box, text box and key choice, as screens (they were loops in
//! `Gfx`).

use std::ffi::c_void;

use crate::keys::is_extended;
use crate::menu::{draw_info_box, draw_input_box};
use crate::screen::{BoxedScreen, Host, Screen, Tick};

// SDL's scancodes.
const SCANCODE_RETURN: u32 = 40;
const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_BACKSPACE: u32 = 42;
const SCANCODE_KP_ENTER: u32 = 88;

/// The sound a menu choice makes.
const SOUND_CHOICE: i32 = 27;

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
    fn tick(&mut self, app: &mut crate::app::App, host: &Host) -> Tick {
        if !self.shown {
            if self.clear_screen {
                host.menu_palette(app);
                host.fill_screen(app, BG_COLOR);
            }
            if let (Some(scr), Some(font)) = (host.screen(app), host.font(app)) {
                draw_info_box(&scr, &font, &self.text, self.x, self.y);
            }
            self.shown = true;
            return Tick::stay(true);
        }

        if !self.waiting {
            host.process(app);
            self.waiting = true;
        }

        while let Some(ev) = host.poll_event(app) {
            if ev.key_down {
                app.clear_keys();
                if self.clear_screen {
                    host.fill_screen(app, BG_COLOR);
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
    fn tick(&mut self, app: &mut crate::app::App, host: &Host) -> Tick {
        while let Some(ev) = host.poll_event(app) {
            if ev.choice && (self.extended || !is_extended(ev.key)) {
                return Tick::close(ev.key as i32);
            }
        }
        Tick::stay(false)
    }
}

/// Called with the text when Enter closes a text box.
pub type InputDone = unsafe fn(ctx: *mut c_void, text: &[u8]);

/// Typing a line into a box over the frozen screen. It shows a frame for
/// each event. Enter hands the text to `done` and closes with 1, Escape
/// closes with 0.
struct InputBox {
    buffer: Vec<u8>,
    max_len: usize,
    x: i32,
    y: i32,
    digits_only: bool,
    done: Option<InputDone>,
    ctx: *mut c_void,
    shown: bool,
    /// The host that turned text input on, to turn it off again.
    host: Option<Host>,
}

impl InputBox {
    /// What a typed character adds, if anything.
    fn accepts(&self, c: u8) -> bool {
        c != 0 && self.buffer.len() < self.max_len && (!self.digits_only || c.is_ascii_digit())
    }
}

impl Screen for InputBox {
    fn tick(&mut self, app: &mut crate::app::App, host: &Host) -> Tick {
        if self.shown {
            let Some(ev) = host.poll_event(app) else {
                return Tick::stay(false);
            };
            if ev.key_down {
                match ev.scancode {
                    SCANCODE_BACKSPACE => {
                        self.buffer.pop();
                    }
                    SCANCODE_RETURN | SCANCODE_KP_ENTER => {
                        if let Some(done) = self.done {
                            unsafe { done(self.ctx, &self.buffer) };
                        }
                        host.play_sound(SOUND_CHOICE);
                        app.clear_keys();
                        return Tick::close(1);
                    }
                    SCANCODE_ESCAPE => {
                        app.clear_keys();
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
        if let (Some(scr), Some(frozen), Some(font)) = (host.screen(app), host.frozen(app), host.font(app)) {
            draw_input_box(&scr, &frozen, &font, &shown, self.x, self.y, true);
        }
        host.text_input(true);
        self.host = Some(host.clone());
        self.shown = true;
        Tick::stay(true)
    }
}

impl Drop for InputBox {
    fn drop(&mut self) {
        // Text input stays off outside the box, so that the browser doesn't
        // act on the keys the game uses.
        if let Some(host) = &self.host {
            host.text_input(false);
        }
    }
}

/// A message centred on (x, y) until a key is pressed. With `clear_screen`
/// it's shown alone on the screen, in the TC's palette.
pub fn info_box_screen(text: &[u8], x: i32, y: i32, clear_screen: bool) -> BoxedScreen {
    Box::new(InfoBox {
        text: text.to_vec(),
        x,
        y,
        clear_screen,
        shown: false,
        waiting: false,
    })
}

/// Waits for a key, joystick button or stick move and closes with its DOS
/// key. Unless `extended`, keys only the extended controls have are passed
/// over.
pub fn key_choice_screen(extended: bool) -> BoxedScreen {
    Box::new(KeyChoice { extended })
}

/// Typing at most `max_len` characters, starting from `text`, in a box
/// centred on x at height y. Enter calls `done(ctx, typed)` and closes with
/// 1; Escape closes with 0.
///
/// # Safety
/// `done` is called with `ctx`.
pub unsafe fn input_box_screen(
    text: &[u8],
    max_len: usize,
    x: i32,
    y: i32,
    digits_only: bool,
    done: Option<InputDone>,
    ctx: *mut c_void,
) -> BoxedScreen {
    Box::new(InputBox {
        buffer: text.to_vec(),
        max_len,
        x,
        y,
        digits_only,
        done,
        ctx,
        shown: false,
        host: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{Event, Platform};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Gives out its events, and logs what the screen asks of it.
    #[derive(Default)]
    struct TestPlatform {
        events: RefCell<VecDeque<Event>>,
        log: RefCell<Vec<String>>,
    }

    impl TestPlatform {
        fn log(&self, line: String) {
            self.log.borrow_mut().push(line);
        }
    }

    impl Platform for TestPlatform {
        fn poll_event(&self, _app: &mut crate::app::App) -> Option<Event> {
            self.events.borrow_mut().pop_front()
        }
        fn play_sound(&self, id: i32) {
            self.log(format!("sound {id}"));
        }
        fn process(&self, _app: &mut crate::app::App) {
            self.log("process".into());
        }
        fn text_input(&self, on: bool) {
            self.log(format!("text {on}"));
        }
    }

    /// A host with `events`, and its platform to read the log from.
    fn host(events: &[Event]) -> (Host, Rc<TestPlatform>) {
        let p = Rc::new(TestPlatform {
            events: RefCell::new(events.iter().copied().collect()),
            ..Default::default()
        });
        (Host::new(p.clone()), p)
    }

    fn typed(c: u8) -> Event {
        Event {
            text: c,
            ..Default::default()
        }
    }

    fn down(scancode: u32) -> Event {
        Event {
            key_down: true,
            scancode,
            ..Default::default()
        }
    }

    unsafe fn keep(ctx: *mut c_void, text: &[u8]) {
        unsafe {
            *(ctx as *mut Vec<u8>) = text.to_vec();
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
            host: None,
        }
    }

    // Runs ticks until the screen closes; returns the result and the frames shown.
    fn run(s: &mut dyn Screen, host: &Host) -> (i32, usize) {
        let mut frames = 0;
        for _ in 0..100 {
            let t = s.tick(&mut crate::app::App::new(), host);
            frames += t.shown as usize;
            if let Some(result) = t.closed() {
                return (result, frames);
            }
        }
        panic!("never closed");
    }

    #[test]
    fn typing_editing_and_enter() {
        let (h, p) = host(&[
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
            p.log.borrow().clone(),
            ["text true"; 6]
                .iter()
                .map(|s| s.to_string())
                .chain(["sound 27".into()])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn digits_only_and_escape() {
        let (h, _) = host(&[typed(b'1'), typed(b'x'), typed(b'2'), down(SCANCODE_ESCAPE)]);
        let mut out = b"unchanged".to_vec();
        let mut b = input(b"", 10, true, &mut out);
        assert_eq!(run(&mut b, &h).0, 0);
        assert_eq!(b.buffer, b"12");
        assert_eq!(out, b"unchanged");
    }

    #[test]
    fn key_choice_skips_extended_keys_unless_asked() {
        let ext = (1..400).find(|&k| is_extended(k)).unwrap();
        let plain = Event {
            choice: true,
            key: 0x11,
            ..Default::default()
        };
        let extended = Event {
            choice: true,
            key: ext,
            ..Default::default()
        };
        let not_a_key = Event {
            key: 0x22,
            ..Default::default()
        };

        let (h, _) = host(&[not_a_key, extended, plain]);
        assert_eq!(run(&mut KeyChoice { extended: false }, &h).0, 0x11);
        let (h, _) = host(&[extended, plain]);
        assert_eq!(run(&mut KeyChoice { extended: true }, &h).0, ext as i32);
    }

    #[test]
    fn info_box_shows_once_and_closes_on_a_key() {
        let (h, p) = host(&[typed(b'a'), down(57)]);
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
        assert_eq!(*p.log.borrow(), ["process"]);
    }
}
