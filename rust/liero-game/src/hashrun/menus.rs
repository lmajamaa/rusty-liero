//! The menus (`src/game/hashmenus.cpp`): the main, match setup, player,
//! options and weapon availability menus and weapon selection, driven with
//! scripted keys from fixed seeds, with a hash of each menu's state and of
//! what is drawn after every step (lines ending in "frame"). The boxes the
//! menus open (text input, messages and the spectator window's "A vs B")
//! are drawn and hashed too. Only items whose actions don't open a dialog,
//! files or a window are used.

use super::game::add_screen;
use super::games::tc_data;
use super::{Fnv, Out};
use crate::app::App;
use crate::controller::State;
use crate::fs::Node;
use crate::game::Rect;
use crate::game_box::Game;
use crate::item::{ItemAction, ItemBehavior};
use crate::level::LevelBox;
use crate::menu::{draw_info_box, draw_input_box, draw_versus, Menu, MenuHooks, MenuItem};
use crate::menu_screen::MenuKind;
use crate::menus::Menus;
use crate::rand::Rand;
use crate::renderer::BitmapBox;
use crate::screen::Host;
use crate::settings::SettingsBox;
use crate::sfx::SoundOut;
use crate::viewport::ViewportKind;
use crate::weapsel::{level_line, Weapsel};
use crate::worm::Control;

// The menus' item ids (`menuIds.hpp`).
const MA_RESUME_GAME: i32 = 0;
const MA_NEW_GAME: i32 = 1;
const MA_REPLAYS: i32 = 7;
const MA_TC: i32 = 9;
const SI_LIVES: i32 = 1;
const SI_FLAGS_TO_WIN: i32 = 5;
const SI_LOADING_TIMES: i32 = 6;
const SI_MAX_BONUSES: i32 = 7;
const SI_LEVEL: i32 = 11;
const SI_WEAPON_OPTIONS: i32 = 13;
const SI_LOAD_OPTIONS: i32 = 14;
const SI_SAVE_OPTIONS: i32 = 15;
const PL_NAME: i32 = 0;
const PL_CONTROLLER: i32 = 18;
const PL_LOAD_PROFILE: i32 = 21;
const HM_DOUBLE_RES: i32 = 2;
const HM_FULLSCREEN: i32 = 3;
const HM_AI_FRAMES: i32 = 4;
const HM_AI_MUTATIONS: i32 = 5;
const HM_PALETTE_SELECT: i32 = 6;
const HM_AI_PARALLELS: i32 = 11;

const SC_A: u32 = 4;
const SC_TAB: u32 = 43;
const SDLK_TAB: i32 = 9;

/// Runs `f` on the app's menus.
fn with_menus<R>(a: &mut App, f: impl FnOnce(&mut Menus, &mut App) -> R) -> R {
    let menus = a.shared_menus();
    let mut m = menus.borrow_mut();
    f(&mut m, a)
}

fn hash_str(f: &mut Fnv, s: &[u8]) {
    f.u32(s.len() as u32);
    f.bytes(s);
}

fn hash_menu(f: &mut Fnv, b: &mut Menu) {
    let m = &b.m;
    for v in [m.selection, m.top_item, m.bottom_item, m.visible_item_count, m.height] {
        f.i32(v);
    }
    hash_str(f, &b.search);
    for i in 0..b.count() {
        let it = b.item_at(i);
        hash_str(f, &it.name);
        hash_str(f, &it.value);
        f.bool(it.has_value);
        f.bool(it.visible);
        f.bool(it.selectable);
    }
}

fn print(out: &mut Out, name: &str, step: i32, f: Fnv) {
    out.line(&format!("{name} {step} {:016x} frame", f.0));
}

/// The index of the item with `id`, or -1.
fn index_of(b: &Menu, id: i32) -> i32 {
    b.index_of(id).map_or(-1, |i| i as i32)
}

fn selection(b: &mut Menu) -> i32 {
    b.m.selection
}

fn selection_valid(b: &mut Menu) -> bool {
    let s = selection(b);
    s >= 0 && s < b.count()
}

/// Weapon availability, as the weapon options show it: each item is the
/// weapon's entry in the settings' weapon table, shown by name.
struct Availability {
    names: [&'static [u8]; 3],
    order: Vec<i32>,
}

impl Availability {
    fn behavior(&self) -> ItemBehavior<'_> {
        ItemBehavior::enumeration(0, self.names.len() as i32 - 1, &self.names)
    }

    /// The table entry `item` edits.
    fn entry(&self, b: &Menu, item: i32) -> usize {
        self.order[b.id(item) as usize] as usize
    }

    fn value(&self, a: &App, b: &Menu, item: i32) -> i32 {
        a.settings_mut().data.weap_table[self.entry(b, item)] as i32
    }

    fn set_value(&self, a: &App, b: &Menu, item: i32, v: i32) {
        a.settings_mut().data.weap_table[self.entry(b, item)] = v as u32;
    }

    fn update(&self, a: &App, b: &mut Menu, item: i32) {
        let text = self.behavior().value_text(self.value(a, b, item));
        b.set_value_text(item, &text);
    }

    fn update_items(&self, a: &App, b: &mut Menu) {
        for i in 0..b.count() {
            self.update(a, b, i);
        }
    }

    fn apply(&self, a: &App, b: &mut Menu, item: i32, act: ItemAction) {
        if act.update_item {
            self.update(a, b, item);
        }
        if act.update_menu {
            self.update_items(a, b);
        }
    }
}

/// What `runMenu` does to a menu: left or right, Enter, showing every
/// item's value, and drawing.
enum Ops {
    /// One of the menus in `menus.rs`.
    Menus(MenuKind),
    Availability(Availability),
}

impl Ops {
    fn left_right(&self, a: &mut App, b: &mut Menu, dir: i32) {
        match self {
            Ops::Menus(kind) => {
                with_menus(a, |m, a| m.left_right(a, &Host::none(), *kind, dir));
            }
            Ops::Availability(av) => {
                if selection_valid(b) {
                    let sel = selection(b);
                    let (v, act) = av.behavior().left_right(av.value(a, b, sel), dir, a.menu_cycles);
                    av.set_value(a, b, sel, v);
                    av.apply(a, b, sel, act);
                }
            }
        }
    }

    fn enter(&self, a: &mut App, b: &mut Menu) {
        match self {
            Ops::Menus(kind) => {
                with_menus(a, |m, a| m.enter_dropping(a, &Host::none(), *kind));
            }
            Ops::Availability(av) => {
                if selection_valid(b) {
                    let sel = selection(b);
                    let (v, act) = av.behavior().enter(av.value(a, b, sel));
                    av.set_value(a, b, sel, v);
                    av.apply(a, b, sel, act);
                }
            }
        }
    }

    fn update_items(&self, a: &mut App, b: &mut Menu) {
        match self {
            Ops::Menus(kind) => with_menus(a, |m, a| m.update_items(a, *kind)),
            Ops::Availability(av) => av.update_items(a, b),
        }
    }

    /// Draws the menu on the app's play screen.
    fn draw(&self, a: &mut App, b: &mut Menu, disabled: bool, x: i32, show: bool) {
        match self {
            Ops::Menus(kind) => with_menus(a, |m, a| {
                let (font, scr) = (a.font(), a.play.bmp.view());
                m.draw(a, *kind, &scr, &font, disabled, x, show)
            }),
            Ops::Availability(_) => {
                unsafe fn no_overlay(_: *mut std::ffi::c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}
                let hooks = MenuHooks {
                    ctx: std::ptr::null_mut(),
                    draw_overlay: no_overlay,
                };
                b.draw_with(&a.play.bmp.view(), &a.font(), &hooks, disabled, x, show);
            }
        }
    }
}

/// Presses random keys in the menu for `steps` steps. Items in `no_enter`
/// and `no_left_right` would open a dialog or change the window.
#[allow(clippy::too_many_arguments)]
fn run_menu(
    a: &mut App,
    out: &mut Out,
    name: &str,
    b: &mut Menu,
    ops: &Ops,
    no_enter: &[i32],
    no_left_right: &[i32],
    contains: bool,
    seed: u32,
    steps: i32,
) {
    let mut r = Rand::new(seed);
    let mut time: u32 = 0;

    ops.update_items(a, b);
    b.move_to(0);

    for step in 0..steps {
        let id = if selection_valid(b) {
            let sel = selection(b);
            b.id(sel)
        } else {
            -1
        };
        match r.below(9) {
            0 => b.movement(-1),
            1 => b.movement(1),
            2 => b.movement_page(if r.below(2) != 0 { 1 } else { -1 }),
            3 => b.scroll(r.below(5) as i32 - 2),
            4 => {
                if !no_left_right.contains(&id) {
                    let dir = if r.below(2) != 0 { 1 } else { -1 };
                    ops.left_right(a, b, dir);
                }
            }
            5 => {
                if !no_enter.contains(&id) {
                    ops.enter(a, b);
                }
            }
            6 => {
                let (sym, scancode) = if r.below(4) == 0 {
                    (SDLK_TAB, SC_TAB)
                } else {
                    let letters = b"AEIOSTNRLMBCDGHPWaeios 1";
                    (i32::from(letters[r.below(24) as usize]), SC_A)
                };
                time = time.wrapping_add(if r.below(4) == 0 { 2000 } else { r.below(700) });
                b.on_key(sym, scancode, contains, time);
            }
            7 => {
                let i = r.below(b.count() as u32) as i32;
                let id = b.id(i);
                if id >= 0 && index_of(b, id) == i {
                    let visible = b.item_at(i).visible;
                    b.set_visibility(i, !visible);
                }
            }
            8 => ops.update_items(a, b),
            _ => unreachable!(),
        }

        a.menu_cycles = a.menu_cycles.wrapping_add(1);

        let mut f = Fnv::default();
        hash_menu(&mut f, b);
        a.play.clear();
        let disabled = r.below(5) == 0;
        let x = if r.below(4) == 0 { r.below(300) as i32 } else { -1 };
        let show = r.below(2) == 1;
        ops.draw(a, b, disabled, x, show);
        add_screen(&mut f, &a.play);
        print(out, name, step, f);
    }
}

fn run_menus(a: &mut App, out: &mut Out) {
    a.settings = Some(SettingsBox::defaults());
    // The menus show the name of the loaded setup.
    a.files.settings_node = Node::new(b"config/LIERO.CFG");
    for (i, ws) in a.settings_mut().data.worms.iter().enumerate() {
        let mut ws = ws.borrow_mut();
        ws.data.name = if i == 0 { b"LEFTY".to_vec() } else { b"RIGHTY".to_vec() };
        ws.data.color = if i == 0 { 32 } else { 41 };
        for (j, control) in ws.data.controls.iter_mut().enumerate().take(7) {
            *control = (2 + j * 5 + i) as u32;
        }
    }

    with_menus(a, |m, _| m.load());
    // SAFETY: the menus' boxes live as long as they do.
    let menu = |a: &mut App, kind| unsafe { &mut *with_menus(a, |m, _| m.box_of(kind) as *mut Menu) };

    // As the menu loop sets them up before a game.
    let main = menu(a, MenuKind::Main);
    main.set_visibility(index_of(main, MA_RESUME_GAME), false);
    // SAFETY: the text is copied.
    main.set_name(index_of(main, MA_NEW_GAME), b"NEW GAME (F1)");

    run_menu(
        a,
        out,
        "menu-main",
        main,
        &Ops::Menus(MenuKind::Main),
        &[MA_REPLAYS, MA_TC],
        &[],
        false,
        1,
        200,
    );

    let no_enter = [
        SI_LEVEL,
        SI_WEAPON_OPTIONS,
        SI_LOAD_OPTIONS,
        SI_SAVE_OPTIONS,
        SI_LOADING_TIMES,
        SI_MAX_BONUSES,
        SI_LIVES,
        SI_FLAGS_TO_WIN,
    ];
    let setup = menu(a, MenuKind::Settings);
    run_menu(
        a,
        out,
        "menu-setup",
        setup,
        &Ops::Menus(MenuKind::Settings),
        &no_enter,
        &[],
        false,
        2,
        400,
    );

    with_menus(a, |m, a| m.player_settings(a, 0));
    let player_no_enter: Vec<i32> = (PL_NAME..=PL_LOAD_PROFILE).filter(|&id| id != PL_CONTROLLER).collect();
    let player = menu(a, MenuKind::Player);
    run_menu(
        a,
        out,
        "menu-player",
        player,
        &Ops::Menus(MenuKind::Player),
        &player_no_enter,
        &[],
        false,
        3,
        400,
    );

    let options = menu(a, MenuKind::Hidden);
    run_menu(
        a,
        out,
        "menu-options",
        options,
        &Ops::Menus(MenuKind::Hidden),
        &[
            HM_DOUBLE_RES,
            HM_FULLSCREEN,
            HM_AI_FRAMES,
            HM_AI_MUTATIONS,
            HM_PALETTE_SELECT,
            HM_AI_PARALLELS,
        ],
        &[HM_DOUBLE_RES, HM_FULLSCREEN],
        false,
        4,
        300,
    );

    // Long enough to scroll, and searched by any part of the name like the
    // file selectors.
    let tc = a.menu_tc();
    let tc = tc_data(&tc);
    let mut menu = Menu::raw(179, 28, false);
    let weapons = &mut menu;
    weapons.m.height = 14;
    weapons.set_top(weapons.m.top_item);
    weapons.m.value_offset_x = 89;
    let order = tc.weapon_order().to_vec();
    for (i, &w) in order.iter().enumerate() {
        let mut item = MenuItem::new(tc.weapon_name(w as usize), 48, 7);
        item.id = i as i32;
        weapons.add_at(item, None, true);
    }
    drop(tc);
    let states: [&'static [u8]; 3] = [b"Menu", b"Bonus", b"Banned"];
    let av = Availability { names: states, order };
    run_menu(
        a,
        out,
        "menu-weapons",
        weapons,
        &Ops::Availability(av),
        &[],
        &[],
        true,
        5,
        400,
    );
}

/// The level name, "A vs B" and the worms' colours, as the spectator
/// window shows them in the menus and weapon selection. The name is taken
/// from `level_file_for_name`.
#[allow(clippy::too_many_arguments)]
fn draw_versus_info(
    a: &App,
    bmp: &BitmapBox,
    level_file: &[u8],
    level_file_for_name: &[u8],
    names: [&[u8]; 2],
    colors: [i32; 2],
    center_x: i32,
    center_y: i32,
) {
    let level = level_line(&a.tc_data(), level_file, level_file_for_name);
    draw_versus(
        &bmp.view(),
        &a.font(),
        &level,
        names[0],
        names[1],
        colors[0],
        colors[1],
        center_x,
        center_y,
    );
}

/// The boxes drawn by the menus' dialogs.
fn run_boxes(a: &mut App, out: &mut Out) {
    let mut r = Rand::new(6);
    let mut frozen = BitmapBox::new();
    frozen.alloc(320, 200, 320);
    let fv = frozen.view();
    for y in 0..200 {
        for x in 0..320 {
            fv.pixels[(y * 320 + x) as usize].set((x ^ y) as u8);
        }
    }

    let texts: [&[u8]; 6] = [
        b"",
        b"A",
        b"NO WEAPONS",
        b"SAVE SETUP AS...",
        b"LEVELS\\SOME.LEV",
        b"1234567890",
    ];
    let defaults = SettingsBox::defaults();
    let default_colors = [
        defaults.borrow().data.worms[0].borrow().data.color,
        defaults.borrow().data.worms[1].borrow().data.color,
    ];

    for step in 0..24 {
        let mut f = Fnv::default();
        let names = [texts[r.below(6) as usize], texts[r.below(6) as usize]];
        let colors = [r.below(256) as i32, r.below(256) as i32];
        let level_file: &[u8] = if r.below(2) != 0 { b"levels/SOME.LEV" } else { b"" };

        a.single.clear();
        let for_name = [b"x/" as &[u8], texts[r.below(6) as usize], b".lev"].concat();
        draw_versus_info(a, &a.single.bmp, level_file, &for_name, names, colors, 320, 100);
        add_screen(&mut f, &a.single);

        a.play.clear();
        let font = a.font();
        let scr = a.play.bmp.view();
        let text = texts[r.below(6) as usize];
        let x = 20 + r.below(280) as i32;
        let y = 10 + r.below(180) as i32;
        draw_info_box(&scr, &font, text, x, y);
        let text = [texts[r.below(6) as usize], b"_"].concat();
        let x = 20 + r.below(200) as i32;
        let y = 10 + r.below(180) as i32;
        let centered = r.below(2) == 1;
        draw_input_box(&scr, &frozen.view(), &font, &text, x, y, centered);
        add_screen(&mut f, &a.play);
        print(out, "menu-boxes", step, f);
    }

    // A one-character level file still names the level.
    let mut f = Fnv::default();
    a.single.clear();
    let names = [
        defaults.borrow().data.worms[0].borrow().data.name.clone(),
        defaults.borrow().data.worms[1].borrow().data.name.clone(),
    ];
    let names: [&[u8]; 2] = [&names[0], &names[1]];
    draw_versus_info(a, &a.single.bmp, b"A", b"x/A.lev", names, default_colors, 320, 100);
    add_screen(&mut f, &a.single);
    print(out, "menu-boxes-short", 0, f);
}

/// The weapon selection screen with both worms' controls pressed at random.
fn run_weapon_selection(
    a: &mut App,
    out: &mut Out,
    name: &str,
    seed: u32,
    controller1: u32,
    select_bot_weapons: u32,
    steps: i32,
) {
    let settings = SettingsBox::defaults();
    a.settings = Some(settings.clone());
    let mut s = settings.borrow_mut();
    s.data.ext.select_bot_weapons = select_bot_weapons;
    let mut r = Rand::new(seed);
    for w in &mut s.data.weap_table {
        *w = if r.below(6) == 0 { 1 + r.below(2) } else { 0 };
    }
    for (i, ws) in s.data.worms.iter().enumerate() {
        let mut ws = ws.borrow_mut();
        ws.data.name = if i == 0 { b"LEFTY".to_vec() } else { b"RIGHTY".to_vec() };
        ws.data.color = if i == 0 { 32 } else { 41 };
        ws.data.controller = if i == 1 { controller1 } else { 0 };
        for weapon in ws.data.weapons.iter_mut().take(5) {
            *weapon = 1 + r.below(40);
        }
    }
    if seed.is_multiple_of(2) {
        s.data.level_file = b"levels/SOME.LEV".to_vec();
    }
    let worms = s.data.worms.clone();
    drop(s);

    a.rand.seed(seed);

    let tc = a.menu_tc();
    let d = tc_data(&tc);
    // SAFETY: the game keeps references to the settings and the TC; its
    // worms and viewports are made as the C++ `Game` view made them.
    let mut game = Game::new();
    unsafe {
        game.set_context(Some(settings.clone()), Some(tc.clone()), SoundOut::NONE);
        for (i, ws) in worms.iter().enumerate() {
            let id = game.new_worm(Some(ws.clone()));
            let ws = ws.borrow();
            game.list_worm(Some(id));
            let w = game.worm_mut(id);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
        }
        let rect = |x1, y1, x2, y2| Rect { x1, y1, x2, y2 };
        game.add_viewport(rect(0, 0, 158, 158), 0, 504, 350, ViewportKind::Worm, false);
        game.add_viewport(rect(160, 0, 158 + 160, 158), 1, 504, 350, ViewportKind::Worm, false);
        game.add_viewport(rect(0, 0, 504 + 68, 350), 0, 504, 350, ViewportKind::Spectator, true);
        let mut level = LevelBox::new();
        level.generate_from_settings(&d.level_common(), &d.palette_colors, &settings.borrow(), &mut a.rand);
        std::mem::swap(game.level_box_mut(), &mut level);
        game.focus(&mut *a.play);
        game.focus(&mut *a.single);
    }

    // SAFETY: the selection works on the game, which outlives it.
    let selection = Box::into_raw(unsafe { Weapsel::start(&mut game, &mut a.rand) });
    // SAFETY: it is new, and freed below.
    let ws = unsafe { &mut *selection };

    for step in 0..steps {
        for w in game.listed_worms() {
            let w = w.expect("an empty place among the game's worms");
            let c = &mut w.data.control_states;
            for (bit, n) in [
                (Control::Left, 6),
                (Control::Right, 6),
                (Control::Up, 5),
                (Control::Down, 4),
                (Control::Fire, 40),
            ] {
                if r.below(n) == 0 {
                    *c |= bit.bit();
                } else {
                    *c &= !bit.bit();
                }
            }
        }

        if step % 40 == 20 {
            ws.set_focused(false);
        } else if step % 40 == 22 {
            ws.set_focused(true);
        }

        // SAFETY: as above.
        let done = unsafe { ws.process_game(&mut a.rand) };
        a.menu_cycles = a.menu_cycles.wrapping_add(1);

        let mut f = Fnv::default();
        for i in 0..ws.menu_count() {
            f.bool(ws.ready(i));
            // SAFETY: the selection's menus live as long as it does.
            hash_menu(&mut f, ws.menu(i));
        }
        for w in game.listed_worms() {
            let w = w.expect("an empty place among the game's worms");
            for j in 0..5 {
                f.u32(w.settings().data.weapons[j]);
                f.i32(w.data.weapons[j].kind);
            }
        }

        // SAFETY: the renderers are the app's.
        unsafe {
            a.play.clear();
            let play: *mut _ = &mut *a.play;
            ws.draw_game(a, play, State::WeaponSelection, false);
            add_screen(&mut f, &a.play);
            a.single.clear();
            let single: *mut _ = &mut *a.single;
            ws.draw_game(a, single, State::WeaponSelection, true);
            add_screen(&mut f, &a.single);
        }
        print(out, name, step, f);

        if done {
            break;
        }
    }
    // SAFETY: the selection isn't used again, and goes before its game.
    drop(unsafe { Box::from_raw(selection) });
    drop(game);
}

pub fn hash_menus(a: &mut App, out: &mut Out) {
    let tc = super::tc::bundled();
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    a.play.load_palette(&tc_data(&tc));
    a.single.load_palette(&tc_data(&tc));
    a.set_tc(Some(tc));
    a.menu_cycles = 0;

    run_menus(a, out);
    run_boxes(a, out);

    run_weapon_selection(a, out, "weapsel-1", 1, 0, 0, 300);
    run_weapon_selection(a, out, "weapsel-2", 2, 1, 1, 300);
    run_weapon_selection(a, out, "weapsel-3", 3, 1, 0, 300);
    run_weapon_selection(a, out, "weapsel-4", 4, 0, 2, 300);
}
