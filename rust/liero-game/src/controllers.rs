//! The game's controllers (`LocalController` and `ReplayController` in
//! C++): a match played here, from weapon selection to the stats screen, or
//! a replay played back. They own their games, and run, draw and fade them.
//! What they open on top of the game (a message, the stats screen) waits in
//! the controller for the game screen to push.

use std::ffi::c_void;

use crate::app::app;
use crate::boxes::liero_info_box_screen;
use crate::controller::{
    LieroControllerData, LIERO_FADE_SHOWN, LIERO_STATE_GAME, LIERO_STATE_GAME_ENDED, LIERO_STATE_INITIAL,
    LIERO_STATE_WEAPON_SELECTION,
};
use crate::fs::LieroFsNode;
use crate::game::{LieroGameHooks, LieroRect};
use crate::game_box::{
    liero_game_add_viewport, liero_game_clone, liero_game_list_worm, liero_game_new, liero_game_new_worm,
    liero_game_set_context, liero_worm_set_ai, LieroGame,
};
use crate::keys::{liero_local_controller_key, LieroWormKeys, LIERO_DK_ESCAPE};
use crate::level::LieroLevelBox;
use crate::renderer::LieroRenderer;
use crate::replay::LieroReplayStatus;
use crate::replay_file::{
    liero_replay_reader_begin, liero_replay_reader_frame, liero_replay_reader_new, liero_replay_writer_begin,
    liero_replay_writer_focus, liero_replay_writer_frame, liero_replay_writer_new, liero_replay_writer_unfocus,
    LieroReplayOutcome, LieroReplayReader, LieroReplayWriter,
};
use crate::screen::{current_host, LieroScreen};
use crate::settings::{liero_settings_box_free, liero_settings_box_new, LieroSettingsBox, LIERO_MAX_CONTROL_EX};
use crate::stats::{liero_stats_presenter_screen, LieroSprite, LieroStatsScene};
use crate::tc::LieroTc;
use crate::text::LieroStr;
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use crate::weapsel::{
    liero_weapsel_draw_game, liero_weapsel_finish, liero_weapsel_process_game, liero_weapsel_set_focused,
    liero_weapsel_start, tc_font, LieroWeapsel,
};

/// `SDL_SCANCODE_R`, which rewinds a replay.
const SCANCODE_R: i32 = 21;

/// The game modes' names on the stats screen (`Texts::gameModes`).
const GAME_MODE_NAMES: [&str; 4] = ["Kill'em All", "Game of Tag", "Holdazone", "Scales of Justice"];

/// A match played here.
struct Local {
    game: Box<LieroGame>,
    ws: Option<Box<LieroWeapsel>>,
    replay: Option<Box<LieroReplayWriter>>,
}

/// A replay played back.
struct Replay {
    reader: Option<Box<LieroReplayReader>>,
    /// The TC the game is made with.
    tc: *mut LieroTc,
    game: Option<Box<LieroGame>>,
    /// The game as it started, which R rewinds to, and where in the replay
    /// that was.
    initial: Option<Box<LieroGame>>,
    initial_pos: usize,
}

enum Kind {
    Local(Local),
    Replay(Replay),
}

/// A controller (`Controller`).
pub struct LieroController {
    /// What both kinds share (`CommonController`), which C++ refers to.
    data: LieroControllerData,
    kind: Kind,
    /// A screen opened over the game, for the game screen to push.
    opened: Option<LieroScreen>,
}

/// A new game with the settings, the TC and the app's sound, its clock
/// seeded from the time, as C++'s `Game` constructor did.
unsafe fn new_game(settings: *mut LieroSettingsBox, tc: *mut LieroTc) -> Box<LieroGame> {
    let host = current_host();
    let mut game = Box::from_raw(liero_game_new());
    liero_game_set_context(&mut game, settings, tc, host.sound());
    crate::rand::liero_rand_seed(&mut game.data_mut().rand, host.time() as u32);
    game
}

unsafe extern "C" fn host_hires_ticks(_: *mut c_void) -> u64 {
    current_host().hires_ticks()
}

/// The game's clock for its stats, with the viewports moved.
fn game_hooks() -> LieroGameHooks {
    LieroGameHooks { ctx: std::ptr::null_mut(), hires_ticks: host_hires_ticks, process_viewports: true }
}

unsafe fn split_viewports(game: &mut LieroGame, worm0: i32, worm1: i32) {
    let r = |x1, y1, x2, y2| LieroRect { x1, y1, x2, y2 };
    liero_game_add_viewport(game, r(0, 0, 158, 158), worm0, 504, 350, LIERO_VIEWPORT_WORM, false);
    liero_game_add_viewport(game, r(160, 0, 158 + 160, 158), worm1, 504, 350, LIERO_VIEWPORT_WORM, false);
}

/// The spectator viewport is always full size, +68 on x to align it in the
/// middle.
unsafe fn spectator_viewport(game: &mut LieroGame, spectator_list: bool) {
    let r = LieroRect { x1: 0, y1: 0, x2: 504 + 68, y2: 350 };
    liero_game_add_viewport(game, r, 0, 504, 350, LIERO_VIEWPORT_SPECTATOR, spectator_list);
}

/// The renderer `game` shows its new settings on: the play screen, or the
/// single screen when a replay is shown on it.
fn primary_renderer() -> *mut LieroRenderer {
    crate::app::liero_app_primary(app())
}

/// The name a replay is recorded under: the time, then up to four letters
/// and digits of each of the first two worms' names.
unsafe fn replay_name(game: &mut LieroGame) -> Vec<u8> {
    let host = current_host();
    let mut name = host.replay_time(host.time());
    name.push(b' ');
    for i in 0..2 {
        let w = game.worm_at(i).expect("an empty place among the game's worms");
        if i > 0 {
            name.push(b'-');
        }
        name.extend(w.settings().name.iter().take(4).filter(|c| c.is_ascii_alphanumeric()));
    }
    name.extend_from_slice(b".lrp");
    name
}

impl LieroController {
    fn new(kind: Kind) -> Box<Self> {
        Box::new(LieroController { data: LieroControllerData::default(), kind, opened: None })
    }

    /// Opens `s` over the game, in place of anything opened before.
    fn open(&mut self, s: LieroScreen) {
        if let Some(old) = self.opened.replace(s) {
            // SAFETY: the screen was never pushed, so it's still ours.
            unsafe { free_screen(old) };
        }
    }

    /// A message over the game (`Gfx::infoBox`).
    fn info_box(&mut self, text: &str) {
        // SAFETY: the text outlives the call, which copies it.
        let s = unsafe { liero_info_box_screen(LieroStr::of(text.as_bytes()), 320 / 2, 200 / 2, true) };
        self.open(s);
    }

    fn game(&mut self) -> Option<&mut LieroGame> {
        match &mut self.kind {
            Kind::Local(l) => Some(&mut l.game),
            Kind::Replay(r) => r.game.as_deref_mut(),
        }
    }

    /// Ends the game's stats and opens the stats screen (`presentStats`).
    unsafe fn present_stats(&mut self) {
        let game = self.game().expect("no game");
        game.stats_finish();
        let names: Vec<LieroStr>;
        let scene;
        {
            let tc = &*game.tc_data();
            names = (0..tc.weapon_count).map(|i| (*tc.weapons.add(i)).name).collect();
            scene = stats_scene(game, &app().play.data, &names);
        }
        let s = liero_stats_presenter_screen(&*game.stats(), &scene);
        self.open(s);
    }

    unsafe fn on_key(&mut self, key: i32, state: bool) {
        let escape = match &mut self.kind {
            Kind::Local(l) => {
                let keys: Vec<LieroWormKeys> = l
                    .game
                    .listed_worms()
                    .map(|w| {
                        let w = w.expect("an empty place among the game's worms");
                        let controls = w.settings().data.controls_ex.as_ptr();
                        LieroWormKeys { controls, states: &mut w.data.control_states, clean: &mut w.data.clean_control_states }
                    })
                    .collect();
                liero_local_controller_key(keys.as_ptr(), keys.len(), LIERO_MAX_CONTROL_EX, key, state)
            }
            Kind::Replay(_) => key as u32 == LIERO_DK_ESCAPE,
        };
        if escape && !self.data.going_to_menu {
            self.data.fade_value = 31;
            self.data.going_to_menu = true;
        }
    }

    unsafe fn unfocus(&mut self) {
        if let Kind::Local(l) = &mut self.kind {
            if let Some(w) = &mut l.replay {
                liero_replay_writer_unfocus(w);
            }
            if self.data.state == LIERO_STATE_WEAPON_SELECTION {
                liero_weapsel_set_focused(l.ws.as_mut().expect("no weapon selection"), false);
            }
        }
    }

    unsafe fn focus(&mut self) {
        let d = &mut self.data;
        if d.state == LIERO_STATE_GAME_ENDED {
            d.going_to_menu = true;
            d.fade_value = 0;
            return;
        }
        match &mut self.kind {
            Kind::Local(l) => {
                if d.state == LIERO_STATE_WEAPON_SELECTION {
                    liero_weapsel_set_focused(l.ws.as_mut().expect("no weapon selection"), true);
                }
                if let Some(w) = &mut l.replay {
                    liero_replay_writer_focus(w);
                }
                if d.state == LIERO_STATE_INITIAL {
                    self.change_state(LIERO_STATE_WEAPON_SELECTION);
                }
            }
            Kind::Replay(r) => {
                if d.state == LIERO_STATE_INITIAL {
                    let settings = liero_settings_box_new();
                    let mut game = new_game(settings, r.tc);
                    liero_settings_box_free(settings);
                    let reader = r.reader.as_mut().expect("no replay");
                    let o = liero_replay_reader_begin(reader, &mut game);
                    if let Some(e) = caught(&o, true) {
                        drop(game);
                        self.info_box(&format!("Error starting replay playback: {e}"));
                        self.data.going_to_menu = true;
                        self.data.fade_value = 0;
                        return;
                    }
                    r.game = Some(game);
                    // Changing state first when game is available
                    self.change_state(LIERO_STATE_GAME);
                }
            }
        }
        let a = app();
        let game = self.game().expect("no game");
        game.focus(&mut *a.play);
        game.focus(&mut *a.single);
        self.data.going_to_menu = false;
        self.data.fade_value = 0;
    }

    unsafe fn process(&mut self) -> bool {
        match &mut self.kind {
            Kind::Local(_) => self.process_local(),
            Kind::Replay(_) => self.process_replay(),
        }
    }

    unsafe fn process_local(&mut self) -> bool {
        let Kind::Local(l) = &mut self.kind else { unreachable!() };
        if self.data.state == LIERO_STATE_WEAPON_SELECTION {
            if liero_weapsel_process_game(l.ws.as_mut().expect("no weapon selection")) {
                self.change_state(LIERO_STATE_GAME);
            }
        } else if self.data.in_game() {
            let ticks = self.data.ticks();
            let mut i = 0;
            while i < ticks && self.data.in_game() {
                let Kind::Local(l) = &mut self.kind else { unreachable!() };
                l.game.process_ais(Some(&game_hooks()));
                if let Some(w) = &mut l.replay {
                    liero_replay_writer_frame(w);
                }
                l.game.process_frame(&game_hooks());
                if l.game.is_over() {
                    self.change_state(LIERO_STATE_GAME_ENDED);
                }
                i += 1;
            }
        }

        // The local controller doesn't run CommonController::process.
        if self.data.going_to_menu {
            if self.data.fade_value > 0 {
                self.data.fade_value -= 1;
            } else {
                if self.data.state == LIERO_STATE_GAME_ENDED {
                    self.end_record();
                    self.present_stats();
                }
                return false;
            }
        } else if self.data.fade_value < LIERO_FADE_SHOWN {
            self.data.fade_value += 1;
        }
        true
    }

    unsafe fn process_replay(&mut self) -> bool {
        if self.data.in_game() {
            let Kind::Replay(r) = &mut self.kind else { unreachable!() };
            let game = r.game.as_mut().expect("no game");
            if crate::keys::liero_keys_test_sdl_once(&mut app().keys, SCANCODE_R) {
                crate::game_box::liero_game_assign(game, r.initial.as_ref().expect("no start to rewind to"));
                crate::replay_file::liero_replay_reader_set_pos(r.reader.as_mut().expect("no replay"), r.initial_pos);
            }

            let ticks = self.data.ticks();
            let mut i = 0;
            while i < ticks && self.data.in_game() {
                let Kind::Replay(r) = &mut self.kind else { unreachable!() };
                if let Some(reader) = &mut r.reader {
                    let o = liero_replay_reader_frame(reader);
                    if o.end {
                        // End of replay
                        r.reader = None;
                    } else {
                        if o.settings_changed {
                            r.game.as_mut().expect("no game").focus(primary_renderer());
                        }
                        if let Some(e) = caught(&o, false) {
                            self.info_box(&format!("Archive error in replay: {e}"));
                            self.change_state(LIERO_STATE_GAME_ENDED);
                            let Kind::Replay(r) = &mut self.kind else { unreachable!() };
                            r.reader = None;
                        }
                    }
                }
                let Kind::Replay(r) = &mut self.kind else { unreachable!() };
                r.game.as_mut().expect("no game").process_frame(&game_hooks());

                let d = &mut self.data;
                if d.going_to_menu {
                    if d.fade_value > 0 {
                        d.fade_value -= 1;
                    } else {
                        break;
                    }
                } else if d.fade_value < LIERO_FADE_SHOWN {
                    d.fade_value += 1;
                }
                i += 1;
            }

            if self.game().expect("no game").is_over() {
                self.change_state(LIERO_STATE_GAME_ENDED);
            }
        }

        // CommonController::process: the frame skip keys, and the frame
        // counted.
        crate::controller::liero_controller_process(&mut self.data, &mut app().keys);

        if self.data.going_to_menu && self.data.fade_value <= 0 {
            if self.data.state == LIERO_STATE_GAME_ENDED {
                self.present_stats();
            }
            return false;
        }

        let Kind::Replay(r) = &self.kind else { unreachable!() };
        if r.reader.is_none() && self.data.state == LIERO_STATE_GAME {
            self.present_stats();
            return false;
        }
        true
    }

    unsafe fn draw(&mut self, renderer: *mut LieroRenderer, spectators: bool) {
        let state = self.data.state;
        match &mut self.kind {
            Kind::Local(l) => {
                if state == LIERO_STATE_WEAPON_SELECTION {
                    liero_weapsel_draw_game(l.ws.as_mut().expect("no weapon selection"), renderer, state, spectators);
                } else {
                    l.game.draw(renderer, state, spectators, false);
                }
            }
            Kind::Replay(r) => {
                if state == LIERO_STATE_GAME || state == LIERO_STATE_GAME_ENDED {
                    r.game.as_mut().expect("no game").draw(renderer, state, spectators, true);
                }
            }
        }
        (*renderer).data.fade_value = self.data.fade_value;
    }

    unsafe fn change_state(&mut self, new_state: i32) {
        if self.data.state == new_state {
            return;
        }
        match &mut self.kind {
            Kind::Local(_) => self.change_local_state(new_state),
            Kind::Replay(_) => self.change_replay_state(new_state),
        }
    }

    unsafe fn change_local_state(&mut self, new_state: i32) {
        let Kind::Local(l) = &mut self.kind else { unreachable!() };
        // The new state is prepared before the old one goes: weapon
        // selection is dropped only once recording has started.
        if self.data.state == LIERO_STATE_WEAPON_SELECTION {
            liero_weapsel_finish(l.ws.as_mut().expect("no weapon selection"));
        }

        if new_state == LIERO_STATE_WEAPON_SELECTION {
            l.ws = Some(Box::from_raw(liero_weapsel_start(&mut l.game)));
        } else if new_state == LIERO_STATE_GAME {
            // Before the replay recording starts below.
            let lives = l.game.settings_box().data.lives;
            for w in l.game.listed_worms() {
                w.expect("an empty place among the game's worms").data.lives = lives;
            }

            let s = &l.game.settings_box().data;
            if s.record_replays {
                if let Err(e) = start_recording(l) {
                    self.info_box(&format!("Error starting replay recording: {e}"));
                    self.data.going_to_menu = true;
                    self.data.fade_value = 0;
                    return;
                }
            }

            let blood = l.game.settings_box().data.blood_particle_max;
            l.game.start(blood);
        } else if new_state == LIERO_STATE_GAME_ENDED && !self.data.going_to_menu {
            self.data.fade_value = 180;
            self.data.going_to_menu = true;
        }

        if self.data.state == LIERO_STATE_WEAPON_SELECTION {
            self.data.fade_value = LIERO_FADE_SHOWN;
            let Kind::Local(l) = &mut self.kind else { unreachable!() };
            l.ws = None;
        }
        self.data.state = new_state;
    }

    unsafe fn change_replay_state(&mut self, new_state: i32) {
        let Kind::Replay(r) = &mut self.kind else { unreachable!() };
        if new_state == LIERO_STATE_GAME {
            let game = r.game.as_mut().expect("no game");
            // The replay's viewports are replaced: they aren't what this
            // screen shows.
            crate::game_box::liero_game_clear_viewports(game);
            // Not stored in replays.
            for (i, x) in [(0, 0), (1, 218)] {
                game.worm_at(i).expect("an empty place among the replay's worms").data.stats_x = x;
            }
            spectator_viewport(game, true);
            if app().menu_settings().data.single_screen_replay {
                // The spectator viewport on the main screen as well.
                spectator_viewport(game, false);
            } else {
                let w0 = game.worm_at(0).expect("no first worm").data.index;
                let w1 = game.worm_at(1).expect("no second worm").data.index;
                split_viewports(game, w0, w1);
            }
            let blood = game.settings_box().data.blood_particle_max;
            game.start(blood);
            r.initial = Some(Box::from_raw(liero_game_clone(game)));
            r.initial_pos = crate::replay_file::liero_replay_reader_pos(r.reader.as_ref().expect("no replay"));
        } else if new_state == LIERO_STATE_GAME_ENDED && !self.data.going_to_menu {
            self.data.fade_value = 180;
            self.data.going_to_menu = true;
        }
        self.data.state = new_state;
    }

    fn end_record(&mut self) {
        if let Kind::Local(l) = &mut self.kind {
            // Dropping the writer ends the file.
            l.replay = None;
        }
    }

    fn running(&self) -> bool {
        self.data.state != LIERO_STATE_GAME_ENDED && self.data.state != LIERO_STATE_INITIAL
    }

    /// The level shown behind the menu, if any.
    fn level(&mut self) -> Option<&mut LieroLevelBox> {
        match &mut self.kind {
            Kind::Local(l) => Some(l.game.level_box_mut()),
            Kind::Replay(r) => match (&mut r.game, &r.reader) {
                (Some(g), Some(_)) => Some(g.level_box_mut()),
                _ => None,
            },
        }
    }
}

impl Drop for LieroController {
    fn drop(&mut self) {
        self.end_record();
        if let Some(s) = self.opened.take() {
            // SAFETY: the screen was never pushed.
            unsafe { free_screen(s) };
        }
    }
}

unsafe fn free_screen(s: LieroScreen) {
    if let Some(free) = s.free {
        free(s.data);
    }
}

/// The error a replay outcome is, as the controllers catch it: a check
/// error, and with `read_too` a read error. Others stop the app, as the
/// exceptions C++ didn't catch did.
unsafe fn caught(o: &LieroReplayOutcome, read_too: bool) -> Option<String> {
    let message = String::from_utf8_lossy(o.message.bytes()).into_owned();
    match o.status {
        LieroReplayStatus::Ok => None,
        LieroReplayStatus::CheckError => Some(message),
        LieroReplayStatus::ReadError if read_too => Some(message),
        LieroReplayStatus::ReadError => panic!("{message}"),
        LieroReplayStatus::OutOfRange => panic!("vector"),
    }
}

/// Starts recording a replay of `l`'s game in the config folder's Replays.
unsafe fn start_recording(l: &mut Local) -> Result<(), String> {
    let name = replay_name(&mut l.game);
    let node = crate::app::app().files.config.go(b"Replays").go(&name);
    let w = liero_replay_writer_new(&LieroFsNode(node.clone()));
    if w.is_null() {
        return Err(format!("Could not write {}", String::from_utf8_lossy(node.full_path())));
    }
    let mut w = Box::from_raw(w);
    let o = liero_replay_writer_begin(&mut w, &mut l.game);
    // The writer is kept even when beginning fails, as C++ kept it.
    l.replay = Some(w);
    match caught(&o, true) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// What the stats screen draws with (`statsScene`).
///
/// # Safety
/// The game has its TC; `names` are the TC's weapons' names.
pub(crate) unsafe fn stats_scene(
    game: &mut LieroGame,
    r: &crate::renderer::LieroRendererData,
    names: &[LieroStr],
) -> LieroStatsScene {
    let tc = &*game.tc_data();
    let sprites = &tc.worm_sprites;
    let size = sprites.width * sprites.height;
    // `wormSpriteObj(2, i == 0 ? 1 : 0, i)`.
    let sprite = |i: i32| {
        let frame = 2 + (if i == 0 { 1 } else { 0 }) * 7 * 3 + i * 2 * 7 * 3;
        assert!(frame < sprites.count);
        LieroSprite { mem: sprites.data.add((frame * size) as usize), width: sprites.width, height: sprites.height, pitch: sprites.width }
    };
    let name = |g: &mut LieroGame, i| {
        let n = &g.worm_at(i).expect("an empty place among the game's worms").settings().name;
        LieroStr::of(n)
    };
    let mode = game.settings_box().data.game_mode as i32;
    LieroStatsScene {
        font: tc_font(tc),
        worm_sprites: [sprite(0), sprite(1)],
        names: [name(game, 0), name(game, 1)],
        game_mode: mode,
        game_mode_name: LieroStr::of(GAME_MODE_NAMES[mode as usize].as_bytes()),
        weapon_names: names.as_ptr(),
        weapon_count: names.len(),
        render_res_x: r.render_res_x,
        render_res_y: r.render_res_y,
        ticks_per_sec: 1_000_000_000,
    }
}

/// A local match with the settings' two players (`LocalController`).
///
/// # Safety
/// `settings` and `tc` have references.
#[no_mangle]
pub unsafe extern "C" fn liero_local_controller_new(settings: *mut LieroSettingsBox, tc: *mut LieroTc) -> *mut LieroController {
    let mut game = new_game(settings, tc);
    let s = &*settings;
    let mut indexes = [0; 2];
    for (i, ws) in s.worms.iter().enumerate() {
        let w = &mut *liero_game_new_worm(&mut game, ws.as_ptr());
        liero_game_list_worm(&mut game, w);
        w.data.health = ws.data.health;
        w.data.index = i as i32;
        w.data.stats_x = if i == 0 { 0 } else { 218 };
        liero_worm_set_ai(w, ws.data.controller, s.data.ai_parallels.max(0) as usize, i == 0);
        indexes[i] = w.data.index;
    }
    split_viewports(&mut game, indexes[0], indexes[1]);
    spectator_viewport(&mut game, true);
    Box::into_raw(LieroController::new(Kind::Local(Local { game, ws: None, replay: None })))
}

/// A replay of `node`, played with `tc` (`ReplayController`), or null if
/// it can't be read.
///
/// # Safety
/// `tc` has a reference, which the controller keeps while it lives.
#[no_mangle]
pub unsafe extern "C" fn liero_replay_controller_new(node: &LieroFsNode, tc: *mut LieroTc) -> *mut LieroController {
    let reader = liero_replay_reader_new(node);
    if reader.is_null() {
        return std::ptr::null_mut();
    }
    let tc = crate::shared::retain_shared(tc);
    let r = Replay { reader: Some(Box::from_raw(reader)), tc, game: None, initial: None, initial_pos: 0 };
    Box::into_raw(LieroController::new(Kind::Replay(r)))
}

impl Drop for Replay {
    fn drop(&mut self) {
        // The games go before the TC they use.
        self.game = None;
        self.initial = None;
        // SAFETY: the reference taken in `liero_replay_controller_new`.
        unsafe { crate::tc::liero_tc_free(self.tc) };
    }
}

/// # Safety
/// `c` comes from a `_new` here, or is null, and isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_free(c: *mut LieroController) {
    if !c.is_null() {
        drop(Box::from_raw(c));
    }
}

/// What the controllers share, at a fixed address.
#[no_mangle]
pub extern "C" fn liero_controller_data(c: &mut LieroController) -> *mut LieroControllerData {
    &mut c.data
}

/// A key went down or up: the worms' controls, or Escape to the menu.
///
/// # Safety
/// `c` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_on_key(c: &mut LieroController, key: i32, state: bool) {
    c.on_key(key, state);
}

/// The menu opened.
///
/// # Safety
/// `c` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_unfocus(c: &mut LieroController) {
    c.unfocus();
}

/// The menu closed to the game.
///
/// # Safety
/// `c` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_focus(c: &mut LieroController) {
    c.focus();
}

/// One frame: false once the controller is done and back to the menu.
///
/// # Safety
/// `c` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_run(c: &mut LieroController) -> bool {
    c.process()
}

/// Draws the game on `r`, the split screen or with `spectators` the
/// spectator one.
///
/// # Safety
/// `c` and `r` are valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_draw(c: &mut LieroController, r: *mut LieroRenderer, spectators: bool) {
    c.draw(r, spectators);
}

/// Whether a game is on, for the menu to offer to resume it.
#[no_mangle]
pub extern "C" fn liero_controller_running(c: &LieroController) -> bool {
    c.running()
}

#[no_mangle]
pub extern "C" fn liero_controller_is_replay(c: &LieroController) -> bool {
    matches!(c.kind, Kind::Replay(_))
}

/// The level behind the menu, or null.
#[no_mangle]
pub extern "C" fn liero_controller_level(c: &mut LieroController) -> *mut LieroLevelBox {
    c.level().map_or(std::ptr::null_mut(), |l| l as *mut LieroLevelBox)
}

/// The game, or null before a replay starts.
#[no_mangle]
pub extern "C" fn liero_controller_game(c: &mut LieroController) -> *mut LieroGame {
    c.game().map_or(std::ptr::null_mut(), |g| g as *mut LieroGame)
}

/// Moves the screen the controller opened into `out`; false if none.
///
/// # Safety
/// `out` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_controller_take_opened(c: &mut LieroController, out: *mut LieroScreen) -> bool {
    match c.opened.take() {
        Some(s) => {
            *out = s;
            true
        }
        None => false,
    }
}

/// What the stats screen draws `g`'s game with, on `r`; `names` are the
/// TC's weapons' names, which outlive the scene.
///
/// # Safety
/// The game has its TC and its first two worms; `names` points to `count`.
#[no_mangle]
pub unsafe extern "C" fn liero_game_stats_scene(
    g: &mut LieroGame,
    r: &LieroRenderer,
    names: *const LieroStr,
    count: usize,
) -> LieroStatsScene {
    let names = if count == 0 { &[][..] } else { std::slice::from_raw_parts(names, count) };
    stats_scene(g, &r.data, names)
}

/// `Game::focus`: `r`'s palette from the level's and the worms' colours.
///
/// # Safety
/// `r` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_game_focus(g: &mut LieroGame, r: *mut LieroRenderer) {
    g.focus(r);
}

#[cfg(test)]
#[path = "controllers_tests.rs"]
mod tests;
