//! Replays: the .lrp format of `src/game/replay.cpp`. A replay starts with
//! the game as it was when recording began, written with gvl's archive, then
//! has each frame's control changes, settings changes and a checksum every
//! 15 seconds.
//!
//! `replay_file.rs` reads and writes replay files with these, on the game
//! itself.
//!
//! gvl's archive writes a worm as a number the first time it comes up, with
//! the whole worm after it; later the number alone refers back to it. So a
//! worm can appear inside another one, as its rope's anchor or as the worm
//! that last killed it, before it appears in the game's list of worms.

use crate::game::LieroRect;
use crate::math::LieroIVec2;
use crate::tc::{LieroTcData, LieroTcWeapon};
use crate::viewport::LieroViewport;
use crate::worm::{LieroWormData, WormId};
use liero_data::settings::{self as data, ArchiveError, In, Out, ReadError, Settings, Source, WormSettings};
use std::collections::HashMap;
use std::ffi::c_void;

/// "LRPF".
const MAGIC: u32 = u32::from_be_bytes(*b"LRPF");
/// The newest version this reads, and the one the game writes.
pub const LIERO_REPLAY_VERSION: i32 = data::REPLAY_VERSION;
/// The checkpoint at the end of each worm.
const CHECKPOINT: u32 = 0x1234_5678;
/// Frames between checksums: 15 seconds.
const CHECKSUM_INTERVAL: i32 = 70 * 15;

/// Frame headers. A byte below 0x80 is the first worm's control changes.
const EMPTY_FRAME: u8 = 0x80;
const GAME_SETTINGS: u8 = 0x81;
const WORM_SETTINGS: u8 = 0x82;
const END: u8 = 0x83;

/// Why a replay call failed.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LieroReplayStatus {
    Ok,
    /// The byte source failed; C++ rethrows what its reader threw.
    ReadError,
    /// `gvl::archive_check_error`, with `liero_replay_result_message`.
    CheckError,
    /// A worm index past the game's worms, which `std::vector::at` threw
    /// `std::out_of_range` for.
    OutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplayError {
    Read,
    Check(&'static str),
    OutOfRange,
}

impl From<ReadError> for ReplayError {
    fn from(_: ReadError) -> Self {
        ReplayError::Read
    }
}

impl From<ArchiveError> for ReplayError {
    fn from(e: ArchiveError) -> Self {
        match e {
            ArchiveError::Read => ReplayError::Read,
            ArchiveError::Check => ReplayError::Check("Expected checkpoint here"),
        }
    }
}

/// A worm weapon's state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroReplayWeapon {
    pub ammo: i32,
    pub delay_left: i32,
    /// The weapon's index in the TC.
    pub weapon: i32,
    pub loading_left: i32,
}

/// What a replay stores of a `Worm`, in the archive's order. Worms refer to
/// each other by their place in the list of worms passed or returned, or -1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroReplayWorm {
    pub pos: [i32; 2],
    pub vel: [i32; 2],
    pub logic_respawn: [i32; 2],
    pub hotspot: [i32; 2],
    pub aiming_angle: i32,
    pub aiming_speed: i32,
    pub able_to_jump: bool,
    pub able_to_dig: bool,
    pub key_change_pressed: bool,
    pub movable: bool,
    pub animate: bool,
    pub visible: bool,
    pub ready: bool,
    pub flag: bool,
    pub make_sight_green: bool,
    pub health: i32,
    pub lives: i32,
    pub kills: i32,
    pub timer: i32,
    pub killed_timer: i32,
    pub current_frame: i32,
    pub flags: i32,
    /// The worm the rope is attached to.
    pub rope_anchor: i32,
    pub rope_attached: bool,
    pub rope_cur_len: i32,
    pub rope_length: i32,
    pub rope_out: bool,
    pub rope_vel: [i32; 2],
    pub rope_pos: [i32; 2],
    pub current_weapon: i32,
    /// `lastKilledByIdx`: a worm's `index`, not its place in the list.
    pub last_killed_by: i32,
    pub fire_cone: i32,
    pub leave_shell_timer: i32,
    pub index: i32,
    pub reacts: [i32; 4],
    pub weapons: [LieroReplayWeapon; 5],
    pub direction: i32,
    pub control_states: u32,
    pub prev_control_states: u32,
}

/// What a replay stores of a `Viewport`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroReplayViewport {
    pub x: i32,
    pub y: i32,
    pub shake: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub center_x: i32,
    pub center_y: i32,
    /// The followed worm's `index`.
    pub worm_idx: i32,
    pub banner_y: i32,
    pub rect: [i32; 4],
}

/// The `Game` fields a replay stores, apart from the settings, worms,
/// viewports and level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroReplayGameFields {
    pub cycles: i32,
    pub got_changed: bool,
    /// `lastKilledIdx`: a worm's `index`.
    pub last_killed: i32,
    pub screen_flash: i32,
    pub rand_x: u32,
    pub rand_c: u32,
}

/// A worm with its settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Worm {
    pub fields: LieroReplayWorm,
    pub settings: WormSettings,
}

/// The level's pixels and palette (`origpal`).
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub palette: [u8; 768],
}

/// A game as a replay starts with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub settings: Settings,
    pub fields: LieroReplayGameFields,
    /// Every worm, as places that `list` and the worms' references use.
    pub worms: Vec<Worm>,
    /// The game's worms in order. Reading can also give worms that are only
    /// referred to, or -1 where the list had none.
    pub list: Vec<i32>,
    pub viewports: Vec<LieroReplayViewport>,
    pub level: Level,
}

// ---- Writing ----

/// Writes the game archive (`archive(out_archive, Game&)`). Like the C++
/// archive this changes the settings: the TC name resets, and replay
/// versions 0 and 1 clear the worms' extended controls.
struct GameWriter<'a> {
    w: Out,
    version: i32,
    game: &'a mut Game,
    /// The number given to each worm written so far, by place.
    ids: HashMap<usize, u32>,
    next_id: u32,
}

impl GameWriter<'_> {
    fn i32(&mut self, v: i32) {
        self.w.u32(v as u32);
    }

    /// A worm by place, written in full the first time.
    fn worm_ref(&mut self, worm: Option<usize>) -> Result<(), ReplayError> {
        let Some(h) = worm else {
            self.w.u32(0);
            return Ok(());
        };
        if let Some(&id) = self.ids.get(&h) {
            self.w.u32(id);
            return Ok(());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert(h, id);
        self.w.u32(id);
        self.worm(h)
    }

    /// The worm at `index` in the game's list, as `std::vector::at` finds it.
    fn listed(&self, index: i32) -> Result<Option<usize>, ReplayError> {
        if index < 0 {
            return Ok(None);
        }
        match self.game.list.get(index as usize) {
            Some(&h) if h >= 0 => Ok(Some(h as usize)),
            Some(_) => Ok(None),
            None => Err(ReplayError::OutOfRange),
        }
    }

    fn worm(&mut self, h: usize) -> Result<(), ReplayError> {
        let f = self.game.worms[h].fields;
        for v in [f.pos, f.vel, f.logic_respawn, f.hotspot].concat() {
            self.i32(v);
        }
        self.i32(f.aiming_angle);
        self.i32(f.aiming_speed);
        for b in [
            f.able_to_jump,
            f.able_to_dig,
            f.key_change_pressed,
            f.movable,
            f.animate,
            f.visible,
            f.ready,
            f.flag,
            f.make_sight_green,
        ] {
            self.w.b(b);
        }
        for v in [
            f.health,
            f.lives,
            f.kills,
            f.timer,
            f.killed_timer,
            f.current_frame,
            f.flags,
        ] {
            self.i32(v);
        }

        self.worm_ref(usize::try_from(f.rope_anchor).ok())?;
        self.w.b(f.rope_attached);
        self.i32(f.rope_cur_len);
        self.i32(f.rope_length);
        self.w.b(f.rope_out);
        for v in [f.rope_vel, f.rope_pos].concat() {
            self.i32(v);
        }
        self.i32(f.current_weapon);
        self.w.b(false);
        let killer = self.listed(f.last_killed_by)?;
        self.worm_ref(killer)?;
        self.i32(f.fire_cone);
        self.i32(f.leave_shell_timer);
        let bytes = data::write_worm(&mut self.game.worms[h].settings, self.version);
        self.w.0.extend_from_slice(&bytes);
        self.i32(f.index);
        for v in f.reacts {
            self.i32(v);
        }

        for w in f.weapons {
            self.i32(w.ammo);
            self.w.b(false);
            self.i32(w.delay_left);
            self.i32(w.weapon);
            self.i32(w.loading_left);
        }

        self.w.u8(f.direction as u32);
        self.w.u8(f.control_states);
        self.w.u8(f.prev_control_states);
        self.w.u32(CHECKPOINT);
        Ok(())
    }

    fn viewport(&mut self, vp: LieroReplayViewport) -> Result<(), ReplayError> {
        for v in [vp.x, vp.y, vp.shake, vp.max_x, vp.max_y, vp.center_x, vp.center_y] {
            self.i32(v);
        }
        let worm = self.listed(vp.worm_idx)?;
        self.worm_ref(worm)?;
        self.i32(vp.banner_y);
        // Where the unused ingameX was.
        self.i32(0);
        for v in vp.rect {
            self.i32(v);
        }
        // Where an unused generator's state was.
        self.w.u32(0);
        self.w.u32(0);
        Ok(())
    }

    fn palette(&mut self) {
        let palette = self.game.level.palette;
        self.w.0.extend_from_slice(&palette);
    }

    fn game(&mut self) -> Result<(), ReplayError> {
        let bytes = data::write_archive(&mut self.game.settings);
        self.w.0.extend_from_slice(&bytes);

        let f = self.game.fields;
        self.i32(f.cycles);
        self.w.b(f.got_changed);
        let last = self.listed(f.last_killed)?;
        self.worm_ref(last)?;
        self.i32(f.screen_flash);
        self.w.u32(f.rand_c);
        self.w.u32(f.rand_x);

        for i in 0..self.game.list.len() {
            self.w.0.push(1);
            let h = self.game.list[i];
            self.worm_ref(usize::try_from(h).ok())?;
        }
        self.w.0.push(0);
        for i in 0..self.game.viewports.len() {
            self.w.0.push(1);
            let vp = self.game.viewports[i];
            self.viewport(vp)?;
        }
        self.w.0.push(0);

        let (width, height) = (self.game.level.width, self.game.level.height);
        self.w.u16(width);
        self.w.u16(height);
        if self.version > 1 {
            self.palette();
        }
        let pixels = std::mem::take(&mut self.game.level.pixels);
        self.w.0.extend_from_slice(&pixels);
        self.game.level.pixels = pixels;
        self.palette();
        Ok(())
    }
}

/// The start of a replay of `version`: its header and the game
/// (`ReplayWriter::beginRecord`). On an error, the bytes written before it.
pub fn write_start(game: &mut Game, version: i32) -> (Vec<u8>, Result<(), ReplayError>) {
    let mut w = Out(Vec::new());
    w.u32(MAGIC);
    // `put` keeps the low byte.
    w.0.push(version as u8);
    let mut gw = GameWriter {
        w,
        version,
        game,
        ids: HashMap::new(),
        next_id: 1,
    };
    let r = gw.game();
    (gw.w.0, r)
}

/// What a frame writes or reads of a worm, beside its settings.
#[derive(Clone, Copy)]
pub struct LieroReplayFrameWorm {
    pub index: i32,
    pub control_states: u32,
    pub prev_control_states: u32,
    /// Whether the worm's settings changed since they were last written.
    pub settings_expired: bool,
}

/// The checksum a replay stores every 15 seconds (`fastGameChecksum`).
pub fn checksum(rand_x: u32, worms: &[[i32; 4]]) -> u32 {
    let mut c = rand_x;
    for &[x, y, vx, vy] in worms {
        c ^= x as u32;
        c = c.wrapping_add(vx as u32);
        c ^= y as u32;
        c = c.wrapping_add(vy as u32);
    }
    c
}

/// A frame (`ReplayWriter::recordFrame`): the game's settings if they
/// changed, each worm's with more than three worms, and the control
/// changes. Writing settings changes them as `write_start` does; the
/// returned flags say which were written.
pub fn write_frame(
    version: i32,
    settings: &mut Settings,
    settings_expired: bool,
    worms: &mut [(LieroReplayFrameWorm, WormSettings)],
    cycles: i32,
    checksum: u32,
) -> (Vec<u8>, bool, Vec<bool>) {
    let mut w = Out(Vec::new());
    if settings_expired {
        w.0.push(GAME_SETTINGS);
        w.0.extend_from_slice(&data::write_archive(settings));
    }

    let mut worms_written = vec![false; worms.len()];
    let write_states = worms.len() <= 3
        || worms
            .iter_mut()
            .zip(&mut worms_written)
            .fold(false, |changed, ((f, ws), written)| {
                if f.settings_expired {
                    w.0.push(WORM_SETTINGS);
                    w.u32(f.index as u32);
                    w.0.extend_from_slice(&data::write_worm(ws, version));
                    *written = true;
                }
                changed || f.control_states != f.prev_control_states
            });

    if write_states {
        for (f, _) in worms.iter() {
            let state = f.control_states ^ f.prev_control_states;
            // C++ checked this with `sassert`, only in debug builds.
            debug_assert!(state < 0x80, "control changes {state:#x} don't fit in a frame");
            w.0.push(state as u8);
        }
    } else {
        w.0.push(EMPTY_FRAME);
    }

    if cycles % CHECKSUM_INTERVAL == 0 {
        w.u32(checksum);
    }
    (w.0, settings_expired, worms_written)
}

/// The end of a replay.
pub fn write_end() -> Vec<u8> {
    vec![END]
}

// ---- Reading ----

struct GameReader<'a, S: Source + ?Sized> {
    r: In<'a, S>,
    version: i32,
    game: Game,
    /// The place of each worm number read so far.
    ids: HashMap<u32, usize>,
}

impl<S: Source + ?Sized> GameReader<'_, S> {
    fn i32(&mut self) -> Result<i32, ReplayError> {
        Ok(self.r.u32()? as i32)
    }

    fn pair(&mut self) -> Result<[i32; 2], ReplayError> {
        Ok([self.i32()?, self.i32()?])
    }

    /// A worm by number, read in full the first time. A new worm gets its
    /// place before it's read, as C++ created it first, so a reference back
    /// to it from inside finds it with the fields read so far.
    fn worm_ref(&mut self) -> Result<Option<usize>, ReplayError> {
        let id = self.r.u32()?;
        if id == 0 {
            return Ok(None);
        }
        if let Some(&h) = self.ids.get(&id) {
            return Ok(Some(h));
        }
        let h = self.game.worms.len();
        self.game.worms.push(Worm::default());
        self.ids.insert(id, h);
        let worm = self.worm()?;
        self.game.worms[h] = worm;
        Ok(Some(h))
    }

    /// The `index` of the worm referred to, or -1.
    fn worm_index(&mut self) -> Result<i32, ReplayError> {
        Ok(match self.worm_ref()? {
            Some(h) => self.game.worms[h].fields.index,
            None => -1,
        })
    }

    fn b(&mut self) -> Result<bool, ReplayError> {
        Ok(self.r.b()?)
    }

    fn worm(&mut self) -> Result<Worm, ReplayError> {
        let mut f = LieroReplayWorm {
            pos: self.pair()?,
            vel: self.pair()?,
            logic_respawn: self.pair()?,
            hotspot: self.pair()?,
            aiming_angle: self.i32()?,
            aiming_speed: self.i32()?,
            able_to_jump: self.b()?,
            able_to_dig: self.b()?,
            key_change_pressed: self.b()?,
            movable: self.b()?,
            animate: self.b()?,
            visible: self.b()?,
            ready: self.b()?,
            flag: self.b()?,
            make_sight_green: self.b()?,
            health: self.i32()?,
            lives: self.i32()?,
            kills: self.i32()?,
            timer: self.i32()?,
            killed_timer: self.i32()?,
            current_frame: self.i32()?,
            flags: self.i32()?,
            ..Default::default()
        };
        f.rope_anchor = self.worm_ref()?.map_or(-1, |h| h as i32);
        f.rope_attached = self.b()?;
        f.rope_cur_len = self.i32()?;
        f.rope_length = self.i32()?;
        f.rope_out = self.b()?;
        f.rope_vel = self.pair()?;
        f.rope_pos = self.pair()?;
        f.current_weapon = self.i32()?;
        self.b()?;
        f.last_killed_by = self.worm_index()?;
        f.fire_cone = self.i32()?;
        f.leave_shell_timer = self.i32()?;
        let mut settings = WormSettings::default();
        data::read_worm(&mut settings, self.version, self.r.0)?;
        f.index = self.i32()?;
        for r in &mut f.reacts {
            *r = self.i32()?;
        }
        for w in &mut f.weapons {
            w.ammo = self.i32()?;
            self.b()?;
            w.delay_left = self.i32()?;
            w.weapon = self.i32()?;
            w.loading_left = self.i32()?;
        }
        f.direction = self.r.u8()? as i32;
        f.control_states = self.r.u8()?;
        f.prev_control_states = self.r.u8()?;
        if self.r.u32()? != CHECKPOINT {
            return Err(ReplayError::Check("Expected checkpoint here"));
        }
        Ok(Worm { fields: f, settings })
    }

    fn viewport(&mut self) -> Result<LieroReplayViewport, ReplayError> {
        let mut vp = LieroReplayViewport {
            x: self.i32()?,
            y: self.i32()?,
            shake: self.i32()?,
            max_x: self.i32()?,
            max_y: self.i32()?,
            center_x: self.i32()?,
            center_y: self.i32()?,
            ..Default::default()
        };
        vp.worm_idx = self.worm_index()?;
        vp.banner_y = self.i32()?;
        self.i32()?;
        for v in &mut vp.rect {
            *v = self.i32()?;
        }
        self.r.u32()?;
        self.r.u32()?;
        Ok(vp)
    }

    fn palette(&mut self, palette: &mut [u8; 768]) -> Result<(), ReplayError> {
        for p in palette.iter_mut() {
            *p = self.r.0.get()?;
        }
        Ok(())
    }

    fn game(&mut self) -> Result<(), ReplayError> {
        data::read_archive(&mut self.game.settings, self.r.0)?;
        self.game.fields.cycles = self.i32()?;
        self.game.fields.got_changed = self.b()?;
        self.game.fields.last_killed = self.worm_index()?;
        self.game.fields.screen_flash = self.i32()?;
        self.game.fields.rand_c = self.r.u32()?;
        self.game.fields.rand_x = self.r.u32()?;

        while self.r.u8()? != 0 {
            let h = self.worm_ref()?;
            self.game.list.push(h.map_or(-1, |h| h as i32));
        }
        while self.r.u8()? != 0 {
            let vp = self.viewport()?;
            self.game.viewports.push(vp);
        }

        let width = self.r.u16()?;
        let height = self.r.u16()?;
        let mut palette = [0; 768];
        if self.version > 1 {
            self.palette(&mut palette)?;
        }
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for _ in 0..width * height {
            pixels.push(self.r.0.get()?);
        }
        self.palette(&mut palette)?;
        self.game.level = Level {
            width,
            height,
            pixels,
            palette,
        };
        Ok(())
    }
}

fn empty_game() -> Game {
    Game {
        settings: Settings::default(),
        fields: LieroReplayGameFields::default(),
        worms: Vec::new(),
        list: Vec::new(),
        viewports: Vec::new(),
        level: Level {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            palette: [0; 768],
        },
    }
}

/// The start of a replay (`ReplayReader::beginPlayback`): its version and
/// the game.
pub fn read_start<S: Source + ?Sized>(src: &mut S) -> Result<(i32, Game), ReplayError> {
    let mut r = In(src);
    if r.u32()? != MAGIC {
        return Err(ReplayError::Check("File does not appear to be a replay"));
    }
    let version = r.u8()? as i32;
    if version > LIERO_REPLAY_VERSION {
        return Err(ReplayError::Check("Replay version is too recent"));
    }
    let mut gr = GameReader {
        r,
        version,
        game: empty_game(),
        ids: HashMap::new(),
    };
    gr.game()?;
    Ok((version, gr.game))
}

/// What a frame's headers did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameRead {
    /// The replay ended instead.
    pub end: bool,
    /// The game's settings were read, so the game needs `updateSettings`.
    pub settings_changed: bool,
}

/// Reads a frame (`ReplayReader::playbackFrame` up to the checksum): any
/// settings before it, then each worm's control changes. Settings are read
/// in place and the worms' controls set as they're read, so on an error
/// what was read stays, as in C++. `touched` says what changed: the game's
/// settings, then each worm's settings.
pub fn read_frame<S: Source + ?Sized>(
    src: &mut S,
    version: i32,
    settings: &mut Settings,
    worms: &mut [(LieroReplayFrameWorm, WormSettings)],
    touched: &mut [bool],
) -> Result<FrameRead, ReplayError> {
    let mut r = In(src);
    let mut read = FrameRead::default();
    loop {
        let first = r.0.get()?;
        match first {
            EMPTY_FRAME => return Ok(read),
            GAME_SETTINGS => {
                touched[0] = true;
                data::read_archive(settings, r.0)?;
                read.settings_changed = true;
            }
            WORM_SETTINGS => {
                // `wormByIdx` takes an int; C++ read past the worms for an
                // index past them, where this skips the settings as it does
                // for a negative one.
                let id = r.u32()? as i32;
                if let Some((_, ws)) = usize::try_from(id).ok().and_then(|i| worms.get_mut(i)) {
                    touched[1 + id as usize] = true;
                    data::read_worm(ws, version, r.0)?;
                    read.settings_changed = true;
                }
            }
            END => {
                read.end = true;
                return Ok(read);
            }
            0..=0x7f => {
                for (i, (f, _)) in worms.iter_mut().enumerate() {
                    let state = if i == 0 { first } else { r.0.get()? };
                    f.control_states = (state as u32 ^ f.prev_control_states) & 0x7f;
                }
                return Ok(read);
            }
            _ => return Err(ReplayError::Check("Unexpected header byte")),
        }
    }
}

/// The checksum after a frame on a multiple of 15 seconds, which has to
/// match the game's.
pub fn read_checksum<S: Source + ?Sized>(src: &mut S, cycles: i32, actual: u32) -> Result<(), ReplayError> {
    if cycles % CHECKSUM_INTERVAL == 0 && In(src).u32()? != actual {
        return Err(ReplayError::Check("Replay has desynced"));
    }
    Ok(())
}

// ---- The game's worms and viewports, as replays hold them ----

/// Weapon `kind`'s place among the TC's weapons, by address, as C++ took
/// it (`w.type - &common.weapons[0]`).
fn weapon_place(kind: *const c_void, tc: &LieroTcData) -> i32 {
    ((kind as isize).wrapping_sub(tc.weapons.as_ptr() as isize) / std::mem::size_of::<LieroTcWeapon>() as isize) as i32
}

/// A worm as replays hold it (`wormFields`). Its rope's anchor is the
/// anchor's place among `ids`, the game's worms written, or -1.
pub fn liero_replay_worm_of(worm: &LieroWormData, ids: &[WormId], tc: &LieroTcData) -> LieroReplayWorm {
    let anchor = worm.ninjarope.anchor;
    let rope = &worm.ninjarope;
    LieroReplayWorm {
        pos: [worm.pos.x, worm.pos.y],
        vel: [worm.vel.x, worm.vel.y],
        logic_respawn: [worm.logic_respawn.x, worm.logic_respawn.y],
        hotspot: [worm.hotspot_x, worm.hotspot_y],
        aiming_angle: worm.aiming_angle,
        aiming_speed: worm.aiming_speed,
        able_to_jump: worm.able_to_jump,
        able_to_dig: worm.able_to_dig,
        key_change_pressed: worm.key_change_pressed,
        movable: worm.movable,
        animate: worm.animate,
        visible: worm.visible,
        ready: worm.ready,
        flag: worm.flag,
        make_sight_green: worm.make_sight_green,
        health: worm.health,
        lives: worm.lives,
        kills: worm.kills,
        timer: worm.timer,
        killed_timer: worm.killed_timer,
        current_frame: worm.current_frame,
        flags: worm.flags,
        // The game's ropes only attach to its own worms.
        rope_anchor: anchor
            .and_then(|a| ids.iter().position(|&w| w == a))
            .map_or(-1, |i| i as i32),
        rope_attached: rope.attached,
        rope_cur_len: rope.cur_len,
        rope_length: rope.length,
        rope_out: rope.out,
        rope_vel: [rope.vel.x, rope.vel.y],
        rope_pos: [rope.pos.x, rope.pos.y],
        current_weapon: worm.current_weapon,
        last_killed_by: worm.last_killed_by_idx,
        fire_cone: worm.fire_cone,
        leave_shell_timer: worm.leave_shell_timer,
        index: worm.index,
        reacts: worm.reacts,
        weapons: worm.weapons.map(|w| LieroReplayWeapon {
            ammo: w.ammo,
            delay_left: w.delay_left,
            weapon: weapon_place(w.kind, tc),
            loading_left: w.loading_left,
        }),
        direction: worm.direction,
        control_states: worm.control_states,
        prev_control_states: worm.prev_control_states,
    }
}

/// Sets a worm from a replay's (`setWormFields`). Its rope's anchor is one
/// of `all`, every worm the replay has. Weapons aren't checked, as in C++.
pub fn liero_replay_worm_set(worm: &mut LieroWormData, f: &LieroReplayWorm, all: &[WormId], tc: &LieroTcData) {
    worm.pos = LieroIVec2 {
        x: f.pos[0],
        y: f.pos[1],
    };
    worm.vel = LieroIVec2 {
        x: f.vel[0],
        y: f.vel[1],
    };
    worm.logic_respawn = LieroIVec2 {
        x: f.logic_respawn[0],
        y: f.logic_respawn[1],
    };
    worm.hotspot_x = f.hotspot[0];
    worm.hotspot_y = f.hotspot[1];
    worm.aiming_angle = f.aiming_angle;
    worm.aiming_speed = f.aiming_speed;
    worm.able_to_jump = f.able_to_jump;
    worm.able_to_dig = f.able_to_dig;
    worm.key_change_pressed = f.key_change_pressed;
    worm.movable = f.movable;
    worm.animate = f.animate;
    worm.visible = f.visible;
    worm.ready = f.ready;
    worm.flag = f.flag;
    worm.make_sight_green = f.make_sight_green;
    worm.health = f.health;
    worm.lives = f.lives;
    worm.kills = f.kills;
    worm.timer = f.timer;
    worm.killed_timer = f.killed_timer;
    worm.current_frame = f.current_frame;
    worm.flags = f.flags;

    let rope = &mut worm.ninjarope;
    rope.anchor = usize::try_from(f.rope_anchor).ok().map(|i| all[i]);
    rope.attached = f.rope_attached;
    rope.cur_len = f.rope_cur_len;
    rope.length = f.rope_length;
    rope.out = f.rope_out;
    rope.vel = LieroIVec2 {
        x: f.rope_vel[0],
        y: f.rope_vel[1],
    };
    rope.pos = LieroIVec2 {
        x: f.rope_pos[0],
        y: f.rope_pos[1],
    };
    worm.current_weapon = f.current_weapon;
    worm.last_killed_by_idx = f.last_killed_by;
    worm.fire_cone = f.fire_cone;
    worm.leave_shell_timer = f.leave_shell_timer;
    worm.index = f.index;
    worm.reacts = f.reacts;
    for (w, r) in worm.weapons.iter_mut().zip(&f.weapons) {
        w.ammo = r.ammo;
        w.delay_left = r.delay_left;
        w.kind = tc.weapons.as_ptr().wrapping_offset(r.weapon as isize).cast();
        w.loading_left = r.loading_left;
    }
    worm.direction = f.direction;
    worm.control_states = f.control_states;
    worm.prev_control_states = f.prev_control_states;
}

/// A split-screen viewport as replays hold it (`viewportFields`).
pub fn liero_replay_viewport_of(vp: &LieroViewport) -> LieroReplayViewport {
    LieroReplayViewport {
        x: vp.x,
        y: vp.y,
        shake: vp.shake,
        max_x: vp.max_x,
        max_y: vp.max_y,
        center_x: vp.center_x,
        center_y: vp.center_y,
        worm_idx: vp.worm_idx,
        banner_y: vp.banner_y,
        rect: [vp.rect.x1, vp.rect.y1, vp.rect.x2, vp.rect.y2],
    }
}

/// Sets a viewport from a replay's (`setViewportFields`).
pub fn liero_replay_viewport_set(vp: &mut LieroViewport, f: &LieroReplayViewport) {
    vp.x = f.x;
    vp.y = f.y;
    vp.shake = f.shake;
    vp.max_x = f.max_x;
    vp.max_y = f.max_y;
    vp.center_x = f.center_x;
    vp.center_y = f.center_y;
    vp.worm_idx = f.worm_idx;
    vp.banner_y = f.banner_y;
    vp.rect = LieroRect {
        x1: f.rect[0],
        y1: f.rect[1],
        x2: f.rect[2],
        y2: f.rect[3],
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use liero_data::settings::Bytes;

    struct Cursor<'a>(&'a [u8]);

    impl Cursor<'_> {
        fn bytes(&mut self, n: usize) -> Vec<u8> {
            let (a, rest) = self.0.split_at(n);
            self.0 = rest;
            a.to_vec()
        }

        fn words(&mut self, n: usize) -> Vec<i32> {
            self.bytes(n * 4)
                .chunks(4)
                .map(|c| i32::from_be_bytes(c.try_into().unwrap()))
                .collect()
        }
    }

    fn worm(index: i32) -> Worm {
        let mut fields = LieroReplayWorm {
            index,
            rope_anchor: -1,
            last_killed_by: -1,
            health: 100 + index,
            ..Default::default()
        };
        fields.pos = [index * 1000, 2000];
        fields.weapons[2].weapon = 39;
        fields.direction = 1;
        fields.control_states = 0x15;
        fields.prev_control_states = 0x05;
        let settings = WormSettings {
            name: format!("worm {index}").into_bytes(),
            controls_ex: [3; 8],
            ..Default::default()
        };
        Worm { fields, settings }
    }

    fn game(worms: Vec<Worm>) -> Game {
        let mut g = empty_game();
        g.list = (0..worms.len() as i32).collect();
        g.worms = worms;
        g.fields = LieroReplayGameFields {
            cycles: 77,
            got_changed: true,
            last_killed: -1,
            screen_flash: 3,
            rand_x: 0xdead_beef,
            rand_c: 42,
        };
        g.viewports = vec![LieroReplayViewport {
            x: 5,
            worm_idx: 1,
            rect: [0, 0, 158, 158],
            ..Default::default()
        }];
        let mut palette = [0; 768];
        for (i, p) in palette.iter_mut().enumerate() {
            *p = (i % 64) as u8;
        }
        g.level = Level {
            width: 3,
            height: 2,
            pixels: vec![1, 2, 3, 4, 5, 6],
            palette,
        };
        g
    }

    fn read(bytes: &[u8]) -> Result<(i32, Game), ReplayError> {
        read_start(&mut Bytes::new(bytes))
    }

    #[test]
    fn a_game_reads_back_as_written() {
        let mut g = game(vec![worm(0), worm(1)]);
        g.worms[0].fields.rope_anchor = 1;
        g.worms[1].fields.last_killed_by = 0;
        for version in 0..=LIERO_REPLAY_VERSION {
            let mut written = g.clone();
            let (bytes, r) = write_start(&mut written, version);
            assert_eq!(r, Ok(()));
            assert_eq!((&bytes[..4], bytes[4]), (b"LRPF".as_slice(), version as u8));
            let (v, back) = read(&bytes).unwrap();
            assert_eq!(v, version);
            // Writing resets the TC, and the oldest versions drop the
            // extended controls.
            assert_eq!(written.settings.ext.tc, Settings::default().ext.tc);
            assert_eq!(written.worms[0].settings.controls_ex == [3; 8], version > 1);
            // Older versions store less of the worms' settings.
            let fields = |g: &Game| g.worms.iter().map(|w| w.fields).collect::<Vec<_>>();
            assert_eq!(fields(&back), fields(&written));
            if version == LIERO_REPLAY_VERSION {
                assert_eq!(back.worms, written.worms);
            }
            assert_eq!(
                (back.list, back.viewports, back.level, back.fields),
                (written.list, written.viewports, written.level, written.fields)
            );
        }
    }

    #[test]
    fn a_worm_is_written_in_the_cpp_archive_order() {
        // Every number field gets its place in `archive(Archive, Worm&)`, so
        // two swapped fields would show; the hash run's games leave some of
        // them 0.
        let mut n = 0;
        let mut next = || {
            n += 1;
            n
        };
        let mut f = LieroReplayWorm {
            rope_anchor: -1,
            last_killed_by: -1,
            ..Default::default()
        };
        f.pos = [next(), next()];
        f.vel = [next(), next()];
        f.logic_respawn = [next(), next()];
        f.hotspot = [next(), next()];
        f.aiming_angle = next();
        f.aiming_speed = next();
        f.health = next();
        f.lives = next();
        f.kills = next();
        f.timer = next();
        f.killed_timer = next();
        f.current_frame = next();
        f.flags = next();
        f.rope_cur_len = next();
        f.rope_length = next();
        f.rope_vel = [next(), next()];
        f.rope_pos = [next(), next()];
        f.current_weapon = next();
        f.fire_cone = next();
        f.leave_shell_timer = next();
        f.index = next();
        f.reacts = [next(), next(), next(), next()];
        for w in &mut f.weapons {
            *w = LieroReplayWeapon {
                ammo: next(),
                delay_left: next(),
                weapon: next(),
                loading_left: next(),
            };
        }
        f.direction = next();
        f.control_states = next() as u32;
        f.prev_control_states = next() as u32;
        f.able_to_jump = true;
        f.visible = true;
        f.rope_out = true;

        let mut g = game(vec![Worm {
            fields: f,
            settings: WormSettings::default(),
        }]);
        let (bytes, _) = write_start(&mut g, LIERO_REPLAY_VERSION);
        let settings = data::write_archive(&mut Settings::default()).len();
        let worm_settings = data::write_worm(&mut WormSettings::default(), LIERO_REPLAY_VERSION).len();
        // Header, settings, cycles, got_changed, last killed (none), screen
        // flash, the generator, the list's 1 and the worm's number.
        let mut c = Cursor(&bytes[5 + settings + 4 + 1 + 4 + 4 + 8 + 1 + 4..]);
        let range = |a: i32, b: i32| (a..=b).collect::<Vec<_>>();
        assert_eq!(c.words(10), range(1, 10));
        assert_eq!(c.bytes(9), [1, 0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(c.words(7), range(11, 17));
        // No anchor, not attached.
        assert_eq!((c.words(1), c.bytes(1)), (vec![0], vec![0]));
        assert_eq!(c.words(2), range(18, 19));
        assert_eq!(c.bytes(1), [1]);
        assert_eq!(c.words(5), range(20, 24));
        // The unused byte, then no killer.
        assert_eq!((c.bytes(1), c.words(1)), (vec![0], vec![0]));
        assert_eq!(c.words(2), range(25, 26));
        c.bytes(worm_settings);
        assert_eq!(c.words(5), range(27, 31));
        for first in (32..52).step_by(4) {
            assert_eq!(
                (c.words(1), c.bytes(1), c.words(3)),
                (vec![first], vec![0], range(first + 1, first + 3))
            );
        }
        assert_eq!(c.bytes(3), [52, 53, 54]);
        assert_eq!(c.words(1), [CHECKPOINT as i32]);
    }

    #[test]
    fn worms_get_places_in_the_order_they_are_first_written() {
        // The game's last killed worm comes before the list, so it's read
        // first, and the list refers to it by its new place.
        let mut g = game(vec![worm(0), worm(1)]);
        g.fields.last_killed = 1;
        let (bytes, _) = write_start(&mut g.clone(), LIERO_REPLAY_VERSION);
        let (_, back) = read(&bytes).unwrap();
        assert_eq!(back.list, vec![1, 0]);
        assert_eq!(back.worms[0].fields.index, 1);
        assert_eq!(back.fields.last_killed, 1);
    }

    #[test]
    fn a_reference_to_a_worm_being_read_finds_its_index_unread() {
        // Worm 5 last killed itself: reading it back, the reference finds the
        // worm before its index is read, so with C++'s default of 0.
        let mut g = game(vec![worm(5)]);
        g.worms[0].fields.last_killed_by = 0;
        g.worms[0].fields.rope_anchor = 0;
        g.viewports.clear();
        let (bytes, _) = write_start(&mut g, LIERO_REPLAY_VERSION);
        let (_, back) = read(&bytes).unwrap();
        assert_eq!(back.worms.len(), 1);
        assert_eq!(back.worms[0].fields.last_killed_by, 0);
        assert_eq!(back.worms[0].fields.rope_anchor, 0);
        assert_eq!(back.worms[0].fields.index, 5);
    }

    #[test]
    fn a_worm_index_past_the_list_fails_after_what_was_written() {
        let mut g = game(vec![worm(0)]);
        g.fields.last_killed = 1;
        let (bytes, r) = write_start(&mut g, LIERO_REPLAY_VERSION);
        assert_eq!(r, Err(ReplayError::OutOfRange));
        let settings = data::write_archive(&mut Settings::default());
        // Header, settings, cycles, got_changed.
        assert_eq!(bytes.len(), 5 + settings.len() + 4 + 1);
    }

    #[test]
    fn broken_starts_fail_like_the_cpp() {
        let (bytes, _) = write_start(&mut game(vec![worm(0), worm(1)]), LIERO_REPLAY_VERSION);
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert_eq!(
            read(&magic).unwrap_err(),
            ReplayError::Check("File does not appear to be a replay")
        );
        let mut version = bytes.clone();
        version[4] = 5;
        assert_eq!(
            read(&version).unwrap_err(),
            ReplayError::Check("Replay version is too recent")
        );
        for len in [0, 4, 5, 100, bytes.len() - 1] {
            assert_eq!(read(&bytes[..len]).unwrap_err(), ReplayError::Read, "{len}");
        }
        // The checkpoint that ends the first worm.
        let mut check = bytes.clone();
        let pos = check.windows(4).position(|w| w == CHECKPOINT.to_be_bytes()).unwrap();
        check[pos] ^= 1;
        assert_eq!(
            read(&check).unwrap_err(),
            ReplayError::Check("Expected checkpoint here")
        );
    }

    fn frame_worm(index: i32, state: u32, prev: u32, expired: bool) -> (LieroReplayFrameWorm, WormSettings) {
        let ws = WormSettings {
            name: format!("w{index}").into_bytes(),
            ..Default::default()
        };
        let f = LieroReplayFrameWorm {
            index,
            control_states: state,
            prev_control_states: prev,
            settings_expired: expired,
        };
        (f, ws)
    }

    #[test]
    fn frames_read_back_as_written() {
        // Two worms always write their states; a checksum follows on 1050.
        let mut worms = vec![frame_worm(0, 0x11, 0x01, true), frame_worm(1, 0x02, 0x02, true)];
        let mut settings = Settings {
            max_bonuses: 9,
            ..Default::default()
        };
        let (bytes, game_written, worms_written) = write_frame(4, &mut settings, true, &mut worms, 1050, 0x0102_0304);
        assert!(game_written);
        assert_eq!(worms_written, vec![false, false]);
        assert_eq!(&bytes[bytes.len() - 6..], &[0x10, 0x00, 1, 2, 3, 4]);

        let mut read_settings = Settings::default();
        let mut read_worms = vec![frame_worm(0, 0, 0x01, false), frame_worm(1, 0x7f, 0x02, false)];
        let mut touched = vec![false; 3];
        let mut src = Bytes::new(&bytes);
        let r = read_frame(&mut src, 4, &mut read_settings, &mut read_worms, &mut touched).unwrap();
        assert_eq!(
            r,
            FrameRead {
                end: false,
                settings_changed: true
            }
        );
        assert_eq!(touched, vec![true, false, false]);
        assert_eq!(read_settings.max_bonuses, 9);
        assert_eq!(
            read_worms.iter().map(|(f, _)| f.control_states).collect::<Vec<_>>(),
            vec![0x11, 0x02]
        );
        assert_eq!(read_checksum(&mut src, 1050, 0x0102_0304), Ok(()));
    }

    #[test]
    fn more_than_three_worms_write_their_settings_and_skip_quiet_frames() {
        let mut worms: Vec<_> = (0..4).map(|i| frame_worm(i + 10, 3, 3, i == 2)).collect();
        let mut settings = Settings::default();
        let (bytes, game_written, worms_written) = write_frame(4, &mut settings, false, &mut worms, 1, 0);
        assert!(!game_written);
        assert_eq!(worms_written, vec![false, false, true, false]);
        assert_eq!(bytes[0], WORM_SETTINGS);
        assert_eq!(&bytes[1..5], &12u32.to_be_bytes());
        assert_eq!(*bytes.last().unwrap(), EMPTY_FRAME);

        // Read back, the 0x82 is by place among the worms, so index 12 is
        // past the four and skipped (C++ read past its worms there).
        let mut read_worms: Vec<_> = (0..4).map(|i| frame_worm(i, 3, 3, false)).collect();
        let mut touched = vec![false; 5];
        let r = read_frame(
            &mut Bytes::new(&bytes[..5]),
            4,
            &mut settings,
            &mut read_worms,
            &mut touched,
        );
        assert_eq!(r, Err(ReplayError::Read));
        assert_eq!(touched, vec![false; 5]);

        let mut by_place = bytes.clone();
        by_place[1..5].copy_from_slice(&2u32.to_be_bytes());
        let r = read_frame(
            &mut Bytes::new(&by_place),
            4,
            &mut settings,
            &mut read_worms,
            &mut touched,
        )
        .unwrap();
        assert!(r.settings_changed);
        assert_eq!(touched, vec![false, false, false, true, false]);
        assert_eq!(read_worms[2].1.name, b"w12");
        // An empty frame leaves the controls as they were.
        assert!(read_worms.iter().all(|(f, _)| f.control_states == 3));
    }

    #[test]
    fn frame_headers_end_or_fail() {
        let mut settings = Settings::default();
        let mut worms = vec![frame_worm(0, 0, 0, false)];
        let mut touched = vec![false; 2];
        let mut frame = |bytes: &[u8]| read_frame(&mut Bytes::new(bytes), 4, &mut settings, &mut worms, &mut touched);
        assert_eq!(
            frame(&[END]),
            Ok(FrameRead {
                end: true,
                settings_changed: false
            })
        );
        assert_eq!(frame(&[0x84]), Err(ReplayError::Check("Unexpected header byte")));
        assert_eq!(frame(&[]), Err(ReplayError::Read));
        // An index that's negative as an int is skipped too.
        assert_eq!(
            frame(&[WORM_SETTINGS, 0xff, 0xff, 0xff, 0xff, EMPTY_FRAME]),
            Ok(FrameRead::default())
        );
        assert_eq!(write_end(), vec![END]);
        // Later worms' bytes aren't headers, so they can have the top bit,
        // which reading drops.
        let mut two = vec![frame_worm(0, 0, 0x01, false), frame_worm(1, 0, 0x03, false)];
        let mut touched = vec![false; 3];
        read_frame(
            &mut Bytes::new(&[0x00, 0xff]),
            4,
            &mut Settings::default(),
            &mut two,
            &mut touched,
        )
        .unwrap();
        assert_eq!((two[0].0.control_states, two[1].0.control_states), (0x01, 0x7c));
    }

    #[test]
    fn checksums_wrap_and_catch_desyncs() {
        let worms = [[1, 2, -1, 0], [0x7fff_ffff, 0, 1, 1]];
        let mut c = 5u32;
        for [x, y, vx, vy] in worms {
            c = ((c ^ x as u32).wrapping_add(vx as u32) ^ y as u32).wrapping_add(vy as u32);
        }
        assert_eq!(checksum(5, &worms), c);
        assert_eq!(read_checksum(&mut Bytes::new(&[]), 1049, 0), Ok(()));
        assert_eq!(
            read_checksum(&mut Bytes::new(&[0, 0, 0, 1]), 2100, 2),
            Err(ReplayError::Check("Replay has desynced"))
        );
    }
}
