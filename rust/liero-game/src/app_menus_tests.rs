use super::*;
use crate::controllers::{liero_controller_data, liero_local_controller_new};
use crate::controller::LIERO_STATE_GAME;
use crate::fs::{LieroFsNode, Node};
use crate::menus::{liero_menus_new, LieroMenusHooks};
use crate::settings::{liero_settings_box_new, LieroSettingsBox, LieroWormSettingsBox};
use crate::screen::LieroScreen;
use crate::tc::LieroTc;

thread_local! {
    static SETTINGS: *mut LieroSettingsBox = liero_settings_box_new();
    static TC: *mut LieroTc = crate::tc::liero_tc_load(&LieroFsNode(Node::new(b"../../TC/openliero")));
}

unsafe extern "C" fn settings() -> *mut LieroSettingsBox {
    SETTINGS.with(|s| *s)
}
unsafe extern "C" fn tc() -> *mut LieroTc {
    TC.with(|t| *t)
}
unsafe extern "C" fn worm_settings(player: i32) -> *mut LieroWormSettingsBox {
    (*settings()).worms[player as usize].as_ptr()
}
unsafe extern "C" fn tc_data() -> *const LieroTcData {
    crate::tc::liero_tc_data(&*tc())
}
unsafe extern "C" fn zero() -> u32 {
    0
}
unsafe extern "C" fn double_res() -> *mut bool {
    thread_local! {
        static ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    ON.with(|b| b.as_ptr())
}
unsafe extern "C" fn set(_: bool) {}
unsafe extern "C" fn selector(_: i32, _: i32) -> LieroScreen {
    LieroScreen::empty()
}
unsafe extern "C" fn no() -> bool {
    false
}
unsafe extern "C" fn path() -> LieroStr {
    LieroStr::of(b"")
}
unsafe extern "C" fn save(_: LieroStr) -> bool {
    false
}
unsafe extern "C" fn save_profile(_: i32, _: LieroStr) -> bool {
    false
}

/// This thread's app with menus, the bundled TC, its screens sized, and a
/// match that hasn't started, on a level of colour 7.
fn set_up() -> &'static mut LieroApp {
    let a = app();
    crate::app::liero_app_set_menu_settings(a, settings);
    crate::app::liero_app_set_menu_tc(a, tc);
    if a.menus.is_null() {
        let hooks = LieroMenusHooks {
            settings,
            worm_settings,
            tc: tc_data,
            menu_cycles: zero,
            double_res,
            set_double_res: set,
            set_fullscreen: set,
            open_selector: selector,
            take_chosen_tc: no,
            settings_path: path,
            save_options: save,
            save_profile_as: save_profile,
        };
        let menus = liero_menus_new(hooks);
        crate::menus::liero_menus_load(unsafe { &mut *menus });
        crate::app::liero_app_set_menus(a, menus);
    }
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    let mut c = unsafe { Box::from_raw(liero_local_controller_new(settings(), tc())) };
    let level = unsafe { &mut *liero_controller_level(&mut c) };
    level.resize(504, 350);
    level.set_pixels(&vec![7; 504 * 350], |_| Default::default());
    a.controller = Some(c);
    a
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
fn the_menu_starts_black_with_the_levels_miniature_behind_it() {
    let a = set_up();
    a.play.data.fade_value = 20;
    a.single.data.fade_value = 20;
    a.menu_cycles = 9;
    menu_start();
    assert_eq!((a.play.data.fade_value, a.single.data.fade_value, a.menu_cycles), (0, 0, 0));

    // The miniature at half size, 126 left of the middle and 208 up from
    // the bottom.
    let s = a.frozen_spectator.view();
    let px = |x: i32, y: i32| a.frozen_spectator.pixels()[(y * s.pitch + x) as usize];
    let (x0, y0) = (320 - 126, 400 - 208);
    assert_eq!((px(x0, y0), px(x0 + 251, y0 + 174)), (7, 7));
    assert_eq!((px(x0 - 1, y0), px(x0, y0 - 1), px(x0 + 252, y0), px(x0, y0 + 175)), (0, 0, 0, 0));
    a.controller = None;
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
fn the_spectator_screen_says_whether_a_game_is_paused() {
    let a = set_up();
    a.frozen_spectator.copy_from(&a.single.bmp);
    let font = a.font();
    let (cx, cy) = (320, 100);
    // Drawing the same words again changes nothing; other words do.
    let shows = |a: &mut LieroApp, words: &[u8]| {
        let before = a.single.bmp.pixels().to_vec();
        unsafe { font.centered(&a.single.bmp.view(), words, cx, cy + 48, 7, 2) };
        a.single.bmp.pixels() == before
    };

    draw_spectator_info();
    assert!(shows(a, b"SETUP") && !shows(a, b"PAUSED"));

    unsafe { (*liero_controller_data(a.controller.as_deref_mut().unwrap())).state = LIERO_STATE_GAME };
    draw_spectator_info();
    assert!(shows(a, b"PAUSED") && !shows(a, b"SETUP"));
    a.controller = None;
}
