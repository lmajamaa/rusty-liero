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
//! A screen is a `Box<dyn Screen>`. It reaches the platform (events, sound,
//! the clock) through a `Host`, over the `Platform` the shell sets once.

use std::cell::RefCell;
use std::rc::Rc;

use crate::gfx::LieroBitmap;
use crate::text::LieroFont;

/// How long one step is, in milliseconds.
pub const STEP_MS: u32 = 14;

/// The most steps one call of `liero_app_frame` runs. A tab coming back from
/// the background skips the time it missed instead of racing to catch up.
pub const MAX_STEPS_PER_FRAME: u32 = 4;

/// A screen, owned by the stack once pushed.
pub type BoxedScreen = Box<dyn Screen>;

/// What happens after a screen's tick.
pub enum Transition {
    /// The screen stays on top.
    Stay,
    /// Opens a screen on top; this one gets `resume` when it closes.
    Open(BoxedScreen),
    /// Closes, handing the result to the screen below.
    Close(i32),
    /// Closes, and the screen takes its place.
    Replace(BoxedScreen),
    /// Closes every screen: the app is done.
    Quit,
}

/// An event as a Rust screen sees it. C++ has already passed it to
/// `Gfx::processEvent`, so the key states are up to date.
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroTypedKey {
    pub sym: i32,
    pub scancode: u32,
}

/// What the screens need of the platform: events and keys, sound, the
/// clock, and showing a frame. Each does nothing by default.
pub trait Platform {
    /// The next event, if one is due, already read into the key states.
    fn poll_event(&self) -> Option<LieroEvent> {
        None
    }
    /// Reads every event due into the key states (`Gfx::process`).
    fn process(&self) {}
    fn clear_keys(&self) {}
    fn play_sound(&self, _id: i32) {}
    /// Turns the browser's text input on or off.
    fn text_input(&self, _on: bool) {}
    /// Whether an SDL key was pressed since it was last asked about, which
    /// it then forgets (`Gfx::testSDLKeyOnce`).
    fn test_key_once(&self, _scancode: u32) -> bool {
        false
    }
    /// Whether an SDL key is held (`Gfx::testSDLKey`).
    fn test_key(&self, _scancode: u32) -> bool {
        false
    }
    /// The keys typed this frame.
    fn typed_keys(&self) -> Vec<LieroTypedKey> {
        Vec::new()
    }
    /// The menus' clock, in milliseconds (`appTicks`).
    fn ticks(&self) -> u32 {
        0
    }
    /// The app draws: the screens draw on the app's screen, frozen screen
    /// and font. Tests leave it off, and drawing is skipped.
    fn app_drawing(&self) -> bool {
        false
    }
    /// The time, in seconds since 1970 (`appTime`).
    fn time(&self) -> i64 {
        0
    }
    /// The stats' clock, in nanoseconds (`appHiresTicks`).
    fn hires_ticks(&self) -> u64 {
        0
    }
    /// `t` as a replay's name starts, "%Y-%m-%d %H.%M.%S", in local time,
    /// or UTC in the hash run.
    fn replay_time(&self, _t: i64) -> Vec<u8> {
        Vec::new()
    }
    /// Where the games' sounds go (`DefaultSoundPlayer::out`).
    fn sound(&self) -> crate::sfx::LieroSoundOut {
        crate::sfx::LieroSoundOut::NONE
    }
    /// Shows the frame a tick drew.
    fn show(&self) {}
    /// The top screen is waiting for an event and showed nothing.
    fn idle(&self) {}
}

/// A platform that does nothing, before the shell sets one.
struct NoPlatform;

impl Platform for NoPlatform {}

thread_local! {
    static PLATFORM: RefCell<Option<Rc<dyn Platform>>> = const { RefCell::new(None) };
}

/// Sets the platform the screens use. Only the first call counts.
pub fn liero_screen_set_host(platform: Rc<dyn Platform>) {
    PLATFORM.with(|p| {
        p.borrow_mut().get_or_insert(platform);
    });
}

/// The platform as the screens use it, with the app's drawing.
#[derive(Clone)]
pub struct Host(Rc<dyn Platform>);

impl Host {
    pub fn new(platform: Rc<dyn Platform>) -> Self {
        Host(platform)
    }
    /// A host that does nothing.
    pub fn none() -> Self {
        Host(Rc::new(NoPlatform))
    }
    pub fn poll_event(&self) -> Option<LieroEvent> {
        self.0.poll_event()
    }
    pub fn process(&self) {
        self.0.process()
    }
    pub fn clear_keys(&self) {
        self.0.clear_keys()
    }
    pub fn play_sound(&self, id: i32) {
        self.0.play_sound(id)
    }
    /// The screen shown, when the app draws.
    pub fn screen(&self) -> Option<LieroBitmap<'static>> {
        self.0.app_drawing().then(|| crate::app::app().play.bmp.view())
    }
    /// The copy of the screen the menus draw over.
    pub fn frozen(&self) -> Option<LieroBitmap<'static>> {
        self.0.app_drawing().then(|| crate::app::app().frozen.view())
    }
    pub fn font(&self) -> Option<LieroFont<'static>> {
        self.0.app_drawing().then(crate::app::menu_font)
    }
    pub fn fill_screen(&self, color: u8) {
        if self.0.app_drawing() {
            crate::gfx::liero_gfx_fill(&crate::app::app().play.bmp.view(), color);
        }
    }
    /// Shows the screen with the TC's own palette.
    pub fn menu_palette(&self) {
        if self.0.app_drawing() {
            let a = crate::app::app();
            a.play.data.pal = crate::app::menu_tc_data().palette_colors;
        }
    }
    pub fn text_input(&self, on: bool) {
        self.0.text_input(on)
    }
    pub fn test_key(&self, scancode: u32) -> bool {
        self.0.test_key(scancode)
    }
    /// The frozen screen with the main menu over it, greyed out while
    /// another menu is up (`Gfx::drawBasicMenu`).
    pub fn draw_basic_menu(&self) {
        if self.0.app_drawing() {
            crate::app_menus::draw_basic_menu();
        }
    }
    pub fn test_key_once(&self, scancode: u32) -> bool {
        self.0.test_key_once(scancode)
    }
    pub fn typed_keys(&self) -> Vec<LieroTypedKey> {
        self.0.typed_keys()
    }
    pub fn ticks(&self) -> u32 {
        self.0.ticks()
    }
    /// Fades the menus in and cycles their colours, once a frame
    /// (`Gfx::menuFade`).
    pub fn menu_fade(&self) {
        if self.0.app_drawing() {
            crate::app_menus::menu_fade(false);
        }
    }
    pub fn time(&self) -> i64 {
        self.0.time()
    }
    pub fn hires_ticks(&self) -> u64 {
        self.0.hires_ticks()
    }
    pub fn replay_time(&self, t: i64) -> Vec<u8> {
        self.0.replay_time(t)
    }
    pub fn sound(&self) -> crate::sfx::LieroSoundOut {
        self.0.sound()
    }
    fn show(&self) {
        self.0.show()
    }
    fn idle(&self) {
        self.0.idle()
    }
}

/// A tick's outcome: what happens next, and whether it drew a frame for
/// the host to show.
pub struct Tick {
    pub then: Transition,
    pub shown: bool,
}

impl Tick {
    pub fn stay(shown: bool) -> Self {
        Tick {
            then: Transition::Stay,
            shown,
        }
    }
    pub fn close(result: i32) -> Self {
        Tick {
            then: Transition::Close(result),
            shown: false,
        }
    }
    pub fn push(next: BoxedScreen) -> Self {
        Tick {
            then: Transition::Open(next),
            shown: false,
        }
    }

    /// The result it closes with, if it closes.
    pub fn closed(&self) -> Option<i32> {
        match self.then {
            Transition::Close(r) => Some(r),
            _ => None,
        }
    }
}

/// A screen. What it needs when it goes, it does in `Drop`.
pub trait Screen {
    fn tick(&mut self, host: &Host) -> Tick;
    /// The screen this one opened closed with `result`.
    fn resume(&mut self, _host: &Host, _result: i32) {}
}

/// The platform the shell set, as a host.
pub(crate) fn current_host() -> Host {
    PLATFORM.with(|p| p.borrow().clone()).map_or_else(Host::none, Host)
}

pub struct LieroApp {
    screens: Vec<BoxedScreen>,
    /// The clock's time of the last step `liero_app_frame` ran, once it has
    /// run one.
    last: Option<u32>,
}

impl LieroApp {
    /// An app with no screens.
    pub fn new() -> Self {
        LieroApp {
            screens: Vec::new(),
            last: None,
        }
    }

    /// One step of the top screen. False once no screen is left.
    fn step(&mut self, host: &Host) -> bool {
        let Some(top) = self.screens.last_mut() else {
            return false;
        };
        let t = top.tick(host);
        if t.shown {
            host.show();
        }
        match t.then {
            Transition::Stay => {
                if !t.shown {
                    host.idle();
                }
            }
            Transition::Open(next) => self.screens.push(next),
            Transition::Close(result) => {
                self.screens.pop();
                if let Some(below) = self.screens.last_mut() {
                    below.resume(host, result);
                }
            }
            Transition::Replace(next) => {
                self.screens.pop();
                self.screens.push(next);
            }
            // The top screen goes first.
            Transition::Quit => while self.screens.pop().is_some() {},
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

impl Default for LieroApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LieroApp {
    /// The top screen goes first.
    fn drop(&mut self) {
        while self.screens.pop().is_some() {}
    }
}

/// Puts `screen` on top; the stack owns it from now on.
pub fn liero_app_push(app: &mut LieroApp, screen: BoxedScreen) {
    app.screens.push(screen);
}

/// How many screens are open.
pub fn liero_app_depth(app: &LieroApp) -> usize {
    app.screens.len()
}

/// Runs one step of the top screen with `host`. Returns false once no
/// screen is left.
pub fn liero_app_step(app: &mut LieroApp, host: &Host) -> bool {
    app.step(host)
}

/// Runs the steps due at `now_ms` on the caller's clock: one on the first
/// call, then one per 14 ms passed, at most four. Returns false once no
/// screen is left.
pub fn liero_app_frame(app: &mut LieroApp, now_ms: u32, host: &Host) -> bool {
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

    /// What a test screen does once it has stayed.
    #[derive(Clone, Copy, PartialEq)]
    enum Then {
        Open,
        Close,
        Replace,
        Quit,
    }

    // A test screen: a name, how many ticks it stays, and what it does then.
    struct Fake {
        name: &'static str,
        stays: u32,
        then: Then,
        open: Option<Box<Fake>>,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl Fake {
        fn log(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
    }

    impl Screen for Fake {
        fn tick(&mut self, _host: &Host) -> Tick {
            self.log(format!("tick {}", self.name));
            let shown = self.stays.is_multiple_of(2);
            if self.stays > 0 {
                self.stays -= 1;
                return Tick::stay(shown);
            }
            let then = match (self.open.take(), self.then) {
                (Some(next), Then::Open) => Transition::Open(next),
                (Some(next), Then::Replace) => Transition::Replace(next),
                (_, Then::Quit) => Transition::Quit,
                // Nothing left to open: close.
                _ => Transition::Close(7),
            };
            Tick { then, shown }
        }

        fn resume(&mut self, _host: &Host, result: i32) {
            self.log(format!("resume {} {}", self.name, result));
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.log(format!("free {}", self.name));
        }
    }

    /// Logs the frames shown and the idle steps.
    struct LogPlatform(Rc<RefCell<Vec<String>>>);

    impl Platform for LogPlatform {
        fn show(&self) {
            self.0.borrow_mut().push("show".into());
        }
        fn idle(&self) {
            self.0.borrow_mut().push("idle".into());
        }
    }

    fn fake(
        name: &'static str,
        stays: u32,
        then: Then,
        open: Option<Box<Fake>>,
        log: &Rc<RefCell<Vec<String>>>,
    ) -> Box<Fake> {
        Box::new(Fake {
            name,
            stays,
            then,
            open,
            log: log.clone(),
        })
    }

    #[test]
    fn push_close_and_replace() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let host = Host::new(Rc::new(LogPlatform(log.clone())));
        let c = fake("c", 0, Then::Close, None, &log);
        let b = fake("b", 1, Then::Replace, Some(c), &log);
        let a = fake("a", 0, Then::Open, Some(b), &log);
        let mut app = LieroApp::new();
        liero_app_push(&mut app, a);
        while liero_app_step(&mut app, &host) {}
        drop(app);
        let log = log.borrow().clone();
        // a opens b; b waits once (no frame, so idle), then shows and is
        // replaced by c; c closes into a, which has nothing left to open and
        // closes too.
        assert_eq!(
            log,
            [
                "tick a",
                "show",
                "tick b",
                "idle",
                "tick b",
                "show",
                "free b",
                "tick c",
                "show",
                "free c",
                "resume a 7",
                "tick a",
                "show",
                "free a"
            ]
        );
    }

    #[test]
    fn quit_frees_every_screen() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let host = Host::none();
        let b = fake("b", 0, Then::Quit, None, &log);
        let a = fake("a", 0, Then::Open, Some(b), &log);
        let mut app = LieroApp::new();
        liero_app_push(&mut app, a);
        assert!(liero_app_step(&mut app, &host));
        assert!(!liero_app_step(&mut app, &host));
        assert_eq!(liero_app_depth(&app), 0);
        assert_eq!(*log.borrow(), ["tick a", "tick b", "free b", "free a"]);
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
