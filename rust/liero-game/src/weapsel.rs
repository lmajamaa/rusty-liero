//! The weapon selection screen before a game (`weapsel.cpp`): picking each
//! worm's starting weapons, changing them with the worm's controls, and
//! drawing the menus. Results have to stay bit-identical to the C++
//! versions, which `tools/hashrun` checks.
//!
//! Rust owns the menus and which worms are ready (`LieroWeapsel`); C++ owns
//! the worms and their settings. Weapons are numbered as the menus list them,
//! from 1; `weap_order` turns a number into a weapon. Hooks read and clear the
//! worms' controls, play sounds and give the worms their weapons.

use std::ffi::c_void;

use crate::gfx::{liero_gfx_draw_rounded_box, LieroBitmap};
use crate::menu::{LieroMenuBox, LieroMenuHooks, LieroMenuItemSpec, Menu, OwnedMenu};
use crate::rand::LieroRand;
use crate::text::{LieroFont, LieroStr};
use crate::weapon::LIERO_SELECTABLE_WEAPONS;
use crate::worm::{LIERO_CONTROL_DOWN, LIERO_CONTROL_FIRE, LIERO_CONTROL_LEFT, LIERO_CONTROL_RIGHT, LIERO_CONTROL_UP};

/// Size of `Settings::weapTable`.
pub const LIERO_WEAPON_TABLE_SIZE: usize = 40;

/// The menu item that picks random weapons, and the one that finishes.
const RANDOMIZE_ITEM: i32 = 0;
const DONE_ITEM: i32 = 6;

/// One worm's weapon menu.
struct WeapselMenu {
    /// "Randomize", the five weapons and "Done".
    menu: LieroMenuBox,
    /// The worm's chosen weapon numbers (`WormSettings::weapons`), 5 of them.
    weapons: *mut u32,
    /// The worm has chosen.
    ready: bool,
    name: Vec<u8>,
    color: i32,
}

/// Weapon selection: each viewport's worm's menu (`WeaponSelection`).
pub struct LieroWeapsel {
    menus: Vec<WeapselMenu>,
    /// Each weapon's name, by weapon index.
    weapon_names: Vec<Vec<u8>>,
}

/// A worm choosing weapons.
#[repr(C)]
pub struct LieroWeapselWorm {
    /// `WormSettings::weapons`, which the menu changes in place.
    pub weapons: *mut u32,
    pub name: LieroStr,
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
#[repr(C)]
pub struct LieroWeapselSpec {
    pub worms: *const LieroWeapselWorm,
    pub worm_count: i32,
    /// Each weapon's name, by weapon index.
    pub weapon_names: *const LieroStr,
    pub weapon_count: i32,
    pub randomize: LieroStr,
    pub done: LieroStr,
}

/// The game's weapon tables.
#[repr(C)]
pub struct LieroWeapselEnv {
    /// `Settings::weapTable`, 40 entries: 0 when a weapon can be chosen.
    pub weap_table: *const u32,
    /// `Common::weapOrder`: the weapon for each number, minus one.
    pub weap_order: *const i32,
    pub weapon_count: i32,
    /// `gfx.rand`, which the menus use.
    pub rand: *mut LieroRand,
}

/// What C++ does for weapon selection. `menu` is a position in the menu list.
#[repr(C)]
pub struct LieroWeapselHooks {
    pub ctx: *mut c_void,
    /// `Worm::pressed`.
    pub pressed: unsafe extern "C" fn(ctx: *mut c_void, menu: i32, control: u32) -> bool,
    /// `Worm::release`.
    pub release: unsafe extern "C" fn(ctx: *mut c_void, menu: i32, control: u32),
    /// `Worm::pressedOnce`: whether it was pressed, then clears it.
    pub pressed_once: unsafe extern "C" fn(ctx: *mut c_void, menu: i32, control: u32) -> bool,
    pub play_sound: unsafe extern "C" fn(ctx: *mut c_void, sound: i32),
    /// Gives the worm weapon `weapon` in `slot`.
    pub set_weapon: unsafe extern "C" fn(ctx: *mut c_void, menu: i32, slot: i32, weapon: i32),
    /// Starts the worm with weapon `weapon` in `slot`, empty, and its first
    /// weapon current.
    pub init_weapon: unsafe extern "C" fn(ctx: *mut c_void, menu: i32, slot: i32, weapon: i32),
}

struct Env<'a> {
    table: &'a [u32],
    order: &'a [i32],
    weapon_count: i32,
    rand: &'a mut LieroRand,
}

impl<'a> Env<'a> {
    unsafe fn new(e: &'a LieroWeapselEnv) -> Self {
        let order = if e.weapon_count <= 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(e.weap_order, e.weapon_count as usize)
        };
        Env {
            table: std::slice::from_raw_parts(e.weap_table, LIERO_WEAPON_TABLE_SIZE),
            order,
            weapon_count: e.weapon_count,
            rand: &mut *e.rand,
        }
    }

    /// The weapon with number `n`, from 1. Numbers outside the list read
    /// other memory in C++; they give weapon 0 here.
    fn weapon(&self, n: u32) -> i32 {
        (n as usize).checked_sub(1).and_then(|i| self.order.get(i)).copied().unwrap_or(0)
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
/// Writes the weapons to `out`.
///
/// # Safety
/// See `LieroWeapselEnv`; `weapons` and `out` hold 5 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_choose(weapons: *mut u32, random: bool, env: &LieroWeapselEnv, out: *mut i32) {
    let mut e = Env::new(env);
    let weapons = std::slice::from_raw_parts_mut(weapons, LIERO_SELECTABLE_WEAPONS);
    let out = std::slice::from_raw_parts_mut(out, LIERO_SELECTABLE_WEAPONS);
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
}

fn item(name: &[u8], color: u8) -> LieroMenuItemSpec {
    LieroMenuItemSpec {
        string: LieroStr { ptr: name.as_ptr(), len: name.len() },
        value: LieroStr { ptr: name.as_ptr(), len: 0 },
        color,
        dis_colour: color,
        has_value: false,
        visible: true,
        selectable: true,
        id: -1,
    }
}

/// Starts weapon selection (`WeaponSelection::WeaponSelection`): each
/// worm's menu below its viewport's centre, with its starting weapons,
/// which the worm is given.
///
/// # Safety
/// `spec`'s pointers are valid for their counts, and see `LieroWeapselEnv`.
/// The worms' `weapons` outlive the selection.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_new(
    spec: &LieroWeapselSpec,
    env: &LieroWeapselEnv,
    hooks: &LieroWeapselHooks,
) -> *mut LieroWeapsel {
    let weapon_names: Vec<Vec<u8>> =
        (0..spec.weapon_count.max(0) as usize).map(|i| (*spec.weapon_names.add(i)).bytes().to_vec()).collect();
    let mut menus = Vec::new();
    for i in 0..spec.worm_count.max(0) as usize {
        let w = &*spec.worms.add(i);
        // Menu(), placed later, with items it doesn't count as visible.
        let mut menu = OwnedMenu::raw(0, 0, false);
        menu.insert(0, &item(spec.randomize.bytes(), 57));
        menu.m.x = w.center_x - 31;
        menu.m.y = w.center_y - 51;

        let mut chosen = [0i32; LIERO_SELECTABLE_WEAPONS];
        liero_weapsel_choose(w.weapons, w.random, env, chosen.as_mut_ptr());
        for (j, &weapon) in chosen.iter().enumerate() {
            (hooks.init_weapon)(hooks.ctx, i as i32, j as i32, weapon);
            menu.insert(menu.len(), &item(&weapon_names[weapon as usize], 48));
        }
        menu.insert(menu.len(), &item(spec.done.bytes(), 10));
        menu.move_to(0);

        menus.push(WeapselMenu {
            menu: LieroMenuBox(menu),
            weapons: w.weapons,
            ready: w.ready,
            name: w.name.bytes().to_vec(),
            color: w.color,
        });
    }
    Box::into_raw(Box::new(LieroWeapsel { menus, weapon_names }))
}

/// # Safety
/// `ws` came from `liero_weapsel_new` and isn't used after.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_free(ws: *mut LieroWeapsel) {
    if !ws.is_null() {
        drop(Box::from_raw(ws));
    }
}

#[no_mangle]
pub extern "C" fn liero_weapsel_menu_count(ws: &LieroWeapsel) -> i32 {
    ws.menus.len() as i32
}

/// Menu `i`, for the hash run to read.
#[no_mangle]
pub extern "C" fn liero_weapsel_menu(ws: &mut LieroWeapsel, i: i32) -> *mut LieroMenuBox {
    &mut ws.menus[i as usize].menu
}

#[no_mangle]
pub extern "C" fn liero_weapsel_ready(ws: &LieroWeapsel, i: i32) -> bool {
    ws.menus[i as usize].ready
}

/// One frame of weapon selection: `WeaponSelection::processFrame`. Each
/// worm not ready changes the selected weapon with left and right, moves
/// with up and down, and picks random weapons or finishes with fire.
/// Returns whether every worm is ready.
///
/// # Safety
/// See `LieroWeapselEnv`.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_process(
    ws: &mut LieroWeapsel,
    env: &LieroWeapselEnv,
    hooks: &LieroWeapselHooks,
) -> bool {
    let mut e = Env::new(env);
    let h = hooks;
    let mut all_ready = true;

    for (i, wm) in ws.menus.iter_mut().enumerate() {
        let i = i as i32;
        let weapons = std::slice::from_raw_parts_mut(wm.weapons, LIERO_SELECTABLE_WEAPONS);
        // Item names to change, once the menu isn't borrowed.
        let mut renames = Vec::new();
        let ready = &mut wm.ready;
        wm.menu.0.with(|m, items, n| {
            let items = if n <= 0 { &mut [][..] } else { std::slice::from_raw_parts_mut(items, n as usize) };
            let mut menu = Menu { m, items };
            let slot = menu.m.selection - 1;

            if *ready {
                return;
            }
            if (0..LIERO_SELECTABLE_WEAPONS as i32).contains(&slot) {
                for (control, sound, dir) in [(LIERO_CONTROL_LEFT, 25, -1), (LIERO_CONTROL_RIGHT, 26, 1)] {
                    if (h.pressed)(h.ctx, i, control) {
                        (h.release)(h.ctx, i, control);
                        (h.play_sound)(h.ctx, sound);
                        let n = &mut weapons[slot as usize];
                        *n = e.step(*n, dir);
                        let w = e.weapon(*n);
                        (h.set_weapon)(h.ctx, i, slot, w);
                        renames.push((slot, w));
                    }
                }
            }

            if (h.pressed_once)(h.ctx, i, LIERO_CONTROL_UP) {
                (h.play_sound)(h.ctx, 26);
                menu.movement(-1);
            }
            if (h.pressed_once)(h.ctx, i, LIERO_CONTROL_DOWN) {
                (h.play_sound)(h.ctx, 25);
                menu.movement(1);
            }

            if (h.pressed)(h.ctx, i, LIERO_CONTROL_FIRE) {
                if menu.m.selection == RANDOMIZE_ITEM {
                    for (j, w) in e.randomize(weapons).into_iter().enumerate() {
                        renames.push((j as i32, w));
                    }
                } else if menu.m.selection == DONE_ITEM {
                    (h.play_sound)(h.ctx, 27);
                    *ready = true;
                }
            }
        });
        for (slot, w) in renames {
            wm.menu.0.names[slot as usize + 1] = ws.weapon_names[w as usize].clone();
        }

        all_ready = all_ready && wm.ready;
    }

    all_ready
}

unsafe extern "C" fn no_overlay(_: *mut c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}

fn draw_menu(wm: &mut WeapselMenu, scr: &LieroBitmap, font: &LieroFont, x: i32) {
    let hooks = LieroMenuHooks { ctx: std::ptr::null_mut(), draw_overlay: no_overlay };
    wm.menu.0.draw_with(scr, font, &hooks, false, x, false);
}

/// The split-screen weapon selection over the frozen game: the title, each
/// worm's name and the menus of worms not ready. `menu_scr` is where the
/// menus go, as C++ drew them on the play screen.
///
/// # Safety
/// See `LieroBitmap`; `title` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_draw(
    ws: &mut LieroWeapsel,
    scr: &LieroBitmap,
    menu_scr: &LieroBitmap,
    font: &LieroFont,
    title: LieroStr,
) {
    let title = title.bytes();
    liero_gfx_draw_rounded_box(scr, 114, 2, 0, 7, font.dims(title).0);
    font.text(scr, title, 116, 3, 50, 1);

    for wm in &mut ws.menus {
        let width = font.dims(&wm.name).0;
        let (x, y) = (wm.menu.0.m.x, wm.menu.0.m.y);
        liero_gfx_draw_rounded_box(scr, x + 29 - width / 2, y - 11, 0, 7, width);
        font.text(scr, &wm.name, x + 31 - width / 2, y - 10, wm.color + 1, 1);
        if !wm.ready {
            draw_menu(wm, menu_scr, font, -1);
        }
    }
}

/// The spectator window's weapon selection: the first two worms' menus, at
/// the sides, while not ready.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_draw_spectator(ws: &mut LieroWeapsel, scr: &LieroBitmap, font: &LieroFont) {
    for (wm, x) in ws.menus.iter_mut().zip([10, 560]) {
        if !wm.ready {
            draw_menu(wm, scr, font, x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rand::LIERO_RAND_DEFAULT_SEED;

    fn env(table: &[u32; 40], order: &[i32], rand: &mut LieroRand) -> LieroWeapselEnv {
        LieroWeapselEnv { weap_table: table.as_ptr(), weap_order: order.as_ptr(), weapon_count: order.len() as i32, rand }
    }

    #[test]
    fn starting_weapons_are_allowed_and_different() {
        let order: Vec<i32> = (0..40).rev().collect();
        let mut table = [0u32; 40];
        for t in table.iter_mut().skip(5) {
            *t = 1;
        }
        let mut rand = LieroRand::new(LIERO_RAND_DEFAULT_SEED);
        // Numbers 36 to 40 are weapons 4 to 0, the only ones allowed.
        let mut weapons = [40, 1, 2, 40, 39];
        let mut out = [0; 5];
        unsafe { liero_weapsel_choose(weapons.as_mut_ptr(), false, &env(&table, &order, &mut rand), out.as_mut_ptr()) };
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
        let mut rand = LieroRand::new(1);
        let raw = env(&table, &order, &mut rand);
        let e = unsafe { Env::new(&raw) };
        assert_eq!(e.step(2, -1), 39, "wraps past the disabled first and last");
        assert_eq!(e.step(39, 1), 2);
        assert_eq!(e.step(5, 1), 6);
    }
}
