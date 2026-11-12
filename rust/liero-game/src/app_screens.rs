//! The app's outer screens (`MainScreen` and `GameScreen` in C++): a level
//! behind the main menu, then whatever the menu chose, and a game run by the
//! controller until it stops. What the menu's choices do is here too
//! (`restartApp` and `afterMenu`); reading the frame's events and the menu
//! screen are C++ (`LieroAppHooks`).

use crate::app::{app, LieroApp};
use crate::controllers::{
    liero_controller_draw, liero_controller_focus, liero_controller_is_replay, liero_controller_level,
    liero_controller_run, liero_controller_take_opened, liero_controller_unfocus, liero_local_controller_new,
};
use crate::level::LieroLevelBox;
use crate::screen::{current_host, into_ffi, Host, LieroScreen, Screen, Tick};

/// What `after_menu` says to do.
pub const LIERO_AFTER_MENU_QUIT: i32 = 0;
pub const LIERO_AFTER_MENU_RESTART: i32 = 1;
pub const LIERO_AFTER_MENU_PLAY: i32 = 2;

/// The main menu's items that start something (`MainMenu`).
const MAIN_RESUME_GAME: i32 = 0;
const MAIN_NEW_GAME: i32 = 1;
const MAIN_QUIT: i32 = 6;
const MAIN_REPLAY: i32 = 8;
const MAIN_TC: i32 = 9;

/// What the app's screens call in C++.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LieroAppHooks {
    /// Reads the frame's events, passing keys to the controller
    /// (`Gfx::process(controller)`).
    pub process: unsafe extern "C" fn(),
    /// The main menu screen.
    pub menu_screen: unsafe extern "C" fn() -> LieroScreen,
}

fn controller(a: &mut LieroApp) -> &mut crate::controllers::LieroController {
    a.controller.as_deref_mut().expect("no controller")
}

/// A new local controller with the menus' settings and TC.
fn new_local(a: &mut LieroApp) -> Box<crate::controllers::LieroController> {
    // SAFETY: the menus' settings and TC are valid while C++ runs.
    unsafe { Box::from_raw(liero_local_controller_new(a.menu_settings(), a.menu_tc())) }
}

/// A level made from the menus' settings with the app's random numbers.
fn new_level(a: &mut LieroApp) -> Box<LieroLevelBox> {
    let mut level = LieroLevelBox::new();
    // SAFETY: the TC has its data while C++ keeps it.
    unsafe {
        let tc = &*crate::tc::liero_tc_data(&*a.menu_tc());
        level.generate_from_settings(&tc.level_common(), &tc.palette_colors, a.menu_settings(), &mut a.rand);
    }
    level
}

/// Puts `level` in the controller's place, and the controller's there.
fn swap_level(c: &mut crate::controllers::LieroController, level: &mut LieroLevelBox) {
    let current = liero_controller_level(c);
    // SAFETY: the controller's level is valid while it is.
    unsafe { crate::level::liero_level_box_swap(current.as_mut().expect("no level"), level) };
}

/// A new controller with a new level behind the menu (`restartApp`).
fn restart(a: &mut LieroApp) {
    a.controller = None;
    a.controller = Some(new_local(a));
    let mut level = new_level(a);
    swap_level(controller(a), &mut level);
    let game = crate::controllers::liero_controller_game(controller(a));
    // SAFETY: a local controller always has its game.
    unsafe {
        crate::controllers::liero_game_focus(&mut *game, &mut *a.play);
        crate::controllers::liero_game_focus(&mut *game, &mut *a.single);
    }
}

/// Does what the menu chose (`afterMenu`): quits, starts over with a new
/// TC, or plays.
fn after_menu(a: &mut LieroApp, selection: i32) -> i32 {
    match selection {
        MAIN_NEW_GAME => {
            let mut new = new_local(a);
            let s = a.menu_settings();
            // SAFETY: the controller's level is valid while it is.
            let old = a
                .controller
                .as_deref_mut()
                .and_then(|c| unsafe { liero_controller_level(c).as_mut() });
            match old {
                Some(old)
                    if !s.data.regenerate_level
                        && s.data.random_level == old.data().old_random_level
                        && s.level_file == old.old_level_file() =>
                {
                    // Take level and palette from old game
                    swap_level(&mut new, old);
                }
                _ => {
                    let mut level = new_level(a);
                    swap_level(&mut new, &mut level);
                }
            }
            a.controller = Some(new);
        }
        MAIN_RESUME_GAME => {
            if liero_controller_is_replay(controller(a)) {
                a.primary_single = true;
            }
        }
        MAIN_QUIT => return LIERO_AFTER_MENU_QUIT,
        MAIN_REPLAY => {
            if a.menu_settings().data.single_screen_replay {
                a.primary_single = true;
            }
        }
        MAIN_TC => {
            restart(a);
            return LIERO_AFTER_MENU_RESTART;
        }
        _ => {}
    }
    LIERO_AFTER_MENU_PLAY
}

/// Draws the controller on both screens, counting a menu frame when
/// `count`.
fn draw(a: &mut LieroApp, count: bool) {
    a.play.clear();
    let c = a.controller.as_deref_mut().expect("no controller");
    // SAFETY: the renderers are the app's.
    unsafe {
        liero_controller_draw(c, &mut *a.play, false);
        a.single.clear();
        liero_controller_draw(c, &mut *a.single, true);
    }
    if count {
        a.menu_cycles = a.menu_cycles.wrapping_add(1);
    }
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
        // SAFETY: `s` is valid.
        unsafe { liero_controller_take_opened(controller(app()), &mut s) }.then_some(s)
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
            // SAFETY: the controller is valid.
            unsafe { liero_controller_focus(controller(app())) };
            self.next = Next::Process;
            if let Some(s) = self.opened() {
                return Tick::push(s);
            }
        }

        if self.next == Next::Process {
            // SAFETY: the controller is valid.
            self.next = if unsafe { liero_controller_run(controller(app())) } {
                Next::Draw
            } else {
                Next::Finish
            };
            if let Some(s) = self.opened() {
                return Tick::push(s);
            }
        }

        if self.next == Next::Finish {
            let a = app();
            a.primary_single = false;
            // SAFETY: the controller is valid.
            unsafe { liero_controller_unfocus(controller(a)) };
            current_host().clear_keys();
            return Tick::close(0);
        }

        draw(app(), true);
        self.next = Next::Process;
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _host: &Host, _result: i32) {
        // What the controller opened needs nothing done after it.
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
            restart(app());
        } else if self.open == Open::InMenu {
            match after_menu(app(), self.selection) {
                LIERO_AFTER_MENU_QUIT => {
                    app().controller = None;
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
        draw(app(), false);
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
    into_ffi(Box::new(MainScreen {
        h: hooks,
        open: Open::Nothing,
        selection: -1,
    }))
}

#[cfg(test)]
#[path = "app_screens_tests.rs"]
mod tests;
