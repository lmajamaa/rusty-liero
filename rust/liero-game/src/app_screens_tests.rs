use super::*;
use crate::controllers::Controller;
use crate::fs::Node;
use crate::settings::new_settings;
use crate::tc::SharedTc;

/// The bundled TC.
fn tc() -> SharedTc {
    crate::tc::load_shared(&Node::new(b"../../TC/openliero"))
}

/// An app with the menus' settings and the bundled TC, and a match behind
/// the menu.
fn set_up() -> App {
    let mut a = App::new();
    a.settings = Some(new_settings());
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
    Controller::replay(&Node::new(path.as_bytes()), tc()).expect("a replay")
}

#[test]
fn resuming_a_replay_shows_the_single_screen() {
    let a = &mut set_up();
    a.primary_single = false;
    assert_eq!(after_menu(a, &Host::none(), MAIN_RESUME_GAME), LIERO_AFTER_MENU_PLAY);
    assert!(!a.primary_single);

    a.controller = Some(replay());
    assert!(a.controller.as_deref().unwrap().is_replay());
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
    let pixels = |a: &mut App| controller(a).level().expect("no level").pixels().to_vec();
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
