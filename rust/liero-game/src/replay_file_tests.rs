use super::*;
use crate::fs::Node;
use crate::game_box::{liero_game_free, liero_game_new};
use crate::level::LieroLevelBox;
use crate::settings::{liero_settings_box_free, liero_settings_box_new};
use crate::tc::{LieroTc, LieroTcData, SharedTc};
use crate::worm::LieroWormData;

const NO_WASM: &str = "miniz_oxide's debug compressor builds its tables on the 64 KB wasm stack";

/// A game for a replay, with what it records beside it.
struct Setup {
    game: *mut LieroGame,
    settings: *mut LieroSettingsBox,
    tc: SharedTc,
}

impl Setup {
    fn empty() -> Self {
        let mut tc = LieroTcData::empty();
        tc.materials[5].flags = 7;
        let s = Setup {
            game: liero_game_new(),
            settings: liero_settings_box_new(),
            tc: LieroTc::of(tc),
        };
        let settings = unsafe { crate::shared::Shared::retain(s.settings) };
        unsafe { (*s.game).set_context_data(Some(settings), Some(s.tc.clone()), crate::sfx::LieroSoundOut::NONE) };
        s
    }

    /// Two worms, the first's rope on the second, two split-screen
    /// viewports and a spectator one, and a 3 x 2 level.
    fn new() -> Self {
        let mut s = Setup::empty();
        unsafe {
            let g = &mut *s.game;
            let mut worms = Vec::new();
            for i in 0..2 {
                let ws = crate::settings::new_worm_settings();
                ws.borrow_mut().name = format!("worm {i}").into_bytes();
                ws.borrow_mut().data.health = 100 + i;
                let w = liero_game_new_worm(g, Some(ws));
                let d = &mut g.worm_mut(w).data;
                d.index = i;
                d.pos.x = 10 * (i + 1);
                d.vel.y = -3;
                liero_game_list_worm(g, Some(w));
                worms.push(w);
            }
            g.worm_mut(worms[0]).data.ninjarope.anchor = Some(worms[1]);
            for i in 0..2 {
                let r = LieroRect {
                    x1: 160 * i,
                    y1: 0,
                    x2: 160 * i + 158,
                    y2: 158,
                };
                let vp = liero_game_add_viewport(g, r, i, 504, 350, LIERO_VIEWPORT_WORM, false);
                (*vp).shake = 4 + i;
            }
            liero_game_add_viewport(
                g,
                LieroRect {
                    x1: 0,
                    y1: 0,
                    x2: 572,
                    y2: 350,
                },
                0,
                504,
                350,
                1,
                true,
            );
            (*liero_game_data(g)).rand.x = 1234;
            (*s.settings).data.lives = 9;
        }
        s.level().resize(3, 2);
        s.level()
            .set_pixels(&[1, 5, 2, 5, 3, 4], |_| crate::material::LieroMaterial::default());
        s.level().data_mut().origpal[5].g = 42;
        s
    }

    fn level(&mut self) -> &mut LieroLevelBox {
        unsafe { &mut *liero_game_level(&mut *self.game) }
    }

    fn worm(&mut self, i: usize) -> &mut LieroWormData {
        unsafe { &mut (*liero_game_worm(&mut *self.game, i)).data }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        unsafe {
            liero_game_free(self.game);
            liero_settings_box_free(self.settings);
        }
    }
}

fn path(name: &str) -> String {
    let dir = format!("target/replay-file-tests/{name}");
    let _ = std::fs::remove_dir_all(&dir);
    format!("{dir}/x.lrp")
}

fn node(path: &str) -> Node {
    Node::new(path.as_bytes())
}

fn ok(o: LieroReplayOutcome) {
    assert_eq!(o.status, LieroReplayStatus::Ok);
}

fn message(o: &LieroReplayOutcome) -> &str {
    unsafe { std::str::from_utf8(o.message.bytes()).unwrap() }
}

/// Records `s` and then the controls in `frames`, one tick each.
fn record(path: &str, s: &mut Setup, frames: &[[u32; 2]]) {
    let w = liero_replay_writer_new(&node(path));
    assert!(!w.is_null());
    unsafe {
        ok(liero_replay_writer_begin(&mut *w, &mut *s.game));
        for f in frames {
            for (i, &c) in f.iter().enumerate() {
                s.worm(i).control_states = c;
            }
            liero_replay_writer_frame(&mut *w);
            for (i, &c) in f.iter().enumerate().take(2) {
                s.worm(i).prev_control_states = c;
            }
        }
        liero_replay_writer_free(w);
    }
}

/// A reader for `path` that has read the game into `s`.
fn play(path: &str, s: &mut Setup) -> *mut LieroReplayReader {
    let r = liero_replay_reader_new(&node(path));
    assert!(!r.is_null());
    unsafe { ok(liero_replay_reader_begin(&mut *r, &mut *s.game)) };
    r
}

/// Reads a frame, carrying the controls on as the game tick would.
unsafe fn frame(r: *mut LieroReplayReader, s: &mut Setup) -> LieroReplayOutcome {
    unsafe {
        let o = liero_replay_reader_frame(&mut *r);
        for i in 0..liero_game_worm_count(&*s.game) {
            let d = s.worm(i);
            d.prev_control_states = d.control_states;
        }
        o
    }
}

#[test]
fn plays_back_the_game_it_recorded() {
    let _ = NO_WASM;
    let p = path("round-trip");
    let mut s = Setup::new();
    let frames = [[1, 0], [1, 4], [0, 0x40]];
    record(&p, &mut s, &frames);

    let mut t = Setup::empty();
    let r = play(&p, &mut t);
    unsafe {
        let g = &mut *t.game;
        assert_eq!(liero_game_worm_count(g), 2);
        for i in 0..2 {
            let w = liero_game_worm(g, i);
            assert_eq!(
                ((*w).data.index, (*w).data.pos.x, (*w).data.vel.y),
                (i as i32, 10 * (i as i32 + 1), -3)
            );
            let ws = liero_worm_settings(&*w).unwrap();
            let ws = ws.borrow();
            assert_eq!(
                (ws.name.clone(), ws.data.health),
                (format!("worm {i}").into_bytes(), 100 + i as i32)
            );
        }
        // The rope is on the new second worm.
        let second = g.listed_ids()[1];
        assert_eq!(g.worm_at(0).unwrap().data.ninjarope.anchor, second);
        // The split-screen viewports, not the spectator one.
        assert_eq!(
            (liero_game_viewport_count(g, false), liero_game_viewport_count(g, true)),
            (2, 0)
        );
        let vp = &*liero_game_viewport(g, false, 1);
        assert_eq!((vp.shake, vp.worm_idx, vp.rect.x1), (5, 1, 160));
        assert_eq!((*liero_game_data(g)).rand.x, 1234);
        assert_eq!((*t.settings).data.lives, 9);
        assert_eq!(t.level().pixels(), &[1, 5, 2, 5, 3, 4]);
        let l = t.level().data().level;
        assert_eq!(((*l.materials.add(1)).flags, (*l.materials).flags), (7, 0));
        assert_eq!(t.level().data().origpal[5].g, 42);

        for f in frames {
            let o = frame(r, &mut t);
            ok(o);
            assert!(!o.end && !o.settings_changed);
            assert_eq!([t.worm(0).control_states, t.worm(1).control_states], f);
        }
        assert!(frame(r, &mut t).end);
        liero_replay_reader_free(r);
    }
}

#[test]
fn rewinds_to_a_position() {
    let p = path("rewind");
    let mut s = Setup::new();
    record(&p, &mut s, &[[1, 0], [2, 0], [4, 0]]);
    let mut t = Setup::empty();
    let r = play(&p, &mut t);
    unsafe {
        let start = liero_replay_reader_pos(&*r);
        ok(frame(r, &mut t));
        ok(frame(r, &mut t));
        assert_eq!(t.worm(0).control_states, 2);
        liero_replay_reader_set_pos(&mut *r, start);
        t.worm(0).prev_control_states = 0;
        ok(frame(r, &mut t));
        assert_eq!(t.worm(0).control_states, 1);
        liero_replay_reader_free(r);
    }
}

#[test]
fn writes_settings_changed_while_unfocused() {
    let p = path("settings");
    let s = Setup::new();
    let w = liero_replay_writer_new(&node(&p));
    unsafe {
        ok(liero_replay_writer_begin(&mut *w, &mut *s.game));
        liero_replay_writer_frame(&mut *w);
        // Unchanged settings aren't written again.
        liero_replay_writer_unfocus(&mut *w);
        liero_replay_writer_focus(&mut *w);
        liero_replay_writer_frame(&mut *w);
        liero_replay_writer_unfocus(&mut *w);
        (*s.settings).data.lives = 3;
        liero_replay_writer_focus(&mut *w);
        liero_replay_writer_frame(&mut *w);
        liero_replay_writer_free(w);
    }

    let mut t = Setup::empty();
    let r = play(&p, &mut t);
    unsafe {
        assert_eq!((*t.settings).data.lives, 9);
        assert!(!frame(r, &mut t).settings_changed);
        assert!(!frame(r, &mut t).settings_changed);
        let o = frame(r, &mut t);
        ok(o);
        assert!(o.settings_changed);
        assert_eq!((*t.settings).data.lives, 3);
        liero_replay_reader_free(r);
    }
}

#[test]
fn reports_desyncs_and_short_files() {
    let p = path("errors");
    let mut s = Setup::new();
    record(&p, &mut s, &[[0, 0], [0, 0]]);

    // Every frame is on tick 0, so each has a checksum.
    let mut t = Setup::empty();
    let r = play(&p, &mut t);
    unsafe {
        ok(frame(r, &mut t));
        t.worm(1).vel.x += 1;
        let o = frame(r, &mut t);
        assert_eq!(
            (o.status, message(&o)),
            (LieroReplayStatus::CheckError, "Replay has desynced")
        );
        liero_replay_reader_free(r);
    }

    // Cut short after the game: a read error at the end of what inflated.
    let data = inflate(&std::fs::read(&p).unwrap());
    let r = liero_replay_reader_new(&node(&p));
    unsafe {
        let mut u = Setup::empty();
        ok(liero_replay_reader_begin(&mut *r, &mut *u.game));
        (*r).data.truncate((*r).pos + 1);
        let o = frame(r, &mut u);
        assert_eq!(
            (o.status, message(&o)),
            (LieroReplayStatus::ReadError, "Read error in get()")
        );
        liero_replay_reader_free(r);
    }
    assert!(!data.error);

    // A game cut short leaves the game as it was.
    let r = liero_replay_reader_new(&node(&p));
    unsafe {
        (*r).data.truncate(40);
        let u = Setup::empty();
        let o = liero_replay_reader_begin(&mut *r, &mut *u.game);
        assert_eq!(o.status, LieroReplayStatus::ReadError);
        assert_eq!(liero_game_worm_count(&*u.game), 0);
        liero_replay_reader_free(r);
    }

    assert!(liero_replay_reader_new(&node("target/replay-file-tests/none.lrp")).is_null());
}

#[test]
fn writes_a_worms_changed_settings_with_more_than_three_worms() {
    let p = path("worm-settings");
    let s = Setup::new();
    unsafe {
        let g = &mut *s.game;
        for i in 2..4 {
            let ws = crate::settings::new_worm_settings();
            ws.borrow_mut().name = format!("worm {i}").into_bytes();
            let w = liero_game_new_worm(g, Some(ws));
            g.worm_mut(w).data.index = i;
            liero_game_list_worm(g, Some(w));
        }
        let w = liero_replay_writer_new(&node(&p));
        ok(liero_replay_writer_begin(&mut *w, g));
        liero_replay_writer_frame(&mut *w);
        liero_replay_writer_unfocus(&mut *w);
        liero_worm_settings(&*liero_game_worm(g, 2)).unwrap().borrow_mut().name = b"renamed".to_vec();
        liero_replay_writer_focus(&mut *w);
        liero_replay_writer_frame(&mut *w);
        // Written once.
        liero_replay_writer_frame(&mut *w);
        liero_replay_writer_free(w);
    }

    let mut t = Setup::empty();
    let r = play(&p, &mut t);
    unsafe {
        let game = t.game;
        let name = |i| {
            liero_worm_settings(&*liero_game_worm(&mut *game, i))
                .unwrap()
                .borrow()
                .name
                .clone()
        };
        assert!(!frame(r, &mut t).settings_changed);
        assert_eq!(name(2), b"worm 2");
        let o = frame(r, &mut t);
        ok(o);
        assert!(o.settings_changed);
        assert_eq!((name(2), name(3)), (b"renamed".to_vec(), b"worm 3".to_vec()));
        assert!(!frame(r, &mut t).settings_changed);
        liero_replay_reader_free(r);
    }
}
