use super::*;
use crate::fs::Node;
use crate::keys::sdl_to_dos;
use crate::tc::SharedTc;

/// The bundled TC, with one reference for the test.
fn tc() -> SharedTc {
    let tc = crate::tc::liero_tc_load(&Node::new(b"../../TC/openliero"));
    assert!(tc.borrow().data().is_some());
    tc
}

/// A local match with the worms named `names`, on a level of its own.
fn local(tc: SharedTc, names: [&str; 2]) -> Box<LieroController> {
    let settings = liero_settings_box_new();
    for (w, name) in settings.borrow().data.worms.iter().zip(names) {
        w.borrow_mut().data.name = name.as_bytes().to_vec();
    }
    let mut c = liero_local_controller_new(&Host::none(), settings, tc);
    c.game().unwrap().level_box_mut().resize(504, 350);
    c
}

#[test]
fn escape_fades_to_the_menu_once() {
    let tc = tc();
    let mut c = local(tc.clone(), ["a", "b"]);
    unsafe {
        c.on_key(LIERO_DK_ESCAPE as i32, true);
        assert_eq!((c.data.fade_value, c.data.going_to_menu), (31, true));
        c.data.fade_value = 5;
        c.on_key(LIERO_DK_ESCAPE as i32, true);
        assert_eq!(c.data.fade_value, 5);

        // A replay takes Escape and nothing else.
        let mut r = LieroController::new(Kind::Replay(Replay {
            reader: None,
            tc: tc.clone(),
            game: None,
            initial: None,
            initial_pos: 0,
        }));
        r.on_key(LIERO_DK_ESCAPE as i32 + 1, true);
        assert!(!r.data.going_to_menu);
        r.on_key(LIERO_DK_ESCAPE as i32, false);
        assert_eq!((r.data.fade_value, r.data.going_to_menu), (31, true));
        drop((c, r));
    }
}

#[test]
fn replays_are_named_after_four_characters_of_each_name() {
    let tc = tc();
    let mut c = local(tc.clone(), ["Ab!cdef", "x-y z9"]);
    // No host, so no time.
    assert_eq!(unsafe { replay_name(&Host::none(), c.game().unwrap()) }, b" Abc-xy.lrp");
    drop(c);
}

#[test]
fn focus_starts_weapon_selection_and_sets_both_palettes() {
    let tc = tc();
    let mut c = local(tc.clone(), ["a", "b"]);
    c.game().unwrap().level_box_mut().data_mut().origpal[200].r = 7;
    let a = &mut LieroApp::new();
    a.play.data.origpal[200].r = 0;
    a.single.data.origpal[200].r = 0;
    unsafe { c.focus(a, &Host::none()) };
    assert_eq!(c.data.state, LIERO_STATE_WEAPON_SELECTION);
    assert_eq!((a.play.data.origpal[200].r, a.single.data.origpal[200].r), (7, 7));
    assert_eq!((c.data.fade_value, c.data.going_to_menu), (0, false));
    drop(c);
}

#[test]
fn the_stats_screen_shows_each_worm_facing_the_middle() {
    let tc = tc();
    let mut c = local(tc.clone(), ["left", "right"]);
    let game = c.game().unwrap();
    game.settings_box().data.game_mode = 2;
    let g = game.stats_game();
    {
        let tc = game.tc_data();
        let s = crate::stats::stats_scene(&tc, &g, 320, 200);
        let sprites = &tc.worm_sprites;
        assert_eq!(s.worm_sprites[0].mem, sprites.sprite(23));
        assert_eq!(s.worm_sprites[1].mem, sprites.sprite(44));
        assert_eq!(s.names, [&b"left"[..], &b"right"[..]]);
        assert_eq!((s.game_mode, s.game_mode_name), (2, &b"Holdazone"[..]));
    }
    drop(c);
}

#[test]
fn r_rewinds_a_replay_to_its_start() {
    let dir = "target/controllers-tests/rewind";
    let _ = std::fs::remove_dir_all(dir);
    let path = format!("{dir}/x.lrp");
    let node = Node::new(path.as_bytes());
    let tc = tc();
    unsafe {
        // A short game recorded.
        let mut c = local(tc.clone(), ["a", "b"]);
        let game = c.game().unwrap();
        // Weapons, as weapon selection gives them.
        for w in game.listed_worms() {
            for (i, ww) in w.unwrap().data.weapons.iter_mut().enumerate() {
                ww.kind = i as i32;
            }
        }
        let blood = game.settings_box().data.ext.blood_particle_max;
        game.start(blood);
        let mut w = crate::replay_file::LieroReplayWriter::new(&node).unwrap();
        assert_eq!(w.begin(game).status, LieroReplayStatus::Ok);
        for i in 0..30 {
            game.worm_at(0).unwrap().data.control_states = if i % 3 == 0 { 1 } else { 4 };
            w.frame(game);
            game.process_frame(&LieroGameHooks { hires_ticks: &|| 0 });
        }
        drop(w);
        drop(c);

        let a = &mut LieroApp::new();
        crate::app::liero_app_set_settings(a, Some(liero_settings_box_new()));
        let mut r = Box::from_raw(liero_replay_controller_new(&node, tc));
        r.focus(a, &Host::none());
        assert_eq!(r.data.state, LIERO_STATE_GAME);
        assert!(r.process(a, &Host::none()));
        let pos = |r: &LieroController| match &r.kind {
            Kind::Replay(r) => r.reader.as_ref().unwrap().pos(),
            _ => unreachable!(),
        };
        let (p1, c1) = (pos(&r), r.game().unwrap().data_mut().cycles);
        for _ in 0..5 {
            assert!(r.process(a, &Host::none()));
        }
        assert_ne!(pos(&r), p1);
        a.keys.set(sdl_to_dos(SCANCODE_R), true);
        assert!(r.process(a, &Host::none()));
        assert_eq!((pos(&r), r.game().unwrap().data_mut().cycles), (p1, c1));
        // The worms' stats places, which replays don't store.
        let g = r.game().unwrap();
        assert_eq!(
            (g.worm_at(0).unwrap().data.stats_x, g.worm_at(1).unwrap().data.stats_x),
            (0, 218)
        );
        drop(r);
    }
}

#[test]
fn the_game_fades_in_to_fully_shown() {
    let tc = tc();
    let mut c = local(tc.clone(), ["a", "b"]);
    let a = &mut LieroApp::new();
    unsafe {
        for _ in 0..40 {
            assert!(c.process(a, &Host::none()));
        }
        assert_eq!(c.data.fade_value, LIERO_FADE_SHOWN);
        // Then out to the menu, which ends it.
        c.data.going_to_menu = true;
        for _ in 0..LIERO_FADE_SHOWN {
            assert!(c.process(a, &Host::none()));
        }
        assert!(!c.process(a, &Host::none()));
    }
    drop(c);
}

#[test]
fn f8_changes_the_tc_of_the_game_behind_the_menu() {
    let tc = tc();
    let mut c = local(tc.clone(), ["a", "b"]);
    let ammo = |c: &mut LieroController| -> Vec<i32> {
        c.game()
            .unwrap()
            .tc_data()
            .model
            .weapons
            .iter()
            .map(|w| w.ammo)
            .collect()
    };
    let before = ammo(&mut c);
    crate::tc::liero_tc_randomize(&tc, 0);
    let after = ammo(&mut c);
    assert_ne!(after, before);
    assert_eq!(
        after,
        crate::tc::tc_data(&tc)
            .model
            .weapons
            .iter()
            .map(|w| w.ammo)
            .collect::<Vec<_>>()
    );
}
