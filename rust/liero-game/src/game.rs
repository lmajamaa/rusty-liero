//! The game tick (`Game::processFrame` in `game.cpp`) and the game rules
//! around it: spawning bonuses, Game of Tag and Holdazone timers, damage
//! and healing (with Scales of Justice moving health between worms) and the
//! end of the game. Results have to stay bit-identical to the C++ versions,
//! which `tools/hashrun` checks.
//!
//! Rust owns the game's state (`GameData`, with the object lists) and
//! each worm's (`WormData`). The rules and the tick (in `world.rs`)
//! borrow them, through `Game`, for the call.

use crate::level::Level;
use crate::objects::{BObjectList, BonusList, NObjectList, SObjectList, WObjectList};
use crate::rand::Rand;
use crate::sfx::SoundOut;
use crate::stats::{Stats, StatsWorm};
use crate::tc::TcData;
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
    pub holder_idx: Option<i32>,
    /// The worm taking the zone, or -1.
    pub contender_idx: Option<i32>,
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

/// A listed worm, as the game rules see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameWorm {
    /// Its place in the game's pool, which ropes are anchored by.
    pub id: WormId,
    /// The worm's full health (`settings->health`).
    pub max_health: i32,
}

/// The game's state, besides its worms and level. Rust owns it, and the
/// fields of C++'s `Game` refer to it. Its object lists keep their slots
/// on the heap, so it can be made and copied anywhere.
#[derive(Clone, Debug)]
pub struct GameData {
    pub cycles: i32,
    /// The worm that died last, which is 'it' in Game of Tag, or -1.
    pub last_killed_idx: Option<i32>,
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

impl Default for GameData {
    fn default() -> Self {
        Self::new()
    }
}

impl GameData {
    /// What C++'s `Game` constructor set, before it seeds `rand`.
    pub fn new() -> Self {
        let mut g = GameData {
            cycles: 0,
            last_killed_idx: None,
            got_changed: false,
            screen_flash: 0,
            paused: true,
            quick_sim: false,
            holdazone: Holdazone {
                rect: Rect::default(),
                holder_idx: None,
                contender_idx: None,
                contender_frames: 0,
                timeout_left: 0,
                zone_width: 50,
                zone_height: 34,
            },
            rand: Rand::default(),
            bonuses: BonusList::default(),
            wobjects: WObjectList::default(),
            sobjects: SObjectList::default(),
            nobjects: NObjectList::default(),
            bobjects: BObjectList::new(1),
        };
        g.bonuses.init();
        g.wobjects.clear();
        g.sobjects.clear();
        g.nobjects.clear();
        g
    }
}

/// The game state the rules read and change, borrowed. `worms` is in the
/// order of C++'s `Game::worms`; worm numbers (killers, holders, 'it') are
/// positions in it.
pub struct GameFields<'a> {
    pub worms: Vec<GameWorm>,
    /// Every worm the game has, by `WormId`, listed or not.
    pub pool: Vec<&'a mut WormData>,
    pub game: &'a mut GameData,
    pub level: Level<'a>,
    /// The TC. Only the tick reads it, so the rules may go without.
    pub tc: Option<&'a TcData>,
    /// The viewports, whose shake and death banners the tick moves.
    pub viewports: Vec<&'a mut Viewport>,
    pub spectator_viewports: Vec<&'a mut Viewport>,
    /// The stats the tick records, if any.
    pub stats: Option<&'a mut Stats>,
    /// Where its sounds go.
    pub sound: SoundOut,
}

impl GameFields<'_> {
    /// The same game, borrowed again for a shorter while.
    pub fn reborrow(&mut self) -> GameFields<'_> {
        GameFields {
            worms: self.worms.clone(),
            pool: self.pool.iter_mut().map(|w| &mut **w).collect(),
            game: self.game,
            level: self.level.reborrow(),
            tc: self.tc,
            viewports: self.viewports.iter_mut().map(|v| &mut **v).collect(),
            spectator_viewports: self.spectator_viewports.iter_mut().map(|v| &mut **v).collect(),
            stats: self.stats.as_deref_mut(),
            sound: self.sound.clone(),
        }
    }

    /// The listed worm at `place`.
    pub fn worm(&self, place: usize) -> &WormData {
        self.pool[self.worms[place].id.0 as usize]
    }

    /// The listed worm at `place`, to change.
    pub fn worm_mut(&mut self, place: usize) -> &mut WormData {
        self.pool[self.worms[place].id.0 as usize]
    }
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
    pub weap_table: [u32; 40],
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
        x: w.pos.x.to_int(),
        y: w.pos.y.to_int(),
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
    /// The listed worms, in order. Worm numbers (killers, holders, 'it')
    /// are positions in it.
    list: Vec<GameWorm>,
    pub(crate) level: Level<'a>,
    pub(crate) s: &'a RuleSettings,
}

impl<'a> Rules<'a> {
    /// The rules on `f`'s state, worms and level. Panics if a worm is
    /// listed twice or isn't in the pool.
    pub(crate) fn new(f: GameFields<'a>, s: &'a RuleSettings) -> Self {
        for (i, w) in f.worms.iter().enumerate() {
            assert!((w.id.0 as usize) < f.pool.len(), "a listed worm not in the pool");
            assert!(!f.worms[..i].iter().any(|o| o.id == w.id), "a worm listed twice");
        }
        Rules {
            data: f.game,
            pool: f.pool,
            list: f.worms,
            level: f.level,
            s,
        }
    }
}

impl Rules<'_> {
    pub(crate) fn worm_count(&self) -> i32 {
        self.list.len() as i32
    }

    /// The id of worm `i`, a position in the game's worms.
    pub(crate) fn worm_id(&self, i: i32) -> WormId {
        self.list[i as usize].id
    }

    /// Worm `i`'s full health (`settings->health`).
    pub(crate) fn max_health(&self, i: i32) -> i32 {
        self.list[i as usize].max_health
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
                w.last_killed_by_idx = (by_idx >= 0).then_some(by_idx);
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
                z.contender_idx = None;
                z.contender_frames = 0;
                z.holder_idx = None;
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

        if let Some(it) = it.filter(|_| !some_invisible && self.cycles() % 70 == 0) {
            let time_to_lose = self.s.time_to_lose;
            let w = self.worm_mut(it);
            if w.timer < time_to_lose {
                w.timer += 1;
            }
        }
    }

    fn holdazone(&mut self) {
        let s = self.s;

        let mut contender_idx = None;
        let mut contenders = 0;

        for i in 0..self.worm_count() {
            let w = self.worm(i);
            if w.visible && self.data.holdazone.rect.contains(w.pos.x.to_int(), w.pos.y.to_int()) {
                contender_idx = Some(w.index);
                contenders += 1;
            }
        }

        let z = &mut self.data.holdazone;

        if contenders == 0 {
            contender_idx = z.holder_idx;
        }

        if contenders <= 1 {
            if contender_idx.is_none() || (z.contender_idx != contender_idx && z.contender_frames != 0) {
                if z.contender_frames == 0 || {
                    z.contender_frames -= 1;
                    z.contender_frames == 0
                } {
                    z.contender_idx = contender_idx;
                    z.holder_idx = None;
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
                    if z.contender_idx.is_some() {
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
        let dec = if let Some(holder) = holder {
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

/// Runs the game for one tick: C++'s `Game::processFrame`. Panics without
/// the TC.
pub fn process_frame(game: GameFields, settings: &RuleSettings, hooks: &GameHooks) {
    World::new(game, settings, hooks).process_frame();
}

/// Hurts worm `target` (a position in `game.worms`) by `amount`, caused by
/// worm `by_idx` or -1: `Game::doDamage`.
pub fn damage(game: GameFields, settings: &RuleSettings, target: i32, amount: i32, by_idx: i32) {
    Rules::new(game, settings).damage(target, amount, by_idx);
}

/// Heals worm `target` by `amount`: `Game::doHealing`.
pub fn heal(game: GameFields, settings: &RuleSettings, target: i32, amount: i32) {
    Rules::new(game, settings).heal(target, amount);
}

/// `Game::startGame`: the start sound, the blood list made
/// `blood_particle_max` long, and in Holdazone the first zone. Panics
/// without the TC.
pub fn start_game(game: GameFields, settings: &RuleSettings, blood_particle_max: i32) {
    let tc = game.tc.expect("starting a game without a TC");
    game.sound.play(tc, 22, crate::sfx::SoundId::NONE, 0);
    let mut g = Rules::new(game, settings);
    g.data.bobjects.resize(blood_particle_max.max(0) as usize);
    if settings.game_mode == Some(GameMode::Holdazone) {
        g.spawn_zone();
    }
}

/// Whether the game has ended: `Game::isGameOver`.
pub fn is_over(game: GameFields, settings: &RuleSettings) -> bool {
    Rules::new(game, settings).is_over()
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
            weap_table: [0; 40],
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
    fn with_game(worms: &mut [WormData], f: impl FnOnce(&mut GameFields)) {
        with_game_tc(worms, None, f);
    }

    /// As `with_game`, with `tc`.
    fn with_game_tc(worms: &mut [WormData], tc: Option<&crate::tc::TcData>, f: impl FnOnce(&mut GameFields)) {
        let list = (0..worms.len())
            .map(|i| GameWorm {
                id: WormId(i as u32),
                max_health: 100,
            })
            .collect();
        let pool = worms
            .iter_mut()
            .enumerate()
            .map(|(i, w)| {
                w.index = i as i32;
                w
            })
            .collect();
        let mut game = GameData::new();
        let mut fields = GameFields {
            worms: list,
            pool,
            game: &mut game,
            level: Level {
                data: &mut [],
                materials: &mut [],
                width: 0,
                height: 0,
            },
            tc,
            viewports: Vec::new(),
            spectator_viewports: Vec::new(),
            stats: None,
            sound: SoundOut::NONE,
        };
        f(&mut fields);
    }

    #[test]
    fn starting_plays_the_sound_and_sizes_the_blood() {
        let played = std::rc::Rc::new(std::cell::Cell::new(-1));
        let tc = crate::tc::TcData::empty();
        with_game_tc(&mut [worm(100), worm(100)], Some(&tc), |f| {
            let played = played.clone();
            f.sound = SoundOut {
                mixer: None,
                report: Some(std::rc::Rc::new(move |s| played.set(s))),
            };
            start_game(f.reborrow(), &settings(Some(GameMode::KillEmAll)), 300);
            assert_eq!((f.game.bobjects.limit(), f.game.bobjects.count), (300, 0));
            // No zone outside Holdazone.
            assert_eq!(f.game.holdazone.rect.x2, 0);
        });
        assert_eq!(played.get(), 22);
    }

    #[test]
    fn new_game_matches_cpp_and_copies_are_separate() {
        {
            let mut g = GameData::new();
            // What C++'s Game() set.
            assert_eq!((g.cycles, g.last_killed_idx, g.screen_flash), (0, None, 0));
            assert!(g.paused && !g.got_changed && !g.quick_sim);
            let z = g.holdazone;
            assert_eq!(
                (z.holder_idx, z.contender_idx, z.zone_width, z.zone_height),
                (None, None, 50, 34)
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
            let mut c = g.clone();
            // Every field is copied, and the blood is separate.
            assert_eq!(format!("{:?}", c), format!("{:?}", g));
            c.cycles = 1;
            (&mut c.bobjects.arr)[1].color = 1;
            assert_eq!((g.cycles, (&g.bobjects.arr)[1].color), (7, 9));

            // Assigning copies everything back.
            c.clone_from(&g);
            assert_eq!(format!("{:?}", c), format!("{:?}", g));
        }
    }

    #[test]
    fn damage_records_the_killer() {
        let mut worms = [worm(10), worm(100)];
        let s = settings(Some(GameMode::KillEmAll));
        with_game(&mut worms, |g| {
            damage(g.reborrow(), &s, 0, 5, 1);
            damage(g.reborrow(), &s, 0, 0, 1);
            damage(g.reborrow(), &s, 0, 7, 1);
        });
        assert_eq!((worms[0].health, worms[0].last_killed_by_idx), (-2, Some(1)));
        assert_eq!(worms[1].health, 100);
    }

    #[test]
    fn scales_of_justice_moves_health() {
        let mut worms = [worm(50), worm(90), worm(90)];
        let s = settings(Some(GameMode::ScalesOfJustice));
        with_game(&mut worms, |g| {
            // Hurt by worm 1: it gets all of it, and an extra life from the
            // overflow.
            damage(g.reborrow(), &s, 0, 20, 1);
        });
        assert_eq!((worms[0].health, worms[1].health, worms[1].lives), (30, 10, 6));

        with_game(&mut worms, |g| {
            // Self-inflicted: split between the others, 6 and then 7,
            // rounding down in turn.
            damage(g.reborrow(), &s, 0, 13, 0);
            // Healing takes from the others in the same way.
            heal(g.reborrow(), &s, 2, 9);
        });
        assert_eq!(worms[0].health, 30 - 13 - 4);
        assert_eq!(worms[1].health, 10 + 6 - 5);
        assert_eq!((worms[2].health, worms[2].lives), (90 + 7 + 9 - 100, 6));
        assert_eq!(worms[0].last_killed_by_idx, None);
    }

    #[test]
    fn healing_is_capped_outside_scales_of_justice() {
        let mut worms = [worm(95), worm(100)];
        let s = settings(Some(GameMode::KillEmAll));
        with_game(&mut worms, |g| heal(g.reborrow(), &s, 0, 20));
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
            with_game(&mut worms, |g| assert_eq!(is_over(g.reborrow(), &s), over));
        }
        worms[1].lives = 1;
        worms[0].timer = 599;
        for mode in 0..4 {
            let s = settings(GameMode::of(mode));
            with_game(&mut worms, |g| assert!(!is_over(g.reborrow(), &s)));
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
