//! Menus (`menu/menu.cpp`, `menu/menuItem.cpp` and the item behaviours):
//! which item is selected and which are in view, typing to search, drawing
//! the items and the scroll bar, and how items change their values. Also the
//! boxes the menus' dialogs draw (`gfx.cpp`). Results have to stay
//! bit-identical to the C++ versions, which `tools/hashrun` checks.
//!
//! C++ still owns the menus and their items, and decides what each item
//! edits. A menu's state is copied in and out as a `LieroMenu`, and its items
//! as `LieroMenuItem`s; only their `visible` flags are written back.

use std::ffi::c_void;

use crate::gfx::{
    liero_gfx_blit_image_no_key_colour, liero_gfx_draw_rounded_box, liero_gfx_fill_rect, LieroBitmap,
};
use crate::text::{time_to_string, time_to_string_ex, LieroFont, LieroStr};

/// A menu's state, the fields of C++'s `Menu`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroMenu {
    pub x: i32,
    pub y: i32,
    /// How many items fit.
    pub height: i32,
    pub item_height: i32,
    /// Where values are drawn, from `x`.
    pub value_offset_x: i32,
    /// The first item in view, and the one after the last, counting visible
    /// items only.
    pub top_item: i32,
    pub bottom_item: i32,
    pub visible_item_count: i32,
    /// The selected item, counting all items.
    pub selection: i32,
    /// When the search text was last typed, in milliseconds.
    pub search_time: u32,
    pub centered: bool,
}

/// A menu item, laid out for Rust.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroMenuItem {
    pub string: LieroStr,
    pub value: LieroStr,
    pub color: u8,
    /// The colour when the menu is disabled.
    pub dis_colour: u8,
    pub has_value: bool,
    pub visible: bool,
    pub selectable: bool,
}

/// Draws what a menu adds after item `item` (a position in the item list).
#[repr(C)]
pub struct LieroMenuHooks {
    pub ctx: *mut c_void,
    pub draw_overlay: unsafe extern "C" fn(ctx: *mut c_void, item: i32, x: i32, y: i32, selected: bool, disabled: bool),
}

/// How typing a key changed the search text: keep its first `keep` bytes,
/// then add the key if `add`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroMenuSearch {
    pub keep: usize,
    pub add: bool,
}

/// A menu with its items, as Rust works on it.
pub(crate) struct Menu<'a> {
    pub m: &'a mut LieroMenu,
    pub items: &'a mut [LieroMenuItem],
}

impl Menu<'_> {
    fn len(&self) -> i32 {
        self.items.len() as i32
    }

    fn item(&self, i: i32) -> Option<&LieroMenuItem> {
        usize::try_from(i).ok().and_then(|i| self.items.get(i))
    }

    fn can_select(&self, i: i32) -> bool {
        self.item(i).is_some_and(|it| it.visible && it.selectable)
    }

    /// The first visible, selectable item from `item` on, or the item count.
    pub fn first_visible_from(&self, item: i32) -> i32 {
        if item < 0 {
            // C++ counted from a huge unsigned number.
            return self.len();
        }
        (item..self.len()).find(|&i| self.can_select(i)).unwrap_or(self.len())
    }

    /// How many visible items are before `item`.
    pub fn visible_item_index(&self, item: i32) -> i32 {
        self.items.iter().take(item.max(0) as usize).filter(|it| it.visible).count() as i32
    }

    /// The item that is `idx` visible items from the start, or the item count.
    pub fn item_from_visible_index(&self, mut idx: i32) -> i32 {
        for (i, it) in self.items.iter().enumerate() {
            if !it.visible {
                continue;
            }
            if idx == 0 {
                return i as i32;
            }
            idx -= 1;
        }
        self.len()
    }

    pub fn set_top(&mut self, top: i32) {
        let top = top.min(self.m.visible_item_count - self.m.height).max(0);
        self.m.top_item = top;
        self.m.bottom_item = (top + self.m.height).min(self.m.visible_item_count);
    }

    pub fn set_bottom(&mut self, bottom: i32) {
        self.set_top(bottom - self.m.height);
    }

    pub fn is_in_view(&self, item: i32) -> bool {
        let v = self.visible_item_index(item);
        v >= self.m.top_item && v < self.m.bottom_item
    }

    pub fn ensure_in_view(&mut self, item: i32) {
        if !self.item(item).is_some_and(|it| it.visible) {
            return;
        }
        let v = self.visible_item_index(item);
        if v < self.m.top_item {
            self.set_top(v);
        } else if v >= self.m.bottom_item {
            self.set_bottom(v + 1);
        }
    }

    pub fn move_to(&mut self, item: i32) {
        let item = item.max(0).min(self.len() - 1);
        self.m.selection = self.first_visible_from(item);
        self.ensure_in_view(self.m.selection);
    }

    /// Selects the next selectable item in `direction`, wrapping around.
    pub fn movement(&mut self, direction: i32) {
        let sel = self.m.selection;
        let found = if direction < 0 {
            (0..sel).rev().chain((sel + 1..self.len()).rev()).find(|&i| self.can_select(i))
        } else if direction > 0 {
            (sel + 1..self.len()).chain(0..sel).find(|&i| self.can_select(i))
        } else {
            None
        };
        if let Some(i) = found {
            self.move_to(i);
        }
    }

    /// Moves half a page.
    pub fn movement_page(&mut self, direction: i32) {
        let offset = direction * (self.m.height / 2);
        let sel = self.visible_item_index(self.m.selection) + offset;
        self.set_top(self.m.top_item + offset);
        let sel = sel.max(0).min(self.m.visible_item_count - 1);
        self.move_to(self.item_from_visible_index(sel));
    }

    pub fn scroll(&mut self, amount: i32) {
        self.set_top(self.m.top_item + amount);
    }

    pub fn set_visibility(&mut self, item: i32, state: bool) {
        let Some(&it) = self.item(item) else { return };
        if it.visible && !state {
            self.m.visible_item_count -= 1;
        } else if !it.visible && state {
            self.m.visible_item_count += 1;
        }
        let real_top = self.item_from_visible_index(self.m.top_item);
        self.items[item as usize].visible = state;
        self.set_top(self.visible_item_index(real_top));
        self.ensure_in_view(self.m.selection);
    }

    /// Where `item` is drawn, if in view.
    pub fn item_position(&self, item: i32) -> Option<(i32, i32)> {
        if !self.is_in_view(item) {
            return None;
        }
        let v = self.visible_item_index(item);
        Some((self.m.x, self.m.y + (v - self.m.top_item) * self.m.item_height))
    }

    /// Selects the next visible item, from the selection on (after it for
    /// Tab), whose name starts with or, if `contains`, contains the search
    /// text plus `key` (None for Tab), ignoring case: one key of
    /// `Menu::onKeys`. `prefix` is the search text so far.
    pub unsafe fn search(&mut self, prefix: &[u8], key: Option<u8>, contains: bool, time: u32) -> LieroMenuSearch {
        let mut keep = prefix.len();
        if key.is_some() && time.wrapping_sub(self.m.search_time) > 1500 {
            keep = 0;
        }

        loop {
            let was_empty = keep == 0;
            let mut text = prefix[..keep].to_vec();
            text.extend(key);
            self.m.search_time = time;

            let skip = if key.is_none() { 1 } else { 0 };
            let len = self.len();
            let found = (skip..len).map(|offs| (self.m.selection + offs) % len).find(|&i| {
                let it = &self.items[i as usize];
                let name = it.string.bytes();
                it.visible && name.len() >= text.len() && matches(name, &text, contains)
            });

            if let Some(i) = found {
                self.move_to(i);
                return LieroMenuSearch { keep, add: key.is_some() };
            }

            keep = 0;
            if was_empty {
                return LieroMenuSearch { keep: 0, add: false };
            }
        }
    }

    /// One item: `MenuItem::draw`.
    unsafe fn draw_item(
        &self,
        scr: &LieroBitmap,
        font: &LieroFont,
        item: &LieroMenuItem,
        mut x: i32,
        y: i32,
        selected: bool,
        disabled: bool,
    ) {
        let string = item.string.bytes();
        let value = item.value.bytes();
        let wid = font.dims(string).0;
        let value_wid = font.dims(value).0;
        if self.m.centered {
            x -= wid >> 1;
        }
        let value_x = x + self.m.value_offset_x - (value_wid >> 1);

        if selected {
            liero_gfx_draw_rounded_box(scr, x, y, 0, 7, wid);
            if item.has_value {
                liero_gfx_draw_rounded_box(scr, value_x, y, 0, 7, value_wid);
            }
        } else {
            font.text(scr, string, x + 3, y + 2, 0, 1);
            if item.has_value {
                font.text(scr, value, value_x + 3, y + 2, 0, 1);
            }
        }

        let c = if disabled {
            item.dis_colour
        } else if selected {
            168
        } else {
            item.color
        };
        font.text(scr, string, x + 2, y + 1, c as i32, 1);
        if item.has_value {
            font.text(scr, value, value_x + 2, y + 1, c as i32, 1);
        }
    }

    /// The items in view and the scroll bar: `Menu::draw`. `x` < 0 means the
    /// menu's own position.
    pub unsafe fn draw(
        &self,
        scr: &LieroBitmap,
        font: &LieroFont,
        hooks: &LieroMenuHooks,
        disabled: bool,
        x: i32,
        show_disabled_selection: bool,
    ) {
        let m = &*self.m;
        let x = if x < 0 { m.x } else { x };
        let mut items_left = m.height;
        let mut cur_y = m.y;

        let mut c = self.item_from_visible_index(m.top_item);
        while items_left > 0 && c < self.len() {
            let item = &self.items[c as usize];
            if item.visible {
                items_left -= 1;
                let selected = c == m.selection && (!disabled || show_disabled_selection);
                self.draw_item(scr, font, item, x, cur_y, selected, disabled);
                (hooks.draw_overlay)(hooks.ctx, c, x, cur_y, selected, disabled);
                cur_y += m.item_height;
            }
            c += 1;
        }

        if m.visible_item_count > m.height {
            let menu_height = m.height * m.item_height + 1;
            font.char(scr, 22, x - 6, m.y + 2, 0);
            font.char(scr, 22, x - 7, m.y + 1, 50);
            font.char(scr, 23, x - 6, m.y + menu_height - 7, 0);
            font.char(scr, 23, x - 7, m.y + menu_height - 8, 50);

            let bar_height = menu_height - 17;
            let tab_height = (m.height * bar_height / m.visible_item_count).min(bar_height).max(0);
            let tab_y = m.y + m.top_item * bar_height / m.visible_item_count;
            liero_gfx_fill_rect(scr, x - 7, tab_y + 9, 7, tab_height, 0);
            liero_gfx_fill_rect(scr, x - 8, tab_y + 8, 7, tab_height, 7);
        }
    }
}

/// `toupper` in the C locale.
fn same_letter(a: u8, b: u8) -> bool {
    a.to_ascii_uppercase() == b.to_ascii_uppercase()
}

fn matches(name: &[u8], text: &[u8], contains: bool) -> bool {
    if contains {
        text.is_empty() || name.windows(text.len()).any(|w| w.iter().zip(text).all(|(&a, &b)| same_letter(a, b)))
    } else {
        name.iter().zip(text).all(|(&a, &b)| same_letter(a, b))
    }
}

unsafe fn menu<'a>(m: &'a mut LieroMenu, items: *mut LieroMenuItem, count: i32) -> Menu<'a> {
    let items = if count <= 0 || items.is_null() {
        &mut [][..]
    } else {
        std::slice::from_raw_parts_mut(items, count as usize)
    };
    Menu { m, items }
}

/// The C functions below take a menu's state and its `count` items, which
/// must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_move_to(m: &mut LieroMenu, items: *mut LieroMenuItem, count: i32, item: i32) {
    menu(m, items, count).move_to(item);
}

/// Selects the next selectable item up (`direction` < 0) or down.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_movement(m: &mut LieroMenu, items: *mut LieroMenuItem, count: i32, direction: i32) {
    menu(m, items, count).movement(direction);
}

/// Moves half a page up or down.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_movement_page(
    m: &mut LieroMenu,
    items: *mut LieroMenuItem,
    count: i32,
    direction: i32,
) {
    menu(m, items, count).movement_page(direction);
}

/// Scrolls so the visible item `top` is first.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_set_top(m: &mut LieroMenu, items: *mut LieroMenuItem, count: i32, top: i32) {
    menu(m, items, count).set_top(top);
}

/// Scrolls by `amount` items.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_scroll(m: &mut LieroMenu, items: *mut LieroMenuItem, count: i32, amount: i32) {
    menu(m, items, count).scroll(amount);
}

/// Shows or hides `item`.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_set_visibility(
    m: &mut LieroMenu,
    items: *mut LieroMenuItem,
    count: i32,
    item: i32,
    state: bool,
) {
    menu(m, items, count).set_visibility(item, state);
}

/// Where `item` is drawn into `x` and `y`, or false if it is out of view.
///
/// # Safety
/// See `liero_menu_move_to`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_item_position(
    m: &mut LieroMenu,
    items: *mut LieroMenuItem,
    count: i32,
    item: i32,
    x: &mut i32,
    y: &mut i32,
) -> bool {
    match menu(m, items, count).item_position(item) {
        Some((px, py)) => {
            (*x, *y) = (px, py);
            true
        }
        None => false,
    }
}

/// One key typed to search the menu: `key` is the character, or -1 for Tab.
/// `prefix` is the search text so far, `prefix_len` bytes.
///
/// # Safety
/// See `liero_menu_move_to`; `prefix` holds `prefix_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_search(
    m: &mut LieroMenu,
    items: *mut LieroMenuItem,
    count: i32,
    prefix: *const u8,
    prefix_len: usize,
    key: i32,
    contains: bool,
    time: u32,
) -> LieroMenuSearch {
    let prefix = LieroStr { ptr: prefix, len: prefix_len };
    let key = (key >= 0).then_some(key as u8);
    menu(m, items, count).search(prefix.bytes(), key, contains, time)
}

/// Draws the menu: `Menu::draw`.
///
/// # Safety
/// See `liero_menu_move_to` and `LieroBitmap`; the items' strings must be
/// valid.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw(
    m: &mut LieroMenu,
    items: *mut LieroMenuItem,
    count: i32,
    scr: &LieroBitmap,
    font: &LieroFont,
    hooks: &LieroMenuHooks,
    disabled: bool,
    x: i32,
    show_disabled_selection: bool,
) {
    menu(m, items, count).draw(scr, font, hooks, disabled, x, show_disabled_selection);
}

/// A number item's value after pressing left or right: `IntegerBehavior`.
/// Held keys repeat every `scroll_interval` menu frames.
#[no_mangle]
pub extern "C" fn liero_menu_integer_left_right(
    v: i32,
    min: i32,
    max: i32,
    step: i32,
    dir: i32,
    menu_cycles: u32,
    scroll_interval: i32,
) -> i32 {
    if scroll_interval == 0 || menu_cycles % scroll_interval as u32 != 0 {
        return v;
    }
    if (dir < 0 && v > min) || (dir > 0 && v < max) {
        v.wrapping_add(dir.wrapping_mul(step))
    } else {
        v
    }
}

/// A choice item's value after moving `dir` choices, wrapping around from
/// `max` to `min`: `EnumBehavior::change`.
#[no_mangle]
pub extern "C" fn liero_menu_enum_change(v: u32, min: u32, max: u32, dir: i32) -> u32 {
    let range = max.wrapping_sub(min).wrapping_add(1);
    if range == 0 {
        return v;
    }
    (v.wrapping_add(dir as u32).wrapping_add(range).wrapping_sub(min) % range).wrapping_add(min)
}

/// Kinds of value text for `liero_menu_value_text`.
pub const LIERO_MENU_VALUE_INTEGER: i32 = 0;
pub const LIERO_MENU_VALUE_PERCENTAGE: i32 = 1;
/// Seconds as "MM:SS".
pub const LIERO_MENU_VALUE_TIME: i32 = 2;
/// Frames as seconds with hundredths.
pub const LIERO_MENU_VALUE_FRAMES: i32 = 3;

/// How a number item shows `v`, written with a NUL to `out` (`cap` bytes).
/// Returns the length.
///
/// # Safety
/// `out` must hold `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_value_text(v: i32, kind: i32, out: *mut u8, cap: usize) -> usize {
    let text = match kind {
        LIERO_MENU_VALUE_PERCENTAGE => format!("{v}%").into_bytes(),
        LIERO_MENU_VALUE_TIME => time_to_string(v).to_vec(),
        LIERO_MENU_VALUE_FRAMES => time_to_string_ex(v.wrapping_mul(14), false, false),
        _ => v.to_string().into_bytes(),
    };
    if cap == 0 {
        return 0;
    }
    let n = text.len().min(cap - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, n);
    *out.add(n) = 0;
    n
}

/// Edit distance between `a` and `b`, ignoring case: `levenshtein`.
pub fn levenshtein(a: &[u8], b: &[u8]) -> u32 {
    let w = a.len() + 1;
    let mut m = vec![0u32; w * (b.len() + 1)];
    for x in 1..=b.len() {
        m[x * w] = m[(x - 1) * w] + 1;
    }
    for y in 1..=a.len() {
        m[y] = m[y - 1] + 1;
    }
    for x in 1..=b.len() {
        for y in 1..=a.len() {
            let c = (a[y - 1].to_ascii_lowercase() != b[x - 1].to_ascii_lowercase()) as u32;
            m[x * w + y] = (m[(x - 1) * w + y] + 1).min(m[x * w + y - 1] + 1).min(m[(x - 1) * w + y - 1] + c);
        }
    }
    m[b.len() * w + a.len()]
}

/// The name closest to `search` for typing a weapon's name, relative to the
/// name's length; the first of equally close ones. Returns -1 if none
/// compares (C++ left the result unset then).
///
/// # Safety
/// `names` holds `count` valid strings, and `search` `search_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_closest_name(
    names: *const LieroStr,
    count: i32,
    search: *const u8,
    search_len: usize,
) -> i32 {
    let search = LieroStr { ptr: search, len: search_len };
    let names = if count <= 0 { &[][..] } else { std::slice::from_raw_parts(names, count as usize) };
    let mut best = -1;
    let mut minimum = f64::MAX;
    for (i, name) in names.iter().enumerate() {
        let name = name.bytes();
        let dist = levenshtein(name, search.bytes()) as f64 / name.len() as f64;
        if dist < minimum {
            best = i as i32;
            minimum = dist;
        }
    }
    best
}

/// A colour component's bar in the player menu: `PlayerMenu::drawItemOverlay`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw_rgb_overlay(
    scr: &LieroBitmap,
    x: i32,
    y: i32,
    selected: bool,
    value: i32,
    color: i32,
) {
    liero_gfx_draw_rounded_box(scr, x + 24, y, if selected { 168 } else { 0 }, 7, value - 1);
    liero_gfx_fill_rect(scr, x + 25, y + 1, value, 5, color as u8);
}

/// The palette entry shown in the options menu: `HiddenMenu::drawItemOverlay`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw_palette_overlay(scr: &LieroBitmap, x: i32, y: i32, selected: bool, color: i32) {
    let w = 30;
    let offs_x = 44;
    liero_gfx_draw_rounded_box(scr, x + offs_x, y, if selected { 168 } else { 0 }, 7, w);
    liero_gfx_fill_rect(scr, x + offs_x + 1, y + 1, w + 1, 5, color as u8);
}

/// The level, "A vs B" and the worms' colours, twice as big, as the spectator
/// window shows them: `drawVersusInfo`. `level` is the level's line of text.
///
/// # Safety
/// See `LieroBitmap`; the strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw_versus(
    scr: &LieroBitmap,
    font: &LieroFont,
    level: LieroStr,
    name0: LieroStr,
    name1: LieroStr,
    color0: i32,
    color1: i32,
    center_x: i32,
    center_y: i32,
) {
    font.centered(scr, level.bytes(), center_x, center_y - 32, 7, 2);

    let mut vs = name0.bytes().to_vec();
    vs.extend_from_slice(b" vs ");
    vs.extend_from_slice(name1.bytes());
    // Room for the colour boxes even without names.
    let size = (font.dims(&vs).0 * 2).max(48);
    font.centered(scr, &vs, center_x, center_y, 7, 2);
    liero_gfx_fill_rect(scr, center_x - size / 2 - 1, center_y + 22, 16, 16, 7);
    liero_gfx_fill_rect(scr, center_x - size / 2, center_y + 23, 14, 14, color0 as u8);
    liero_gfx_fill_rect(scr, center_x + size / 2 - 17, center_y + 22, 16, 16, 7);
    liero_gfx_fill_rect(scr, center_x + size / 2 - 16, center_y + 23, 14, 14, color1 as u8);
}

/// A message in a box centred on (x, y): `drawInfoBox`.
///
/// # Safety
/// See `LieroBitmap`; `text` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw_info_box(scr: &LieroBitmap, font: &LieroFont, text: LieroStr, x: i32, y: i32) {
    let text = text.bytes();
    let (width, height) = font.dims(text);
    let cx = x - width / 2 - 2;
    let cy = y - height / 2 - 2;
    liero_gfx_draw_rounded_box(scr, cx, cy, 0, height + 1, width + 1);
    font.text(scr, text, cx + 2, cy + 2, 6, 1);
}

/// Text being typed in a box at (x, y), over a strip of `frozen`, the screen
/// before the box: `drawInputBox`.
///
/// # Safety
/// See `LieroBitmap`; `text` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_menu_draw_input_box(
    scr: &LieroBitmap,
    frozen: &LieroBitmap,
    font: &LieroFont,
    text: LieroStr,
    x: i32,
    y: i32,
    centered: bool,
) {
    let text = text.bytes();
    let width = font.dims(text).0;
    let adjust = if centered { width / 2 } else { 0 };
    let clr_x = x - 10 - adjust;

    // C++ copies clr_x + 10 + width pixels a row, 8 rows, from (clr_x, y) of
    // `frozen`, so this is a width, not an x. Skip it where it would read
    // outside `frozen`.
    let copy_w = clr_x + 10 + width;
    let start = y as i64 * frozen.pitch as i64 + clr_x as i64;
    let end = start + 7 * frozen.pitch as i64 + copy_w as i64;
    if start >= 0 && end <= frozen.pitch as i64 * frozen.h as i64 {
        let mem = frozen.pixels.offset(start as isize);
        liero_gfx_blit_image_no_key_colour(scr, mem, clr_x, y, copy_w, 8, frozen.pitch);
    }

    liero_gfx_draw_rounded_box(scr, x - 2 - adjust, y, 0, 7, width);
    font.text(scr, text, x - adjust, y + 1, 50, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &'static [u8]) -> LieroStr {
        LieroStr { ptr: text.as_ptr(), len: text.len() }
    }

    fn item(name: &'static [u8]) -> LieroMenuItem {
        LieroMenuItem {
            string: s(name),
            value: s(b""),
            color: 48,
            dis_colour: 7,
            has_value: false,
            visible: true,
            selectable: true,
        }
    }

    fn state(items: &[LieroMenuItem], height: i32) -> LieroMenu {
        let mut m = LieroMenu { height, item_height: 8, ..Default::default() };
        m.visible_item_count = items.iter().filter(|i| i.visible).count() as i32;
        let mut menu = Menu { m: &mut m, items: &mut items.to_vec() };
        menu.set_top(0);
        m
    }

    const NAMES: [&[u8]; 6] = [b"BAZOOKA", b"MINIGUN", b"MINE", b"LASER", b"LARPA", b"ZIMM"];

    #[test]
    fn movement_wraps_and_skips_hidden_items() {
        let mut items: Vec<_> = NAMES.iter().map(|n| item(n)).collect();
        items[1].selectable = false;
        items[4].visible = false;
        let mut m = state(&items, 3);
        m.visible_item_count = 5;
        let mut menu = Menu { m: &mut m, items: &mut items };

        menu.move_to(0);
        menu.movement(1);
        assert_eq!(menu.m.selection, 2, "item 1 can't be selected");
        menu.movement(1);
        menu.movement(1);
        assert_eq!(menu.m.selection, 5, "item 4 is hidden");
        assert_eq!((menu.m.top_item, menu.m.bottom_item), (2, 5));
        menu.movement(1);
        assert_eq!(menu.m.selection, 0, "wraps around");
        assert_eq!(menu.m.top_item, 0);
        menu.movement(-1);
        assert_eq!(menu.m.selection, 5);
    }

    #[test]
    fn visibility_keeps_the_view() {
        let mut items: Vec<_> = NAMES.iter().map(|n| item(n)).collect();
        let mut m = state(&items, 3);
        let mut menu = Menu { m: &mut m, items: &mut items };
        menu.move_to(5);
        assert_eq!(menu.m.top_item, 3);
        menu.set_visibility(0, false);
        assert_eq!(menu.m.visible_item_count, 5);
        assert_eq!(menu.m.top_item, 2, "the same item stays on top");
        assert_eq!(menu.item_position(5), Some((0, 16)));
        assert_eq!(menu.item_position(1), None);
    }

    #[test]
    fn typing_searches_names() {
        let mut items: Vec<_> = NAMES.iter().map(|n| item(n)).collect();
        let mut m = state(&items, 6);
        let mut menu = Menu { m: &mut m, items: &mut items };
        menu.move_to(0);
        unsafe {
            let r = menu.search(b"", Some(b'm'), false, 100);
            assert_eq!((menu.m.selection, r), (1, LieroMenuSearch { keep: 0, add: true }));
            let r = menu.search(b"m", Some(b'i'), false, 200);
            assert_eq!((menu.m.selection, r), (1, LieroMenuSearch { keep: 1, add: true }));
            // Tab finds the next match.
            let r = menu.search(b"mi", None, false, 300);
            assert_eq!((menu.m.selection, r), (2, LieroMenuSearch { keep: 2, add: false }));
            // No name starts with "MIX", so the search starts over from "X".
            let r = menu.search(b"mi", Some(b'x'), false, 400);
            assert_eq!((menu.m.selection, r), (2, LieroMenuSearch { keep: 0, add: false }));
            // After 1.5 seconds the old text is dropped.
            let r = menu.search(b"la", Some(b'z'), false, 2000);
            assert_eq!((menu.m.selection, r), (5, LieroMenuSearch { keep: 0, add: true }));
            // File selectors match anywhere in the name.
            let r = menu.search(b"", Some(b'k'), true, 2100);
            assert_eq!((menu.m.selection, r), (0, LieroMenuSearch { keep: 0, add: true }));
        }
    }

    #[test]
    fn centered_menus_center_each_item() {
        use crate::game::LieroRect;
        use crate::gfx::LieroFontChar;

        unsafe extern "C" fn no_overlay(_: *mut c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}
        let chars = vec![LieroFontChar { data: [1; 56], width: 8 }; 250];
        let font = LieroFont { chars: chars.as_ptr(), count: chars.len() };
        let hooks = LieroMenuHooks { ctx: std::ptr::null_mut(), draw_overlay: no_overlay };

        for (centered, first) in [(false, 102), (true, 94)] {
            let mut items = vec![item(b"AB"), item(b"MINE")];
            let mut m = state(&items, 5);
            m.x = 100;
            m.y = 20;
            m.selection = 1;
            m.centered = centered;
            let mut px = vec![255u8; 200 * 50];
            let scr = LieroBitmap {
                pixels: px.as_mut_ptr(),
                w: 200,
                h: 50,
                pitch: 200,
                clip: LieroRect { x1: 0, y1: 0, x2: 200, y2: 50 },
            };
            unsafe { Menu { m: &mut m, items: &mut items }.draw(&scr, &font, &hooks, false, -1, false) };
            // "AB" is 16 pixels wide; its text starts 2 pixels in.
            assert_eq!((px[21 * 200 + first - 1], px[21 * 200 + first]), (255, 48), "centered: {centered}");
        }
    }

    #[test]
    fn negative_positions_select_nothing() {
        let mut items: Vec<_> = NAMES.iter().map(|n| item(n)).collect();
        let mut m = state(&items, 6);
        let menu = Menu { m: &mut m, items: &mut items };
        assert_eq!(menu.first_visible_from(-1), 6);
        let mut none: Vec<LieroMenuItem> = Vec::new();
        let mut m = state(&none, 6);
        let mut empty = Menu { m: &mut m, items: &mut none };
        empty.move_to(3);
        assert_eq!(empty.m.selection, 0, "the item count, as in C++");
    }

    #[test]
    fn search_text_lasts_one_and_a_half_seconds() {
        let mut items: Vec<_> = NAMES.iter().map(|n| item(n)).collect();
        let mut m = state(&items, 6);
        let mut menu = Menu { m: &mut m, items: &mut items };
        unsafe {
            menu.search(b"", Some(b'l'), false, 1000);
            let r = menu.search(b"l", Some(b'a'), false, 2500);
            assert_eq!(r, LieroMenuSearch { keep: 1, add: true }, "1500 ms later the text is kept");
            assert_eq!(menu.m.selection, 3);
            let r = menu.search(b"la", Some(b'z'), false, 4001);
            assert_eq!(r, LieroMenuSearch { keep: 0, add: true }, "1501 ms later it starts over");
            assert_eq!(menu.m.selection, 5);
        }
    }

    #[test]
    fn values_change_like_cpp() {
        assert_eq!(liero_menu_integer_left_right(5, 0, 10, 2, 1, 10, 5), 7);
        assert_eq!(liero_menu_integer_left_right(5, 0, 10, 2, 1, 11, 5), 5, "waits for the repeat");
        assert_eq!(liero_menu_integer_left_right(10, 0, 10, 2, 1, 0, 5), 10);
        assert_eq!(liero_menu_integer_left_right(9, 0, 10, 2, 1, 0, 5), 11, "C++ can pass max by a step");
        assert_eq!(liero_menu_enum_change(3, 0, 3, 1), 0);
        assert_eq!(liero_menu_enum_change(0, 0, 3, -1), 3);
        assert_eq!(liero_menu_enum_change(1, 1, 40, -1), 40);

        let mut out = [0u8; 16];
        let text = |v, kind, out: &mut [u8; 16]| unsafe {
            let n = liero_menu_value_text(v, kind, out.as_mut_ptr(), 16);
            out[..n].to_vec()
        };
        assert_eq!(text(150, LIERO_MENU_VALUE_PERCENTAGE, &mut out), b"150%");
        assert_eq!(text(-3, LIERO_MENU_VALUE_INTEGER, &mut out), b"-3");
        assert_eq!(text(754, LIERO_MENU_VALUE_TIME, &mut out), b"12:34");
        assert_eq!(text(700, LIERO_MENU_VALUE_FRAMES, &mut out), b"09.80");
    }

    #[test]
    fn closest_weapon_name() {
        assert_eq!(levenshtein(b"kitten", b"SITTING"), 3);
        assert_eq!(levenshtein(b"", b"abc"), 3);
        let names: Vec<_> = NAMES.iter().map(|n| s(n)).collect();
        let find = |t: &[u8]| unsafe { liero_menu_closest_name(names.as_ptr(), names.len() as i32, t.as_ptr(), t.len()) };
        assert_eq!(find(b"lasr"), 3);
        assert_eq!(find(b"MINI"), 2, "relative to the name's length");
        assert_eq!(find(b"zim"), 5);
    }
}
