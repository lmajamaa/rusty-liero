//! The stack of screens that replaces the game's nested loops.
//!
//! Each of today's blocking loops (the main menu, a file selector, a text
//! box, a game) becomes a screen. A screen's `tick` runs one step of what its
//! loop body did: it reads the events, updates, draws, and says whether it
//! showed a frame and what happens next: it stays, opens a screen on top,
//! closes with a result for the screen below, or replaces itself.
//!
//! `liero_app_step` runs one step of the top screen. The host shows the
//! frame when the screen drew one, and when a screen is waiting for an event
//! (it stays and shows nothing) the host is told to idle, so a run driven by
//! a script can skip to the next event as the old blocking waits did.
//! `liero_app_frame` runs the steps due by a clock, 14 ms each, for a caller
//! woken by the browser or the window.
//!
//! Every screen is seen here through a table of functions (`LieroScreen`):
//! C++ screens through `src/game/screen.cpp`, Rust screens through
//! `into_ffi`. A Rust screen reaches what is still C++ (events, sound, the
//! screen's bitmaps) through `LieroScreenHost`, which C++ sets once.

use std::ffi::c_void;
use std::sync::OnceLock;

use crate::game::LieroRect;
use crate::gfx::LieroBitmap;
use crate::text::LieroFont;

/// How long one step is, in milliseconds.
pub const STEP_MS: u32 = 14;

/// The most steps one call of `liero_app_frame` runs. A tab coming back from
/// the background skips the time it missed instead of racing to catch up.
pub const MAX_STEPS_PER_FRAME: u32 = 4;

/// A screen, owned by the stack once pushed.
#[repr(C)]
pub struct LieroScreen {
    pub data: *mut c_void,
    /// Runs one step and fills in what happens next, and `next` for
    /// `TickPush` and `TickReplace`.
    pub tick: Option<unsafe extern "C" fn(data: *mut c_void, out: *mut LieroTick, next: *mut LieroScreen)>,
    /// The screen opened on top of this one closed with `result`.
    pub resume: Option<unsafe extern "C" fn(data: *mut c_void, result: i32)>,
    /// Frees the screen, when it closes or the stack is cleared.
    pub free: Option<unsafe extern "C" fn(data: *mut c_void)>,
}

/// What a screen does after its tick.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Prefixed, since C puts the names in the global scope.
#[allow(clippy::enum_variant_names)]
pub enum LieroTickKind {
    /// Stays on top.
    TickStay = 0,
    /// Opens `screen` on top; this one gets `resume` when it closes.
    TickPush = 1,
    /// Closes, handing `result` to the screen below.
    TickClose = 2,
    /// Closes, and `screen` takes its place.
    TickReplace = 3,
    /// Closes every screen: the app is done.
    TickQuit = 4,
}

/// A tick's outcome.
#[repr(C)]
pub struct LieroTick {
    pub kind: LieroTickKind,
    /// The tick drew a frame for the host to show.
    pub shown: bool,
    /// For `TickClose`.
    pub result: i32,
}

/// What the stack calls back into.
#[repr(C)]
pub struct LieroAppHost {
    pub data: *mut c_void,
    /// Shows the frame a tick drew.
    pub show: Option<unsafe extern "C" fn(data: *mut c_void)>,
    /// The top screen is waiting for an event and showed nothing.
    pub idle: Option<unsafe extern "C" fn(data: *mut c_void)>,
}

/// An event as a Rust screen sees it. C++ has already passed it to
/// `Gfx::processEvent`, so the key states are up to date.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroEvent {
    /// A key went down; `scancode` is SDL's.
    pub key_down: bool,
    pub scancode: u32,
    /// The event is a key, joystick button or stick move one could choose
    /// for a control, and `key` is its DOS key.
    pub choice: bool,
    pub key: u32,
    /// A character typed, in the DOS code page, or 0.
    pub text: u8,
}

/// A key typed this frame, for a menu's search: SDL's key code and scancode.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroTypedKey {
    pub sym: i32,
    pub scancode: u32,
}

/// What a Rust screen calls in C++.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LieroScreenHost {
    /// The next event, if one is due (`Gfx::pollEvent`, then `processEvent`).
    pub poll_event: Option<unsafe extern "C" fn(out: *mut LieroEvent) -> bool>,
    /// Reads every event due into the key states (`Gfx::process`).
    pub process: Option<unsafe extern "C" fn()>,
    pub clear_keys: Option<unsafe extern "C" fn()>,
    pub play_sound: Option<unsafe extern "C" fn(id: i32)>,
    /// The screen shown, the copy of it the menus draw over, and the font.
    pub screen: Option<unsafe extern "C" fn(out: *mut LieroBitmap)>,
    pub frozen: Option<unsafe extern "C" fn(out: *mut LieroBitmap)>,
    pub font: Option<unsafe extern "C" fn(out: *mut LieroFont)>,
    /// Fills the screen with one colour.
    pub fill_screen: Option<unsafe extern "C" fn(color: u8)>,
    /// Shows the screen with the TC's own palette.
    pub menu_palette: Option<unsafe extern "C" fn()>,
    /// Turns the browser's text input on or off.
    pub text_input: Option<unsafe extern "C" fn(on: bool)>,
    /// Whether an SDL key was pressed since it was last asked about, which
    /// it then forgets (`Gfx::testSDLKeyOnce`).
    pub test_key_once: Option<unsafe extern "C" fn(scancode: u32) -> bool>,
    /// The keys typed this frame, at most `cap`; returns how many.
    pub typed_keys: Option<unsafe extern "C" fn(out: *mut LieroTypedKey, cap: usize) -> usize>,
    /// The menus' clock, in milliseconds (`appTicks`).
    pub ticks: Option<unsafe extern "C" fn() -> u32>,
    /// Fades the menus in and cycles their colours, once a frame
    /// (`Gfx::menuFade`).
    pub menu_fade: Option<unsafe extern "C" fn()>,
    /// Whether an SDL key is held (`Gfx::testSDLKey`).
    pub test_key: Option<unsafe extern "C" fn(scancode: u32) -> bool>,
    /// The frozen screen with the main menu over it, greyed out while
    /// another menu is up (`Gfx::drawBasicMenu`).
    pub draw_basic_menu: Option<unsafe extern "C" fn()>,
    /// The app draws: the screen, the frozen screen, the font, filling the
    /// screen, the menu palette, the menu fade and the basic menu come from
    /// the Rust app (`app_menus.rs`) instead of the functions above. Tests
    /// leave it off.
    pub app_drawing: bool,
    /// The time, in seconds since 1970 (`appTime`).
    pub time: Option<unsafe extern "C" fn() -> i64>,
    /// The stats' clock, in nanoseconds (`appHiresTicks`).
    pub hires_ticks: Option<unsafe extern "C" fn() -> u64>,
    /// `t` as a replay's name starts, "%Y-%m-%d %H.%M.%S", in UTC under
    /// the app driver and local time otherwise. Writes at most `cap` bytes
    /// and returns how many.
    pub replay_time: Option<unsafe extern "C" fn(t: i64, out: *mut u8, cap: usize) -> usize>,
    /// Where the games' sounds go (`DefaultSoundPlayer::out`).
    pub sound: Option<unsafe extern "C" fn() -> crate::sfx::LieroSoundOut>,
}

static HOST: OnceLock<LieroScreenHost> = OnceLock::new();

/// Sets the functions Rust screens call. Only the first call counts.
#[no_mangle]
pub extern "C" fn liero_screen_set_host(host: &LieroScreenHost) {
    let _ = HOST.set(*host);
}

/// `LieroScreenHost` for Rust code: a missing function does nothing.
#[derive(Clone, Copy)]
pub struct Host(pub LieroScreenHost);

impl Host {
    pub fn poll_event(&self) -> Option<LieroEvent> {
        let mut ev = LieroEvent::default();
        let f = self.0.poll_event?;
        unsafe { f(&mut ev) }.then_some(ev)
    }
    pub fn process(&self) {
        if let Some(f) = self.0.process {
            unsafe { f() }
        }
    }
    pub fn clear_keys(&self) {
        if let Some(f) = self.0.clear_keys {
            unsafe { f() }
        }
    }
    pub fn play_sound(&self, id: i32) {
        if let Some(f) = self.0.play_sound {
            unsafe { f(id) }
        }
    }
    fn bitmap(f: Option<unsafe extern "C" fn(*mut LieroBitmap)>) -> Option<LieroBitmap> {
        let f = f?;
        let mut b = LieroBitmap { pixels: std::ptr::null_mut(), w: 0, h: 0, pitch: 0, clip: LieroRect::default() };
        unsafe { f(&mut b) };
        Some(b)
    }
    pub fn screen(&self) -> Option<LieroBitmap> {
        if self.0.app_drawing {
            return Some(crate::app::app().play.bmp.view());
        }
        Self::bitmap(self.0.screen)
    }
    pub fn frozen(&self) -> Option<LieroBitmap> {
        if self.0.app_drawing {
            return Some(crate::app::app().frozen.view());
        }
        Self::bitmap(self.0.frozen)
    }
    pub fn font(&self) -> Option<LieroFont> {
        if self.0.app_drawing {
            return Some(crate::app::app().font());
        }
        let f = self.0.font?;
        let mut font = LieroFont { chars: std::ptr::null(), count: 0 };
        unsafe { f(&mut font) };
        Some(font)
    }
    pub fn fill_screen(&self, color: u8) {
        if self.0.app_drawing {
            // SAFETY: the bitmap is the app's.
            unsafe { crate::gfx::liero_gfx_fill(&crate::app::app().play.bmp.view(), color) };
            return;
        }
        if let Some(f) = self.0.fill_screen {
            unsafe { f(color) }
        }
    }
    pub fn menu_palette(&self) {
        if self.0.app_drawing {
            let a = crate::app::app();
            a.play.data.pal = a.tc_data().palette_colors;
            return;
        }
        if let Some(f) = self.0.menu_palette {
            unsafe { f() }
        }
    }
    pub fn text_input(&self, on: bool) {
        if let Some(f) = self.0.text_input {
            unsafe { f(on) }
        }
    }
    pub fn test_key(&self, scancode: u32) -> bool {
        self.0.test_key.is_some_and(|f| unsafe { f(scancode) })
    }
    pub fn draw_basic_menu(&self) {
        if self.0.app_drawing {
            crate::app_menus::draw_basic_menu();
            return;
        }
        if let Some(f) = self.0.draw_basic_menu {
            unsafe { f() }
        }
    }
    pub fn test_key_once(&self, scancode: u32) -> bool {
        self.0.test_key_once.is_some_and(|f| unsafe { f(scancode) })
    }
    pub fn typed_keys(&self) -> Vec<LieroTypedKey> {
        let Some(f) = self.0.typed_keys else {
            return Vec::new();
        };
        let mut keys = vec![LieroTypedKey::default(); 32];
        let n = unsafe { f(keys.as_mut_ptr(), keys.len()) };
        keys.truncate(n.min(32));
        keys
    }
    pub fn ticks(&self) -> u32 {
        self.0.ticks.map_or(0, |f| unsafe { f() })
    }
    pub fn menu_fade(&self) {
        if self.0.app_drawing {
            crate::app_menus::menu_fade(false);
            return;
        }
        if let Some(f) = self.0.menu_fade {
            unsafe { f() }
        }
    }
    pub fn time(&self) -> i64 {
        self.0.time.map_or(0, |f| unsafe { f() })
    }
    pub fn hires_ticks(&self) -> u64 {
        self.0.hires_ticks.map_or(0, |f| unsafe { f() })
    }
    pub fn replay_time(&self, t: i64) -> Vec<u8> {
        let Some(f) = self.0.replay_time else {
            return Vec::new();
        };
        let mut buf = vec![0; 512];
        let n = unsafe { f(t, buf.as_mut_ptr(), buf.len()) };
        buf.truncate(n.min(512));
        buf
    }
    pub fn sound(&self) -> crate::sfx::LieroSoundOut {
        self.0.sound.map_or(crate::sfx::LieroSoundOut::NONE, |f| unsafe { f() })
    }
}

/// What a Rust screen's tick does next.
pub struct Tick {
    pub kind: LieroTickKind,
    pub shown: bool,
    pub result: i32,
    pub next: Option<LieroScreen>,
}

impl Tick {
    pub fn stay(shown: bool) -> Self {
        Tick { kind: LieroTickKind::TickStay, shown, result: 0, next: None }
    }
    pub fn close(result: i32) -> Self {
        Tick { kind: LieroTickKind::TickClose, shown: false, result, next: None }
    }
    pub fn push(next: LieroScreen) -> Self {
        Tick { kind: LieroTickKind::TickPush, shown: false, result: 0, next: Some(next) }
    }
}

/// A screen written in Rust.
pub trait Screen {
    fn tick(&mut self, host: &Host) -> Tick;
    /// The screen this one opened closed with `result`.
    fn resume(&mut self, _host: &Host, _result: i32) {}
    /// Called before the screen is dropped.
    fn close(&mut self, _host: &Host) {}
}

fn host() -> Host {
    Host(HOST.get().copied().unwrap_or_default())
}

/// The host, for a screen being made from C++.
pub(crate) fn current_host() -> Host {
    host()
}

type BoxedScreen = Box<dyn Screen>;

unsafe extern "C" fn rust_tick(data: *mut c_void, out: *mut LieroTick, next: *mut LieroScreen) {
    let s = &mut *(data as *mut BoxedScreen);
    let t = s.tick(&host());
    *out = LieroTick { kind: t.kind, shown: t.shown, result: t.result };
    if let Some(n) = t.next {
        *next = n;
    }
}

unsafe extern "C" fn rust_resume(data: *mut c_void, result: i32) {
    let s = &mut *(data as *mut BoxedScreen);
    s.resume(&host(), result);
}

unsafe extern "C" fn rust_free(data: *mut c_void) {
    let mut s = Box::from_raw(data as *mut BoxedScreen);
    s.close(&host());
}

/// A Rust screen as the stack holds it.
pub fn into_ffi(s: BoxedScreen) -> LieroScreen {
    LieroScreen {
        data: Box::into_raw(Box::new(s)).cast(),
        tick: Some(rust_tick),
        resume: Some(rust_resume),
        free: Some(rust_free),
    }
}

pub struct LieroApp {
    screens: Vec<LieroScreen>,
    /// The clock's time of the last step `liero_app_frame` ran, once it has
    /// run one.
    last: Option<u32>,
}

impl LieroScreen {
    pub(crate) fn empty() -> Self {
        LieroScreen { data: std::ptr::null_mut(), tick: None, resume: None, free: None }
    }

    pub(crate) unsafe fn release(self) {
        if let Some(free) = self.free {
            free(self.data);
        }
    }
}

impl LieroApp {
    fn new() -> Self {
        LieroApp { screens: Vec::new(), last: None }
    }

    unsafe fn clear(&mut self) {
        while let Some(s) = self.screens.pop() {
            s.release();
        }
    }

    /// One step of the top screen. False once no screen is left.
    unsafe fn step(&mut self, host: &LieroAppHost) -> bool {
        let Some(top) = self.screens.last() else {
            return false;
        };
        let mut out = LieroTick { kind: LieroTickKind::TickStay, shown: false, result: 0 };
        let mut next = LieroScreen::empty();
        if let Some(tick) = top.tick {
            tick(top.data, &mut out, &mut next);
        }
        if out.shown {
            if let Some(show) = host.show {
                show(host.data);
            }
        }
        match out.kind {
            LieroTickKind::TickStay => {
                if !out.shown {
                    if let Some(idle) = host.idle {
                        idle(host.data);
                    }
                }
            }
            LieroTickKind::TickPush => self.screens.push(next),
            LieroTickKind::TickClose => {
                if let Some(s) = self.screens.pop() {
                    s.release();
                }
                if let Some(below) = self.screens.last() {
                    if let Some(resume) = below.resume {
                        resume(below.data, out.result);
                    }
                }
            }
            LieroTickKind::TickReplace => {
                if let Some(s) = self.screens.pop() {
                    s.release();
                }
                self.screens.push(next);
            }
            LieroTickKind::TickQuit => self.clear(),
        }
        !self.screens.is_empty()
    }

    /// How many steps are due at `now`, and moves the clock past them.
    fn due(&mut self, now: u32) -> u32 {
        let Some(last) = self.last else {
            self.last = Some(now);
            return 1;
        };
        let steps = now.wrapping_sub(last) / STEP_MS;
        if steps > MAX_STEPS_PER_FRAME {
            self.last = Some(now);
            MAX_STEPS_PER_FRAME
        } else {
            self.last = Some(last.wrapping_add(steps * STEP_MS));
            steps
        }
    }
}

#[no_mangle]
pub extern "C" fn liero_app_new() -> *mut LieroApp {
    Box::into_raw(Box::new(LieroApp::new()))
}

/// Frees the app and every screen on it.
///
/// # Safety
/// `app` comes from `liero_app_new` and isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_app_free(app: *mut LieroApp) {
    if !app.is_null() {
        let mut app = Box::from_raw(app);
        app.clear();
    }
}

/// Puts `screen` on top; the stack owns it from now on.
#[no_mangle]
pub extern "C" fn liero_app_push(app: &mut LieroApp, screen: LieroScreen) {
    app.screens.push(screen);
}

/// How many screens are open.
#[no_mangle]
pub extern "C" fn liero_app_depth(app: &LieroApp) -> usize {
    app.screens.len()
}

/// Closes every screen.
///
/// # Safety
/// The screens' functions are safe to call.
#[no_mangle]
pub unsafe extern "C" fn liero_app_clear(app: &mut LieroApp) {
    app.clear();
}

/// Runs one step of the top screen. Returns false once no screen is left.
///
/// # Safety
/// The screens' and host's functions are safe to call with their data.
#[no_mangle]
pub unsafe extern "C" fn liero_app_step(app: &mut LieroApp, host: &LieroAppHost) -> bool {
    app.step(host)
}

/// Runs the steps due at `now_ms` on the caller's clock: one on the first
/// call, then one per 14 ms passed, at most four. Returns false once no
/// screen is left.
///
/// # Safety
/// The screens' and host's functions are safe to call with their data.
#[no_mangle]
pub unsafe extern "C" fn liero_app_frame(app: &mut LieroApp, now_ms: u32, host: &LieroAppHost) -> bool {
    for _ in 0..app.due(now_ms) {
        if !app.step(host) {
            return false;
        }
    }
    !app.screens.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // A test screen: a name, how many ticks it stays, and what it does then.
    struct Fake {
        name: &'static str,
        stays: u32,
        then: LieroTickKind,
        open: Option<Box<Fake>>,
        log: *const RefCell<Vec<String>>,
    }

    fn log(f: &Fake, s: String) {
        unsafe { (*f.log).borrow_mut().push(s) };
    }

    unsafe extern "C" fn tick(data: *mut c_void, out: *mut LieroTick, next_out: *mut LieroScreen) {
        let f = &mut *(data as *mut Fake);
        log(f, format!("tick {}", f.name));
        let out = &mut *out;
        out.shown = f.stays % 2 == 0;
        if f.stays > 0 {
            f.stays -= 1;
            return;
        }
        out.result = 7;
        out.kind = match f.open.take() {
            Some(next) => {
                *next_out = screen(next);
                f.then
            }
            // Nothing left to open: close.
            None if f.then == LieroTickKind::TickPush => LieroTickKind::TickClose,
            None => f.then,
        };
    }

    unsafe extern "C" fn resume(data: *mut c_void, result: i32) {
        let f = &*(data as *mut Fake);
        log(f, format!("resume {} {}", f.name, result));
    }

    unsafe extern "C" fn free(data: *mut c_void) {
        let f = Box::from_raw(data as *mut Fake);
        log(&f, format!("free {}", f.name));
    }

    fn screen(f: Box<Fake>) -> LieroScreen {
        LieroScreen { data: Box::into_raw(f).cast(), tick: Some(tick), resume: Some(resume), free: Some(free) }
    }

    unsafe extern "C" fn show(data: *mut c_void) {
        (*(data as *const RefCell<Vec<String>>)).borrow_mut().push("show".into());
    }

    unsafe extern "C" fn idle(data: *mut c_void) {
        (*(data as *const RefCell<Vec<String>>)).borrow_mut().push("idle".into());
    }

    fn fake(name: &'static str, stays: u32, then: LieroTickKind, open: Option<Box<Fake>>, log: &RefCell<Vec<String>>) -> Box<Fake> {
        Box::new(Fake { name, stays, then, open, log })
    }

    #[test]
    fn push_close_and_replace() {
        let log = RefCell::new(Vec::new());
        let host = LieroAppHost { data: (&log as *const RefCell<Vec<String>>).cast_mut().cast(), show: Some(show), idle: Some(idle) };
        let c = fake("c", 0, LieroTickKind::TickClose, None, &log);
        let b = fake("b", 1, LieroTickKind::TickReplace, Some(c), &log);
        let a = fake("a", 0, LieroTickKind::TickPush, Some(b), &log);
        let app = liero_app_new();
        unsafe {
            liero_app_push(&mut *app, screen(a));
            while liero_app_step(&mut *app, &host) {}
            liero_app_free(app);
        }
        let log = log.into_inner();
        // a opens b; b waits once (no frame, so idle), then shows and is
        // replaced by c; c closes into a, which has nothing left to open and
        // closes too.
        assert_eq!(
            log,
            [
                "tick a", "show", "tick b", "idle", "tick b", "show", "free b", "tick c", "show", "free c",
                "resume a 7", "tick a", "show", "free a"
            ]
        );
    }

    #[test]
    fn quit_frees_every_screen() {
        let log = RefCell::new(Vec::new());
        let host = LieroAppHost { data: std::ptr::null_mut(), show: None, idle: None };
        let b = fake("b", 0, LieroTickKind::TickQuit, None, &log);
        let a = fake("a", 0, LieroTickKind::TickPush, Some(b), &log);
        let app = liero_app_new();
        unsafe {
            liero_app_push(&mut *app, screen(a));
            assert!(liero_app_step(&mut *app, &host));
            assert!(!liero_app_step(&mut *app, &host));
            assert_eq!(liero_app_depth(&*app), 0);
            liero_app_free(app);
        }
        assert_eq!(log.into_inner(), ["tick a", "tick b", "free b", "free a"]);
    }

    #[test]
    fn frame_runs_the_steps_due_and_caps_them() {
        let mut app = LieroApp::new();
        assert_eq!(app.due(1000), 1);
        assert_eq!(app.due(1013), 0);
        assert_eq!(app.due(1014), 1);
        assert_eq!(app.due(1014 + 3 * 14 + 5), 3);
        // The 5 ms left over carries into the next frame.
        assert_eq!(app.due(1014 + 4 * 14), 1);
        // Back from the background: four steps, and the rest is skipped.
        assert_eq!(app.due(100_000), MAX_STEPS_PER_FRAME);
        assert_eq!(app.due(100_014), 1);
    }
}
