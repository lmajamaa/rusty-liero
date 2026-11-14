//! The app's outer screens (`MainScreen` and `GameScreen` in C++): a level
//! behind the main menu, then whatever the menu chose, and a game run by the
//! controller until it stops. What the menu's choices do is here too
//! (`restartApp` and `afterMenu`).
//! The menus themselves are `menu::screen`.

use crate::app::App;
use crate::controllers::Controller;
use crate::level::LevelBox;
use crate::menu::menus::{MAIN_NEW_GAME, MAIN_QUIT, MAIN_REPLAY, MAIN_RESUME_GAME, MAIN_TC};
use crate::menu::screen::menu_screen;
use crate::screen::{BoxedScreen, Host, Screen, Tick};

/// What `after_menu` says to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterMenu {
    Quit,
    Restart,
    Play,
}

fn controller(a: &mut App) -> &mut crate::controllers::Controller {
    a.controller.as_deref_mut().expect("no controller")
}

/// Runs `f` on the app's controller, which is taken out of the app while
/// it runs.
fn with_controller<R>(a: &mut App, f: impl FnOnce(&mut Controller, &mut App) -> R) -> R {
    let mut c = a.controller.take().expect("no controller");
    let r = f(&mut c, a);
    a.controller = Some(c);
    r
}

/// A new local controller with the menus' settings and TC.
fn new_local(a: &mut App, host: &Host) -> Box<crate::controllers::Controller> {
    Controller::local(host, a.shared_settings(), a.menu_tc())
}

/// A level made from the menus' settings with the app's random numbers.
fn new_level(a: &mut App) -> Box<LevelBox> {
    let mut level = LevelBox::new();
    let (tc, settings) = (a.menu_tc(), a.shared_settings());
    let tc = crate::tc::tc_data(&tc);
    level.generate_from_settings(&tc.level_common(), &tc.palette_colors, &settings.borrow(), &mut a.rand);
    level
}

/// Puts `level` in the controller's place, and the controller's there.
fn swap_level(c: &mut crate::controllers::Controller, level: &mut LevelBox) {
    std::mem::swap(c.level().expect("no level"), level);
}

/// A new controller with a new level behind the menu (`restartApp`).
fn restart(a: &mut App, host: &Host) {
    a.controller = None;
    a.controller = Some(new_local(a, host));
    let mut level = new_level(a);
    swap_level(controller(a), &mut level);
    let App {
        controller,
        play,
        single,
        ..
    } = a;
    let game = controller
        .as_deref_mut()
        .and_then(|c| c.game())
        .expect("a local controller has its game");
    game.focus(play);
    game.focus(single);
}

/// Does what the menu chose (`afterMenu`): quits, starts over with a new
/// TC, or plays.
fn after_menu(a: &mut App, host: &Host, selection: i32) -> AfterMenu {
    match selection {
        MAIN_NEW_GAME => {
            let mut new = new_local(a, host);
            let settings = a.shared_settings();
            let old = a.controller.as_deref_mut().and_then(|c| c.level()).filter(|old| {
                let s = settings.borrow();
                !s.data.regenerate_level
                    && s.data.random_level == old.data().old_random_level
                    && s.data.level_file == old.old_level_file()
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
            if controller(a).is_replay() {
                a.primary_single = true;
            }
        }
        MAIN_QUIT => return AfterMenu::Quit,
        MAIN_REPLAY => {
            if a.settings_mut().data.ext.single_screen_replay {
                a.primary_single = true;
            }
        }
        MAIN_TC => {
            restart(a, host);
            return AfterMenu::Restart;
        }
        _ => {}
    }
    AfterMenu::Play
}

/// Draws the controller on both screens, counting a menu frame when
/// `count`.
fn draw(a: &mut App, count: bool) {
    a.play.clear();
    with_controller(a, |c, a| {
        c.draw(a, false);
        a.single.clear();
        c.draw(a, true);
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
    fn opened(app: &mut App) -> Option<BoxedScreen> {
        controller(app).take_opened()
    }
}

impl Screen for GameScreen {
    fn tick(&mut self, app: &mut App, host: &Host) -> Tick {
        // Each tick reads the events that came with the frame before it,
        // unless a screen the controller opened just closed: it read them.
        if self.resumed {
            self.resumed = false;
        } else if self.shown {
            host.process_controller(app);
        }

        if self.next == Next::Focus {
            // SAFETY: the controller is valid.
            with_controller(app, |c, app| c.focus(app, host));
            self.next = Next::Process;
            if let Some(s) = Self::opened(app) {
                return Tick::push(s);
            }
        }

        if self.next == Next::Process {
            // SAFETY: the controller is valid.
            self.next = if with_controller(app, |c, app| c.process(app, host)) {
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
            controller(app).unfocus();
            app.clear_keys();
            return Tick::close(0);
        }

        draw(app, true);
        self.next = Next::Process;
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _app: &mut App, _host: &Host, _result: i32) {
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
    fn tick(&mut self, app: &mut App, host: &Host) -> Tick {
        if self.open == Open::Nothing {
            restart(app, host);
        } else if self.open == Open::InMenu {
            match after_menu(app, host, self.selection) {
                AfterMenu::Quit => {
                    app.controller = None;
                    return Tick::close(0);
                }
                AfterMenu::Play => {
                    self.open = Open::InGame;
                    return Tick::push(Box::new(GameScreen {
                        next: Next::Focus,
                        shown: false,
                        resumed: false,
                    }));
                }
                AfterMenu::Restart => {}
            }
        }

        // The level, or the game left off, behind the menu.
        draw(app, false);
        self.open = Open::InMenu;
        Tick::push(menu_screen(app.shared_menus()))
    }

    fn resume(&mut self, _app: &mut App, _host: &Host, result: i32) {
        self.selection = result;
    }
}

/// The app's first screen.
pub fn main_screen() -> BoxedScreen {
    Box::new(MainScreen {
        open: Open::Nothing,
        selection: -1,
    })
}

#[cfg(test)]
#[path = "app_screens_tests.rs"]
mod tests;
