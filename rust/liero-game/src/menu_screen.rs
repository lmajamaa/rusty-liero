//! The main menu screen: the main menu with the match setup, player or
//! options menu beside it, its keys, and the fade-out when an item is
//! chosen (`MenuScreen` and `Gfx::menuStep` in C++). It closes with the item
//! chosen.
//!
//! The menus and their items are `LieroMenus` (`menus.rs`): an item may
//! open a screen, and the screen goes on where it stopped when that one
//! closes. The app sets the menus up and draws them (`app_menus.rs`);
//! saving the settings is the app's (`app_files.rs`), and F8's random TC
//! is seeded by the platform.

use crate::menus::LieroMenus;
use crate::screen::{BoxedScreen, Host, Screen, Tick};

const SCANCODE_RETURN: u32 = 40;
const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_F1: u32 = 58;
const SCANCODE_F2: u32 = 59;
const SCANCODE_F3: u32 = 60;
const SCANCODE_F5: u32 = 62;
const SCANCODE_F6: u32 = 63;
const SCANCODE_F7: u32 = 64;
const SCANCODE_F8: u32 = 65;
const SCANCODE_PAGEUP: u32 = 75;
const SCANCODE_PAGEDOWN: u32 = 78;
const SCANCODE_RIGHT: u32 = 79;
const SCANCODE_LEFT: u32 = 80;
const SCANCODE_DOWN: u32 = 81;
const SCANCODE_UP: u32 = 82;
const SCANCODE_KP_ENTER: u32 = 88;

const SOUND_UP: i32 = 26;
const SOUND_DOWN: i32 = 25;
const SOUND_CHOICE: i32 = 27;

/// The menus.
pub const LIERO_MENU_MAIN: i32 = 0;
pub const LIERO_MENU_SETTINGS: i32 = 1;
pub const LIERO_MENU_PLAYER: i32 = 2;
pub const LIERO_MENU_HIDDEN: i32 = 3;

/// The main menu's item ids (`MainMenu`).
pub const LIERO_MAIN_SETTINGS: i32 = 2;
pub const LIERO_MAIN_PLAYER1_SETTINGS: i32 = 3;
pub const LIERO_MAIN_PLAYER2_SETTINGS: i32 = 4;
pub const LIERO_MAIN_ADVANCED: i32 = 5;
pub const LIERO_MAIN_QUIT: i32 = 6;
pub const LIERO_MAIN_REPLAYS: i32 = 7;
pub const LIERO_MAIN_TC: i32 = 9;

/// Where a pass stopped for a screen an item opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Top,
    AfterEnter,
    AfterF3,
}

/// What the item's result means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Use {
    Ignore,
    Select,
    Tc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Setup,
    Menu,
    FadeOut,
}

/// How a pass ended.
enum Step {
    Done,
    Opened(BoxedScreen),
    Close,
}

struct MenuScreen {
    /// The app's menus, which outlive the screen.
    menus: *mut LieroMenus,
    phase: Phase,
    start_item: i32,
    selected: i32,
    part: Part,
    how: Use,
    entered: i32,
    resumed: bool,
}

impl MenuScreen {
    // The `&mut` is to the menus behind a raw pointer, not to `self`; phase 4
    // of PLAN.md replaces these pointers.
    #[allow(clippy::mut_from_ref)]
    fn menus(&self) -> &mut LieroMenus {
        // SAFETY: C++'s menus outlive the screen, and only it uses them
        // while it runs.
        unsafe { &mut *self.menus }
    }

    fn menu(&self, kind: i32) -> &mut crate::menu::LieroMenuBox {
        self.menus().box_of(kind)
    }

    fn cur(&self) -> i32 {
        self.menus().cur()
    }

    fn set_cur(&self, kind: i32) {
        self.menus().set_cur(kind)
    }

    fn move_main_to(&self, id: i32) {
        let main = self.menu(LIERO_MENU_MAIN);
        let i = (0..main.0.len())
            .find(|&i| main.0.ids[i] == id)
            .map_or(-1, |i| i as i32);
        main.0.move_to(i);
    }

    fn main_selected_id(&self) -> i32 {
        let main = self.menu(LIERO_MENU_MAIN);
        main.0.selected().map_or(-1, |i| main.0.ids[i])
    }

    /// What an item's `onEnter` returned, put to `how`. True if the menu
    /// closes at once (a new TC).
    fn take_result(&mut self, how: Use, r: i32) -> bool {
        match how {
            Use::Select => self.selected = r,
            Use::Tc if r == LIERO_MAIN_TC => {
                self.selected = LIERO_MAIN_TC;
                return true;
            }
            _ => {}
        }
        false
    }

    /// Enter on `kind`'s item. Returns the screen it opened, if any; else
    /// whether the menu closes at once.
    fn enter(&mut self, kind: i32, how: Use) -> Result<bool, BoxedScreen> {
        let (r, opened) = self.menus().enter(kind);
        if let Some(opened) = opened {
            self.how = how;
            return Err(opened);
        }
        Ok(self.take_result(how, r))
    }

    /// One pass of the menu's loop (`Gfx::menuStep`).
    fn step(&mut self, host: &Host) -> Step {
        if self.part != Part::Top && self.take_result(self.how, self.entered) {
            return Step::Close;
        }

        // Enter or F3 on an item: stop at `part` if it opened a screen.
        macro_rules! enter {
            ($kind:expr, $how:expr, $part:expr) => {
                match self.enter($kind, $how) {
                    Err(screen) => {
                        self.part = $part;
                        return Step::Opened(screen);
                    }
                    Ok(close) => close,
                }
            };
        }

        if self.part == Part::Top {
            let a = crate::app::app();
            if a.save_settings_on_change {
                a.save_settings_if_changed();
            }
            crate::app_menus::draw_menus();

            if host.test_key_once(SCANCODE_ESCAPE) {
                if self.cur() == LIERO_MENU_MAIN {
                    self.move_main_to(LIERO_MAIN_QUIT);
                } else {
                    self.set_cur(LIERO_MENU_MAIN);
                }
            }
            if host.test_key_once(SCANCODE_UP) {
                host.play_sound(SOUND_UP);
                self.menu(self.cur()).0.movement(-1);
            }
            if host.test_key_once(SCANCODE_DOWN) {
                host.play_sound(SOUND_DOWN);
                self.menu(self.cur()).0.movement(1);
            }

            if host.test_key_once(SCANCODE_RETURN) || host.test_key_once(SCANCODE_KP_ENTER) {
                let cur = self.cur();
                if cur == LIERO_MENU_MAIN {
                    host.play_sound(SOUND_CHOICE);
                    let s = self.main_selected_id();
                    match s {
                        LIERO_MAIN_SETTINGS => self.set_cur(LIERO_MENU_SETTINGS),
                        LIERO_MAIN_PLAYER1_SETTINGS | LIERO_MAIN_PLAYER2_SETTINGS => {
                            self.menus().player_settings(s - LIERO_MAIN_PLAYER1_SETTINGS)
                        }
                        LIERO_MAIN_ADVANCED => self.menus().open_hidden(),
                        LIERO_MAIN_REPLAYS => {
                            enter!(cur, Use::Select, Part::AfterEnter);
                        }
                        LIERO_MAIN_TC => {
                            if enter!(cur, Use::Tc, Part::AfterEnter) {
                                return Step::Close;
                            }
                        }
                        _ => {
                            self.set_cur(LIERO_MENU_MAIN);
                            self.selected = s;
                        }
                    }
                } else if cur == LIERO_MENU_SETTINGS {
                    enter!(cur, Use::Ignore, Part::AfterEnter);
                } else {
                    enter!(cur, Use::Select, Part::AfterEnter);
                }
            }
        }

        if self.part != Part::AfterF3 {
            if host.test_key_once(SCANCODE_F1) {
                self.set_cur(LIERO_MENU_MAIN);
                self.move_main_to(self.start_item);
                self.selected = self.start_item;
            }
            if host.test_key_once(SCANCODE_F2) {
                self.move_main_to(LIERO_MAIN_ADVANCED);
                self.menus().open_hidden();
            }
            if host.test_key_once(SCANCODE_F3) {
                self.set_cur(LIERO_MENU_MAIN);
                self.move_main_to(LIERO_MAIN_REPLAYS);
                enter!(LIERO_MENU_MAIN, Use::Select, Part::AfterF3);
            }
        }

        if host.test_key_once(SCANCODE_F5) {
            self.move_main_to(LIERO_MAIN_PLAYER1_SETTINGS);
            self.menus().player_settings(0);
        }
        if host.test_key_once(SCANCODE_F6) {
            self.move_main_to(LIERO_MAIN_PLAYER2_SETTINGS);
            self.menus().player_settings(1);
        }
        if host.test_key_once(SCANCODE_F7) {
            self.move_main_to(LIERO_MAIN_SETTINGS);
            self.set_cur(LIERO_MENU_SETTINGS);
        }
        if host.test_key_once(SCANCODE_F8) {
            // F8: the developers' random TC.
            let a = crate::app::app();
            if a.has_tc() {
                crate::tc::liero_tc_randomize(&a.menu_tc(), host.random_seed());
            }
        }

        for (key, dir) in [(SCANCODE_LEFT, -1), (SCANCODE_RIGHT, 1)] {
            if host.test_key(key) && !self.menus().left_right(self.cur(), dir) {
                // Let go of them, so they act once (`resetLeftRight`).
                let keys = &mut crate::app::app().keys;
                keys.release_sdl(SCANCODE_LEFT as i32);
                keys.release_sdl(SCANCODE_RIGHT as i32);
            }
        }

        if host.test_key_once(SCANCODE_PAGEUP) {
            host.play_sound(SOUND_UP);
            self.menu(self.cur()).0.movement_page(-1);
        }
        if host.test_key_once(SCANCODE_PAGEDOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu(self.cur()).0.movement_page(1);
        }

        self.part = Part::Top;
        Step::Done
    }

    fn run_step(&mut self, host: &Host) -> Tick {
        match self.step(host) {
            Step::Close => Tick::close(self.selected),
            Step::Opened(screen) => Tick::push(screen),
            Step::Done => {
                host.menu_fade();
                Tick::stay(true)
            }
        }
    }
}

impl Screen for MenuScreen {
    fn tick(&mut self, host: &Host) -> Tick {
        // The pass an item stopped goes on; the screen that closed read the
        // events.
        if self.resumed {
            self.resumed = false;
            return self.run_step(host);
        }

        // Each tick reads the events that came with the frame before it.
        if self.phase != Phase::Start {
            host.process();
        }

        match self.phase {
            Phase::Start => {
                crate::app_menus::clear_palettes();
                self.phase = Phase::Setup;
                Tick::stay(true)
            }
            Phase::Setup => {
                self.start_item = crate::app_menus::menu_start();
                self.selected = -1;
                self.phase = Phase::Menu;
                self.run_step(host)
            }
            Phase::Menu => {
                if self.selected < 0 {
                    return self.run_step(host);
                }
                self.phase = Phase::FadeOut;
                crate::app::app().play.data.fade_value = 32;
                crate::app_menus::menu_fade(true);
                Tick::stay(true)
            }
            Phase::FadeOut => {
                let v = &mut crate::app::app().play.data.fade_value;
                *v -= 1;
                if *v > 0 {
                    crate::app_menus::menu_fade(true);
                    return Tick::stay(true);
                }
                Tick::close(self.selected)
            }
        }
    }

    fn resume(&mut self, _host: &Host, result: i32) {
        self.entered = self.menus().then(result);
        self.resumed = true;
    }
}

/// The main menu screen.
pub fn liero_menu_screen(menus: *mut LieroMenus) -> BoxedScreen {
    Box::new(MenuScreen {
        menus,
        phase: Phase::Start,
        start_item: 0,
        selected: -1,
        part: Part::Top,
        how: Use::Ignore,
        entered: -1,
        resumed: false,
    })
}
