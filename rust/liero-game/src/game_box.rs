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

use crate::ai::LieroWormAi;
use crate::controller::LIERO_STATE_GAME;
use crate::game::LieroGameSettings;
use crate::game::{LieroGameData, LieroGameFields, LieroGameWorm, LieroRect};
use crate::gfx::LieroPalette;
use crate::level::LieroLevelBox;
use crate::renderer::{LieroRenderer, LieroRendererData};
use crate::scene::{liero_game_draw_viewport, LieroDrawSettings};
use crate::settings::{LieroSettingsBox, LieroWormSettingsBox, SharedWormSettings};
use crate::sfx::LieroSoundOut;
use crate::shared::Shared;
use crate::stats::LieroStats;
use crate::tc::{tc_data, LieroTcData, SharedTc};
use crate::text::LieroStr;
use crate::viewport::LieroViewport;
use crate::worm::{LieroWormData, WormId};
use std::cell::{Ref, RefMut};

/// A worm: its state, its settings and its bot, if any.
pub struct LieroWorm {
    pub data: LieroWormData,
    /// None before the worm has settings.
    settings: Option<SharedWormSettings>,
    ai: Option<Box<LieroWormAi>>,
}

impl LieroWorm {
    fn new(settings: Option<SharedWormSettings>) -> Box<Self> {
        let mut w = Box::new(LieroWorm {
            data: LieroWormData::default(),
            settings: None,
            ai: None,
        });
        w.set_settings(settings);
        w
    }

    /// `Worm(Worm const&)`: the state copied as it is, rope anchor and all,
    /// with the same settings and no bot.
    fn copy(&self) -> Box<Self> {
        Box::new(LieroWorm {
            data: self.data,
            settings: self.settings.clone(),
            ai: None,
        })
    }

    /// The worm's bot, if any.
    pub(crate) fn ai_mut(&mut self) -> Option<&mut LieroWormAi> {
        self.ai.as_deref_mut()
    }

    fn set_settings(&mut self, s: Option<SharedWormSettings>) {
        self.settings = s;
    }

    /// The worm's settings. Panics if it has none.
    pub(crate) fn settings(&self) -> Ref<'_, LieroWormSettingsBox> {
        self.settings.as_ref().expect("a worm without settings").borrow()
    }

    /// The worm's settings, to change. Panics if it has none.
    pub(crate) fn settings_mut(&mut self) -> RefMut<'_, LieroWormSettingsBox> {
        self.settings.as_ref().expect("a worm without settings").borrow_mut()
    }
}

/// The game.
pub struct LieroGame {
    data: Box<LieroGameData>,
    /// Every worm made for the game, at a fixed address, by `WormId`. A
    /// replay's worms that the game doesn't list stay here, as ropes can be
    /// anchored to them.
    // Boxed so the views of them stay valid as the pool grows.
    #[allow(clippy::vec_box)]
    pool: Vec<Box<LieroWorm>>,
    /// The game's worms (`Game::worms`), in order. A replay's list can have
    /// empty places.
    list: Vec<Option<WormId>>,
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    viewports: Vec<Box<LieroViewport>>,
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    spectators: Vec<Box<LieroViewport>>,
    /// What the game records for the stats screen, at a fixed address.
    stats: Box<LieroStats>,
    /// The level, at a fixed address. Copies of the game have their own.
    level: Box<LieroLevelBox>,
    /// What the game is played with: the settings, the TC and where the
    /// sounds go. Copies share them.
    settings: Option<Shared<LieroSettingsBox>>,
    tc: Option<SharedTc>,
    sound: LieroSoundOut,
}

impl LieroGame {
    /// The game's TC's data, from its context. Panics if it has none.
    pub(crate) fn tc_data(&self) -> Ref<'_, LieroTcData> {
        tc_data(self.tc.as_ref().expect("the game has no TC"))
    }

    /// Where the game's TC's data is, or null if it has none, for the views
    /// made of raw pointers. Only F8 on the menu changes it, never while the
    /// game runs.
    pub(crate) fn tc_ptr(&self) -> *const LieroTcData {
        self.tc.as_ref().map_or(std::ptr::null(), |t| &*tc_data(t))
    }

    /// The game's settings, from its context.
    // The `&mut` is to the settings behind a raw pointer, not to `self`; phase 4
    // of PLAN.md replaces these pointers.
    #[allow(clippy::mut_from_ref)]
    pub(crate) fn settings_box(&self) -> &mut LieroSettingsBox {
        // SAFETY: the game's reference keeps them alive.
        unsafe { &mut *liero_game_settings(self) }
    }

    /// Sets the context.
    pub(crate) fn set_context_data(
        &mut self,
        settings: Option<Shared<LieroSettingsBox>>,
        tc: Option<SharedTc>,
        sound: LieroSoundOut,
    ) {
        (self.settings, self.tc, self.sound) = (settings, tc, sound);
    }

    pub(crate) fn sound(&self) -> LieroSoundOut {
        self.sound
    }

    pub(crate) fn level_box(&self) -> &LieroLevelBox {
        &self.level
    }

    /// The listed worm at `i` (`Game::wormByIdx`), or None for an empty
    /// place.
    pub(crate) fn worm_at(&mut self, i: usize) -> Option<&mut LieroWorm> {
        let id = self.list[i]?;
        Some(self.worm_mut(id))
    }

    pub(crate) fn listed_worms(&mut self) -> impl Iterator<Item = Option<&mut LieroWorm>> {
        let mut by_id: Vec<Option<&mut LieroWorm>> = self.pool.iter_mut().map(|w| Some(&mut **w)).collect();
        self.list
            .iter()
            .map(move |id| id.map(|id| by_id[id.0 as usize].take().expect("a worm listed twice")))
    }

    /// The ids of the listed worms, None for an empty place.
    pub(crate) fn listed_ids(&self) -> &[Option<WormId>] {
        &self.list
    }

    /// The game's worm `id`.
    pub(crate) fn worm_mut(&mut self, id: WormId) -> &mut LieroWorm {
        &mut self.pool[id.0 as usize]
    }

    /// The split-screen viewports.
    pub(crate) fn split_viewports(&self) -> &[Box<LieroViewport>] {
        &self.viewports
    }

    pub(crate) fn data_mut(&mut self) -> &mut LieroGameData {
        &mut self.data
    }

    pub(crate) fn level_box_mut(&mut self) -> &mut LieroLevelBox {
        &mut self.level
    }

    pub(crate) fn stats(&mut self) -> *mut LieroStats {
        &mut *self.stats
    }

    /// Runs `f` with the game's view (`GameView`).
    fn with_view<R>(&mut self, f: impl FnOnce(&LieroGameFields, &LieroGameSettings) -> R) -> R {
        // SAFETY: the view is freed before the game changes.
        unsafe {
            let v = liero_game_view_new(self);
            let r = f(&(*v).fields, &(*v).settings);
            liero_game_view_free(v);
            r
        }
    }

    /// `Game::processFrame`: one tick.
    pub(crate) fn process_frame(&mut self, hooks: &crate::game::LieroGameHooks) {
        // SAFETY: the view is the game's own.
        self.with_view(|f, s| unsafe { crate::game::liero_game_process_frame(f, s, hooks) });
    }

    /// `Game::processAis`: the bots think, timed for the stats with `hooks`.
    pub(crate) fn process_ais(&mut self, hooks: Option<&crate::game::LieroGameHooks>) {
        let d = &self.settings_box().data;
        let ai = crate::ai::sim::LieroAiSettings {
            frames: d.ai_frames,
            mutations: d.ai_mutations,
            traces: d.ai_traces,
        };
        // SAFETY: the view is the game's own.
        self.with_view(|f, s| unsafe { crate::ai::liero_game_process_ais(f, s, &ai, hooks) });
    }

    /// `Game::startGame`.
    pub(crate) fn start(&mut self, blood_particle_max: i32) {
        // SAFETY: the view is the game's own.
        self.with_view(|f, s| unsafe { crate::game::liero_game_start(f, s, blood_particle_max) });
    }

    /// `Game::isGameOver`.
    pub(crate) fn is_over(&mut self) -> bool {
        // SAFETY: the view is the game's own.
        self.with_view(|f, s| unsafe { crate::game::liero_game_is_over(f, s) })
    }

    /// `NormalStatsRecorder::finish`.
    pub(crate) fn stats_finish(&mut self) {
        // SAFETY: the view is the game's own.
        self.with_view(|f, _| unsafe { crate::game::liero_game_stats_finish(f) });
    }

    /// `Game::focus`: the renderer's palette from the level's and the
    /// worms' colours.
    ///
    /// # Safety
    /// `r` is valid.
    pub(crate) unsafe fn focus(&mut self, r: *mut LieroRenderer) {
        unsafe {
            let level_pal = self.level.data().origpal;
            liero_game_origpal(self, &mut (*r).data.origpal, &level_pal);
        }
    }

    /// `Game::draw`: the split-screen viewports, or with `spectators` the
    /// spectator ones, on `r`, and the palette with the TC's colour
    /// animations, the fade and the screen flash.
    ///
    /// # Safety
    /// The game has its context; `r` is valid and the game's worms don't
    /// point into it.
    pub(crate) unsafe fn draw(&mut self, r: *mut LieroRenderer, state: i32, spectators: bool, is_replay: bool) {
        unsafe {
            let d = draw_settings(&self.settings_box().data, &(*r).data, state, is_replay);
            let scr = (*r).bmp.view();
            let v = liero_game_view_new(self);
            let (f, gs) = (&(*v).fields, &(*v).settings);
            let (list, count) = if spectators {
                (f.spectator_viewports, f.spectator_viewport_count)
            } else {
                (f.viewports, f.viewport_count)
            };
            for i in 0..count.max(0) as usize {
                liero_game_draw_viewport(f, gs, &d, *list.add(i), &scr);
            }
            liero_game_view_free(v);
            let data = &mut (*r).data;
            crate::gfx::liero_game_palette(
                &mut data.pal,
                &data.origpal,
                &self.tc_data().color_anim,
                self.data.cycles,
                data.fade_value,
                self.data.screen_flash,
            );
        }
    }

    fn worms(&self) -> impl Iterator<Item = Option<&LieroWorm>> {
        self.list.iter().map(|id| id.map(|id| &*self.pool[id.0 as usize]))
    }

    /// `Game(Game const&)` and `postClone(original, true)`: the state and
    /// split-screen viewports copied, and every worm, at the same ids, with
    /// the same settings. A copy has no spectator viewports and its worms no
    /// bots. A copy's ropes are anchored to its own worms; C++'s stayed on
    /// the original's.
    fn copy(&self) -> Box<Self> {
        Box::new(LieroGame {
            data: self.data.clone_boxed(),
            pool: self.pool.iter().map(|w| w.copy()).collect(),
            list: self.list.clone(),
            viewports: self.viewports.clone(),
            spectators: Vec::new(),
            stats: self.stats.clone(),
            level: {
                let mut l = LieroLevelBox::new();
                l.assign(&self.level);
                l
            },
            settings: self.settings.clone(),
            tc: self.tc.clone(),
            sound: self.sound,
        })
    }
}

/// A new game with no worms or viewports (`Game::Game`).
pub fn liero_game_new() -> *mut LieroGame {
    Box::into_raw(Box::new(LieroGame {
        data: LieroGameData::new_boxed(),
        pool: Vec::new(),
        list: Vec::new(),
        viewports: Vec::new(),
        spectators: Vec::new(),
        stats: Box::default(),
        level: LieroLevelBox::new(),
        settings: None,
        tc: None,
        sound: LieroSoundOut::NONE,
    }))
}

/// # Safety
/// `g` comes from `liero_game_new` or `_clone`, or is null, and isn't used
/// again.
pub unsafe fn liero_game_free(g: *mut LieroGame) {
    unsafe {
        if !g.is_null() {
            drop(Box::from_raw(g));
        }
    }
}

/// A copy of the game, as a replay keeps to rewind to.
pub fn liero_game_clone(g: &LieroGame) -> *mut LieroGame {
    Box::into_raw(g.copy())
}

/// Makes `to` a copy of `from` (rewinding a replay), but for the spectator
/// viewports and the worms' bots, which `to` keeps, by worm id.
pub fn liero_game_assign(to: &mut LieroGame, from: &LieroGame) {
    let c = from.copy();
    to.data.assign(&c.data);
    let LieroGame {
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
    for (w, old) in pool.iter_mut().zip(&mut to.pool) {
        w.ai = old.ai.take();
    }
    to.pool = pool;
    to.list = list;
    to.viewports = viewports;
    *to.stats = *stats;
    to.level.assign(&level);
    (to.settings, to.tc, to.sound) = (settings, tc, sound);
}

/// What drawing a viewport takes of the settings and the renderer.
fn draw_settings(
    s: &crate::settings::LieroSettingsData,
    r: &LieroRendererData,
    state: i32,
    is_replay: bool,
) -> LieroDrawSettings {
    LieroDrawSettings {
        names_on_bonuses: s.names_on_bonuses,
        allow_viewing_spawn_point: s.allow_viewing_spawn_point,
        map: s.map,
        render_res_x: r.render_res_x,
        render_res_y: r.render_res_y,
        state_game: state == LIERO_STATE_GAME,
        is_replay,
    }
}

/// Sets what the game is played with: the settings (or null), the TC (or
/// none) and where its sounds go. The game and its copies keep references
/// to the settings and the TC.
///
/// # Safety
/// `settings` has a reference, or is null.
pub unsafe fn liero_game_set_context(
    g: &mut LieroGame,
    settings: *mut LieroSettingsBox,
    tc: Option<SharedTc>,
    sound: LieroSoundOut,
) {
    unsafe {
        g.set_context_data((!settings.is_null()).then(|| Shared::retain(settings)), tc, sound);
    }
}

/// The game's level, at a fixed address.
pub fn liero_game_level(g: &mut LieroGame) -> *mut LieroLevelBox {
    &mut *g.level
}

/// The game's settings box.
pub fn liero_game_settings(g: &LieroGame) -> *mut LieroSettingsBox {
    g.settings.as_ref().map_or(std::ptr::null_mut(), |s| s.as_ptr())
}

/// The game's state, at a fixed address.
pub fn liero_game_data(g: &mut LieroGame) -> *mut LieroGameData {
    &mut *g.data
}

/// The game's stats, at a fixed address.
pub fn liero_game_stats(g: &mut LieroGame) -> *mut LieroStats {
    &mut *g.stats
}

/// A new worm with `settings` (which may be none for now), which the game
/// keeps but doesn't list.
pub fn liero_game_new_worm(g: &mut LieroGame, settings: Option<SharedWormSettings>) -> WormId {
    g.pool.push(LieroWorm::new(settings));
    WormId(g.pool.len() as u32 - 1)
}

/// Adds worm `w`, or an empty place for None, to the end of the game's
/// list (`Game::addWorm`).
pub fn liero_game_list_worm(g: &mut LieroGame, w: Option<WormId>) {
    assert!(w.is_none_or(|w| (w.0 as usize) < g.pool.len()), "not the game's worm");
    g.list.push(w);
}

pub fn liero_game_worm_count(g: &LieroGame) -> usize {
    g.list.len()
}

/// The game's worm `i`, or null for an empty place.
pub fn liero_game_worm(g: &mut LieroGame, i: usize) -> *mut LieroWorm {
    g.worm_at(i).map_or(std::ptr::null_mut(), |w| w)
}

pub fn liero_worm_data(w: &mut LieroWorm) -> *mut LieroWormData {
    &mut w.data
}

/// The worm's settings, if it has any.
pub fn liero_worm_settings(w: &LieroWorm) -> Option<SharedWormSettings> {
    w.settings.clone()
}

pub fn liero_worm_set_settings(w: &mut LieroWorm, s: Option<SharedWormSettings>) {
    w.set_settings(s);
}

/// Gives the worm the bot for `controller`, or none for a human (0). See
/// `liero_worm_ai_new`.
pub fn liero_worm_set_ai(w: &mut LieroWorm, controller: u32, candidates: usize, testing: bool) {
    let ai = crate::ai::liero_worm_ai_new(controller, candidates, testing);
    // SAFETY: a new bot, or null, which isn't read.
    w.ai = (!ai.is_null()).then(|| unsafe { Box::from_raw(ai) });
}

/// The worm's bot, or null.
pub fn liero_worm_ai(w: &mut LieroWorm) -> *mut LieroWormAi {
    w.ai.as_deref_mut().map_or(std::ptr::null_mut(), |a| a)
}

/// Adds a viewport (`Game::addViewport`, or `addSpectatorViewport` with
/// `spectator_list`), at a fixed address.
pub fn liero_game_add_viewport(
    g: &mut LieroGame,
    rect: LieroRect,
    worm_idx: i32,
    level_width: i32,
    level_height: i32,
    kind: i32,
    spectator_list: bool,
) -> *mut LieroViewport {
    let mut vp = LieroViewport::new(rect, worm_idx, level_width, level_height);
    vp.kind = kind;
    if spectator_list {
        let mut b = Box::new(vp);
        let p: *mut LieroViewport = &mut *b;
        g.spectators.push(b);
        p
    } else {
        let mut b = Box::new(vp);
        let p: *mut LieroViewport = &mut *b;
        g.viewports.push(b);
        p
    }
}

/// `Game::clearViewports`.
pub fn liero_game_clear_viewports(g: &mut LieroGame) {
    g.viewports.clear();
    g.spectators.clear();
}

pub fn liero_game_viewport_count(g: &LieroGame, spectator_list: bool) -> usize {
    if spectator_list {
        g.spectators.len()
    } else {
        g.viewports.len()
    }
}

pub fn liero_game_viewport(g: &mut LieroGame, spectator_list: bool, i: usize) -> *mut LieroViewport {
    if spectator_list {
        &mut *g.spectators[i]
    } else {
        &mut *g.viewports[i]
    }
}

/// The palette the game starts from (`Game::updateSettings`): the level's,
/// with the first two worms' colours from their settings.
pub fn liero_game_origpal(g: &LieroGame, origpal: &mut LieroPalette, level_pal: &LieroPalette) {
    *origpal = *level_pal;
    for w in g.worms().flatten() {
        if (0..2).contains(&w.data.index) {
            let s = w.settings();
            crate::gfx::liero_palette_set_worm_colour(origpal, w.data.index, s.data.color, &s.data.rgb);
        }
    }
}

/// The game as the tick, the bots and the viewports see it, with the
/// lists it points to.
pub struct LieroGameView {
    fields: LieroGameFields,
    settings: LieroGameSettings,
    _worms: Vec<LieroGameWorm>,
    _pool: Vec<*mut LieroWormData>,
    _viewports: Vec<*mut LieroViewport>,
    _spectators: Vec<*mut LieroViewport>,
}

/// The game's fields and settings, valid while the game's worms, viewports
/// and settings stay and the view isn't freed. The game has its settings
/// and TC (`liero_game_set_context`).
pub fn liero_game_view_new(g: &mut LieroGame) -> *mut LieroGameView {
    // SAFETY: the game keeps its context alive.
    let s = &*g.settings_box();
    let settings = game_settings(&s.data, g.tc.as_ref().map(|t| tc_data(t)).as_deref());
    let pool: Vec<*mut LieroWormData> = g.pool.iter_mut().map(|w| &mut w.data as *mut LieroWormData).collect();
    let worms: Vec<LieroGameWorm> = g
        .list
        .iter()
        .map(|&id| {
            let id = id.expect("an empty place among the game's worms");
            let w = &mut g.pool[id.0 as usize];
            // The name stays where it is while the menus don't change it,
            // which they don't while a view is used.
            let (max_health, name, color) = {
                let s = w.settings();
                (
                    s.data.health,
                    LieroStr {
                        ptr: s.name.as_ptr(),
                        len: s.name.len(),
                    },
                    s.data.color,
                )
            };
            LieroGameWorm {
                data: &mut w.data,
                id,
                max_health,
                ai: liero_worm_ai(w),
                name,
                color,
            }
        })
        .collect();
    let viewports: Vec<*mut LieroViewport> = g.viewports.iter_mut().map(|v| &mut **v as *mut LieroViewport).collect();
    let spectators: Vec<*mut LieroViewport> = g
        .spectators
        .iter_mut()
        .map(|v| &mut **v as *mut LieroViewport)
        .collect();
    let fields = LieroGameFields {
        worms: worms.as_ptr(),
        worm_count: worms.len() as i32,
        pool: pool.as_ptr(),
        pool_count: pool.len(),
        game: &mut *g.data,
        level: g.level.data().level,
        tc: g.tc_ptr(),
        viewports: viewports.as_ptr(),
        viewport_count: viewports.len() as i32,
        spectator_viewports: spectators.as_ptr(),
        spectator_viewport_count: spectators.len() as i32,
        stats: &mut *g.stats,
        sound: g.sound,
    };
    Box::into_raw(Box::new(LieroGameView {
        fields,
        settings,
        _worms: worms,
        _pool: pool,
        _viewports: viewports,
        _spectators: spectators,
    }))
}

pub fn liero_game_view_fields(v: &LieroGameView) -> *const LieroGameFields {
    &v.fields
}

pub fn liero_game_view_settings(v: &LieroGameView) -> *const LieroGameSettings {
    &v.settings
}

/// What the rules take of the settings and the TC (`GameView`). With no
/// TC, as in tests, the TC's parts are zero.
pub(crate) fn game_settings(s: &crate::settings::LieroSettingsData, tc: Option<&LieroTcData>) -> LieroGameSettings {
    use crate::tc::{c, h};
    let constant = |name| tc.map_or(0, |tc| tc.constant(c(name)));
    let hack = |name| tc.is_some_and(|tc| tc.hack(h(name)));
    let t = tc.map_or([0; 4], |tc| tc.bonus_rand_timer);
    LieroGameSettings {
        game_mode: s.game_mode as i32,
        max_bonuses: s.max_bonuses,
        time_to_lose: s.time_to_lose,
        zone_timeout: s.zone_timeout,
        zone_capture_time: 70,
        weap_table: s.weap_table.as_ptr(),
        weapon_count: tc.map_or(0, |tc| tc.weapons.len() as i32),
        bonus_rand_timer: [[t[0], t[1]], [t[2], t[3]]],
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

/// # Safety
/// `v` comes from `liero_game_view_new`, or is null, and isn't used again.
pub unsafe fn liero_game_view_free(v: *mut LieroGameView) {
    unsafe {
        if !v.is_null() {
            drop(Box::from_raw(v));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::new_worm_settings;
    use crate::worm::LIERO_GAME_HOLDAZONE;
    use std::rc::Rc;

    fn rect(x1: i32, y1: i32, x2: i32, y2: i32) -> LieroRect {
        LieroRect { x1, y1, x2, y2 }
    }

    fn settings(health: i32, name: &str) -> SharedWormSettings {
        let s = new_worm_settings();
        s.borrow_mut().data.health = health;
        s.borrow_mut().name = name.as_bytes().to_vec();
        s
    }

    /// A game with two listed worms (the first a bot) and an unlisted one,
    /// an empty place, two viewports and a spectator one.
    unsafe fn game(s: &[SharedWormSettings; 3]) -> *mut LieroGame {
        unsafe {
            let g = &mut *liero_game_new();
            let a = liero_game_new_worm(g, Some(s[0].clone()));
            let b = liero_game_new_worm(g, None);
            liero_worm_set_settings(g.worm_mut(b), Some(s[1].clone()));
            let c = liero_game_new_worm(g, Some(s[2].clone()));
            g.worm_mut(a).data.health = 11;
            g.worm_mut(b).data.health = 22;
            g.worm_mut(c).data.health = 33;
            liero_worm_set_ai(g.worm_mut(a), 1, 0, false);
            liero_game_list_worm(g, Some(a));
            liero_game_list_worm(g, None);
            liero_game_list_worm(g, Some(b));
            liero_game_add_viewport(g, rect(0, 0, 158, 158), 0, 504, 350, 0, false);
            liero_game_add_viewport(g, rect(160, 0, 318, 158), 1, 504, 350, 0, false);
            liero_game_add_viewport(g, rect(0, 0, 572, 350), 0, 504, 350, 1, true);
            g
        }
    }

    #[test]
    fn copies_share_the_settings_but_not_bots_or_spectators() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            let g = game(&s);
            assert_eq!(Rc::strong_count(&s[0]), 2);
            assert_eq!(Rc::strong_count(&s[2]), 2);

            let c = &mut *liero_game_clone(&*g);
            assert_eq!(liero_game_worm_count(c), 3);
            assert!(liero_game_worm(c, 1).is_null());
            let (a, b) = (liero_game_worm(c, 0), liero_game_worm(c, 2));
            assert_ne!(a, liero_game_worm(&mut *g, 0));
            assert_eq!(((*a).data.health, (*b).data.health), (11, 22));
            // The unlisted worms are copied too, at the same ids.
            assert_eq!(c.pool.len(), 3);
            assert_eq!(
                (
                    Rc::strong_count(&s[0]),
                    Rc::strong_count(&s[1]),
                    Rc::strong_count(&s[2])
                ),
                (3, 3, 3)
            );
            assert!(!liero_worm_ai(&mut *liero_game_worm(&mut *g, 0)).is_null());
            assert!(liero_worm_ai(&mut *a).is_null());
            assert!(liero_worm_ai(&mut *b).is_null());

            // The state and viewports are the copy's own, and it has no
            // spectator viewport.
            (*a).data.health = 1;
            assert_eq!((*liero_game_worm(&mut *g, 0)).data.health, 11);
            assert_eq!(liero_game_viewport_count(c, false), 2);
            assert_ne!(liero_game_viewport(c, false, 1), liero_game_viewport(&mut *g, false, 1));
            assert_eq!((*liero_game_viewport(c, false, 1)).worm_idx, 1);
            assert_eq!(liero_game_viewport_count(c, true), 0);

            // The stats are the copy's own.
            assert_ne!(liero_game_stats(c), liero_game_stats(&mut *g));
            (*liero_game_stats(&mut *g)).ai_process_time(0, 5);
            assert_ne!(*liero_game_stats(c), *liero_game_stats(&mut *g));

            // So is the level, but not the settings.
            let set = crate::settings::liero_settings_box_new();
            liero_game_set_context(&mut *g, set, None, LieroSoundOut::NONE);
            (*liero_game_level(&mut *g)).resize(3, 2);
            let d = &mut *liero_game_clone(&*g);
            assert_eq!(liero_game_settings(d), set);
            assert_ne!(liero_game_level(d), liero_game_level(&mut *g));
            assert_eq!((*liero_game_level(d)).data().level.width, 3);
            (*liero_game_level(&mut *g)).resize(1, 1);
            assert_eq!((*liero_game_level(d)).data().level.width, 3);
            // Assigning copies them too, keeping the level's address.
            let l = liero_game_level(c);
            liero_game_assign(c, d);
            assert_eq!((liero_game_settings(c), liero_game_level(c)), (set, l));
            assert_eq!((*l).data().level.width, 3);
            liero_game_free(d);
            crate::settings::liero_settings_box_free(set);

            liero_game_free(c);
            assert_eq!(
                (
                    Rc::strong_count(&s[0]),
                    Rc::strong_count(&s[1]),
                    Rc::strong_count(&s[2])
                ),
                (2, 2, 2)
            );
            liero_game_free(g);
            assert!(s.iter().all(|b| Rc::strong_count(b) == 1));
        }
    }

    #[test]
    fn assigning_replaces_the_worms_but_keeps_bots_and_spectators() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            let from = game(&s);
            let to = &mut *liero_game_new();
            let old = liero_game_new_worm(to, Some(s[0].clone()));
            liero_game_list_worm(to, Some(old));
            liero_game_add_viewport(to, rect(0, 0, 1, 1), 0, 1, 1, 0, false);
            let spectator = liero_game_add_viewport(to, rect(0, 0, 1, 1), 0, 1, 1, 0, true);
            liero_worm_set_ai(to.worm_mut(old), 1, 0, false);
            let bot = liero_worm_ai(to.worm_mut(old));
            assert!(!bot.is_null());

            let stats = liero_game_stats(to);
            (*liero_game_stats(&mut *from)).ai_process_time(1, 7);
            liero_game_assign(to, &*from);
            // Copied in place, and not shared.
            assert_eq!(liero_game_stats(to), stats);
            assert_eq!(*stats, *liero_game_stats(&mut *from));
            (*stats).ai_process_time(1, 9);
            assert_ne!(*stats, *liero_game_stats(&mut *from));
            assert_eq!(liero_game_worm_count(to), 3);
            assert_eq!((*liero_game_worm(to, 0)).data.health, 11);
            assert!(liero_game_worm(to, 1).is_null());
            assert_eq!(liero_game_viewport_count(to, false), 2);
            // The old worm is gone: the ids are `from`'s. Its bot stays with
            // its id, and the spectator viewport with the game.
            assert_eq!(to.pool.len(), 3);
            assert_eq!(to.worm_mut(WormId(2)).data.health, 33);
            assert_eq!(liero_worm_ai(to.worm_mut(old)), bot);
            assert!(liero_worm_ai(to.worm_mut(WormId(2))).is_null());
            assert_eq!(liero_game_viewport_count(to, true), 1);
            assert_eq!(liero_game_viewport(to, true, 0), spectator);

            liero_game_clear_viewports(to);
            assert_eq!(
                (
                    liero_game_viewport_count(to, false),
                    liero_game_viewport_count(to, true)
                ),
                (0, 0)
            );
            liero_game_free(to);
            liero_game_free(from);
            assert!(s.iter().all(|b| Rc::strong_count(b) == 1));
        }
    }

    #[test]
    fn the_palette_has_the_first_two_worms_colours() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            for (i, b) in s.iter().enumerate() {
                b.borrow_mut().data.color = 0x50 + 0x10 * i as i32;
                b.borrow_mut().data.rgb = [63, 0, 0];
            }
            let g = &mut *game(&s);
            let level = [crate::gfx::LieroColor {
                r: 1,
                g: 2,
                b: 3,
                unused: 0,
            }; 256];
            let mut pal = [crate::gfx::LieroColor::default(); 256];
            (*liero_game_worm(g, 2)).data.index = 1;
            liero_game_origpal(g, &mut pal, &level);
            // The first worm's colour, and the second's from the worm at
            // index 1, not the empty place.
            assert_eq!((pal[0x50].r, pal[0x59].r), (63, 63));
            assert_eq!((pal[0x60].r, pal[0x79].r), (63, 63));
            assert_eq!(pal[0x70].r, 1);
            assert_eq!((pal[0].r, pal[255].b), (1, 3));
            // Only the first two indexes have colours.
            (*liero_game_worm(g, 0)).data.index = 2;
            liero_game_origpal(g, &mut pal, &level);
            assert_eq!(pal[0x50].r, 1);
            liero_game_free(g);
        }
    }

    #[test]
    fn the_rules_take_the_tcs_constants_hacks_and_bonus_timers() {
        use crate::tc::{c, h};
        let constants: Vec<i32> = (0..liero_data::exe::CONSTANT_ORDER.len() as i32)
            .map(|i| 1000 + i)
            .collect();
        let mut tc = LieroTcData::empty();
        tc.constants = constants;
        tc.bonus_rand_timer = [1, 2, 3, 4];
        // SAFETY: all zeroes is a valid weapon.
        tc.weapons = (0..40).map(|_| crate::tc::LieroTcWeapon::default()).collect();
        tc.hacks = vec![false; liero_data::exe::HACKS.len()];
        let s = crate::settings::liero_settings_box_new();
        unsafe {
            (*s).data.max_bonuses = 6;
            (*s).data.load_change = true;
        }
        let g = game_settings(unsafe { &(*s).data }, Some(&tc));
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
            let mut hacks = vec![false; liero_data::exe::HACKS.len()];
            hacks[h(name)] = true;
            tc.hacks = hacks;
            let g = game_settings(unsafe { &(*s).data }, Some(&tc));
            let got = [
                g.bonus_spawn_rect,
                g.bonus_only_health,
                g.bonus_only_weapon,
                g.bonus_disable,
            ];
            assert_eq!(got, std::array::from_fn(|j| j == i), "{name}");
        }
        unsafe { crate::settings::liero_settings_box_free(s) };
    }

    #[test]
    fn viewports_are_drawn_with_the_settings_and_the_renderers_size() {
        use crate::controller::{LIERO_STATE_GAME_ENDED, LIERO_STATE_WEAPON_SELECTION};
        let b = crate::settings::liero_settings_box_new();
        let mut r = crate::renderer::LieroRenderer::new();
        r.set_render_resolution(640, 400);
        unsafe {
            let s = &mut (*b).data;
            s.names_on_bonuses = true;
            let d = draw_settings(s, &r.data, LIERO_STATE_GAME, true);
            assert_eq!(
                (d.names_on_bonuses, d.allow_viewing_spawn_point, d.map),
                (true, false, false)
            );
            assert_eq!(
                (d.render_res_x, d.render_res_y, d.state_game, d.is_replay),
                (640, 400, true, true)
            );
            (s.names_on_bonuses, s.allow_viewing_spawn_point, s.map) = (false, true, true);
            let d = draw_settings(s, &r.data, LIERO_STATE_GAME_ENDED, false);
            assert_eq!(
                (d.names_on_bonuses, d.allow_viewing_spawn_point, d.map),
                (false, true, true)
            );
            assert!(!d.state_game && !d.is_replay);
            assert!(!draw_settings(s, &r.data, LIERO_STATE_WEAPON_SELECTION, false).state_game);
            crate::settings::liero_settings_box_free(b);
        }
    }

    #[test]
    fn the_view_has_the_worms_and_viewports() {
        unsafe {
            let s = [settings(100, "aa"), settings(50, "bbb"), settings(10, "c")];
            s[1].borrow_mut().data.color = 7;
            let g = &mut *game(&s);
            // The view takes no empty places.
            g.list.remove(1);
            let set = crate::settings::liero_settings_box_new();
            (*set).data.game_mode = LIERO_GAME_HOLDAZONE as u32;
            (*set).data.blood = 37;
            liero_game_set_context(g, set, None, LieroSoundOut::NONE);
            (*liero_game_level(g)).resize(5, 4);
            let v = liero_game_view_new(g);
            let f = &*liero_game_view_fields(&*v);
            assert_eq!((f.level.width, f.level.height), (5, 4));
            assert_eq!(f.level.data, (*liero_game_level(g)).data().level.data);
            let gs = &*liero_game_view_settings(&*v);
            assert_eq!(
                (gs.game_mode, gs.blood, gs.zone_capture_time),
                (LIERO_GAME_HOLDAZONE, 37, 70)
            );
            assert_eq!(gs.weap_table, (*set).data.weap_table.as_ptr());
            // No TC, no TC constants.
            assert_eq!((gs.weapon_count, gs.bonus_drop_chance, gs.bonus_disable), (0, 0, false));
            assert_eq!(f.worm_count, 2);
            let w = std::slice::from_raw_parts(f.worms, 2);
            assert_eq!((w[0].max_health, w[1].max_health), (100, 50));
            assert_eq!((w[1].color, w[1].name.len), (7, 3));
            assert_eq!(w[1].data, liero_worm_data(&mut *liero_game_worm(g, 1)));
            assert_eq!(w[0].ai, liero_worm_ai(&mut *liero_game_worm(g, 0)));
            assert!(w[1].ai.is_null());
            assert_eq!(f.game, liero_game_data(g));
            assert_eq!(f.stats, liero_game_stats(g));
            assert_eq!((f.viewport_count, f.spectator_viewport_count), (2, 1));
            assert_eq!(*f.viewports.add(1), liero_game_viewport(g, false, 1));
            assert_eq!(*f.spectator_viewports, liero_game_viewport(g, true, 0));
            liero_game_view_free(v);
            liero_game_free(g);
            crate::settings::liero_settings_box_free(set);
        }
    }
}
