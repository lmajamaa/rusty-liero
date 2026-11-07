//! The file selectors (a level, a profile, a replay, saved options, a TC) as
//! screens: the folder tree, its lists, the keys and the drawing (they were
//! `FileSelector` and the selector screens in C++).
//!
//! Listing a folder and what choosing a file does stay in C++ for now
//! (`LieroSelectorHooks`): C++'s file layer also reads `.zip` files as
//! folders, and choosing loads a level, profile, replay, options or TC.

use std::ffi::c_void;

use crate::gfx::{liero_gfx_draw_rounded_box, LieroBitmap};
use crate::menu::{
    liero_menu_draw, liero_menu_move_to, liero_menu_movement, liero_menu_movement_page, liero_menu_search,
    liero_menu_set_top, LieroMenu, LieroMenuHooks, LieroMenuItem,
};
use crate::screen::{into_ffi, Host, LieroScreen, Screen, Tick};
use crate::text::{LieroFont, LieroStr};

// SDL's scancodes.
const SCANCODE_RETURN: u32 = 40;
const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_TAB: u32 = 43;
const SCANCODE_RIGHT: u32 = 79;
const SCANCODE_LEFT: u32 = 80;
const SCANCODE_DOWN: u32 = 81;
const SCANCODE_UP: u32 = 82;
const SCANCODE_PAGEUP: u32 = 75;
const SCANCODE_PAGEDOWN: u32 = 78;
const SCANCODE_KP_ENTER: u32 = 88;

const SOUND_UP: i32 = 26;
const SOUND_DOWN: i32 = 25;
const SOUND_CHOICE: i32 = 27;

/// Where the lists and titles are.
const LIST_X: i32 = 178;
const PARENT_X: i32 = 28;
const TITLE_Y: i32 = 20;
const TITLE_COLOR: i32 = 50;

/// What a selector calls in C++. `rel` is a path below the root, its parts
/// joined with '/'.
#[repr(C)]
pub struct LieroSelectorHooks {
    pub ctx: *mut c_void,
    /// Lists folder `rel`: `add(list, name, is_dir)` for each entry, with a
    /// `.zip` file as a folder without its extension.
    pub list: Option<
        unsafe extern "C" fn(
            ctx: *mut c_void,
            rel: LieroStr,
            list: *mut c_void,
            add: unsafe extern "C" fn(list: *mut c_void, name: LieroStr, is_dir: bool),
        ),
    >,
    /// Whether `rel` exists.
    pub exists: Option<unsafe extern "C" fn(ctx: *mut c_void, rel: LieroStr) -> bool>,
    /// The cursor moved to file `rel`: show what it is.
    pub preview: Option<unsafe extern "C" fn(ctx: *mut c_void, rel: LieroStr)>,
    /// Enter on file `rel`, whose full path is `full_path` and name `name`,
    /// or on the random level. Returns what the screen closes with.
    pub choose: Option<
        unsafe extern "C" fn(ctx: *mut c_void, rel: LieroStr, full_path: LieroStr, name: LieroStr, random: bool) -> i32,
    >,
    /// Frees `ctx` when the screen closes.
    pub free: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
}

/// What a selector lists and where it starts.
#[repr(C)]
pub struct LieroSelectorSpec {
    /// The root folder's full path.
    pub root: LieroStr,
    /// The extension of the files listed, or empty for all.
    pub filter: LieroStr,
    /// The title, before the folder's path.
    pub title: LieroStr,
    /// If not empty, a first item with this name chooses a random level.
    pub random_name: LieroStr,
    /// The full path to start at, when `has_select`, and the one to start at
    /// when that one isn't there or not given, when not empty.
    pub select: LieroStr,
    pub has_select: bool,
    pub fallback: LieroStr,
    /// Only the root's folders with a `tc.cfg`, chosen as files.
    pub tcs: bool,
    /// Enter makes the choice sound, even on a folder.
    pub enter_sound: bool,
}

fn upper(c: u8) -> u8 {
    c.to_ascii_uppercase()
}

fn ci_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| upper(*x) == upper(*y))
}

fn ci_starts_with(text: &[u8], start: &[u8]) -> bool {
    start.len() <= text.len() && ci_eq(&text[..start.len()], start)
}

fn ci_less(a: &[u8], b: &[u8]) -> bool {
    for (i, &x) in a.iter().enumerate() {
        let Some(&y) = b.get(i) else {
            return false;
        };
        match upper(x).cmp(&upper(y)) {
            std::cmp::Ordering::Less => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    b.len() > a.len()
}

fn extension(name: &[u8]) -> &[u8] {
    name.iter().rposition(|&c| c == b'.').map_or(&[][..], |i| &name[i + 1..])
}

fn basename(name: &[u8]) -> &[u8] {
    name.iter().rposition(|&c| c == b'.').map_or(name, |i| &name[..i])
}

fn join_path(root: &[u8], leaf: &[u8]) -> Vec<u8> {
    let mut r = root.to_vec();
    if !root.is_empty() && !matches!(root[root.len() - 1], b'\\' | b'/') {
        r.push(b'/');
    }
    r.extend_from_slice(leaf);
    r
}

fn lstr(b: &[u8]) -> LieroStr {
    LieroStr { ptr: b.as_ptr(), len: b.len() }
}

/// A menu whose items Rust owns: C++'s `Menu` with plain items.
struct OwnedMenu {
    m: LieroMenu,
    names: Vec<Vec<u8>>,
    colors: Vec<u8>,
    visible: Vec<bool>,
    search: Vec<u8>,
}

unsafe extern "C" fn no_overlay(_: *mut c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}

impl OwnedMenu {
    /// `Menu(178, 28)` with `setHeight(14)`, as a file list is.
    fn file_list() -> Self {
        let mut menu = OwnedMenu {
            m: LieroMenu { x: LIST_X, y: 28, height: 14, item_height: 8, ..Default::default() },
            names: Vec::new(),
            colors: Vec::new(),
            visible: Vec::new(),
            search: Vec::new(),
        };
        let top = menu.m.top_item;
        menu.with(|m, items, n| unsafe { liero_menu_set_top(m, items, n, top) });
        menu
    }

    /// Runs `f` on the menu as the Rust menu code takes it, and keeps the
    /// items' visibility it sets.
    fn with<R>(&mut self, f: impl FnOnce(&mut LieroMenu, *mut LieroMenuItem, i32) -> R) -> R {
        let mut items: Vec<LieroMenuItem> = self
            .names
            .iter()
            .zip(&self.colors)
            .zip(&self.visible)
            .map(|((name, &color), &visible)| LieroMenuItem {
                string: lstr(name),
                value: lstr(&[]),
                color,
                dis_colour: 7,
                has_value: false,
                visible,
                selectable: true,
            })
            .collect();
        let r = f(&mut self.m, items.as_mut_ptr(), items.len() as i32);
        for (v, it) in self.visible.iter_mut().zip(&items) {
            *v = it.visible;
        }
        r
    }

    fn add(&mut self, name: Vec<u8>, color: u8) {
        self.names.push(name);
        self.colors.push(color);
        self.visible.push(true);
        self.m.visible_item_count += 1;
    }

    fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    fn move_to(&mut self, i: i32) {
        self.with(|m, items, n| unsafe { liero_menu_move_to(m, items, n, i) });
    }

    fn movement(&mut self, dir: i32) {
        self.with(|m, items, n| unsafe { liero_menu_movement(m, items, n, dir) });
    }

    fn movement_page(&mut self, dir: i32) {
        self.with(|m, items, n| unsafe { liero_menu_movement_page(m, items, n, dir) });
    }

    /// The selected item, if there is one.
    fn selected(&self) -> Option<usize> {
        usize::try_from(self.m.selection).ok().filter(|&i| i < self.names.len())
    }

    /// Typing searches the list (`Menu::onKeys` with `contains`).
    fn on_keys(&mut self, host: &Host) {
        for k in host.typed_keys() {
            let tab = k.scancode == SCANCODE_TAB;
            if !((32..=127).contains(&k.sym) || tab) {
                continue;
            }
            let ch = k.sym as u8;
            let key = if tab { -1 } else { i32::from(ch) };
            let time = host.ticks();
            let search = std::mem::take(&mut self.search);
            let r = self.with(|m, items, n| unsafe {
                liero_menu_search(m, items, n, search.as_ptr(), search.len(), key, true, time)
            });
            self.search = search;
            self.search.truncate(r.keep);
            if r.add {
                self.search.push(ch);
            }
        }
    }

    fn draw(&mut self, scr: &LieroBitmap, font: &LieroFont, disabled: bool, x: i32, show_disabled_selection: bool) {
        let hooks = LieroMenuHooks { ctx: std::ptr::null_mut(), draw_overlay: no_overlay };
        self.with(|m, items, n| unsafe {
            liero_menu_draw(m, items, n, scr, font, &hooks, disabled, x, show_disabled_selection)
        });
    }
}

/// A file or folder in the tree (`FileNode`).
struct Node {
    name: Vec<u8>,
    full_path: Vec<u8>,
    /// The path below the root, for the hooks.
    rel: Vec<u8>,
    folder: bool,
    parent: Option<usize>,
    children: Vec<usize>,
    filled: bool,
    menu: OwnedMenu,
}

struct Selector {
    hooks: LieroSelectorHooks,
    nodes: Vec<Node>,
    current: usize,
    filter: Vec<u8>,
    title: Vec<u8>,
    /// The random level's node.
    random: Option<usize>,
    enter_sound: bool,
    /// The file last shown by `preview`.
    previewed: Option<usize>,
    shown: bool,
}

const ROOT: usize = 0;

unsafe extern "C" fn add_entry(list: *mut c_void, name: LieroStr, is_dir: bool) {
    (*(list as *mut Vec<(Vec<u8>, bool)>)).push((name.bytes().to_vec(), is_dir));
}

impl Selector {
    fn node(&mut self, name: Vec<u8>, full_path: Vec<u8>, rel: Vec<u8>, folder: bool, parent: Option<usize>) -> usize {
        self.nodes.push(Node {
            name,
            full_path,
            rel,
            folder,
            parent,
            children: Vec::new(),
            filled: false,
            menu: OwnedMenu::file_list(),
        });
        self.nodes.len() - 1
    }

    /// Lists folder `n` (`FileNode::fill`).
    fn fill(&mut self, n: usize) {
        let mut entries: Vec<(Vec<u8>, bool)> = Vec::new();
        if let Some(list) = self.hooks.list {
            let rel = self.nodes[n].rel.clone();
            unsafe { list(self.hooks.ctx, lstr(&rel), (&mut entries as *mut Vec<(Vec<u8>, bool)>).cast(), add_entry) };
        }
        let mut children = Vec::new();
        for (name, is_dir) in entries {
            let full_path = join_path(&self.nodes[n].full_path, &name);
            let rel = if self.nodes[n].rel.is_empty() { name.clone() } else { join_path(&self.nodes[n].rel, &name) };
            if is_dir {
                children.push(self.node(name, full_path, rel, true, Some(n)));
            } else if self.filter.is_empty() || ci_eq(extension(&name), &self.filter) {
                children.push(self.node(basename(&name).to_vec(), full_path, rel, false, Some(n)));
            }
        }
        // Folders first, then by name.
        children.sort_by(|&a, &b| {
            let (a, b) = (&self.nodes[a], &self.nodes[b]);
            if a.folder != b.folder {
                b.folder.cmp(&a.folder)
            } else if ci_less(&a.name, &b.name) {
                std::cmp::Ordering::Less
            } else if ci_less(&b.name, &a.name) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        self.nodes[n].children = children;
        self.nodes[n].filled = true;
    }

    fn ensure_filled(&mut self, n: usize) {
        if !self.nodes[n].filled && self.nodes[n].parent.is_some() {
            self.fill(n);
        }
    }

    /// The node's list, made the first time it's needed (`getMenu`).
    fn menu(&mut self, n: usize) -> &mut OwnedMenu {
        self.ensure_filled(n);
        if self.nodes[n].menu.is_empty() {
            let items: Vec<(Vec<u8>, u8)> = self.nodes[n]
                .children
                .iter()
                .map(|&c| (self.nodes[c].name.clone(), if self.nodes[c].folder { 47 } else { 48 }))
                .collect();
            let menu = &mut self.nodes[n].menu;
            for (name, color) in items {
                menu.add(name, color);
            }
            menu.move_to(0);
        }
        &mut self.nodes[n].menu
    }

    /// The node with full path `path` (`FileNode::find`).
    fn find(&mut self, n: usize, path: &[u8]) -> Option<usize> {
        if ci_eq(&self.nodes[n].full_path, path) {
            return Some(n);
        }
        if !ci_starts_with(path, &self.nodes[n].full_path) || !self.nodes[n].folder {
            return None;
        }
        self.ensure_filled(n);
        let children = self.nodes[n].children.clone();
        children.into_iter().find_map(|c| self.find(c, path))
    }

    /// Opens the folder of `path` with it selected (`FileSelector::select`).
    fn select(&mut self, path: &[u8]) -> bool {
        let Some(found) = self.find(ROOT, path) else {
            return false;
        };
        if let Some(parent) = self.nodes[found].parent {
            let mut p = found;
            while let Some(up) = self.nodes[p].parent {
                let child = p;
                p = up;
                let children = self.nodes[p].children.clone();
                for (i, &c) in children.iter().enumerate() {
                    if c == child {
                        self.menu(p).move_to(i as i32);
                    }
                }
            }
            self.current = if self.nodes[found].folder { found } else { parent };
        }
        true
    }

    /// The node selected in the open folder.
    fn cur_sel(&mut self) -> Option<usize> {
        let cur = self.current;
        let i = self.menu(cur).selected()?;
        self.nodes[cur].children.get(i).copied()
    }

    /// Opens the selected folder, or returns the selected file.
    fn enter(&mut self) -> Option<usize> {
        let c = self.cur_sel()?;
        if self.nodes[c].folder {
            self.current = c;
            return None;
        }
        Some(c)
    }

    fn exit(&mut self) {
        if let Some(p) = self.nodes[self.current].parent {
            self.current = p;
        }
    }

    /// The keys: false on Escape (`FileSelector::process`).
    fn process(&mut self, host: &Host) -> bool {
        let cur = self.current;
        if host.test_key_once(SCANCODE_UP) {
            host.play_sound(SOUND_UP);
            self.menu(cur).movement(-1);
        }
        if host.test_key_once(SCANCODE_DOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu(cur).movement(1);
        }
        if host.test_key_once(SCANCODE_PAGEUP) {
            host.play_sound(SOUND_UP);
            self.menu(cur).movement_page(-1);
        }
        if host.test_key_once(SCANCODE_PAGEDOWN) {
            host.play_sound(SOUND_DOWN);
            self.menu(cur).movement_page(1);
        }
        if host.test_key_once(SCANCODE_ESCAPE) {
            return false;
        }
        if host.test_key_once(SCANCODE_LEFT) {
            self.exit();
        }
        if host.test_key_once(SCANCODE_RIGHT) {
            self.enter();
        }
        let cur = self.current;
        self.menu(cur).on_keys(host);
        true
    }

    fn framed_text(scr: &LieroBitmap, font: &LieroFont, text: &[u8], x: i32, y: i32) {
        unsafe {
            let width = font.dims(text).0;
            liero_gfx_draw_rounded_box(scr, x, y, 0, 7, width);
            font.text(scr, text, x + 2, y + 1, TITLE_COLOR, 1);
        }
    }

    /// Previews the file under the cursor if it's new there.
    fn update_preview(&mut self) {
        let Some(preview) = self.hooks.preview else {
            return;
        };
        let cur = self.cur_sel();
        if self.previewed != cur {
            if let Some(c) = cur.filter(|&c| Some(c) != self.random && !self.nodes[c].folder) {
                unsafe { preview(self.hooks.ctx, lstr(&self.nodes[c].rel)) };
                self.previewed = Some(c);
            }
        }
    }

    fn draw(&mut self, host: &Host) {
        let (Some(scr), Some(font)) = (host.screen(), host.font()) else {
            return;
        };

        let mut title = self.title.clone();
        let path = &self.nodes[self.current].full_path;
        if !path.is_empty() {
            title.push(b' ');
            title.extend_from_slice(path);
        }
        Self::framed_text(&scr, &font, &title, LIST_X, TITLE_Y);

        self.update_preview();

        let cur = self.current;
        if let Some(parent) = self.nodes[cur].parent {
            Self::framed_text(&scr, &font, b"Parent directory", PARENT_X, TITLE_Y);
            self.menu(parent).draw(&scr, &font, true, PARENT_X, true);
        }
        self.menu(cur).draw(&scr, &font, false, LIST_X, false);
    }

    /// Enter: what the screen closes with, if it closes.
    fn choose(&mut self, host: &Host) -> Option<i32> {
        if self.enter_sound {
            host.play_sound(SOUND_CHOICE);
        }
        let c = self.enter()?;
        let random = Some(c) == self.random;
        let Some(choose) = self.hooks.choose else {
            return Some(-1);
        };
        let n = &self.nodes[c];
        Some(unsafe { choose(self.hooks.ctx, lstr(&n.rel), lstr(&n.full_path), lstr(&n.name), random) })
    }
}

/// Copies the screen as it was before the selector opened over it.
fn restore_frozen(host: &Host) {
    let (Some(scr), Some(frozen)) = (host.screen(), host.frozen()) else {
        return;
    };
    let len = |b: &LieroBitmap| b.pitch.max(0) as usize * b.h.max(0) as usize;
    let n = len(&scr).min(len(&frozen));
    if n > 0 && !scr.pixels.is_null() && !frozen.pixels.is_null() {
        unsafe { std::ptr::copy_nonoverlapping(frozen.pixels, scr.pixels, n) };
    }
}

impl Screen for Selector {
    fn tick(&mut self, host: &Host) -> Tick {
        if self.shown {
            host.process();
        }

        restore_frozen(host);
        self.draw(host);

        if !self.process(host) {
            return Tick::close(-1);
        }

        if host.test_key_once(SCANCODE_RETURN) || host.test_key_once(SCANCODE_KP_ENTER) {
            if let Some(result) = self.choose(host) {
                return Tick::close(result);
            }
        }

        host.menu_fade();
        self.shown = true;
        Tick::stay(true)
    }

    fn close(&mut self, _host: &Host) {
        if let Some(free) = self.hooks.free {
            unsafe { free(self.hooks.ctx) };
        }
    }
}

impl Selector {
    fn new(spec: &LieroSelectorSpec, hooks: LieroSelectorHooks) -> Self {
        let b = |s: &LieroStr| unsafe { s.bytes().to_vec() };
        let mut sel = Selector {
            hooks,
            nodes: Vec::new(),
            current: ROOT,
            filter: b(&spec.filter),
            title: b(&spec.title),
            random: None,
            enter_sound: spec.enter_sound,
            previewed: None,
            shown: false,
        };
        let root = sel.node(Vec::new(), b(&spec.root), Vec::new(), true, None);
        sel.fill(root);

        let random_name = b(&spec.random_name);
        if !random_name.is_empty() {
            let r = sel.node(random_name, Vec::new(), Vec::new(), false, Some(root));
            sel.nodes[root].children.insert(0, r);
            sel.random = Some(r);
        }

        if spec.tcs {
            let children = std::mem::take(&mut sel.nodes[root].children);
            let mut kept = Vec::new();
            for c in children {
                let cfg = join_path(&sel.nodes[c].rel, b"tc.cfg");
                let exists = sel.hooks.exists.is_some_and(|f| unsafe { f(sel.hooks.ctx, lstr(&cfg)) });
                if exists {
                    sel.nodes[c].folder = false;
                    kept.push(c);
                }
            }
            sel.nodes[root].children = kept;
        }

        let selected = spec.has_select && sel.select(&b(&spec.select));
        let fallback = b(&spec.fallback);
        if !selected && !fallback.is_empty() {
            sel.select(&fallback);
        }
        sel
    }
}

/// A file selector screen.
///
/// # Safety
/// The strings in `spec` must be valid; the hooks are called with `ctx`
/// until `free`.
#[no_mangle]
pub unsafe extern "C" fn liero_selector_screen(spec: &LieroSelectorSpec, hooks: LieroSelectorHooks) -> LieroScreen {
    into_ffi(Box::new(Selector::new(spec, hooks)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        // Folder (below the root) to its entries.
        static TREE: RefCell<HashMap<String, Vec<(String, bool)>>> = RefCell::new(HashMap::new());
        static CHOSEN: RefCell<Vec<String>> = RefCell::new(Vec::new());
    }

    unsafe extern "C" fn list(
        _: *mut c_void,
        rel: LieroStr,
        out: *mut c_void,
        add: unsafe extern "C" fn(*mut c_void, LieroStr, bool),
    ) {
        let rel = String::from_utf8(rel.bytes().to_vec()).unwrap();
        let entries = TREE.with(|t| t.borrow().get(&rel).cloned().unwrap_or_default());
        for (name, dir) in entries {
            add(out, lstr(name.as_bytes()), dir);
        }
    }

    unsafe extern "C" fn exists(_: *mut c_void, rel: LieroStr) -> bool {
        let rel = String::from_utf8(rel.bytes().to_vec()).unwrap();
        let (dir, file) = rel.rsplit_once('/').unwrap_or(("", &rel));
        TREE.with(|t| t.borrow().get(dir).is_some_and(|e| e.iter().any(|(n, _)| n == file)))
    }

    unsafe extern "C" fn choose(_: *mut c_void, rel: LieroStr, full: LieroStr, name: LieroStr, random: bool) -> i32 {
        let s = |x: LieroStr| String::from_utf8(x.bytes().to_vec()).unwrap();
        CHOSEN.with(|c| c.borrow_mut().push(format!("{}|{}|{}|{}", s(rel), s(full), s(name), random)));
        5
    }

    fn tree(entries: &[(&str, &[(&str, bool)])]) {
        TREE.with(|t| {
            *t.borrow_mut() = entries
                .iter()
                .map(|(dir, e)| (dir.to_string(), e.iter().map(|(n, d)| (n.to_string(), *d)).collect()))
                .collect()
        });
        CHOSEN.with(|c| c.borrow_mut().clear());
    }

    fn hooks() -> LieroSelectorHooks {
        LieroSelectorHooks {
            ctx: std::ptr::null_mut(),
            list: Some(list),
            exists: Some(exists),
            preview: None,
            choose: Some(choose),
            free: None,
        }
    }

    fn spec<'a>(filter: &'a [u8], select: &'a [u8], random: &'a [u8], tcs: bool) -> LieroSelectorSpec {
        LieroSelectorSpec {
            root: lstr(b"/cfg"),
            filter: lstr(filter),
            title: lstr(b"Select:"),
            random_name: lstr(random),
            select: lstr(select),
            has_select: !select.is_empty(),
            fallback: lstr(b""),
            tcs,
            enter_sound: false,
        }
    }

    fn names(sel: &mut Selector, n: usize) -> Vec<String> {
        sel.menu(n).names.iter().map(|b| String::from_utf8(b.clone()).unwrap()).collect()
    }

    #[test]
    fn lists_folders_first_filtered_and_sorted_without_case() {
        tree(&[("", &[("b.LEV", false), ("zeta", true), ("A.lev", false), ("notes.txt", false), ("Alpha", true)])]);
        let mut sel = Selector::new(&spec(b"LEV", b"", b"Random", false), hooks());
        assert_eq!(names(&mut sel, ROOT), ["Random", "Alpha", "zeta", "A", "b"]);
    }

    #[test]
    fn selects_a_file_in_a_subfolder_and_chooses_it() {
        tree(&[("", &[("Replays", true), ("x.lrp", false)]), ("Replays", &[("one.lrp", false), ("two.lrp", false)])]);
        let mut sel = Selector::new(&spec(b"LRP", b"/cfg/Replays/two.lrp", b"", false), hooks());
        assert_eq!(sel.nodes[sel.current].rel, b"Replays");
        assert_eq!(sel.cur_sel().map(|c| sel.nodes[c].name.clone()), Some(b"two".to_vec()));
        assert_eq!(sel.choose(&Host(Default::default())), Some(5));
        assert_eq!(CHOSEN.with(|c| c.borrow().clone()), ["Replays/two.lrp|/cfg/Replays/two.lrp|two|false"]);
        // Out to the root, where Replays stays selected.
        sel.exit();
        assert_eq!(sel.current, ROOT);
        assert_eq!(sel.cur_sel().map(|c| sel.nodes[c].name.clone()), Some(b"Replays".to_vec()));
    }

    #[test]
    fn enter_on_a_folder_opens_it() {
        tree(&[("", &[("Replays", true)]), ("Replays", &[("one.lrp", false)])]);
        let mut sel = Selector::new(&spec(b"LRP", b"", b"", false), hooks());
        assert_eq!(sel.choose(&Host(Default::default())), None);
        assert_eq!(sel.nodes[sel.current].rel, b"Replays");
    }

    #[test]
    fn random_level_is_chosen_as_random() {
        tree(&[("", &[("a.lev", false)])]);
        let mut sel = Selector::new(&spec(b"LEV", b"", b"Random", false), hooks());
        assert_eq!(sel.choose(&Host(Default::default())), Some(5));
        assert_eq!(CHOSEN.with(|c| c.borrow().clone()), ["||Random|true"]);
    }

    #[test]
    fn tcs_are_folders_with_a_tc_cfg_chosen_as_files() {
        tree(&[("", &[("good", true), ("bad", true)]), ("good", &[("tc.cfg", false)]), ("bad", &[])]);
        let mut sel = Selector::new(&spec(b"", b"", b"", true), hooks());
        assert_eq!(names(&mut sel, ROOT), ["good"]);
        assert_eq!(sel.choose(&Host(Default::default())), Some(5));
    }

    thread_local! {
        static KEYS: RefCell<Vec<u32>> = RefCell::new(Vec::new());
        static TYPED: RefCell<Vec<crate::screen::LieroTypedKey>> = RefCell::new(Vec::new());
        static PREVIEWS: RefCell<u32> = RefCell::new(0);
    }

    unsafe extern "C" fn key_once(sc: u32) -> bool {
        KEYS.with(|k| {
            let mut k = k.borrow_mut();
            let found = k.iter().position(|&x| x == sc);
            found.map(|i| k.remove(i)).is_some()
        })
    }

    unsafe extern "C" fn typed(out: *mut crate::screen::LieroTypedKey, cap: usize) -> usize {
        TYPED.with(|t| {
            let t = std::mem::take(&mut *t.borrow_mut());
            for (i, k) in t.iter().take(cap).enumerate() {
                *out.add(i) = *k;
            }
            t.len().min(cap)
        })
    }

    unsafe extern "C" fn preview(_: *mut c_void, _: LieroStr) {
        PREVIEWS.with(|p| *p.borrow_mut() += 1);
    }

    fn key_host(keys: &[u32], typed_text: &[u8]) -> Host {
        KEYS.with(|k| *k.borrow_mut() = keys.to_vec());
        TYPED.with(|t| {
            *t.borrow_mut() = typed_text
                .iter()
                .map(|&c| crate::screen::LieroTypedKey { sym: i32::from(c), scancode: 0 })
                .collect()
        });
        Host(crate::screen::LieroScreenHost {
            test_key_once: Some(key_once),
            typed_keys: Some(typed),
            ..Default::default()
        })
    }

    #[test]
    fn escape_wins_over_left_and_right() {
        tree(&[("", &[("Replays", true)]), ("Replays", &[("one.lrp", false)])]);
        let mut sel = Selector::new(&spec(b"LRP", b"/cfg/Replays/one.lrp", b"", false), hooks());
        let h = key_host(&[SCANCODE_LEFT, SCANCODE_ESCAPE], b"");
        assert!(!sel.process(&h));
        // Left wasn't read, and the folder is still open.
        assert_eq!(sel.nodes[sel.current].rel, b"Replays");
        assert_eq!(KEYS.with(|k| k.borrow().clone()), [SCANCODE_LEFT]);
    }

    #[test]
    fn typing_finds_names_containing_the_text() {
        tree(&[("", &[("alpha.lrp", false), ("beta.lrp", false), ("gamma.lrp", false)])]);
        let mut sel = Selector::new(&spec(b"LRP", b"", b"", false), hooks());
        // "mm" is only inside "gamma".
        assert!(sel.process(&key_host(&[], b"mm")));
        assert_eq!(sel.cur_sel().map(|c| sel.nodes[c].name.clone()), Some(b"gamma".to_vec()));
    }

    #[test]
    fn a_level_is_previewed_once_until_the_cursor_moves() {
        tree(&[("", &[("a.lev", false), ("b.lev", false)])]);
        let mut hk = hooks();
        hk.preview = Some(preview);
        let mut sel = Selector::new(&spec(b"LEV", b"", b"Random", false), hk);
        PREVIEWS.with(|p| *p.borrow_mut() = 0);
        // Random is selected first: nothing to preview.
        sel.update_preview();
        assert_eq!(PREVIEWS.with(|p| *p.borrow()), 0);
        sel.menu(ROOT).movement(1);
        sel.update_preview();
        sel.update_preview();
        assert_eq!(PREVIEWS.with(|p| *p.borrow()), 1);
        sel.menu(ROOT).movement(1);
        sel.update_preview();
        assert_eq!(PREVIEWS.with(|p| *p.borrow()), 2);
    }

    #[test]
    fn case_insensitive_helpers() {
        assert!(ci_eq(b"Lev", b"LEV"));
        assert!(ci_less(b"abc", b"ABD"));
        assert!(ci_less(b"ab", b"AbC"));
        assert!(!ci_less(b"abc", b"ab"));
        assert!(ci_starts_with(b"/CFG/x", b"/cfg"));
        assert_eq!(extension(b"a.b.lev"), b"lev");
        assert_eq!(basename(b"a.b.lev"), b"a.b");
        assert_eq!(join_path(b"/cfg/", b"x"), b"/cfg/x");
        assert_eq!(join_path(b"/cfg", b"x"), b"/cfg/x");
    }
}
