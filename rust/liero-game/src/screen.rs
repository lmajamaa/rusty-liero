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
//! The screens are C++ objects for now, seen here through a table of
//! functions (`LieroScreen`).

use std::ffi::c_void;

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

pub struct LieroApp {
    screens: Vec<LieroScreen>,
    /// The clock's time of the last step `liero_app_frame` ran, once it has
    /// run one.
    last: Option<u32>,
}

impl LieroScreen {
    fn empty() -> Self {
        LieroScreen { data: std::ptr::null_mut(), tick: None, resume: None, free: None }
    }

    unsafe fn release(self) {
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
