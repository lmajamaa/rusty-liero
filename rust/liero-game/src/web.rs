//! The web build's shell, on `wasm32-unknown-unknown`: what `src/game` (the
//! C++ on SDL and Emscripten) does in the browser, in Rust. It draws the
//! app's frames on the page's canvas, turns the page's key and gamepad
//! events into the app's, plays the mixer through Web Audio and runs the
//! screens every animation frame. The files are a tree in memory
//! (`disk.rs`) with the bundled TC; the page (`web/index.html`) loads the
//! saves into it before the game starts and keeps what changes in IndexedDB.
//!
//! What the screens call here is the same host the hash run's app section
//! implements (`hashrun/app.rs`), with real events, clock and sound.
//!
//! Keys: the app's key settings and screens use SDL's scancodes, which are
//! USB's key positions, as `KeyboardEvent.code` names them. So a key is the
//! same physical key on every layout, as SDL does natively; Emscripten's SDL
//! went by `keyCode` instead, which differs for some keys on other layouts.

use crate::app::App;
use crate::app_screens::main_screen;
use crate::gfx::{fit_screen, palette_activate, scale_draw, Color};
use crate::keys::{joy_axis, joy_axis_key, joy_button, joy_button_key, sdl_to_dos_keysym, JoyButtons, KeyChanges};
use crate::menu_screen::LIERO_MENU_HIDDEN;
use crate::menus::Menus;
use crate::screen::{Event, Host, Platform, ScreenStack, TypedKey};
use crate::sfx::{Mixer, SoundOut};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{
    AudioContext, AudioContextOptions, AudioProcessingEvent, CanvasRenderingContext2d, Gamepad, GamepadButton,
    HtmlCanvasElement, ImageData, KeyboardEvent,
};

include!(concat!(env!("OUT_DIR"), "/tc_files.rs"));

/// The config folder, where the saves are, and the folder of TCs. The
/// game reads `/x` as `./x`, the tree's `x`.
const CONFIG: &[u8] = b"/liero";
const TC_ROOT: &[u8] = b"/TC";

/// SDL's audio as the C++ opened it: 44.1 kHz mono, 2,048 samples a call.
const SAMPLE_RATE: f32 = 44_100.0;
const AUDIO_FRAMES: u32 = 2048;

/// An event from the page, as SDL gave the C++ shell.
#[derive(Clone, Copy)]
enum Ev {
    /// `sym` is the key code the layout gives, for the menus' typing search.
    Key {
        down: bool,
        scancode: i32,
        sym: i32,
    },
    /// A character typed in a text box, in the DOS code page.
    Text(u8),
    JoyAxis {
        joy: i32,
        axis: u8,
        value: i16,
    },
    JoyButton {
        joy: i32,
        button: u8,
        down: bool,
    },
}

/// A gamepad as last read, to make events of what changed.
struct Pad {
    buttons: Vec<bool>,
    axes: Vec<i16>,
    /// The app's view of its buttons (`Joystick::btnState`).
    state: JoyButtons,
}

#[derive(Default)]
struct Shell {
    events: VecDeque<Ev>,
    /// The keys pressed since the frame's events were last read
    /// (`Gfx::keyBuf`).
    key_buf: Vec<TypedKey>,
    text_input: bool,
    pads: Vec<Option<Pad>>,
    canvas: Option<HtmlCanvasElement>,
    ctx: Option<CanvasRenderingContext2d>,
    /// The canvas's pixels, as RGBA.
    rgba: Vec<u8>,
    prev_mag: i32,
    mixer: Option<*mut Mixer>,
    audio: Option<AudioContext>,
    /// When the game started, in `performance.now()` milliseconds.
    start_ms: f64,
    /// The page's function that keeps a save: (path, bytes or null).
    persist: Option<js_sys::Function>,
    /// The menus' sounds since the last frame, played with the app's TC at
    /// its end.
    sounds: Vec<i32>,
    /// The browser left full screen by itself since the last frame.
    left_fullscreen: bool,
}

thread_local! {
    static SHELL: RefCell<Shell> = RefCell::new(Shell::default());
}

/// `f` on the shell. It must not call into the game, which calls back here.
fn shell<R>(f: impl FnOnce(&mut Shell) -> R) -> R {
    SHELL.with(|s| f(&mut s.borrow_mut()))
}

fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

fn now_ms() -> f64 {
    window().performance().map_or(0.0, |p| p.now())
}

// ---- Keys

/// The SDL scancode of a `KeyboardEvent.code`, which names the same USB key
/// positions; 0 for a key SDL has no scancode for.
fn scancode(code: &str) -> i32 {
    let letter_or_digit = |rest: &str, base: i32| {
        let c = rest.as_bytes();
        (c.len() == 1).then(|| base + c[0] as i32)
    };
    if let Some(n) = code
        .strip_prefix("Key")
        .and_then(|r| letter_or_digit(r, 4 - b'A' as i32))
    {
        return n;
    }
    if let Some(d) = code
        .strip_prefix("Digit")
        .and_then(|r| letter_or_digit(r, -(b'0' as i32)))
    {
        return if d == 0 { 39 } else { 29 + d };
    }
    if let Some(d) = code
        .strip_prefix("Numpad")
        .and_then(|r| letter_or_digit(r, -(b'0' as i32)))
    {
        return if d == 0 { 98 } else { 88 + d };
    }
    if let Some(n) = code.strip_prefix('F').and_then(|r| r.parse::<i32>().ok()) {
        return match n {
            1..=12 => 57 + n,
            13..=24 => 91 + n,
            _ => 0,
        };
    }
    match code {
        "Enter" => 40,
        "Escape" => 41,
        "Backspace" => 42,
        "Tab" => 43,
        "Space" => 44,
        "Minus" => 45,
        "Equal" => 46,
        "BracketLeft" => 47,
        "BracketRight" => 48,
        "Backslash" => 49,
        "Semicolon" => 51,
        "Quote" => 52,
        "Backquote" => 53,
        "Comma" => 54,
        "Period" => 55,
        "Slash" => 56,
        "CapsLock" => 57,
        "PrintScreen" => 70,
        "ScrollLock" => 71,
        "Pause" => 72,
        "Insert" => 73,
        "Home" => 74,
        "PageUp" => 75,
        "Delete" => 76,
        "End" => 77,
        "PageDown" => 78,
        "ArrowRight" => 79,
        "ArrowLeft" => 80,
        "ArrowDown" => 81,
        "ArrowUp" => 82,
        "NumLock" => 83,
        "NumpadDivide" => 84,
        "NumpadMultiply" => 85,
        "NumpadSubtract" => 86,
        "NumpadAdd" => 87,
        "NumpadEnter" => 88,
        "NumpadDecimal" => 99,
        "IntlBackslash" => 100,
        "ContextMenu" => 101,
        "NumpadEqual" => 103,
        "IntlRo" => 135,
        "IntlYen" => 137,
        "ControlLeft" => 224,
        "ShiftLeft" => 225,
        "AltLeft" => 226,
        "MetaLeft" => 227,
        "ControlRight" => 228,
        "ShiftRight" => 229,
        "AltRight" => 230,
        "MetaRight" => 231,
        _ => 0,
    }
}

/// The key code SDL gives a key: the character for one that types one
/// (lower case), else the scancode with bit 30 (`SDL_SCANCODE_TO_KEYCODE`).
fn key_code(key: &str, scancode: i32) -> i32 {
    match key {
        "Enter" => 13,
        "Escape" => 27,
        "Backspace" => 8,
        "Tab" => 9,
        "Delete" => 127,
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii() => c.to_ascii_lowercase() as i32,
                _ => scancode | 1 << 30,
            }
        }
    }
}

/// The character typed, in the DOS code page: ASCII, the Nordic letters, or
/// '?' (`utf8ToDOS`).
fn dos_char(c: char) -> u8 {
    match c {
        c if c.is_ascii() => c as u8,
        'å' => 0x86,
        'ä' => 0x84,
        'ö' => 0x94,
        'Å' => 0x8f,
        'Ä' => 0x8e,
        'Ö' => 0x99,
        _ => b'?',
    }
}

fn on_key(e: &KeyboardEvent, down: bool) {
    // Cmd shortcuts stay the browser's (Cmd+R, Cmd+L, ...).
    if e.meta_key() {
        return;
    }
    // Every other key is the game's: Space doesn't scroll and / doesn't
    // open Firefox's quick find.
    e.prevent_default();
    let sc = scancode(&e.code());
    let key = e.key();
    shell(|s| {
        if sc != 0 {
            s.events.push_back(Ev::Key {
                down,
                scancode: sc,
                sym: key_code(&key, sc),
            });
        }
        // SDL's text input: after the key, the character it typed.
        if down && s.text_input && !e.ctrl_key() {
            let mut chars = key.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                s.events.push_back(Ev::Text(dos_char(c)));
            }
        }
    });
}

// ---- Gamepads

/// Reads the gamepads into events, as SDL's joystick events: a button
/// pressed or let go, or an axis moved, from -32767 to 32767.
fn poll_gamepads() {
    let Ok(pads) = window().navigator().get_gamepads() else {
        return;
    };
    let mut events = Vec::new();
    shell(|s| {
        for i in 0..pads.length() {
            let pad = pads.get(i).dyn_into::<Gamepad>().ok();
            if s.pads.len() <= i as usize {
                s.pads.resize_with(i as usize + 1, || None);
            }
            let Some(pad) = pad else {
                s.pads[i as usize] = None;
                continue;
            };
            let buttons: Vec<bool> = pad
                .buttons()
                .iter()
                .map(|b| b.dyn_into::<GamepadButton>().is_ok_and(|b| b.pressed()))
                .collect();
            let axes: Vec<i16> = pad
                .axes()
                .iter()
                .map(|a| (a.as_f64().unwrap_or(0.0).clamp(-1.0, 1.0) * 32767.0) as i16)
                .collect();
            let joy = i as i32;
            let last = s.pads[i as usize].get_or_insert_with(|| Pad {
                buttons: vec![false; buttons.len()],
                axes: vec![0; axes.len()],
                state: [false; crate::keys::LIERO_MAX_JOY_BUTTONS as usize],
            });
            for (b, (&now, was)) in buttons.iter().zip(last.buttons.iter_mut()).enumerate() {
                if now != *was {
                    *was = now;
                    events.push(Ev::JoyButton {
                        joy,
                        button: b as u8,
                        down: now,
                    });
                }
            }
            for (a, (&now, was)) in axes.iter().zip(last.axes.iter_mut()).enumerate() {
                if now != *was {
                    *was = now;
                    events.push(Ev::JoyAxis {
                        joy,
                        axis: a as u8,
                        value: now,
                    });
                }
            }
        }
        s.events.extend(events);
    });
}

// ---- Events, as `Gfx::processEvent`

/// Passes a key to the app's controller, if it has one.
fn send_key(a: &mut App, to_controller: bool, key: u32, down: bool) {
    if !to_controller || key == 0 {
        return;
    }
    if let Some(c) = a.controller.as_deref_mut() {
        // SAFETY: the app's controller is a live one.
        unsafe { c.on_key(key as i32, down) };
    }
}

fn send_keys(a: &mut App, to_controller: bool, changes: KeyChanges) {
    for i in 0..changes.count as usize {
        send_key(a, to_controller, changes.keys[i], changes.states[i]);
    }
}

/// The app's view of gamepad `joy`'s buttons.
/// `f` on pad `joy`'s button states as the app sees them, if it's there.
fn on_pad(joy: i32, f: impl FnOnce(&mut JoyButtons) -> KeyChanges) -> Option<KeyChanges> {
    shell(|s| {
        s.pads
            .get_mut(joy as usize)
            .and_then(Option::as_mut)
            .map(|p| f(&mut p.state))
    })
}

fn process_event(a: &mut App, ev: Ev, to_controller: bool) {
    match ev {
        Ev::Key { down, scancode, sym } => {
            if down {
                shell(|s| {
                    if s.key_buf.len() < 32 {
                        s.key_buf.push(TypedKey {
                            sym,
                            scancode: scancode as u32,
                        });
                    }
                });
            }
            let dos = a.keys.event(scancode, down);
            send_key(a, to_controller, dos, down);
            if down && scancode == SC_F11 {
                set_fullscreen(a, !a.fullscreen(), true);
            }
        }
        Ev::Text(_) => {}
        Ev::JoyAxis { joy, axis, value } => {
            if let Some(changes) = on_pad(joy, |b| joy_axis(b, joy, axis, value)) {
                send_keys(a, to_controller, changes);
            }
        }
        Ev::JoyButton { joy, button, down } => {
            if let Some(changes) = on_pad(joy, |b| joy_button(b, joy, button, down)) {
                send_keys(a, to_controller, changes);
            }
        }
    }
}

const SC_F11: i32 = 68;

fn next_event() -> Option<Ev> {
    shell(|s| s.events.pop_front())
}

/// `Gfx::process`: every event that came, into the key states.
fn process(a: &mut App, to_controller: bool) {
    shell(|s| s.key_buf.clear());
    while let Some(ev) = next_event() {
        process_event(a, ev, to_controller);
    }
}

// ---- The window

/// Shows the canvas at the app's size, double or not (`Gfx::setVideoMode`).
fn set_video_mode(double_res: bool) {
    let (w, h) = if double_res { (640, 400) } else { (320, 200) };
    shell(|s| {
        if let Some(c) = &s.canvas {
            c.set_width(w);
            c.set_height(h);
        }
        s.rgba = vec![0; (w * h * 4) as usize];
        s.prev_mag = 0;
    });
}

/// `Gfx::setFullscreen`: the setting, and the browser's full screen.
fn set_fullscreen(a: &mut App, on: bool, update_menu: bool) {
    if on == a.fullscreen() {
        return;
    }
    a.set_fullscreen(on);
    show_fullscreen(on);
    if update_menu {
        a.shared_menus().borrow_mut().update_items(a, LIERO_MENU_HIDDEN);
    }
}

fn show_fullscreen(on: bool) {
    let Some(doc) = window().document() else { return };
    // The browser only allows it during a key press or click, which F11, the
    // options menu and the click that starts the game are.
    if on {
        if let Some(el) = doc.document_element() {
            let _ = el.request_fullscreen();
        }
    } else if doc.fullscreen_element().is_some() {
        doc.exit_fullscreen();
    }
}

/// When the browser leaves full screen by itself (Esc, or it refused the
/// request), the setting goes off too, at the next frame (`left_fullscreen`).
fn on_fullscreen_change() {
    let shown = window().document().is_some_and(|d| d.fullscreen_element().is_some());
    if !shown {
        shell(|s| s.left_fullscreen = true);
    }
}

/// The browser left full screen: the setting goes off, so the menu shows it
/// and F11 turns it back on in one press.
fn left_fullscreen(a: &mut App) {
    if a.fullscreen() {
        a.set_fullscreen(false);
        a.shared_menus().borrow_mut().update_items(a, LIERO_MENU_HIDDEN);
    }
}

/// `Gfx::draw` and `flip`: the primary renderer's frame on the canvas.
fn show_frame(a: &App) {
    let r = if a.primary_single { &*a.single } else { &*a.play };
    let bmp = r.bmp.view();
    let d = &r.data;
    let mut real = [Color::default(); 256];
    palette_activate(&d.pal, &mut real);
    // RGBA in memory, as ImageData has it.
    let pal32: [u32; 256] = std::array::from_fn(|i| u32::from_le_bytes([real[i].r, real[i].g, real[i].b, 255]));
    let (res_x, res_y) = (d.render_res_x, d.render_res_y);
    shell(|s| {
        let (Some(canvas), Some(ctx)) = (&s.canvas, &s.ctx) else {
            return;
        };
        let (w, h) = (canvas.width() as i32, canvas.height() as i32);
        let (mut ox, mut oy) = (0, 0);
        let mag = fit_screen(w, h, res_x, res_y, &mut ox, &mut oy);
        if mag != s.prev_mag {
            // Clear what a bigger magnification left.
            s.rgba.iter_mut().for_each(|b| *b = 0);
        }
        s.prev_mag = mag;
        let pitch = w as usize * 4;
        if mag > 0 {
            // The frame is `res_x` x `res_y` of the bitmap, which fits the
            // canvas `mag` times from the offset (`fit_screen`).
            let dest = &mut s.rgba[oy as usize * pitch + ox as usize * 4..];
            scale_draw(bmp.pixels, res_x, res_y, bmp.pitch as usize, dest, pitch, mag, &pal32);
        }
        if let Ok(img) = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&s.rgba), w as u32, h as u32) {
            let _ = ctx.put_image_data(&img, 0.0, 0.0);
        }
    });
}

// ---- What the screens call

fn mixer() -> *mut Mixer {
    shell(|s| s.mixer).unwrap_or(std::ptr::null_mut())
}

fn ticks_now() -> u32 {
    (now_ms() - shell(|s| s.start_ms)) as u32
}

/// The browser, as the screens see it.
struct WebPlatform;

impl Platform for WebPlatform {
    fn poll_event(&self, app: &mut App) -> Option<Event> {
        let ev = next_event()?;
        process_event(app, ev, false);
        let mut e = Event::default();
        match ev {
            Ev::Key { down, scancode, .. } => {
                e.key_down = down;
                if down {
                    e.scancode = scancode as u32;
                    e.choice = true;
                    e.key = sdl_to_dos_keysym(scancode);
                }
            }
            Ev::Text(c) => e.text = c,
            Ev::JoyAxis { joy, axis, value } => {
                e.key = joy_axis_key(joy, axis, value);
                e.choice = e.key != 0;
            }
            Ev::JoyButton { joy, button, down } => {
                if down {
                    e.key = joy_button_key(joy, button);
                    e.choice = true;
                }
            }
        }
        Some(e)
    }

    fn process(&self, app: &mut App) {
        process(app, false);
    }

    fn process_controller(&self, app: &mut App) {
        process(app, true);
    }

    fn play_sound(&self, id: i32) {
        shell(|s| s.sounds.push(id));
    }

    fn text_input(&self, on: bool) {
        shell(|s| s.text_input = on);
    }

    fn typed_keys(&self) -> Vec<TypedKey> {
        shell(|s| s.key_buf.clone())
    }

    fn ticks(&self) -> u32 {
        ticks_now()
    }

    fn app_drawing(&self) -> bool {
        true
    }

    fn hires_ticks(&self) -> u64 {
        (now_ms() * 1_000_000.0) as u64
    }

    fn time(&self) -> i64 {
        (js_sys::Date::now() / 1000.0) as i64
    }

    /// `t` as "%Y-%m-%d %H.%M.%S" in local time, as `strftime` wrote it.
    fn replay_time(&self, t: i64) -> Vec<u8> {
        let d = js_sys::Date::new(&JsValue::from_f64(t as f64 * 1000.0));
        format!(
            "{:04}-{:02}-{:02} {:02}.{:02}.{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date(),
            d.get_hours(),
            d.get_minutes(),
            d.get_seconds()
        )
        .into_bytes()
    }

    fn sound(&self) -> SoundOut {
        SoundOut {
            mixer: mixer(),
            report: None,
        }
    }

    fn set_double_res(&self, on: bool) {
        // The options menu shows the change itself.
        set_video_mode(on);
    }

    fn set_fullscreen(&self, app: &mut App, on: bool) {
        set_fullscreen(app, on, false);
    }

    fn random_seed(&self) -> u32 {
        (js_sys::Math::random() * 4_294_967_296.0) as u32
    }

    fn show(&self, app: &App) {
        show_frame(app);
    }

    // The next step comes with a later animation frame, by which time the
    // event may have come: `idle` does nothing.
}

// ---- Sound

/// Starts Web Audio pulling the mixer, or None if the browser won't.
fn start_audio(mixer: *mut Mixer) -> Option<AudioContext> {
    let opts = AudioContextOptions::new();
    opts.set_sample_rate(SAMPLE_RATE);
    let ctx = AudioContext::new_with_context_options(&opts).ok()?;
    // A ScriptProcessorNode, as Emscripten's SDL uses: it runs on the page's
    // thread, between frames, so the mixer is never used from two threads.
    let node = ctx
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            AUDIO_FRAMES,
            0,
            1,
        )
        .ok()?;
    let mut samples = vec![0i16; AUDIO_FRAMES as usize];
    let mut floats = vec![0f32; AUDIO_FRAMES as usize];
    let on_audio = Closure::<dyn FnMut(AudioProcessingEvent)>::new(move |e: AudioProcessingEvent| {
        let Ok(out) = e.output_buffer() else { return };
        let n = out.length() as usize;
        samples.resize(n, 0);
        floats.resize(n, 0.0);
        // SAFETY: the mixer lives as long as the page, and nothing else
        // uses it during this call.
        unsafe { (*mixer).mix(&mut samples) };
        for (f, &s) in floats.iter_mut().zip(&samples) {
            *f = f32::from(s) / 32768.0;
        }
        let _ = out.copy_to_channel(&floats, 0);
    });
    node.set_onaudioprocess(Some(on_audio.as_ref().unchecked_ref()));
    on_audio.forget();
    node.connect_with_audio_node(&ctx.destination()).ok()?;
    Some(ctx)
}

// ---- Saves

/// Gives the page every save that changed since the last call.
fn persist_changes() {
    let Some(persist) = shell(|s| s.persist.clone()) else {
        return;
    };
    for (path, data) in crate::disk::take_changes() {
        // Only the config folder's files are saves; the TC is bundled.
        let Some(rest) = path.strip_prefix(b"liero/") else {
            continue;
        };
        let path = JsValue::from_str(&format!("/liero/{}", js_path(rest)));
        let data = data.map_or(JsValue::NULL, |d| js_sys::Uint8Array::from(d.as_slice()).into());
        if let Err(e) = persist.call2(&JsValue::NULL, &path, &data) {
            web_sys::console::warn_2(&"Saving failed:".into(), &e);
        }
    }
}

/// A file name's bytes as the page keys it: one character per byte, so
/// DOS characters (ö is 0x94) come back as they were.
fn js_path(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// The bytes `js_path` made `path` from. The Emscripten build's names,
/// copied once, can have other characters; those become `?`.
fn path_bytes(path: &str) -> Vec<u8> {
    path.chars().map(|c| u8::try_from(c).unwrap_or(b'?')).collect()
}

/// Puts a save into the files before the game starts, with its folders.
/// `path` is from the page's root, as `/liero/liero.cfg`.
#[wasm_bindgen]
pub fn liero_web_add_save(path: &str, data: &[u8]) {
    set_up_files();
    let path = path_bytes(path);
    let parts: Vec<&[u8]> = path.split(|&c| c == b'/').filter(|p| !p.is_empty()).collect();
    if parts.first() != Some(&&b"liero"[..]) {
        return;
    }
    let mut dir = Vec::new();
    for part in &parts[..parts.len() - 1] {
        dir.push(b'/');
        dir.extend_from_slice(part);
        crate::disk::create_dir(&dir);
    }
    crate::disk::write(&path, data);
    // Loaded, not changed.
    crate::disk::take_changes();
}

/// Gives the page the saves that changed, as when the tab is hidden.
#[wasm_bindgen]
pub fn liero_web_flush() {
    persist_changes();
}

/// The files: a tree in memory with the bundled TC, made once.
fn set_up_files() {
    if crate::disk::uses_tree() {
        return;
    }
    crate::disk::use_tree();
    put_tc();
    crate::disk::create_dir(CONFIG);
    crate::disk::take_changes();
}

/// Puts the bundled TC into the tree, at /TC/openliero.
fn put_tc() {
    for dir in [&b"/TC"[..], b"/TC/openliero"] {
        crate::disk::create_dir(dir);
    }
    for (path, data) in TC_FILES {
        let mut dir = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            dir.push('/');
            dir.push_str(part);
            crate::disk::create_dir(dir.as_bytes());
        }
        crate::disk::write(format!("/{path}").as_bytes(), data);
    }
}

/// The hash run on this build (`tools/hashrun/run.ts --wasm`), with the
/// files in a tree holding only the TC, as the native run's folder does.
/// `out` is called with each line it prints. `only` is a comma-separated
/// list of sections, or empty for all.
#[cfg(feature = "hashrun-export")]
#[wasm_bindgen]
pub fn liero_web_hash_run(ticks: i32, interval: i32, only: &str, out: &js_sys::Function) -> Result<(), JsValue> {
    report_panics();
    crate::disk::use_tree();
    put_tc();
    let only: Vec<String> = only.split(',').filter(|n| !n.is_empty()).map(str::to_owned).collect();
    crate::hashrun::hash_run(ticks, interval, &only, &mut |line| {
        out.call1(&JsValue::NULL, &line.into()).expect("can't print a line");
    })
    .map_err(|e| JsValue::from_str(&e))
}

// ---- Starting

fn report_panics() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("The game stopped: {info}").into());
    }));
}

/// Starts the game on `canvas`, as `main` did: the settings and the TC, the
/// window, sound and the screens, run every animation frame until the
/// player quits. `persist(path, bytes or null)` keeps a save that changed.
/// Called in the click that starts the game, so the browser lets it play
/// sound and go full screen.
#[wasm_bindgen]
pub fn liero_web_start(canvas: HtmlCanvasElement, persist: js_sys::Function) -> Result<(), JsValue> {
    report_panics();
    web_sys::console::log_1(&crate::VERSION.into());
    set_up_files();

    let ctx = canvas
        .get_context("2d")?
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
        .ok_or("no 2D canvas")?;
    shell(|s| {
        s.canvas = Some(canvas);
        s.ctx = Some(ctx);
        s.persist = Some(persist);
        s.start_ms = now_ms();
    });

    let mut app = App::new();
    let a = &mut app;
    // TODO from the C++: better seeding.
    a.rand.seed((js_sys::Date::now() / 1000.0) as u32);
    let menus = Menus::shared();
    menus.borrow_mut().load();
    a.primary_single = false;
    a.menus = Some(menus);
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    // The browser can close the tab at any time, so the menu saves liero.cfg
    // when the settings change.
    a.save_settings_on_change = true;
    a.start(CONFIG, TC_ROOT, b"").map_err(|e| JsValue::from_str(&e))?;
    // The page has one canvas, so there is no second window to show.
    a.set_spectator_window(false);
    set_video_mode(a.double_res);
    if a.fullscreen() {
        show_fullscreen(true);
    }

    // Kept for the page's life.
    let mixer: *mut Mixer = Box::into_raw(Box::default());
    match start_audio(mixer) {
        Some(audio) => shell(|s| {
            s.mixer = Some(mixer);
            s.audio = Some(audio);
        }),
        None => web_sys::console::warn_1(&"No sound: the browser didn't start Web Audio.".into()),
    }

    if let Some(doc) = window().document() {
        for kind in ["fullscreenchange", "fullscreenerror"] {
            let f = Closure::<dyn FnMut()>::new(on_fullscreen_change);
            doc.add_event_listener_with_callback(kind, f.as_ref().unchecked_ref())?;
            f.forget();
        }
    }
    for (kind, down) in [("keydown", true), ("keyup", false)] {
        let f = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| on_key(&e, down));
        window().add_event_listener_with_callback(kind, f.as_ref().unchecked_ref())?;
        f.forget();
    }

    let platform: Rc<dyn Platform> = Rc::new(WebPlatform);
    let mut stack = ScreenStack::new();
    stack.push(main_screen());
    run_frames(stack, Host::new(platform), app);
    Ok(())
}

/// The function each animation frame calls.
type FrameFn = Closure<dyn FnMut()>;

/// Runs the screens on `app` every animation frame (as the C++ shell's
/// `emscripten_set_main_loop` did), with the 14 ms steps due, until they
/// close; then saves the settings, as the game does when it ends.
fn run_frames(stack: ScreenStack, host: Host, mut app: App) {
    let mut stack = Some(stack);
    let next: Rc<RefCell<Option<FrameFn>>> = Rc::new(RefCell::new(None));
    let again = next.clone();
    *next.borrow_mut() = Some(Closure::new(move || {
        poll_gamepads();
        let ms = ticks_now();
        let Some(s) = stack.as_mut() else { return };
        if shell(|s| std::mem::take(&mut s.left_fullscreen)) {
            left_fullscreen(&mut app);
        }
        let running = s.frame(ms, &mut app, &host);
        for id in shell(|s| std::mem::take(&mut s.sounds)) {
            // SAFETY: the mixer is the shell's, or null.
            unsafe { app.play_menu_sound(mixer(), id) };
        }
        if running {
            persist_changes();
            if let Some(f) = again.borrow().as_ref() {
                let _ = window().request_animation_frame(f.as_ref().unchecked_ref());
            }
            return;
        }
        stack = None;
        if let Err(e) = app.save_settings_at_end() {
            web_sys::console::error_1(&e.into());
        }
        persist_changes();
        if let Some(audio) = shell(|s| s.audio.take()) {
            let _ = audio.close();
        }
        // Drops this closure after it returns.
        let _ = again.borrow_mut().take().map(|f| f.forget());
    }));
    if let Some(f) = next.borrow().as_ref() {
        let _ = window().request_animation_frame(f.as_ref().unchecked_ref());
    };
}
