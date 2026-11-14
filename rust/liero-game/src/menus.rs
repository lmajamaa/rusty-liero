//! The menus beside the main menu screen (C++'s `MainMenu`, `SettingsMenu`,
//! `PlayerMenu` and `HiddenMenu`): their items, which setting each one
//! changes, how it shows its value, and the screens an item opens (a key
//! choice, a text box, the weapon options or a file selector). An item that
//! opens a screen goes on when that screen closes (`LieroMenus::then`).
//!
//! Each switch, number or enum names the setting it edits (`Setting`), which
//! is read and written in the app's settings when a key changes it. The
//! window's double size and full screen are shown by the platform.

use crate::app::LieroApp;
use crate::boxes::{liero_input_box_screen, liero_key_choice_screen};
use crate::gfx::LieroBitmap;
use crate::item::{LieroItemAction, LieroItemBehavior};
use crate::keys::{is_extended, key_name, LIERO_DK_ESCAPE};
use crate::menu::{
    liero_menu_closest_name, liero_menu_draw_palette_overlay, liero_menu_draw_rgb_overlay, LieroMenuBox,
    LieroMenuHooks, LieroMenuItemSpec, OwnedMenu, LIERO_MENU_VALUE_FRAMES, LIERO_MENU_VALUE_PERCENTAGE,
    LIERO_MENU_VALUE_TIME,
};
use crate::menu_screen::{LIERO_MENU_HIDDEN, LIERO_MENU_MAIN, LIERO_MENU_PLAYER, LIERO_MENU_SETTINGS};
use crate::screen::{BoxedScreen, Host};
use crate::settings::{GameSettings, LieroWormSettingsBox};
use crate::tc::{c, s};
use crate::text::{LieroFont, LieroStr};
use crate::weapon_options::{liero_weapon_options_screen, LieroWeaponOptionsSpec};
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

const SOUND_CHOICE: i32 = 27;

/// The main menu's item ids (`MainMenu`).
pub const LIERO_MAIN_RESUME_GAME: i32 = 0;
pub const LIERO_MAIN_NEW_GAME: i32 = 1;
const MA_SETTINGS: i32 = 2;
const MA_PLAYER1_SETTINGS: i32 = 3;
const MA_PLAYER2_SETTINGS: i32 = 4;
const MA_ADVANCED: i32 = 5;
const MA_QUIT: i32 = 6;
const MA_REPLAYS: i32 = 7;
/// What choosing a replay closes the menu with.
pub const LIERO_MAIN_REPLAY: i32 = 8;
const MA_TC: i32 = 9;

// The match setup's item ids (`SettingsMenu`).
const SI_GAME_MODE: i32 = 0;
const SI_LIVES: i32 = 1;
const SI_TIME_TO_LOSE: i32 = 2;
const SI_TIME_TO_WIN: i32 = 3;
const SI_ZONE_TIMEOUT: i32 = 4;
const SI_FLAGS_TO_WIN: i32 = 5;
const SI_LOADING_TIMES: i32 = 6;
const SI_MAX_BONUSES: i32 = 7;
const SI_NAMES_ON_BONUSES: i32 = 8;
const SI_MAP: i32 = 9;
const SI_AMOUNT_OF_BLOOD: i32 = 10;
const SI_LEVEL: i32 = 11;
const SI_REGENERATE_LEVEL: i32 = 12;
const SI_WEAPON_OPTIONS: i32 = 13;
const SI_LOAD_OPTIONS: i32 = 14;
const SI_SAVE_OPTIONS: i32 = 15;
const SI_LOAD_CHANGE: i32 = 16;

// The player menu's item ids (`PlayerMenu`).
const PL_NAME: i32 = 0;
const PL_HEALTH: i32 = 1;
const PL_RED: i32 = 2;
const PL_BLUE: i32 = 4;
const PL_UP: i32 = 5;
const PL_JUMP: i32 = 11;
const PL_DIG: i32 = 12;
const PL_WEAP0: i32 = 13;
const PL_CONTROLLER: i32 = PL_WEAP0 + 5;
const PL_SAVE_PROFILE: i32 = PL_CONTROLLER + 1;
const PL_SAVE_PROFILE_AS: i32 = PL_CONTROLLER + 2;
const PL_LOAD_PROFILE: i32 = PL_CONTROLLER + 3;
const PL_LOADED_PROFILE: i32 = PL_CONTROLLER + 4;

// The options menu's item ids (`HiddenMenu`).
const HM_RECORD_REPLAYS: i32 = 0;
const HM_LOAD_POWER_LEVELS: i32 = 1;
const HM_DOUBLE_RES: i32 = 2;
const HM_FULLSCREEN: i32 = 3;
const HM_AI_FRAMES: i32 = 4;
const HM_AI_MUTATIONS: i32 = 5;
const HM_PALETTE_SELECT: i32 = 6;
const HM_SHADOWS: i32 = 7;
const HM_SCREEN_SYNC: i32 = 8;
const HM_SELECT_BOT_WEAPONS: i32 = 9;
const HM_AI_TRACES: i32 = 10;
const HM_AI_PARALLELS: i32 = 11;
const HM_ALLOW_VIEWING_SPAWN_POINT: i32 = 12;
const HM_SINGLE_SCREEN_REPLAY: i32 = 13;

/// The file selectors (`app_files::open_selector`).
pub const LIERO_SELECTOR_LEVEL: i32 = 0;
pub const LIERO_SELECTOR_PROFILE: i32 = 1;
pub const LIERO_SELECTOR_REPLAY: i32 = 2;
pub const LIERO_SELECTOR_OPTIONS: i32 = 3;
pub const LIERO_SELECTOR_TC: i32 = 4;

// `Texts` in C++.
const ON_OFF: [&[u8]; 2] = [b"OFF", b"ON"];
const GAME_MODES: [&[u8]; 4] = [b"Kill'em All", b"Game of Tag", b"Holdazone", b"Scales of Justice"];
const CONTROLLERS: [&[u8]; 3] = [b"Human", b"CPU", b"AI"];
const WEAPON_STATES: [&[u8]; 3] = [b"Menu", b"Bonus", b"Banned"];
const BOT_WEAPONS: [&[u8]; 3] = [b"RANDOM", b"PICK", b"KEEP"];

/// The setting a switch, number or enum edits, as a number (a switch is 0
/// or 1, an enum's `u32` is read as is).
#[derive(Clone, Copy)]
enum Setting {
    Flag(fn(&mut GameSettings) -> &mut bool),
    Number(fn(&mut GameSettings) -> &mut i32),
    Choice(fn(&mut GameSettings) -> &mut u32),
    WormHealth,
    WormRgb(usize),
    WormController,
    WormWeapon(usize),
    /// The options menu's palette entry.
    PaletteColor,
    /// The window's double size and full screen, which the platform shows.
    DoubleRes,
    Fullscreen,
}

/// What an item does (`ItemBehavior` and its kinds).
enum Behavior {
    None,
    /// A switch, number or enum (`RustItemBehavior`) and what it edits.
    Item(LieroItemBehavior<'static>, Setting),
    /// One of a player's keys: `controls_ex[index]`, and `controls[index]`
    /// for a key the original controls have.
    Key {
        index: usize,
        extended: bool,
    },
    WormName,
    ProfileSave {
        save_as: bool,
    },
    ProfileLoaded,
    ProfileLoad,
    /// A player's weapon, an enum of the TC's weapons shown by name.
    Weapon {
        slot: usize,
    },
    Level,
    WeaponOptions,
    OptionsSave,
    OptionsSelect,
    ReplaySelect,
    TcSelect,
}

/// What happens when the screen an item opened closes.
enum Then {
    Key { item: i32, index: usize },
    WormName { item: i32 },
    ProfileSaveAs,
    WeaponSearch { slot: usize },
    Integer { item: i32 },
    OptionsSave { item: i32 },
    Level { item: i32 },
    Sound,
    SoundAndUpdate,
    Replay,
    Tc,
}

/// The app's menus, which the menu screen shares.
pub type SharedMenus = Rc<RefCell<LieroMenus>>;

struct Pending {
    kind: i32,
    then: Then,
    /// What the text box typed, before Enter.
    // Boxed: the text box is given its address.
    #[allow(clippy::box_collection)]
    typed: Box<Vec<u8>>,
}

/// The menus (`Gfx`'s `mainMenu`, `settingsMenu`, `playerMenu`,
/// `hiddenMenu` and `curMenu`).
pub struct LieroMenus {
    menus: [Box<LieroMenuBox>; 4],
    cur: i32,
    /// Whose settings the player menu shows.
    player: i32,
    /// The palette entry the options menu shows (`HiddenMenu::paletteColor`).
    palette_color: i32,
    pending: Option<Pending>,
}

fn lstr(b: &[u8]) -> LieroStr {
    LieroStr {
        ptr: b.as_ptr(),
        len: b.len(),
    }
}

fn names(n: &[&'static [u8]]) -> Vec<LieroStr> {
    n.iter().map(|s| lstr(s)).collect()
}

/// `getBasename(getLeaf(path))`: the name without folders or extension.
fn base_name(path: &[u8]) -> Vec<u8> {
    let leaf = match path.iter().rposition(|&b| b == b'/' || b == b'\\') {
        Some(i) => &path[i + 1..],
        None => path,
    };
    match leaf.iter().rposition(|&b| b == b'.') {
        Some(i) => leaf[..i].to_vec(),
        None => leaf.to_vec(),
    }
}

fn integer(min: i32, max: i32, step: i32, percentage: bool) -> LieroItemBehavior<'static> {
    let mut b = LieroItemBehavior::integer(min, max);
    b.step = step;
    if percentage {
        b.value_kind = LIERO_MENU_VALUE_PERCENTAGE;
    }
    b
}

/// Seconds, or frames shown as seconds; can't be typed in (`TimeBehavior`).
fn time(min: i32, max: i32, step: i32, frames: bool) -> LieroItemBehavior<'static> {
    let mut b = integer(min, max, step, false);
    b.value_kind = if frames {
        LIERO_MENU_VALUE_FRAMES
    } else {
        LIERO_MENU_VALUE_TIME
    };
    b.allow_entry = false;
    b
}

fn switch(setting: Setting) -> Behavior {
    Behavior::Item(LieroItemBehavior::switch(&ON_OFF), setting)
}

fn flag(f: fn(&mut GameSettings) -> &mut bool) -> Behavior {
    switch(Setting::Flag(f))
}

fn number(b: LieroItemBehavior<'static>, f: fn(&mut GameSettings) -> &mut i32) -> Behavior {
    Behavior::Item(b, Setting::Number(f))
}

/// An enum shown with `n`'s names (`ArrayEnumBehavior`).
fn array_enum(setting: Setting, n: &'static [&'static [u8]]) -> Behavior {
    Behavior::Item(LieroItemBehavior::enumeration(0, n.len() as i32 - 1, n), setting)
}

impl LieroMenus {
    fn new() -> Self {
        let raw = |x, y| Box::new(LieroMenuBox(OwnedMenu::raw(x, y, false)));
        LieroMenus {
            menus: [raw(53, 20), raw(178, 20), raw(178, 20), raw(178, 20)],
            cur: LIERO_MENU_MAIN,
            player: 0,
            palette_color: 0,
            pending: None,
        }
    }

    fn menu(&mut self, kind: i32) -> &mut OwnedMenu {
        &mut self.menus[kind as usize].0
    }

    /// Runs `f` on the player menu's worm's settings.
    fn with_worm<R>(&self, app: &LieroApp, f: impl FnOnce(&mut LieroWormSettingsBox) -> R) -> R {
        let ws = app.settings_mut().data.worms[self.player as usize].clone();
        let mut w = ws.borrow_mut();
        f(&mut w)
    }

    /// `Gfx::loadMenus`: the items, added to what the menus have.
    fn load(&mut self) {
        fn add(m: &mut OwnedMenu, color: u8, dis_colour: u8, name: &[u8], id: i32, selectable: bool) {
            let spec = LieroMenuItemSpec {
                string: lstr(name),
                value: lstr(&[]),
                color,
                dis_colour,
                has_value: false,
                visible: true,
                selectable,
                id,
            };
            m.insert(m.len(), &spec);
            m.m.visible_item_count += 1;
        }
        let item = |m: &mut OwnedMenu, color: u8, name: &[u8], id: i32| add(m, color, 7, name, id, true);

        let hidden = self.menu(LIERO_MENU_HIDDEN);
        for (name, id) in [
            (&b"FULLSCREEN (F11)"[..], HM_FULLSCREEN),
            (b"DOUBLE SIZE", HM_DOUBLE_RES),
            (b"POWERLEVEL PALETTES", HM_LOAD_POWER_LEVELS),
            (b"SHADOWS", HM_SHADOWS),
            (b"AUTO-RECORD REPLAYS", HM_RECORD_REPLAYS),
            (b"AI FRAMES", HM_AI_FRAMES),
            (b"AI MUTATIONS", HM_AI_MUTATIONS),
            (b"AI PARALLELS", HM_AI_PARALLELS),
            (b"AI TRACES", HM_AI_TRACES),
            (b"PALETTE", HM_PALETTE_SELECT),
            (b"BOT WEAPONS", HM_SELECT_BOT_WEAPONS),
            (b"SEE SPAWN POINT", HM_ALLOW_VIEWING_SPAWN_POINT),
            (b"SINGLE SCREEN REPLAY", HM_SINGLE_SCREEN_REPLAY),
        ] {
            item(hidden, 48, name, id);
        }

        let player = self.menu(LIERO_MENU_PLAYER);
        for (name, id) in [
            (&b"PROFILE LOADED"[..], PL_LOADED_PROFILE),
            (b"SAVE PROFILE", PL_SAVE_PROFILE),
            (b"SAVE PROFILE AS...", PL_SAVE_PROFILE_AS),
            (b"LOAD PROFILE", PL_LOAD_PROFILE),
        ] {
            item(player, 3, name, id);
        }
        for (name, id) in [
            (&b"NAME"[..], PL_NAME),
            (b"HEALTH", PL_HEALTH),
            (b"Red", PL_RED),
            (b"Green", PL_RED + 1),
            (b"Blue", PL_BLUE),
            (b"AIM UP", PL_UP),
            (b"AIM DOWN", PL_UP + 1),
            (b"MOVE LEFT", PL_UP + 2),
            (b"MOVE RIGHT", PL_UP + 3),
            (b"FIRE", PL_UP + 4),
            (b"CHANGE", PL_UP + 5),
            (b"JUMP", PL_JUMP),
            (b"DIG", PL_DIG),
        ] {
            item(player, 48, name, id);
        }
        for i in 0..5 {
            item(player, 48, format!("WEAPON {}", i + 1).as_bytes(), PL_WEAP0 + i);
        }
        item(player, 48, b"CONTROLLER", PL_CONTROLLER);

        let settings = self.menu(LIERO_MENU_SETTINGS);
        for (name, id) in [
            (&b"GAME MODE"[..], SI_GAME_MODE),
            (b"TIME TO LOSE", SI_TIME_TO_LOSE),
            (b"TIME TO WIN", SI_TIME_TO_WIN),
            (b"ZONE TIMEOUT", SI_ZONE_TIMEOUT),
            (b"FLAGS TO WIN", SI_FLAGS_TO_WIN),
            (b"LIVES", SI_LIVES),
            (b"LEVEL", SI_LEVEL),
            (b"LOADING TIMES", SI_LOADING_TIMES),
            (b"WEAPON OPTIONS", SI_WEAPON_OPTIONS),
            (b"MAX BONUSES", SI_MAX_BONUSES),
            (b"NAMES ON BONUSES", SI_NAMES_ON_BONUSES),
            (b"MAP", SI_MAP),
            (b"AMOUNT OF BLOOD", SI_AMOUNT_OF_BLOOD),
            (b"LOAD+CHANGE", SI_LOAD_CHANGE),
            (b"REGENERATE LEVEL", SI_REGENERATE_LEVEL),
            (b"SAVE SETUP AS...", SI_SAVE_OPTIONS),
            (b"LOAD SETUP", SI_LOAD_OPTIONS),
        ] {
            item(settings, 48, name, id);
        }

        let main = self.menu(LIERO_MENU_MAIN);
        // The first two names are set when the menu opens.
        add(main, 10, 10, b"", LIERO_MAIN_RESUME_GAME, true);
        add(main, 10, 10, b"", LIERO_MAIN_NEW_GAME, true);
        add(main, 48, 48, b"OPTIONS (F2)", MA_ADVANCED, true);
        add(main, 48, 48, b"REPLAYS (F3)", MA_REPLAYS, true);
        add(main, 48, 48, b"TC", MA_TC, true);
        add(main, 6, 6, b"QUIT TO OS", MA_QUIT, true);
        add(main, 0, 0, b"", -1, false);
        add(main, 48, 48, b"LEFT PLAYER (F5)", MA_PLAYER1_SETTINGS, true);
        add(main, 48, 48, b"RIGHT PLAYER (F6)", MA_PLAYER2_SETTINGS, true);
        add(main, 48, 48, b"MATCH SETUP (F7)", MA_SETTINGS, true);

        self.menu(LIERO_MENU_SETTINGS).m.value_offset_x = 100;
        self.menu(LIERO_MENU_PLAYER).m.value_offset_x = 95;
        self.menu(LIERO_MENU_HIDDEN).m.value_offset_x = 120;
    }

    /// What `kind`'s item `i` does (`getItemBehavior`).
    fn behavior(&mut self, app: &mut LieroApp, kind: i32, i: usize) -> Behavior {
        let id = self.menu(kind).ids[i];
        match kind {
            LIERO_MENU_MAIN => match id {
                MA_REPLAYS => Behavior::ReplaySelect,
                MA_TC => Behavior::TcSelect,
                _ => Behavior::None,
            },
            LIERO_MENU_SETTINGS => match id {
                SI_NAMES_ON_BONUSES => flag(|s| &mut s.names_on_bonuses),
                SI_MAP => flag(|s| &mut s.map),
                SI_REGENERATE_LEVEL => flag(|s| &mut s.regenerate_level),
                SI_LOADING_TIMES => number(integer(0, 9999, 1, true), |s| &mut s.loading_time),
                SI_MAX_BONUSES => number(integer(0, 99, 1, false), |s| &mut s.max_bonuses),
                SI_AMOUNT_OF_BLOOD => {
                    let tc = app.tc_data();
                    let mut b = integer(0, tc.constant(c("BloodLimit")), tc.constant(c("BloodStepUp")), true);
                    b.allow_entry = false;
                    number(b, |s| &mut s.blood)
                }
                SI_LIVES => number(integer(1, 999, 1, false), |s| &mut s.lives),
                SI_TIME_TO_LOSE | SI_TIME_TO_WIN => number(time(60, 3600, 10, false), |s| &mut s.time_to_lose),
                SI_ZONE_TIMEOUT => number(time(10, 3600, 10, false), |s| &mut s.ext.zone_timeout),
                SI_FLAGS_TO_WIN => number(integer(1, 999, 1, false), |s| &mut s.flags_to_win),
                SI_LEVEL => Behavior::Level,
                SI_GAME_MODE => array_enum(Setting::Choice(|s| &mut s.game_mode), &GAME_MODES),
                SI_WEAPON_OPTIONS => Behavior::WeaponOptions,
                SI_LOAD_OPTIONS => Behavior::OptionsSelect,
                SI_SAVE_OPTIONS => Behavior::OptionsSave,
                SI_LOAD_CHANGE => flag(|s| &mut s.load_change),
                _ => Behavior::None,
            },
            LIERO_MENU_PLAYER => {
                if (PL_WEAP0..PL_WEAP0 + 5).contains(&id) {
                    return Behavior::Weapon {
                        slot: (id - PL_WEAP0) as usize,
                    };
                }
                match id {
                    PL_NAME => Behavior::WormName,
                    PL_HEALTH => {
                        let mut b = integer(1, 10000, 1, true);
                        b.scroll_interval = 4;
                        Behavior::Item(b, Setting::WormHealth)
                    }
                    PL_RED..=PL_BLUE => {
                        let mut b = integer(0, 63, 1, false);
                        b.scroll_interval = 4;
                        Behavior::Item(b, Setting::WormRgb((id - PL_RED) as usize))
                    }
                    // The extended controls are always on (`Settings::extensions`).
                    PL_UP..=PL_DIG => Behavior::Key {
                        index: (id - PL_UP) as usize,
                        extended: true,
                    },
                    PL_CONTROLLER => array_enum(Setting::WormController, &CONTROLLERS),
                    PL_SAVE_PROFILE => Behavior::ProfileSave { save_as: false },
                    PL_SAVE_PROFILE_AS => Behavior::ProfileSave { save_as: true },
                    PL_LOAD_PROFILE => Behavior::ProfileLoad,
                    PL_LOADED_PROFILE => Behavior::ProfileLoaded,
                    _ => Behavior::None,
                }
            }
            _ => match id {
                HM_RECORD_REPLAYS => flag(|s| &mut s.ext.record_replays),
                HM_LOAD_POWER_LEVELS => flag(|s| &mut s.ext.load_powerlevel_palette),
                HM_DOUBLE_RES => switch(Setting::DoubleRes),
                HM_FULLSCREEN => switch(Setting::Fullscreen),
                HM_AI_FRAMES => number(integer(1, 70 * 5, 1, false), |s| &mut s.ext.ai_frames),
                HM_AI_MUTATIONS => number(integer(1, 20, 1, false), |s| &mut s.ext.ai_mutations),
                HM_PALETTE_SELECT => Behavior::Item(integer(0, 255, 1, false), Setting::PaletteColor),
                HM_SHADOWS => flag(|s| &mut s.shadow),
                HM_SCREEN_SYNC => flag(|s| &mut s.screen_sync),
                HM_SELECT_BOT_WEAPONS => array_enum(Setting::Choice(|s| &mut s.ext.select_bot_weapons), &BOT_WEAPONS),
                HM_AI_TRACES => flag(|s| &mut s.ext.ai_traces),
                HM_AI_PARALLELS => number(integer(1, 16, 1, false), |s| &mut s.ext.ai_parallels),
                HM_ALLOW_VIEWING_SPAWN_POINT => flag(|s| &mut s.ext.allow_viewing_spawn_point),
                HM_SINGLE_SCREEN_REPLAY => flag(|s| &mut s.ext.single_screen_replay),
                _ => Behavior::None,
            },
        }
    }

    /// The value `setting` has now.
    fn get(&self, app: &LieroApp, setting: Setting) -> i32 {
        match setting {
            Setting::Flag(f) => i32::from(*f(&mut app.settings_mut().data)),
            Setting::Number(f) => *f(&mut app.settings_mut().data),
            Setting::Choice(f) => *f(&mut app.settings_mut().data) as i32,
            Setting::WormHealth => self.with_worm(app, |w| w.data.health),
            Setting::WormRgb(i) => self.with_worm(app, |w| w.data.rgb[i]),
            Setting::WormController => self.with_worm(app, |w| w.data.controller as i32),
            Setting::WormWeapon(slot) => self.with_worm(app, |w| w.data.weapons[slot] as i32),
            Setting::PaletteColor => self.palette_color,
            Setting::DoubleRes => i32::from(app.double_res),
            Setting::Fullscreen => i32::from(app.settings_mut().data.ext.fullscreen),
        }
    }

    /// Changes `setting` to `v`.
    fn set(&mut self, app: &mut LieroApp, host: &Host, setting: Setting, v: i32) {
        match setting {
            Setting::Flag(f) => *f(&mut app.settings_mut().data) = v != 0,
            Setting::Number(f) => *f(&mut app.settings_mut().data) = v,
            Setting::Choice(f) => *f(&mut app.settings_mut().data) = v as u32,
            Setting::WormHealth => self.with_worm(app, |w| w.data.health = v),
            Setting::WormRgb(i) => self.with_worm(app, |w| w.data.rgb[i] = v),
            Setting::WormController => self.with_worm(app, |w| w.data.controller = v as u32),
            Setting::WormWeapon(slot) => self.with_worm(app, |w| w.data.weapons[slot] = v as u32),
            Setting::PaletteColor => self.palette_color = v,
            // The platform shows these; the options menu updates itself.
            Setting::DoubleRes => {
                app.double_res = v != 0;
                host.set_double_res(v != 0);
            }
            // The platform sets it, if it has a window to show it in.
            Setting::Fullscreen => host.set_fullscreen(app, v != 0),
        }
    }

    fn set_value(&mut self, kind: i32, i: usize, v: Vec<u8>) {
        self.menu(kind).set_value(i, v);
    }

    fn index_of(&mut self, kind: i32, id: i32) -> Option<usize> {
        self.menu(kind).ids.iter().position(|&x| x == id)
    }

    /// The player's profile file, if there is one.
    fn profile_path(&self, app: &LieroApp) -> Option<Vec<u8>> {
        self.with_worm(app, |w| w.profile_path())
    }

    /// Shows item `i`'s value (`onUpdate`).
    fn update(&mut self, app: &mut LieroApp, kind: i32, i: usize, b: &Behavior) {
        match b {
            Behavior::Weapon { slot } => {
                // Shown by the weapon's name, in menu order.
                let v = self.get(app, Setting::WormWeapon(*slot)) as usize;
                let tc = app.tc_data();
                let name = tc.weapon_name(tc.weapon_order()[v - 1] as usize).to_vec();
                self.set_value(kind, i, name);
            }
            Behavior::Item(it, setting) => {
                let v = it.value_text(self.get(app, *setting));
                self.set_value(kind, i, v);
            }
            Behavior::Key { index, extended } => {
                let k = self.with_worm(app, |w| {
                    if *extended {
                        w.data.controls_ex[*index]
                    } else {
                        w.data.controls[*index]
                    }
                });
                self.set_value(kind, i, key_name(k));
            }
            Behavior::WormName => {
                let name = self.with_worm(app, |w| w.data.name.clone());
                self.set_value(kind, i, name);
            }
            Behavior::ProfileSave { save_as: false } => {
                let has = self.profile_path(app).is_some();
                self.menu(kind).visible[i] = has;
            }
            Behavior::ProfileLoaded => {
                let p = self.profile_path(app);
                self.set_value(kind, i, p.as_deref().map(base_name).unwrap_or_default());
                self.menu(kind).visible[i] = p.is_some();
            }
            Behavior::Level => self.update_level(app, kind, i),
            Behavior::OptionsSave => {
                let name = base_name(&crate::app_files::settings_path(app));
                self.set_value(kind, i, name);
            }
            _ => {}
        }
    }

    /// The level item's value, and the regenerate item's name to go with it.
    fn update_level(&mut self, app: &mut LieroApp, kind: i32, i: usize) {
        let st = app.settings_mut();
        let tc = app.tc_data();
        let (value, regen) = if !st.data.random_level {
            let mut v = b"\"".to_vec();
            v.extend(base_name(&st.data.level_file));
            v.push(b'"');
            (v, tc.text(s("ReloadLevel")).to_vec())
        } else {
            (tc.text(s("Random2")).to_vec(), tc.text(s("RegenLevel")).to_vec())
        };
        drop((st, tc));
        self.set_value(kind, i, value);
        let r = self.index_of(kind, SI_REGENERATE_LEVEL).expect("no regenerate item");
        self.menu(kind).names[r] = regen;
    }

    /// `Menu::updateItems`: every item's value, then what the menu shows.
    pub fn update_items(&mut self, app: &mut LieroApp, kind: i32) {
        for i in 0..self.menu(kind).len() {
            let b = self.behavior(app, kind, i);
            self.update(app, kind, i, &b);
        }
        if kind == LIERO_MENU_SETTINGS {
            // `SettingsMenu::onUpdate`: the game mode's own items.
            let mode = app.settings_mut().data.game_mode;
            let shown: &[i32] = match mode {
                0 | 3 => &[SI_LIVES],
                1 => &[SI_TIME_TO_LOSE],
                2 => &[SI_TIME_TO_WIN, SI_ZONE_TIMEOUT],
                _ => &[],
            };
            for id in [
                SI_LIVES,
                SI_TIME_TO_LOSE,
                SI_TIME_TO_WIN,
                SI_ZONE_TIMEOUT,
                SI_FLAGS_TO_WIN,
            ] {
                self.set_visibility(kind, id, false);
            }
            for &id in shown {
                self.set_visibility(kind, id, true);
            }
        }
    }

    fn set_visibility(&mut self, kind: i32, id: i32, state: bool) {
        let i = self.index_of(kind, id).expect("no such item") as i32;
        self.menu(kind)
            .with(|m, items, n| unsafe { crate::menu::liero_menu_set_visibility(m, items, n, i, state) });
    }

    fn selection(&mut self, kind: i32) -> Option<usize> {
        self.menu(kind).selected()
    }

    /// Left or right on the selected item. False to stop repeating it.
    pub fn left_right(&mut self, app: &mut LieroApp, host: &Host, kind: i32, dir: i32) -> bool {
        let Some(i) = self.selection(kind) else { return false };
        let b = self.behavior(app, kind, i);
        let (it, setting) = match &b {
            Behavior::Item(it, setting) => (*it, *setting),
            Behavior::Weapon { slot } => (self.weapon_item(app), Setting::WormWeapon(*slot)),
            _ => return true,
        };
        let cycles = app.menu_cycles;
        if let Some(s) = it.left_right_sound(dir) {
            host.play_sound(s);
        }
        let a = self.change(app, host, &it, setting, |it, v| it.left_right(v, dir, cycles));
        // Left and right never open the number box.
        let _ = self.apply(app, kind, i, &b, a);
        a.keep_going
    }

    /// A player's weapon: one of the TC's, shown by name.
    fn weapon_item(&self, app: &LieroApp) -> LieroItemBehavior<'static> {
        LieroItemBehavior::enumeration(1, app.tc_data().model.weapons.len() as i32, &[])
    }

    /// Runs a key on item `it`, which edits `setting`, and writes the
    /// value it gives back if it changed.
    fn change(
        &mut self,
        app: &mut LieroApp,
        host: &Host,
        it: &LieroItemBehavior,
        setting: Setting,
        key: impl FnOnce(&LieroItemBehavior, i32) -> (i32, LieroItemAction),
    ) -> LieroItemAction {
        let v = self.get(app, setting);
        let (new_v, a) = key(it, v);
        if new_v != v {
            self.set(app, host, setting, new_v);
        }
        a
    }

    /// Does what a switch, number or enum asked: show its value, update
    /// the menu or open the number box.
    fn apply(
        &mut self,
        app: &mut LieroApp,
        kind: i32,
        i: usize,
        b: &Behavior,
        a: LieroItemAction,
    ) -> Option<BoxedScreen> {
        if a.update_item {
            self.update(app, kind, i, b);
        }
        // The window's switches show in other items too (`setFullscreen`
        // and `setDoubleRes` update the menu).
        let id = self.menu(kind).ids[i];
        if a.update_menu || (kind == LIERO_MENU_HIDDEN && a.update_item && (id == HM_DOUBLE_RES || id == HM_FULLSCREEN))
        {
            self.update_items(app, kind);
        }
        if a.enter_digits > 0 {
            let (x, y) = self.menu(kind).item_position(i as i32)?;
            let x = x + self.menu(kind).m.value_offset_x;
            let Behavior::Item(_, setting) = b else { return None };
            let text = self.get(app, *setting).to_string().into_bytes();
            return Some(self.input(
                kind,
                Then::Integer { item: i as i32 },
                &text,
                a.enter_digits as usize,
                x + 2,
                y,
                true,
            ));
        }
        None
    }

    /// Opens a text box; `then` gets what it typed.
    #[allow(clippy::too_many_arguments)]
    fn input(
        &mut self,
        kind: i32,
        then: Then,
        text: &[u8],
        max_len: usize,
        x: i32,
        y: i32,
        digits: bool,
    ) -> BoxedScreen {
        unsafe fn keep(ctx: *mut c_void, text: &[u8]) {
            unsafe {
                *(ctx as *mut Vec<u8>) = text.to_vec();
            }
        }
        let mut typed = Box::new(text.to_vec());
        let ctx = (&mut *typed as *mut Vec<u8>).cast();
        let screen = unsafe { liero_input_box_screen(text, max_len, x, y, digits, Some(keep), ctx) };
        self.pending = Some(Pending { kind, then, typed });
        screen
    }

    fn open(&mut self, kind: i32, then: Then, screen: BoxedScreen) -> BoxedScreen {
        self.pending = Some(Pending {
            kind,
            then,
            typed: Box::default(),
        });
        screen
    }

    /// Where a text box opens beside item `i`'s value.
    fn value_position(&mut self, kind: i32, i: usize) -> Option<(i32, i32)> {
        let (x, y) = self.menu(kind).item_position(i as i32)?;
        Some((x + self.menu(kind).m.value_offset_x + 2, y))
    }

    /// A file selector.
    fn selector(&mut self, app: &mut LieroApp, kind: i32, then: Then, selector: i32) -> Option<BoxedScreen> {
        let screen = crate::app_files::open_selector(app, selector, self.player);
        Some(self.open(kind, then, screen))
    }

    /// Enter on the selected item (`onEnter`): what it chose, or -1, and
    /// the screen it opened, if any.
    pub fn enter(&mut self, app: &mut LieroApp, host: &Host, kind: i32) -> (i32, Option<BoxedScreen>) {
        let Some(i) = self.selection(kind) else {
            return (0, None);
        };
        let b = self.behavior(app, kind, i);
        let sound = || host.play_sound(SOUND_CHOICE);
        let opened = match b {
            Behavior::None | Behavior::ProfileLoaded => None,
            Behavior::Item(it, setting) => {
                host.play_sound(crate::item::SOUND_ENTER);
                let a = self.change(app, host, &it, setting, |it, v| it.enter(v));
                self.apply(app, kind, i, &b, a)
            }
            Behavior::Key { index, extended } => {
                sound();
                let screen = liero_key_choice_screen(extended);
                Some(self.open(kind, Then::Key { item: i as i32, index }, screen))
            }
            Behavior::WormName => {
                sound();
                self.with_worm(app, |w| w.data.random_name = false);
                match self.value_position(kind, i) {
                    None => None,
                    Some((x, y)) => {
                        let name = self.with_worm(app, |w| w.data.name.clone());
                        Some(self.input(kind, Then::WormName { item: i as i32 }, &name, 20, x, y, false))
                    }
                }
            }
            Behavior::ProfileSave { save_as } => {
                sound();
                match self.value_position(kind, i) {
                    None => None,
                    Some((x, y)) if save_as => Some(self.input(kind, Then::ProfileSaveAs, b"", 30, x, y, false)),
                    Some(_) => {
                        if self.with_worm(app, |w| w.save_to_profile()) {
                            self.update_items(app, kind);
                        }
                        None
                    }
                }
            }
            Behavior::Weapon { slot } => {
                sound();
                self.value_position(kind, i)
                    .map(|(x, y)| self.input(kind, Then::WeaponSearch { slot }, b"", 10, x, y, false))
            }
            Behavior::ProfileLoad => {
                sound();
                self.selector(app, kind, Then::SoundAndUpdate, LIERO_SELECTOR_PROFILE)
            }
            Behavior::Level => {
                sound();
                self.selector(app, kind, Then::Level { item: i as i32 }, LIERO_SELECTOR_LEVEL)
            }
            Behavior::WeaponOptions => {
                sound();
                let screen = self.weapon_options(app);
                Some(self.open(kind, Then::Sound, screen))
            }
            Behavior::OptionsSave => {
                sound();
                match self.value_position(kind, i) {
                    None => None,
                    Some((x, y)) => {
                        let name = base_name(&crate::app_files::settings_path(app));
                        Some(self.input(kind, Then::OptionsSave { item: i as i32 }, &name, 30, x, y, false))
                    }
                }
            }
            Behavior::OptionsSelect => {
                sound();
                self.selector(app, kind, Then::SoundAndUpdate, LIERO_SELECTOR_OPTIONS)
            }
            Behavior::ReplaySelect => {
                sound();
                self.selector(app, kind, Then::Replay, LIERO_SELECTOR_REPLAY)
            }
            Behavior::TcSelect => {
                sound();
                self.selector(app, kind, Then::Tc, LIERO_SELECTOR_TC)
            }
        };
        (-1, opened)
    }

    /// The weapon options screen, on the settings' weapon table.
    fn weapon_options(&mut self, app: &mut LieroApp) -> BoxedScreen {
        let tc = app.tc_data();
        let order = tc.weapon_order();
        let weapons: Vec<LieroStr> = order.iter().map(|&w| lstr(tc.weapon_name(w as usize))).collect();
        let states = names(&WEAPON_STATES);
        let spec = LieroWeaponOptionsSpec {
            names: weapons.as_ptr(),
            order: order.as_ptr(),
            count: weapons.len(),
            weap_table: app.settings_mut().data.weap_table.as_mut_ptr(),
            states: states.as_ptr(),
            state_count: states.len(),
            weapon_title: lstr(tc.text(s("Weapon"))),
            availability_title: lstr(tc.text(s("Availability"))),
            no_weapons: lstr(tc.text(s("NoWeaps"))),
        };
        unsafe { liero_weapon_options_screen(&spec) }
    }

    /// The screen the last item opened closed with `result`: what the item
    /// does with it, and what it chose, or -1.
    pub fn then(&mut self, app: &mut LieroApp, host: &Host, result: i32) -> i32 {
        let Some(Pending { kind, then, typed }) = self.pending.take() else {
            return -1;
        };
        let sound = || host.play_sound(SOUND_CHOICE);
        let ok = result != 0;
        let typed = *typed;
        match then {
            Then::Key { item, index } => {
                let k = result as u32;
                if k != LIERO_DK_ESCAPE {
                    self.with_worm(app, |w| {
                        let ws = &mut w.data;
                        // Dig has only an extended control (`PlDig`'s
                        // `KeyBehavior` binds `controlsEx` twice).
                        if !is_extended(k) && index < ws.controls.len() {
                            ws.controls[index] = k;
                        }
                        ws.controls_ex[index] = k;
                    });
                    let b = self.behavior(app, kind, item as usize);
                    self.update(app, kind, item as usize, &b);
                }
                app.clear_keys();
            }
            Then::WormName { item } => {
                if ok {
                    self.with_worm(app, |w| w.data.name = typed);
                }
                // An empty name stays empty: `Settings::generateName` does
                // nothing.
                sound();
                let b = self.behavior(app, kind, item as usize);
                self.update(app, kind, item as usize, &b);
            }
            Then::ProfileSaveAs => {
                if ok && !typed.is_empty() {
                    crate::app_files::save_profile_as(app, self.player, &typed);
                }
                sound();
                self.update_items(app, kind);
            }
            Then::WeaponSearch { slot } => {
                if ok {
                    let closest = {
                        let tc = app.tc_data();
                        let order = tc.weapon_order();
                        let names: Vec<LieroStr> = order.iter().map(|&w| lstr(tc.weapon_name(w as usize))).collect();
                        unsafe {
                            liero_menu_closest_name(names.as_ptr(), names.len() as i32, typed.as_ptr(), typed.len())
                        }
                    };
                    if closest >= 0 {
                        self.set(app, host, Setting::WormWeapon(slot), 1 + closest);
                    }
                    self.update_items(app, kind);
                }
            }
            Then::Integer { item } => {
                let b = self.behavior(app, kind, item as usize);
                if let Behavior::Item(it, setting) = &b {
                    if ok && !typed.is_empty() {
                        let v = atoi(&typed).clamp(it.min, it.max);
                        self.set(app, host, *setting, v);
                    }
                }
                self.update(app, kind, item as usize, &b);
            }
            Then::OptionsSave { item } => {
                if ok && !typed.is_empty() {
                    crate::app_files::save_options(app, &typed);
                }
                sound();
                let b = self.behavior(app, kind, item as usize);
                self.update(app, kind, item as usize, &b);
            }
            Then::Level { item } => {
                sound();
                self.update_level(app, kind, item as usize);
            }
            Then::Sound => sound(),
            Then::SoundAndUpdate => {
                sound();
                self.update_items(app, kind);
            }
            Then::Replay => {
                sound();
                return result;
            }
            Then::Tc => {
                if crate::app_files::take_chosen_tc(app) {
                    return MA_TC;
                }
            }
        }
        -1
    }

    /// Opens the player menu on `player`'s settings (`Gfx::playerSettings`).
    pub fn player_settings(&mut self, app: &mut LieroApp, player: i32) {
        self.player = player;
        self.update_items(app, LIERO_MENU_PLAYER);
        self.menu(LIERO_MENU_PLAYER).move_to(0);
        self.cur = LIERO_MENU_PLAYER;
    }

    /// `Gfx::openHiddenMenu`.
    pub fn open_hidden(&mut self, app: &mut LieroApp) {
        if self.cur == LIERO_MENU_HIDDEN {
            return;
        }
        self.cur = LIERO_MENU_HIDDEN;
        self.update_items(app, LIERO_MENU_HIDDEN);
        self.menu(LIERO_MENU_HIDDEN).move_to(0);
    }

    pub fn cur(&self) -> i32 {
        self.cur
    }

    pub fn set_cur(&mut self, kind: i32) {
        self.cur = kind;
    }

    pub fn box_of(&mut self, kind: i32) -> &mut LieroMenuBox {
        &mut self.menus[kind as usize]
    }

    /// Draws `kind`, with the player menu's colour bars and the options
    /// menu's palette entry.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        app: &LieroApp,
        kind: i32,
        scr: &LieroBitmap,
        font: &LieroFont,
        disabled: bool,
        x: i32,
        show: bool,
    ) {
        struct Ctx<'a> {
            ids: Vec<i32>,
            kind: i32,
            scr: LieroBitmap<'a>,
            player: i32,
            palette_color: i32,
            settings: crate::settings::SharedSettings,
        }
        unsafe fn overlay(ctx: *mut c_void, item: i32, x: i32, y: i32, selected: bool, _disabled: bool) {
            unsafe {
                let c = &*(ctx as *const Ctx);
                let id = c.ids[item as usize];
                if c.kind == LIERO_MENU_PLAYER && (PL_RED..=PL_BLUE).contains(&id) {
                    let ws = c.settings.borrow().data.worms[c.player as usize].clone();
                    let ws = &ws.borrow().data;
                    liero_menu_draw_rgb_overlay(&c.scr, x, y, selected, ws.rgb[(id - PL_RED) as usize], ws.color);
                } else if c.kind == LIERO_MENU_HIDDEN && id == HM_PALETTE_SELECT {
                    liero_menu_draw_palette_overlay(&c.scr, x, y, selected, c.palette_color);
                }
            }
        }
        let ctx = Ctx {
            ids: self.menu(kind).ids.clone(),
            kind,
            scr: *scr,
            player: self.player,
            palette_color: self.palette_color,
            settings: app.shared_settings(),
        };
        let hooks = LieroMenuHooks {
            ctx: &ctx as *const Ctx as *mut c_void,
            draw_overlay: overlay,
        };
        self.menu(kind).draw_with(scr, font, &hooks, disabled, x, show);
    }
}

/// C's `atoi` on digits: the leading number, saturating as a 32-bit int.
fn atoi(s: &[u8]) -> i32 {
    let mut v: i64 = 0;
    for &b in s.iter().take_while(|b| b.is_ascii_digit()) {
        v = (v * 10 + i64::from(b - b'0')).min(i64::from(i32::MAX));
    }
    v as i32
}

/// The menus, empty (`Menu(x, y)` for each).
pub fn liero_menus_new() -> SharedMenus {
    Rc::new(RefCell::new(LieroMenus::new()))
}

/// Empties the menus, as they were made, keeping their addresses.
pub fn liero_menus_reset(m: &mut LieroMenus) {
    let fresh = LieroMenus::new();
    for (dst, src) in m.menus.iter_mut().zip(fresh.menus) {
        dst.0 = src.0.clone();
    }
    m.cur = LIERO_MENU_MAIN;
    m.player = 0;
    m.palette_color = 0;
    m.pending = None;
}

/// Adds the menus' items (`Gfx::loadMenus`).
pub fn liero_menus_load(m: &mut LieroMenus) {
    m.load();
}

pub fn liero_menus_box(m: &mut LieroMenus, kind: i32) -> *mut LieroMenuBox {
    m.box_of(kind)
}

pub fn liero_menus_cur(m: &LieroMenus) -> i32 {
    m.cur
}

pub fn liero_menus_set_cur(m: &mut LieroMenus, kind: i32) {
    m.cur = kind;
}

pub fn liero_menus_player_settings(m: &mut LieroMenus, app: &mut LieroApp, player: i32) {
    m.player_settings(app, player);
}

pub fn liero_menus_update_items(m: &mut LieroMenus, app: &mut LieroApp, kind: i32) {
    m.update_items(app, kind);
}

pub fn liero_menus_left_right(m: &mut LieroMenus, app: &mut LieroApp, host: &Host, kind: i32, dir: i32) -> bool {
    m.left_right(app, host, kind, dir)
}

/// Enter on `kind`'s selected item. A screen it opened is dropped: the menu
/// checks only press items that open none.
pub fn liero_menus_enter(m: &mut LieroMenus, app: &mut LieroApp, host: &Host, kind: i32) -> i32 {
    let (r, opened) = m.enter(app, host, kind);
    if let Some(s) = opened {
        drop(s);
        m.pending = None;
    }
    r
}

#[allow(clippy::too_many_arguments)]
pub fn liero_menus_draw(
    m: &mut LieroMenus,
    app: &LieroApp,
    kind: i32,
    scr: &LieroBitmap,
    font: &LieroFont,
    disabled: bool,
    x: i32,
    show_disabled_selection: bool,
) {
    m.draw(app, kind, scr, font, disabled, x, show_disabled_selection);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_names() {
        assert_eq!(base_name(b"config/levels\\FOO.LEV"), b"FOO");
        assert_eq!(base_name(b"a.b/c"), b"c");
        assert_eq!(base_name(b"x.tar.gz"), b"x.tar");
        assert_eq!(base_name(b""), b"");
    }

    #[test]
    fn atoi_reads_the_leading_digits() {
        assert_eq!(atoi(b"0042"), 42);
        assert_eq!(atoi(b"12a3"), 12);
        assert_eq!(atoi(b"99999999999"), i32::MAX);
    }

    /// The menus with their items, and an app with new settings.
    fn menus() -> (LieroMenus, LieroApp) {
        let mut a = LieroApp::new();
        a.settings = Some(crate::settings::liero_settings_box_new());
        a.files.settings_node = crate::fs::Node::new(b"config/LIERO.CFG");
        a.double_res = false;
        a.menu_cycles = 0;
        let mut m = LieroMenus::new();
        m.load();
        (m, a)
    }

    /// The player menu's worm's settings.
    fn worm(a: &LieroApp, m: &LieroMenus) -> crate::settings::SharedWormSettings {
        a.settings_mut().data.worms[m.player as usize].clone()
    }

    fn select(m: &mut LieroMenus, kind: i32, id: i32) -> usize {
        let i = m.index_of(kind, id).unwrap();
        m.menu(kind).move_to(i as i32);
        assert_eq!(m.selection(kind), Some(i));
        i
    }

    fn value(m: &mut LieroMenus, kind: i32, i: usize) -> Vec<u8> {
        m.menu(kind).values[i].clone()
    }

    #[test]
    fn a_key_only_the_extended_controls_have_leaves_the_original_ones() {
        let (mut m, mut app) = menus();
        select(&mut m, LIERO_MENU_PLAYER, PL_UP + 1);
        let w = worm(&app, &m);
        w.borrow_mut().data.controls[1] = 30;
        w.borrow_mut().data.controls_ex[1] = 30;
        let keys = || {
            let ws = &w.borrow().data;
            (ws.controls[1], ws.controls_ex[1])
        };

        let (r, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_PLAYER);
        assert_eq!(r, -1);
        drop(opened.expect("key choice"));
        assert!(is_extended(177));
        m.then(&mut app, &Host::none(), 177);
        assert_eq!(keys(), (30, 177));

        let (_, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_PLAYER);
        drop(opened.unwrap());
        m.then(&mut app, &Host::none(), 31);
        assert_eq!(keys(), (31, 31));

        // Escape keeps the key.
        let (_, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_PLAYER);
        drop(opened.unwrap());
        m.then(&mut app, &Host::none(), LIERO_DK_ESCAPE as i32);
        assert_eq!(keys(), (31, 31));
    }

    #[test]
    fn dig_takes_any_key_into_the_extended_controls_only() {
        let (mut m, mut app) = menus();
        for player in 0..2 {
            m.player = player;
            select(&mut m, LIERO_MENU_PLAYER, PL_DIG);
            let w = worm(&app, &m);
            let before = w.borrow().data.controls;
            let (_, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_PLAYER);
            drop(opened.expect("key choice"));
            // C, which has a DOS key.
            assert!(!is_extended(46));
            m.then(&mut app, &Host::none(), 46);
            let ws = &w.borrow().data;
            assert_eq!((ws.controls, ws.controls_ex[7]), (before, 46));
        }
    }

    #[test]
    fn a_typed_number_is_kept_in_range() {
        let (mut m, mut app) = menus();
        let i = select(&mut m, LIERO_MENU_SETTINGS, SI_LIVES);
        for (typed, lives) in [(&b"5000"[..], 999), (b"0", 1), (b"12", 12)] {
            let (_, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_SETTINGS);
            drop(opened.expect("number box"));
            *m.pending.as_mut().unwrap().typed = typed.to_vec();
            assert_eq!(m.then(&mut app, &Host::none(), 1), -1);
            assert_eq!(app.settings_mut().data.lives, lives);
            assert_eq!(value(&mut m, LIERO_MENU_SETTINGS, i), lives.to_string().into_bytes());
        }
        // Escape keeps the number.
        let (_, opened) = m.enter(&mut app, &Host::none(), LIERO_MENU_SETTINGS);
        drop(opened.unwrap());
        *m.pending.as_mut().unwrap().typed = b"7".to_vec();
        m.then(&mut app, &Host::none(), 0);
        assert_eq!(app.settings_mut().data.lives, 12);
    }

    #[test]
    fn held_keys_step_health_every_fourth_frame() {
        let (mut m, mut app) = menus();
        select(&mut m, LIERO_MENU_PLAYER, PL_HEALTH);
        worm(&app, &m).borrow_mut().data.health = 100;
        let start = 100;
        let mut steps = Vec::new();
        for cycle in 1..=12 {
            app.menu_cycles = cycle;
            assert!(m.left_right(&mut app, &Host::none(), LIERO_MENU_PLAYER, -1));
            steps.push(start - worm(&app, &m).borrow().data.health);
        }
        assert_eq!(steps, [0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3]);
    }

    #[test]
    fn double_size_shows_the_whole_options_menu_again() {
        let (mut m, mut app) = menus();
        let frames = m.index_of(LIERO_MENU_HIDDEN, HM_AI_FRAMES).unwrap();
        select(&mut m, LIERO_MENU_HIDDEN, HM_DOUBLE_RES);
        app.settings_mut().data.ext.ai_frames = 123;
        m.left_right(&mut app, &Host::none(), LIERO_MENU_HIDDEN, 1);
        assert!(app.double_res);
        assert_eq!(value(&mut m, LIERO_MENU_HIDDEN, frames), b"123");
    }
}
