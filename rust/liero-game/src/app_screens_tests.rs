use super::*;
use crate::controllers::{liero_controller_is_replay, Controller};
use crate::fs::Node;
use crate::settings::liero_settings_box_new;
use crate::tc::SharedTc;

/// The bundled TC.
fn tc() -> SharedTc {
    crate::tc::liero_tc_load(&Node::new(b"../../TC/openliero"))
}

/// An app with the menus' settings and the bundled TC, and a match behind
/// the menu.
fn set_up() -> App {
    let mut a = App::new();
    crate::app::liero_app_set_settings(&mut a, Some(liero_settings_box_new()));
    a.set_tc(Some(tc()));
    restart(&mut a, &Host::none());
    a
}

/// A replay controller that hasn't started.
fn replay() -> Box<Controller> {
    let dir = "target/app-screens-tests";
    std::fs::create_dir_all(dir).unwrap();
    let path = format!("{dir}/empty.lrp");
    std::fs::write(&path, b"").unwrap();
    let c = crate::controllers::liero_replay_controller_new(&Node::new(path.as_bytes()), tc());
    assert!(!c.is_null());
    unsafe { Box::from_raw(c) }
}

#[test]
fn resuming_a_replay_shows_the_single_screen() {
    let a = &mut set_up();
    a.primary_single = false;
    assert_eq!(after_menu(a, &Host::none(), MAIN_RESUME_GAME), LIERO_AFTER_MENU_PLAY);
    assert!(!a.primary_single);

    a.controller = Some(replay());
    assert!(liero_controller_is_replay(a.controller.as_deref().unwrap()));
    assert_eq!(after_menu(a, &Host::none(), MAIN_RESUME_GAME), LIERO_AFTER_MENU_PLAY);
    assert!(a.primary_single);

    // A replay chosen in the menu, as the settings say.
    a.primary_single = false;
    a.settings_mut().data.ext.single_screen_replay = false;
    assert_eq!(after_menu(a, &Host::none(), MAIN_REPLAY), LIERO_AFTER_MENU_PLAY);
    assert!(!a.primary_single);
    a.settings_mut().data.ext.single_screen_replay = true;
    assert_eq!(after_menu(a, &Host::none(), MAIN_REPLAY), LIERO_AFTER_MENU_PLAY);
    assert!(a.primary_single);
    a.controller = None;
}

#[test]
fn a_finished_game_goes_back_to_the_play_screen() {
    let a = &mut set_up();
    a.primary_single = true;
    let mut g = GameScreen {
        next: Next::Finish,
        shown: false,
        resumed: true,
    };
    let t = g.tick(a, &Host::none());
    assert_eq!((t.closed(), t.shown), (Some(0), false));
    assert!(!a.primary_single);
    a.controller = None;
}

#[test]
fn a_new_game_keeps_the_level_unless_the_settings_changed() {
    let a = &mut set_up();
    {
        let mut s = a.settings_mut();
        (s.data.random_level, s.data.regenerate_level) = (true, false);
    }
    restart(a, &Host::none());
    let pixels = |a: &mut App| unsafe { (*liero_controller_level(controller(a))).pixels().to_vec() };
    let first = pixels(a);
    assert_eq!(after_menu(a, &Host::none(), MAIN_NEW_GAME), LIERO_AFTER_MENU_PLAY);
    assert_eq!(pixels(a), first);
    a.settings_mut().data.regenerate_level = true;
    after_menu(a, &Host::none(), MAIN_NEW_GAME);
    assert_ne!(pixels(a), first);

    assert_eq!(after_menu(a, &Host::none(), MAIN_QUIT), LIERO_AFTER_MENU_QUIT);
    assert_eq!(after_menu(a, &Host::none(), MAIN_TC), LIERO_AFTER_MENU_RESTART);
    let cycles = a.menu_cycles;
    draw(a, true);
    draw(a, false);
    assert_eq!(a.menu_cycles, cycles.wrapping_add(1));
    a.controller = None;
}
