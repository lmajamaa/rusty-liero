//! The weapon options screen: which weapons a game has, each on, off for
//! bots or off (`WeaponOptionsScreen` in C++). Escape closes it while one
//! weapon or more is on, and otherwise says so.

use crate::boxes::liero_info_box_screen;
use crate::gfx::liero_gfx_draw_rounded_box;
use crate::item::{LieroItemBehavior, LieroItemKind};
use crate::menu::{LieroMenu, OwnedMenu};
use crate::screen::{into_ffi, Host, LieroScreen, Screen, Tick};
use crate::text::LieroStr;

const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_RIGHT: u32 = 79;
const SCANCODE_LEFT: u32 = 80;
const SCANCODE_DOWN: u32 = 81;
const SCANCODE_UP: u32 = 82;
const SCANCODE_PAGEUP: u32 = 75;
const SCANCODE_PAGEDOWN: u32 = 78;

const SOUND_UP: i32 = 26;
const SOUND_DOWN: i32 = 25;

/// The weapons in a settings table.
const WEAPON_COUNT: usize = 40;

/// What the weapon options screen shows and edits.
#[repr(C)]
pub struct LieroWeaponOptionsSpec {
    /// The weapons' names in menu order, and each one's index in
    /// `weap_table`.
    pub names: *const LieroStr,
    pub order: *const i32,
    pub count: usize,
    /// The settings' weapon table (40 entries): 0 on, 1 bots only, 2 off.
    pub weap_table: *mut u32,
    /// The names of those states.
    pub states: *const LieroStr,
    pub state_count: usize,
    /// The column titles, and the message when every weapon is off.
    pub weapon_title: LieroStr,
    pub availability_title: LieroStr,
    pub no_weapons: LieroStr,
}

struct WeaponOptions {
    menu: OwnedMenu,
    order: Vec<usize>,
    weap_table: *mut u32,
    states: Vec<Vec<u8>>,
    weapon_title: Vec<u8>,
    availability_title: Vec<u8>,
    no_weapons: Vec<u8>,
    shown: bool,
    resumed: bool,
}

impl WeaponOptions {
    /// The availability item for menu item `i`, with `names` the states'.
    fn behavior(&self, i: usize, names: &[LieroStr]) -> LieroItemBehavior {
        // SAFETY: `order` holds indexes into the 40-entry table.
        let value = unsafe { self.weap_table.add(self.order[i]) } as *mut i32;
        let mut b = LieroItemBehavior::integer(value, 0, names.len() as i32 - 1);
        b.kind = LieroItemKind::Enum;
        b.names = names.as_ptr();
        b.name_count = names.len();
        b
    }

    fn state_names(&self) -> Vec<LieroStr> {
        self.states
            .iter()
            .map(|s| LieroStr {
                ptr: s.as_ptr(),
                len: s.len(),
            })
            .collect()
    }

    /// `Menu::updateItems`: every item's value.
    fn update_items(&mut self) {
        let names = self.state_names();
        for i in 0..self.order.len() {
            let text = unsafe { self.behavior(i, &names).value_text() };
            self.menu.set_value(i, text);
        }
    }

    /// `Menu::onLeftRight` on the selected item.
    fn left_right(&mut self, dir: i32) {
        let Some(i) = self.menu.selected() else {
            return;
        };
        let names = self.state_names();
        let a = unsafe { self.behavior(i, &names).left_right(dir, 0) };
        if a.update_menu {
            self.update_items();
        } else if a.update_item {
            self.menu.set_value(i, unsafe { self.behavior(i, &names).value_text() });
        }
    }

    fn any_on(&self) -> bool {
        // SAFETY: the table has 40 entries.
        (0..WEAPON_COUNT).any(|i| unsafe { *self.weap_table.add(i) } == 0)
    }

    fn draw(&mut self, host: &Host) {
        let (Some(scr), Some(font)) = (host.screen(), host.font()) else {
            return;
        };
        unsafe {
            for (x, title) in [(179, &self.weapon_title), (249, &self.availability_title)] {
                liero_gfx_draw_rounded_box(&scr, x, 20, 0, 7, font.dims(title).0);
                font.text(&scr, title, x + 2, 21, 50, 1);
            }
        }
        self.menu.draw(&scr, &font, false, -1, false);
    }
}

impl Screen for WeaponOptions {
    fn tick(&mut self, host: &Host) -> Tick {
        // Back from the message: it read the events.
        if self.shown && !self.resumed {
            host.process();
            if host.test_key_once(SCANCODE_ESCAPE) {
                if self.any_on() {
                    return Tick::close(0);
                }
                let text = LieroStr {
                    ptr: self.no_weapons.as_ptr(),
                    len: self.no_weapons.len(),
                };
                return Tick::push(unsafe { liero_info_box_screen(text, 223, 68, false) });
            }
        }
        self.resumed = false;

        host.draw_basic_menu();
        self.draw(host);

        if host.test_key_once(SCANCODE_UP) {
            host.play_sound(SOUND_UP);
            self.menu.movement(-1);
        }
        if host.test_key_once(SCANCODE_DOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu.movement(1);
        }
        if host.test_key_once(SCANCODE_LEFT) {
            self.left_right(-1);
        }
        if host.test_key_once(SCANCODE_RIGHT) {
            self.left_right(1);
        }
        if host.test_key_once(SCANCODE_PAGEUP) {
            host.play_sound(SOUND_UP);
            self.menu.movement_page(-1);
        }
        if host.test_key_once(SCANCODE_PAGEDOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu.movement_page(1);
        }
        self.menu.on_keys(host, false);

        host.menu_fade();
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _host: &Host, _result: i32) {
        self.resumed = true;
    }
}

/// The weapon options screen. It edits `spec.weap_table` in place, which
/// must outlive it; the rest of `spec` is copied.
///
/// # Safety
/// `spec`'s pointers are valid for their counts, and `order`'s entries are
/// below 40.
#[no_mangle]
pub unsafe extern "C" fn liero_weapon_options_screen(spec: &LieroWeaponOptionsSpec) -> LieroScreen {
    unsafe { into_ffi(Box::new(WeaponOptions::new(spec))) }
}

impl WeaponOptions {
    /// # Safety
    /// As for `liero_weapon_options_screen`.
    unsafe fn new(spec: &LieroWeaponOptionsSpec) -> Self {
        unsafe {
            let text = |s: LieroStr| s.bytes().to_vec();
            // Menu(179, 28) with setHeight(14), before any items.
            let mut menu = OwnedMenu::new(LieroMenu {
                x: 179,
                y: 28,
                height: 14,
                item_height: 8,
                ..Default::default()
            });
            menu.m.value_offset_x = 89;
            let mut order = Vec::with_capacity(spec.count);
            for i in 0..spec.count {
                menu.add(text(*spec.names.add(i)), 48);
                order.push(*spec.order.add(i) as usize);
            }
            let states = (0..spec.state_count).map(|i| text(*spec.states.add(i))).collect();
            let mut screen = WeaponOptions {
                menu,
                order,
                weap_table: spec.weap_table,
                states,
                weapon_title: text(spec.weapon_title),
                availability_title: text(spec.availability_title),
                no_weapons: text(spec.no_weapons),
                shown: false,
                resumed: false,
            };
            screen.menu.move_to(0);
            screen.update_items();
            screen
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{LieroScreenHost, LieroTickKind};
    use std::cell::RefCell;

    thread_local! {
        static KEYS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn key_once(sc: u32) -> bool {
        KEYS.with(|k| {
            let mut k = k.borrow_mut();
            let found = k.iter().position(|&x| x == sc);
            found.map(|i| k.remove(i)).is_some()
        })
    }

    fn press(keys: &[u32]) {
        KEYS.with(|k| *k.borrow_mut() = keys.to_vec());
    }

    fn s(b: &'static [u8]) -> LieroStr {
        LieroStr {
            ptr: b.as_ptr(),
            len: b.len(),
        }
    }

    fn screen(table: &mut [u32; WEAPON_COUNT]) -> WeaponOptions {
        let names = [s(b"BAZOOKA"), s(b"ZIMM")];
        let order = [1, 0];
        let states = [s(b"MENU"), s(b"BONUS"), s(b"BANNED")];
        let spec = LieroWeaponOptionsSpec {
            names: names.as_ptr(),
            order: order.as_ptr(),
            count: 2,
            weap_table: table.as_mut_ptr(),
            states: states.as_ptr(),
            state_count: 3,
            weapon_title: s(b"WEAPON"),
            availability_title: s(b"AVAILABILITY"),
            no_weapons: s(b"NONE"),
        };
        unsafe { WeaponOptions::new(&spec) }
    }

    fn host() -> Host {
        Host(LieroScreenHost {
            test_key_once: Some(key_once),
            ..Default::default()
        })
    }

    #[test]
    fn values_follow_the_table_in_menu_order() {
        let mut table = [0; WEAPON_COUNT];
        table[1] = 2;
        let mut w = screen(&mut table);
        assert_eq!(w.menu.values, [b"BANNED".to_vec(), b"MENU".to_vec()]);
        w.left_right(1);
        assert_eq!(table[1], 0, "wraps round");
        assert_eq!(w.menu.values[0], b"MENU".to_vec());
    }

    #[test]
    fn escape_needs_a_weapon_on() {
        let mut table = [1; WEAPON_COUNT];
        let mut w = screen(&mut table);
        let h = host();
        press(&[SCANCODE_ESCAPE]);
        assert_eq!(
            w.tick(&h).kind,
            LieroTickKind::TickStay,
            "keys wait for the first frame"
        );
        press(&[SCANCODE_ESCAPE]);
        let t = w.tick(&h);
        assert_eq!(t.kind, LieroTickKind::TickPush, "every weapon off: the message");

        // Back from the message, which read the events: no keys this tick.
        w.resume(&h, 0);
        press(&[SCANCODE_ESCAPE]);
        assert_eq!(w.tick(&h).kind, LieroTickKind::TickStay);

        table[5] = 0;
        press(&[SCANCODE_ESCAPE]);
        assert_eq!(w.tick(&h).kind, LieroTickKind::TickClose);
    }
}
