//! Replays (`src/game/hashreplays.cpp`): the bot games' replays played
//! back, and short replays recorded in every version, started part way
//! through a game, with settings changed while recording, and broken in
//! ways a damaged or hand-made file could be.

use super::game::hash_game;
use super::games::{bot_tick, set_up_game, tc_data, GAME_OF_TAG, HOLDAZONE, HOOKS, KILL_EM_ALL};
use super::{Fnv, Out};
use crate::app::App;
use crate::controllers::liero_game_focus;
use crate::deflate::{inflate, Deflater, Inflated};
use crate::fs::Node;
use crate::game::Rect;
use crate::game_box::Game;
use crate::renderer::Renderer;
use crate::replay::{ReplayStatus, LIERO_REPLAY_VERSION};
use crate::replay_file::{ReplayOutcome, ReplayReader, ReplayWriter};
use crate::settings::{SettingsBox, WormSettingsBox};
use crate::sfx::SoundOut;
use crate::tc::SharedTc;
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use liero_data::gash::Hash;
use liero_data::settings::{self as data, Settings, WormSettings};

fn node(path: &str) -> Node {
    Node::new(path.as_bytes())
}

/// What a replay file inflates to, and whether reading on failed.
fn inflated_file(path: &str) -> Inflated {
    let file = Node::new(path.as_bytes())
        .read()
        .unwrap_or_else(|| panic!("Could not read {path}"));
    inflate(&file)
}

/// A replay file's bytes before compression.
fn read_replay(path: &str) -> Vec<u8> {
    let r = inflated_file(path);
    assert!(!r.error, "Error while deflating");
    r.data
}

/// Compresses `data` into a replay file, as the replay writer does.
fn write_replay(path: &str, data: &[u8]) {
    let mut out = Deflater::new(&Node::new(path.as_bytes())).unwrap_or_else(|| panic!("Could not write {path}"));
    out.write(data);
    // Dropping it finishes the file.
}

fn add_hash(f: &mut Fnv, h: &Hash) {
    for &v in h {
        f.u32(v as u32);
        f.u32((v >> 32) as u32);
    }
}

fn add64(f: &mut Fnv, v: u64) {
    f.u32(v as u32);
    f.u32((v >> 32) as u32);
}

/// `Settings::updateHash`, which leaves the settings as written.
fn settings_hash(b: &mut SettingsBox) -> Hash {
    let mut s = b.settings();
    let h = data::archive_hash(&mut s);
    b.set_settings(&s);
    h
}

/// `WormSettings::updateHash`.
fn worm_hash(b: &mut WormSettingsBox) -> Hash {
    let mut w = b.worm();
    let h = data::worm_hash(&mut w);
    b.set_worm(&w);
    h
}

/// The game's settings and worms' settings hashes.
fn add_settings_hashes(f: &mut Fnv, game: &mut Game, with_index: bool) {
    add_hash(f, &settings_hash(&mut game.settings_box()));
    if with_index {
        f.u32(game.listed_worms().count() as u32);
    }
    for w in game.listed_worms() {
        let w = w.expect("an empty place among the game's worms");
        if with_index {
            f.i32(w.data.index);
        }
        add_hash(f, &worm_hash(&mut w.settings_mut()));
    }
}

/// The settings, worm settings and indexes, viewports and level palette
/// and materials, which a replay stores but `hash_game` leaves out.
fn hash_loaded(f: &mut Fnv, game: &mut Game) {
    add_settings_hashes(f, game, true);

    let vps = game.split_viewports();
    f.u32(vps.len() as u32);
    for vp in vps {
        let r = vp.rect;
        for v in [
            vp.x,
            vp.y,
            vp.shake,
            vp.max_x,
            vp.max_y,
            vp.center_x,
            vp.center_y,
            vp.worm_idx,
            vp.banner_y,
            r.x1,
            r.y1,
            r.x2,
            r.y2,
        ] {
            f.i32(v);
        }
    }

    let level = game.level_box().data();
    let l = &level.level;
    f.i32(l.width);
    f.i32(l.height);
    for c in &level.origpal {
        for v in [c.r, c.g, c.b] {
            f.u32(u32::from(v));
        }
    }
    let n = l.width.max(0) as usize * l.height.max(0) as usize;
    if n > 0 {
        // SAFETY: the level has a material for each pixel.
        for m in unsafe { std::slice::from_raw_parts(l.materials, n) } {
            f.u32(u32::from(m.flags));
        }
    }
}

/// What C++ threw for an outcome: "archive error" for a checkpoint, an
/// exception otherwise.
fn thrown(o: &ReplayOutcome) -> Option<String> {
    let message = o.message;
    match o.status {
        ReplayStatus::ReadError => Some(format!("exception: {message}")),
        ReplayStatus::CheckError => Some(format!("archive error: {message}")),
        // What std::vector::at threw in libc++.
        ReplayStatus::OutOfRange => Some("exception: vector".into()),
        _ => None,
    }
}

/// `ReplayReader::playbackFrame`: false at the end of the replay.
fn playback_frame(reader: &mut ReplayReader, game: &mut Game, renderer: &mut Renderer) -> Result<bool, String> {
    let o = reader.frame(game);
    if o.end {
        return Ok(false);
    }
    if o.settings_changed {
        // SAFETY: the renderer is a live one.
        unsafe { liero_game_focus(game, renderer) };
    }
    thrown(&o).map_or(Ok(true), Err)
}

/// Plays `game` back from the reader as the replay controller does, for at
/// most `max_ticks` ticks, rewinding at `rewind_at` if it isn't 0. The ticks
/// played, and the error that stopped it.
#[allow(clippy::too_many_arguments)]
fn play_back(
    out: &mut Out,
    name: &str,
    reader: &mut ReplayReader,
    game: &mut Game,
    renderer: &mut Renderer,
    interval: i32,
    max_ticks: i32,
    rewind_at: i32,
    ticks: &mut i32,
) -> Result<(), String> {
    // As the replay controller's game state, without the single screen
    // option.
    game.clear_viewports();
    for (i, x) in [(0, 0), (1, 218)] {
        game.worm_at(i)
            .expect("an empty place among the replay's worms")
            .data
            .stats_x = x;
    }
    let r = |x1, y1, x2, y2| Rect { x1, y1, x2, y2 };
    let w0 = game.worm_at(0).expect("no first worm").data.index;
    let w1 = game.worm_at(1).expect("no second worm").data.index;
    game.add_viewport(r(0, 0, 504 + 68, 350), 0, 504, 350, LIERO_VIEWPORT_SPECTATOR, true);
    game.add_viewport(r(0, 0, 158, 158), w0, 504, 350, LIERO_VIEWPORT_WORM, false);
    game.add_viewport(r(160, 0, 158 + 160, 158), w1, 504, 350, LIERO_VIEWPORT_WORM, false);
    let blood = game.settings_box().data.ext.blood_particle_max;
    game.start(blood);
    // SAFETY: the renderer is a live one.
    unsafe { liero_game_focus(game, renderer) };

    // What R rewinds to in the replay controller.
    let mut initial = (rewind_at > 0).then(|| {
        // SAFETY: the clone is the caller's.
        game.copy()
    });
    let initial_pos = reader.pos();

    while *ticks < max_ticks {
        if !playback_frame(reader, game, renderer)? {
            break;
        }
        game.process_frame(&HOOKS);
        *ticks += 1;
        if *ticks % interval == 0 {
            out.line(&format!("{name}-replay {ticks} {:016x}", hash_game(game)));
        }

        if *ticks == rewind_at {
            if let Some(start) = initial.take() {
                // As R in the replay controller: back to the start, then
                // play to here again, which has to give the same game.
                game.assign(&start);
                reader.set_pos(initial_pos);
                let mut t = 0;
                while t < rewind_at && playback_frame(reader, game, renderer)? {
                    game.process_frame(&HOOKS);
                    t += 1;
                }
                out.line(&format!("{name}-replay-rewind {t} {:016x}", hash_game(game)));
            }
        }
    }
    Ok(())
}

/// Plays a replay back as the replay controller does, for at most
/// `max_ticks` ticks (`hashReplay`). Hashes the replay file, the game it
/// starts with, the game state every `interval` ticks and how it ended: the
/// end of the replay, or the error that stopped it, with the game and
/// settings then. With `rewind_at`, the replay is also rewound there, as R
/// does in a replay.
pub(super) fn hash_replay(
    out: &mut Out,
    tc: &SharedTc,
    name: &str,
    path: &str,
    interval: i32,
    max_ticks: i32,
    rewind_at: i32,
) {
    {
        // What the file inflates to, not its compressed bytes, which
        // depend on the compressor.
        let file = inflated_file(path);
        let mut f = Fnv::default();
        f.u32(file.data.len() as u32);
        f.bytes(&file.data);
        f.bool(file.error);
        out.hash(&format!("{name}-replay-file"), 0, f);
    }

    let mut renderer = Renderer::new();
    renderer.set_render_resolution(320, 200);
    renderer.load_palette(&tc_data(tc));

    let mut reader = ReplayReader::new(&node(path)).unwrap_or_else(|| panic!("Could not read {path}"));
    let r = &mut reader;

    let mut game: Option<Box<Game>> = None;
    let mut ticks = 0;
    let result = (|| {
        // `beginPlayback`: a game with default settings that the replay
        // fills in.
        let settings = SettingsBox::defaults();
        // SAFETY: a new game.
        let mut g = Game::new();
        g.set_context(Some(settings), Some(tc.clone()), SoundOut::NONE);
        let o = r.begin(&mut g);
        if let Some(e) = thrown(&o) {
            return Err(e);
        }
        let g = game.insert(g);

        let mut f = Fnv::default();
        hash_loaded(&mut f, g);
        add64(&mut f, hash_game(g));
        out.hash(&format!("{name}-replay-load"), 0, f);

        play_back(
            out,
            name,
            r,
            g,
            &mut renderer,
            interval,
            max_ticks,
            rewind_at,
            &mut ticks,
        )
    })();
    let outcome = match result {
        Ok(()) if ticks == max_ticks => "stopped".to_string(),
        Ok(()) => "replay ended".to_string(),
        Err(e) => e,
    };

    let mut f = Fnv::default();
    f.bytes(outcome.as_bytes());
    f.i32(ticks);
    f.bool(game.is_some());
    if let Some(g) = game.as_deref_mut() {
        add64(&mut f, hash_game(g));
        add_settings_hashes(&mut f, g, false);
    }
    out.hash(&format!("{name}-replay-end"), ticks as i64, f);
    eprintln!("{name}: {outcome} after {ticks} ticks");

    drop(reader);
    drop(game);
    // The run has a fixed heap, which holds the in-memory files too.
    crate::disk::remove_file(super::on_disk(path).as_bytes());
}

/// A change to make to the settings while recording.
struct SettingsChange {
    tick: i32,
    loading_time: i32,
    max_bonuses: i32,
    blood: i32,
    worm_name: Option<&'static str>,
}

/// Starts a bot game as the bot games do and runs it `ticks` ticks.
fn bot_game(a: &mut App, tc: &SharedTc, seed: u32, game_mode: u32, ticks: i32) -> Box<Game> {
    let mut game = set_up_game(a, tc, seed, game_mode, None);
    let blood = game.settings_box().data.ext.blood_particle_max;
    game.start(blood);
    for _ in 0..ticks {
        bot_tick(&mut game);
        game.process_frame(&HOOKS);
    }
    game
}

/// Records `ticks` ticks of a bot game into `path`. With `start_tick` 0 the
/// recording starts as the local controller starts it, before the game
/// starts; otherwise after that many ticks.
#[allow(clippy::too_many_arguments)]
fn record_game(
    a: &mut App,
    tc: &SharedTc,
    path: &str,
    seed: u32,
    game_mode: u32,
    start_tick: i32,
    ticks: i32,
    version: i32,
    changes: &[SettingsChange],
) {
    let mut writer = ReplayWriter::new(&node(path)).unwrap_or_else(|| panic!("Could not write {path}"));
    writer.set_version(version);
    let mut game = if start_tick == 0 {
        let mut game = set_up_game(a, tc, seed, game_mode, None);
        assert!(thrown(&writer.begin(&mut game)).is_none(), "the replay didn't begin");
        let blood = game.settings_box().data.ext.blood_particle_max;
        game.start(blood);
        game
    } else {
        let mut game = bot_game(a, tc, seed, game_mode, start_tick);
        assert!(thrown(&writer.begin(&mut game)).is_none(), "the replay didn't begin");
        game
    };

    for tick in 0..ticks {
        for c in changes.iter().filter(|c| c.tick == tick) {
            // As when the options menu is opened during a game.
            writer.unfocus(&mut game);
            {
                let s = &mut game.settings_box().data;
                s.loading_time = c.loading_time;
                s.max_bonuses = c.max_bonuses;
                s.blood = c.blood;
            }
            if let Some(name) = c.worm_name {
                game.worm_at(1).expect("no second worm").settings_mut().data.name = name.as_bytes().to_vec();
            }
            writer.focus(&mut game);
        }
        bot_tick(&mut game);
        writer.frame(&mut game);
        game.process_frame(&HOOKS);
    }
    // Finishes the file.
    drop(writer);
}

fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_be_bytes());
}

fn replace(mut data: Vec<u8>, pos: usize, v: u8) -> Vec<u8> {
    data[pos] = v;
    data
}

fn insert(mut data: Vec<u8>, pos: usize, what: &[u8]) -> Vec<u8> {
    data.splice(pos..pos, what.iter().copied());
    data
}

fn truncate(mut data: Vec<u8>, len: usize) -> Vec<u8> {
    data.resize(len, 0);
    data
}

/// Where frame `k` of a recording from the start of a game begins: each
/// frame has a byte per worm, and frames on a multiple of 1050 a checksum.
fn frame_offset(first_frame: usize, k: usize) -> usize {
    first_frame + 2 * k + 4 * k.div_ceil(1050)
}

pub fn hash_replays(a: &mut App, out: &mut Out) {
    let tc = super::tc::bundled();
    let tc = &tc;

    // Every version, from the start of a game past the second checksum.
    for version in 0..=LIERO_REPLAY_VERSION {
        let name = format!("replay-v{version}");
        let path = format!("/tmp/{name}.lrp");
        record_game(a, tc, &path, 21, KILL_EM_ALL, 0, 2200, version, &[]);
        hash_replay(out, tc, &name, &path, 200, 2200, 600);
    }

    // Started part way through games, when ropes, kills and the tag can
    // point at worms.
    for mode in [KILL_EM_ALL, GAME_OF_TAG, HOLDAZONE] {
        for start in [150, 900, 2400, 5000] {
            let name = format!("replay-mode{mode}-from{start}");
            let path = format!("/tmp/{name}.lrp");
            record_game(a, tc, &path, 22 + mode, mode, start, 600, LIERO_REPLAY_VERSION, &[]);
            hash_replay(out, tc, &name, &path, 200, 600, 0);
        }
    }

    // Settings changed while recording, also on a checksum frame and in an
    // old version. Changing only a worm's name isn't recorded with two
    // worms.
    let change = |tick, loading_time, max_bonuses, blood, worm_name| SettingsChange {
        tick,
        loading_time,
        max_bonuses,
        blood,
        worm_name,
    };
    let changes = [
        change(100, 50, 2, 300, None),
        change(101, 50, 2, 300, Some("Changed")),
        change(400, 200, 6, 0, None),
        change(1050, 100, 4, 100, None),
    ];
    for version in [LIERO_REPLAY_VERSION, 1] {
        let name = format!("replay-settings-v{version}");
        let path = format!("/tmp/{name}.lrp");
        record_game(a, tc, &path, 24, KILL_EM_ALL, 0, 1300, version, &changes);
        hash_replay(out, tc, &name, &path, 100, 1300, 0);
    }

    // Broken replays, made from one recorded from the start of a game. Its
    // 1200 frames end with the end marker.
    let base_path = "/tmp/replay-base.lrp";
    record_game(a, tc, base_path, 25, KILL_EM_ALL, 0, 1200, LIERO_REPLAY_VERSION, &[]);
    let base = read_replay(base_path);
    crate::disk::remove_file(super::on_disk(base_path).as_bytes());
    let first_frame = base.len() - 1 - frame_offset(0, 1200);

    // The run's heap can't hold all the broken copies at once, so each is
    // played as it's made.
    let mut n = 0;
    let mut play = |data: Vec<u8>| {
        let name = format!("replay-broken-{n}");
        n += 1;
        let path = format!("/tmp/{name}.lrp");
        write_replay(&path, &data);
        drop(data);
        hash_replay(out, tc, &name, &path, 300, 1200, 0);
    };
    let b = || base.clone();

    play(replace(b(), 0, base[0] ^ 1)); // Magic
    play(replace(b(), 4, 5)); // Version too recent
    play(replace(b(), 4, 0xff));
    for i in 0..40 {
        play(truncate(b(), first_frame * i / 40));
    }
    for k in [0, 1, 2, 3, 5, 6, 7] {
        play(truncate(b(), first_frame + k));
    }
    play(truncate(b(), frame_offset(first_frame, 500) + 1));
    play(truncate(b(), base.len() - 1)); // No end marker

    // The first worm's checkpoint.
    if let Some(i) = (0..first_frame.saturating_sub(3)).find(|&i| base[i..i + 4] == [0x12, 0x34, 0x56, 0x78]) {
        play(replace(b(), i + 3, 0x79));
    }

    let frame10 = frame_offset(first_frame, 10);
    for header in [0x80, 0x83, 0x84, 0xff] {
        play(replace(b(), frame10, header));
    }
    play(replace(b(), first_frame + 2, base[first_frame + 2] ^ 1)); // Checksum at frame 0
    let checksum1050 = frame_offset(first_frame, 1050) + 2;
    play(replace(b(), checksum1050, base[checksum1050] ^ 0x40));

    // Settings for the game and for each worm in the middle of the frames,
    // for worms that exist and one that doesn't.
    let changed = Settings {
        loading_time: 30,
        lives: 3,
        blood: 500,
        ..Default::default()
    };
    let mut settings_bytes = vec![0x81];
    // Writing sets the settings too, so a copy is written.
    settings_bytes.extend(data::write_archive(&mut changed.clone()));
    play(insert(b(), frame10, &settings_bytes));
    play(insert(b(), frame10, &settings_bytes[..settings_bytes.len() / 2]));
    play(truncate(
        insert(b(), frame10, &settings_bytes),
        frame10 + settings_bytes.len() / 2,
    ));

    let ws = WormSettings {
        name: b"Inserted".to_vec(),
        color: 40,
        health: 50,
        ..WormSettings::default()
    };
    let worm_archive = data::write_worm(&mut ws.clone(), LIERO_REPLAY_VERSION);
    for id in [0u32, 1] {
        let mut worm_bytes = vec![0x82];
        put_u32(&mut worm_bytes, id);
        worm_bytes.extend_from_slice(&worm_archive);
        play(insert(b(), frame10, &worm_bytes));
        play(truncate(
            insert(b(), frame10, &worm_bytes),
            frame10 + worm_bytes.len() - 3,
        ));
    }
    {
        let mut unknown = vec![0x82];
        put_u32(&mut unknown, 0xffff_ffff);
        play(insert(b(), frame10, &unknown));
    }
    {
        // Two settings in one frame, then the frame.
        let mut both = settings_bytes.clone();
        both.push(0x82);
        put_u32(&mut both, 0);
        both.extend_from_slice(&worm_archive);
        play(insert(b(), frame_offset(first_frame, 1050), &both));
    }
}
