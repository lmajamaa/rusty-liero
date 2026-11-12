//! The game as C++'s `Game` holds it: its state, its worms and its
//! viewports, which Rust owns. C++'s `Worm` and `Viewport` are views on
//! them.
//!
//! A worm holds a reference to its settings box, which the C++
//! `WormSettings` share, and its bot, which copies of the game share as
//! C++'s did. Copies of the game share the spectator viewports too, as
//! C++'s `Game(Game const&)` and `postClone` left them.

use std::cell::UnsafeCell;
use std::rc::Rc;

use crate::ai::LieroWormAi;
use crate::game::{LieroGameData, LieroGameFields, LieroGameWorm, LieroRect};
use crate::gfx::LieroColor;
use crate::controller::LIERO_STATE_GAME;
use crate::game::LieroGameSettings;
use crate::renderer::{LieroRenderer, LieroRendererData};
use crate::scene::{liero_game_draw_viewport, LieroDrawSettings};
use crate::level::LieroLevelBox;
use crate::settings::{LieroSettingsBox, LieroWormSettingsBox, WormSettingsRef};
use crate::sfx::LieroSoundOut;
use crate::stats::LieroStats;
use crate::shared::Shared;
use crate::tc::{LieroTc, LieroTcData};
use crate::text::LieroStr;
use crate::viewport::LieroViewport;
use crate::worm::LieroWormData;

/// A worm: its state, its settings and its bot, if any.
pub struct LieroWorm {
    pub data: LieroWormData,
    /// None before the worm has settings.
    settings: Option<WormSettingsRef>,
    ai: Option<Rc<UnsafeCell<LieroWormAi>>>,
}

impl LieroWorm {
    fn new(settings: *mut LieroWormSettingsBox) -> Box<Self> {
        let mut w = Box::new(LieroWorm { data: LieroWormData::default(), settings: None, ai: None });
        w.set_settings(settings);
        w
    }

    /// `Worm(Worm const&)`: the state copied as it is, rope anchor and all,
    /// with the same settings and bot.
    fn copy(&self) -> Box<Self> {
        Box::new(LieroWorm { data: self.data, settings: self.settings.clone(), ai: self.ai.clone() })
    }

    /// `s` is a valid box, or null.
    fn set_settings(&mut self, s: *mut LieroWormSettingsBox) {
        // SAFETY: the callers pass valid boxes.
        self.settings = (!s.is_null()).then(|| unsafe { WormSettingsRef::retain(s) });
    }

    pub(crate) fn settings(&self) -> &LieroWormSettingsBox {
        self.settings.as_deref().expect("a worm without settings")
    }

    pub(crate) fn settings_mut(&mut self) -> &mut LieroWormSettingsBox {
        self.settings.as_deref_mut().expect("a worm without settings")
    }
}

/// The game.
pub struct LieroGame {
    data: Box<LieroGameData>,
    /// Every worm made for the game, at a fixed address. A replay's worms
    /// that the game doesn't list stay here, as ropes can be anchored to
    /// them.
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    pool: Vec<Box<LieroWorm>>,
    /// The game's worms (`Game::worms`), in order. A replay's list can have
    /// empty places.
    list: Vec<*mut LieroWorm>,
    // Boxed so the raw pointers to them stay valid as the list grows.
    #[allow(clippy::vec_box)]
    viewports: Vec<Box<LieroViewport>>,
    spectators: Vec<Rc<UnsafeCell<LieroViewport>>>,
    /// What the game records for the stats screen, at a fixed address.
    stats: Box<LieroStats>,
    /// The level, at a fixed address. Copies of the game have their own.
    level: Box<LieroLevelBox>,
    /// What the game is played with: the settings, the TC and where the
    /// sounds go. Copies share them.
    settings: Option<Shared<LieroSettingsBox>>,
    /// The TC, and its data.
    tc_owner: Option<Shared<LieroTc>>,
    tc: *const LieroTcData,
    sound: LieroSoundOut,
}

impl LieroGame {
    /// The game's TC, from its context.
    pub(crate) fn tc_data(&self) -> *const LieroTcData {
        self.tc
    }

    /// The game's settings, from its context.
    // The `&mut` is to the settings behind a raw pointer, not to `self`; phase 4
    // of PLAN.md replaces these pointers.
    #[allow(clippy::mut_from_ref)]
    pub(crate) fn settings_box(&self) -> &mut LieroSettingsBox {
        // SAFETY: the game's reference keeps them alive.
        unsafe { &mut *liero_game_settings(self) }
    }

    /// Sets the context; `tc` is `tc_owner`'s data, or for tests any TC.
    pub(crate) fn set_context_data(
        &mut self,
        settings: Option<Shared<LieroSettingsBox>>,
        tc_owner: Option<Shared<LieroTc>>,
        tc: *const LieroTcData,
        sound: LieroSoundOut,
    ) {
        (self.settings, self.tc_owner, self.tc, self.sound) = (settings, tc_owner, tc, sound);
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
        // SAFETY: listed worms are in the pool.
        unsafe { self.list[i].as_mut() }
    }

    pub(crate) fn listed_worms(&mut self) -> impl Iterator<Item = Option<&mut LieroWorm>> {
        // SAFETY: listed worms are in the pool, each listed once.
        self.list.iter().map(|&w| unsafe { w.as_mut() })
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
        let ai = crate::ai::sim::LieroAiSettings { frames: d.ai_frames, mutations: d.ai_mutations, traces: d.ai_traces };
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
        let level_pal = self.level.data().origpal;
        liero_game_origpal(self, (*r).data.origpal.as_mut_ptr(), level_pal.as_ptr());
    }

    /// `Game::draw`: the split-screen viewports, or with `spectators` the
    /// spectator ones, on `r`, and the palette with the TC's colour
    /// animations, the fade and the screen flash.
    ///
    /// # Safety
    /// The game has its context; `r` is valid and the game's worms don't
    /// point into it.
    pub(crate) unsafe fn draw(&mut self, r: *mut LieroRenderer, state: i32, spectators: bool, is_replay: bool) {
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
            data.pal.as_mut_ptr(),
            data.origpal.as_ptr(),
            &(*self.tc).color_anim,
            self.data.cycles,
            data.fade_value,
            self.data.screen_flash,
        );
    }

    fn owns(&self, w: *const LieroWorm) -> bool {
        self.pool.iter().any(|p| std::ptr::eq(&**p, w))
    }

    fn worms(&self) -> impl Iterator<Item = Option<&LieroWorm>> {
        // SAFETY: listed worms are in the pool.
        self.list.iter().map(|&w| unsafe { w.as_ref() })
    }

    /// `Game(Game const&)` and `postClone(original, true)`: the state and
    /// split-screen viewports copied, and each listed worm. The copies share
    /// the spectator viewports, and their worms the settings and bots.
    fn copy(&self) -> Box<Self> {
        let mut pool = Vec::new();
        let mut list = Vec::new();
        for w in self.worms() {
            match w {
                Some(w) => {
                    let mut c = w.copy();
                    list.push(&mut *c as *mut LieroWorm);
                    pool.push(c);
                }
                None => list.push(std::ptr::null_mut()),
            }
        }
        Box::new(LieroGame {
            data: self.data.clone_boxed(),
            pool,
            list,
            viewports: self.viewports.clone(),
            spectators: self.spectators.clone(),
            stats: self.stats.clone(),
            level: {
                let mut l = LieroLevelBox::new();
                l.assign(&self.level);
                l
            },
            settings: self.settings.clone(),
            tc_owner: self.tc_owner.clone(),
            tc: self.tc,
            sound: self.sound,
        })
    }
}

/// A new game with no worms or viewports (`Game::Game`).
#[no_mangle]
pub extern "C" fn liero_game_new() -> *mut LieroGame {
    Box::into_raw(Box::new(LieroGame {
        data: LieroGameData::new_boxed(),
        pool: Vec::new(),
        list: Vec::new(),
        viewports: Vec::new(),
        spectators: Vec::new(),
        stats: Box::default(),
        level: LieroLevelBox::new(),
        settings: None,
        tc_owner: None,
        tc: std::ptr::null(),
        sound: LieroSoundOut::NONE,
    }))
}

/// # Safety
/// `g` comes from `liero_game_new` or `_clone`, or is null, and isn't used
/// again.
#[no_mangle]
pub unsafe extern "C" fn liero_game_free(g: *mut LieroGame) {
    if !g.is_null() {
        drop(Box::from_raw(g));
    }
}

/// A copy of the game, as a replay keeps to rewind to.
#[no_mangle]
pub extern "C" fn liero_game_clone(g: &LieroGame) -> *mut LieroGame {
    Box::into_raw(g.copy())
}

/// Makes `to` a copy of `from` (rewinding a replay). `to`'s old worms are
/// kept, as ropes copied before may be anchored to them.
#[no_mangle]
pub extern "C" fn liero_game_assign(to: &mut LieroGame, from: &LieroGame) {
    let c = from.copy();
    to.data.assign(&c.data);
    let LieroGame { pool, list, viewports, spectators, stats, level, settings, tc_owner, tc, sound, .. } = *c;
    to.pool.extend(pool);
    to.list = list;
    to.viewports = viewports;
    to.spectators = spectators;
    *to.stats = *stats;
    to.level.assign(&level);
    (to.settings, to.tc_owner, to.tc, to.sound) = (settings, tc_owner, tc, sound);
}

/// What drawing a viewport takes of the settings and the renderer.
fn draw_settings(s: &crate::settings::LieroSettingsData, r: &LieroRendererData, state: i32, is_replay: bool) -> LieroDrawSettings {
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

/// `Game::draw`; see `LieroGame::draw`.
///
/// # Safety
/// See `LieroGame::draw`.
#[no_mangle]
pub unsafe extern "C" fn liero_game_draw(g: &mut LieroGame, r: *mut LieroRenderer, state: i32, spectators: bool, is_replay: bool) {
    g.draw(r, state, spectators, is_replay);
}

/// Sets what the game is played with: the settings, the TC (or null) and
/// where its sounds go. The game and its copies keep references to the
/// settings and the TC.
///
/// # Safety
/// `settings` and `tc` have references, or are null.
#[no_mangle]
pub unsafe extern "C" fn liero_game_set_context(g: &mut LieroGame, settings: *mut LieroSettingsBox, tc: *mut LieroTc, sound: LieroSoundOut) {
    let tc_owner = (!tc.is_null()).then(|| Shared::retain(tc));
    let data = tc_owner.as_ref().map_or(std::ptr::null(), |t| crate::tc::liero_tc_data(t));
    g.set_context_data((!settings.is_null()).then(|| Shared::retain(settings)), tc_owner, data, sound);
}

/// The game's level, at a fixed address.
#[no_mangle]
pub extern "C" fn liero_game_level(g: &mut LieroGame) -> *mut LieroLevelBox {
    &mut *g.level
}

/// The game's settings box.
#[no_mangle]
pub extern "C" fn liero_game_settings(g: &LieroGame) -> *mut LieroSettingsBox {
    g.settings.as_ref().map_or(std::ptr::null_mut(), |s| s.as_ptr())
}

/// The game's state, at a fixed address.
#[no_mangle]
pub extern "C" fn liero_game_data(g: &mut LieroGame) -> *mut LieroGameData {
    &mut *g.data
}

/// The game's stats, at a fixed address.
#[no_mangle]
pub extern "C" fn liero_game_stats(g: &mut LieroGame) -> *mut LieroStats {
    &mut *g.stats
}

/// A new worm with `settings` (which may be null for now), which the game
/// keeps but doesn't list.
///
/// # Safety
/// `settings` is a valid box, or null.
#[no_mangle]
pub unsafe extern "C" fn liero_game_new_worm(g: &mut LieroGame, settings: *mut LieroWormSettingsBox) -> *mut LieroWorm {
    let mut w = LieroWorm::new(settings);
    let p: *mut LieroWorm = &mut *w;
    g.pool.push(w);
    p
}

/// Adds `w`, one of the game's worms, or an empty place for null, to the
/// end of the game's list (`Game::addWorm`).
#[no_mangle]
pub extern "C" fn liero_game_list_worm(g: &mut LieroGame, w: *mut LieroWorm) {
    assert!(w.is_null() || g.owns(w), "not the game's worm");
    g.list.push(w);
}

#[no_mangle]
pub extern "C" fn liero_game_worm_count(g: &LieroGame) -> usize {
    g.list.len()
}

/// The game's worm `i`, or null for an empty place.
#[no_mangle]
pub extern "C" fn liero_game_worm(g: &LieroGame, i: usize) -> *mut LieroWorm {
    g.list[i]
}

#[no_mangle]
pub extern "C" fn liero_worm_data(w: &mut LieroWorm) -> *mut LieroWormData {
    &mut w.data
}

/// The worm's settings box, or null.
#[no_mangle]
pub extern "C" fn liero_worm_settings(w: &LieroWorm) -> *mut LieroWormSettingsBox {
    w.settings.as_ref().map_or(std::ptr::null_mut(), |s| s.as_ptr())
}

/// # Safety
/// `s` is a valid box, or null.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_set_settings(w: &mut LieroWorm, s: *mut LieroWormSettingsBox) {
    w.set_settings(s);
}

/// Gives the worm the bot for `controller`, or none for a human (0). See
/// `liero_worm_ai_new`.
#[no_mangle]
pub extern "C" fn liero_worm_set_ai(w: &mut LieroWorm, controller: u32, candidates: usize, testing: bool) {
    let ai = crate::ai::liero_worm_ai_new(controller, candidates, testing);
    // SAFETY: a new bot, or null, which isn't read.
    w.ai = (!ai.is_null()).then(|| Rc::new(UnsafeCell::new(*unsafe { Box::from_raw(ai) })));
}

/// The worm's bot, or null.
#[no_mangle]
pub extern "C" fn liero_worm_ai(w: &LieroWorm) -> *mut LieroWormAi {
    w.ai.as_ref().map_or(std::ptr::null_mut(), |a| a.get())
}

/// Adds a viewport (`Game::addViewport`, or `addSpectatorViewport` with
/// `spectator_list`), at a fixed address.
#[no_mangle]
pub extern "C" fn liero_game_add_viewport(
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
        let c = Rc::new(UnsafeCell::new(vp));
        let p = c.get();
        g.spectators.push(c);
        p
    } else {
        let mut b = Box::new(vp);
        let p: *mut LieroViewport = &mut *b;
        g.viewports.push(b);
        p
    }
}

/// `Game::clearViewports`.
#[no_mangle]
pub extern "C" fn liero_game_clear_viewports(g: &mut LieroGame) {
    g.viewports.clear();
    g.spectators.clear();
}

#[no_mangle]
pub extern "C" fn liero_game_viewport_count(g: &LieroGame, spectator_list: bool) -> usize {
    if spectator_list {
        g.spectators.len()
    } else {
        g.viewports.len()
    }
}

#[no_mangle]
pub extern "C" fn liero_game_viewport(g: &mut LieroGame, spectator_list: bool, i: usize) -> *mut LieroViewport {
    if spectator_list {
        g.spectators[i].get()
    } else {
        &mut *g.viewports[i]
    }
}

/// The palette the game starts from (`Game::updateSettings`): the level's,
/// with the first two worms' colours from their settings.
///
/// # Safety
/// `origpal` and `level_pal` hold 256 entries, and the listed worms have
/// settings.
#[no_mangle]
pub unsafe extern "C" fn liero_game_origpal(g: &LieroGame, origpal: *mut LieroColor, level_pal: *const LieroColor) {
    std::ptr::copy_nonoverlapping(level_pal, origpal, 256);
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
    _viewports: Vec<*mut LieroViewport>,
    _spectators: Vec<*mut LieroViewport>,
}

/// The game's fields and settings, valid while the game's worms, viewports
/// and settings stay and the view isn't freed. The game has its settings
/// and TC (`liero_game_set_context`).
#[no_mangle]
pub extern "C" fn liero_game_view_new(g: &mut LieroGame) -> *mut LieroGameView {
    // SAFETY: the game keeps its context alive.
    let (s, tc) = unsafe { (&*g.settings_box(), g.tc.as_ref()) };
    let settings = game_settings(&s.data, tc);
    let worms: Vec<LieroGameWorm> = g
        .list
        .iter()
        .map(|&w| {
            // SAFETY: listed worms are in the pool.
            let w = unsafe { w.as_mut() }.expect("an empty place among the game's worms");
            let s = w.settings();
            let (max_health, name, color) = (s.data.health, LieroStr { ptr: s.name.as_ptr(), len: s.name.len() }, s.data.color);
            LieroGameWorm { data: &mut w.data, max_health, ai: liero_worm_ai(w), name, color }
        })
        .collect();
    let viewports: Vec<*mut LieroViewport> = g.viewports.iter_mut().map(|v| &mut **v as *mut LieroViewport).collect();
    let spectators: Vec<*mut LieroViewport> = g.spectators.iter().map(|v| v.get()).collect();
    let fields = LieroGameFields {
        worms: worms.as_ptr(),
        worm_count: worms.len() as i32,
        game: &mut *g.data,
        level: g.level.data().level,
        tc: g.tc,
        viewports: viewports.as_ptr(),
        viewport_count: viewports.len() as i32,
        spectator_viewports: spectators.as_ptr(),
        spectator_viewport_count: spectators.len() as i32,
        stats: &mut *g.stats,
        sound: g.sound,
    };
    Box::into_raw(Box::new(LieroGameView { fields, settings, _worms: worms, _viewports: viewports, _spectators: spectators }))
}

#[no_mangle]
pub extern "C" fn liero_game_view_fields(v: &LieroGameView) -> *const LieroGameFields {
    &v.fields
}

#[no_mangle]
pub extern "C" fn liero_game_view_settings(v: &LieroGameView) -> *const LieroGameSettings {
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
        weapon_count: tc.map_or(0, |tc| tc.weapon_count as i32),
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
#[no_mangle]
pub unsafe extern "C" fn liero_game_view_free(v: *mut LieroGameView) {
    if !v.is_null() {
        drop(Box::from_raw(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worm::LIERO_GAME_HOLDAZONE;
    use crate::level::liero_level_box_resize;
    use crate::settings::{liero_worm_settings_box_free, liero_worm_settings_box_new};

    fn rect(x1: i32, y1: i32, x2: i32, y2: i32) -> LieroRect {
        LieroRect { x1, y1, x2, y2 }
    }

    fn settings(health: i32, name: &str) -> *mut LieroWormSettingsBox {
        let s = liero_worm_settings_box_new();
        unsafe {
            (*s).data.health = health;
            (*s).name = name.as_bytes().to_vec();
        }
        s
    }

    /// A game with two listed worms (the first a bot) and an unlisted one,
    /// an empty place, two viewports and a spectator one.
    unsafe fn game(s: [*mut LieroWormSettingsBox; 3]) -> *mut LieroGame {
        let g = &mut *liero_game_new();
        let a = liero_game_new_worm(g, s[0]);
        let b = liero_game_new_worm(g, std::ptr::null_mut());
        liero_worm_set_settings(&mut *b, s[1]);
        let c = liero_game_new_worm(g, s[2]);
        (*a).data.health = 11;
        (*b).data.health = 22;
        (*c).data.health = 33;
        liero_worm_set_ai(&mut *a, 1, 0, false);
        liero_game_list_worm(g, a);
        liero_game_list_worm(g, std::ptr::null_mut());
        liero_game_list_worm(g, b);
        liero_game_add_viewport(g, rect(0, 0, 158, 158), 0, 504, 350, 0, false);
        liero_game_add_viewport(g, rect(160, 0, 318, 158), 1, 504, 350, 0, false);
        liero_game_add_viewport(g, rect(0, 0, 572, 350), 0, 504, 350, 1, true);
        g
    }

    #[test]
    fn copies_share_settings_bots_and_spectators() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            let g = game(s);
            assert_eq!((*s[0]).refs(), 2);
            assert_eq!((*s[2]).refs(), 2);

            let c = &mut *liero_game_clone(&*g);
            assert_eq!(liero_game_worm_count(c), 3);
            assert!(liero_game_worm(c, 1).is_null());
            let (a, b) = (liero_game_worm(c, 0), liero_game_worm(c, 2));
            assert_ne!(a, liero_game_worm(&*g, 0));
            assert_eq!(((*a).data.health, (*b).data.health), (11, 22));
            // Only the listed worms are copied.
            assert_eq!(c.pool.len(), 2);
            assert_eq!(((*s[0]).refs(), (*s[1]).refs(), (*s[2]).refs()), (3, 3, 2));
            assert_eq!(liero_worm_ai(&*a), liero_worm_ai(&*liero_game_worm(&*g, 0)));
            assert!(!liero_worm_ai(&*a).is_null());
            assert!(liero_worm_ai(&*b).is_null());

            // The state is the copy's own; the spectator viewport isn't.
            (*a).data.health = 1;
            assert_eq!((*liero_game_worm(&*g, 0)).data.health, 11);
            assert_eq!(liero_game_viewport_count(c, false), 2);
            assert_ne!(liero_game_viewport(c, false, 1), liero_game_viewport(&mut *g, false, 1));
            assert_eq!((*liero_game_viewport(c, false, 1)).worm_idx, 1);
            assert_eq!(liero_game_viewport(c, true, 0), liero_game_viewport(&mut *g, true, 0));

            // The stats are the copy's own.
            assert_ne!(liero_game_stats(c), liero_game_stats(&mut *g));
            (*liero_game_stats(&mut *g)).ai_process_time(0, 5);
            assert_ne!(*liero_game_stats(c), *liero_game_stats(&mut *g));

            // So is the level, but not the settings.
            let set = crate::settings::liero_settings_box_new();
            liero_game_set_context(&mut *g, set, std::ptr::null_mut(), LieroSoundOut::NONE);
            liero_level_box_resize(&mut *liero_game_level(&mut *g), 3, 2);
            let d = &mut *liero_game_clone(&*g);
            assert_eq!(liero_game_settings(d), set);
            assert_ne!(liero_game_level(d), liero_game_level(&mut *g));
            assert_eq!((*liero_game_level(d)).data().level.width, 3);
            liero_level_box_resize(&mut *liero_game_level(&mut *g), 1, 1);
            assert_eq!((*liero_game_level(d)).data().level.width, 3);
            // Assigning copies them too, keeping the level's address.
            let l = liero_game_level(c);
            liero_game_assign(c, d);
            assert_eq!((liero_game_settings(c), liero_game_level(c)), (set, l));
            assert_eq!((*l).data().level.width, 3);
            liero_game_free(d);
            crate::settings::liero_settings_box_free(set);

            liero_game_free(c);
            assert_eq!(((*s[0]).refs(), (*s[1]).refs()), (2, 2));
            liero_game_free(g);
            for b in s {
                assert_eq!((*b).refs(), 1);
                liero_worm_settings_box_free(b);
            }
        }
    }

    #[test]
    fn assigning_keeps_the_old_worms() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            let from = game(s);
            let to = &mut *liero_game_new();
            let old = liero_game_new_worm(to, s[0]);
            liero_game_list_worm(to, old);
            liero_game_add_viewport(to, rect(0, 0, 1, 1), 0, 1, 1, 0, false);
            (*old).data.health = 99;

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
            assert_eq!(liero_game_viewport_count(to, true), 1);
            // A rope copied before may still point at it.
            assert_eq!((*old).data.health, 99);
            assert_eq!(to.pool.len(), 3);

            liero_game_clear_viewports(to);
            assert_eq!((liero_game_viewport_count(to, false), liero_game_viewport_count(to, true)), (0, 0));
            liero_game_free(to);
            liero_game_free(from);
            for b in s {
                assert_eq!((*b).refs(), 1);
                liero_worm_settings_box_free(b);
            }
        }
    }

    #[test]
    fn lists_only_its_own_worms() {
        unsafe {
            let a = &mut *liero_game_new();
            let b = &mut *liero_game_new();
            let w = liero_game_new_worm(b, std::ptr::null_mut());
            assert!(!a.owns(w) && b.owns(w));
            liero_game_free(a);
            liero_game_free(b);
        }
    }

    #[test]
    fn the_palette_has_the_first_two_worms_colours() {
        unsafe {
            let s = [settings(100, "a"), settings(50, "b"), settings(10, "c")];
            for (i, &b) in s.iter().enumerate() {
                (*b).data.color = 0x50 + 0x10 * i as i32;
                (*b).data.rgb = [63, 0, 0];
            }
            let g = &mut *game(s);
            let level = [LieroColor { r: 1, g: 2, b: 3, unused: 0 }; 256];
            let mut pal = [LieroColor::default(); 256];
            (*liero_game_worm(g, 2)).data.index = 1;
            liero_game_origpal(g, pal.as_mut_ptr(), level.as_ptr());
            // The first worm's colour, and the second's from the worm at
            // index 1, not the empty place.
            assert_eq!((pal[0x50].r, pal[0x59].r), (63, 63));
            assert_eq!((pal[0x60].r, pal[0x79].r), (63, 63));
            assert_eq!(pal[0x70].r, 1);
            assert_eq!((pal[0].r, pal[255].b), (1, 3));
            // Only the first two indexes have colours.
            (*liero_game_worm(g, 0)).data.index = 2;
            liero_game_origpal(g, pal.as_mut_ptr(), level.as_ptr());
            assert_eq!(pal[0x50].r, 1);
            liero_game_free(g);
            for b in s {
                liero_worm_settings_box_free(b);
            }
        }
    }

    #[test]
    fn the_rules_take_the_tcs_constants_hacks_and_bonus_timers() {
        use crate::tc::{c, h};
        let constants: Vec<i32> = (0..liero_data::exe::CONSTANT_ORDER.len() as i32).map(|i| 1000 + i).collect();
        let layout = std::alloc::Layout::new::<LieroTcData>();
        // SAFETY: all zeroes is a valid TC: null pointers and zero counts.
        let mut tc: Box<LieroTcData> = unsafe { Box::from_raw(std::alloc::alloc_zeroed(layout).cast()) };
        tc.constants = constants.as_ptr();
        tc.constant_count = constants.len();
        tc.bonus_rand_timer = [1, 2, 3, 4];
        tc.weapon_count = 40;
        let no_hacks = vec![false; liero_data::exe::HACKS.len()];
        tc.hacks = no_hacks.as_ptr();
        tc.hack_count = no_hacks.len();
        let s = crate::settings::liero_settings_box_new();
        unsafe {
            (*s).data.max_bonuses = 6;
            (*s).data.load_change = true;
        }
        let g = game_settings(unsafe { &(*s).data }, Some(&tc));
        assert_eq!(g.bonus_rand_timer, [[1, 2], [3, 4]]);
        assert_eq!((g.weapon_count, g.max_bonuses, g.load_change), (40, 6, true));
        let k = |name| 1000 + c(name) as i32;
        assert_eq!((g.bonus_spawn_rect_x, g.bonus_spawn_rect_y), (k("BonusSpawnRectX"), k("BonusSpawnRectY")));
        assert_eq!((g.bonus_spawn_rect_w, g.bonus_spawn_rect_h), (k("BonusSpawnRectW"), k("BonusSpawnRectH")));
        assert_eq!(g.bonus_drop_chance, k("BonusDropChance"));
        // Each hack sets only its own field.
        let names = ["BonusSpawnRect", "BonusOnlyHealth", "BonusOnlyWeapon", "BonusDisable"];
        for (i, name) in names.iter().enumerate() {
            let mut hacks = vec![false; liero_data::exe::HACKS.len()];
            hacks[h(name)] = true;
            tc.hacks = hacks.as_ptr();
            tc.hack_count = hacks.len();
            let g = game_settings(unsafe { &(*s).data }, Some(&tc));
            let got = [g.bonus_spawn_rect, g.bonus_only_health, g.bonus_only_weapon, g.bonus_disable];
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
            assert_eq!((d.names_on_bonuses, d.allow_viewing_spawn_point, d.map), (true, false, false));
            assert_eq!((d.render_res_x, d.render_res_y, d.state_game, d.is_replay), (640, 400, true, true));
            (s.names_on_bonuses, s.allow_viewing_spawn_point, s.map) = (false, true, true);
            let d = draw_settings(s, &r.data, LIERO_STATE_GAME_ENDED, false);
            assert_eq!((d.names_on_bonuses, d.allow_viewing_spawn_point, d.map), (false, true, true));
            assert!(!d.state_game && !d.is_replay);
            assert!(!draw_settings(s, &r.data, LIERO_STATE_WEAPON_SELECTION, false).state_game);
            crate::settings::liero_settings_box_free(b);
        }
    }

    #[test]
    fn the_view_has_the_worms_and_viewports() {
        unsafe {
            let s = [settings(100, "aa"), settings(50, "bbb"), settings(10, "c")];
            (*s[1]).data.color = 7;
            let g = &mut *game(s);
            // The view takes no empty places.
            g.list.remove(1);
            let set = crate::settings::liero_settings_box_new();
            (*set).data.game_mode = LIERO_GAME_HOLDAZONE as u32;
            (*set).data.blood = 37;
            liero_game_set_context(g, set, std::ptr::null_mut(), LieroSoundOut::NONE);
            liero_level_box_resize(&mut *liero_game_level(g), 5, 4);
            let v = liero_game_view_new(g);
            let f = &*liero_game_view_fields(&*v);
            assert_eq!((f.level.width, f.level.height), (5, 4));
            assert_eq!(f.level.data, (*liero_game_level(g)).data().level.data);
            let gs = &*liero_game_view_settings(&*v);
            assert_eq!((gs.game_mode, gs.blood, gs.zone_capture_time), (LIERO_GAME_HOLDAZONE, 37, 70));
            assert_eq!(gs.weap_table, (*set).data.weap_table.as_ptr());
            // No TC, no TC constants.
            assert_eq!((gs.weapon_count, gs.bonus_drop_chance, gs.bonus_disable), (0, 0, false));
            assert_eq!(f.worm_count, 2);
            let w = std::slice::from_raw_parts(f.worms, 2);
            assert_eq!((w[0].max_health, w[1].max_health), (100, 50));
            assert_eq!((w[1].color, w[1].name.len), (7, 3));
            assert_eq!(w[1].data, liero_worm_data(&mut *liero_game_worm(g, 1)));
            assert_eq!(w[0].ai, liero_worm_ai(&*liero_game_worm(g, 0)));
            assert!(w[1].ai.is_null());
            assert_eq!(f.game, liero_game_data(g));
            assert_eq!(f.stats, liero_game_stats(g));
            assert_eq!((f.viewport_count, f.spectator_viewport_count), (2, 1));
            assert_eq!(*f.viewports.add(1), liero_game_viewport(g, false, 1));
            assert_eq!(*f.spectator_viewports, liero_game_viewport(g, true, 0));
            liero_game_view_free(v);
            liero_game_free(g);
            crate::settings::liero_settings_box_free(set);
            for b in s {
                liero_worm_settings_box_free(b);
            }
        }
    }
}
