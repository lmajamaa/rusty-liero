use super::*;
use crate::controllers::{liero_controller_is_replay, LieroController};
use crate::fs::{LieroFsNode, Node};
use crate::settings::{liero_settings_box_new, LieroSettingsBox};
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

/// This thread's app with the menus' settings and the bundled TC, and a
/// match behind the menu.
fn set_up() -> &'static mut LieroApp {
    let a = app();
    // SAFETY: the app takes references of its own.
    unsafe {
        crate::app::liero_app_set_settings(a, settings());
        crate::app::liero_app_set_tc(a, tc());
    }
    restart(a);
    a
}

/// A replay controller that hasn't started.
fn replay() -> Box<LieroController> {
    let dir = "target/app-screens-tests";
    std::fs::create_dir_all(dir).unwrap();
    let path = format!("{dir}/empty.lrp");
    std::fs::write(&path, b"").unwrap();
    let c = unsafe { crate::controllers::liero_replay_controller_new(&LieroFsNode(Node::new(path.as_bytes())), tc()) };
    assert!(!c.is_null());
    unsafe { Box::from_raw(c) }
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
fn resuming_a_replay_shows_the_single_screen() {
    let a = set_up();
    a.primary_single = false;
    assert_eq!(after_menu(a, MAIN_RESUME_GAME), LIERO_AFTER_MENU_PLAY);
    assert!(!a.primary_single);

    a.controller = Some(replay());
    assert!(liero_controller_is_replay(a.controller.as_deref().unwrap()));
    assert_eq!(after_menu(a, MAIN_RESUME_GAME), LIERO_AFTER_MENU_PLAY);
    assert!(a.primary_single);

    // A replay chosen in the menu, as the settings say.
    a.primary_single = false;
    a.menu_settings().data.single_screen_replay = false;
    assert_eq!(after_menu(a, MAIN_REPLAY), LIERO_AFTER_MENU_PLAY);
    assert!(!a.primary_single);
    a.menu_settings().data.single_screen_replay = true;
    assert_eq!(after_menu(a, MAIN_REPLAY), LIERO_AFTER_MENU_PLAY);
    assert!(a.primary_single);
    a.controller = None;
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
fn a_finished_game_goes_back_to_the_play_screen() {
    let a = set_up();
    a.primary_single = true;
    let hooks = LieroAppHooks { process: no_process, menu_screen: no_menu };
    let mut g = GameScreen { h: hooks, next: Next::Finish, shown: false, resumed: true };
    let t = g.tick(&Host(Default::default()));
    assert_eq!((t.kind, t.shown), (crate::screen::LieroTickKind::TickClose, false));
    assert!(!a.primary_single);
    a.controller = None;
}

unsafe extern "C" fn no_process() {}

unsafe extern "C" fn no_menu() -> LieroScreen {
    LieroScreen::empty()
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
fn a_new_game_keeps_the_level_unless_the_settings_changed() {
    let a = set_up();
    let s = a.menu_settings();
    (s.data.random_level, s.data.regenerate_level) = (true, false);
    restart(a);
    let pixels = |a: &mut LieroApp| unsafe { (*liero_controller_level(controller(a))).pixels().to_vec() };
    let first = pixels(a);
    assert_eq!(after_menu(a, MAIN_NEW_GAME), LIERO_AFTER_MENU_PLAY);
    assert_eq!(pixels(a), first);
    s.data.regenerate_level = true;
    after_menu(a, MAIN_NEW_GAME);
    assert_ne!(pixels(a), first);

    assert_eq!(after_menu(a, MAIN_QUIT), LIERO_AFTER_MENU_QUIT);
    assert_eq!(after_menu(a, MAIN_TC), LIERO_AFTER_MENU_RESTART);
    let cycles = a.menu_cycles;
    draw(a, true);
    draw(a, false);
    assert_eq!(a.menu_cycles, cycles.wrapping_add(1));
    a.controller = None;
}
