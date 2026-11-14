//! The weapon selection screen before a game (`weapsel.cpp`): picking each
//! worm's starting weapons, changing them with the worm's controls, and
//! drawing the menus. Results have to stay bit-identical to the C++
//! versions, which `tools/hashrun` checks.
//!
//! Rust owns the menus and which worms are ready (`Weapsel`); C++ owns
//! the worms and their settings. Weapons are numbered as the menus list them,
//! from 1; `weap_order` turns a number into a weapon. Hooks read and clear the
//! worms' controls, play sounds and give the worms their weapons.

use std::ffi::c_void;

use crate::app::App;
use crate::game_box::Game;
use crate::gfx::{draw_rounded_box, palette_fade, palette_rotate_from, Bitmap};
use crate::menu::draw_versus;
use crate::menu::{Menu, MenuHooks, MenuItem};
use crate::rand::Rand;
use crate::renderer::Renderer;
use crate::sfx::SoundOut;
use crate::tc::s as text;
use crate::tc::TcData;
use crate::text::Font;
use crate::weapon::LIERO_SELECTABLE_WEAPONS;
use crate::worm::{
    WormData, LIERO_CONTROL_DOWN, LIERO_CONTROL_FIRE, LIERO_CONTROL_LEFT, LIERO_CONTROL_RIGHT, LIERO_CONTROL_UP,
};

/// Size of `Settings::weapTable`.
pub const LIERO_WEAPON_TABLE_SIZE: usize = 40;

/// The menu item that picks random weapons, and the one that finishes.
const RANDOMIZE_ITEM: i32 = 0;
const DONE_ITEM: i32 = 6;

/// One worm's weapon menu.
struct WeapselMenu {
    /// "Randomize", the five weapons and "Done".
    menu: Menu,
    /// The worm's chosen weapon numbers (`WormSettings::weapons`).
    weapons: *mut [u32; LIERO_SELECTABLE_WEAPONS],
    /// The worm, whose controls pick and which is given the weapons.
    worm: *mut WormData,
    /// The worm has chosen.
    ready: bool,
    name: Vec<u8>,
    color: i32,
}

/// Weapon selection: each viewport's worm's menu (`WeaponSelection`).
pub struct Weapsel {
    menus: Vec<WeapselMenu>,
    /// Each weapon's name, by weapon index.
    weapon_names: Vec<Vec<u8>>,
    /// The game whose worms choose, when started from one
    /// (`Weapsel::start`).
    game: *mut Game,
    /// What's behind the menus has been drawn and frozen, on the play and
    /// the spectator screen.
    cached: bool,
    cached_spectator: bool,
    /// Shown, not behind the menu.
    focused: bool,
}

/// A worm choosing weapons.
pub struct WeapselWorm {
    /// The worm in the viewport.
    pub worm: *mut WormData,
    /// `WormSettings::weapons`, which the menu changes in place.
    pub weapons: *mut [u32; LIERO_SELECTABLE_WEAPONS],
    pub name: Vec<u8>,
    pub color: i32,
    /// The centre of the worm's viewport.
    pub center_x: i32,
    pub center_y: i32,
    /// Starts with random weapons (a bot that doesn't pick).
    pub random: bool,
    /// Has already chosen (a bot that doesn't pick at all).
    pub ready: bool,
}

/// What weapon selection starts with.
pub struct WeapselSpec<'a> {
    pub worms: &'a [WeapselWorm],
    /// Each weapon's name, by weapon index.
    pub weapon_names: Vec<Vec<u8>>,
    pub randomize: &'a [u8],
    pub done: &'a [u8],
}

/// The game's weapon tables, the random numbers the menus use, and their
/// sounds.
pub struct WeapselEnv<'a> {
    /// `Settings::weapTable`: 0 when a weapon can be chosen.
    pub table: [u32; LIERO_WEAPON_TABLE_SIZE],
    /// `Common::weapOrder`: the weapon for each number, minus one.
    pub order: &'a [i32],
    pub weapon_count: i32,
    /// `gfx.rand`.
    pub rand: &'a mut Rand,
    /// The menus' sounds, which play on `sound`.
    pub tc: Option<&'a TcData>,
    pub sound: SoundOut,
}

impl WeapselEnv<'_> {
    /// Plays a menu sound (`soundPlayer->play`).
    ///
    /// # Safety
    /// The sound's mixer is valid.
    unsafe fn play(&self, sound: i32) {
        if let Some(tc) = self.tc {
            unsafe { self.sound.play(tc, sound, std::ptr::null_mut(), 0) };
        }
    }

    /// The weapon with number `n`, from 1. Numbers outside the list read
    /// other memory in C++; they give weapon 0 here.
    fn weapon(&self, n: u32) -> i32 {
        (n as usize)
            .checked_sub(1)
            .and_then(|i| self.order.get(i))
            .copied()
            .unwrap_or(0)
    }

    /// Whether weapon number `n` can be chosen.
    fn allowed(&self, n: u32) -> bool {
        self.table.get(self.weapon(n) as usize).is_some_and(|&t| t == 0)
    }

    fn random_number(&mut self) -> u32 {
        // C++ always used 1 to 40 here, whatever the TC's weapon count.
        self.rand.range(1, 41)
    }

    fn enough_weapons(&self) -> bool {
        self.table.iter().filter(|&&t| t == 0).count() >= LIERO_SELECTABLE_WEAPONS
    }

    /// Picks random allowed weapons, each different if there are enough.
    /// Returns the weapons.
    fn randomize(&mut self, weapons: &mut [u32]) -> [i32; LIERO_SELECTABLE_WEAPONS] {
        let mut used = [false; 256];
        let enough = self.enough_weapons();
        let mut out = [0; LIERO_SELECTABLE_WEAPONS];
        for (j, n) in weapons.iter_mut().enumerate() {
            loop {
                *n = self.random_number();
                let w = self.weapon(*n);
                if (!enough || !used[w as usize & 255]) && self.allowed(*n) {
                    break;
                }
            }
            let w = self.weapon(*n);
            used[w as usize & 255] = true;
            out[j] = w;
        }
        out
    }

    /// The next allowed weapon number from `n` in `dir`, wrapping around.
    fn step(&self, mut n: u32, dir: i32) -> u32 {
        // C++ looped until it found one; stop after a full turn.
        for _ in 0..=self.weapon_count.max(0) {
            if dir < 0 {
                n = n.wrapping_sub(1);
                if n < 1 {
                    n = self.weapon_count as u32;
                }
            } else {
                n = n.wrapping_add(1);
                if n > self.weapon_count as u32 {
                    n = 1;
                }
            }
            if self.allowed(n) {
                break;
            }
        }
        n
    }
}

/// A worm's starting weapons, from the constructor of `WeaponSelection`:
/// keeps `weapons` (5 numbers) where allowed and replaces the rest, or all
/// of them if `random`, with random ones that differ if there are enough.
/// Returns the weapons.
fn choose(
    weapons: &mut [u32; LIERO_SELECTABLE_WEAPONS],
    random: bool,
    e: &mut WeapselEnv,
) -> [i32; LIERO_SELECTABLE_WEAPONS] {
    let mut out = [0; LIERO_SELECTABLE_WEAPONS];
    let mut used = [false; 256];
    let enough = e.enough_weapons();

    for (j, n) in weapons.iter_mut().enumerate() {
        if *n == 0 || random {
            *n = e.random_number();
        }
        if !e.allowed(*n) {
            loop {
                *n = e.random_number();
                let w = e.weapon(*n);
                if (!enough || !used[w as usize & 255]) && e.allowed(*n) {
                    break;
                }
            }
        }
        let w = e.weapon(*n);
        used[w as usize & 255] = true;
        out[j] = w;
    }
    out
}

fn item(name: &[u8], color: u8) -> MenuItem {
    MenuItem::new(name, color, color)
}

/// `Worm::pressed`.
fn pressed(worm: &WormData, control: u32) -> bool {
    worm.control_states >> control & 1 != 0
}

/// `Worm::release`.
fn release(worm: &mut WormData, control: u32) {
    worm.control_states &= !(1 << control);
}

/// `Worm::pressedOnce`: whether it was pressed, then clears it.
fn pressed_once(worm: &mut WormData, control: u32) -> bool {
    let p = pressed(worm, control);
    release(worm, control);
    p
}

/// Gives the worm its chosen weapons (`numbers`, from its settings),
/// loaded, with the first current (`WeaponSelection::finalize`,
/// `Worm::initWeapons`), and releases its controls.
pub fn finalize(worm: &mut WormData, numbers: &[u32; LIERO_SELECTABLE_WEAPONS], tc: &TcData) {
    worm.current_weapon = 0; // It was 1 in OpenLiero A1
    for (ww, &n) in worm.weapons.iter_mut().zip(numbers) {
        let w = tc.weapon_order()[(n as usize).wrapping_sub(1)];
        let t = &tc.model.weapons[w as usize];
        ww.kind = w;
        ww.ammo = t.ammo;
        ww.delay_left = 0;
        ww.loading_left = 0;
    }
    // `Game::releaseControls`: the seven controls, not dig.
    worm.control_states &= !0x7f;
}

unsafe fn no_overlay(_: *mut c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}

fn draw_menu(wm: &mut WeapselMenu, scr: &Bitmap, font: &Font, x: i32) {
    let hooks = MenuHooks {
        ctx: std::ptr::null_mut(),
        draw_overlay: no_overlay,
    };
    wm.menu.draw_with(scr, font, &hooks, false, x, false);
}

/// The weapon tables of `game`, with the app's random numbers.
///
/// # Safety
/// The game has its context.
unsafe fn game_env<'a>(game: &'a Game, rand: &'a mut Rand) -> WeapselEnv<'a> {
    let tc = unsafe { &*game.tc_ptr() };
    WeapselEnv {
        table: game.settings_box().data.weap_table,
        order: &tc.weap_order,
        weapon_count: tc.model.weapons.len() as i32,
        rand,
        tc: Some(tc),
        sound: game.sound(),
    }
}

/// The TC's font.
pub(crate) fn tc_font(tc: &TcData) -> Font<'_> {
    Font { chars: &tc.model.font }
}

/// The part of a path after the last slash (`getLeaf`).
pub(crate) fn leaf(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|&c| c == b'/' || c == b'\\')
        .map_or(path, |i| &path[i + 1..])
}

/// A file name without what follows its last dot (`getBasename`).
pub(crate) fn basename(name: &[u8]) -> &[u8] {
    name.iter().rposition(|&c| c == b'.').map_or(name, |i| &name[..i])
}

/// The line naming the level: random if the game's settings have no level
/// file, and otherwise the menus' level file's name, as C++ took it.
pub(crate) fn level_line(tc: &TcData, game_level_file: &[u8], menu_level_file: &[u8]) -> Vec<u8> {
    if game_level_file.is_empty() {
        tc.text(text("LevelRandom")).to_vec()
    } else {
        [
            tc.text(text("LevelIs1")),
            basename(leaf(menu_level_file)),
            tc.text(text("LevelIs2")),
        ]
        .concat()
    }
}

/// The renderer's palette as the menus show it: cycling and fading.
unsafe fn menu_palette(r: *mut Renderer, menu_cycles: u32) {
    unsafe {
        let d = &mut (*r).data;
        d.pal = d.origpal;
        palette_rotate_from(&mut d.pal, &d.origpal, 168, 174, menu_cycles);
        palette_fade(&mut d.pal, d.fade_value);
    }
}

impl Weapsel {
    /// Starts weapon selection (`WeaponSelection::WeaponSelection`): each
    /// worm's menu below its viewport's centre, with its starting weapons,
    /// which the worm is given, empty, with its first weapon current.
    ///
    /// # Safety
    /// `spec`'s worms and their `weapons` are valid and outlive the
    /// selection, and the sound's mixer is valid.
    pub unsafe fn new(spec: WeapselSpec, env: &mut WeapselEnv) -> Box<Weapsel> {
        let mut menus = Vec::new();
        for w in spec.worms {
            // Menu(), placed later, with items it doesn't count as visible.
            let mut menu = Menu::raw(0, 0, false);
            menu.add_at(item(spec.randomize, 57), None, false);
            menu.m.x = w.center_x - 31;
            menu.m.y = w.center_y - 51;

            // SAFETY: the worm and its weapons are valid, as the caller says.
            let (worm, weapons) = unsafe { (&mut *w.worm, &mut *w.weapons) };
            let chosen = choose(weapons, w.random, env);
            for (j, &weapon) in chosen.iter().enumerate() {
                let ww = &mut worm.weapons[j];
                ww.ammo = 0;
                ww.kind = weapon;
                worm.current_weapon = 0;
                menu.add_at(item(&spec.weapon_names[weapon as usize], 48), None, false);
            }
            menu.add_at(item(spec.done, 10), None, false);
            menu.move_to(0);

            menus.push(WeapselMenu {
                menu,
                weapons: w.weapons,
                worm: w.worm,
                ready: w.ready,
                name: w.name.clone(),
                color: w.color,
            });
        }
        Box::new(Weapsel {
            menus,
            weapon_names: spec.weapon_names,
            game: std::ptr::null_mut(),
            cached: false,
            cached_spectator: false,
            focused: true,
        })
    }

    /// Starts weapon selection for `game`'s split-screen viewports' worms
    /// (`WeaponSelection::WeaponSelection`): bots that don't choose take
    /// random weapons, or their own, as the settings say.
    ///
    /// # Safety
    /// The game has its context, and outlives the selection; each
    /// viewport's worm is listed and has settings.
    pub unsafe fn start(game: &mut Game, rand: &mut Rand) -> Box<Weapsel> {
        let g: *mut Game = game;
        let select = game.settings_box().data.ext.select_bot_weapons;
        let viewports: Vec<(i32, i32, i32)> = game
            .split_viewports()
            .iter()
            .map(|vp| {
                (
                    vp.worm_idx,
                    (vp.rect.x1 + vp.rect.x2) / 2,
                    (vp.rect.y1 + vp.rect.y2) / 2,
                )
            })
            .collect();
        let mut worms = Vec::new();
        for (idx, center_x, center_y) in viewports {
            let w = game.worm_at(idx as usize).expect("an empty place in a viewport");
            let worm: *mut WormData = &mut w.data;
            let mut ws = w.settings_mut();
            let bot = ws.data.controller != 0;
            worms.push(WeapselWorm {
                worm,
                weapons: &mut ws.data.weapons,
                name: ws.data.name.clone(),
                color: ws.data.color,
                center_x,
                center_y,
                random: bot && select == 0,
                ready: bot && select != 1,
            });
        }
        // SAFETY: the game has its context.
        let tc = unsafe { &*game.tc_ptr() };
        let spec = WeapselSpec {
            worms: &worms,
            weapon_names: tc.model.weapons.iter().map(|w| w.name.clone()).collect(),
            randomize: tc.text(text("Randomize")),
            done: tc.text(text("Done")),
        };
        let mut env = unsafe { game_env(game, rand) };
        // SAFETY: the worms and their settings are the game's, which
        // outlives the selection.
        let mut ws = unsafe { Weapsel::new(spec, &mut env) };
        ws.game = g;
        ws
    }

    pub fn menu_count(&self) -> usize {
        self.menus.len()
    }

    /// Menu `i`, for the hash run to read.
    pub fn menu(&mut self, i: usize) -> &mut Menu {
        &mut self.menus[i].menu
    }

    pub fn ready(&self, i: usize) -> bool {
        self.menus[i].ready
    }

    /// Hides the menus (the game went behind the menu), or shows them again.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// One frame of weapon selection: `WeaponSelection::processFrame`. Each
    /// worm not ready changes the selected weapon with left and right,
    /// moves with up and down, and picks random weapons or finishes with
    /// fire. Returns whether every worm is ready.
    ///
    /// # Safety
    /// The worms and their weapons are valid, and the sound's mixer.
    pub unsafe fn process(&mut self, e: &mut WeapselEnv) -> bool {
        let mut all_ready = true;

        for wm in self.menus.iter_mut() {
            // SAFETY: as the caller says.
            let (worm, weapons) = unsafe { (&mut *wm.worm, &mut *wm.weapons) };
            // Item names to change, once the menu isn't borrowed.
            let mut renames = Vec::new();
            let ready = &mut wm.ready;
            let menu = &mut wm.menu;
            (|| {
                let slot = menu.m.selection - 1;

                if *ready {
                    return;
                }
                if (0..LIERO_SELECTABLE_WEAPONS as i32).contains(&slot) {
                    for (control, sound, dir) in [(LIERO_CONTROL_LEFT, 25, -1), (LIERO_CONTROL_RIGHT, 26, 1)] {
                        if pressed(worm, control) {
                            release(worm, control);
                            unsafe { e.play(sound) };
                            let n = &mut weapons[slot as usize];
                            *n = e.step(*n, dir);
                            let w = e.weapon(*n);
                            worm.weapons[slot as usize].kind = w;
                            renames.push((slot, w));
                        }
                    }
                }

                if pressed_once(worm, LIERO_CONTROL_UP) {
                    unsafe { e.play(26) };
                    menu.movement(-1);
                }
                if pressed_once(worm, LIERO_CONTROL_DOWN) {
                    unsafe { e.play(25) };
                    menu.movement(1);
                }

                if pressed(worm, LIERO_CONTROL_FIRE) {
                    if menu.m.selection == RANDOMIZE_ITEM {
                        for (j, w) in e.randomize(weapons).into_iter().enumerate() {
                            renames.push((j as i32, w));
                        }
                    } else if menu.m.selection == DONE_ITEM {
                        unsafe { e.play(27) };
                        *ready = true;
                    }
                }
            })();
            for (slot, w) in renames {
                wm.menu.items[slot as usize + 1].name = self.weapon_names[w as usize].clone();
            }

            all_ready = all_ready && wm.ready;
        }

        all_ready
    }

    /// `processFrame` for a selection started from a game.
    ///
    /// # Safety
    /// See `start`.
    pub unsafe fn process_game(&mut self, rand: &mut Rand) -> bool {
        unsafe {
            let mut env = game_env(&*self.game, rand);
            self.process(&mut env)
        }
    }

    /// `finalize` for a selection started from a game: gives each of the
    /// game's worms the weapons its settings chose.
    ///
    /// # Safety
    /// See `start`; every listed worm has settings.
    pub unsafe fn finish(&mut self) {
        let game = unsafe { &mut *self.game };
        let tc = game.tc_ptr();
        for w in game.listed_worms() {
            let w = w.expect("an empty place among the game's worms");
            let numbers = w.settings().data.weapons;
            // SAFETY: the TC isn't the worms'.
            finalize(&mut w.data, &numbers, unsafe { &*tc });
        }
    }

    /// The split-screen weapon selection over the frozen game: the title,
    /// each worm's name and the menus of worms not ready. `menu_scr` is
    /// where the menus go, as C++ drew them on the play screen.
    pub fn draw(&mut self, scr: &Bitmap, menu_scr: &Bitmap, font: &Font, title: &[u8]) {
        draw_rounded_box(scr, 114, 2, 0, 7, font.dims(title).0);
        font.text(scr, title, 116, 3, 50, 1);

        for wm in &mut self.menus {
            let width = font.dims(&wm.name).0;
            let (x, y) = (wm.menu.m.x, wm.menu.m.y);
            draw_rounded_box(scr, x + 29 - width / 2, y - 11, 0, 7, width);
            font.text(scr, &wm.name, x + 31 - width / 2, y - 10, wm.color + 1, 1);
            if !wm.ready {
                draw_menu(wm, menu_scr, font, -1);
            }
        }
    }

    /// The spectator window's weapon selection: the first two worms' menus,
    /// at the sides, while not ready.
    pub fn draw_spectator(&mut self, scr: &Bitmap, font: &Font) {
        for (wm, x) in self.menus.iter_mut().zip([10, 560]) {
            if !wm.ready {
                draw_menu(wm, scr, font, x);
            }
        }
    }

    /// `WeaponSelection::draw` for a selection started from a game: on the
    /// spectator screen, who plays whom and the level, and on the play screen
    /// the frozen game, with the menus over it while focused. The palette
    /// changes are the app's renderers', whichever `r` is.
    ///
    /// # Safety
    /// See `start`; `r` is valid.
    pub unsafe fn draw_game(&mut self, a: &mut App, r: *mut Renderer, state: i32, spectators: bool) {
        unsafe {
            let game = &mut *self.game;
            let tc = &*game.tc_ptr();
            let font = tc_font(tc);
            let g: *const Game = game;
            let menu_settings = a.shared_settings();
            // The game's settings can be the menus' own, so one at a time.
            let line = || {
                let game_file = (*g).settings_box().data.level_file.clone();
                let menu_file = menu_settings.borrow().data.level_file.clone();
                level_line(tc, &game_file, &menu_file)
            };
            if spectators {
                let (cx, cy) = ((*r).data.render_res_x / 2, (*r).data.render_res_y / 4);
                if !self.cached_spectator {
                    let scr = (*r).bmp.view();
                    let w = game.settings_box().data.worms.clone();
                    let w = [w[0].borrow(), w[1].borrow()];
                    draw_versus(
                        &scr,
                        &font,
                        &line(),
                        &w[0].data.name,
                        &w[1].data.name,
                        w[0].data.color,
                        w[1].data.color,
                        cx,
                        cy,
                    );
                    font.centered(&scr, b"WEAPON SELECTION", cx, cy + 48, 7, 2);
                    game.level_box()
                        .draw_miniature(&scr, cx - 126, (*r).data.render_res_y - 208, 2);
                    a.frozen_spectator.copy_from(&(*r).bmp);
                    self.cached_spectator = true;
                }
                (*r).bmp.copy_from(&a.frozen_spectator);
                if !self.focused {
                    return;
                }
                self.draw_spectator(&(*r).bmp.view(), &font);
                menu_palette(&mut *a.single, a.menu_cycles);
            } else {
                if !self.cached {
                    game.draw(&mut *r, state, false, false);
                    font.text(&(*r).bmp.view(), &line(), 0, 162, 50, 1);
                    a.frozen.copy_from(&(*r).bmp);
                    self.cached = true;
                }
                (*r).bmp.copy_from(&a.frozen);
                if !self.focused {
                    return;
                }
                // C++ drew the menus on the play screen, whatever `r` was.
                let menu_scr = a.play.bmp.view();
                self.draw(&(*r).bmp.view(), &menu_scr, &font, tc.text(text("SelWeap")));
                menu_palette(&mut *a.play, a.menu_cycles);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liero_data::model::Weapon;

    #[test]
    fn the_level_line_names_the_menus_level_file() {
        let mut tc = TcData::empty();
        tc.model.texts = liero_data::exe::TEXTS
            .iter()
            .map(|&(n, _)| (n, n.as_bytes().to_vec()))
            .collect();
        assert_eq!(level_line(&tc, b"", b"a/b.lev"), b"LevelRandom");
        assert_eq!(level_line(&tc, b"x", b"C:\\lev/a.b.lev"), b"LevelIs1a.bLevelIs2");
        assert_eq!(level_line(&tc, b"x", b"dir.d\\name"), b"LevelIs1nameLevelIs2");
        assert_eq!(
            (leaf(b"a/b\\c"), leaf(b"abc"), leaf(b"a/")),
            (&b"c"[..], &b"abc"[..], &b""[..])
        );
        assert_eq!(
            (basename(b"a.b.c"), basename(b"abc"), basename(b".x")),
            (&b"a.b"[..], &b"abc"[..], &b""[..])
        );
    }
    use crate::rand::LIERO_RAND_DEFAULT_SEED;

    fn env<'a>(table: &[u32; 40], order: &'a [i32], rand: &'a mut Rand) -> WeapselEnv<'a> {
        WeapselEnv {
            table: *table,
            order,
            weapon_count: order.len() as i32,
            rand,
            tc: None,
            sound: SoundOut::NONE,
        }
    }

    #[test]
    fn starting_weapons_are_allowed_and_different() {
        let order: Vec<i32> = (0..40).rev().collect();
        let mut table = [0u32; 40];
        for t in table.iter_mut().skip(5) {
            *t = 1;
        }
        let mut rand = Rand::new(LIERO_RAND_DEFAULT_SEED);
        // Numbers 36 to 40 are weapons 4 to 0, the only ones allowed.
        let mut weapons = [40, 1, 2, 40, 39];
        let out = choose(&mut weapons, false, &mut env(&table, &order, &mut rand));
        assert_eq!((weapons[0], out[0]), (40, 0), "allowed, so kept");
        assert_eq!(weapons[3], 40, "kept even though weapon 0 is taken");
        assert!(out.iter().all(|&w| w < 5));
        let mut first_three = out[..3].to_vec();
        first_three.sort();
        first_three.dedup();
        assert_eq!(first_three.len(), 3, "replaced ones differ from earlier ones");
    }

    #[test]
    fn left_and_right_skip_disabled_weapons() {
        let order: Vec<i32> = (0..40).collect();
        let mut table = [0u32; 40];
        table[0] = 1;
        table[39] = 2;
        let mut rand = Rand::new(1);
        let e = env(&table, &order, &mut rand);
        assert_eq!(e.step(2, -1), 39, "wraps past the disabled first and last");
        assert_eq!(e.step(39, 1), 2);
        assert_eq!(e.step(5, 1), 6);
    }

    thread_local! {
        static SOUNDS: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    unsafe fn report(sound: i32) {
        SOUNDS.with(|s| s.borrow_mut().push(sound));
    }

    /// A TC with `n` weapons, each with ammo 10 + its index.
    /// Boxed: it is too big for the wasm tests' stack.
    fn tc(mut weapons: Vec<Weapon>, order: &[i32]) -> Box<TcData> {
        for (i, w) in weapons.iter_mut().enumerate() {
            w.ammo = 10 + i as i32;
        }
        let mut tc = TcData::empty();
        tc.model.weapons = weapons;
        tc.weap_order = order.to_vec();
        tc
    }

    #[test]
    fn menus_play_sounds_and_set_the_worm() {
        let order: Vec<i32> = (0..40).collect();
        let weapons: Vec<Weapon> = (0..40).map(|_| Weapon::default()).collect();
        let tc = tc(weapons, &order);
        let table = [0u32; 40];
        let mut rand = Rand::new(1);
        let mut raw = env(&table, &order, &mut rand);
        raw.tc = Some(&tc);
        raw.sound = SoundOut {
            mixer: std::ptr::null_mut(),
            report: Some(report),
        };

        let mut worm = WormData::default();
        let mut numbers = [1u32, 2, 3, 4, 5];
        let spec_worm = WeapselWorm {
            worm: &mut worm,
            weapons: &mut numbers,
            name: b"W".to_vec(),
            color: 0,
            center_x: 100,
            center_y: 100,
            random: false,
            ready: false,
        };
        let spec = WeapselSpec {
            worms: std::slice::from_ref(&spec_worm),
            weapon_names: vec![b"W".to_vec(); 40],
            randomize: b"W",
            done: b"W",
        };
        unsafe {
            let mut ws = Weapsel::new(spec, &mut raw);
            assert_eq!(worm.weapons[2].kind, 2);
            assert_eq!(worm.weapons[2].ammo, 0);

            let mut press = |worm: &mut WormData, control: u32| {
                worm.control_states = 1 << control;
                ws.process(&mut raw);
            };
            press(&mut worm, LIERO_CONTROL_DOWN);
            assert_eq!(worm.control_states, 0, "pressed once");
            press(&mut worm, LIERO_CONTROL_RIGHT);
            assert_eq!(worm.control_states, 0, "released");
            assert_eq!(numbers[0], 2);
            assert_eq!(worm.weapons[0].kind, 1);
            press(&mut worm, LIERO_CONTROL_UP);
            press(&mut worm, LIERO_CONTROL_LEFT);
            assert_eq!(numbers[0], 2, "left and right only change weapons");
            SOUNDS.with(|s| assert_eq!(*s.borrow(), vec![25, 26, 26]));
        }
    }

    #[test]
    fn finalize_loads_the_weapons_and_releases_the_controls() {
        let order: Vec<i32> = (0..10).rev().collect();
        let weapons: Vec<Weapon> = (0..10).map(|_| Weapon::default()).collect();
        let tc = tc(weapons, &order);
        let mut worm = WormData {
            current_weapon: 3,
            control_states: 0xff,
            ..Default::default()
        };
        worm.weapons[1].delay_left = 5;
        worm.weapons[1].loading_left = 5;
        let numbers = [1u32, 2, 3, 4, 10];
        finalize(&mut worm, &numbers, &tc);
        assert_eq!(worm.control_states, 0x80, "dig stays");
        assert_eq!(worm.current_weapon, 0);
        assert_eq!(worm.weapons[0].kind, 9);
        assert_eq!(worm.weapons[0].ammo, 19);
        assert_eq!(worm.weapons[4].ammo, 10);
        assert_eq!((worm.weapons[1].delay_left, worm.weapons[1].loading_left), (0, 0));
    }
}
