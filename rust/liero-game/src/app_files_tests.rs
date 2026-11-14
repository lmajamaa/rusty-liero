use super::*;
use crate::menus::LIERO_SELECTOR_TC;

/// An empty folder for one test, and an app with default settings and its
/// config folder there.
fn set_up(name: &str) -> (App, String) {
    let dir = format!("target/app-files-tests/{name}");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut a = App::new();
    a.settings = Some(SettingsBox::defaults());
    a.files.config = Node::new(dir.as_bytes());
    (a, dir)
}

/// The full path of `rel` in the config folder.
fn in_config(a: &App, rel: &str) -> Vec<u8> {
    join_path(a.files.config.full_path(), rel.as_bytes())
}

fn source(root: &str, choice: Choice) -> AppFiles {
    AppFiles {
        root: Node::new(root.as_bytes()),
        choice,
    }
}

/// The bundled TC, as the app's.
fn with_tc(a: &mut App) {
    a.set_tc(Some(
        crate::tc::load_checked(&Node::new(b"../../TC/openliero")).unwrap(),
    ));
}

#[test]
fn loading_options_makes_new_settings_and_shows_their_file() {
    let (a, dir) = &mut set_up("load");
    let file = Node::new(format!("{dir}/setup.cfg").as_bytes());
    a.settings_mut().data.lives = 7;
    a.settings_mut().data.worms[0].borrow_mut().data.name = b"NEW".to_vec();
    assert!(a.save_settings(&file));

    a.files.settings_node = Node::default();
    a.settings_mut().data.lives = 3;
    a.settings_mut().data.worms[0].borrow_mut().data.name = b"OLD".to_vec();
    // A game's worms keep the settings they had.
    let old_box = a.settings.clone().unwrap();
    let old_worm = a.settings_mut().data.worms[0].clone();

    // The selector loads from a folder below its root.
    std::fs::create_dir_all(format!("{dir}/sub")).unwrap();
    std::fs::copy(format!("{dir}/setup.cfg"), format!("{dir}/sub/other.cfg")).unwrap();
    let mut s = source(dir, Choice::Options);
    assert_eq!(s.choose(a, b"sub/other.cfg", b"", b"other", false), -1);

    assert!(!std::rc::Rc::ptr_eq(a.settings.as_ref().unwrap(), &old_box));
    assert_eq!(a.settings_mut().data.lives, 7);
    assert_eq!(old_worm.borrow().data.name, b"OLD");
    assert_eq!(old_box.borrow().data.lives, 3);
    assert_eq!(settings_path(a), in_config(a, "sub/other.cfg"));

    // A file that isn't there gives the defaults, and is still shown.
    let missing = Node::new(format!("{dir}/missing.cfg").as_bytes());
    assert!(!a.load_settings(Some(&missing), false));
    assert_eq!(a.settings_mut().data.lives, SettingsBox::defaults().borrow().data.lives);
    assert_eq!(settings_path(a), in_config(a, "missing.cfg"));
    // LIERO.DAT doesn't change which file is shown.
    a.load_settings(Some(&file), true);
    assert_eq!(settings_path(a), in_config(a, "missing.cfg"));
}

#[test]
fn liero_cfg_is_written_only_when_the_settings_change() {
    let (a, dir) = &mut set_up("save-if-changed");
    let cfg = format!("{dir}/liero.cfg");
    a.mark_settings_saved();
    a.save_settings_if_changed();
    assert!(std::fs::metadata(&cfg).is_err());

    a.settings_mut().data.lives = 9;
    a.save_settings_if_changed();
    assert_eq!(std::fs::read(&cfg).unwrap(), a.settings_mut().text());

    std::fs::remove_file(&cfg).unwrap();
    a.save_settings_if_changed();
    assert!(std::fs::metadata(&cfg).is_err());
}

#[test]
fn choosing_a_level_sets_its_full_path_or_random() {
    let (a, dir) = &mut set_up("level");
    let mut s = source(dir, Choice::Level);
    a.settings_mut().data.random_level = true;
    assert_eq!(s.choose(a, b"sub/A.LEV", b"/cfg/sub/A.LEV", b"A", false), -1);
    assert!(!a.settings_mut().data.random_level);
    assert_eq!(a.settings_mut().data.level_file, b"/cfg/sub/A.LEV");
    s.choose(a, b"", b"", b"Random", true);
    assert!(a.settings_mut().data.random_level);
    assert!(a.settings_mut().data.level_file.is_empty());
}

#[test]
fn a_profile_saved_as_a_name_loads_back() {
    let (a, dir) = &mut set_up("profile");
    a.settings_mut().data.worms[1].borrow_mut().data.name = b"SAVED".to_vec();
    a.settings_mut().data.worms[1].borrow_mut().data.health = 77;
    save_profile_as(a, 1, b"mine");
    assert!(std::fs::metadata(format!("{dir}/Profiles/mine.lpf")).is_ok());

    a.settings_mut().data.worms[1].borrow_mut().data.name = b"OTHER".to_vec();
    a.settings_mut().data.worms[1].borrow_mut().data.health = 5;
    let ws = a.settings_mut().data.worms[1].clone();
    let mut s = source(dir, Choice::Profile(ws));
    s.choose(a, b"Profiles/mine.lpf", b"", b"mine", false);
    assert_eq!(a.settings_mut().data.worms[1].borrow_mut().data.name, b"SAVED");
    assert_eq!(a.settings_mut().data.worms[1].borrow_mut().data.health, 77);
    // Player 0's are left alone.
    assert_ne!(a.settings_mut().data.worms[0].borrow_mut().data.name, b"SAVED");
}

#[test]
fn saving_options_writes_a_cfg_in_the_config_folder() {
    let (a, dir) = &mut set_up("save-options");
    a.settings_mut().data.lives = 42;
    save_options(a, b"match");
    assert_eq!(
        std::fs::read(format!("{dir}/match.cfg")).unwrap(),
        a.settings_mut().text()
    );
    assert_eq!(settings_path(a), in_config(a, "match.cfg"));
}

#[test]
fn the_selectors_start_where_the_game_left_off() {
    let (a, _) = &mut set_up("specs");
    a.settings_mut().data.level_file = b"some/LEVEL.LEV".to_vec();
    with_tc(a);

    let (spec, _, choice) = selector_spec(a, LIERO_SELECTOR_LEVEL, 0);
    assert!(matches!(choice, Choice::Level));
    assert_eq!(
        (spec.filter.as_slice(), spec.select.as_slice(), spec.has_select),
        (&b"LEV"[..], &b"some/LEVEL.LEV"[..], true)
    );
    assert!(spec.enter_sound && !spec.random_name.is_empty());

    let (spec, _, choice) = selector_spec(a, LIERO_SELECTOR_PROFILE, 1);
    let Choice::Profile(ws) = choice else {
        panic!("not a profile")
    };
    assert!(std::rc::Rc::ptr_eq(&ws, &a.settings_mut().data.worms[1]));
    assert_eq!((spec.select, spec.has_select), (in_config(a, "Profiles"), true));

    let (spec, _, _) = selector_spec(a, LIERO_SELECTOR_REPLAY, 0);
    assert!(!spec.has_select);
    assert_eq!(spec.fallback, in_config(a, "Replays"));
    a.files.prev_replay = b"x/last.lrp".to_vec();
    let (spec, _, _) = selector_spec(a, LIERO_SELECTOR_REPLAY, 0);
    assert_eq!((spec.select.as_slice(), spec.has_select), (&b"x/last.lrp"[..], true));

    let (spec, _, choice) = selector_spec(a, LIERO_SELECTOR_OPTIONS, 0);
    assert!(matches!(choice, Choice::Options));
    assert_eq!((spec.filter.as_slice(), spec.has_select), (&b"CFG"[..], false));

    a.files.tc_root = Node::new(b"../../TC");
    let (spec, root, choice) = selector_spec(a, LIERO_SELECTOR_TC, 0);
    assert!(matches!(choice, Choice::Tc) && spec.tcs);
    assert_eq!(
        (spec.root.as_slice(), root.full_path()),
        (&b"./../../TC"[..], &b"./../../TC"[..])
    );
}

#[test]
fn a_chosen_tc_is_switched_to_once() {
    let (a, _) = &mut set_up("tc");
    with_tc(a);
    let before = a.menu_tc();
    a.settings_mut().data.ext.tc = b"other".to_vec();
    let mut s = source("../../TC", Choice::Tc);
    assert_eq!(
        s.choose(a, b"openliero", b"../../TC/openliero", b"openliero", false),
        -1
    );
    assert_eq!(a.settings_mut().data.ext.tc, b"openliero");
    assert!(std::rc::Rc::ptr_eq(&a.menu_tc(), &before));
    assert!(take_chosen_tc(a));
    assert!(!std::rc::Rc::ptr_eq(&a.menu_tc(), &before));
    assert!(!take_chosen_tc(a));
}

#[test]
fn choosing_a_replay_plays_it_and_remembers_it() {
    let (a, dir) = &mut set_up("replay");
    with_tc(a);
    std::fs::write(format!("{dir}/empty.lrp"), b"").unwrap();
    let mut s = source(dir, Choice::Replay);
    let full = format!("{dir}/empty.lrp");
    assert_eq!(
        s.choose(a, b"empty.lrp", full.as_bytes(), b"empty", false),
        LIERO_MAIN_REPLAY
    );
    assert_eq!(a.files.prev_replay, full.into_bytes());
    assert!(a.controller.as_deref_mut().unwrap().is_replay());
    a.controller = None;
}

#[test]
fn a_level_is_previewed_in_miniature_on_both_screens() {
    let (a, dir) = &mut set_up("preview");
    with_tc(a);
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    a.play.clear();
    a.single.clear();
    a.frozen.copy_from(&a.play.bmp);
    a.frozen_spectator.copy_from(&a.single.bmp);
    std::fs::write(format!("{dir}/A.LEV"), vec![7u8; 504 * 350]).unwrap();
    // A file cut short isn't shown.
    std::fs::write(format!("{dir}/SHORT.LEV"), vec![9u8; 100]).unwrap();

    let mut s = source(dir, Choice::Level);
    s.preview(a, b"SHORT.LEV");
    assert!(a.frozen.pixels().iter().all(|&p| p == 0));
    s.preview(a, b"A.LEV");

    // 504 x 350 at one pixel in 10 is 50 x 35, at (134, 162).
    let v = a.frozen.view();
    let px = |x: i32, y: i32| a.frozen.pixels()[(y * v.pitch + x) as usize];
    assert_eq!((px(134, 162), px(183, 196)), (7, 7));
    assert_eq!((px(133, 162), px(184, 162), px(134, 197)), (0, 0, 0));
    // At one in 2, 252 x 175, 126 left of the middle and 208 up.
    let v = a.frozen_spectator.view();
    let px = |x: i32, y: i32| a.frozen_spectator.pixels()[(y * v.pitch + x) as usize];
    let (x0, y0) = (320 - 126, 400 - 208);
    assert_eq!((px(x0, y0), px(x0 + 251, y0 + 174)), (7, 7));
    assert_eq!((px(x0 + 252, y0), px(x0, y0 + 175)), (0, 0));
}
