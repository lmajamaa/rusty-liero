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

use crate::game::{LieroGameFields, LieroGameSettings};
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

/// A predictive AI for C++'s `FollowAI` to own.
pub struct LieroFollowAi(FollowAi);

/// A new predictive AI with `candidates` plans (`aiParallels`). `testing`
/// also tries changing to loading weapons; the game gives it to the first
/// worm's AI.
#[no_mangle]
pub extern "C" fn liero_follow_ai_new(candidates: usize, testing: bool) -> *mut LieroFollowAi {
    Box::into_raw(Box::new(LieroFollowAi(FollowAi::new(Weights::default(), candidates, testing))))
}

/// # Safety
/// `ai` must come from `liero_follow_ai_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_follow_ai_free(ai: *mut LieroFollowAi) {
    if !ai.is_null() {
        drop(Box::from_raw(ai));
    }
}

/// One tick for the worm at `place` in the game: the controls to press.
///
/// # Safety
/// The pointers in `game` and `settings` must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn liero_follow_ai_process(
    ai: &mut LieroFollowAi,
    game: &LieroGameFields,
    settings: &LieroGameSettings,
    ai_settings: &LieroAiSettings,
    place: i32,
) -> u32 {
    sim::process(&mut ai.0, game, settings, ai_settings, place)
}

/// The AI's own generator, which C++ can seed.
#[no_mangle]
pub extern "C" fn liero_follow_ai_rand(ai: &mut LieroFollowAi) -> &mut LieroRand {
    &mut ai.0.rand
}

/// The positions the last tick's plans passed through, with `aiTraces` on,
/// for C++ to draw. Valid until the next tick.
#[no_mangle]
pub extern "C" fn liero_follow_ai_traces(ai: &LieroFollowAi, count: &mut usize) -> *const LieroAiTrace {
    *count = ai.0.evaluate_positions.len();
    ai.0.evaluate_positions.as_ptr()
}

/// A hash of the AI's whole state, for the hash run.
#[no_mangle]
pub extern "C" fn liero_follow_ai_hash(ai: &LieroFollowAi) -> u64 {
    hash(&ai.0)
}

/// One tick of the original bot for the worm at `place`, with its own
/// generator: the worm's controls after it.
///
/// # Safety
/// The pointers in `game` must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn liero_dumb_ai_process(rand: &mut LieroRand, game: &LieroGameFields, place: i32) -> u32 {
    sim::process_dumb(rand, game, place)
}
