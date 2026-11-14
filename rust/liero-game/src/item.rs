//! What the menus' common items do: on/off switches, numbers, times and
//! enums (`src/game/menu/*Behavior.hpp`). An item is its kind and limits;
//! the menu holding it reads the setting it edits, passes the value in and
//! writes back what comes out, then does what the result says (redraw the
//! item or the whole menu, or open the number box).

use crate::menu::{
    liero_menu_enum_change, liero_menu_integer_left_right, liero_menu_value_text, LIERO_MENU_VALUE_INTEGER,
};

const SOUND_RIGHT: i32 = 25;
const SOUND_LEFT: i32 = 26;
/// The sound Enter on any item plays.
pub const SOUND_ENTER: i32 = 27;

/// How an item shows and changes its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// On or off (0 or 1), shown with `names` (off, on).
    Switch,
    /// A number from `min` to `max`, shown as `value_kind`.
    Integer,
    /// One of `min` to `max`, wrapping round: shown with `names` when there
    /// are any, else as a number. Enums are `uint32_t`, read as is.
    Enum,
}

/// An item's kind and limits. The value it edits is the caller's: each call
/// takes the value and gives back the new one.
#[derive(Clone, Copy, Debug)]
pub struct ItemBehavior<'a> {
    pub kind: ItemKind,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    /// Frames between steps while left or right is held.
    pub scroll_interval: i32,
    /// One of the `LIERO_MENU_VALUE_*` kinds.
    pub value_kind: i32,
    /// Whether Enter opens a box to type a number.
    pub allow_entry: bool,
    pub names: &'a [&'a [u8]],
}

/// What the menu does after a key on an item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemAction {
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

impl<'a> ItemBehavior<'a> {
    /// An integer item with the C++ constructor's defaults.
    pub fn integer(min: i32, max: i32) -> Self {
        ItemBehavior {
            kind: ItemKind::Integer,
            min,
            max,
            step: 1,
            scroll_interval: 5,
            value_kind: LIERO_MENU_VALUE_INTEGER,
            allow_entry: true,
            names: &[],
        }
    }

    /// One of `min` to `max`, wrapping round (`EnumBehavior`).
    pub fn enumeration(min: i32, max: i32, names: &'a [&'a [u8]]) -> Self {
        ItemBehavior {
            kind: ItemKind::Enum,
            names,
            ..Self::integer(min, max)
        }
    }

    /// On or off, shown with `names` (`BooleanSwitchBehavior`).
    pub fn switch(names: &'a [&'a [u8]]) -> Self {
        ItemBehavior {
            kind: ItemKind::Switch,
            names,
            ..Self::integer(0, 0)
        }
    }

    fn change_enum(&self, v: i32, dir: i32) -> (i32, ItemAction) {
        let new_v = liero_menu_enum_change(v as u32, self.min as u32, self.max as u32, dir) as i32;
        let a = ItemAction {
            update_menu: new_v != v,
            ..Default::default()
        };
        (new_v, a)
    }

    /// The sound left (`dir` < 0) or right plays first, if any: switches
    /// and enums play one, numbers none.
    pub fn left_right_sound(&self, dir: i32) -> Option<i32> {
        (self.kind != ItemKind::Integer).then_some(if dir > 0 { SOUND_RIGHT } else { SOUND_LEFT })
    }

    /// Left (`dir` < 0) or right on the item with value `v`, with the menu's
    /// frame count for numbers that repeat while the key is held: the new
    /// value and what to do. The caller plays `left_right_sound`.
    pub fn left_right(&self, v: i32, dir: i32, menu_cycles: u32) -> (i32, ItemAction) {
        match self.kind {
            ItemKind::Switch => {
                let a = ItemAction {
                    update_item: true,
                    ..Default::default()
                };
                (i32::from(v == 0), a)
            }
            ItemKind::Integer => {
                let new_v = liero_menu_integer_left_right(
                    v,
                    self.min,
                    self.max,
                    self.step,
                    dir,
                    menu_cycles,
                    self.scroll_interval,
                );
                let a = ItemAction {
                    keep_going: true,
                    update_item: new_v != v,
                    ..Default::default()
                };
                (new_v, a)
            }
            ItemKind::Enum => self.change_enum(v, dir),
        }
    }

    /// Enter on the item with value `v`: the new value and what to do.
    /// The caller plays `SOUND_ENTER` first.
    pub fn enter(&self, v: i32) -> (i32, ItemAction) {
        match self.kind {
            ItemKind::Switch => {
                let a = ItemAction {
                    update_item: true,
                    ..Default::default()
                };
                (i32::from(v == 0), a)
            }
            ItemKind::Integer => {
                if !self.allow_entry {
                    return (v, ItemAction::default());
                }
                let digits = 1 + (self.max as f64).log10().floor() as i32;
                let a = ItemAction {
                    enter_digits: digits,
                    ..Default::default()
                };
                (v, a)
            }
            ItemKind::Enum => self.change_enum(v, 1),
        }
    }

    /// The value `v` as shown after the item's name.
    pub fn value_text(&self, v: i32) -> Vec<u8> {
        let name = |i: usize| match self.names.get(i) {
            Some(n) => n.to_vec(),
            None => panic!("item value {i} has no name"),
        };
        match self.kind {
            ItemKind::Switch => name(usize::from(v != 0)),
            ItemKind::Enum if !self.names.is_empty() => name(v as u32 as usize),
            ItemKind::Integer | ItemKind::Enum => liero_menu_value_text(v, self.value_kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{LIERO_MENU_VALUE_FRAMES, LIERO_MENU_VALUE_PERCENTAGE, LIERO_MENU_VALUE_TIME};

    fn text(b: &ItemBehavior, v: i32) -> String {
        String::from_utf8(b.value_text(v)).unwrap()
    }

    #[test]
    fn switch() {
        let names: [&[u8]; 2] = [b"OFF", b"ON"];
        let b = ItemBehavior::switch(&names);
        assert_eq!(text(&b, 0), "OFF");
        let (v, a) = b.left_right(0, -1, 0);
        assert_eq!(
            a,
            ItemAction {
                update_item: true,
                ..Default::default()
            }
        );
        assert_eq!(text(&b, v), "ON");
        let (v, _) = b.enter(v);
        assert_eq!(text(&b, v), "OFF");
    }

    #[test]
    fn integer() {
        let mut b = ItemBehavior::integer(1, 999);
        let (v, a) = b.left_right(5, 1, 0);
        assert_eq!(
            a,
            ItemAction {
                keep_going: true,
                update_item: true,
                ..Default::default()
            }
        );
        let (v, a) = b.left_right(v, 1, 1);
        assert_eq!(
            a,
            ItemAction {
                keep_going: true,
                ..Default::default()
            },
            "waits for the repeat"
        );
        assert_eq!(
            b.enter(v),
            (
                v,
                ItemAction {
                    enter_digits: 3,
                    ..Default::default()
                }
            )
        );
        b.max = 1000;
        assert_eq!(b.enter(v).1.enter_digits, 4);
        b.allow_entry = false;
        assert_eq!(b.enter(v), (v, ItemAction::default()));

        assert_eq!(v, 6);
        assert_eq!(text(&b, v), "6");
        b.value_kind = LIERO_MENU_VALUE_PERCENTAGE;
        assert_eq!(text(&b, v), "6%");
        b.value_kind = LIERO_MENU_VALUE_TIME;
        assert_eq!(text(&b, 125), "02:05");
        b.value_kind = LIERO_MENU_VALUE_FRAMES;
        assert!(!text(&b, 125).is_empty());
    }

    #[test]
    fn enums() {
        let names: [&[u8]; 3] = [b"A", b"B", b"C"];
        let mut b = ItemBehavior::enumeration(0, 2, &names);
        assert_eq!(text(&b, 2), "C");
        let (v, a) = b.enter(2);
        assert_eq!(
            a,
            ItemAction {
                update_menu: true,
                ..Default::default()
            }
        );
        assert_eq!(text(&b, v), "A");
        let (v, _) = b.left_right(v, -1, 0);
        assert_eq!(text(&b, v), "C");

        b.names = &[];
        b.min = 2;
        b.max = 2;
        assert_eq!(b.enter(2), (2, ItemAction::default()), "one value: no change");
        assert_eq!(text(&b, 2), "2");
    }
}
