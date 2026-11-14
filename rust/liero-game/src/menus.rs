//! The menus beside the main menu screen (C++'s `MainMenu`, `SettingsMenu`,
//! `PlayerMenu` and `HiddenMenu`): their items, which setting each one
//! changes, how it shows its value, and the screens an item opens (a key
//! choice, a text box, the weapon options or a file selector). An item that
//! opens a screen goes on when that screen closes (`Menus::then`).
//!
//! Each switch, number or enum names the setting it edits (`Setting`), which
//! is read and written in the app's settings when a key changes it. The
//! window's double size and full screen are shown by the platform.

use crate::app::App;
use crate::boxes::{input_box_screen, key_choice_screen};
use crate::gfx::Bitmap;
use crate::item::{ItemAction, ItemBehavior};
use crate::keys::{is_extended, key_name, DK_ESCAPE};
use crate::menu::{closest_name, draw_palette_overlay, draw_rgb_overlay, Menu, MenuHooks, MenuItem, ValueKind};
use crate::menu_screen::MenuKind;
use crate::screen::{BoxedScreen, Host};
use crate::settings::{GameSettings, WormSettingsBox};
use crate::tc::{c, s};
use crate::text::{Font, Str};
use crate::weapon_options::{weapon_options_screen, WeaponOptionsSpec};
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

const SOUND_CHOICE: i32 = 27;

/// The main menu's item ids (`MainMenu`), which are also what it closes
/// with.
pub const MAIN_RESUME_GAME: i32 = 0;
pub const MAIN_NEW_GAME: i32 = 1;
pub const MAIN_SETTINGS: i32 = 2;
pub const MAIN_PLAYER1_SETTINGS: i32 = 3;
pub const MAIN_PLAYER2_SETTINGS: i32 = 4;
pub const MAIN_ADVANCED: i32 = 5;
pub const MAIN_QUIT: i32 = 6;
pub const MAIN_REPLAYS: i32 = 7;
/// What choosing a replay closes the menu with.
pub const MAIN_REPLAY: i32 = 8;
pub const MAIN_TC: i32 = 9;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector {
    Level,
    Profile,
    Replay,
    Options,
    Tc,
}

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
    Item(ItemBehavior<'static>, Setting),
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
pub type SharedMenus = Rc<RefCell<Menus>>;

struct Pending {
    kind: MenuKind,
    then: Then,
    /// What the text box typed, before Enter.
    // Boxed: the text box is given its address.
    #[allow(clippy::box_collection)]
    typed: Box<Vec<u8>>,
}

/// The menus (`Gfx`'s `mainMenu`, `settingsMenu`, `playerMenu`,
/// `hiddenMenu` and `curMenu`).
pub struct Menus {
    menus: [Box<Menu>; 4],
    cur: MenuKind,
    /// Whose settings the player menu shows.
    player: i32,
    /// The palette entry the options menu shows (`HiddenMenu::paletteColor`).
    palette_color: i32,
    pending: Option<Pending>,
}

fn lstr(b: &[u8]) -> Str {
    Str {
        ptr: b.as_ptr(),
        len: b.len(),
    }
}

fn names(n: &[&'static [u8]]) -> Vec<Str> {
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

fn integer(min: i32, max: i32, step: i32, percentage: bool) -> ItemBehavior<'static> {
    let mut b = ItemBehavior::integer(min, max);
    b.step = step;
    if percentage {
        b.value_kind = ValueKind::Percentage;
    }
    b
}

/// Seconds, or frames shown as seconds; can't be typed in (`TimeBehavior`).
fn time(min: i32, max: i32, step: i32, frames: bool) -> ItemBehavior<'static> {
    let mut b = integer(min, max, step, false);
    b.value_kind = if frames { ValueKind::Frames } else { ValueKind::Time };
    b.allow_entry = false;
    b
}

fn switch(setting: Setting) -> Behavior {
    Behavior::Item(ItemBehavior::switch(&ON_OFF), setting)
}

fn flag(f: fn(&mut GameSettings) -> &mut bool) -> Behavior {
    switch(Setting::Flag(f))
}

fn number(b: ItemBehavior<'static>, f: fn(&mut GameSettings) -> &mut i32) -> Behavior {
    Behavior::Item(b, Setting::Number(f))
}

/// An enum shown with `n`'s names (`ArrayEnumBehavior`).
fn array_enum(setting: Setting, n: &'static [&'static [u8]]) -> Behavior {
    Behavior::Item(ItemBehavior::enumeration(0, n.len() as i32 - 1, n), setting)
}

impl Menus {
    fn new() -> Self {
        let raw = |x, y| Box::new(Menu::raw(x, y, false));
        Menus {
            menus: [raw(53, 20), raw(178, 20), raw(178, 20), raw(178, 20)],
            cur: MenuKind::Main,
            player: 0,
            palette_color: 0,
            pending: None,
        }
    }

    fn menu(&mut self, kind: MenuKind) -> &mut Menu {
        &mut self.menus[kind as usize]
    }

    /// Runs `f` on the player menu's worm's settings.
    fn with_worm<R>(&self, app: &App, f: impl FnOnce(&mut WormSettingsBox) -> R) -> R {
        let ws = app.settings_mut().data.worms[self.player as usize].clone();
        let mut w = ws.borrow_mut();
        f(&mut w)
    }

    /// `Gfx::loadMenus`: the items, added to what the menus have.
    /// Adds the menus' items (`Gfx::loadMenus`).
    pub fn load(&mut self) {
        fn add(m: &mut Menu, color: u8, dis_colour: u8, name: &[u8], id: i32, selectable: bool) {
            let mut item = MenuItem::new(name, color, dis_colour);
            item.selectable = selectable;
            item.id = id;
            m.add_at(item, None, true);
        }
        let item = |m: &mut Menu, color: u8, name: &[u8], id: i32| add(m, color, 7, name, id, true);

        let hidden = self.menu(MenuKind::Hidden);
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

        let player = self.menu(MenuKind::Player);
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

        let settings = self.menu(MenuKind::Settings);
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

        let main = self.menu(MenuKind::Main);
        // The first two names are set when the menu opens.
        add(main, 10, 10, b"", MAIN_RESUME_GAME, true);
        add(main, 10, 10, b"", MAIN_NEW_GAME, true);
        add(main, 48, 48, b"OPTIONS (F2)", MAIN_ADVANCED, true);
        add(main, 48, 48, b"REPLAYS (F3)", MAIN_REPLAYS, true);
        add(main, 48, 48, b"TC", MAIN_TC, true);
        add(main, 6, 6, b"QUIT TO OS", MAIN_QUIT, true);
        add(main, 0, 0, b"", -1, false);
        add(main, 48, 48, b"LEFT PLAYER (F5)", MAIN_PLAYER1_SETTINGS, true);
        add(main, 48, 48, b"RIGHT PLAYER (F6)", MAIN_PLAYER2_SETTINGS, true);
        add(main, 48, 48, b"MATCH SETUP (F7)", MAIN_SETTINGS, true);

        self.menu(MenuKind::Settings).m.value_offset_x = 100;
        self.menu(MenuKind::Player).m.value_offset_x = 95;
        self.menu(MenuKind::Hidden).m.value_offset_x = 120;
    }

    /// What `kind`'s item `i` does (`getItemBehavior`).
    fn behavior(&mut self, app: &mut App, kind: MenuKind, i: usize) -> Behavior {
        let id = self.menu(kind).items[i].id;
        match kind {
            MenuKind::Main => match id {
                MAIN_REPLAYS => Behavior::ReplaySelect,
                MAIN_TC => Behavior::TcSelect,
                _ => Behavior::None,
            },
            MenuKind::Settings => match id {
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
            MenuKind::Player => {
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
    fn get(&self, app: &App, setting: Setting) -> i32 {
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
    fn set(&mut self, app: &mut App, host: &Host, setting: Setting, v: i32) {
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

    fn set_value(&mut self, kind: MenuKind, i: usize, v: Vec<u8>) {
        self.menu(kind).set_value(i, v);
    }

    fn index_of(&mut self, kind: MenuKind, id: i32) -> Option<usize> {
        self.menu(kind).index_of(id)
    }

    /// The player's profile file, if there is one.
    fn profile_path(&self, app: &App) -> Option<Vec<u8>> {
        self.with_worm(app, |w| w.profile_path())
    }

    /// Shows item `i`'s value (`onUpdate`).
    fn update(&mut self, app: &mut App, kind: MenuKind, i: usize, b: &Behavior) {
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
                self.menu(kind).items[i].visible = has;
            }
            Behavior::ProfileLoaded => {
                let p = self.profile_path(app);
                self.set_value(kind, i, p.as_deref().map(base_name).unwrap_or_default());
                self.menu(kind).items[i].visible = p.is_some();
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
    fn update_level(&mut self, app: &mut App, kind: MenuKind, i: usize) {
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
        self.menu(kind).items[r].name = regen;
    }

    /// `Menu::updateItems`: every item's value, then what the menu shows.
    pub fn update_items(&mut self, app: &mut App, kind: MenuKind) {
        for i in 0..self.menu(kind).len() {
            let b = self.behavior(app, kind, i);
            self.update(app, kind, i, &b);
        }
        if kind == MenuKind::Settings {
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

    fn set_visibility(&mut self, kind: MenuKind, id: i32, state: bool) {
        let i = self.index_of(kind, id).expect("no such item") as i32;
        self.menu(kind).set_visibility(i, state);
    }

    fn selection(&mut self, kind: MenuKind) -> Option<usize> {
        self.menu(kind).selected()
    }

    /// Left or right on the selected item. False to stop repeating it.
    pub fn left_right(&mut self, app: &mut App, host: &Host, kind: MenuKind, dir: i32) -> bool {
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
    fn weapon_item(&self, app: &App) -> ItemBehavior<'static> {
        ItemBehavior::enumeration(1, app.tc_data().model.weapons.len() as i32, &[])
    }

    /// Runs a key on item `it`, which edits `setting`, and writes the
    /// value it gives back if it changed.
    fn change(
        &mut self,
        app: &mut App,
        host: &Host,
        it: &ItemBehavior,
        setting: Setting,
        key: impl FnOnce(&ItemBehavior, i32) -> (i32, ItemAction),
    ) -> ItemAction {
        let v = self.get(app, setting);
        let (new_v, a) = key(it, v);
        if new_v != v {
            self.set(app, host, setting, new_v);
        }
        a
    }

    /// Does what a switch, number or enum asked: show its value, update
    /// the menu or open the number box.
    fn apply(&mut self, app: &mut App, kind: MenuKind, i: usize, b: &Behavior, a: ItemAction) -> Option<BoxedScreen> {
        if a.update_item {
            self.update(app, kind, i, b);
        }
        // The window's switches show in other items too (`setFullscreen`
        // and `setDoubleRes` update the menu).
        let id = self.menu(kind).items[i].id;
        if a.update_menu || (kind == MenuKind::Hidden && a.update_item && (id == HM_DOUBLE_RES || id == HM_FULLSCREEN))
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
        kind: MenuKind,
        then: Then,
        text: &[u8],
        max_len: usize,
        x: i32,
        y: i32,
        digits: bool,
    ) -> BoxedScreen {
        unsafe fn keep(ctx: *mut c_void, text: &[u8]) {
            unsafe {
                *ctx.cast::<Vec<u8>>() = text.to_vec();
            }
        }
        let mut typed = Box::new(text.to_vec());
        let ctx = (&mut *typed as *mut Vec<u8>).cast();
        let screen = unsafe { input_box_screen(text, max_len, x, y, digits, Some(keep), ctx) };
        self.pending = Some(Pending { kind, then, typed });
        screen
    }

    fn open(&mut self, kind: MenuKind, then: Then, screen: BoxedScreen) -> BoxedScreen {
        self.pending = Some(Pending {
            kind,
            then,
            typed: Box::default(),
        });
        screen
    }

    /// Where a text box opens beside item `i`'s value.
    fn value_position(&mut self, kind: MenuKind, i: usize) -> Option<(i32, i32)> {
        let (x, y) = self.menu(kind).item_position(i as i32)?;
        Some((x + self.menu(kind).m.value_offset_x + 2, y))
    }

    /// A file selector.
    fn selector(&mut self, app: &mut App, kind: MenuKind, then: Then, selector: Selector) -> Option<BoxedScreen> {
        let screen = crate::app_files::open_selector(app, selector, self.player);
        Some(self.open(kind, then, screen))
    }

    /// Enter on the selected item (`onEnter`): what it chose, or -1, and
    /// the screen it opened, if any.
    pub fn enter(&mut self, app: &mut App, host: &Host, kind: MenuKind) -> (Option<i32>, Option<BoxedScreen>) {
        let Some(i) = self.selection(kind) else {
            // C++ returned 0 here, the resume item's id.
            return (Some(0), None);
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
                let screen = key_choice_screen(extended);
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
                self.selector(app, kind, Then::SoundAndUpdate, Selector::Profile)
            }
            Behavior::Level => {
                sound();
                self.selector(app, kind, Then::Level { item: i as i32 }, Selector::Level)
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
                self.selector(app, kind, Then::SoundAndUpdate, Selector::Options)
            }
            Behavior::ReplaySelect => {
                sound();
                self.selector(app, kind, Then::Replay, Selector::Replay)
            }
            Behavior::TcSelect => {
                sound();
                self.selector(app, kind, Then::Tc, Selector::Tc)
            }
        };
        (None, opened)
    }

    /// The weapon options screen, on the settings' weapon table.
    fn weapon_options(&mut self, app: &mut App) -> BoxedScreen {
        let tc = app.tc_data();
        let order = tc.weapon_order();
        let weapons: Vec<Str> = order.iter().map(|&w| lstr(tc.weapon_name(w as usize))).collect();
        let states = names(&WEAPON_STATES);
        let spec = WeaponOptionsSpec {
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
        unsafe { weapon_options_screen(&spec) }
    }

    /// The screen the last item opened closed with `result`: what the item
    /// does with it, and what it chose, if anything.
    pub fn then(&mut self, app: &mut App, host: &Host, result: i32) -> Option<i32> {
        let Pending { kind, then, typed } = self.pending.take()?;
        let sound = || host.play_sound(SOUND_CHOICE);
        let ok = result != 0;
        let typed = *typed;
        match then {
            Then::Key { item, index } => {
                let k = result as u32;
                if k != DK_ESCAPE {
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
                        closest_name(order.iter().map(|&w| tc.weapon_name(w as usize)), &typed)
                    };
                    if let Some(closest) = closest {
                        self.set(app, host, Setting::WormWeapon(slot), 1 + closest as i32);
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
                // A cancelled selector closes with -1.
                return Some(result).filter(|&r| r >= 0);
            }
            Then::Tc => {
                if crate::app_files::take_chosen_tc(app) {
                    return Some(MAIN_TC);
                }
            }
        }
        None
    }

    /// Opens the player menu on `player`'s settings (`Gfx::playerSettings`).
    pub fn player_settings(&mut self, app: &mut App, player: i32) {
        self.player = player;
        self.update_items(app, MenuKind::Player);
        self.menu(MenuKind::Player).move_to(0);
        self.cur = MenuKind::Player;
    }

    /// `Gfx::openHiddenMenu`.
    pub fn open_hidden(&mut self, app: &mut App) {
        if self.cur == MenuKind::Hidden {
            return;
        }
        self.cur = MenuKind::Hidden;
        self.update_items(app, MenuKind::Hidden);
        self.menu(MenuKind::Hidden).move_to(0);
    }

    pub fn cur(&self) -> MenuKind {
        self.cur
    }

    pub fn set_cur(&mut self, kind: MenuKind) {
        self.cur = kind;
    }

    /// The menus, empty (`Menu(x, y)` for each).
    pub fn shared() -> SharedMenus {
        Rc::new(RefCell::new(Menus::new()))
    }

    /// Empties the menus, as they were made.
    pub fn reset(&mut self) {
        *self = Menus::new();
    }

    /// Enter on `kind`'s selected item. A screen it opened is dropped: the
    /// menu checks only press items that open none.
    pub fn enter_dropping(&mut self, app: &mut App, host: &Host, kind: MenuKind) -> Option<i32> {
        let (r, opened) = self.enter(app, host, kind);
        if opened.is_some() {
            self.pending = None;
        }
        r
    }

    pub fn box_of(&mut self, kind: MenuKind) -> &mut Menu {
        &mut self.menus[kind as usize]
    }

    /// Draws `kind`, with the player menu's colour bars and the options
    /// menu's palette entry.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, app: &App, kind: MenuKind, scr: &Bitmap, font: &Font, disabled: bool, x: i32, show: bool) {
        struct Ctx<'a> {
            ids: Vec<i32>,
            kind: MenuKind,
            scr: Bitmap<'a>,
            player: i32,
            palette_color: i32,
            settings: crate::settings::SharedSettings,
        }
        unsafe fn overlay(ctx: *mut c_void, item: i32, x: i32, y: i32, selected: bool, _disabled: bool) {
            unsafe {
                let c = &*(ctx as *const Ctx);
                let id = c.ids[item as usize];
                if c.kind == MenuKind::Player && (PL_RED..=PL_BLUE).contains(&id) {
                    let ws = c.settings.borrow().data.worms[c.player as usize].clone();
                    let ws = &ws.borrow().data;
                    draw_rgb_overlay(&c.scr, x, y, selected, ws.rgb[(id - PL_RED) as usize], ws.color);
                } else if c.kind == MenuKind::Hidden && id == HM_PALETTE_SELECT {
                    draw_palette_overlay(&c.scr, x, y, selected, c.palette_color);
                }
            }
        }
        let ctx = Ctx {
            ids: self.menu(kind).items.iter().map(|it| it.id).collect(),
            kind,
            scr: *scr,
            player: self.player,
            palette_color: self.palette_color,
            settings: app.shared_settings(),
        };
        let hooks = MenuHooks {
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
    fn menus() -> (Menus, App) {
        let mut a = App::new();
        a.settings = Some(crate::settings::new_settings());
        a.files.settings_node = crate::fs::Node::new(b"config/LIERO.CFG");
        a.double_res = false;
        a.menu_cycles = 0;
        let mut m = Menus::new();
        m.load();
        (m, a)
    }

    /// The player menu's worm's settings.
    fn worm(a: &App, m: &Menus) -> crate::settings::SharedWormSettings {
        a.settings_mut().data.worms[m.player as usize].clone()
    }

    fn select(m: &mut Menus, kind: MenuKind, id: i32) -> usize {
        let i = m.index_of(kind, id).unwrap();
        m.menu(kind).move_to(i as i32);
        assert_eq!(m.selection(kind), Some(i));
        i
    }

    fn value(m: &mut Menus, kind: MenuKind, i: usize) -> Vec<u8> {
        m.menu(kind).items[i].value.clone()
    }

    #[test]
    fn a_key_only_the_extended_controls_have_leaves_the_original_ones() {
        let (mut m, mut app) = menus();
        select(&mut m, MenuKind::Player, PL_UP + 1);
        let w = worm(&app, &m);
        w.borrow_mut().data.controls[1] = 30;
        w.borrow_mut().data.controls_ex[1] = 30;
        let keys = || {
            let ws = &w.borrow().data;
            (ws.controls[1], ws.controls_ex[1])
        };

        let (r, opened) = m.enter(&mut app, &Host::none(), MenuKind::Player);
        assert_eq!(r, None);
        drop(opened.expect("key choice"));
        assert!(is_extended(177));
        m.then(&mut app, &Host::none(), 177);
        assert_eq!(keys(), (30, 177));

        let (_, opened) = m.enter(&mut app, &Host::none(), MenuKind::Player);
        drop(opened.unwrap());
        m.then(&mut app, &Host::none(), 31);
        assert_eq!(keys(), (31, 31));

        // Escape keeps the key.
        let (_, opened) = m.enter(&mut app, &Host::none(), MenuKind::Player);
        drop(opened.unwrap());
        m.then(&mut app, &Host::none(), DK_ESCAPE as i32);
        assert_eq!(keys(), (31, 31));
    }

    #[test]
    fn dig_takes_any_key_into_the_extended_controls_only() {
        let (mut m, mut app) = menus();
        for player in 0..2 {
            m.player = player;
            select(&mut m, MenuKind::Player, PL_DIG);
            let w = worm(&app, &m);
            let before = w.borrow().data.controls;
            let (_, opened) = m.enter(&mut app, &Host::none(), MenuKind::Player);
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
        let i = select(&mut m, MenuKind::Settings, SI_LIVES);
        for (typed, lives) in [(&b"5000"[..], 999), (b"0", 1), (b"12", 12)] {
            let (_, opened) = m.enter(&mut app, &Host::none(), MenuKind::Settings);
            drop(opened.expect("number box"));
            *m.pending.as_mut().unwrap().typed = typed.to_vec();
            assert_eq!(m.then(&mut app, &Host::none(), 1), None);
            assert_eq!(app.settings_mut().data.lives, lives);
            assert_eq!(value(&mut m, MenuKind::Settings, i), lives.to_string().into_bytes());
        }
        // Escape keeps the number.
        let (_, opened) = m.enter(&mut app, &Host::none(), MenuKind::Settings);
        drop(opened.unwrap());
        *m.pending.as_mut().unwrap().typed = b"7".to_vec();
        m.then(&mut app, &Host::none(), 0);
        assert_eq!(app.settings_mut().data.lives, 12);
    }

    #[test]
    fn held_keys_step_health_every_fourth_frame() {
        let (mut m, mut app) = menus();
        select(&mut m, MenuKind::Player, PL_HEALTH);
        worm(&app, &m).borrow_mut().data.health = 100;
        let start = 100;
        let mut steps = Vec::new();
        for cycle in 1..=12 {
            app.menu_cycles = cycle;
            assert!(m.left_right(&mut app, &Host::none(), MenuKind::Player, -1));
            steps.push(start - worm(&app, &m).borrow().data.health);
        }
        assert_eq!(steps, [0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3]);
    }

    #[test]
    fn double_size_shows_the_whole_options_menu_again() {
        let (mut m, mut app) = menus();
        let frames = m.index_of(MenuKind::Hidden, HM_AI_FRAMES).unwrap();
        select(&mut m, MenuKind::Hidden, HM_DOUBLE_RES);
        app.settings_mut().data.ext.ai_frames = 123;
        m.left_right(&mut app, &Host::none(), MenuKind::Hidden, 1);
        assert!(app.double_res);
        assert_eq!(value(&mut m, MenuKind::Hidden, frames), b"123");
    }
}
