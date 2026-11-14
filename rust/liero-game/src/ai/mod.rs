//! The bots: the predictive AI (`predictive`, with its path finding in
//! `path`) and the original game's simpler one (`dumb`).
//!
//! The predictive AI plays its plans out on copies of the game (`sim`),
//! which run the Rust tick without sounds, stats or viewports. C++ passes
//! the game as the tick sees it and gets the controls to press.

pub mod dumb;
pub mod path;
pub mod predictive;
pub mod sim;

use crate::game::{GameFields, GameHooks, RuleSettings};
use crate::rand::Rand;
use path::{NONE, NO_CELL};
use predictive::{AiTrace, FollowAi, Weights};
use sim::AiSettings;

/// 64-bit FNV-1a over the bytes of 32-bit words, as the hash run's `Fnv`.
struct Fnv(u64);

impl Fnv {
    fn add(&mut self, v: u32) {
        for i in 0..4 {
            self.0 ^= u64::from((v >> (i * 8)) & 0xff);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn int(&mut self, v: i32) {
        self.add(v as u32);
    }

    fn double(&mut self, d: f64) {
        let bits = d.to_bits();
        self.add(bits as u32);
        self.add((bits >> 32) as u32);
    }
}

/// The hash run's hash of an AI's state (`hashAi` in `hashai.cpp`, which
/// hashed the C++ AI's fields in this order).
fn hash(ai: &FollowAi) -> u64 {
    let mut f = Fnv(0xcbf29ce484222325);
    f.int(ai.frame);
    f.add(ai.rand.x);
    f.add(ai.rand.c);
    let c = &ai.current_context;
    for v in [
        c.current_state.0,
        c.wanted_weapon,
        c.hidden_frames,
        c.facing_enemy,
        c.ninjarope_out,
        ai.evaluation_budget,
    ] {
        f.int(v);
    }
    f.add(u32::from(ai.testing));

    f.add(ai.cand_plan.len() as u32);
    for p in &ai.cand_plan {
        f.add(p.plan.len() as u32);
        for s in &p.plan {
            f.int(s.0);
        }
        f.add(p.prev_result.score_over_time.len() as u32);
        for &d in &p.prev_result.score_over_time {
            f.double(d);
        }
        f.double(p.prev_result.future_score);
        f.int(p.prev_result_age);
    }
    f.int(ai.best.map_or(-1, |b| b as i32));

    for row in &ai.model.trans {
        for &d in row {
            f.double(d);
        }
    }
    for cell in &ai.dlevel.cells {
        f.int(cell.cost);
        f.int(cell.state);
        if cell.state != NONE {
            f.int(cell.g);
            f.int(if cell.parent == NO_CELL { -1 } else { cell.parent as i32 });
        }
    }

    f.add(ai.evaluate_positions.len() as u32);
    for p in &ai.evaluate_positions {
        f.int(p.x);
        f.int(p.y);
        f.add(u32::from(p.color));
    }
    f.0
}

/// A worm's bot, as its `controller` setting picks it: the original
/// game's (1) or the predictive one (2).
pub enum WormAi {
    Dumb(Rand),
    Follow(Box<FollowAi>),
}

impl WormAi {
    /// The bot for `controller`, or none for a human (0). The predictive
    /// one has `candidates` plans (`aiParallels`); `testing` also tries
    /// changing to loading weapons, which the game gives the first worm's.
    pub fn new(controller: u32, candidates: usize, testing: bool) -> Option<Box<WormAi>> {
        let ai = match controller {
            1 => WormAi::Dumb(Rand::default()),
            2 => WormAi::Follow(Box::new(FollowAi::new(Weights::default(), candidates, testing))),
            _ => return None,
        };
        Some(Box::new(ai))
    }

    /// The bot's own generator, which the hash run seeds.
    pub fn rand(&mut self) -> &mut Rand {
        match self {
            WormAi::Dumb(r) => r,
            WormAi::Follow(f) => &mut f.rand,
        }
    }

    /// The positions the predictive bot's last plans passed through, with
    /// `aiTraces` on, drawn as debug output.
    pub(crate) fn traces(&self) -> &[AiTrace] {
        match self {
            WormAi::Dumb(_) => &[],
            WormAi::Follow(f) => &f.evaluate_positions,
        }
    }
}

impl WormAi {
    /// A hash of the predictive bot's whole state, for the hash run; 0 for
    /// the original one.
    pub fn hash(&self) -> u64 {
        match self {
            WormAi::Dumb(_) => 0,
            WormAi::Follow(f) => hash(f),
        }
    }
}

/// One tick of the worms' bots, as `LocalController` ran them: from the
/// first worm on even cycles and the second on odd ones, each setting its
/// worm's controls. `ais` are the listed worms' bots, in the same order.
/// With `hooks`, each bot's time goes into the game's stats, if any.
pub fn process_ais(
    game: &mut GameFields,
    ais: &mut [Option<&mut WormAi>],
    settings: &RuleSettings,
    ai_settings: &AiSettings,
    hooks: Option<&GameHooks>,
) {
    let n = game.worms.len();
    let phase = (game.game.cycles % 2) as usize;
    for i in 0..n {
        let place = (i + phase) % n;
        let Some(ai) = ais[place].as_deref_mut() else { continue };
        let start = hooks.map(|h| (h.hires_ticks)());
        let controls = match ai {
            WormAi::Dumb(r) => sim::process_dumb(r, game, place),
            WormAi::Follow(f) => sim::process(f, game, settings, ai_settings, place as i32),
        };
        let w = game.worm_mut(place);
        w.control_states = controls;
        let index = w.index;
        if let (Some(h), Some(start), Some(stats)) = (hooks, start, game.stats.as_deref_mut()) {
            stats.ai_process_time(index, (h.hires_ticks)().wrapping_sub(start));
        }
    }
}
