//! The game tick (`Game::processFrame` in `game.cpp`) and the game rules
//! around it: spawning bonuses, Game of Tag and Holdazone timers, damage
//! and healing (with Scales of Justice moving health between worms) and the
//! end of the game. Results have to stay bit-identical to the C++ versions,
//! which `tools/hashrun` checks.
//!
//! C++ still owns the game, its worms and object lists. The tick runs in
//! Rust in the original order and calls C++ hooks for each object list,
//! each worm and the viewports and stats, which are not game logic.

use std::ffi::c_void;

use crate::level::LieroLevel;
use crate::math::{ftoi, itof, LieroIVec2};
use crate::rand::LieroRand;
use crate::worm::{LIERO_GAME_GAME_OF_TAG, LIERO_GAME_HOLDAZONE, LIERO_GAME_KILL_EM_ALL, LIERO_GAME_SCALES_OF_JUSTICE};

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

/// Pointers to the fields of a C++ `Worm` that the game rules use.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroGameWorm {
    pub pos: *mut LieroIVec2,
    pub visible: *mut bool,
    pub health: *mut i32,
    pub lives: *mut i32,
    /// Game of Tag and Holdazone time.
    pub timer: *mut i32,
    pub last_killed_by_idx: *mut i32,
    /// `Worm::index`.
    pub index: i32,
    /// The worm's full health (`settings->health`).
    pub max_health: i32,
}

/// The game state the rules read and change. `worms` has `worm_count`
/// entries in the order of C++'s `Game::worms`; worm numbers (killers,
/// holders, 'it') are positions in it.
#[repr(C)]
pub struct LieroGameFields {
    pub worms: *const LieroGameWorm,
    pub worm_count: i32,
    pub cycles: *mut i32,
    /// The worm that died last, which is 'it' in Game of Tag, or -1.
    pub last_killed_idx: *mut i32,
    pub holdazone: *mut LieroHoldazone,
    pub rand: *mut LieroRand,
    pub level: LieroLevel,
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
}

/// What C++ does during a tick. `ctx` is passed back to each function.
#[repr(C)]
pub struct LieroGameHooks {
    pub ctx: *mut c_void,
    /// Stats, screen flash and viewport shake at the start of the tick.
    pub begin_tick: unsafe extern "C" fn(ctx: *mut c_void),
    pub process_bonuses: unsafe extern "C" fn(ctx: *mut c_void),
    /// Moves the viewports' death banners, every other tick.
    pub update_banners: unsafe extern "C" fn(ctx: *mut c_void),
    pub process_sobjects: unsafe extern "C" fn(ctx: *mut c_void),
    pub process_wobjects: unsafe extern "C" fn(ctx: *mut c_void),
    pub process_nobjects: unsafe extern "C" fn(ctx: *mut c_void),
    pub process_bobjects: unsafe extern "C" fn(ctx: *mut c_void),
    /// How many bonuses there are.
    pub bonus_count: unsafe extern "C" fn(ctx: *mut c_void) -> i32,
    /// A new bonus, or null when the list is full.
    pub new_bonus: unsafe extern "C" fn(ctx: *mut c_void) -> *mut c_void,
    /// Sets the fields of a bonus from `new_bonus`.
    pub set_bonus: unsafe extern "C" fn(ctx: *mut c_void, bonus: *mut c_void, state: &LieroBonusState),
    /// `SObjectType::create` for sobject type `kind`, owned by nobody.
    pub create_sobject: unsafe extern "C" fn(ctx: *mut c_void, kind: i32, x: i32, y: i32),
    /// `Worm::process` and `Ninjarope::process` for worm `i`.
    pub process_worm: unsafe extern "C" fn(ctx: *mut c_void, i: i32),
    pub process_rope: unsafe extern "C" fn(ctx: *mut c_void, i: i32),
    /// Viewports, the control states replays compare against, and stats.
    pub end_tick: unsafe extern "C" fn(ctx: *mut c_void),
}

/// Getters and setters for the game's fields. SAFETY for all of them: the
/// pointers are valid for the call, and no reference outlives the statement.
struct Game<'a> {
    f: &'a LieroGameFields,
    s: &'a LieroGameSettings,
}

impl Game<'_> {
    fn worm(&self, i: i32) -> LieroGameWorm {
        assert!(0 <= i && i < self.f.worm_count);
        unsafe { *self.f.worms.add(i as usize) }
    }

    fn worms(&self) -> impl Iterator<Item = (i32, LieroGameWorm)> + '_ {
        (0..self.f.worm_count).map(|i| (i, self.worm(i)))
    }

    fn cycles(&self) -> i32 {
        unsafe { *self.f.cycles }
    }

    fn zone(&self) -> &mut LieroHoldazone {
        // SAFETY: valid for the call, and only used within a statement or
        // between hook calls.
        unsafe { &mut *self.f.holdazone }
    }

    fn rand(&self, max: u32) -> u32 {
        // SAFETY: valid for the call, and no hook runs during this.
        unsafe { (*self.f.rand).below(max) }
    }

    /// `Game::doDamageDirect`.
    fn damage_direct(&self, w: LieroGameWorm, amount: i32, by_idx: i32) {
        if amount > 0 {
            unsafe {
                *w.health -= amount;
                if *w.health <= 0 {
                    *w.last_killed_by_idx = by_idx;
                }
            }
        }
    }

    /// `Game::doHealingDirect`.
    fn heal_direct(&self, w: LieroGameWorm, amount: i32) {
        unsafe {
            *w.health += amount;
            if self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
                while *w.health > w.max_health {
                    *w.lives += 1;
                    *w.health -= w.max_health;
                }
            } else if *w.health > w.max_health {
                *w.health = w.max_health;
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
    fn damage(&self, target: i32, amount: i32, by_idx: i32) {
        let w = self.worm(target);
        self.damage_direct(w, amount, by_idx);

        if amount > 0 && self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            if by_idx < 0 || by_idx == w.index {
                self.share(target, amount, |other, k| self.heal_direct(other, k));
            } else {
                self.heal_direct(self.worm(by_idx), amount);
            }
        }
    }

    /// `Game::doHealing`: in Scales of Justice, the other worms lose the
    /// health the worm gains.
    fn heal(&self, target: i32, amount: i32) {
        let w = self.worm(target);
        self.heal_direct(w, amount);

        if self.s.game_mode == LIERO_GAME_SCALES_OF_JUSTICE {
            self.share(target, amount, |other, k| self.damage_direct(other, k, w.index));
        } else {
            unsafe {
                if *w.health > w.max_health {
                    *w.health = w.max_health;
                }
            }
        }
    }

    /// `checkBonusSpawnPosition`: no dirt or rock within 2 pixels.
    fn bonus_spawn_position_free(&self, x: i32, y: i32) -> bool {
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

    /// `Game::createBonus`.
    fn create_bonus(&self, hooks: &LieroGameHooks) {
        let s = self.s;
        let ctx = hooks.ctx;

        if unsafe { (hooks.bonus_count)(ctx) } >= s.max_bonuses {
            return;
        }

        for _ in 0..50000 {
            let mut ix = self.rand(s.bonus_spawn_rect_w as u32) as i32;
            let mut iy = self.rand(s.bonus_spawn_rect_h as u32) as i32;

            if s.bonus_spawn_rect {
                ix += s.bonus_spawn_rect_x;
                iy += s.bonus_spawn_rect_y;
            }

            if !self.bonus_spawn_position_free(ix, iy) {
                continue;
            }

            let frame = if s.bonus_only_health {
                1
            } else if s.bonus_only_weapon {
                0
            } else {
                self.rand(2) as i32
            };

            let bonus = unsafe { (hooks.new_bonus)(ctx) };
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
                (hooks.set_bonus)(ctx, bonus, &b);
                (hooks.create_sobject)(ctx, 7, ix, iy);
            }
            return;
        } // 234F
    }

    /// `Game::spawnZone`: moves Holdazone's zone, halving it until it fits.
    fn spawn_zone(&self) {
        let z = self.zone();

        while z.zone_width >= 5 {
            // SAFETY: the view and rand are valid for the call.
            let pos = unsafe { self.f.level.get().select_spawn(&mut *self.f.rand, z.zone_width, z.zone_height - 8) };
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
        let some_invisible = self.worms().any(|(_, w)| !unsafe { *w.visible });
        let it = unsafe { *self.f.last_killed_idx };

        if !some_invisible && it >= 0 && self.cycles() % 70 == 0 {
            let w = self.worm(it);
            unsafe {
                if *w.timer < self.s.time_to_lose {
                    *w.timer += 1;
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
            let pos = unsafe { *w.pos };
            if unsafe { *w.visible } && z.rect.contains(ftoi(pos.x), ftoi(pos.y)) {
                contender_idx = w.index;
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
                unsafe { *holder.timer += 1 };
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

    fn process_frame(&self, hooks: &LieroGameHooks) {
        let s = self.s;
        let ctx = hooks.ctx;

        unsafe {
            (hooks.begin_tick)(ctx);
            (hooks.process_bonuses)(ctx);

            if self.cycles() & 1 == 0 {
                (hooks.update_banners)(ctx);
            }

            (hooks.process_sobjects)(ctx);
            (hooks.process_wobjects)(ctx);
            (hooks.process_nobjects)(ctx);
            (hooks.process_bobjects)(ctx);

            // This was originally the beginning of the tick, but has been
            // rotated down to separate out the drawing.
            *self.f.cycles += 1;
        }

        if !s.bonus_disable && s.max_bonuses > 0 && self.rand(s.bonus_drop_chance as u32) == 0 {
            self.create_bonus(hooks);
        }

        for i in 0..self.f.worm_count {
            unsafe { (hooks.process_worm)(ctx, i) };
        }
        for i in 0..self.f.worm_count {
            unsafe { (hooks.process_rope)(ctx, i) };
        }

        match s.game_mode {
            LIERO_GAME_GAME_OF_TAG => self.game_of_tag(),
            LIERO_GAME_HOLDAZONE => self.holdazone(),
            _ => {}
        }

        unsafe { (hooks.end_tick)(ctx) };
    }

    fn is_over(&self) -> bool {
        match self.s.game_mode {
            LIERO_GAME_KILL_EM_ALL | LIERO_GAME_SCALES_OF_JUSTICE => self.worms().any(|(_, w)| unsafe { *w.lives } <= 0),
            LIERO_GAME_GAME_OF_TAG | LIERO_GAME_HOLDAZONE => {
                self.worms().any(|(_, w)| unsafe { *w.timer } >= self.s.time_to_lose)
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
    Game { f: game, s: settings }.process_frame(hooks);
}

/// Hurts worm `target` (a position in `game.worms`) by `amount`, caused by
/// worm `by_idx` or -1: `Game::doDamage`.
///
/// # Safety
/// See above.
#[no_mangle]
pub unsafe extern "C" fn liero_game_damage(
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
#[no_mangle]
pub unsafe extern "C" fn liero_game_heal(game: &LieroGameFields, settings: &LieroGameSettings, target: i32, amount: i32) {
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

    struct TestWorm {
        pos: LieroIVec2,
        visible: bool,
        health: i32,
        lives: i32,
        timer: i32,
        last_killed_by_idx: i32,
    }

    fn worm(health: i32) -> TestWorm {
        TestWorm { pos: LieroIVec2::default(), visible: true, health, lives: 5, timer: 0, last_killed_by_idx: -1 }
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
        }
    }

    /// Runs `f` on a game of `worms`, all with 100 full health.
    fn with_game(worms: &mut [TestWorm], f: impl FnOnce(&LieroGameFields)) {
        let views: Vec<LieroGameWorm> = worms
            .iter_mut()
            .enumerate()
            .map(|(i, w)| LieroGameWorm {
                pos: &mut w.pos,
                visible: &mut w.visible,
                health: &mut w.health,
                lives: &mut w.lives,
                timer: &mut w.timer,
                last_killed_by_idx: &mut w.last_killed_by_idx,
                index: i as i32,
                max_health: 100,
            })
            .collect();
        let (mut cycles, mut last_killed, mut zone, mut rand) =
            (0, -1, LieroHoldazone::default(), LieroRand::default());
        let fields = LieroGameFields {
            worms: views.as_ptr(),
            worm_count: views.len() as i32,
            cycles: &mut cycles,
            last_killed_idx: &mut last_killed,
            holdazone: &mut zone,
            rand: &mut rand,
            level: LieroLevel { data: std::ptr::null_mut(), materials: std::ptr::null_mut(), width: 0, height: 0 },
        };
        f(&fields);
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
