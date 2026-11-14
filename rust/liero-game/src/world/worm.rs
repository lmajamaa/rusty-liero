//! Worms: movement, physics, aiming, the ninja rope controls, weapon
//! changing, firing and reloading, the laser sight, bonus pickups, dying
//! and respawning (`Worm::process` and the functions it calls in
//! `worm.cpp`). Results have to stay bit-identical to the C++ versions,
//! which `tools/hashrun` checks.
//!
//! The worm's fields are read and written in place, field by field, where
//! the C++ did, since what the tick calls can change them (healing changes
//! health and lives, and an explosion can push the worm).

use super::*;
use crate::material::Material;
use crate::object::Ninjarope;
use crate::objects::Bonus;
use crate::weapon::{LIERO_SHOT_STEERABLE, SELECTABLE_WEAPONS};
use crate::worm::*;

impl World<'_> {
    /// `Worm::process` for worm `i`.
    pub(super) fn process_worm(&mut self, i: i32) {
        let s = self.g.s;
        let k = self.worm_constants();
        let env = WormEnv {
            cycles: self.cycles(),
            game_mode: s.game_mode,
            blood: s.blood,
            loading_time: s.loading_time,
            load_change: s.load_change,
            shadow: s.shadow,
            quick_sim: self.g.data.quick_sim,
            anim_tab: WORM_ANIM_TAB,
        };

        Worm {
            max_health: self.g.max_health(i),
            world: self,
            k: &k,
            env: &env,
            i,
            bonuses: Cursor::default(),
            wobjects: Cursor::default(),
        }
        .process();
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

/// Getters and setters for the worm's fields.
macro_rules! fields {
    ($($get:ident, $set:ident: $t:ty;)*) => {
        $(
            #[allow(dead_code)]
            fn $get(&self) -> $t {
                self.w().$get
            }
            #[allow(dead_code)]
            fn $set(&mut self, v: $t) {
                self.w_mut().$get = v
            }
        )*
    };
}

struct Worm<'w, 'a> {
    world: &'w mut World<'a>,
    /// The worm's full health (`settings->health`).
    max_health: i32,
    k: &'w WormConstants,
    env: &'w WormEnv,
    /// The worm's position in the game's worms.
    i: i32,
    /// Where `bonus_next` and `steerable_next` are in their lists.
    bonuses: Cursor,
    wobjects: Cursor,
}

impl Worm<'_, '_> {
    fields! {
        pos, set_pos: IVec2;
        vel, set_vel: IVec2;
        logic_respawn, set_logic_respawn: IVec2;
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
        leave_shell_timer, set_leave_shell_timer: i32;
    }

    fn w(&self) -> &WormData {
        self.world.worm(self.i)
    }

    fn w_mut(&mut self) -> &mut WormData {
        self.world.g.worm_mut(self.i)
    }

    fn react(&self, dir: usize) -> i32 {
        self.w().reacts[dir]
    }

    fn set_react(&mut self, dir: usize, v: i32) {
        self.w_mut().reacts[dir] = v;
    }

    fn add_react(&mut self, dir: usize, v: i32) {
        self.w_mut().reacts[dir] += v;
    }

    fn index(&self) -> i32 {
        self.w().index
    }

    fn rope(&self) -> &Ninjarope {
        &self.w().ninjarope
    }

    fn rope_mut(&mut self) -> &mut Ninjarope {
        &mut self.w_mut().ninjarope
    }

    /// The current weapon slot.
    fn slot(&self) -> &WormWeapon {
        &self.w().weapons[self.current_weapon() as usize]
    }

    fn slot_mut(&mut self) -> &mut WormWeapon {
        let n = self.current_weapon() as usize;
        &mut self.w_mut().weapons[n]
    }

    /// The id the current weapon slot's looping sounds play with.
    fn slot_sound_id(&self) -> *mut c_void {
        std::ptr::from_ref(self.slot()).cast_mut().cast()
    }

    /// The current weapon slot, as the objects it fires record it.
    fn fired_by(&self) -> FiredBy {
        FiredBy {
            worm: self.world.g.worm_id(self.i).0 as i32,
            slot: self.current_weapon(),
        }
    }

    fn available(&self) -> bool {
        self.slot().loading_left == 0
    }

    /// The current weapon's TC weapon.
    fn tc_weapon(&self) -> &Weapon {
        self.world.weapon(self.slot().kind)
    }

    /// The current weapon's type.
    fn weapon(&self) -> WeaponView {
        weapon_view(self.slot().kind, self.tc_weapon())
    }

    fn pressed(&self, c: Control) -> bool {
        self.control_states() & c.bit() != 0
    }

    fn set_control(&mut self, c: Control, v: bool) {
        let s = self.control_states();
        self.set_control_states(if v { s | c.bit() } else { s & !c.bit() });
    }

    fn release(&mut self, c: Control) {
        self.set_control(c, false);
    }

    fn pressed_once(&mut self, c: Control) -> bool {
        let state = self.pressed(c);
        self.release(c);
        state
    }

    fn rand(&mut self, max: u32) -> u32 {
        self.world.rand(max)
    }

    fn rand_u32(&mut self) -> u32 {
        self.world.g.data.rand.next_u32()
    }

    fn play_sound(&self, sound: i32) {
        self.world.play(sound, null_mut(), 0);
    }

    fn create_nobject1(&mut self, kind: i32, vel: IVec2, pos: IVec2) {
        let index = self.index();
        self.world.create_nobject1(kind, vel, pos, 0, index, FiredBy::NONE);
    }

    fn create_nobject2(&mut self, kind: i32, angle: i32, vel: IVec2, pos: IVec2) {
        let index = self.index();
        self.world
            .create_nobject2(kind, angle, vel, pos, 0, index, FiredBy::NONE);
    }

    fn stop_loop_sound_if_any(&self) {
        if self.weapon().loop_sound != 0 {
            self.world.stop(self.slot_sound_id());
        }
    }

    fn level_size(&self) -> (i32, i32) {
        (self.world.g.level.width, self.world.g.level.height)
    }

    fn mat(&self, x: i32, y: i32) -> Material {
        self.world.g.level.mat(x, y)
    }

    /// `Level::checkedMatWrap`: the material at x + y * width, or the
    /// material of colour 0 when that is outside the level.
    fn checked_mat_wrap(&self, x: i32, y: i32) -> Material {
        let lv = &self.world.g.level;
        let idx = x.wrapping_add(y.wrapping_mul(lv.width)) as u32 as usize;
        lv.materials.get(idx).copied().unwrap_or(self.world.tc.materials[0])
    }

    fn draw_dirt_effect(&mut self, effect: i32, x: i32, y: i32) {
        let w = &mut *self.world;
        let common = w.tc.level_common();
        w.g.level.draw_dirt_effect(&common, &mut w.g.data.rand, effect, x, y);
    }

    fn correct_shadow(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        if self.env.shadow {
            let w = &mut *self.world;
            let common = w.tc.level_common();
            w.g.level.correct_shadow(&common, x1, y1, x2, y2);
        }
    }

    /// The worm's own sound: `sound`, unless one is playing.
    fn play_worm_sound(&self, sound: i32) {
        self.world.play(sound, self.world.worm_sound_id(self.i), 0);
    }

    /// The next bonus, by its slot, and a copy of it.
    fn bonus_next(&mut self) -> Option<Bonus> {
        let l = &self.world.g.data.bonuses;
        self.bonuses.next(l).map(|i| l.arr[i])
    }

    /// Frees the bonus `bonus_next` returned last.
    fn bonus_free(&mut self) {
        self.world.g.data.bonuses.free(self.bonuses.last());
    }

    /// The next of the worm's weapon objects of its current weapon, by slot.
    fn steerable_next(&mut self) -> Option<usize> {
        let kind = self.slot().kind;
        let index = self.index();
        let l = &self.world.g.data.wobjects;
        while let Some(n) = self.wobjects.next(l) {
            if l.arr[n].kind == kind && l.arr[n].owner_idx == index {
                return Some(n);
            }
        }
        None
    }

    /// Puts weapon `weapon` (an index into the TC's weapons) with full ammo
    /// in slot `slot`.
    fn give_weapon(&mut self, slot: i32, weapon: i32) {
        let kind = self.world.weapon(weapon);
        let ww = &mut self.w_mut().weapons[slot as usize];
        ww.kind = weapon;
        ww.ammo = kind.ammo;
    }

    /// The current weapon's launch sound, looping or not.
    fn launch_sound(&self) {
        let id = self.slot_sound_id();
        let w = self.tc_weapon();

        if w.launch_sound >= 0 {
            if w.loop_sound != 0 {
                if !self.world.playing(id) {
                    self.world.play(w.launch_sound, id, -1);
                }
            } else {
                self.world.play(w.launch_sound, null_mut(), 0);
            }
        }
    }

    /// `Worm::fire`: fires the current weapon `w`.
    fn fire(&mut self, w: &WeaponView) {
        let angle = ftoi(self.aiming_angle());

        let ww = self.slot_mut();
        ww.ammo -= 1;
        ww.delay_left = w.delay;

        self.set_fire_cone(w.fire_cone);

        let firing = sub(
            add(mul_div(cos_sin(angle), w.detect_distance + 5, 1), self.pos()),
            IVec2 { x: 0, y: itof(1) },
        );

        if w.leave_shells > 0 && self.rand(w.leave_shells as u32) == 0 {
            self.set_leave_shell_timer(w.leave_shell_delay);
        }

        self.launch_sound();

        let mut speed = w.speed;
        let mut firing_vel = IVec2::default();

        if w.affect_by_worm {
            if speed < 100 {
                speed = 100;
            }
            firing_vel = mul_div(self.vel(), 100, speed);
        }

        for _ in 0..w.parts.max(0) {
            // `Weapon::fire`: one part of the shot.
            let kind = self.slot().kind;
            let (index, fired_by) = (self.index(), self.fired_by());
            self.world
                .fire_weapon(kind, angle, firing_vel, speed, firing, index, fired_by);
        }

        let mut recoil = w.recoil;
        if self.k.signed_recoil && recoil >= 128 {
            recoil -= 256;
        }

        self.set_vel(sub(self.vel(), mul_div(cos_sin(angle), recoil, 100)));
    }

    fn calculate_reaction_force(&mut self, new_x: i32, new_y: i32, dir: usize) {
        self.set_react(dir, 0);

        for &(dx, dy) in COL_POINTS[dir] {
            if !self.checked_mat_wrap(new_x + dx, new_y + dy).background() {
                self.add_react(dir, 1);
            }
        }
    }

    fn process_physics(&mut self) {
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
        let mbh = if vel.x > 0 {
            k.min_bounce_right
        } else {
            -k.min_bounce_left
        };
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
    fn calculate_reaction_forces(&mut self) {
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
            if self.react(low) < 2 && self.react(high) > 0 && (self.react(RF_LEFT) > 0 || self.react(RF_RIGHT) > 0) {
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

    fn pick_up_bonuses(&mut self) {
        let k = self.k;
        let ipos = ftoi_vec(self.pos());

        self.bonuses = Cursor::default();
        while let Some(b) = self.bonus_next() {
            let (bx, by) = (ftoi(b.x), ftoi(b.y));
            if !(ipos.x + 5 > bx && ipos.x - 5 < bx && ipos.y + 5 > by && ipos.y - 5 < by) {
                continue;
            }

            if b.frame == 1 {
                if self.health() < self.max_health {
                    self.bonus_free();

                    // C++ did this in unsigned arithmetic.
                    let amount = self
                        .rand(k.bonus_health_var as u32)
                        .wrapping_add(k.bonus_min_health as u32)
                        .wrapping_mul(self.max_health as u32)
                        / 100;
                    self.world.g.heal(self.i, amount as i32);
                }
            } else if b.frame == 0 {
                if self.rand(k.bonus_explode_risk as u32) > 1 {
                    let slot = self.current_weapon();

                    if !k.bonus_reload_only {
                        self.set_fire_cone(0);
                        self.give_weapon(slot, b.weapon);
                    }

                    self.play_sound(24);

                    self.bonus_free();

                    self.w_mut().weapons[slot as usize].loading_left = 0;
                } else {
                    self.bonus_free();
                    let index = self.index();
                    self.world.create_sobject(0, bx, by, index, FiredBy::NONE, None);
                }
            }
        }
    }

    fn process_steerables(&mut self) {
        self.set_steerable_count(0);
        self.set_steerable_sum_x(0);
        self.set_steerable_sum_y(0);

        if self.weapon().shot_type != LIERO_SHOT_STEERABLE {
            return;
        }

        let turn = (self.env.cycles & 1) + 1;
        self.wobjects = Cursor::default();
        while let Some(n) = self.steerable_next() {
            let pos = self.world.wobj(n).pos;
            let (left, right) = (self.pressed(Control::Left), self.pressed(Control::Right));
            self.world.update_wobj(n, |o| {
                if left {
                    o.cur_frame -= turn;
                }
                if right {
                    o.cur_frame += turn;
                }
                o.cur_frame &= 127; // Wrap
            });
            self.set_movable(false);

            self.set_steerable_sum_x(self.steerable_sum_x() + ftoi(pos.x));
            self.set_steerable_sum_y(self.steerable_sum_y() + ftoi(pos.y));
            self.set_steerable_count(self.steerable_count() + 1);
        }
    }

    fn process_aiming(&mut self) {
        let k = self.k;
        let up = self.pressed(Control::Up);
        let down = self.pressed(Control::Down);

        if self.aiming_speed() != 0 {
            self.set_aiming_angle(self.aiming_angle() + self.aiming_speed());

            if !up && !down {
                self.set_aiming_speed(self.aiming_speed().wrapping_mul(k.aim_fric_mult) / k.aim_fric_div);
            }

            // On the left, the max limit is the lower angle.
            let limit = |w: &mut Self, angle: i32| {
                w.set_aiming_speed(0);
                w.set_aiming_angle(itof(angle));
            };
            if self.direction() == 1 {
                if ftoi(self.aiming_angle()) > k.aim_max_right {
                    limit(self, k.aim_max_right);
                }
                if ftoi(self.aiming_angle()) < k.aim_min_right {
                    limit(self, k.aim_min_right);
                }
            } else {
                if ftoi(self.aiming_angle()) < k.aim_max_left {
                    limit(self, k.aim_max_left);
                }
                if ftoi(self.aiming_angle()) > k.aim_min_left {
                    limit(self, k.aim_min_left);
                }
            }
        }

        let rope_out = self.rope().out;
        if self.movable() && (!rope_out || !self.pressed(Control::Change)) {
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

    fn process_tasks(&mut self) {
        let k = self.k;

        if self.pressed(Control::Change) {
            if self.rope().out {
                let (up, down) = (self.pressed(Control::Up), self.pressed(Control::Down));
                let r = self.rope_mut();
                if up {
                    r.length -= k.nr_pull_vel;
                }
                if down {
                    r.length += k.nr_release_vel;
                }

                if r.length < k.nr_min_length {
                    r.length = k.nr_min_length;
                }
                if r.length > k.nr_max_length {
                    r.length = k.nr_max_length;
                }
            }

            if self.pressed_once(Control::Jump) {
                let r = self.rope_mut();
                r.out = true;
                r.attached = false;

                self.play_sound(5);

                let pos = self.pos();
                let dir = cos_sin(ftoi(self.aiming_angle()));
                let r = self.rope_mut();
                r.pos = pos;
                r.vel = IVec2 {
                    x: dir.x.wrapping_shl(k.nr_throw_vel_x as u32),
                    y: dir.y.wrapping_shl(k.nr_throw_vel_y as u32),
                };

                r.length = k.nr_initial_length;
            }
        } else if self.pressed(Control::Jump) {
            // Jump removes the rope and jumps.
            let r = self.rope_mut();
            r.out = false;
            r.attached = false;

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

    fn fire_or_stop(&mut self) {
        let ww = *self.slot();
        let w = self.weapon();

        if self.pressed(Control::Fire) && !self.pressed(Control::Change) && ww.loading_left == 0 && ww.delay_left <= 0 {
            self.fire(&w);
        } else if w.loop_sound != 0 {
            self.world.stop(self.slot_sound_id());
        }
    }

    /// `Worm::processWeapons`: counts down weapon delays and reloads, the
    /// fire cone and the shell casing timer.
    fn process_weapons(&mut self) {
        let current = self.weapon();

        for ww in &mut self.w_mut().weapons[..SELECTABLE_WEAPONS] {
            if ww.delay_left >= 0 {
                ww.delay_left -= 1;
            }
        }

        let loading_time = self.env.loading_time;
        let ww = self.slot_mut();

        if ww.ammo <= 0 {
            ww.loading_left = current.computed_loading_time(loading_time);
            ww.ammo = current.ammo;
        }

        // The computed loading time is never 0, so this works.
        if ww.loading_left > 0 {
            ww.loading_left -= 1;
            if ww.loading_left <= 0 && current.play_reload_sound {
                self.play_sound(24);
            }
        }

        if self.fire_cone() > 0 {
            self.set_fire_cone(self.fire_cone() - 1);
        }

        if self.leave_shell_timer() > 0 {
            self.set_leave_shell_timer(self.leave_shell_timer() - 1);
            if self.leave_shell_timer() <= 0 {
                let vel_y = -(self.rand(20000) as i32);
                let vel_x = self.rand(16000) as i32 - 8000;
                self.create_nobject1(7, IVec2 { x: vel_x, y: vel_y }, self.pos());
            }
        }
    }

    fn process_sight(&mut self) {
        let w = self.weapon();
        let (width, height) = self.level_size();

        if !(self.available() && (w.laser_sight || w.id == self.k.laser_weapon - 1)) {
            self.set_make_sight_green(false);
            return;
        }

        let dir = cos_sin(ftoi(self.aiming_angle()));
        let pos = self.pos();
        let mut temp = IVec2 {
            x: pos.x.wrapping_add(dir.x.wrapping_mul(6)),
            y: pos.y.wrapping_add(dir.y.wrapping_mul(6)).wrapping_sub(itof(1)),
        };

        loop {
            temp = add(temp, dir);
            let hit = self.world.worm_hit(ftoi(temp.x), ftoi(temp.y), 0, self.i);
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

    fn process_weapon_change(&mut self) {
        if !self.key_change_pressed() {
            self.release(Control::Left);
            self.release(Control::Right);
            self.set_key_change_pressed(true);
        }

        self.set_fire_cone(0);
        self.set_animate(false);

        self.stop_loop_sound_if_any();

        if self.available() || self.env.load_change {
            let n = SELECTABLE_WEAPONS as i32;
            for (key, step) in [(Control::Left, n - 1), (Control::Right, 1)] {
                if self.pressed_once(key) {
                    self.set_current_weapon((self.current_weapon() + step) % n);

                    let pos = self.pos();
                    self.set_hotspot_x(ftoi(pos.x));
                    self.set_hotspot_y(ftoi(pos.y));
                }
            }
        }
    }

    fn dig(&mut self) {
        let dir = cos_sin(ftoi(self.aiming_angle()));
        let twice = IVec2 {
            x: dir.x.wrapping_mul(2),
            y: dir.y.wrapping_mul(2),
        };

        let mut dig_pos = sub(add(twice, self.pos()), IVec2 { x: itof(7), y: itof(7) });

        for i in 0..2 {
            if i == 1 {
                dig_pos = add(dig_pos, twice);
            }
            let p = ftoi_vec(dig_pos);
            self.draw_dirt_effect(7, p.x, p.y);
            self.correct_shadow(p.x - 3, p.y - 3, p.x + 18, p.y + 18);
        }
    }

    fn process_movement(&mut self) {
        let k = self.k;

        if !self.movable() {
            return;
        }

        let left = self.pressed(Control::Left);
        let right = self.pressed(Control::Right);

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

    fn bleed(&mut self) {
        if self.health() >= self.max_health / 4 || self.rand(self.health().wrapping_add(6) as u32) != 0 {
            return;
        }

        if self.rand(3) == 0 {
            // This must be outside the unpredictable branch below.
            let snd = 18 + self.rand(3) as i32;
            if !self.world.playing(self.world.worm_sound_id(self.i)) {
                self.play_worm_sound(snd);
            }
        }

        self.create_nobject1(6, self.vel(), self.pos());
    }

    fn die(&mut self) {
        let env = self.env;

        self.set_leave_shell_timer(0);
        self.set_make_sight_green(false);

        self.stop_loop_sound_if_any();

        let death_snd = 15 + self.rand(3) as i32;
        self.play_worm_sound(death_snd);

        self.set_fire_cone(0);
        self.rope_mut().out = false;

        if env.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            while self.health() <= 0 {
                self.set_health(self.health() + self.max_health);
                self.set_lives(self.lives() - 1);
            }
        } else {
            self.set_lives(self.lives() - 1);
        }

        let (by, index) = (self.last_killed_by_idx(), self.index());
        let data = &mut *self.world.g.data;
        let old_last_killed = data.last_killed_idx;
        // In Game of Tag, 'it' doesn't change if the killer was not 'it',
        // the worm itself, unknown, or there was no 'it'.
        if env.game_mode != LIERO_GAME_GAME_OF_TAG
            || old_last_killed < 0
            || by < 0
            || by == index
            || by == old_last_killed
        {
            data.last_killed_idx = index;
        }
        data.got_changed = old_last_killed != data.last_killed_idx;

        if by >= 0 && by != index {
            self.world.g.worm_mut(by).kills += 1;
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

        if let Some(s) = self.world.stats() {
            s.after_death(self.i);
        }

        self.release(Control::Fire);
    }

    fn process_alive(&mut self) {
        self.calculate_reaction_forces();
        self.pick_up_bonuses();
        self.process_steerables();

        // process_steerables sets movable to false; does this interfere?
        if !self.movable() && !self.pressed(Control::Left) && !self.pressed(Control::Right) {
            self.set_movable(true);
        } // 2FB1

        self.process_aiming();
        self.process_tasks();
        self.process_weapons();
        self.fire_or_stop();
        self.process_physics();
        self.process_sight();

        if self.pressed(Control::Change) {
            self.process_weapon_change();
        } else {
            self.set_key_change_pressed(false);
            self.process_movement();
        }

        self.bleed();

        if self.health() <= 0 {
            self.die();
        }

        let anim_frame = if self.animate() {
            ((self.env.cycles & 31) >> 3) as usize
        } else {
            0
        };
        self.set_current_frame(angle_frame(self.aiming_angle(), self.direction()) + self.env.anim_tab[anim_frame]);
    }

    /// `checkRespawnPosition`: whether (x, y) is far enough from where the
    /// worm died (old) and from the enemy, with no rock around it.
    fn check_respawn_position(&mut self, enemy: IVec2, old: IVec2, x: i32, y: i32) -> bool {
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

    fn begin_respawn(&mut self) {
        let k = self.k;
        let (_, height) = self.level_size();

        let temp = ftoi_vec(self.pos());
        self.set_logic_respawn(sub(temp, IVec2 { x: 80, y: 80 }));

        let mut enemy = temp;
        // When there are exactly two worms, the other one.
        if self.world.worm_count() == 2 {
            enemy = ftoi_vec(self.world.worm(self.index() ^ 1).pos);
        }

        let mut trials = 0;
        loop {
            let x = (k.worm_spawn_rect_x as u32).wrapping_add(self.rand(k.worm_spawn_rect_w as u32)) as i32;
            let y = (k.worm_spawn_rect_y as u32).wrapping_add(self.rand(k.worm_spawn_rect_h as u32)) as i32;
            let mut pos = IVec2 { x: itof(x), y: itof(y) };

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

    fn do_respawning(&mut self) {
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

        let mut dest = IVec2 {
            x: ipos.x - 80,
            y: ipos.y - 80,
        };
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
        self.set_vel(IVec2::default());
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

        if let Some(s) = self.world.stats() {
            s.after_spawn(self.i);
        }
    }

    fn process_dead(&mut self) {
        self.set_steerable_count(0);

        if self.pressed_once(Control::Fire) {
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

    fn process(&mut self) {
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
