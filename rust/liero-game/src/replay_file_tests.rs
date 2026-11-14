use super::*;
use crate::fs::Node;

use crate::level::LevelBox;
use crate::settings::{liero_settings_box_new, SharedSettings};
use crate::tc::{SharedTc, Tc, TcData};
use crate::worm::WormData;

const NO_WASM: &str = "miniz_oxide's debug compressor builds its tables on the 64 KB wasm stack";

/// A game for a replay, with what it records beside it.
struct Setup {
    game: Box<Game>,
    settings: SharedSettings,
    _tc: SharedTc,
}

impl Setup {
    fn empty() -> Self {
        let mut tc = TcData::empty();
        tc.materials[5].flags = 7;
        // SAFETY: a new game, which the setup owns.
        let mut s = Setup {
            game: Game::new(),
            settings: liero_settings_box_new(),
            _tc: Tc::of(tc),
        };
        s.game.set_context(
            Some(s.settings.clone()),
            Some(s._tc.clone()),
            crate::sfx::SoundOut::NONE,
        );
        s
    }

    /// Two worms, the first's rope on the second, two split-screen
    /// viewports and a spectator one, and a 3 x 2 level.
    fn new() -> Self {
        let mut s = Setup::empty();
        let g = &mut *s.game;
        let mut worms = Vec::new();
        for i in 0..2 {
            let ws = crate::settings::new_worm_settings();
            ws.borrow_mut().data.name = format!("worm {i}").into_bytes();
            ws.borrow_mut().data.health = 100 + i;
            let w = g.new_worm(Some(ws));
            let d = &mut g.worm_mut(w).data;
            d.index = i;
            d.pos.x = 10 * (i + 1);
            d.vel.y = -3;
            g.list_worm(Some(w));
            worms.push(w);
        }
        g.worm_mut(worms[0]).data.ninjarope.anchor = Some(worms[1]);
        for i in 0..2 {
            let r = Rect {
                x1: 160 * i,
                y1: 0,
                x2: 160 * i + 158,
                y2: 158,
            };
            g.add_viewport(r, i, 504, 350, LIERO_VIEWPORT_WORM, false).shake = 4 + i;
        }
        g.add_viewport(
            Rect {
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
        g.data_mut().rand.x = 1234;
        s.settings.borrow_mut().data.lives = 9;
        s.level().resize(3, 2);
        s.level()
            .set_pixels(&[1, 5, 2, 5, 3, 4], |_| crate::material::Material::default());
        s.level().data_mut().origpal[5].g = 42;
        s
    }

    fn level(&mut self) -> &mut LevelBox {
        self.game.level_box_mut()
    }

    fn worm(&mut self, i: usize) -> &mut WormData {
        &mut self.game.worm_at(i).expect("no such worm").data
    }

    fn worm_count(&self) -> usize {
        self.game.listed_ids().len()
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

fn ok(o: ReplayOutcome) {
    assert_eq!(o.status, ReplayStatus::Ok);
}

/// Records `s` and then the controls in `frames`, one tick each.
fn record(path: &str, s: &mut Setup, frames: &[[u32; 2]]) {
    let mut w = ReplayWriter::new(&node(path)).unwrap();
    ok(w.begin(&mut s.game));
    for f in frames {
        for (i, &c) in f.iter().enumerate() {
            s.worm(i).control_states = c;
        }
        w.frame(&mut s.game);
        for (i, &c) in f.iter().enumerate().take(2) {
            s.worm(i).prev_control_states = c;
        }
    }
}

/// A reader for `path` that has read the game into `s`.
fn play(path: &str, s: &mut Setup) -> ReplayReader {
    let mut r = ReplayReader::new(&node(path)).unwrap();
    ok(r.begin(&mut s.game));
    r
}

/// Reads a frame, carrying the controls on as the game tick would.
fn frame(r: &mut ReplayReader, s: &mut Setup) -> ReplayOutcome {
    let o = r.frame(&mut s.game);
    for i in 0..s.worm_count() {
        let d = s.worm(i);
        d.prev_control_states = d.control_states;
    }
    o
}

#[test]
fn plays_back_the_game_it_recorded() {
    let _ = NO_WASM;
    let p = path("round-trip");
    let mut s = Setup::new();
    let frames = [[1, 0], [1, 4], [0, 0x40]];
    record(&p, &mut s, &frames);

    let mut t = Setup::empty();
    let mut r = play(&p, &mut t);
    let g = &mut *t.game;
    assert_eq!(g.listed_ids().len(), 2);
    for i in 0..2 {
        let w = g.worm_at(i).unwrap();
        assert_eq!(
            (w.data.index, w.data.pos.x, w.data.vel.y),
            (i as i32, 10 * (i as i32 + 1), -3)
        );
        let ws = w.settings();
        assert_eq!(
            (ws.data.name.clone(), ws.data.health),
            (format!("worm {i}").into_bytes(), 100 + i as i32)
        );
    }
    // The rope is on the new second worm.
    let second = g.listed_ids()[1];
    assert_eq!(g.worm_at(0).unwrap().data.ninjarope.anchor, second);
    // The split-screen viewports, not the spectator one.
    assert_eq!((g.viewport_count(false), g.viewport_count(true)), (2, 0));
    let vp = &g.split_viewports()[1];
    assert_eq!((vp.shake, vp.worm_idx, vp.rect.x1), (5, 1, 160));
    assert_eq!(g.data().rand.x, 1234);
    assert_eq!(t.settings.borrow_mut().data.lives, 9);
    assert_eq!(t.level().pixels(), &[1, 5, 2, 5, 3, 4]);
    let materials = t.level().level().materials.to_vec();
    assert_eq!((materials[1].flags, materials[0].flags), (7, 0));
    assert_eq!(t.level().data().origpal[5].g, 42);

    for f in frames {
        let o = frame(&mut r, &mut t);
        ok(o);
        assert!(!o.end && !o.settings_changed);
        assert_eq!([t.worm(0).control_states, t.worm(1).control_states], f);
    }
    assert!(frame(&mut r, &mut t).end);
}

#[test]
fn rewinds_to_a_position() {
    let p = path("rewind");
    let mut s = Setup::new();
    record(&p, &mut s, &[[1, 0], [2, 0], [4, 0]]);
    let mut t = Setup::empty();
    let mut r = play(&p, &mut t);
    let start = r.pos();
    ok(frame(&mut r, &mut t));
    ok(frame(&mut r, &mut t));
    assert_eq!(t.worm(0).control_states, 2);
    r.set_pos(start);
    t.worm(0).prev_control_states = 0;
    ok(frame(&mut r, &mut t));
    assert_eq!(t.worm(0).control_states, 1);
}

#[test]
fn writes_settings_changed_while_unfocused() {
    let p = path("settings");
    let mut s = Setup::new();
    {
        let mut w = ReplayWriter::new(&node(&p)).unwrap();
        let g = &mut *s.game;
        ok(w.begin(g));
        w.frame(g);
        // Unchanged settings aren't written again.
        w.unfocus(g);
        w.focus(g);
        w.frame(g);
        w.unfocus(g);
        s.settings.borrow_mut().data.lives = 3;
        w.focus(g);
        w.frame(g);
    }

    let mut t = Setup::empty();
    let mut r = play(&p, &mut t);
    assert_eq!(t.settings.borrow_mut().data.lives, 9);
    assert!(!frame(&mut r, &mut t).settings_changed);
    assert!(!frame(&mut r, &mut t).settings_changed);
    let o = frame(&mut r, &mut t);
    ok(o);
    assert!(o.settings_changed);
    assert_eq!(t.settings.borrow_mut().data.lives, 3);
}

#[test]
fn reports_desyncs_and_short_files() {
    let p = path("errors");
    let mut s = Setup::new();
    record(&p, &mut s, &[[0, 0], [0, 0]]);

    // Every frame is on tick 0, so each has a checksum.
    let mut t = Setup::empty();
    let mut r = play(&p, &mut t);
    ok(frame(&mut r, &mut t));
    t.worm(1).vel.x += 1;
    let o = frame(&mut r, &mut t);
    assert_eq!((o.status, o.message), (ReplayStatus::CheckError, "Replay has desynced"));

    // Cut short after the game: a read error at the end of what inflated.
    let data = inflate(&std::fs::read(&p).unwrap());
    let mut r = ReplayReader::new(&node(&p)).unwrap();
    let mut u = Setup::empty();
    ok(r.begin(&mut u.game));
    r.data.truncate(r.pos + 1);
    let o = frame(&mut r, &mut u);
    assert_eq!((o.status, o.message), (ReplayStatus::ReadError, "Read error in get()"));
    assert!(!data.error);

    // A game cut short leaves the game as it was.
    let mut r = ReplayReader::new(&node(&p)).unwrap();
    r.data.truncate(40);
    let mut u = Setup::empty();
    let o = r.begin(&mut u.game);
    assert_eq!(o.status, ReplayStatus::ReadError);
    assert_eq!(u.worm_count(), 0);

    assert!(ReplayReader::new(&node("target/replay-file-tests/none.lrp")).is_none());
}

#[test]
fn writes_a_worms_changed_settings_with_more_than_three_worms() {
    let p = path("worm-settings");
    let mut s = Setup::new();
    {
        let g = &mut *s.game;
        for i in 2..4 {
            let ws = crate::settings::new_worm_settings();
            ws.borrow_mut().data.name = format!("worm {i}").into_bytes();
            let w = g.new_worm(Some(ws));
            g.worm_mut(w).data.index = i;
            g.list_worm(Some(w));
        }
        let mut w = ReplayWriter::new(&node(&p)).unwrap();
        ok(w.begin(g));
        w.frame(g);
        w.unfocus(g);
        g.worm_at(2).unwrap().settings_mut().data.name = b"renamed".to_vec();
        w.focus(g);
        w.frame(g);
        // Written once.
        w.frame(g);
    }

    let mut t = Setup::empty();
    let mut r = play(&p, &mut t);
    let name = |t: &mut Setup, i| t.game.worm_at(i).unwrap().settings().data.name.clone();
    assert!(!frame(&mut r, &mut t).settings_changed);
    assert_eq!(name(&mut t, 2), b"worm 2");
    let o = frame(&mut r, &mut t);
    ok(o);
    assert!(o.settings_changed);
    assert_eq!(
        (name(&mut t, 2), name(&mut t, 3)),
        (b"renamed".to_vec(), b"worm 3".to_vec())
    );
    assert!(!frame(&mut r, &mut t).settings_changed);
}
