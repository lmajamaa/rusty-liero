//! C interface to the settings formats in `liero_data::settings`, and the
//! settings the game runs with (`LieroSettingsBox`, at the end).
//!
//! The C++ `Settings` and `WormSettings` refer to fields Rust owns. To
//! read or write them, C++ passes a `LieroSettings` or `LieroWormSettings`
//! view of its fields, and gets back a `LieroSettingsResult` holding the
//! settings afterwards (reads change them, and so do some writes), the bytes
//! written or the hash, and whether it worked. C++ copies what it needs out
//! of the result and frees it.

use crate::fs::Node;
use crate::text::LieroStr;
use liero_data::settings::{
    self as data, ArchiveError, Bytes, Extensions, ReadError, Settings, Source, TextError, WormSettings,
};
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

/// Up, down, left, right, fire, change and jump.
pub const LIERO_MAX_CONTROL: usize = 7;
/// The same and dig.
pub const LIERO_MAX_CONTROL_EX: usize = 8;

#[derive(Clone, Copy)]
pub struct LieroWormSettings {
    pub health: i32,
    pub controller: u32,
    pub controls: [u32; LIERO_MAX_CONTROL],
    pub weapons: [u32; 5],
    pub name: LieroStr,
    pub rgb: [i32; 3],
    pub random_name: bool,
    pub color: i32,
    pub controls_ex: [u32; LIERO_MAX_CONTROL_EX],
}

#[derive(Clone, Copy)]
pub struct LieroSettings {
    pub record_replays: bool,
    pub load_powerlevel_palette: bool,
    pub blood_particle_max: i32,
    pub ai_frames: i32,
    pub ai_mutations: i32,
    pub ai_traces: bool,
    pub ai_parallels: i32,
    pub fullscreen: bool,
    pub zone_timeout: i32,
    pub select_bot_weapons: u32,
    pub allow_viewing_spawn_point: bool,
    pub single_screen_replay: bool,
    pub spectator_window: bool,
    pub tc: LieroStr,

    pub weap_table: [u32; 40],
    pub max_bonuses: i32,
    pub blood: i32,
    pub time_to_lose: i32,
    pub flags_to_win: i32,
    pub game_mode: u32,
    pub shadow: bool,
    pub load_change: bool,
    pub names_on_bonuses: bool,
    pub regenerate_level: bool,
    pub lives: i32,
    pub loading_time: i32,
    pub random_level: bool,
    pub level_file: LieroStr,
    pub map: bool,
    pub screen_sync: bool,

    pub worms: [LieroWormSettings; 2],
}

/// What a settings call did.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LieroSettingsStatus {
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

/// Bytes for the binary readers. `get` returns the next byte, or -1 when
/// the C++ reader throws, which C++ keeps to rethrow.
pub struct LieroByteSource {
    pub ctx: *mut c_void,
    pub get: unsafe fn(ctx: *mut c_void) -> i32,
}

pub(crate) struct Hooked<'a>(pub(crate) &'a LieroByteSource);

impl Source for Hooked<'_> {
    fn get(&mut self) -> Result<u8, ReadError> {
        // SAFETY: the caller of the reading function promises that `get`
        // can be called with `ctx`.
        let b = unsafe { (self.0.get)(self.0.ctx) };
        u8::try_from(b).map_err(|_| ReadError)
    }
}

/// A gash value (`gvl::gash::value_type`).
#[derive(Clone, Copy, Default)]
pub struct LieroHash {
    pub value: [u64; 4],
}

pub struct LieroSettingsResult {
    status: LieroSettingsStatus,
    settings: Option<Settings>,
    worm: Option<WormSettings>,
    bytes: Vec<u8>,
    hash: LieroHash,
    settings_view: Option<LieroSettings>,
    worm_view: Option<LieroWormSettings>,
}

pub(crate) fn str(s: &[u8]) -> LieroStr {
    LieroStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

pub(crate) fn worm_view(ws: &WormSettings) -> LieroWormSettings {
    LieroWormSettings {
        health: ws.health,
        controller: ws.controller,
        controls: ws.controls,
        weapons: ws.weapons,
        name: str(&ws.name),
        rgb: ws.rgb,
        random_name: ws.random_name,
        color: ws.color,
        controls_ex: ws.controls_ex,
    }
}

pub(crate) fn settings_view(s: &Settings) -> LieroSettings {
    let e = &s.ext;
    LieroSettings {
        record_replays: e.record_replays,
        load_powerlevel_palette: e.load_powerlevel_palette,
        blood_particle_max: e.blood_particle_max,
        ai_frames: e.ai_frames,
        ai_mutations: e.ai_mutations,
        ai_traces: e.ai_traces,
        ai_parallels: e.ai_parallels,
        fullscreen: e.fullscreen,
        zone_timeout: e.zone_timeout,
        select_bot_weapons: e.select_bot_weapons,
        allow_viewing_spawn_point: e.allow_viewing_spawn_point,
        single_screen_replay: e.single_screen_replay,
        spectator_window: e.spectator_window,
        tc: str(&e.tc),
        weap_table: s.weap_table,
        max_bonuses: s.max_bonuses,
        blood: s.blood,
        time_to_lose: s.time_to_lose,
        flags_to_win: s.flags_to_win,
        game_mode: s.game_mode,
        shadow: s.shadow,
        load_change: s.load_change,
        names_on_bonuses: s.names_on_bonuses,
        regenerate_level: s.regenerate_level,
        lives: s.lives,
        loading_time: s.loading_time,
        random_level: s.random_level,
        level_file: str(&s.level_file),
        map: s.map,
        screen_sync: s.screen_sync,
        worms: [worm_view(&s.worms[0]), worm_view(&s.worms[1])],
    }
}

/// # Safety
/// The view's strings must be valid.
pub(crate) unsafe fn worm_from(v: &LieroWormSettings) -> WormSettings {
    unsafe {
        WormSettings {
            health: v.health,
            controller: v.controller,
            controls: v.controls,
            weapons: v.weapons,
            name: v.name.bytes().to_vec(),
            rgb: v.rgb,
            random_name: v.random_name,
            color: v.color,
            controls_ex: v.controls_ex,
        }
    }
}

/// # Safety
/// The view's strings must be valid.
pub(crate) unsafe fn settings_from(v: &LieroSettings) -> Settings {
    unsafe {
        Settings {
            ext: Extensions {
                record_replays: v.record_replays,
                load_powerlevel_palette: v.load_powerlevel_palette,
                blood_particle_max: v.blood_particle_max,
                ai_frames: v.ai_frames,
                ai_mutations: v.ai_mutations,
                ai_traces: v.ai_traces,
                ai_parallels: v.ai_parallels,
                fullscreen: v.fullscreen,
                zone_timeout: v.zone_timeout,
                select_bot_weapons: v.select_bot_weapons,
                allow_viewing_spawn_point: v.allow_viewing_spawn_point,
                single_screen_replay: v.single_screen_replay,
                spectator_window: v.spectator_window,
                tc: v.tc.bytes().to_vec(),
            },
            weap_table: v.weap_table,
            max_bonuses: v.max_bonuses,
            blood: v.blood,
            time_to_lose: v.time_to_lose,
            flags_to_win: v.flags_to_win,
            game_mode: v.game_mode,
            shadow: v.shadow,
            load_change: v.load_change,
            names_on_bonuses: v.names_on_bonuses,
            regenerate_level: v.regenerate_level,
            lives: v.lives,
            loading_time: v.loading_time,
            random_level: v.random_level,
            level_file: v.level_file.bytes().to_vec(),
            map: v.map,
            screen_sync: v.screen_sync,
            worms: [worm_from(&v.worms[0]), worm_from(&v.worms[1])],
        }
    }
}

impl LieroSettingsResult {
    fn new(status: LieroSettingsStatus) -> Self {
        LieroSettingsResult {
            status,
            settings: None,
            worm: None,
            bytes: Vec::new(),
            hash: LieroHash::default(),
            settings_view: None,
            worm_view: None,
        }
    }

    fn settings(mut self, s: Settings) -> Self {
        self.settings = Some(s);
        self
    }

    fn bytes(mut self, bytes: Vec<u8>) -> Self {
        self.bytes = bytes;
        self
    }

    fn hash(mut self, hash: liero_data::gash::Hash) -> Self {
        self.hash = LieroHash { value: hash };
        self
    }

    /// Boxes the result, with the views pointing into its own strings.
    fn finish(self) -> *mut LieroSettingsResult {
        let mut b = Box::new(self);
        b.settings_view = b.settings.as_ref().map(settings_view);
        b.worm_view = b.worm.as_ref().map(worm_view);
        Box::into_raw(b)
    }
}

fn archive_status(r: Result<(), ArchiveError>) -> LieroSettingsStatus {
    match r {
        Ok(()) => LieroSettingsStatus::Ok,
        Err(ArchiveError::Read) => LieroSettingsStatus::ReadError,
        Err(ArchiveError::Check) => LieroSettingsStatus::CheckError,
    }
}

/// # Safety
/// `data` must be valid for `len` bytes.
unsafe fn slice<'a>(data: *const u8, len: usize) -> &'a [u8] {
    unsafe {
        if len == 0 || data.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(data, len)
        }
    }
}

/// The default settings, as `Settings::Settings` makes them.
pub fn liero_settings_default() -> *mut LieroSettingsResult {
    LieroSettingsResult::new(LieroSettingsStatus::Ok)
        .settings(Settings::default())
        .finish()
}

/// Reads liero.cfg over `s`: Ok, Failed or MissingKey, with the settings
/// as far as they were read.
///
/// # Safety
/// `s`'s strings must be valid, and `data` valid for `len` bytes.
pub unsafe fn liero_settings_read_text(s: &LieroSettings, data: *const u8, len: usize) -> *mut LieroSettingsResult {
    unsafe {
        let mut settings = settings_from(s);
        let status = match data::read_text(&mut settings, slice(data, len)) {
            Ok(()) => LieroSettingsStatus::Ok,
            Err(TextError::Parse(_)) => LieroSettingsStatus::Failed,
            Err(TextError::MissingKey(_)) => LieroSettingsStatus::MissingKey,
        };
        LieroSettingsResult::new(status).settings(settings).finish()
    }
}

/// liero.cfg for `s`, in the result's bytes.
///
/// # Safety
/// `s`'s strings must be valid.
pub unsafe fn liero_settings_write_text(s: &LieroSettings) -> *mut LieroSettingsResult {
    unsafe {
        let bytes = data::write_text(&settings_from(s));
        LieroSettingsResult::new(LieroSettingsStatus::Ok).bytes(bytes).finish()
    }
}

/// Loads a profile from `node` over the settings, keeping their colour,
/// and remembers the profile (`WormSettings::loadProfile`). False, with
/// nothing changed, if the file can't be read. A profile that ends early
/// or fails its checkpoint is loaded as far as it was read, with its
/// colour: `status` says so, and C++ threw before putting the colour back.
pub fn liero_worm_settings_box_load_profile(
    s: &mut LieroWormSettingsBox,
    node: Option<&Node>,
    status: &mut LieroSettingsStatus,
) -> bool {
    let Some(node) = node else { return false };
    let Some(d) = node.read() else { return false };
    let mut worm = s.worm();
    *status = archive_status(data::read_worm(
        &mut worm,
        crate::replay::LIERO_REPLAY_VERSION,
        &mut Bytes::new(&d),
    ));
    let color = s.data.color;
    s.profile = Some(node.clone());
    s.set_worm(&worm);
    if matches!(status, LieroSettingsStatus::Ok) {
        s.data.color = color;
    }
    true
}

/// Saves the settings to `node` as a profile, in the current replay
/// version's archive, and remembers the profile
/// (`WormSettings::saveProfile`). False, with nothing changed, if the file
/// can't be written.
pub fn liero_worm_settings_box_save_profile(s: &mut LieroWormSettingsBox, node: Option<&Node>) -> bool {
    let Some(node) = node else { return false };
    // Opened before the archive is made, as `toOctetWriter` was.
    if node.prepare_write().is_none() {
        return false;
    }
    let mut worm = s.worm();
    let bytes = data::write_worm(&mut worm, crate::replay::LIERO_REPLAY_VERSION);
    if !node.write(&bytes) {
        return false;
    }
    s.profile = Some(node.clone());
    s.set_worm(&worm);
    true
}

/// Reads the settings archive over `s`: Ok, ReadError or CheckError.
///
/// # Safety
/// `s`'s strings must be valid, and `source.get` callable with its `ctx`.
pub unsafe fn liero_settings_read_archive(s: &LieroSettings, source: &LieroByteSource) -> *mut LieroSettingsResult {
    unsafe {
        let mut settings = settings_from(s);
        let status = archive_status(data::read_archive(&mut settings, &mut Hooked(source)));
        LieroSettingsResult::new(status).settings(settings).finish()
    }
}

/// The settings archive for `s` in the result's bytes, and the settings
/// after writing it (with the default TC name).
///
/// # Safety
/// `s`'s strings must be valid.
pub unsafe fn liero_settings_write_archive(s: &LieroSettings) -> *mut LieroSettingsResult {
    unsafe {
        let mut settings = settings_from(s);
        let bytes = data::write_archive(&mut settings);
        LieroSettingsResult::new(LieroSettingsStatus::Ok)
            .settings(settings)
            .bytes(bytes)
            .finish()
    }
}

/// `Settings::updateHash`: the hash, and the settings after it.
///
/// # Safety
/// `s`'s strings must be valid.
pub unsafe fn liero_settings_hash(s: &LieroSettings) -> *mut LieroSettingsResult {
    unsafe {
        let mut settings = settings_from(s);
        let hash = data::archive_hash(&mut settings);
        LieroSettingsResult::new(LieroSettingsStatus::Ok)
            .settings(settings)
            .hash(hash)
            .finish()
    }
}

/// # Safety
/// `r` must come from one of the functions above and not be freed.
pub unsafe fn liero_settings_result_status(r: &LieroSettingsResult) -> LieroSettingsStatus {
    r.status
}

/// The settings, or null if the call doesn't return any. Valid until
/// `liero_settings_result_free`.
///
/// # Safety
/// `r` must come from one of the functions above and not be freed.
pub unsafe fn liero_settings_result_settings(r: &LieroSettingsResult) -> *const LieroSettings {
    r.settings_view.as_ref().map_or(std::ptr::null(), |v| v)
}

/// The bytes written.
pub fn liero_settings_result_bytes(r: &LieroSettingsResult) -> &[u8] {
    &r.bytes
}

/// # Safety
/// `r` must come from one of the functions above and not be freed.
pub unsafe fn liero_settings_result_hash(r: &LieroSettingsResult) -> LieroHash {
    r.hash
}

/// # Safety
/// `r` must come from one of the functions above, or be null, and not be
/// used afterwards.
pub unsafe fn liero_settings_result_free(r: *mut LieroSettingsResult) {
    unsafe {
        if !r.is_null() {
            drop(Box::from_raw(r));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Owned(*mut LieroSettingsResult);

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { liero_settings_result_free(self.0) }
        }
    }

    impl Owned {
        fn r(&self) -> &LieroSettingsResult {
            unsafe { &*self.0 }
        }
        fn settings(&self) -> Settings {
            unsafe { settings_from(&*liero_settings_result_settings(self.r())) }
        }
        fn bytes(&self) -> Vec<u8> {
            liero_settings_result_bytes(self.r()).to_vec()
        }
    }

    #[test]
    fn control_counts_match_the_data() {
        assert_eq!(
            (LIERO_MAX_CONTROL, LIERO_MAX_CONTROL_EX),
            (data::MAX_CONTROL, data::MAX_CONTROL_EX)
        );
    }

    #[test]
    fn views_carry_every_field() {
        let mut s = Settings::default();
        s.ext.tc = b"tc".to_vec();
        s.level_file = b"level".to_vec();
        s.worms[1].name = b"name".to_vec();
        s.worms[1].controls_ex[7] = 9;
        let view = settings_view(&s);
        assert_eq!(unsafe { settings_from(&view) }, s);
    }

    #[test]
    fn text_goes_through_the_interface() {
        let s = Owned(liero_settings_default());
        let view = unsafe { *liero_settings_result_settings(s.r()) };
        let text = Owned(unsafe { liero_settings_write_text(&view) });
        let bytes = text.bytes();
        let read = Owned(unsafe { liero_settings_read_text(&view, bytes.as_ptr(), bytes.len()) });
        assert_eq!(
            unsafe { liero_settings_result_status(read.r()) },
            LieroSettingsStatus::Ok
        );
        assert_eq!(read.settings(), Settings::default());

        // Everything up to `lives`.
        let end = bytes.windows(7).position(|w| w == b"\r\nlives").unwrap();
        let missing = Owned(unsafe { liero_settings_read_text(&view, bytes.as_ptr(), end) });
        assert_eq!(
            unsafe { liero_settings_result_status(missing.r()) },
            LieroSettingsStatus::MissingKey
        );
    }

    #[test]
    fn archives_read_from_the_hook_and_report_its_failure() {
        struct Data {
            bytes: Vec<u8>,
            pos: usize,
        }
        unsafe fn get(ctx: *mut c_void) -> i32 {
            unsafe {
                let d = &mut *(ctx as *mut Data);
                let b = d.bytes.get(d.pos).map_or(-1, |&b| b as i32);
                d.pos += 1;
                b
            }
        }

        let s = Owned(liero_settings_default());
        let view = unsafe { *liero_settings_result_settings(s.r()) };
        let written = Owned(unsafe { liero_settings_write_archive(&view) });
        let mut data = Data {
            bytes: written.bytes(),
            pos: 0,
        };
        let source = LieroByteSource {
            ctx: &mut data as *mut Data as *mut c_void,
            get,
        };
        let read = Owned(unsafe { liero_settings_read_archive(&view, &source) });
        assert_eq!(
            unsafe { liero_settings_result_status(read.r()) },
            LieroSettingsStatus::Ok
        );
        assert_eq!(read.settings(), Settings::default());

        data.pos = 1;
        let short = Owned(unsafe { liero_settings_read_archive(&view, &source) });
        assert_eq!(
            unsafe { liero_settings_result_status(short.r()) },
            LieroSettingsStatus::ReadError
        );
    }

    #[test]
    fn hashes_match_the_bytes_written() {
        let s = Owned(liero_settings_default());
        let view = unsafe { *liero_settings_result_settings(s.r()) };
        let written = Owned(unsafe { liero_settings_write_archive(&view) });
        let hash = Owned(unsafe { liero_settings_hash(&view) });
        assert_eq!(
            unsafe { liero_settings_result_hash(hash.r()) }.value,
            liero_data::gash::gash(&written.bytes())
        );
    }
}

// ---------------------------------------------------------------------------
// The settings the game runs with, owned by Rust.
//
// C++'s `Settings` and `WormSettings` hold a box each and refer to the
// fields in it, so Rust code can work on the same settings directly. The
// text fields live beside the plain ones, behind functions.

/// The plain fields of the game's settings.
#[derive(Clone, Copy)]
pub struct LieroSettingsData {
    pub record_replays: bool,
    pub load_powerlevel_palette: bool,
    pub blood_particle_max: i32,
    pub ai_frames: i32,
    pub ai_mutations: i32,
    pub ai_traces: bool,
    pub ai_parallels: i32,
    pub fullscreen: bool,
    pub zone_timeout: i32,
    pub select_bot_weapons: u32,
    pub allow_viewing_spawn_point: bool,
    pub single_screen_replay: bool,
    pub spectator_window: bool,

    pub weap_table: [u32; 40],
    pub max_bonuses: i32,
    pub blood: i32,
    pub time_to_lose: i32,
    pub flags_to_win: i32,
    pub game_mode: u32,
    pub shadow: bool,
    pub load_change: bool,
    pub names_on_bonuses: bool,
    pub regenerate_level: bool,
    pub lives: i32,
    pub loading_time: i32,
    pub random_level: bool,
    pub map: bool,
    pub screen_sync: bool,
}

/// The plain fields of a worm's settings.
#[derive(Clone, Copy, Default)]
pub struct LieroWormSettingsData {
    pub health: i32,
    pub controller: u32,
    pub controls: [u32; LIERO_MAX_CONTROL],
    pub weapons: [u32; 5],
    pub rgb: [i32; 3],
    pub random_name: bool,
    pub color: i32,
    pub controls_ex: [u32; LIERO_MAX_CONTROL_EX],
}

/// The game's settings, with a reference to each player's worm settings,
/// which the game's worms share. A clone shares them too, as C++'s
/// `Settings::clone` did.
#[derive(Clone)]
pub struct LieroSettingsBox {
    pub data: LieroSettingsData,
    pub tc: Vec<u8>,
    pub level_file: Vec<u8>,
    pub worms: [SharedWormSettings; 2],
}

/// A worm's settings, shared by the settings that hold them, their clones
/// and the game's worms (C++'s `shared_ptr<WormSettings>`). The menus
/// change them while a game behind the menu plays with them.
pub type SharedWormSettings = Rc<RefCell<LieroWormSettingsBox>>;

impl LieroSettingsBox {
    /// The settings as the formats read and write them, with the worms'.
    pub(crate) fn settings(&self) -> Settings {
        let mut s = data_settings(self);
        s.worms = [self.worms[0].borrow().worm(), self.worms[1].borrow().worm()];
        s
    }

    /// Sets the settings and the worms' from `s`.
    pub(crate) fn set_settings(&mut self, s: &Settings) {
        let v = settings_view(s);
        let d = &mut self.data;
        d.record_replays = v.record_replays;
        d.load_powerlevel_palette = v.load_powerlevel_palette;
        d.blood_particle_max = v.blood_particle_max;
        d.ai_frames = v.ai_frames;
        d.ai_mutations = v.ai_mutations;
        d.ai_traces = v.ai_traces;
        d.ai_parallels = v.ai_parallels;
        d.fullscreen = v.fullscreen;
        d.zone_timeout = v.zone_timeout;
        d.select_bot_weapons = v.select_bot_weapons;
        d.allow_viewing_spawn_point = v.allow_viewing_spawn_point;
        d.single_screen_replay = v.single_screen_replay;
        d.spectator_window = v.spectator_window;
        d.weap_table = v.weap_table;
        d.max_bonuses = v.max_bonuses;
        d.blood = v.blood;
        d.time_to_lose = v.time_to_lose;
        d.flags_to_win = v.flags_to_win;
        d.game_mode = v.game_mode;
        d.shadow = v.shadow;
        d.load_change = v.load_change;
        d.names_on_bonuses = v.names_on_bonuses;
        d.regenerate_level = v.regenerate_level;
        d.lives = v.lives;
        d.loading_time = v.loading_time;
        d.random_level = v.random_level;
        d.map = v.map;
        d.screen_sync = v.screen_sync;
        self.tc = s.ext.tc.clone();
        self.level_file = s.level_file.clone();
        // The game's settings first: a worm's can be one of these, which
        // the caller may set after.
        self.worms[0].borrow_mut().set_worm(&s.worms[0]);
        self.worms[1].borrow_mut().set_worm(&s.worms[1]);
    }
}

impl LieroSettingsBox {
    /// New settings with the defaults, with one reference (`new Settings`).
    pub(crate) fn defaults() -> crate::shared::Shared<LieroSettingsBox> {
        // SAFETY: the new box's reference is taken over.
        let b = unsafe { crate::shared::Shared::adopt(liero_settings_box_new()) };
        // SAFETY: nothing else has the new box.
        unsafe { (*b.as_ptr()).set_settings(&Settings::default()) };
        b
    }

    /// Reads liero.cfg (or LIERO.DAT, if `legacy`) from `node` over the
    /// settings, as far as it reads (`Settings::load` and `loadLegacy`):
    /// None if the file can't be read, else whether it read whole.
    pub(crate) fn load(&mut self, node: &crate::fs::Node, legacy: bool) -> Option<LieroSettingsStatus> {
        let bytes = node.read()?;
        let mut settings = self.settings();
        let status = if legacy {
            if data::read_legacy(&mut settings, &bytes) {
                LieroSettingsStatus::Ok
            } else {
                LieroSettingsStatus::Failed
            }
        } else {
            match data::read_text(&mut settings, &bytes) {
                Ok(()) => LieroSettingsStatus::Ok,
                Err(TextError::Parse(_)) => LieroSettingsStatus::Failed,
                Err(TextError::MissingKey(_)) => LieroSettingsStatus::MissingKey,
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

/// The settings without the worms', which are left as the defaults.
fn data_settings(b: &LieroSettingsBox) -> Settings {
    let d = &b.data;
    let w = worm_view(&WormSettings::default());
    let v = LieroSettings {
        record_replays: d.record_replays,
        load_powerlevel_palette: d.load_powerlevel_palette,
        blood_particle_max: d.blood_particle_max,
        ai_frames: d.ai_frames,
        ai_mutations: d.ai_mutations,
        ai_traces: d.ai_traces,
        ai_parallels: d.ai_parallels,
        fullscreen: d.fullscreen,
        zone_timeout: d.zone_timeout,
        select_bot_weapons: d.select_bot_weapons,
        allow_viewing_spawn_point: d.allow_viewing_spawn_point,
        single_screen_replay: d.single_screen_replay,
        spectator_window: d.spectator_window,
        tc: str(&b.tc),
        weap_table: d.weap_table,
        max_bonuses: d.max_bonuses,
        blood: d.blood,
        time_to_lose: d.time_to_lose,
        flags_to_win: d.flags_to_win,
        game_mode: d.game_mode,
        shadow: d.shadow,
        load_change: d.load_change,
        names_on_bonuses: d.names_on_bonuses,
        regenerate_level: d.regenerate_level,
        lives: d.lives,
        loading_time: d.loading_time,
        random_level: d.random_level,
        level_file: str(&b.level_file),
        map: d.map,
        screen_sync: d.screen_sync,
        worms: [w, w],
    };
    // SAFETY: the strings are the box's and the default worm's, which live
    // through the call.
    unsafe { settings_from(&v) }
}

/// A worm's settings (`WormSettings`).
#[derive(Default)]
pub struct LieroWormSettingsBox {
    pub data: LieroWormSettingsData,
    pub name: Vec<u8>,
    /// The profile they were last loaded from or saved to, if any. A clone
    /// starts without one, as a new `WormSettings` did.
    pub profile: Option<crate::fs::Node>,
}

impl LieroWormSettingsBox {
    /// The settings as the archives read and write them.
    pub(crate) fn worm(&self) -> WormSettings {
        let d = &self.data;
        let view = LieroWormSettings {
            health: d.health,
            controller: d.controller,
            controls: d.controls,
            weapons: d.weapons,
            name: str(&self.name),
            rgb: d.rgb,
            random_name: d.random_name,
            color: d.color,
            controls_ex: d.controls_ex,
        };
        // SAFETY: the name is this box's.
        unsafe { worm_from(&view) }
    }

    pub(crate) fn set_worm(&mut self, w: &WormSettings) {
        let v = worm_view(w);
        self.data = LieroWormSettingsData {
            health: v.health,
            controller: v.controller,
            controls: v.controls,
            weapons: v.weapons,
            rgb: v.rgb,
            random_name: v.random_name,
            color: v.color,
            controls_ex: v.controls_ex,
        };
        self.name = w.name.clone();
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
        liero_worm_settings_box_save_profile(self, node.as_ref())
    }
}

impl Clone for LieroWormSettingsBox {
    fn clone(&self) -> Self {
        LieroWormSettingsBox {
            data: self.data,
            name: self.name.clone(),
            profile: None,
        }
    }
}

impl Default for LieroSettingsData {
    fn default() -> Self {
        LieroSettingsData {
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
            map: false,
            screen_sync: false,
        }
    }
}

/// Settings with every field zero or empty, and the worms' too; C++ then
/// sets the defaults.
pub fn liero_settings_box_new() -> *mut LieroSettingsBox {
    crate::shared::new_shared(LieroSettingsBox {
        data: LieroSettingsData::default(),
        tc: Vec::new(),
        level_file: Vec::new(),
        worms: [new_worm_settings(), new_worm_settings()],
    })
}

/// Player `i`'s worm settings.
pub fn liero_settings_box_worm(s: &LieroSettingsBox, i: usize) -> SharedWormSettings {
    s.worms[i].clone()
}

/// Makes player `i`'s worm settings `w`.
pub fn liero_settings_box_set_worm(s: &mut LieroSettingsBox, i: usize, w: SharedWormSettings) {
    s.worms[i] = w;
}

pub fn liero_settings_box_clone(s: &LieroSettingsBox) -> *mut LieroSettingsBox {
    crate::shared::new_shared(s.clone())
}

/// Gives up a reference to `s` (from `liero_settings_box_new` or `_clone`,
/// or a game's), freeing it with the last.
///
/// # Safety
/// `s` has a reference, or is null, and this reference isn't used again.
pub unsafe fn liero_settings_box_free(s: *mut LieroSettingsBox) {
    unsafe {
        if !s.is_null() {
            crate::shared::release_shared(s);
        }
    }
}

/// The plain fields, which stay where they are for the box's life.
pub fn liero_settings_box_data(s: &mut LieroSettingsBox) -> *mut LieroSettingsData {
    &mut s.data
}

/// A worm's settings with every field zero or empty.
pub fn new_worm_settings() -> SharedWormSettings {
    Rc::default()
}

/// A copy of `s`, shared by no one yet.
pub fn clone_worm_settings(s: &LieroWormSettingsBox) -> SharedWormSettings {
    Rc::new(RefCell::new(s.clone()))
}

#[cfg(test)]
mod box_tests {
    use super::*;

    #[test]
    fn a_clone_shares_the_worms() {
        unsafe {
            let a = liero_settings_box_new();
            let w0 = liero_settings_box_worm(&*a, 0);
            assert!(!Rc::ptr_eq(&w0, &liero_settings_box_worm(&*a, 1)));
            let b = liero_settings_box_clone(&*a);
            assert!(Rc::ptr_eq(&liero_settings_box_worm(&*b, 0), &w0));
            // `a`, `b` and `w0`.
            assert_eq!(Rc::strong_count(&w0), 3);

            // Setting one is the box's own.
            let w = new_worm_settings();
            liero_settings_box_set_worm(&mut *b, 0, w.clone());
            assert_eq!((Rc::strong_count(&w), Rc::strong_count(&w0)), (2, 2));
            assert!(Rc::ptr_eq(&liero_settings_box_worm(&*a, 0), &w0));
            liero_settings_box_free(b);
            assert_eq!(Rc::strong_count(&w), 1);
            liero_settings_box_free(a);
            assert_eq!(Rc::strong_count(&w0), 1);
        }
    }

    #[test]
    fn a_clone_is_separate() {
        let a = liero_settings_box_new();
        unsafe {
            (*liero_settings_box_data(&mut *a)).lives = 3;
            (*a).tc = b"openliero".to_vec();
            let b = liero_settings_box_clone(&*a);
            (*liero_settings_box_data(&mut *a)).lives = 5;
            (*a).tc = b"other".to_vec();
            assert_eq!((*b).data.lives, 3);
            assert_eq!((*b).tc, b"openliero");
            liero_settings_box_free(a);
            liero_settings_box_free(b);
        }
    }

    #[test]
    fn worm_name_and_fields() {
        let w = new_worm_settings();
        w.borrow_mut().data.health = 7;
        w.borrow_mut().name = b"BOT A".to_vec();
        let c = clone_worm_settings(&w.borrow());
        assert_eq!(c.borrow().name, b"BOT A");
        assert_eq!(c.borrow().data.health, 7);
    }

    #[test]
    fn profiles_are_remembered() {
        // Relative, as the game's nodes take paths: under the crate's target.
        let dir = std::path::PathBuf::from(format!("target/liero-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.lpf");
        let path_bytes = path.to_str().unwrap().as_bytes().to_vec();
        let w = LieroWormSettingsBox::default();
        {
            let mut w = w;
            let w = &mut w;
            assert!(!w.save_to_profile(), "no profile to save over");
            w.name = b"WORMY".to_vec();
            w.data.health = 50;
            w.data.color = 3;

            let node = Node::new(&path_bytes);
            let path_bytes = node.full_path().to_vec();
            assert!(liero_worm_settings_box_save_profile(w, Some(&node)));
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.profile = None;
            w.data.health = 1;
            w.data.color = 9;
            let mut status = LieroSettingsStatus::Failed;
            assert!(liero_worm_settings_box_load_profile(w, Some(&node), &mut status));
            assert!(matches!(status, LieroSettingsStatus::Ok));
            assert_eq!(
                (w.data.health, w.data.color, &w.name[..]),
                (50, 9, &b"WORMY"[..]),
                "the colour stays"
            );
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.data.health = 2;
            assert!(w.save_to_profile(), "saved over the profile");
            w.data.health = 3;
            assert!(liero_worm_settings_box_load_profile(w, Some(&node), &mut status));
            assert_eq!(w.data.health, 2);

            let missing = Node::new(b"/no/such/dir/x.lpf");
            assert!(!liero_worm_settings_box_load_profile(w, Some(&missing), &mut status));
            assert!(!liero_worm_settings_box_save_profile(w, None));
            assert_eq!(w.profile_path(), Some(path_bytes), "unchanged by failures");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_clone_has_no_profile() {
        let node = Node::new(b"a/b.lpf");
        let mut w = LieroWormSettingsBox {
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
