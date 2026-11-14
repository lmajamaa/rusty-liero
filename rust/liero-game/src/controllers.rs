//! The game's controllers (`LocalController` and `ReplayController` in
//! C++): a match played here, from weapon selection to the stats screen, or
//! a replay played back. They own their games, and run, draw and fade them.
//! What they open on top of the game (a message, the stats screen) waits in
//! the controller for the game screen to push.

use crate::app::App;
use crate::boxes::info_box_screen;
use crate::controller::{ControllerData, State, FADE_SHOWN};
use crate::fs::Node;
use crate::game::{GameHooks, Rect};
use crate::game_box::Game;
use crate::keys::{local_controller_key, WormKeys, DK_ESCAPE};
use crate::level::LevelBox;
use crate::renderer::Renderer;
use crate::replay::ReplayStatus;
use crate::replay_file::{ReplayOutcome, ReplayReader, ReplayWriter};
use crate::screen::{BoxedScreen, Host};
use crate::settings::{new_settings, SharedSettings, MAX_CONTROL_EX};
use crate::stats::stats_presenter_screen;
use crate::tc::SharedTc;
use crate::viewport::ViewportKind;
use crate::weapsel::Weapsel;

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
fn new_game(host: &Host, settings: SharedSettings, tc: SharedTc) -> Box<Game> {
    let mut game = Game::new();
    game.set_context(Some(settings), Some(tc), host.sound());
    game.data_mut().rand.seed(host.time() as u32);
    game
}

fn split_viewports(game: &mut Game, worm0: i32, worm1: i32) {
    let r = |x1, y1, x2, y2| Rect { x1, y1, x2, y2 };
    game.add_viewport(r(0, 0, 158, 158), worm0, 504, 350, ViewportKind::Worm, false);
    game.add_viewport(r(160, 0, 158 + 160, 158), worm1, 504, 350, ViewportKind::Worm, false);
}

/// The spectator viewport is always full size, +68 on x to align it in the
/// middle.
fn spectator_viewport(game: &mut Game, spectator_list: bool) {
    let r = Rect {
        x1: 0,
        y1: 0,
        x2: 504 + 68,
        y2: 350,
    };
    game.add_viewport(r, 0, 504, 350, ViewportKind::Spectator, spectator_list);
}

/// The renderer `game` shows its new settings on: the play screen, or the
/// single screen when a replay is shown on it.
fn primary_renderer(app: &mut App) -> &mut Renderer {
    app.primary()
}

/// The name a replay is recorded under: the time, then up to four letters
/// and digits of each of the first two worms' names.
fn replay_name(host: &Host, game: &mut Game) -> Vec<u8> {
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
        let s = info_box_screen(text.as_bytes(), 320 / 2, 200 / 2, true);
        self.open(s);
    }

    /// The game, or none before a replay starts.
    pub fn game(&mut self) -> Option<&mut Game> {
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
        let s = stats_presenter_screen(app, host, game.stats(), game.tc(), g, x, y);
        self.open(s);
    }

    /// A key went down or up: the worms' controls, or Escape to the menu.
    pub fn on_key(&mut self, key: i32, state: bool) {
        let escape = match &mut self.kind {
            Kind::Local(l) => {
                let mut worms: Vec<_> = l
                    .game
                    .listed_worms()
                    .map(|w| w.expect("an empty place among the game's worms").data_and_settings())
                    .collect();
                let mut keys: Vec<WormKeys> = worms
                    .iter_mut()
                    .map(|(w, s)| WormKeys {
                        controls: &s.data.controls_ex,
                        states: &mut w.control_states,
                        clean: &mut w.clean_control_states,
                    })
                    .collect();
                local_controller_key(&mut keys, MAX_CONTROL_EX, key, state)
            }
            Kind::Replay(_) => key as u32 == DK_ESCAPE,
        };
        if escape && !self.data.going_to_menu {
            self.data.fade_value = 31;
            self.data.going_to_menu = true;
        }
    }

    /// The menu opened.
    pub fn unfocus(&mut self) {
        if let Kind::Local(l) = &mut self.kind {
            if let Some(w) = &mut l.replay {
                w.unfocus(&mut l.game);
            }
            if self.data.state == State::WeaponSelection {
                l.ws.as_mut().expect("no weapon selection").set_focused(false);
            }
        }
    }

    /// The menu closed to the game.
    pub fn focus(&mut self, app: &mut App, host: &Host) {
        let d = &mut self.data;
        if d.state == State::GameEnded {
            d.going_to_menu = true;
            d.fade_value = 0;
            return;
        }
        match &mut self.kind {
            Kind::Local(l) => {
                if d.state == State::WeaponSelection {
                    l.ws.as_mut().expect("no weapon selection").set_focused(true);
                }
                if let Some(w) = &mut l.replay {
                    w.focus(&mut l.game);
                }
                if d.state == State::Initial {
                    self.change_state(app, host, State::WeaponSelection);
                }
            }
            Kind::Replay(r) => {
                if d.state == State::Initial {
                    let mut game = new_game(host, new_settings(), r.tc.clone());
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
                    self.change_state(app, host, State::Game);
                }
            }
        }
        let game = self.game().expect("no game");
        game.focus(&mut app.play);
        game.focus(&mut app.single);
        self.data.going_to_menu = false;
        self.data.fade_value = 0;
    }

    /// One frame: false once the controller is done and back to the menu.
    pub fn process(&mut self, app: &mut App, host: &Host) -> bool {
        match &mut self.kind {
            Kind::Local(_) => self.process_local(app, host),
            Kind::Replay(_) => self.process_replay(app, host),
        }
    }

    fn process_local(&mut self, app: &mut App, host: &Host) -> bool {
        let Kind::Local(l) = &mut self.kind else { unreachable!() };
        if self.data.state == State::WeaponSelection {
            if l.ws
                .as_mut()
                .expect("no weapon selection")
                .process_game(&mut l.game, &mut app.rand)
            {
                self.change_state(app, host, State::Game);
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
                    self.change_state(app, host, State::GameEnded);
                }
                i += 1;
            }
        }

        // The local controller doesn't run CommonController::process.
        if self.data.going_to_menu {
            if self.data.fade_value > 0 {
                self.data.fade_value -= 1;
            } else {
                if self.data.state == State::GameEnded {
                    self.end_record();
                    self.present_stats(app, host);
                }
                return false;
            }
        } else if self.data.fade_value < FADE_SHOWN {
            self.data.fade_value += 1;
        }
        true
    }

    fn process_replay(&mut self, app: &mut App, host: &Host) -> bool {
        if self.data.in_game() {
            let Kind::Replay(r) = &mut self.kind else {
                unreachable!()
            };
            let game = r.game.as_mut().expect("no game");
            if app.keys.test_sdl_once(SCANCODE_R) {
                game.assign(r.initial.as_ref().expect("no start to rewind to"));
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
                            self.change_state(app, host, State::GameEnded);
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
                } else if d.fade_value < FADE_SHOWN {
                    d.fade_value += 1;
                }
                i += 1;
            }

            if self.game().expect("no game").is_over() {
                self.change_state(app, host, State::GameEnded);
            }
        }

        // CommonController::process: the frame skip keys, and the frame
        // counted.
        self.data.process(&mut app.keys);

        if self.data.going_to_menu && self.data.fade_value <= 0 {
            if self.data.state == State::GameEnded {
                self.present_stats(app, host);
            }
            return false;
        }

        let Kind::Replay(r) = &self.kind else { unreachable!() };
        if r.reader.is_none() && self.data.state == State::Game {
            self.present_stats(app, host);
            return false;
        }
        true
    }

    /// Draws the game: the split screen on the app's play renderer, or with
    /// `spectators` the spectator one on its single renderer.
    pub fn draw(&mut self, app: &mut App, spectators: bool) {
        let state = self.data.state;
        match &mut self.kind {
            Kind::Local(l) => {
                if state == State::WeaponSelection {
                    l.ws.as_mut()
                        .expect("no weapon selection")
                        .draw_game(&mut l.game, app, state, spectators);
                } else {
                    l.game.draw(app.renderer(spectators), state, spectators, false);
                }
            }
            Kind::Replay(r) => {
                if state == State::Game || state == State::GameEnded {
                    r.game
                        .as_mut()
                        .expect("no game")
                        .draw(app.renderer(spectators), state, spectators, true);
                }
            }
        }
        app.renderer(spectators).data.fade_value = self.data.fade_value;
    }

    fn change_state(&mut self, app: &mut App, host: &Host, new_state: State) {
        if self.data.state == new_state {
            return;
        }
        match &mut self.kind {
            Kind::Local(_) => self.change_local_state(app, host, new_state),
            Kind::Replay(_) => self.change_replay_state(app, new_state),
        }
    }

    fn change_local_state(&mut self, app: &mut App, host: &Host, new_state: State) {
        let Kind::Local(l) = &mut self.kind else { unreachable!() };
        // The new state is prepared before the old one goes: weapon
        // selection is dropped only once recording has started.
        if self.data.state == State::WeaponSelection {
            l.ws.as_mut().expect("no weapon selection").finish(&mut l.game);
        }

        if new_state == State::WeaponSelection {
            l.ws = Some(Weapsel::start(&mut l.game, &mut app.rand));
        } else if new_state == State::Game {
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
        } else if new_state == State::GameEnded && !self.data.going_to_menu {
            self.data.fade_value = 180;
            self.data.going_to_menu = true;
        }

        if self.data.state == State::WeaponSelection {
            self.data.fade_value = FADE_SHOWN;
            let Kind::Local(l) = &mut self.kind else { unreachable!() };
            l.ws = None;
        }
        self.data.state = new_state;
    }

    fn change_replay_state(&mut self, app: &mut App, new_state: State) {
        let Kind::Replay(r) = &mut self.kind else {
            unreachable!()
        };
        if new_state == State::Game {
            let game = r.game.as_mut().expect("no game");
            // The replay's viewports are replaced: they aren't what this
            // screen shows.
            game.clear_viewports();
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
            r.initial = Some(game.copy());
            r.initial_pos = r.reader.as_ref().expect("no replay").pos();
        } else if new_state == State::GameEnded && !self.data.going_to_menu {
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

    /// Whether a game is on, for the menu to offer to resume it.
    pub fn running(&self) -> bool {
        self.data.state != State::GameEnded && self.data.state != State::Initial
    }

    /// The level shown behind the menu, if any.
    pub fn level(&mut self) -> Option<&mut LevelBox> {
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
fn start_recording(app: &App, host: &Host, l: &mut Local) -> Result<(), String> {
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

impl Controller {
    /// A local match with the settings' two players (`LocalController`), with
    /// `host`'s sound and clock.
    pub fn local(host: &Host, settings: SharedSettings, tc: SharedTc) -> Box<Controller> {
        // SAFETY: the game is new, and the worms are its own.
        let mut game = new_game(host, settings.clone(), tc);
        let s = settings.borrow();
        let mut indexes = [0; 2];
        for (i, ws) in s.data.worms.iter().enumerate() {
            let id = game.new_worm(Some(ws.clone()));
            let ws = ws.borrow();
            game.list_worm(Some(id));
            let w = game.worm_mut(id);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
            w.set_ai(ws.data.controller, s.data.ext.ai_parallels.max(0) as usize, i == 0);
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

    /// A replay of `node`, played with `tc` (`ReplayController`), or none
    /// if it can't be read.
    pub fn replay(node: &Node, tc: SharedTc) -> Option<Box<Controller>> {
        let reader = ReplayReader::new(node)?;
        let r = Replay {
            reader: Some(reader),
            tc,
            game: None,
            initial: None,
            initial_pos: 0,
        };
        Some(Controller::new(Kind::Replay(r)))
    }

    /// What the controllers share.
    pub fn data(&mut self) -> &mut ControllerData {
        &mut self.data
    }

    pub fn is_replay(&self) -> bool {
        matches!(self.kind, Kind::Replay(_))
    }

    /// The screen the controller opened, if any, to push.
    pub fn take_opened(&mut self) -> Option<BoxedScreen> {
        self.opened.take()
    }
}

#[cfg(test)]
#[path = "controllers_tests.rs"]
mod tests;
