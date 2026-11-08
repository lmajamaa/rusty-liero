//! C interface to the settings formats in `liero_data::settings`, and the
//! settings the game runs with (`LieroSettingsBox`, at the end).
//!
//! The C++ `Settings` and `WormSettings` refer to fields Rust owns. To
//! read or write them, C++ passes a `LieroSettings` or `LieroWormSettings`
//! view of its fields, and gets back a `LieroSettingsResult` holding the
//! settings afterwards (reads change them, and so do some writes), the bytes
//! written or the hash, and whether it worked. C++ copies what it needs out
//! of the result and frees it.

use crate::text::LieroStr;
use crate::fs::LieroFsNode;
use liero_data::settings::{self as data, ArchiveError, Bytes, Extensions, ReadError, Settings, Source, TextError, WormSettings};
use std::ffi::c_void;

/// Up, down, left, right, fire, change and jump.
pub const LIERO_MAX_CONTROL: usize = 7;
/// The same and dig.
pub const LIERO_MAX_CONTROL_EX: usize = 8;

#[repr(C)]
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

#[repr(C)]
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
/// cbindgen:prefix-with-name
#[repr(C)]
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
#[repr(C)]
pub struct LieroByteSource {
    pub ctx: *mut c_void,
    pub get: unsafe extern "C" fn(ctx: *mut c_void) -> i32,
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
#[repr(C)]
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
    LieroStr { ptr: s.as_ptr(), len: s.len() }
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

/// # Safety
/// The view's strings must be valid.
pub(crate) unsafe fn settings_from(v: &LieroSettings) -> Settings {
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

    fn worm(mut self, ws: WormSettings) -> Self {
        self.worm = Some(ws);
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
    if len == 0 || data.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(data, len)
    }
}

/// The default settings, as `Settings::Settings` makes them.
#[no_mangle]
pub extern "C" fn liero_settings_default() -> *mut LieroSettingsResult {
    LieroSettingsResult::new(LieroSettingsStatus::Ok).settings(Settings::default()).finish()
}

/// The default worm settings, as `WormSettings::WormSettings` makes them.
#[no_mangle]
pub extern "C" fn liero_worm_settings_default() -> *mut LieroSettingsResult {
    LieroSettingsResult::new(LieroSettingsStatus::Ok).worm(WormSettings::default()).finish()
}

/// Reads liero.cfg over `s`: Ok, Failed or MissingKey, with the settings
/// as far as they were read.
///
/// # Safety
/// `s`'s strings must be valid, and `data` valid for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_read_text(s: &LieroSettings, data: *const u8, len: usize) -> *mut LieroSettingsResult {
    let mut settings = settings_from(s);
    let status = match data::read_text(&mut settings, slice(data, len)) {
        Ok(()) => LieroSettingsStatus::Ok,
        Err(TextError::Parse(_)) => LieroSettingsStatus::Failed,
        Err(TextError::MissingKey(_)) => LieroSettingsStatus::MissingKey,
    };
    LieroSettingsResult::new(status).settings(settings).finish()
}

/// liero.cfg for `s`, in the result's bytes.
///
/// # Safety
/// `s`'s strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_write_text(s: &LieroSettings) -> *mut LieroSettingsResult {
    let bytes = data::write_text(&settings_from(s));
    LieroSettingsResult::new(LieroSettingsStatus::Ok).bytes(bytes).finish()
}

/// Reads LIERO.DAT over `s`: Ok, or Failed if it ends early.
///
/// # Safety
/// `s`'s strings must be valid, and `data` valid for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_read_legacy(s: &LieroSettings, data: *const u8, len: usize) -> *mut LieroSettingsResult {
    let mut settings = settings_from(s);
    let status = if data::read_legacy(&mut settings, slice(data, len)) {
        LieroSettingsStatus::Ok
    } else {
        LieroSettingsStatus::Failed
    };
    LieroSettingsResult::new(status).settings(settings).finish()
}

/// Reads liero.cfg (or LIERO.DAT, if `legacy`) from `node` over `s`, as
/// `liero_settings_read_text` or `liero_settings_read_legacy` would. Null if
/// the file can't be read.
///
/// # Safety
/// `s`'s strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_load(s: &LieroSettings, node: &LieroFsNode, legacy: bool) -> *mut LieroSettingsResult {
    match node.0.read() {
        Some(d) if legacy => liero_settings_read_legacy(s, d.as_ptr(), d.len()),
        Some(d) => liero_settings_read_text(s, d.as_ptr(), d.len()),
        None => std::ptr::null_mut(),
    }
}

/// Writes `s` to `node` as liero.cfg. False if the file can't be written.
///
/// # Safety
/// `s`'s strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_save(s: &LieroSettings, node: &LieroFsNode) -> bool {
    node.0.write(&data::write_text(&settings_from(s)))
}

/// Loads a profile from `node` over the settings, keeping their colour,
/// and remembers the profile (`WormSettings::loadProfile`). False, with
/// nothing changed, if the file can't be read. A profile that ends early
/// or fails its checkpoint is loaded as far as it was read, with its
/// colour: `status` says so, and C++ threw before putting the colour back.
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_load_profile(
    s: &mut LieroWormSettingsBox,
    node: Option<&LieroFsNode>,
    status: &mut LieroSettingsStatus,
) -> bool {
    let Some(node) = node else { return false };
    let Some(d) = node.0.read() else { return false };
    let mut worm = s.worm();
    *status = archive_status(data::read_worm(&mut worm, crate::replay::LIERO_REPLAY_VERSION, &mut Bytes::new(&d)));
    let color = s.data.color;
    s.profile = Some(node.0.clone());
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
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_save_profile(s: &mut LieroWormSettingsBox, node: Option<&LieroFsNode>) -> bool {
    let Some(node) = node else { return false };
    // Opened before the archive is made, as `toOctetWriter` was.
    if node.0.prepare_write().is_none() {
        return false;
    }
    let mut worm = s.worm();
    let bytes = data::write_worm(&mut worm, crate::replay::LIERO_REPLAY_VERSION);
    if !node.0.write(&bytes) {
        return false;
    }
    s.profile = Some(node.0.clone());
    s.set_worm(&worm);
    true
}

/// Reads the settings archive over `s`: Ok, ReadError or CheckError.
///
/// # Safety
/// `s`'s strings must be valid, and `source.get` callable with its `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_read_archive(s: &LieroSettings, source: &LieroByteSource) -> *mut LieroSettingsResult {
    let mut settings = settings_from(s);
    let status = archive_status(data::read_archive(&mut settings, &mut Hooked(source)));
    LieroSettingsResult::new(status).settings(settings).finish()
}

/// The settings archive for `s` in the result's bytes, and the settings
/// after writing it (with the default TC name).
///
/// # Safety
/// `s`'s strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_write_archive(s: &LieroSettings) -> *mut LieroSettingsResult {
    let mut settings = settings_from(s);
    let bytes = data::write_archive(&mut settings);
    LieroSettingsResult::new(LieroSettingsStatus::Ok).settings(settings).bytes(bytes).finish()
}

/// `Settings::updateHash`: the hash, and the settings after it.
///
/// # Safety
/// `s`'s strings must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_hash(s: &LieroSettings) -> *mut LieroSettingsResult {
    let mut settings = settings_from(s);
    let hash = data::archive_hash(&mut settings);
    LieroSettingsResult::new(LieroSettingsStatus::Ok).settings(settings).hash(hash).finish()
}

/// Reads a worm's archive for `replay_version` over `ws`: Ok or ReadError.
///
/// # Safety
/// `ws`'s name must be valid, and `source.get` callable with its `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_read_archive(
    ws: &LieroWormSettings,
    replay_version: i32,
    source: &LieroByteSource,
) -> *mut LieroSettingsResult {
    let mut worm = worm_from(ws);
    let status = archive_status(data::read_worm(&mut worm, replay_version, &mut Hooked(source)));
    LieroSettingsResult::new(status).worm(worm).finish()
}

/// A worm's archive for `replay_version` in the result's bytes, and the
/// worm settings after writing it.
///
/// # Safety
/// `ws`'s name must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_write_archive(ws: &LieroWormSettings, replay_version: i32) -> *mut LieroSettingsResult {
    let mut worm = worm_from(ws);
    let bytes = data::write_worm(&mut worm, replay_version);
    LieroSettingsResult::new(LieroSettingsStatus::Ok).worm(worm).bytes(bytes).finish()
}

/// `WormSettings::updateHash`: the hash, and the worm settings after it.
///
/// # Safety
/// `ws`'s name must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_hash(ws: &LieroWormSettings) -> *mut LieroSettingsResult {
    let mut worm = worm_from(ws);
    let hash = data::worm_hash(&mut worm);
    LieroSettingsResult::new(LieroSettingsStatus::Ok).worm(worm).hash(hash).finish()
}

/// # Safety
/// `r` must come from one of the functions above and not be freed.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_status(r: &LieroSettingsResult) -> LieroSettingsStatus {
    r.status
}

/// The settings, or null if the call doesn't return any. Valid until
/// `liero_settings_result_free`.
///
/// # Safety
/// `r` must come from one of the functions above and not be freed.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_settings(r: &LieroSettingsResult) -> *const LieroSettings {
    r.settings_view.as_ref().map_or(std::ptr::null(), |v| v)
}

/// The worm settings, or null if the call doesn't return any. Valid until
/// `liero_settings_result_free`.
///
/// # Safety
/// `r` must come from one of the functions above and not be freed.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_worm(r: &LieroSettingsResult) -> *const LieroWormSettings {
    r.worm_view.as_ref().map_or(std::ptr::null(), |v| v)
}

/// The bytes written. Valid until `liero_settings_result_free`.
///
/// # Safety
/// `r` must come from one of the functions above and not be freed.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_bytes(r: &LieroSettingsResult) -> LieroStr {
    str(&r.bytes)
}

/// # Safety
/// `r` must come from one of the functions above and not be freed.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_hash(r: &LieroSettingsResult) -> LieroHash {
    r.hash
}

/// # Safety
/// `r` must come from one of the functions above, or be null, and not be
/// used afterwards.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_result_free(r: *mut LieroSettingsResult) {
    if !r.is_null() {
        drop(Box::from_raw(r));
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
            unsafe { liero_settings_result_bytes(self.r()).bytes().to_vec() }
        }
    }

    #[test]
    fn control_counts_match_the_data() {
        assert_eq!((LIERO_MAX_CONTROL, LIERO_MAX_CONTROL_EX), (data::MAX_CONTROL, data::MAX_CONTROL_EX));
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
        assert_eq!(unsafe { liero_settings_result_status(read.r()) }, LieroSettingsStatus::Ok);
        assert_eq!(read.settings(), Settings::default());

        // Everything up to `lives`.
        let end = bytes.windows(7).position(|w| w == b"\r\nlives").unwrap();
        let missing = Owned(unsafe { liero_settings_read_text(&view, bytes.as_ptr(), end) });
        assert_eq!(unsafe { liero_settings_result_status(missing.r()) }, LieroSettingsStatus::MissingKey);
    }

    #[test]
    fn archives_read_from_the_hook_and_report_its_failure() {
        struct Data {
            bytes: Vec<u8>,
            pos: usize,
        }
        unsafe extern "C" fn get(ctx: *mut c_void) -> i32 {
            let d = &mut *(ctx as *mut Data);
            let b = d.bytes.get(d.pos).map_or(-1, |&b| b as i32);
            d.pos += 1;
            b
        }

        let s = Owned(liero_settings_default());
        let view = unsafe { *liero_settings_result_settings(s.r()) };
        let written = Owned(unsafe { liero_settings_write_archive(&view) });
        let mut data = Data { bytes: written.bytes(), pos: 0 };
        let source = LieroByteSource { ctx: &mut data as *mut Data as *mut c_void, get };
        let read = Owned(unsafe { liero_settings_read_archive(&view, &source) });
        assert_eq!(unsafe { liero_settings_result_status(read.r()) }, LieroSettingsStatus::Ok);
        assert_eq!(read.settings(), Settings::default());

        data.pos = 1;
        let short = Owned(unsafe { liero_settings_read_archive(&view, &source) });
        assert_eq!(unsafe { liero_settings_result_status(short.r()) }, LieroSettingsStatus::ReadError);
    }

    #[test]
    fn hashes_match_the_bytes_written() {
        let s = Owned(liero_settings_default());
        let view = unsafe { *liero_settings_result_settings(s.r()) };
        let written = Owned(unsafe { liero_settings_write_archive(&view) });
        let hash = Owned(unsafe { liero_settings_hash(&view) });
        assert_eq!(unsafe { liero_settings_result_hash(hash.r()) }.value, liero_data::gash::gash(&written.bytes()));
    }
}

// ---------------------------------------------------------------------------
// The settings the game runs with, owned by Rust.
//
// C++'s `Settings` and `WormSettings` hold a box each and refer to the
// fields in it, so Rust code can work on the same settings directly. The
// text fields live beside the plain ones, behind functions.

/// The plain fields of the game's settings.
#[repr(C)]
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
#[repr(C)]
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

/// The game's settings, without the worms', which C++ keeps apart since a
/// worm holds on to its own.
#[derive(Clone)]
pub struct LieroSettingsBox {
    pub data: LieroSettingsData,
    pub tc: Vec<u8>,
    pub level_file: Vec<u8>,
}

/// A worm's settings, shared by the C++ `WormSettings` that refer to them
/// and the game's worms: each holds a reference (`_retain` and `_free`).
pub struct LieroWormSettingsBox {
    pub data: LieroWormSettingsData,
    pub name: Vec<u8>,
    /// The profile they were last loaded from or saved to, if any. A clone
    /// starts without one, as a new `WormSettings` did.
    pub profile: Option<crate::fs::Node>,
    refs: usize,
}

impl Default for LieroWormSettingsBox {
    fn default() -> Self {
        LieroWormSettingsBox { data: LieroWormSettingsData::default(), name: Vec::new(), profile: None, refs: 1 }
    }
}

impl LieroWormSettingsBox {
    /// The settings as the archives read and write them.
    fn worm(&self) -> WormSettings {
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

    fn set_worm(&mut self, w: &WormSettings) {
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
        self.profile.as_ref().filter(|n| !n.is_null()).map(|n| n.full_path().to_vec())
    }

    /// Saves the settings over their profile (the menus' "save profile").
    pub(crate) fn save_to_profile(&mut self) -> bool {
        let node = self.profile.clone().map(crate::fs::LieroFsNode);
        liero_worm_settings_box_save_profile(self, node.as_ref())
    }
}

impl Clone for LieroWormSettingsBox {
    fn clone(&self) -> Self {
        LieroWormSettingsBox { data: self.data, name: self.name.clone(), profile: None, refs: 1 }
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

fn set_bytes(dest: &mut Vec<u8>, text: LieroStr) {
    // SAFETY: the caller passes a valid string.
    *dest = unsafe { text.bytes() }.to_vec();
}

/// Settings with every field zero or empty; C++ then sets the defaults.
#[no_mangle]
pub extern "C" fn liero_settings_box_new() -> *mut LieroSettingsBox {
    Box::into_raw(Box::new(LieroSettingsBox { data: LieroSettingsData::default(), tc: Vec::new(), level_file: Vec::new() }))
}

#[no_mangle]
pub extern "C" fn liero_settings_box_clone(s: &LieroSettingsBox) -> *mut LieroSettingsBox {
    Box::into_raw(Box::new(s.clone()))
}

/// # Safety
/// `s` comes from `liero_settings_box_new` or `_clone` and isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_box_free(s: *mut LieroSettingsBox) {
    if !s.is_null() {
        drop(Box::from_raw(s));
    }
}

/// The plain fields, which stay where they are for the box's life.
#[no_mangle]
pub extern "C" fn liero_settings_box_data(s: &mut LieroSettingsBox) -> *mut LieroSettingsData {
    &mut s.data
}

/// The TC's name, valid until it's set.
#[no_mangle]
pub extern "C" fn liero_settings_box_tc(s: &LieroSettingsBox) -> LieroStr {
    str(&s.tc)
}

/// # Safety
/// `text` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_box_set_tc(s: &mut LieroSettingsBox, text: LieroStr) {
    set_bytes(&mut s.tc, text);
}

/// The level file, valid until it's set.
#[no_mangle]
pub extern "C" fn liero_settings_box_level_file(s: &LieroSettingsBox) -> LieroStr {
    str(&s.level_file)
}

/// # Safety
/// `text` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_settings_box_set_level_file(s: &mut LieroSettingsBox, text: LieroStr) {
    set_bytes(&mut s.level_file, text);
}

/// A worm's settings with every field zero or empty.
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_new() -> *mut LieroWormSettingsBox {
    Box::into_raw(Box::default())
}

#[no_mangle]
pub extern "C" fn liero_worm_settings_box_clone(s: &LieroWormSettingsBox) -> *mut LieroWormSettingsBox {
    Box::into_raw(Box::new(s.clone()))
}

/// Another reference to `s`, which is freed when the last one is.
///
/// # Safety
/// `s` comes from `liero_worm_settings_box_new` or `_clone` and hasn't been
/// freed.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_box_retain(s: *mut LieroWormSettingsBox) -> *mut LieroWormSettingsBox {
    (*s).refs += 1;
    s
}

/// Drops a reference to `s`, freeing it with the last one.
///
/// # Safety
/// `s` comes from `liero_worm_settings_box_new`, `_clone` or `_retain`, or
/// is null, and that reference isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_box_free(s: *mut LieroWormSettingsBox) {
    if !s.is_null() {
        (*s).refs -= 1;
        if (*s).refs == 0 {
            drop(Box::from_raw(s));
        }
    }
}

/// A new node for the settings' profile, or null with none.
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_profile(s: &LieroWormSettingsBox) -> *mut LieroFsNode {
    match &s.profile {
        Some(n) => Box::into_raw(Box::new(LieroFsNode(n.clone()))),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn liero_worm_settings_box_set_profile(s: &mut LieroWormSettingsBox, node: Option<&LieroFsNode>) {
    s.profile = node.map(|n| n.0.clone());
}

/// The plain fields, which stay where they are for the box's life.
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_data(s: &mut LieroWormSettingsBox) -> *mut LieroWormSettingsData {
    &mut s.data
}

/// The worm's name, valid until it's set.
#[no_mangle]
pub extern "C" fn liero_worm_settings_box_name(s: &LieroWormSettingsBox) -> LieroStr {
    str(&s.name)
}

/// # Safety
/// `text` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_worm_settings_box_set_name(s: &mut LieroWormSettingsBox, text: LieroStr) {
    set_bytes(&mut s.name, text);
}

#[cfg(test)]
mod box_tests {
    use super::*;

    fn s(b: &[u8]) -> LieroStr {
        str(b)
    }

    #[test]
    fn a_clone_is_separate() {
        let a = liero_settings_box_new();
        unsafe {
            (*liero_settings_box_data(&mut *a)).lives = 3;
            liero_settings_box_set_tc(&mut *a, s(b"openliero"));
            let b = liero_settings_box_clone(&*a);
            (*liero_settings_box_data(&mut *a)).lives = 5;
            liero_settings_box_set_tc(&mut *a, s(b"other"));
            assert_eq!((*b).data.lives, 3);
            assert_eq!(liero_settings_box_tc(&*b).bytes(), b"openliero");
            liero_settings_box_free(a);
            liero_settings_box_free(b);
        }
    }

    #[test]
    fn worm_name_and_fields() {
        let w = liero_worm_settings_box_new();
        unsafe {
            (*liero_worm_settings_box_data(&mut *w)).health = 7;
            liero_worm_settings_box_set_name(&mut *w, s(b"BOT A"));
            let c = liero_worm_settings_box_clone(&*w);
            assert_eq!(liero_worm_settings_box_name(&*c).bytes(), b"BOT A");
            assert_eq!((*c).data.health, 7);
            liero_worm_settings_box_free(w);
            liero_worm_settings_box_free(c);
        }
    }

    #[test]
    fn profiles_are_remembered() {
        // Relative, as the game's nodes take paths: under the crate's target.
        let dir = std::path::PathBuf::from(format!("target/liero-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.lpf");
        let path_bytes = path.to_str().unwrap().as_bytes().to_vec();
        let w = liero_worm_settings_box_new();
        unsafe {
            let w = &mut *w;
            assert!(!w.save_to_profile(), "no profile to save over");
            w.name = b"WORMY".to_vec();
            w.data.health = 50;
            w.data.color = 3;

            let node = crate::fs::liero_fs_node_new(s(&path_bytes));
            let path_bytes = (*node).0.full_path().to_vec();
            assert!(liero_worm_settings_box_save_profile(w, Some(&*node)));
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.profile = None;
            w.data.health = 1;
            w.data.color = 9;
            let mut status = LieroSettingsStatus::Failed;
            assert!(liero_worm_settings_box_load_profile(w, Some(&*node), &mut status));
            assert!(matches!(status, LieroSettingsStatus::Ok));
            assert_eq!((w.data.health, w.data.color, &w.name[..]), (50, 9, &b"WORMY"[..]), "the colour stays");
            assert_eq!(w.profile_path(), Some(path_bytes.clone()));

            w.data.health = 2;
            assert!(w.save_to_profile(), "saved over the profile");
            w.data.health = 3;
            assert!(liero_worm_settings_box_load_profile(w, Some(&*node), &mut status));
            assert_eq!(w.data.health, 2);

            let missing = crate::fs::liero_fs_node_new(s(b"/no/such/dir/x.lpf"));
            assert!(!liero_worm_settings_box_load_profile(w, Some(&*missing), &mut status));
            assert!(!liero_worm_settings_box_save_profile(w, None));
            assert_eq!(w.profile_path(), Some(path_bytes), "unchanged by failures");
            crate::fs::liero_fs_node_free(node);
            crate::fs::liero_fs_node_free(missing);
            liero_worm_settings_box_free(w);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shared_until_the_last_reference() {
        let w = liero_worm_settings_box_new();
        unsafe {
            let node = crate::fs::liero_fs_node_new(s(b"a/b.lpf"));
            liero_worm_settings_box_set_profile(&mut *w, Some(&*node));
            let again = liero_worm_settings_box_retain(w);
            liero_worm_settings_box_free(w);
            (*liero_worm_settings_box_data(&mut *again)).health = 3;
            assert_eq!((*again).refs, 1);
            assert!(!liero_worm_settings_box_profile(&*again).is_null());
            let c = liero_worm_settings_box_clone(&*again);
            assert!(liero_worm_settings_box_profile(&*c).is_null(), "a clone has no profile");
            assert_eq!((*c).data.health, 3);
            assert_eq!((*again).profile_path().as_deref(), Some((*node).0.full_path()));
            (*again).profile = Some(crate::fs::Node::default());
            assert_eq!((*again).profile_path(), None, "a null node isn't a profile");
            liero_worm_settings_box_set_profile(&mut *again, None);
            assert!(liero_worm_settings_box_profile(&*again).is_null(), "cleared");
            liero_worm_settings_box_free(again);
            liero_worm_settings_box_free(c);
            crate::fs::liero_fs_node_free(node);
        }
    }
}
