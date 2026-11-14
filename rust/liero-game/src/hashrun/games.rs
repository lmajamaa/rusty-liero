//! The bot games (`runGame` and `setUpGame` in `src/game/hashrun.cpp`, and
//! the stats screen from `hashstats.cpp`): games from fixed seeds, hashed
//! every `interval` ticks with their frames and stats, then the stats
//! screen, and the replay each recorded played back.

use super::game::{add_screen, hash_frame, hash_game, hash_stats};
use super::{Fnv, Out};
use crate::app::App;
use crate::fs::Node;
use crate::game::{GameHooks, Rect};
use crate::game_box::Game;
use crate::level::LevelBox;
use crate::renderer::Renderer;
use crate::replay_file::ReplayWriter;
use crate::settings::SettingsBox;
use crate::sfx::SoundOut;
use crate::stats::{stats_scene, StatsScreen};
use crate::tc::{SharedTc, TcData};
use crate::viewport::{LIERO_VIEWPORT_SPECTATOR, LIERO_VIEWPORT_WORM};
use crate::weapsel::Weapsel;

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
    RunConfig {
        name: "kill-1",
        seed: 1,
        game_mode: KILL_EM_ALL,
        weapons: None,
    },
    RunConfig {
        name: "kill-2",
        seed: 2,
        game_mode: KILL_EM_ALL,
        weapons: None,
    },
    RunConfig {
        name: "kill-3",
        seed: 3,
        game_mode: KILL_EM_ALL,
        weapons: None,
    },
    RunConfig {
        name: "kill-4",
        seed: 4,
        game_mode: KILL_EM_ALL,
        weapons: None,
    },
    RunConfig {
        name: "tag-5",
        seed: 5,
        game_mode: GAME_OF_TAG,
        weapons: None,
    },
    RunConfig {
        name: "zone-6",
        seed: 6,
        game_mode: HOLDAZONE,
        weapons: None,
    },
    RunConfig {
        name: "explosives-7",
        seed: 7,
        game_mode: KILL_EM_ALL,
        weapons: Some(&EXPLOSIVES),
    },
    RunConfig {
        name: "bombs-8",
        seed: 8,
        game_mode: KILL_EM_ALL,
        weapons: Some(&BOMBS),
    },
    RunConfig {
        name: "guns-9",
        seed: 9,
        game_mode: KILL_EM_ALL,
        weapons: Some(&GUNS),
    },
    RunConfig {
        name: "shotguns-10",
        seed: 10,
        game_mode: KILL_EM_ALL,
        weapons: Some(&SHOTGUNS),
    },
    RunConfig {
        name: "rockets-11",
        seed: 11,
        game_mode: KILL_EM_ALL,
        weapons: Some(&ROCKETS),
    },
    RunConfig {
        name: "beams-12",
        seed: 12,
        game_mode: KILL_EM_ALL,
        weapons: Some(&BEAMS),
    },
    RunConfig {
        name: "throwers-13",
        seed: 13,
        game_mode: KILL_EM_ALL,
        weapons: Some(&THROWERS),
    },
    RunConfig {
        name: "traps-14",
        seed: 14,
        game_mode: KILL_EM_ALL,
        weapons: Some(&TRAPS),
    },
    RunConfig {
        name: "soj-15",
        seed: 15,
        game_mode: SCALES_OF_JUSTICE,
        weapons: None,
    },
    RunConfig {
        name: "soj-guns-16",
        seed: 16,
        game_mode: SCALES_OF_JUSTICE,
        weapons: Some(&GUNS),
    },
    RunConfig {
        name: "zone-shotguns-17",
        seed: 17,
        game_mode: HOLDAZONE,
        weapons: Some(&SHOTGUNS),
    },
];

pub(super) use crate::tc::tc_data;

/// The menu positions of the named weapons.
fn weapon_positions(d: &TcData, names: &[&str; 5]) -> [u32; 5] {
    let (weapons, order) = (&d.model.weapons, &d.weap_order);
    names.map(|name| {
        // The last of the same name, as the C++ loop left it.
        let found = order.iter().rposition(|&w| {
            let w: &liero_data::model::Weapon = &weapons[w as usize];
            w.name == name.as_bytes()
        });
        found.unwrap_or_else(|| panic!("No weapon called {name} in the TC")) as u32 + 1
    })
}

fn no_ticks() -> u64 {
    0
}

/// What a tick runs with: the stats clock isn't hashed.
pub(super) const HOOKS: GameHooks<'static> = GameHooks { hires_ticks: &no_ticks };

/// A bot-vs-bot game as the hash run sets it up, before it starts, with
/// random weapons when `weapons` is None (`setUpGame`).
pub(super) fn set_up_game(
    a: &mut App,
    tc: &SharedTc,
    seed: u32,
    game_mode: u32,
    weapons: Option<&[&str; 5]>,
) -> Box<Game> {
    let d = tc_data(tc);
    let settings = SettingsBox::defaults();
    let mut s = settings.borrow_mut();
    s.data.game_mode = game_mode;
    for ws in &s.data.worms {
        let mut ws = ws.borrow_mut();
        // The original bot
        ws.data.controller = 1;
        if let Some(names) = weapons {
            ws.data.weapons = weapon_positions(&d, names);
        }
    }
    // 0 gives bots random weapons; 2 keeps the ones set above. (The
    // default, 1, would keep the menu's default weapons: all bazookas.)
    s.data.ext.select_bot_weapons = if weapons.is_some() { 2 } else { 0 };
    let worms = s.data.worms.clone();
    drop(s);

    // Everything random comes from these seeds. Weapon selection and the
    // level use the app's random numbers, as in the menus.
    a.rand.seed(seed);

    // SAFETY: the game, its worms and viewports are made as the local
    // controller makes them, with settings and a TC that it keeps.
    unsafe {
        let mut game = Game::new();
        game.set_context(Some(settings.clone()), Some(tc.clone()), SoundOut::NONE);
        game.data_mut().rand.seed(seed.wrapping_mul(7919));

        for (i, ws) in worms.iter().enumerate() {
            let id = game.new_worm(Some(ws.clone()));
            let ws = ws.borrow();
            game.list_worm(Some(id));
            let w = game.worm_mut(id);
            w.data.health = ws.data.health;
            w.data.index = i as i32;
            w.data.stats_x = if i == 0 { 0 } else { 218 };
            w.set_ai(1, 0, false);
            let ai = w.ai_mut().expect("a bot");
            ai.rand().seed(seed.wrapping_mul(104_729).wrapping_add(i as u32));
        }

        let r = |x1, y1, x2, y2| Rect { x1, y1, x2, y2 };
        game.add_viewport(r(0, 0, 158, 158), 0, 504, 350, LIERO_VIEWPORT_WORM, false);
        game.add_viewport(r(160, 0, 158 + 160, 158), 1, 504, 350, LIERO_VIEWPORT_WORM, false);
        game.add_viewport(r(0, 0, 504 + 68, 350), 0, 504, 350, LIERO_VIEWPORT_SPECTATOR, true);

        let mut level = LevelBox::new();
        level.generate_from_settings(&d.level_common(), &d.palette_colors, &settings.borrow(), &mut a.rand);
        std::mem::swap(game.level_box_mut(), &mut level);

        Weapsel::start(&mut game, &mut a.rand).finish();

        let lives = settings.borrow().data.lives;
        for w in game.listed_worms() {
            w.expect("an empty place among the game's worms").data.lives = lives;
        }
        game
    }
}

/// The bots' controls for the next tick (`botTick`), untimed.
pub(super) fn bot_tick(game: &mut Game) {
    game.process_ais(None);
}

/// Ends the game's stats and draws the stats screen over the last frame, at
/// each pane and part way between them, scrolled to several depths
/// (`hashStatsScreen`).
fn hash_stats_screen(out: &mut Out, name: &str, game: &mut Game, renderer: &mut Renderer, d: &TcData) {
    game.stats_finish();
    let g = game.stats_game();
    let stats = game.stats();
    out.line(&format!("{name}-stats-end 0 {:016x}", hash_stats(stats)));

    let mut bg = crate::renderer::BitmapBox::new();
    bg.copy_from(&renderer.bmp);
    let screen = StatsScreen::new(stats, renderer.data.render_res_x);
    let scene = stats_scene(d, &g, renderer.data.render_res_x, renderer.data.render_res_y);

    let mut n = 0;
    for pane in [0.0, -1.0, 1.0, 0.37, -0.81] {
        for offset in [0.0, -35.5, -140.0, -400.0, -1200.0] {
            renderer.bmp.copy_from(&bg);
            screen.draw(stats, &scene, &renderer.bmp.view(), pane, offset);
            renderer.data.pal = d.palette_colors;

            let mut f = Fnv::default();
            add_screen(&mut f, renderer);
            out.line(&format!("{name}-statsview {n} {:016x}", f.0));
            n += 1;
        }
    }
}

fn run_game(a: &mut App, out: &mut Out, tc: &SharedTc, config: &RunConfig, ticks: i32, interval: i32) {
    let d = tc_data(tc);
    let mut game = set_up_game(a, tc, config.seed, config.game_mode, config.weapons);
    let name = config.name;

    // Records a replay of the game, as the local controller does when
    // replays are on, to play it back afterwards.
    let path = format!("/tmp/{name}.lrp");
    let mut writer = ReplayWriter::new(&Node::new(path.as_bytes())).unwrap_or_else(|| panic!("Could not write {path}"));
    let o = writer.begin(&mut game);
    assert!(o.status == crate::replay::ReplayStatus::Ok, "the replay didn't begin");

    let blood = game.settings_box().data.ext.blood_particle_max;
    game.start(blood);

    // Drawing doesn't change the game, so the game hashes don't depend on it.
    let mut split = Renderer::new();
    let mut spectator = Renderer::new();
    split.set_render_resolution(320, 200);
    spectator.set_render_resolution(640, 400);
    for r in [&mut split, &mut spectator] {
        r.load_palette(&d);
        // SAFETY: the renderer is a live one.
        unsafe { game.focus(&mut **r) };
    }

    out.line(&format!("{name} 0 {:016x}", hash_game(&mut game)));
    out.line(&format!(
        "{name} 0 {:016x} frame",
        hash_frame(&mut game, &mut split, &mut spectator)
    ));

    for tick in 1..=ticks {
        bot_tick(&mut game);
        writer.frame(&mut game);
        game.process_frame(&HOOKS);

        let over = game.is_over();
        if over || tick % interval == 0 {
            let end = if over { " end" } else { "" };
            out.line(&format!("{name} {tick} {:016x}{end}", hash_game(&mut game)));
            out.line(&format!(
                "{name} {tick} {:016x} frame",
                hash_frame(&mut game, &mut split, &mut spectator)
            ));
            out.line(&format!("{name}-stats {tick} {:016x}", hash_stats(game.stats())));
        }
        if over {
            break;
        }
    }

    hash_stats_screen(out, name, &mut game, &mut split, &d);
    // Finishes the file.
    drop(writer);
    super::replays::hash_replay(out, tc, name, &path, 1000, ticks, 0);

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

/// Every bot game, `ticks` long at most and hashed every `interval`.
pub fn hash_games(a: &mut App, out: &mut Out, ticks: i32, interval: i32) {
    let tc = super::tc::bundled();
    for config in &CONFIGS {
        run_game(a, out, &tc, config, ticks, interval);
    }
}
