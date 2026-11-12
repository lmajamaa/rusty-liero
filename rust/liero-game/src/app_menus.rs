//! How the app sets up and draws the main menu (`Gfx::menuStart`,
//! `drawBasicMenu`, `drawSpectatorInfo` and `menuFade` in C++): the frozen
//! game behind the menus, who plays whom on the spectator screen, and the
//! menus' palettes fading in and cycling.

use crate::app::{app, LieroApp};
use crate::controllers::{liero_controller_level, liero_controller_running};
use crate::gfx::{liero_gfx_fill_rect, liero_palette_fade, liero_palette_rotate_from, liero_palette_set_worm_colour};
use crate::text::LieroFont;
use crate::level::liero_level_draw_miniature;
use crate::menu::{
    liero_menu_box_count, liero_menu_box_id, liero_menu_box_move_to, liero_menu_box_set_string,
    liero_menu_box_set_visibility, liero_menu_draw_versus, LieroMenuBox,
};
use crate::menu_screen::{LIERO_MENU_MAIN, LIERO_MENU_SETTINGS};
use crate::menus::{liero_menus_box, liero_menus_cur, liero_menus_draw, liero_menus_set_cur, liero_menus_update_items};
use crate::renderer::LieroRenderer;
use crate::tc::{s as text, LieroTcData};
use crate::text::LieroStr;
use crate::weapsel::{level_line, tc_font};

/// The main menu's items `menu_start` changes (`MainMenu`).
const MAIN_RESUME_GAME: i32 = 0;
const MAIN_NEW_GAME: i32 = 1;

impl LieroApp {
    /// The menus' TC's data.
    pub(crate) fn tc_data(&self) -> &'static LieroTcData {
        // SAFETY: C++ keeps the TC, which has its data, while it runs.
        unsafe { &*crate::tc::liero_tc_data(&*self.menu_tc()) }
    }

    pub(crate) fn font(&self) -> LieroFont {
        tc_font(self.tc_data())
    }

    fn menus(&self) -> &'static mut crate::menus::LieroMenus {
        // SAFETY: C++ keeps the menus while it runs.
        unsafe { self.menus.as_mut().expect("no menus") }
    }

    // Clippy's suggestion, passing the function, doesn't compile: an
    // `extern "C"` fn isn't a Rust closure over the reference.
    #[allow(clippy::redundant_closure)]
    fn running(&mut self) -> bool {
        self.controller.as_deref().is_some_and(|c| liero_controller_running(c))
    }
}

/// The index of the item with `id`, or -1.
fn index_of(b: &LieroMenuBox, id: i32) -> i32 {
    (0..liero_menu_box_count(b)).find(|&i| liero_menu_box_id(b, i) == id).unwrap_or(-1)
}

fn set_string(b: &mut LieroMenuBox, id: i32, s: &str) {
    // SAFETY: the text outlives the call, which copies it.
    unsafe { liero_menu_box_set_string(b, index_of(b, id), LieroStr::of(s.as_bytes())) };
}

/// Sets the menus up for the main menu screen (`Gfx::menuStart`): the
/// copyright line, resume or new game first, the frozen screens with the
/// level's miniature on the spectator one. Returns the item F1 picks.
pub(crate) fn menu_start() -> i32 {
    let a = app();
    let tc = a.tc_data();
    let font = a.font();
    let center_x = a.single.data.render_res_x / 2;
    let scr = a.play.bmp.view();
    // SAFETY: the bitmaps are the app's.
    unsafe {
        liero_gfx_fill_rect(&scr, 0, 151, 160, 7, 0);
        font.text(&scr, tc.text(text("Copyright2")), 2, 152, 19, 1);
    }

    let menus = a.menus();
    let main = unsafe { &mut *liero_menus_box(menus, LIERO_MENU_MAIN) };
    let start = if a.running() {
        liero_menu_box_set_visibility(main, index_of(main, MAIN_RESUME_GAME), true);
        set_string(main, MAIN_RESUME_GAME, "RESUME GAME (F1)");
        set_string(main, MAIN_NEW_GAME, "NEW GAME");
        MAIN_RESUME_GAME
    } else {
        liero_menu_box_set_visibility(main, index_of(main, MAIN_RESUME_GAME), false);
        set_string(main, MAIN_NEW_GAME, "NEW GAME (F1)");
        MAIN_NEW_GAME
    };

    liero_menu_box_move_to(main, 0);
    liero_menu_box_move_to(unsafe { &mut *liero_menus_box(menus, LIERO_MENU_SETTINGS) }, 0);
    liero_menus_update_items(menus, LIERO_MENU_SETTINGS);

    a.play.data.fade_value = 0;
    a.single.data.fade_value = 0;
    liero_menus_set_cur(menus, LIERO_MENU_MAIN);

    a.frozen.copy_from(&a.play.bmp);
    a.single.clear();
    if let Some(c) = a.controller.as_deref_mut() {
        // SAFETY: the controller's level and the app's bitmap are valid.
        if let Some(level) = unsafe { liero_controller_level(c).as_ref() } {
            let s = a.single.bmp.view();
            let l = level.data().level;
            unsafe { liero_level_draw_miniature(&l, s.pixels, s.pitch, center_x - 126, a.single.data.render_res_y - 208, 2) };
        }
    }
    a.frozen_spectator.copy_from(&a.single.bmp);

    a.menu_cycles = 0;
    start
}

/// The frozen screen with the main menu over it, greyed out while another
/// menu is up (`Gfx::drawBasicMenu`).
pub(crate) fn draw_basic_menu() {
    let a = app();
    a.play.bmp.copy_from(&a.frozen);
    let menus = a.menus();
    let disabled = liero_menus_cur(menus) != LIERO_MENU_MAIN;
    // SAFETY: the bitmap and font are valid.
    unsafe { liero_menus_draw(menus, LIERO_MENU_MAIN, &a.play.bmp.view(), &a.font(), disabled, -1, true) };
}

/// Who plays whom, the level and whether a game is paused, on the frozen
/// spectator screen (`Gfx::drawSpectatorInfo`).
pub(crate) fn draw_spectator_info() {
    let a = app();
    let (cx, cy) = (a.single.data.render_res_x / 2, a.single.data.render_res_y / 4);
    a.single.bmp.copy_from(&a.frozen_spectator);
    let s = a.menu_settings();
    let line = level_line(a.tc_data(), &s.level_file, &s.level_file);
    let scr = a.single.bmp.view();
    let font = a.font();
    let w = &s.worms;
    // SAFETY: the bitmap, font and texts are valid.
    unsafe {
        liero_menu_draw_versus(&scr, &font, LieroStr::of(&line), LieroStr::of(&w[0].name), LieroStr::of(&w[1].name),
            w[0].data.color, w[1].data.color, cx, cy);
        let state: &[u8] = if a.running() { b"PAUSED" } else { b"SETUP" };
        font.centered(&scr, state, cx, cy + 48, 7, 2);
    }
}

/// The main menu screen's frame: the frozen screens with the menus over
/// them, the settings menu beside the main menu while it's current.
pub(crate) fn draw_menus() {
    draw_basic_menu();
    draw_spectator_info();
    let a = app();
    let menus = a.menus();
    let cur = liero_menus_cur(menus);
    let (scr, font) = (a.play.bmp.view(), a.font());
    // SAFETY: the bitmap and font are valid.
    unsafe {
        if cur == LIERO_MENU_MAIN {
            liero_menus_draw(menus, LIERO_MENU_SETTINGS, &scr, &font, true, -1, false);
        } else {
            liero_menus_draw(menus, cur, &scr, &font, false, -1, false);
        }
    }
}

/// The menus' palette for the next frame (`Gfx::menuFade`): fading in
/// unless `quitting`, the colours cycling, and the worms' colours.
pub(crate) fn menu_fade(quitting: bool) {
    let a = app();
    let s = a.menu_settings();
    a.menu_cycles = a.menu_cycles.wrapping_add(1);
    for r in [&mut *a.play, &mut *a.single] {
        if r.data.fade_value < 32 && !quitting {
            r.data.fade_value += 1;
        }
    }
    for r in [&mut *a.play, &mut *a.single] {
        palette(r, a.menu_cycles, s);
    }
}

fn palette(r: &mut LieroRenderer, cycles: u32, s: &crate::settings::LieroSettingsBox) {
    let d = &mut r.data;
    d.pal = d.origpal;
    // SAFETY: the palettes hold 256 colours.
    unsafe {
        liero_palette_rotate_from(d.pal.as_mut_ptr(), d.origpal.as_ptr(), 168, 174, cycles);
        for (i, w) in s.worms.iter().enumerate() {
            liero_palette_set_worm_colour(d.pal.as_mut_ptr(), i as i32, w.data.color, &w.data.rgb);
        }
        liero_palette_fade(d.pal.as_mut_ptr(), d.fade_value);
    }
}

/// Both screens black before the menu's first frame.
pub(crate) fn clear_palettes() {
    let a = app();
    for r in [&mut *a.play, &mut *a.single] {
        r.data.pal = [Default::default(); 256];
    }
}

#[cfg(test)]
#[path = "app_menus_tests.rs"]
mod tests;
