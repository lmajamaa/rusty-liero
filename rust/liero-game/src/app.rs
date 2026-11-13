//! What the app keeps besides the game (C++'s `Gfx`): the two renderers,
//! the screens frozen behind the menus, the random numbers for things that
//! don't affect the game, the keys, the menus' frame count, the menus'
//! settings and TC, and the files (`app_files.rs`). Rust owns it, and
//! `Gfx`'s fields refer to it.
//!
//! There is one app per thread, made when first asked for. The game runs on
//! one thread; tests running on others each get their own.

use std::cell::Cell;

use crate::controllers::LieroController;
use crate::keys::LieroKeys;
use crate::rand::{LieroRand, LIERO_RAND_DEFAULT_SEED};
use crate::renderer::{LieroBitmapBox, LieroRenderer};
use crate::settings::{LieroSettingsBox, SharedSettings};
use crate::tc::{tc_data, LieroTcData, SharedTc};
use std::cell::{Ref, RefMut};

pub struct LieroApp {
    /// For things that don't affect the game: the menus, weapon selection
    /// and making levels (`gfx.rand`).
    pub rand: LieroRand,
    /// Renders everything for actual play.
    pub play: Box<LieroRenderer>,
    /// Renders everything on a single screen, for single screen replay and
    /// the spectator window.
    pub single: Box<LieroRenderer>,
    /// The game on the play screen, frozen behind the menu.
    pub frozen: Box<LieroBitmapBox>,
    /// The same on the spectator screen.
    pub frozen_spectator: Box<LieroBitmapBox>,
    /// The DOS keys down (`gfx.dosKeys`).
    pub keys: LieroKeys,
    /// Frames the menus have shown, which their colours cycle with.
    pub menu_cycles: u32,
    /// The window's double size (`Gfx::doubleRes`), on to start with.
    pub double_res: bool,
    /// The menus' settings (`gfx.settings`), which can be another game's
    /// than the one played. Loading options replaces them.
    pub settings: Option<SharedSettings>,
    /// The TC the menus and new games use (`gfx.common`), set with
    /// `set_tc`.
    tc: Option<SharedTc>,
    /// The TC's font, which F8 doesn't change, for drawing with while
    /// nothing borrows the TC.
    font: Vec<crate::gfx::LieroFontChar>,
    /// The folders, the settings file and the TC chosen (`app_files.rs`).
    pub files: crate::app_files::Files,
    /// The menus, which C++ makes.
    pub menus: *mut crate::menus::LieroMenus,
    /// The match or replay being played, or behind the menu.
    pub controller: Option<Box<LieroController>>,
    /// Whether the screen shows the single screen renderer, for a replay on
    /// one screen (`gfx.primaryRenderer`), rather than the play one.
    pub primary_single: bool,
    /// Whether the menu writes liero.cfg as soon as the settings change, as
    /// in the browser, where a closed tab never reaches the save at the end.
    pub save_settings_on_change: bool,
}

impl LieroApp {
    /// The menus' settings, to share with a game.
    pub fn shared_settings(&self) -> SharedSettings {
        self.settings.clone().expect("no menu settings")
    }
}

impl LieroApp {
    /// The menus' TC.
    pub fn menu_tc(&self) -> SharedTc {
        self.tc.clone().expect("no TC")
    }

    /// Whether the menus have a TC yet.
    pub fn has_tc(&self) -> bool {
        self.tc.is_some()
    }

    /// Makes `tc` (or none) the menus' TC.
    pub fn set_tc(&mut self, tc: Option<SharedTc>) {
        self.font = tc.as_ref().map_or(Vec::new(), |t| tc_data(t).font.to_vec());
        self.tc = tc;
    }
}

/// The menus' TC's font.
pub(crate) fn menu_font() -> crate::text::LieroFont<'static> {
    let a: &'static LieroApp = app();
    crate::text::LieroFont { chars: &a.font }
}

/// The menus' settings. Panics if there are none, or if they're borrowed.
pub fn menu_settings() -> RefMut<'static, LieroSettingsBox> {
    let a: &'static LieroApp = app();
    a.settings.as_ref().expect("no menu settings").borrow_mut()
}

/// The menus' TC's data. Panics if there's no TC.
pub(crate) fn menu_tc_data() -> Ref<'static, LieroTcData> {
    let a: &'static LieroApp = app();
    tc_data(a.tc.as_ref().expect("no TC"))
}

impl Default for LieroApp {
    fn default() -> Self {
        Self::new()
    }
}

impl LieroApp {
    /// An app with no settings, TC, menus or game yet.
    pub fn new() -> Self {
        let mut rand = LieroRand { x: 0, c: 0 };
        rand.seed(LIERO_RAND_DEFAULT_SEED);
        LieroApp {
            rand,
            play: LieroRenderer::new(),
            single: LieroRenderer::new(),
            frozen: LieroBitmapBox::new(),
            frozen_spectator: LieroBitmapBox::new(),
            keys: LieroKeys::default(),
            menu_cycles: 0,
            double_res: true,
            settings: None,
            tc: None,
            font: Vec::new(),
            files: Default::default(),
            controller: None,
            menus: std::ptr::null_mut(),
            primary_single: false,
            save_settings_on_change: false,
        }
    }
}

thread_local! {
    static APP: Cell<*mut LieroApp> = const { Cell::new(std::ptr::null_mut()) };
}

/// This thread's app.
pub fn app() -> &'static mut LieroApp {
    APP.with(|a| {
        if a.get().is_null() {
            a.set(Box::into_raw(Box::new(LieroApp::new())));
        }
        // SAFETY: the app is never freed, and only this thread has it.
        unsafe { &mut *a.get() }
    })
}

/// The app, at a fixed address.
pub fn liero_app() -> *mut LieroApp {
    app()
}

/// The play renderer, or with `single`, the single screen one.
pub fn liero_app_renderer(a: &mut LieroApp, single: bool) -> *mut LieroRenderer {
    if single {
        &mut *a.single
    } else {
        &mut *a.play
    }
}

/// The frozen play screen, or with `spectator`, the spectator one.
pub fn liero_app_frozen(a: &mut LieroApp, spectator: bool) -> *mut LieroBitmapBox {
    if spectator {
        &mut *a.frozen_spectator
    } else {
        &mut *a.frozen
    }
}

/// Makes `s` (or none) the menus' settings.
pub fn liero_app_set_settings(a: &mut LieroApp, s: Option<SharedSettings>) {
    a.settings = s;
}

/// The renderer the screen shows.
pub fn liero_app_primary(a: &mut LieroApp) -> *mut LieroRenderer {
    liero_app_renderer(a, a.primary_single)
}

/// The menus the app draws.
pub fn liero_app_set_menus(a: &mut LieroApp, menus: *mut crate::menus::LieroMenus) {
    a.menus = menus;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_thread_has_one_app() {
        let a: *mut LieroApp = app();
        assert_eq!(a, liero_app());
        app().menu_cycles = 5;
        let other = std::thread::spawn(|| (app().menu_cycles, app() as *mut LieroApp as usize))
            .join()
            .unwrap();
        assert_eq!(other.0, 0);
        assert_ne!(other.1, a as usize);
        assert_eq!(unsafe { (*a).menu_cycles }, 5);
    }

    #[test]
    fn the_app_is_made_once() {
        let a: *mut LieroApp = app();
        app().menu_cycles += 1;
        assert_eq!(liero_app(), a);
        assert!(std::ptr::eq(liero_app_renderer(app(), true), &*app().single));
        assert!(std::ptr::eq(liero_app_frozen(app(), false), &*app().frozen));
    }
}
