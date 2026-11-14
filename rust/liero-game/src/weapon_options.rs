//! The weapon options screen: which weapons a game has, each on, off for
//! bots or off (`WeaponOptionsScreen` in C++). Escape closes it while one
//! weapon or more is on, and otherwise says so.

use crate::boxes::info_box_screen;
use crate::gfx::draw_rounded_box;
use crate::item::ItemBehavior;
use crate::menu::{Menu, MenuState};
use crate::screen::{BoxedScreen, Host, Screen, Tick};
use crate::text::Str;

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
pub struct WeaponOptionsSpec {
    /// The weapons' names in menu order, and each one's index in
    /// `weap_table`.
    pub names: *const Str,
    pub order: *const i32,
    pub count: usize,
    /// The settings' weapon table (40 entries): 0 on, 1 bots only, 2 off.
    pub weap_table: *mut u32,
    /// The names of those states.
    pub states: *const Str,
    pub state_count: usize,
    /// The column titles, and the message when every weapon is off.
    pub weapon_title: Str,
    pub availability_title: Str,
    pub no_weapons: Str,
}

struct WeaponOptions {
    menu: Menu,
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
    /// The availability item, with `names` the states'.
    fn behavior<'a>(names: &'a [&'a [u8]]) -> ItemBehavior<'a> {
        ItemBehavior::enumeration(0, names.len() as i32 - 1, names)
    }

    fn state_names(&self) -> Vec<&[u8]> {
        self.states.iter().map(|s| &s[..]).collect()
    }

    /// Menu item `i`'s entry in the weapon table.
    fn entry(&self, i: usize) -> *mut u32 {
        // SAFETY: `order` holds indexes into the 40-entry table.
        unsafe { self.weap_table.add(self.order[i]) }
    }

    fn value(&self, i: usize) -> i32 {
        // SAFETY: the table outlives the screen.
        unsafe { *self.entry(i) as i32 }
    }

    /// `Menu::updateItems`: every item's value.
    fn update_items(&mut self) {
        for i in 0..self.order.len() {
            let text = Self::behavior(&self.state_names()).value_text(self.value(i));
            self.menu.set_value(i, text);
        }
    }

    /// `Menu::onLeftRight` on the selected item.
    fn left_right(&mut self, host: &Host, dir: i32) {
        let Some(i) = self.menu.selected() else {
            return;
        };
        let names = self.state_names();
        let b = Self::behavior(&names);
        if let Some(s) = b.left_right_sound(dir) {
            host.play_sound(s);
        }
        let (v, a) = b.left_right(self.value(i), dir, 0);
        // SAFETY: as in `value`.
        unsafe { *self.entry(i) = v as u32 };
        let text = b.value_text(v);
        drop(names);
        if a.update_menu {
            self.update_items();
        } else if a.update_item {
            self.menu.set_value(i, text);
        }
    }

    fn any_on(&self) -> bool {
        // SAFETY: the table has 40 entries.
        (0..WEAPON_COUNT).any(|i| unsafe { *self.weap_table.add(i) } == 0)
    }

    fn draw(&mut self, app: &crate::app::App, host: &Host) {
        let (Some(scr), Some(font)) = (host.screen(app), host.font(app)) else {
            return;
        };
        for (x, title) in [(179, &self.weapon_title), (249, &self.availability_title)] {
            draw_rounded_box(&scr, x, 20, 0, 7, font.dims(title).0);
            font.text(&scr, title, x + 2, 21, 50, 1);
        }
        self.menu.draw(&scr, &font, false, -1, false);
    }
}

impl Screen for WeaponOptions {
    fn tick(&mut self, app: &mut crate::app::App, host: &Host) -> Tick {
        // Back from the message: it read the events.
        if self.shown && !self.resumed {
            host.process(app);
            if app.test_key_once(SCANCODE_ESCAPE) {
                if self.any_on() {
                    return Tick::close(0);
                }
                let text = Str {
                    ptr: self.no_weapons.as_ptr(),
                    len: self.no_weapons.len(),
                };
                return Tick::push(unsafe { info_box_screen(text.bytes(), 223, 68, false) });
            }
        }
        self.resumed = false;

        host.draw_basic_menu(app);
        self.draw(app, host);

        if app.test_key_once(SCANCODE_UP) {
            host.play_sound(SOUND_UP);
            self.menu.movement(-1);
        }
        if app.test_key_once(SCANCODE_DOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu.movement(1);
        }
        if app.test_key_once(SCANCODE_LEFT) {
            self.left_right(host, -1);
        }
        if app.test_key_once(SCANCODE_RIGHT) {
            self.left_right(host, 1);
        }
        if app.test_key_once(SCANCODE_PAGEUP) {
            host.play_sound(SOUND_UP);
            self.menu.movement_page(-1);
        }
        if app.test_key_once(SCANCODE_PAGEDOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu.movement_page(1);
        }
        self.menu.on_keys(host, false);

        host.menu_fade(app);
        self.shown = true;
        Tick::stay(true)
    }

    fn resume(&mut self, _app: &mut crate::app::App, _host: &Host, _result: i32) {
        self.resumed = true;
    }
}

/// The weapon options screen. It edits `spec.weap_table` in place, which
/// must outlive it; the rest of `spec` is copied.
///
/// # Safety
/// `spec`'s pointers are valid for their counts, and `order`'s entries are
/// below 40.
pub unsafe fn weapon_options_screen(spec: &WeaponOptionsSpec) -> BoxedScreen {
    unsafe { Box::new(WeaponOptions::new(spec)) }
}

impl WeaponOptions {
    /// # Safety
    /// As for `weapon_options_screen`.
    unsafe fn new(spec: &WeaponOptionsSpec) -> Self {
        unsafe {
            let text = |s: Str| s.bytes().to_vec();
            // Menu(179, 28) with setHeight(14), before any items.
            let mut menu = Menu::new(MenuState {
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
    use crate::screen::Transition;

    fn s(b: &'static [u8]) -> Str {
        Str {
            ptr: b.as_ptr(),
            len: b.len(),
        }
    }

    fn screen(table: &mut [u32; WEAPON_COUNT]) -> WeaponOptions {
        let names = [s(b"BAZOOKA"), s(b"ZIMM")];
        let order = [1, 0];
        let states = [s(b"MENU"), s(b"BONUS"), s(b"BANNED")];
        let spec = WeaponOptionsSpec {
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

    #[test]
    fn values_follow_the_table_in_menu_order() {
        let mut table = [0; WEAPON_COUNT];
        table[1] = 2;
        let mut w = screen(&mut table);
        let values: Vec<_> = w.menu.items.iter().map(|it| it.value.clone()).collect();
        assert_eq!(values, [b"BANNED".to_vec(), b"MENU".to_vec()]);
        w.left_right(&Host::none(), 1);
        assert_eq!(table[1], 0, "wraps round");
        assert_eq!(w.menu.items[0].value, b"MENU".to_vec());
    }

    #[test]
    fn escape_needs_a_weapon_on() {
        let mut table = [1; WEAPON_COUNT];
        let mut w = screen(&mut table);
        let h = Host::none();
        let mut app = crate::app::App::new();
        app.press(&[SCANCODE_ESCAPE]);
        assert!(
            matches!(w.tick(&mut app, &h).then, Transition::Stay),
            "keys wait for the first frame"
        );
        app.press(&[SCANCODE_ESCAPE]);
        let t = w.tick(&mut app, &h);
        assert!(matches!(t.then, Transition::Open(_)), "every weapon off: the message");

        // Back from the message, which read the events: no keys this tick.
        w.resume(&mut app, &h, 0);
        app.press(&[SCANCODE_ESCAPE]);
        assert!(matches!(w.tick(&mut app, &h).then, Transition::Stay));

        // The screen reads the table through its pointer, which the
        // compiler doesn't see.
        #[allow(unused_assignments)]
        {
            table[5] = 0;
        }
        app.press(&[SCANCODE_ESCAPE]);
        assert_eq!(w.tick(&mut app, &h).closed(), Some(0));
    }
}
