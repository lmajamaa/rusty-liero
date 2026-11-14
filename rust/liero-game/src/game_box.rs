//! The game as C++'s `Game` holds it: its state, its worms and its
//! viewports, which Rust owns. C++'s `Worm` and `Viewport` are views on
//! them.
//!
//! A worm holds a reference to its settings box, which the C++
//! `WormSettings` share, and its bot. A copy of the game (a replay's point
//! to rewind to) has no bots and no spectator viewports, since it is never
//! played or drawn; rewinding keeps the game's own. C++'s copies shared
//! them with the original, so they were the same objects after a rewind
//! too.

use crate::ai::WormAi;
use crate::controller::State;
use crate::game::RuleSettings;
use crate::game::{GameData, GameFields, GameWorm, Rect};
use crate::gfx::Palette;
use crate::level::LevelBox;
use crate::renderer::{Renderer, RendererData};
use crate::scene::{DrawSettings, SceneSource, WormLooks};
use crate::settings::{SettingsBox, SharedSettings, SharedWormSettings, WormSettingsBox};
use crate::sfx::SoundOut;
use crate::stats::Stats;
use crate::tc::{tc_data, SharedTc, TcData};
use crate::viewport::Viewport;
use crate::viewport::ViewportKind;
use crate::worm::GameMode;
use crate::worm::{WormData, WormId};
use std::cell::{Ref, RefMut};

/// A worm: its state, its settings and its bot, if any.
pub struct Worm {
    pub data: WormData,
    /// None before the worm has settings.
    settings: Option<SharedWormSettings>,
    ai: Option<Box<WormAi>>,
}

impl Worm {
    fn new(settings: Option<SharedWormSettings>) -> Box<Self> {
        let mut w = Box::new(Worm {
            data: WormData::default(),
            settings: None,
            ai: None,
        });
        w.set_settings(settings);
        w
    }

    /// `Worm(Worm const&)`: the state copied as it is, rope anchor and all,
    /// with the same settings and no bot.
    fn copy(&self) -> Box<Self> {
        Box::new(Worm {
            data: self.data,
            settings: self.settings.clone(),
            ai: None,
        })
    }

    /// The worm's bot, if any.
    pub(crate) fn ai(&self) -> Option<&WormAi> {
        self.ai.as_deref()
    }

    /// The worm's bot, if any.
    pub(crate) fn ai_mut(&mut self) -> Option<&mut WormAi> {
        self.ai.as_deref_mut()
    }

    pub fn set_settings(&mut self, s: Option<SharedWormSettings>) {
        self.settings = s;
    }

    /// The worm's settings. Panics if it has none.
    pub(crate) fn settings(&self) -> Ref<'_, WormSettingsBox> {
        self.settings.as_ref().expect("a worm without settings").borrow()
    }

    /// The worm's settings, to change. Panics if it has none.
    pub(crate) fn settings_mut(&mut self) -> RefMut<'_, WormSettingsBox> {
        self.settings.as_ref().expect("a worm without settings").borrow_mut()
    }
}

/// The game.
pub struct Game {
    data: Box<GameData>,
    /// Every worm made for the game, at a fixed address, by `WormId`. A
    /// replay's worms that the game doesn't list stay here, as ropes can be
    /// anchored to them.
    // Boxed so the views of them stay valid as the pool grows.
    #[allow(clippy::vec_box)]
    pool: Vec<Box<Worm>>,
    /// The game's worms (`Game::worms`), in order. A replay's list can have
    /// empty places.
    list: Vec<Option<WormId>>,
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    viewports: Vec<Box<Viewport>>,
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    spectators: Vec<Box<Viewport>>,
    /// What the game records for the stats screen, at a fixed address.
    stats: Box<Stats>,
    /// The level, at a fixed address. Copies of the game have their own.
    level: Box<LevelBox>,
    /// What the game is played with: the settings, the TC and where the
    /// sounds go. Copies share them.
    settings: Option<SharedSettings>,
    tc: Option<SharedTc>,
    sound: SoundOut,
}

impl Game {
    /// The game's TC's data, from its context. Panics if it has none.
    #[cfg(test)]
    pub(crate) fn tc_data(&self) -> Ref<'_, TcData> {
        tc_data(self.tc.as_ref().expect("the game has no TC"))
    }

    /// Where the game's TC's data is, or null if it has none, for weapon
    /// selection. Only F8 on the menu changes it, never while the game runs.
    pub(crate) fn tc_ptr(&self) -> *const TcData {
        self.tc.as_ref().map_or(std::ptr::null(), |t| &*tc_data(t))
    }

    /// The game's settings, to share. Panics if it has none.
    pub(crate) fn settings(&self) -> SharedSettings {
        self.settings.clone().expect("the game has no settings")
    }

    /// The game's settings, from its context. Panics if it has none.
    pub(crate) fn settings_box(&self) -> RefMut<'_, SettingsBox> {
        self.settings.as_ref().expect("the game has no settings").borrow_mut()
    }

    pub(crate) fn sound(&self) -> SoundOut {
        self.sound.clone()
    }

    pub(crate) fn level_box(&self) -> &LevelBox {
        &self.level
    }

    /// The listed worm at `i` (`Game::wormByIdx`), or None for an empty
    /// place.
    pub(crate) fn worm_at(&mut self, i: usize) -> Option<&mut Worm> {
        let id = self.list[i]?;
        Some(self.worm_mut(id))
    }

    pub(crate) fn listed_worms(&mut self) -> impl Iterator<Item = Option<&mut Worm>> {
        let mut by_id: Vec<Option<&mut Worm>> = self.pool.iter_mut().map(|w| Some(&mut **w)).collect();
        self.list
            .iter()
            .map(move |id| id.map(|id| by_id[id.0 as usize].take().expect("a worm listed twice")))
    }

    /// The ids of the listed worms, None for an empty place.
    pub(crate) fn listed_ids(&self) -> &[Option<WormId>] {
        &self.list
    }

    /// The game's worm `id`.
    pub(crate) fn worm(&self, id: WormId) -> &Worm {
        &self.pool[id.0 as usize]
    }

    /// The game's worm `id`.
    pub(crate) fn worm_mut(&mut self, id: WormId) -> &mut Worm {
        &mut self.pool[id.0 as usize]
    }

    /// The split-screen viewports.
    pub(crate) fn split_viewports(&self) -> &[Box<Viewport>] {
        &self.viewports
    }

    pub(crate) fn data(&self) -> &GameData {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut GameData {
        &mut self.data
    }

    pub(crate) fn level_box_mut(&mut self) -> &mut LevelBox {
        &mut self.level
    }

    pub(crate) fn stats(&self) -> &Stats {
        &self.stats
    }

    /// The game's TC, to share. Panics if it has none.
    pub(crate) fn tc(&self) -> SharedTc {
        self.tc.clone().expect("the game has no TC")
    }

    /// What the stats screen shows of the game besides its stats. Panics if
    /// it has fewer than two worms.
    pub(crate) fn stats_game(&mut self) -> crate::stats::StatsGame {
        let name = |g: &mut Self, i| {
            let w = g.worm_at(i).expect("an empty place among the game's worms");
            w.settings().data.name.clone()
        };
        crate::stats::StatsGame {
            names: [name(self, 0), name(self, 1)],
            game_mode: self.settings_box().data.game_mode as i32,
        }
    }

    /// Runs `f` on the game as the tick, the rules and the bots see it: its
    /// fields, the listed worms' bots in the same order, and the rules'
    /// settings. Panics on an empty place among the listed worms, or if the
    /// game has no settings (`set_context`).
    fn with_view<R>(&mut self, f: impl FnOnce(GameFields<'_>, Vec<Option<&mut WormAi>>, &RuleSettings) -> R) -> R {
        let tc = self.tc.as_ref().map(|t| tc_data(t));
        let settings = game_settings(&self.settings_box().data, tc.as_deref());
        let worms: Vec<GameWorm> = self
            .list
            .iter()
            .map(|&id| {
                let id = id.expect("an empty place among the game's worms");
                GameWorm {
                    id,
                    max_health: self.pool[id.0 as usize].settings().data.health,
                }
            })
            .collect();
        let Game {
            data,
            pool,
            viewports,
            spectators,
            stats,
            level,
            sound,
            ..
        } = self;
        let (pool, mut ais): (Vec<_>, Vec<_>) = pool
            .iter_mut()
            .map(|w| {
                let Worm { data, ai, .. } = &mut **w;
                (data, ai.as_deref_mut())
            })
            .unzip();
        let listed_ais = worms.iter().map(|w| ais[w.id.0 as usize].take()).collect();
        let fields = GameFields {
            worms,
            pool,
            game: data,
            level: level.level(),
            tc: tc.as_deref(),
            viewports: viewports.iter_mut().map(|v| &mut **v).collect(),
            spectator_viewports: spectators.iter_mut().map(|v| &mut **v).collect(),
            stats: Some(stats),
            sound: sound.clone(),
        };
        f(fields, listed_ais, &settings)
    }

    /// `Game::processFrame`: one tick.
    pub(crate) fn process_frame(&mut self, hooks: &crate::game::GameHooks) {
        self.with_view(|f, _, s| crate::game::process_frame(f, s, hooks));
        self.process_viewports();
    }

    /// The listed worms with their settings, for the viewports. Panics on
    /// an empty place.
    fn worms_and_settings<'a>(
        pool: &'a [Box<Worm>],
        list: &[Option<WormId>],
    ) -> Vec<(&'a Worm, Ref<'a, WormSettingsBox>)> {
        list.iter()
            .map(|id| {
                let w = &*pool[id.expect("an empty place among the game's worms").0 as usize];
                (w, w.settings())
            })
            .collect()
    }

    /// `Game::processViewports`: each viewport follows its worm and moves
    /// its banner.
    fn process_viewports(&mut self) {
        let Game {
            pool,
            list,
            viewports,
            spectators,
            ..
        } = self;
        let ws = Self::worms_and_settings(pool, list);
        let views = view_worms(&ws);
        crate::scene::process_viewports(
            &views,
            viewports.iter_mut().chain(spectators.iter_mut()).map(|v| &mut **v),
        );
    }

    /// `Game::processAis`: the bots think, timed for the stats with `hooks`.
    pub(crate) fn process_ais(&mut self, hooks: Option<&crate::game::GameHooks>) {
        let ai = {
            let d = &self.settings_box().data;
            crate::ai::sim::AiSettings {
                frames: d.ext.ai_frames,
                mutations: d.ext.ai_mutations,
                traces: d.ext.ai_traces,
            }
        };
        self.with_view(|mut f, mut ais, s| crate::ai::process_ais(&mut f, &mut ais, s, &ai, hooks));
    }

    /// `Game::startGame`.
    pub(crate) fn start(&mut self, blood_particle_max: i32) {
        self.with_view(|f, _, s| crate::game::start_game(f, s, blood_particle_max));
    }

    /// `Game::isGameOver`.
    pub(crate) fn is_over(&mut self) -> bool {
        self.with_view(|f, _, s| crate::game::is_over(f, s))
    }

    /// `NormalStatsRecorder::finish`.
    pub(crate) fn stats_finish(&mut self) {
        let ws = Self::worms_and_settings(&self.pool, &self.list);
        let worms: Vec<_> = ws
            .iter()
            .map(|(w, s)| crate::game::stats_worm(&w.data, s.data.health))
            .collect();
        drop(ws);
        self.stats.finish(&worms);
    }

    /// `Game::focus`: the renderer's palette from the level's and the
    /// worms' colours.
    ///
    /// # Safety
    /// `r` is valid.
    pub unsafe fn focus(&mut self, r: *mut Renderer) {
        unsafe {
            let level_pal = self.level.data().origpal;
            self.origpal(&mut (*r).data.origpal, &level_pal);
        }
    }

    /// `Game::draw`: the split-screen viewports, or with `spectators` the
    /// spectator ones, on `r`, and the palette with the TC's colour
    /// animations, the fade and the screen flash. The game has its context.
    pub(crate) fn draw(&mut self, r: &mut Renderer, state: State, spectators: bool, is_replay: bool) {
        let Game {
            data,
            pool,
            list,
            viewports,
            spectators: spectator_list,
            level,
            settings,
            tc,
            ..
        } = self;
        let settings = settings.as_ref().expect("the game has no settings").borrow();
        let tc = tc_data(tc.as_ref().expect("the game has no TC"));
        let gs = game_settings(&settings.data, Some(&tc));
        let d = draw_settings(&settings.data, &r.data, state, is_replay);
        let ws = Self::worms_and_settings(pool, list);
        let src = SceneSource {
            data,
            worms: view_worms(&ws),
            tc: &tc,
            level: level.view(),
            settings: &gs,
        };
        let scr = r.bmp.view();
        let spectator_list = spectators.then_some(&mut spectator_list[..]);
        crate::scene::draw_viewports(&src, &d, viewports, spectator_list, &scr);
        let rd = &mut r.data;
        crate::gfx::game_palette(
            &mut rd.pal,
            &rd.origpal,
            &tc.model.color_anim,
            data.cycles,
            rd.fade_value,
            data.screen_flash,
        );
    }

    fn worms(&self) -> impl Iterator<Item = Option<&Worm>> {
        self.list.iter().map(|id| id.map(|id| &*self.pool[id.0 as usize]))
    }

    /// `Game(Game const&)` and `postClone(original, true)`: the state and
    /// split-screen viewports copied, and every worm, at the same ids, with
    /// the same settings. A copy has no spectator viewports and its worms no
    /// bots. A copy's ropes are anchored to its own worms; C++'s stayed on
    /// the original's.
    pub fn copy(&self) -> Box<Self> {
        Box::new(Game {
            data: self.data.clone_boxed(),
            pool: self.pool.iter().map(|w| w.copy()).collect(),
            list: self.list.clone(),
            viewports: self.viewports.clone(),
            spectators: Vec::new(),
            stats: self.stats.clone(),
            level: {
                let mut l = LevelBox::new();
                l.assign(&self.level);
                l
            },
            settings: self.settings.clone(),
            tc: self.tc.clone(),
            sound: self.sound.clone(),
        })
    }
}

impl Game {
    /// A new game with no worms or viewports (`Game::Game`).
    pub fn new() -> Box<Game> {
        Box::new(Game {
            data: GameData::new_boxed(),
            pool: Vec::new(),
            list: Vec::new(),
            viewports: Vec::new(),
            spectators: Vec::new(),
            stats: Box::default(),
            level: LevelBox::new(),
            settings: None,
            tc: None,
            sound: SoundOut::NONE,
        })
    }

    /// Makes the game a copy of `from` (rewinding a replay), but for the
    /// spectator viewports and the worms' bots, which it keeps, by worm id.
    pub fn assign(&mut self, from: &Game) {
        let c = from.copy();
        self.data.assign(&c.data);
        let Game {
            mut pool,
            list,
            viewports,
            stats,
            level,
            settings,
            tc,
            sound,
            ..
        } = *c;
        for (w, old) in pool.iter_mut().zip(&mut self.pool) {
            w.ai = old.ai.take();
        }
        self.pool = pool;
        self.list = list;
        self.viewports = viewports;
        *self.stats = *stats;
        self.level.assign(&level);
        (self.settings, self.tc, self.sound) = (settings, tc, sound);
    }

    /// Sets what the game is played with: the settings and the TC (or none)
    /// and where its sounds go. The game and its copies share the settings
    /// and the TC.
    pub fn set_context(&mut self, settings: Option<SharedSettings>, tc: Option<SharedTc>, sound: SoundOut) {
        (self.settings, self.tc, self.sound) = (settings, tc, sound);
    }

    /// A new worm with `settings` (which may be none for now), which the
    /// game keeps but doesn't list.
    pub fn new_worm(&mut self, settings: Option<SharedWormSettings>) -> WormId {
        self.pool.push(Worm::new(settings));
        WormId(self.pool.len() as u32 - 1)
    }

    /// Adds worm `w`, or an empty place for None, to the end of the game's
    /// list (`Game::addWorm`).
    pub fn list_worm(&mut self, w: Option<WormId>) {
        assert!(
            w.is_none_or(|w| (w.0 as usize) < self.pool.len()),
            "not the game's worm"
        );
        self.list.push(w);
    }

    /// How many places the game's list has.
    pub fn worm_count(&self) -> usize {
        self.list.len()
    }

    /// Adds a viewport (`Game::addViewport`, or `addSpectatorViewport` with
    /// `spectator_list`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_viewport(
        &mut self,
        rect: Rect,
        worm_idx: i32,
        level_width: i32,
        level_height: i32,
        kind: ViewportKind,
        spectator_list: bool,
    ) -> &mut Viewport {
        let mut vp = Viewport::new(rect, worm_idx, level_width, level_height);
        vp.kind = kind;
        let list = if spectator_list {
            &mut self.spectators
        } else {
            &mut self.viewports
        };
        list.push(Box::new(vp));
        list.last_mut().expect("just added")
    }

    /// `Game::clearViewports`.
    pub fn clear_viewports(&mut self) {
        self.viewports.clear();
        self.spectators.clear();
    }

    pub fn viewport_count(&self, spectator_list: bool) -> usize {
        if spectator_list {
            self.spectators.len()
        } else {
            self.viewports.len()
        }
    }

    pub fn viewport_mut(&mut self, spectator_list: bool, i: usize) -> &mut Viewport {
        if spectator_list {
            &mut self.spectators[i]
        } else {
            &mut self.viewports[i]
        }
    }

    /// The palette the game starts from (`Game::updateSettings`): the
    /// level's, with the first two worms' colours from their settings.
    fn origpal(&self, origpal: &mut Palette, level_pal: &Palette) {
        *origpal = *level_pal;
        for w in self.worms().flatten() {
            if (0..2).contains(&w.data.index) {
                let s = w.settings();
                crate::gfx::palette_set_worm_colour(origpal, w.data.index, s.data.color, &s.data.rgb);
            }
        }
    }
}

impl Worm {
    /// Gives the worm the bot for `controller`, or none for a human (0).
    /// See `WormAi::new`.
    pub fn set_ai(&mut self, controller: u32, candidates: usize, testing: bool) {
        self.ai = WormAi::new(controller, candidates, testing);
    }
}

/// The views of the listed worms, in order.
fn view_worms<'a>(ws: &'a [(&'a Worm, Ref<'a, WormSettingsBox>)]) -> Vec<crate::viewport::ViewWorm<'a>> {
    ws.iter()
        .map(|(w, s)| {
            let looks = WormLooks {
                max_health: s.data.health,
                name: &s.data.name,
                color: s.data.color,
            };
            crate::scene::view_worm(&w.data, looks, w.ai())
        })
        .collect()
}

/// What drawing a viewport takes of the settings and the renderer.
fn draw_settings(s: &crate::settings::GameSettings, r: &RendererData, state: State, is_replay: bool) -> DrawSettings {
    DrawSettings {
        names_on_bonuses: s.names_on_bonuses,
        allow_viewing_spawn_point: s.ext.allow_viewing_spawn_point,
        map: s.map,
        render_res_x: r.render_res_x,
        render_res_y: r.render_res_y,
        state_game: state == State::Game,
        is_replay,
    }
}

/// What the rules take of the settings and the TC (`Game::with_view`). With no
/// TC, as in tests, the TC's parts are zero.
pub(crate) fn game_settings(s: &crate::settings::GameSettings, tc: Option<&TcData>) -> RuleSettings {
    use crate::tc::{c, h};
    let constant = |name| tc.map_or(0, |tc| tc.constant(c(name)));
    let hack = |name| tc.is_some_and(|tc| tc.hack(h(name)));
    RuleSettings {
        game_mode: GameMode::of(s.game_mode),
        max_bonuses: s.max_bonuses,
        time_to_lose: s.time_to_lose,
        zone_timeout: s.ext.zone_timeout,
        zone_capture_time: 70,
        weap_table: s.weap_table,
        weapon_count: tc.map_or(0, |tc| tc.model.weapons.len() as i32),
        bonus_rand_timer: tc.map_or([[0; 2]; 2], |tc| tc.model.bonus_rand_timer),
        bonus_spawn_rect_x: constant("BonusSpawnRectX"),
        bonus_spawn_rect_y: constant("BonusSpawnRectY"),
        bonus_spawn_rect_w: constant("BonusSpawnRectW"),
        bonus_spawn_rect_h: constant("BonusSpawnRectH"),
        bonus_drop_chance: constant("BonusDropChance"),
        bonus_spawn_rect: hack("BonusSpawnRect"),
        bonus_only_health: hack("BonusOnlyHealth"),
        bonus_only_weapon: hack("BonusOnlyWeapon"),
        bonus_disable: hack("BonusDisable"),
        blood: s.blood,
        shadow: s.shadow,
        loading_time: s.loading_time,
        load_change: s.load_change,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::new_worm_settings;
    use crate::worm::GameMode;
    use std::rc::Rc;

    fn rect(x1: i32, y1: i32, x2: i32, y2: i32) -> Rect {
        Rect { x1, y1, x2, y2 }
    }

    fn settings(health: i32, name: &str) -> SharedWormSettings {
        let s = new_worm_settings();
        s.borrow_mut().data.health = health;
        s.borrow_mut().data.name = name.as_bytes().to_vec();
        s
    }

    /// A game with two listed worms (the first a bot) and an unlisted one,
    /// an empty place, two viewports and a spectator one.
    fn game(s: &[SharedWormSettings; 3]) -> Box<Game> {
        let mut g = Game::new();
        let a = g.new_worm(Some(s[0].clone()));
        let b = g.new_worm(None);
        g.worm_mut(b).set_settings(Some(s[1].clone()));
        let c = g.new_worm(Some(s[2].clone()));
        g.worm_mut(a).data.health = 11;
        g.worm_mut(b).data.health = 22;
        g.worm_mut(c).data.health = 33;
        g.worm_mut(a).set_ai(1, 0, false);
        g.list_worm(Some(a));
        g.list_worm(None);
        g.list_worm(Some(b));
        g.add_viewport(rect(0, 0, 158, 158), 0, 504, 350, ViewportKind::Worm, false);
        g.add_viewport(rect(160, 0, 318, 158), 1, 504, 350, ViewportKind::Worm, false);
        g.add_viewport(rect(0, 0, 572, 350), 0, 504, 350, ViewportKind::Spectator, true);
        g
    }

    #[test]
    fn copies_share_the_settings_but_not_bots_or_spectators() {
        let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
        let mut g = game(&s);
        let counts = |s: &[SharedWormSettings; 3]| s.each_ref().map(Rc::strong_count);
        assert_eq!(counts(&s), [2, 2, 2]);

        let mut c = g.copy();
        assert_eq!(c.worm_count(), 3);
        assert!(c.worm_at(1).is_none());
        assert!(!std::ptr::eq(c.worm(WormId(0)), g.worm(WormId(0))));
        assert_eq!((c.worm(WormId(0)).data.health, c.worm(WormId(1)).data.health), (11, 22));
        // The unlisted worms are copied too, at the same ids.
        assert_eq!(c.pool.len(), 3);
        assert_eq!(counts(&s), [3, 3, 3]);
        assert!(g.worm(WormId(0)).ai().is_some());
        assert!(c.worm(WormId(0)).ai().is_none() && c.worm(WormId(1)).ai().is_none());

        // The state and viewports are the copy's own, and it has no
        // spectator viewport.
        c.worm_mut(WormId(0)).data.health = 1;
        assert_eq!(g.worm(WormId(0)).data.health, 11);
        assert_eq!(c.viewport_count(false), 2);
        assert!(!std::ptr::eq(&*c.viewports[1], &*g.viewports[1]));
        assert_eq!(c.viewport_mut(false, 1).worm_idx, 1);
        assert_eq!(c.viewport_count(true), 0);

        // The stats are the copy's own.
        assert!(!std::ptr::eq(c.stats(), g.stats()));
        g.stats.ai_process_time(0, 5);
        assert_ne!(*c.stats(), *g.stats());

        // So is the level, but not the settings.
        let shared = crate::settings::new_settings();
        g.set_context(Some(shared.clone()), None, SoundOut::NONE);
        g.level_box_mut().resize(3, 2);
        let d = g.copy();
        assert!(Rc::ptr_eq(&d.settings(), &shared));
        assert!(!std::ptr::eq(d.level_box(), g.level_box()));
        assert_eq!(d.level_box().data().level.width, 3);
        g.level_box_mut().resize(1, 1);
        assert_eq!(d.level_box().data().level.width, 3);
        // Assigning copies them too, keeping the level's address.
        let l: *const LevelBox = c.level_box();
        c.assign(&d);
        assert!(Rc::ptr_eq(&c.settings(), &shared));
        assert!(std::ptr::eq(c.level_box(), l));
        assert_eq!(c.level_box().data().level.width, 3);
        drop(d);

        drop(c);
        assert_eq!(counts(&s), [2, 2, 2]);
        drop(g);
        assert!(s.iter().all(|b| Rc::strong_count(b) == 1));
    }

    #[test]
    fn assigning_replaces_the_worms_but_keeps_bots_and_spectators() {
        let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
        let mut from = game(&s);
        let mut to = Game::new();
        let old = to.new_worm(Some(s[0].clone()));
        to.list_worm(Some(old));
        to.add_viewport(rect(0, 0, 1, 1), 0, 1, 1, ViewportKind::Worm, false);
        let spectator: *const Viewport = to.add_viewport(rect(0, 0, 1, 1), 0, 1, 1, ViewportKind::Worm, true);
        to.worm_mut(old).set_ai(1, 0, false);
        let bot: *const WormAi = to.worm(old).ai().expect("a bot");

        let stats: *const Stats = to.stats();
        from.stats.ai_process_time(1, 7);
        to.assign(&from);
        // Copied in place, and not shared.
        assert!(std::ptr::eq(to.stats(), stats));
        assert_eq!(*to.stats(), *from.stats());
        to.stats.ai_process_time(1, 9);
        assert_ne!(*to.stats(), *from.stats());
        assert_eq!(to.worm_count(), 3);
        assert_eq!(to.worm_at(0).expect("a worm").data.health, 11);
        assert!(to.worm_at(1).is_none());
        assert_eq!(to.viewport_count(false), 2);
        // The old worm is gone: the ids are `from`'s. Its bot stays with
        // its id, and the spectator viewport with the game.
        assert_eq!(to.pool.len(), 3);
        assert_eq!(to.worm(WormId(2)).data.health, 33);
        assert!(std::ptr::eq(to.worm(old).ai().expect("the bot"), bot));
        assert!(to.worm(WormId(2)).ai().is_none());
        assert_eq!(to.viewport_count(true), 1);
        assert!(std::ptr::eq(to.viewport_mut(true, 0), spectator));

        to.clear_viewports();
        assert_eq!((to.viewport_count(false), to.viewport_count(true)), (0, 0));
        drop(to);
        drop(from);
        assert!(s.iter().all(|b| Rc::strong_count(b) == 1));
    }

    #[test]
    fn the_palette_has_the_first_two_worms_colours() {
        let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
        for (i, b) in s.iter().enumerate() {
            b.borrow_mut().data.color = 0x50 + 0x10 * i as i32;
            b.borrow_mut().data.rgb = [63, 0, 0];
        }
        let mut g = game(&s);
        let level = [crate::gfx::Color {
            r: 1,
            g: 2,
            b: 3,
            unused: 0,
        }; 256];
        let mut pal = [crate::gfx::Color::default(); 256];
        g.worm_at(2).expect("a worm").data.index = 1;
        g.origpal(&mut pal, &level);
        // The first worm's colour, and the second's from the worm at
        // index 1, not the empty place.
        assert_eq!((pal[0x50].r, pal[0x59].r), (63, 63));
        assert_eq!((pal[0x60].r, pal[0x79].r), (63, 63));
        assert_eq!(pal[0x70].r, 1);
        assert_eq!((pal[0].r, pal[255].b), (1, 3));
        // Only the first two indexes have colours.
        g.worm_at(0).expect("a worm").data.index = 2;
        g.origpal(&mut pal, &level);
        assert_eq!(pal[0x50].r, 1);
    }

    #[test]
    fn the_rules_take_the_tcs_constants_hacks_and_bonus_timers() {
        use crate::tc::c;
        let order = liero_data::exe::CONSTANT_ORDER;
        let hacks = |on: Option<&str>| {
            liero_data::exe::HACKS
                .iter()
                .map(|&(n, _)| (n, Some(n) == on))
                .collect()
        };
        let mut tc = TcData::empty();
        tc.model.constants = order.iter().zip(1000..).map(|(&n, v)| (n, v)).collect();
        tc.model.bonus_rand_timer = [[1, 2], [3, 4]];
        tc.model.weapons = (0..40).map(|_| liero_data::model::Weapon::default()).collect();
        tc.model.hacks = hacks(None);
        let s = crate::settings::new_settings();
        s.borrow_mut().data.max_bonuses = 6;
        s.borrow_mut().data.load_change = true;
        let g = game_settings(&s.borrow().data, Some(&tc));
        assert_eq!(g.bonus_rand_timer, [[1, 2], [3, 4]]);
        assert_eq!((g.weapon_count, g.max_bonuses, g.load_change), (40, 6, true));
        let k = |name| 1000 + c(name) as i32;
        assert_eq!(
            (g.bonus_spawn_rect_x, g.bonus_spawn_rect_y),
            (k("BonusSpawnRectX"), k("BonusSpawnRectY"))
        );
        assert_eq!(
            (g.bonus_spawn_rect_w, g.bonus_spawn_rect_h),
            (k("BonusSpawnRectW"), k("BonusSpawnRectH"))
        );
        assert_eq!(g.bonus_drop_chance, k("BonusDropChance"));
        // Each hack sets only its own field.
        let names = ["BonusSpawnRect", "BonusOnlyHealth", "BonusOnlyWeapon", "BonusDisable"];
        for (i, name) in names.iter().enumerate() {
            tc.model.hacks = hacks(Some(name));
            let g = game_settings(&s.borrow().data, Some(&tc));
            let got = [
                g.bonus_spawn_rect,
                g.bonus_only_health,
                g.bonus_only_weapon,
                g.bonus_disable,
            ];
            assert_eq!(got, std::array::from_fn(|j| j == i), "{name}");
        }
    }

    #[test]
    fn viewports_are_drawn_with_the_settings_and_the_renderers_size() {
        use crate::controller::State;
        let b = crate::settings::new_settings();
        let mut r = crate::renderer::Renderer::new();
        r.set_render_resolution(640, 400);
        {
            let mut b = b.borrow_mut();
            let s = &mut b.data;
            s.names_on_bonuses = true;
            let d = draw_settings(s, &r.data, State::Game, true);
            assert_eq!(
                (d.names_on_bonuses, d.allow_viewing_spawn_point, d.map),
                (true, false, false)
            );
            assert_eq!(
                (d.render_res_x, d.render_res_y, d.state_game, d.is_replay),
                (640, 400, true, true)
            );
            (s.names_on_bonuses, s.ext.allow_viewing_spawn_point, s.map) = (false, true, true);
            let d = draw_settings(s, &r.data, State::GameEnded, false);
            assert_eq!(
                (d.names_on_bonuses, d.allow_viewing_spawn_point, d.map),
                (false, true, true)
            );
            assert!(!d.state_game && !d.is_replay);
            assert!(!draw_settings(s, &r.data, State::WeaponSelection, false).state_game);
        }
    }

    #[test]
    fn the_view_has_the_worms_and_viewports() {
        let s = [settings(100, "aa"), settings(50, "bbb"), settings(10, "c")];
        s[1].borrow_mut().data.color = 7;
        let mut g = game(&s);
        // The view takes no empty places.
        g.list.remove(1);
        let set = crate::settings::new_settings();
        set.borrow_mut().data.game_mode = GameMode::Holdazone as u32;
        set.borrow_mut().data.blood = 37;
        g.set_context(Some(set.clone()), None, SoundOut::NONE);
        g.level_box_mut().resize(5, 4);
        let level: *const u8 = g.level_box().pixels().as_ptr();
        let data: *const GameData = g.data();
        let stats: *const Stats = g.stats();
        let worm1: *const WormData = &g.worm(WormId(1)).data;
        let bot: *const WormAi = g.worm(WormId(0)).ai().expect("a bot");
        let viewport: *const Viewport = &*g.viewports[1];
        let spectator: *const Viewport = &*g.spectators[0];
        let table = set.borrow().data.weap_table;
        g.with_view(|f, ais, gs| {
            assert_eq!((f.level.width, f.level.height), (5, 4));
            assert_eq!(f.level.data.as_ptr(), level);
            assert_eq!(
                (gs.game_mode, gs.blood, gs.zone_capture_time),
                (Some(GameMode::Holdazone), 37, 70)
            );
            assert_eq!(gs.weap_table, table);
            // No TC, no TC constants.
            assert_eq!((gs.weapon_count, gs.bonus_drop_chance, gs.bonus_disable), (0, 0, false));
            assert!(f.tc.is_none());
            let w = &f.worms;
            assert_eq!((w.len(), w[0].max_health, w[1].max_health), (2, 100, 50));
            assert_eq!((w[0].id, w[1].id), (WormId(0), WormId(1)));
            // The pool has every worm, listed or not.
            assert_eq!(f.pool.len(), 3);
            assert!(std::ptr::eq(f.worm(1), worm1));
            assert!(std::ptr::eq(ais[0].as_deref().expect("a bot"), bot));
            assert!(ais[1].is_none());
            assert!(std::ptr::eq(&*f.game, data));
            assert!(std::ptr::eq(f.stats.as_deref().expect("stats"), stats));
            assert_eq!((f.viewports.len(), f.spectator_viewports.len()), (2, 1));
            assert!(std::ptr::eq(&*f.viewports[1], viewport));
            assert!(std::ptr::eq(&*f.spectator_viewports[0], spectator));
        });
    }
}
