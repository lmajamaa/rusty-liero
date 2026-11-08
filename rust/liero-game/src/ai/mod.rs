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

use crate::game::{LieroGameFields, LieroGameHooks, LieroGameSettings};
use crate::rand::LieroRand;
use path::{NONE, NO_CELL};
use predictive::{FollowAi, LieroAiTrace, Weights};
use sim::LieroAiSettings;

/// 64-bit FNV-1a over the bytes of 32-bit words, as the hash run's `Fnv`.
struct Fnv(u64);

impl Fnv {
    fn add(&mut self, v: u32) {
        for i in 0..4 {
            self.0 ^= ((v >> (i * 8)) & 0xff) as u64;
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
    for v in [c.current_state.0, c.wanted_weapon, c.hidden_frames, c.facing_enemy, c.ninjarope_out, ai.evaluation_budget] {
        f.int(v);
    }
    f.add(ai.testing as u32);

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
        f.add(p.color as u32);
    }
    f.0
}

/// A worm's bot, as its `controller` setting picks it: the original
/// game's (1) or the predictive one (2).
pub enum LieroWormAi {
    Dumb(LieroRand),
    Follow(FollowAi),
}

/// The bot for `controller`, or null for a human (0). The predictive one
/// has `candidates` plans (`aiParallels`); `testing` also tries changing
/// to loading weapons, which the game gives the first worm's.
#[no_mangle]
pub extern "C" fn liero_worm_ai_new(controller: u32, candidates: usize, testing: bool) -> *mut LieroWormAi {
    let ai = match controller {
        1 => LieroWormAi::Dumb(LieroRand::default()),
        2 => LieroWormAi::Follow(FollowAi::new(Weights::default(), candidates, testing)),
        _ => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(ai))
}

/// # Safety
/// `ai` must come from `liero_worm_ai_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_ai_free(ai: *mut LieroWormAi) {
    if !ai.is_null() {
        drop(Box::from_raw(ai));
    }
}

/// The bot's own generator, which C++ can seed.
#[no_mangle]
pub extern "C" fn liero_worm_ai_rand(ai: &mut LieroWormAi) -> &mut LieroRand {
    match ai {
        LieroWormAi::Dumb(r) => r,
        LieroWormAi::Follow(f) => &mut f.rand,
    }
}

/// The positions the predictive bot's last plans passed through, with
/// `aiTraces` on, for C++ to draw. Valid until the next tick.
#[no_mangle]
pub extern "C" fn liero_worm_ai_traces(ai: &LieroWormAi, count: &mut usize) -> *const LieroAiTrace {
    match ai {
        LieroWormAi::Dumb(_) => {
            *count = 0;
            std::ptr::NonNull::dangling().as_ptr()
        }
        LieroWormAi::Follow(f) => {
            *count = f.evaluate_positions.len();
            f.evaluate_positions.as_ptr()
        }
    }
}

/// A hash of the predictive bot's whole state, for the hash run; 0 for
/// the original one.
#[no_mangle]
pub extern "C" fn liero_worm_ai_hash(ai: &LieroWormAi) -> u64 {
    match ai {
        LieroWormAi::Dumb(_) => 0,
        LieroWormAi::Follow(f) => hash(f),
    }
}

/// One tick of the worms' bots, as `LocalController` ran them: from the
/// first worm on even cycles and the second on odd ones, each setting its
/// worm's controls. With `hooks`, each bot's time goes into the game's
/// stats, if any.
///
/// # Safety
/// The pointers in `game` and `settings` must be valid for the call, and
/// the worms' bots must come from `liero_worm_ai_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_game_process_ais(
    game: &LieroGameFields,
    settings: &LieroGameSettings,
    ai_settings: &LieroAiSettings,
    hooks: Option<&LieroGameHooks>,
) {
    let n = game.worm_count as usize;
    let phase = ((*game.game).cycles % 2) as usize;
    for i in 0..n {
        let place = (i + phase) % n;
        let w = *game.worms.add(place);
        let Some(ai) = w.ai.as_mut() else { continue };
        let start = hooks.map(|h| (h.hires_ticks)(h.ctx));
        let controls = match ai {
            LieroWormAi::Dumb(r) => sim::process_dumb(r, game, place as i32),
            LieroWormAi::Follow(f) => sim::process(f, game, settings, ai_settings, place as i32),
        };
        (*w.data).control_states = controls;
        if let (Some(h), Some(start), Some(stats)) = (hooks, start, game.stats.as_mut()) {
            stats.ai_process_time((*w.data).index, (h.hires_ticks)(h.ctx).wrapping_sub(start));
        }
    }
}
