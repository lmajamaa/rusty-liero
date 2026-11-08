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
use crate::stats::{
    liero_stats_after_death, liero_stats_after_spawn, liero_stats_damage_dealt, liero_stats_damage_potential,
    liero_stats_hit, liero_stats_pre_tick, liero_stats_shot, liero_stats_tick, LieroStats,
};
use crate::level::{
    liero_level_blit_image, liero_level_correct_shadow, liero_level_draw_dirt_effect, LieroLevelCommon,
};
use crate::math::{cos_sin, ftoi, itof, LieroIVec2};
use crate::object::{
    add, div, ftoi_vec, liero_bobject_process, liero_bonus_process, liero_explosion_damage,
    liero_explosion_push_object, liero_explosion_push_worm, liero_ninjarope_process, liero_nobject_process,
    liero_sobject_animate, LieroBonusConstants, LieroNObjectEnv, LieroNObjectFields, LieroNObjectHooks,
    LieroNObjectType, LieroNinjaropeConstants, LieroNinjaropeHooks, LieroWormMotion,
};
use crate::objects::{
    LieroBObjectList, LieroBonus, LieroBonusList, LieroNObject, LieroNObjectList, LieroSObject, LieroSObjectList,
    LieroWObject, LieroWObjectList, LIERO_MAX_BONUSES, LIERO_MAX_NOBJECTS, LIERO_MAX_SOBJECTS, LIERO_MAX_WOBJECTS,
};
use crate::object::LieroBObject;
use crate::rand::LieroRand;
use crate::tc::{c, h, LieroTcData, LieroTcNObject, LieroTcSObject, LieroTcWeapon};
use crate::weapon::{
    liero_wobject_blow_up, liero_wobject_process, LieroBlowUpHooks, LieroWObjectEnv, LieroWObjectFields,
    LieroWeapon, LieroWormFiringHooks, LieroWormWeapon, LIERO_HIT_EXPLODE, LIERO_HIT_REMOVE,
};
use crate::worm::{
    liero_worm_process, LieroBonusRef, LieroSteerable, LieroWormConstants, LieroWormData, LieroWormEnv,
    LieroWormHooks, LIERO_CONTROL_CHANGE, LIERO_CONTROL_FIRE, LIERO_CONTROL_UP,
};

/// `Settings::wormAnimTab`.
const WORM_ANIM_TAB: [i32; 4] = [0, 7, 0, 14];

// ---------------------------------------------------------------------------
// The object lists, as C++'s ExactObjectList and BloodList work on them.

/// An object with a `used` flag.
pub(crate) trait Slot {
    /// # Safety
    /// `p` is valid.
    unsafe fn used(p: *const Self) -> bool;
}

macro_rules! slot {
    ($($t:ty),*) => {
        $(
            impl Slot for $t {
                unsafe fn used(p: *const Self) -> bool {
                    (*p).used
                }
            }
        )*
    };
}

slot!(LieroBonus, LieroWObject, LieroSObject, LieroNObject);

/// `ExactObjectList::range`: the objects in use, from first to last. It
/// stops at the sentinel slot, which is always used.
pub(crate) struct Range<T> {
    cur: *mut T,
    end: *mut T,
}

impl<T: Slot> Range<T> {
    /// # Safety
    /// The list is valid, and its sentinel is used.
    pub(crate) unsafe fn next(&mut self) -> Option<*mut T> {
        while !T::used(self.cur) {
            self.cur = self.cur.add(1);
        }
        let ret = self.cur;
        self.cur = self.cur.add(1);
        (ret != self.end).then_some(ret)
    }

    /// The object `next` returned last.
    fn last(&self) -> *mut T {
        self.cur.wrapping_sub(1)
    }
}

macro_rules! exact_list {
    ($list:ty, $t:ty, $limit:expr) => {
        // Not every list uses every function.
        #[allow(dead_code)]
        impl $list {
            /// The slots, then the sentinel, from the list's own pointer.
            fn slots(l: *mut Self) -> *mut $t {
                l.cast::<$t>()
            }

            unsafe fn get_free_object(l: *mut Self) -> *mut $t {
                assert!((*l).count < $limit);
                (*l).count += 1;

                let mut ptr: *mut $t = null_mut();
                for i in 0..(*l).free_list.len() {
                    let f = (*l).free_list[i];
                    if f != 0 {
                        let bit = f.trailing_zeros();
                        ptr = Self::slots(l).add((i << 5) + bit as usize);
                        (*l).free_list[i] &= !(1u32 << bit);
                        break;
                    }
                }

                assert!(!ptr.is_null() && !(*ptr).used);
                (*ptr).used = true;
                ptr
            }

            /// A free slot, or the last slot when there is none.
            pub(crate) unsafe fn new_object_reuse(l: *mut Self) -> *mut $t {
                if (*l).count >= $limit {
                    Self::slots(l).add($limit - 1)
                } else {
                    Self::get_free_object(l)
                }
            }

            /// A free slot, or null.
            pub(crate) unsafe fn new_object(l: *mut Self) -> *mut $t {
                if (*l).count >= $limit {
                    null_mut()
                } else {
                    Self::get_free_object(l)
                }
            }

            pub(crate) unsafe fn free(l: *mut Self, ptr: *mut $t) {
                assert!((*ptr).used);
                let index = ptr.offset_from(Self::slots(l)) as usize;
                (*l).free_list[index >> 5] |= 1u32 << (index & 31);
                (*ptr).used = false;
                assert!((*l).count > 0);
                (*l).count -= 1;
            }

            pub(crate) unsafe fn all(l: *mut Self) -> Range<$t> {
                Range { cur: Self::slots(l), end: Self::slots(l).add($limit) }
            }
        }
    };
}

exact_list!(LieroBonusList, LieroBonus, LIERO_MAX_BONUSES);
exact_list!(LieroWObjectList, LieroWObject, LIERO_MAX_WOBJECTS);
exact_list!(LieroSObjectList, LieroSObject, LIERO_MAX_SOBJECTS);
exact_list!(LieroNObjectList, LieroNObject, LIERO_MAX_NOBJECTS);

impl LieroBObjectList {
    /// `BloodList::newObjectReuse`.
    unsafe fn new_object_reuse(l: *mut Self) -> *mut LieroBObject {
        if (*l).count == (*l).limit {
            return (*l).arr.add((*l).limit - 1);
        }
        assert!((*l).count < (*l).limit);
        let p = (*l).arr.add((*l).count);
        (*l).count += 1;
        p
    }
}

// ---------------------------------------------------------------------------

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

fn nobject_view(t: &LieroTcNObject) -> LieroNObjectType {
    LieroNObjectType {
        gravity: t.gravity,
        bounce: t.bounce,
        hit_damage: t.hit_damage,
        expl_ground: t.expl_ground,
        blood_trail: t.blood_trail,
        blood_trail_delay: t.blood_trail_delay,
        start_frame: t.start_frame,
        num_frames: t.num_frames,
        draw_on_map: t.draw_on_map,
        leave_obj: t.leave_obj,
        leave_obj_delay: t.leave_obj_delay,
        time_to_explo: t.time_to_explo,
        create_on_exp: t.create_on_exp,
        dirt_effect: t.dirt_effect,
        splinter_amount: t.splinter_amount,
        splinter_colour: t.splinter_colour,
        splinter_type: t.splinter_type,
    }
}

/// `v * m / d` on both components, rounding towards zero like C++.
fn mul_div(v: LieroIVec2, m: i32, d: i32) -> LieroIVec2 {
    LieroIVec2 { x: v.x.wrapping_mul(m) / d, y: v.y.wrapping_mul(m) / d }
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
            materials: tc.materials.as_ptr().cast(),
            textures: tc.textures.as_ptr(),
            large_sprites: tc.large_sprites.data,
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

    // Sounds, in C++, and stats.

    fn play(&self, sound: i32, id: *mut c_void, loops: i32) {
        unsafe { (self.h.play_sound)(self.h.ctx, sound, id, loops) }
    }

    fn playing(&self, id: *mut c_void) -> bool {
        unsafe { (self.h.sound_playing)(self.h.ctx, id) }
    }

    fn stop(&self, id: *mut c_void) {
        unsafe { (self.h.stop_sound)(self.h.ctx, id) }
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
        unsafe {
            let obj = LieroBObjectList::new_object_reuse(self.bobjects());
            (*obj).color =
                self.rand(self.constant(c("NumBloodColours")) as u32) as i32 + self.constant(c("FirstBloodColour"));
            (*obj).pos = pos;
            (*obj).vel = vel;
        }
    }

    /// `createNObject`.
    fn create_nobject(
        &self,
        kind: i32,
        vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: *mut LieroWormWeapon,
    ) -> *mut LieroNObject {
        let t = self.nobject_type(kind);
        unsafe {
            let obj = LieroNObjectList::new_object_reuse(self.nobjects());

            (*obj).kind = (t as *const LieroTcNObject).cast();
            (*obj).owner_idx = owner_idx;
            (*obj).pos = pos;
            (*obj).vel = vel;

            // STATS
            (*obj).fired_by = fired_by;
            (*obj).has_hit = false;

            self.damage_potential(owner_idx, fired_by, t.hit_damage);

            (*obj).cur_frame = if t.start_frame > 0 {
                self.rand((t.num_frames + 1) as u32) as i32
            } else if color != 0 {
                color
            } else {
                t.color_bullets
            };

            (*obj).time_left = t.time_to_explo;
            if t.time_to_explo_v != 0 {
                (*obj).time_left -= self.rand(t.time_to_explo_v as u32) as i32;
            }

            obj
        }
    }

    /// `createNObject1`.
    fn create_nobject1(
        &self,
        kind: i32,
        mut vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: *mut LieroWormWeapon,
    ) {
        let t = self.nobject_type(kind);
        let d = t.distribution;
        if d != 0 {
            vel.x = vel.x.wrapping_add(d.wrapping_sub(self.rand(d.wrapping_mul(2) as u32) as i32));
            vel.y = vel.y.wrapping_add(d.wrapping_sub(self.rand(d.wrapping_mul(2) as u32) as i32));
        }
        self.create_nobject(kind, vel, pos, color, owner_idx, fired_by);
    }

    /// `createNObject2`.
    #[allow(clippy::too_many_arguments)]
    fn create_nobject2(
        &self,
        kind: i32,
        angle: i32,
        mut vel: LieroIVec2,
        pos: LieroIVec2,
        color: i32,
        owner_idx: i32,
        fired_by: *mut LieroWormWeapon,
    ) {
        let t = self.nobject_type(kind);
        let real_speed = t.speed.wrapping_sub(self.rand(t.speed_v as u32) as i32);

        vel = add(vel, mul_div(cos_sin(angle), real_speed, 100));

        // TODO in C++: !REPLAYS Make the distributions use the same code
        let d = t.distribution;
        if d != 0 {
            vel.x = vel.x.wrapping_add((self.rand(d.wrapping_mul(2) as u32) as i32).wrapping_sub(d));
            vel.y = vel.y.wrapping_add((self.rand(d.wrapping_mul(2) as u32) as i32).wrapping_sub(d));
        }

        let obj = self.create_nobject(kind, vel, pos, color, owner_idx, fired_by);
        unsafe { (*obj).pos = add((*obj).pos, (*obj).vel) };
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
        from: *mut LieroWObject,
    ) {
        let t = self.sobject_type(kind);
        unsafe {
            let obj = LieroSObjectList::new_object_reuse(self.sobjects());

            assert!(t.num_sounds < 10);

            if t.start_sound >= 0 {
                self.play(self.rand(t.num_sounds as u32) as i32 + t.start_sound, null_mut(), 0);
            }

            for i in 0..self.g.f.viewport_count {
                let v = *self.g.f.viewports.add(i as usize);
                let r = (*v).rect;
                if x > (*v).x && x < (*v).x + (r.x2 - r.x1) && y > (*v).y && y < (*v).y + (r.y2 - r.y1) {
                    if itof(t.shake) > (*v).shake {
                        (*v).shake = itof(t.shake);
                    }
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

                        if !from.is_null() && !(*from).has_hit {
                            self.hit(owner_idx, fired_by, i);
                            (*from).has_hit = true;
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

                let mut wr = LieroWObjectList::all(self.wobjects());
                while let Some(i) = wr.next() {
                    let weapon = &*((*i).kind as *const LieroTcWeapon);

                    if weapon.affect_by_explosions
                        && liero_explosion_push_object((*i).pos, &mut (*i).vel, x, y, t.detect_range, obj_blow_away)
                        && weapon.chain_explosion
                    {
                        self.blow_up_wobject(i, owner_idx);
                    }
                }

                let mut nr = LieroNObjectList::all(self.nobjects());
                while let Some(i) = nr.next() {
                    let nt = &*((*i).kind as *const LieroTcNObject);
                    if nt.affect_by_explosions {
                        liero_explosion_push_object((*i).pos, &mut (*i).vel, x, y, t.detect_range, obj_blow_away);
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
                                LieroIVec2 { x: itof(cx), y: itof(cy) },
                                pix as i32,
                                owner_idx,
                                fired_by,
                            );
                        }
                    }
                }
            }

            if t.dirt_effect >= 0 {
                let level = self.g.f.level;
                let common = self.common();
                liero_level_draw_dirt_effect(&level, &common, &mut *self.rand_ptr(), t.dirt_effect, x - 7, y - 7);

                if self.g.s.shadow {
                    liero_level_correct_shadow(&level, &common, x - 10, y - 10, x + 11, y + 11);
                }
            }

            let mut br = LieroBonusList::all(self.bonuses());
            while let Some(i) = br.next() {
                let (ix, iy) = (ftoi((*i).x), ftoi((*i).y));

                if ix > x - t.detect_range && ix < x + t.detect_range && iy > y - t.detect_range && iy < y + t.detect_range
                {
                    LieroBonusList::free(self.bonuses(), br.last());
                    self.create_sobject(0, ix, iy, owner_idx, fired_by, null_mut());
                }
            }
        }
    }

    /// `fireWeapon`: one part of a shot of `kind`.
    #[allow(clippy::too_many_arguments)]
    fn fire_weapon(
        &self,
        kind: *const LieroTcWeapon,
        angle: i32,
        vel: LieroIVec2,
        speed: i32,
        pos: LieroIVec2,
        owner_idx: i32,
        ww: *mut LieroWormWeapon,
    ) {
        unsafe {
            let obj = LieroWObjectList::new_object_reuse(self.wobjects());

            (*obj).kind = kind.cast();
            (*obj).pos = pos;
            (*obj).owner_idx = owner_idx;

            // STATS
            (*obj).fired_by = ww;
            (*obj).has_hit = false;

            self.damage_potential(owner_idx, ww, (*kind).hit_damage);
            self.shot(owner_idx, ww);

            let launch = weapon_view(&*kind).launch(&mut *self.rand_ptr(), angle, vel, speed);
            (*obj).vel = launch.vel;
            (*obj).cur_frame = launch.cur_frame;
            (*obj).time_left = launch.time_left;
        }
    }

    fn wobject_env(&self) -> LieroWObjectEnv {
        LieroWObjectEnv {
            level: self.g.f.level,
            common: self.common(),
            rand: self.rand_ptr(),
            cycles: self.cycles(),
            shadow: self.g.s.shadow,
            larpa_vel_div: self.constant(c("SplinterLarpaVelDiv")),
            crackler_vel_div: self.constant(c("SplinterCracklerVelDiv")),
        }
    }

    /// `WObject::blowUpObject`.
    fn blow_up_wobject(&self, obj: *mut LieroWObject, cause_idx: i32) {
        unsafe {
            let (p, v) = ((*obj).pos, (*obj).vel);

            LieroWObjectList::free(self.wobjects(), obj);

            let w = weapon_view(&*((*obj).kind as *const LieroTcWeapon));
            let env = self.wobject_env();
            let mut context = BlowUpContext { w: *self, obj, cause_idx };
            let hooks = LieroBlowUpHooks {
                ctx: (&mut context as *mut BlowUpContext).cast(),
                create_sobject: blow_up_create_sobject,
                play_sound: blow_up_play_sound,
                create_nobject1: blow_up_create_nobject1,
                create_nobject2: blow_up_create_nobject2,
            };

            liero_wobject_blow_up(&w, p, v, &env, &hooks);
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

            let sprites = &self.tc().worm_sprites;
            let frame = (*w).current_frame + (*w).direction * 7 * 3;
            assert!(frame >= 0 && frame < sprites.count);
            let size = (sprites.width * sprites.height) as usize;
            let sprite = sprites.data.add(frame as usize * size);

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
                    let pix = *sprite.add((cy * 16 + cx) as usize);
                    if materials[pix as usize] & crate::material::LIERO_MATERIAL_WORM != 0 {
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

    /// `WObject::process`.
    fn process_wobject(&self, obj: *mut LieroWObject) {
        unsafe {
            let owner_idx = (*obj).owner_idx;
            let owner = self.worm(owner_idx);
            let kind = (*obj).kind as *const LieroTcWeapon;

            // As liero would do this while rendering, we try to do it as early as possible
            if self.hack(h("RemExp"))
                && kind.offset_from(self.tc().weapons) as i32 == self.constant(c("RemExpObject")) - 1
                && pressed(owner, LIERO_CONTROL_CHANGE)
                && pressed(owner, LIERO_CONTROL_FIRE)
            {
                (*obj).time_left = 0;
            }

            let mut context = WObjectContext { w: *self, obj, owner: owner_idx };
            let fields = LieroWObjectFields {
                pos: addr_of_mut!((*obj).pos),
                vel: addr_of_mut!((*obj).vel),
                cur_frame: addr_of_mut!((*obj).cur_frame),
                time_left: addr_of_mut!((*obj).time_left),
            };
            let w = weapon_view(&*kind);
            let env = self.wobject_env();
            let hooks = LieroWObjectHooks {
                ctx: (&mut context as *mut WObjectContext).cast(),
                owner_steers: w_owner_steers,
                create_sobject: w_create_sobject,
                create_nobject1: w_create_nobject1,
                create_nobject2: w_create_nobject2,
                push_objects: w_push_objects,
                hit_worms: w_hit_worms,
                blow_up: w_blow_up,
                free: w_free,
                used: w_used,
            };

            liero_wobject_process(&fields, &w, &env, &hooks);
        }
    }

    /// `NObject::process`.
    fn process_nobject(&self, obj: *mut LieroNObject) {
        unsafe {
            let mut context = NObjectContext { w: *self, obj };
            let fields = LieroNObjectFields {
                pos: addr_of_mut!((*obj).pos),
                vel: addr_of_mut!((*obj).vel),
                time_left: addr_of_mut!((*obj).time_left),
                cur_frame: addr_of_mut!((*obj).cur_frame),
            };
            let t = nobject_view(&*((*obj).kind as *const LieroTcNObject));
            let env = LieroNObjectEnv {
                level: self.g.f.level,
                common: self.common(),
                rand: self.rand_ptr(),
                cycles: self.cycles(),
                shadow: self.g.s.shadow,
            };
            let hooks = LieroNObjectHooks {
                ctx: (&mut context as *mut NObjectContext).cast(),
                create_bobject: n_create_bobject,
                create_sobject: n_create_sobject,
                create_nobject2: n_create_nobject2,
                draw_on_map: n_draw_on_map,
                hit_worms: n_hit_worms,
                free: n_free,
            };

            liero_nobject_process(&fields, &t, &env, &hooks);
        }
    }

    /// `SObject::process`.
    fn process_sobject(&self, obj: *mut LieroSObject) {
        unsafe {
            let t = self.sobject_type((*obj).id);
            if liero_sobject_animate(&mut (*obj).cur_frame, &mut (*obj).anim_delay, t.anim_delay, t.num_frames) {
                LieroSObjectList::free(self.sobjects(), obj);
            }
        }
    }

    /// `Bonus::process`.
    fn process_bonus(&self, b: *mut LieroBonus) {
        unsafe {
            let level = self.g.f.level;
            let ix = ftoi((*b).x);
            assert!(ix >= 0 && ix < level.width);

            let k = LieroBonusConstants {
                gravity: self.constant(c("BonusGravity")),
                bounce_mul: self.constant(c("BonusBounceMul")),
                bounce_div: self.constant(c("BonusBounceDiv")),
            };

            if liero_bonus_process((*b).x, &mut (*b).y, &mut (*b).vel_y, &mut (*b).timer, &level, &k) {
                let iy = ftoi((*b).y);
                let kind = self.tc().bonus_sobjects[(*b).frame as usize];
                self.create_sobject(kind, ix, iy, 0, null_mut(), null_mut());
                if (*b).used {
                    LieroBonusList::free(self.bonuses(), b);
                }
            }
        }
    }

    /// The blood particles' loop in `Game::processFrame`.
    fn process_bobjects(&self) {
        unsafe {
            let l = self.bobjects();
            let level = self.g.f.level;
            let common = self.common();
            let gravity = self.constant(c("BObjGravity"));
            let mut i = 0;
            while i < (*l).count {
                let p = (*l).arr.add(i);
                if liero_bobject_process(&mut *p, &level, &common, &mut *self.rand_ptr(), gravity) {
                    i += 1;
                } else {
                    // BloodList::free: the last particle takes its place.
                    (*l).count -= 1;
                    *p = *(*l).arr.add((*l).count);
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
                bonuses: LieroBonusList::all(self.bonuses()),
                wobjects: LieroWObjectList::all(self.wobjects()),
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
                worm_sound_playing: worm_sound_playing,
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

    /// `Ninjarope::process` for worm `i`.
    fn process_rope(&self, i: i32) {
        unsafe {
            let owner = self.worm(i);
            let mut context = RopeContext { w: *self, owner: i };
            let k = LieroNinjaropeConstants {
                force_shl_x: self.constant(c("NRForceShlX")),
                force_div_x: self.constant(c("NRForceDivX")),
                force_shl_y: self.constant(c("NRForceShlY")),
                force_div_y: self.constant(c("NRForceDivY")),
                force_len_shl: self.constant(c("NRForceLenShl")),
                attach_length: self.constant(c("NRAttachLength")),
                gravity: self.constant(c("NinjaropeGravity")),
            };
            let hooks = LieroNinjaropeHooks {
                ctx: (&mut context as *mut RopeContext).cast(),
                find_anchor: rope_find_anchor,
                worm_motion: rope_worm_motion,
                create_nobject2: rope_create_nobject2,
            };
            let level = self.g.f.level;

            liero_ninjarope_process(
                addr_of_mut!((*owner).ninjarope),
                rope_worm_motion(owner.cast()),
                &level,
                self.rand_ptr(),
                &k,
                &hooks,
            );
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

            let bonus = unsafe { LieroBonusList::new_object(self.bonuses()) };
            if bonus.is_null() {
                return;
            }

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

            unsafe {
                (*bonus).x = b.x;
                (*bonus).y = b.y;
                (*bonus).vel_y = b.vel_y;
                (*bonus).frame = b.frame;
                (*bonus).timer = b.timer;
                // Health bonuses keep the weapon of the bonus that used the slot before.
                if b.frame == 0 {
                    (*bonus).weapon = b.weapon;
                }
            }

            self.create_sobject(7, ix, iy, 0, null_mut(), null_mut());
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

            let mut br = LieroBonusList::all(self.bonuses());
            while let Some(i) = br.next() {
                self.process_bonus(i);
            }

            if self.cycles() & 1 == 0 {
                self.update_banners();
            }

            let mut sr = LieroSObjectList::all(self.sobjects());
            while let Some(i) = sr.next() {
                self.process_sobject(i);
            }

            let mut wr = LieroWObjectList::all(self.wobjects());
            while let Some(i) = wr.next() {
                self.process_wobject(i);
            }

            let mut nr = LieroNObjectList::all(self.nobjects());
            while let Some(i) = nr.next() {
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
        // replays). Neither the viewports nor the stats in `end_tick` read
        // them.
        for i in 0..self.worm_count() {
            unsafe {
                let w = self.worm(i);
                (*w).prev_control_states = (*w).control_states;
            }
        }

        unsafe { (self.h.end_tick)(self.h.ctx) };
        self.stats_tick(true);
    }
}

/// Whether worm `w` holds control `c`.
unsafe fn pressed(w: *const LieroWormData, c: u32) -> bool {
    ((*w).control_states >> c) & 1 != 0
}

// ---------------------------------------------------------------------------
// The hooks. Each `ctx` is the context its caller above made.

struct BlowUpContext<'a> {
    w: World<'a>,
    obj: *mut LieroWObject,
    cause_idx: i32,
}

unsafe fn ctx<'x, T>(ctx: *mut c_void) -> &'x T {
    &*(ctx as *const T)
}

unsafe extern "C" fn blow_up_create_sobject(ctx_: *mut c_void, kind: i32, x: i32, y: i32) {
    let c: &BlowUpContext = ctx(ctx_);
    c.w.create_sobject(kind, x, y, c.cause_idx, (*c.obj).fired_by, c.obj);
}

unsafe extern "C" fn blow_up_play_sound(ctx_: *mut c_void, sound: i32) {
    let c: &BlowUpContext = ctx(ctx_);
    c.w.play(sound, null_mut(), 0);
}

unsafe extern "C" fn blow_up_create_nobject1(ctx_: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32) {
    let c: &BlowUpContext = ctx(ctx_);
    c.w.create_nobject1(kind, vel, pos, color, c.cause_idx, (*c.obj).fired_by);
}

unsafe extern "C" fn blow_up_create_nobject2(
    ctx_: *mut c_void,
    kind: i32,
    angle: i32,
    vel: LieroIVec2,
    pos: LieroIVec2,
    color: i32,
) {
    let c: &BlowUpContext = ctx(ctx_);
    c.w.create_nobject2(kind, angle, vel, pos, color, c.cause_idx, (*c.obj).fired_by);
}

struct WObjectContext<'a> {
    w: World<'a>,
    obj: *mut LieroWObject,
    owner: i32,
}

unsafe extern "C" fn w_owner_steers(ctx_: *mut c_void) -> bool {
    let c: &WObjectContext = ctx(ctx_);
    let owner = c.w.worm(c.owner);
    (*owner).visible && pressed(owner, LIERO_CONTROL_UP)
}

unsafe extern "C" fn w_create_sobject(ctx_: *mut c_void, kind: i32, x: i32, y: i32) {
    let c: &WObjectContext = ctx(ctx_);
    c.w.create_sobject(kind, x, y, (*c.obj).owner_idx, (*c.obj).fired_by, null_mut());
}

unsafe extern "C" fn w_create_nobject1(ctx_: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32) {
    let c: &WObjectContext = ctx(ctx_);
    c.w.create_nobject1(kind, vel, pos, color, (*c.obj).owner_idx, (*c.obj).fired_by);
}

unsafe extern "C" fn w_create_nobject2(
    ctx_: *mut c_void,
    kind: i32,
    angle: i32,
    vel: LieroIVec2,
    pos: LieroIVec2,
    color: i32,
) {
    let c: &WObjectContext = ctx(ctx_);
    c.w.create_nobject2(kind, angle, vel, pos, color, (*c.obj).owner_idx, (*c.obj).fired_by);
}

/// Whether integer `p` is within 2 pixels of fixed-point `q` on both axes,
/// as C++ compared them.
fn near(pos: LieroIVec2, q: LieroIVec2) -> bool {
    pos.x >= q.x - itof(2) && pos.x <= q.x + itof(2) && pos.y >= q.y - itof(2) && pos.y <= q.y + itof(2)
}

unsafe extern "C" fn w_push_objects(ctx_: *mut c_void, impulse: LieroIVec2) {
    let c: &WObjectContext = ctx(ctx_);
    let o = c.obj;
    let pos = (*o).pos;

    let mut wr = LieroWObjectList::all(c.w.wobjects());
    while let Some(i) = wr.next() {
        if ((*i).kind != (*o).kind || (*i).owner_idx != (*o).owner_idx) && near(pos, (*i).pos) {
            (*i).vel = add((*i).vel, impulse);
        }
    }

    let mut nr = LieroNObjectList::all(c.w.nobjects());
    while let Some(i) = nr.next() {
        if near(pos, (*i).pos) {
            (*i).vel = add((*i).vel, impulse);
        }
    }
}

unsafe extern "C" fn w_hit_worms(ctx_: *mut c_void) -> u32 {
    let c: &WObjectContext = ctx(ctx_);
    let world = &c.w;
    let o = c.obj;
    let w = &*((*o).kind as *const LieroTcWeapon);
    let mut result = 0;

    for i in 0..world.worm_count() {
        let worm = world.worm(i);

        if (w.hit_damage != 0 || w.blow_away != 0 || w.blood_on_hit != 0 || w.worm_collide)
            && world.spec_worm_hit(ftoi((*o).pos.x), ftoi((*o).pos.y), w.detect_distance, i)
        {
            (*worm).vel = add((*worm).vel, mul_div((*o).vel, w.blow_away, 100));

            world.g.damage(i, w.hit_damage, (*o).owner_idx);
            world.damage_dealt(c.owner, (*o).fired_by, i, w.hit_damage, (*o).has_hit);
            if !(*o).has_hit {
                world.hit(c.owner, (*o).fired_by, i);
            }
            (*o).has_hit = true;

            let blood_amount = w.blood_on_hit.wrapping_mul(world.g.s.blood) / 100;

            for _ in 0..blood_amount.max(0) {
                let angle = world.rand(128) as i32;
                world.create_nobject2(6, angle, div((*o).vel, 3), (*o).pos, 0, (*worm).index, (*o).fired_by);
            }

            if w.hit_damage > 0 && (*worm).health > 0 && world.rand(3) == 0 {
                world.hurt_sound(i);
            }

            if w.worm_collide && world.rand(w.worm_collide as u32) == 0 {
                if w.worm_explode {
                    result |= LIERO_HIT_EXPLODE;
                }
                result |= LIERO_HIT_REMOVE;
            }
        }
    }

    result
}

unsafe extern "C" fn w_blow_up(ctx_: *mut c_void) {
    let c: &WObjectContext = ctx(ctx_);
    c.w.blow_up_wobject(c.obj, (*c.obj).owner_idx);
}

unsafe extern "C" fn w_free(ctx_: *mut c_void) {
    let c: &WObjectContext = ctx(ctx_);
    LieroWObjectList::free(c.w.wobjects(), c.obj);
}

unsafe extern "C" fn w_used(ctx_: *mut c_void) -> bool {
    let c: &WObjectContext = ctx(ctx_);
    (*c.obj).used
}

struct NObjectContext<'a> {
    w: World<'a>,
    obj: *mut LieroNObject,
}

unsafe extern "C" fn n_create_bobject(ctx_: *mut c_void, pos: LieroIVec2, vel: LieroIVec2) {
    let c: &NObjectContext = ctx(ctx_);
    c.w.create_bobject(pos, vel);
}

unsafe extern "C" fn n_create_sobject(ctx_: *mut c_void, kind: i32, x: i32, y: i32) {
    let c: &NObjectContext = ctx(ctx_);
    c.w.create_sobject(kind, x, y, (*c.obj).owner_idx, (*c.obj).fired_by, null_mut());
}

unsafe extern "C" fn n_create_nobject2(
    ctx_: *mut c_void,
    kind: i32,
    angle: i32,
    vel: LieroIVec2,
    pos: LieroIVec2,
    color: i32,
) {
    let c: &NObjectContext = ctx(ctx_);
    c.w.create_nobject2(kind, angle, vel, pos, color, (*c.obj).owner_idx, null_mut());
}

unsafe extern "C" fn n_draw_on_map(ctx_: *mut c_void, x: i32, y: i32) {
    let c: &NObjectContext = ctx(ctx_);
    let t = &*((*c.obj).kind as *const LieroTcNObject);
    let sprites = &c.w.tc().small_sprites;
    let frame = t.start_frame + (*c.obj).cur_frame;
    assert!(frame >= 0 && frame < sprites.count);
    let sprite = sprites.data.add((frame * sprites.width * sprites.height) as usize);

    let level = c.w.g.f.level;
    let common = c.w.common();
    liero_level_blit_image(&level, &common, sprite, x, y, 7, 7);
    // x and y are 3 left of and above the object. This seems like an overly
    // large rectangle.
    if c.w.g.s.shadow {
        liero_level_correct_shadow(&level, &common, x - 5, y - 5, x + 12, y + 12);
    }
}

unsafe extern "C" fn n_hit_worms(ctx_: *mut c_void) -> bool {
    let c: &NObjectContext = ctx(ctx_);
    let world = &c.w;
    let o = c.obj;
    let t = &*((*o).kind as *const LieroTcNObject);
    let mut do_explode = false;

    for i in 0..world.worm_count() {
        let w = world.worm(i);

        if world.spec_worm_hit(ftoi((*o).pos.x), ftoi((*o).pos.y), t.detect_distance, i) {
            (*w).vel = add((*w).vel, mul_div((*o).vel, t.blow_away, 100));

            world.g.damage(i, t.hit_damage, (*o).owner_idx);

            world.damage_dealt((*o).owner_idx, (*o).fired_by, i, t.hit_damage, (*o).has_hit);
            (*o).has_hit = true;

            if t.hit_damage > 0 && (*w).health > 0 && world.rand(3) == 0 {
                world.hurt_sound(i);
            }

            let blood = t.blood_on_hit.wrapping_mul(world.g.s.blood) / 100;

            for _ in 0..blood.max(0) {
                let angle = world.rand(128) as i32;
                world.create_nobject2(6, angle, div((*o).vel, 3), (*o).pos, 0, (*o).owner_idx, null_mut());
            }

            if t.worm_explode {
                do_explode = true;
            } else if t.worm_destroy && (*o).used {
                LieroNObjectList::free(world.nobjects(), o);
            }
        }
    }

    do_explode
}

unsafe extern "C" fn n_free(ctx_: *mut c_void) {
    let c: &NObjectContext = ctx(ctx_);
    if (*c.obj).used {
        LieroNObjectList::free(c.w.nobjects(), c.obj);
    }
}

struct WormContext<'a> {
    w: World<'a>,
    /// The worm's position in the game's worms.
    i: i32,
    worm: *mut LieroWormData,
    bonuses: Range<LieroBonus>,
    wobjects: Range<LieroWObject>,
}

unsafe fn worm_ctx<'x>(ctx_: *mut c_void) -> &'x mut WormContext<'x> {
    &mut *(ctx_ as *mut WormContext)
}

impl WormContext<'_> {
    unsafe fn current(&self) -> *mut LieroWormWeapon {
        addr_of_mut!((*self.worm).weapons[(*self.worm).current_weapon as usize])
    }

    unsafe fn index(&self) -> i32 {
        (*self.worm).index
    }
}

unsafe extern "C" fn worm_launch_sound(ctx_: *mut c_void) {
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

unsafe extern "C" fn worm_fire_part(ctx_: *mut c_void, angle: i32, vel: LieroIVec2, speed: i32, pos: LieroIVec2) {
    let c = worm_ctx(ctx_);
    let ww = c.current();
    c.w.fire_weapon((*ww).kind.cast(), angle, vel, speed, pos, c.index(), ww);
}

unsafe extern "C" fn worm_play_sound(ctx_: *mut c_void, sound: i32) {
    worm_ctx(ctx_).w.play(sound, null_mut(), 0);
}

unsafe extern "C" fn worm_create_nobject1(ctx_: *mut c_void, kind: i32, vel: LieroIVec2, pos: LieroIVec2, color: i32) {
    let c = worm_ctx(ctx_);
    c.w.create_nobject1(kind, vel, pos, color, c.index(), null_mut());
}

unsafe extern "C" fn worm_weapon(_: *mut c_void, kind: *const c_void) -> LieroWeapon {
    weapon_view(&*(kind as *const LieroTcWeapon))
}

unsafe extern "C" fn worm_give_weapon(ctx_: *mut c_void, slot: i32, weapon: i32) {
    let c = worm_ctx(ctx_);
    let ww = addr_of_mut!((*c.worm).weapons[slot as usize]);
    let kind = c.w.weapon(weapon);
    (*ww).kind = kind.cast();
    (*ww).ammo = (*kind).ammo;
}

unsafe extern "C" fn worm_bonuses_begin(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    c.bonuses = LieroBonusList::all(c.w.bonuses());
}

unsafe extern "C" fn worm_bonus_next(ctx_: *mut c_void) -> LieroBonusRef {
    let c = worm_ctx(ctx_);
    match c.bonuses.next() {
        Some(b) => LieroBonusRef { found: true, x: (*b).x, y: (*b).y, frame: (*b).frame, weapon: (*b).weapon },
        None => LieroBonusRef::default(),
    }
}

unsafe extern "C" fn worm_bonus_free(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    LieroBonusList::free(c.w.bonuses(), c.bonuses.last());
}

unsafe extern "C" fn worm_heal(ctx_: *mut c_void, amount: i32) {
    let c = worm_ctx(ctx_);
    c.w.g.heal(c.i, amount);
}

unsafe extern "C" fn worm_create_sobject(ctx_: *mut c_void, kind: i32, x: i32, y: i32) {
    let c = worm_ctx(ctx_);
    c.w.create_sobject(kind, x, y, c.index(), null_mut(), null_mut());
}

unsafe extern "C" fn worm_create_nobject2(
    ctx_: *mut c_void,
    kind: i32,
    angle: i32,
    vel: LieroIVec2,
    pos: LieroIVec2,
    color: i32,
) {
    let c = worm_ctx(ctx_);
    c.w.create_nobject2(kind, angle, vel, pos, color, c.index(), null_mut());
}

unsafe extern "C" fn worm_play_worm_sound(ctx_: *mut c_void, sound: i32) {
    let c = worm_ctx(ctx_);
    c.w.play(sound, c.worm.cast(), 0);
}

unsafe extern "C" fn worm_sound_playing(ctx_: *mut c_void) -> bool {
    let c = worm_ctx(ctx_);
    c.w.playing(c.worm.cast())
}

unsafe extern "C" fn worm_stop_loop_sound(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    c.w.stop(c.current().cast());
}

unsafe extern "C" fn worm_worm_hit(ctx_: *mut c_void, x: i32, y: i32) -> bool {
    let c = worm_ctx(ctx_);
    c.w.worm_hit(x, y, 0, c.i)
}

unsafe extern "C" fn worm_steerables_begin(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    c.wobjects = LieroWObjectList::all(c.w.wobjects());
}

unsafe extern "C" fn worm_steerable_next(ctx_: *mut c_void) -> LieroSteerable {
    let c = worm_ctx(ctx_);
    let kind = (*c.current()).kind;
    let index = c.index();
    while let Some(i) = c.wobjects.next() {
        if (*i).kind == kind && (*i).owner_idx == index {
            return LieroSteerable { cur_frame: addr_of_mut!((*i).cur_frame), pos: (*i).pos };
        }
    }
    LieroSteerable { cur_frame: null_mut(), pos: LieroIVec2::default() }
}

unsafe extern "C" fn worm_enemy_pos(ctx_: *mut c_void, pos: *mut LieroIVec2) -> bool {
    let c = worm_ctx(ctx_);
    if c.w.worm_count() != 2 {
        return false;
    }
    *pos = ftoi_vec((*c.w.worm(c.index() ^ 1)).pos);
    true
}

unsafe extern "C" fn worm_add_kill(ctx_: *mut c_void, idx: i32) {
    let c = worm_ctx(ctx_);
    (*c.w.worm(idx)).kills += 1;
}

unsafe extern "C" fn worm_after_death(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    if let Some(s) = c.w.stats() {
        liero_stats_after_death(s, c.i);
    }
}

unsafe extern "C" fn worm_after_spawn(ctx_: *mut c_void) {
    let c = worm_ctx(ctx_);
    if let Some(s) = c.w.stats() {
        liero_stats_after_spawn(s, c.i);
    }
}

struct RopeContext<'a> {
    w: World<'a>,
    /// The owner's position in the game's worms.
    owner: i32,
}

unsafe extern "C" fn rope_find_anchor(ctx_: *mut c_void, x: i32, y: i32) -> *mut c_void {
    let c: &RopeContext = ctx(ctx_);
    for i in 0..c.w.worm_count() {
        if i != c.owner && c.w.spec_worm_hit(x, y, 1, i) {
            return c.w.worm(i).cast();
        }
    }
    null_mut()
}

unsafe extern "C" fn rope_worm_motion(worm: *mut c_void) -> LieroWormMotion {
    let w = worm as *mut LieroWormData;
    LieroWormMotion { pos: addr_of_mut!((*w).pos), vel: addr_of_mut!((*w).vel) }
}

unsafe extern "C" fn rope_create_nobject2(
    ctx_: *mut c_void,
    kind: i32,
    angle: i32,
    vel: LieroIVec2,
    pos: LieroIVec2,
    color: i32,
) {
    let c: &RopeContext = ctx(ctx_);
    let index = (*c.w.worm(c.owner)).index;
    c.w.create_nobject2(kind, angle, vel, pos, color, index, null_mut());
}

use crate::weapon::LieroWObjectHooks;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::LieroGameData;

    #[test]
    fn constant_and_hack_indexes_follow_the_cpp_enums() {
        assert_eq!(c("NRInitialLength"), 0);
        assert_eq!(c("RemExpObject"), liero_data::exe::CONSTANT_ORDER.len() - 1);
        assert_eq!(h("FallDamage"), 0);
        assert_eq!(h("MultiJump"), 10);
    }

    #[test]
    fn lists_hand_out_the_lowest_free_slot_and_reuse_the_last_when_full() {
        let mut g = LieroGameData::new_boxed();
        let l: *mut LieroSObjectList = &mut g.sobjects;
        unsafe {
            let base = LieroSObjectList::slots(l);
            let a = LieroSObjectList::new_object(l);
            let b = LieroSObjectList::new_object(l);
            let c = LieroSObjectList::new_object(l);
            assert_eq!((a, b, c), (base, base.add(1), base.add(2)));

            LieroSObjectList::free(l, b);
            assert_eq!((*l).count, 2);
            assert_eq!(LieroSObjectList::new_object(l), b);

            for _ in 3..LIERO_MAX_SOBJECTS {
                assert!(!LieroSObjectList::new_object(l).is_null());
            }
            assert!(LieroSObjectList::new_object(l).is_null());
            assert_eq!(LieroSObjectList::new_object_reuse(l), base.add(LIERO_MAX_SOBJECTS - 1));
            assert_eq!((*l).count, LIERO_MAX_SOBJECTS);
        }
    }

    #[test]
    fn ranges_skip_free_slots_and_survive_freeing_the_current_one() {
        let mut g = LieroGameData::new_boxed();
        let l: *mut LieroBonusList = &mut g.bonuses;
        unsafe {
            let objs: Vec<_> = (0..5).map(|_| LieroBonusList::new_object(l)).collect();
            LieroBonusList::free(l, objs[1]);

            let mut seen = Vec::new();
            let mut r = LieroBonusList::all(l);
            while let Some(b) = r.next() {
                seen.push(b);
                if b == objs[2] {
                    LieroBonusList::free(l, r.last());
                }
            }
            assert_eq!(seen, vec![objs[0], objs[2], objs[3], objs[4]]);
            assert_eq!((*l).count, 3);
        }
    }

    #[test]
    fn blood_reuses_the_last_particle_when_full() {
        let mut g = LieroGameData::new_boxed();
        g.bobjects.resize(2);
        let l: *mut LieroBObjectList = &mut g.bobjects;
        unsafe {
            let a = LieroBObjectList::new_object_reuse(l);
            let b = LieroBObjectList::new_object_reuse(l);
            assert_eq!(b, a.add(1));
            assert_eq!(LieroBObjectList::new_object_reuse(l), b);
            assert_eq!((*l).count, 2);
        }
    }
}
