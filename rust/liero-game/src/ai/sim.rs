//! The games the AI plays its plans out on: the game it is in, and copies
//! of it as C++'s `Game(Game const&)` and `postClone` made them, with no
//! sounds or stats, no viewports and `quick_sim` on.
//!
//! One thing the copies keep, as the C++ ones did, is the game's spectator
//! viewports: their shake and death banner move with every tick the AI
//! plays out.

use super::dumb::{self, DumbAiWorm};
use super::path::LevelView;
use super::predictive::{AiGame, AiWeapon, AiWorm, FollowAi, Games};
use crate::game::{GameData, GameFields, GameHooks, RuleSettings};
use crate::level::Level;
use crate::material::Material;
use crate::rand::Rand;
use crate::sfx::SoundOut;
use crate::tc::TcData;
use crate::world::World;
use crate::worm::WormData;

/// The AI's settings (`Settings::aiFrames`, `aiMutations`, `aiTraces`).
#[derive(Clone, Copy, Debug, Default)]
pub struct AiSettings {
    pub frames: i32,
    pub mutations: i32,
    pub traces: bool,
}

/// A copy of the game, which owns its state, level and listed worms.
struct SimCopy {
    data: Box<GameData>,
    pixels: Vec<u8>,
    materials: Vec<Material>,
    width: i32,
    height: i32,
    /// The listed worms, in the game's order.
    worms: Vec<WormData>,
}

/// One of the AI's games: the one it is in, or a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sim {
    Real,
    Copy(usize),
}

/// The games the AI plays out plans on: the one it is in, borrowed, and
/// the copies it made, by number.
struct Sims<'r, 'a> {
    real: &'r mut GameFields<'a>,
    settings: &'r RuleSettings,
    ai: &'r AiSettings,
    copies: Vec<Option<SimCopy>>,
}

fn no_ticks() -> u64 {
    0
}

/// `NullSoundPlayer` and the base `StatsRecorder`, and no viewports to
/// process.
const NO_HOOKS: GameHooks<'static> = GameHooks { hires_ticks: &no_ticks };

impl Sims<'_, '_> {
    fn copy(&self, i: usize) -> &SimCopy {
        self.copies[i].as_ref().expect("a freed game")
    }

    fn tc(&self) -> &TcData {
        self.real.tc.expect("the AI needs the TC")
    }

    fn data(&self, g: Sim) -> &GameData {
        match g {
            Sim::Real => self.real.game,
            Sim::Copy(i) => &self.copy(i).data,
        }
    }

    /// The listed worm at `place` in `g`.
    fn worm_data(&self, g: Sim, place: usize) -> &WormData {
        match g {
            Sim::Real => self.real.worm(place),
            Sim::Copy(i) => &self.copy(i).worms[place],
        }
    }

    fn worm_data_mut(&mut self, g: Sim, place: usize) -> &mut WormData {
        match g {
            Sim::Real => self.real.worm_mut(place),
            Sim::Copy(i) => &mut self.copies[i].as_mut().expect("a freed game").worms[place],
        }
    }

    /// `g`'s pixels and materials, width and height.
    fn level_data(&self, g: Sim) -> (&[u8], &[Material], i32, i32) {
        match g {
            Sim::Real => {
                let l = &self.real.level;
                (&*l.data, &*l.materials, l.width, l.height)
            }
            Sim::Copy(i) => {
                let c = self.copy(i);
                (&c.pixels, &c.materials, c.width, c.height)
            }
        }
    }

    /// `g` as the tick sees it. A copy has no sounds, stats or viewports,
    /// but as in C++ it shares two things with the game it came from: the
    /// worms the game doesn't list, which ropes can be anchored to, and the
    /// spectator viewports, whose shake and death banners its ticks move,
    /// which shows in the frames drawn.
    fn fields(&mut self, g: Sim) -> GameFields<'_> {
        let Sims { real, copies, .. } = self;
        let Sim::Copy(i) = g else { return real.reborrow() };
        let c = copies[i].as_mut().expect("a freed game");
        let mut pool: Vec<&mut WormData> = real.pool.iter_mut().map(|w| &mut **w).collect();
        for (w, copy) in real.worms.iter().zip(&mut c.worms) {
            pool[w.id.0 as usize] = copy;
        }
        GameFields {
            worms: real.worms.clone(),
            pool,
            game: &mut c.data,
            level: Level {
                data: &mut c.pixels,
                materials: &mut c.materials,
                width: c.width,
                height: c.height,
            },
            tc: real.tc,
            viewports: Vec::new(),
            spectator_viewports: real.spectator_viewports.iter_mut().map(|v| &mut **v).collect(),
            stats: None,
            sound: SoundOut::NONE,
        }
    }
}

impl Games for Sims<'_, '_> {
    type G = Sim;

    /// `Game(Game const&)` and `postClone(original)`, with `quick_sim` on.
    fn clone_game(&mut self, g: Sim) -> Sim {
        let mut data = self.data(g).clone_boxed();
        data.quick_sim = true;
        let (pixels, materials, width, height) = self.level_data(g);
        let copy = SimCopy {
            data,
            pixels: pixels.to_vec(),
            materials: materials.to_vec(),
            width,
            height,
            worms: (0..self.real.worms.len()).map(|p| *self.worm_data(g, p)).collect(),
        };
        match self.copies.iter().position(Option::is_none) {
            Some(i) => {
                self.copies[i] = Some(copy);
                Sim::Copy(i)
            }
            None => {
                self.copies.push(Some(copy));
                Sim::Copy(self.copies.len() - 1)
            }
        }
    }

    fn free_game(&mut self, g: Sim) {
        if let Sim::Copy(i) = g {
            self.copies[i] = None;
        }
    }

    fn process_frame(&mut self, g: Sim) {
        let settings = self.settings;
        World::new(self.fields(g), settings, &NO_HOOKS).process_frame();
    }

    fn game(&mut self, g: Sim) -> AiGame {
        let d = self.data(g);
        let z = &d.holdazone;
        AiGame {
            game_mode: self.settings.game_mode,
            holder_idx: z.holder_idx,
            contender_idx: z.contender_idx,
            contender_frames: z.contender_frames,
            zone_center: [(z.rect.x1 + z.rect.x2) / 2, (z.rect.y1 + z.rect.y2) / 2],
            last_killed_idx: d.last_killed_idx,
            ai_frames: self.ai.frames,
            ai_mutations: self.ai.mutations,
            ai_traces: self.ai.traces,
        }
    }

    fn worm(&mut self, g: Sim, place: i32) -> AiWorm {
        let tc = self.tc();
        let w = self.worm_data(g, place as usize);
        AiWorm {
            index: w.index,
            pos: w.pos.raw(),
            aiming_angle: w.aiming_angle,
            direction: w.direction,
            visible: w.visible,
            ready: w.ready,
            health: w.health,
            lives: w.lives,
            settings_health: self.real.worms[place as usize].max_health,
            current_weapon: w.current_weapon,
            weapons: w.weapons.map(|ww| {
                let t = &tc.model.weapons[ww.kind as usize];
                AiWeapon {
                    ammo: ww.ammo,
                    delay_left: ww.delay_left,
                    loading_left: ww.loading_left,
                    loading_time: t.loading_time,
                    max_ammo: t.ammo,
                }
            }),
            rope_out: w.ninjarope.out,
            steerable_count: w.steerable_count,
            steerable_sum: [w.steerable_sum_x, w.steerable_sum_y],
            control_states: w.control_states,
        }
    }

    fn set_controls(&mut self, g: Sim, place: i32, controls: u32) {
        self.worm_data_mut(g, place as usize).control_states = controls;
    }

    fn level(&self, g: Sim) -> LevelView<'_> {
        let (data, _, width, height) = self.level_data(g);
        LevelView {
            data,
            width,
            height,
            materials: &self.tc().materials,
        }
    }
}

/// One tick of the predictive AI for the worm at `place`.
pub fn process(
    ai: &mut FollowAi,
    fields: &mut GameFields,
    settings: &RuleSettings,
    ai_settings: &AiSettings,
    place: i32,
) -> u32 {
    let mut sims = Sims {
        real: fields,
        settings,
        ai: ai_settings,
        copies: Vec::new(),
    };
    ai.process(&mut sims, Sim::Real, place)
}

/// One tick of the original bot for the worm at `place`, with its own
/// generator.
pub fn process_dumb(rand: &mut Rand, fields: &GameFields, place: usize) -> u32 {
    let me = fields.worm(place);
    let tc = fields.tc.expect("the AI needs the TC");
    let w = &tc.model.weapons[me.weapons[me.current_weapon as usize].kind as usize];

    let worm = DumbAiWorm {
        pos: me.pos.raw(),
        visible: me.visible,
        aiming_angle: me.aiming_angle,
        direction: me.direction,
        reacts: me.reacts,
        rope_out: me.ninjarope.out,
        rope_attached: me.ninjarope.attached,
        control_states: me.control_states,
        time_to_explo: w.time_to_explo,
        time_to_explo_v: w.time_to_explo_v,
        speed: w.speed,
        gravity: w.gravity,
    };

    let others: Vec<[i32; 2]> = (0..fields.worms.len())
        .filter(|&i| i != place)
        .map(|i| fields.worm(i).pos.raw())
        .collect();

    dumb::process(rand, &worm, &others, &tc.model.ai_params)
}
