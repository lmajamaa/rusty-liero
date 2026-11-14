//! Games between the predictive bots (`src/game/hashai.cpp`), which the bot
//! games don't use: they play the original bot, which is much cheaper.
//! Each predictive tick simulates hundreds of frames on copies of the game,
//! so these games are short and most turn the AI's search down.
//!
//! Besides the game state, they hash the AI's own state: its candidate
//! plans and their scores, the learnt transition model, the path-finding
//! cells and the positions it traced (`liero_worm_ai_hash`).

use super::game::{hash_frame, hash_game};
use super::games::{bot_tick, set_up_game, tc_data, GAME_OF_TAG, HOLDAZONE, HOOKS, KILL_EM_ALL};
use super::{Fnv, Out};
use crate::ai::{liero_worm_ai_hash, liero_worm_ai_rand};
use crate::app::LieroApp;
use crate::controllers::liero_game_focus;
use crate::game_box::liero_worm_set_ai;
use crate::renderer::LieroRenderer;
use crate::tc::SharedTc;

struct AiConfig {
    name: &'static str,
    seed: u32,
    game_mode: u32,
    weapons: Option<&'static [&'static str; 5]>,
    ai_frames: i32,
    ai_mutations: i32,
    ai_parallels: i32,
    ai_traces: bool,
    ticks: i32,
}

const ROCKETS: [&str; 5] = ["BAZOOKA", "MISSILE", "MINI ROCKETS", "DOOMSDAY", "HELLRAIDER"];
const MIXED: [&str; 5] = ["MISSILE", "SHOTGUN", "LASER", "GRENADE", "NAPALM"];

const fn ai(
    name: &'static str,
    seed: u32,
    game_mode: u32,
    weapons: Option<&'static [&'static str; 5]>,
    (ai_frames, ai_mutations, ai_parallels, ai_traces): (i32, i32, i32, bool),
    ticks: i32,
) -> AiConfig {
    AiConfig {
        name,
        seed,
        game_mode,
        weapons,
        ai_frames,
        ai_mutations,
        ai_parallels,
        ai_traces,
        ticks,
    }
}

const CONFIGS: [AiConfig; 6] = [
    ai("ai-kill-1", 101, KILL_EM_ALL, None, (30, 1, 3, false), 1500),
    ai("ai-tag-2", 102, GAME_OF_TAG, None, (30, 1, 3, false), 1500),
    ai("ai-zone-3", 103, HOLDAZONE, None, (30, 1, 3, false), 1500),
    ai(
        "ai-rockets-4",
        104,
        KILL_EM_ALL,
        Some(&ROCKETS),
        (30, 1, 2, false),
        1500,
    ),
    ai("ai-mixed-5", 105, KILL_EM_ALL, Some(&MIXED), (20, 0, 4, true), 1000),
    // The menu's AI settings.
    ai(
        "ai-default-6",
        106,
        KILL_EM_ALL,
        Some(&MIXED),
        (70 * 2, 2, 3, true),
        120,
    ),
];

fn run_ai_game(a: &mut LieroApp, out: &mut Out, tc: &SharedTc, c: &AiConfig, interval: i32) {
    let mut game = set_up_game(a, tc, c.seed, c.game_mode, c.weapons);
    {
        let s = &mut game.settings_box().data;
        s.ai_frames = c.ai_frames;
        s.ai_mutations = c.ai_mutations;
        s.ai_parallels = c.ai_parallels;
        s.ai_traces = c.ai_traces;
    }

    // As the app makes them for controller 2.
    for i in 0..2 {
        let w = game.worm_at(i).expect("no worm");
        w.settings_mut().data.controller = 2;
        let testing = w.data.index == 0;
        liero_worm_set_ai(w, 2, c.ai_parallels as usize, testing);
        liero_worm_ai_rand(w.ai_mut().expect("a bot")).seed(c.seed.wrapping_mul(31).wrapping_add(i as u32));
    }

    let blood = game.settings_box().data.blood_particle_max;
    game.start(blood);

    let mut split = LieroRenderer::new();
    let mut spectator = LieroRenderer::new();
    split.set_render_resolution(320, 200);
    spectator.set_render_resolution(640, 400);
    for r in [&mut split, &mut spectator] {
        r.load_palette(&tc_data(tc));
        // SAFETY: the renderer is a live one.
        unsafe { liero_game_focus(&mut game, &mut **r) };
    }

    // The controls every tick, which the game hashes only sample.
    let mut controls = Fnv::default();
    let name = c.name;
    for tick in 1..=c.ticks {
        bot_tick(&mut game);
        for w in game.listed_worms() {
            controls.u32(w.expect("an empty place among the game's worms").data.control_states);
        }
        game.process_frame(&HOOKS);

        let over = game.is_over();
        if over || tick % interval == 0 {
            let end = if over { " end" } else { "" };
            out.line(&format!("{name} {tick} {:016x}{end}", hash_game(&mut game)));
            out.hash(&format!("{name}-controls"), tick.into(), controls);
            for i in 0..2 {
                let w = game.worm_at(i).expect("no worm");
                let h = liero_worm_ai_hash(w.ai_mut().expect("a bot"));
                out.line(&format!("{name}-ai{i} {tick} {h:016x}"));
            }
            out.line(&format!(
                "{name} {tick} {:016x} frame",
                hash_frame(&mut game, &mut split, &mut spectator)
            ));
        }
        if over {
            break;
        }
    }

    let g = game.data_mut().cycles;
    let w: Vec<(i32, i32)> = game
        .listed_worms()
        .map(|w| w.map_or((0, 0), |w| (w.data.kills, w.data.lives)))
        .collect();
    out.line(&format!(
        "{name} summary: {g} ticks, kills {}-{}, lives {}-{}",
        w[0].0, w[1].0, w[0].1, w[1].1
    ));
}

pub fn hash_ai_games(a: &mut LieroApp, out: &mut Out) {
    let tc = super::tc::bundled();
    for c in &CONFIGS {
        run_ai_game(a, out, &tc, c, if c.ticks <= 200 { 10 } else { 50 });
    }
}
