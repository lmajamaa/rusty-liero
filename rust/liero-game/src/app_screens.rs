//! The app's outer screens (`MainScreen` and `GameScreen` in C++): a level
//! behind the main menu, then whatever the menu chose, and a game run by the
//! controller until it stops. What the menu's choices do is here too
//! (`restartApp` and `afterMenu`).
//! The menus themselves are `menu_screen.rs`.

use crate::app::LieroApp;
use crate::controllers::LieroController;
use crate::controllers::{
    liero_controller_draw, liero_controller_focus, liero_controller_is_replay, liero_controller_level,
    liero_controller_run, liero_controller_take_opened, liero_controller_unfocus, liero_local_controller_new,
};
use crate::level::LieroLevelBox;
use crate::menu_screen::liero_menu_screen;
use crate::renderer::LieroRenderer;
use crate::screen::{BoxedScreen, Host, Screen, Tick};

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

fn controller(a: &mut LieroApp) -> &mut crate::controllers::LieroController {
    a.controller.as_deref_mut().expect("no controller")
}

/// Runs `f` on the app's controller, which is taken out of the app while
/// it runs.
fn with_controller<R>(a: &mut LieroApp, f: impl FnOnce(&mut LieroController, &mut LieroApp) -> R) -> R {
    let mut c = a.controller.take().expect("no controller");
    let r = f(&mut c, a);
    a.controller = Some(c);
    r
}

/// A new local controller with the menus' settings and TC.
fn new_local(a: &mut LieroApp) -> Box<crate::controllers::LieroController> {
    // SAFETY: the app has a host while it runs.
    unsafe { Box::from_raw(liero_local_controller_new(a.shared_settings(), a.menu_tc())) }
}

/// A level made from the menus' settings with the app's random numbers.
fn new_level(a: &mut LieroApp) -> Box<LieroLevelBox> {
    let mut level = LieroLevelBox::new();
    let (tc, settings) = (a.menu_tc(), a.shared_settings());
    let tc = crate::tc::tc_data(&tc);
    level.generate_from_settings(&tc.level_common(), &tc.palette_colors, &settings.borrow(), &mut a.rand);
    level
}

/// Puts `level` in the controller's place, and the controller's there.
fn swap_level(c: &mut crate::controllers::LieroController, level: &mut LieroLevelBox) {
    let current = liero_controller_level(c);
    // SAFETY: the controller's level is valid while it is.
    std::mem::swap(unsafe { current.as_mut() }.expect("no level"), level);
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
            let settings = a.shared_settings();
            // SAFETY: the controller's level is valid while it is.
            let old = a
                .controller
                .as_deref_mut()
                .and_then(|c| unsafe { liero_controller_level(c).as_mut() })
                .filter(|old| {
                    let s = settings.borrow();
                    !s.data.regenerate_level
                        && s.data.random_level == old.data().old_random_level
                        && s.level_file == old.old_level_file()
                });
            match old {
                Some(old) => {
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
            if a.settings_mut().data.single_screen_replay {
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
    with_controller(a, |c, a| {
        // SAFETY: the renderers are the app's.
        unsafe {
            let play: *mut LieroRenderer = &mut *a.play;
            liero_controller_draw(c, a, play, false);
            a.single.clear();
            let single: *mut LieroRenderer = &mut *a.single;
            liero_controller_draw(c, a, single, true);
        }
    });
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
    next: Next,
    shown: bool,
    resumed: bool,
}

impl GameScreen {
    fn opened(app: &mut LieroApp) -> Option<BoxedScreen> {
        liero_controller_take_opened(controller(app))
    }
}

impl Screen for GameScreen {
    fn tick(&mut self, app: &mut LieroApp, host: &Host) -> Tick {
        // Each tick reads the events that came with the frame before it,
        // unless a screen the controller opened just closed: it read them.
        if self.resumed {
            self.resumed = false;
        } else if self.shown {
            host.process_controller(app);
        }

        if self.next == Next::Focus {
            // SAFETY: the controller is valid.
            with_controller(app, |c, app| unsafe { liero_controller_focus(c, app) });
            self.next = Next::Process;
            if let Some(s) = Self::opened(app) {
                return Tick::push(s);
            }
        }

        if self.next == Next::Process {
            // SAFETY: the controller is valid.
            self.next = if with_controller(app, |c, app| unsafe { liero_controller_run(c, app) }) {
                Next::Draw
            } else {
                Next::Finish
            };
            if let Some(s) = Self::opened(app) {
                return Tick::push(s);
            }
        }

        if self.next == Next::Finish {
            app.primary_single = false;
            liero_controller_unfocus(controller(app));
            app.clear_keys();
            return Tick::close(0);
        }

        draw(app, true);
        self.next = Next::Process;
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _app: &mut LieroApp, _host: &Host, _result: i32) {
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
    /// What's open on top of it.
    open: Open,
    selection: i32,
}

impl Screen for MainScreen {
    fn tick(&mut self, app: &mut LieroApp, _host: &Host) -> Tick {
        if self.open == Open::Nothing {
            restart(app);
        } else if self.open == Open::InMenu {
            match after_menu(app, self.selection) {
                LIERO_AFTER_MENU_QUIT => {
                    app.controller = None;
                    return Tick::close(0);
                }
                LIERO_AFTER_MENU_PLAY => {
                    self.open = Open::InGame;
                    return Tick::push(Box::new(GameScreen {
                        next: Next::Focus,
                        shown: false,
                        resumed: false,
                    }));
                }
                _ => {}
            }
        }

        // The level, or the game left off, behind the menu.
        draw(app, false);
        self.open = Open::InMenu;
        Tick::push(liero_menu_screen(app.shared_menus()))
    }

    fn resume(&mut self, _app: &mut LieroApp, _host: &Host, result: i32) {
        self.selection = result;
    }
}

/// The app's first screen.
pub fn liero_main_screen() -> BoxedScreen {
    Box::new(MainScreen {
        open: Open::Nothing,
        selection: -1,
    })
}

#[cfg(test)]
#[path = "app_screens_tests.rs"]
mod tests;
