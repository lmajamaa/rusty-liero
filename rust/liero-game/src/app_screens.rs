//! The app's outer screens (`MainScreen` and `GameScreen` in C++): a level
//! behind the main menu, then whatever the menu chose, and a game run by the
//! controller until it stops. The controller and what the menu's choices do
//! stay in C++ for now (`LieroAppHooks`).

use crate::screen::{into_ffi, Host, LieroScreen, Screen, Tick};

/// What `after_menu` says to do.
pub const LIERO_AFTER_MENU_QUIT: i32 = 0;
pub const LIERO_AFTER_MENU_RESTART: i32 = 1;
pub const LIERO_AFTER_MENU_PLAY: i32 = 2;

/// What the app's screens call in C++.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LieroAppHooks {
    /// Reads the frame's events, passing keys to the controller
    /// (`Gfx::process(controller)`).
    pub process: unsafe extern "C" fn(),
    /// The controller's `focus`, `process` (false when it stops) and
    /// `unfocus` with the keys cleared.
    pub focus: unsafe extern "C" fn(),
    pub run: unsafe extern "C" fn() -> bool,
    pub finish: unsafe extern "C" fn(),
    /// Draws the controller on both screens, counting a menu frame when
    /// `count`.
    pub draw: unsafe extern "C" fn(count: bool),
    /// Moves the screen the controller opened (`Gfx::open`) into `out`.
    /// False if it opened none.
    pub take_opened: unsafe extern "C" fn(out: *mut LieroScreen) -> bool,
    /// What the controller does with that screen's result.
    pub then: unsafe extern "C" fn(result: i32),
    /// A new controller with a new level behind the menu.
    pub restart: unsafe extern "C" fn(),
    /// Does what the menu chose (`selection`): a `LIERO_AFTER_MENU_*`.
    pub after_menu: unsafe extern "C" fn(selection: i32) -> i32,
    /// Drops the controller before quitting.
    pub quit: unsafe extern "C" fn(),
    /// The main menu screen.
    pub menu_screen: unsafe extern "C" fn() -> LieroScreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Next {
    Focus,
    Process,
    Draw,
    Finish,
}

/// A game, until the controller stops it. What the controller opens while it
/// runs (the stats screen, a message) is pushed on top, and the game goes on
/// from where it stopped when that closes.
struct GameScreen {
    h: LieroAppHooks,
    next: Next,
    shown: bool,
    resumed: bool,
}

impl GameScreen {
    fn opened(&self) -> Option<LieroScreen> {
        let mut s = LieroScreen::empty();
        unsafe { (self.h.take_opened)(&mut s) }.then_some(s)
    }
}

impl Screen for GameScreen {
    fn tick(&mut self, _host: &Host) -> Tick {
        let h = self.h;
        // Each tick reads the events that came with the frame before it,
        // unless a screen the controller opened just closed: it read them.
        if self.resumed {
            self.resumed = false;
        } else if self.shown {
            unsafe { (h.process)() };
        }

        if self.next == Next::Focus {
            unsafe { (h.focus)() };
            self.next = Next::Process;
            if let Some(s) = self.opened() {
                return Tick::push(s);
            }
        }

        if self.next == Next::Process {
            self.next = if unsafe { (h.run)() } { Next::Draw } else { Next::Finish };
            if let Some(s) = self.opened() {
                return Tick::push(s);
            }
        }

        if self.next == Next::Finish {
            unsafe { (h.finish)() };
            return Tick::close(0);
        }

        unsafe { (h.draw)(true) };
        self.next = Next::Process;
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _host: &Host, result: i32) {
        unsafe { (self.h.then)(result) };
        self.resumed = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Nothing,
    InMenu,
    InGame,
}

/// The app: a level behind the main menu, then whatever the menu chose.
struct MainScreen {
    h: LieroAppHooks,
    /// What's open on top of it.
    open: Open,
    selection: i32,
}

impl Screen for MainScreen {
    fn tick(&mut self, _host: &Host) -> Tick {
        let h = self.h;
        if self.open == Open::Nothing {
            unsafe { (h.restart)() };
        } else if self.open == Open::InMenu {
            match unsafe { (h.after_menu)(self.selection) } {
                LIERO_AFTER_MENU_QUIT => {
                    unsafe { (h.quit)() };
                    return Tick::close(0);
                }
                LIERO_AFTER_MENU_PLAY => {
                    self.open = Open::InGame;
                    return Tick::push(into_ffi(Box::new(GameScreen {
                        h,
                        next: Next::Focus,
                        shown: false,
                        resumed: false,
                    })));
                }
                _ => {}
            }
        }

        // The level, or the game left off, behind the menu.
        unsafe { (h.draw)(false) };
        self.open = Open::InMenu;
        Tick::push(unsafe { (h.menu_screen)() })
    }

    fn resume(&mut self, _host: &Host, result: i32) {
        self.selection = result;
    }
}

/// The app's first screen.
#[no_mangle]
pub extern "C" fn liero_main_screen(hooks: LieroAppHooks) -> LieroScreen {
    into_ffi(Box::new(MainScreen { h: hooks, open: Open::Nothing, selection: -1 }))
}
