//! The settings the game runs with: `liero_data`'s `Settings`, with each
//! player's worm settings shared (`SettingsBox`), and a worm's settings
//! with the profile they were kept in (`WormSettingsBox`).

use crate::fs::Node;
use liero_data::settings::{self as data, ArchiveError, Bytes, Extensions, Settings, TextError, WormSettings};
use std::cell::RefCell;
use std::rc::Rc;

/// Up, down, left, right, fire, change and jump; and the same and dig.
pub use data::{MAX_CONTROL, MAX_CONTROL_EX};

/// What a settings call did.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsStatus {
    Ok,
    /// liero.cfg didn't parse (`gvl::toml::parse_error`), or LIERO.DAT ended
    /// early.
    Failed,
    /// A setting is missing from liero.cfg. The C++ reader threw
    /// `std::out_of_range` for this.
    MissingKey,
    /// The byte source failed; C++ rethrows what its reader threw.
    ReadError,
    /// The settings archive's checkpoint is wrong
    /// (`gvl::archive_check_error`).
    CheckError,
}

fn archive_status(r: Result<(), ArchiveError>) -> SettingsStatus {
    match r {
        Ok(()) => SettingsStatus::Ok,
        Err(ArchiveError::Read) => SettingsStatus::ReadError,
        Err(ArchiveError::Check) => SettingsStatus::CheckError,
    }
}

/// The game's settings, with each player's worm settings shared by the
/// game's worms.
pub type GameSettings = Settings<SharedWormSettings>;

/// The game's settings. A clone shares the worms' settings, as C++'s
/// `Settings::clone` did.
#[derive(Clone)]
pub struct SettingsBox {
    pub data: GameSettings,
}

/// A worm's settings, shared by the settings that hold them, their clones
/// and the game's worms (C++'s `shared_ptr<WormSettings>`). The menus
/// change them while a game behind the menu plays with them.
pub type SharedWormSettings = Rc<RefCell<WormSettingsBox>>;

/// The game's settings, shared by the app's menus and the games played with
/// them (C++'s `shared_ptr<Settings>`): the menus change them while a game
/// behind the menu plays. A replay's game has its own.
pub type SharedSettings = Rc<RefCell<SettingsBox>>;

impl SettingsBox {
    /// The settings as the formats read and write them, with the worms'.
    pub(crate) fn settings(&self) -> Settings {
        let worms = self.data.worms.each_ref().map(|w| w.borrow().worm());
        self.data.clone().with_worms(worms).0
    }

    /// Sets the settings and the worms' from `s`.
    pub(crate) fn set_settings(&mut self, s: &Settings) {
        let (data, worms) = s.clone().with_worms(self.data.worms.clone());
        self.data = data;
        // The game's settings first: a worm's can be one of these, which
        // the caller may set after.
        for (w, ws) in self.data.worms.iter().zip(&worms) {
            w.borrow_mut().set_worm(ws);
        }
    }

    /// New settings with the defaults (`new Settings`).
    pub(crate) fn defaults() -> SharedSettings {
        let b = new_settings();
        b.borrow_mut().set_settings(&Settings::default());
        b
    }

    /// Reads liero.cfg (or LIERO.DAT, if `legacy`) from `node` over the
    /// settings, as far as it reads (`Settings::load` and `loadLegacy`):
    /// None if the file can't be read, else whether it read whole.
    pub(crate) fn load(&mut self, node: &crate::fs::Node, legacy: bool) -> Option<SettingsStatus> {
        let bytes = node.read()?;
        let mut settings = self.settings();
        let status = if legacy {
            if data::read_legacy(&mut settings, &bytes) {
                SettingsStatus::Ok
            } else {
                SettingsStatus::Failed
            }
        } else {
            match data::read_text(&mut settings, &bytes) {
                Ok(()) => SettingsStatus::Ok,
                Err(TextError::Parse(_)) => SettingsStatus::Failed,
                Err(TextError::MissingKey(_)) => SettingsStatus::MissingKey,
            }
        };
        self.set_settings(&settings);
        Some(status)
    }

    /// The settings as liero.cfg (`Settings::text`).
    pub(crate) fn text(&self) -> Vec<u8> {
        data::write_text(&self.settings())
    }
}

/// A worm's settings (`WormSettings`), and where they were last kept.
pub struct WormSettingsBox {
    pub data: WormSettings,
    /// The profile they were last loaded from or saved to, if any. A clone
    /// starts without one, as a new `WormSettings` did.
    pub profile: Option<crate::fs::Node>,
}

impl WormSettingsBox {
    /// The settings as the archives read and write them.
    pub(crate) fn worm(&self) -> WormSettings {
        self.data.clone()
    }

    pub(crate) fn set_worm(&mut self, w: &WormSettings) {
        self.data = w.clone();
    }

    /// The profile's file, if there is one (a null node isn't one).
    pub(crate) fn profile_path(&self) -> Option<Vec<u8>> {
        self.profile
            .as_ref()
            .filter(|n| !n.is_null())
            .map(|n| n.full_path().to_vec())
    }

    /// Saves the settings over their profile (the menus' "save profile").
    pub(crate) fn save_to_profile(&mut self) -> bool {
        let node = self.profile.clone();
        self.save_profile(node.as_ref())
    }

    /// Loads a profile from `node` over the settings, keeping their colour,
    /// and remembers the profile (`WormSettings::loadProfile`). False, with
    /// nothing changed, if the file can't be read. A profile that ends early
    /// or fails its checkpoint is loaded as far as it was read, with its
    /// colour: `status` says so, and C++ threw before putting the colour back.
    pub fn load_profile(&mut self, node: Option<&Node>, status: &mut SettingsStatus) -> bool {
        let Some(node) = node else { return false };
        let Some(d) = node.read() else { return false };
        let mut worm = self.worm();
        *status = archive_status(data::read_worm(
            &mut worm,
            crate::replay::REPLAY_VERSION,
            &mut Bytes::new(&d),
        ));
        let color = self.data.color;
        self.profile = Some(node.clone());
        self.set_worm(&worm);
        if matches!(status, SettingsStatus::Ok) {
            self.data.color = color;
        }
        true
    }

    /// Saves the settings to `node` as a profile, in the current replay
    /// version's archive, and remembers the profile
    /// (`WormSettings::saveProfile`). False, with nothing changed, if the file
    /// can't be written.
    pub fn save_profile(&mut self, node: Option<&Node>) -> bool {
        let Some(node) = node else { return false };
        // Opened before the archive is made, as `toOctetWriter` was.
        if node.prepare_write().is_none() {
            return false;
        }
        let mut worm = self.worm();
        let bytes = data::write_worm(&mut worm, crate::replay::REPLAY_VERSION);
        if !node.write(&bytes) {
            return false;
        }
        self.profile = Some(node.clone());
        self.set_worm(&worm);
        true
    }
}

impl Clone for WormSettingsBox {
    fn clone(&self) -> Self {
        WormSettingsBox {
            data: self.data.clone(),
            profile: None,
        }
    }
}

/// Every field zero or empty, as a new C++ `WormSettings` box was before
/// the defaults were set.
impl Default for WormSettingsBox {
    fn default() -> Self {
        WormSettingsBox {
            data: WormSettings {
                health: 0,
                controller: 0,
                controls: [0; MAX_CONTROL],
                weapons: [0; 5],
                name: Vec::new(),
                rgb: [0; 3],
                random_name: false,
                color: 0,
                controls_ex: [0; MAX_CONTROL_EX],
            },
            profile: None,
        }
    }
}

/// Settings with every field zero or empty, and the worms' too; C++ then
/// sets the defaults.
pub fn new_settings() -> SharedSettings {
    Rc::new(RefCell::new(SettingsBox {
        data: Settings {
            ext: Extensions {
                record_replays: false,
                load_powerlevel_palette: false,
                blood_particle_max: 0,
                ai_frames: 0,
                ai_mutations: 0,
                ai_traces: false,
                ai_parallels: 0,
                fullscreen: false,
                zone_timeout: 0,
                select_bot_weapons: 0,
                allow_viewing_spawn_point: false,
                single_screen_replay: false,
                spectator_window: false,
                tc: Vec::new(),
            },
            weap_table: [0; 40],
            max_bonuses: 0,
            blood: 0,
            time_to_lose: 0,
            flags_to_win: 0,
            game_mode: 0,
            shadow: false,
            load_change: false,
            names_on_bonuses: false,
            regenerate_level: false,
            lives: 0,
            loading_time: 0,
            random_level: false,
            level_file: Vec::new(),
            map: false,
            screen_sync: false,
            worms: [new_worm_settings(), new_worm_settings()],
        },
    }))
}

/// A copy of `s` that shares its worms' settings.
pub fn clone_settings(s: &SettingsBox) -> SharedSettings {
    Rc::new(RefCell::new(s.clone()))
}

/// A worm's settings with every field zero or empty.
pub fn new_worm_settings() -> SharedWormSettings {
    Rc::default()
}

/// A copy of `s`, shared by no one yet.
pub fn clone_worm_settings(s: &WormSettingsBox) -> SharedWormSettings {
    Rc::new(RefCell::new(s.clone()))
}

#[cfg(test)]
mod box_tests {
    use super::*;

    #[test]
    fn a_clone_shares_the_worms() {
        let a = new_settings();
        let w0 = a.borrow().data.worms[0].clone();
        assert!(!Rc::ptr_eq(&w0, &a.borrow().data.worms[1].clone()));
        let b = clone_settings(&a.borrow());
        assert!(Rc::ptr_eq(&b.borrow().data.worms[0].clone(), &w0));
        // `a`, `b` and `w0`.
        assert_eq!(Rc::strong_count(&w0), 3);

        // Setting one is the box's own.
        let w = new_worm_settings();
        b.borrow_mut().data.worms[0] = w.clone();
        assert_eq!((Rc::strong_count(&w), Rc::strong_count(&w0)), (2, 2));
        assert!(Rc::ptr_eq(&a.borrow().data.worms[0].clone(), &w0));
        drop(b);
        assert_eq!(Rc::strong_count(&w), 1);
        drop(a);
        assert_eq!(Rc::strong_count(&w0), 1);
    }

    #[test]
    fn a_clone_is_separate() {
        let a = new_settings();
        a.borrow_mut().data.lives = 3;
        a.borrow_mut().data.ext.tc = b"openliero".to_vec();
        let b = clone_settings(&a.borrow());
        a.borrow_mut().data.lives = 5;
        a.borrow_mut().data.ext.tc = b"other".to_vec();
        assert_eq!(b.borrow().data.lives, 3);
        assert_eq!(b.borrow().data.ext.tc, b"openliero");
    }

    #[test]
    fn worm_name_and_fields() {
        let w = new_worm_settings();
        w.borrow_mut().data.health = 7;
        w.borrow_mut().data.name = b"BOT A".to_vec();
        let c = clone_worm_settings(&w.borrow());
        assert_eq!(c.borrow().data.name, b"BOT A");
        assert_eq!(c.borrow().data.health, 7);
    }

    #[test]
    fn profiles_are_remembered() {
        // Relative, as the game's nodes take paths: under the crate's target.
        let dir = std::path::PathBuf::from(format!("target/liero-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.lpf");
        let path_bytes = path.to_str().unwrap().as_bytes().to_vec();
        let w = WormSettingsBox::default();
        {
            let mut w = w;
            let w = &mut w;
            assert!(!w.save_to_profile(), "no profile to save over");
            w.data.name = b"WORMY".to_vec();
            w.data.health = 50;
            w.data.color = 3;

            let node = Node::new(&path_bytes);
            let path_bytes = node.full_path().to_vec();
            assert!(w.save_profile(Some(&node)));
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.profile = None;
            w.data.health = 1;
            w.data.color = 9;
            let mut status = SettingsStatus::Failed;
            assert!(w.load_profile(Some(&node), &mut status));
            assert!(matches!(status, SettingsStatus::Ok));
            assert_eq!(
                (w.data.health, w.data.color, &w.data.name[..]),
                (50, 9, &b"WORMY"[..]),
                "the colour stays"
            );
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.data.health = 2;
            assert!(w.save_to_profile(), "saved over the profile");
            w.data.health = 3;
            assert!(w.load_profile(Some(&node), &mut status));
            assert_eq!(w.data.health, 2);

            let missing = Node::new(b"/no/such/dir/x.lpf");
            assert!(!w.load_profile(Some(&missing), &mut status));
            assert!(!w.save_profile(None));
            assert_eq!(w.profile_path(), Some(path_bytes), "unchanged by failures");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_clone_has_no_profile() {
        let node = Node::new(b"a/b.lpf");
        let mut w = WormSettingsBox {
            profile: Some(node.clone()),
            ..Default::default()
        };
        w.data.health = 3;
        let c = w.clone();
        assert!(c.profile.is_none(), "a clone has no profile");
        assert_eq!(c.data.health, 3);
        assert_eq!(w.profile_path().as_deref(), Some(node.full_path()));
        w.profile = Some(crate::fs::Node::default());
        assert_eq!(w.profile_path(), None, "a null node isn't a profile");
    }
}
