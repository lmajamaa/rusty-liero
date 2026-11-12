//! The bot games (`runGame` and `setUpGame` in `src/game/hashrun.cpp`, and
//! the stats screen from `hashstats.cpp`): games from fixed seeds, hashed
//! every `interval` ticks with their frames and stats, then the stats
//! screen, and the replay each recorded played back.

use super::game::{add_screen, hash_frame, hash_game, hash_stats};
use super::{Fnv, Out};
use crate::app::app;
use crate::controllers::{liero_game_focus, liero_game_stats_scene};
use crate::fs::{LieroFsNode, Node};
use crate::game::{LieroGameHooks, LieroRect};
use crate::game_box::{
    liero_game_add_viewport, liero_game_list_worm, liero_game_new, liero_game_new_worm, liero_worm_ai, liero_worm_set_ai,
    LieroGame,
};
use crate::level::{liero_level_box_swap, LieroLevelBox};
use crate::rand::liero_rand_seed;
use crate::renderer::LieroRenderer;
use crate::replay_file::{
    liero_replay_writer_begin, liero_replay_writer_frame, liero_replay_writer_free, liero_replay_writer_new,
};
use crate::settings::LieroSettingsBox;
use crate::sfx::LieroSoundOut;
use crate::shared::Shared;
use crate::stats::{liero_stats_screen_draw, LieroStatsScreen};
use crate::tc::{LieroTc, LieroTcData, LieroTcWeapon};
use crate::text::LieroStr;
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use crate::weapsel::{liero_weapsel_finish, liero_weapsel_free, liero_weapsel_start};
use std::ffi::c_void;

pub const KILL_EM_ALL: u32 = 0;
pub const GAME_OF_TAG: u32 = 1;
pub const HOLDAZONE: u32 = 2;
pub const SCALES_OF_JUSTICE: u32 = 3;

/// One game: its name, seed, mode and the weapons both worms get, by name,
/// or None for random ones.
struct RunConfig {
    name: &'static str,
    seed: u32,
    game_mode: u32,
    weapons: Option<&'static [&'static str; 5]>,
}

// Fixed loadouts that together cover all 40 weapons of the bundled TC.
const EXPLOSIVES: [&str; 5] = ["GRENADE", "MINE", "CLUSTER BOMB", "BIG NUKE", "BOUNCY MINE"];
const BOMBS: [&str; 5] = ["GRASSHOPPER", "CHIQUITA BOMB", "NAPALM", "MINI NUKE", "FLOAT MINE"];
const GUNS: [&str; 5] = ["MINIGUN", "UZI", "HANDGUN", "RIFLE", "WINCHESTER"];
const SHOTGUNS: [&str; 5] = ["SHOTGUN", "SUPER SHOTGUN", "CHAINGUN", "GAUSS GUN", "BLASTER"];
const ROCKETS: [&str; 5] = ["BAZOOKA", "MISSILE", "MINI ROCKETS", "DOOMSDAY", "HELLRAIDER"];
const BEAMS: [&str; 5] = ["LASER", "LARPA", "BOUNCY LARPA", "CRACKLER", "ZIMM"];
pub const THROWERS: [&str; 5] = ["FLAMER", "FAN", "DART", "DIRTBALL", "GREENBALL"];
const TRAPS: [&str; 5] = ["BOOBY TRAP", "EXPLOSIVES", "SPIKEBALLS", "CANNON", "RB RAMPAGE"];

/// Random weapons in each game mode, then each fixed loadout.
const CONFIGS: [RunConfig; 17] = [
    RunConfig { name: "kill-1", seed: 1, game_mode: KILL_EM_ALL, weapons: None },
    RunConfig { name: "kill-2", seed: 2, game_mode: KILL_EM_ALL, weapons: None },
    RunConfig { name: "kill-3", seed: 3, game_mode: KILL_EM_ALL, weapons: None },
    RunConfig { name: "kill-4", seed: 4, game_mode: KILL_EM_ALL, weapons: None },
    RunConfig { name: "tag-5", seed: 5, game_mode: GAME_OF_TAG, weapons: None },
    RunConfig { name: "zone-6", seed: 6, game_mode: HOLDAZONE, weapons: None },
    RunConfig { name: "explosives-7", seed: 7, game_mode: KILL_EM_ALL, weapons: Some(&EXPLOSIVES) },
    RunConfig { name: "bombs-8", seed: 8, game_mode: KILL_EM_ALL, weapons: Some(&BOMBS) },
    RunConfig { name: "guns-9", seed: 9, game_mode: KILL_EM_ALL, weapons: Some(&GUNS) },
    RunConfig { name: "shotguns-10", seed: 10, game_mode: KILL_EM_ALL, weapons: Some(&SHOTGUNS) },
    RunConfig { name: "rockets-11", seed: 11, game_mode: KILL_EM_ALL, weapons: Some(&ROCKETS) },
    RunConfig { name: "beams-12", seed: 12, game_mode: KILL_EM_ALL, weapons: Some(&BEAMS) },
    RunConfig { name: "throwers-13", seed: 13, game_mode: KILL_EM_ALL, weapons: Some(&THROWERS) },
    RunConfig { name: "traps-14", seed: 14, game_mode: KILL_EM_ALL, weapons: Some(&TRAPS) },
    RunConfig { name: "soj-15", seed: 15, game_mode: SCALES_OF_JUSTICE, weapons: None },
    RunConfig { name: "soj-guns-16", seed: 16, game_mode: SCALES_OF_JUSTICE, weapons: Some(&GUNS) },
    RunConfig { name: "zone-shotguns-17", seed: 17, game_mode: HOLDAZONE, weapons: Some(&SHOTGUNS) },
];

/// # Safety
/// `p` is valid for `n` items, or `n` is 0.
unsafe fn items<'a, T>(p: *const T, n: usize) -> &'a [T] {
    if n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, n)
    }
}

/// The TC's data.
pub(super) fn tc_data(tc: &Shared<LieroTc>) -> &LieroTcData {
    // SAFETY: a loaded TC has its data while it lives.
    unsafe { &*crate::tc::liero_tc_data(&**tc) }
}

/// The menu positions of the named weapons.
fn weapon_positions(d: &LieroTcData, names: &[&str; 5]) -> [u32; 5] {
    // SAFETY: the TC's lists are as long as their counts.
    let (weapons, order) = unsafe { (items(d.weapons, d.weapon_count), items(d.weap_order, d.weapon_count)) };
    names.map(|name| {
        // The last of the same name, as the C++ loop left it.
        let found = order.iter().rposition(|&w| {
            let w: &LieroTcWeapon = &weapons[w as usize];
            // SAFETY: the weapon's name is the TC's.
            (unsafe { w.name.bytes() }) == name.as_bytes()
        });
        found.unwrap_or_else(|| panic!("No weapon called {name} in the TC")) as u32 + 1
    })
}

unsafe extern "C" fn no_ticks(_: *mut c_void) -> u64 {
    0
}

/// What a tick runs with: the stats clock isn't hashed.
pub(super) const HOOKS: LieroGameHooks = LieroGameHooks { ctx: std::ptr::null_mut(), hires_ticks: no_ticks, process_viewports: true };

/// A bot-vs-bot game as the hash run sets it up, before it starts, with
/// random weapons when `weapons` is None (`setUpGame`).
pub(super) fn set_up_game(tc: &Shared<LieroTc>, seed: u32, game_mode: u32, weapons: Option<&[&str; 5]>) -> Box<LieroGame> {
    let d = tc_data(tc);
    let settings = LieroSettingsBox::defaults();
    // SAFETY: nothing else has the new settings yet.
    let s = unsafe { &mut *settings.as_ptr() };
    s.data.game_mode = game_mode;
    for ws in &s.worms {
        // SAFETY: the settings' worms are theirs alone.
        let ws = unsafe { &mut *ws.as_ptr() };
        // The original bot
        ws.data.controller = 1;
        if let Some(names) = weapons {
            ws.data.weapons = weapon_positions(d, names);
        }
    }
    // 0 gives bots random weapons; 2 keeps the ones set above. (The
    // default, 1, would keep the menu's default weapons: all bazookas.)
    s.data.select_bot_weapons = if weapons.is_some() { 2 } else { 0 };

    // Everything random comes from these seeds. Weapon selection and the
    // level use the app's random numbers, as in the menus.
    let a = app();
    liero_rand_seed(&mut a.rand, seed);

    // SAFETY: the game, its worms and viewports are made as the local
    // controller makes them, with settings and a TC that it keeps.
    unsafe {
        let mut game = Box::from_raw(liero_game_new());
        crate::game_box::liero_game_set_context(&mut game, settings.as_ptr(), tc.as_ptr(), LieroSoundOut::NONE);
        liero_rand_seed(&mut game.data_mut().rand, seed.wrapping_mul(7919));

        for (i, ws) in s.worms.iter().enumerate() {
            let w = &mut *liero_game_new_worm(&mut game, ws.as_ptr());
            liero_game_list_worm(&mut game, w);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
            liero_worm_set_ai(w, 1, 0, false);
            let ai = &mut *liero_worm_ai(w);
            liero_rand_seed(crate::ai::liero_worm_ai_rand(ai), seed.wrapping_mul(104_729).wrapping_add(i as u32));
        }

        let r = |x1, y1, x2, y2| LieroRect { x1, y1, x2, y2 };
        liero_game_add_viewport(&mut game, r(0, 0, 158, 158), 0, 504, 350, LIERO_VIEWPORT_WORM, false);
        liero_game_add_viewport(&mut game, r(160, 0, 158 + 160, 158), 1, 504, 350, LIERO_VIEWPORT_WORM, false);
        liero_game_add_viewport(&mut game, r(0, 0, 504 + 68, 350), 0, 504, 350, LIERO_VIEWPORT_SPECTATOR, true);

        let mut level = LieroLevelBox::new();
        level.generate_from_settings(&d.level_common(), &d.palette_colors, s, &mut a.rand);
        liero_level_box_swap(game.level_box_mut(), &mut level);

        let ws = liero_weapsel_start(&mut game);
        liero_weapsel_finish(&mut *ws);
        liero_weapsel_free(ws);

        let lives = s.data.lives;
        for w in game.listed_worms() {
            w.expect("an empty place among the game's worms").data.lives = lives;
        }
        game
    }
}

/// The bots' controls for the next tick (`botTick`), untimed.
pub(super) fn bot_tick(game: &mut LieroGame) {
    game.process_ais(None);
}

/// Ends the game's stats and draws the stats screen over the last frame, at
/// each pane and part way between them, scrolled to several depths
/// (`hashStatsScreen`).
fn hash_stats_screen(out: &mut Out, name: &str, game: &mut LieroGame, renderer: &mut LieroRenderer, d: &LieroTcData) {
    game.stats_finish();
    // SAFETY: the game's stats live as long as it does.
    let stats = unsafe { &*game.stats() };
    out.line(&format!("{name}-stats-end 0 {:016x}", hash_stats(stats)));

    let mut bg = crate::renderer::LieroBitmapBox::new();
    bg.copy_from(&renderer.bmp);
    // SAFETY: the TC's weapons are as many as its count.
    let names: Vec<LieroStr> = unsafe { items(d.weapons, d.weapon_count) }.iter().map(|w| w.name).collect();
    let screen = LieroStatsScreen::new(stats, renderer.data.render_res_x);

    let mut n = 0;
    for pane in [0.0, -1.0, 1.0, 0.37, -0.81] {
        for offset in [0.0, -35.5, -140.0, -400.0, -1200.0] {
            renderer.bmp.copy_from(&bg);
            // SAFETY: as the stats screen draws itself, with the TC's names.
            unsafe {
                let scene = liero_game_stats_scene(game, renderer, names.as_ptr(), names.len());
                liero_stats_screen_draw(&screen, stats, &scene, &renderer.bmp.view(), pane, offset);
            }
            renderer.data.pal = d.palette_colors;

            let mut f = Fnv::default();
            add_screen(&mut f, renderer);
            out.line(&format!("{name}-statsview {n} {:016x}", f.0));
            n += 1;
        }
    }
}

fn run_game(out: &mut Out, tc: &Shared<LieroTc>, config: &RunConfig, ticks: i32, interval: i32) {
    let d = tc_data(tc);
    let mut game = set_up_game(tc, config.seed, config.game_mode, config.weapons);
    let name = config.name;

    // Records a replay of the game, as the local controller does when
    // replays are on, to play it back afterwards.
    let path = format!("/tmp/{name}.lrp");
    let writer = liero_replay_writer_new(&LieroFsNode(Node::new(path.as_bytes())));
    assert!(!writer.is_null(), "Could not write {path}");
    // SAFETY: the writer is new, and records the game while both live.
    unsafe {
        let o = liero_replay_writer_begin(&mut *writer, &mut game);
        assert!(o.status == crate::replay::LieroReplayStatus::Ok, "the replay didn't begin");
    }

    let blood = game.settings_box().data.blood_particle_max;
    game.start(blood);

    // Drawing doesn't change the game, so the game hashes don't depend on it.
    let mut split = LieroRenderer::new();
    let mut spectator = LieroRenderer::new();
    split.set_render_resolution(320, 200);
    spectator.set_render_resolution(640, 400);
    for r in [&mut split, &mut spectator] {
        r.load_palette(d);
        // SAFETY: the renderer is a live one.
        unsafe { liero_game_focus(&mut game, &mut **r) };
    }

    out.line(&format!("{name} 0 {:016x}", hash_game(&mut game)));
    out.line(&format!("{name} 0 {:016x} frame", hash_frame(&mut game, &mut split, &mut spectator)));

    for tick in 1..=ticks {
        bot_tick(&mut game);
        // SAFETY: the writer records the game, which lives.
        unsafe { liero_replay_writer_frame(&mut *writer) };
        game.process_frame(&HOOKS);

        let over = game.is_over();
        if over || tick % interval == 0 {
            let end = if over { " end" } else { "" };
            out.line(&format!("{name} {tick} {:016x}{end}", hash_game(&mut game)));
            out.line(&format!("{name} {tick} {:016x} frame", hash_frame(&mut game, &mut split, &mut spectator)));
            // SAFETY: the game's stats live as long as it does.
            out.line(&format!("{name}-stats {tick} {:016x}", hash_stats(unsafe { &*game.stats() })));
        }
        if over {
            break;
        }
    }

    hash_stats_screen(out, name, &mut game, &mut split, d);
    // SAFETY: the writer is done with; freeing it finishes the file.
    unsafe { liero_replay_writer_free(writer) };
    super::replays::hash_replay(out, tc, name, &path, 1000, ticks, 0);

    let g = game.data_mut().cycles;
    let w: Vec<(i32, i32)> = game.listed_worms().map(|w| w.map_or((0, 0), |w| (w.data.kills, w.data.lives))).collect();
    out.line(&format!("{name} summary: {g} ticks, kills {}-{}, lives {}-{}", w[0].0, w[1].0, w[0].1, w[1].1));
}

/// Every bot game, `ticks` long at most and hashed every `interval`.
pub fn hash_games(out: &mut Out, ticks: i32, interval: i32) {
    let tc = super::tc::bundled();
    for config in &CONFIGS {
        run_game(out, &tc, config, ticks, interval);
    }
}
