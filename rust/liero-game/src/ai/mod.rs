//! The bots: the predictive AI (`predictive`, with its path finding in
//! `path`) and the original game's simpler one (`dumb`).
//!
//! The predictive AI plays its plans out on copies of the game, which stay
//! C++ `Game`s for now. C++ passes a table of hooks to copy, run and free a
//! game, and to read what the AI needs of a game and its worms; Rust keeps
//! everything else and returns the controls to press.

pub mod dumb;
pub mod path;
pub mod predictive;

use crate::material::LieroMaterial;
use crate::rand::LieroRand;
use path::{LevelView, NONE, NO_CELL};
use predictive::{FollowAi, Games, LieroAiGame, LieroAiTrace, LieroAiWorm, Weights};
use std::ffi::c_void;

/// A game's level, for the AI to find paths and obstacles in.
#[repr(C)]
pub struct LieroAiLevel {
    pub data: *const u8,
    pub len: usize,
    pub width: i32,
    pub height: i32,
    /// The TC's 256 materials.
    pub materials: *const LieroMaterial,
}

/// What the predictive AI calls back into C++ for. A game is a `Game*`:
/// the one the AI is in, or a copy from `clone_game`.
#[repr(C)]
pub struct LieroAiHooks {
    /// A copy of the game with no sounds, stats or viewports, and
    /// `quickSim` on.
    pub clone_game: unsafe extern "C" fn(game: *mut c_void) -> *mut c_void,
    /// Frees a copy and its worms.
    pub free_game: unsafe extern "C" fn(game: *mut c_void),
    pub process_frame: unsafe extern "C" fn(game: *mut c_void),
    pub game: unsafe extern "C" fn(game: *mut c_void, out: *mut LieroAiGame),
    /// The worm at `place` in the game's list.
    pub worm: unsafe extern "C" fn(game: *mut c_void, place: i32, out: *mut LieroAiWorm),
    pub set_controls: unsafe extern "C" fn(game: *mut c_void, place: i32, controls: u32),
    pub level: unsafe extern "C" fn(game: *mut c_void, out: *mut LieroAiLevel),
}

struct Hooked<'a>(&'a LieroAiHooks);

// SAFETY, for each call: the caller of `liero_follow_ai_process` promises
// the hooks work on the games they're given, which are the one it passed
// and copies the hooks made.
impl Games for Hooked<'_> {
    type G = *mut c_void;

    fn clone_game(&mut self, g: Self::G) -> Self::G {
        unsafe { (self.0.clone_game)(g) }
    }

    fn free_game(&mut self, g: Self::G) {
        unsafe { (self.0.free_game)(g) }
    }

    fn process_frame(&mut self, g: Self::G) {
        unsafe { (self.0.process_frame)(g) }
    }

    fn game(&mut self, g: Self::G) -> LieroAiGame {
        let mut out = LieroAiGame::default();
        unsafe { (self.0.game)(g, &mut out) };
        out
    }

    fn worm(&mut self, g: Self::G, place: i32) -> LieroAiWorm {
        let mut out = LieroAiWorm::default();
        unsafe { (self.0.worm)(g, place, &mut out) };
        out
    }

    fn set_controls(&mut self, g: Self::G, place: i32, controls: u32) {
        unsafe { (self.0.set_controls)(g, place, controls) }
    }

    fn level(&self, g: Self::G) -> LevelView<'_> {
        let mut l = LieroAiLevel { data: std::ptr::null(), len: 0, width: 0, height: 0, materials: std::ptr::null() };
        unsafe {
            (self.0.level)(g, &mut l);
            // The level stays as it is while the AI looks at it: only
            // `process_frame` changes it.
            LevelView {
                data: if l.len == 0 { &[] } else { std::slice::from_raw_parts(l.data, l.len) },
                width: l.width,
                height: l.height,
                materials: std::slice::from_raw_parts(l.materials, 256),
            }
        }
    }
}

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

/// One tick for the worm at `place` in `game`: the controls to press.
///
/// # Safety
/// The hooks must work on `game` and on the copies they make of it.
#[no_mangle]
pub unsafe extern "C" fn liero_follow_ai_process(ai: &mut LieroFollowAi, game: *mut c_void, place: i32, hooks: &LieroAiHooks) -> u32 {
    ai.0.process(&mut Hooked(hooks), game, place)
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

/// One tick of the original bot: the worm's controls after it. `others`
/// are the other worms' positions, `k` the TC's `aiParams`.
///
/// # Safety
/// `others` must point to `count` positions.
#[no_mangle]
pub unsafe extern "C" fn liero_dumb_ai_process(
    rand: &mut LieroRand,
    worm: &dumb::LieroDumbAiWorm,
    others: *const [i32; 2],
    count: usize,
    k: &[[i32; 7]; 2],
) -> u32 {
    let others = if count == 0 || others.is_null() { &[][..] } else { std::slice::from_raw_parts(others, count) };
    dumb::process(rand, worm, others, k)
}
