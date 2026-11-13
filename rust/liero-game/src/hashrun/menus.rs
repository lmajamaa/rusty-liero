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
use crate::app::{app, LieroApp};
use crate::controller::LIERO_STATE_WEAPON_SELECTION;
use crate::controllers::liero_game_focus;
use crate::fs::Node;
use crate::game::LieroRect;
use crate::game_box::{
    liero_game_add_viewport, liero_game_list_worm, liero_game_new, liero_game_new_worm, liero_game_set_context,
};
use crate::item::{
    liero_item_enter, liero_item_left_right, liero_item_value_text, LieroItemAction, LieroItemBehavior, LieroItemKind,
};
use crate::level::LieroLevelBox;
use crate::menu::{
    liero_menu_box_add, liero_menu_box_count, liero_menu_box_draw, liero_menu_box_free, liero_menu_box_id,
    liero_menu_box_item, liero_menu_box_move_to, liero_menu_box_movement, liero_menu_box_movement_page,
    liero_menu_box_new, liero_menu_box_on_key, liero_menu_box_scroll, liero_menu_box_search_text,
    liero_menu_box_set_string, liero_menu_box_set_top, liero_menu_box_set_value, liero_menu_box_set_visibility,
    liero_menu_box_state, liero_menu_draw_info_box, liero_menu_draw_input_box, liero_menu_draw_versus, LieroMenuBox,
    LieroMenuHooks, LieroMenuItemSpec, LIERO_MENU_VALUE_INTEGER,
};
use crate::menu_screen::{LIERO_MENU_HIDDEN, LIERO_MENU_MAIN, LIERO_MENU_PLAYER, LIERO_MENU_SETTINGS};
use crate::menus::{
    liero_menus_box, liero_menus_draw, liero_menus_enter, liero_menus_left_right, liero_menus_load,
    liero_menus_player_settings, liero_menus_update_items, LieroMenus,
};
use crate::rand::LieroRand;
use crate::renderer::{LieroBitmapBox, LieroRenderer};
use crate::settings::LieroSettingsBox;
use crate::sfx::LieroSoundOut;
use crate::text::LieroStr;
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use crate::weapsel::{
    level_line, liero_weapsel_draw_game, liero_weapsel_free, liero_weapsel_menu, liero_weapsel_menu_count,
    liero_weapsel_process_game, liero_weapsel_ready, liero_weapsel_set_focused, liero_weapsel_start,
};
use crate::worm::{LIERO_CONTROL_DOWN, LIERO_CONTROL_FIRE, LIERO_CONTROL_LEFT, LIERO_CONTROL_RIGHT, LIERO_CONTROL_UP};

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

fn menus(a: &LieroApp) -> &'static mut LieroMenus {
    // SAFETY: C++ keeps the menus while it runs.
    unsafe { a.menus.as_mut().expect("no menus") }
}

fn bytes(s: LieroStr) -> Vec<u8> {
    // SAFETY: the menus' texts live until they change.
    unsafe { s.bytes() }.to_vec()
}

fn hash_str(f: &mut Fnv, s: &[u8]) {
    f.u32(s.len() as u32);
    f.bytes(s);
}

fn hash_menu(f: &mut Fnv, b: &mut LieroMenuBox) {
    let m = liero_menu_box_state(b);
    // SAFETY: the state is the box's own.
    let m = unsafe { &*m };
    for v in [m.selection, m.top_item, m.bottom_item, m.visible_item_count, m.height] {
        f.i32(v);
    }
    hash_str(f, liero_menu_box_search_text(b));
    for i in 0..liero_menu_box_count(b) {
        let it = liero_menu_box_item(b, i);
        hash_str(f, &bytes(it.string));
        hash_str(f, &bytes(it.value));
        f.bool(it.has_value);
        f.bool(it.visible);
        f.bool(it.selectable);
    }
}

fn print(out: &mut Out, name: &str, step: i32, f: Fnv) {
    out.line(&format!("{name} {step} {:016x} frame", f.0));
}

/// The index of the item with `id`, or -1.
fn index_of(b: &LieroMenuBox, id: i32) -> i32 {
    (0..liero_menu_box_count(b))
        .find(|&i| liero_menu_box_id(b, i) == id)
        .unwrap_or(-1)
}

fn selection(b: &mut LieroMenuBox) -> i32 {
    // SAFETY: the state is the box's own.
    unsafe { (*liero_menu_box_state(b)).selection }
}

fn selection_valid(b: &mut LieroMenuBox) -> bool {
    let s = selection(b);
    s >= 0 && s < liero_menu_box_count(b)
}

/// Weapon availability, as the weapon options show it: each item is the
/// weapon's entry in the settings' weapon table, shown by name.
struct Availability {
    names: Vec<LieroStr>,
    order: Vec<i32>,
}

impl Availability {
    fn behavior(&self, b: &LieroMenuBox, item: i32) -> LieroItemBehavior {
        let mut s = crate::app::menu_settings();
        LieroItemBehavior {
            kind: LieroItemKind::Enum,
            flag: std::ptr::null_mut(),
            set_flag: None,
            value: (&mut s.data.weap_table[self.order[liero_menu_box_id(b, item) as usize] as usize] as *mut u32)
                .cast(),
            min: 0,
            max: self.names.len() as i32 - 1,
            step: 1,
            scroll_interval: 5,
            value_kind: LIERO_MENU_VALUE_INTEGER,
            allow_entry: true,
            names: self.names.as_ptr(),
            name_count: self.names.len(),
        }
    }

    fn update(&self, b: &mut LieroMenuBox, item: i32) {
        let be = self.behavior(b, item);
        let mut buf = [0u8; 64];
        // SAFETY: the buffer is as long as said.
        let len = unsafe { liero_item_value_text(&be, buf.as_mut_ptr(), buf.len()) };
        // SAFETY: the value is copied.
        liero_menu_box_set_value(b, item, &buf[..len.min(buf.len())]);
    }

    fn update_items(&self, b: &mut LieroMenuBox) {
        for i in 0..liero_menu_box_count(b) {
            self.update(b, i);
        }
    }

    fn apply(&self, b: &mut LieroMenuBox, item: i32, a: LieroItemAction) {
        if a.update_item {
            self.update(b, item);
        }
        if a.update_menu {
            self.update_items(b);
        }
    }
}

/// What `runMenu` does to a menu: left or right, Enter, showing every
/// item's value, and drawing.
enum Ops {
    /// One of the menus in `menus.rs`.
    Menus(i32),
    Availability(Availability),
}

impl Ops {
    fn left_right(&self, b: &mut LieroMenuBox, dir: i32) {
        match self {
            Ops::Menus(kind) => {
                liero_menus_left_right(menus(app()), *kind, dir);
            }
            Ops::Availability(av) => {
                if selection_valid(b) {
                    let sel = selection(b);
                    let be = av.behavior(b, sel);
                    // SAFETY: the behaviour's setting is the menus' settings'.
                    let a = unsafe { liero_item_left_right(&be, dir, app().menu_cycles) };
                    av.apply(b, sel, a);
                }
            }
        }
    }

    fn enter(&self, b: &mut LieroMenuBox) {
        match self {
            // SAFETY: the menus are C++'s, with the app's settings.
            Ops::Menus(kind) => unsafe {
                liero_menus_enter(menus(app()), *kind);
            },
            Ops::Availability(av) => {
                if selection_valid(b) {
                    let sel = selection(b);
                    let be = av.behavior(b, sel);
                    // SAFETY: as above.
                    let a = unsafe { liero_item_enter(&be) };
                    av.apply(b, sel, a);
                }
            }
        }
    }

    fn update_items(&self, b: &mut LieroMenuBox) {
        match self {
            Ops::Menus(kind) => liero_menus_update_items(menus(app()), *kind),
            Ops::Availability(av) => av.update_items(b),
        }
    }

    fn draw(&self, b: &mut LieroMenuBox, r: &LieroRenderer, disabled: bool, x: i32, show: bool) {
        let a = app();
        let font = crate::app::menu_font();
        let scr = r.bmp.view();
        match self {
            Ops::Menus(kind) => liero_menus_draw(menus(a), *kind, &scr, &font, disabled, x, show),
            Ops::Availability(_) => {
                unsafe fn no_overlay(_: *mut std::ffi::c_void, _: i32, _: i32, _: i32, _: bool, _: bool) {}
                let hooks = LieroMenuHooks {
                    ctx: std::ptr::null_mut(),
                    draw_overlay: no_overlay,
                };
                liero_menu_box_draw(b, &scr, &font, &hooks, disabled, x, show);
            }
        }
    }
}

/// Presses random keys in the menu for `steps` steps. Items in `no_enter`
/// and `no_left_right` would open a dialog or change the window.
#[allow(clippy::too_many_arguments)]
fn run_menu(
    out: &mut Out,
    name: &str,
    b: &mut LieroMenuBox,
    ops: &Ops,
    no_enter: &[i32],
    no_left_right: &[i32],
    contains: bool,
    seed: u32,
    steps: i32,
) {
    let a = app();
    let mut r = LieroRand::new(seed);
    let mut time: u32 = 0;

    ops.update_items(b);
    liero_menu_box_move_to(b, 0);

    for step in 0..steps {
        let id = if selection_valid(b) {
            let sel = selection(b);
            liero_menu_box_id(b, sel)
        } else {
            -1
        };
        match r.below(9) {
            0 => liero_menu_box_movement(b, -1),
            1 => liero_menu_box_movement(b, 1),
            2 => liero_menu_box_movement_page(b, if r.below(2) != 0 { 1 } else { -1 }),
            3 => liero_menu_box_scroll(b, r.below(5) as i32 - 2),
            4 => {
                if !no_left_right.contains(&id) {
                    let dir = if r.below(2) != 0 { 1 } else { -1 };
                    ops.left_right(b, dir);
                }
            }
            5 => {
                if !no_enter.contains(&id) {
                    ops.enter(b);
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
                liero_menu_box_on_key(b, sym, scancode, contains, time);
            }
            7 => {
                let i = r.below(liero_menu_box_count(b) as u32) as i32;
                let id = liero_menu_box_id(b, i);
                if id >= 0 && index_of(b, id) == i {
                    let visible = liero_menu_box_item(b, i).visible;
                    liero_menu_box_set_visibility(b, i, !visible);
                }
            }
            8 => ops.update_items(b),
            _ => unreachable!(),
        }

        a.menu_cycles = a.menu_cycles.wrapping_add(1);

        let mut f = Fnv::default();
        hash_menu(&mut f, b);
        a.play.clear();
        let disabled = r.below(5) == 0;
        let x = if r.below(4) == 0 { r.below(300) as i32 } else { -1 };
        let show = r.below(2) == 1;
        ops.draw(b, &a.play, disabled, x, show);
        add_screen(&mut f, &a.play);
        print(out, name, step, f);
    }
}

fn run_menus(out: &mut Out) {
    let a = app();
    a.settings = Some(LieroSettingsBox::defaults());
    // The menus show the name of the loaded setup.
    a.files.settings_node = Node::new(b"config/LIERO.CFG");
    for (i, ws) in crate::app::menu_settings().worms.iter().enumerate() {
        let mut ws = ws.borrow_mut();
        ws.name = if i == 0 { b"LEFTY".to_vec() } else { b"RIGHTY".to_vec() };
        ws.data.color = if i == 0 { 32 } else { 41 };
        for (j, control) in ws.data.controls.iter_mut().enumerate().take(7) {
            *control = (2 + j * 5 + i) as u32;
        }
    }

    let m = menus(a);
    liero_menus_load(m);
    // SAFETY: the menus' boxes live as long as they do.
    let menu = |kind| unsafe { &mut *liero_menus_box(menus(app()), kind) };

    // As the menu loop sets them up before a game.
    let main = menu(LIERO_MENU_MAIN);
    liero_menu_box_set_visibility(main, index_of(main, MA_RESUME_GAME), false);
    // SAFETY: the text is copied.
    liero_menu_box_set_string(main, index_of(main, MA_NEW_GAME), b"NEW GAME (F1)");

    run_menu(
        out,
        "menu-main",
        main,
        &Ops::Menus(LIERO_MENU_MAIN),
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
    run_menu(
        out,
        "menu-setup",
        menu(LIERO_MENU_SETTINGS),
        &Ops::Menus(LIERO_MENU_SETTINGS),
        &no_enter,
        &[],
        false,
        2,
        400,
    );

    liero_menus_player_settings(menus(a), 0);
    let player_no_enter: Vec<i32> = (PL_NAME..=PL_LOAD_PROFILE).filter(|&id| id != PL_CONTROLLER).collect();
    run_menu(
        out,
        "menu-player",
        menu(LIERO_MENU_PLAYER),
        &Ops::Menus(LIERO_MENU_PLAYER),
        &player_no_enter,
        &[],
        false,
        3,
        400,
    );

    run_menu(
        out,
        "menu-options",
        menu(LIERO_MENU_HIDDEN),
        &Ops::Menus(LIERO_MENU_HIDDEN),
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
    let tc = crate::app::menu_tc_data();
    let b = liero_menu_box_new(179, 28, false);
    // SAFETY: the box is new, and freed below.
    let weapons = unsafe { &mut *b };
    {
        // SAFETY: the state is the box's own.
        let s = unsafe { &mut *liero_menu_box_state(weapons) };
        s.height = 14;
        let top = s.top_item;
        liero_menu_box_set_top(weapons, top);
        // SAFETY: as above.
        unsafe { (*liero_menu_box_state(weapons)).value_offset_x = 89 };
    }
    let order = tc.weapon_order().to_vec();
    for (i, &w) in order.iter().enumerate() {
        let spec = LieroMenuItemSpec {
            string: LieroStr::of(tc.weapon_name(w as usize)),
            value: LieroStr::of(b""),
            color: 48,
            dis_colour: 7,
            has_value: false,
            visible: true,
            selectable: true,
            id: i as i32,
        };
        // SAFETY: the texts are copied.
        unsafe { liero_menu_box_add(weapons, &spec, -1, true) };
    }
    let states: [&[u8]; 3] = [b"Menu", b"Bonus", b"Banned"];
    let av = Availability {
        names: states.iter().map(|s| LieroStr::of(s)).collect(),
        order,
    };
    run_menu(
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
    // SAFETY: the box isn't used again.
    unsafe { liero_menu_box_free(b) };
}

/// The level name, "A vs B" and the worms' colours, as the spectator
/// window shows them in the menus and weapon selection. The name is taken
/// from `level_file_for_name`.
#[allow(clippy::too_many_arguments)]
fn draw_versus_info(
    bmp: &LieroBitmapBox,
    level_file: &[u8],
    level_file_for_name: &[u8],
    names: [&[u8]; 2],
    colors: [i32; 2],
    center_x: i32,
    center_y: i32,
) {
    let level = level_line(&crate::app::menu_tc_data(), level_file, level_file_for_name);
    // SAFETY: the bitmap and font are live.
    unsafe {
        liero_menu_draw_versus(
            &bmp.view(),
            &crate::app::menu_font(),
            LieroStr::of(&level),
            LieroStr::of(names[0]),
            LieroStr::of(names[1]),
            colors[0],
            colors[1],
            center_x,
            center_y,
        )
    };
}

/// The boxes drawn by the menus' dialogs.
fn run_boxes(out: &mut Out) {
    let a = app();
    let mut r = LieroRand::new(6);
    let mut frozen = LieroBitmapBox::new();
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
    let defaults = LieroSettingsBox::defaults();
    let default_colors = [
        defaults.borrow().worms[0].borrow().data.color,
        defaults.borrow().worms[1].borrow().data.color,
    ];

    for step in 0..24 {
        let mut f = Fnv::default();
        let names = [texts[r.below(6) as usize], texts[r.below(6) as usize]];
        let colors = [r.below(256) as i32, r.below(256) as i32];
        let level_file: &[u8] = if r.below(2) != 0 { b"levels/SOME.LEV" } else { b"" };

        a.single.clear();
        let for_name = [b"x/" as &[u8], texts[r.below(6) as usize], b".lev"].concat();
        draw_versus_info(&a.single.bmp, level_file, &for_name, names, colors, 320, 100);
        add_screen(&mut f, &a.single);

        a.play.clear();
        let font = crate::app::menu_font();
        let scr = a.play.bmp.view();
        let text = texts[r.below(6) as usize];
        let x = 20 + r.below(280) as i32;
        let y = 10 + r.below(180) as i32;
        liero_menu_draw_info_box(&scr, &font, text, x, y);
        let text = [texts[r.below(6) as usize], b"_"].concat();
        let x = 20 + r.below(200) as i32;
        let y = 10 + r.below(180) as i32;
        let centered = r.below(2) == 1;
        liero_menu_draw_input_box(&scr, &frozen.view(), &font, &text, x, y, centered);
        add_screen(&mut f, &a.play);
        print(out, "menu-boxes", step, f);
    }

    // A one-character level file still names the level.
    let mut f = Fnv::default();
    a.single.clear();
    let names = [
        defaults.borrow().worms[0].borrow().name.clone(),
        defaults.borrow().worms[1].borrow().name.clone(),
    ];
    let names: [&[u8]; 2] = [&names[0], &names[1]];
    draw_versus_info(&a.single.bmp, b"A", b"x/A.lev", names, default_colors, 320, 100);
    add_screen(&mut f, &a.single);
    print(out, "menu-boxes-short", 0, f);
}

/// The weapon selection screen with both worms' controls pressed at random.
fn run_weapon_selection(out: &mut Out, name: &str, seed: u32, controller1: u32, select_bot_weapons: u32, steps: i32) {
    let a = app();
    let settings = LieroSettingsBox::defaults();
    a.settings = Some(settings.clone());
    let mut s = settings.borrow_mut();
    s.data.select_bot_weapons = select_bot_weapons;
    let mut r = LieroRand::new(seed);
    for w in &mut s.data.weap_table {
        *w = if r.below(6) == 0 { 1 + r.below(2) } else { 0 };
    }
    for (i, ws) in s.worms.iter().enumerate() {
        let mut ws = ws.borrow_mut();
        ws.name = if i == 0 { b"LEFTY".to_vec() } else { b"RIGHTY".to_vec() };
        ws.data.color = if i == 0 { 32 } else { 41 };
        ws.data.controller = if i == 1 { controller1 } else { 0 };
        for weapon in ws.data.weapons.iter_mut().take(5) {
            *weapon = 1 + r.below(40);
        }
    }
    if seed.is_multiple_of(2) {
        s.level_file = b"levels/SOME.LEV".to_vec();
    }
    let worms = s.worms.clone();
    drop(s);

    a.rand.seed(seed);

    let tc = a.menu_tc();
    let d = tc_data(&tc);
    // SAFETY: the game keeps references to the settings and the TC; its
    // worms and viewports are made as the C++ `Game` view made them.
    let mut game = unsafe { Box::from_raw(liero_game_new()) };
    unsafe {
        liero_game_set_context(&mut game, Some(settings.clone()), Some(tc.clone()), LieroSoundOut::NONE);
        for (i, ws) in worms.iter().enumerate() {
            let id = liero_game_new_worm(&mut game, Some(ws.clone()));
            let ws = ws.borrow();
            liero_game_list_worm(&mut game, Some(id));
            let w = game.worm_mut(id);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
        }
        let rect = |x1, y1, x2, y2| LieroRect { x1, y1, x2, y2 };
        liero_game_add_viewport(&mut game, rect(0, 0, 158, 158), 0, 504, 350, LIERO_VIEWPORT_WORM, false);
        liero_game_add_viewport(
            &mut game,
            rect(160, 0, 158 + 160, 158),
            1,
            504,
            350,
            LIERO_VIEWPORT_WORM,
            false,
        );
        liero_game_add_viewport(
            &mut game,
            rect(0, 0, 504 + 68, 350),
            0,
            504,
            350,
            LIERO_VIEWPORT_SPECTATOR,
            true,
        );
        let mut level = LieroLevelBox::new();
        level.generate_from_settings(&d.level_common(), &d.palette_colors, &settings.borrow(), &mut a.rand);
        std::mem::swap(game.level_box_mut(), &mut level);
        liero_game_focus(&mut game, &mut *a.play);
        liero_game_focus(&mut game, &mut *a.single);
    }

    // SAFETY: the selection works on the game, which outlives it.
    let ws = unsafe { &mut *liero_weapsel_start(&mut game) };

    for step in 0..steps {
        for w in game.listed_worms() {
            let w = w.expect("an empty place among the game's worms");
            let c = &mut w.data.control_states;
            for (bit, n) in [
                (LIERO_CONTROL_LEFT, 6),
                (LIERO_CONTROL_RIGHT, 6),
                (LIERO_CONTROL_UP, 5),
                (LIERO_CONTROL_DOWN, 4),
                (LIERO_CONTROL_FIRE, 40),
            ] {
                if r.below(n) == 0 {
                    *c |= 1 << bit;
                } else {
                    *c &= !(1 << bit);
                }
            }
        }

        if step % 40 == 20 {
            liero_weapsel_set_focused(ws, false);
        } else if step % 40 == 22 {
            liero_weapsel_set_focused(ws, true);
        }

        // SAFETY: as above.
        let done = unsafe { liero_weapsel_process_game(ws) };
        a.menu_cycles = a.menu_cycles.wrapping_add(1);

        let mut f = Fnv::default();
        for i in 0..liero_weapsel_menu_count(ws) {
            f.bool(liero_weapsel_ready(ws, i));
            // SAFETY: the selection's menus live as long as it does.
            hash_menu(&mut f, unsafe { &mut *liero_weapsel_menu(ws, i) });
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
            liero_weapsel_draw_game(ws, &mut *a.play, LIERO_STATE_WEAPON_SELECTION, false);
            add_screen(&mut f, &a.play);
            a.single.clear();
            liero_weapsel_draw_game(ws, &mut *a.single, LIERO_STATE_WEAPON_SELECTION, true);
            add_screen(&mut f, &a.single);
        }
        print(out, name, step, f);

        if done {
            break;
        }
    }
    // SAFETY: the selection isn't used again, and goes before its game.
    unsafe { liero_weapsel_free(ws) };
    drop(game);
}

pub fn hash_menus(out: &mut Out) {
    let a = app();
    let tc = super::tc::bundled();
    a.play.set_render_resolution(320, 200);
    a.single.set_render_resolution(640, 400);
    a.play.load_palette(&tc_data(&tc));
    a.single.load_palette(&tc_data(&tc));
    a.set_tc(Some(tc));
    a.menu_cycles = 0;

    run_menus(out);
    run_boxes(out);

    run_weapon_selection(out, "weapsel-1", 1, 0, 0, 300);
    run_weapon_selection(out, "weapsel-2", 2, 1, 1, 300);
    run_weapon_selection(out, "weapsel-3", 3, 1, 0, 300);
    run_weapon_selection(out, "weapsel-4", 4, 0, 2, 300);
}
