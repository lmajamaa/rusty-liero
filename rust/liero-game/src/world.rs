//! The game tick with everything in it: the object lists, creating
//! objects, explosions, worms hitting and being hit, bonuses, blood, the
//! worms and their ropes (`game.cpp`, `weapon.cpp`, `nobject.cpp`,
//! `sobject.cpp`, `bonus.cpp`, `bobject.cpp`, `ninjarope.cpp` and
//! `worm.cpp` in C++). Results have to stay bit-identical to the C++
//! versions, which `tools/hashrun` checks.
//!
//! The physics of each kind of object is in `object.rs`, `weapon.rs` and
//! `worm.rs`, which call back through tables of hooks. The hooks are here,
//! in Rust; only sounds, stats and the viewports' own processing call out
//! to C++ (`LieroGameHooks`).
//!
//! As in C++, a hook can change any object, including the one being
//! processed (an explosion pushes nearby objects, and a new object can
//! reuse a freed slot), so objects are worked on through raw pointers and
//! read at the points the C++ code read them.

use std::ffi::c_void;
use std::ptr::{addr_of_mut, null_mut};

use crate::game::{stats_worms, Game, LieroBonusState, LieroGameHooks, LieroRect};
use crate::level::LieroLevelCommon;
use crate::math::{cos_sin, ftoi, itof, vector_length, LieroIVec2};
use crate::object::LieroBObject;
use crate::object::{
    add, div, ftoi_vec, inside, liero_explosion_damage, liero_explosion_push_object, liero_explosion_push_worm, sub,
    LieroBonusConstants,
};
use crate::objects::{
    Cursor, LieroBObjectList, LieroBonusList, LieroNObject, LieroNObjectList, LieroSObjectList, LieroWObject,
    LieroWObjectList,
};
use crate::rand::LieroRand;
use crate::stats::{
    liero_stats_after_death, liero_stats_after_spawn, liero_stats_damage_dealt, liero_stats_damage_potential,
    liero_stats_hit, liero_stats_pre_tick, liero_stats_shot, liero_stats_tick, LieroStats,
};
use crate::tc::{c, h, LieroTcData, LieroTcNObject, LieroTcSObject, LieroTcWeapon};
use crate::weapon::{LieroWeapon, LieroWormFiringHooks, LieroWormWeapon};
use crate::worm::{
    liero_worm_process, LieroBonusRef, LieroSteerable, LieroWormConstants, LieroWormData, LieroWormEnv, LieroWormHooks,
    WormId, LIERO_CONTROL_CHANGE, LIERO_CONTROL_FIRE, LIERO_CONTROL_UP,
};

mod nobject;
mod rope;
mod wobject;

/// `Settings::wormAnimTab`.
const WORM_ANIM_TAB: [i32; 4] = [0, 7, 0, 14];

/// The parts of a TC weapon that `weapon.rs` uses (C++'s `toRust`).
pub(crate) fn weapon_view(w: &LieroTcWeapon) -> LieroWeapon {
    LieroWeapon {
        id: w.id,
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
        shot_type: w.shot_type,
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

/// `v * m / d` on both components, rounding towards zero like C++.
fn mul_div(v: LieroIVec2, m: i32, d: i32) -> LieroIVec2 {
    LieroIVec2 {
        x: v.x.wrapping_mul(m) / d,
        y: v.y.wrapping_mul(m) / d,
    }
}

/// The whole game, with the hooks to C++.
#[derive(Clone, Copy)]
pub(crate) struct World<'a> {
    pub(crate) g: Game<'a>,
    pub(crate) h: &'a LieroGameHooks,
}

// SAFETY for the methods: the pointers in the game's fields are valid for
// the call, as `liero_game_process_frame` requires.
impl<'a> World<'a> {
    fn data(&self) -> *mut crate::game::LieroGameData {
        self.g.f.game
    }

    fn tc(&self) -> &'a LieroTcData {
        unsafe { &*self.g.f.tc }
    }

    fn constant(&self, i: usize) -> i32 {
        unsafe { *self.tc().constants.add(i) }
    }

    fn hack(&self, i: usize) -> bool {
        unsafe { *self.tc().hacks.add(i) }
    }

    fn weapon(&self, i: i32) -> *const LieroTcWeapon {
        let tc = self.tc();
        assert!(0 <= i && (i as usize) < tc.weapon_count);
        unsafe { tc.weapons.add(i as usize) }
    }

    fn nobject_type(&self, i: i32) -> &'a LieroTcNObject {
        let tc = self.tc();
        assert!(0 <= i && (i as usize) < tc.nobject_count);
        unsafe { &*tc.nobjects.add(i as usize) }
    }

    fn sobject_type(&self, i: i32) -> &'a LieroTcSObject {
        let tc = self.tc();
        assert!(0 <= i && (i as usize) < tc.sobject_count);
        unsafe { &*tc.sobjects.add(i as usize) }
    }

    /// `Common::levelCommon`.
    fn common(&self) -> LieroLevelCommon {
        let tc = self.tc();
        LieroLevelCommon {
            materials: tc.materials.as_ptr(),
            textures: tc.textures.as_ptr(),
            large_sprites: tc.large_sprites.pixels().as_ptr(),
            large_sprite_count: tc.large_sprites.count,
        }
    }

    fn rand(&self, max: u32) -> u32 {
        unsafe { (*self.data()).rand.below(max) }
    }

    fn rand_ptr(&self) -> *mut LieroRand {
        unsafe { addr_of_mut!((*self.data()).rand) }
    }

    fn cycles(&self) -> i32 {
        unsafe { (*self.data()).cycles }
    }

    fn worm_count(&self) -> i32 {
        self.g.f.worm_count
    }

    /// The game's worm `id`, listed or not.
    fn pool_worm(&self, id: WormId) -> *mut LieroWormData {
        let f = &self.g.f;
        assert!((id.0 as usize) < f.pool_count, "no worm {id:?}");
        // SAFETY: the pool holds `pool_count` worms.
        unsafe { *f.pool.add(id.0 as usize) }
    }

    /// Worm `i`, a position in the game's worms.
    fn worm(&self, i: i32) -> *mut LieroWormData {
        self.g.worm(i).data
    }

    fn wobjects(&self) -> *mut LieroWObjectList {
        unsafe { addr_of_mut!((*self.data()).wobjects) }
    }

    fn nobjects(&self) -> *mut LieroNObjectList {
        unsafe { addr_of_mut!((*self.data()).nobjects) }
    }

    fn sobjects(&self) -> *mut LieroSObjectList {
        unsafe { addr_of_mut!((*self.data()).sobjects) }
    }

    fn bonuses(&self) -> *mut LieroBonusList {
        unsafe { addr_of_mut!((*self.data()).bonuses) }
    }

    fn bobjects(&self) -> *mut LieroBObjectList {
        unsafe { addr_of_mut!((*self.data()).bobjects) }
    }

    // Sounds and stats.

    fn play(&self, sound: i32, id: *mut c_void, loops: i32) {
        unsafe { self.g.f.sound.play(self.tc(), sound, id, loops) }
    }

    fn playing(&self, id: *mut c_void) -> bool {
        unsafe { self.g.f.sound.playing(id) }
    }

    fn stop(&self, id: *mut c_void) {
        unsafe { self.g.f.sound.stop(id) }
    }

    /// The stats the game records, if any.
    fn stats(&self) -> Option<&mut LieroStats> {
        unsafe { self.g.f.stats.as_mut() }
    }

    /// The weapon's index in the TC, or -1 for none (`weaponIndex`).
    fn weapon_index(weapon: *mut LieroWormWeapon) -> i32 {
        if weapon.is_null() {
            -1
        } else {
            unsafe { (*((*weapon).kind as *const LieroTcWeapon)).id }
        }
    }

    fn damage_potential(&self, by: i32, weapon: *mut LieroWormWeapon, hp: i32) {
        if let Some(s) = self.stats() {
            liero_stats_damage_potential(s, by, Self::weapon_index(weapon), hp);
        }
    }

    fn damage_dealt(&self, by: i32, weapon: *mut LieroWormWeapon, to: i32, hp: i32, has_hit: bool) {
        if let Some(s) = self.stats() {
            let pos = unsafe { (*self.worm(to)).pos };
            let w = Self::weapon_index(weapon);
            liero_stats_damage_dealt(s, by, w, to, ftoi(pos.x), ftoi(pos.y), hp, has_hit);
        }
    }

    fn shot(&self, by: i32, weapon: *mut LieroWormWeapon) {
        if let Some(s) = self.stats() {
            liero_stats_shot(s, by, Self::weapon_index(weapon));
        }
    }

    fn hit(&self, by: i32, weapon: *mut LieroWormWeapon, to: i32) {
        if let Some(s) = self.stats() {
            liero_stats_hit(s, by, Self::weapon_index(weapon), to);
        }
    }

    /// The stats' start or end of a tick.
    fn stats_tick(&self, end: bool) {
        if let Some(s) = self.stats() {
            unsafe {
                let ticks = (self.h.hires_ticks)(self.h.ctx);
                let worms = stats_worms(self.g.f);
                if end {
                    liero_stats_tick(s, ticks, worms.as_ptr(), worms.len());
                } else {
                    liero_stats_pre_tick(s, ticks, worms.as_ptr(), worms.len());
                }
            }
        }
    }

    /// A worm's hurt sound: one of three, unless one of its own is playing.
    fn hurt_sound(&self, worm: i32) {
        let snd = 18 + self.rand(3) as i32; // NOTE: MUST be outside the unpredictable branch below
        let id = self.worm(worm).cast();
        if !self.playing(id) {
            self.play(snd, id, 0);
        }
    }

    // Creating objects.

    /// `Game::createBObject`.
    fn create_bobject(&self, pos: LieroIVec2, vel: LieroIVec2) {
        // Taking the slot uses no random numbers, so the colour can come first.
        let color = self.rand(self.constant(c("NumBloodColours")) as u32) as i32 + self.constant(c("FirstBloodColour"));
        // SAFETY: nothing else has the list during the call.
        *unsafe { &mut *self.bobjects() }.new_object_reuse() = LieroBObject { pos, vel, color };
    }

    /// `createSObject`: an explosion or smoke, which hurts and pushes what
    /// is near when it does damage.
    #[allow(clippy::too_many_arguments)]
    fn create_sobject(
        &self,
        kind: i32,
        x: i32,
        y: i32,
        owner_idx: i32,
        fired_by: *mut LieroWormWeapon,
        from: Option<usize>,
    ) {
        let t = self.sobject_type(kind);
        unsafe {
            let obj = addr_of_mut!((*self.sobjects()).arr[(*self.sobjects()).new_object_reuse()]);

            assert!(t.num_sounds < 10);

            if t.start_sound >= 0 {
                self.play(self.rand(t.num_sounds as u32) as i32 + t.start_sound, null_mut(), 0);
            }

            for i in 0..self.g.f.viewport_count {
                let v = *self.g.f.viewports.add(i as usize);
                let r = (*v).rect;
                if x > (*v).x
                    && x < (*v).x + (r.x2 - r.x1)
                    && y > (*v).y
                    && y < (*v).y + (r.y2 - r.y1)
                    && itof(t.shake) > (*v).shake
                {
                    (*v).shake = itof(t.shake);
                }
            }

            (*obj).id = t.id;
            (*obj).x = x - 8;
            (*obj).y = y - 8;
            (*obj).cur_frame = 0;
            (*obj).anim_delay = t.anim_delay;

            if t.flash > (*self.data()).screen_flash {
                (*self.data()).screen_flash = t.flash;
            }

            self.damage_potential(owner_idx, fired_by, t.damage);

            if t.damage > 0 {
                for i in 0..self.worm_count() {
                    let w = self.worm(i);

                    let mut power_sum = 0;
                    if liero_explosion_push_worm(
                        (*w).pos,
                        &mut (*w).vel,
                        x,
                        y,
                        t.detect_range,
                        t.blow_away,
                        &mut power_sum,
                    ) {
                        let z = liero_explosion_damage(t.damage, power_sum, t.detect_range);

                        if let Some(from) = from {
                            let from = addr_of_mut!((*self.wobjects()).arr[from]);
                            if !(*from).has_hit {
                                self.hit(owner_idx, fired_by, i);
                                (*from).has_hit = true;
                            }
                        }

                        if (*w).health > 0 {
                            self.g.damage(i, z, owner_idx);
                            self.damage_dealt(owner_idx, fired_by, i, z, false);

                            let blood_amount = self.g.s.blood.wrapping_mul(power_sum) / 100;

                            for _ in 0..blood_amount.max(0) {
                                let angle = self.rand(128) as i32;
                                self.create_nobject2(6, angle, div((*w).vel, 3), (*w).pos, 0, (*w).index, fired_by);
                            }

                            if self.rand(3) == 0 {
                                self.hurt_sound(i);
                            }
                        }
                    }
                }

                let obj_blow_away = t.blow_away / 3; // TODO in C++: Read from EXE

                let mut wr = Cursor::default();
                while let Some(n) = wr.next(&*self.wobjects()) {
                    let i = &mut (*self.wobjects()).arr[n];
                    let weapon = &*(i.kind as *const LieroTcWeapon);

                    if weapon.affect_by_explosions
                        && liero_explosion_push_object(i.pos, &mut i.vel, x, y, t.detect_range, obj_blow_away)
                        && weapon.chain_explosion
                    {
                        self.blow_up_wobject(n, owner_idx);
                    }
                }

                let mut nr = Cursor::default();
                while let Some(n) = nr.next(&*self.nobjects()) {
                    let i = &mut (*self.nobjects()).arr[n];
                    let nt = &*(i.kind as *const LieroTcNObject);
                    if nt.affect_by_explosions {
                        liero_explosion_push_object(i.pos, &mut i.vel, x, y, t.detect_range, obj_blow_away);
                    }
                }

                let width = t.detect_range / 2;
                let level = self.g.f.level;
                let rect = LieroRect {
                    x1: (x - width).max(0),
                    y1: (y - width).max(0),
                    x2: (x + width + 1).min(level.width),
                    y2: (y + width + 1).min(level.height),
                };

                for cy in rect.y1..rect.y2 {
                    for cx in rect.x1..rect.x2 {
                        let (any_dirt, pix) = {
                            let lv = level.get();
                            (lv.mat(cx, cy).any_dirt(), lv.pixel(cx, cy))
                        };
                        if any_dirt && self.rand(8) == 0 {
                            let angle = self.rand(128) as i32;
                            self.create_nobject2(
                                2,
                                angle,
                                LieroIVec2::default(),
                                LieroIVec2 {
                                    x: itof(cx),
                                    y: itof(cy),
                                },
                                pix as i32,
                                owner_idx,
                                fired_by,
                            );
                        }
                    }
                }
            }

            if t.dirt_effect >= 0 {
                let mut level = self.g.f.level.get();
                let common = self.tc().level_common();
                level.draw_dirt_effect(&common, &mut *self.rand_ptr(), t.dirt_effect, x - 7, y - 7);

                if self.g.s.shadow {
                    level.correct_shadow(&common, x - 10, y - 10, x + 11, y + 11);
                }
            }

            let mut br = Cursor::default();
            while let Some(i) = br.next(&*self.bonuses()) {
                let b = &(*self.bonuses()).arr[i];
                let (ix, iy) = (ftoi(b.x), ftoi(b.y));

                if ix > x - t.detect_range
                    && ix < x + t.detect_range
                    && iy > y - t.detect_range
                    && iy < y + t.detect_range
                {
                    (*self.bonuses()).free(br.last());
                    self.create_sobject(0, ix, iy, owner_idx, fired_by, None);
                }
            }
        }
    }

    /// `checkForSpecWormHit`: whether (x, y) is within `dist` of a worm
    /// pixel of worm `i`.
    fn spec_worm_hit(&self, x: i32, y: i32, dist: i32, i: i32) -> bool {
        unsafe {
            let w = self.worm(i);
            if !(*w).visible {
                return false;
            }

            let sprite = self
                .tc()
                .worm_sprites
                .frame((*w).current_frame + (*w).direction * 7 * 3);

            let delta_x = x - ftoi((*w).pos.x) + 7;
            let delta_y = y - ftoi((*w).pos.y) + 5;

            let x1 = (delta_x - dist).max(0);
            let y1 = (delta_y - dist).max(0);
            let x2 = (delta_x + dist + 1).min(16);
            let y2 = (delta_y + dist + 1).min(16);

            let materials = &self.tc().materials;
            for cy in y1..y2 {
                for cx in x1..x2 {
                    assert!(cy * 16 + cx < 16 * 16);
                    let pix = sprite[(cy * 16 + cx) as usize];
                    if materials[pix as usize].flags & crate::material::LIERO_MATERIAL_WORM != 0 {
                        return true;
                    }
                }
            }
            false
        }
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
    fn process_sobject(&self, slot: usize) {
        // SAFETY: nothing else has the list during the call.
        let l = unsafe { &mut *self.sobjects() };
        let t = self.sobject_type(l.arr[slot].id);
        if l.arr[slot].animate(t.anim_delay, t.num_frames) {
            l.free(slot);
        }
    }

    /// `Bonus::process`.
    fn process_bonus(&self, slot: usize) {
        unsafe {
            let level = self.g.f.level;
            let b = addr_of_mut!((*self.bonuses()).arr[slot]);
            let ix = ftoi((*b).x);
            assert!(ix >= 0 && ix < level.width);

            let k = LieroBonusConstants {
                gravity: self.constant(c("BonusGravity")),
                bounce_mul: self.constant(c("BonusBounceMul")),
                bounce_div: self.constant(c("BonusBounceDiv")),
            };

            if (*b).process(&level, &k) {
                let iy = ftoi((*b).y);
                let kind = self.tc().bonus_sobjects[(*b).frame as usize];
                self.create_sobject(kind, ix, iy, 0, null_mut(), None);
                // The explosion may have freed it.
                if (*b).used {
                    (*self.bonuses()).free(slot);
                }
            }
        }
    }

    /// The blood particles' loop in `Game::processFrame`.
    fn process_bobjects(&self) {
        unsafe {
            let l = &mut *self.bobjects();
            let rand = &mut *self.rand_ptr();
            let level = self.g.f.level;
            let common = self.common();
            let gravity = self.constant(c("BObjGravity"));
            let mut i = 0;
            while i < l.count {
                if l.arr[i].process(&level, &common, rand, gravity) {
                    i += 1;
                } else {
                    l.free(i);
                }
            }
        }
    }

    fn worm_constants(&self) -> LieroWormConstants {
        let k = |name| self.constant(c(name));
        LieroWormConstants {
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

    /// `Worm::process` for worm `i`.
    fn process_worm(&self, i: i32) {
        unsafe {
            let worm = self.worm(i);
            let s = self.g.s;
            let mut context = WormContext {
                w: *self,
                i,
                worm,
                bonuses: Cursor::default(),
                wobjects: Cursor::default(),
            };
            let ctx: *mut c_void = (&mut context as *mut WormContext).cast();

            let k = self.worm_constants();
            let env = LieroWormEnv {
                level: self.g.f.level,
                common: self.common(),
                game: self.data(),
                cycles: self.cycles(),
                game_mode: s.game_mode,
                blood: s.blood,
                loading_time: s.loading_time,
                load_change: s.load_change,
                shadow: s.shadow,
                quick_sim: (*self.data()).quick_sim,
                anim_tab: WORM_ANIM_TAB,
            };

            let firing = LieroWormFiringHooks {
                ctx,
                launch_sound: worm_launch_sound,
                fire_part: worm_fire_part,
                play_sound: worm_play_sound,
                create_nobject1: worm_create_nobject1,
            };

            let hooks = LieroWormHooks {
                ctx,
                firing: &firing,
                weapon: worm_weapon,
                give_weapon: worm_give_weapon,
                bonuses_begin: worm_bonuses_begin,
                bonus_next: worm_bonus_next,
                bonus_free: worm_bonus_free,
                heal: worm_heal,
                create_sobject: worm_create_sobject,
                create_nobject2: worm_create_nobject2,
                play_worm_sound: worm_play_worm_sound,
                worm_sound_playing,
                stop_loop_sound: worm_stop_loop_sound,
                worm_hit: worm_worm_hit,
                steerables_begin: worm_steerables_begin,
                steerable_next: worm_steerable_next,
                enemy_pos: worm_enemy_pos,
                add_kill: worm_add_kill,
                after_death: worm_after_death,
                after_spawn: worm_after_spawn,
            };

            liero_worm_process(worm, self.g.worm(i).max_health, &k, &env, &hooks);
        }
    }

    /// `Game::createBonus`.
    fn create_bonus(&self) {
        let s = self.g.s;

        if unsafe { (*self.bonuses()).count } as i32 >= s.max_bonuses {
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

            // SAFETY: nothing else has the list during the call.
            let slot = unsafe { (*self.bonuses()).new_object() };
            let Some(slot) = slot else {
                return;
            };

            let [min_time, rand_time] = s.bonus_rand_timer[frame as usize];
            let mut b = LieroBonusState {
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

            // SAFETY: as above.
            let bonus = unsafe { &mut (*self.bonuses()).arr[slot] };
            bonus.x = b.x;
            bonus.y = b.y;
            bonus.vel_y = b.vel_y;
            bonus.frame = b.frame;
            bonus.timer = b.timer;
            // Health bonuses keep the weapon of the bonus that used the slot before.
            if b.frame == 0 {
                bonus.weapon = b.weapon;
            }

            self.create_sobject(7, ix, iy, 0, null_mut(), None);
            return;
        } // 234F
    }

    /// Screen flash and viewport shake at the start of the tick.
    fn begin_tick(&self) {
        unsafe {
            self.stats_tick(false);

            if (*self.data()).screen_flash > 0 {
                (*self.data()).screen_flash -= 1;
            }

            for (vs, n) in [
                (self.g.f.viewports, self.g.f.viewport_count),
                (self.g.f.spectator_viewports, self.g.f.spectator_viewport_count),
            ] {
                for i in 0..n {
                    let v = *vs.add(i as usize);
                    if (*v).shake > 0 {
                        (*v).shake -= 4000; // TODO in C++: Read 4000 from exe?
                    }
                }
            }
        }
    }

    /// Moves the viewports' death banners.
    fn update_banners(&self) {
        fn move_banner(banner_y: &mut i32, down: bool) {
            if down {
                if *banner_y < 2 {
                    *banner_y += 1;
                }
            } else if *banner_y > -8 {
                *banner_y -= 1;
            }
        }

        unsafe {
            let killed = |i: i32| (*self.worm(i)).killed_timer > 16;

            for i in 0..self.g.f.viewport_count {
                let v = *self.g.f.viewports.add(i as usize);
                move_banner(&mut (*v).banner_y, killed((*v).worm_idx));
            }

            for i in 0..self.g.f.spectator_viewport_count {
                let v = *self.g.f.spectator_viewports.add(i as usize);
                move_banner(&mut (*v).banner_y, killed(0) || killed(1));
            }
        }
    }

    /// `Game::processFrame`.
    pub(crate) fn process_frame(&self) {
        let s = self.g.s;

        unsafe {
            self.begin_tick();

            let mut br = Cursor::default();
            while let Some(i) = br.next(&*self.bonuses()) {
                self.process_bonus(i);
            }

            if self.cycles() & 1 == 0 {
                self.update_banners();
            }

            let mut sr = Cursor::default();
            while let Some(i) = sr.next(&*self.sobjects()) {
                self.process_sobject(i);
            }

            let mut wr = Cursor::default();
            while let Some(i) = wr.next(&*self.wobjects()) {
                self.process_wobject(i);
            }

            let mut nr = Cursor::default();
            while let Some(i) = nr.next(&*self.nobjects()) {
                self.process_nobject(i);
            }

            self.process_bobjects();

            // This was originally the beginning of the tick, but has been
            // rotated down to separate out the drawing.
            (*self.data()).cycles += 1;
        }

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
            unsafe {
                let w = self.worm(i);
                (*w).prev_control_states = (*w).control_states;
            }
        }

        if self.h.process_viewports {
            unsafe { crate::scene::process_viewports(self.g.f) };
        }
        self.stats_tick(true);
    }
}

/// Whether worm `w` holds control `c`.
unsafe fn pressed(w: *const LieroWormData, c: u32) -> bool {
    unsafe { ((*w).control_states >> c) & 1 != 0 }
}

// ---------------------------------------------------------------------------
// The hooks. Each `ctx` is the context its caller above made.

struct WormContext<'a> {
    w: World<'a>,
    /// The worm's position in the game's worms.
    i: i32,
    worm: *mut LieroWormData,
    bonuses: Cursor,
    wobjects: Cursor,
}

unsafe fn worm_ctx<'x>(ctx_: *mut c_void) -> &'x mut WormContext<'x> {
    unsafe { &mut *(ctx_ as *mut WormContext) }
}

impl WormContext<'_> {
    unsafe fn current(&self) -> *mut LieroWormWeapon {
        unsafe { addr_of_mut!((*self.worm).weapons[(*self.worm).current_weapon as usize]) }
    }

    unsafe fn index(&self) -> i32 {
        unsafe { (*self.worm).index }
    }
}

unsafe fn worm_launch_sound(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        let ww = c.current();
        let w = &*((*ww).kind as *const LieroTcWeapon);

        if w.launch_sound >= 0 {
            if w.loop_sound != 0 {
                if !c.w.playing(ww.cast()) {
                    c.w.play(w.launch_sound, ww.cast(), -1);
                }
            } else {
                c.w.play(w.launch_sound, null_mut(), 0);
            }
        }
    }
}

unsafe fn worm_fire_part(ctx_: *mut c_void, angle: i32, vel: LieroIVec2, speed: i32, pos: LieroIVec2) {
    unsafe {
        let c = worm_ctx(ctx_);
        let ww = c.current();
        c.w.fire_weapon(
            &*(*ww).kind.cast::<LieroTcWeapon>(),
            angle,
            vel,
            speed,
            pos,
            c.index(),
            ww,
        );
    }
}

unsafe fn worm_play_sound(ctx_: *mut c_void, sound: i32) {
    unsafe {
        worm_ctx(ctx_).w.play(sound, null_mut(), 0);
    }
}

unsafe fn worm_create_nobject1(ctx_: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.create_nobject1(kind, vel, pos, color, c.index(), null_mut());
    }
}

unsafe fn worm_weapon(_: *mut c_void, kind: *const c_void) -> LieroWeapon {
    unsafe { weapon_view(&*(kind as *const LieroTcWeapon)) }
}

unsafe fn worm_give_weapon(ctx_: *mut c_void, slot: i32, weapon: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        let ww = addr_of_mut!((*c.worm).weapons[slot as usize]);
        let kind = c.w.weapon(weapon);
        (*ww).kind = kind.cast();
        (*ww).ammo = (*kind).ammo;
    }
}

unsafe fn worm_bonuses_begin(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.bonuses = Cursor::default();
    }
}

unsafe fn worm_bonus_next(ctx_: *mut c_void) -> LieroBonusRef {
    unsafe {
        let c = worm_ctx(ctx_);
        let l = &*c.w.bonuses();
        match c.bonuses.next(l) {
            Some(i) => LieroBonusRef {
                found: true,
                x: l.arr[i].x,
                y: l.arr[i].y,
                frame: l.arr[i].frame,
                weapon: l.arr[i].weapon,
            },
            None => LieroBonusRef::default(),
        }
    }
}

unsafe fn worm_bonus_free(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        (*c.w.bonuses()).free(c.bonuses.last());
    }
}

unsafe fn worm_heal(ctx_: *mut c_void, amount: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.g.heal(c.i, amount);
    }
}

unsafe fn worm_create_sobject(ctx_: *mut c_void, kind: i32, x: i32, y: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.create_sobject(kind, x, y, c.index(), null_mut(), None);
    }
}

unsafe fn worm_create_nobject2(ctx_: *mut c_void, kind: i32, angle: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.create_nobject2(kind, angle, vel, pos, color, c.index(), null_mut());
    }
}

unsafe fn worm_play_worm_sound(ctx_: *mut c_void, sound: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.play(sound, c.worm.cast(), 0);
    }
}

unsafe fn worm_sound_playing(ctx_: *mut c_void) -> bool {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.playing(c.worm.cast())
    }
}

unsafe fn worm_stop_loop_sound(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.stop(c.current().cast());
    }
}

unsafe fn worm_worm_hit(ctx_: *mut c_void, x: i32, y: i32) -> bool {
    unsafe {
        let c = worm_ctx(ctx_);
        c.w.worm_hit(x, y, 0, c.i)
    }
}

unsafe fn worm_steerables_begin(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        c.wobjects = Cursor::default();
    }
}

unsafe fn worm_steerable_next(ctx_: *mut c_void) -> LieroSteerable {
    unsafe {
        let c = worm_ctx(ctx_);
        let kind = (*c.current()).kind;
        let index = c.index();
        while let Some(n) = c.wobjects.next(&*c.w.wobjects()) {
            let i = addr_of_mut!((*c.w.wobjects()).arr[n]);
            if (*i).kind == kind && (*i).owner_idx == index {
                return LieroSteerable {
                    cur_frame: addr_of_mut!((*i).cur_frame),
                    pos: (*i).pos,
                };
            }
        }
        LieroSteerable {
            cur_frame: null_mut(),
            pos: LieroIVec2::default(),
        }
    }
}

unsafe fn worm_enemy_pos(ctx_: *mut c_void, pos: *mut LieroIVec2) -> bool {
    unsafe {
        let c = worm_ctx(ctx_);
        if c.w.worm_count() != 2 {
            return false;
        }
        *pos = ftoi_vec((*c.w.worm(c.index() ^ 1)).pos);
        true
    }
}

unsafe fn worm_add_kill(ctx_: *mut c_void, idx: i32) {
    unsafe {
        let c = worm_ctx(ctx_);
        (*c.w.worm(idx)).kills += 1;
    }
}

unsafe fn worm_after_death(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        if let Some(s) = c.w.stats() {
            liero_stats_after_death(s, c.i);
        }
    }
}

unsafe fn worm_after_spawn(ctx_: *mut c_void) {
    unsafe {
        let c = worm_ctx(ctx_);
        if let Some(s) = c.w.stats() {
            liero_stats_after_spawn(s, c.i);
        }
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
