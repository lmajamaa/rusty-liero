//! Weapon objects (`WObject` in `weapon.cpp`): what a worm fires.
//!
//! As with non-weapon objects (`nobject.rs`), an object is worked on by its
//! slot. What a step calls can change it, or free it: an explosion pushes
//! or chain-explodes it. So the step reads the slot afresh after each such
//! call, at the points C++ read it, and changes it through a copy that is
//! written back at once.

use super::*;
use crate::weapon::{ENDLESS_LASER_ID, LIERO_SHOT_DTYPE2, LIERO_SHOT_LASER, LIERO_SHOT_STEERABLE};

impl World<'_> {
    /// A copy of weapon object `slot`.
    fn wobj(&self, slot: usize) -> LieroWObject {
        // SAFETY: the list is valid for the tick, and nothing holds a
        // reference into it while this copies.
        unsafe { (*self.wobjects()).arr[slot] }
    }

    /// Changes weapon object `slot` with `f`, through a copy that is
    /// written back before this returns.
    fn update_wobj<R>(&self, slot: usize, f: impl FnOnce(&mut LieroWObject) -> R) -> R {
        let mut o = self.wobj(slot);
        let r = f(&mut o);
        // SAFETY: as in `wobj`.
        unsafe { (*self.wobjects()).arr[slot] = o };
        r
    }

    /// The weapon of weapon object `slot`.
    fn wobj_weapon(&self, slot: usize) -> &LieroTcWeapon {
        // SAFETY: a weapon object's weapon is the TC's.
        unsafe { &*(self.wobj(slot).kind as *const LieroTcWeapon) }
    }

    fn spread(&self, d: i32) -> i32 {
        self.rand((d * 2) as u32) as i32 - d
    }

    /// `fireWeapon`: one part of a shot of `kind`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_weapon(
        &self,
        kind: &LieroTcWeapon,
        angle: i32,
        vel: LieroIVec2,
        speed: i32,
        pos: LieroIVec2,
        owner_idx: i32,
        ww: *mut LieroWormWeapon,
    ) {
        // SAFETY: as in `wobj`.
        let slot = unsafe { (*self.wobjects()).new_object_reuse() };

        self.update_wobj(slot, |o| {
            o.kind = (kind as *const LieroTcWeapon).cast();
            o.pos = pos;
            o.owner_idx = owner_idx;

            // STATS
            o.fired_by = ww;
            o.has_hit = false;
        });

        self.damage_potential(owner_idx, ww, kind.hit_damage);
        self.shot(owner_idx, ww);

        // SAFETY: nothing else has the game's random numbers during the call.
        let launch = weapon_view(kind).launch(unsafe { &mut *self.rand_ptr() }, angle, vel, speed);
        self.update_wobj(slot, |o| {
            o.vel = launch.vel;
            o.cur_frame = launch.cur_frame;
            o.time_left = launch.time_left;
        });
    }

    /// `WObject::blowUpObject`: frees the object, then makes its explosion,
    /// sound, splinters and dirt effect, caused by worm `cause_idx`.
    pub(super) fn blow_up_wobject(&self, slot: usize, cause_idx: i32) {
        let o = self.wobj(slot);
        let (pos, vel) = (o.pos, o.vel);

        // SAFETY: as in `wobj`.
        unsafe { (*self.wobjects()).free(slot) };

        // A freed slot keeps its object until a new one takes it, and only
        // a worm firing makes weapon objects.
        let w = weapon_view(self.wobj_weapon(slot));
        let (x, y) = (pos.x, pos.y);

        if w.create_on_exp >= 0 {
            let fired_by = self.wobj(slot).fired_by;
            self.create_sobject(w.create_on_exp, ftoi(x), ftoi(y), cause_idx, fired_by, Some(slot));
        }

        if w.explo_sound >= 0 {
            self.play(w.explo_sound, null_mut(), 0);
        }

        for _ in 0..w.splinter_amount.max(0) {
            if w.splinter_scatter == 0 {
                let angle = self.rand(128) as i32;
                let color_sub = self.rand(2) as i32;
                let fired_by = self.wobj(slot).fired_by;
                self.create_nobject2(
                    w.splinter_type,
                    angle,
                    LieroIVec2::default(),
                    pos,
                    w.splinter_colour - color_sub,
                    cause_idx,
                    fired_by,
                );
            } else {
                let color_sub = self.rand(2) as i32;
                let fired_by = self.wobj(slot).fired_by;
                self.create_nobject1(
                    w.splinter_type,
                    vel,
                    pos,
                    w.splinter_colour - color_sub,
                    cause_idx,
                    fired_by,
                );
            }
        }

        if w.dirt_effect >= 0 {
            let (ix, iy) = (ftoi(x), ftoi(y));
            let common = self.tc().level_common();
            // SAFETY: the level is valid for the tick, and nothing else has
            // the game's random numbers during the call.
            unsafe {
                let mut lv = self.g.f.level.get();
                lv.draw_dirt_effect(&common, &mut *self.rand_ptr(), w.dirt_effect, ix - 7, iy - 7);
                if self.g.s.shadow {
                    lv.correct_shadow(&common, ix - 10, iy - 10, ix + 11, iy + 11);
                }
            }
        }
    }

    /// An explosion or smoke of `kind` at the object's position, owned
    /// like it.
    fn wobj_create_sobject(&self, slot: usize, kind: i32) {
        let o = self.wobj(slot);
        self.create_sobject(kind, ftoi(o.pos.x), ftoi(o.pos.y), o.owner_idx, o.fired_by, None);
    }

    /// `WObject::process`.
    pub(super) fn process_wobject(&self, slot: usize) {
        let o = self.wobj(slot);
        let owner_idx = o.owner_idx;
        let owner = self.worm(owner_idx);
        let kind = self.wobj_weapon(slot);
        let index = ((kind as *const LieroTcWeapon as usize).wrapping_sub(self.tc().weapons as usize)
            / std::mem::size_of::<LieroTcWeapon>()) as i32;

        // As liero would do this while rendering, we try to do it as early as possible
        // SAFETY: the worm is valid for the tick.
        if self.hack(h("RemExp"))
            && index == self.constant(c("RemExpObject")) - 1
            && unsafe { pressed(owner, LIERO_CONTROL_CHANGE) && pressed(owner, LIERO_CONTROL_FIRE) }
        {
            self.update_wobj(slot, |o| o.time_left = 0);
        }

        let w = weapon_view(kind);
        let level = self.g.f.level;
        let cycles = self.cycles();
        let larpa_vel_div = self.constant(c("SplinterLarpaVelDiv"));
        let crackler_vel_div = self.constant(c("SplinterCracklerVelDiv"));
        let mut iter = 0;

        loop {
            iter += 1;
            let mut do_explode = false;
            let mut do_remove = false;

            self.update_wobj(slot, |o| o.pos = add(o.pos, o.vel));

            if w.shot_type == LIERO_SHOT_STEERABLE {
                let dir = cos_sin(self.wobj(slot).cur_frame);
                let mut new_vel = mul_div(dir, w.speed, 100);

                // Whether the owner is steering: visible and holding up.
                // SAFETY: the worm is valid for the tick.
                if unsafe { (*owner).visible && pressed(owner, LIERO_CONTROL_UP) } {
                    new_vel = add(new_vel, mul_div(dir, w.add_speed, 100));
                }

                self.update_wobj(slot, |o| o.vel = div(add(mul_div(o.vel, 8, 1), new_vel), 9));
            } else if w.shot_type == LIERO_SHOT_DTYPE2 {
                self.update_wobj(slot, |o| {
                    o.vel = add(o.vel, mul_div(cos_sin(o.cur_frame), w.add_speed, 100));
                });

                if w.distribution != 0 {
                    let dx = self.spread(w.distribution);
                    self.update_wobj(slot, |o| o.vel.x = o.vel.x.wrapping_add(dx));
                    let dy = self.spread(w.distribution);
                    self.update_wobj(slot, |o| o.vel.y = o.vel.y.wrapping_add(dy));
                }
            }

            if w.bounce > 0 {
                let o = self.wobj(slot);
                let ipos = ftoi_vec(o.pos);
                let inew_pos = ftoi_vec(add(o.pos, o.vel));

                // TODO in C++: read the 4/5 from the EXE.
                if self.blocked(inew_pos.x, ipos.y) {
                    self.update_wobj(slot, |o| {
                        if w.bounce != 100 {
                            o.vel.x = o.vel.x.wrapping_neg().wrapping_mul(w.bounce) / 100;
                            o.vel.y = o.vel.y.wrapping_mul(4) / 5;
                        } else {
                            o.vel.x = o.vel.x.wrapping_neg();
                        }
                    });
                }

                if self.blocked(ipos.x, inew_pos.y) {
                    self.update_wobj(slot, |o| {
                        if w.bounce != 100 {
                            o.vel.y = o.vel.y.wrapping_neg().wrapping_mul(w.bounce) / 100;
                            o.vel.x = o.vel.x.wrapping_mul(4) / 5;
                        } else {
                            o.vel.y = o.vel.y.wrapping_neg();
                        }
                    });
                }
            }

            if w.mult_speed != 100 {
                self.update_wobj(slot, |o| o.vel = mul_div(o.vel, w.mult_speed, 100));
            }

            if w.obj_trail_type >= 0 && cycles % w.obj_trail_delay == 0 {
                self.wobj_create_sobject(slot, w.obj_trail_type);
            }

            if w.part_trail_obj >= 0 && cycles % w.part_trail_delay == 0 {
                if w.part_trail_type == 1 {
                    let o = self.wobj(slot);
                    let vel = div(o.vel, larpa_vel_div);
                    self.create_nobject1(w.part_trail_obj, vel, o.pos, 0, o.owner_idx, o.fired_by);
                } else {
                    let angle = self.rand(128) as i32;
                    let o = self.wobj(slot);
                    let vel = div(o.vel, crackler_vel_div);
                    self.create_nobject2(w.part_trail_obj, angle, vel, o.pos, 0, o.owner_idx, o.fired_by);
                }
            }

            if w.collide_with_objects {
                self.push_objects(slot, mul_div(self.wobj(slot).vel, w.blow_away, 100));
            }

            let o = self.wobj(slot);
            let inew_pos = ftoi_vec(add(o.pos, o.vel));

            self.update_wobj(slot, |o| {
                if inew_pos.x < 0 {
                    o.pos.x = 0;
                }
                if inew_pos.y < 0 {
                    o.pos.y = 0;
                }
                if inew_pos.x >= level.width {
                    o.pos.x = itof(level.width - 1);
                }
                if inew_pos.y >= level.height {
                    o.pos.y = itof(level.height - 1);
                }
            });

            if self.blocked(inew_pos.x, inew_pos.y) {
                if w.bounce == 0 {
                    if w.expl_ground {
                        do_explode = true;
                    } else {
                        self.update_wobj(slot, |o| o.vel = LieroIVec2::default());
                    }
                }
            } else {
                self.update_wobj(slot, |o| {
                    // The original tested gravity first, which doesn't seem like a
                    // gain.
                    o.vel.y = o.vel.y.wrapping_add(w.gravity);

                    if w.num_frames > 0 && (cycles & 7) == 0 {
                        // Without loop_anim the frames always run forwards; with
                        // it they follow the direction of travel.
                        if !w.loop_anim || o.vel.x > 0 {
                            o.cur_frame += 1;
                            if o.cur_frame > w.num_frames {
                                o.cur_frame = 0;
                            }
                        } else if o.vel.x < 0 {
                            o.cur_frame -= 1;
                            if o.cur_frame < 0 {
                                o.cur_frame = w.num_frames;
                            }
                        }
                    }
                });
            }

            if w.time_to_explo > 0 {
                let left = self.update_wobj(slot, |o| {
                    o.time_left -= 1;
                    o.time_left
                });
                if left < 0 {
                    do_explode = true;
                }
            }

            let (explode, remove) = self.wobj_hit_worms(slot, owner_idx);
            do_explode |= explode;
            do_remove |= remove;

            if do_explode {
                self.blow_up_wobject(slot, self.wobj(slot).owner_idx);
                return;
            } else if do_remove {
                // SAFETY: as in `wobj`.
                unsafe { (*self.wobjects()).free(slot) };
                return;
            }

            if !(w.shot_type == LIERO_SHOT_LASER && self.wobj(slot).used && (iter < 8 || w.id == ENDLESS_LASER_ID)) {
                return;
            }
        }
    }

    /// Adds `impulse` to the objects within 2 pixels of weapon object
    /// `slot`, but the weapon objects of its weapon and owner.
    fn push_objects(&self, slot: usize, impulse: LieroIVec2) {
        let o = self.wobj(slot);
        let (pos, kind, owner) = (o.pos, o.kind, o.owner_idx);

        // SAFETY: nothing else has the lists during the loops, which call
        // nothing.
        let l = unsafe { &mut *self.wobjects() };
        let mut wr = Cursor::default();
        while let Some(n) = wr.next(l) {
            let i = &mut l.arr[n];
            if (i.kind != kind || i.owner_idx != owner) && near(pos, i.pos) {
                i.vel = add(i.vel, impulse);
            }
        }

        // SAFETY: as above.
        let l = unsafe { &mut *self.nobjects() };
        let mut nr = Cursor::default();
        while let Some(n) = nr.next(l) {
            let i = &mut l.arr[n];
            if near(pos, i.pos) {
                i.vel = add(i.vel, impulse);
            }
        }
    }

    /// The worm loop in `WObject::process`: hurts the worms weapon object
    /// `slot` hits. `owner` is its owner when the step began. Returns
    /// whether it should explode, and whether it should disappear.
    fn wobj_hit_worms(&self, slot: usize, owner: i32) -> (bool, bool) {
        let w = self.wobj_weapon(slot);
        let (mut explode, mut remove) = (false, false);

        for i in 0..self.worm_count() {
            let worm = self.worm(i);
            let o = self.wobj(slot);

            if (w.hit_damage != 0 || w.blow_away != 0 || w.blood_on_hit != 0 || w.worm_collide)
                && self.spec_worm_hit(ftoi(o.pos.x), ftoi(o.pos.y), w.detect_distance, i)
            {
                // SAFETY: the worm is valid for the tick.
                unsafe { (*worm).vel = add((*worm).vel, mul_div(o.vel, w.blow_away, 100)) };

                self.g.damage(i, w.hit_damage, o.owner_idx);
                let o = self.wobj(slot);
                self.damage_dealt(owner, o.fired_by, i, w.hit_damage, o.has_hit);
                if !o.has_hit {
                    self.hit(owner, o.fired_by, i);
                }
                self.update_wobj(slot, |o| o.has_hit = true);

                let blood_amount = w.blood_on_hit.wrapping_mul(self.g.s.blood) / 100;

                for _ in 0..blood_amount.max(0) {
                    let angle = self.rand(128) as i32;
                    let o = self.wobj(slot);
                    // SAFETY: as above.
                    let index = unsafe { (*worm).index };
                    self.create_nobject2(6, angle, div(o.vel, 3), o.pos, 0, index, o.fired_by);
                }

                // SAFETY: as above.
                if w.hit_damage > 0 && unsafe { (*worm).health } > 0 && self.rand(3) == 0 {
                    self.hurt_sound(i);
                }

                if w.worm_collide && self.rand(w.worm_collide as u32) == 0 {
                    if w.worm_explode {
                        explode = true;
                    }
                    remove = true;
                }
            }
        }

        (explode, remove)
    }
}

/// Whether integer `p` is within 2 pixels of fixed-point `q` on both axes,
/// as C++ compared them.
fn near(pos: LieroIVec2, q: LieroIVec2) -> bool {
    pos.x >= q.x - itof(2) && pos.x <= q.x + itof(2) && pos.y >= q.y - itof(2) && pos.y <= q.y + itof(2)
}
