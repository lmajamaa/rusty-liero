//! Replay files as the game records and plays them (C++'s `ReplayWriter`
//! and `ReplayReader`): the format in `replay.rs`, worked straight on the
//! game, its settings and its level.
//!
//! A writer compresses as it goes and ends the file when it's dropped. It
//! remembers which worms' settings it has written, and the settings'
//! hashes when the game loses focus, so that settings changed in the menus
//! are written again. A reader inflates the whole file first, then plays
//! it from a position the replay viewer's R can go back to. Each call
//! takes the game it records or plays.
//!
//! Errors come back as a `LieroReplayOutcome`, which the controllers and
//! the hash run turn into the messages C++'s replay code threw.

use liero_data::gash::Hash;
use liero_data::settings::{self as data, ReadError, Settings, Source, WormSettings};

use crate::deflate::{inflate, Deflater};
use crate::fs::Node;
use crate::game::LieroRect;
use crate::game_box::{
    liero_game_add_viewport, liero_game_list_worm, liero_game_new_worm, liero_worm_set_settings, LieroGame, LieroWorm,
};
use crate::replay::{
    checksum, liero_replay_viewport_of, liero_replay_viewport_set, liero_replay_worm_of, liero_replay_worm_set,
    read_checksum, read_frame, read_start, write_end, write_frame, write_start, Game, Level, LieroReplayFrameWorm,
    LieroReplayGameFields, LieroReplayStatus, ReplayError, Worm, LIERO_REPLAY_VERSION,
};
use crate::settings::{new_worm_settings, LieroSettingsBox, LieroWormSettingsBox};
use crate::tc::tc_data;
use crate::viewport::LIERO_VIEWPORT_WORM;
use crate::worm::WormId;

/// How a replay call ended, to report as C++'s replay code threw it.
#[derive(Clone, Copy)]
pub struct LieroReplayOutcome {
    pub status: LieroReplayStatus,
    /// For `CheckError`, the `gvl::archive_check_error` message; for
    /// `ReadError`, what the reader threw at the end of its bytes.
    pub message: &'static str,
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
            message,
            end: false,
            settings_changed: false,
        }
    }
}

/// The ids of the game's worms, in order. Recorded and played games list
/// no empty places.
fn game_worm_ids(game: &LieroGame) -> Vec<WormId> {
    game.listed_ids()
        .iter()
        .map(|id| id.expect("an empty place among a replay's worms"))
        .collect()
}

/// `fastGameChecksum`: the generator and the worms' positions and
/// velocities.
fn game_checksum(game: &LieroGame) -> u32 {
    let worms: Vec<[i32; 4]> = game_worm_ids(game)
        .iter()
        .map(|&id| {
            let d = &game.worm(id).data;
            [d.pos.x, d.pos.y, d.vel.x, d.vel.y]
        })
        .collect();
    checksum(game.data().rand.x, &worms)
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
fn frame_worm(w: &LieroWorm, settings_expired: bool, settings: WormSettings) -> (LieroReplayFrameWorm, WormSettings) {
    let d = &w.data;
    let f = LieroReplayFrameWorm {
        index: d.index,
        control_states: d.control_states,
        prev_control_states: d.prev_control_states,
        settings_expired,
    };
    (f, settings)
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

/// A replay being recorded (`ReplayWriter`). It records one game.
pub struct LieroReplayWriter {
    out: Deflater,
    version: i32,
    settings_expired: bool,
    last_settings_hash: Hash,
    /// Each worm the writer has seen, in the order C++'s map from `Worm*`
    /// got them.
    worms: Vec<(WormId, WormData)>,
}

impl Drop for LieroReplayWriter {
    /// `endRecord`, then the stream is finished as the deflater drops.
    fn drop(&mut self) {
        self.out.write(&write_end());
    }
}

impl LieroReplayWriter {
    /// Starts writing a replay at `node`, or None if it can't be created.
    pub fn new(node: &Node) -> Option<Self> {
        Some(LieroReplayWriter {
            out: Deflater::new(node)?,
            version: LIERO_REPLAY_VERSION,
            settings_expired: true,
            last_settings_hash: Hash::default(),
            worms: Vec::new(),
        })
    }

    /// The worm's entry, made as `std::map::operator[]` made it.
    fn worm(&mut self, id: WormId) -> &mut WormData {
        let i = match self.worms.iter().position(|&(w, _)| w == id) {
            Some(i) => i,
            None => {
                self.worms.push((id, WormData::default()));
                self.worms.len() - 1
            }
        };
        &mut self.worms[i].1
    }

    /// The version to write, before `begin`; the hash run writes old ones.
    pub fn set_version(&mut self, version: i32) {
        self.version = version;
    }

    /// `beginRecord`: writes the game as it is, with its settings, level and
    /// the TC's weapons. Writing changes the settings as `write_start` says;
    /// with an `OutOfRange` outcome, what was written stays. The game has
    /// its settings (`liero_game_set_context`).
    pub fn begin(&mut self, game: &mut LieroGame) -> LieroReplayOutcome {
        let settings = game.settings();
        let ids = game_worm_ids(game);
        let d = game.data();
        let level = game.level_box();
        let l = level.data();
        let mut palette = [0u8; 768];
        for (i, c) in l.origpal.iter().enumerate() {
            palette[i * 3..i * 3 + 3].copy_from_slice(&[c.r, c.g, c.b]);
        }
        let mut g = Game {
            settings: settings.borrow().settings(),
            fields: LieroReplayGameFields {
                cycles: d.cycles,
                got_changed: d.got_changed,
                last_killed: d.last_killed_idx,
                screen_flash: d.screen_flash,
                rand_x: d.rand.x,
                rand_c: d.rand.c,
            },
            worms: ids
                .iter()
                .map(|&id| {
                    let w = game.worm(id);
                    Worm {
                        fields: liero_replay_worm_of(&w.data, &ids),
                        settings: w.settings().worm(),
                    }
                })
                .collect(),
            list: (0..ids.len() as i32).collect(),
            viewports: game
                .split_viewports()
                .iter()
                .map(|v| liero_replay_viewport_of(v))
                .collect(),
            level: Level {
                width: l.level.width as u32,
                height: l.level.height as u32,
                pixels: level.pixels().to_vec(),
                palette,
            },
        };
        let (bytes, r) = write_start(&mut g, self.version);

        // The game's settings first: their worms can be the worms' own.
        settings.borrow_mut().set_settings(&g.settings);
        for (&id, rw) in ids.iter().zip(&g.worms) {
            game.worm_mut(id).settings_mut().set_worm(&rw.settings);
        }
        self.out.write(&bytes);
        if r.is_err() {
            return LieroReplayOutcome::new(r, "");
        }

        // They were just written, so they're up to date.
        for &id in &ids {
            self.worm(id).expired = false;
        }
        self.settings_expired = false;
        LieroReplayOutcome::new(Ok(()), "")
    }

    /// `recordFrame`: the game's controls this tick, and any settings that
    /// changed since they were written (a worm's only with more than three
    /// worms).
    pub fn frame(&mut self, game: &mut LieroGame) {
        let settings = game.settings();
        let ids = game_worm_ids(game);
        let with_settings = ids.len() > 3;
        let mut fw: Vec<_> = ids
            .iter()
            .map(|&id| {
                let expired = with_settings && self.worm(id).expired;
                let w = game.worm(id);
                // Converting the settings only when they're written keeps
                // frames cheap.
                let ws = if expired {
                    w.settings().worm()
                } else {
                    WormSettings::default()
                };
                frame_worm(w, expired, ws)
            })
            .collect();
        let mut s = if self.settings_expired {
            settings.borrow().settings()
        } else {
            Settings::default()
        };
        let sum = game_checksum(game);
        let cycles = game.data().cycles;
        let (bytes, game_written, worms_written) =
            write_frame(self.version, &mut s, self.settings_expired, &mut fw, cycles, sum);

        if game_written {
            settings.borrow_mut().set_settings(&s);
            self.settings_expired = false;
        }
        for ((&id, (_, ws)), written) in ids.iter().zip(&fw).zip(worms_written) {
            if written {
                game.worm_mut(id).settings_mut().set_worm(ws);
                self.worm(id).expired = false;
            }
        }
        self.out.write(&bytes);
    }

    /// The game lost focus (the menu opened): remembers the settings'
    /// hashes.
    pub fn unfocus(&mut self, game: &mut LieroGame) {
        for (id, d) in self.worms.iter_mut() {
            d.last_hash = worm_hash(&mut game.worm_mut(*id).settings_mut());
        }
        self.last_settings_hash = settings_hash(&mut game.settings_box());
    }

    /// The game has focus again: settings that changed are written with the
    /// next frame.
    pub fn focus(&mut self, game: &mut LieroGame) {
        for (id, d) in self.worms.iter_mut() {
            if d.last_hash != worm_hash(&mut game.worm_mut(*id).settings_mut()) {
                d.expired = true;
            }
        }
        if self.last_settings_hash != settings_hash(&mut game.settings_box()) {
            self.settings_expired = true;
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
    /// Reads and inflates the replay at `node`, or None if it can't be read.
    pub fn new(node: &Node) -> Option<Self> {
        let file = node.read()?;
        let r = inflate(&file);
        Some(LieroReplayReader {
            data: r.data,
            pos: 0,
            end_error: if r.error {
                "Error while deflating"
            } else {
                "Read error in get()"
            },
            version: LIERO_REPLAY_VERSION,
        })
    }

    fn source(&mut self) -> ReaderSource<'_> {
        ReaderSource {
            data: &self.data,
            pos: &mut self.pos,
        }
    }

    fn outcome(&self, r: Result<(), ReplayError>) -> LieroReplayOutcome {
        LieroReplayOutcome::new(r, self.end_error)
    }

    /// How far playback has read, to go back to with `set_pos`.
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// `beginPlayback`: reads the game the replay starts with into `game`, a
    /// new one with no worms or viewports, and its new settings and level.
    /// On an error they are left as they were. The game has its settings
    /// and TC (`liero_game_set_context`).
    pub fn begin(&mut self, game: &mut LieroGame) -> LieroReplayOutcome {
        let (version, g) = match read_start(&mut self.source()) {
            Ok(read) => read,
            Err(e) => return self.outcome(Err(e)),
        };
        self.version = version;

        game.settings_box().set_settings(&g.settings);
        let d = game.data_mut();
        d.cycles = g.fields.cycles;
        d.got_changed = g.fields.got_changed;
        d.last_killed_idx = g.fields.last_killed;
        d.screen_flash = g.fields.screen_flash;
        d.rand.x = g.fields.rand_x;
        d.rand.c = g.fields.rand_c;

        // Every worm read, including any only referred to, which the game
        // keeps without listing.
        let all: Vec<_> = g.worms.iter().map(|_| liero_game_new_worm(game, None)).collect();
        for (&id, rw) in all.iter().zip(&g.worms) {
            let w = game.worm_mut(id);
            liero_replay_worm_set(&mut w.data, &rw.fields, &all);
            let ws = new_worm_settings();
            ws.borrow_mut().set_worm(&rw.settings);
            liero_worm_set_settings(w, Some(ws));
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
            liero_replay_viewport_set(p, vp);
        }

        let tc = game.tc();
        let tc = tc_data(&tc);
        let level = game.level_box_mut();
        level.resize(g.level.width as i32, g.level.height as i32);
        level.set_pixels(&g.level.pixels, |p| tc.materials[p as usize]);
        for (i, c) in level.data_mut().origpal.iter_mut().enumerate() {
            c.r = g.level.palette[i * 3];
            c.g = g.level.palette[i * 3 + 1];
            c.b = g.level.palette[i * 3 + 2];
        }
        self.outcome(Ok(()))
    }

    /// `playbackFrame` without the renderer: sets the worms' controls for
    /// the tick and any settings the frame changes, then checks the
    /// checksum due. What was read stays, even when the frame broke off.
    pub fn frame(&mut self, game: &mut LieroGame) -> LieroReplayOutcome {
        let settings = game.settings();
        let ids = game_worm_ids(game);
        let mut s = settings.borrow().settings();
        let mut fw: Vec<_> = ids
            .iter()
            .map(|&id| {
                let w = game.worm(id);
                frame_worm(w, false, w.settings().worm())
            })
            .collect();
        let mut touched = vec![false; 1 + ids.len()];
        let version = self.version;
        let read = read_frame(&mut self.source(), version, &mut s, &mut fw, &mut touched);

        // The game's settings first, as their worms can be the worms' own.
        if touched[0] {
            settings.borrow_mut().set_settings(&s);
        }
        for (i, (&id, (f, ws))) in ids.iter().zip(&fw).enumerate() {
            let w = game.worm_mut(id);
            if touched[1 + i] {
                w.settings_mut().set_worm(ws);
            }
            w.data.control_states = f.control_states;
        }

        let frame = match read {
            Ok(frame) => frame,
            Err(e) => return self.outcome(Err(e)),
        };
        if frame.end {
            let mut o = self.outcome(Ok(()));
            o.end = true;
            return o;
        }
        let cycles = game.data().cycles;
        let sum = game_checksum(game);
        let checked = read_checksum(&mut self.source(), cycles, sum);
        let mut o = self.outcome(checked);
        o.settings_changed = frame.settings_changed;
        o
    }
}

#[cfg(test)]
#[path = "replay_file_tests.rs"]
mod tests;
