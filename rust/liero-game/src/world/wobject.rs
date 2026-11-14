//! Weapon objects (`WObject` in `weapon.cpp`): what a worm fires.
//!
//! As with non-weapon objects (`nobject.rs`), an object is worked on by its
//! slot. What a step calls can change it, or free it: an explosion pushes
//! or chain-explodes it. So the step reads the slot afresh after each such
//! call, at the points C++ read it, and changes it in its slot between
//! calls.

use super::*;
use crate::weapon::{ShotType, ENDLESS_LASER_ID};

impl<'a> World<'a> {
    /// A copy of weapon object `slot`.
    pub(super) fn wobj(&self, slot: usize) -> WObject {
        self.g.data.wobjects.arr[slot]
    }

    /// Changes weapon object `slot` in place with `f`.
    pub(super) fn update_wobj<R>(&mut self, slot: usize, f: impl FnOnce(&mut WObject) -> R) -> R {
        f(&mut self.g.data.wobjects.arr[slot])
    }

    /// The weapon of weapon object `slot`.
    fn wobj_weapon(&self, slot: usize) -> &'a Weapon {
        self.weapon(self.wobj(slot).kind)
    }

    fn spread(&mut self, d: i32) -> i32 {
        self.rand((d * 2) as u32) as i32 - d
    }

    /// `fireWeapon`: one part of a shot of `kind`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_weapon(
        &mut self,
        kind: i32,
        angle: i32,
        vel: FVec2,
        speed: i32,
        pos: FVec2,
        owner_idx: i32,
        ww: FiredBy,
    ) {
        let slot = self.g.data.wobjects.new_object_reuse();
        let t = self.weapon(kind);

        self.update_wobj(slot, |o| {
            o.kind = kind;
            o.pos = pos;
            o.owner_idx = owner_idx;

            // STATS
            o.fired_by = ww;
            o.has_hit = false;
        });

        self.damage_potential(owner_idx, ww, t.hit_damage);
        self.shot(owner_idx, ww);

        let launch = weapon_view(kind, t).launch(&mut self.g.data.rand, angle, vel, speed);
        self.update_wobj(slot, |o| {
            o.vel = launch.vel;
            o.cur_frame = launch.cur_frame;
            o.time_left = launch.time_left;
        });
    }

    /// `WObject::blowUpObject`: frees the object, then makes its explosion,
    /// sound, splinters and dirt effect, caused by worm `cause_idx`.
    pub(super) fn blow_up_wobject(&mut self, slot: usize, cause_idx: i32) {
        let o = self.wobj(slot);
        let (pos, vel) = (o.pos, o.vel);

        self.g.data.wobjects.free(slot);

        // A freed slot keeps its object until a new one takes it, and only
        // a worm firing makes weapon objects.
        let w = weapon_view(self.wobj(slot).kind, self.wobj_weapon(slot));
        let IVec2 { x: ix, y: iy } = pos.to_int();

        if w.create_on_exp >= 0 {
            let fired_by = self.wobj(slot).fired_by;
            self.create_sobject(w.create_on_exp, ix, iy, cause_idx, fired_by, Some(slot));
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
                    FVec2::default(),
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
            self.dirt_effect(w.dirt_effect, ix - 7, iy - 7, [ix - 10, iy - 10, ix + 11, iy + 11]);
        }
    }

    /// An explosion or smoke of `kind` at the object's position, owned
    /// like it.
    fn wobj_create_sobject(&mut self, slot: usize, kind: i32) {
        let o = self.wobj(slot);
        self.create_sobject(kind, o.pos.x.to_int(), o.pos.y.to_int(), o.owner_idx, o.fired_by, None);
    }

    /// `WObject::process`.
    pub(super) fn process_wobject(&mut self, slot: usize) {
        let o = self.wobj(slot);
        let owner_idx = o.owner_idx;
        let (kind, index) = (self.wobj_weapon(slot), o.kind);

        // As liero would do this while rendering, we try to do it as early as possible
        if self.hack(h("RemExp"))
            && index == self.constant(c("RemExpObject")) - 1
            && self.worm(owner_idx).pressed(Control::Change)
            && self.worm(owner_idx).pressed(Control::Fire)
        {
            self.update_wobj(slot, |o| o.time_left = 0);
        }

        let w = weapon_view(index, kind);
        let (width, height) = (self.g.level.width, self.g.level.height);
        let cycles = self.cycles();
        let larpa_vel_div = self.constant(c("SplinterLarpaVelDiv"));
        let crackler_vel_div = self.constant(c("SplinterCracklerVelDiv"));
        let mut iter = 0;

        loop {
            iter += 1;
            let mut do_explode = false;
            let mut do_remove = false;

            self.update_wobj(slot, |o| o.pos += o.vel);

            if w.shot_type == Some(ShotType::Steerable) {
                let dir = cos_sin(self.wobj(slot).cur_frame);
                let mut new_vel = dir * w.speed / 100;

                // Whether the owner is steering: visible and holding up.
                let owner = self.worm(owner_idx);
                if owner.visible && owner.pressed(Control::Up) {
                    new_vel += dir * w.add_speed / 100;
                }

                self.update_wobj(slot, |o| o.vel = (o.vel * 8 + new_vel) / 9);
            } else if w.shot_type == Some(ShotType::DType2) {
                self.update_wobj(slot, |o| {
                    o.vel += cos_sin(o.cur_frame) * w.add_speed / 100;
                });

                if w.distribution != 0 {
                    let dx = self.spread(w.distribution);
                    self.update_wobj(slot, |o| o.vel.x += Fixed(dx));
                    let dy = self.spread(w.distribution);
                    self.update_wobj(slot, |o| o.vel.y += Fixed(dy));
                }
            }

            if w.bounce > 0 {
                let o = self.wobj(slot);
                let ipos = o.pos.to_int();
                let inew_pos = (o.pos + o.vel).to_int();

                // TODO in C++: read the 4/5 from the EXE.
                if self.blocked(inew_pos.x, ipos.y) {
                    self.update_wobj(slot, |o| {
                        if w.bounce != 100 {
                            o.vel.x = -o.vel.x * w.bounce / 100;
                            o.vel.y = o.vel.y * 4 / 5;
                        } else {
                            o.vel.x = -o.vel.x;
                        }
                    });
                }

                if self.blocked(ipos.x, inew_pos.y) {
                    self.update_wobj(slot, |o| {
                        if w.bounce != 100 {
                            o.vel.y = -o.vel.y * w.bounce / 100;
                            o.vel.x = o.vel.x * 4 / 5;
                        } else {
                            o.vel.y = -o.vel.y;
                        }
                    });
                }
            }

            if w.mult_speed != 100 {
                self.update_wobj(slot, |o| o.vel = o.vel * w.mult_speed / 100);
            }

            if w.obj_trail_type >= 0 && cycles % w.obj_trail_delay == 0 {
                self.wobj_create_sobject(slot, w.obj_trail_type);
            }

            if w.part_trail_obj >= 0 && cycles % w.part_trail_delay == 0 {
                if w.part_trail_type == 1 {
                    let o = self.wobj(slot);
                    let vel = o.vel / larpa_vel_div;
                    self.create_nobject1(w.part_trail_obj, vel, o.pos, 0, o.owner_idx, o.fired_by);
                } else {
                    let angle = self.rand(128) as i32;
                    let o = self.wobj(slot);
                    let vel = o.vel / crackler_vel_div;
                    self.create_nobject2(w.part_trail_obj, angle, vel, o.pos, 0, o.owner_idx, o.fired_by);
                }
            }

            if w.collide_with_objects {
                self.push_objects(slot, self.wobj(slot).vel * w.blow_away / 100);
            }

            let o = self.wobj(slot);
            let inew_pos = (o.pos + o.vel).to_int();

            self.update_wobj(slot, |o| {
                if inew_pos.x < 0 {
                    o.pos.x = Fixed::ZERO;
                }
                if inew_pos.y < 0 {
                    o.pos.y = Fixed::ZERO;
                }
                if inew_pos.x >= width {
                    o.pos.x = Fixed::from_int(width - 1);
                }
                if inew_pos.y >= height {
                    o.pos.y = Fixed::from_int(height - 1);
                }
            });

            if self.blocked(inew_pos.x, inew_pos.y) {
                if w.bounce == 0 {
                    if w.expl_ground {
                        do_explode = true;
                    } else {
                        self.update_wobj(slot, |o| o.vel = FVec2::default());
                    }
                }
            } else {
                self.update_wobj(slot, |o| {
                    // The original tested gravity first, which doesn't seem like a
                    // gain.
                    o.vel.y += Fixed(w.gravity);

                    if w.num_frames > 0 && (cycles & 7) == 0 {
                        // Without loop_anim the frames always run forwards; with
                        // it they follow the direction of travel.
                        if !w.loop_anim || o.vel.x > Fixed::ZERO {
                            o.cur_frame += 1;
                            if o.cur_frame > w.num_frames {
                                o.cur_frame = 0;
                            }
                        } else if o.vel.x < Fixed::ZERO {
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
                self.g.data.wobjects.free(slot);
                return;
            }

            if !(w.shot_type == Some(ShotType::Laser) && self.wobj(slot).used && (iter < 8 || w.id == ENDLESS_LASER_ID))
            {
                return;
            }
        }
    }

    /// Adds `impulse` to the objects within 2 pixels of weapon object
    /// `slot`, but the weapon objects of its weapon and owner.
    fn push_objects(&mut self, slot: usize, impulse: FVec2) {
        let o = self.wobj(slot);
        let (pos, kind, owner) = (o.pos, o.kind, o.owner_idx);

        let l = &mut self.g.data.wobjects;
        let mut wr = Cursor::default();
        while let Some(n) = wr.next(l) {
            let i = &mut l.arr[n];
            if (i.kind != kind || i.owner_idx != owner) && near(pos, i.pos) {
                i.vel += impulse;
            }
        }

        let l = &mut self.g.data.nobjects;
        let mut nr = Cursor::default();
        while let Some(n) = nr.next(l) {
            let i = &mut l.arr[n];
            if near(pos, i.pos) {
                i.vel += impulse;
            }
        }
    }

    /// The worm loop in `WObject::process`: hurts the worms weapon object
    /// `slot` hits. `owner` is its owner when the step began. Returns
    /// whether it should explode, and whether it should disappear.
    fn wobj_hit_worms(&mut self, slot: usize, owner: i32) -> (bool, bool) {
        let w = self.wobj_weapon(slot);
        let (mut explode, mut remove) = (false, false);

        for i in 0..self.worm_count() {
            let o = self.wobj(slot);

            if (w.hit_damage != 0 || w.blow_away != 0 || w.blood_on_hit != 0 || w.worm_collide)
                && self.spec_worm_hit(o.pos.x.to_int(), o.pos.y.to_int(), w.detect_distance, i)
            {
                let worm = self.g.worm_mut(i);
                worm.vel += o.vel * w.blow_away / 100;

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
                    let index = self.worm(i).index;
                    self.create_nobject2(6, angle, o.vel / 3, o.pos, 0, index, o.fired_by);
                }

                if w.hit_damage > 0 && self.worm(i).health > 0 && self.rand(3) == 0 {
                    self.hurt_sound(i);
                }

                if w.worm_collide && self.rand(u32::from(w.worm_collide)) == 0 {
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

/// Whether `pos` is within 2 pixels of `q` on both axes.
fn near(pos: FVec2, q: FVec2) -> bool {
    let two = Fixed::from_int(2);
    pos.x >= q.x - two && pos.x <= q.x + two && pos.y >= q.y - two && pos.y <= q.y + two
}
