//! The app's files (`Gfx`'s settings and file selectors in C++): the config
//! and TC folders, loading and saving the settings and profiles, what the
//! file selectors list and what choosing a file does, and switching TC.
//!
//! Errors C++ threw and never caught panic here, as they ended the app
//! there: a profile, replay or TC that can't be read, liero.cfg missing a
//! setting, and a setup or profile that can't be written.

use crate::app::App;
use crate::fs::{join_path, Node};
use crate::level::LevelBox;
use crate::menus::{Selector, MAIN_REPLAY};
use crate::screen::BoxedScreen;
use crate::selector::{selector_screen, FileSource, Spec};
use crate::settings::{SettingsBox, SettingsStatus, SharedWormSettings};
use crate::tc::s as text;

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
    pub chosen_tc: Option<crate::tc::SharedTc>,
}

fn path(n: &Node) -> String {
    String::from_utf8_lossy(n.full_path()).into_owned()
}

impl App {
    /// New settings with `node` read over them (`Gfx::loadSettings`, or
    /// with `legacy`, `loadSettingsLegacy`). False if it can't be read or
    /// didn't read whole.
    pub fn load_settings(&mut self, node: Option<&Node>, legacy: bool) -> bool {
        if !legacy {
            self.files.settings_node = node.cloned().unwrap_or_default();
        }
        let settings = SettingsBox::defaults();
        let status = node.and_then(|n| settings.borrow_mut().load(n, legacy));
        self.settings = Some(settings);
        match status {
            Some(SettingsStatus::Ok) => true,
            // What std::map::at threw in the C++ reader. Nothing caught it.
            Some(SettingsStatus::MissingKey) => panic!("Missing setting in liero.cfg"),
            _ => false,
        }
    }

    /// Writes the settings to `node` as liero.cfg and remembers it
    /// (`Gfx::saveSettings`). False if it can't be written.
    pub fn save_settings(&mut self, node: &Node) -> bool {
        self.files.settings_node = node.clone();
        node.write(&self.settings_mut().text())
    }

    /// Takes the settings as liero.cfg has them.
    pub fn mark_settings_saved(&mut self) {
        let text = self.settings_mut().text();
        self.files.saved_text = text;
    }

    /// Writes liero.cfg in the config folder if the settings changed since
    /// it was last read or written here (`Gfx::saveSettingsIfChanged`). The
    /// browser's menu calls it, since a closed tab never reaches the save at
    /// the end.
    pub fn save_settings_if_changed(&mut self) {
        let text = self.settings_mut().text();
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
    Profile(SharedWormSettings),
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
fn preview_level(a: &App, node: &Node) {
    // A file that can't be read isn't shown.
    let Some(bytes) = node.read() else { return };
    let tc = a.tc_data();
    let mut level = LevelBox::new();
    let powerlevel = a.settings_mut().data.ext.load_powerlevel_palette;
    if !level.load(&tc.level_common(), &tc.palette_colors, powerlevel, &bytes) {
        return;
    }
    let center_x = a.single.data.render_res_x / 2;
    let (play, spectator) = (a.frozen.view(), a.frozen_spectator.view());
    level.draw_miniature(&play, 134, 162, 10);
    level.draw_miniature(&spectator, center_x - 126, a.single.data.render_res_y - 208, 2);
}

impl FileSource for AppFiles {
    fn list(&mut self, rel: &[u8]) -> Vec<(Vec<u8>, bool)> {
        self.node(rel).list().into_iter().map(|e| (e.name, e.is_dir)).collect()
    }

    fn exists(&mut self, rel: &[u8]) -> bool {
        self.node(rel).exists()
    }

    fn preview(&mut self, a: &App, rel: &[u8]) {
        if matches!(self.choice, Choice::Level) {
            preview_level(a, &self.node(rel));
        }
    }

    fn choose(&mut self, a: &mut App, rel: &[u8], full_path: &[u8], name: &[u8], random: bool) -> i32 {
        let node = self.node(rel);
        match &mut self.choice {
            Choice::Level => {
                let mut s = a.settings_mut();
                s.data.random_level = random;
                s.data.level_file = if random { Vec::new() } else { full_path.to_vec() };
            }
            Choice::Profile(ws) => {
                let mut status = SettingsStatus::Ok;
                if !ws.borrow_mut().load_profile(Some(&node.clone()), &mut status) {
                    panic!("Could not read {}", path(&node));
                }
                // A profile that ends early, or fails its checkpoint, threw
                // with the profile's colour, and nothing caught it.
                match status {
                    SettingsStatus::ReadError => panic!("Read error in get()"),
                    SettingsStatus::CheckError => panic!("Expected checkpoint here"),
                    _ => {}
                }
            }
            Choice::Replay => {
                a.files.prev_replay = full_path.to_vec();
                // Reset controller before opening the replay, since we may
                // be recording it
                a.controller = None;
                let c = crate::controllers::Controller::replay(&node.clone(), a.menu_tc());
                a.controller = Some(c.unwrap_or_else(|| panic!("Could not read {}", path(&node))));
                return MAIN_REPLAY;
            }
            Choice::Options => {
                a.load_settings(Some(&node), false);
            }
            Choice::Tc => match crate::tc::load_checked(&node) {
                Ok(tc) => {
                    a.settings_mut().data.ext.tc = name.to_vec();
                    a.files.chosen_tc = Some(tc);
                }
                Err(e) => panic!("{e}"),
            },
        }
        -1
    }
}

/// A file selector, for `player`'s profile.
pub(crate) fn open_selector(a: &App, kind: Selector, player: i32) -> BoxedScreen {
    let (spec, root, choice) = selector_spec(a, kind, player);
    selector_screen(&spec, Box::new(AppFiles { root, choice }))
}

/// What selector `kind` lists and where it starts, its root and its choice.
fn selector_spec(a: &App, kind: Selector, player: i32) -> (Spec, Node, Choice) {
    let config = a.files.config.clone();
    let config_path = config.full_path().to_vec();
    let spec = |filter: &[u8], title: &[u8]| Spec {
        root: config_path.clone(),
        filter: filter.to_vec(),
        title: title.to_vec(),
        ..Default::default()
    };
    match kind {
        Selector::Level => {
            let tc = a.tc_data();
            let spec = Spec {
                random_name: tc.text(text("Random")).to_vec(),
                select: a.settings_mut().data.level_file.clone(),
                has_select: true,
                enter_sound: true,
                ..spec(b"LEV", tc.text(text("SelLevel")))
            };
            (spec, config, Choice::Level)
        }
        Selector::Profile => {
            let spec = Spec {
                select: join_path(&config_path, b"Profiles"),
                has_select: true,
                ..spec(b"LPF", b"Select profile:")
            };
            let ws = a.settings_mut().data.worms[player as usize].clone();
            (spec, config, Choice::Profile(ws))
        }
        Selector::Replay => {
            let prev = a.files.prev_replay.clone();
            let spec = Spec {
                has_select: !prev.is_empty(),
                select: prev,
                fallback: join_path(&config_path, b"Replays"),
                ..spec(b"LRP", b"Select replay:")
            };
            (spec, config, Choice::Replay)
        }
        Selector::Options => (spec(b"CFG", b"Select options:"), config, Choice::Options),
        Selector::Tc => {
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
pub(crate) fn take_chosen_tc(a: &mut App) -> bool {
    match a.files.chosen_tc.take() {
        Some(tc) => {
            a.set_tc(Some(tc));
            true
        }
        None => false,
    }
}

/// The setup file loaded or saved last.
pub(crate) fn settings_path(a: &App) -> Vec<u8> {
    a.files.settings_node.full_path().to_vec()
}

/// Saves the setup as `name`.cfg in the config folder.
pub(crate) fn save_options(a: &mut App, name: &[u8]) {
    let mut file = name.to_vec();
    file.extend_from_slice(b".cfg");
    let node = a.files.config.go(&file);
    if !a.save_settings(&node) {
        panic!("Could not write {}", path(&node));
    }
}

/// Saves `player`'s profile as Profiles/`name`.lpf in the config folder.
pub(crate) fn save_profile_as(a: &App, player: i32, name: &[u8]) {
    let mut file = name.to_vec();
    file.extend_from_slice(b".lpf");
    let node = a.files.config.go(b"Profiles").go(&file);
    let ws = a.settings_mut().data.worms[player as usize].clone();
    let mut ws = ws.borrow_mut();
    if !ws.save_profile(Some(&node.clone())) {
        panic!("Could not write {}", path(&node));
    }
}

#[cfg(test)]
#[path = "app_files_tests.rs"]
mod tests;
