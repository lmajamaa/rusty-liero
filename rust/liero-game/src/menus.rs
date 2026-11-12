//! The menus beside the main menu screen (C++'s `MainMenu`, `SettingsMenu`,
//! `PlayerMenu` and `HiddenMenu`): their items, which setting each one
//! changes, how it shows its value, and the screens an item opens (a key
//! choice, a text box, the weapon options or a file selector). An item that
//! opens a screen goes on when that screen closes (`LieroMenus::then`).
//!
//! What stays C++ (`LieroMenusHooks`): the settings and the TC, the file
//! selectors, which load levels, profiles, setups, replays and TCs, saving a
//! setup or profile, and the window's size.

use crate::boxes::{liero_input_box_screen, liero_key_choice_screen};
use crate::gfx::LieroBitmap;
use crate::item::{LieroItemAction, LieroItemBehavior, LieroItemKind};
use crate::keys::{is_extended, key_name, LIERO_DK_ESCAPE};
use crate::menu::{
    liero_menu_closest_name, liero_menu_draw_palette_overlay, liero_menu_draw_rgb_overlay, LieroMenuBox,
    LieroMenuHooks, LieroMenuItemSpec, OwnedMenu, LIERO_MENU_VALUE_FRAMES, LIERO_MENU_VALUE_PERCENTAGE,
    LIERO_MENU_VALUE_TIME,
};
use crate::menu_screen::{LIERO_MENU_HIDDEN, LIERO_MENU_MAIN, LIERO_MENU_PLAYER, LIERO_MENU_SETTINGS};
use crate::screen::{current_host, LieroScreen};
use crate::settings::{LieroSettingsData, LieroWormSettingsBox, LieroWormSettingsData};
use crate::tc::{c, s, LieroTcData};
use crate::text::{LieroFont, LieroStr};
use crate::weapon_options::{liero_weapon_options_screen, LieroWeaponOptionsSpec};
use std::ffi::c_void;

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

/// What the menus call in C++. The settings, TC and files are the app's.
#[derive(Clone, Copy)]
pub struct LieroMenusHooks {
    /// Frames the menu has shown, for how fast held keys step a number.
    pub menu_cycles: unsafe fn() -> u32,
    /// The window's double size, and setting it or full screen. These don't
    /// update the options menu, which does it itself.
    pub double_res: unsafe fn() -> *mut bool,
    pub set_double_res: unsafe fn(on: bool),
    pub set_fullscreen: unsafe fn(on: bool),
}

/// What an item does (`ItemBehavior` and its kinds).
enum Behavior {
    None,
    /// A switch, number or enum (`RustItemBehavior`), with its names.
    Item(LieroItemBehavior, Vec<LieroStr>),
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
    /// A player's weapon, as an enum shown by name.
    Weapon {
        slot: usize,
        item: LieroItemBehavior,
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
    h: LieroMenusHooks,
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

fn integer(value: *mut i32, min: i32, max: i32, step: i32, percentage: bool) -> LieroItemBehavior {
    let mut b = LieroItemBehavior::integer(value, min, max);
    b.step = step;
    if percentage {
        b.value_kind = LIERO_MENU_VALUE_PERCENTAGE;
    }
    b
}

/// Seconds, or frames shown as seconds; can't be typed in (`TimeBehavior`).
fn time(value: *mut i32, min: i32, max: i32, step: i32, frames: bool) -> LieroItemBehavior {
    let mut b = integer(value, min, max, step, false);
    b.value_kind = if frames {
        LIERO_MENU_VALUE_FRAMES
    } else {
        LIERO_MENU_VALUE_TIME
    };
    b.allow_entry = false;
    b
}

/// One of `min` to `max`, wrapping round (`EnumBehavior`).
fn enumeration(value: *mut u32, min: u32, max: u32) -> LieroItemBehavior {
    let mut b = LieroItemBehavior::integer(value.cast(), min as i32, max as i32);
    b.kind = LieroItemKind::Enum;
    b
}

fn switch(flag: *mut bool, set_flag: Option<unsafe fn(bool)>) -> Behavior {
    let mut b = LieroItemBehavior::integer(std::ptr::null_mut(), 0, 0);
    b.kind = LieroItemKind::Switch;
    b.flag = flag;
    b.set_flag = set_flag;
    Behavior::Item(b, names(&ON_OFF))
}

/// An enum shown with `n`'s names (`ArrayEnumBehavior`).
fn array_enum(value: *mut u32, n: &[&'static [u8]]) -> Behavior {
    Behavior::Item(enumeration(value, 0, n.len() as u32 - 1), names(n))
}

fn item(b: LieroItemBehavior) -> Behavior {
    Behavior::Item(b, Vec::new())
}

/// `b` with its names pointing into `n`.
fn with_names(b: &LieroItemBehavior, n: &[LieroStr]) -> LieroItemBehavior {
    let mut v = *b;
    v.names = n.as_ptr();
    v.name_count = n.len();
    v
}

impl LieroMenus {
    fn new(h: LieroMenusHooks) -> Self {
        let raw = |x, y| Box::new(LieroMenuBox(OwnedMenu::raw(x, y, false)));
        LieroMenus {
            h,
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

    // The app's settings and TC outlive the call they're used in, and
    // nothing else holds them meanwhile.
    fn settings<'a>(&self) -> &'a mut LieroSettingsData {
        &mut crate::app::app().menu_settings().data
    }

    fn worm<'a>(&self) -> &'a mut LieroWormSettingsBox {
        worm_settings(self.player)
    }

    fn worm_data<'a>(&self) -> &'a mut LieroWormSettingsData {
        &mut self.worm().data
    }

    fn tc<'a>(&self) -> &'a LieroTcData {
        crate::app::app().tc_data()
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
    fn behavior(&mut self, kind: i32, i: usize) -> Behavior {
        let id = self.menu(kind).ids[i];
        match kind {
            LIERO_MENU_MAIN => match id {
                MA_REPLAYS => Behavior::ReplaySelect,
                MA_TC => Behavior::TcSelect,
                _ => Behavior::None,
            },
            LIERO_MENU_SETTINGS => {
                let st = self.settings();
                match id {
                    SI_NAMES_ON_BONUSES => switch(&mut st.names_on_bonuses, None),
                    SI_MAP => switch(&mut st.map, None),
                    SI_REGENERATE_LEVEL => switch(&mut st.regenerate_level, None),
                    SI_LOADING_TIMES => item(integer(&mut st.loading_time, 0, 9999, 1, true)),
                    SI_MAX_BONUSES => item(integer(&mut st.max_bonuses, 0, 99, 1, false)),
                    SI_AMOUNT_OF_BLOOD => {
                        let tc = self.tc();
                        let mut b = integer(
                            &mut st.blood,
                            0,
                            tc.constant(c("BloodLimit")),
                            tc.constant(c("BloodStepUp")),
                            true,
                        );
                        b.allow_entry = false;
                        item(b)
                    }
                    SI_LIVES => item(integer(&mut st.lives, 1, 999, 1, false)),
                    SI_TIME_TO_LOSE | SI_TIME_TO_WIN => item(time(&mut st.time_to_lose, 60, 3600, 10, false)),
                    SI_ZONE_TIMEOUT => item(time(&mut st.zone_timeout, 10, 3600, 10, false)),
                    SI_FLAGS_TO_WIN => item(integer(&mut st.flags_to_win, 1, 999, 1, false)),
                    SI_LEVEL => Behavior::Level,
                    SI_GAME_MODE => array_enum(&mut st.game_mode, &GAME_MODES),
                    SI_WEAPON_OPTIONS => Behavior::WeaponOptions,
                    SI_LOAD_OPTIONS => Behavior::OptionsSelect,
                    SI_SAVE_OPTIONS => Behavior::OptionsSave,
                    SI_LOAD_CHANGE => switch(&mut st.load_change, None),
                    _ => Behavior::None,
                }
            }
            LIERO_MENU_PLAYER => {
                let ws = self.worm_data();
                if (PL_WEAP0..PL_WEAP0 + 5).contains(&id) {
                    let slot = (id - PL_WEAP0) as usize;
                    let count = self.tc().weapon_count as u32;
                    return Behavior::Weapon {
                        slot,
                        item: enumeration(&mut ws.weapons[slot], 1, count),
                    };
                }
                match id {
                    PL_NAME => Behavior::WormName,
                    PL_HEALTH => {
                        let mut b = integer(&mut ws.health, 1, 10000, 1, true);
                        b.scroll_interval = 4;
                        item(b)
                    }
                    PL_RED..=PL_BLUE => {
                        let mut b = integer(&mut ws.rgb[(id - PL_RED) as usize], 0, 63, 1, false);
                        b.scroll_interval = 4;
                        item(b)
                    }
                    // The extended controls are always on (`Settings::extensions`).
                    PL_UP..=PL_DIG => Behavior::Key {
                        index: (id - PL_UP) as usize,
                        extended: true,
                    },
                    PL_CONTROLLER => array_enum(&mut ws.controller, &CONTROLLERS),
                    PL_SAVE_PROFILE => Behavior::ProfileSave { save_as: false },
                    PL_SAVE_PROFILE_AS => Behavior::ProfileSave { save_as: true },
                    PL_LOAD_PROFILE => Behavior::ProfileLoad,
                    PL_LOADED_PROFILE => Behavior::ProfileLoaded,
                    _ => Behavior::None,
                }
            }
            _ => {
                let st = self.settings();
                match id {
                    HM_RECORD_REPLAYS => switch(&mut st.record_replays, None),
                    HM_LOAD_POWER_LEVELS => switch(&mut st.load_powerlevel_palette, None),
                    HM_DOUBLE_RES => switch(unsafe { (self.h.double_res)() }, Some(self.h.set_double_res)),
                    HM_FULLSCREEN => switch(&mut st.fullscreen, Some(self.h.set_fullscreen)),
                    HM_AI_FRAMES => item(integer(&mut st.ai_frames, 1, 70 * 5, 1, false)),
                    HM_AI_MUTATIONS => item(integer(&mut st.ai_mutations, 1, 20, 1, false)),
                    HM_PALETTE_SELECT => item(integer(&mut self.palette_color, 0, 255, 1, false)),
                    HM_SHADOWS => switch(&mut st.shadow, None),
                    HM_SCREEN_SYNC => switch(&mut st.screen_sync, None),
                    HM_SELECT_BOT_WEAPONS => array_enum(&mut st.select_bot_weapons, &BOT_WEAPONS),
                    HM_AI_TRACES => switch(&mut st.ai_traces, None),
                    HM_AI_PARALLELS => item(integer(&mut st.ai_parallels, 1, 16, 1, false)),
                    HM_ALLOW_VIEWING_SPAWN_POINT => switch(&mut st.allow_viewing_spawn_point, None),
                    HM_SINGLE_SCREEN_REPLAY => switch(&mut st.single_screen_replay, None),
                    _ => Behavior::None,
                }
            }
        }
    }

    fn set_value(&mut self, kind: i32, i: usize, v: Vec<u8>) {
        self.menu(kind).set_value(i, v);
    }

    fn index_of(&mut self, kind: i32, id: i32) -> Option<usize> {
        self.menu(kind).ids.iter().position(|&x| x == id)
    }

    /// The player's profile file, if there is one.
    fn profile_path(&self) -> Option<Vec<u8>> {
        self.worm().profile_path()
    }

    /// Shows item `i`'s value (`onUpdate`).
    fn update(&mut self, kind: i32, i: usize, b: &Behavior) {
        match b {
            Behavior::Item(it, n) => {
                let v = unsafe { with_names(it, n).value_text() };
                self.set_value(kind, i, v);
            }
            Behavior::Key { index, extended } => {
                let ws = self.worm_data();
                let k = if *extended {
                    ws.controls_ex[*index]
                } else {
                    ws.controls[*index]
                };
                self.set_value(kind, i, key_name(k));
            }
            Behavior::WormName => {
                let name = self.worm().name.clone();
                self.set_value(kind, i, name);
            }
            Behavior::ProfileSave { save_as: false } => {
                let has = self.profile_path().is_some();
                self.menu(kind).visible[i] = has;
            }
            Behavior::ProfileLoaded => {
                let p = self.profile_path();
                self.set_value(kind, i, p.as_deref().map(base_name).unwrap_or_default());
                self.menu(kind).visible[i] = p.is_some();
            }
            Behavior::Weapon { item, .. } => {
                let v = unsafe { *item.value } as usize;
                let tc = self.tc();
                let name = tc.weapon_name(tc.weapon_order()[v - 1] as usize).to_vec();
                self.set_value(kind, i, name);
            }
            Behavior::Level => self.update_level(kind, i),
            Behavior::OptionsSave => {
                let name = base_name(&crate::app_files::settings_path());
                self.set_value(kind, i, name);
            }
            _ => {}
        }
    }

    /// The level item's value, and the regenerate item's name to go with it.
    fn update_level(&mut self, kind: i32, i: usize) {
        let st = self.settings();
        let tc = self.tc();
        let (value, regen) = if !st.random_level {
            let file = &crate::app::app().menu_settings().level_file;
            let mut v = b"\"".to_vec();
            v.extend(base_name(file));
            v.push(b'"');
            (v, tc.text(s("ReloadLevel")).to_vec())
        } else {
            (tc.text(s("Random2")).to_vec(), tc.text(s("RegenLevel")).to_vec())
        };
        self.set_value(kind, i, value);
        let r = self.index_of(kind, SI_REGENERATE_LEVEL).expect("no regenerate item");
        self.menu(kind).names[r] = regen;
    }

    /// `Menu::updateItems`: every item's value, then what the menu shows.
    pub fn update_items(&mut self, kind: i32) {
        for i in 0..self.menu(kind).len() {
            let b = self.behavior(kind, i);
            self.update(kind, i, &b);
        }
        if kind == LIERO_MENU_SETTINGS {
            // `SettingsMenu::onUpdate`: the game mode's own items.
            let mode = self.settings().game_mode;
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
    pub fn left_right(&mut self, kind: i32, dir: i32) -> bool {
        let Some(i) = self.selection(kind) else { return false };
        let b = self.behavior(kind, i);
        let it = match &b {
            Behavior::Item(it, n) => with_names(it, n),
            Behavior::Weapon { item, .. } => *item,
            _ => return true,
        };
        let a = unsafe { it.left_right(dir, (self.h.menu_cycles)()) };
        // Left and right never open the number box.
        let _ = self.apply(kind, i, &b, a);
        a.keep_going
    }

    /// Does what a switch, number or enum asked: show its value, update
    /// the menu or open the number box.
    fn apply(&mut self, kind: i32, i: usize, b: &Behavior, a: LieroItemAction) -> Option<LieroScreen> {
        if a.update_item {
            self.update(kind, i, b);
        }
        // The window's switches show in other items too (`setFullscreen`
        // and `setDoubleRes` update the menu).
        let id = self.menu(kind).ids[i];
        if a.update_menu || (kind == LIERO_MENU_HIDDEN && a.update_item && (id == HM_DOUBLE_RES || id == HM_FULLSCREEN))
        {
            self.update_items(kind);
        }
        if a.enter_digits > 0 {
            let (x, y) = self.menu(kind).item_position(i as i32)?;
            let x = x + self.menu(kind).m.value_offset_x;
            let Behavior::Item(it, _) = b else { return None };
            let text = unsafe { *it.value }.to_string().into_bytes();
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
    ) -> LieroScreen {
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

    fn open(&mut self, kind: i32, then: Then, screen: LieroScreen) -> LieroScreen {
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
    fn selector(&mut self, kind: i32, then: Then, selector: i32) -> Option<LieroScreen> {
        let screen = crate::app_files::open_selector(selector, self.player);
        Some(self.open(kind, then, screen))
    }

    /// Enter on the selected item (`onEnter`): what it chose, or -1, and
    /// the screen it opened, if any.
    pub fn enter(&mut self, kind: i32) -> (i32, Option<LieroScreen>) {
        let Some(i) = self.selection(kind) else {
            return (0, None);
        };
        let b = self.behavior(kind, i);
        let host = current_host();
        let sound = || host.play_sound(SOUND_CHOICE);
        let opened = match b {
            Behavior::None | Behavior::ProfileLoaded => None,
            Behavior::Item(it, ref n) => {
                let a = unsafe { with_names(&it, n).enter() };
                let b = Behavior::Item(it, n.clone());
                self.apply(kind, i, &b, a)
            }
            Behavior::Key { index, extended } => {
                sound();
                let screen = liero_key_choice_screen(extended);
                Some(self.open(kind, Then::Key { item: i as i32, index }, screen))
            }
            Behavior::WormName => {
                sound();
                self.worm_data().random_name = false;
                match self.value_position(kind, i) {
                    None => None,
                    Some((x, y)) => {
                        let name = self.worm().name.clone();
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
                        if self.worm().save_to_profile() {
                            self.update_items(kind);
                        }
                        None
                    }
                }
            }
            Behavior::Weapon { slot, .. } => {
                sound();
                self.value_position(kind, i)
                    .map(|(x, y)| self.input(kind, Then::WeaponSearch { slot }, b"", 10, x, y, false))
            }
            Behavior::ProfileLoad => {
                sound();
                self.selector(kind, Then::SoundAndUpdate, LIERO_SELECTOR_PROFILE)
            }
            Behavior::Level => {
                sound();
                self.selector(kind, Then::Level { item: i as i32 }, LIERO_SELECTOR_LEVEL)
            }
            Behavior::WeaponOptions => {
                sound();
                let screen = self.weapon_options();
                Some(self.open(kind, Then::Sound, screen))
            }
            Behavior::OptionsSave => {
                sound();
                match self.value_position(kind, i) {
                    None => None,
                    Some((x, y)) => {
                        let name = base_name(&crate::app_files::settings_path());
                        Some(self.input(kind, Then::OptionsSave { item: i as i32 }, &name, 30, x, y, false))
                    }
                }
            }
            Behavior::OptionsSelect => {
                sound();
                self.selector(kind, Then::SoundAndUpdate, LIERO_SELECTOR_OPTIONS)
            }
            Behavior::ReplaySelect => {
                sound();
                self.selector(kind, Then::Replay, LIERO_SELECTOR_REPLAY)
            }
            Behavior::TcSelect => {
                sound();
                self.selector(kind, Then::Tc, LIERO_SELECTOR_TC)
            }
        };
        (-1, opened)
    }

    /// The weapon options screen, on the settings' weapon table.
    fn weapon_options(&mut self) -> LieroScreen {
        let tc = self.tc();
        let order = tc.weapon_order();
        let weapons: Vec<LieroStr> = order.iter().map(|&w| lstr(tc.weapon_name(w as usize))).collect();
        let states = names(&WEAPON_STATES);
        let spec = LieroWeaponOptionsSpec {
            names: weapons.as_ptr(),
            order: order.as_ptr(),
            count: weapons.len(),
            weap_table: self.settings().weap_table.as_mut_ptr(),
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
    pub fn then(&mut self, result: i32) -> i32 {
        let Some(Pending { kind, then, typed }) = self.pending.take() else {
            return -1;
        };
        let host = current_host();
        let sound = || host.play_sound(SOUND_CHOICE);
        let ok = result != 0;
        let typed = *typed;
        match then {
            Then::Key { item, index } => {
                let k = result as u32;
                if k != LIERO_DK_ESCAPE {
                    let ws = self.worm_data();
                    // Dig has only an extended control (`PlDig`'s
                    // `KeyBehavior` binds `controlsEx` twice).
                    if !is_extended(k) && index < ws.controls.len() {
                        ws.controls[index] = k;
                    }
                    ws.controls_ex[index] = k;
                    let b = self.behavior(kind, item as usize);
                    self.update(kind, item as usize, &b);
                }
                host.clear_keys();
            }
            Then::WormName { item } => {
                if ok {
                    self.worm().name = typed;
                }
                // An empty name stays empty: `Settings::generateName` does
                // nothing.
                sound();
                let b = self.behavior(kind, item as usize);
                self.update(kind, item as usize, &b);
            }
            Then::ProfileSaveAs => {
                if ok && !typed.is_empty() {
                    crate::app_files::save_profile_as(self.player, &typed);
                }
                sound();
                self.update_items(kind);
            }
            Then::WeaponSearch { slot } => {
                if ok {
                    let tc = self.tc();
                    let order = tc.weapon_order();
                    let names: Vec<LieroStr> = order.iter().map(|&w| lstr(tc.weapon_name(w as usize))).collect();
                    let closest = unsafe {
                        liero_menu_closest_name(names.as_ptr(), names.len() as i32, typed.as_ptr(), typed.len())
                    };
                    if closest >= 0 {
                        self.worm_data().weapons[slot] = 1 + closest as u32;
                    }
                    self.update_items(kind);
                }
            }
            Then::Integer { item } => {
                let b = self.behavior(kind, item as usize);
                if let Behavior::Item(it, _) = &b {
                    if ok && !typed.is_empty() {
                        let v = atoi(&typed).clamp(it.min, it.max);
                        unsafe { *it.value = v };
                    }
                }
                self.update(kind, item as usize, &b);
            }
            Then::OptionsSave { item } => {
                if ok && !typed.is_empty() {
                    crate::app_files::save_options(&typed);
                }
                sound();
                let b = self.behavior(kind, item as usize);
                self.update(kind, item as usize, &b);
            }
            Then::Level { item } => {
                sound();
                self.update_level(kind, item as usize);
            }
            Then::Sound => sound(),
            Then::SoundAndUpdate => {
                sound();
                self.update_items(kind);
            }
            Then::Replay => {
                sound();
                return result;
            }
            Then::Tc => {
                if crate::app_files::take_chosen_tc() {
                    return MA_TC;
                }
            }
        }
        -1
    }

    /// Opens the player menu on `player`'s settings (`Gfx::playerSettings`).
    pub fn player_settings(&mut self, player: i32) {
        self.player = player;
        self.update_items(LIERO_MENU_PLAYER);
        self.menu(LIERO_MENU_PLAYER).move_to(0);
        self.cur = LIERO_MENU_PLAYER;
    }

    /// `Gfx::openHiddenMenu`.
    pub fn open_hidden(&mut self) {
        if self.cur == LIERO_MENU_HIDDEN {
            return;
        }
        self.cur = LIERO_MENU_HIDDEN;
        self.update_items(LIERO_MENU_HIDDEN);
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
    fn draw(&mut self, kind: i32, scr: &LieroBitmap, font: &LieroFont, disabled: bool, x: i32, show: bool) {
        struct Ctx<'a> {
            ids: Vec<i32>,
            kind: i32,
            scr: LieroBitmap<'a>,
            player: i32,
            palette_color: i32,
        }
        unsafe fn overlay(ctx: *mut c_void, item: i32, x: i32, y: i32, selected: bool, _disabled: bool) {
            unsafe {
                let c = &*(ctx as *const Ctx);
                let id = c.ids[item as usize];
                if c.kind == LIERO_MENU_PLAYER && (PL_RED..=PL_BLUE).contains(&id) {
                    let ws = &worm_settings(c.player).data;
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
        };
        let hooks = LieroMenuHooks {
            ctx: &ctx as *const Ctx as *mut c_void,
            draw_overlay: overlay,
        };
        self.menu(kind).draw_with(scr, font, &hooks, disabled, x, show);
    }
}

/// `player`'s settings in the app's.
fn worm_settings<'a>(player: i32) -> &'a mut LieroWormSettingsBox {
    // SAFETY: the app's settings hold a reference to them.
    unsafe { &mut *crate::app::app().menu_settings().worms[player as usize].as_ptr() }
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
pub fn liero_menus_new(hooks: LieroMenusHooks) -> *mut LieroMenus {
    Box::into_raw(Box::new(LieroMenus::new(hooks)))
}

/// Empties the menus, as they were made, keeping their addresses.
pub fn liero_menus_reset(m: &mut LieroMenus) {
    let fresh = LieroMenus::new(m.h);
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

pub fn liero_menus_player_settings(m: &mut LieroMenus, player: i32) {
    m.player_settings(player);
}

pub fn liero_menus_update_items(m: &mut LieroMenus, kind: i32) {
    m.update_items(kind);
}

pub fn liero_menus_left_right(m: &mut LieroMenus, kind: i32, dir: i32) -> bool {
    m.left_right(kind, dir)
}

/// Enter on `kind`'s selected item. A screen it opened is dropped: the menu
/// checks only press items that open none.
///
/// # Safety
/// The menus' hooks, which the item may call, are valid.
pub unsafe fn liero_menus_enter(m: &mut LieroMenus, kind: i32) -> i32 {
    unsafe {
        let (r, opened) = m.enter(kind);
        if let Some(s) = opened {
            s.release();
            m.pending = None;
        }
        r
    }
}

pub fn liero_menus_draw(
    m: &mut LieroMenus,
    kind: i32,
    scr: &LieroBitmap,
    font: &LieroFont,
    disabled: bool,
    x: i32,
    show_disabled_selection: bool,
) {
    m.draw(kind, scr, font, disabled, x, show_disabled_selection);
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

    use std::cell::Cell;

    thread_local! {
        static CYCLES: Cell<u32> = const { Cell::new(0) };
        static DOUBLE_RES: Cell<*mut bool> = const { Cell::new(std::ptr::null_mut()) };
    }

    unsafe fn menu_cycles() -> u32 {
        CYCLES.get()
    }
    unsafe fn double_res() -> *mut bool {
        DOUBLE_RES.get()
    }
    unsafe fn set_double_res(on: bool) {
        unsafe {
            *DOUBLE_RES.get() = on;
        }
    }
    unsafe fn set_fullscreen(_: bool) {}

    /// The menus with their items, on the app's new settings.
    fn menus() -> LieroMenus {
        let a = crate::app::app();
        // SAFETY: the new box's reference is the app's.
        a.settings = Some(unsafe { crate::shared::Shared::adopt(crate::settings::liero_settings_box_new()) });
        a.files.settings_node = crate::fs::Node::new(b"config/LIERO.CFG");
        DOUBLE_RES.set(Box::leak(Box::new(false)));
        CYCLES.set(0);
        let mut m = LieroMenus::new(LieroMenusHooks {
            menu_cycles,
            double_res,
            set_double_res,
            set_fullscreen,
        });
        m.load();
        m
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
        let mut m = menus();
        select(&mut m, LIERO_MENU_PLAYER, PL_UP + 1);
        let ws = m.worm_data();
        ws.controls[1] = 30;
        ws.controls_ex[1] = 30;

        let (r, opened) = m.enter(LIERO_MENU_PLAYER);
        assert_eq!(r, -1);
        unsafe { opened.expect("key choice").release() };
        assert!(is_extended(177));
        m.then(177);
        assert_eq!((ws.controls[1], ws.controls_ex[1]), (30, 177));

        let (_, opened) = m.enter(LIERO_MENU_PLAYER);
        unsafe { opened.unwrap().release() };
        m.then(31);
        assert_eq!((ws.controls[1], ws.controls_ex[1]), (31, 31));

        // Escape keeps the key.
        let (_, opened) = m.enter(LIERO_MENU_PLAYER);
        unsafe { opened.unwrap().release() };
        m.then(LIERO_DK_ESCAPE as i32);
        assert_eq!((ws.controls[1], ws.controls_ex[1]), (31, 31));
    }

    #[test]
    fn dig_takes_any_key_into_the_extended_controls_only() {
        let mut m = menus();
        for player in 0..2 {
            m.player = player;
            select(&mut m, LIERO_MENU_PLAYER, PL_DIG);
            let ws = m.worm_data();
            let before = ws.controls;
            let (_, opened) = m.enter(LIERO_MENU_PLAYER);
            unsafe { opened.expect("key choice").release() };
            // C, which has a DOS key.
            assert!(!is_extended(46));
            m.then(46);
            assert_eq!((ws.controls, ws.controls_ex[7]), (before, 46));
        }
    }

    #[test]
    fn a_typed_number_is_kept_in_range() {
        let mut m = menus();
        let i = select(&mut m, LIERO_MENU_SETTINGS, SI_LIVES);
        for (typed, lives) in [(&b"5000"[..], 999), (b"0", 1), (b"12", 12)] {
            let (_, opened) = m.enter(LIERO_MENU_SETTINGS);
            unsafe { opened.expect("number box").release() };
            *m.pending.as_mut().unwrap().typed = typed.to_vec();
            assert_eq!(m.then(1), -1);
            assert_eq!(m.settings().lives, lives);
            assert_eq!(value(&mut m, LIERO_MENU_SETTINGS, i), lives.to_string().into_bytes());
        }
        // Escape keeps the number.
        let (_, opened) = m.enter(LIERO_MENU_SETTINGS);
        unsafe { opened.unwrap().release() };
        *m.pending.as_mut().unwrap().typed = b"7".to_vec();
        m.then(0);
        assert_eq!(m.settings().lives, 12);
    }

    #[test]
    fn held_keys_step_health_every_fourth_frame() {
        let mut m = menus();
        select(&mut m, LIERO_MENU_PLAYER, PL_HEALTH);
        m.worm_data().health = 100;
        let start = 100;
        let mut steps = Vec::new();
        for cycle in 1..=12 {
            CYCLES.set(cycle);
            assert!(m.left_right(LIERO_MENU_PLAYER, -1));
            steps.push(start - m.worm_data().health);
        }
        assert_eq!(steps, [0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3]);
    }

    #[test]
    fn double_size_shows_the_whole_options_menu_again() {
        let mut m = menus();
        let frames = m.index_of(LIERO_MENU_HIDDEN, HM_AI_FRAMES).unwrap();
        select(&mut m, LIERO_MENU_HIDDEN, HM_DOUBLE_RES);
        m.settings().ai_frames = 123;
        m.left_right(LIERO_MENU_HIDDEN, 1);
        assert!(unsafe { *DOUBLE_RES.get() });
        assert_eq!(value(&mut m, LIERO_MENU_HIDDEN, frames), b"123");
    }
}
