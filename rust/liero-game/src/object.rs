//! Object physics: blood particles (`bobject.cpp`), bonuses (`bonus.cpp`),
//! special objects such as explosions (`sobject.cpp`), non-weapon objects
//! such as shrapnel and dirt (`nobject.cpp`) and the ninja rope
//! (`ninjarope.cpp`). Results have to stay bit-identical to the C++ versions,
//! which the replay system and `tools/hashrun` check.
//!
//! C++ still owns the objects, their lists and the worms. Rust gets pointers
//! to an object's fields and a table of C++ functions ("hooks") for what
//! stays in C++: creating other objects, hitting worms, sounds and stats.
//! A hook can change any object, including the one being processed (an
//! explosion pushes nearby objects, and a new object can reuse a freed
//! slot), so Rust reads and writes fields through the pointers at the same
//! points the C++ code did, and never keeps references across a hook.

use std::ffi::c_void;

use crate::level::{LieroLevel, LieroLevelCommon};
use crate::math::{ftoi, itof, vector_length, LieroIVec2};
use crate::rand::LieroRand;

pub(crate) fn add(a: LieroIVec2, b: LieroIVec2) -> LieroIVec2 {
    LieroIVec2 { x: a.x.wrapping_add(b.x), y: a.y.wrapping_add(b.y) }
}

pub(crate) fn sub(a: LieroIVec2, b: LieroIVec2) -> LieroIVec2 {
    LieroIVec2 { x: a.x.wrapping_sub(b.x), y: a.y.wrapping_sub(b.y) }
}

/// Divides both components, rounding towards zero like C++.
pub(crate) fn div(a: LieroIVec2, d: i32) -> LieroIVec2 {
    LieroIVec2 { x: a.x.wrapping_div(d), y: a.y.wrapping_div(d) }
}

pub(crate) fn ftoi_vec(v: LieroIVec2) -> LieroIVec2 {
    LieroIVec2 { x: ftoi(v.x), y: ftoi(v.y) }
}

/// Like C++'s `Level::inside`: negative coordinates wrap to large unsigned
/// values and fail.
pub(crate) fn inside(level: &LieroLevel, x: i32, y: i32) -> bool {
    (x as u32) < (level.width as u32) && (y as u32) < (level.height as u32)
}

/// A blood particle, laid out like C++'s `BObject`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroBObject {
    pub pos: LieroIVec2,
    pub vel: LieroIVec2,
    pub color: i32,
}

impl LieroBObject {
    /// Moves the particle one tick. Returns false when it left the level or
    /// landed and coloured a pixel, so C++ frees it.
    pub fn process(
        &mut self,
        level: &LieroLevel,
        common: &LieroLevelCommon,
        rand: &mut LieroRand,
        gravity: i32,
    ) -> bool {
        self.pos = add(self.pos, self.vel);
        let ipos = ftoi_vec(self.pos);

        if !inside(level, ipos.x, ipos.y) {
            return false;
        }

        // SAFETY: the caller's views are valid for the call.
        let (mut lv, cv) = unsafe { (level.get(), common.get()) };
        let c = lv.pixel(ipos.x, ipos.y);
        let m = lv.mat(ipos.x, ipos.y);

        if m.background() {
            self.vel.y = self.vel.y.wrapping_add(gravity);
        }

        // The colours of blood and of worms. TODO in C++: read from the EXE.
        let base = if (1..=2).contains(&c) || (77..=79).contains(&c) {
            77
        } else if m.any_dirt() {
            82
        } else if m.rock() {
            85
        } else {
            return true;
        };

        let w = base + rand.below(3) as u8;
        lv.set_pixel(ipos.x, ipos.y, w, &cv);
        false
    }
}

/// Moves blood particle `obj` one tick: C++'s `BObject::process`. Returns
/// false when C++ should free it.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
#[no_mangle]
pub unsafe extern "C" fn liero_bobject_process(
    obj: &mut LieroBObject,
    level: &LieroLevel,
    common: &LieroLevelCommon,
    rand: &mut LieroRand,
    gravity: i32,
) -> bool {
    obj.process(level, common, rand, gravity)
}

/// Constants for bonuses, from the TC (`LC(Bonus...)` in C++).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroBonusConstants {
    pub gravity: i32,
    pub bounce_mul: i32,
    pub bounce_div: i32,
}

/// Moves a bonus box one tick: falling, bouncing on the ground and counting
/// down `timer`. Returns true when the timer ran out, so C++ explodes it.
/// `x` is fixed-point and must be inside the level.
///
/// # Safety
/// See `LieroLevel`.
#[no_mangle]
pub unsafe extern "C" fn liero_bonus_process(
    x: i32,
    y: &mut i32,
    vel_y: &mut i32,
    timer: &mut i32,
    level: &LieroLevel,
    constants: &LieroBonusConstants,
) -> bool {
    let lv = level.get();

    *y = y.wrapping_add(*vel_y);

    let ix = ftoi(x);
    let iy = ftoi(*y);

    if inside(level, ix, iy + 1) && lv.mat(ix, iy + 1).background() {
        *vel_y = vel_y.wrapping_add(constants.gravity);
    }

    let inew_y = ftoi(y.wrapping_add(*vel_y));
    if inew_y < 0 || inew_y >= level.height - 1 || lv.mat(ix, inew_y).dirt_rock() {
        *vel_y = -(vel_y.wrapping_mul(constants.bounce_mul)) / constants.bounce_div;

        // TODO in C++: read from the EXE.
        if vel_y.abs() < 100 {
            *vel_y = 0;
        }
    }

    *timer -= 1;
    *timer <= 0
}

/// Advances an explosion's animation one tick. Returns true when the
/// animation is over, so C++ frees the object.
#[no_mangle]
pub extern "C" fn liero_sobject_animate(
    cur_frame: &mut i32,
    anim_delay: &mut i32,
    type_anim_delay: i32,
    num_frames: i32,
) -> bool {
    *anim_delay -= 1;
    if *anim_delay <= 0 {
        *anim_delay = type_anim_delay;
        *cur_frame += 1;
        if *cur_frame > num_frames {
            return true;
        }
    }
    false
}

/// Whether integer position (px, py) is strictly within `range` of (x, y)
/// on both axes, as explosions test it.
fn in_blast(px: i32, py: i32, x: i32, y: i32, range: i32) -> bool {
    px < x + range && px > x - range && py < y + range && py > y - range
}

/// Pushes a worm at fixed-point `pos` away from an explosion at (x, y), as
/// `SObjectType::create` does. A worm already moving at 2 pixels per tick or
/// more on an axis isn't pushed on that axis. Returns false when the worm is
/// out of range; otherwise sets `power_sum`, which sets the damage.
#[no_mangle]
pub extern "C" fn liero_explosion_push_worm(
    pos: LieroIVec2,
    vel: &mut LieroIVec2,
    x: i32,
    y: i32,
    range: i32,
    blow_away: i32,
    power_sum: &mut i32,
) -> bool {
    let (wix, wiy) = (ftoi(pos.x), ftoi(pos.y));
    if !in_blast(wix, wiy, x, y, range) {
        return false;
    }

    let delta = wix - x;
    let power = range - delta.abs();
    let mut sum = power;

    // TODO in C++: read from the EXE.
    if vel.x.abs() < itof(2) {
        let push = blow_away.wrapping_mul(power);
        vel.x = if delta > 0 { vel.x.wrapping_add(push) } else { vel.x.wrapping_sub(push) };
    }

    let delta = wiy - y;
    let power = range - delta.abs();
    sum = (sum + power) / 2;

    if vel.y.abs() < itof(2) {
        let push = blow_away.wrapping_mul(power);
        vel.y = if delta > 0 { vel.y.wrapping_add(push) } else { vel.y.wrapping_sub(push) };
    }

    *power_sum = sum;
    true
}

/// The damage an explosion does to a worm, from `power_sum`.
#[no_mangle]
pub extern "C" fn liero_explosion_damage(damage: i32, power_sum: i32, range: i32) -> i32 {
    let z = damage.wrapping_mul(power_sum);
    if range != 0 {
        z / range
    } else {
        z
    }
}

/// Pushes an object that explosions affect away from an explosion at
/// (x, y). Returns whether it was in range (for chain explosions).
#[no_mangle]
pub extern "C" fn liero_explosion_push_object(
    pos: LieroIVec2,
    vel: &mut LieroIVec2,
    x: i32,
    y: i32,
    range: i32,
    blow_away: i32,
) -> bool {
    let ipos = ftoi_vec(pos);
    if !in_blast(ipos.x, ipos.y, x, y, range) {
        return false;
    }

    for (p, c, v) in [(ipos.x, x, &mut vel.x), (ipos.y, y, &mut vel.y)] {
        let delta = p - c;
        let power = range - delta.abs();
        if power > 0 {
            let push = blow_away.wrapping_mul(power);
            if delta > 0 {
                *v = v.wrapping_add(push);
            } else if delta < 0 {
                *v = v.wrapping_sub(push);
            }
        }
    }
    true
}

/// The parts of a `NObjectType` that its physics uses.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroNObjectType {
    pub gravity: i32,
    pub bounce: i32,
    pub hit_damage: i32,
    pub expl_ground: bool,
    pub blood_trail: bool,
    pub blood_trail_delay: i32,
    pub start_frame: i32,
    pub num_frames: i32,
    pub draw_on_map: bool,
    pub leave_obj: i32,
    pub leave_obj_delay: i32,
    pub time_to_explo: i32,
    pub create_on_exp: i32,
    pub dirt_effect: i32,
    pub splinter_amount: i32,
    pub splinter_colour: i32,
    pub splinter_type: i32,
}

/// Pointers to the fields of one C++ `NObject`.
#[repr(C)]
pub struct LieroNObjectFields {
    pub pos: *mut LieroIVec2,
    pub vel: *mut LieroIVec2,
    pub time_left: *mut i32,
    pub cur_frame: *mut i32,
}

/// What C++ does for an `NObject`. `ctx` is passed back to each function.
#[repr(C)]
pub struct LieroNObjectHooks {
    pub ctx: *mut c_void,
    /// `Game::createBObject`.
    pub create_bobject: unsafe extern "C" fn(ctx: *mut c_void, pos: LieroIVec2, vel: LieroIVec2),
    /// `SObjectType::create` for sobject type `kind`, owned like the object.
    pub create_sobject: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, x: i32, y: i32),
    /// `NObjectType::create2` for nobject type `kind`, owned like the object.
    pub create_nobject2:
        unsafe extern "C" fn(ctx: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
    /// Draws the object's current frame onto the level at (x, y), with the
    /// shadow corrected.
    pub draw_on_map: unsafe extern "C" fn(ctx: *mut c_void, x: i32, y: i32),
    /// The worm loop in `NObject::process`: hurts the worms the object hits
    /// and may free it. Returns true when it should explode.
    pub hit_worms: unsafe extern "C" fn(ctx: *mut c_void) -> bool,
    /// Frees the object if it is still in use.
    pub free: unsafe extern "C" fn(ctx: *mut c_void),
}

/// Everything else `NObject::process` reads from the game.
#[repr(C)]
pub struct LieroNObjectEnv {
    pub level: LieroLevel,
    pub common: LieroLevelCommon,
    pub rand: *mut LieroRand,
    pub cycles: i32,
    pub shadow: bool,
}

struct Fields<'a>(&'a LieroNObjectFields);

// SAFETY for all of these: the pointers are valid for the call (see
// `liero_nobject_process`), and no reference outlives the statement.
impl Fields<'_> {
    fn pos(&self) -> LieroIVec2 {
        unsafe { *self.0.pos }
    }
    fn set_pos(&self, v: LieroIVec2) {
        unsafe { *self.0.pos = v }
    }
    fn vel(&self) -> LieroIVec2 {
        unsafe { *self.0.vel }
    }
    fn set_vel(&self, v: LieroIVec2) {
        unsafe { *self.0.vel = v }
    }
    fn time_left(&self) -> i32 {
        unsafe { *self.0.time_left }
    }
    fn set_time_left(&self, v: i32) {
        unsafe { *self.0.time_left = v }
    }
    fn cur_frame(&self) -> i32 {
        unsafe { *self.0.cur_frame }
    }
    fn set_cur_frame(&self, v: i32) {
        unsafe { *self.0.cur_frame = v }
    }
}

impl LieroNObjectEnv {
    /// `Game::pixelMat`: the material of the pixel's colour. This reads the
    /// TC's table, not the level's material cache, as C++ does.
    fn pixel_dirt_rock(&self, x: i32, y: i32) -> bool {
        // SAFETY: the views are valid for the call.
        let (lv, cv) = unsafe { (self.level.get(), self.common.get()) };
        cv.materials[lv.pixel(x, y) as usize].dirt_rock()
    }

    fn blocked(&self, x: i32, y: i32) -> bool {
        !inside(&self.level, x, y) || self.pixel_dirt_rock(x, y)
    }

    fn rand(&self, max: u32) -> u32 {
        // SAFETY: valid for the call, and hooks don't run during this.
        unsafe { (*self.rand).below(max) }
    }
}

/// Moves a non-weapon object one tick: C++'s `NObject::process`.
///
/// # Safety
/// The field pointers, views and `rand` must be valid for the call. The
/// hooks may change them (but not free their memory).
#[no_mangle]
pub unsafe extern "C" fn liero_nobject_process(
    obj: &LieroNObjectFields,
    t: &LieroNObjectType,
    env: &LieroNObjectEnv,
    hooks: &LieroNObjectHooks,
) {
    let o = Fields(obj);
    let ctx = hooks.ctx;

    let mut bounced = false;
    let mut do_explode = false;

    o.set_pos(add(o.pos(), o.vel()));

    let mut inew_pos = ftoi_vec(add(o.pos(), o.vel()));
    let ipos = ftoi_vec(o.pos());

    if t.bounce > 0 {
        if env.blocked(inew_pos.x, ipos.y) {
            let v = o.vel();
            o.set_vel(LieroIVec2 {
                x: v.x.wrapping_neg().wrapping_mul(t.bounce) / 100,
                // TODO in C++: read from the EXE.
                y: v.y.wrapping_mul(4) / 5,
            });
            bounced = true;
        }

        if env.blocked(ipos.x, inew_pos.y) {
            let v = o.vel();
            o.set_vel(LieroIVec2 {
                x: v.x.wrapping_mul(4) / 5,
                y: v.y.wrapping_neg().wrapping_mul(t.bounce) / 100,
            });
            bounced = true;
        }
    }

    if t.blood_trail && t.blood_trail_delay > 0 && env.cycles % t.blood_trail_delay == 0 {
        // TODO in C++: read from the EXE.
        (hooks.create_bobject)(ctx, o.pos(), div(o.vel(), 4));
    }

    // Yes, again.
    inew_pos = ftoi_vec(add(o.pos(), o.vel()));

    let mut p = o.pos();
    if inew_pos.x < 0 {
        p.x = 0;
    }
    if inew_pos.y < 0 {
        p.y = 0;
    }
    if inew_pos.x >= env.level.width {
        p.x = itof(env.level.width);
    }
    if inew_pos.y >= env.level.height {
        p.y = itof(env.level.height);
    }
    o.set_pos(p);

    if env.blocked(inew_pos.x, inew_pos.y) {
        o.set_vel(LieroIVec2::default());

        if t.expl_ground {
            if t.start_frame > 0 && t.draw_on_map {
                (hooks.draw_on_map)(ctx, ipos.x - 3, ipos.y - 3);
            }
            do_explode = true;
        }
    } else {
        // `leave_obj >= 0`: not in Liero, but some TCs forget to set
        // leave_obj_delay to 0 when they don't use this trail.
        if !bounced && t.leave_obj_delay != 0 && t.leave_obj >= 0 && env.cycles % t.leave_obj_delay == 0 {
            let p = o.pos();
            (hooks.create_sobject)(ctx, t.leave_obj, ftoi(p.x), ftoi(p.y));
        }

        let v = o.vel();
        o.set_vel(LieroIVec2 { x: v.x, y: v.y.wrapping_add(t.gravity) });
    }

    // TODO in C++: read the 7 from the EXE.
    if t.num_frames > 0 && (env.cycles & 7) == 0 {
        let vx = o.vel().x;
        if vx > 0 {
            let f = o.cur_frame() + 1;
            o.set_cur_frame(if f > t.num_frames { 0 } else { f });
        } else if vx < 0 {
            let f = o.cur_frame() - 1;
            o.set_cur_frame(if f < 0 { t.num_frames } else { f });
        }
    }

    if t.time_to_explo > 0 {
        let left = o.time_left() - 1;
        o.set_time_left(left);
        if left <= 0 {
            do_explode = true;
        }
    }

    if !do_explode && t.hit_damage > 0 && (hooks.hit_worms)(ctx) {
        do_explode = true;
    }

    if do_explode {
        if t.create_on_exp >= 0 {
            let p = o.pos();
            (hooks.create_sobject)(ctx, t.create_on_exp, ftoi(p.x), ftoi(p.y));
        }

        if t.dirt_effect >= 0 {
            let p = ftoi_vec(o.pos());
            {
                let (mut lv, cv) = (env.level.get(), env.common.get());
                lv.draw_dirt_effect(&cv, &mut *env.rand, t.dirt_effect, p.x - 7, p.y - 7);
                if env.shadow {
                    lv.correct_shadow(&cv, p.x - 10, p.y - 10, p.x + 11, p.y + 11);
                }
            }
        }

        for _ in 0..t.splinter_amount.max(0) {
            let angle = env.rand(128) as i32;
            let color_sub = env.rand(2) as i32;
            (hooks.create_nobject2)(
                ctx,
                t.splinter_type,
                angle,
                LieroIVec2::default(),
                o.pos(),
                t.splinter_colour - color_sub,
            );
        }

        (hooks.free)(ctx);
    }
}

/// The ninja rope, laid out like C++'s `Ninjarope`.
#[repr(C)]
pub struct LieroNinjarope {
    pub out: bool,
    pub attached: bool,
    /// The worm the rope is attached to, or null.
    pub anchor: *mut c_void,
    pub pos: LieroIVec2,
    pub vel: LieroIVec2,
    pub length: i32,
    pub cur_len: i32,
}

/// Pointers to a worm's position and velocity.
#[repr(C)]
pub struct LieroWormMotion {
    pub pos: *mut LieroIVec2,
    pub vel: *mut LieroIVec2,
}

/// Constants for the ninja rope, from the TC (`LC(NR...)` in C++).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroNinjaropeConstants {
    pub force_shl_x: i32,
    pub force_div_x: i32,
    pub force_shl_y: i32,
    pub force_div_y: i32,
    pub force_len_shl: i32,
    pub attach_length: i32,
    pub gravity: i32,
}

/// What C++ does for the ninja rope. `ctx` is passed back to each function.
#[repr(C)]
pub struct LieroNinjaropeHooks {
    pub ctx: *mut c_void,
    /// The first worm other than the owner that (x, y) hits, or null.
    pub find_anchor: unsafe extern "C" fn(ctx: *mut c_void, x: i32, y: i32) -> *mut c_void,
    /// Where worm `worm` keeps its position and velocity.
    pub worm_motion: unsafe extern "C" fn(worm: *mut c_void) -> LieroWormMotion,
    /// `NObjectType::create2` for nobject type `kind`, owned by the owner.
    pub create_nobject2:
        unsafe extern "C" fn(ctx: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32),
}

/// Moves a worm's ninja rope one tick: C++'s `Ninjarope::process`.
///
/// # Safety
/// All pointers must be valid for the call, and `owner` must be the
/// rope's worm.
#[no_mangle]
pub unsafe extern "C" fn liero_ninjarope_process(
    rope: *mut LieroNinjarope,
    owner: LieroWormMotion,
    level: &LieroLevel,
    rand: *mut LieroRand,
    constants: &LieroNinjaropeConstants,
    hooks: &LieroNinjaropeHooks,
) {
    let r = rope;
    let k = constants;
    let ctx = hooks.ctx;

    if !(*r).out {
        return;
    }

    (*r).pos = add((*r).pos, (*r).vel);
    let ipos = ftoi_vec((*r).pos);

    (*r).anchor = (hooks.find_anchor)(ctx, ipos.x, ipos.y);

    let diff = sub((*r).pos, *owner.pos);
    let force = LieroIVec2 {
        x: diff.x.wrapping_shl(k.force_shl_x as u32) / k.force_div_x,
        y: diff.y.wrapping_shl(k.force_shl_y as u32) / k.force_div_y,
    };

    (*r).cur_len = (vector_length(ftoi(diff.x), ftoi(diff.y)) + 1).wrapping_shl(k.force_len_shl as u32);

    let hits_terrain = ipos.x <= 0
        || ipos.x >= level.width - 1
        || ipos.y <= 0
        || ipos.y >= level.height - 1
        || level.get().mat(ipos.x, ipos.y).dirt_rock();

    if hits_terrain {
        if !(*r).attached {
            (*r).length = k.attach_length;
            (*r).attached = true;

            if inside(level, ipos.x, ipos.y) {
                let (any_dirt, pix) = {
                    let lv = level.get();
                    (lv.mat(ipos.x, ipos.y).any_dirt(), lv.pixel(ipos.x, ipos.y))
                };
                if any_dirt {
                    // TODO in C++: check 11 and read it from the EXE.
                    for _ in 0..11 {
                        let angle = (*rand).below(128) as i32;
                        (hooks.create_nobject2)(ctx, 2, angle, LieroIVec2::default(), (*r).pos, pix as i32);
                    }
                }
            }
        }

        (*r).vel = LieroIVec2::default();
    } else if !(*r).anchor.is_null() {
        if !(*r).attached {
            // TODO in C++: should this be separate from attaching to terrain?
            (*r).length = k.attach_length;
            (*r).attached = true;
        }

        let anchor = (hooks.worm_motion)((*r).anchor);
        if (*r).cur_len > (*r).length {
            *anchor.vel = sub(*anchor.vel, div(force, (*r).cur_len));
        }

        (*r).vel = *anchor.vel;
        (*r).pos = *anchor.pos;
    } else {
        (*r).attached = false;
    }

    if (*r).attached {
        // cur_len can't be 0.
        if (*r).cur_len > (*r).length {
            *owner.vel = add(*owner.vel, div(force, (*r).cur_len));
        }
    } else {
        (*r).vel.y = (*r).vel.y.wrapping_add(k.gravity);

        if (*r).cur_len > (*r).length {
            (*r).vel = sub((*r).vel, div(force, (*r).cur_len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sobject_animation_ends_after_last_frame() {
        let (mut frame, mut delay) = (0, 1);
        let mut ticks = 0;
        while !liero_sobject_animate(&mut frame, &mut delay, 2, 3) {
            ticks += 1;
        }
        // Frames 1, 2 and 3 each last 2 ticks after the first.
        assert_eq!((ticks, frame), (6, 4));
    }

    #[test]
    fn explosion_pushes_worm_away_and_sets_power() {
        let mut vel = LieroIVec2::default();
        let mut sum = 0;
        let pos = LieroIVec2 { x: itof(13), y: itof(8) };
        assert!(liero_explosion_push_worm(pos, &mut vel, 10, 10, 6, 100, &mut sum));
        // power 3 on x, 4 on y; y pushed up since the worm is above.
        assert_eq!(vel, LieroIVec2 { x: 300, y: -400 });
        assert_eq!(sum, 3);
        assert_eq!(liero_explosion_damage(10, sum, 6), 5);

        let far = LieroIVec2 { x: itof(16), y: itof(10) };
        assert!(!liero_explosion_push_worm(far, &mut vel, 10, 10, 6, 100, &mut sum));
    }

    #[test]
    fn fast_worm_is_not_pushed() {
        let mut vel = LieroIVec2 { x: itof(2), y: -itof(3) };
        let mut sum = 0;
        let pos = LieroIVec2 { x: itof(9), y: itof(12) };
        assert!(liero_explosion_push_worm(pos, &mut vel, 10, 10, 6, 100, &mut sum));
        assert_eq!(vel, LieroIVec2 { x: itof(2), y: -itof(3) });
    }

    #[test]
    fn object_at_centre_is_not_pushed() {
        let mut vel = LieroIVec2::default();
        let pos = LieroIVec2 { x: itof(10), y: itof(12) };
        assert!(liero_explosion_push_object(pos, &mut vel, 10, 10, 6, 30));
        assert_eq!(vel, LieroIVec2 { x: 0, y: 30 * 4 });
    }
}
