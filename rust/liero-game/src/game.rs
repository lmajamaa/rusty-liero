//! The game tick (`Game::processFrame` in `game.cpp`) and the game rules
//! around it: spawning bonuses, Game of Tag and Holdazone timers, damage
//! and healing (with Scales of Justice moving health between worms) and the
//! end of the game. Results have to stay bit-identical to the C++ versions,
//! which `tools/hashrun` checks.
//!
//! Rust owns the game's state (`LieroGameData`, with the object lists) and
//! each worm's (`LieroWormData`). The tick itself is in `world.rs`; it calls
//! C++ only for sounds, stats and the viewports' own processing.

use std::ffi::c_void;

use crate::level::LieroLevel;
use crate::objects::{zeroed_box, LieroBObjectList, LieroBonusList, LieroNObjectList, LieroSObjectList, LieroWObjectList};
use crate::math::ftoi;
use crate::rand::LieroRand;
use crate::tc::LieroTcData;
use crate::viewport::LieroViewport;
use crate::weapon::LieroWormWeapon;
use crate::world::World;
use crate::worm::{LieroWormData, LIERO_GAME_GAME_OF_TAG, LIERO_GAME_HOLDAZONE, LIERO_GAME_KILL_EM_ALL, LIERO_GAME_SCALES_OF_JUSTICE};

/// Rectangle from (x1, y1) to (x2, y2), x2 and y2 excluded, laid out like
/// `gvl::rect`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroRect {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

/// Holdazone's zone, laid out like C++'s `Holdazone`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroHoldazone {
    pub rect: LieroRect,
    /// The worm holding the zone, or -1.
    pub holder_idx: i32,
    /// The worm taking the zone, or -1.
    pub contender_idx: i32,
    pub contender_frames: i32,
    pub timeout_left: i32,
    pub zone_width: i32,
    pub zone_height: i32,
}

/// A new bonus's fields, as `set_bonus` copies them into C++'s `Bonus`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroBonusState {
    pub x: i32,
    pub y: i32,
    pub vel_y: i32,
    pub frame: i32,
    pub timer: i32,
    pub weapon: i32,
}

/// A worm, as the game rules see it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroGameWorm {
    pub data: *mut LieroWormData,
    /// The worm's full health (`settings->health`).
    pub max_health: i32,
}

impl LieroGameWorm {
    fn index(&self) -> i32 {
        // SAFETY: valid while the game is.
        unsafe { (*self.data).index }
    }
}

/// The game's state, besides its worms and level. Rust owns it, and the
/// fields of C++'s `Game` refer to it (`liero_game_data_new`).
///
/// It is 74 KB, more than the web build's stack, so it is only made and
/// copied on the heap: `new_boxed`, `clone_boxed` and `assign`.
#[repr(C)]
#[derive(Debug)]
pub struct LieroGameData {
    pub cycles: i32,
    /// The worm that died last, which is 'it' in Game of Tag, or -1.
    pub last_killed_idx: i32,
    /// Whether 'it' changed this tick.
    pub got_changed: bool,
    pub screen_flash: i32,
    pub paused: bool,
    /// Worms don't respawn in quick simulations (the AI's look-ahead).
    pub quick_sim: bool,
    pub holdazone: LieroHoldazone,
    pub rand: LieroRand,
    pub bonuses: LieroBonusList,
    pub wobjects: LieroWObjectList,
    pub sobjects: LieroSObjectList,
    pub nobjects: LieroNObjectList,
    pub bobjects: LieroBObjectList,
}

impl LieroGameData {
    /// What C++'s `Game` constructor set, before it seeds `rand`.
    pub fn new_boxed() -> Box<Self> {
        // SAFETY: all zeros is a valid value of every field but `bobjects`,
        // which is written before the box is used.
        let mut g: Box<Self> = unsafe {
            let mut g = zeroed_box::<Self>();
            std::ptr::addr_of_mut!((*g.as_mut_ptr()).bobjects).write(LieroBObjectList::new(1));
            g.assume_init()
        };
        g.last_killed_idx = -1;
        g.paused = true;
        g.holdazone = LieroHoldazone {
            rect: LieroRect::default(),
            holder_idx: -1,
            contender_idx: -1,
            contender_frames: 0,
            timeout_left: 0,
            zone_width: 50,
            zone_height: 34,
        };
        g.rand = LieroRand::default();
        g.bonuses.init();
        g.wobjects.clear();
        g.sobjects.clear();
        g.nobjects.clear();
        g
    }

    /// A copy, made on the heap.
    pub fn clone_boxed(&self) -> Box<Self> {
        // SAFETY: a bitwise copy of every field but `bobjects`, which gets
        // particles of its own without dropping the shared pointer.
        unsafe {
            let mut g = Box::<Self>::new_uninit();
            std::ptr::copy_nonoverlapping(self, g.as_mut_ptr(), 1);
            std::ptr::addr_of_mut!((*g.as_mut_ptr()).bobjects).write(self.bobjects.clone());
            g.assume_init()
        }
    }

    /// Makes this a copy of `other`, in place.
    pub fn assign(&mut self, other: &Self) {
        if std::ptr::eq(self, other) {
            return;
        }
        // SAFETY: as in `clone_boxed`; the old particles are freed once.
        unsafe {
            let old = std::ptr::read(&self.bobjects);
            std::ptr::copy_nonoverlapping(other, self, 1);
            std::ptr::addr_of_mut!(self.bobjects).write(other.bobjects.clone());
            drop(old);
        }
    }
}

#[no_mangle]
pub extern "C" fn liero_game_data_new() -> *mut LieroGameData {
    Box::into_raw(LieroGameData::new_boxed())
}

/// # Safety
/// `g` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_game_data_clone(g: &LieroGameData) -> *mut LieroGameData {
    Box::into_raw(g.clone_boxed())
}

/// `Game::operator=`.
///
/// # Safety
/// Both are valid, and nothing points into `to`'s blood particles after.
#[no_mangle]
pub unsafe extern "C" fn liero_game_data_assign(to: &mut LieroGameData, from: &LieroGameData) {
    to.assign(from);
}

/// # Safety
/// `g` comes from `liero_game_data_new` or `_clone`, and isn't used after.
#[no_mangle]
pub unsafe extern "C" fn liero_game_data_free(g: *mut LieroGameData) {
    if !g.is_null() {
        drop(Box::from_raw(g));
    }
}

/// The game state the rules read and change. `worms` has `worm_count`
/// entries in the order of C++'s `Game::worms`; worm numbers (killers,
/// holders, 'it') are positions in it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LieroGameFields {
    pub worms: *const LieroGameWorm,
    pub worm_count: i32,
    pub game: *mut LieroGameData,
    pub level: LieroLevel,
    /// The TC. Only the tick reads it, so the rules may leave it null.
    pub tc: *const LieroTcData,
    /// The viewports, whose shake and death banners the tick moves.
    pub viewports: *const *mut LieroViewport,
    pub viewport_count: i32,
    pub spectator_viewports: *const *mut LieroViewport,
    pub spectator_viewport_count: i32,
}

/// Game settings, and TC constants and hacks, for the rules.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroGameSettings {
    /// `LIERO_GAME_*`.
    pub game_mode: i32,
    pub max_bonuses: i32,
    pub time_to_lose: i32,
    /// Seconds.
    pub zone_timeout: i32,
    /// Ticks.
    pub zone_capture_time: i32,
    /// `Settings::weapTable`, one entry per weapon; 2 means the weapon is
    /// never in bonuses.
    pub weap_table: *const u32,
    pub weapon_count: i32,
    /// `Common::bonusRandTimer`: for each bonus frame, the shortest time
    /// and the random part.
    pub bonus_rand_timer: [[i32; 2]; 2],
    pub bonus_spawn_rect_x: i32,
    pub bonus_spawn_rect_y: i32,
    pub bonus_spawn_rect_w: i32,
    pub bonus_spawn_rect_h: i32,
    pub bonus_drop_chance: i32,
    /// The TC's hacks (`H[...]`).
    pub bonus_spawn_rect: bool,
    pub bonus_only_health: bool,
    pub bonus_only_weapon: bool,
    pub bonus_disable: bool,
    /// Blood amount, in percent.
    pub blood: i32,
    pub shadow: bool,
    /// Weapon loading time, in percent.
    pub loading_time: i32,
    /// Whether weapons can be changed while loading.
    pub load_change: bool,
}

/// What C++ does during a tick: sounds, stats and the viewports. `ctx` is
/// passed back to each function. Worms are positions in the game's worms;
/// a negative `by` is no worm.
#[repr(C)]
pub struct LieroGameHooks {
    pub ctx: *mut c_void,
    /// Stats at the start of the tick.
    pub pre_tick: unsafe extern "C" fn(ctx: *mut c_void),
    /// Viewports and stats at the end of the tick.
    pub end_tick: unsafe extern "C" fn(ctx: *mut c_void),
    /// Plays `sound` as `id`'s (null for none), looping when `loops` isn't 0.
    pub play_sound: unsafe extern "C" fn(ctx: *mut c_void, sound: i32, id: *mut c_void, loops: i32),
    pub sound_playing: unsafe extern "C" fn(ctx: *mut c_void, id: *mut c_void) -> bool,
    pub stop_sound: unsafe extern "C" fn(ctx: *mut c_void, id: *mut c_void),
    pub damage_potential: unsafe extern "C" fn(ctx: *mut c_void, by: i32, weapon: *mut LieroWormWeapon, hp: i32),
    pub damage_dealt:
        unsafe extern "C" fn(ctx: *mut c_void, by: i32, weapon: *mut LieroWormWeapon, to: i32, hp: i32, has_hit: bool),
    pub shot: unsafe extern "C" fn(ctx: *mut c_void, by: i32, weapon: *mut LieroWormWeapon),
    pub hit: unsafe extern "C" fn(ctx: *mut c_void, by: i32, weapon: *mut LieroWormWeapon, to: i32),
    pub after_death: unsafe extern "C" fn(ctx: *mut c_void, worm: i32),
    pub after_spawn: unsafe extern "C" fn(ctx: *mut c_void, worm: i32),
}

/// Getters and setters for the game's fields. SAFETY for all of them: the
/// pointers are valid for the call, and no reference outlives the statement.
#[derive(Clone, Copy)]
pub(crate) struct Game<'a> {
    pub(crate) f: &'a LieroGameFields,
    pub(crate) s: &'a LieroGameSettings,
}

impl Game<'_> {
    pub(crate) fn worm(&self, i: i32) -> LieroGameWorm {
        assert!(0 <= i && i < self.f.worm_count);
        unsafe { *self.f.worms.add(i as usize) }
    }

    pub(crate) fn worms(&self) -> impl Iterator<Item = (i32, LieroGameWorm)> + '_ {
        (0..self.f.worm_count).map(|i| (i, self.worm(i)))
    }

    fn cycles(&self) -> i32 {
        unsafe { (*self.f.game).cycles }
    }

    fn zone(&self) -> &mut LieroHoldazone {
        // SAFETY: valid for the call, and only used within a statement or
        // between hook calls.
        unsafe { &mut (*self.f.game).holdazone }
    }

    /// `Game::doDamageDirect`.
    fn damage_direct(&self, w: LieroGameWorm, amount: i32, by_idx: i32) {
        if amount > 0 {
            unsafe {
                (*w.data).health -= amount;
                if (*w.data).health <= 0 {
                    (*w.data).last_killed_by_idx = by_idx;
                }
            }
        }
    }

    /// `Game::doHealingDirect`.
    fn heal_direct(&self, w: LieroGameWorm, amount: i32) {
        unsafe {
            (*w.data).health += amount;
            if self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
                while (*w.data).health > w.max_health {
                    (*w.data).lives += 1;
                    (*w.data).health -= w.max_health;
                }
            } else if (*w.data).health > w.max_health {
                (*w.data).health = w.max_health;
            }
        }
    }

    /// Splits `amount` between the worms other than `target`, as evenly as
    /// whole numbers allow.
    fn share(&self, target: i32, amount: i32, mut f: impl FnMut(LieroGameWorm, i32)) {
        let mut parts = self.f.worm_count - 1;
        let mut left = amount;

        for (i, other) in self.worms() {
            if i != target {
                let k = left / parts;
                f(other, k);
                parts -= 1;
                left -= k;
            }
        }
    }

    /// `Game::doDamage`: in Scales of Justice, the killer (or, without one,
    /// everyone else) gets the health the worm loses.
    pub(crate) fn damage(&self, target: i32, amount: i32, by_idx: i32) {
        let w = self.worm(target);
        self.damage_direct(w, amount, by_idx);

        if amount > 0 && self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            if by_idx < 0 || by_idx == w.index() {
                self.share(target, amount, |other, k| self.heal_direct(other, k));
            } else {
                self.heal_direct(self.worm(by_idx), amount);
            }
        }
    }

    /// `Game::doHealing`: in Scales of Justice, the other worms lose the
    /// health the worm gains.
    pub(crate) fn heal(&self, target: i32, amount: i32) {
        let w = self.worm(target);
        self.heal_direct(w, amount);

        if self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            self.share(target, amount, |other, k| self.damage_direct(other, k, w.index()));
        } else {
            unsafe {
                if (*w.data).health > w.max_health {
                    (*w.data).health = w.max_health;
                }
            }
        }
    }

    /// `checkBonusSpawnPosition`: no dirt or rock within 2 pixels.
    pub(crate) fn bonus_spawn_position_free(&self, x: i32, y: i32) -> bool {
        // SAFETY: the view is valid for the call.
        let lv = unsafe { self.f.level.get() };
        let (x1, y1) = ((x - 2).max(0), (y - 2).max(0));
        let (x2, y2) = ((x + 3).min(lv.width), (y + 3).min(lv.height));

        for cx in x1..x2 {
            for cy in y1..y2 {
                if lv.mat(cx, cy).dirt_rock() {
                    return false;
                }
            }
        }
        true
    }

    /// `Game::spawnZone`: moves Holdazone's zone, halving it until it fits.
    pub(crate) fn spawn_zone(&self) {
        let z = self.zone();

        while z.zone_width >= 5 {
            // SAFETY: the view and rand are valid for the call.
            let pos = unsafe { self.f.level.get().select_spawn(&mut (*self.f.game).rand, z.zone_width, z.zone_height - 8) };
            if let Some(pos) = pos {
                z.rect = LieroRect { x1: pos.x, y1: pos.y, x2: pos.x + z.zone_width, y2: pos.y + z.zone_height };
                z.timeout_left = self.s.zone_timeout * 70;
                z.contender_idx = -1;
                z.contender_frames = 0;
                z.holder_idx = -1;
                break;
            }

            z.zone_width /= 2;
            z.zone_height /= 2;
        }
    }

    /// 'It' gains a second every 70 ticks while all worms are alive.
    fn game_of_tag(&self) {
        let some_invisible = self.worms().any(|(_, w)| !unsafe { (*w.data).visible });
        let it = unsafe { (*self.f.game).last_killed_idx };

        if !some_invisible && it >= 0 && self.cycles() % 70 == 0 {
            let w = self.worm(it);
            unsafe {
                if (*w.data).timer < self.s.time_to_lose {
                    (*w.data).timer += 1;
                }
            }
        }
    }

    fn holdazone(&self) {
        let s = self.s;
        let z = self.zone();

        let mut contender_idx = -1;
        let mut contenders = 0;

        for (_, w) in self.worms() {
            let pos = unsafe { (*w.data).pos };
            if unsafe { (*w.data).visible } && z.rect.contains(ftoi(pos.x), ftoi(pos.y)) {
                contender_idx = w.index();
                contenders += 1;
            }
        }

        if contenders == 0 {
            contender_idx = z.holder_idx;
        }

        if contenders <= 1 {
            if contender_idx < 0 || (z.contender_idx != contender_idx && z.contender_frames != 0) {
                if z.contender_frames == 0 || {
                    z.contender_frames -= 1;
                    z.contender_frames == 0
                } {
                    z.contender_idx = contender_idx;
                    z.holder_idx = -1;
                }
            } else {
                z.contender_idx = contender_idx;

                if z.contender_frames < s.zone_capture_time && {
                    z.contender_frames += 1;
                    z.contender_frames >= s.zone_capture_time
                } && z.holder_idx != z.contender_idx
                {
                    // New holder
                    let mut new_timeout = z.timeout_left;
                    if z.contender_idx >= 0 {
                        new_timeout += s.zone_timeout * 70 / 4;
                    } else {
                        new_timeout += s.zone_timeout * 70 / 8;
                    }

                    z.timeout_left = new_timeout.min(s.zone_timeout * 70);
                    z.holder_idx = z.contender_idx;
                }
            }
        }

        let dec = if z.holder_idx >= 0 {
            if self.cycles() % 70 == 0 {
                let holder = self.worm(z.holder_idx);
                unsafe { (*holder.data).timer += 1 };
            }
            true
        } else {
            self.cycles() % 4 == 0
        };

        if dec {
            z.timeout_left -= 1;
            if z.timeout_left <= 0 {
                self.spawn_zone();
            }
        }
    }

    /// The game mode's part of the tick.
    pub(crate) fn game_mode_tick(&self) {
        match self.s.game_mode {
            LIERO_GAME_GAME_OF_TAG => self.game_of_tag(),
            LIERO_GAME_HOLDAZONE => self.holdazone(),
            _ => {}
        }
    }

    fn is_over(&self) -> bool {
        match self.s.game_mode {
            LIERO_GAME_KILL_EM_ALL | LIERO_GAME_SCALES_OF_JUSTICE => self.worms().any(|(_, w)| unsafe { (*w.data).lives } <= 0),
            LIERO_GAME_GAME_OF_TAG | LIERO_GAME_HOLDAZONE => {
                self.worms().any(|(_, w)| unsafe { (*w.data).timer } >= self.s.time_to_lose)
            }
            _ => false,
        }
    }
}

// The functions C++ calls. In each, the pointers in `game` and `settings`
// must be valid for the call; hooks may change what they point to (but
// not free it).

/// Runs the game for one tick: C++'s `Game::processFrame`.
///
/// # Safety
/// See above.
#[no_mangle]
pub unsafe extern "C" fn liero_game_process_frame(
    game: &LieroGameFields,
    settings: &LieroGameSettings,
    hooks: &LieroGameHooks,
) {
    World { g: Game { f: game, s: settings }, h: hooks }.process_frame();
}

/// Hurts worm `target` (a position in `game.worms`) by `amount`, caused by
/// worm `by_idx` or -1: `Game::doDamage`.
///
/// # Safety
/// See above.
pub unsafe fn liero_game_damage(
    game: &LieroGameFields,
    settings: &LieroGameSettings,
    target: i32,
    amount: i32,
    by_idx: i32,
) {
    Game { f: game, s: settings }.damage(target, amount, by_idx);
}

/// Heals worm `target` by `amount`: `Game::doHealing`.
///
/// # Safety
/// See above.
pub unsafe fn liero_game_heal(game: &LieroGameFields, settings: &LieroGameSettings, target: i32, amount: i32) {
    Game { f: game, s: settings }.heal(target, amount);
}

/// Moves Holdazone's zone: `Game::spawnZone`.
///
/// # Safety
/// See above.
#[no_mangle]
pub unsafe extern "C" fn liero_game_spawn_zone(game: &LieroGameFields, settings: &LieroGameSettings) {
    Game { f: game, s: settings }.spawn_zone();
}

/// Whether the game has ended: `Game::isGameOver`.
///
/// # Safety
/// See above.
#[no_mangle]
pub unsafe extern "C" fn liero_game_is_over(game: &LieroGameFields, settings: &LieroGameSettings) -> bool {
    Game { f: game, s: settings }.is_over()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worm(health: i32) -> LieroWormData {
        LieroWormData { visible: true, health, lives: 5, ..LieroWormData::default() }
    }

    fn settings(game_mode: i32) -> LieroGameSettings {
        LieroGameSettings {
            game_mode,
            max_bonuses: 0,
            time_to_lose: 600,
            zone_timeout: 30,
            zone_capture_time: 70,
            weap_table: std::ptr::null(),
            weapon_count: 0,
            bonus_rand_timer: [[0; 2]; 2],
            bonus_spawn_rect_x: 0,
            bonus_spawn_rect_y: 0,
            bonus_spawn_rect_w: 0,
            bonus_spawn_rect_h: 0,
            bonus_drop_chance: 0,
            bonus_spawn_rect: false,
            bonus_only_health: false,
            bonus_only_weapon: false,
            bonus_disable: false,
            blood: 100,
            shadow: false,
            loading_time: 100,
            load_change: false,
        }
    }

    /// Runs `f` on a game of `worms`, all with 100 full health.
    fn with_game(worms: &mut [LieroWormData], f: impl FnOnce(&LieroGameFields)) {
        let views: Vec<LieroGameWorm> = worms
            .iter_mut()
            .enumerate()
            .map(|(i, w)| {
                w.index = i as i32;
                LieroGameWorm { data: w, max_health: 100 }
            })
            .collect();
        let mut game = LieroGameData::new_boxed();
        let fields = LieroGameFields {
            worms: views.as_ptr(),
            worm_count: views.len() as i32,
            game: &mut *game,
            level: LieroLevel { data: std::ptr::null_mut(), materials: std::ptr::null_mut(), width: 0, height: 0 },
            tc: std::ptr::null(),
            viewports: std::ptr::null(),
            viewport_count: 0,
            spectator_viewports: std::ptr::null(),
            spectator_viewport_count: 0,
        };
        f(&fields);
    }

    #[test]
    fn new_game_matches_cpp_and_copies_are_separate() {
        unsafe {
            let g = liero_game_data_new();
            // What C++'s Game() set.
            assert_eq!(((*g).cycles, (*g).last_killed_idx, (*g).screen_flash), (0, -1, 0));
            assert!((*g).paused && !(*g).got_changed && !(*g).quick_sim);
            let z = (*g).holdazone;
            assert_eq!((z.holder_idx, z.contender_idx, z.zone_width, z.zone_height), (-1, -1, 50, 34));
            assert_eq!(((*g).wobjects.count, (*g).bobjects.limit, (*g).bobjects.count), (0, 1, 0));
            assert!((*g).nobjects.sentinel.used && (*g).bonuses.arr[5].frame == -1);

            (*g).cycles = 7;
            (*g).quick_sim = true;
            (*g).holdazone.timeout_left = 3;
            (*g).rand.next_u32();
            (*g).sobjects.arr[3].used = true;
            (*g).wobjects.count = 2;
            crate::objects::liero_bobject_list_resize(&mut (*g).bobjects, 3);
            (*g).bobjects.count = 2;
            (*(*g).bobjects.arr.add(1)).color = 9;
            let c = liero_game_data_clone(&*g);
            // Every field is copied, and the blood is separate.
            assert_eq!(format!("{:?}", *c), format!("{:?}", *g));
            (*c).cycles = 1;
            (*(*c).bobjects.arr.add(1)).color = 1;
            assert_eq!(((*g).cycles, (*(*g).bobjects.arr.add(1)).color), (7, 9));

            // Assigning copies everything back.
            liero_game_data_assign(&mut *c, &*g);
            assert_eq!(format!("{:?}", *c), format!("{:?}", *g));
            liero_game_data_free(c);
            liero_game_data_free(g);
        }
    }

    #[test]
    fn damage_records_the_killer() {
        let mut worms = [worm(10), worm(100)];
        let s = settings(LIERO_GAME_KILL_EM_ALL);
        with_game(&mut worms, |g| unsafe {
            liero_game_damage(g, &s, 0, 5, 1);
            liero_game_damage(g, &s, 0, 0, 1);
            liero_game_damage(g, &s, 0, 7, 1);
        });
        assert_eq!((worms[0].health, worms[0].last_killed_by_idx), (-2, 1));
        assert_eq!(worms[1].health, 100);
    }

    #[test]
    fn scales_of_justice_moves_health() {
        let mut worms = [worm(50), worm(90), worm(90)];
        let s = settings(LIERO_GAME_SCALES_OF_JUSTICE);
        with_game(&mut worms, |g| unsafe {
            // Hurt by worm 1: it gets all of it, and an extra life from the
            // overflow.
            liero_game_damage(g, &s, 0, 20, 1);
        });
        assert_eq!((worms[0].health, worms[1].health, worms[1].lives), (30, 10, 6));

        with_game(&mut worms, |g| unsafe {
            // Self-inflicted: split between the others, 6 and then 7,
            // rounding down in turn.
            liero_game_damage(g, &s, 0, 13, 0);
            // Healing takes from the others in the same way.
            liero_game_heal(g, &s, 2, 9);
        });
        assert_eq!(worms[0].health, 30 - 13 - 4);
        assert_eq!(worms[1].health, 10 + 6 - 5);
        assert_eq!((worms[2].health, worms[2].lives), (90 + 7 + 9 - 100, 6));
        assert_eq!(worms[0].last_killed_by_idx, -1);
    }

    #[test]
    fn healing_is_capped_outside_scales_of_justice() {
        let mut worms = [worm(95), worm(100)];
        let s = settings(LIERO_GAME_KILL_EM_ALL);
        with_game(&mut worms, |g| unsafe { liero_game_heal(g, &s, 0, 20) });
        assert_eq!((worms[0].health, worms[0].lives), (100, 5));
    }

    #[test]
    fn game_over_by_mode() {
        let mut worms = [worm(100), worm(100)];
        worms[1].lives = 0;
        worms[0].timer = 600;
        for (mode, over) in [
            (LIERO_GAME_KILL_EM_ALL, true),
            (LIERO_GAME_SCALES_OF_JUSTICE, true),
            (LIERO_GAME_GAME_OF_TAG, true),
            (LIERO_GAME_HOLDAZONE, true),
        ] {
            let s = settings(mode);
            with_game(&mut worms, |g| assert_eq!(unsafe { liero_game_is_over(g, &s) }, over));
        }
        worms[1].lives = 1;
        worms[0].timer = 599;
        for mode in 0..4 {
            let s = settings(mode);
            with_game(&mut worms, |g| assert!(!unsafe { liero_game_is_over(g, &s) }));
        }
    }

    #[test]
    fn rect_contains_excludes_far_edges() {
        let r = LieroRect { x1: 10, y1: 20, x2: 15, y2: 22 };
        assert!(r.contains(10, 20) && r.contains(14, 21));
        assert!(!r.contains(15, 20) && !r.contains(10, 22) && !r.contains(9, 20));
    }
}
