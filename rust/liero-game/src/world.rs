//! The game tick with everything in it: the object lists, creating
//! objects, explosions, worms hitting and being hit, bonuses, blood, the
//! worms and their ropes (`game.cpp`, `weapon.cpp`, `nobject.cpp`,
//! `sobject.cpp`, `bonus.cpp`, `bobject.cpp`, `ninjarope.cpp` and
//! `worm.cpp` in C++). Results have to stay bit-identical to the C++
//! versions, which `tools/hashrun` checks.
//!
//! `World` borrows the whole game for the tick. As in C++, what a step
//! calls can change any object, including the one being processed (an
//! explosion pushes nearby objects, and a new object can reuse a freed
//! slot), so objects are worked on by their slots and read afresh at the
//! points the C++ code read them. Only sounds and the stats' clock call
//! out (`GameHooks`).

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::game::{BonusState, GameFields, GameHooks, Rect, RuleSettings, Rules};
use crate::level::slice;
use crate::math::{cos_sin, ftoi, itof, vector_length, FVec2, Fixed, IVec2};
use crate::object::BObject;
use crate::object::{explosion_damage, explosion_push_object, explosion_push_worm, inside, BonusConstants};
use crate::objects::{Cursor, FiredBy, NObject, WObject};
use crate::sfx::SoundOut;
use crate::stats::Stats;
use crate::tc::{c, h, TcData};
use crate::viewport::Viewport;
use crate::weapon::ShotType;
use crate::weapon::{WeaponView, WormWeapon};
use crate::worm::{Control, WormConstants, WormData, WormEnv, WormId};
use liero_data::model::{NObject as NObjectType, SObject as SObjectType, Weapon};

mod nobject;
mod rope;
mod wobject;
mod worm;

/// `Settings::wormAnimTab`.
const WORM_ANIM_TAB: [i32; 4] = [0, 7, 0, 14];

/// The parts of TC weapon `id`, `w`, that `weapon.rs` uses (C++'s `toRust`).
pub(crate) fn weapon_view(id: i32, w: &Weapon) -> WeaponView {
    WeaponView {
        id,
        detect_distance: w.detect_distance,
        affect_by_worm: w.affect_by_worm,
        blow_away: w.blow_away,
        gravity: w.gravity,
        explo_sound: w.explo_sound,
        speed: w.speed,
        add_speed: w.add_speed,
        distribution: w.distribution,
        parts: w.parts,
        recoil: w.recoil,
        mult_speed: w.mult_speed,
        delay: w.delay,
        loading_time: w.loading_time,
        ammo: w.ammo,
        create_on_exp: w.create_on_exp,
        dirt_effect: w.dirt_effect,
        leave_shells: w.leave_shells,
        leave_shell_delay: w.leave_shell_delay,
        play_reload_sound: w.play_reload_sound,
        expl_ground: w.expl_ground,
        fire_cone: w.fire_cone,
        collide_with_objects: w.collide_with_objects,
        bounce: w.bounce,
        time_to_explo: w.time_to_explo,
        time_to_explo_v: w.time_to_explo_v,
        start_frame: w.start_frame,
        num_frames: w.num_frames,
        loop_anim: w.loop_anim,
        shot_type: ShotType::of(w.shot_type),
        color_bullets: w.color_bullets,
        splinter_amount: w.splinter_amount,
        splinter_colour: w.splinter_colour,
        splinter_type: w.splinter_type,
        splinter_scatter: w.splinter_scatter,
        obj_trail_type: w.obj_trail_type,
        obj_trail_delay: w.obj_trail_delay,
        part_trail_type: w.part_trail_type,
        part_trail_obj: w.part_trail_obj,
        part_trail_delay: w.part_trail_delay,
        loop_sound: w.loop_sound,
        laser_sight: w.laser_sight,
    }
}

/// The whole game, borrowed for a tick, with the clock from C++.
pub(crate) struct World<'a> {
    pub(crate) g: Rules<'a>,
    tc: &'a TcData,
    /// The split-screen and spectator viewports, whose shake and death
    /// banners the tick moves.
    viewports: Vec<&'a mut Viewport>,
    spectators: Vec<&'a mut Viewport>,
    /// The stats the tick records, if any.
    stats: Option<&'a mut Stats>,
    sound: SoundOut,
    h: &'a GameHooks<'a>,
}

impl<'a> World<'a> {
    /// Borrows what `f` points to.
    ///
    /// # Safety
    /// As `Game::new`; the TC, the viewports and the stats (which may be
    /// null) are valid for `'a` too, and nothing else uses the viewports
    /// or the stats meanwhile.
    pub(crate) unsafe fn new(f: &'a GameFields, s: &'a RuleSettings, h: &'a GameHooks<'a>) -> Self {
        unsafe {
            let viewports = |list: *const *mut Viewport, n: i32| -> Vec<&'a mut Viewport> {
                slice(list, n.max(0) as usize).iter().map(|&v| &mut *v).collect()
            };
            World {
                g: Rules::new(f, s),
                tc: &*f.tc,
                viewports: viewports(f.viewports, f.viewport_count),
                spectators: viewports(f.spectator_viewports, f.spectator_viewport_count),
                stats: f.stats.as_mut(),
                sound: f.sound,
                h,
            }
        }
    }

    fn constant(&self, i: usize) -> i32 {
        self.tc.constant(i)
    }

    fn hack(&self, i: usize) -> bool {
        self.tc.hack(i)
    }

    fn weapon(&self, i: i32) -> &'a Weapon {
        assert!(0 <= i, "no weapon {i}");
        &self.tc.model.weapons[i as usize]
    }

    fn nobject_type(&self, i: i32) -> &'a NObjectType {
        assert!(0 <= i, "no object type {i}");
        &self.tc.model.nobjects[i as usize]
    }

    fn sobject_type(&self, i: i32) -> &'a SObjectType {
        assert!(0 <= i, "no object type {i}");
        &self.tc.model.sobjects[i as usize]
    }

    fn rand(&mut self, max: u32) -> u32 {
        self.g.data.rand.below(max)
    }

    fn cycles(&self) -> i32 {
        self.g.data.cycles
    }

    fn worm_count(&self) -> i32 {
        self.g.worm_count()
    }

    /// Worm `i`, a position in the game's worms.
    fn worm(&self, i: i32) -> &WormData {
        self.g.worm(i)
    }

    /// Draws dirt effect `effect` at (x, y), and with shadows on corrects
    /// them around it, from (x1, y1) to (x2, y2).
    fn dirt_effect(&mut self, effect: i32, x: i32, y: i32, shadow: [i32; 4]) {
        let common = self.tc.level_common();
        self.g
            .level
            .draw_dirt_effect(&common, &mut self.g.data.rand, effect, x, y);
        if self.g.s.shadow {
            let [x1, y1, x2, y2] = shadow;
            self.g.level.correct_shadow(&common, x1, y1, x2, y2);
        }
    }

    // Sounds and stats.

    fn play(&self, sound: i32, id: *mut c_void, loops: i32) {
        unsafe { self.sound.play(self.tc, sound, id, loops) }
    }

    fn playing(&self, id: *mut c_void) -> bool {
        unsafe { self.sound.playing(id) }
    }

    fn stop(&self, id: *mut c_void) {
        unsafe { self.sound.stop(id) }
    }

    /// The stats the game records, if any.
    fn stats(&mut self) -> Option<&mut Stats> {
        self.stats.as_deref_mut()
    }

    /// The index in the TC of the weapon in the slot, or -1 for none
    /// (`weaponIndex`).
    fn weapon_index(&self, by: FiredBy) -> i32 {
        if by.worm < 0 {
            -1
        } else {
            self.g.pool_worm(WormId(by.worm as u32)).weapons[by.slot as usize].kind
        }
    }

    fn damage_potential(&mut self, by: i32, weapon: FiredBy, hp: i32) {
        if self.stats.is_some() {
            let w = self.weapon_index(weapon);
            self.stats.as_deref_mut().unwrap().damage_potential(by, w, hp);
        }
    }

    fn damage_dealt(&mut self, by: i32, weapon: FiredBy, to: i32, hp: i32, has_hit: bool) {
        if self.stats.is_some() {
            let pos = self.worm(to).pos;
            let w = self.weapon_index(weapon);
            let s = self.stats.as_deref_mut().unwrap();
            s.damage_dealt(by, w, to, pos.x.to_int(), pos.y.to_int(), hp, has_hit);
        }
    }

    fn shot(&mut self, by: i32, weapon: FiredBy) {
        if self.stats.is_some() {
            let w = self.weapon_index(weapon);
            self.stats.as_deref_mut().unwrap().shot(by, w);
        }
    }

    fn hit(&mut self, by: i32, weapon: FiredBy, to: i32) {
        if self.stats.is_some() {
            let w = self.weapon_index(weapon);
            self.stats.as_deref_mut().unwrap().hit(by, w, to);
        }
    }

    /// The stats' start or end of a tick.
    fn stats_tick(&mut self, end: bool) {
        if self.stats.is_some() {
            let ticks = (self.h.hires_ticks)();
            let worms = self.g.stats_worms();
            let s = self.stats.as_deref_mut().unwrap();
            if end {
                s.tick(ticks, &worms);
            } else {
                s.pre_tick(ticks, &worms);
            }
        }
    }

    /// The id worm `i`'s own sounds play with.
    fn worm_sound_id(&self, i: i32) -> *mut c_void {
        std::ptr::from_ref(self.worm(i)).cast_mut().cast()
    }

    /// A worm's hurt sound: one of three, unless one of its own is playing.
    fn hurt_sound(&mut self, worm: i32) {
        let snd = 18 + self.rand(3) as i32; // NOTE: MUST be outside the unpredictable branch below
        let id = self.worm_sound_id(worm);
        if !self.playing(id) {
            self.play(snd, id, 0);
        }
    }

    // Creating objects.

    /// `Game::createBObject`.
    fn create_bobject(&mut self, pos: FVec2, vel: FVec2) {
        // Taking the slot uses no random numbers, so the colour can come first.
        let n = self.constant(c("NumBloodColours")) as u32;
        let color = self.rand(n) as i32 + self.constant(c("FirstBloodColour"));
        *self.g.data.bobjects.new_object_reuse() = BObject { pos, vel, color };
    }

    /// `createSObject`: an explosion or smoke, which hurts and pushes what
    /// is near when it does damage.
    #[allow(clippy::too_many_arguments)]
    fn create_sobject(&mut self, kind: i32, x: i32, y: i32, owner_idx: i32, fired_by: FiredBy, from: Option<usize>) {
        let t = self.sobject_type(kind);
        let slot = self.g.data.sobjects.new_object_reuse();

        assert!(t.num_sounds < 10);

        if t.start_sound >= 0 {
            let snd = self.rand(t.num_sounds as u32) as i32 + t.start_sound;
            self.play(snd, null_mut(), 0);
        }

        for v in &mut self.viewports {
            let r = v.rect;
            if x > v.x && x < v.x + (r.x2 - r.x1) && y > v.y && y < v.y + (r.y2 - r.y1) && itof(t.shake) > v.shake {
                v.shake = itof(t.shake);
            }
        }

        let obj = &mut self.g.data.sobjects.arr[slot];
        obj.id = kind;
        obj.x = x - 8;
        obj.y = y - 8;
        obj.cur_frame = 0;
        obj.anim_delay = t.anim_delay;

        if t.flash > self.g.data.screen_flash {
            self.g.data.screen_flash = t.flash;
        }

        self.damage_potential(owner_idx, fired_by, t.damage);

        if t.damage > 0 {
            for i in 0..self.worm_count() {
                let w = self.g.worm_mut(i);

                let mut power_sum = 0;
                if explosion_push_worm(w.pos, &mut w.vel, x, y, t.detect_range, t.blow_away, &mut power_sum) {
                    let z = explosion_damage(t.damage, power_sum, t.detect_range);

                    if let Some(from) = from {
                        if !self.g.data.wobjects.arr[from].has_hit {
                            self.hit(owner_idx, fired_by, i);
                            self.g.data.wobjects.arr[from].has_hit = true;
                        }
                    }

                    if self.worm(i).health > 0 {
                        self.g.damage(i, z, owner_idx);
                        self.damage_dealt(owner_idx, fired_by, i, z, false);

                        let blood_amount = self.g.s.blood.wrapping_mul(power_sum) / 100;

                        for _ in 0..blood_amount.max(0) {
                            let angle = self.rand(128) as i32;
                            let w = self.worm(i);
                            let (vel, pos, index) = (w.vel / 3, w.pos, w.index);
                            self.create_nobject2(6, angle, vel, pos, 0, index, fired_by);
                        }

                        if self.rand(3) == 0 {
                            self.hurt_sound(i);
                        }
                    }
                }
            }

            let obj_blow_away = t.blow_away / 3; // TODO in C++: Read from EXE

            let mut wr = Cursor::default();
            while let Some(n) = wr.next(&self.g.data.wobjects) {
                let i = &mut self.g.data.wobjects.arr[n];
                let weapon = &self.tc.model.weapons[i.kind as usize];

                if weapon.affect_by_explosions
                    && explosion_push_object(i.pos, &mut i.vel, x, y, t.detect_range, obj_blow_away)
                    && weapon.chain_explosion
                {
                    self.blow_up_wobject(n, owner_idx);
                }
            }

            let mut nr = Cursor::default();
            while let Some(n) = nr.next(&self.g.data.nobjects) {
                let i = &mut self.g.data.nobjects.arr[n];
                let nt = &self.tc.model.nobjects[i.kind as usize];
                if nt.affect_by_explosions {
                    explosion_push_object(i.pos, &mut i.vel, x, y, t.detect_range, obj_blow_away);
                }
            }

            let width = t.detect_range / 2;
            let rect = Rect {
                x1: (x - width).max(0),
                y1: (y - width).max(0),
                x2: (x + width + 1).min(self.g.level.width),
                y2: (y + width + 1).min(self.g.level.height),
            };

            for cy in rect.y1..rect.y2 {
                for cx in rect.x1..rect.x2 {
                    let any_dirt = self.g.level.mat(cx, cy).any_dirt();
                    let pix = self.g.level.pixel(cx, cy);
                    if any_dirt && self.rand(8) == 0 {
                        let angle = self.rand(128) as i32;
                        self.create_nobject2(
                            2,
                            angle,
                            FVec2::default(),
                            FVec2::new(Fixed::from_int(cx), Fixed::from_int(cy)),
                            i32::from(pix),
                            owner_idx,
                            fired_by,
                        );
                    }
                }
            }
        }

        if t.dirt_effect >= 0 {
            self.dirt_effect(t.dirt_effect, x - 7, y - 7, [x - 10, y - 10, x + 11, y + 11]);
        }

        let mut br = Cursor::default();
        while let Some(i) = br.next(&self.g.data.bonuses) {
            let b = &self.g.data.bonuses.arr[i];
            let (ix, iy) = (ftoi(b.x), ftoi(b.y));

            if ix > x - t.detect_range && ix < x + t.detect_range && iy > y - t.detect_range && iy < y + t.detect_range
            {
                self.g.data.bonuses.free(br.last());
                self.create_sobject(0, ix, iy, owner_idx, fired_by, None);
            }
        }
    }

    /// `checkForSpecWormHit`: whether (x, y) is within `dist` of a worm
    /// pixel of worm `i`.
    fn spec_worm_hit(&self, x: i32, y: i32, dist: i32, i: i32) -> bool {
        let w = self.worm(i);
        if !w.visible {
            return false;
        }

        let sprite = self
            .tc
            .worm_sprites
            .sprite((w.current_frame + w.direction * 7 * 3) as usize);

        let delta_x = x - w.pos.x.to_int() + 7;
        let delta_y = y - w.pos.y.to_int() + 5;

        let x1 = (delta_x - dist).max(0);
        let y1 = (delta_y - dist).max(0);
        let x2 = (delta_x + dist + 1).min(16);
        let y2 = (delta_y + dist + 1).min(16);

        let materials = &self.tc.materials;
        for cy in y1..y2 {
            for cx in x1..x2 {
                assert!(cy * 16 + cx < 16 * 16);
                let pix = sprite[(cy * 16 + cx) as usize];
                if materials[pix as usize].flags & crate::material::MATERIAL_WORM != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// `checkForWormHit`: tests only the first worm that isn't `own`.
    fn worm_hit(&self, x: i32, y: i32, dist: i32, own: i32) -> bool {
        for i in 0..self.worm_count() {
            if i != own {
                return self.spec_worm_hit(x, y, dist, i);
            }
        }
        false
    }

    /// `SObject::process`.
    fn process_sobject(&mut self, slot: usize) {
        let t = self.sobject_type(self.g.data.sobjects.arr[slot].id);
        let l = &mut self.g.data.sobjects;
        if l.arr[slot].animate(t.anim_delay, t.num_frames) {
            l.free(slot);
        }
    }

    /// `Bonus::process`.
    fn process_bonus(&mut self, slot: usize) {
        let ix = ftoi(self.g.data.bonuses.arr[slot].x);
        assert!(ix >= 0 && ix < self.g.level.width);

        let k = BonusConstants {
            gravity: self.constant(c("BonusGravity")),
            bounce_mul: self.constant(c("BonusBounceMul")),
            bounce_div: self.constant(c("BonusBounceDiv")),
        };

        if self.g.data.bonuses.arr[slot].process(&self.g.level, &k) {
            let b = self.g.data.bonuses.arr[slot];
            let kind = self.tc.model.bonus_sobjects[b.frame as usize];
            self.create_sobject(kind, ix, ftoi(b.y), 0, FiredBy::NONE, None);
            // The explosion may have freed it.
            if self.g.data.bonuses.arr[slot].used {
                self.g.data.bonuses.free(slot);
            }
        }
    }

    /// The blood particles' loop in `Game::processFrame`.
    fn process_bobjects(&mut self) {
        let gravity = self.constant(c("BObjGravity"));
        let common = self.tc.level_common();
        let (data, level) = (&mut *self.g.data, &mut self.g.level);
        let l = &mut data.bobjects;
        let mut i = 0;
        while i < l.count {
            if l.arr[i].process(level, &common, &mut data.rand, gravity) {
                i += 1;
            } else {
                l.free(i);
            }
        }
    }
    fn worm_constants(&self) -> WormConstants {
        let k = |name| self.constant(c(name));
        WormConstants {
            min_bounce_up: k("MinBounceUp"),
            min_bounce_down: k("MinBounceDown"),
            min_bounce_left: k("MinBounceLeft"),
            min_bounce_right: k("MinBounceRight"),
            worm_gravity: k("WormGravity"),
            walk_vel_left: k("WalkVelLeft"),
            max_vel_left: k("MaxVelLeft"),
            walk_vel_right: k("WalkVelRight"),
            max_vel_right: k("MaxVelRight"),
            jump_force: k("JumpForce"),
            max_aim_vel_left: k("MaxAimVelLeft"),
            aim_acc_left: k("AimAccLeft"),
            max_aim_vel_right: k("MaxAimVelRight"),
            aim_acc_right: k("AimAccRight"),
            nr_initial_length: k("NRInitialLength"),
            nr_min_length: k("NRMinLength"),
            nr_max_length: k("NRMaxLength"),
            nr_throw_vel_x: k("NRThrowVelX"),
            nr_throw_vel_y: k("NRThrowVelY"),
            nr_pull_vel: k("NRPullVel"),
            nr_release_vel: k("NRReleaseVel"),
            worm_fric_mult: k("WormFricMult"),
            worm_fric_div: k("WormFricDiv"),
            worm_min_spawn_dist_last: k("WormMinSpawnDistLast"),
            worm_min_spawn_dist_enemy: k("WormMinSpawnDistEnemy"),
            worm_spawn_rect_x: k("WormSpawnRectX"),
            worm_spawn_rect_y: k("WormSpawnRectY"),
            worm_spawn_rect_w: k("WormSpawnRectW"),
            worm_spawn_rect_h: k("WormSpawnRectH"),
            aim_fric_mult: k("AimFricMult"),
            aim_fric_div: k("AimFricDiv"),
            aim_max_right: k("AimMaxRight"),
            aim_min_right: k("AimMinRight"),
            aim_max_left: k("AimMaxLeft"),
            aim_min_left: k("AimMinLeft"),
            bonus_explode_risk: k("BonusExplodeRisk"),
            bonus_health_var: k("BonusHealthVar"),
            bonus_min_health: k("BonusMinHealth"),
            laser_weapon: k("LaserWeapon"),
            fall_damage_right: k("FallDamageRight"),
            fall_damage_down: k("FallDamageDown"),
            worm_float_level: k("WormFloatLevel"),
            worm_float_power: k("WormFloatPower"),
            fall_damage: self.hack(h("FallDamage")),
            worm_float: self.hack(h("WormFloat")),
            bonus_reload_only: self.hack(h("BonusReloadOnly")),
            air_jump: self.hack(h("AirJump")),
            multi_jump: self.hack(h("MultiJump")),
            signed_recoil: self.hack(h("SignedRecoil")),
        }
    }

    /// `Game::createBonus`.
    fn create_bonus(&mut self) {
        let s = self.g.s;

        if self.g.data.bonuses.count as i32 >= s.max_bonuses {
            return;
        }

        for _ in 0..50000 {
            let mut ix = self.rand(s.bonus_spawn_rect_w as u32) as i32;
            let mut iy = self.rand(s.bonus_spawn_rect_h as u32) as i32;

            if s.bonus_spawn_rect {
                ix += s.bonus_spawn_rect_x;
                iy += s.bonus_spawn_rect_y;
            }

            if !self.g.bonus_spawn_position_free(ix, iy) {
                continue;
            }

            let frame = if s.bonus_only_health {
                1
            } else if s.bonus_only_weapon {
                0
            } else {
                self.rand(2) as i32
            };

            let Some(slot) = self.g.data.bonuses.new_object() else {
                return;
            };

            let [min_time, rand_time] = s.bonus_rand_timer[frame as usize];
            let mut b = BonusState {
                x: itof(ix),
                y: itof(iy),
                vel_y: 0,
                frame,
                timer: self.rand(rand_time as u32).wrapping_add(min_time as u32) as i32,
                weapon: 0,
            };

            if frame == 0 {
                loop {
                    b.weapon = self.rand(s.weapon_count as u32) as i32;
                    // SAFETY: weap_table has weapon_count entries.
                    if unsafe { *s.weap_table.add(b.weapon as usize) } != 2 {
                        break;
                    }
                }
            }

            let bonus = &mut self.g.data.bonuses.arr[slot];
            bonus.x = b.x;
            bonus.y = b.y;
            bonus.vel_y = b.vel_y;
            bonus.frame = b.frame;
            bonus.timer = b.timer;
            // Health bonuses keep the weapon of the bonus that used the slot before.
            if b.frame == 0 {
                bonus.weapon = b.weapon;
            }

            self.create_sobject(7, ix, iy, 0, FiredBy::NONE, None);
            return;
        } // 234F
    }

    /// Screen flash and viewport shake at the start of the tick.
    fn begin_tick(&mut self) {
        self.stats_tick(false);

        if self.g.data.screen_flash > 0 {
            self.g.data.screen_flash -= 1;
        }

        for v in self.viewports.iter_mut().chain(&mut self.spectators) {
            if v.shake > 0 {
                v.shake -= 4000; // TODO in C++: Read 4000 from exe?
            }
        }
    }

    /// Moves the viewports' death banners.
    fn update_banners(&mut self) {
        fn move_banner(banner_y: &mut i32, down: bool) {
            if down {
                if *banner_y < 2 {
                    *banner_y += 1;
                }
            } else if *banner_y > -8 {
                *banner_y -= 1;
            }
        }

        let g = &self.g;
        let killed = |i: i32| g.worm(i).killed_timer > 16;

        for v in &mut self.viewports {
            move_banner(&mut v.banner_y, killed(v.worm_idx));
        }

        for v in &mut self.spectators {
            move_banner(&mut v.banner_y, killed(0) || killed(1));
        }
    }

    /// `Game::processFrame`, but for the viewports' own processing, which
    /// `Game::process_frame` does after.
    pub(crate) fn process_frame(&mut self) {
        let s = self.g.s;

        self.begin_tick();

        let mut br = Cursor::default();
        while let Some(i) = br.next(&self.g.data.bonuses) {
            self.process_bonus(i);
        }

        if self.cycles() & 1 == 0 {
            self.update_banners();
        }

        let mut sr = Cursor::default();
        while let Some(i) = sr.next(&self.g.data.sobjects) {
            self.process_sobject(i);
        }

        let mut wr = Cursor::default();
        while let Some(i) = wr.next(&self.g.data.wobjects) {
            self.process_wobject(i);
        }

        let mut nr = Cursor::default();
        while let Some(i) = nr.next(&self.g.data.nobjects) {
            self.process_nobject(i);
        }

        self.process_bobjects();

        // This was originally the beginning of the tick, but has been
        // rotated down to separate out the drawing.
        self.g.data.cycles += 1;

        if !s.bonus_disable && s.max_bonuses > 0 && self.rand(s.bonus_drop_chance as u32) == 0 {
            self.create_bonus();
        }

        for i in 0..self.worm_count() {
            self.process_worm(i);
        }
        for i in 0..self.worm_count() {
            self.process_rope(i);
        }

        self.g.game_mode_tick();

        // Store old control states so we can see what changes (mainly for
        // replays). Neither the viewports nor the stats at the end read
        // them.
        for i in 0..self.worm_count() {
            let w = self.g.worm_mut(i);
            w.prev_control_states = w.control_states;
        }

        self.stats_tick(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_and_hack_indexes_follow_the_cpp_enums() {
        assert_eq!(c("NRInitialLength"), 0);
        assert_eq!(c("RemExpObject"), liero_data::exe::CONSTANT_ORDER.len() - 1);
        assert_eq!(h("FallDamage"), 0);
        assert_eq!(h("MultiJump"), 10);
    }
}
