//! Weapons: firing (`Worm::fire`, `Weapon::fire`), reloading
//! (`Worm::processWeapons`) and weapon objects such as bullets, grenades,
//! mines and lasers (`WObject::process` and `WObject::blowUpObject`).
//! Results have to stay bit-identical to the C++ versions, which
//! `tools/hashrun` checks.
//!
//! As in `object.rs`, the code gets pointers to the fields and a table of
//! hooks (`world.rs`) for creating objects, hitting worms, sounds and
//! stats, and it reads and writes fields through the pointers where the C++
//! did, since a hook can change them.

use std::ffi::c_void;

use crate::level::{LieroLevel, LieroLevelCommon};
use crate::math::{cos_sin, ftoi, itof, LieroIVec2};
use crate::object::{add, div, ftoi_vec, inside, sub};
use crate::rand::LieroRand;

/// `v * m / d` on both components, rounding towards zero like C++.
fn mul_div(v: LieroIVec2, m: i32, d: i32) -> LieroIVec2 {
    LieroIVec2 { x: v.x.wrapping_mul(m) / d, y: v.y.wrapping_mul(m) / d }
}

/// `Weapon::shotType`.
pub const LIERO_SHOT_NORMAL: i32 = 0;
pub const LIERO_SHOT_DTYPE1: i32 = 1;
pub const LIERO_SHOT_STEERABLE: i32 = 2;
pub const LIERO_SHOT_DTYPE2: i32 = 3;
pub const LIERO_SHOT_LASER: i32 = 4;

/// The weapon with this id keeps a laser going until it hits something. The
/// other lasers move 8 steps per tick.
const ENDLESS_LASER_ID: i32 = 28;

/// The parts of a C++ `Weapon` that Rust uses.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroWeapon {
    pub id: i32,
    pub detect_distance: i32,
    pub affect_by_worm: bool,
    pub blow_away: i32,
    pub gravity: i32,
    pub explo_sound: i32,
    pub speed: i32,
    pub add_speed: i32,
    pub distribution: i32,
    pub parts: i32,
    pub recoil: i32,
    pub mult_speed: i32,
    pub delay: i32,
    pub loading_time: i32,
    pub ammo: i32,
    pub create_on_exp: i32,
    pub dirt_effect: i32,
    pub leave_shells: i32,
    pub leave_shell_delay: i32,
    pub play_reload_sound: bool,
    pub expl_ground: bool,
    pub fire_cone: i32,
    pub collide_with_objects: bool,
    pub bounce: i32,
    pub time_to_explo: i32,
    pub time_to_explo_v: i32,
    pub start_frame: i32,
    pub num_frames: i32,
    pub loop_anim: bool,
    pub shot_type: i32,
    pub color_bullets: i32,
    pub splinter_amount: i32,
    pub splinter_colour: i32,
    pub splinter_type: i32,
    pub splinter_scatter: i32,
    pub obj_trail_type: i32,
    pub obj_trail_delay: i32,
    pub part_trail_type: i32,
    pub part_trail_obj: i32,
    pub part_trail_delay: i32,
    /// Non-zero when the launch sound loops while firing.
    pub loop_sound: i32,
    pub laser_sight: bool,
}

/// A worm's weapon slot, laid out like C++'s `WormWeapon`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroWormWeapon {
    /// The C++ `Weapon`.
    pub kind: *const c_void,
    pub ammo: i32,
    pub delay_left: i32,
    pub loading_left: i32,
}

/// Number of weapons a worm carries (`Settings::selectableWeapons`).
pub const LIERO_SELECTABLE_WEAPONS: usize = 5;

/// How a new weapon object starts, from `Weapon::fire`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroLaunch {
    pub vel: LieroIVec2,
    pub cur_frame: i32,
    pub time_left: i32,
}

impl LieroWeapon {
    /// `Weapon::computedLoadingTime`: the reload time with the game's
    /// loading time setting (a percentage), at least 1 tick.
    pub fn computed_loading_time(&self, loading_time_percent: i32) -> i32 {
        let t = loading_time_percent.wrapping_mul(self.loading_time) / 100;
        if t == 0 {
            1
        } else {
            t
        }
    }

    /// Velocity, first frame and fuse of a shot fired at `angle` (0 to 127)
    /// with `speed` (percent) and the shooter's `vel`.
    pub fn launch(&self, rand: &mut LieroRand, angle: i32, vel: LieroIVec2, speed: i32) -> LieroLaunch {
        let mut v = add(mul_div(cos_sin(angle), speed, 100), vel);

        if self.distribution != 0 {
            let d = self.distribution;
            v.x = v.x.wrapping_add(rand.below((d * 2) as u32) as i32 - d);
            v.y = v.y.wrapping_add(rand.below((d * 2) as u32) as i32 - d);
        }

        let cur_frame = if self.start_frame >= 0 {
            match self.shot_type {
                LIERO_SHOT_NORMAL if self.loop_anim => {
                    if self.num_frames != 0 {
                        rand.below((self.num_frames + 1) as u32) as i32
                    } else {
                        rand.below(2) as i32
                    }
                }
                LIERO_SHOT_DTYPE1 => {
                    let a = if angle > 64 { angle - 1 } else { angle };
                    ((a - 12) >> 3).clamp(0, 12)
                }
                LIERO_SHOT_DTYPE2 | LIERO_SHOT_STEERABLE => angle,
                _ => 0,
            }
        } else {
            self.color_bullets - rand.below(2) as i32
        };

        let mut time_left = self.time_to_explo;
        if self.time_to_explo_v != 0 {
            time_left -= rand.below(self.time_to_explo_v as u32) as i32;
        }

        LieroLaunch { vel: v, cur_frame, time_left }
    }
}

/// The start of a shot: C++'s `Weapon::fire` without the object list and
/// stats.
pub fn liero_weapon_launch(
    w: &LieroWeapon,
    rand: &mut LieroRand,
    angle: i32,
    vel: LieroIVec2,
    speed: i32,
) -> LieroLaunch {
    w.launch(rand, angle, vel, speed)
}

/// Pointers to the fields of a C++ `Worm` that firing and reloading use.
#[repr(C)]
pub struct LieroWormFiring {
    pub pos: *mut LieroIVec2,
    pub vel: *mut LieroIVec2,
    /// Fixed-point, 0 to 128.
    pub aiming_angle: i32,
    pub fire_cone: *mut i32,
    pub leave_shell_timer: *mut i32,
    /// All five weapon slots.
    pub weapons: *mut LieroWormWeapon,
    pub current_weapon: i32,
}

/// What C++ does when a worm fires or reloads.
#[repr(C)]
pub struct LieroWormFiringHooks {
    pub ctx: *mut c_void,
    /// The weapon's launch sound, looping or not.
    pub launch_sound: unsafe extern "C" fn(ctx: *mut c_void),
    /// `Weapon::fire` for the current weapon: one part of a shot.
    pub fire_part: unsafe extern "C" fn(ctx: *mut c_void, angle: i32, vel: LieroIVec2, speed: i32, pos: LieroIVec2),
    /// Plays sound `sound`.
    pub play_sound: unsafe extern "C" fn(ctx: *mut c_void, sound: i32),
    /// `NObjectType::create1` for nobject type `kind`, owned by the worm.
    pub create_nobject1: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
}

/// Fires the worm's current weapon `w`: C++'s `Worm::fire`.
/// `signed_recoil` is the TC's hack that makes recoils of 128 and more
/// negative.
///
/// # Safety
/// The pointers in `worm` must be valid for the call.
pub unsafe fn liero_worm_fire(
    worm: &LieroWormFiring,
    w: &LieroWeapon,
    rand: *mut LieroRand,
    signed_recoil: bool,
    hooks: &LieroWormFiringHooks,
) {
    let ww = worm.weapons.add(worm.current_weapon as usize);
    let angle = ftoi(worm.aiming_angle);
    let ctx = hooks.ctx;

    (*ww).ammo -= 1;
    (*ww).delay_left = w.delay;

    *worm.fire_cone = w.fire_cone;

    let firing = sub(
        add(mul_div(cos_sin(angle), w.detect_distance + 5, 1), *worm.pos),
        LieroIVec2 { x: 0, y: itof(1) },
    );

    if w.leave_shells > 0 && (*rand).below(w.leave_shells as u32) == 0 {
        *worm.leave_shell_timer = w.leave_shell_delay;
    }

    (hooks.launch_sound)(ctx);

    let mut speed = w.speed;
    let mut firing_vel = LieroIVec2::default();

    if w.affect_by_worm {
        if speed < 100 {
            speed = 100;
        }
        firing_vel = mul_div(*worm.vel, 100, speed);
    }

    for _ in 0..w.parts.max(0) {
        (hooks.fire_part)(ctx, angle, firing_vel, speed, firing);
    }

    let mut recoil = w.recoil;
    if signed_recoil && recoil >= 128 {
        recoil -= 256;
    }

    *worm.vel = sub(*worm.vel, mul_div(cos_sin(angle), recoil, 100));
}

/// Counts down weapon delays and reloads, the fire cone and the shell
/// casing timer: C++'s `Worm::processWeapons`. `current` is the current
/// weapon's type and `loading_time_percent` the game's loading time
/// setting.
///
/// # Safety
/// The pointers in `worm` must be valid for the call.
pub unsafe fn liero_worm_process_weapons(
    worm: &LieroWormFiring,
    current: &LieroWeapon,
    loading_time_percent: i32,
    rand: *mut LieroRand,
    hooks: &LieroWormFiringHooks,
) {
    let ctx = hooks.ctx;

    for i in 0..LIERO_SELECTABLE_WEAPONS {
        let ww = worm.weapons.add(i);
        if (*ww).delay_left >= 0 {
            (*ww).delay_left -= 1;
        }
    }

    let ww = worm.weapons.add(worm.current_weapon as usize);

    if (*ww).ammo <= 0 {
        (*ww).loading_left = current.computed_loading_time(loading_time_percent);
        (*ww).ammo = current.ammo;
    }

    // The computed loading time is never 0, so this works.
    if (*ww).loading_left > 0 {
        (*ww).loading_left -= 1;
        if (*ww).loading_left <= 0 && current.play_reload_sound {
            (hooks.play_sound)(ctx, 24);
        }
    }

    if *worm.fire_cone > 0 {
        *worm.fire_cone -= 1;
    }

    if *worm.leave_shell_timer > 0 {
        *worm.leave_shell_timer -= 1;
        if *worm.leave_shell_timer <= 0 {
            let vel_y = -((*rand).below(20000) as i32);
            let vel_x = (*rand).below(16000) as i32 - 8000;
            (hooks.create_nobject1)(ctx, 7, LieroIVec2 { x: vel_x, y: vel_y }, *worm.pos, 0);
        }
    }
}

/// Pointers to the fields of one C++ `WObject`.
#[repr(C)]
pub struct LieroWObjectFields {
    pub pos: *mut LieroIVec2,
    pub vel: *mut LieroIVec2,
    pub cur_frame: *mut i32,
    pub time_left: *mut i32,
}

/// What C++ does for a `WObject`. `ctx` is passed back to each function.
#[repr(C)]
pub struct LieroWObjectHooks {
    pub ctx: *mut c_void,
    /// Whether the owner is steering the object: visible and holding up.
    pub owner_steers: unsafe extern "C" fn(ctx: *mut c_void) -> bool,
    /// `SObjectType::create` for sobject type `kind`, owned like the object.
    pub create_sobject: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, x: i32, y: i32),
    /// `NObjectType::create1` for nobject type `kind`, owned like the object.
    pub create_nobject1: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
    /// `NObjectType::create2` for nobject type `kind`, owned like the object.
    pub create_nobject2:
        unsafe extern "C" fn(ctx: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
    /// Adds `impulse` to the other objects within 2 pixels.
    pub push_objects: unsafe extern "C" fn(ctx: *mut c_void, impulse: LieroIVec2),
    /// The worm loop in `WObject::process`: hurts the worms the object hits.
    /// Returns `LIERO_HIT_*` flags.
    pub hit_worms: unsafe extern "C" fn(ctx: *mut c_void) -> u32,
    /// `WObject::blowUpObject` with the owner as the cause.
    pub blow_up: unsafe extern "C" fn(ctx: *mut c_void),
    /// Frees the object.
    pub free: unsafe extern "C" fn(ctx: *mut c_void),
    /// Whether the object is still in use.
    pub used: unsafe extern "C" fn(ctx: *mut c_void) -> bool,
}

/// `hit_worms` result: the object should explode.
pub const LIERO_HIT_EXPLODE: u32 = 1;
/// `hit_worms` result: the object should disappear.
pub const LIERO_HIT_REMOVE: u32 = 2;

/// Everything else weapon objects read from the game.
#[repr(C)]
pub struct LieroWObjectEnv {
    pub level: LieroLevel,
    pub common: LieroLevelCommon,
    pub rand: *mut LieroRand,
    pub cycles: i32,
    pub shadow: bool,
    /// `LC(SplinterLarpaVelDiv)` and `LC(SplinterCracklerVelDiv)`.
    pub larpa_vel_div: i32,
    pub crackler_vel_div: i32,
}

impl LieroWObjectEnv {
    /// Outside the level, or `Game::pixelMat` is dirt or rock.
    fn blocked(&self, x: i32, y: i32) -> bool {
        if !inside(&self.level, x, y) {
            return true;
        }
        // SAFETY: the views are valid for the call.
        let (lv, cv) = unsafe { (self.level.get(), self.common.get()) };
        cv.materials[lv.pixel(x, y) as usize].dirt_rock()
    }

    fn rand(&self, max: u32) -> u32 {
        // SAFETY: valid for the call, and no hook runs during this.
        unsafe { (*self.rand).below(max) }
    }

    fn spread(&self, d: i32) -> i32 {
        self.rand((d * 2) as u32) as i32 - d
    }
}

/// Moves a weapon object one tick: C++'s `WObject::process`, after the
/// remote-explosion check.
///
/// # Safety
/// The field pointers, views and `rand` must be valid for the call. The
/// hooks may change them, or free the object (after which this returns
/// without touching it).
pub unsafe fn liero_wobject_process(
    obj: &LieroWObjectFields,
    w: &LieroWeapon,
    env: &LieroWObjectEnv,
    hooks: &LieroWObjectHooks,
) {
    let ctx = hooks.ctx;
    let (pos, vel) = (obj.pos, obj.vel);
    let mut iter = 0;

    loop {
        iter += 1;
        let mut do_explode = false;
        let mut do_remove = false;

        *pos = add(*pos, *vel);

        if w.shot_type == LIERO_SHOT_STEERABLE {
            let dir = cos_sin(*obj.cur_frame);
            let mut new_vel = mul_div(dir, w.speed, 100);

            if (hooks.owner_steers)(ctx) {
                new_vel = add(new_vel, mul_div(dir, w.add_speed, 100));
            }

            *vel = div(add(mul_div(*vel, 8, 1), new_vel), 9);
        } else if w.shot_type == LIERO_SHOT_DTYPE2 {
            let dir = cos_sin(*obj.cur_frame);
            *vel = add(*vel, mul_div(dir, w.add_speed, 100));

            if w.distribution != 0 {
                (*vel).x = (*vel).x.wrapping_add(env.spread(w.distribution));
                (*vel).y = (*vel).y.wrapping_add(env.spread(w.distribution));
            }
        }

        if w.bounce > 0 {
            let ipos = ftoi_vec(*pos);
            let inew_pos = ftoi_vec(add(*pos, *vel));

            // TODO in C++: read the 4/5 from the EXE.
            if env.blocked(inew_pos.x, ipos.y) {
                if w.bounce != 100 {
                    (*vel).x = (*vel).x.wrapping_neg().wrapping_mul(w.bounce) / 100;
                    (*vel).y = (*vel).y.wrapping_mul(4) / 5;
                } else {
                    (*vel).x = (*vel).x.wrapping_neg();
                }
            }

            if env.blocked(ipos.x, inew_pos.y) {
                if w.bounce != 100 {
                    (*vel).y = (*vel).y.wrapping_neg().wrapping_mul(w.bounce) / 100;
                    (*vel).x = (*vel).x.wrapping_mul(4) / 5;
                } else {
                    (*vel).y = (*vel).y.wrapping_neg();
                }
            }
        }

        if w.mult_speed != 100 {
            *vel = mul_div(*vel, w.mult_speed, 100);
        }

        if w.obj_trail_type >= 0 && env.cycles % w.obj_trail_delay == 0 {
            let p = *pos;
            (hooks.create_sobject)(ctx, w.obj_trail_type, ftoi(p.x), ftoi(p.y));
        }

        if w.part_trail_obj >= 0 && env.cycles % w.part_trail_delay == 0 {
            if w.part_trail_type == 1 {
                (hooks.create_nobject1)(ctx, w.part_trail_obj, div(*vel, env.larpa_vel_div), *pos, 0);
            } else {
                let angle = env.rand(128) as i32;
                (hooks.create_nobject2)(ctx, w.part_trail_obj, angle, div(*vel, env.crackler_vel_div), *pos, 0);
            }
        }

        if w.collide_with_objects {
            (hooks.push_objects)(ctx, mul_div(*vel, w.blow_away, 100));
        }

        let inew_pos = ftoi_vec(add(*pos, *vel));

        if inew_pos.x < 0 {
            (*pos).x = 0;
        }
        if inew_pos.y < 0 {
            (*pos).y = 0;
        }
        if inew_pos.x >= env.level.width {
            (*pos).x = itof(env.level.width - 1);
        }
        if inew_pos.y >= env.level.height {
            (*pos).y = itof(env.level.height - 1);
        }

        if env.blocked(inew_pos.x, inew_pos.y) {
            if w.bounce == 0 {
                if w.expl_ground {
                    do_explode = true;
                } else {
                    *vel = LieroIVec2::default();
                }
            }
        } else {
            // The original tested gravity first, which doesn't seem like a
            // gain.
            (*vel).y = (*vel).y.wrapping_add(w.gravity);

            if w.num_frames > 0 && (env.cycles & 7) == 0 {
                let f = obj.cur_frame;
                // Without loop_anim the frames always run forwards; with
                // it they follow the direction of travel.
                if !w.loop_anim || (*vel).x > 0 {
                    *f += 1;
                    if *f > w.num_frames {
                        *f = 0;
                    }
                } else if (*vel).x < 0 {
                    *f -= 1;
                    if *f < 0 {
                        *f = w.num_frames;
                    }
                }
            }
        }

        if w.time_to_explo > 0 {
            *obj.time_left -= 1;
            if *obj.time_left < 0 {
                do_explode = true;
            }
        }

        let hit = (hooks.hit_worms)(ctx);
        if hit & LIERO_HIT_EXPLODE != 0 {
            do_explode = true;
        }
        if hit & LIERO_HIT_REMOVE != 0 {
            do_remove = true;
        }

        if do_explode {
            (hooks.blow_up)(ctx);
            return;
        } else if do_remove {
            (hooks.free)(ctx);
            return;
        }

        if !(w.shot_type == LIERO_SHOT_LASER && (hooks.used)(ctx) && (iter < 8 || w.id == ENDLESS_LASER_ID)) {
            return;
        }
    }
}

/// What C++ does when a weapon object blows up.
#[repr(C)]
pub struct LieroBlowUpHooks {
    pub ctx: *mut c_void,
    /// `SObjectType::create` for sobject type `kind` at (x, y), caused by
    /// the exploding object.
    pub create_sobject: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, x: i32, y: i32),
    /// Plays sound `sound`.
    pub play_sound: unsafe extern "C" fn(ctx: *mut c_void, sound: i32),
    /// `NObjectType::create1` for nobject type `kind`.
    pub create_nobject1: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
    /// `NObjectType::create2` for nobject type `kind`.
    pub create_nobject2:
        unsafe extern "C" fn(ctx: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
}

/// The effects of a weapon object blowing up at `pos` with `vel`: C++'s
/// `WObject::blowUpObject` after it freed the object. Uses `env.level`,
/// `env.common`, `env.rand` and `env.shadow`.
///
/// # Safety
/// The views and `rand` must be valid for the call.
pub unsafe fn liero_wobject_blow_up(
    w: &LieroWeapon,
    pos: LieroIVec2,
    vel: LieroIVec2,
    env: &LieroWObjectEnv,
    hooks: &LieroBlowUpHooks,
) {
    let ctx = hooks.ctx;
    let (x, y) = (pos.x, pos.y);

    if w.create_on_exp >= 0 {
        (hooks.create_sobject)(ctx, w.create_on_exp, ftoi(x), ftoi(y));
    }

    if w.explo_sound >= 0 {
        (hooks.play_sound)(ctx, w.explo_sound);
    }

    for _ in 0..w.splinter_amount.max(0) {
        if w.splinter_scatter == 0 {
            let angle = env.rand(128) as i32;
            let color_sub = env.rand(2) as i32;
            (hooks.create_nobject2)(
                ctx,
                w.splinter_type,
                angle,
                LieroIVec2::default(),
                pos,
                w.splinter_colour - color_sub,
            );
        } else {
            let color_sub = env.rand(2) as i32;
            (hooks.create_nobject1)(ctx, w.splinter_type, vel, pos, w.splinter_colour - color_sub);
        }
    }

    if w.dirt_effect >= 0 {
        let (ix, iy) = (ftoi(x), ftoi(y));
        let (mut lv, cv) = (env.level.get(), env.common.get());
        lv.draw_dirt_effect(&cv, &mut *env.rand, w.dirt_effect, ix - 7, iy - 7);
        if env.shadow {
            lv.correct_shadow(&cv, ix - 10, iy - 10, ix + 11, iy + 11);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon() -> LieroWeapon {
        LieroWeapon { speed: 100, start_frame: -1, color_bullets: 40, ..Default::default() }
    }

    #[test]
    fn loading_time_is_at_least_one_tick() {
        let w = LieroWeapon { loading_time: 3, ..weapon() };
        assert_eq!(w.computed_loading_time(100), 3);
        assert_eq!(w.computed_loading_time(10), 1);
        assert_eq!(w.computed_loading_time(200), 6);
    }

    #[test]
    fn launch_aims_along_the_table_and_adds_velocity() {
        let mut rand = LieroRand::new(1);
        let mut copy = rand;
        let l = weapon().launch(&mut rand, 32, LieroIVec2 { x: 5, y: 7 }, 200);
        // Entry 32 is (-65536, 0); only the colour uses the RNG here.
        assert_eq!(l.vel, LieroIVec2 { x: -131072 + 5, y: 7 });
        assert_eq!(l.cur_frame, 40 - copy.below(2) as i32);
        assert_eq!(l.time_left, 0);
    }

    #[test]
    fn dtype1_frame_follows_the_angle() {
        let w = LieroWeapon { start_frame: 0, shot_type: LIERO_SHOT_DTYPE1, ..weapon() };
        let mut rand = LieroRand::new(1);
        let frames: Vec<i32> = [0, 12, 20, 64, 65, 127]
            .iter()
            .map(|&a| w.launch(&mut rand, a, LieroIVec2::default(), 100).cur_frame)
            .collect();
        assert_eq!(frames, [0, 0, 1, 6, 6, 12]);
    }
}
