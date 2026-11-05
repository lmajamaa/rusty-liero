//! Game settings and the formats they are kept in, ported from
//! src/game/settings.hpp and the `WormSettings` archive in src/game/worm.hpp:
//!
//! - liero.cfg, a TOML file (`read_text`, `write_text`)
//! - the original game's LIERO.DAT (`read_legacy`)
//! - the binary archives in replays and .lpf profiles (`read_archive`,
//!   `write_archive`, `read_worm`, `write_worm`) and their gash hashes
//!
//! Reads update the settings field by field, as the C++ archives did, so a
//! read that fails part way leaves the fields before the failure changed.
//! Writes assert that 8- and 16-bit fields fit, as the C++ build's `sassert`
//! does.

use crate::gash::{gash, Hash};
use crate::toml_reader::Reader;
use crate::toml_writer::Writer;

/// The extension version the settings archive is written with
/// (`Extensions::myVersion`).
pub const ARCHIVE_VERSION: i32 = 9;
/// The version worm settings archives are written with (`myGameVersion`).
pub const GAME_VERSION: i32 = 10;
/// The replay version the game writes (`myReplayVersion`), and the one worm
/// settings hashes use.
pub const REPLAY_VERSION: i32 = 4;

pub const MAX_CONTROL: usize = 7;
pub const MAX_CONTROL_EX: usize = 8;

/// The settings the original LIERO.DAT didn't have. A LIERO.DAT whose
/// extensions don't read gets these defaults back.
#[derive(Clone, Debug, PartialEq)]
pub struct Extensions {
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
    pub tc: Vec<u8>,
}

impl Default for Extensions {
    fn default() -> Self {
        Extensions {
            record_replays: true,
            load_powerlevel_palette: true,
            blood_particle_max: 700,
            ai_frames: 70 * 2,
            ai_mutations: 2,
            ai_traces: false,
            ai_parallels: 3,
            fullscreen: false,
            zone_timeout: 30,
            select_bot_weapons: 1,
            allow_viewing_spawn_point: false,
            single_screen_replay: false,
            spectator_window: false,
            tc: b"openliero".to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WormSettings {
    pub health: i32,
    /// 0 for a human, 1 and 2 for the bots.
    pub controller: u32,
    /// DOS scan codes for up, down, left, right, fire, change and jump, as
    /// LIERO.DAT and the old archives store them.
    pub controls: [u32; MAX_CONTROL],
    pub weapons: [u32; 5],
    pub name: Vec<u8>,
    pub rgb: [i32; 3],
    pub random_name: bool,
    pub color: i32,
    /// The controls the game uses: the seven above and dig.
    pub controls_ex: [u32; MAX_CONTROL_EX],
}

impl Default for WormSettings {
    fn default() -> Self {
        WormSettings {
            health: 100,
            controller: 0,
            controls: [0; MAX_CONTROL],
            weapons: [1; 5],
            name: Vec::new(),
            rgb: [26, 26, 62],
            random_name: true,
            color: 0,
            controls_ex: [0; MAX_CONTROL_EX],
        }
    }
}

impl WormSettings {
    /// `controls[c]`. The C++ archives read `controls[7]` for dig, one past
    /// the end, which is `weapons[0]` in the struct's layout.
    fn control_or_weapon(&self, c: usize) -> u32 {
        if c < MAX_CONTROL {
            self.controls[c]
        } else {
            self.weapons[c - MAX_CONTROL]
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub ext: Extensions,
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
    pub level_file: Vec<u8>,
    pub map: bool,
    pub screen_sync: bool,
    pub worms: [WormSettings; 2],
}

impl Default for Settings {
    fn default() -> Self {
        // DOS scan codes for up, down, left, right, fire, change and jump.
        // Player 1: R, F, D, G, Left Ctrl, Left Shift, Left Alt.
        // Player 2: I, K, J, L, Slash, Right Alt, Right Shift. The original
        // arrows and Right Ctrl don't work on a Mac: there is no Right Ctrl,
        // and with player 1 holding Left Ctrl, Ctrl+arrows switch Spaces.
        const CONTROLS: [[u32; MAX_CONTROL]; 2] =
            [[0x13, 0x21, 0x20, 0x22, 0x1D, 0x2A, 0x38], [0x17, 0x25, 0x24, 0x26, 0x35, 0x90, 0x36]];
        const RGB: [[i32; 3]; 2] = [[26, 26, 63], [15, 43, 15]];
        const COLORS: [i32; 2] = [32, 41];

        let worms = [0, 1].map(|i| {
            let mut ws = WormSettings { color: COLORS[i], rgb: RGB[i], controls: CONTROLS[i], ..Default::default() };
            ws.controls_ex[..MAX_CONTROL].copy_from_slice(&CONTROLS[i]);
            ws
        });

        Settings {
            ext: Extensions::default(),
            weap_table: [0; 40],
            max_bonuses: 4,
            blood: 100,
            time_to_lose: 600,
            flags_to_win: 20,
            game_mode: 0,
            shadow: true,
            load_change: true,
            names_on_bonuses: false,
            regenerate_level: false,
            lives: 15,
            loading_time: 100,
            random_level: true,
            level_file: Vec::new(),
            map: true,
            screen_sync: true,
            worms,
        }
    }
}

/// `limit<0, H>` for unsigned values.
fn limit(v: u32, h: u32) -> u32 {
    if v >= h {
        h - 1
    } else {
        v
    }
}

// liero.cfg

/// Why liero.cfg didn't load.
#[derive(Clone, Debug, PartialEq)]
pub enum TextError {
    /// Not TOML, or a value of the wrong type or an array of the wrong
    /// length (`gvl::toml::parse_error`).
    Parse(String),
    /// A setting is missing. The C++ reader threw `std::out_of_range` from
    /// `std::map::at` for this, which `Settings::load` didn't catch.
    MissingKey(String),
}

/// Reads liero.cfg into `s`, in the order `archive_text` did. Fields that
/// were read before an error keep their new values.
pub fn read_text(s: &mut Settings, data: &[u8]) -> Result<(), TextError> {
    let mut r = Reader::parse(data, "liero.cfg").map_err(|e| TextError::Parse(e.0))?;
    read_text_fields(s, &mut r).map_err(|e| if r.missing_key() { TextError::MissingKey(e.0) } else { TextError::Parse(e.0) })
}

fn read_text_fields(s: &mut Settings, r: &mut Reader) -> Result<(), crate::Error> {
    s.max_bonuses = r.i32(Some("maxBonuses"))?;
    s.loading_time = r.i32(Some("loadingTime"))?;
    s.lives = r.i32(Some("lives"))?;
    s.time_to_lose = r.i32(Some("timeToLose"))?;
    s.flags_to_win = r.i32(Some("flagsToWin"))?;
    s.screen_sync = r.b("screenSync")?;
    s.map = r.b("map")?;
    s.random_level = r.b("randomLevel")?;
    s.blood = r.i32(Some("blood"))?;
    s.game_mode = r.i32(Some("gameMode"))? as u32;
    s.names_on_bonuses = r.b("namesOnBonuses")?;
    s.regenerate_level = r.b("regenerateLevel")?;
    s.shadow = r.b("shadow")?;
    s.load_change = r.b("loadChange")?;
    s.level_file = r.str(Some("levelFile"))?;

    let e = &mut s.ext;
    e.record_replays = r.b("recordReplays")?;
    e.load_powerlevel_palette = r.b("loadPowerlevelPalette")?;
    e.ai_mutations = r.i32(Some("aiMutations"))?;
    e.ai_frames = r.i32(Some("aiFrames"))?;
    e.select_bot_weapons = r.i32(Some("selectBotWeapons"))? as u32;
    e.zone_timeout = r.i32(Some("zoneTimeout"))?;
    e.ai_traces = r.b("aiTraces")?;
    e.ai_parallels = r.i32(Some("aiParallels"))?;
    e.allow_viewing_spawn_point = r.b("allowViewingSpawnPoint")?;
    e.single_screen_replay = r.b("singleScreenReplay")?;
    e.spectator_window = r.b("spectatorWindow")?;
    e.fullscreen = r.b("fullscreen")?;
    e.tc = r.str(Some("tc"))?;

    let weap_table = &mut s.weap_table;
    r.arr_each("weapTable", weap_table.len(), |r, i| {
        weap_table[i] = limit(r.i32(None)? as u32, 3);
        Ok(())
    })?;

    let worms = &mut s.worms;
    r.arr_each("worms", worms.len(), |r, i| {
        let ws = &mut worms[i];
        ws.controller = limit(r.i32(Some("controller"))? as u32, 3);
        let rgb = &mut ws.rgb;
        r.arr_each("color", rgb.len(), |r, j| {
            rgb[j] = r.i32(None)? & 63;
            Ok(())
        })?;
        let weapons = &mut ws.weapons;
        r.arr_each("weapons", weapons.len(), |r, j| {
            weapons[j] = r.i32(None)? as u32;
            Ok(())
        })?;
        ws.health = r.i32(Some("health"))?;
        ws.name = r.str(Some("name"))?;
        let controls = &mut ws.controls_ex;
        r.arr_each("controls", controls.len(), |r, j| {
            controls[j] = r.i32(None)? as u32;
            Ok(())
        })
    })
}

/// Writes liero.cfg as `archive_text` did. A worm with a random name is
/// saved with an empty one.
pub fn write_text(s: &Settings) -> Vec<u8> {
    let mut w = Writer::new();
    w.i32("maxBonuses", s.max_bonuses);
    w.i32("loadingTime", s.loading_time);
    w.i32("lives", s.lives);
    w.i32("timeToLose", s.time_to_lose);
    w.i32("flagsToWin", s.flags_to_win);
    w.bool("screenSync", s.screen_sync);
    w.bool("map", s.map);
    w.bool("randomLevel", s.random_level);
    w.i32("blood", s.blood);
    w.i32("gameMode", s.game_mode as i32);
    w.bool("namesOnBonuses", s.names_on_bonuses);
    w.bool("regenerateLevel", s.regenerate_level);
    w.bool("shadow", s.shadow);
    w.bool("loadChange", s.load_change);
    w.str("levelFile", &s.level_file);

    let e = &s.ext;
    w.bool("recordReplays", e.record_replays);
    w.bool("loadPowerlevelPalette", e.load_powerlevel_palette);
    w.i32("aiMutations", e.ai_mutations);
    w.i32("aiFrames", e.ai_frames);
    w.i32("selectBotWeapons", e.select_bot_weapons as i32);
    w.i32("zoneTimeout", e.zone_timeout);
    w.bool("aiTraces", e.ai_traces);
    w.i32("aiParallels", e.ai_parallels);
    w.bool("allowViewingSpawnPoint", e.allow_viewing_spawn_point);
    w.bool("singleScreenReplay", e.single_screen_replay);
    w.bool("spectatorWindow", e.spectator_window);
    w.bool("fullscreen", e.fullscreen);
    w.str("tc", &e.tc);

    w.arr("weapTable", &s.weap_table, |w, &v| w.i32_value(v as i32));
    w.array_obj("worms", &s.worms, |w, ws| {
        w.i32("controller", ws.controller as i32);
        w.arr("color", &ws.rgb, |w, &c| w.i32_value(c));
        w.arr("weapons", &ws.weapons, |w, &v| w.i32_value(v as i32));
        w.i32("health", ws.health);
        w.str("name", if ws.random_name { b"" } else { &ws.name });
        w.arr("controls", &ws.controls_ex, |w, &c| w.i32_value(c as i32));
    });
    w.finish()
}

// Binary formats

/// The bytes ran out, or the C++ stream they come from failed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadError;

/// Where the binary readers get their bytes.
pub trait Source {
    fn get(&mut self) -> Result<u8, ReadError>;
}

/// Bytes in memory, failing at the end like `octet_reader::get`.
pub struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Bytes { data, pos: 0 }
    }
}

impl Source for Bytes<'_> {
    fn get(&mut self) -> Result<u8, ReadError> {
        let b = *self.data.get(self.pos).ok_or(ReadError)?;
        self.pos += 1;
        Ok(b)
    }
}

/// `gvl::in_archive`'s reads. Integers are assigned only once all their
/// bytes are read; strings are cleared and grow a byte at a time.
struct In<'a, S: Source + ?Sized>(&'a mut S);

impl<S: Source + ?Sized> In<'_, S> {
    fn u8(&mut self) -> Result<u32, ReadError> {
        self.0.get().map(u32::from)
    }

    fn b(&mut self) -> Result<bool, ReadError> {
        Ok(self.0.get()? != 0)
    }

    fn u16(&mut self) -> Result<u32, ReadError> {
        Ok(self.u8()? << 8 | self.u8()?)
    }

    fn u16_le(&mut self) -> Result<u32, ReadError> {
        Ok(self.u8()? | self.u8()? << 8)
    }

    fn u32(&mut self) -> Result<u32, ReadError> {
        Ok(self.u16()? << 16 | self.u16()?)
    }

    fn str(&mut self, v: &mut Vec<u8>) -> Result<(), ReadError> {
        let len = self.u32()?;
        v.clear();
        for _ in 0..len {
            v.push(self.0.get()?);
        }
        Ok(())
    }

    /// A length byte and `field_len - 1` bytes of text and padding.
    fn pascal_str(&mut self, v: &mut Vec<u8>, field_len: usize) -> Result<(), ReadError> {
        let len = (self.0.get()? as usize).min(field_len - 1);
        v.clear();
        for _ in 0..len {
            v.push(self.0.get()?);
        }
        for _ in len..field_len - 1 {
            self.0.get()?;
        }
        Ok(())
    }
}

/// `gvl::out_archive`'s writes, with its asserts.
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u32) {
        assert!(v < 0x100, "{v} doesn't fit in 8 bits");
        self.0.push(v as u8);
    }

    fn b(&mut self, v: bool) {
        self.0.push(v as u8);
    }

    fn u16(&mut self, v: u32) {
        assert!(v < 0x10000, "{v} doesn't fit in 16 bits");
        self.0.extend_from_slice(&(v as u16).to_be_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn str(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
}

/// Reads LIERO.DAT into `s`, as `archive_liero` did. Returns false if the
/// original game's part ends early, leaving what was read. If the extensions
/// after it don't read, they go back to their defaults (all but the level
/// file, which isn't one of them) and the load still succeeds.
pub fn read_legacy(s: &mut Settings, data: &[u8]) -> bool {
    let mut src = Bytes::new(data);
    let mut r = In(&mut src);
    if read_legacy_base(s, &mut r).is_err() {
        return false;
    }
    if read_legacy_extensions(s, &mut r).is_err() {
        s.ext = Extensions::default();
        for ws in &mut s.worms {
            ws.controls_ex = [0; MAX_CONTROL_EX];
        }
    }
    true
}

fn read_legacy_base<S: Source + ?Sized>(s: &mut Settings, r: &mut In<S>) -> Result<(), ReadError> {
    s.max_bonuses = r.u8()? as i32;
    s.loading_time = r.u16_le()? as i32;
    s.lives = r.u16_le()? as i32;
    s.time_to_lose = r.u16_le()? as i32;
    s.flags_to_win = r.u16_le()? as i32;
    s.screen_sync = r.b()?;
    s.map = r.b()?;
    s.worms[0].controller = r.u8()?;
    s.worms[1].controller = r.u8()?;
    s.random_level = r.b()?;
    s.blood = r.u16_le()? as i32;
    s.game_mode = r.u8()?;
    s.names_on_bonuses = r.b()?;
    s.regenerate_level = r.b()?;
    s.shadow = r.b()?;

    for ws in &mut s.worms {
        ws.controller %= 3;
    }

    for w in &mut s.weap_table {
        *w = limit(r.u8()?, 3);
    }

    for i in 0..2 {
        for j in 0..3 {
            s.worms[i].rgb[j] = (r.u8()? & 63) as i32;
        }
    }

    for ws in &mut s.worms {
        for w in &mut ws.weapons {
            *w = r.u8()?;
        }
    }

    s.worms[0].health = r.u16_le()? as i32;
    s.worms[1].health = r.u16_le()? as i32;

    for ws in &mut s.worms {
        r.pascal_str(&mut ws.name, 21)?;
        // An empty name keeps randomName. Settings::generateName, which was
        // to pick one from NAMES.DAT, does nothing.
        if !ws.name.is_empty() {
            ws.random_name = false;
        }
    }

    s.load_change = r.b()?;

    // "\x05LIERO\0\0" and its terminator, not checked.
    for _ in 0..9 {
        r.u8()?;
    }

    for ws in &mut s.worms {
        for c in &mut ws.controls {
            *c = limit(r.u8()?, 177);
        }
    }

    r.pascal_str(&mut s.level_file, 9)
}

fn read_legacy_extensions<S: Source + ?Sized>(s: &mut Settings, r: &mut In<S>) -> Result<(), ReadError> {
    let v = r.u8()? as i32;

    r.b()?;
    s.ext.record_replays = r.b()?;
    s.ext.load_powerlevel_palette = r.b()?;
    r.u8()?;
    r.u16()?; // old fullscreenW
    r.u16()?; // old fullscreenH

    if v >= 4 {
        r.str(&mut s.level_file)?;
    }
    if v >= 2 {
        r.b()?;
    }

    for _ in 0..2 * MAX_CONTROL {
        if v >= 2 {
            r.u8()?;
            r.u8()?;
        }
    }

    for ws in &mut s.worms {
        for c in 0..MAX_CONTROL_EX {
            ws.controls_ex[c] = if v >= 3 { r.u32()? } else { ws.control_or_weapon(c) };
        }
    }

    let e = &mut s.ext;
    e.ai_mutations = if v >= 4 { r.u16()? as i32 } else { 2 };
    e.ai_frames = if v >= 4 { r.u16()? as i32 } else { 140 };
    e.select_bot_weapons = if v >= 4 { r.u8()? } else { 0 };
    e.zone_timeout = if v >= 4 { r.u16()? as i32 } else { 30 };
    e.ai_traces = if v >= 5 { r.b()? } else { false };
    e.ai_parallels = if v >= 5 { r.u16()? as i32 } else { 3 };
    e.allow_viewing_spawn_point = if v >= 6 { r.b()? } else { false };
    e.single_screen_replay = if v >= 7 { r.b()? } else { false };
    e.spectator_window = if v >= 8 { r.b()? } else { false };
    e.fullscreen = if v >= 9 { r.b()? } else { false };
    if v >= 10 {
        r.str(&mut e.tc)?;
    } else {
        e.tc = b"openliero".to_vec();
    }
    Ok(())
}

/// Why a binary archive didn't read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArchiveError {
    /// The source failed.
    Read,
    /// The settings archive's checkpoint isn't 0x12345678
    /// (`gvl::archive_check_error`).
    Check,
}

impl From<ReadError> for ArchiveError {
    fn from(_: ReadError) -> Self {
        ArchiveError::Read
    }
}

/// Reads the settings archive (`archive(in_archive, Settings&)`), without
/// the worms, which replays store with each worm.
pub fn read_archive<S: Source + ?Sized>(s: &mut Settings, src: &mut S) -> Result<(), ArchiveError> {
    let mut r = In(src);
    for w in &mut s.weap_table {
        *w = r.u8()?;
    }

    s.max_bonuses = r.u16()? as i32;
    s.blood = r.u16()? as i32;
    s.time_to_lose = r.u16()? as i32;
    s.flags_to_win = r.u16()? as i32;
    s.game_mode = r.u16()?;
    s.shadow = r.b()?;
    s.load_change = r.b()?;
    s.names_on_bonuses = r.b()?;
    s.regenerate_level = r.b()?;
    s.lives = r.u16()? as i32;
    s.loading_time = r.u16()? as i32;
    s.random_level = r.b()?;
    s.map = r.b()?;
    s.screen_sync = r.b()?;
    r.str(&mut s.level_file)?;

    let v = r.u8()? as i32;
    let e = &mut s.ext;
    r.b()?;
    e.record_replays = r.b()?;
    e.load_powerlevel_palette = r.b()?;
    r.u8()?;
    r.u16()?; // old fullscreenW
    r.u16()?; // old fullscreenH
    if v >= 2 {
        r.b()?;
    }
    e.zone_timeout = if v >= 4 { r.u16()? as i32 } else { 30 };
    e.allow_viewing_spawn_point = if v >= 6 { r.b()? } else { false };
    e.single_screen_replay = if v >= 7 { r.b()? } else { false };
    e.spectator_window = if v >= 8 { r.b()? } else { false };
    e.fullscreen = if v >= 9 { r.b()? } else { false };
    if v >= 10 {
        r.str(&mut e.tc)?;
    } else {
        e.tc = b"openliero".to_vec();
    }

    if r.u32()? != 0x1234_5678 {
        return Err(ArchiveError::Check);
    }
    Ok(())
}

/// Writes the settings archive with extension version 9. That version has
/// no TC name, and like the C++ archive, writing sets `tc` to the default
/// a reader of it would get.
pub fn write_archive(s: &mut Settings) -> Vec<u8> {
    let mut w = Out(Vec::new());
    for &v in &s.weap_table {
        w.u8(v);
    }

    w.u16(s.max_bonuses as u32);
    w.u16(s.blood as u32);
    w.u16(s.time_to_lose as u32);
    w.u16(s.flags_to_win as u32);
    w.u16(s.game_mode);
    w.b(s.shadow);
    w.b(s.load_change);
    w.b(s.names_on_bonuses);
    w.b(s.regenerate_level);
    w.u16(s.lives as u32);
    w.u16(s.loading_time as u32);
    w.b(s.random_level);
    w.b(s.map);
    w.b(s.screen_sync);
    w.str(&s.level_file);

    let v = ARCHIVE_VERSION;
    let e = &mut s.ext;
    w.u8(v as u32);
    w.b(true);
    w.b(e.record_replays);
    w.b(e.load_powerlevel_palette);
    w.u8(0);
    w.u16(0); // old fullscreenW
    w.u16(0); // old fullscreenH
    w.b(true);
    w.u16(e.zone_timeout as u32);
    w.b(e.allow_viewing_spawn_point);
    w.b(e.single_screen_replay);
    w.b(e.spectator_window);
    w.b(e.fullscreen);
    e.tc = b"openliero".to_vec();
    w.u32(0x1234_5678);
    w.0
}

/// `Settings::updateHash`: the gash of the settings archive, which also sets
/// `tc` to the default.
pub fn archive_hash(s: &mut Settings) -> Hash {
    gash(&write_archive(s))
}

/// Reads a worm's settings as stored in replays of `replay_version` and in
/// .lpf profiles (`archive(in_archive, WormSettings&)`).
pub fn read_worm<S: Source + ?Sized>(ws: &mut WormSettings, replay_version: i32, src: &mut S) -> Result<(), ArchiveError> {
    let mut r = In(src);
    ws.color = r.u32()? as i32;
    ws.health = r.u32()? as i32;
    ws.controller = r.u16()?;
    for c in &mut ws.controls {
        *c = r.u16()?;
    }
    for w in &mut ws.weapons {
        *w = r.u16()?;
    }
    for c in &mut ws.rgb {
        *c = r.u16()? as i32;
    }
    ws.random_name = r.b()?;
    r.str(&mut ws.name)?;

    if replay_version <= 1 {
        ws.controls_ex = [0; MAX_CONTROL_EX];
        return Ok(());
    }

    let v = r.u8()? as i32;
    for _ in 0..MAX_CONTROL {
        if v >= 2 {
            r.u8()?;
            r.u8()?;
        }
    }
    for c in 0..MAX_CONTROL_EX {
        ws.controls_ex[c] = if v >= 3 { r.u32()? } else { ws.control_or_weapon(c) };
    }
    Ok(())
}

/// Writes a worm's settings for a replay of `replay_version`. Versions 0
/// and 1 had no extended controls, and writing them clears them, as the C++
/// archive did.
pub fn write_worm(ws: &mut WormSettings, replay_version: i32) -> Vec<u8> {
    let mut w = Out(Vec::new());
    w.u32(ws.color as u32);
    w.u32(ws.health as u32);
    w.u16(ws.controller);
    for &c in &ws.controls {
        w.u16(c);
    }
    for &v in &ws.weapons {
        w.u16(v);
    }
    for &c in &ws.rgb {
        w.u16(c as u32);
    }
    w.b(ws.random_name);
    w.str(&ws.name);

    if replay_version <= 1 {
        ws.controls_ex = [0; MAX_CONTROL_EX];
        return w.0;
    }

    w.u8(GAME_VERSION as u32);
    for _ in 0..MAX_CONTROL {
        w.u8(0);
        w.u8(0);
    }
    for &c in &ws.controls_ex {
        w.u32(c);
    }
    w.0
}

/// `WormSettings::updateHash`: the gash of the worm's archive for the
/// current replay version.
pub fn worm_hash(ws: &mut WormSettings) -> Hash {
    gash(&write_worm(ws, REPLAY_VERSION))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn odd_settings() -> Settings {
        let mut s = Settings::default();
        s.max_bonuses = -3;
        s.game_mode = 0xffff_fff0;
        s.level_file = b"a\"b\\c\x01\xff".to_vec();
        s.ext.tc = b"my tc".to_vec();
        s.weap_table[3] = 2;
        s.worms[0].name = b"Worm".to_vec();
        s.worms[0].random_name = false;
        s.worms[1].controls_ex[7] = 0x1234;
        s
    }

    #[test]
    fn text_round_trips() {
        let s = odd_settings();
        let text = write_text(&s);
        let mut t = Settings::default();
        assert_eq!(read_text(&mut t, &text), Ok(()));
        // liero.cfg has no randomName; loading leaves it as it was.
        let mut expected = s.clone();
        expected.worms[0].random_name = true;
        assert_eq!(t, expected);
    }

    #[test]
    fn text_saves_random_names_empty() {
        let mut s = Settings::default();
        s.worms[1].name = b"Kept out".to_vec();
        let text = write_text(&s);
        assert!(!String::from_utf8_lossy(&text).contains("Kept out"));
    }

    #[test]
    fn text_limits_what_it_reads() {
        let mut s = Settings::default();
        s.weap_table[0] = 7;
        s.worms[0].controller = 5;
        s.worms[0].rgb[0] = 64 + 5;
        let mut t = Settings::default();
        read_text(&mut t, &write_text(&s)).unwrap();
        assert_eq!(t.weap_table[0], 2);
        assert_eq!(t.worms[0].controller, 2);
        assert_eq!(t.worms[0].rgb[0], 5);
    }

    #[test]
    fn text_errors_keep_what_was_read() {
        let text = String::from_utf8(write_text(&odd_settings())).unwrap();
        let missing = text.replace("lives = 15", "");
        let mut t = Settings::default();
        assert!(matches!(read_text(&mut t, missing.as_bytes()), Err(TextError::MissingKey(_))));
        assert_eq!((t.max_bonuses, t.lives), (-3, 15));

        let wrong_type = text.replace("blood = 100", "blood = true");
        let mut t = Settings::default();
        assert!(matches!(read_text(&mut t, wrong_type.as_bytes()), Err(TextError::Parse(_))));
        assert_eq!(t.random_level, true);

        let short = text.replace("weapTable = [0, 0, 0, 2", "weapTable = [0, 0, 2");
        let mut t = Settings::default();
        t.weap_table[0] = 1;
        assert!(matches!(read_text(&mut t, short.as_bytes()), Err(TextError::Parse(_))));
        assert_eq!(t.weap_table[0], 1, "the length is checked before any element is read");
    }

    #[test]
    fn archive_round_trips_without_the_tc() {
        let mut s = odd_settings();
        s.max_bonuses = 0xfffe;
        s.game_mode = 3;
        let bytes = write_archive(&mut s);
        assert_eq!(s.ext.tc, b"openliero");
        let mut t = Settings::default();
        t.ext.tc = b"other".to_vec();
        assert_eq!(read_archive(&mut t, &mut Bytes::new(&bytes)), Ok(()));
        let mut expected = s.clone();
        expected.worms = t.worms.clone();
        assert_eq!(t, expected);
    }

    #[test]
    fn archive_checks_its_checkpoint_and_end() {
        let mut bytes = write_archive(&mut Settings::default());
        let n = bytes.len();
        assert_eq!(read_archive(&mut Settings::default(), &mut Bytes::new(&bytes[..n - 1])), Err(ArchiveError::Read));
        bytes[n - 1] ^= 1;
        assert_eq!(read_archive(&mut Settings::default(), &mut Bytes::new(&bytes)), Err(ArchiveError::Check));
    }

    #[test]
    #[should_panic(expected = "16 bits")]
    fn archive_asserts_that_fields_fit() {
        let mut s = Settings::default();
        s.lives = 0x10000;
        write_archive(&mut s);
    }

    #[test]
    fn worm_archive_round_trips() {
        let mut ws = WormSettings { name: b"Name".to_vec(), controls_ex: [1, 2, 3, 4, 5, 6, 7, 8], ..Default::default() };
        let bytes = write_worm(&mut ws, REPLAY_VERSION);
        let mut t = WormSettings::default();
        read_worm(&mut t, REPLAY_VERSION, &mut Bytes::new(&bytes)).unwrap();
        assert_eq!(t, ws);

        // Old replays have no extended controls.
        let bytes = write_worm(&mut ws, 1);
        assert_eq!(ws.controls_ex, [0; 8]);
        read_worm(&mut t, 1, &mut Bytes::new(&bytes)).unwrap();
        assert_eq!(t, ws);
    }

    #[test]
    fn old_worm_archives_copy_the_controls_and_the_first_weapon() {
        let mut ws = WormSettings { controls: [1, 2, 3, 4, 5, 6, 7], weapons: [40, 0, 0, 0, 0], ..Default::default() };
        let mut bytes = write_worm(&mut ws, REPLAY_VERSION);
        let version_at = bytes.len() - 1 - 14 - 32;
        bytes.truncate(version_at);
        bytes.push(1);
        let mut t = WormSettings::default();
        read_worm(&mut t, REPLAY_VERSION, &mut Bytes::new(&bytes)).unwrap();
        assert_eq!(t.controls_ex, [1, 2, 3, 4, 5, 6, 7, 40]);
    }

    #[test]
    fn legacy_reads_a_dat_without_extensions() {
        let mut dat = vec![0u8; 1 + 8 + 2 + 2 + 1 + 2 + 1 + 3 + 40 + 6 + 10 + 4];
        dat[0] = 9; // maxBonuses
        dat[12] = 5; // controller of worm 2, before % 3
        dat.extend(std::iter::once(4).chain(*b"Test").chain([0; 16])); // name 1
        dat.extend([0; 21]); // name 2, empty
        dat.push(1); // loadChange
        dat.extend(b"\x05LIERO\0\0\0");
        dat.extend([200; 14]); // controls
        dat.extend([0; 9]); // level file

        let mut s = Settings::default();
        s.ext.ai_frames = 1;
        s.worms[0].controls_ex[0] = 99;
        assert!(read_legacy(&mut s, &dat));
        assert_eq!(s.max_bonuses, 9);
        assert_eq!(s.worms[1].controller, 2);
        assert_eq!(s.worms[0].name, b"Test");
        assert!(!s.worms[0].random_name && s.worms[1].random_name);
        assert_eq!(s.worms[0].controls, [176; 7]);
        assert_eq!(s.ext, Extensions::default(), "no extension block, so the defaults");
        assert_eq!(s.worms[0].controls_ex, [0; 8]);

        assert!(!read_legacy(&mut s, &dat[..dat.len() - 1]));
    }
}
