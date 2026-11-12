//! Settings (`hashSettings`, from `src/game/hashsettings.cpp`): random
//! settings from fixed seeds, run through every way the game reads and
//! writes them: liero.cfg, LIERO.DAT, the replay archive and its hash, and
//! the worm archive in replays and .lpf profiles. Inputs are also built by
//! hand, cut short and corrupted, so the error paths are covered too. Each
//! line hashes what was written, the settings afterwards and the error, if
//! any, that the C++ threw.

use super::{Fnv, Out};
use crate::fs::{LieroFsNode, Node};
use crate::settings::{
    liero_worm_settings_box_load_profile, liero_worm_settings_box_save_profile, LieroSettingsStatus,
    LieroWormSettingsBox,
};
use liero_data::gash::Hash;
use liero_data::settings::{self as data, ArchiveError, Bytes, Settings, TextError, WormSettings};

struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Xorshift(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(1))
    }

    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }

    fn chance(&mut self, n: u32) -> bool {
        self.below(n) == 0
    }

    /// Mostly small numbers, with the edges of each width now and then.
    fn value(&mut self) -> u32 {
        match self.below(8) {
            0 => self.below(4),
            1 => self.below(64),
            2 => self.below(256),
            3 => self.below(0x10000),
            4 => (self.below(300) as i32).wrapping_neg() as u32,
            5 => 0x7fff_ffff - self.below(3),
            6 => 0x8000_0000 + self.below(3),
            _ => self.next(),
        }
    }

    fn flag(&mut self) -> bool {
        self.next() & 1 != 0
    }

    fn str(&mut self, max_len: u32) -> Vec<u8> {
        let len = self.below(max_len + 1);
        (0..len)
            .map(|_| match self.below(4) {
                0 => self.below(256) as u8,
                1 => b"\"\\ \r\n\t[]=.,u0"[self.below(13) as usize],
                _ => b'a' + self.below(26) as u8,
            })
            .collect()
    }
}

fn add_bytes(h: &mut Fnv, b: &[u8]) {
    h.u32(b.len() as u32);
    h.bytes(b);
}

fn add_hash(h: &mut Fnv, v: &Hash) {
    for &x in v {
        h.u32(x as u32);
        h.u32((x >> 32) as u32);
    }
}

fn add_worm(h: &mut Fnv, ws: &WormSettings) {
    h.i32(ws.health);
    h.u32(ws.controller);
    ws.controls.iter().for_each(|&c| h.u32(c));
    ws.weapons.iter().for_each(|&w| h.u32(w));
    add_bytes(h, &ws.name);
    ws.rgb.iter().for_each(|&c| h.i32(c));
    h.bool(ws.random_name);
    h.i32(ws.color);
    ws.controls_ex.iter().for_each(|&c| h.u32(c));
}

fn add_settings(h: &mut Fnv, s: &Settings) {
    let e = &s.ext;
    h.bool(e.record_replays);
    h.bool(e.load_powerlevel_palette);
    h.i32(e.blood_particle_max);
    h.i32(e.ai_frames);
    h.i32(e.ai_mutations);
    h.bool(e.ai_traces);
    h.i32(e.ai_parallels);
    h.bool(e.fullscreen);
    h.i32(e.zone_timeout);
    h.u32(e.select_bot_weapons);
    h.bool(e.allow_viewing_spawn_point);
    h.bool(e.single_screen_replay);
    h.bool(e.spectator_window);
    add_bytes(h, &e.tc);

    s.weap_table.iter().for_each(|&w| h.u32(w));
    h.i32(s.max_bonuses);
    h.i32(s.blood);
    h.i32(s.time_to_lose);
    h.i32(s.flags_to_win);
    h.u32(s.game_mode);
    h.bool(s.shadow);
    h.bool(s.load_change);
    h.bool(s.names_on_bonuses);
    h.bool(s.regenerate_level);
    h.i32(s.lives);
    h.i32(s.loading_time);
    h.bool(s.random_level);
    add_bytes(h, &s.level_file);
    h.bool(s.map);
    h.bool(s.screen_sync);

    s.worms.iter().for_each(|ws| add_worm(h, ws));
}

/// What the C++ threw: `ArchiveCheckError` (1), a `std::runtime_error` (3)
/// or `std::out_of_range` (4), with its message.
enum Thrown {
    Check,
    Runtime(String),
    OutOfRange,
}

impl From<ArchiveError> for Thrown {
    fn from(e: ArchiveError) -> Self {
        match e {
            ArchiveError::Read => Thrown::Runtime("Read error in get()".into()),
            ArchiveError::Check => Thrown::Check,
        }
    }
}

/// Adds which error `r` is, if any, and its message (`attempt`).
fn attempt(h: &mut Fnv, r: Result<(), Thrown>) {
    let (kind, what) = match r {
        Ok(()) => (0, String::new()),
        Err(Thrown::Check) => (1, "Expected checkpoint here".into()),
        Err(Thrown::Runtime(m)) => (3, m),
        Err(Thrown::OutOfRange) => (4, String::new()),
    };
    h.i32(kind);
    add_bytes(h, what.as_bytes());
}

/// `attempt(h, [&] { h.add(f()); })`: the result goes in before the error.
fn attempt_bool(h: &mut Fnv, r: Result<bool, Thrown>) {
    let r = r.map(|ok| h.bool(ok));
    attempt(h, r);
}

fn random_worm(r: &mut Xorshift, ws: &mut WormSettings) {
    ws.health = r.value() as i32;
    ws.controller = r.value();
    ws.controls.iter_mut().for_each(|c| *c = r.value());
    ws.weapons.iter_mut().for_each(|w| *w = r.value());
    ws.name = r.str(25);
    ws.rgb.iter_mut().for_each(|c| *c = r.value() as i32);
    ws.random_name = r.flag();
    ws.color = r.value() as i32;
    ws.controls_ex.iter_mut().for_each(|c| *c = r.value());
}

fn random_settings(r: &mut Xorshift, s: &mut Settings) {
    let e = &mut s.ext;
    e.record_replays = r.flag();
    e.load_powerlevel_palette = r.flag();
    e.blood_particle_max = r.value() as i32;
    e.ai_frames = r.value() as i32;
    e.ai_mutations = r.value() as i32;
    e.ai_traces = r.flag();
    e.ai_parallels = r.value() as i32;
    e.fullscreen = r.flag();
    e.zone_timeout = r.value() as i32;
    e.select_bot_weapons = r.value();
    e.allow_viewing_spawn_point = r.flag();
    e.single_screen_replay = r.flag();
    e.spectator_window = r.flag();
    e.tc = r.str(12);

    for w in &mut s.weap_table {
        *w = if r.chance(2) { r.below(4) } else { r.value() };
    }
    s.max_bonuses = r.value() as i32;
    s.blood = r.value() as i32;
    s.time_to_lose = r.value() as i32;
    s.flags_to_win = r.value() as i32;
    s.game_mode = r.value();
    s.shadow = r.flag();
    s.load_change = r.flag();
    s.names_on_bonuses = r.flag();
    s.regenerate_level = r.flag();
    s.lives = r.value() as i32;
    s.loading_time = r.value() as i32;
    s.random_level = r.flag();
    s.level_file = r.str(12);
    s.map = r.flag();
    s.screen_sync = r.flag();

    s.worms.iter_mut().for_each(|ws| random_worm(r, ws));
}

/// The binary archives assert that 8- and 16-bit fields fit, and the game
/// aborts when they don't, so archived settings are cut down to fit first.
fn fit_worm(ws: &mut WormSettings) {
    ws.controller &= 0xffff;
    ws.controls.iter_mut().for_each(|c| *c &= 0xffff);
    ws.weapons.iter_mut().for_each(|w| *w &= 0xffff);
    ws.rgb.iter_mut().for_each(|c| *c &= 0xffff);
}

fn fit_settings(s: &mut Settings) {
    s.weap_table.iter_mut().for_each(|w| *w &= 0xff);
    s.max_bonuses &= 0xffff;
    s.blood &= 0xffff;
    s.time_to_lose &= 0xffff;
    s.flags_to_win &= 0xffff;
    s.game_mode &= 0xffff;
    s.lives &= 0xffff;
    s.loading_time &= 0xffff;
    s.ext.zone_timeout &= 0xffff;
    s.worms.iter_mut().for_each(fit_worm);
}

/// Defaults or random settings, so that a load that stops part way shows
/// which fields it had set.
fn start_settings(r: &mut Xorshift) -> Settings {
    let mut s = Settings::default();
    if r.flag() {
        random_settings(r, &mut s);
    }
    s
}

/// Default or random worm settings.
fn start_worm(r: &mut Xorshift) -> WormSettings {
    let mut ws = WormSettings::default();
    if r.flag() {
        random_worm(r, &mut ws);
    }
    ws
}

/// Breaks a liero.cfg in one to three places.
fn corrupt_text(r: &mut Xorshift, mut b: Vec<u8>) -> Vec<u8> {
    const INTERESTING: &[u8] = b" \r\n\t[]=\"\\u0123456789-,.tfnx";
    let ops = 1 + r.below(3);
    for _ in 0..ops {
        let n = b.len();
        match r.below(7) {
            // Change a byte. Clang works out the value before the place.
            0 if n > 0 => {
                let v = if r.chance(3) {
                    r.below(256) as u8
                } else {
                    INTERESTING[r.below(INTERESTING.len() as u32) as usize]
                };
                let at = r.below(n as u32) as usize;
                b[at] = v;
            }
            // Delete a few bytes
            1 if n > 0 => {
                let at = r.below(n as u32) as usize;
                let len = (1 + r.below(10) as usize).min(n - at);
                b.drain(at..at + len);
            }
            // Insert a byte
            2 => {
                let at = r.below(n as u32 + 1) as usize;
                let v = if r.chance(2) { b'\n' } else { r.below(256) as u8 };
                b.insert(at, v);
            }
            // Cut the end off
            3 => b.truncate(r.below(n as u32 + 1) as usize),
            // Repeat or drop a line
            4 | 5 if n > 0 => {
                let at = r.below(n as u32) as usize;
                let (mut beg, mut end) = (at, at);
                while beg > 0 && b[beg - 1] != b'\n' {
                    beg -= 1;
                }
                while end < n && b[end] != b'\n' {
                    end += 1;
                }
                if end < n {
                    end += 1;
                }
                if r.flag() {
                    let line = b[beg..end].to_vec();
                    let to = r.below(n as u32 + 1) as usize;
                    b.splice(to..to, line);
                } else {
                    b.drain(beg..end);
                }
            }
            // Add an element to an array
            6 => {
                let ends: Vec<usize> = (1..n).filter(|&j| b[j] == b']' && b[j - 1] != b']').collect();
                if !ends.is_empty() {
                    let at = ends[r.below(ends.len() as u32) as usize];
                    b.splice(at..at, *b", 1");
                }
            }
            _ => {}
        }
    }
    b
}

/// Whether the C++ TOML reader would reach the end of `b` inside a string,
/// where it looped until it ran out of memory. Every quote it meets outside
/// a string either starts one or is a parse error, so counting quotes and
/// escapes from the start finds the strings it reads.
fn ends_in_string(b: &[u8]) -> bool {
    let mut inside = false;
    let mut i = 0;
    while i < b.len() {
        if !inside {
            inside = b[i] == b'"';
        } else if b[i] == b'"' {
            inside = false;
        } else if b[i] == b'\\' {
            let Some(&e) = b.get(i + 1) else { return true };
            match e {
                b'\\' | b'/' | b'"' => i += 1,
                b'u' => i += 5,
                // A parse error
                _ => return false,
            }
        }
        i += 1;
    }
    inside
}

/// Breaks a liero.cfg, but not in the one way that hung the C++ reader.
fn broken_text(r: &mut Xorshift, text: &[u8]) -> Vec<u8> {
    let mut b = corrupt_text(r, text.to_vec());
    // An escape at the end can swallow the first quotes.
    while ends_in_string(&b) {
        b.push(b'"');
    }
    b
}

struct Builder<'a> {
    r: &'a mut Xorshift,
    b: Vec<u8>,
}

impl<'a> Builder<'a> {
    fn new(r: &'a mut Xorshift) -> Self {
        Builder { r, b: Vec::new() }
    }

    fn put(&mut self, v: u32) {
        self.b.push(v as u8);
    }

    fn u8(&mut self) {
        let v = self.r.below(256);
        self.put(v);
    }

    fn boolean(&mut self) {
        let v = if self.r.chance(4) {
            self.r.below(256)
        } else {
            self.r.below(2)
        };
        self.put(v);
    }

    fn u16le(&mut self) {
        let v = self.r.value();
        self.put(v);
        self.put(v >> 8);
    }

    fn put16(&mut self, v: u32) {
        self.put(v >> 8);
        self.put(v);
    }

    fn u16(&mut self) {
        let v = self.r.value();
        self.put16(v);
    }

    fn put32(&mut self, v: u32) {
        self.put16(v >> 16);
        self.put16(v);
    }

    fn u32(&mut self) {
        let v = self.r.value();
        self.put32(v);
    }

    fn str(&mut self) {
        if self.r.chance(16) {
            // Likely longer than what's left
            self.u32();
            return;
        }
        let s = self.r.str(10);
        self.put32(s.len() as u32);
        self.b.extend_from_slice(&s);
    }

    /// Lengths up to 255, with the bytes after the field read as text too.
    fn pascal(&mut self, field_len: u32) {
        let len = if self.r.chance(4) {
            self.r.below(256)
        } else {
            self.r.below(field_len + 2)
        };
        self.put(len);
        for _ in 1..field_len {
            let v = if self.r.chance(2) {
                u32::from(b'a') + self.r.below(26)
            } else {
                self.r.below(256)
            };
            self.put(v);
        }
    }

    fn maybe_truncate(mut self) -> Vec<u8> {
        if self.r.chance(3) {
            let n = self.r.below(self.b.len() as u32 + 1) as usize;
            self.b.truncate(n);
        }
        self.b
    }
}

/// A LIERO.DAT, laid out as the legacy reader reads it, with random values
/// and an extension block of a random version.
fn legacy_dat(r: &mut Xorshift) -> Vec<u8> {
    let mut w = Builder::new(r);
    w.u8();
    (0..4).for_each(|_| w.u16le());
    w.boolean();
    w.boolean();
    w.u8();
    w.u8();
    w.boolean();
    w.u16le();
    w.u8();
    (0..3).for_each(|_| w.boolean());
    for _ in 0..40 {
        let v = if w.r.chance(2) { w.r.below(4) } else { w.r.below(256) };
        w.put(v);
    }
    (0..16).for_each(|_| w.u8());
    w.u16le();
    w.u16le();
    w.pascal(21);
    w.pascal(21);
    w.boolean();
    b"\x05LIERO\0\0\0".iter().for_each(|&c| w.put(u32::from(c)));
    for _ in 0..14 {
        let v = if w.r.chance(2) { w.r.below(180) } else { w.r.below(256) };
        w.put(v);
    }
    w.pascal(9);

    if !w.r.chance(8) {
        let v = if w.r.chance(2) { 10 } else { w.r.below(13) };
        w.put(v);
        (0..3).for_each(|_| w.boolean());
        w.u8();
        w.u16();
        w.u16();
        if v >= 4 {
            w.str();
        }
        if v >= 2 {
            w.boolean();
            (0..28).for_each(|_| w.u8());
        }
        if v >= 3 {
            (0..16).for_each(|_| w.u32());
        }
        if v >= 4 {
            w.u16();
            w.u16();
            w.u8();
            w.u16();
        }
        if v >= 5 {
            w.boolean();
            w.u16();
        }
        for at in 6..=9 {
            if v >= at {
                w.boolean();
            }
        }
        if v >= 10 {
            w.str();
        }
    }
    w.maybe_truncate()
}

/// A settings archive, laid out as `read_archive` reads it, with an
/// extension version other than the one the game writes.
fn binary_settings(r: &mut Xorshift) -> Vec<u8> {
    let mut w = Builder::new(r);
    (0..40).for_each(|_| w.u8());
    (0..5).for_each(|_| w.u16());
    (0..4).for_each(|_| w.boolean());
    w.u16();
    w.u16();
    (0..3).for_each(|_| w.boolean());
    w.str();
    let v = w.r.below(13);
    w.put(v);
    (0..3).for_each(|_| w.boolean());
    w.u8();
    w.u16();
    w.u16();
    if v >= 2 {
        w.boolean();
    }
    if v >= 4 {
        w.u16();
    }
    for at in 6..=9 {
        if v >= at {
            w.boolean();
        }
    }
    if v >= 10 {
        w.str();
    }
    let check = if w.r.chance(4) { w.r.value() } else { 0x1234_5678 };
    w.put32(check);
    w.maybe_truncate()
}

/// A worm settings archive with a random version.
fn binary_worm(r: &mut Xorshift) -> Vec<u8> {
    let mut w = Builder::new(r);
    w.u32();
    w.u32();
    w.u16();
    (0..15).for_each(|_| w.u16());
    w.boolean();
    w.str();
    let v = w.r.below(13);
    w.put(v);
    if v >= 2 {
        (0..14).for_each(|_| w.u8());
    }
    if v >= 3 {
        (0..8).for_each(|_| w.u32());
    }
    w.maybe_truncate()
}

fn read_settings(h: &mut Fnv, data: &[u8], s: &mut Settings) {
    attempt(h, data::read_archive(s, &mut Bytes::new(data)).map_err(Thrown::from));
    add_settings(h, s);
}

fn write_settings(h: &mut Fnv, s: &mut Settings) -> Vec<u8> {
    let b = data::write_archive(s);
    add_bytes(h, &b);
    add_settings(h, s);
    b
}

fn read_worm(h: &mut Fnv, data: &[u8], ws: &mut WormSettings, version: i32) {
    attempt(
        h,
        data::read_worm(ws, version, &mut Bytes::new(data)).map_err(Thrown::from),
    );
    add_worm(h, ws);
}

fn write_worm(h: &mut Fnv, ws: &mut WormSettings, version: i32) -> Vec<u8> {
    let b = data::write_worm(ws, version);
    add_bytes(h, &b);
    add_worm(h, ws);
    b
}

/// `Settings::load`: true if liero.cfg read, false if it didn't, and
/// out of range if a setting was missing.
fn load_text(node: &Node, s: &mut Settings) -> Result<bool, Thrown> {
    let Some(d) = node.read() else { return Ok(false) };
    match data::read_text(s, &d) {
        Ok(()) => Ok(true),
        Err(TextError::MissingKey(_)) => Err(Thrown::OutOfRange),
        Err(TextError::Parse(_)) => Ok(false),
    }
}

/// `Settings::loadLegacy`.
fn load_legacy(node: &Node, s: &mut Settings) -> bool {
    node.read().is_some_and(|d| data::read_legacy(s, &d))
}

/// A worm's settings in a box of their own, without a profile.
fn worm_box(ws: &WormSettings) -> LieroWormSettingsBox {
    let mut b = LieroWormSettingsBox::default();
    b.set_worm(ws);
    b
}

/// `WormSettings::saveProfile`.
fn save_profile(node: &LieroFsNode, ws: &WormSettings) -> Result<(), Thrown> {
    if liero_worm_settings_box_save_profile(&mut worm_box(ws), Some(node)) {
        Ok(())
    } else {
        Err(Thrown::Runtime(format!(
            "Could not write {}",
            String::from_utf8_lossy(node.0.full_path())
        )))
    }
}

/// `WormSettings::loadProfile` over `ws`.
fn load_profile(node: &LieroFsNode, ws: &mut WormSettings) -> Result<(), Thrown> {
    let mut b = worm_box(ws);
    let mut status = LieroSettingsStatus::Ok;
    let read = liero_worm_settings_box_load_profile(&mut b, Some(node), &mut status);
    *ws = b.worm();
    if !read {
        return Err(Thrown::Runtime(format!(
            "Could not read {}",
            String::from_utf8_lossy(node.0.full_path())
        )));
    }
    match status {
        LieroSettingsStatus::ReadError => Err(ArchiveError::Read.into()),
        LieroSettingsStatus::CheckError => Err(ArchiveError::Check.into()),
        _ => Ok(()),
    }
}

const REPLAY_VERSIONS: [i32; 5] = [0, 1, 2, 3, 4];

pub fn hash_settings(out: &mut Out) {
    let mut print = |what: &str, i: usize, h: Fnv| out.hash(&format!("settings-{what}"), i as i64, h);

    let mut h = Fnv::default();
    add_settings(&mut h, &Settings::default());
    print("default", 0, h);

    let cfg = Node::new(b"/tmp/liero.cfg");
    let dat = Node::new(b"/tmp/LIERO.DAT");
    let lpf = LieroFsNode(Node::new(b"/tmp/worm.lpf"));

    for i in 0..64 {
        let r = &mut Xorshift::new(i as u64);
        let mut s = Settings::default();
        if i > 0 {
            random_settings(r, &mut s);
        }

        // liero.cfg written and read back.
        let mut h = Fnv::default();
        assert!(cfg.write(&data::write_text(&s)), "Could not write /tmp/liero.cfg");
        let text = cfg.read().unwrap_or_default();
        add_bytes(&mut h, &text);
        add_settings(&mut h, &s);
        print("save", i, h);

        let mut h = Fnv::default();
        let mut t = start_settings(r);
        attempt_bool(&mut h, load_text(&cfg, &mut t));
        add_settings(&mut h, &t);
        print("load", i, h);

        // Broken liero.cfg files.
        let mut h = Fnv::default();
        for _ in 0..24 {
            cfg.write(&broken_text(r, &text));
            let mut t = start_settings(r);
            attempt_bool(&mut h, load_text(&cfg, &mut t));
            add_settings(&mut h, &t);
        }
        print("load-broken", i, h);

        // The settings archive and its hash.
        fit_settings(&mut s);
        let mut h = Fnv::default();
        let mut copy = s.clone();
        add_hash(&mut h, &data::archive_hash(&mut copy));
        add_settings(&mut h, &copy);
        print("hash", i, h);

        let mut h = Fnv::default();
        let bin = write_settings(&mut h, &mut s);
        let mut t = start_settings(r);
        read_settings(&mut h, &bin, &mut t);
        print("archive", i, h);

        let mut h = Fnv::default();
        for j in 0..12 {
            let b = if j < 4 {
                let mut b = bin.clone();
                b.truncate(r.below(b.len() as u32 + 1) as usize);
                b
            } else {
                binary_settings(r)
            };
            let mut t = start_settings(r);
            fit_settings(&mut t);
            read_settings(&mut h, &b, &mut t);
            // Write what was read, which normalises the version.
            write_settings(&mut h, &mut t);
        }
        print("archive-broken", i, h);

        // LIERO.DAT files.
        let mut h = Fnv::default();
        for _ in 0..12 {
            dat.write(&legacy_dat(r));
            let mut t = start_settings(r);
            attempt_bool(&mut h, Ok(load_legacy(&dat, &mut t)));
            add_settings(&mut h, &t);
        }
        print("legacy", i, h);

        // Worm settings: replay archives of every version, their hash and
        // profiles.
        for w in 0..2 {
            let ws = &s.worms[w];
            let n = i * 2 + w;

            let mut h = Fnv::default();
            for rv in REPLAY_VERSIONS {
                let b = write_worm(&mut h, &mut ws.clone(), rv);
                let mut t = start_worm(r);
                read_worm(&mut h, &b, &mut t, rv);
            }
            let mut copy = ws.clone();
            add_hash(&mut h, &data::worm_hash(&mut copy));
            add_worm(&mut h, &copy);
            print("worm", n, h);

            let mut h = Fnv::default();
            for j in 0..12 {
                let rv = REPLAY_VERSIONS[r.below(5) as usize];
                let b = if j < 4 {
                    let mut b = write_worm(&mut Fnv::default(), &mut ws.clone(), rv);
                    b.truncate(r.below(b.len() as u32 + 1) as usize);
                    b
                } else {
                    binary_worm(r)
                };
                let mut t = start_worm(r);
                read_worm(&mut h, &b, &mut t, rv);
            }
            print("worm-broken", n, h);

            let mut h = Fnv::default();
            attempt(&mut h, save_profile(&lpf, ws));
            let b = lpf.0.read().unwrap_or_default();
            add_bytes(&mut h, &b);
            for j in 0..4 {
                if j > 0 {
                    let c = if j < 3 {
                        let mut c = b.clone();
                        c.truncate(r.below(c.len() as u32 + 1) as usize);
                        c
                    } else {
                        binary_worm(r)
                    };
                    lpf.0.write(&c);
                }
                let mut t = start_worm(r);
                let loaded = load_profile(&lpf, &mut t);
                attempt(&mut h, loaded);
                add_worm(&mut h, &t);
            }
            print("profile", n, h);
        }
    }
}
