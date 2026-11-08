//! The games the AI plays its plans out on: the game it is in, and copies
//! of it as C++'s `Game(Game const&)` and `postClone` made them, with no
//! sounds or stats, no viewports and `quick_sim` on.
//!
//! One thing the copies keep, as the C++ ones did, is the game's spectator
//! viewports: their shake and death banner move with every tick the AI
//! plays out.

use std::ffi::c_void;
use std::ptr::null_mut;

use super::dumb::{self, LieroDumbAiWorm};
use super::path::LevelView;
use super::predictive::{Games, LieroAiGame, LieroAiWeapon, LieroAiWorm};
use crate::game::{Game, LieroGameData, LieroGameFields, LieroGameHooks, LieroGameSettings, LieroGameWorm};
use crate::level::LieroLevel;
use crate::material::LieroMaterial;
use crate::rand::LieroRand;
use crate::tc::LieroTcWeapon;
use crate::world::World;
use crate::worm::LieroWormData;

/// The AI's settings (`Settings::aiFrames`, `aiMutations`, `aiTraces`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroAiSettings {
    pub frames: i32,
    pub mutations: i32,
    pub traces: bool,
}

/// A game for the AI: the one it is in (borrowed, owning nothing) or a
/// copy, which owns its state, level and worms. `fields.worms` points into
/// `worms`.
pub struct SimGame {
    fields: LieroGameFields,
    settings: LieroGameSettings,
    ai: LieroAiSettings,
    worms: Vec<LieroGameWorm>,
    _data: Option<Box<LieroGameData>>,
    _pixels: Vec<u8>,
    _materials: Vec<LieroMaterial>,
    _worm_data: Vec<Box<LieroWormData>>,
}

unsafe extern "C" fn no_tick(_: *mut c_void) {}
unsafe extern "C" fn no_ticks(_: *mut c_void) -> u64 {
    0
}

/// `NullSoundPlayer` and the base `StatsRecorder`, and no viewports to
/// process.
const NO_HOOKS: LieroGameHooks = LieroGameHooks {
    ctx: null_mut(),
    hires_ticks: no_ticks,
    end_tick: no_tick,
};

impl SimGame {
    /// The game the AI is in.
    ///
    /// # Safety
    /// The pointers in `fields` and `settings` must be valid while it is
    /// used.
    unsafe fn borrowed(fields: &LieroGameFields, settings: &LieroGameSettings, ai: &LieroAiSettings) -> Box<Self> {
        let worms = std::slice::from_raw_parts(fields.worms, fields.worm_count as usize).to_vec();
        let mut g = Box::new(SimGame {
            fields: *fields,
            settings: *settings,
            ai: *ai,
            worms,
            _data: None,
            _pixels: Vec::new(),
            _materials: Vec::new(),
            _worm_data: Vec::new(),
        });
        g.fields.worms = g.worms.as_ptr();
        g
    }

    /// `Game(Game const&)` and `postClone(original)`.
    ///
    /// # Safety
    /// As for `borrowed`.
    unsafe fn copy(&self) -> Box<Self> {
        let mut data = (*self.fields.game).clone_boxed();
        data.quick_sim = true;

        let level = self.fields.level;
        let len = (level.width.max(0) * level.height.max(0)) as usize;
        let mut pixels = std::slice::from_raw_parts(level.data, len).to_vec();
        let mut materials = std::slice::from_raw_parts(level.materials, len).to_vec();

        let mut worm_data: Vec<Box<LieroWormData>> = self.worms.iter().map(|w| Box::new(*w.data)).collect();
        let worms: Vec<LieroGameWorm> = worm_data
            .iter_mut()
            .zip(&self.worms)
            .map(|(d, w)| LieroGameWorm { data: &mut **d, max_health: w.max_health })
            .collect();

        let mut fields = self.fields;
        fields.game = &mut *data;
        fields.level = LieroLevel {
            data: pixels.as_mut_ptr(),
            materials: materials.as_mut_ptr(),
            width: level.width,
            height: level.height,
        };
        fields.viewports = std::ptr::null();
        fields.viewport_count = 0;
        fields.stats = std::ptr::null_mut();
        fields.sound = crate::sfx::LieroSoundOut::NONE;

        let mut g = Box::new(SimGame {
            fields,
            settings: self.settings,
            ai: self.ai,
            worms,
            _data: Some(data),
            _pixels: pixels,
            _materials: materials,
            _worm_data: worm_data,
        });
        g.fields.worms = g.worms.as_ptr();
        g
    }

    fn worm(&self, place: i32) -> *mut LieroWormData {
        self.worms[place as usize].data
    }
}

/// The games, as `*mut SimGame`.
struct Sims;

// SAFETY, for each call: the games are the one `liero_follow_ai_process`
// was given or copies made here, and are valid until freed.
impl Games for Sims {
    type G = *mut SimGame;

    fn clone_game(&mut self, g: Self::G) -> Self::G {
        unsafe { Box::into_raw((*g).copy()) }
    }

    fn free_game(&mut self, g: Self::G) {
        unsafe { drop(Box::from_raw(g)) }
    }

    fn process_frame(&mut self, g: Self::G) {
        unsafe {
            let g = &*g;
            World { g: Game { f: &g.fields, s: &g.settings }, h: &NO_HOOKS }.process_frame();
        }
    }

    fn game(&mut self, g: Self::G) -> LieroAiGame {
        unsafe {
            let g = &*g;
            let d = &*g.fields.game;
            let z = &d.holdazone;
            LieroAiGame {
                game_mode: g.settings.game_mode as u32,
                holder_idx: z.holder_idx,
                contender_idx: z.contender_idx,
                contender_frames: z.contender_frames,
                zone_center: [(z.rect.x1 + z.rect.x2) / 2, (z.rect.y1 + z.rect.y2) / 2],
                last_killed_idx: d.last_killed_idx,
                ai_frames: g.ai.frames,
                ai_mutations: g.ai.mutations,
                ai_traces: g.ai.traces,
            }
        }
    }

    fn worm(&mut self, g: Self::G, place: i32) -> LieroAiWorm {
        unsafe {
            let g = &*g;
            let w = &*g.worm(place);
            LieroAiWorm {
                index: w.index,
                pos: [w.pos.x, w.pos.y],
                aiming_angle: w.aiming_angle,
                direction: w.direction,
                visible: w.visible,
                ready: w.ready,
                health: w.health,
                lives: w.lives,
                settings_health: g.worms[place as usize].max_health,
                current_weapon: w.current_weapon,
                weapons: w.weapons.map(|ww| {
                    let t = &*(ww.kind as *const LieroTcWeapon);
                    LieroAiWeapon {
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
    }

    fn set_controls(&mut self, g: Self::G, place: i32, controls: u32) {
        unsafe { (*(*g).worm(place)).control_states = controls }
    }

    fn level(&self, g: Self::G) -> LevelView<'_> {
        unsafe {
            let g = &*g;
            let l = g.fields.level;
            let len = (l.width.max(0) * l.height.max(0)) as usize;
            // The level stays as it is while the AI looks at it: only
            // `process_frame` changes it.
            LevelView {
                data: if len == 0 { &[] } else { std::slice::from_raw_parts(l.data, len) },
                width: l.width,
                height: l.height,
                materials: std::slice::from_raw_parts((*g.fields.tc).materials.as_ptr().cast(), 256),
            }
        }
    }
}

/// One tick of the predictive AI for the worm at `place`.
///
/// # Safety
/// The pointers in `fields` and `settings` must be valid for the call.
pub unsafe fn process(
    ai: &mut super::predictive::FollowAi,
    fields: &LieroGameFields,
    settings: &LieroGameSettings,
    ai_settings: &LieroAiSettings,
    place: i32,
) -> u32 {
    let game = Box::into_raw(SimGame::borrowed(fields, settings, ai_settings));
    let controls = ai.process(&mut Sims, game, place);
    drop(Box::from_raw(game));
    controls
}

/// One tick of the original bot for the worm at `place`, with its own
/// generator.
///
/// # Safety
/// The pointers in `fields` must be valid for the call.
pub unsafe fn process_dumb(rand: &mut LieroRand, fields: &LieroGameFields, place: i32) -> u32 {
    let worms = std::slice::from_raw_parts(fields.worms, fields.worm_count as usize);
    let me = &*worms[place as usize].data;
    let w = &*(me.weapons[me.current_weapon as usize].kind as *const LieroTcWeapon);

    let worm = LieroDumbAiWorm {
        pos: [me.pos.x, me.pos.y],
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

    let others: Vec<[i32; 2]> = worms
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != place as usize)
        .map(|(_, o)| [(*o.data).pos.x, (*o.data).pos.y])
        .collect();

    let k = &*((*fields.tc).ai_params.as_ptr() as *const [[i32; 7]; 2]);
    dumb::process(rand, &worm, &others, k)
}
