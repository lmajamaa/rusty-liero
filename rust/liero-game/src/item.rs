//! What the menus' common items do: on/off switches, numbers, times and
//! enums (`src/game/menu/*Behavior.hpp`). Each item edits one setting in
//! place through a pointer; C++ makes the description when the menu asks for
//! an item's behaviour and does what the result says (redraw the item or the
//! whole menu, or open the number box, which is a C++ screen call).

use crate::menu::{
    liero_menu_enum_change, liero_menu_integer_left_right, liero_menu_value_text, LIERO_MENU_VALUE_INTEGER,
};
use crate::screen::current_host;
use crate::text::LieroStr;

const SOUND_RIGHT: i32 = 25;
const SOUND_LEFT: i32 = 26;
const SOUND_ENTER: i32 = 27;

/// What an item edits.
///
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LieroItemKind {
    /// On or off: `flag`, shown with `names` (off, on).
    Switch,
    /// A number from `min` to `max`, shown as `value_kind`.
    Integer,
    /// One of `min` to `max`, wrapping round: shown with `names` when there
    /// are any, else as a number.
    Enum,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroItemBehavior {
    pub kind: LieroItemKind,
    /// The switch's setting.
    pub flag: *mut bool,
    /// Called with a switch's new value in place of writing `flag`.
    pub set_flag: Option<unsafe extern "C" fn(bool)>,
    /// The number's or enum's setting (enums are `uint32_t`, read as is).
    pub value: *mut i32,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    /// Frames between steps while left or right is held.
    pub scroll_interval: i32,
    /// One of the `LIERO_MENU_VALUE_*` kinds.
    pub value_kind: i32,
    /// Whether Enter opens a box to type a number.
    pub allow_entry: bool,
    pub names: *const LieroStr,
    pub name_count: usize,
}

/// What C++ does after a key on an item.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroItemAction {
    /// For left and right: whether the menu goes on to its own handling of
    /// the key (`ItemBehavior::onLeftRight`'s result).
    pub keep_going: bool,
    /// Redraw this item's value.
    pub update_item: bool,
    /// Redraw all the menu's values.
    pub update_menu: bool,
    /// More than 0: open the number box with this many digits.
    pub enter_digits: i32,
}

impl LieroItemBehavior {
    /// An integer item with the C++ constructor's defaults.
    pub fn integer(value: *mut i32, min: i32, max: i32) -> Self {
        LieroItemBehavior {
            kind: LieroItemKind::Integer,
            flag: std::ptr::null_mut(),
            set_flag: None,
            value,
            min,
            max,
            step: 1,
            scroll_interval: 5,
            value_kind: LIERO_MENU_VALUE_INTEGER,
            allow_entry: true,
            names: std::ptr::null(),
            name_count: 0,
        }
    }

    /// # Safety
    /// `flag` (and `set_flag`) are valid.
    unsafe fn toggle(&self) {
        let v = !*self.flag;
        match self.set_flag {
            Some(set) => set(v),
            None => *self.flag = v,
        }
    }

    /// # Safety
    /// `value` is valid.
    unsafe fn change_enum(&self, dir: i32) -> LieroItemAction {
        let v = *self.value as u32;
        let new_v = liero_menu_enum_change(v, self.min as u32, self.max as u32, dir);
        if new_v != v {
            *self.value = new_v as i32;
            return LieroItemAction { update_menu: true, ..Default::default() };
        }
        LieroItemAction::default()
    }

    /// # Safety
    /// As for `liero_item_left_right`.
    pub unsafe fn left_right(&self, dir: i32, menu_cycles: u32) -> LieroItemAction {
        let host = current_host();
        let side_sound = || host.play_sound(if dir > 0 { SOUND_RIGHT } else { SOUND_LEFT });
        match self.kind {
            LieroItemKind::Switch => {
                side_sound();
                self.toggle();
                LieroItemAction { update_item: true, ..Default::default() }
            }
            LieroItemKind::Integer => {
                let v = *self.value;
                let new_v = liero_menu_integer_left_right(
                    v,
                    self.min,
                    self.max,
                    self.step,
                    dir,
                    menu_cycles,
                    self.scroll_interval,
                );
                if new_v != v {
                    *self.value = new_v;
                }
                LieroItemAction { keep_going: true, update_item: new_v != v, ..Default::default() }
            }
            LieroItemKind::Enum => {
                side_sound();
                self.change_enum(dir)
            }
        }
    }

    /// # Safety
    /// As for `liero_item_enter`.
    pub unsafe fn enter(&self) -> LieroItemAction {
        current_host().play_sound(SOUND_ENTER);
        match self.kind {
            LieroItemKind::Switch => {
                self.toggle();
                LieroItemAction { update_item: true, ..Default::default() }
            }
            LieroItemKind::Integer => {
                if !self.allow_entry {
                    return LieroItemAction::default();
                }
                let digits = 1 + (self.max as f64).log10().floor() as i32;
                LieroItemAction { enter_digits: digits, ..Default::default() }
            }
            LieroItemKind::Enum => self.change_enum(1),
        }
    }

    /// The value shown after the item's name.
    ///
    /// # Safety
    /// As for `liero_item_value_text`.
    pub unsafe fn value_text(&self) -> Vec<u8> {
        let name = |i: usize| {
            assert!(i < self.name_count, "item value {i} has no name");
            (*self.names.add(i)).bytes().to_vec()
        };
        match self.kind {
            LieroItemKind::Switch => name(*self.flag as usize),
            LieroItemKind::Enum if self.name_count > 0 => name(*self.value as u32 as usize),
            LieroItemKind::Integer | LieroItemKind::Enum => {
                let mut buf = [0u8; 32];
                let len = liero_menu_value_text(*self.value, self.value_kind, buf.as_mut_ptr(), buf.len());
                buf[..len].to_vec()
            }
        }
    }
}

/// Left (`dir` < 0) or right on an item, with the menu's frame count for
/// numbers that repeat while the key is held.
///
/// # Safety
/// The item's setting pointer for its kind is valid, and its names too.
#[no_mangle]
pub unsafe extern "C" fn liero_item_left_right(b: &LieroItemBehavior, dir: i32, menu_cycles: u32) -> LieroItemAction {
    b.left_right(dir, menu_cycles)
}

/// Enter on an item.
///
/// # Safety
/// As for `liero_item_left_right`.
#[no_mangle]
pub unsafe extern "C" fn liero_item_enter(b: &LieroItemBehavior) -> LieroItemAction {
    b.enter()
}

/// Writes the item's value text into `out` (cut to `cap` bytes) and returns
/// its full length.
///
/// # Safety
/// As for `liero_item_left_right`, and `out` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_item_value_text(b: &LieroItemBehavior, out: *mut u8, cap: usize) -> usize {
    let text = b.value_text();
    let n = text.len().min(cap);
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, n);
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{LIERO_MENU_VALUE_FRAMES, LIERO_MENU_VALUE_PERCENTAGE, LIERO_MENU_VALUE_TIME};

    fn s(b: &'static [u8]) -> LieroStr {
        LieroStr { ptr: b.as_ptr(), len: b.len() }
    }

    fn text(b: &LieroItemBehavior) -> String {
        String::from_utf8(unsafe { b.value_text() }).unwrap()
    }

    #[test]
    fn switch() {
        let names = [s(b"OFF"), s(b"ON")];
        let mut flag = false;
        let mut b = LieroItemBehavior::integer(std::ptr::null_mut(), 0, 0);
        b.kind = LieroItemKind::Switch;
        b.flag = &mut flag;
        b.names = names.as_ptr();
        b.name_count = 2;
        assert_eq!(text(&b), "OFF");
        let a = unsafe { b.left_right(-1, 0) };
        assert_eq!(a, LieroItemAction { update_item: true, ..Default::default() });
        assert_eq!(text(&b), "ON");
        unsafe { b.enter() };
        assert_eq!(text(&b), "OFF");
    }

    static SET: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
    unsafe extern "C" fn set(v: bool) {
        SET.store(v as i32, std::sync::atomic::Ordering::Relaxed);
    }

    #[test]
    fn switch_with_setter_leaves_the_flag() {
        let mut flag = true;
        let mut b = LieroItemBehavior::integer(std::ptr::null_mut(), 0, 0);
        b.kind = LieroItemKind::Switch;
        b.flag = &mut flag;
        b.set_flag = Some(set);
        unsafe { b.enter() };
        assert!(flag);
        assert_eq!(SET.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn integer() {
        let mut v = 5;
        let mut b = LieroItemBehavior::integer(&mut v, 1, 999);
        let a = unsafe { b.left_right(1, 0) };
        assert_eq!(a, LieroItemAction { keep_going: true, update_item: true, ..Default::default() });
        let a = unsafe { b.left_right(1, 1) };
        assert_eq!(a, LieroItemAction { keep_going: true, ..Default::default() }, "waits for the repeat");
        assert_eq!(unsafe { b.enter() }.enter_digits, 3);
        b.max = 1000;
        assert_eq!(unsafe { b.enter() }.enter_digits, 4);
        b.allow_entry = false;
        assert_eq!(unsafe { b.enter() }, LieroItemAction::default());

        assert_eq!(unsafe { *b.value }, 6);
        assert_eq!(text(&b), "6");
        b.value_kind = LIERO_MENU_VALUE_PERCENTAGE;
        assert_eq!(text(&b), "6%");
        unsafe { *b.value = 125 };
        b.value_kind = LIERO_MENU_VALUE_TIME;
        assert_eq!(text(&b), "02:05");
        b.value_kind = LIERO_MENU_VALUE_FRAMES;
        assert!(!text(&b).is_empty());
    }

    #[test]
    fn enums() {
        let names = [s(b"A"), s(b"B"), s(b"C")];
        let mut v = 2;
        let mut b = LieroItemBehavior::integer(&mut v, 0, 2);
        b.kind = LieroItemKind::Enum;
        b.names = names.as_ptr();
        b.name_count = 3;
        assert_eq!(text(&b), "C");
        let a = unsafe { b.enter() };
        assert_eq!(a, LieroItemAction { update_menu: true, ..Default::default() });
        assert_eq!(text(&b), "A");
        unsafe { b.left_right(-1, 0) };
        assert_eq!(text(&b), "C");

        b.names = std::ptr::null();
        b.name_count = 0;
        b.min = 2;
        b.max = 2;
        assert_eq!(unsafe { b.enter() }, LieroItemAction::default(), "one value: no change");
        assert_eq!(text(&b), "2");
    }
}
