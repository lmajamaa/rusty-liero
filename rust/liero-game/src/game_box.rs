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
use crate::level::LieroLevel;
use crate::settings::{liero_worm_settings_box_free, liero_worm_settings_box_retain, LieroWormSettingsBox};
use crate::sfx::LieroSoundOut;
use crate::stats::LieroStats;
use crate::tc::LieroTcData;
use crate::text::LieroStr;
use crate::viewport::LieroViewport;
use crate::worm::LieroWormData;

/// A worm: its state, its settings and its bot, if any.
pub struct LieroWorm {
    pub data: LieroWormData,
    /// One reference, or null before the worm has settings.
    settings: *mut LieroWormSettingsBox,
    ai: Option<Rc<UnsafeCell<LieroWormAi>>>,
}

impl LieroWorm {
    fn new(settings: *mut LieroWormSettingsBox) -> Box<Self> {
        let mut w = Box::new(LieroWorm { data: LieroWormData::default(), settings: std::ptr::null_mut(), ai: None });
        w.set_settings(settings);
        w
    }

    /// `Worm(Worm const&)`: the state copied as it is, rope anchor and all,
    /// with the same settings and bot.
    fn copy(&self) -> Box<Self> {
        let mut w = Box::new(LieroWorm { data: self.data, settings: std::ptr::null_mut(), ai: self.ai.clone() });
        w.set_settings(self.settings);
        w
    }

    fn set_settings(&mut self, s: *mut LieroWormSettingsBox) {
        // SAFETY: boxes passed in are valid, and `self.settings` holds a
        // reference.
        unsafe {
            if !s.is_null() {
                liero_worm_settings_box_retain(s);
            }
            liero_worm_settings_box_free(self.settings);
        }
        self.settings = s;
    }

    fn settings(&self) -> &LieroWormSettingsBox {
        assert!(!self.settings.is_null(), "a worm without settings");
        // SAFETY: the worm holds a reference.
        unsafe { &*self.settings }
    }
}

impl Drop for LieroWorm {
    fn drop(&mut self) {
        self.set_settings(std::ptr::null_mut());
    }
}

/// The game.
pub struct LieroGame {
    data: Box<LieroGameData>,
    /// Every worm made for the game, at a fixed address. A replay's worms
    /// that the game doesn't list stay here, as ropes can be anchored to
    /// them.
    pool: Vec<Box<LieroWorm>>,
    /// The game's worms (`Game::worms`), in order. A replay's list can have
    /// empty places.
    list: Vec<*mut LieroWorm>,
    viewports: Vec<Box<LieroViewport>>,
    spectators: Vec<Rc<UnsafeCell<LieroViewport>>>,
    /// What the game records for the stats screen, at a fixed address.
    stats: Box<LieroStats>,
}

impl LieroGame {
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
    let LieroGame { pool, list, viewports, spectators, stats, .. } = *c;
    to.pool.extend(pool);
    to.list = list;
    to.viewports = viewports;
    to.spectators = spectators;
    *to.stats = *stats;
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
    w.settings
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
pub unsafe extern "C" fn liero_worm_set_ai(w: &mut LieroWorm, controller: u32, candidates: usize, testing: bool) {
    let ai = crate::ai::liero_worm_ai_new(controller, candidates, testing);
    w.ai = (!ai.is_null()).then(|| Rc::new(UnsafeCell::new(*Box::from_raw(ai))));
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

/// What the game's fields take from C++.
#[repr(C)]
pub struct LieroGameExtras {
    pub level: LieroLevel,
    pub tc: *const LieroTcData,
    pub sound: LieroSoundOut,
}

/// The game as the tick, the bots and the viewports see it, with the
/// lists it points to.
pub struct LieroGameView {
    fields: LieroGameFields,
    _worms: Vec<LieroGameWorm>,
    _viewports: Vec<*mut LieroViewport>,
    _spectators: Vec<*mut LieroViewport>,
}

/// The game's fields, valid while the game's worms and viewports stay and
/// the view isn't freed.
#[no_mangle]
pub extern "C" fn liero_game_view_new(g: &mut LieroGame, extras: &LieroGameExtras) -> *mut LieroGameView {
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
        level: extras.level,
        tc: extras.tc,
        viewports: viewports.as_ptr(),
        viewport_count: viewports.len() as i32,
        spectator_viewports: spectators.as_ptr(),
        spectator_viewport_count: spectators.len() as i32,
        stats: &mut *g.stats,
        sound: extras.sound,
    };
    Box::into_raw(Box::new(LieroGameView { fields, _worms: worms, _viewports: viewports, _spectators: spectators }))
}

#[no_mangle]
pub extern "C" fn liero_game_view_fields(v: &LieroGameView) -> *const LieroGameFields {
    &v.fields
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
    use crate::settings::liero_worm_settings_box_new;

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
    fn the_view_has_the_worms_and_viewports() {
        unsafe {
            let s = [settings(100, "aa"), settings(50, "bbb"), settings(10, "c")];
            (*s[1]).data.color = 7;
            let g = &mut *game(s);
            // The view takes no empty places.
            g.list.remove(1);
            let extras = LieroGameExtras {
                level: LieroLevel { data: std::ptr::null_mut(), materials: std::ptr::null_mut(), width: 0, height: 0 },
                tc: std::ptr::null(),
                sound: LieroSoundOut::NONE,
            };
            let v = liero_game_view_new(g, &extras);
            let f = &*liero_game_view_fields(&*v);
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
            for b in s {
                liero_worm_settings_box_free(b);
            }
        }
    }
}
