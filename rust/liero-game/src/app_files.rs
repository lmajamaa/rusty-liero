//! The app's files (`Gfx`'s settings and file selectors in C++): the config
//! and TC folders, loading and saving the settings and profiles, what the
//! file selectors list and what choosing a file does, and switching TC.
//!
//! Errors C++ threw and never caught panic here, as they ended the app
//! there: a profile, replay or TC that can't be read, liero.cfg missing a
//! setting, and a setup or profile that can't be written.

use crate::app::{app, LieroApp};
use crate::fs::{join_path, LieroFsNode, Node};
use crate::level::{liero_level_draw_miniature, LieroLevelBox};
use crate::menus::{
    LIERO_MAIN_REPLAY, LIERO_SELECTOR_LEVEL, LIERO_SELECTOR_OPTIONS, LIERO_SELECTOR_PROFILE, LIERO_SELECTOR_REPLAY,
};
use crate::screen::LieroScreen;
use crate::selector::{selector_screen, FileSource, Spec};
use crate::settings::{
    liero_worm_settings_box_load_profile, liero_worm_settings_box_save_profile, LieroSettingsBox, LieroSettingsStatus,
    WormSettingsRef,
};
use crate::shared::Shared;
use crate::tc::{s as text, LieroTc};
use crate::text::LieroStr;

/// The app's folders and files.
#[derive(Default)]
pub struct Files {
    /// The config folder (`gfx.configNode`), with the settings, profiles,
    /// replays and levels.
    pub config: Node,
    /// The folder of TCs (`gfx.tcNode`).
    pub tc_root: Node,
    /// The settings file loaded or saved last (`gfx.settingsNode`), whose
    /// name the menu shows.
    pub settings_node: Node,
    /// liero.cfg as `save_settings_if_changed` last read or wrote it.
    pub saved_text: Vec<u8>,
    /// The replay chosen last, where the replay selector starts.
    pub prev_replay: Vec<u8>,
    /// The TC the TC selector loaded, until the menu switches to it.
    pub chosen_tc: Option<Shared<LieroTc>>,
}

fn path(n: &Node) -> String {
    String::from_utf8_lossy(n.full_path()).into_owned()
}

impl LieroApp {
    /// New settings with `node` read over them (`Gfx::loadSettings`, or
    /// with `legacy`, `loadSettingsLegacy`). False if it can't be read or
    /// didn't read whole.
    pub fn load_settings(&mut self, node: Option<&Node>, legacy: bool) -> bool {
        if !legacy {
            self.files.settings_node = node.cloned().unwrap_or_default();
        }
        let settings = LieroSettingsBox::defaults();
        // SAFETY: nothing else has the new box yet.
        let status = node.and_then(|n| unsafe { (*settings.as_ptr()).load(n, legacy) });
        self.settings = Some(settings);
        match status {
            Some(LieroSettingsStatus::Ok) => true,
            // What std::map::at threw in the C++ reader. Nothing caught it.
            Some(LieroSettingsStatus::MissingKey) => panic!("Missing setting in liero.cfg"),
            _ => false,
        }
    }

    /// Writes the settings to `node` as liero.cfg and remembers it
    /// (`Gfx::saveSettings`). False if it can't be written.
    pub fn save_settings(&mut self, node: &Node) -> bool {
        self.files.settings_node = node.clone();
        node.write(&self.menu_settings().text())
    }

    /// Takes the settings as liero.cfg has them.
    pub fn mark_settings_saved(&mut self) {
        self.files.saved_text = self.menu_settings().text();
    }

    /// Writes liero.cfg in the config folder if the settings changed since
    /// it was last read or written here (`Gfx::saveSettingsIfChanged`). The
    /// browser's menu calls it, since a closed tab never reaches the save at
    /// the end.
    pub fn save_settings_if_changed(&mut self) {
        let text = self.menu_settings().text();
        if text == self.files.saved_text {
            return;
        }
        self.files.saved_text = text;
        // Not saving is no reason to stop the game.
        let _ = self.files.config.go(b"liero.cfg").write(&self.files.saved_text);
    }
}

/// What choosing a file does.
enum Choice {
    Level,
    /// Loads a player's profile.
    Profile(WormSettingsRef),
    Replay,
    Options,
    Tc,
}

/// A selector's folder in the file system, and its choice.
struct AppFiles {
    root: Node,
    choice: Choice,
}

impl AppFiles {
    /// The node at `rel` below the root.
    fn node(&self, rel: &[u8]) -> Node {
        if rel.is_empty() {
            return self.root.clone();
        }
        rel.split(|&c| c == b'/').fold(self.root.clone(), |n, part| n.go(part))
    }
}

/// Shows the level in `node` in miniature on the menu and the spectator
/// screen, if it loads.
fn preview_level(node: &Node) {
    let a = app();
    // A file that can't be read isn't shown.
    let Some(bytes) = node.read() else { return };
    let tc = a.tc_data();
    let mut level = LieroLevelBox::new();
    let powerlevel = a.menu_settings().data.load_powerlevel_palette;
    // SAFETY: the TC's data lives as long as it does.
    if !unsafe { level.load(&tc.level_common(), &tc.palette_colors, powerlevel, &bytes) } {
        return;
    }
    let l = level.data().level;
    let center_x = a.single.data.render_res_x / 2;
    let (play, spectator) = (a.frozen.view(), a.frozen_spectator.view());
    // SAFETY: the miniatures fit the frozen screens.
    unsafe {
        liero_level_draw_miniature(&l, play.pixels, play.pitch, 134, 162, 10);
        liero_level_draw_miniature(
            &l,
            spectator.pixels,
            spectator.pitch,
            center_x - 126,
            a.single.data.render_res_y - 208,
            2,
        );
    }
}

impl FileSource for AppFiles {
    fn list(&mut self, rel: &[u8]) -> Vec<(Vec<u8>, bool)> {
        self.node(rel).list().into_iter().map(|e| (e.name, e.is_dir)).collect()
    }

    fn exists(&mut self, rel: &[u8]) -> bool {
        self.node(rel).exists()
    }

    fn preview(&mut self, rel: &[u8]) {
        if matches!(self.choice, Choice::Level) {
            preview_level(&self.node(rel));
        }
    }

    fn choose(&mut self, rel: &[u8], full_path: &[u8], name: &[u8], random: bool) -> i32 {
        let a = app();
        let node = self.node(rel);
        match &mut self.choice {
            Choice::Level => {
                let s = a.menu_settings();
                s.data.random_level = random;
                s.level_file = if random { Vec::new() } else { full_path.to_vec() };
            }
            Choice::Profile(ws) => {
                let mut status = LieroSettingsStatus::Ok;
                if !liero_worm_settings_box_load_profile(ws, Some(&LieroFsNode(node.clone())), &mut status) {
                    panic!("Could not read {}", path(&node));
                }
                // A profile that ends early, or fails its checkpoint, threw
                // with the profile's colour, and nothing caught it.
                match status {
                    LieroSettingsStatus::ReadError => panic!("Read error in get()"),
                    LieroSettingsStatus::CheckError => panic!("Expected checkpoint here"),
                    _ => {}
                }
            }
            Choice::Replay => {
                a.files.prev_replay = full_path.to_vec();
                // Reset controller before opening the replay, since we may
                // be recording it
                a.controller = None;
                // SAFETY: the app's TC has a reference.
                let c =
                    unsafe { crate::controllers::liero_replay_controller_new(&LieroFsNode(node.clone()), a.menu_tc()) };
                if c.is_null() {
                    panic!("Could not read {}", path(&node));
                }
                // SAFETY: the controller is new.
                a.controller = Some(unsafe { Box::from_raw(c) });
                return LIERO_MAIN_REPLAY;
            }
            Choice::Options => {
                a.load_settings(Some(&node), false);
            }
            Choice::Tc => match crate::tc::load_checked(&node) {
                Ok(tc) => {
                    a.menu_settings().tc = name.to_vec();
                    a.files.chosen_tc = Some(tc);
                }
                Err(e) => panic!("{e}"),
            },
        }
        -1
    }
}

/// A file selector (`LIERO_SELECTOR_*`), for `player`'s profile.
pub(crate) fn open_selector(kind: i32, player: i32) -> LieroScreen {
    let (spec, root, choice) = selector_spec(kind, player);
    selector_screen(&spec, Box::new(AppFiles { root, choice }))
}

/// What selector `kind` lists and where it starts, its root and its choice.
fn selector_spec(kind: i32, player: i32) -> (Spec, Node, Choice) {
    let a = app();
    let config = a.files.config.clone();
    let config_path = config.full_path().to_vec();
    let spec = |filter: &[u8], title: &[u8]| Spec {
        root: config_path.clone(),
        filter: filter.to_vec(),
        title: title.to_vec(),
        ..Default::default()
    };
    match kind {
        LIERO_SELECTOR_LEVEL => {
            let tc = a.tc_data();
            let spec = Spec {
                random_name: tc.text(text("Random")).to_vec(),
                select: a.menu_settings().level_file.clone(),
                has_select: true,
                enter_sound: true,
                ..spec(b"LEV", tc.text(text("SelLevel")))
            };
            (spec, config, Choice::Level)
        }
        LIERO_SELECTOR_PROFILE => {
            let spec = Spec {
                select: join_path(&config_path, b"Profiles"),
                has_select: true,
                ..spec(b"LPF", b"Select profile:")
            };
            let ws = a.menu_settings().worms[player as usize].clone();
            (spec, config, Choice::Profile(ws))
        }
        LIERO_SELECTOR_REPLAY => {
            let prev = a.files.prev_replay.clone();
            let spec = Spec {
                has_select: !prev.is_empty(),
                select: prev,
                fallback: join_path(&config_path, b"Replays"),
                ..spec(b"LRP", b"Select replay:")
            };
            (spec, config, Choice::Replay)
        }
        LIERO_SELECTOR_OPTIONS => (spec(b"CFG", b"Select options:"), config, Choice::Options),
        _ => {
            let root = a.files.tc_root.clone();
            let spec = Spec {
                root: root.full_path().to_vec(),
                title: b"Select TC:".to_vec(),
                tcs: true,
                ..Default::default()
            };
            (spec, root, Choice::Tc)
        }
    }
}

/// Switches to the TC the TC selector loaded, if it loaded one.
pub(crate) fn take_chosen_tc() -> bool {
    let a = app();
    match a.files.chosen_tc.take() {
        Some(tc) => {
            a.tc = Some(tc);
            true
        }
        None => false,
    }
}

/// The setup file loaded or saved last.
pub(crate) fn settings_path() -> Vec<u8> {
    app().files.settings_node.full_path().to_vec()
}

/// Saves the setup as `name`.cfg in the config folder.
pub(crate) fn save_options(name: &[u8]) {
    let a = app();
    let mut file = name.to_vec();
    file.extend_from_slice(b".cfg");
    let node = a.files.config.go(&file);
    if !a.save_settings(&node) {
        panic!("Could not write {}", path(&node));
    }
}

/// Saves `player`'s profile as Profiles/`name`.lpf in the config folder.
pub(crate) fn save_profile_as(player: i32, name: &[u8]) {
    let a = app();
    let mut file = name.to_vec();
    file.extend_from_slice(b".lpf");
    let node = a.files.config.go(b"Profiles").go(&file);
    let ws = &mut *a.menu_settings().worms[player as usize];
    if !liero_worm_settings_box_save_profile(ws, Some(&LieroFsNode(node.clone()))) {
        panic!("Could not write {}", path(&node));
    }
}

// What C++ calls: the folders, and the settings at the start and end.

/// # Safety
/// `text` is valid.
unsafe fn node_of(text: LieroStr) -> Node {
    unsafe { Node::new(text.bytes()) }
}

/// Sets the config folder (`Gfx::setConfigPath`).
///
/// # Safety
/// `path` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_app_set_config_path(a: &mut LieroApp, path: LieroStr) {
    unsafe {
        a.files.config = node_of(path);
    }
}

/// Sets the folder of TCs (`Gfx::setTcPath`).
///
/// # Safety
/// `path` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_app_set_tc_path(a: &mut LieroApp, path: LieroStr) {
    unsafe {
        a.files.tc_root = node_of(path);
    }
}

/// The TC folder `TC` in the config folder.
#[no_mangle]
pub extern "C" fn liero_app_set_tc_in_config(a: &mut LieroApp) {
    a.files.tc_root = a.files.config.go(b"TC");
}

/// A new node for the config folder.
#[no_mangle]
pub extern "C" fn liero_app_config(a: &mut LieroApp) -> *mut LieroFsNode {
    Box::into_raw(Box::new(LieroFsNode(a.files.config.clone())))
}

/// A new node for the folder of TCs.
#[no_mangle]
pub extern "C" fn liero_app_tc_root(a: &mut LieroApp) -> *mut LieroFsNode {
    Box::into_raw(Box::new(LieroFsNode(a.files.tc_root.clone())))
}

/// Sets the settings file the menu shows, without reading it.
#[no_mangle]
pub extern "C" fn liero_app_set_settings_node(a: &mut LieroApp, node: Option<&LieroFsNode>) {
    a.files.settings_node = node.map(|n| n.0.clone()).unwrap_or_default();
}

/// `LieroApp::load_settings`.
#[no_mangle]
pub extern "C" fn liero_app_load_settings(a: &mut LieroApp, node: Option<&LieroFsNode>, legacy: bool) -> bool {
    a.load_settings(node.map(|n| &n.0), legacy)
}

/// `LieroApp::save_settings`.
#[no_mangle]
pub extern "C" fn liero_app_save_settings(a: &mut LieroApp, node: &LieroFsNode) -> bool {
    a.save_settings(&node.0)
}

/// `LieroApp::mark_settings_saved`.
#[no_mangle]
pub extern "C" fn liero_app_mark_settings_saved(a: &mut LieroApp) {
    a.mark_settings_saved();
}

#[cfg(test)]
#[path = "app_files_tests.rs"]
mod tests;
