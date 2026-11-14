//! The game's controllers (`LocalController` and `ReplayController` in
//! C++): a match played here, from weapon selection to the stats screen, or
//! a replay played back. They own their games, and run, draw and fade them.
//! What they open on top of the game (a message, the stats screen) waits in
//! the controller for the game screen to push.

use crate::app::App;
use crate::boxes::liero_info_box_screen;
use crate::controller::{
    ControllerData, LIERO_FADE_SHOWN, LIERO_STATE_GAME, LIERO_STATE_GAME_ENDED, LIERO_STATE_INITIAL,
    LIERO_STATE_WEAPON_SELECTION,
};
use crate::fs::Node;
use crate::game::{GameHooks, Rect};
use crate::game_box::{
    liero_game_add_viewport, liero_game_clone, liero_game_list_worm, liero_game_new, liero_game_new_worm,
    liero_game_set_context, liero_worm_set_ai, Game,
};
use crate::keys::{local_controller_key, WormKeys, LIERO_DK_ESCAPE};
use crate::level::LevelBox;
use crate::renderer::Renderer;
use crate::replay::ReplayStatus;
use crate::replay_file::{ReplayOutcome, ReplayReader, ReplayWriter};
use crate::screen::{BoxedScreen, Host};
use crate::settings::{liero_settings_box_new, SharedSettings, LIERO_MAX_CONTROL_EX};
use crate::stats::liero_stats_presenter_screen;
use crate::tc::SharedTc;
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use crate::weapsel::{
    liero_weapsel_draw_game, liero_weapsel_finish, liero_weapsel_process_game, liero_weapsel_set_focused,
    liero_weapsel_start, Weapsel,
};

/// `SDL_SCANCODE_R`, which rewinds a replay.
const SCANCODE_R: i32 = 21;

/// A match played here.
struct Local {
    game: Box<Game>,
    ws: Option<Box<Weapsel>>,
    replay: Option<ReplayWriter>,
}

/// A replay played back.
struct Replay {
    reader: Option<ReplayReader>,
    /// The TC the game is made with.
    tc: SharedTc,
    game: Option<Box<Game>>,
    /// The game as it started, which R rewinds to, and where in the replay
    /// that was.
    initial: Option<Box<Game>>,
    initial_pos: usize,
}

enum Kind {
    Local(Local),
    Replay(Replay),
}

/// A controller (`Controller`).
pub struct Controller {
    /// What both kinds share (`CommonController`), which C++ refers to.
    data: ControllerData,
    kind: Kind,
    /// A screen opened over the game, for the game screen to push.
    opened: Option<BoxedScreen>,
}

/// A new game with the settings, the TC and the app's sound, its clock
/// seeded from the time, as C++'s `Game` constructor did.
unsafe fn new_game(host: &Host, settings: SharedSettings, tc: SharedTc) -> Box<Game> {
    unsafe {
        let mut game = Box::from_raw(liero_game_new());
        liero_game_set_context(&mut game, Some(settings), Some(tc), host.sound());
        game.data_mut().rand.seed(host.time() as u32);
        game
    }
}

unsafe fn split_viewports(game: &mut Game, worm0: i32, worm1: i32) {
    let r = |x1, y1, x2, y2| Rect { x1, y1, x2, y2 };
    liero_game_add_viewport(game, r(0, 0, 158, 158), worm0, 504, 350, LIERO_VIEWPORT_WORM, false);
    liero_game_add_viewport(
        game,
        r(160, 0, 158 + 160, 158),
        worm1,
        504,
        350,
        LIERO_VIEWPORT_WORM,
        false,
    );
}

/// The spectator viewport is always full size, +68 on x to align it in the
/// middle.
unsafe fn spectator_viewport(game: &mut Game, spectator_list: bool) {
    let r = Rect {
        x1: 0,
        y1: 0,
        x2: 504 + 68,
        y2: 350,
    };
    liero_game_add_viewport(game, r, 0, 504, 350, LIERO_VIEWPORT_SPECTATOR, spectator_list);
}

/// The renderer `game` shows its new settings on: the play screen, or the
/// single screen when a replay is shown on it.
fn primary_renderer(app: &mut App) -> *mut Renderer {
    crate::app::liero_app_primary(app)
}

/// The name a replay is recorded under: the time, then up to four letters
/// and digits of each of the first two worms' names.
unsafe fn replay_name(host: &Host, game: &mut Game) -> Vec<u8> {
    let mut name = host.replay_time(host.time());
    name.push(b' ');
    for i in 0..2 {
        let w = game.worm_at(i).expect("an empty place among the game's worms");
        if i > 0 {
            name.push(b'-');
        }
        name.extend(
            w.settings()
                .data
                .name
                .iter()
                .take(4)
                .filter(|c| c.is_ascii_alphanumeric()),
        );
    }
    name.extend_from_slice(b".lrp");
    name
}

impl Controller {
    fn new(kind: Kind) -> Box<Self> {
        Box::new(Controller {
            data: ControllerData::default(),
            kind,
            opened: None,
        })
    }

    /// Opens `s` over the game, in place of anything opened before.
    fn open(&mut self, s: BoxedScreen) {
        self.opened = Some(s);
    }

    /// A message over the game (`Gfx::infoBox`).
    fn info_box(&mut self, text: &str) {
        // SAFETY: the text outlives the call, which copies it.
        let s = liero_info_box_screen(text.as_bytes(), 320 / 2, 200 / 2, true);
        self.open(s);
    }

    fn game(&mut self) -> Option<&mut Game> {
        match &mut self.kind {
            Kind::Local(l) => Some(&mut l.game),
            Kind::Replay(r) => r.game.as_deref_mut(),
        }
    }

    /// Ends the game's stats and opens the stats screen (`presentStats`).
    fn present_stats(&mut self, app: &mut App, host: &Host) {
        let game = self.game().expect("no game");
        game.stats_finish();
        let g = game.stats_game();
        let (x, y) = (app.play.data.render_res_x, app.play.data.render_res_y);
        let s = liero_stats_presenter_screen(app, host, game.stats(), game.tc(), g, x, y);
        self.open(s);
    }

    unsafe fn on_key(&mut self, key: i32, state: bool) {
        unsafe {
            let escape = match &mut self.kind {
                Kind::Local(l) => {
                    let keys: Vec<WormKeys> = l
                        .game
                        .listed_worms()
                        .map(|w| {
                            let w = w.expect("an empty place among the game's worms");
                            let controls = w.settings().data.controls_ex.as_ptr();
                            WormKeys {
                                controls,
                                states: &mut w.data.control_states,
                                clean: &mut w.data.clean_control_states,
                            }
                        })
                        .collect();
                    local_controller_key(&keys, LIERO_MAX_CONTROL_EX, key, state)
                }
                Kind::Replay(_) => key as u32 == LIERO_DK_ESCAPE,
            };
            if escape && !self.data.going_to_menu {
                self.data.fade_value = 31;
                self.data.going_to_menu = true;
            }
        }
    }

    fn unfocus(&mut self) {
        if let Kind::Local(l) = &mut self.kind {
            if let Some(w) = &mut l.replay {
                w.unfocus(&mut l.game);
            }
            if self.data.state == LIERO_STATE_WEAPON_SELECTION {
                liero_weapsel_set_focused(l.ws.as_mut().expect("no weapon selection"), false);
            }
        }
    }

    unsafe fn focus(&mut self, app: &mut App, host: &Host) {
        unsafe {
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
                        w.focus(&mut l.game);
                    }
                    if d.state == LIERO_STATE_INITIAL {
                        self.change_state(app, host, LIERO_STATE_WEAPON_SELECTION);
                    }
                }
                Kind::Replay(r) => {
                    if d.state == LIERO_STATE_INITIAL {
                        let mut game = new_game(host, liero_settings_box_new(), r.tc.clone());
                        let reader = r.reader.as_mut().expect("no replay");
                        let o = reader.begin(&mut game);
                        if let Some(e) = caught(&o, true) {
                            drop(game);
                            self.info_box(&format!("Error starting replay playback: {e}"));
                            self.data.going_to_menu = true;
                            self.data.fade_value = 0;
                            return;
                        }
                        r.game = Some(game);
                        // Changing state first when game is available
                        self.change_state(app, host, LIERO_STATE_GAME);
                    }
                }
            }
            let game = self.game().expect("no game");
            game.focus(&mut *app.play);
            game.focus(&mut *app.single);
            self.data.going_to_menu = false;
            self.data.fade_value = 0;
        }
    }

    unsafe fn process(&mut self, app: &mut App, host: &Host) -> bool {
        unsafe {
            match &mut self.kind {
                Kind::Local(_) => self.process_local(app, host),
                Kind::Replay(_) => self.process_replay(app, host),
            }
        }
    }

    unsafe fn process_local(&mut self, app: &mut App, host: &Host) -> bool {
        unsafe {
            let Kind::Local(l) = &mut self.kind else { unreachable!() };
            if self.data.state == LIERO_STATE_WEAPON_SELECTION {
                if liero_weapsel_process_game(l.ws.as_mut().expect("no weapon selection"), &mut app.rand) {
                    self.change_state(app, host, LIERO_STATE_GAME);
                }
            } else if self.data.in_game() {
                let ticks = self.data.ticks();
                let mut i = 0;
                while i < ticks && self.data.in_game() {
                    let Kind::Local(l) = &mut self.kind else { unreachable!() };
                    l.game.process_ais(Some(&GameHooks {
                        hires_ticks: &|| host.hires_ticks(),
                    }));
                    if let Some(w) = &mut l.replay {
                        w.frame(&mut l.game);
                    }
                    l.game.process_frame(&GameHooks {
                        hires_ticks: &|| host.hires_ticks(),
                    });
                    if l.game.is_over() {
                        self.change_state(app, host, LIERO_STATE_GAME_ENDED);
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
                        self.present_stats(app, host);
                    }
                    return false;
                }
            } else if self.data.fade_value < LIERO_FADE_SHOWN {
                self.data.fade_value += 1;
            }
            true
        }
    }

    unsafe fn process_replay(&mut self, app: &mut App, host: &Host) -> bool {
        unsafe {
            if self.data.in_game() {
                let Kind::Replay(r) = &mut self.kind else {
                    unreachable!()
                };
                let game = r.game.as_mut().expect("no game");
                if app.keys.test_sdl_once(SCANCODE_R) {
                    crate::game_box::liero_game_assign(game, r.initial.as_ref().expect("no start to rewind to"));
                    r.reader.as_mut().expect("no replay").set_pos(r.initial_pos);
                }

                let ticks = self.data.ticks();
                let mut i = 0;
                while i < ticks && self.data.in_game() {
                    let Kind::Replay(r) = &mut self.kind else {
                        unreachable!()
                    };
                    if let Some(reader) = &mut r.reader {
                        let o = reader.frame(r.game.as_mut().expect("no game"));
                        if o.end {
                            // End of replay
                            r.reader = None;
                        } else {
                            if o.settings_changed {
                                r.game.as_mut().expect("no game").focus(primary_renderer(app));
                            }
                            if let Some(e) = caught(&o, false) {
                                self.info_box(&format!("Archive error in replay: {e}"));
                                self.change_state(app, host, LIERO_STATE_GAME_ENDED);
                                let Kind::Replay(r) = &mut self.kind else {
                                    unreachable!()
                                };
                                r.reader = None;
                            }
                        }
                    }
                    let Kind::Replay(r) = &mut self.kind else {
                        unreachable!()
                    };
                    r.game.as_mut().expect("no game").process_frame(&GameHooks {
                        hires_ticks: &|| host.hires_ticks(),
                    });

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
                    self.change_state(app, host, LIERO_STATE_GAME_ENDED);
                }
            }

            // CommonController::process: the frame skip keys, and the frame
            // counted.
            crate::controller::liero_controller_process(&mut self.data, &mut app.keys);

            if self.data.going_to_menu && self.data.fade_value <= 0 {
                if self.data.state == LIERO_STATE_GAME_ENDED {
                    self.present_stats(app, host);
                }
                return false;
            }

            let Kind::Replay(r) = &self.kind else { unreachable!() };
            if r.reader.is_none() && self.data.state == LIERO_STATE_GAME {
                self.present_stats(app, host);
                return false;
            }
            true
        }
    }

    unsafe fn draw(&mut self, app: &mut App, renderer: *mut Renderer, spectators: bool) {
        unsafe {
            let state = self.data.state;
            match &mut self.kind {
                Kind::Local(l) => {
                    if state == LIERO_STATE_WEAPON_SELECTION {
                        liero_weapsel_draw_game(
                            l.ws.as_mut().expect("no weapon selection"),
                            app,
                            renderer,
                            state,
                            spectators,
                        );
                    } else {
                        l.game.draw(&mut *renderer, state, spectators, false);
                    }
                }
                Kind::Replay(r) => {
                    if state == LIERO_STATE_GAME || state == LIERO_STATE_GAME_ENDED {
                        r.game
                            .as_mut()
                            .expect("no game")
                            .draw(&mut *renderer, state, spectators, true);
                    }
                }
            }
            (*renderer).data.fade_value = self.data.fade_value;
        }
    }

    unsafe fn change_state(&mut self, app: &mut App, host: &Host, new_state: i32) {
        unsafe {
            if self.data.state == new_state {
                return;
            }
            match &mut self.kind {
                Kind::Local(_) => self.change_local_state(app, host, new_state),
                Kind::Replay(_) => self.change_replay_state(app, new_state),
            }
        }
    }

    unsafe fn change_local_state(&mut self, app: &mut App, host: &Host, new_state: i32) {
        unsafe {
            let Kind::Local(l) = &mut self.kind else { unreachable!() };
            // The new state is prepared before the old one goes: weapon
            // selection is dropped only once recording has started.
            if self.data.state == LIERO_STATE_WEAPON_SELECTION {
                liero_weapsel_finish(l.ws.as_mut().expect("no weapon selection"));
            }

            if new_state == LIERO_STATE_WEAPON_SELECTION {
                l.ws = Some(Box::from_raw(liero_weapsel_start(&mut l.game, &mut app.rand)));
            } else if new_state == LIERO_STATE_GAME {
                // Before the replay recording starts below.
                let lives = l.game.settings_box().data.lives;
                for w in l.game.listed_worms() {
                    w.expect("an empty place among the game's worms").data.lives = lives;
                }

                let record = l.game.settings_box().data.ext.record_replays;
                if record {
                    if let Err(e) = start_recording(app, host, l) {
                        self.info_box(&format!("Error starting replay recording: {e}"));
                        self.data.going_to_menu = true;
                        self.data.fade_value = 0;
                        return;
                    }
                }

                let blood = l.game.settings_box().data.ext.blood_particle_max;
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
    }

    unsafe fn change_replay_state(&mut self, app: &mut App, new_state: i32) {
        unsafe {
            let Kind::Replay(r) = &mut self.kind else {
                unreachable!()
            };
            if new_state == LIERO_STATE_GAME {
                let game = r.game.as_mut().expect("no game");
                // The replay's viewports are replaced: they aren't what this
                // screen shows.
                crate::game_box::liero_game_clear_viewports(game);
                // Not stored in replays.
                for (i, x) in [(0, 0), (1, 218)] {
                    game.worm_at(i)
                        .expect("an empty place among the replay's worms")
                        .data
                        .stats_x = x;
                }
                spectator_viewport(game, true);
                if app.settings_mut().data.ext.single_screen_replay {
                    // The spectator viewport on the main screen as well.
                    spectator_viewport(game, false);
                } else {
                    let w0 = game.worm_at(0).expect("no first worm").data.index;
                    let w1 = game.worm_at(1).expect("no second worm").data.index;
                    split_viewports(game, w0, w1);
                }
                let blood = game.settings_box().data.ext.blood_particle_max;
                game.start(blood);
                r.initial = Some(Box::from_raw(liero_game_clone(game)));
                r.initial_pos = r.reader.as_ref().expect("no replay").pos();
            } else if new_state == LIERO_STATE_GAME_ENDED && !self.data.going_to_menu {
                self.data.fade_value = 180;
                self.data.going_to_menu = true;
            }
            self.data.state = new_state;
        }
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
    fn level(&mut self) -> Option<&mut LevelBox> {
        match &mut self.kind {
            Kind::Local(l) => Some(l.game.level_box_mut()),
            Kind::Replay(r) => match (&mut r.game, &r.reader) {
                (Some(g), Some(_)) => Some(g.level_box_mut()),
                _ => None,
            },
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.end_record();
    }
}

/// The error a replay outcome is, as the controllers catch it: a check
/// error, and with `read_too` a read error. Others stop the app, as the
/// exceptions C++ didn't catch did.
fn caught(o: &ReplayOutcome, read_too: bool) -> Option<String> {
    let message = o.message.to_string();
    match o.status {
        ReplayStatus::Ok => None,
        ReplayStatus::CheckError => Some(message),
        ReplayStatus::ReadError if read_too => Some(message),
        ReplayStatus::ReadError => panic!("{message}"),
        ReplayStatus::OutOfRange => panic!("vector"),
    }
}

/// Starts recording a replay of `l`'s game in the config folder's Replays.
unsafe fn start_recording(app: &App, host: &Host, l: &mut Local) -> Result<(), String> {
    unsafe {
        let name = replay_name(host, &mut l.game);
        let node = app.files.config.go(b"Replays").go(&name);
        let Some(mut w) = ReplayWriter::new(&node) else {
            return Err(format!("Could not write {}", String::from_utf8_lossy(node.full_path())));
        };
        let o = w.begin(&mut l.game);
        // The writer is kept even when beginning fails, as C++ kept it.
        l.replay = Some(w);
        match caught(&o, true) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// A local match with the settings' two players (`LocalController`), with
/// `host`'s sound and clock.
pub fn liero_local_controller_new(host: &Host, settings: SharedSettings, tc: SharedTc) -> Box<Controller> {
    // SAFETY: the game is new, and the worms are its own.
    unsafe {
        let mut game = new_game(host, settings.clone(), tc);
        let s = settings.borrow();
        let mut indexes = [0; 2];
        for (i, ws) in s.data.worms.iter().enumerate() {
            let id = liero_game_new_worm(&mut game, Some(ws.clone()));
            let ws = ws.borrow();
            liero_game_list_worm(&mut game, Some(id));
            let w = game.worm_mut(id);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
            liero_worm_set_ai(w, ws.data.controller, s.data.ext.ai_parallels.max(0) as usize, i == 0);
            indexes[i] = w.data.index;
        }
        split_viewports(&mut game, indexes[0], indexes[1]);
        spectator_viewport(&mut game, true);
        Controller::new(Kind::Local(Local {
            game,
            ws: None,
            replay: None,
        }))
    }
}

/// A replay of `node`, played with `tc` (`ReplayController`), or null if
/// it can't be read.
pub fn liero_replay_controller_new(node: &Node, tc: SharedTc) -> *mut Controller {
    let Some(reader) = ReplayReader::new(node) else {
        return std::ptr::null_mut();
    };
    let r = Replay {
        reader: Some(reader),
        tc,
        game: None,
        initial: None,
        initial_pos: 0,
    };
    Box::into_raw(Controller::new(Kind::Replay(r)))
}

/// # Safety
/// `c` comes from a `_new` here, or is null, and isn't used again.
pub unsafe fn liero_controller_free(c: *mut Controller) {
    unsafe {
        if !c.is_null() {
            drop(Box::from_raw(c));
        }
    }
}

/// What the controllers share, at a fixed address.
pub fn liero_controller_data(c: &mut Controller) -> *mut ControllerData {
    &mut c.data
}

/// A key went down or up: the worms' controls, or Escape to the menu.
///
/// # Safety
/// `c` is valid.
pub unsafe fn liero_controller_on_key(c: &mut Controller, key: i32, state: bool) {
    unsafe {
        c.on_key(key, state);
    }
}

/// The menu opened.
pub fn liero_controller_unfocus(c: &mut Controller) {
    c.unfocus();
}

/// The menu closed to the game.
///
/// # Safety
/// `c` is valid.
pub unsafe fn liero_controller_focus(c: &mut Controller, app: &mut App, host: &Host) {
    unsafe {
        c.focus(app, host);
    }
}

/// One frame: false once the controller is done and back to the menu.
///
/// # Safety
/// `c` is valid.
pub unsafe fn liero_controller_run(c: &mut Controller, app: &mut App, host: &Host) -> bool {
    unsafe { c.process(app, host) }
}

/// Draws the game on `r`, the split screen or with `spectators` the
/// spectator one.
///
/// # Safety
/// `c` and `r` are valid.
pub unsafe fn liero_controller_draw(c: &mut Controller, app: &mut App, r: *mut Renderer, spectators: bool) {
    unsafe {
        c.draw(app, r, spectators);
    }
}

/// Whether a game is on, for the menu to offer to resume it.
pub fn liero_controller_running(c: &Controller) -> bool {
    c.running()
}

pub fn liero_controller_is_replay(c: &Controller) -> bool {
    matches!(c.kind, Kind::Replay(_))
}

/// The level behind the menu, or null.
pub fn liero_controller_level(c: &mut Controller) -> *mut LevelBox {
    c.level().map_or(std::ptr::null_mut(), |l| l as *mut LevelBox)
}

/// The game, or null before a replay starts.
pub fn liero_controller_game(c: &mut Controller) -> *mut Game {
    c.game().map_or(std::ptr::null_mut(), |g| g as *mut Game)
}

/// The screen the controller opened, if any, to push.
pub fn liero_controller_take_opened(c: &mut Controller) -> Option<BoxedScreen> {
    c.opened.take()
}

/// `Game::focus`: `r`'s palette from the level's and the worms' colours.
///
/// # Safety
/// `r` is valid.
pub unsafe fn liero_game_focus(g: &mut Game, r: *mut Renderer) {
    unsafe {
        g.focus(r);
    }
}

#[cfg(test)]
#[path = "controllers_tests.rs"]
mod tests;
