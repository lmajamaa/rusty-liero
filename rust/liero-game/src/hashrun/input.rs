//! Sound and input (`src/game/hashsfx.cpp`).
//!
//! Sound: the 16-bit samples made from each TC sound, a mixer driven by
//! random adds, stops, volume changes and mixes of random lengths (sounds
//! starting in the past and the future, more sounds than channels, looping,
//! repeated and null ids, volumes past the mask, a full-scale square wave
//! for clipping and sounds one and two samples long), and four bot games
//! whose sounds are mixed 630 frames per tick, as at 44.1 kHz and 70 ticks
//! a second.
//!
//! Input: the SDL scancode to DOS key mapping for every scancode, key and
//! joystick events into the key state and a controller as `Gfx::processEvent`
//! passes them, the key tests the menus use, key names, the local controller
//! turning keys into worm controls and the frame skip keys.

use super::game::hash_game;
use super::games::{bot_tick, set_up_game, tc_data, HOLDAZONE, HOOKS, KILL_EM_ALL, THROWERS};
use super::{Fnv, Out};
use crate::app::app;
use crate::controller::liero_controller_process;
use crate::controllers::{
    liero_controller_data, liero_controller_free, liero_controller_game, liero_controller_on_key,
    liero_local_controller_new,
};
use crate::game_box::liero_game_set_context;
use crate::keys::{
    is_extended, joy_axis, joy_button, joy_hat, joy_key, key_name, sdl_to_dos, sdl_to_dos_keysym, LieroKeyChanges,
    LieroKeys, LIERO_DK_ESCAPE, LIERO_MAX_JOY_BUTTONS,
};
use crate::rand::LieroRand;
use crate::settings::LieroSettingsBox;
use crate::sfx::{LieroMixer, LieroSoundOut, LIERO_SOUND_LOOP, LIERO_SOUND_NORMAL};
use crate::tc::{LieroTcSound, SharedTc};

fn sounds(tc: &SharedTc) -> std::cell::Ref<'_, [LieroTcSound]> {
    std::cell::Ref::map(tc_data(tc), |d| &d.sounds[..])
}

fn samples(s: &LieroTcSound) -> &[i16] {
    &s.samples
}

fn hash_samples(out: &mut Out, tc: &SharedTc) {
    for (i, s) in sounds(tc).iter().enumerate() {
        let mut f = Fnv::default();
        f.bool(s.valid);
        if s.valid {
            f.u32(s.samples.len() as u32);
            samples(s).iter().for_each(|&v| f.i32(v.into()));
        }
        out.hash("sfx-sample", i as i64, f);
    }
}

fn hash_buffer(f: &mut Fnv, buf: &[i16]) {
    f.u32(buf.len() as u32);
    buf.iter().for_each(|&v| f.i32(v.into()));
}

fn mixer_script(out: &mut Out, tc: &SharedTc, seed: u32, steps: i32) {
    let all = sounds(tc);
    let mut sounds: Vec<&[i16]> = all.iter().filter(|s| s.valid).map(samples).collect();
    // A full-scale square wave, which clips as soon as two play at once.
    let square: Vec<i16> = (0..300)
        .map(|i| if (i / 25) % 2 == 1 { 32767 } else { -32768 })
        .collect();
    sounds.push(&square);
    sounds.push(&[-32768]);
    sounds.push(&[12345, -23456]);

    // Ids are only compared, never followed.
    let ids: [usize; 6] = [0, 16, 32, 48, 64, 80];
    let name = format!("sfx-mixer-{seed}");

    let mut mixer = LieroMixer::default();
    let mut r = LieroRand::new(seed);
    let mut buf = vec![0i16; 5000];
    let mut f = Fnv::default();
    for step in 1..=steps {
        let id = ids[r.below(ids.len() as u32) as usize];
        let op = r.below(10);
        if op < 4 {
            let snd = sounds[r.below(sounds.len() as u32) as usize];
            let time = mixer.now().wrapping_add(r.below(6000) as i32).wrapping_sub(3000) as u32;
            let flags = if r.below(4) == 0 {
                LIERO_SOUND_LOOP
            } else {
                LIERO_SOUND_NORMAL
            };
            // SAFETY: the samples outlive the mixer.
            let added = unsafe { mixer.add(snd.as_ptr(), snd.len(), time, id, flags) };
            // What C's mixer returned, `id` or null if not added, compared
            // with `id` and with null.
            f.bool(added || id == 0);
            f.bool(added && id != 0);
        } else if op == 4 {
            mixer.stop(id);
        } else if op == 5 {
            f.bool(mixer.is_playing(id));
        } else if op == 6 {
            mixer.set_volume(id, f64::from(r.below(2600)) / 1000.0);
        } else {
            let max = if r.below(3) == 0 { 5000 } else { 700 };
            let n = 1 + r.below(max) as usize;
            mixer.mix(&mut buf[..n]);
            hash_buffer(&mut f, &buf[..n]);
            f.i32(mixer.now());
        }

        if step % 100 == 0 {
            out.hash(&name, step.into(), f);
        }
    }
}

fn sound_game(
    out: &mut Out,
    tc: &SharedTc,
    name: &str,
    seed: u32,
    game_mode: u32,
    weapons: Option<&[&str; 5]>,
    ticks: i32,
) {
    let mut game = set_up_game(tc, seed, game_mode, weapons);
    let mut mixer = Box::<LieroMixer>::default();
    let m: *mut LieroMixer = &mut *mixer;
    let settings = game.settings();
    // The mixer lives until the game's sound goes back to none.
    liero_game_set_context(
        &mut game,
        Some(settings.clone()),
        Some(tc.clone()),
        LieroSoundOut { mixer: m, report: None },
    );

    let blood = game.settings_box().data.blood_particle_max;
    game.start(blood);

    let game_name = format!("{name}-game");
    let mut buf = [0i16; 630];
    let mut f = Fnv::default();
    for tick in 1..=ticks {
        bot_tick(&mut game);
        game.process_frame(&HOOKS);
        // SAFETY: the game only uses the mixer during its frame.
        unsafe { (*m).mix(&mut buf) };
        hash_buffer(&mut f, &buf);

        let over = game.is_over();
        if over || tick % 100 == 0 {
            out.hash(name, tick.into(), f);
            // Sounds don't change the game.
            out.line(&format!("{game_name} {tick} {:016x}", hash_game(&mut game)));
        }
        if over {
            break;
        }
    }

    // The mixer is dropped once the game has no sound.
    liero_game_set_context(&mut game, Some(settings), Some(tc.clone()), LieroSoundOut::NONE);
    drop(mixer);
}

const LOUD: [&str; 5] = ["BIG NUKE", "MINIGUN", "CHAINGUN", "LASER", "HELLRAIDER"];

fn hash_sound(out: &mut Out, tc: &SharedTc) {
    hash_samples(out, tc);
    for seed in 1..=4 {
        mixer_script(out, tc, seed, 2000);
    }
    sound_game(out, tc, "sfx-kill-1", 1, KILL_EM_ALL, None, 3000);
    sound_game(out, tc, "sfx-throwers-13", 13, KILL_EM_ALL, Some(&THROWERS), 3000);
    sound_game(out, tc, "sfx-loud-21", 21, KILL_EM_ALL, Some(&LOUD), 3000);
    sound_game(out, tc, "sfx-zone-6", 6, HOLDAZONE, None, 3000);
}

const SDL_NUM_SCANCODES: i32 = 512;
const SC_A: i32 = 4;
const SC_1: i32 = 30;
const SC_2: i32 = 31;
const SC_5: i32 = 34;
const SC_9: i32 = 38;
const SC_0: i32 = 39;
const SC_F11: i32 = 68;
const SC_RIGHT: i32 = 79;
const SC_LCTRL: i32 = 224;
const SC_RCTRL: i32 = 228;

fn hash_key_map(out: &mut Out) {
    // Every scancode SDL has, in blocks, then some it doesn't.
    for block in 0..SDL_NUM_SCANCODES / 64 {
        let mut f = Fnv::default();
        for sc in block * 64..block * 64 + 64 {
            f.u32(sdl_to_dos(sc));
            f.u32(sdl_to_dos_keysym(sc));
        }
        out.hash("keys-map", block.into(), f);
    }
    let mut f = Fnv::default();
    for sc in [-1, -1000, 512, 513, 1000, 0x7fff_ffff] {
        f.u32(sdl_to_dos(sc));
    }
    out.hash("keys-map", (SDL_NUM_SCANCODES / 64).into(), f);
}

type JoyButtons = [bool; LIERO_MAX_JOY_BUTTONS as usize];

/// What `Gfx::processEvent` keeps besides the key state: the 32 key
/// presses it buffers until `Gfx::process` empties them, and each
/// joystick's buttons.
struct Events {
    key_buf: Vec<i32>,
    joysticks: [JoyButtons; 2],
}

/// An event, with only the fields the hash run sets.
enum Event {
    Key { down: bool, scancode: i32 },
    JoyAxis { which: usize, axis: u8, value: i16 },
    JoyHat { which: usize, value: u8 },
    JoyButton { which: usize, button: u8, pressed: bool },
}

/// `KeyRecorder`: hashes the keys a controller is given.
fn record(rec: &mut Fnv, key: u32, state: bool) {
    rec.u32(key);
    rec.bool(state);
}

fn send_keys(rec: Option<&mut Fnv>, changes: LieroKeyChanges) {
    if let Some(rec) = rec {
        for i in 0..changes.count as usize {
            record(rec, changes.keys[i], changes.states[i]);
        }
    }
}

impl Events {
    /// `Gfx::processEvent`, with `rec` as the controller. F11, which
    /// toggles fullscreen, never comes.
    fn process(&mut self, keys: &mut LieroKeys, ev: &Event, rec: Option<&mut Fnv>) {
        match *ev {
            Event::Key { down, scancode } => {
                if down && self.key_buf.len() < 32 {
                    self.key_buf.push(scancode);
                }
                let dos = keys.event(scancode, down);
                if let Some(rec) = rec.filter(|_| dos != 0) {
                    record(rec, dos, down);
                }
            }
            Event::JoyAxis { which, axis, value } => {
                send_keys(rec, joy_axis(&mut self.joysticks[which], which as i32, axis, value))
            }
            Event::JoyHat { which, value } => send_keys(rec, joy_hat(&mut self.joysticks[which], which as i32, value)),
            Event::JoyButton { which, button, pressed } => send_keys(
                rec,
                joy_button(&mut self.joysticks[which], which as i32, button, pressed),
            ),
        }
    }
}

fn hash_key_state(f: &mut Fnv, keys: &LieroKeys, ev: &Events) {
    keys.down.iter().for_each(|&k| f.bool(k));
    for js in &ev.joysticks {
        js.iter().for_each(|&b| f.bool(b));
    }
    f.u32(ev.key_buf.len() as u32);
}

fn random_scancode(r: &mut LieroRand) -> i32 {
    loop {
        let sc = match r.below(4) {
            0 => r.below(SDL_NUM_SCANCODES as u32) as i32,
            // Arrows and the keys after them
            1 => SC_RIGHT + r.below(8) as i32,
            _ => r.below(100) as i32,
        };
        // F11 toggles fullscreen, which needs a window.
        if sc != SC_F11 {
            return sc;
        }
    }
}

fn random_axis(r: &mut LieroRand) -> i16 {
    const VALUES: [i16; 9] = [-32768, -10001, -10000, -1, 0, 1, 10000, 10001, 32767];
    if r.below(3) == 0 {
        return (r.below(65536) as i32 - 32768) as i16;
    }
    VALUES[r.below(9) as usize]
}

fn hash_events(out: &mut Out, seed: u32, steps: i32) {
    let keys = &mut app().keys;
    keys.clear();
    let mut ev = Events {
        key_buf: Vec::new(),
        joysticks: [[false; LIERO_MAX_JOY_BUTTONS as usize]; 2],
    };

    let name = format!("keys-events-{seed}");
    let mut rec = Fnv::default();
    let mut r = LieroRand::new(seed);
    let mut f = Fnv::default();
    for step in 1..=steps {
        let op = r.below(12);
        let event = match op {
            0..=3 => {
                let down = r.below(2) != 0;
                Some(Event::Key {
                    down,
                    scancode: random_scancode(&mut r),
                })
            }
            4 => {
                let which = r.below(2) as usize;
                let axis = r.below(6) as u8;
                Some(Event::JoyAxis {
                    which,
                    axis,
                    value: random_axis(&mut r),
                })
            }
            5 => {
                let which = r.below(2) as usize;
                Some(Event::JoyHat {
                    which,
                    value: r.below(16) as u8,
                })
            }
            6 => {
                // The event's type, then its fields; the button state is
                // what counts.
                r.below(2);
                let which = r.below(2) as usize;
                let button = r.below(16) as u8;
                Some(Event::JoyButton {
                    which,
                    button,
                    pressed: r.below(2) != 0,
                })
            }
            _ => {
                let k = r.below(177);
                let sc = random_scancode(&mut r);
                match op {
                    7 => f.bool(keys.test_once(k)),
                    8 => {
                        f.bool(keys.test(k));
                        keys.toggle(k);
                    }
                    9 => f.bool(keys.test_sdl_once(sc)),
                    10 => {
                        f.bool(keys.test_sdl(sc));
                        keys.release_sdl(sc);
                    }
                    _ => keys.set(k, r.below(2) != 0),
                }
                None
            }
        };

        if let Some(e) = event {
            let to_rec = r.below(4) != 0;
            ev.process(keys, &e, to_rec.then_some(&mut rec));
        }

        hash_key_state(&mut f, keys, &ev);
        f.u32(rec.0 as u32);
        f.u32((rec.0 >> 32) as u32);

        if step % 40 == 0 {
            // The key buffer holds 32 presses until `Gfx::process` empties it.
            ev.key_buf.drain(..).for_each(|sc| f.i32(sc));
        }
        if step % 100 == 0 {
            out.hash(&name, step.into(), f);
        }
    }

    keys.clear();
}

fn hash_key_names(out: &mut Out) {
    let mut f = Fnv::default();
    for k in 0..1200 {
        let n = key_name(k);
        f.u32(n.len() as u32);
        f.bytes(&n);
        f.bool(is_extended(k));
    }
    for j in 0..4 {
        for b in 0..LIERO_MAX_JOY_BUTTONS as i32 {
            f.u32(joy_key(j, b));
        }
    }
    out.hash("keys-names", 0, f);
}

fn hash_controls(out: &mut Out, tc: &SharedTc, seed: u32, steps: i32) {
    let settings = LieroSettingsBox::defaults();
    let mut r = LieroRand::new(seed);
    // A small pool of keys, so the two worms and their controls share some.
    let pool: [u32; 12] = std::array::from_fn(|_| {
        if r.below(4) == 0 {
            let joy = r.below(2) as i32;
            joy_key(joy, r.below(32) as i32)
        } else {
            2 + r.below(100)
        }
    });
    for ws in &settings.borrow().worms {
        let mut ws = ws.borrow_mut();
        ws.data.controller = 0;
        ws.data
            .controls
            .iter_mut()
            .for_each(|c| *c = pool[r.below(12) as usize]);
        ws.data
            .controls_ex
            .iter_mut()
            .for_each(|c| *c = pool[r.below(12) as usize]);
    }

    // SAFETY: the controller keeps references to the settings and the TC,
    // and is freed below.
    let lc = unsafe { liero_local_controller_new(settings.clone(), tc.clone()) };
    // SAFETY: the controller is new; its data and game live as long as it
    // does.
    let (d, game) = unsafe { (liero_controller_data(&mut *lc), &mut *liero_controller_game(&mut *lc)) };
    let keys = &mut app().keys;

    let name = format!("keys-controls-{seed}");
    let mut f = Fnv::default();
    for step in 1..=steps {
        let op = r.below(10);
        if op < 7 {
            let key = if r.below(8) == 0 {
                LIERO_DK_ESCAPE
            } else if r.below(8) == 0 {
                r.below(177)
            } else {
                pool[r.below(12) as usize]
            };
            // SAFETY: the controller is a live one.
            unsafe { liero_controller_on_key(&mut *lc, key as i32, r.below(2) != 0) };
        } else if op == 7 {
            // SAFETY: the controller's data lives as long as it does.
            unsafe { (*d).going_to_menu = false };
        } else if op == 8 {
            // The frame skip keys, maybe with ctrl.
            const KEYS: [i32; 8] = [SC_0, SC_1, SC_2, SC_5, SC_9, SC_LCTRL, SC_RCTRL, SC_A];
            for _ in 0..3 {
                let k = sdl_to_dos(KEYS[r.below(8) as usize]);
                keys.set(k, r.below(2) != 0);
            }
            // SAFETY: as above.
            liero_controller_process(unsafe { &mut *d }, keys);
        } else {
            let i = r.below(2) as usize;
            let w = game.worm_at(i).expect("no worm");
            w.data.control_states = r.below(128);
        }

        for w in game.listed_worms() {
            let w = w.expect("an empty place among the game's worms");
            f.u32(w.data.control_states);
            f.u32(w.data.clean_control_states);
        }
        // SAFETY: as above.
        let d = unsafe { &*d };
        f.i32(d.fade_value);
        f.bool(d.going_to_menu);
        f.i32(d.frame_skip);
        f.bool(d.inverse_frame_skip);
        f.i32(d.cycles);
        keys.down.iter().for_each(|&k| f.bool(k));

        if step % 100 == 0 {
            out.hash(&name, step.into(), f);
        }
    }

    keys.clear();
    // SAFETY: the controller isn't used again.
    unsafe { liero_controller_free(lc) };
}

fn hash_input(out: &mut Out, tc: &SharedTc) {
    hash_key_map(out);
    for seed in 1..=3 {
        hash_events(out, seed, 2000);
    }
    hash_key_names(out);
    for seed in 1..=3 {
        hash_controls(out, tc, seed, 1000);
    }
}

pub fn hash_sound_and_input(out: &mut Out) {
    let tc = super::tc::bundled();
    hash_sound(out, &tc);
    hash_input(out, &tc);
}
