//! The game tick (`Game::processFrame` in `game.cpp`) and the game rules
//! around it: spawning bonuses, Game of Tag and Holdazone timers, damage
//! and healing (with Scales of Justice moving health between worms) and the
//! end of the game. Results have to stay bit-identical to the C++ versions,
//! which `tools/hashrun` checks.
//!
//! Rust owns the game's state (`GameData`, with the object lists) and
//! each worm's (`WormData`). The rules and the tick (in `world.rs`)
//! borrow them, through `Game`, for the call.

use crate::level::{slice, Level, RawLevel};
use crate::math::ftoi;
use crate::objects::{zeroed_box, BObjectList, BonusList, NObjectList, SObjectList, WObjectList};
use crate::rand::Rand;
use crate::sfx::SoundOut;
use crate::stats::{Stats, StatsWorm};
use crate::tc::TcData;
use crate::text::Str;
use crate::viewport::Viewport;
use crate::world::World;
use crate::worm::{GameMode, WormData, WormId};

/// Rectangle from (x1, y1) to (x2, y2), x2 and y2 excluded, laid out like
/// `gvl::rect`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

/// Holdazone's zone, laid out like C++'s `Holdazone`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holdazone {
    pub rect: Rect,
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
#[derive(Clone, Copy, Debug, Default)]
pub struct BonusState {
    pub x: i32,
    pub y: i32,
    pub vel_y: i32,
    pub frame: i32,
    pub timer: i32,
    pub weapon: i32,
}

/// A worm, as the game rules see it.
#[derive(Clone, Copy, Debug)]
pub struct GameWorm {
    pub data: *mut WormData,
    /// Its place in the game's pool, which ropes are anchored by.
    pub id: crate::worm::WormId,
    /// The worm's full health (`settings->health`).
    pub max_health: i32,
    /// The worm's bot, if any (null for a human).
    pub ai: *mut crate::ai::WormAi,
    /// The worm's name and colour, from its settings.
    pub name: Str,
    pub color: i32,
}

/// The game's state, besides its worms and level. Rust owns it, and the
/// fields of C++'s `Game` refer to it (`GameData::new_boxed`).
///
/// It is 74 KB, more than the web build's stack, so it is only made and
/// copied on the heap: `new_boxed`, `clone_boxed` and `assign`.
#[derive(Debug)]
pub struct GameData {
    pub cycles: i32,
    /// The worm that died last, which is 'it' in Game of Tag, or -1.
    pub last_killed_idx: i32,
    /// Whether 'it' changed this tick.
    pub got_changed: bool,
    pub screen_flash: i32,
    pub paused: bool,
    /// Worms don't respawn in quick simulations (the AI's look-ahead).
    pub quick_sim: bool,
    pub holdazone: Holdazone,
    pub rand: Rand,
    pub bonuses: BonusList,
    pub wobjects: WObjectList,
    pub sobjects: SObjectList,
    pub nobjects: NObjectList,
    pub bobjects: BObjectList,
}

impl GameData {
    /// What C++'s `Game` constructor set, before it seeds `rand`.
    pub fn new_boxed() -> Box<Self> {
        // SAFETY: all zeros is a valid value of every field but `bobjects`,
        // which is written before the box is used.
        let mut g: Box<Self> = unsafe {
            let mut g = zeroed_box::<Self>();
            std::ptr::addr_of_mut!((*g.as_mut_ptr()).bobjects).write(BObjectList::new(1));
            g.assume_init()
        };
        g.last_killed_idx = -1;
        g.paused = true;
        g.holdazone = Holdazone {
            rect: Rect::default(),
            holder_idx: -1,
            contender_idx: -1,
            contender_frames: 0,
            timeout_left: 0,
            zone_width: 50,
            zone_height: 34,
        };
        g.rand = Rand::default();
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

/// The game state the rules read and change. `worms` has `worm_count`
/// entries in the order of C++'s `Game::worms`; worm numbers (killers,
/// holders, 'it') are positions in it.
#[derive(Clone, Copy)]
pub struct GameFields {
    pub worms: *const GameWorm,
    pub worm_count: i32,
    /// Every worm the game has, by `WormId`, listed or not.
    pub pool: *const *mut WormData,
    pub pool_count: usize,
    pub game: *mut GameData,
    pub level: RawLevel,
    /// The TC. Only the tick reads it, so the rules may leave it null.
    pub tc: *const TcData,
    /// The viewports, whose shake and death banners the tick moves.
    pub viewports: *const *mut Viewport,
    pub viewport_count: i32,
    pub spectator_viewports: *const *mut Viewport,
    pub spectator_viewport_count: i32,
    /// The stats the tick records, or null for none.
    pub stats: *mut Stats,
    /// Where its sounds go.
    pub sound: SoundOut,
}

/// Game settings, and TC constants and hacks, for the rules.
#[derive(Clone, Copy, Debug)]
pub struct RuleSettings {
    pub game_mode: Option<GameMode>,
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

/// What the tick takes from the host: the clock.
pub struct GameHooks<'a> {
    /// The clock the stats time ticks with (`appHiresTicks`).
    pub hires_ticks: &'a dyn Fn() -> u64,
}

/// A worm as the stats read it (`statsWorm` in C++).
pub(crate) fn stats_worm(w: &WormData, max_health: i32) -> StatsWorm {
    use crate::worm::Control;
    StatsWorm {
        index: w.index,
        x: ftoi(w.pos.x),
        y: ftoi(w.pos.y),
        visible: w.visible,
        health: w.health,
        lives: w.lives,
        timer: w.timer,
        kills: w.kills,
        max_health,
        fire: w.pressed(Control::Fire),
        change: w.pressed(Control::Change),
        left: w.pressed(Control::Left),
        right: w.pressed(Control::Right),
        current_weapon: w.current_weapon,
        loading_left: w.weapons.map(|ww| ww.loading_left),
    }
}

/// The game as the rules change it: its state, every worm it has (listed
/// or not) by `WormId`, and its level, borrowed for the call.
pub(crate) struct Rules<'a> {
    pub(crate) data: &'a mut GameData,
    pool: Vec<&'a mut WormData>,
    /// The listed worms, in order: their ids and full health. Worm numbers
    /// (killers, holders, 'it') are positions in it.
    list: Vec<(WormId, i32)>,
    pub(crate) level: Level<'a>,
    pub(crate) s: &'a RuleSettings,
}

impl<'a> Rules<'a> {
    /// Borrows what `f` points to.
    ///
    /// # Safety
    /// `f`'s state, worms and level are valid for `'a`, and nothing else
    /// uses them meanwhile. This checks that the pool's worms are distinct,
    /// that no worm is listed twice, and that each listed worm's data is
    /// the pool's worm of its id.
    pub(crate) unsafe fn new(f: &'a GameFields, s: &'a RuleSettings) -> Self {
        unsafe {
            let pool_ptrs = slice(f.pool, f.pool_count);
            for (i, p) in pool_ptrs.iter().enumerate() {
                assert!(!pool_ptrs[..i].contains(p), "a worm twice in the pool");
            }
            let list: Vec<(WormId, i32)> = slice(f.worms, f.worm_count.max(0) as usize)
                .iter()
                .map(|w| {
                    assert!(
                        std::ptr::eq(pool_ptrs[w.id.0 as usize], w.data),
                        "a listed worm not in the pool"
                    );
                    (w.id, w.max_health)
                })
                .collect();
            for (i, (id, _)) in list.iter().enumerate() {
                assert!(!list[..i].iter().any(|(other, _)| other == id), "a worm listed twice");
            }
            Rules {
                data: &mut *f.game,
                pool: pool_ptrs.iter().map(|&p| &mut *p).collect(),
                list,
                level: f.level.get(),
                s,
            }
        }
    }
}

impl Rules<'_> {
    pub(crate) fn worm_count(&self) -> i32 {
        self.list.len() as i32
    }

    /// The id of worm `i`, a position in the game's worms.
    pub(crate) fn worm_id(&self, i: i32) -> WormId {
        self.list[i as usize].0
    }

    /// Worm `i`'s full health (`settings->health`).
    pub(crate) fn max_health(&self, i: i32) -> i32 {
        self.list[i as usize].1
    }

    /// Worm `i`, a position in the game's worms.
    pub(crate) fn worm(&self, i: i32) -> &WormData {
        self.pool_worm(self.worm_id(i))
    }

    pub(crate) fn worm_mut(&mut self, i: i32) -> &mut WormData {
        self.pool_worm_mut(self.worm_id(i))
    }

    /// The game's worm `id`, listed or not.
    pub(crate) fn pool_worm(&self, id: WormId) -> &WormData {
        self.pool.get(id.0 as usize).unwrap_or_else(|| panic!("no worm {id:?}"))
    }

    pub(crate) fn pool_worm_mut(&mut self, id: WormId) -> &mut WormData {
        self.pool
            .get_mut(id.0 as usize)
            .unwrap_or_else(|| panic!("no worm {id:?}"))
    }

    /// The listed worms as the stats read them.
    pub(crate) fn stats_worms(&self) -> Vec<StatsWorm> {
        (0..self.worm_count())
            .map(|i| stats_worm(self.worm(i), self.max_health(i)))
            .collect()
    }

    fn cycles(&self) -> i32 {
        self.data.cycles
    }

    /// `Game::doDamageDirect`.
    fn damage_direct(&mut self, i: i32, amount: i32, by_idx: i32) {
        if amount > 0 {
            let w = self.worm_mut(i);
            w.health -= amount;
            if w.health <= 0 {
                w.last_killed_by_idx = by_idx;
            }
        }
    }

    /// `Game::doHealingDirect`.
    fn heal_direct(&mut self, i: i32, amount: i32) {
        let max_health = self.max_health(i);
        let scales = self.s.game_mode == Some(GameMode::ScalesOfJustice);
        let w = self.worm_mut(i);
        w.health += amount;
        if scales {
            while w.health > max_health {
                w.lives += 1;
                w.health -= max_health;
            }
        } else if w.health > max_health {
            w.health = max_health;
        }
    }

    /// Splits `amount` between the worms other than `target`, as evenly as
    /// whole numbers allow.
    fn share(&mut self, target: i32, amount: i32, mut f: impl FnMut(&mut Self, i32, i32)) {
        let mut parts = self.worm_count() - 1;
        let mut left = amount;

        for i in 0..self.worm_count() {
            if i != target {
                let k = left / parts;
                f(self, i, k);
                parts -= 1;
                left -= k;
            }
        }
    }

    /// `Game::doDamage`: in Scales of Justice, the killer (or, without one,
    /// everyone else) gets the health the worm loses.
    pub(crate) fn damage(&mut self, target: i32, amount: i32, by_idx: i32) {
        self.damage_direct(target, amount, by_idx);

        if amount > 0 && self.s.game_mode == Some(GameMode::ScalesOfJustice) {
            if by_idx < 0 || by_idx == self.worm(target).index {
                self.share(target, amount, |g, other, k| g.heal_direct(other, k));
            } else {
                self.heal_direct(by_idx, amount);
            }
        }
    }

    /// `Game::doHealing`: in Scales of Justice, the other worms lose the
    /// health the worm gains.
    pub(crate) fn heal(&mut self, target: i32, amount: i32) {
        self.heal_direct(target, amount);

        if self.s.game_mode == Some(GameMode::ScalesOfJustice) {
            let index = self.worm(target).index;
            self.share(target, amount, |g, other, k| g.damage_direct(other, k, index));
        } else {
            let max_health = self.max_health(target);
            let w = self.worm_mut(target);
            if w.health > max_health {
                w.health = max_health;
            }
        }
    }

    /// `checkBonusSpawnPosition`: no dirt or rock within 2 pixels.
    pub(crate) fn bonus_spawn_position_free(&self, x: i32, y: i32) -> bool {
        let lv = &self.level;
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
    pub(crate) fn spawn_zone(&mut self) {
        let z = &mut self.data.holdazone;

        while z.zone_width >= 5 {
            let pos = self
                .level
                .select_spawn(&mut self.data.rand, z.zone_width, z.zone_height - 8);
            if let Some(pos) = pos {
                z.rect = Rect {
                    x1: pos.x,
                    y1: pos.y,
                    x2: pos.x + z.zone_width,
                    y2: pos.y + z.zone_height,
                };
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
    fn game_of_tag(&mut self) {
        let some_invisible = (0..self.worm_count()).any(|i| !self.worm(i).visible);
        let it = self.data.last_killed_idx;

        if !some_invisible && it >= 0 && self.cycles() % 70 == 0 {
            let time_to_lose = self.s.time_to_lose;
            let w = self.worm_mut(it);
            if w.timer < time_to_lose {
                w.timer += 1;
            }
        }
    }

    fn holdazone(&mut self) {
        let s = self.s;

        let mut contender_idx = -1;
        let mut contenders = 0;

        for i in 0..self.worm_count() {
            let w = self.worm(i);
            if w.visible && self.data.holdazone.rect.contains(ftoi(w.pos.x), ftoi(w.pos.y)) {
                contender_idx = w.index;
                contenders += 1;
            }
        }

        let z = &mut self.data.holdazone;

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

                if z.contender_frames < s.zone_capture_time
                    && {
                        z.contender_frames += 1;
                        z.contender_frames >= s.zone_capture_time
                    }
                    && z.holder_idx != z.contender_idx
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

        let holder = self.data.holdazone.holder_idx;
        let dec = if holder >= 0 {
            if self.cycles() % 70 == 0 {
                self.worm_mut(holder).timer += 1;
            }
            true
        } else {
            self.cycles() % 4 == 0
        };

        if dec {
            let z = &mut self.data.holdazone;
            z.timeout_left -= 1;
            if z.timeout_left <= 0 {
                self.spawn_zone();
            }
        }
    }

    /// The game mode's part of the tick.
    pub(crate) fn game_mode_tick(&mut self) {
        match self.s.game_mode {
            Some(GameMode::GameOfTag) => self.game_of_tag(),
            Some(GameMode::Holdazone) => self.holdazone(),
            _ => {}
        }
    }

    fn is_over(&self) -> bool {
        let mut worms = (0..self.worm_count()).map(|i| self.worm(i));
        match self.s.game_mode {
            Some(GameMode::KillEmAll) | Some(GameMode::ScalesOfJustice) => worms.any(|w| w.lives <= 0),
            Some(GameMode::GameOfTag) | Some(GameMode::Holdazone) => worms.any(|w| w.timer >= self.s.time_to_lose),
            _ => false,
        }
    }
}

// The functions C++ calls. In each, the pointers in `game` and `settings`
// must be valid for the call, and nothing else may use what they point to
// meanwhile.

/// Runs the game for one tick: C++'s `Game::processFrame`.
///
/// # Safety
/// See above.
pub unsafe fn process_frame(game: &GameFields, settings: &RuleSettings, hooks: &GameHooks) {
    unsafe {
        World::new(game, settings, hooks).process_frame();
    }
}

/// Hurts worm `target` (a position in `game.worms`) by `amount`, caused by
/// worm `by_idx` or -1: `Game::doDamage`.
///
/// # Safety
/// See above.
pub unsafe fn damage(game: &GameFields, settings: &RuleSettings, target: i32, amount: i32, by_idx: i32) {
    unsafe { Rules::new(game, settings) }.damage(target, amount, by_idx);
}

/// Heals worm `target` by `amount`: `Game::doHealing`.
///
/// # Safety
/// See above.
pub unsafe fn heal(game: &GameFields, settings: &RuleSettings, target: i32, amount: i32) {
    unsafe { Rules::new(game, settings) }.heal(target, amount);
}

/// `Game::startGame`: the start sound, the blood list made
/// `blood_particle_max` long, and in Holdazone the first zone.
///
/// # Safety
/// See above; the TC is valid too.
pub unsafe fn start_game(game: &GameFields, settings: &RuleSettings, blood_particle_max: i32) {
    unsafe {
        game.sound.play(&*game.tc, 22, std::ptr::null_mut(), 0);
        let mut g = Rules::new(game, settings);
        g.data.bobjects.resize(blood_particle_max.max(0) as usize);
        if settings.game_mode == Some(GameMode::Holdazone) {
            g.spawn_zone();
        }
    }
}

/// Whether the game has ended: `Game::isGameOver`.
///
/// # Safety
/// See above.
pub unsafe fn is_over(game: &GameFields, settings: &RuleSettings) -> bool {
    unsafe { Rules::new(game, settings) }.is_over()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worm(health: i32) -> WormData {
        WormData {
            visible: true,
            health,
            lives: 5,
            ..WormData::default()
        }
    }

    fn settings(game_mode: Option<GameMode>) -> RuleSettings {
        RuleSettings {
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
    fn with_game(worms: &mut [WormData], f: impl FnOnce(&GameFields)) {
        let views: Vec<GameWorm> = worms
            .iter_mut()
            .enumerate()
            .map(|(i, w)| {
                w.index = i as i32;
                GameWorm {
                    id: crate::worm::WormId(i as u32),
                    data: w,
                    max_health: 100,
                    ai: std::ptr::null_mut(),
                    name: Str {
                        ptr: std::ptr::null(),
                        len: 0,
                    },
                    color: 0,
                }
            })
            .collect();
        let pool: Vec<*mut WormData> = views.iter().map(|w| w.data).collect();
        let mut game = GameData::new_boxed();
        let fields = GameFields {
            worms: views.as_ptr(),
            worm_count: views.len() as i32,
            pool: pool.as_ptr(),
            pool_count: pool.len(),
            game: &mut *game,
            level: RawLevel {
                data: std::ptr::null_mut(),
                materials: std::ptr::null_mut(),
                width: 0,
                height: 0,
            },
            tc: std::ptr::null(),
            viewports: std::ptr::null(),
            viewport_count: 0,
            spectator_viewports: std::ptr::null(),
            spectator_viewport_count: 0,
            stats: std::ptr::null_mut(),
            sound: SoundOut::NONE,
        };
        f(&fields);
    }

    #[test]
    fn starting_plays_the_sound_and_sizes_the_blood() {
        use std::sync::atomic::{AtomicI32, Ordering};
        static PLAYED: AtomicI32 = AtomicI32::new(-1);
        unsafe fn report(sound: i32) {
            PLAYED.store(sound, Ordering::SeqCst);
        }
        let tc = crate::tc::TcData::empty();
        with_game(&mut [worm(100), worm(100)], |f| {
            let mut f = *f;
            f.tc = &*tc;
            f.sound = SoundOut {
                mixer: std::ptr::null_mut(),
                report: Some(report),
            };
            unsafe {
                start_game(&f, &settings(Some(GameMode::KillEmAll)), 300);
                assert_eq!(((*f.game).bobjects.limit(), (*f.game).bobjects.count), (300, 0));
                // No zone outside Holdazone.
                assert_eq!((*f.game).holdazone.rect.x2, 0);
            }
        });
        assert_eq!(PLAYED.load(Ordering::SeqCst), 22);
    }

    #[test]
    fn new_game_matches_cpp_and_copies_are_separate() {
        {
            let mut g = GameData::new_boxed();
            // What C++'s Game() set.
            assert_eq!((g.cycles, g.last_killed_idx, g.screen_flash), (0, -1, 0));
            assert!(g.paused && !g.got_changed && !g.quick_sim);
            let z = g.holdazone;
            assert_eq!(
                (z.holder_idx, z.contender_idx, z.zone_width, z.zone_height),
                (-1, -1, 50, 34)
            );
            assert_eq!((g.wobjects.count, g.bobjects.limit(), g.bobjects.count), (0, 1, 0));
            assert!(g.nobjects.sentinel.used && g.bonuses.arr[5].frame == -1);

            g.cycles = 7;
            g.quick_sim = true;
            g.holdazone.timeout_left = 3;
            g.rand.next_u32();
            g.sobjects.arr[3].used = true;
            g.wobjects.count = 2;
            g.bobjects.resize(3);
            g.bobjects.count = 2;
            (&mut g.bobjects.arr)[1].color = 9;
            let mut c = g.clone_boxed();
            // Every field is copied, and the blood is separate.
            assert_eq!(format!("{:?}", c), format!("{:?}", g));
            c.cycles = 1;
            (&mut c.bobjects.arr)[1].color = 1;
            assert_eq!((g.cycles, (&g.bobjects.arr)[1].color), (7, 9));

            // Assigning copies everything back.
            c.assign(&g);
            assert_eq!(format!("{:?}", c), format!("{:?}", g));
        }
    }

    #[test]
    fn damage_records_the_killer() {
        let mut worms = [worm(10), worm(100)];
        let s = settings(Some(GameMode::KillEmAll));
        with_game(&mut worms, |g| unsafe {
            damage(g, &s, 0, 5, 1);
            damage(g, &s, 0, 0, 1);
            damage(g, &s, 0, 7, 1);
        });
        assert_eq!((worms[0].health, worms[0].last_killed_by_idx), (-2, 1));
        assert_eq!(worms[1].health, 100);
    }

    #[test]
    fn scales_of_justice_moves_health() {
        let mut worms = [worm(50), worm(90), worm(90)];
        let s = settings(Some(GameMode::ScalesOfJustice));
        with_game(&mut worms, |g| unsafe {
            // Hurt by worm 1: it gets all of it, and an extra life from the
            // overflow.
            damage(g, &s, 0, 20, 1);
        });
        assert_eq!((worms[0].health, worms[1].health, worms[1].lives), (30, 10, 6));

        with_game(&mut worms, |g| unsafe {
            // Self-inflicted: split between the others, 6 and then 7,
            // rounding down in turn.
            damage(g, &s, 0, 13, 0);
            // Healing takes from the others in the same way.
            heal(g, &s, 2, 9);
        });
        assert_eq!(worms[0].health, 30 - 13 - 4);
        assert_eq!(worms[1].health, 10 + 6 - 5);
        assert_eq!((worms[2].health, worms[2].lives), (90 + 7 + 9 - 100, 6));
        assert_eq!(worms[0].last_killed_by_idx, -1);
    }

    #[test]
    fn healing_is_capped_outside_scales_of_justice() {
        let mut worms = [worm(95), worm(100)];
        let s = settings(Some(GameMode::KillEmAll));
        with_game(&mut worms, |g| unsafe { heal(g, &s, 0, 20) });
        assert_eq!((worms[0].health, worms[0].lives), (100, 5));
    }

    #[test]
    fn game_over_by_mode() {
        let mut worms = [worm(100), worm(100)];
        worms[1].lives = 0;
        worms[0].timer = 600;
        for (mode, over) in [
            (Some(GameMode::KillEmAll), true),
            (Some(GameMode::ScalesOfJustice), true),
            (Some(GameMode::GameOfTag), true),
            (Some(GameMode::Holdazone), true),
        ] {
            let s = settings(mode);
            with_game(&mut worms, |g| assert_eq!(unsafe { is_over(g, &s) }, over));
        }
        worms[1].lives = 1;
        worms[0].timer = 599;
        for mode in 0..4 {
            let s = settings(GameMode::of(mode));
            with_game(&mut worms, |g| assert!(!unsafe { is_over(g, &s) }));
        }
    }

    #[test]
    fn rect_contains_excludes_far_edges() {
        let r = Rect {
            x1: 10,
            y1: 20,
            x2: 15,
            y2: 22,
        };
        assert!(r.contains(10, 20) && r.contains(14, 21));
        assert!(!r.contains(15, 20) && !r.contains(10, 22) && !r.contains(9, 20));
    }
}
