//! Weapons: firing (`Worm::fire`, `Weapon::fire`) and reloading
//! (`Worm::processWeapons`). Results have to stay bit-identical to the C++
//! versions, which `tools/hashrun` checks.
//!
//! Weapon objects such as bullets, grenades, mines and lasers
//! (`WObject::process` and `WObject::blowUpObject`) are in
//! `world/wobject.rs`.

use std::ffi::c_void;

use crate::math::{cos_sin, ftoi, itof, LieroIVec2};
use crate::object::{add, sub};
use crate::rand::LieroRand;

/// `v * m / d` on both components, rounding towards zero like C++.
fn mul_div(v: LieroIVec2, m: i32, d: i32) -> LieroIVec2 {
    LieroIVec2 {
        x: v.x.wrapping_mul(m) / d,
        y: v.y.wrapping_mul(m) / d,
    }
}

/// `Weapon::shotType`.
pub const LIERO_SHOT_NORMAL: i32 = 0;
pub const LIERO_SHOT_DTYPE1: i32 = 1;
pub const LIERO_SHOT_STEERABLE: i32 = 2;
pub const LIERO_SHOT_DTYPE2: i32 = 3;
pub const LIERO_SHOT_LASER: i32 = 4;

/// The weapon with this id keeps a laser going until it hits something. The
/// other lasers move 8 steps per tick.
pub(crate) const ENDLESS_LASER_ID: i32 = 28;

/// The parts of a C++ `Weapon` that Rust uses.
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

        LieroLaunch {
            vel: v,
            cur_frame,
            time_left,
        }
    }
}

/// Pointers to the fields of a C++ `Worm` that firing and reloading use.
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
pub struct LieroWormFiringHooks {
    pub ctx: *mut c_void,
    /// The weapon's launch sound, looping or not.
    pub launch_sound: unsafe fn(ctx: *mut c_void),
    /// `Weapon::fire` for the current weapon: one part of a shot.
    pub fire_part: unsafe fn(ctx: *mut c_void, angle: i32, vel: LieroIVec2, speed: i32, pos: LieroIVec2),
    /// Plays sound `sound`.
    pub play_sound: unsafe fn(ctx: *mut c_void, sound: i32),
    /// `NObjectType::create1` for nobject type `kind`, owned by the worm.
    pub create_nobject1: unsafe fn(ctx: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
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
    unsafe {
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
    unsafe {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon() -> LieroWeapon {
        LieroWeapon {
            speed: 100,
            start_frame: -1,
            color_bullets: 40,
            ..Default::default()
        }
    }

    #[test]
    fn loading_time_is_at_least_one_tick() {
        let w = LieroWeapon {
            loading_time: 3,
            ..weapon()
        };
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
        let w = LieroWeapon {
            start_frame: 0,
            shot_type: LIERO_SHOT_DTYPE1,
            ..weapon()
        };
        let mut rand = LieroRand::new(1);
        let frames: Vec<i32> = [0, 12, 20, 64, 65, 127]
            .iter()
            .map(|&a| w.launch(&mut rand, a, LieroIVec2::default(), 100).cur_frame)
            .collect();
        assert_eq!(frames, [0, 0, 1, 6, 6, 12]);
    }
}
