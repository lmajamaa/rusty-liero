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
use crate::sfx::LieroSoundOut;
use crate::tc::{LieroTcData, LieroTcWeapon};
use crate::worm::{LieroWormData, LIERO_CONTROL_DOWN, LIERO_CONTROL_FIRE, LIERO_CONTROL_LEFT, LIERO_CONTROL_RIGHT, LIERO_CONTROL_UP};

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
    /// The worm, whose controls pick and which is given the weapons.
    worm: *mut LieroWormData,
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
    /// The worm in the viewport.
    pub worm: *mut LieroWormData,
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
    /// The weapons the worms are given, and the menus' sounds, which play
    /// on `sound`.
    pub tc: *const LieroTcData,
    pub sound: LieroSoundOut,
}

struct Env<'a> {
    table: &'a [u32],
    order: &'a [i32],
    weapon_count: i32,
    rand: &'a mut LieroRand,
    tc: *const LieroTcData,
    sound: LieroSoundOut,
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
            tc: e.tc,
            sound: e.sound,
        }
    }

    /// Plays a menu sound (`soundPlayer->play`).
    unsafe fn play(&self, sound: i32) {
        self.sound.play(&*self.tc, sound, std::ptr::null_mut(), 0);
    }

    /// The TC's weapon `w`, as a worm's weapon slot points to it.
    unsafe fn kind(&self, w: i32) -> *const c_void {
        assert!((w as usize) < (*self.tc).weapon_count);
        (*self.tc).weapons.add(w as usize).cast()
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
/// which the worm is given, empty, with its first weapon current.
///
/// # Safety
/// `spec`'s pointers are valid for their counts, and see `LieroWeapselEnv`.
/// The worms and their `weapons` outlive the selection.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_new(spec: &LieroWeapselSpec, env: &LieroWeapselEnv) -> *mut LieroWeapsel {
    let e = Env::new(env);
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
        let worm = &mut *w.worm;
        for (j, &weapon) in chosen.iter().enumerate() {
            let ww = &mut worm.weapons[j];
            ww.ammo = 0;
            ww.kind = e.kind(weapon);
            worm.current_weapon = 0;
            menu.insert(menu.len(), &item(&weapon_names[weapon as usize], 48));
        }
        menu.insert(menu.len(), &item(spec.done.bytes(), 10));
        menu.move_to(0);

        menus.push(WeapselMenu {
            menu: LieroMenuBox(menu),
            weapons: w.weapons,
            worm: w.worm,
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

/// `Worm::pressed`.
fn pressed(worm: &LieroWormData, control: u32) -> bool {
    worm.control_states >> control & 1 != 0
}

/// `Worm::release`.
fn release(worm: &mut LieroWormData, control: u32) {
    worm.control_states &= !(1 << control);
}

/// `Worm::pressedOnce`: whether it was pressed, then clears it.
fn pressed_once(worm: &mut LieroWormData, control: u32) -> bool {
    let p = pressed(worm, control);
    release(worm, control);
    p
}

/// Gives each worm its chosen weapons, loaded, with the first current
/// (`WeaponSelection::finalize`, `Worm::initWeapons`), and releases their
/// controls.
///
/// # Safety
/// `worms` holds `count` valid worms, each with its settings' 5 weapon
/// numbers in `weapons`, and `tc` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_finalize(worms: *const *mut LieroWormData, weapons: *const *const u32, count: i32, tc: &LieroTcData) {
    for i in 0..count.max(0) as usize {
        let worm = &mut **worms.add(i);
        let numbers = std::slice::from_raw_parts(*weapons.add(i), LIERO_SELECTABLE_WEAPONS);
        worm.current_weapon = 0; // It was 1 in OpenLiero A1
        for (ww, &n) in worm.weapons.iter_mut().zip(numbers) {
            let w = tc.weapon_order()[(n as usize).wrapping_sub(1)];
            assert!((w as usize) < tc.weapon_count);
            let t = &*tc.weapons.add(w as usize);
            ww.kind = (t as *const LieroTcWeapon).cast();
            ww.ammo = t.ammo;
            ww.delay_left = 0;
            ww.loading_left = 0;
        }
        // `Game::releaseControls`: the seven controls, not dig.
        worm.control_states &= !0x7f;
    }
}

/// One frame of weapon selection: `WeaponSelection::processFrame`. Each
/// worm not ready changes the selected weapon with left and right, moves
/// with up and down, and picks random weapons or finishes with fire.
/// Returns whether every worm is ready.
///
/// # Safety
/// See `LieroWeapselEnv`.
#[no_mangle]
pub unsafe extern "C" fn liero_weapsel_process(ws: &mut LieroWeapsel, env: &LieroWeapselEnv) -> bool {
    let mut e = Env::new(env);
    let mut all_ready = true;

    for wm in ws.menus.iter_mut() {
        let worm = &mut *wm.worm;
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
                    if pressed(worm, control) {
                        release(worm, control);
                        e.play(sound);
                        let n = &mut weapons[slot as usize];
                        *n = e.step(*n, dir);
                        let w = e.weapon(*n);
                        worm.weapons[slot as usize].kind = e.kind(w);
                        renames.push((slot, w));
                    }
                }
            }

            if pressed_once(worm, LIERO_CONTROL_UP) {
                e.play(26);
                menu.movement(-1);
            }
            if pressed_once(worm, LIERO_CONTROL_DOWN) {
                e.play(25);
                menu.movement(1);
            }

            if pressed(worm, LIERO_CONTROL_FIRE) {
                if menu.m.selection == RANDOMIZE_ITEM {
                    for (j, w) in e.randomize(weapons).into_iter().enumerate() {
                        renames.push((j as i32, w));
                    }
                } else if menu.m.selection == DONE_ITEM {
                    e.play(27);
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
        LieroWeapselEnv {
            weap_table: table.as_ptr(),
            weap_order: order.as_ptr(),
            weapon_count: order.len() as i32,
            rand,
            tc: std::ptr::null(),
            sound: LieroSoundOut::NONE,
        }
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

    thread_local! {
        static SOUNDS: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn report(sound: i32) {
        SOUNDS.with(|s| s.borrow_mut().push(sound));
    }

    /// A TC with `n` weapons, each with ammo 10 + its index.
    /// Boxed: it is too big for the wasm tests' stack.
    fn tc(weapons: &mut [LieroTcWeapon], order: &[i32]) -> Box<LieroTcData> {
        for (i, w) in weapons.iter_mut().enumerate() {
            w.id = i as i32;
            w.ammo = 10 + i as i32;
        }
        let layout = std::alloc::Layout::new::<LieroTcData>();
        // SAFETY: all zeroes is a valid TC: null pointers and zero counts.
        let mut tc: Box<LieroTcData> = unsafe { Box::from_raw(std::alloc::alloc_zeroed(layout).cast()) };
        tc.weapons = weapons.as_ptr();
        tc.weapon_count = weapons.len();
        tc.weap_order = order.as_ptr();
        tc
    }

    #[test]
    fn menus_play_sounds_and_set_the_worm() {
        let order: Vec<i32> = (0..40).collect();
        let mut weapons: Vec<LieroTcWeapon> = (0..40).map(|_| unsafe { std::mem::zeroed() }).collect();
        let tc = tc(&mut weapons, &order);
        let table = [0u32; 40];
        let mut rand = LieroRand::new(1);
        let mut raw = env(&table, &order, &mut rand);
        raw.tc = &*tc;
        raw.sound = LieroSoundOut { mixer: std::ptr::null_mut(), report: Some(report) };

        let mut worm = LieroWormData::default();
        let mut numbers = [1u32, 2, 3, 4, 5];
        let names = [LieroStr { ptr: b"W".as_ptr(), len: 1 }; 40];
        let spec_worm = LieroWeapselWorm {
            worm: &mut worm,
            weapons: numbers.as_mut_ptr(),
            name: names[0],
            color: 0,
            center_x: 100,
            center_y: 100,
            random: false,
            ready: false,
        };
        let spec = LieroWeapselSpec {
            worms: &spec_worm,
            worm_count: 1,
            weapon_names: names.as_ptr(),
            weapon_count: 40,
            randomize: names[0],
            done: names[0],
        };
        unsafe {
            let ws = liero_weapsel_new(&spec, &raw);
            assert_eq!(worm.weapons[2].kind, weapons.as_ptr().add(2).cast());
            assert_eq!(worm.weapons[2].ammo, 0);

            let press = |worm: &mut LieroWormData, control: u32| {
                worm.control_states = 1 << control;
                liero_weapsel_process(&mut *ws, &raw);
            };
            press(&mut worm, LIERO_CONTROL_DOWN);
            assert_eq!(worm.control_states, 0, "pressed once");
            press(&mut worm, LIERO_CONTROL_RIGHT);
            assert_eq!(worm.control_states, 0, "released");
            assert_eq!(numbers[0], 2);
            assert_eq!(worm.weapons[0].kind, weapons.as_ptr().add(1).cast());
            press(&mut worm, LIERO_CONTROL_UP);
            press(&mut worm, LIERO_CONTROL_LEFT);
            assert_eq!(numbers[0], 2, "left and right only change weapons");
            SOUNDS.with(|s| assert_eq!(*s.borrow(), vec![25, 26, 26]));
            liero_weapsel_free(ws);
        }
    }

    #[test]
    fn finalize_loads_the_weapons_and_releases_the_controls() {
        let order: Vec<i32> = (0..10).rev().collect();
        let mut weapons: Vec<LieroTcWeapon> = (0..10).map(|_| unsafe { std::mem::zeroed() }).collect();
        let tc = tc(&mut weapons, &order);
        let mut worm = LieroWormData::default();
        worm.current_weapon = 3;
        worm.control_states = 0xff;
        worm.weapons[1].delay_left = 5;
        worm.weapons[1].loading_left = 5;
        let numbers = [1u32, 2, 3, 4, 10];
        let worms = [&mut worm as *mut LieroWormData];
        let lists = [numbers.as_ptr()];
        unsafe { liero_weapsel_finalize(worms.as_ptr(), lists.as_ptr(), 1, &tc) };
        assert_eq!(worm.control_states, 0x80, "dig stays");
        assert_eq!(worm.current_weapon, 0);
        assert_eq!(worm.weapons[0].kind, unsafe { weapons.as_ptr().add(9) }.cast());
        assert_eq!(worm.weapons[0].ammo, 19);
        assert_eq!(worm.weapons[4].ammo, 10);
        assert_eq!((worm.weapons[1].delay_left, worm.weapons[1].loading_left), (0, 0));
    }
}
