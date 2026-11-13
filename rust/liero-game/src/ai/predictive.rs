//! The predictive bot (`FollowAI` in `ai/predictive_ai.cpp`). Each tick it
//! keeps a few candidate plans, a control choice per frame for the next
//! `aiFrames` frames, and plays each out on a copy of the game against a
//! simple model of the other worm (`SimpleAI`), scoring every frame. It
//! mutates the plans that score best, then plays the first frame of the
//! best one. New choices come from a transition model of which controls
//! follow which, learnt from its own play in its first two minutes.
//!
//! The arithmetic follows the C++ operation for operation, in doubles, so
//! the scores and so the choices are the same.

use super::path::{LevelView, PathLevel, NO_CELL};
use crate::math::{ftoi, itof, vector_length};
use crate::rand::LieroRand;

/// Game modes, as `Settings::GameMode` numbers them.
const GM_GAME_OF_TAG: u32 = 1;
const GM_HOLDAZONE: u32 = 2;

/// Control bits, as `Worm::Control` numbers them.
pub const UP: u32 = 0;
pub const DOWN: u32 = 1;
pub const LEFT: u32 = 2;
pub const RIGHT: u32 = 3;
pub const FIRE: u32 = 4;
pub const CHANGE: u32 = 5;
pub const JUMP: u32 = 6;

fn set(cs: &mut u32, control: u32, on: bool) {
    if on {
        *cs |= 1 << control;
    } else {
        *cs &= !(1 << control);
    }
}

fn pressed(cs: u32, control: u32) -> bool {
    (cs >> control) & 1 != 0
}

const PI: f64 = std::f64::consts::PI;

/// A worm weapon, as the AI reads it.
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroAiWeapon {
    pub ammo: i32,
    pub delay_left: i32,
    pub loading_left: i32,
    /// The weapon type's loading time and ammo.
    pub loading_time: i32,
    pub max_ammo: i32,
}

/// What the AI reads of a worm.
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroAiWorm {
    pub index: i32,
    pub pos: [i32; 2],
    pub aiming_angle: i32,
    pub direction: i32,
    pub visible: bool,
    pub ready: bool,
    pub health: i32,
    pub lives: i32,
    /// The health worms start with, from the worm's settings.
    pub settings_health: i32,
    pub current_weapon: i32,
    pub weapons: [LieroAiWeapon; 5],
    pub rope_out: bool,
    pub steerable_count: i32,
    pub steerable_sum: [i32; 2],
    pub control_states: u32,
}

/// What the AI reads of a game.
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroAiGame {
    pub game_mode: u32,
    pub holder_idx: i32,
    pub contender_idx: i32,
    pub contender_frames: i32,
    /// The middle of the zone (`rect.center_x()` and `center_y()`).
    pub zone_center: [i32; 2],
    pub last_killed_idx: i32,
    pub ai_frames: i32,
    pub ai_mutations: i32,
    pub ai_traces: bool,
}

/// The games the AI plays out plans on. `G` names one: the game the AI is
/// in, or a copy of it.
pub trait Games {
    type G: Copy;
    /// A copy of `g` for the AI to run, as `evaluate` made one: no sounds,
    /// stats or viewports, and `quickSim` on.
    fn clone_game(&mut self, g: Self::G) -> Self::G;
    fn free_game(&mut self, g: Self::G);
    fn process_frame(&mut self, g: Self::G);
    fn game(&mut self, g: Self::G) -> LieroAiGame;
    /// The worm at `place` in the game's list, which is its index.
    fn worm(&mut self, g: Self::G, place: i32) -> LieroAiWorm;
    fn set_controls(&mut self, g: Self::G, place: i32, controls: u32);
    fn level(&self, g: Self::G) -> LevelView<'_>;
}

// ---- Input states ----

#[derive(Clone, Copy, Debug, PartialEq)]
enum InputType {
    /// 48 states: aiming (3), moving (4), firing and jumping (4).
    MoveJumpFire,
    /// 5 states, one per weapon.
    ChangeWeapon,
    /// 3 states: the rope out, or up or down it.
    RopeUpDown,
}

/// One frame's controls, as one of 56 states (`InputState`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState(pub i32);

impl InputState {
    fn decompose(self) -> (InputType, i32, i32, i32) {
        let i = self.0;
        if i < 48 {
            return (InputType::MoveJumpFire, i >> 4, (i >> 2) & 3, i & 3);
        }
        let i = i - 48;
        if i < 5 {
            return (InputType::ChangeWeapon, i, 0, 0);
        }
        // C++ asserted the state was in range, in debug builds.
        (InputType::RopeUpDown, i - 5, 0, 0)
    }

    fn compose_change_weapon(weapon: i32) -> Self {
        InputState(48 + weapon)
    }

    fn is_neutral(self) -> bool {
        self.decompose().0 == InputType::ChangeWeapon
    }

    fn is_firing(self) -> bool {
        let (t, _, _, pc) = self.decompose();
        t == InputType::MoveJumpFire && (pc >> 1) & 1 != 0
    }
}

fn weapon_change_offset(wanted: i32, current: i32) -> i32 {
    let offset = wanted - current;
    ((offset + 2 + 5) % 5) - 2
}

/// What the next state is chosen from (`InputContext`).
#[derive(Clone, Copy, Debug, Default)]
pub struct InputContext {
    pub current_state: InputState,
    pub wanted_weapon: i32,
    pub hidden_frames: i32,
    pub facing_enemy: i32,
    pub ninjarope_out: i32,
}

const STATES: usize = 56;
const CONTEXTS: usize = STATES * 2 * 2;

impl InputContext {
    fn pack(&self) -> usize {
        let i = self.ninjarope_out;
        let i = i * 2 + self.facing_enemy;
        (i * STATES as i32 + self.current_state.0) as usize
    }

    fn unpack(idx: usize) -> (InputState, i32, i32) {
        let s = idx % STATES;
        let idx = idx / STATES;
        (InputState(s as i32), (idx % 2) as i32, (idx / 2) as i32)
    }

    /// The controls for `new_state` for the worm at `place`, which then
    /// becomes the current state.
    fn update<W: Games>(&mut self, new_state: InputState, games: &mut W, g: W::G, place: i32) -> u32 {
        let worm = games.worm(g, place);
        let mut cs = 0;

        if worm.visible && self.wanted_weapon != worm.current_weapon {
            let offset = weapon_change_offset(self.wanted_weapon, worm.current_weapon);
            set(&mut cs, LEFT, offset < 0);
            set(&mut cs, RIGHT, offset > 0);
            set(&mut cs, CHANGE, true);
        } else {
            let (t, pa, pb, pc) = new_state.decompose();
            match t {
                InputType::MoveJumpFire => {
                    set(&mut cs, UP, (pa >> 1) & 1 != 0);
                    set(&mut cs, DOWN, pa & 1 != 0);
                    set(&mut cs, LEFT, (pb >> 1) & 1 != 0);
                    set(&mut cs, RIGHT, pb & 1 != 0);
                    set(&mut cs, FIRE, (pc >> 1) & 1 != 0);
                    set(&mut cs, JUMP, pc & 1 != 0);
                }
                InputType::ChangeWeapon => {
                    if self.wanted_weapon != pa {
                        self.wanted_weapon = pa;
                        set(&mut cs, CHANGE, true);
                    }
                }
                InputType::RopeUpDown => {
                    set(&mut cs, UP, (pa >> 1) & 1 != 0);
                    set(&mut cs, DOWN, pa & 1 != 0);
                    set(&mut cs, CHANGE, true);
                    if pa == 0 {
                        set(&mut cs, JUMP, true);
                    }
                }
            }
        }

        self.current_state = new_state;
        if !worm.visible && !worm.ready {
            self.hidden_frames += 1;
        } else {
            self.hidden_frames = 0;
        }

        let other = games.worm(g, worm.index ^ 1);
        self.facing_enemy = ((other.pos[0] > worm.pos[0]) as i32 == worm.direction) as i32;
        self.ninjarope_out = worm.rope_out as i32;
        cs
    }
}

// ---- The transition model ----

/// How much each part of the score counts (`Weights`); the game only uses
/// the defaults.
#[derive(Clone, Copy, Debug)]
pub struct Weights {
    pub health: f64,
    pub aim: f64,
    pub distance: f64,
    pub ammo: f64,
    pub missile: f64,
    pub defense: f64,
    pub firing: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            health: 1.0,
            aim: 1.0,
            distance: 1.0,
            ammo: 1.0,
            missile: 1.0,
            defense: 1.3,
            firing: 1.0,
        }
    }
}

fn select<const N: usize>(n: i32, values: [f64; N]) -> f64 {
    values[n as usize]
}

/// `transToM`: the chance of a move/jump/fire state given the previous one.
#[allow(clippy::too_many_arguments)]
fn trans_to_m(
    weights: &Weights,
    p: &mut f64,
    pa: i32,
    pb: i32,
    pc: i32,
    facing_enemy: i32,
    ninjarope_out: i32,
    pa2: i32,
    pb2: i32,
    pc2: i32,
) {
    match pa {
        0 => *p *= select(pa2, [0.1, 0.45, 0.45]),
        1 => *p *= select(pa2, [0.025, 0.9, 0.075]),
        2 => *p *= select(pa2, [0.025, 0.075, 0.9]),
        _ => {}
    }
    match pb {
        0 => *p *= select(pb2, [0.05, 0.4, 0.4, 0.15]),
        1 => *p *= select(pb2, [0.005, 0.965, 0.005, 0.025]),
        2 => *p *= select(pb2, [0.005, 0.005, 0.965, 0.025]),
        3 => *p *= select(pb2, [0.1, 0.35, 0.35, 0.2]),
        _ => {}
    }

    let start_shoot_facing = 0.15 * weights.firing;
    let start_shoot_unfacing = 0.03 * weights.firing;
    let start_shoot = if facing_enemy != 0 {
        start_shoot_facing
    } else {
        start_shoot_unfacing
    };
    *p *= select((pc & 2) | ((pc2 & 2) >> 1), [1.0 - start_shoot, start_shoot, 0.4, 0.6]);

    let start_jump = if ninjarope_out != 0 { 0.015 } else { 0.1 };
    *p *= select(
        ((pc & 1) << 1) | (pc2 & 1),
        [1.0 - start_jump, start_jump, 0.999, 0.001],
    );
}

/// `TransModel`: for each context, a weight for each next state.
#[derive(Clone)]
pub struct TransModel {
    pub trans: Vec<[f64; STATES]>,
}

impl TransModel {
    fn new(weights: &Weights) -> Self {
        let mut trans = vec![[0.0; STATES]; CONTEXTS];
        for (i, row) in trans.iter_mut().enumerate() {
            let (prev, facing_enemy, ninjarope_out) = InputContext::unpack(i);
            let (t, pa, pb, pc) = prev.decompose();

            for (j, cell) in row.iter_mut().enumerate() {
                let (t2, pa2, pb2, pc2) = InputState(j as i32).decompose();
                let mut p = 1.0;

                match t {
                    InputType::MoveJumpFire => {
                        let (mut m, mut c, r) = (0.95, 0.03, 0.02);
                        c += 0.03;
                        m -= 0.03;
                        match t2 {
                            InputType::MoveJumpFire => {
                                p *= m;
                                trans_to_m(weights, &mut p, pa, pb, pc, facing_enemy, ninjarope_out, pa2, pb2, pc2);
                            }
                            InputType::ChangeWeapon => {
                                p *= c;
                                p *= 1.0 / 5.0;
                            }
                            InputType::RopeUpDown => {
                                p *= r;
                                p *= select(pa2, [0.98, 0.01, 0.01]);
                            }
                        }
                    }
                    InputType::ChangeWeapon => match t2 {
                        InputType::MoveJumpFire => {
                            p *= 0.97;
                            trans_to_m(weights, &mut p, 0, 0, 0, facing_enemy, ninjarope_out, pa2, pb2, pc2);
                        }
                        InputType::ChangeWeapon => {
                            p *= 0.001;
                            p *= 1.0 / 5.0;
                        }
                        InputType::RopeUpDown => {
                            p *= 0.029;
                            p *= select(pa2, [0.98, 0.01, 0.01]);
                        }
                    },
                    InputType::RopeUpDown => match t2 {
                        InputType::MoveJumpFire => {
                            p *= 0.95;
                            trans_to_m(weights, &mut p, 0, 0, 0, facing_enemy, ninjarope_out, pa2, pb2, pc2);
                        }
                        InputType::ChangeWeapon => {
                            p *= 0.049;
                            p *= 1.0 / 5.0;
                        }
                        InputType::RopeUpDown => {
                            p *= 0.001;
                            p *= select(pa2, [0.98, 0.01, 0.01]);
                        }
                    },
                }
                *cell = p;
            }

            // The C++ printed any row that didn't add up to about 1; none
            // does that.
            let sum: f64 = row.iter().fold(0.0, |a, &b| a + b);
            if !(0.999..=1.0001).contains(&sum) {
                println!("{i}: {sum:.6}");
            }
        }
        TransModel { trans }
    }

    fn update(&mut self, context: &InputContext, v: InputState) {
        self.trans[context.pack()][v.0 as usize] += 0.005;
    }

    /// A next state, with the row's weights as chances (`Model::random`).
    fn random(&self, context: &InputContext, rand: &mut LieroRand) -> InputState {
        let v = &self.trans[context.pack()];
        let max = v.iter().fold(0.0, |a, &b| a + b);
        let mut el = rand.double_below(max);
        for (i, &w) in v.iter().enumerate() {
            el -= w;
            if el < 0.0 {
                return InputState(i as i32);
            }
        }
        InputState(STATES as i32 - 1)
    }
}

// ---- Scoring ----

fn max(a: f64, b: f64) -> f64 {
    // std::max returns its first argument unless the second is larger.
    if a < b {
        b
    } else {
        a
    }
}

fn total_health(w: &LieroAiWorm) -> f64 {
    if w.lives == 0 {
        return -10000.0;
    }
    let mut h = w.health.max(0);
    if !w.visible {
        h = w.settings_health;
    }
    w.lives as f64 * (w.settings_health as f64 * 5.0 / 4.0) + h as f64
}

fn total_health_norm(w: &LieroAiWorm) -> f64 {
    total_health(w) * 100.0 / w.settings_health as f64
}

fn total_ammo_worth(w: &LieroAiWorm) -> f64 {
    let mut worth = 0.0;
    for weap in &w.weapons {
        if weap.loading_left > 0 {
            worth += 3.0 - (weap.loading_left as f64 * 3.0) / weap.loading_time as f64;
        } else {
            worth += 3.0 + (weap.ammo as f64 * 3.0) / weap.max_ammo as f64;
        }
    }
    worth
}

fn ready_weapons(w: &LieroAiWorm) -> i32 {
    w.weapons
        .iter()
        .filter(|weap| weap.loading_left == 0 && weap.delay_left < 70)
        .count() as i32
}

fn worm_distance(from: &LieroAiWorm, to: &LieroAiWorm) -> i32 {
    vector_length(ftoi(to.pos[0]) - ftoi(from.pos[0]), ftoi(to.pos[1]) - ftoi(from.pos[1]))
}

/// An aiming angle, fixed point 0..128, in radians.
fn langle_to_radians(langle: i32) -> f64 {
    ((langle + itof(32)) & (itof(128) - 1)) as f64 * 0.04908738521234052 / 65536.0
}

fn normalized_langle(langle: i32) -> i32 {
    if langle < itof(64) {
        itof(128) - langle
    } else {
        langle
    }
}

fn radian_diff(a: f64, b: f64) -> f64 {
    let mut d = b - a;
    while d < -PI {
        d += 2.0 * PI;
    }
    while d > PI {
        d -= 2.0 * PI;
    }
    d
}

/// How far `from` aims from `to`, past a tolerance.
fn aiming_diff_worms(from: &LieroAiWorm, to: &LieroAiWorm) -> f64 {
    let xo = (to.pos[0] - from.pos[0]).abs() as f64;
    let yo = (to.pos[1] - from.pos[1]) as f64;
    let angle_to_target = if xo != 0.0 || yo != 0.0 { yo.atan2(xo) } else { 0.0 };
    let current_aim = langle_to_radians(normalized_langle(from.aiming_angle));
    let tolerance = PI / 8.0;
    let aim_diff = radian_diff(angle_to_target, current_aim).abs();
    max(aim_diff - tolerance, 0.0) / 6.0
}

fn obstacle(level: &LevelView, x: f64, y: f64) -> i32 {
    let m = level.material(level.checked_pixel_wrap(x as i32, y as i32));
    if !m.background() {
        if m.dirt() {
            2
        } else {
            3
        }
    } else {
        0
    }
}

/// Rock and dirt between two pixels, sampled every 2 pixels.
fn obstacles_between(level: &LevelView, from: (i32, i32), to: (i32, i32)) -> i32 {
    let org = (from.0 as f64, from.1 as f64);
    let mut dir = (to.0 as f64, to.1 as f64);
    dir.0 -= org.0;
    dir.1 -= org.1;
    let l = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
    dir.0 /= l;
    dir.1 /= l;

    let mut obst = 0;
    let mut d = 0.0;
    while d < l {
        obst += obstacle(level, org.0 + dir.0 * d, org.1 + dir.1 * d);
        d += 2.0;
    }
    obst
}

/// The same between two worms, from their fixed-point positions.
fn obstacles_worms(level: &LevelView, from: &LieroAiWorm, to: &LieroAiWorm) -> i32 {
    let org_x = from.pos[0] as f64 / 65536.0;
    let org_y = from.pos[1] as f64 / 65536.0;
    let mut dir_x = (to.pos[0] - from.pos[0]) as f64 / 65536.0;
    let mut dir_y = (to.pos[1] - from.pos[1]) as f64 / 65536.0;
    let l = (dir_x * dir_x + dir_y * dir_y).sqrt();
    dir_x /= l;
    dir_y /= l;

    let mut obst = 0;
    let mut d = 0.0;
    while d < l {
        obst += obstacle(level, org_x + dir_x * d, org_y + dir_y * d);
        d += 2.0;
    }
    obst
}

fn sigmoid(x: f64) -> f64 {
    // musl's exp, which the recorded hashes used, on every target.
    1.0 / (1.0 + crate::exp::exp(-x))
}

/// 0 at 0, about 0.5 at 1 and 1 at infinity.
fn psigmoid(x: f64) -> f64 {
    (sigmoid(x) - 0.5) * 2.0
}

/// `SimpleAI`: what the AI expects the other worm to do. It holds the
/// controls it had, aims at the AI's worm and fires when it's in sight.
fn simple_ai<W: Games>(games: &mut W, g: W::G, place: i32, initial: u32) {
    let worm = games.worm(g, place);
    let target = games.worm(g, worm.index ^ 1);
    let mut cs = worm.control_states;

    if !worm.visible {
        set(&mut cs, FIRE, true);
    } else {
        let current_aim = langle_to_radians(normalized_langle(worm.aiming_angle));
        let dir_x = (target.pos[0] - worm.pos[0]).abs() as f64;
        let dir_y = (target.pos[1] - worm.pos[1]) as f64;
        let angle_to_target = dir_y.atan2(dir_x);
        let tolerance = 2.0 * PI / 32.0;
        let aim_diff = radian_diff(angle_to_target, current_aim);

        let fire =
            aim_diff >= -tolerance && aim_diff <= tolerance && obstacles_worms(&games.level(g), &worm, &target) < 4;

        cs = initial;
        set(&mut cs, DOWN, aim_diff < -tolerance);
        set(&mut cs, UP, aim_diff > tolerance);
        set(&mut cs, FIRE, fire || pressed(initial, FIRE));
        set(&mut cs, CHANGE, false);

        if pressed(cs, FIRE) && target.pos[0] < worm.pos[0] && worm.direction != 0 {
            set(&mut cs, LEFT, true);
            set(&mut cs, RIGHT, false);
        } else if pressed(cs, FIRE) && target.pos[0] > worm.pos[0] && worm.direction != 1 {
            set(&mut cs, LEFT, false);
            set(&mut cs, RIGHT, true);
        }
    }
    games.set_controls(g, place, cs);
}

/// A plan's score: each frame's change in the state's score.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluateResult {
    pub score_over_time: Vec<f64>,
    pub future_score: f64,
}

impl EvaluateResult {
    /// Later frames count less.
    pub fn weighted_score(&self) -> f64 {
        let n = self.score_over_time.len();
        let mut r = 0.0;
        for (i, score) in self.score_over_time.iter().enumerate().skip(1) {
            let weight = (n - i) as f64 / n as f64;
            r += score * weight;
        }
        r + self.future_score
    }
}

#[derive(Clone, Debug, Default)]
pub struct CandPlan {
    pub plan: Vec<InputState>,
    pub prev_result: EvaluateResult,
    pub prev_result_age: i32,
}

/// How a plan changes while it's played out (`MutationStrategy`).
#[derive(Clone, Copy)]
enum Mutation {
    /// As it is.
    Identity,
    /// New states from `start` up to `stop`.
    Range(u32, u32),
    /// Waiting frames change to weapons that are loading, when there's time
    /// to change back before it fires.
    Optimize,
}

/// A traced position, the copy's worm in fixed point, and its colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroAiTrace {
    pub x: i32,
    pub y: i32,
    pub color: u8,
}

/// `FollowAI` with the path finding of the `AiContext` it inherits. The
/// C++ also kept a grid of where its worm had been and been hurt, which
/// only code under `#if 0` read, so it's gone.
#[derive(Clone)]
pub struct FollowAi {
    pub rand: LieroRand,
    pub frame: i32,
    pub current_context: InputContext,
    pub model: TransModel,
    pub evaluation_budget: i32,
    pub evaluate_positions: Vec<LieroAiTrace>,
    pub cand_plan: Vec<CandPlan>,
    pub best: Option<usize>,
    pub testing: bool,
    pub weights: Weights,

    pub dlevel: PathLevel,
}

impl FollowAi {
    /// `candidates` plans, as many as `aiParallels`. The C++ gave the first
    /// worm's AI `testing`, which tries changing to loading weapons.
    pub fn new(weights: Weights, candidates: usize, testing: bool) -> Self {
        FollowAi {
            rand: LieroRand::default(),
            frame: 0,
            current_context: InputContext::default(),
            model: TransModel::new(&weights),
            evaluation_budget: 0,
            evaluate_positions: Vec::new(),
            cand_plan: vec![CandPlan::default(); candidates],
            best: None,
            testing,
            weights,
            dlevel: PathLevel::default(),
        }
    }

    /// The cell at a pixel, if the search from the target reaches it.
    fn path_find(&mut self, x: i32, y: i32) -> Option<u32> {
        let cell = PathLevel::cell_from_px(x, y);
        self.dlevel.run(cell).then_some(cell)
    }

    /// How far the worm aims from where the path from its cell leads: the
    /// furthest of the next 15 cells it can see.
    fn aiming_diff_path(&self, level: &LevelView, from: &LieroAiWorm, cell: Option<u32>) -> f64 {
        let cells = &self.dlevel.cells;
        let Some(cell) = cell.filter(|&c| cells[c as usize].parent != NO_CELL) else {
            return 1.0;
        };
        let orgl = PathLevel::coords_level(cell);

        let mut dir_x = 1.0;
        let mut dir_y = 0.0;
        let mut c = cell;
        let mut count = 15;
        while cells[c as usize].parent != NO_CELL && {
            let more = count > 0;
            count -= 1;
            more
        } {
            c = cells[c as usize].parent;
            let path = PathLevel::coords_level(c);
            if (path.0 != orgl.0 && path.1 != orgl.1) && obstacles_between(level, orgl, path) > 4 {
                break;
            }
            dir_x = (path.0 - orgl.0).abs() as f64;
            dir_y = (path.1 - orgl.1) as f64;
        }

        let current_aim = langle_to_radians(normalized_langle(from.aiming_angle));
        let angle_to_target = dir_y.atan2(dir_x);
        let tolerance = PI / 8.0;
        let aim_diff = radian_diff(angle_to_target, current_aim).abs();
        max(aim_diff - tolerance, 0.0) / 6.0
    }

    /// The score of the copy `g` for the worm at `me` against `target`.
    /// `org` is the game the AI is in.
    fn evaluate_state<W: Games>(&mut self, games: &mut W, g: W::G, me: i32, target: i32, org: W::G) -> f64 {
        let mut score = 0.0;
        let weights = self.weights;
        let me_w = games.worm(g, me);
        let target_w = games.worm(g, target);
        let info = games.game(g);

        let worm_cell = self.path_find(ftoi(me_w.pos[0]), ftoi(me_w.pos[1]));
        let me_org = games.worm(org, me_w.index);

        let mut len = 200.0;
        if let Some(c) = worm_cell {
            let mut optimal_dist = 10.0;
            if ready_weapons(&me_org) <= 1 {
                optimal_dist = 50.0;
            }
            let g = self.dlevel.cells[c as usize].g;
            let d = max((g as f64 / 256.0 - optimal_dist).abs() - 10.0, 0.0) * weights.distance;
            len *= psigmoid(d / 100.0);
        } else {
            len += worm_distance(&me_w, &target_w) as f64 / 10.0;
        }

        if me_org.steerable_count == 1 {
            let missile_cell = PathLevel::cell_from_px(me_w.steerable_sum[0], me_w.steerable_sum[1]);
            let missile_path = self.dlevel.run(missile_cell);
            if let (true, Some(wc)) = (missile_path, worm_cell) {
                let mg = self.dlevel.cells[missile_cell as usize].g;
                let wg = self.dlevel.cells[wc as usize].g;
                if mg < wg {
                    let closer = wg as f64 - mg as f64;
                    score += psigmoid(closer / wg as f64) * 100.0 * weights.missile;
                }
            }
        }

        let me_health = total_health_norm(&me_w);
        let target_health = total_health_norm(&target_w);
        score += me_health * weights.health * weights.defense;
        score -= target_health * weights.health;

        if info.game_mode == GM_HOLDAZONE && info.holder_idx == me_w.index {
            score -= aiming_diff_worms(&me_w, &target_w) * 2.0 * weights.aim;
        } else {
            let aim_diff = self.aiming_diff_path(&games.level(g), &me_w, worm_cell);
            score -= aim_diff * 2.0 * weights.aim;
        }

        score += total_ammo_worth(&me_w) * 2.0 * weights.ammo;

        if info.game_mode == GM_HOLDAZONE {
            let scale = if info.holder_idx != me_w.index { 1.0 } else { 1.0 / 4.0 };
            score -= len * scale;
        } else if info.game_mode == GM_GAME_OF_TAG {
            if info.last_killed_idx >= 0 && info.last_killed_idx != me_w.index {
                score += len;
            } else {
                score -= len;
            }
        } else {
            score -= len;
        }

        if info.game_mode == GM_HOLDAZONE {
            if info.holder_idx == me_w.index {
                score += 50.0;
            }
            if info.contender_idx == me_w.index {
                score += info.contender_frames as f64 * 30.0 / 70.0;
            }
            if info.holder_idx == target_w.index {
                score -= 50.0;
            }
            if info.contender_idx == target_w.index {
                score -= info.contender_frames as f64 * 30.0 / 70.0;
            }
        }
        score
    }

    /// Plays `plan`'s first `plan_size` frames out on a copy of `org`,
    /// changing them as `ms` says and making up any it doesn't have, and
    /// scores them into `result`.
    #[allow(clippy::too_many_arguments)]
    fn evaluate<W: Games>(
        &mut self,
        result: &mut EvaluateResult,
        games: &mut W,
        org: W::G,
        me: i32,
        target: i32,
        plan: &mut Vec<InputState>,
        plan_size: usize,
        ms: Mutation,
    ) {
        let copy = games.clone_game(org);
        let me_copy = games.worm(org, me).index;
        let target_copy = games.worm(org, target).index;

        let mut context = self.current_context;
        let initial = games.worm(copy, target_copy).control_states;

        let mut prev_s = self.evaluate_state(games, copy, me_copy, target_copy, org);

        if result.score_over_time.len() < plan_size + 1 {
            result.score_over_time.resize(plan_size + 1, 0.0);
        }
        result.score_over_time[0] = 0.0;

        let mut weapon_changes_left = Vec::new();
        if let Mutation::Optimize = ms {
            weapon_changes_left.resize(plan_size, 0);
            let mut changes_left = 0;
            for j in (0..plan_size.min(plan.len())).rev() {
                if plan[j].is_firing() {
                    changes_left = 0;
                } else if plan[j].is_neutral() {
                    changes_left += 1;
                }
                weapon_changes_left[j] = changes_left;
            }
        }

        let ai_traces = games.game(org).ai_traces;

        for i in 0..plan_size {
            if plan.len() <= i {
                plan.push(self.model.random(&context, &mut self.rand));
            }

            match ms {
                Mutation::Identity => {}
                Mutation::Range(start, stop) => {
                    if i as u64 >= start as u64 && (i as u64) < stop as u64 {
                        plan[i] = self.model.random(&context, &mut self.rand);
                    }
                }
                Mutation::Optimize => {
                    let w = games.worm(copy, me_copy);
                    if plan[i].is_neutral() && w.weapons[w.current_weapon as usize].loading_left == 0 {
                        for wi in 0..5 {
                            if w.weapons[wi].loading_left > 0 {
                                let distance_from_current = weapon_change_offset(wi as i32, w.current_weapon).abs();
                                let mut loaded_distance = 5;
                                for (lw, weapon) in w.weapons.iter().enumerate().take(5) {
                                    if weapon.loading_left == 0 {
                                        loaded_distance =
                                            loaded_distance.min(weapon_change_offset(lw as i32, wi as i32).abs());
                                    }
                                }
                                if distance_from_current + loaded_distance <= weapon_changes_left[i] {
                                    plan[i] = InputState::compose_change_weapon(wi as i32);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let cs = context.update(plan[i], games, copy, me_copy);
            games.set_controls(copy, me_copy, cs);
            simple_ai(games, copy, target_copy, initial);
            games.process_frame(copy);

            let s = self.evaluate_state(games, copy, me_copy, target_copy, org);

            if ai_traces {
                let t = 119 - (i * (119 - 104 + 1) / plan_size) as i32;
                let w = games.worm(copy, me_copy);
                self.evaluate_positions.push(LieroAiTrace {
                    x: w.pos[0],
                    y: w.pos[1],
                    color: t as u8,
                });
            }

            result.score_over_time[i + 1] = s - prev_s;
            prev_s = s;
        }

        games.free_game(copy);
    }

    /// Replays `candidate` with new states from a random range, mostly one
    /// that ends where its last score was lowest.
    #[allow(clippy::too_many_arguments)]
    fn mutate<W: Games>(
        &mut self,
        result: &mut EvaluateResult,
        games: &mut W,
        org: W::G,
        me: i32,
        target: i32,
        candidate: &mut Vec<InputState>,
        prev_result: &EvaluateResult,
    ) {
        let len = candidate.len() as u32;
        let s = &prev_result.score_over_time;
        // The lowest sum of a suffix.
        let mut j = (s.len() - 1) as u32;
        let mut sum = s[j as usize];
        let mut min = sum;
        let mut minj = j;
        while j > 0 {
            j -= 1;
            sum += s[j as usize];
            if sum < min {
                minj = j;
                min = sum;
            }
        }

        let (start, stop);
        if self.rand.below(8) < 7 {
            stop = self.rand.below(minj);
            start = self.rand.range(stop.max(10) - 10, stop);
        } else {
            start = self.rand.range(0, len);
            stop = self.rand.range(start, (start + 10).min(len));
        }

        let frames = games.game(org).ai_frames as usize;
        self.evaluate(
            result,
            games,
            org,
            me,
            target,
            candidate,
            frames,
            Mutation::Range(start, stop),
        );
    }

    /// One tick for the worm at `me` in `org`: the controls to press.
    pub fn process<W: Games>(&mut self, games: &mut W, org: W::G, me: i32) -> u32 {
        let me_w = games.worm(org, me);
        let target = me_w.index ^ 1;
        let target_w = games.worm(org, target);
        let info = games.game(org);

        let (mut tx, mut ty) = (ftoi(target_w.pos[0]), ftoi(target_w.pos[1]));
        if info.game_mode == GM_HOLDAZONE {
            (tx, ty) = (info.zone_center[0], info.zone_center[1]);
        }
        self.dlevel.build(&games.level(org));
        let target_cell = PathLevel::cell_from_px(tx, ty);
        self.dlevel.cells[target_cell as usize].cost = 1;
        self.dlevel.set_origin(target_cell);

        // With no plans the C++ read past its empty list.
        if self.cand_plan.is_empty() {
            self.frame += 1;
            return 0;
        }

        let frames = info.ai_frames;
        let mut best_score = f64::NEG_INFINITY;
        self.evaluate_positions.clear();
        self.evaluation_budget += (info.ai_mutations + 1) * frames;

        let first = if self.testing {
            Mutation::Optimize
        } else {
            Mutation::Identity
        };
        let mut prio = Vec::with_capacity(self.cand_plan.len());
        for i in 0..self.cand_plan.len() {
            let mut cand = std::mem::take(&mut self.cand_plan[i]);
            if cand.prev_result_age < 2 && !cand.prev_result.score_over_time.is_empty() {
            } else {
                let size = if cand.prev_result.score_over_time.is_empty() {
                    frames
                } else {
                    frames / 2
                };
                self.evaluate(
                    &mut cand.prev_result,
                    games,
                    org,
                    me,
                    target,
                    &mut cand.plan,
                    size as usize,
                    first,
                );
                self.evaluation_budget -= size;
                cand.prev_result_age = 0;
            }

            let weighted = cand.prev_result.weighted_score();
            if weighted >= best_score {
                best_score = weighted;
                self.best = Some(i);
            }
            prio.push((weighted, i));
            self.cand_plan[i] = cand;
        }

        // std::sort, which isn't stable: plans with the same score stay in
        // libc++'s order.
        crate::cpp_sort::sort(&mut prio, &|a: &(f64, usize), b: &(f64, usize)| a.0 > b.0);

        let mut cand_idx = 0;
        while self.evaluation_budget > 0 {
            let ci = prio[cand_idx].1;
            let mut candidate = self.cand_plan[ci].plan.clone();
            let prev = std::mem::take(&mut self.cand_plan[ci].prev_result);
            let mut result = EvaluateResult::default();
            self.mutate(&mut result, games, org, me, target, &mut candidate, &prev);
            self.cand_plan[ci].prev_result = prev;
            self.evaluation_budget -= frames;

            let weighted = result.weighted_score();
            if weighted > best_score {
                let cand = &mut self.cand_plan[ci];
                cand.plan = candidate;
                self.best = Some(ci);
                best_score = weighted;
                cand.prev_result = result;
                cand.prev_result_age = 0;
            } else {
                cand_idx = (cand_idx + 1) % self.cand_plan.len();
            }
        }

        // The C++ used the best plan of an earlier tick if no score beat
        // minus infinity, which only NaN scores can do.
        // An empty plan (with aiFrames 0) read past the end in C++.
        let best = self.best.unwrap_or(0);
        let first_state = self.cand_plan[best].plan.first().copied().unwrap_or_default();
        let cs = self.current_context.update(first_state, games, org, me);

        if self.frame < 70 * 2 * 60 {
            self.model.update(&self.current_context, first_state);
        }

        for p in &mut self.cand_plan {
            if !p.plan.is_empty() {
                p.plan.remove(0);
            }
            p.prev_result.score_over_time.remove(0);
            p.prev_result.score_over_time.push(0.0);
            p.prev_result_age += 1;
        }

        self.frame += 1;
        cs
    }
}

#[cfg(test)]
#[path = "predictive_tests.rs"]
mod tests;
