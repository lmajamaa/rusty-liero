//! What the app keeps besides the game (C++'s `Gfx`): the two renderers,
//! the screens frozen behind the menus, the random numbers for things that
//! don't affect the game, the keys, the menus' frame count, the menus'
//! settings and TC, and the files (`app_files.rs`). The entry point (the
//! web shell, or the hash run) owns it and lends it to the screens.

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
    font: Vec<liero_data::model::FontChar>,
    /// The folders, the settings file and the TC chosen (`app_files.rs`).
    pub files: crate::app_files::Files,
    /// The menus, which the menu screen shares.
    pub menus: Option<crate::menus::SharedMenus>,
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

    /// The menus' TC's data. Panics if there's no TC.
    pub(crate) fn tc_data(&self) -> Ref<'_, LieroTcData> {
        tc_data(self.tc.as_ref().expect("no TC"))
    }

    /// The menus' settings. Panics if there are none, or if they're borrowed.
    pub fn settings_mut(&self) -> RefMut<'_, LieroSettingsBox> {
        self.settings.as_ref().expect("no menu settings").borrow_mut()
    }

    /// The menus' TC's font.
    pub(crate) fn font(&self) -> crate::text::LieroFont<'_> {
        crate::text::LieroFont { chars: &self.font }
    }

    /// The menus. Panics if there are none yet.
    pub(crate) fn shared_menus(&self) -> crate::menus::SharedMenus {
        self.menus.clone().expect("no menus")
    }

    /// Makes `tc` (or none) the menus' TC.
    pub fn set_tc(&mut self, tc: Option<SharedTc>) {
        self.font = tc.as_ref().map_or(Vec::new(), |t| tc_data(t).model.font.clone());
        self.tc = tc;
    }
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
            menus: None,
            primary_single: false,
            save_settings_on_change: false,
        }
    }
}

impl LieroApp {
    /// Whether an SDL key was pressed since it was last asked about, which
    /// it then forgets (`Gfx::testSDLKeyOnce`).
    pub fn test_key_once(&mut self, scancode: u32) -> bool {
        self.keys.test_sdl_once(scancode as i32)
    }
    /// Whether an SDL key is held (`Gfx::testSDLKey`).
    pub fn test_key(&self, scancode: u32) -> bool {
        self.keys.test_sdl(scancode as i32)
    }
    pub fn clear_keys(&mut self) {
        self.keys.clear();
    }
    /// Holds down SDL keys, as their events would.
    #[cfg(test)]
    pub fn press(&mut self, scancodes: &[u32]) {
        for &sc in scancodes {
            self.keys.set(crate::keys::sdl_to_dos(sc as i32), true);
        }
    }
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
pub fn liero_app_set_menus(a: &mut LieroApp, menus: crate::menus::SharedMenus) {
    a.menus = Some(menus);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_renderers_are_the_apps() {
        let mut a = LieroApp::new();
        assert!(std::ptr::eq(liero_app_renderer(&mut a, true), &*a.single));
        assert!(std::ptr::eq(liero_app_frozen(&mut a, false), &*a.frozen));
    }
}
