//! Worms: movement, physics, aiming, the ninja rope controls, weapon
//! changing, the laser sight, bonus pickups, dying and respawning
//! (`Worm::process` and the functions it calls in `worm.cpp`). Results have
//! to stay bit-identical to the C++ versions, which `tools/hashrun` checks.
//!
//! As in `object.rs`, C++ owns the worms. Rust gets pointers to a worm's
//! fields and a table of C++ hooks for what stays in C++: the bonus and
//! weapon object lists, sounds, healing, creating objects, the other worm
//! and stats. Rust reads and writes fields through the pointers where the
//! C++ did, since a hook can change them (healing changes health and lives,
//! and an explosion can push the worm).

use std::ffi::c_void;

use crate::level::{LieroLevel, LieroLevelCommon};
use crate::material::LieroMaterial;
use crate::math::{cos_sin, ftoi, itof, LieroIVec2};
use crate::object::{add, div, ftoi_vec, sub, LieroNinjarope};
use crate::rand::LieroRand;
use crate::weapon::{
    liero_worm_fire, liero_worm_process_weapons, LieroWeapon, LieroWormFiring, LieroWormFiringHooks,
    LieroWormWeapon, LIERO_SELECTABLE_WEAPONS, LIERO_SHOT_STEERABLE,
};

/// Indices into `reacts`: how much terrain pushes the worm in each
/// direction (`Worm::RFDown` and so on). RF_UP is ground below the worm.
const RF_DOWN: usize = 0;
const RF_LEFT: usize = 1;
const RF_UP: usize = 2;
const RF_RIGHT: usize = 3;

/// Bits of `control_states` (`Worm::Control`).
pub const LIERO_CONTROL_UP: u32 = 0;
pub const LIERO_CONTROL_DOWN: u32 = 1;
pub const LIERO_CONTROL_LEFT: u32 = 2;
pub const LIERO_CONTROL_RIGHT: u32 = 3;
pub const LIERO_CONTROL_FIRE: u32 = 4;
pub const LIERO_CONTROL_CHANGE: u32 = 5;
pub const LIERO_CONTROL_JUMP: u32 = 6;

/// `Settings::gameMode`.
pub const LIERO_GAME_KILL_EM_ALL: i32 = 0;
pub const LIERO_GAME_GAME_OF_TAG: i32 = 1;
pub const LIERO_GAME_HOLDAZONE: i32 = 2;
pub const LIERO_GAME_SCALES_OF_JUSTICE: i32 = 3;

/// A worm's state. Rust owns it, and the fields of C++'s `Worm` refer to
/// it (`liero_worm_data_new`), except its settings and AI.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroWormData {
    pub pos: LieroIVec2,
    pub vel: LieroIVec2,
    /// Where the respawn camera is, in pixels.
    pub logic_respawn: LieroIVec2,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    /// Fixed-point, 0 to 128.
    pub aiming_angle: i32,
    pub aiming_speed: i32,
    pub able_to_jump: bool,
    pub able_to_dig: bool,
    pub key_change_pressed: bool,
    pub movable: bool,
    pub animate: bool,
    pub visible: bool,
    pub ready: bool,
    /// Whether the worm has a flag.
    pub flag: bool,
    pub make_sight_green: bool,
    pub health: i32,
    pub lives: i32,
    pub kills: i32,
    /// Time as 'it' in Game of Tag.
    pub timer: i32,
    pub killed_timer: i32,
    pub current_frame: i32,
    pub flags: i32,
    pub ninjarope: LieroNinjarope,
    pub current_weapon: i32,
    pub last_killed_by_idx: i32,
    pub fire_cone: i32,
    pub leave_shell_timer: i32,
    /// 0 or 1.
    pub index: i32,
    /// Indexed by `RF_*`.
    pub reacts: [i32; 4],
    pub weapons: [LieroWormWeapon; LIERO_SELECTABLE_WEAPONS],
    /// 0 is left, 1 is right.
    pub direction: i32,
    /// One bit per `LIERO_CONTROL_*`: the controls the game sees, those of
    /// the last tick, and the ones the player holds (`LocalController`).
    pub control_states: u32,
    pub prev_control_states: u32,
    pub clean_control_states: u32,
    pub steerable_sum_x: i32,
    pub steerable_sum_y: i32,
    pub steerable_count: i32,
    /// Where the worm's stats are drawn.
    pub stats_x: i32,
}

impl Default for LieroWormData {
    /// What C++'s `Worm()` set.
    fn default() -> Self {
        let zero = LieroIVec2 { x: 0, y: 0 };
        LieroWormData {
            pos: zero,
            vel: zero,
            logic_respawn: zero,
            hotspot_x: 0,
            hotspot_y: 0,
            aiming_angle: 0,
            aiming_speed: 0,
            able_to_jump: false,
            able_to_dig: false,
            key_change_pressed: false,
            movable: true,
            animate: false,
            visible: false,
            ready: true,
            flag: false,
            make_sight_green: false,
            health: 0,
            lives: 0,
            kills: 0,
            timer: 0,
            killed_timer: 150,
            current_frame: 0,
            flags: 0,
            ninjarope: LieroNinjarope {
                out: false,
                attached: false,
                anchor: std::ptr::null_mut(),
                pos: zero,
                vel: zero,
                length: 0,
                cur_len: 0,
            },
            current_weapon: 0,
            last_killed_by_idx: -1,
            fire_cone: 0,
            leave_shell_timer: 0,
            index: 0,
            reacts: [0; 4],
            weapons: [LieroWormWeapon { kind: std::ptr::null(), ammo: 0, delay_left: 0, loading_left: 0 };
                LIERO_SELECTABLE_WEAPONS],
            direction: 0,
            control_states: 0,
            prev_control_states: 0,
            clean_control_states: 0,
            steerable_sum_x: 0,
            steerable_sum_y: 0,
            steerable_count: 0,
            stats_x: 0,
        }
    }
}

#[no_mangle]
pub extern "C" fn liero_worm_data_new() -> *mut LieroWormData {
    Box::into_raw(Box::default())
}

/// # Safety
/// `w` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_data_clone(w: &LieroWormData) -> *mut LieroWormData {
    Box::into_raw(Box::new(*w))
}

/// # Safety
/// `w` comes from `liero_worm_data_new` or `_clone`, and isn't used after.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_data_free(w: *mut LieroWormData) {
    if !w.is_null() {
        drop(Box::from_raw(w));
    }
}

/// Constants for worms from the TC (`LC(...)` and `H[...]` in C++).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroWormConstants {
    pub min_bounce_up: i32,
    pub min_bounce_down: i32,
    pub min_bounce_left: i32,
    pub min_bounce_right: i32,
    pub worm_gravity: i32,
    pub walk_vel_left: i32,
    pub max_vel_left: i32,
    pub walk_vel_right: i32,
    pub max_vel_right: i32,
    pub jump_force: i32,
    pub max_aim_vel_left: i32,
    pub aim_acc_left: i32,
    pub max_aim_vel_right: i32,
    pub aim_acc_right: i32,
    pub nr_initial_length: i32,
    pub nr_min_length: i32,
    pub nr_max_length: i32,
    pub nr_throw_vel_x: i32,
    pub nr_throw_vel_y: i32,
    pub nr_pull_vel: i32,
    pub nr_release_vel: i32,
    pub worm_fric_mult: i32,
    pub worm_fric_div: i32,
    pub worm_min_spawn_dist_last: i32,
    pub worm_min_spawn_dist_enemy: i32,
    pub worm_spawn_rect_x: i32,
    pub worm_spawn_rect_y: i32,
    pub worm_spawn_rect_w: i32,
    pub worm_spawn_rect_h: i32,
    pub aim_fric_mult: i32,
    pub aim_fric_div: i32,
    pub aim_max_right: i32,
    pub aim_min_right: i32,
    pub aim_max_left: i32,
    pub aim_min_left: i32,
    pub bonus_explode_risk: i32,
    pub bonus_health_var: i32,
    pub bonus_min_health: i32,
    /// 1-based id of the weapon that always has a laser sight.
    pub laser_weapon: i32,
    pub fall_damage_right: i32,
    pub fall_damage_down: i32,
    pub worm_float_level: i32,
    pub worm_float_power: i32,
    /// The TC's hacks (`H[...]`).
    pub fall_damage: bool,
    pub worm_float: bool,
    pub bonus_reload_only: bool,
    pub air_jump: bool,
    pub multi_jump: bool,
    pub signed_recoil: bool,
}

/// Everything else `Worm::process` reads from the game.
#[repr(C)]
pub struct LieroWormEnv {
    pub level: LieroLevel,
    pub common: LieroLevelCommon,
    pub rand: *mut LieroRand,
    pub cycles: i32,
    /// `LIERO_GAME_*`.
    pub game_mode: i32,
    /// Blood amount, in percent.
    pub blood: i32,
    /// Weapon loading time, in percent.
    pub loading_time: i32,
    /// Whether weapons can be changed while loading.
    pub load_change: bool,
    pub shadow: bool,
    /// Worms don't respawn in quick simulations.
    pub quick_sim: bool,
    /// `Settings::wormAnimTab`: frame offsets for the walking animation.
    pub anim_tab: [i32; 4],
    /// `Game::lastKilledIdx` and `Game::gotChanged`.
    pub last_killed_idx: *mut i32,
    pub got_changed: *mut bool,
}

/// A bonus from `bonus_next`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroBonusRef {
    /// False when there are no more bonuses.
    pub found: bool,
    pub x: i32,
    pub y: i32,
    /// 0 is a weapon, 1 is health.
    pub frame: i32,
    pub weapon: i32,
}

/// A steerable weapon object from `steerable_next`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroSteerable {
    /// The object's frame, which is its direction; null when there are no
    /// more objects.
    pub cur_frame: *mut i32,
    pub pos: LieroIVec2,
}

/// What C++ does for a worm. `ctx` is passed back to each function.
#[repr(C)]
pub struct LieroWormHooks {
    pub ctx: *mut c_void,
    /// The hooks for `liero_worm_fire` and `liero_worm_process_weapons`.
    /// Their `play_sound` and `create_nobject1` are used for the worm too.
    pub firing: *const LieroWormFiringHooks,
    /// The weapon a slot's `kind` points to.
    pub weapon: unsafe extern "C" fn(ctx: *mut c_void, kind: *const c_void) -> LieroWeapon,
    /// Puts weapon `weapon` (an index into the TC's weapons) with full
    /// ammo in slot `slot`.
    pub give_weapon: unsafe extern "C" fn(ctx: *mut c_void, slot: i32, weapon: i32),
    /// Starts going through the bonuses.
    pub bonuses_begin: unsafe extern "C" fn(ctx: *mut c_void),
    /// The next bonus.
    pub bonus_next: unsafe extern "C" fn(ctx: *mut c_void) -> LieroBonusRef,
    /// Frees the bonus `bonus_next` returned last.
    pub bonus_free: unsafe extern "C" fn(ctx: *mut c_void),
    /// `Game::doHealing` for the worm.
    pub heal: unsafe extern "C" fn(ctx: *mut c_void, amount: i32),
    /// `SObjectType::create` for sobject type `kind`, owned by the worm.
    pub create_sobject: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, x: i32, y: i32),
    /// `NObjectType::create2` for nobject type `kind`, owned by the worm.
    pub create_nobject2:
        unsafe extern "C" fn(ctx: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
    /// Plays sound `sound` as the worm's own sound.
    pub play_worm_sound: unsafe extern "C" fn(ctx: *mut c_void, sound: i32),
    /// Whether the worm's own sound is playing.
    pub worm_sound_playing: unsafe extern "C" fn(ctx: *mut c_void) -> bool,
    /// Stops the current weapon's looping sound.
    pub stop_loop_sound: unsafe extern "C" fn(ctx: *mut c_void),
    /// Whether (x, y) hits the other worm (`checkForWormHit`).
    pub worm_hit: unsafe extern "C" fn(ctx: *mut c_void, x: i32, y: i32) -> bool,
    /// Starts going through the worm's objects of its current weapon.
    pub steerables_begin: unsafe extern "C" fn(ctx: *mut c_void),
    /// The next of them.
    pub steerable_next: unsafe extern "C" fn(ctx: *mut c_void) -> LieroSteerable,
    /// Sets `pos` to the other worm's position, in pixels, and returns
    /// true, when there are exactly two worms.
    pub enemy_pos: unsafe extern "C" fn(ctx: *mut c_void, pos: *mut LieroIVec2) -> bool,
    /// Gives worm `idx` a kill.
    pub add_kill: unsafe extern "C" fn(ctx: *mut c_void, idx: i32),
    /// Stats after the worm died and after it respawned.
    pub after_death: unsafe extern "C" fn(ctx: *mut c_void),
    pub after_spawn: unsafe extern "C" fn(ctx: *mut c_void),
}

/// The sprite frame for a worm aiming at `aiming_angle` (fixed-point) in
/// `direction`, without the walking animation: `Worm::angleFrame`.
#[no_mangle]
pub extern "C" fn liero_worm_angle_frame(aiming_angle: i32, direction: i32) -> i32 {
    let mut x = ftoi(aiming_angle) - 12;

    if direction != 0 {
        x -= 49;
    }

    x >>= 3;
    x = x.clamp(0, 6);

    if direction != 0 {
        x = 6 - x;
    }

    x
}

/// Clamps (x, y) to (0, 0) - (max_x, max_y): `limitXY`.
fn limit_xy(p: &mut LieroIVec2, max_x: i32, max_y: i32) {
    if p.x < 0 {
        p.x = 0;
    } else if p.x > max_x {
        p.x = max_x;
    }

    if p.y < 0 {
        p.y = 0;
    }
    if p.y > max_y {
        p.y = max_y;
    }
}

/// Points around (x, y) that push the worm in each direction when they are
/// not background (`Worm::calculateReactionForce`).
const COL_POINTS: [&[(i32, i32)]; 4] = [
    &[(-1, -4), (0, -4), (1, -4)],
    &[(1, -3), (1, -2), (1, -1), (1, 0), (1, 1), (1, 2), (1, 3)],
    &[(-1, 4), (0, 4), (1, 4)],
    &[(-1, -3), (-1, -2), (-1, -1), (-1, 0), (-1, 1), (-1, 2), (-1, 3)],
];

/// Getters and setters for the worm's fields. SAFETY for all of them: the
/// pointers are valid for the call (see `liero_worm_process`), and no
/// reference outlives the statement.
macro_rules! fields {
    ($($get:ident, $set:ident: $t:ty;)*) => {
        $(
            #[allow(dead_code)]
            fn $get(&self) -> $t {
                unsafe { (*self.w).$get }
            }
            #[allow(dead_code)]
            fn $set(&self, v: $t) {
                unsafe { (*self.w).$get = v }
            }
        )*
    };
}

struct Worm<'a> {
    /// Read and written in place, field by field: a hook can change it.
    w: *mut LieroWormData,
    /// The worm's full health (`settings->health`).
    max_health: i32,
    k: &'a LieroWormConstants,
    env: &'a LieroWormEnv,
    hooks: &'a LieroWormHooks,
}

impl Worm<'_> {
    fields! {
        pos, set_pos: LieroIVec2;
        vel, set_vel: LieroIVec2;
        logic_respawn, set_logic_respawn: LieroIVec2;
        hotspot_x, set_hotspot_x: i32;
        hotspot_y, set_hotspot_y: i32;
        aiming_angle, set_aiming_angle: i32;
        aiming_speed, set_aiming_speed: i32;
        able_to_jump, set_able_to_jump: bool;
        able_to_dig, set_able_to_dig: bool;
        key_change_pressed, set_key_change_pressed: bool;
        movable, set_movable: bool;
        animate, set_animate: bool;
        visible, set_visible: bool;
        ready, set_ready: bool;
        make_sight_green, set_make_sight_green: bool;
        health, set_health: i32;
        lives, set_lives: i32;
        killed_timer, set_killed_timer: i32;
        current_frame, set_current_frame: i32;
        current_weapon, set_current_weapon: i32;
        last_killed_by_idx, set_last_killed_by_idx: i32;
        fire_cone, set_fire_cone: i32;
        direction, set_direction: i32;
        control_states, set_control_states: u32;
        steerable_sum_x, set_steerable_sum_x: i32;
        steerable_sum_y, set_steerable_sum_y: i32;
        steerable_count, set_steerable_count: i32;
    }

    fn react(&self, dir: usize) -> i32 {
        unsafe { (*self.w).reacts[dir] }
    }

    fn set_react(&self, dir: usize, v: i32) {
        unsafe { (*self.w).reacts[dir] = v }
    }

    fn add_react(&self, dir: usize, v: i32) {
        self.set_react(dir, self.react(dir) + v);
    }

    fn index(&self) -> i32 {
        unsafe { (*self.w).index }
    }

    fn weapons(&self) -> *mut LieroWormWeapon {
        unsafe { std::ptr::addr_of_mut!((*self.w).weapons) as *mut LieroWormWeapon }
    }

    fn rope(&self) -> *mut LieroNinjarope {
        unsafe { std::ptr::addr_of_mut!((*self.w).ninjarope) }
    }

    /// The current weapon slot.
    fn slot(&self) -> *mut LieroWormWeapon {
        // SAFETY: there are five slots and current_weapon is one of them.
        unsafe { self.weapons().add(self.current_weapon() as usize) }
    }

    fn available(&self) -> bool {
        unsafe { (*self.slot()).loading_left == 0 }
    }

    /// The current weapon's type.
    fn weapon(&self) -> LieroWeapon {
        unsafe { (self.hooks.weapon)(self.hooks.ctx, (*self.slot()).kind) }
    }

    fn pressed(&self, c: u32) -> bool {
        (self.control_states() >> c) & 1 != 0
    }

    fn set_control(&self, c: u32, v: bool) {
        let s = self.control_states();
        self.set_control_states(if v { s | 1 << c } else { s & !(1 << c) });
    }

    fn release(&self, c: u32) {
        self.set_control(c, false);
    }

    fn pressed_once(&self, c: u32) -> bool {
        let state = self.pressed(c);
        self.release(c);
        state
    }

    fn rand(&self, max: u32) -> u32 {
        // SAFETY: valid for the call, and no hook runs during this.
        unsafe { (*self.env.rand).below(max) }
    }

    fn rand_u32(&self) -> u32 {
        unsafe { (*self.env.rand).next_u32() }
    }

    fn play_sound(&self, sound: i32) {
        unsafe { ((*self.hooks.firing).play_sound)(self.hooks.ctx, sound) }
    }

    fn create_nobject1(&self, kind: i32, vel: LieroIVec2, pos: LieroIVec2) {
        unsafe { ((*self.hooks.firing).create_nobject1)(self.hooks.ctx, kind, vel, pos, 0) }
    }

    fn create_nobject2(&self, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2) {
        unsafe { (self.hooks.create_nobject2)(self.hooks.ctx, kind, angle, vel, pos, 0) }
    }

    fn stop_loop_sound_if_any(&self) {
        if self.weapon().loop_sound != 0 {
            unsafe { (self.hooks.stop_loop_sound)(self.hooks.ctx) }
        }
    }

    fn level_size(&self) -> (i32, i32) {
        (self.env.level.width, self.env.level.height)
    }

    fn mat(&self, x: i32, y: i32) -> LieroMaterial {
        // SAFETY: the view is valid for the call.
        unsafe { self.env.level.get() }.mat(x, y)
    }

    /// `Level::checkedMatWrap`: the material at x + y * width, or the
    /// material of colour 0 when that is outside the level.
    fn checked_mat_wrap(&self, x: i32, y: i32) -> LieroMaterial {
        // SAFETY: the views are valid for the call.
        let (lv, cv) = unsafe { (self.env.level.get(), self.env.common.get()) };
        let idx = x.wrapping_add(y.wrapping_mul(lv.width)) as u32 as usize;
        lv.materials.get(idx).copied().unwrap_or(cv.materials[0])
    }

    fn draw_dirt_effect(&self, effect: i32, x: i32, y: i32) {
        // SAFETY: the views and rand are valid for the call, and no hook
        // runs during this.
        unsafe {
            let (mut lv, cv) = (self.env.level.get(), self.env.common.get());
            lv.draw_dirt_effect(&cv, &mut *self.env.rand, effect, x, y);
        }
    }

    fn correct_shadow(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        if self.env.shadow {
            unsafe {
                let (mut lv, cv) = (self.env.level.get(), self.env.common.get());
                lv.correct_shadow(&cv, x1, y1, x2, y2);
            }
        }
    }

    fn firing(&self) -> LieroWormFiring {
        LieroWormFiring {
            pos: unsafe { std::ptr::addr_of_mut!((*self.w).pos) },
            vel: unsafe { std::ptr::addr_of_mut!((*self.w).vel) },
            aiming_angle: self.aiming_angle(),
            fire_cone: unsafe { std::ptr::addr_of_mut!((*self.w).fire_cone) },
            leave_shell_timer: unsafe { std::ptr::addr_of_mut!((*self.w).leave_shell_timer) },
            weapons: self.weapons(),
            current_weapon: self.current_weapon(),
        }
    }

    fn calculate_reaction_force(&self, new_x: i32, new_y: i32, dir: usize) {
        self.set_react(dir, 0);

        for &(dx, dy) in COL_POINTS[dir] {
            if !self.checked_mat_wrap(new_x + dx, new_y + dy).background() {
                self.add_react(dir, 1);
            }
        }
    }

    fn process_physics(&self) {
        let k = self.k;
        let mut vel = self.vel();

        if self.react(RF_UP) > 0 {
            vel.x = vel.x.wrapping_mul(k.worm_fric_mult) / k.worm_fric_div;
            self.set_vel(vel);
        }

        let abs_x = vel.x.wrapping_abs();
        let abs_y = vel.y.wrapping_abs();

        let rh = self.react(if vel.x >= 0 { RF_LEFT } else { RF_RIGHT });
        let rv = self.react(if vel.y >= 0 { RF_UP } else { RF_DOWN });
        let mbh = if vel.x > 0 { k.min_bounce_right } else { -k.min_bounce_left };
        let mbv = if vel.y > 0 { k.min_bounce_down } else { -k.min_bounce_up };

        if vel.x != 0 && rh != 0 {
            if abs_x > mbh {
                if k.fall_damage {
                    self.set_health(self.health() - k.fall_damage_right);
                } else {
                    self.play_sound(14);
                }
                vel = self.vel();
                vel.x = -vel.x / 3;
            } else {
                vel = self.vel();
                vel.x = 0;
            }
            self.set_vel(vel);
        }

        if vel.y != 0 && rv != 0 {
            if abs_y > mbv {
                if k.fall_damage {
                    self.set_health(self.health() - k.fall_damage_down);
                } else {
                    self.play_sound(14);
                }
                vel = self.vel();
                vel.y = -vel.y / 3;
            } else {
                vel = self.vel();
                vel.y = 0;
            }
            self.set_vel(vel);
        }

        if self.react(RF_UP) == 0 {
            vel.y = vel.y.wrapping_add(k.worm_gravity);
            self.set_vel(vel);
        }

        let mut pos = self.pos();
        if self.react(if vel.x >= 0 { RF_LEFT } else { RF_RIGHT }) < 2 {
            pos.x = pos.x.wrapping_add(vel.x);
        }
        if self.react(if vel.y >= 0 { RF_UP } else { RF_DOWN }) < 2 {
            pos.y = pos.y.wrapping_add(vel.y);
        }
        self.set_pos(pos);
    }

    /// Liero.exe: 291C
    fn calculate_reaction_forces(&self) {
        let k = self.k;
        let (width, height) = self.level_size();
        let mut next = add(self.pos(), self.vel());
        let mut inext = ftoi_vec(next);

        for i in 0..4 {
            self.calculate_reaction_force(inext.x, inext.y, i);

            // Yes, Liero does this in every iteration. Keep it this way.
            if inext.x < 4 {
                self.add_react(RF_RIGHT, 5);
            } else if inext.x > width - 5 {
                self.add_react(RF_LEFT, 5);
            }

            if inext.y < 5 {
                self.add_react(RF_DOWN, 5);
            } else if k.worm_float {
                if inext.y > k.worm_float_level {
                    let mut vel = self.vel();
                    vel.y = vel.y.wrapping_sub(k.worm_float_power);
                    self.set_vel(vel);
                }
            } else if inext.y > height - 6 {
                self.add_react(RF_UP, 5);
            }
        }

        // Low or no push down, push up, and push left or right: step up.
        // Then the same the other way round.
        for (low, high, step) in [(RF_DOWN, RF_UP, -1), (RF_UP, RF_DOWN, 1)] {
            if self.react(low) < 2
                && self.react(high) > 0
                && (self.react(RF_LEFT) > 0 || self.react(RF_RIGHT) > 0)
            {
                let mut pos = self.pos();
                pos.y = pos.y.wrapping_add(itof(step));
                self.set_pos(pos);
                next.y = pos.y.wrapping_add(self.vel().y);
                inext.y = ftoi(next.y);

                self.calculate_reaction_force(inext.x, inext.y, RF_LEFT);
                self.calculate_reaction_force(inext.x, inext.y, RF_RIGHT);
            }
        }
    }

    fn pick_up_bonuses(&self) {
        let k = self.k;
        let ctx = self.hooks.ctx;
        let ipos = ftoi_vec(self.pos());

        unsafe { (self.hooks.bonuses_begin)(ctx) };
        loop {
            let b = unsafe { (self.hooks.bonus_next)(ctx) };
            if !b.found {
                break;
            }
            let (bx, by) = (ftoi(b.x), ftoi(b.y));
            if !(ipos.x + 5 > bx && ipos.x - 5 < bx && ipos.y + 5 > by && ipos.y - 5 < by) {
                continue;
            }

            if b.frame == 1 {
                if self.health() < self.max_health {
                    unsafe { (self.hooks.bonus_free)(ctx) };

                    // C++ did this in unsigned arithmetic.
                    let amount = self
                        .rand(k.bonus_health_var as u32)
                        .wrapping_add(k.bonus_min_health as u32)
                        .wrapping_mul(self.max_health as u32)
                        / 100;
                    unsafe { (self.hooks.heal)(ctx, amount as i32) };
                }
            } else if b.frame == 0 {
                if self.rand(k.bonus_explode_risk as u32) > 1 {
                    let slot = self.current_weapon();

                    if !k.bonus_reload_only {
                        self.set_fire_cone(0);
                        unsafe { (self.hooks.give_weapon)(ctx, slot, b.weapon) };
                    }

                    self.play_sound(24);

                    unsafe { (self.hooks.bonus_free)(ctx) };

                    unsafe { (*self.weapons().add(slot as usize)).loading_left = 0 };
                } else {
                    unsafe {
                        (self.hooks.bonus_free)(ctx);
                        (self.hooks.create_sobject)(ctx, 0, bx, by);
                    }
                }
            }
        }
    }

    fn process_steerables(&self) {
        self.set_steerable_count(0);
        self.set_steerable_sum_x(0);
        self.set_steerable_sum_y(0);

        if self.weapon().shot_type != LIERO_SHOT_STEERABLE {
            return;
        }

        let ctx = self.hooks.ctx;
        let turn = (self.env.cycles & 1) + 1;
        unsafe { (self.hooks.steerables_begin)(ctx) };
        loop {
            let s = unsafe { (self.hooks.steerable_next)(ctx) };
            if s.cur_frame.is_null() {
                break;
            }
            // SAFETY: the hook returned a pointer to a live object.
            unsafe {
                if self.pressed(LIERO_CONTROL_LEFT) {
                    *s.cur_frame -= turn;
                }
                if self.pressed(LIERO_CONTROL_RIGHT) {
                    *s.cur_frame += turn;
                }
                *s.cur_frame &= 127; // Wrap
            }
            self.set_movable(false);

            self.set_steerable_sum_x(self.steerable_sum_x() + ftoi(s.pos.x));
            self.set_steerable_sum_y(self.steerable_sum_y() + ftoi(s.pos.y));
            self.set_steerable_count(self.steerable_count() + 1);
        }
    }

    fn process_aiming(&self) {
        let k = self.k;
        let up = self.pressed(LIERO_CONTROL_UP);
        let down = self.pressed(LIERO_CONTROL_DOWN);

        if self.aiming_speed() != 0 {
            self.set_aiming_angle(self.aiming_angle() + self.aiming_speed());

            if !up && !down {
                self.set_aiming_speed(self.aiming_speed().wrapping_mul(k.aim_fric_mult) / k.aim_fric_div);
            }

            // On the left, the max limit is the lower angle.
            let limit = |angle: i32| {
                self.set_aiming_speed(0);
                self.set_aiming_angle(itof(angle));
            };
            if self.direction() == 1 {
                if ftoi(self.aiming_angle()) > k.aim_max_right {
                    limit(k.aim_max_right);
                }
                if ftoi(self.aiming_angle()) < k.aim_min_right {
                    limit(k.aim_min_right);
                }
            } else {
                if ftoi(self.aiming_angle()) < k.aim_max_left {
                    limit(k.aim_max_left);
                }
                if ftoi(self.aiming_angle()) > k.aim_min_left {
                    limit(k.aim_min_left);
                }
            }
        }

        let rope_out = unsafe { (*self.rope()).out };
        if self.movable() && (!rope_out || !self.pressed(LIERO_CONTROL_CHANGE)) {
            // Up turns towards the top on either side, down away from it.
            for (key, dir) in [(up, 0), (down, 1)] {
                if !key {
                    continue;
                }
                if self.direction() == dir {
                    if self.aiming_speed() < k.max_aim_vel_left {
                        self.set_aiming_speed(self.aiming_speed() + k.aim_acc_left);
                    }
                } else if self.aiming_speed() > k.max_aim_vel_right {
                    self.set_aiming_speed(self.aiming_speed() - k.aim_acc_right);
                }
            }
        }
    }

    fn process_tasks(&self) {
        let k = self.k;
        let r = self.rope();

        // SAFETY: the rope pointer is valid for the call.
        unsafe {
            if self.pressed(LIERO_CONTROL_CHANGE) {
                if (*r).out {
                    if self.pressed(LIERO_CONTROL_UP) {
                        (*r).length -= k.nr_pull_vel;
                    }
                    if self.pressed(LIERO_CONTROL_DOWN) {
                        (*r).length += k.nr_release_vel;
                    }

                    if (*r).length < k.nr_min_length {
                        (*r).length = k.nr_min_length;
                    }
                    if (*r).length > k.nr_max_length {
                        (*r).length = k.nr_max_length;
                    }
                }

                if self.pressed_once(LIERO_CONTROL_JUMP) {
                    (*r).out = true;
                    (*r).attached = false;

                    self.play_sound(5);

                    (*r).pos = self.pos();
                    let dir = cos_sin(ftoi(self.aiming_angle()));
                    (*r).vel = LieroIVec2 {
                        x: dir.x.wrapping_shl(k.nr_throw_vel_x as u32),
                        y: dir.y.wrapping_shl(k.nr_throw_vel_y as u32),
                    };

                    (*r).length = k.nr_initial_length;
                }
            } else if self.pressed(LIERO_CONTROL_JUMP) {
                // Jump removes the rope and jumps.
                (*r).out = false;
                (*r).attached = false;

                if (self.react(RF_UP) > 0 || k.air_jump) && (self.able_to_jump() || k.multi_jump) {
                    let mut vel = self.vel();
                    vel.y = vel.y.wrapping_sub(k.jump_force);
                    self.set_vel(vel);
                    self.set_able_to_jump(false);
                }
            } else {
                self.set_able_to_jump(true);
            }
        }
    }

    fn fire_or_stop(&self) {
        let ctx_hooks = unsafe { &*self.hooks.firing };
        let ww = unsafe { *self.slot() };
        let w = self.weapon();

        if self.pressed(LIERO_CONTROL_FIRE)
            && !self.pressed(LIERO_CONTROL_CHANGE)
            && ww.loading_left == 0
            && ww.delay_left <= 0
        {
            unsafe { liero_worm_fire(&self.firing(), &w, self.env.rand, self.k.signed_recoil, ctx_hooks) };
        } else if w.loop_sound != 0 {
            unsafe { (self.hooks.stop_loop_sound)(self.hooks.ctx) };
        }
    }

    fn process_weapons(&self) {
        let w = self.weapon();
        unsafe {
            liero_worm_process_weapons(&self.firing(), &w, self.env.loading_time, self.env.rand, &*self.hooks.firing)
        };
    }

    fn process_sight(&self) {
        let w = self.weapon();
        let (width, height) = self.level_size();

        if !(self.available() && (w.laser_sight || w.id == self.k.laser_weapon - 1)) {
            self.set_make_sight_green(false);
            return;
        }

        let dir = cos_sin(ftoi(self.aiming_angle()));
        let pos = self.pos();
        let mut temp = LieroIVec2 {
            x: pos.x.wrapping_add(dir.x.wrapping_mul(6)),
            y: pos.y.wrapping_add(dir.y.wrapping_mul(6)).wrapping_sub(itof(1)),
        };

        loop {
            temp = add(temp, dir);
            let hit = unsafe { (self.hooks.worm_hit)(self.hooks.ctx, ftoi(temp.x), ftoi(temp.y)) };
            self.set_make_sight_green(hit);

            let go_on = temp.x >= 0
                && temp.y >= 0
                && temp.x < itof(width)
                && temp.y < itof(height)
                && self.mat(ftoi(temp.x), ftoi(temp.y)).background()
                && !hit;
            if !go_on {
                break;
            }
        }

        self.set_hotspot_x(ftoi(temp.x));
        self.set_hotspot_y(ftoi(temp.y));
    }

    fn process_weapon_change(&self) {
        if !self.key_change_pressed() {
            self.release(LIERO_CONTROL_LEFT);
            self.release(LIERO_CONTROL_RIGHT);
            self.set_key_change_pressed(true);
        }

        self.set_fire_cone(0);
        self.set_animate(false);

        self.stop_loop_sound_if_any();

        if self.available() || self.env.load_change {
            let n = LIERO_SELECTABLE_WEAPONS as i32;
            for (key, step) in [(LIERO_CONTROL_LEFT, n - 1), (LIERO_CONTROL_RIGHT, 1)] {
                if self.pressed_once(key) {
                    self.set_current_weapon((self.current_weapon() + step) % n);

                    let pos = self.pos();
                    self.set_hotspot_x(ftoi(pos.x));
                    self.set_hotspot_y(ftoi(pos.y));
                }
            }
        }
    }

    fn dig(&self) {
        let dir = cos_sin(ftoi(self.aiming_angle()));
        let twice = LieroIVec2 { x: dir.x.wrapping_mul(2), y: dir.y.wrapping_mul(2) };

        let mut dig_pos = sub(add(twice, self.pos()), LieroIVec2 { x: itof(7), y: itof(7) });

        for i in 0..2 {
            if i == 1 {
                dig_pos = add(dig_pos, twice);
            }
            let p = ftoi_vec(dig_pos);
            self.draw_dirt_effect(7, p.x, p.y);
            self.correct_shadow(p.x - 3, p.y - 3, p.x + 18, p.y + 18);
        }
    }

    fn process_movement(&self) {
        let k = self.k;

        if !self.movable() {
            return;
        }

        let left = self.pressed(LIERO_CONTROL_LEFT);
        let right = self.pressed(LIERO_CONTROL_RIGHT);

        if left && !right {
            let mut vel = self.vel();
            if vel.x > k.max_vel_left {
                vel.x = vel.x.wrapping_sub(k.walk_vel_left);
                self.set_vel(vel);
            }

            if self.direction() != 0 {
                self.set_aiming_speed(0);
                if self.aiming_angle() >= itof(64) {
                    self.set_aiming_angle(itof(128) - self.aiming_angle());
                }
                self.set_direction(0);
            }

            self.set_animate(true);
        }

        if !left && right {
            let mut vel = self.vel();
            if vel.x < k.max_vel_right {
                vel.x = vel.x.wrapping_add(k.walk_vel_right);
                self.set_vel(vel);
            }

            if self.direction() != 1 {
                self.set_aiming_speed(0);
                if self.aiming_angle() <= itof(64) {
                    self.set_aiming_angle(itof(128) - self.aiming_angle());
                }
                self.set_direction(1);
            }

            self.set_animate(true);
        }

        if left && right {
            if self.able_to_dig() {
                self.set_able_to_dig(false);
                self.dig();
            }
        } else {
            self.set_able_to_dig(true);
        }

        if !left && !right {
            // Don't animate the worm unless it is moving.
            self.set_animate(false);
        }
    }

    fn bleed(&self) {
        if self.health() >= self.max_health / 4 || self.rand(self.health().wrapping_add(6) as u32) != 0 {
            return;
        }

        if self.rand(3) == 0 {
            // This must be outside the unpredictable branch below.
            let snd = 18 + self.rand(3) as i32;
            unsafe {
                if !(self.hooks.worm_sound_playing)(self.hooks.ctx) {
                    (self.hooks.play_worm_sound)(self.hooks.ctx, snd);
                }
            }
        }

        self.create_nobject1(6, self.vel(), self.pos());
    }

    fn die(&self) {
        let ctx = self.hooks.ctx;
        let env = self.env;

        unsafe { (*self.w).leave_shell_timer = 0 };
        self.set_make_sight_green(false);

        self.stop_loop_sound_if_any();

        let death_snd = 15 + self.rand(3) as i32;
        unsafe { (self.hooks.play_worm_sound)(ctx, death_snd) };

        self.set_fire_cone(0);
        unsafe { (*self.rope()).out = false };

        if env.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            while self.health() <= 0 {
                self.set_health(self.health() + self.max_health);
                self.set_lives(self.lives() - 1);
            }
        } else {
            self.set_lives(self.lives() - 1);
        }

        // SAFETY: the game's fields are valid for the call.
        unsafe {
            let last_killed = &mut *env.last_killed_idx;
            let old_last_killed = *last_killed;
            let by = self.last_killed_by_idx();
            // In Game of Tag, 'it' doesn't change if the killer was not 'it',
            // the worm itself, unknown, or there was no 'it'.
            if env.game_mode != LIERO_GAME_GAME_OF_TAG
                || *last_killed < 0
                || by < 0
                || by == self.index()
                || by == *last_killed
            {
                *last_killed = self.index();
            }
            *env.got_changed = old_last_killed != *last_killed;
        }

        let by = self.last_killed_by_idx();
        if by >= 0 && by != self.index() {
            unsafe { (self.hooks.add_kill)(ctx, by) };
        }

        self.set_visible(false);
        self.set_killed_timer(150);

        let max = 120 * env.blood / 100;

        if max > 1 {
            for _ in 1..=max {
                let angle = self.rand(128) as i32;
                self.create_nobject2(6, angle, div(self.vel(), 3), self.pos());
            }
        }

        for i in (7..=105).step_by(14) {
            let angle = i + self.rand(14) as i32;
            self.create_nobject2(self.index(), angle, div(self.vel(), 3), self.pos());
        }

        unsafe { (self.hooks.after_death)(ctx) };

        self.release(LIERO_CONTROL_FIRE);
    }

    fn process_alive(&self) {
        self.calculate_reaction_forces();
        self.pick_up_bonuses();
        self.process_steerables();

        // process_steerables sets movable to false; does this interfere?
        if !self.movable() && !self.pressed(LIERO_CONTROL_LEFT) && !self.pressed(LIERO_CONTROL_RIGHT) {
            self.set_movable(true);
        } // 2FB1

        self.process_aiming();
        self.process_tasks();
        self.process_weapons();
        self.fire_or_stop();
        self.process_physics();
        self.process_sight();

        if self.pressed(LIERO_CONTROL_CHANGE) {
            self.process_weapon_change();
        } else {
            self.set_key_change_pressed(false);
            self.process_movement();
        }

        self.bleed();

        if self.health() <= 0 {
            self.die();
        }

        let anim_frame = if self.animate() { ((self.env.cycles & 31) >> 3) as usize } else { 0 };
        self.set_current_frame(
            liero_worm_angle_frame(self.aiming_angle(), self.direction()) + self.env.anim_tab[anim_frame],
        );
    }

    /// `checkRespawnPosition`: whether (x, y) is far enough from where the
    /// worm died (old) and from the enemy, with no rock around it.
    fn check_respawn_position(&self, enemy: LieroIVec2, old: LieroIVec2, x: i32, y: i32) -> bool {
        let k = self.k;
        let (width, height) = self.level_size();

        // Liero compares the old x itself, not its distance. Keep it.
        let delta_x = old.x;
        let delta_y = old.y - y;
        let enemy_dx = enemy.x - x;
        let enemy_dy = enemy.y - y;

        if (delta_x.abs() <= k.worm_min_spawn_dist_last && delta_y.abs() <= k.worm_min_spawn_dist_last)
            || (enemy_dx.abs() <= k.worm_min_spawn_dist_enemy && enemy_dy.abs() <= k.worm_min_spawn_dist_enemy)
        {
            return false;
        }

        let max_x = (x + 3).min(width - 1);
        let max_y = (y + 4).min(height - 1);
        let min_x = (x - 3).max(0);
        let min_y = (y - 4).max(0);

        // C++ loops with !=, so keep that.
        let mut i = min_x;
        while i != max_x {
            let mut j = min_y;
            while j != max_y {
                // The special rock respawn bug is here.
                if self.mat(i, j).rock() {
                    return false;
                }
                j += 1;
            }
            i += 1;
        }

        true
    }

    fn begin_respawn(&self) {
        let k = self.k;
        let (_, height) = self.level_size();

        let temp = ftoi_vec(self.pos());
        self.set_logic_respawn(sub(temp, LieroIVec2 { x: 80, y: 80 }));

        let mut enemy = temp;
        unsafe { (self.hooks.enemy_pos)(self.hooks.ctx, &mut enemy) };

        let mut trials = 0;
        loop {
            let x = (k.worm_spawn_rect_x as u32).wrapping_add(self.rand(k.worm_spawn_rect_w as u32)) as i32;
            let y = (k.worm_spawn_rect_y as u32).wrapping_add(self.rand(k.worm_spawn_rect_h as u32)) as i32;
            let mut pos = LieroIVec2 { x: itof(x), y: itof(y) };

            // The original didn't have + 4 in both, which seems to be done
            // in the exe and makes sense.
            while ftoi(pos.y) + 4 < height && self.mat(ftoi(pos.x), ftoi(pos.y) + 4).background() {
                pos.y += itof(1);
            }
            self.set_pos(pos);

            trials += 1;
            if trials >= 50000 {
                break;
            }
            if self.check_respawn_position(enemy, temp, ftoi(pos.x), ftoi(pos.y)) {
                break;
            }
        }

        self.set_killed_timer(-1);
    }

    fn do_respawning(&self) {
        let (width, height) = self.level_size();
        let ipos = ftoi_vec(self.pos());
        let mut lr = self.logic_respawn();

        for _ in 0..4 {
            if lr.x < ipos.x - 80 {
                lr.x += 1;
            } else if lr.x > ipos.x - 80 {
                lr.x -= 1;
            }

            if lr.y < ipos.y - 80 {
                lr.y += 1;
            } else if lr.y > ipos.y - 80 {
                lr.y -= 1;
            }
        }

        limit_xy(&mut lr, width - 158, height - 158);
        self.set_logic_respawn(lr);

        let mut dest = LieroIVec2 { x: ipos.x - 80, y: ipos.y - 80 };
        limit_xy(&mut dest, width - 158, height - 158);

        if !(lr.x < dest.x + 5 && lr.x > dest.x - 5 && lr.y < dest.y + 5 && lr.y > dest.y - 5 && self.ready()) {
            return;
        }

        self.draw_dirt_effect(0, ipos.x - 7, ipos.y - 7);
        self.correct_shadow(ipos.x - 10, ipos.y - 10, ipos.x + 11, ipos.y + 11);

        self.set_ready(false);
        self.play_sound(21);

        self.set_visible(true);
        self.set_fire_cone(0);
        self.set_vel(LieroIVec2::default());
        if self.env.game_mode != LIERO_GAME_SCALES_OF_JUSTICE {
            self.set_health(self.max_health);
        }

        // This was done at death before, but here seems to make more sense.
        if self.rand_u32() & 1 != 0 {
            self.set_aiming_angle(itof(32));
            self.set_direction(0);
        } else {
            self.set_aiming_angle(itof(96));
            self.set_direction(1);
        }

        unsafe { (self.hooks.after_spawn)(self.hooks.ctx) };
    }

    fn process_dead(&self) {
        self.set_steerable_count(0);

        if self.pressed_once(LIERO_CONTROL_FIRE) {
            self.set_ready(true);
        }

        if self.killed_timer() > 0 {
            self.set_killed_timer(self.killed_timer() - 1);
        }

        // Worms don't respawn in quick simulations.
        if self.killed_timer() == 0 && !self.env.quick_sim {
            self.begin_respawn();
        }

        if self.killed_timer() < 0 {
            self.do_respawning();
        }
    }

    fn process(&self) {
        if self.health() > self.max_health {
            self.set_health(self.max_health);
        }

        let mode = self.env.game_mode;
        if (mode != LIERO_GAME_KILL_EM_ALL && mode != LIERO_GAME_SCALES_OF_JUSTICE) || self.lives() > 0 {
            if self.visible() {
                self.process_alive();
            } else {
                self.process_dead();
            }
        }
    }
}

/// Runs a worm for one tick: C++'s `Worm::process`.
///
/// # Safety
/// `worm`, the views and the pointers in `env` must be valid for the call.
/// The hooks may change them (but not free their memory).
#[no_mangle]
pub unsafe extern "C" fn liero_worm_process(
    worm: *mut LieroWormData,
    max_health: i32,
    constants: &LieroWormConstants,
    env: &LieroWormEnv,
    hooks: &LieroWormHooks,
) {
    Worm { w: worm, max_health, k: constants, env, hooks }.process();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_frame_matches_cpp() {
        // Worm::angleFrame, written out.
        fn cpp(aiming_angle: i32, direction: i32) -> i32 {
            let mut x = ftoi(aiming_angle) - 12;
            if direction != 0 {
                x -= 49;
            }
            x >>= 3;
            if x < 0 {
                x = 0;
            } else if x > 6 {
                x = 6;
            }
            if direction != 0 {
                x = 6 - x;
            }
            x
        }
        for angle in -10..140 {
            for dir in 0..2 {
                assert_eq!(liero_worm_angle_frame(itof(angle), dir), cpp(itof(angle), dir));
            }
        }
        assert_eq!(liero_worm_angle_frame(itof(32), 0), 2);
        assert_eq!(liero_worm_angle_frame(itof(96), 1), 2);
    }

    #[test]
    fn new_worm_matches_cpp_and_copies_are_separate() {
        unsafe {
            let w = liero_worm_data_new();
            // What C++'s Worm() set.
            assert!((*w).ready && (*w).movable && !(*w).visible);
            assert_eq!((*w).killed_timer, 150);
            assert_eq!((*w).last_killed_by_idx, -1);
            assert_eq!((*w).reacts, [0; 4]);
            assert!((*w).ninjarope.anchor.is_null() && !(*w).ninjarope.out);

            (*w).health = 75;
            (*w).kills = 3;
            (*w).flags = 2;
            (*w).weapons[2].ammo = 9;
            let c = liero_worm_data_clone(&*w);
            // Every field is copied.
            assert_eq!(format!("{:?}", *c), format!("{:?}", *w));
            (*c).health = 1;
            (*c).weapons[2].ammo = 0;
            assert_eq!((*w).health, 75);
            assert_eq!((*w).weapons[2].ammo, 9);
            assert_eq!((*c).health, 1);
            liero_worm_data_free(c);
            liero_worm_data_free(w);
        }
    }

    #[test]
    fn limit_xy_clamps() {
        let mut p = LieroIVec2 { x: -3, y: 500 };
        limit_xy(&mut p, 100, 200);
        assert_eq!((p.x, p.y), (0, 200));
        let mut p = LieroIVec2 { x: 150, y: -1 };
        limit_xy(&mut p, 100, 200);
        assert_eq!((p.x, p.y), (100, 0));
    }
}
