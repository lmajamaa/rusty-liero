//! Non-weapon objects (`nobject.cpp`): splinters, dirt, blood drops and the
//! like.
//!
//! An object is worked on by its slot. As in C++, what a step calls can
//! change any object, this one included: an explosion pushes it, hitting a
//! worm can free it, and a new object can take its slot when the list is
//! full. So the step reads the slot afresh after each such call, at the
//! points C++ read it, and changes it in its slot between calls.

use super::*;

impl<'a> World<'a> {
    /// A copy of non-weapon object `slot`.
    fn nobj(&self, slot: usize) -> LieroNObject {
        self.g.data.nobjects.arr[slot]
    }

    /// Changes non-weapon object `slot` in place with `f`.
    fn update_nobj<R>(&mut self, slot: usize, f: impl FnOnce(&mut LieroNObject) -> R) -> R {
        f(&mut self.g.data.nobjects.arr[slot])
    }

    /// The type of non-weapon object `slot`.
    fn nobj_type(&self, slot: usize) -> &'a LieroTcNObject {
        self.nobject_type(self.nobj(slot).kind)
    }

    /// `Game::pixelMat`: whether the pixel's colour is dirt or rock. This
    /// reads the TC's table, not the level's material cache, as C++ does.
    fn pixel_dirt_rock(&self, x: i32, y: i32) -> bool {
        let pix = self.g.level.pixel(x, y);
        self.tc.materials[pix as usize].dirt_rock()
    }

    pub(super) fn blocked(&self, x: i32, y: i32) -> bool {
        !inside(&self.g.level, x, y) || self.pixel_dirt_rock(x, y)
    }

    /// `createNObject`: a non-weapon object of `kind`, in a free slot or
    /// the last one. Returns the slot.
    pub(super) fn create_nobject(
        &mut self,
        kind: i32,
        vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: FiredBy,
    ) -> usize {
        let t = self.nobject_type(kind);
        let slot = self.g.data.nobjects.new_object_reuse();

        self.update_nobj(slot, |o| {
            o.kind = kind;
            o.owner_idx = owner_idx;
            o.pos = pos;
            o.vel = vel;

            // STATS
            o.fired_by = fired_by;
            o.has_hit = false;
        });

        self.damage_potential(owner_idx, fired_by, t.hit_damage);

        let cur_frame = if t.start_frame > 0 {
            self.rand((t.num_frames + 1) as u32) as i32
        } else if color != 0 {
            color
        } else {
            t.color_bullets
        };

        let mut time_left = t.time_to_explo;
        if t.time_to_explo_v != 0 {
            time_left -= self.rand(t.time_to_explo_v as u32) as i32;
        }

        self.update_nobj(slot, |o| {
            o.cur_frame = cur_frame;
            o.time_left = time_left;
        });
        slot
    }

    /// `createNObject1`.
    pub(super) fn create_nobject1(
        &mut self,
        kind: i32,
        mut vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: FiredBy,
    ) {
        let t = self.nobject_type(kind);
        let d = t.distribution;
        if d != 0 {
            vel.x = vel
                .x
                .wrapping_add(d.wrapping_sub(self.rand(d.wrapping_mul(2) as u32) as i32));
            vel.y = vel
                .y
                .wrapping_add(d.wrapping_sub(self.rand(d.wrapping_mul(2) as u32) as i32));
        }
        self.create_nobject(kind, vel, pos, color, owner_idx, fired_by);
    }

    /// `createNObject2`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_nobject2(
        &mut self,
        kind: i32,
        angle: i32,
        mut vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: FiredBy,
    ) {
        let t = self.nobject_type(kind);
        let real_speed = t.speed.wrapping_sub(self.rand(t.speed_v as u32) as i32);

        vel = add(vel, mul_div(cos_sin(angle), real_speed, 100));

        // TODO in C++: !REPLAYS Make the distributions use the same code
        let d = t.distribution;
        if d != 0 {
            vel.x = vel
                .x
                .wrapping_add((self.rand(d.wrapping_mul(2) as u32) as i32).wrapping_sub(d));
            vel.y = vel
                .y
                .wrapping_add((self.rand(d.wrapping_mul(2) as u32) as i32).wrapping_sub(d));
        }

        let slot = self.create_nobject(kind, vel, pos, color, owner_idx, fired_by);
        self.update_nobj(slot, |o| o.pos = add(o.pos, o.vel));
    }

    /// An explosion or smoke of `kind` at the object's position, owned
    /// like it.
    fn nobj_create_sobject(&mut self, slot: usize, kind: i32) {
        let o = self.nobj(slot);
        self.create_sobject(kind, ftoi(o.pos.x), ftoi(o.pos.y), o.owner_idx, o.fired_by, None);
    }

    /// `NObject::process`.
    pub(super) fn process_nobject(&mut self, slot: usize) {
        let t = self.nobj_type(slot);
        let (width, height) = (self.g.level.width, self.g.level.height);

        let mut bounced = false;
        let mut do_explode = false;

        let o = self.update_nobj(slot, |o| {
            o.pos = add(o.pos, o.vel);
            *o
        });

        let mut inew_pos = ftoi_vec(add(o.pos, o.vel));
        let ipos = ftoi_vec(o.pos);

        if t.bounce > 0 {
            if self.blocked(inew_pos.x, ipos.y) {
                self.update_nobj(slot, |o| {
                    o.vel = LieroIVec2 {
                        x: o.vel.x.wrapping_neg().wrapping_mul(t.bounce) / 100,
                        // TODO in C++: read from the EXE.
                        y: o.vel.y.wrapping_mul(4) / 5,
                    }
                });
                bounced = true;
            }

            if self.blocked(ipos.x, inew_pos.y) {
                self.update_nobj(slot, |o| {
                    o.vel = LieroIVec2 {
                        x: o.vel.x.wrapping_mul(4) / 5,
                        y: o.vel.y.wrapping_neg().wrapping_mul(t.bounce) / 100,
                    }
                });
                bounced = true;
            }
        }

        if t.blood_trail && t.blood_trail_delay > 0 && self.cycles() % t.blood_trail_delay == 0 {
            let o = self.nobj(slot);
            // TODO in C++: read from the EXE.
            self.create_bobject(o.pos, div(o.vel, 4));
        }

        // Yes, again.
        let o = self.nobj(slot);
        inew_pos = ftoi_vec(add(o.pos, o.vel));

        self.update_nobj(slot, |o| {
            if inew_pos.x < 0 {
                o.pos.x = 0;
            }
            if inew_pos.y < 0 {
                o.pos.y = 0;
            }
            if inew_pos.x >= width {
                o.pos.x = itof(width);
            }
            if inew_pos.y >= height {
                o.pos.y = itof(height);
            }
        });

        if self.blocked(inew_pos.x, inew_pos.y) {
            self.update_nobj(slot, |o| o.vel = LieroIVec2::default());

            if t.expl_ground {
                if t.start_frame > 0 && t.draw_on_map {
                    self.nobj_draw_on_map(slot, ipos.x - 3, ipos.y - 3);
                }
                do_explode = true;
            }
        } else {
            // `leave_obj >= 0`: not in Liero, but some TCs forget to set
            // leave_obj_delay to 0 when they don't use this trail.
            if !bounced && t.leave_obj_delay != 0 && t.leave_obj >= 0 && self.cycles() % t.leave_obj_delay == 0 {
                self.nobj_create_sobject(slot, t.leave_obj);
            }

            self.update_nobj(slot, |o| o.vel.y = o.vel.y.wrapping_add(t.gravity));
        }

        // TODO in C++: read the 7 from the EXE.
        if t.num_frames > 0 && (self.cycles() & 7) == 0 {
            self.update_nobj(slot, |o| {
                if o.vel.x > 0 {
                    let f = o.cur_frame + 1;
                    o.cur_frame = if f > t.num_frames { 0 } else { f };
                } else if o.vel.x < 0 {
                    let f = o.cur_frame - 1;
                    o.cur_frame = if f < 0 { t.num_frames } else { f };
                }
            });
        }

        if t.time_to_explo > 0 {
            let left = self.update_nobj(slot, |o| {
                o.time_left -= 1;
                o.time_left
            });
            if left <= 0 {
                do_explode = true;
            }
        }

        if !do_explode && t.hit_damage > 0 && self.nobj_hit_worms(slot) {
            do_explode = true;
        }

        if do_explode {
            if t.create_on_exp >= 0 {
                self.nobj_create_sobject(slot, t.create_on_exp);
            }

            if t.dirt_effect >= 0 {
                let p = ftoi_vec(self.nobj(slot).pos);
                self.dirt_effect(
                    t.dirt_effect,
                    p.x - 7,
                    p.y - 7,
                    [p.x - 10, p.y - 10, p.x + 11, p.y + 11],
                );
            }

            for _ in 0..t.splinter_amount.max(0) {
                let angle = self.rand(128) as i32;
                let color_sub = self.rand(2) as i32;
                let o = self.nobj(slot);
                self.create_nobject2(
                    t.splinter_type,
                    angle,
                    LieroIVec2::default(),
                    o.pos,
                    t.splinter_colour - color_sub,
                    o.owner_idx,
                    FiredBy::NONE,
                );
            }

            if self.nobj(slot).used {
                self.g.data.nobjects.free(slot);
            }
        }
    }

    /// Draws the object's current frame onto the level at (x, y), with the
    /// shadow corrected.
    fn nobj_draw_on_map(&mut self, slot: usize, x: i32, y: i32) {
        let t = self.nobj_type(slot);
        let sprite = self.tc.small_sprites.frame(t.start_frame + self.nobj(slot).cur_frame);
        let level = &mut self.g.level;
        let common = self.tc.level_common();
        level.blit_image(&common, sprite, x, y, 7, 7);
        // x and y are 3 left of and above the object. This seems like an overly
        // large rectangle.
        if self.g.s.shadow {
            level.correct_shadow(&common, x - 5, y - 5, x + 12, y + 12);
        }
    }

    /// The worm loop in `NObject::process`: hurts the worms the object
    /// hits, and may free it. Returns true when it should explode.
    fn nobj_hit_worms(&mut self, slot: usize) -> bool {
        let t = self.nobj_type(slot);
        let mut do_explode = false;

        for i in 0..self.worm_count() {
            let o = self.nobj(slot);

            if self.spec_worm_hit(ftoi(o.pos.x), ftoi(o.pos.y), t.detect_distance, i) {
                let w = self.g.worm_mut(i);
                w.vel = add(w.vel, mul_div(o.vel, t.blow_away, 100));

                self.g.damage(i, t.hit_damage, o.owner_idx);

                let o = self.nobj(slot);
                self.damage_dealt(o.owner_idx, o.fired_by, i, t.hit_damage, o.has_hit);
                self.update_nobj(slot, |o| o.has_hit = true);

                if t.hit_damage > 0 && self.worm(i).health > 0 && self.rand(3) == 0 {
                    self.hurt_sound(i);
                }

                let blood = t.blood_on_hit.wrapping_mul(self.g.s.blood) / 100;

                for _ in 0..blood.max(0) {
                    let angle = self.rand(128) as i32;
                    let o = self.nobj(slot);
                    self.create_nobject2(6, angle, div(o.vel, 3), o.pos, 0, o.owner_idx, FiredBy::NONE);
                }

                if t.worm_explode {
                    do_explode = true;
                } else if t.worm_destroy && self.nobj(slot).used {
                    self.g.data.nobjects.free(slot);
                }
            }
        }

        do_explode
    }
}
