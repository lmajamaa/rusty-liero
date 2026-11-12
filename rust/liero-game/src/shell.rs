//! What the C++ shell (`src/game`: SDL's window, events and sound) asks of
//! the app: starting it from the command line's folders, saving the
//! settings at the end, the window's settings, and a sound for the menus.

use crate::app::LieroApp;
use crate::fs::Node;
use crate::settings::LieroSettingsBox;
use crate::sfx::{LieroMixer, LieroSoundOut};
use crate::text::LieroStr;
use std::cell::RefCell;

thread_local! {
    /// The message of the last call that failed.
    static ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn fail(message: String) -> bool {
    ERROR.with(|e| *e.borrow_mut() = message.into_bytes());
    false
}

/// Why the last call that returned false failed, valid until the next
/// call.
#[no_mangle]
pub extern "C" fn liero_shell_error() -> LieroStr {
    ERROR.with(|e| LieroStr::of(&e.borrow()))
}

impl LieroApp {
    /// Starts the app as `main` did: the config folder `config` (empty for
    /// the current one) and the folder of TCs `tc_root` (empty for TC in
    /// the config folder); the settings from liero.cfg, else LIERO.DAT,
    /// else the defaults, written to liero.cfg; then the TC `tc`, or the
    /// settings' one if empty. The message, if it can't write liero.cfg or
    /// load the TC.
    pub fn start(&mut self, config: &[u8], tc_root: &[u8], tc: &[u8]) -> Result<(), String> {
        self.files.config = Node::new(config);
        self.files.tc_root = if tc_root.is_empty() { self.files.config.go(b"TC") } else { Node::new(tc_root) };

        let cfg = self.files.config.go(b"liero.cfg");
        if !self.load_settings(Some(&cfg), false) && !self.load_settings(Some(&self.files.config.go(b"LIERO.DAT")), true) {
            self.settings = Some(LieroSettingsBox::defaults());
            if !self.save_settings(&cfg) {
                return Err(format!("Could not write {}", String::from_utf8_lossy(cfg.full_path())));
            }
        }
        if !tc.is_empty() {
            self.menu_settings().tc = tc.to_vec();
        }
        if self.save_settings_on_change {
            // What liero.cfg holds now, so the menu only writes it on a
            // change.
            self.mark_settings_saved();
        }

        let node = self.files.tc_root.go(&self.menu_settings().tc);
        let loaded = crate::tc::load_checked(&node)?;
        self.tc = Some(loaded);
        let d = self.tc_data();
        self.play.load_palette(d);
        Ok(())
    }

    /// Writes the settings to liero.cfg in the config folder, as the game
    /// does when it ends.
    pub fn save_settings_at_end(&mut self) -> Result<(), String> {
        let node = self.files.config.go(b"liero.cfg");
        if node.write(&self.menu_settings().text()) {
            Ok(())
        } else {
            Err(format!("Could not write {}", String::from_utf8_lossy(node.full_path())))
        }
    }
}

/// `LieroApp::start`. False if it failed, with the message in
/// `liero_shell_error`.
///
/// # Safety
/// The paths are valid.
#[no_mangle]
pub unsafe extern "C" fn liero_shell_start(a: &mut LieroApp, config: LieroStr, tc_root: LieroStr, tc: LieroStr) -> bool { unsafe {
    a.start(config.bytes(), tc_root.bytes(), tc.bytes()).map_or_else(fail, |()| true)
}}

/// `LieroApp::save_settings_at_end`. False if it failed, with the message in
/// `liero_shell_error`.
#[no_mangle]
pub extern "C" fn liero_shell_save_settings(a: &mut LieroApp) -> bool {
    a.save_settings_at_end().map_or_else(fail, |()| true)
}

/// The config folder's path, for the window's icon.
#[no_mangle]
pub extern "C" fn liero_shell_config_path(a: &LieroApp) -> LieroStr {
    LieroStr::of(a.files.config.full_path())
}

#[no_mangle]
pub extern "C" fn liero_shell_fullscreen(a: &LieroApp) -> bool {
    a.menu_settings().data.fullscreen
}

#[no_mangle]
pub extern "C" fn liero_shell_set_fullscreen(a: &mut LieroApp, on: bool) {
    a.menu_settings().data.fullscreen = on;
}

#[no_mangle]
pub extern "C" fn liero_shell_spectator_window(a: &LieroApp) -> bool {
    a.menu_settings().data.spectator_window
}

#[no_mangle]
pub extern "C" fn liero_shell_set_spectator_window(a: &mut LieroApp, on: bool) {
    a.menu_settings().data.spectator_window = on;
}

/// Plays the menus' TC's `sound` on `mixer`, if there is one.
///
/// # Safety
/// `mixer` is valid or null.
#[no_mangle]
pub unsafe extern "C" fn liero_shell_play_sound(a: &LieroApp, mixer: *mut LieroMixer, sound: i32) { unsafe {
    if mixer.is_null() {
        return;
    }
    LieroSoundOut { mixer, report: None }.play(a.tc_data(), sound, std::ptr::null_mut(), 0);
}}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app;

    /// An empty folder of the test's own under target/, as a path from the
    /// crate's folder (`Node::new` reads a leading `/` as the current folder).
    fn folder(name: &str) -> String {
        let p = format!("target/shell-tests/{name}");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn bytes(s: LieroStr) -> Vec<u8> {
        // SAFETY: the string is valid until the next call.
        unsafe { s.bytes().to_vec() }
    }

    fn read(path: &str) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    #[test]
    fn start_writes_the_defaults_and_loads_the_tc() {
        let dir = folder("defaults");
        let a = app();
        a.start(dir.as_bytes(), b"../../TC", b"").unwrap();
        assert_eq!(read(&format!("{dir}/liero.cfg")), LieroSettingsBox::defaults().text());
        assert_eq!(bytes(liero_shell_config_path(a)), format!("./{dir}").as_bytes());
        assert_eq!(a.menu_settings().tc, b"openliero");
        assert!(a.tc.is_some());
    }

    #[test]
    fn start_reads_liero_cfg_and_keeps_it() {
        let dir = folder("read");
        let a = app();
        a.settings = Some(LieroSettingsBox::defaults());
        a.menu_settings().data.lives = 7;
        let text = a.menu_settings().text();
        std::fs::write(format!("{dir}/liero.cfg"), &text).unwrap();
        a.settings = None;

        a.start(dir.as_bytes(), b"../../TC", b"").unwrap();
        assert_eq!(a.menu_settings().data.lives, 7);
        assert_eq!(read(&format!("{dir}/liero.cfg")), text);
    }

    #[test]
    fn start_takes_the_tc_and_root_given() {
        let dir = folder("tc");
        let a = app();
        // The TC folder in the config folder, by default: none here.
        assert!(a.start(dir.as_bytes(), b"", b"").unwrap_err().starts_with("Could not load the TC"));
        assert_eq!(a.files.tc_root.full_path(), format!("./{dir}/TC").as_bytes());
        // The TC named replaces the settings' one, but isn't saved.
        assert!(a.start(dir.as_bytes(), b"../../TC", b"nothing").unwrap_err().starts_with("Could not load the TC"));
        assert_eq!(a.files.tc_root.full_path(), b"./../../TC");
        assert_eq!(a.menu_settings().tc, b"nothing");
        a.start(dir.as_bytes(), b"../../TC", b"openliero").unwrap();
        assert_eq!(read(&format!("{dir}/liero.cfg")), LieroSettingsBox::defaults().text());
    }

    #[test]
    fn start_fails_if_it_cannot_write_liero_cfg() {
        // A folder in its place: it can't be read or written.
        let dir = folder("unwritable");
        std::fs::create_dir(format!("{dir}/liero.cfg")).unwrap();
        let a = app();
        let e = a.start(dir.as_bytes(), b"../../TC", b"").unwrap_err();
        assert_eq!(e, format!("Could not write ./{dir}/liero.cfg"));
        // SAFETY: the strings are valid.
        assert!(!unsafe { liero_shell_start(a, LieroStr::of(dir.as_bytes()), LieroStr::of(b"../../TC"), LieroStr::of(b"")) });
        assert_eq!(bytes(liero_shell_error()), e.as_bytes());
    }

    #[test]
    fn start_marks_the_settings_saved_if_the_menu_saves_them() {
        let dir = folder("mark");
        let a = app();
        a.save_settings_on_change = true;
        a.start(dir.as_bytes(), b"../../TC", b"").unwrap();
        std::fs::remove_file(format!("{dir}/liero.cfg")).unwrap();
        a.save_settings_if_changed();
        assert!(!std::path::Path::new(&format!("{dir}/liero.cfg")).exists());
        a.menu_settings().data.lives = 3;
        a.save_settings_if_changed();
        assert!(std::path::Path::new(&format!("{dir}/liero.cfg")).exists());
    }

    #[test]
    fn save_settings_writes_liero_cfg() {
        let dir = folder("save");
        let a = app();
        a.start(dir.as_bytes(), b"../../TC", b"").unwrap();
        a.menu_settings().data.lives = 9;
        assert!(liero_shell_save_settings(a));
        assert_eq!(read(&format!("{dir}/liero.cfg")), a.menu_settings().text());

        std::fs::remove_file(format!("{dir}/liero.cfg")).unwrap();
        std::fs::create_dir(format!("{dir}/liero.cfg")).unwrap();
        assert!(!liero_shell_save_settings(a));
        assert_eq!(bytes(liero_shell_error()), format!("Could not write ./{dir}/liero.cfg").as_bytes());
    }

    #[test]
    fn window_settings() {
        let a = app();
        a.settings = Some(LieroSettingsBox::defaults());
        liero_shell_set_fullscreen(a, true);
        liero_shell_set_spectator_window(a, false);
        assert!(liero_shell_fullscreen(a) && a.menu_settings().data.fullscreen);
        assert!(!liero_shell_spectator_window(a));
        liero_shell_set_fullscreen(a, false);
        liero_shell_set_spectator_window(a, true);
        assert!(!liero_shell_fullscreen(a));
        assert!(liero_shell_spectator_window(a) && a.menu_settings().data.spectator_window);
    }
}
