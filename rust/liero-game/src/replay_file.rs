//! Replay files as the game records and plays them (C++'s `ReplayWriter`
//! and `ReplayReader`): the format in `replay.rs`, worked straight on the
//! game, its settings and its level, which Rust owns.
//!
//! A writer compresses as it goes and ends the file when it's freed. It
//! remembers which worms' settings it has written, and the settings'
//! hashes when the game loses focus, so that settings changed in the menus
//! are written again. A reader inflates the whole file first, then plays
//! it from a position the replay viewer's R can go back to.
//!
//! Errors come back as a `LieroReplayOutcome`, which C++ turns into the
//! exceptions its replay code threw.

use std::ptr::null_mut;

use liero_data::gash::Hash;
use liero_data::settings::{self as data, ReadError, Settings, Source, WormSettings};

use crate::deflate::{inflate, Deflater};
use crate::fs::Node;
use crate::game::LieroRect;
use crate::game_box::{
    liero_game_add_viewport, liero_game_data, liero_game_level, liero_game_list_worm, liero_game_new_worm,
    liero_game_settings, liero_game_viewport, liero_game_viewport_count, liero_game_worm, liero_game_worm_count,
    liero_worm_data, liero_worm_set_settings, liero_worm_settings, LieroGame, LieroWorm,
};
use crate::replay::{
    checksum, liero_replay_viewport_of, liero_replay_viewport_set, liero_replay_worm_of, liero_replay_worm_set,
    read_checksum, read_frame, read_start, write_end, write_frame, write_start, Game, Level, LieroReplayFrameWorm,
    LieroReplayGameFields, LieroReplayStatus, ReplayError, Worm, LIERO_REPLAY_VERSION,
};
use crate::settings::{
    liero_worm_settings_box_free, liero_worm_settings_box_new, LieroSettingsBox, LieroWormSettingsBox,
};
use crate::text::LieroStr;
use crate::viewport::LIERO_VIEWPORT_WORM;
use crate::worm::WormId;

/// How a replay call ended, for C++ to throw as its replay code did.
#[derive(Clone, Copy)]
pub struct LieroReplayOutcome {
    pub status: LieroReplayStatus,
    /// For `CheckError`, the `gvl::archive_check_error` message; for
    /// `ReadError`, what the reader threw at the end of its bytes.
    pub message: LieroStr,
    /// The replay ended (a frame).
    pub end: bool,
    /// The frame read settings, so the game needs `updateSettings`, which
    /// C++ calls before throwing a checksum error.
    pub settings_changed: bool,
}

impl LieroReplayOutcome {
    fn new(r: Result<(), ReplayError>, read_error: &'static str) -> Self {
        let (status, message) = match r {
            Ok(()) => (LieroReplayStatus::Ok, ""),
            Err(ReplayError::Read) => (LieroReplayStatus::ReadError, read_error),
            Err(ReplayError::Check(m)) => (LieroReplayStatus::CheckError, m),
            Err(ReplayError::OutOfRange) => (LieroReplayStatus::OutOfRange, ""),
        };
        LieroReplayOutcome {
            status,
            message: LieroStr {
                ptr: message.as_ptr(),
                len: message.len(),
            },
            end: false,
            settings_changed: false,
        }
    }
}

/// The game's worms, in order. Recorded and played games list no empty
/// places.
///
/// # Safety
/// `game` is valid.
unsafe fn game_worms(game: &mut LieroGame) -> Vec<*mut LieroWorm> {
    (0..liero_game_worm_count(game))
        .map(|i| {
            let w = liero_game_worm(game, i);
            assert!(!w.is_null(), "an empty place among a replay's worms");
            w
        })
        .collect()
}

/// The ids of the game's worms, as `game_worms` has them.
fn game_worm_ids(game: &LieroGame) -> Vec<WormId> {
    game.listed_ids()
        .iter()
        .map(|id| id.expect("an empty place among a replay's worms"))
        .collect()
}

/// # Safety
/// `w` is valid and has settings.
unsafe fn worm_settings<'a>(w: *mut LieroWorm) -> &'a mut LieroWormSettingsBox {
    unsafe {
        let s = liero_worm_settings(&*w);
        assert!(!s.is_null(), "a worm without settings");
        &mut *s
    }
}

/// `fastGameChecksum`: the generator and the worms' positions and
/// velocities.
///
/// # Safety
/// `game` is valid.
unsafe fn game_checksum(game: &mut LieroGame) -> u32 {
    unsafe {
        let worms: Vec<[i32; 4]> = game_worms(game)
            .iter()
            .map(|&w| {
                let d = &(*w).data;
                [d.pos.x, d.pos.y, d.vel.x, d.vel.y]
            })
            .collect();
        checksum((*liero_game_data(game)).rand.x, &worms)
    }
}

/// `Settings::updateHash`: the hash of the settings archive. Writing it
/// sets the TC name to the default, since the archive has none.
fn settings_hash(s: &mut LieroSettingsBox) -> Hash {
    let mut settings = s.settings();
    let h = data::archive_hash(&mut settings);
    s.set_settings(&settings);
    h
}

/// `WormSettings::updateHash`.
fn worm_hash(s: &mut LieroWormSettingsBox) -> Hash {
    let mut ws = s.worm();
    let h = data::worm_hash(&mut ws);
    s.set_worm(&ws);
    h
}

/// A frame's view of a worm, with its settings.
///
/// # Safety
/// `w` is valid and has settings.
unsafe fn frame_worm(
    w: *mut LieroWorm,
    settings_expired: bool,
    settings: WormSettings,
) -> (LieroReplayFrameWorm, WormSettings) {
    unsafe {
        let d = &(*w).data;
        let f = LieroReplayFrameWorm {
            index: d.index,
            control_states: d.control_states,
            prev_control_states: d.prev_control_states,
            settings_expired,
        };
        (f, settings)
    }
}

// ---- Writing ----

/// What a writer remembers of a worm's settings (`GameSerializationContext::
/// WormData`).
#[derive(Clone, Copy)]
struct WormData {
    last_hash: Hash,
    expired: bool,
}

impl Default for WormData {
    fn default() -> Self {
        WormData {
            last_hash: Hash::default(),
            expired: true,
        }
    }
}

/// A replay being recorded (`ReplayWriter`).
pub struct LieroReplayWriter {
    out: Deflater,
    version: i32,
    game: *mut LieroGame,
    settings: *mut LieroSettingsBox,
    settings_expired: bool,
    last_settings_hash: Hash,
    /// Each worm the writer has seen, as C++'s map from `Worm*` had them.
    worms: Vec<(*mut LieroWorm, WormData)>,
}

impl LieroReplayWriter {
    /// The worm's entry, made as `std::map::operator[]` made it.
    fn worm(&mut self, w: *mut LieroWorm) -> &mut WormData {
        let i = match self.worms.iter().position(|&(p, _)| p == w) {
            Some(i) => i,
            None => {
                self.worms.push((w, WormData::default()));
                self.worms.len() - 1
            }
        };
        &mut self.worms[i].1
    }
}

impl Drop for LieroReplayWriter {
    /// `endRecord`, then the stream is finished as the deflater drops.
    fn drop(&mut self) {
        self.out.write(&write_end());
    }
}

/// Starts writing a replay at `node`, or null if it can't be created.
pub fn liero_replay_writer_new(node: &Node) -> *mut LieroReplayWriter {
    match Deflater::new(node) {
        Some(out) => Box::into_raw(Box::new(LieroReplayWriter {
            out,
            version: LIERO_REPLAY_VERSION,
            game: null_mut(),
            settings: null_mut(),
            settings_expired: true,
            last_settings_hash: Hash::default(),
            worms: Vec::new(),
        })),
        None => null_mut(),
    }
}

/// Ends the replay and closes the file.
///
/// # Safety
/// `w` comes from `liero_replay_writer_new`, or is null, and isn't used
/// again.
pub unsafe fn liero_replay_writer_free(w: *mut LieroReplayWriter) {
    unsafe {
        if !w.is_null() {
            drop(Box::from_raw(w));
        }
    }
}

/// The version to write, before `liero_replay_writer_begin`; the hash run
/// writes old ones.
pub fn liero_replay_writer_set_version(w: &mut LieroReplayWriter, version: i32) {
    w.version = version;
}

/// `beginRecord`: writes the game as it is, with its settings, level and
/// the TC's weapons. The game and settings must stay while the writer
/// records them. Writing changes the settings as `write_start` says; with
/// an `OutOfRange` outcome, what was written stays.
///
/// # Safety
/// The game has its settings and TC (`liero_game_set_context`).
pub unsafe fn liero_replay_writer_begin(w: &mut LieroReplayWriter, game: &mut LieroGame) -> LieroReplayOutcome {
    unsafe {
        let settings = &mut *liero_game_settings(game);
        let level = &*liero_game_level(game);
        w.game = game;
        w.settings = settings;

        let worms = game_worms(game);
        let ids = game_worm_ids(game);
        let datas: Vec<_> = worms.iter().map(|&p| liero_worm_data(&mut *p)).collect();
        let d = &*liero_game_data(game);
        let l = level.data();
        let mut palette = [0u8; 768];
        for (i, c) in l.origpal.iter().enumerate() {
            palette[i * 3..i * 3 + 3].copy_from_slice(&[c.r, c.g, c.b]);
        }
        let mut g = Game {
            settings: settings.settings(),
            fields: LieroReplayGameFields {
                cycles: d.cycles,
                got_changed: d.got_changed,
                last_killed: d.last_killed_idx,
                screen_flash: d.screen_flash,
                rand_x: d.rand.x,
                rand_c: d.rand.c,
            },
            worms: worms
                .iter()
                .zip(&datas)
                .map(|(&p, &data)| Worm {
                    fields: liero_replay_worm_of(&*data, &ids),
                    settings: worm_settings(p).worm(),
                })
                .collect(),
            list: (0..worms.len() as i32).collect(),
            viewports: (0..liero_game_viewport_count(game, false))
                .map(|i| liero_replay_viewport_of(&*liero_game_viewport(game, false, i)))
                .collect(),
            level: Level {
                width: l.level.width as u32,
                height: l.level.height as u32,
                pixels: level.pixels().to_vec(),
                palette,
            },
        };
        let (bytes, r) = write_start(&mut g, w.version);

        // The game's settings first: their worms can be the worms' own.
        settings.set_settings(&g.settings);
        for (&p, rw) in worms.iter().zip(&g.worms) {
            worm_settings(p).set_worm(&rw.settings);
        }
        w.out.write(&bytes);
        if r.is_err() {
            return LieroReplayOutcome::new(r, "");
        }

        // They were just written, so they're up to date.
        for &p in &worms {
            w.worm(p).expired = false;
        }
        w.settings_expired = false;
        LieroReplayOutcome::new(Ok(()), "")
    }
}

/// `recordFrame`: the game's controls this tick, and any settings that
/// changed since they were written (a worm's only with more than three
/// worms).
///
/// # Safety
/// The game and settings `liero_replay_writer_begin` had are still valid.
pub unsafe fn liero_replay_writer_frame(w: &mut LieroReplayWriter) {
    unsafe {
        let game = &mut *w.game;
        let settings = &mut *w.settings;
        let worms = game_worms(game);
        let with_settings = worms.len() > 3;
        let mut fw: Vec<_> = worms
            .iter()
            .map(|&p| {
                let expired = with_settings && w.worm(p).expired;
                // Converting the settings only when they're written keeps frames
                // cheap.
                let ws = if expired {
                    worm_settings(p).worm()
                } else {
                    WormSettings::default()
                };
                frame_worm(p, expired, ws)
            })
            .collect();
        let mut s = if w.settings_expired {
            settings.settings()
        } else {
            Settings::default()
        };
        let sum = game_checksum(game);
        let cycles = (*liero_game_data(game)).cycles;
        let (bytes, game_written, worms_written) =
            write_frame(w.version, &mut s, w.settings_expired, &mut fw, cycles, sum);

        if game_written {
            settings.set_settings(&s);
            w.settings_expired = false;
        }
        for ((&p, (_, ws)), written) in worms.iter().zip(&fw).zip(worms_written) {
            if written {
                worm_settings(p).set_worm(ws);
                w.worm(p).expired = false;
            }
        }
        w.out.write(&bytes);
    }
}

/// The game lost focus (the menu opened): remembers the settings' hashes.
///
/// # Safety
/// As for `liero_replay_writer_frame`, once recording has begun.
pub unsafe fn liero_replay_writer_unfocus(w: &mut LieroReplayWriter) {
    unsafe {
        for (p, d) in w.worms.iter_mut() {
            d.last_hash = worm_hash(worm_settings(*p));
        }
        w.last_settings_hash = settings_hash(&mut *w.settings);
    }
}

/// The game has focus again: settings that changed are written with the
/// next frame.
///
/// # Safety
/// As for `liero_replay_writer_unfocus`.
pub unsafe fn liero_replay_writer_focus(w: &mut LieroReplayWriter) {
    unsafe {
        for (p, d) in w.worms.iter_mut() {
            if d.last_hash != worm_hash(worm_settings(*p)) {
                d.expired = true;
            }
        }
        if w.last_settings_hash != settings_hash(&mut *w.settings) {
            w.settings_expired = true;
        }
    }
}

// ---- Reading ----

/// A replay being played (`ReplayReader`).
pub struct LieroReplayReader {
    data: Vec<u8>,
    pos: usize,
    /// What a read past the end throws: the inflater's error, if it had
    /// one, as gvl's reader did.
    end_error: &'static str,
    version: i32,
    game: *mut LieroGame,
    settings: *mut LieroSettingsBox,
}

struct ReaderSource<'a> {
    data: &'a [u8],
    pos: &'a mut usize,
}

impl Source for ReaderSource<'_> {
    fn get(&mut self) -> Result<u8, ReadError> {
        let b = *self.data.get(*self.pos).ok_or(ReadError)?;
        *self.pos += 1;
        Ok(b)
    }
}

impl LieroReplayReader {
    fn source(&mut self) -> ReaderSource<'_> {
        ReaderSource {
            data: &self.data,
            pos: &mut self.pos,
        }
    }

    fn outcome(&self, r: Result<(), ReplayError>) -> LieroReplayOutcome {
        LieroReplayOutcome::new(r, self.end_error)
    }
}

/// Reads and inflates the replay at `node`, or null if it can't be read.
pub fn liero_replay_reader_new(node: &Node) -> *mut LieroReplayReader {
    let Some(file) = node.read() else { return null_mut() };
    let r = inflate(&file);
    Box::into_raw(Box::new(LieroReplayReader {
        data: r.data,
        pos: 0,
        end_error: if r.error {
            "Error while deflating"
        } else {
            "Read error in get()"
        },
        version: LIERO_REPLAY_VERSION,
        game: null_mut(),
        settings: null_mut(),
    }))
}

/// # Safety
/// `r` comes from `liero_replay_reader_new`, or is null, and isn't used
/// again.
pub unsafe fn liero_replay_reader_free(r: *mut LieroReplayReader) {
    unsafe {
        if !r.is_null() {
            drop(Box::from_raw(r));
        }
    }
}

/// How far playback has read, to go back to with `_set_pos`.
pub fn liero_replay_reader_pos(r: &LieroReplayReader) -> usize {
    r.pos
}

pub fn liero_replay_reader_set_pos(r: &mut LieroReplayReader, pos: usize) {
    r.pos = pos;
}

/// `beginPlayback`: reads the game the replay starts with into `game`, a
/// new one with no worms or viewports, and its new settings and level.
/// They must stay while the reader plays them. On an error they are left
/// as they were.
///
/// # Safety
/// The game has its settings and TC (`liero_game_set_context`).
pub unsafe fn liero_replay_reader_begin(r: &mut LieroReplayReader, game: &mut LieroGame) -> LieroReplayOutcome {
    unsafe {
        let settings = &mut *liero_game_settings(game);
        let level = &mut *liero_game_level(game);
        let tc = &*game.tc_data();
        let (version, g) = match read_start(&mut r.source()) {
            Ok(read) => read,
            Err(e) => return r.outcome(Err(e)),
        };
        r.version = version;
        r.game = game;
        r.settings = settings;

        settings.set_settings(&g.settings);
        let d = &mut *liero_game_data(game);
        d.cycles = g.fields.cycles;
        d.got_changed = g.fields.got_changed;
        d.last_killed_idx = g.fields.last_killed;
        d.screen_flash = g.fields.screen_flash;
        d.rand.x = g.fields.rand_x;
        d.rand.c = g.fields.rand_c;

        // Every worm read, including any only referred to, which the game
        // keeps without listing.
        let all: Vec<_> = g.worms.iter().map(|_| liero_game_new_worm(game, null_mut())).collect();
        for (&id, rw) in all.iter().zip(&g.worms) {
            let w = game.worm_mut(id);
            liero_replay_worm_set(&mut w.data, &rw.fields, &all);
            let ws = liero_worm_settings_box_new();
            (*ws).set_worm(&rw.settings);
            liero_worm_set_settings(w, ws);
            liero_worm_settings_box_free(ws);
        }
        for &i in &g.list {
            liero_game_list_worm(game, usize::try_from(i).ok().map(|i| all[i]));
        }

        for vp in &g.viewports {
            let rect = LieroRect {
                x1: 0,
                y1: 0,
                x2: 0,
                y2: 0,
            };
            let p = liero_game_add_viewport(game, rect, 0, 0, 0, LIERO_VIEWPORT_WORM, false);
            liero_replay_viewport_set(&mut *p, vp);
        }

        level.resize(g.level.width as i32, g.level.height as i32);
        level.set_pixels(&g.level.pixels, |p| tc.materials[p as usize]);
        for (i, c) in level.data_mut().origpal.iter_mut().enumerate() {
            c.r = g.level.palette[i * 3];
            c.g = g.level.palette[i * 3 + 1];
            c.b = g.level.palette[i * 3 + 2];
        }
        r.outcome(Ok(()))
    }
}

/// `playbackFrame` without the renderer: sets the worms' controls for the
/// tick and any settings the frame changes, then checks the checksum due.
/// What was read stays, even when the frame broke off.
///
/// # Safety
/// The game and settings `liero_replay_reader_begin` had are still valid.
pub unsafe fn liero_replay_reader_frame(r: &mut LieroReplayReader) -> LieroReplayOutcome {
    unsafe {
        let game = &mut *r.game;
        let settings = &mut *r.settings;
        let worms = game_worms(game);
        let mut s = settings.settings();
        let mut fw: Vec<_> = worms
            .iter()
            .map(|&p| frame_worm(p, false, worm_settings(p).worm()))
            .collect();
        let mut touched = vec![false; 1 + worms.len()];
        let version = r.version;
        let read = read_frame(&mut r.source(), version, &mut s, &mut fw, &mut touched);

        // The game's settings first, as their worms can be the worms' own.
        if touched[0] {
            settings.set_settings(&s);
        }
        for (i, (&p, (f, ws))) in worms.iter().zip(&fw).enumerate() {
            if touched[1 + i] {
                worm_settings(p).set_worm(ws);
            }
            (*p).data.control_states = f.control_states;
        }

        let frame = match read {
            Ok(frame) => frame,
            Err(e) => return r.outcome(Err(e)),
        };
        if frame.end {
            let mut o = r.outcome(Ok(()));
            o.end = true;
            return o;
        }
        let cycles = (*liero_game_data(game)).cycles;
        let sum = game_checksum(game);
        let checked = read_checksum(&mut r.source(), cycles, sum);
        let mut o = r.outcome(checked);
        o.settings_changed = frame.settings_changed;
        o
    }
}

#[cfg(test)]
#[path = "replay_file_tests.rs"]
mod tests;
