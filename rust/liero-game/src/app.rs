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
use crate::rand::{liero_rand_seed, LieroRand, LIERO_RAND_DEFAULT_SEED};
use crate::renderer::{LieroBitmapBox, LieroRenderer};
use crate::settings::LieroSettingsBox;
use crate::shared::Shared;
use crate::tc::LieroTc;

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
    /// The menus' settings (`gfx.settings`), which can be another game's
    /// than the one played. Loading options replaces them.
    pub settings: Option<Shared<LieroSettingsBox>>,
    /// The TC the menus and new games use (`gfx.common`).
    pub tc: Option<Shared<LieroTc>>,
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
    /// The menus' settings.
    pub fn menu_settings(&self) -> &'static mut LieroSettingsBox {
        // SAFETY: the app's reference keeps the box, and the game runs on
        // one thread.
        unsafe { &mut *self.settings.as_ref().expect("no menu settings").as_ptr() }
    }
}

impl LieroApp {
    /// The menus' TC.
    pub fn menu_tc(&self) -> *mut LieroTc {
        self.tc.as_ref().expect("no TC").as_ptr()
    }
}

impl LieroApp {
    fn new() -> Self {
        let mut rand = LieroRand { x: 0, c: 0 };
        liero_rand_seed(&mut rand, LIERO_RAND_DEFAULT_SEED);
        LieroApp {
            rand,
            play: LieroRenderer::new(),
            single: LieroRenderer::new(),
            frozen: LieroBitmapBox::new(),
            frozen_spectator: LieroBitmapBox::new(),
            keys: LieroKeys::default(),
            menu_cycles: 0,
            settings: None,
            tc: None,
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
#[no_mangle]
pub extern "C" fn liero_app() -> *mut LieroApp {
    app()
}

#[no_mangle]
pub extern "C" fn liero_app_rand(a: &mut LieroApp) -> *mut LieroRand {
    &mut a.rand
}

/// The play renderer, or with `single`, the single screen one.
#[no_mangle]
pub extern "C" fn liero_app_renderer(a: &mut LieroApp, single: bool) -> *mut LieroRenderer {
    if single {
        &mut *a.single
    } else {
        &mut *a.play
    }
}

/// The frozen play screen, or with `spectator`, the spectator one.
#[no_mangle]
pub extern "C" fn liero_app_frozen(a: &mut LieroApp, spectator: bool) -> *mut LieroBitmapBox {
    if spectator {
        &mut *a.frozen_spectator
    } else {
        &mut *a.frozen
    }
}

#[no_mangle]
pub extern "C" fn liero_app_keys(a: &mut LieroApp) -> *mut LieroKeys {
    &mut a.keys
}

/// The menus' settings, or null.
#[no_mangle]
pub extern "C" fn liero_app_settings(a: &mut LieroApp) -> *mut LieroSettingsBox {
    a.settings.as_ref().map_or(std::ptr::null_mut(), Shared::as_ptr)
}

/// Makes `s` (or none, if null) the menus' settings, taking a reference to
/// it.
///
/// # Safety
/// `s` is a valid box, or null.
#[no_mangle]
pub unsafe extern "C" fn liero_app_set_settings(a: &mut LieroApp, s: *mut LieroSettingsBox) {
    unsafe {
        a.settings = (!s.is_null()).then(|| Shared::retain(s));
    }
}

/// Shows the single screen renderer, or the play one.
#[no_mangle]
pub extern "C" fn liero_app_set_primary_single(a: &mut LieroApp, single: bool) {
    a.primary_single = single;
}

/// The renderer the screen shows.
#[no_mangle]
pub extern "C" fn liero_app_primary(a: &mut LieroApp) -> *mut LieroRenderer {
    liero_app_renderer(a, a.primary_single)
}

/// The menus' TC, or null.
#[no_mangle]
pub extern "C" fn liero_app_tc(a: &mut LieroApp) -> *mut LieroTc {
    a.tc.as_ref().map_or(std::ptr::null_mut(), Shared::as_ptr)
}

/// Makes `tc` (or none, if null) the menus' TC, taking a reference to it.
///
/// # Safety
/// `tc` comes from `liero_tc_load` and has a reference, or is null.
#[no_mangle]
pub unsafe extern "C" fn liero_app_set_tc(a: &mut LieroApp, tc: *mut LieroTc) {
    unsafe {
        a.tc = (!tc.is_null()).then(|| Shared::retain(tc));
    }
}

/// The menus the app draws.
#[no_mangle]
pub extern "C" fn liero_app_set_menus(a: &mut LieroApp, menus: *mut crate::menus::LieroMenus) {
    a.menus = menus;
}

/// The controller, or null.
#[no_mangle]
pub extern "C" fn liero_app_controller(a: &mut LieroApp) -> *mut LieroController {
    a.controller
        .as_deref_mut()
        .map_or(std::ptr::null_mut(), |c| c as *mut LieroController)
}

/// Replaces the controller with `c` (from a `_new`, or null), dropping the
/// old one first.
///
/// # Safety
/// `c` isn't used by C++ after.
#[no_mangle]
pub unsafe extern "C" fn liero_app_set_controller(a: &mut LieroApp, c: *mut LieroController) {
    unsafe {
        a.controller = None;
        a.controller = (!c.is_null()).then(|| Box::from_raw(c));
    }
}

#[no_mangle]
pub extern "C" fn liero_app_menu_cycles(a: &mut LieroApp) -> *mut u32 {
    &mut a.menu_cycles
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
