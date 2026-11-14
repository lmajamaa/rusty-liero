//! Rollback, offline (step 1 of docs/003-multiplayer.md): bot games played
//! straight through, and again as a networked game would play them, with
//! worm 1's inputs arriving late. Each frame the game goes back to the last
//! frame whose inputs are all known (`Game::restore`), plays the frames
//! whose inputs have arrived since, with sounds and stats, saves itself
//! (`Game::save`) and plays on to the present quietly, guessing that worm 1
//! keeps its last known controls. Every confirmed frame must hash as the
//! straight run's did: the game, the viewports, and at intervals the drawn
//! frames and the stats; and the sounds must be the same, at the same
//! frames.
//!
//! The bots only ever think on confirmed frames, so their own state is never
//! rolled back; the guesses stand in for their controls in between. Both
//! sides of a network game could run them that way and agree.

use super::game::{hash_frame, hash_game, hash_stats};
use super::games::{bot_tick, set_up_game, GAME_OF_TAG, HOLDAZONE, HOOKS, KILL_EM_ALL, SCALES_OF_JUSTICE, THROWERS};
use super::Fnv;
use crate::app::App;
use crate::fs::Node;
use crate::game_box::Game;
use crate::renderer::Renderer;
use crate::sfx::SoundOut;
use crate::tc::{tc_data, SharedTc};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The TC the hash run plays with, from the repository (tests run in
/// `rust/liero-game`).
fn tc() -> SharedTc {
    crate::tc::load_checked(&Node::new(b"../../TC/openliero")).expect("the TC in TC/openliero")
}

/// Who controls the worms.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bots {
    /// Nobody: the controls are given each frame.
    None,
    /// The original bot.
    Dumb,
    /// The predictive bot, with its search turned down as in the hash run.
    Predictive,
}

#[derive(Clone, Copy)]
struct Setup {
    seed: u32,
    game_mode: u32,
    weapons: Option<&'static [&'static str; 5]>,
    bots: Bots,
}

/// Where the controls of the worms come from on confirmed frames.
#[derive(Clone, Copy)]
enum Inputs<'a> {
    /// Each frame's controls, both worms', in order from frame 1.
    Given(&'a [[u32; 2]]),
    /// The bots.
    Bots,
}

/// What a run gives, frame by frame.
#[derive(Debug, Default, PartialEq, Eq)]
struct Trace {
    /// The game's hash after each frame.
    games: Vec<u64>,
    /// The viewports' after each frame.
    viewports: Vec<u64>,
    /// The drawn frames and the stats, every `DRAW_EVERY` frames.
    frames: Vec<(i32, u64)>,
    stats: Vec<(i32, u64)>,
    /// Each sound played, with the frame it was played on (0 for the start).
    sounds: Vec<(i32, i32)>,
}

const DRAW_EVERY: i32 = 50;

/// A game being played, with what it has given so far.
struct Run {
    game: Box<Game>,
    split: Box<Renderer>,
    spectator: Box<Renderer>,
    /// The frame being played, for the sounds.
    frame: Rc<Cell<i32>>,
    sounds: Rc<RefCell<Vec<(i32, i32)>>>,
    trace: Trace,
}

impl Run {
    /// The hash run's bot game `s`, started, its sounds noted.
    fn new(tc: &SharedTc, s: &Setup) -> Run {
        let mut a = App::new();
        let mut game = set_up_game(&mut a, tc, s.seed, s.game_mode, s.weapons);
        match s.bots {
            Bots::None => {
                for i in 0..2 {
                    game.worm_at(i).expect("no worm").set_ai(0, 0, false);
                }
            }
            Bots::Dumb => {}
            Bots::Predictive => {
                // As the hash run's AI games set them up.
                {
                    let d = &mut game.settings_box().data;
                    (d.ext.ai_frames, d.ext.ai_mutations, d.ext.ai_parallels, d.ext.ai_traces) = (30, 1, 3, false);
                }
                for i in 0..2 {
                    let w = game.worm_at(i).expect("no worm");
                    w.settings_mut().data.controller = 2;
                    w.set_ai(2, 3, i == 0);
                    let ai = w.ai_mut().expect("a bot");
                    ai.rand().seed(s.seed.wrapping_mul(31).wrapping_add(i as u32));
                }
            }
        }

        let frame = Rc::new(Cell::new(0));
        let sounds = Rc::new(RefCell::new(Vec::new()));
        let report = {
            let (frame, sounds) = (frame.clone(), sounds.clone());
            Rc::new(move |id| sounds.borrow_mut().push((frame.get(), id)))
        };
        let settings = game.settings();
        game.set_context(
            Some(settings),
            Some(tc.clone()),
            SoundOut {
                mixer: None,
                report: Some(report),
            },
        );
        let blood = game.settings_box().data.ext.blood_particle_max;
        game.start(blood);

        let mut split = Renderer::new();
        let mut spectator = Renderer::new();
        split.set_render_resolution(320, 200);
        spectator.set_render_resolution(640, 400);
        for r in [&mut split, &mut spectator] {
            r.load_palette(&tc_data(tc));
            game.focus(r);
        }
        Run {
            game,
            split,
            spectator,
            frame,
            sounds,
            trace: Trace::default(),
        }
    }

    fn set_controls(&mut self, worm: usize, controls: u32) {
        self.game.worm_at(worm).expect("no worm").data.control_states = controls;
    }

    fn controls(&mut self, worm: usize) -> u32 {
        self.game.worm_at(worm).expect("no worm").data.control_states
    }

    /// Plays confirmed frame `t`, with sounds and stats, and notes its
    /// hashes. Whether the game is over after it.
    fn confirmed(&mut self, t: i32, inputs: Inputs) -> bool {
        self.frame.set(t);
        match inputs {
            Inputs::Given(c) => {
                for (i, &c) in c[t as usize - 1].iter().enumerate() {
                    self.set_controls(i, c);
                }
            }
            Inputs::Bots => bot_tick(&mut self.game),
        }
        self.game.process_frame(&HOOKS);

        let tr = &mut self.trace;
        tr.games.push(hash_game(&mut self.game));
        let mut f = Fnv::default();
        for spectators in [false, true] {
            for i in 0..self.game.viewport_count(spectators) {
                f.bytes(format!("{:?}", self.game.viewport_mut(spectators, i)).as_bytes());
            }
        }
        tr.viewports.push(f.0);
        if t % DRAW_EVERY == 0 {
            tr.frames
                .push((t, hash_frame(&mut self.game, &mut self.split, &mut self.spectator)));
            tr.stats.push((t, hash_stats(self.game.stats())));
        }
        self.game.is_over()
    }

    fn finish(self) -> Trace {
        let mut t = self.trace;
        t.sounds = self.sounds.take();
        t
    }
}

/// Plays `s` straight through for `ticks` frames, or until it is over.
fn straight(tc: &SharedTc, s: &Setup, inputs: Inputs, ticks: i32) -> Trace {
    let mut run = Run::new(tc, s);
    for t in 1..=ticks {
        if run.confirmed(t, inputs) {
            break;
        }
    }
    run.finish()
}

/// What rolling back did.
#[derive(Debug, Default)]
struct Rollbacks {
    /// Frames played again, in all.
    replayed: usize,
    /// The most frames gone back at once.
    deepest: i32,
    /// Guessed frames whose game differed from the confirmed one.
    wrong_guesses: usize,
}

/// Plays `s` with worm 1's input for frame `f` arriving at frame
/// `f + delay(f)`, in order, rolling back every frame. The confirmed frames
/// must give `expected`.
fn rolled_back(
    tc: &SharedTc,
    s: &Setup,
    inputs: Inputs,
    expected: &Trace,
    delay: impl Fn(i32) -> i32,
) -> (Trace, Rollbacks) {
    let end = expected.games.len() as i32;
    let mut run = Run::new(tc, s);
    let mut saved = run.game.snapshot();
    let mut rb = Rollbacks::default();
    let (mut confirmed, mut arrived) = (0, 0);
    let mut now = 0;
    while confirmed < end {
        now += 1;
        while arrived < end && arrived + 1 + delay(arrived + 1) <= now {
            arrived += 1;
        }

        // Back to the last confirmed frame, then the newly confirmed ones.
        run.game.restore(&saved);
        rb.deepest = rb.deepest.max(now - 1 - confirmed);
        rb.replayed += (now - 1 - confirmed) as usize;
        let target = arrived.min(now);
        for t in confirmed + 1..=target {
            run.confirmed(t, inputs);
        }
        confirmed = target;
        run.game.save(&mut saved);

        // Then up to the present, guessed: worm 1 keeps its controls, and
        // with bots, so does worm 0.
        for t in confirmed + 1..=now.min(end) {
            if let Inputs::Given(c) = inputs {
                run.set_controls(0, c[t as usize - 1][0]);
            }
            run.game.process_frame_quietly();
            if t == now && hash_game(&mut run.game) != expected.games[t as usize - 1] {
                rb.wrong_guesses += 1;
            }
        }
    }
    (run.finish(), rb)
}

/// Worm 1's controls, as the original bots played `s` for `ticks` frames.
fn record(tc: &SharedTc, s: &Setup, ticks: i32) -> Vec<[u32; 2]> {
    let mut run = Run::new(tc, &Setup { bots: Bots::Dumb, ..*s });
    let mut out = Vec::new();
    for _ in 0..ticks {
        bot_tick(&mut run.game);
        out.push([run.controls(0), run.controls(1)]);
        run.game.process_frame(&HOOKS);
    }
    out
}

/// A delay from 1 to 8 frames, different for each frame.
fn jitter(f: i32) -> i32 {
    1 + (Fnv::of(&f.to_le_bytes()).0 % 8) as i32
}

fn check(tc: &SharedTc, s: &Setup, inputs: Inputs, expected: &Trace, delay: impl Fn(i32) -> i32) -> Rollbacks {
    let (got, rb) = rolled_back(tc, s, inputs, expected, delay);
    let first = |a: &[u64], b: &[u64]| a.iter().zip(b).position(|(x, y)| x != y);
    assert_eq!(
        first(&got.games, &expected.games),
        None,
        "the game differs at frame (index)"
    );
    assert_eq!(first(&got.viewports, &expected.viewports), None, "the viewports differ");
    assert_eq!(got, *expected);
    assert!(rb.wrong_guesses > 0, "no guess was wrong, so nothing was tested");
    rb
}

fn given(seed: u32, game_mode: u32, weapons: Option<&'static [&'static str; 5]>) -> Setup {
    Setup {
        seed,
        game_mode,
        weapons,
        bots: Bots::None,
    }
}

#[test]
fn each_fixed_delay_matches_the_straight_run() {
    let tc = tc();
    let s = given(1, KILL_EM_ALL, None);
    let inputs = record(&tc, &s, 700);
    let expected = straight(&tc, &s, Inputs::Given(&inputs), 700);
    assert!(expected.sounds.len() > 1 && !expected.stats.is_empty());
    for n in 1..=8 {
        let rb = check(&tc, &s, Inputs::Given(&inputs), &expected, |_| n);
        assert_eq!(rb.deepest, n);
    }
}

#[test]
fn jittery_delays_match_the_straight_run_in_each_mode() {
    let tc = tc();
    for s in [
        given(5, GAME_OF_TAG, None),
        given(6, HOLDAZONE, None),
        given(13, KILL_EM_ALL, Some(&THROWERS)),
        given(15, SCALES_OF_JUSTICE, None),
    ] {
        let inputs = record(&tc, &s, 1500);
        let expected = straight(&tc, &s, Inputs::Given(&inputs), 1500);
        let rb = check(&tc, &s, Inputs::Given(&inputs), &expected, jitter);
        assert_eq!(rb.deepest, 8);
    }
}

#[test]
fn bots_thinking_on_confirmed_frames_match_the_straight_run() {
    let tc = tc();
    for (s, ticks) in [
        (
            Setup {
                bots: Bots::Dumb,
                ..given(2, KILL_EM_ALL, None)
            },
            1500,
        ),
        (
            Setup {
                bots: Bots::Predictive,
                ..given(101, KILL_EM_ALL, None)
            },
            150,
        ),
    ] {
        let expected = straight(&tc, &s, Inputs::Bots, ticks);
        check(&tc, &s, Inputs::Bots, &expected, jitter);
    }
}

#[test]
fn restoring_brings_back_everything_saved() {
    let tc = tc();
    let s = given(9, KILL_EM_ALL, None);
    let inputs = record(&tc, &s, 300);
    let mut run = Run::new(&tc, &s);
    for t in 1..=200 {
        run.confirmed(t, Inputs::Given(&inputs));
    }
    let saved = run.game.snapshot();
    let at = hash_game(&mut run.game);
    for t in 201..=300 {
        run.confirmed(t, Inputs::Given(&inputs));
    }
    assert_ne!(hash_game(&mut run.game), at);

    run.game.restore(&saved);
    assert_eq!(hash_game(&mut run.game), at);
    assert_eq!(format!("{:?}", run.game.snapshot()), format!("{saved:?}"));
    // A quiet frame plays no sound and records no stats.
    let (sounds, stats) = (run.sounds.borrow().len(), hash_stats(run.game.stats()));
    for _ in 0..100 {
        run.game.process_frame_quietly();
    }
    assert_eq!(
        (run.sounds.borrow().len(), hash_stats(run.game.stats())),
        (sounds, stats)
    );
}

/// What rollback costs, in a release build:
/// `cargo test --release rollback_costs -- --ignored --nocapture`.
#[test]
#[ignore]
fn rollback_costs() {
    use std::time::Instant;
    let tc = tc();
    let s = given(13, KILL_EM_ALL, Some(&THROWERS));
    let inputs = record(&tc, &s, 1000);
    let mut run = Run::new(&tc, &s);
    for t in 1..=1000 {
        run.confirmed(t, Inputs::Given(&inputs));
    }
    let mut saved = run.game.snapshot();
    let n = 2000;
    let time = |f: &mut dyn FnMut()| {
        let start = Instant::now();
        for _ in 0..n {
            f();
        }
        start.elapsed().as_secs_f64() * 1e6 / f64::from(n)
    };
    let save = time(&mut || run.game.save(&mut saved));
    let restore = time(&mut || run.game.restore(&saved));
    let snapshot = time(&mut || drop(run.game.snapshot()));
    let tick = time(&mut || {
        run.game.restore(&saved);
        run.game.process_frame_quietly();
    }) - restore;
    eprintln!(
        "snapshot {} bytes; save {save:.1} us, restore {restore:.1} us, fresh snapshot {snapshot:.1} us, quiet tick {tick:.1} us",
        saved.bytes()
    );

    for bots in [Bots::Dumb, Bots::Predictive] {
        let mut run = Run::new(
            &tc,
            &Setup {
                bots,
                ..given(101, KILL_EM_ALL, None)
            },
        );
        let (mut think, mut play) = (0.0, 0.0);
        for _ in 0..150 {
            let t0 = Instant::now();
            bot_tick(&mut run.game);
            let t1 = Instant::now();
            run.game.process_frame(&HOOKS);
            think += (t1 - t0).as_secs_f64();
            play += t1.elapsed().as_secs_f64();
        }
        eprintln!(
            "{}: thinking {:.1} us a frame, the tick {:.1} us",
            if bots == Bots::Dumb {
                "original bots"
            } else {
                "predictive bots (30 frames, 1 mutation, 3 parallels)"
            },
            think * 1e6 / 150.0,
            play * 1e6 / 150.0
        );
    }
}
