use super::*;
use crate::controller::LIERO_STATE_GAME;
use crate::fs::Node;
use crate::menus::Menus;
use crate::settings::new_settings;

/// An app with menus, the bundled TC, its screens sized, and a match that
/// hasn't started, on a level of colour 7.
fn set_up() -> App {
    let mut app = App::new();
    let a = &mut app;
    a.settings = Some(new_settings());
    a.set_tc(Some(
        crate::tc::load_checked(&Node::new(b"../../TC/openliero")).unwrap(),
    ));
    let menus = Menus::shared();
    menus.borrow_mut().load();
    a.menus = Some(menus);
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    let mut c = crate::controllers::Controller::local(&crate::screen::Host::none(), a.shared_settings(), a.menu_tc());
    let level = c.level().expect("no level");
    level.resize(504, 350);
    level.set_pixels(&vec![7; 504 * 350], |_| Default::default());
    a.controller = Some(c);
    app
}

#[test]
fn the_menu_starts_black_with_the_levels_miniature_behind_it() {
    let a = &mut set_up();
    a.play.data.fade_value = 20;
    a.single.data.fade_value = 20;
    a.menu_cycles = 9;
    menu_start(a);
    assert_eq!(
        (a.play.data.fade_value, a.single.data.fade_value, a.menu_cycles),
        (0, 0, 0)
    );

    // The miniature at half size, 126 left of the middle and 208 up from
    // the bottom.
    let s = a.frozen_spectator.view();
    let px = |x: i32, y: i32| a.frozen_spectator.pixels()[(y * s.pitch + x) as usize];
    let (x0, y0) = (320 - 126, 400 - 208);
    assert_eq!((px(x0, y0), px(x0 + 251, y0 + 174)), (7, 7));
    assert_eq!(
        (px(x0 - 1, y0), px(x0, y0 - 1), px(x0 + 252, y0), px(x0, y0 + 175)),
        (0, 0, 0, 0)
    );
    a.controller = None;
}

#[test]
fn the_spectator_screen_says_whether_a_game_is_paused() {
    let a = &mut set_up();
    a.frozen_spectator.copy_from(&a.single.bmp);
    let chars = a.font().chars.to_vec();
    let font = crate::text::Font { chars: &chars };
    let (cx, cy) = (320, 100);
    // Drawing the same words again changes nothing; other words do.
    let shows = |a: &mut App, words: &[u8]| {
        let before = a.single.bmp.pixels().to_vec();
        font.centered(&a.single.bmp.view(), words, cx, cy + 48, 7, 2);
        a.single.bmp.pixels() == before
    };

    draw_spectator_info(a);
    assert!(shows(a, b"SETUP") && !shows(a, b"PAUSED"));

    a.controller.as_deref_mut().unwrap().data().state = LIERO_STATE_GAME;
    draw_spectator_info(a);
    assert!(shows(a, b"PAUSED") && !shows(a, b"SETUP"));
    a.controller = None;
}
