//! The whole app driven by scripted keys (`hashApp` in the C++ hash run).
//!
//! The app's screens run on a screen stack as the game runs them, with a
//! headless host in place of the window: each frame shown prints a hash of
//! the screen and its palette ("app N hash frame"), and the events come
//! from a timeline of key presses and typing fixed before the run. Each
//! sound started prints a line too ("app-sound N id"). Nothing about it
//! depends on how the screens are written, so a version that runs them
//! differently must show the same frames.
//!
//! Time only moves when a frame is shown, or when a screen waits for an
//! event (a text box, a message, choosing a key), which skips to that
//! event.
//!
//! The script counts key presses to reach each menu item, so a change that
//! moves the menus (an item added, a different start) shows as different
//! frames here; check where the script goes before re-recording.

use super::game::add_screen;
use super::games::tc_data;
use super::{Fnv, Out};
use crate::app::app;
use crate::app_screens::liero_main_screen;
use crate::controllers::liero_controller_on_key;
use crate::fs::Node;
use crate::keys::sdl_to_dos_keysym;
use crate::menus::{liero_menus_load, liero_menus_reset};
use crate::screen::{
    liero_app_push, liero_app_step, liero_screen_set_host, Host, LieroEvent, LieroTypedKey, Platform, ScreenStack,
};
use crate::settings::LieroSettingsBox;
use crate::sfx::LieroSoundOut;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

// SDL's scancodes the script presses.
const SC_A: i32 = 4;
const SC_Z: i32 = 29;
const SC_1: i32 = 30;
const SC_9: i32 = 38;
const SC_0: i32 = 39;
const SC_RETURN: i32 = 40;
const SC_ESCAPE: i32 = 41;
const SC_BACKSPACE: i32 = 42;
const SC_SPACE: i32 = 44;
const SC_SLASH: i32 = 56;
const SC_F1: i32 = 58;
const SC_F2: i32 = 59;
const SC_F3: i32 = 60;
const SC_F7: i32 = 64;
const SC_PAGEUP: i32 = 75;
const SC_PAGEDOWN: i32 = 78;
const SC_RIGHT: i32 = 79;
const SC_LEFT: i32 = 80;
const SC_DOWN: i32 = 81;
const SC_UP: i32 = 82;
const fn sc_letter(c: u8) -> i32 {
    SC_A + (c - b'a') as i32
}
const SC_K: i32 = sc_letter(b'k');
const SC_L: i32 = sc_letter(b'l');
const SC_W: i32 = sc_letter(b'w');

/// The key code SDL's default keymap gives a scancode, which the menus'
/// typing search reads.
fn key_code(sc: i32) -> i32 {
    match sc {
        SC_A..=SC_Z => b'a' as i32 + (sc - SC_A),
        SC_1..=SC_9 => b'1' as i32 + (sc - SC_1),
        SC_0 => b'0' as i32,
        // SDL_SCANCODE_TO_KEYCODE
        _ => sc | 1 << 30,
    }
}

#[derive(Clone, Copy)]
enum Ev {
    Key { down: bool, scancode: i32 },
    Text(u8),
}

struct Step {
    at: i32,
    ev: Ev,
}

/// The headless window: the timeline, the clock and what it printed.
#[derive(Default)]
struct Driver {
    steps: VecDeque<Step>,
    /// Frames shown, plus the frames skipped while waiting for events.
    now: i32,
    shown: i32,
    limit: i32,
    /// Waits for an event since the last frame or event, to stop a run
    /// whose screens wait for an event that never comes.
    idles: u32,
    /// The keys pressed since the frame's events were last read
    /// (`Gfx::keyBuf`).
    key_buf: Vec<LieroTypedKey>,
    lines: Vec<String>,
    /// Why the run stopped early, which ends the screens.
    failed: Option<&'static str>,
}

thread_local! {
    static DRIVER: RefCell<Driver> = RefCell::new(Driver::default());
}

fn driver<R>(f: impl FnOnce(&mut Driver) -> R) -> R {
    DRIVER.with(|d| f(&mut d.borrow_mut()))
}

fn next_event() -> Option<Ev> {
    driver(|d| {
        if d.steps.front().is_none_or(|s| s.at > d.now) {
            return None;
        }
        d.idles = 0;
        d.steps.pop_front().map(|s| s.ev)
    })
}

/// `Gfx::processEvent`, giving the keys to the controller with
/// `to_controller`. F11, which toggles fullscreen, never comes.
fn process_event(ev: Ev, to_controller: bool) {
    let Ev::Key { down, scancode } = ev else { return };
    if down {
        driver(|d| {
            if d.key_buf.len() < 32 {
                d.key_buf.push(LieroTypedKey {
                    sym: key_code(scancode),
                    scancode: scancode as u32,
                });
            }
        });
    }
    let dos = app().keys.event(scancode, down);
    if dos != 0 && to_controller {
        if let Some(c) = app().controller.as_deref_mut() {
            // SAFETY: the app's controller is a live one.
            unsafe { liero_controller_on_key(c, dos as i32, down) };
        }
    }
}

/// `Gfx::process`: every event due, into the key states.
fn process(to_controller: bool) {
    driver(|d| d.key_buf.clear());
    while let Some(ev) = next_event() {
        process_event(ev, to_controller);
    }
}

fn report_sound(id: i32) {
    driver(|d| {
        let line = format!("app-sound {} {:016x}", d.shown, id as u32);
        d.lines.push(line);
    });
}

fn ticks() -> u32 {
    driver(|d| (d.now as u32).wrapping_mul(14))
}

/// `t` as "%Y-%m-%d %H.%M.%S" in UTC, the same in every time zone.
fn utc_text(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}.{:02}.{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// The hash run's script, as the screens see it.
struct HashPlatform;

impl Platform for HashPlatform {
    fn poll_event(&self) -> Option<LieroEvent> {
        let ev = next_event()?;
        process_event(ev, false);
        let mut e = LieroEvent::default();
        match ev {
            Ev::Key { down, scancode } => {
                e.key_down = down;
                if down {
                    e.scancode = scancode as u32;
                    e.choice = true;
                    e.key = sdl_to_dos_keysym(scancode);
                }
            }
            // The script types only ASCII, which is the same in DOS.
            Ev::Text(c) => e.text = c,
        }
        Some(e)
    }

    fn process(&self) {
        process(false);
    }

    /// The frame's events, with their keys for the controller.
    fn process_controller(&self) {
        process(true);
    }

    fn clear_keys(&self) {
        app().keys.clear();
    }

    fn play_sound(&self, id: i32) {
        report_sound(id);
    }

    fn test_key_once(&self, scancode: u32) -> bool {
        app().keys.test_sdl_once(scancode as i32)
    }

    fn test_key(&self, scancode: u32) -> bool {
        app().keys.test_sdl(scancode as i32)
    }

    fn typed_keys(&self) -> Vec<LieroTypedKey> {
        driver(|d| d.key_buf.clone())
    }

    fn ticks(&self) -> u32 {
        ticks()
    }

    fn app_drawing(&self) -> bool {
        true
    }

    fn hires_ticks(&self) -> u64 {
        u64::from(ticks()) * 1_000_000
    }

    fn time(&self) -> i64 {
        // 2001-09-09, then 70 frames a second.
        driver(|d| 1_000_000_000 + i64::from(d.now / 70))
    }

    fn replay_time(&self, t: i64) -> Vec<u8> {
        utc_text(t).into_bytes()
    }

    fn sound(&self) -> LieroSoundOut {
        // No mixer: the hash run has no sound.
        LieroSoundOut {
            mixer: std::ptr::null_mut(),
            report: Some(report_sound),
        }
    }

    /// A frame shown: the screen's pixels and palette.
    fn show(&self) {
        let a = app();
        let r = if a.primary_single { &*a.single } else { &*a.play };
        let mut f = Fnv::default();
        add_screen(&mut f, r);
        driver(|d| {
            let line = format!("app {} {:016x} frame", d.shown, f.0);
            d.lines.push(line);
            d.shown += 1;
            d.now += 1;
            d.idles = 0;
            if d.shown > d.limit {
                d.failed.get_or_insert("the app ran past the end of its script");
            }
        });
    }

    /// The top screen waits for an event: skip to the next one.
    fn idle(&self) {
        driver(|d| match d.steps.front() {
            Some(s) => {
                d.now = d.now.max(s.at);
                d.idles += 1;
                if d.idles > 1000 {
                    d.failed.get_or_insert("the app kept waiting for an event that was due");
                }
            }
            None => {
                d.failed
                    .get_or_insert("the app waited for an event after its script ended");
            }
        });
    }
}

/// Builds the timeline: `t` is the frame the next event happens on.
struct Script {
    steps: VecDeque<Step>,
    t: i32,
}

impl Script {
    fn key(&mut self, scancode: i32, down: bool) {
        self.steps.push_back(Step {
            at: self.t,
            ev: Ev::Key { down, scancode },
        });
    }

    /// Held for `hold` frames, then `gap` frames before the next event.
    fn press_for(&mut self, sc: i32, times: i32, hold: i32, gap: i32) {
        for _ in 0..times {
            self.key(sc, true);
            self.t += hold;
            self.key(sc, false);
            self.t += gap;
        }
    }

    fn press_n(&mut self, sc: i32, times: i32) {
        self.press_for(sc, times, 3, 5);
    }

    fn press(&mut self, sc: i32) {
        self.press_n(sc, 1);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.bytes() {
            self.steps.push_back(Step {
                at: self.t,
                ev: Ev::Text(c),
            });
            self.t += 4;
        }
    }

    fn wait(&mut self, frames: i32) {
        self.t += frames;
    }
}

fn build_script(s: &mut Script) {
    // The main menu fades in.
    s.wait(40);

    // Match setup: one life each, then the level and weapon screens, then a
    // saved and reloaded setup.
    s.press(SC_F7);
    s.press(SC_DOWN); // LIVES
    s.press(SC_RETURN); // typed in a box
    s.press_n(SC_BACKSPACE, 2);
    s.type_text("1");
    s.press(SC_RETURN);
    s.press(SC_DOWN); // LEVEL
    s.press(SC_RETURN);
    s.wait(20);
    s.press(SC_DOWN);
    s.press(SC_UP);
    s.press(SC_ESCAPE);
    s.press_n(SC_DOWN, 2); // WEAPON OPTIONS
    s.press(SC_RETURN);
    s.wait(20);
    s.press_n(SC_DOWN, 3);
    s.press(SC_RIGHT); // bots only, and back
    s.press(SC_LEFT);
    s.press(SC_UP);
    s.press(SC_LEFT); // off, wrapping round
    s.press(SC_PAGEDOWN);
    s.press(SC_PAGEUP);
    s.press(SC_Z); // the first name starting with Z
    s.press(SC_ESCAPE);
    s.press(SC_DOWN); // MAX BONUSES, held up, then
    s.press_for(SC_RIGHT, 1, 30, 5); // more digits than the box takes
    s.press(SC_RETURN);
    s.press_n(SC_BACKSPACE, 3);
    s.type_text("123");
    s.press(SC_RETURN);
    s.press_n(SC_DOWN, 3); // AMOUNT OF BLOOD, not typed in
    s.press(SC_RETURN);
    s.press_n(SC_DOWN, 3); // SAVE SETUP AS...
    s.press(SC_RETURN);
    s.type_text("HASHAPP");
    s.press(SC_RETURN);
    s.press(SC_DOWN); // LOAD SETUP
    s.press(SC_RETURN);
    s.wait(20);
    s.press(SC_DOWN);
    s.press(SC_RETURN);
    s.press(SC_ESCAPE);

    // Bots keep the weapons they have, so the weapon screen waits only for
    // the right player. Games are recorded, which is on by default.
    s.press(SC_F2);
    s.press_n(SC_DOWN, 10); // BOT WEAPONS
    s.press(SC_RIGHT);
    s.press(SC_ESCAPE);

    // The left player, chosen in the main menu: a name, a key, a saved and
    // reloaded profile, and the CPU bot.
    s.press_n(SC_DOWN, 4); // OPTIONS to LEFT PLAYER
    s.press(SC_RETURN);
    s.wait(10);
    s.press_n(SC_DOWN, 2); // NAME
    s.press(SC_RETURN);
    s.press_n(SC_BACKSPACE, 3);
    s.type_text("BOT A");
    s.press(SC_RETURN);
    s.press(SC_DOWN); // HEALTH, low so a game ends soon
    s.press(SC_RETURN);
    s.press_n(SC_BACKSPACE, 3);
    s.type_text("5");
    s.press(SC_RETURN);
    s.press_n(SC_DOWN, 4); // AIM UP
    s.press(SC_RETURN);
    s.wait(30);
    s.press(SC_W);
    s.press_n(SC_UP, 7); // SAVE PROFILE AS...
    s.press(SC_RETURN);
    s.type_text("BOTA");
    s.press(SC_RETURN);
    s.press(SC_DOWN); // LOAD PROFILE
    s.press(SC_RETURN);
    s.wait(20);
    s.press(SC_DOWN);
    s.press(SC_RETURN);
    s.press_n(SC_UP, 4); // CONTROLLER, past the profile items
    s.press(SC_RIGHT);
    s.press(SC_ESCAPE);

    // A game until one player is out of lives, then the stats screen. The
    // right player, on its default keys, picks weapons: a new first one,
    // then down to DONE.
    s.press(SC_F1);
    s.wait(60);
    s.press(SC_K);
    s.press(SC_L);
    s.press_n(SC_K, 5);
    s.press(SC_SLASH);
    s.wait(3000 - 60 - 8 * 8);
    // Scrolled down, then held up past the top; to the right pane and past
    // it, then left with the key held, which moves one pane.
    s.press_for(SC_DOWN, 1, 8, 5);
    s.wait(20);
    s.press_for(SC_UP, 1, 20, 5);
    s.wait(20);
    s.press_n(SC_RIGHT, 2);
    s.wait(30);
    s.press_for(SC_LEFT, 1, 6, 5);
    s.wait(30);
    s.press(SC_ESCAPE);
    s.wait(60);

    // The replay just recorded, and a broken one, which shows a message.
    s.press(SC_F3);
    s.wait(20);
    s.press(SC_RETURN);
    s.wait(300);
    s.press(SC_ESCAPE);
    s.wait(60);
    s.press(SC_F3);
    s.wait(20);
    s.press(SC_DOWN);
    s.press(SC_RETURN);
    // The message shows 34 frames after the Enter, past the menu's fade-out.
    // A key pressed on its first frame is dropped, and the next closes it.
    s.wait(34 - 8);
    s.press(SC_SPACE);
    s.wait(30);
    s.press(SC_SPACE);
    s.wait(60);

    // Choosing the TC restarts the main loop.
    s.press_n(SC_DOWN, 3); // TC
    s.press(SC_RETURN);
    s.wait(20);
    s.press(SC_RETURN);
    s.wait(60);

    // Quit.
    s.press(SC_ESCAPE);
    s.press(SC_RETURN);
    s.wait(60);
}

pub fn hash_app(out: &mut Out) {
    // A config folder of its own, with a broken replay to fail on. The game
    // reads "/hashapp" as "./hashapp" (`super::on_disk`).
    let root = "/hashapp";
    for dir in ["", "/Profiles", "/Replays"] {
        crate::disk::create_dir(super::on_disk(&format!("{root}{dir}")).as_bytes());
    }
    assert!(
        crate::disk::write(
            super::on_disk(&format!("{root}/Replays/BROKEN.lrp")).as_bytes(),
            b"This is not a replay."
        ),
        "can't write the broken replay"
    );

    // Fresh menus, as the game starts with: the menu checks before this one
    // leave items hidden and selected.
    let a = app();
    assert!(!a.menus.is_null(), "no menus");
    // SAFETY: the app's menus are C++'s, which live as long as it does.
    liero_menus_reset(unsafe { &mut *a.menus });
    a.settings = Some(LieroSettingsBox::defaults());
    let tc = super::tc::bundled();
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    a.play.load_palette(&tc_data(&tc));
    a.single.load_palette(&tc_data(&tc));
    a.set_tc(Some(tc));
    a.files.config = Node::new(root.as_bytes());
    a.files.tc_root = Node::new(b"TC");
    a.files.settings_node = Node::new(format!("{root}/liero.cfg").as_bytes());
    a.rand.seed(77);
    a.primary_single = false;
    // As in the browser, where the lines were recorded.
    a.save_settings_on_change = true;
    crate::app::menu_settings().data.spectator_window = false;
    a.keys.clear();
    // SAFETY: as above.
    liero_menus_load(unsafe { &mut *a.menus });

    let mut script = Script {
        steps: VecDeque::new(),
        t: 0,
    };
    build_script(&mut script);
    DRIVER.set(Driver {
        limit: script.t + 2000,
        steps: script.steps,
        ..Default::default()
    });

    let platform: Rc<dyn Platform> = Rc::new(HashPlatform);
    liero_screen_set_host(platform.clone());
    let host = Host::new(platform);

    let mut stack = ScreenStack::new();
    liero_app_push(&mut stack, liero_main_screen());
    while liero_app_step(&mut stack, app(), &host) && driver(|d| d.failed.is_none()) {}
    drop(stack);

    let d = DRIVER.take();
    d.lines.iter().for_each(|l| out.line(l));
    match d.failed {
        None => out.line(&format!("app-end {} {:016x}", d.shown, d.steps.len())),
        Some(why) => {
            eprintln!("app: {why} after {} frames", d.shown);
            out.line(&format!("app-error {} {:016x}", d.shown, d.steps.len()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_names_are_utc() {
        assert_eq!(utc_text(1_000_000_000), "2001-09-09 01.46.40");
        assert_eq!(utc_text(0), "1970-01-01 00.00.00");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00.00.00");
    }
}
