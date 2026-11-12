use super::*;
use crate::ai::path::{FULL_HEIGHT, FULL_WIDTH};
use crate::material::LieroMaterial;
use crate::material::LIERO_MATERIAL_BACKGROUND;

#[test]
fn states_decompose_into_their_parts() {
    let mut seen = std::collections::HashSet::new();
    for i in 0..STATES as i32 {
        let (t, a, b, c) = InputState(i).decompose();
        assert!(seen.insert((t as u8, a, b, c)), "state {i}");
        match t {
            InputType::MoveJumpFire => {
                assert!(i < 48 && (0..3).contains(&a) && (0..4).contains(&b) && (0..4).contains(&c));
                assert_eq!(a * 16 + b * 4 + c, i);
            }
            InputType::ChangeWeapon => assert_eq!(InputState::compose_change_weapon(a), InputState(i)),
            InputType::RopeUpDown => assert!((0..3).contains(&a)),
        }
        assert_eq!(InputState(i).is_neutral(), (48..53).contains(&i));
        assert_eq!(InputState(i).is_firing(), i < 48 && i & 2 != 0);
    }
}

#[test]
fn contexts_pack_and_unpack() {
    for idx in 0..CONTEXTS {
        let (s, facing_enemy, ninjarope_out) = InputContext::unpack(idx);
        let c = InputContext {
            current_state: s,
            facing_enemy,
            ninjarope_out,
            ..Default::default()
        };
        assert_eq!(c.pack(), idx);
    }
}

#[test]
fn weapon_change_offsets_go_the_short_way() {
    for current in 0..5 {
        for wanted in 0..5 {
            let o = weapon_change_offset(wanted, current);
            assert!((-2..=2).contains(&o));
            assert_eq!((current + o + 5) % 5, wanted);
        }
    }
}

#[test]
fn model_rows_are_chances() {
    let m = TransModel::new(&Weights::default());
    assert_eq!(m.trans.len(), CONTEXTS);
    for (i, row) in m.trans.iter().enumerate() {
        let sum: f64 = row.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "row {i}: {sum}");
        assert!(row.iter().all(|&p| p > 0.0), "row {i}");
    }
    // Holding fire is likelier facing the enemy.
    let row = |s: i32, facing: i32| {
        m.trans[InputContext {
            current_state: InputState(s),
            facing_enemy: facing,
            ..Default::default()
        }
        .pack()]
    };
    assert!(row(0, 1)[2] > row(0, 0)[2]);
    // A worm on the rope jumps less.
    let rope = m.trans[InputContext {
        ninjarope_out: 1,
        ..Default::default()
    }
    .pack()];
    assert!(rope[1] < row(0, 0)[1]);
}

#[test]
fn model_random_follows_the_weights() {
    let mut m = TransModel::new(&Weights::default());
    let c = InputContext::default();
    m.trans[c.pack()] = [0.0; STATES];
    m.trans[c.pack()][17] = 1.0;
    let mut rand = LieroRand::new(3);
    for _ in 0..100 {
        assert_eq!(m.random(&c, &mut rand), InputState(17));
    }
    m.update(&c, InputState(40));
    assert_eq!(m.trans[c.pack()][40], 0.005);
    let picks = (0..10000).filter(|_| m.random(&c, &mut rand) == InputState(40)).count();
    // 0.005 / 1.005 of the picks, about 50.
    assert!((20..100).contains(&picks), "{picks}");
}

/// Two worms in an empty level. Frames move each worm a pixel the way it
/// presses and wear the target's health down while it's being aimed at.
#[derive(Clone, Default)]
struct FakeGame {
    worms: [LieroAiWorm; 2],
    info: LieroAiGame,
    freed: bool,
}

struct Fake {
    games: Vec<FakeGame>,
    level: Vec<u8>,
    materials: Vec<LieroMaterial>,
    clones: usize,
    frees: usize,
}

impl Fake {
    fn new() -> Self {
        let worm = |index, x: i32, direction| LieroAiWorm {
            index,
            pos: [x << 16, 100 << 16],
            aiming_angle: 32 << 16,
            direction,
            visible: true,
            ready: true,
            health: 100,
            lives: 3,
            settings_health: 100,
            weapons: [LieroAiWeapon {
                ammo: 5,
                loading_time: 50,
                max_ammo: 5,
                ..Default::default()
            }; 5],
            ..Default::default()
        };
        let info = LieroAiGame {
            ai_frames: 20,
            ai_mutations: 2,
            ai_traces: true,
            last_killed_idx: -1,
            holder_idx: -1,
            contender_idx: -1,
            ..Default::default()
        };
        Fake {
            games: vec![FakeGame {
                worms: [worm(0, 100, 1), worm(1, 300, 0)],
                info,
                freed: false,
            }],
            level: vec![0; (FULL_WIDTH * FULL_HEIGHT) as usize],
            materials: vec![
                LieroMaterial {
                    flags: LIERO_MATERIAL_BACKGROUND
                };
                256
            ],
            clones: 0,
            frees: 0,
        }
    }
}

impl Games for Fake {
    type G = usize;

    fn clone_game(&mut self, g: usize) -> usize {
        assert!(!self.games[g].freed);
        self.clones += 1;
        let copy = self.games[g].clone();
        self.games.push(copy);
        self.games.len() - 1
    }

    fn free_game(&mut self, g: usize) {
        assert!(g != 0 && !self.games[g].freed, "freed game {g} twice, or the original");
        self.frees += 1;
        self.games[g].freed = true;
    }

    fn process_frame(&mut self, g: usize) {
        let game = &mut self.games[g];
        assert!(!game.freed);
        for w in &mut game.worms {
            let cs = w.control_states;
            if pressed(cs, LEFT) {
                w.pos[0] -= 1 << 16;
                w.direction = 0;
            }
            if pressed(cs, RIGHT) {
                w.pos[0] += 1 << 16;
                w.direction = 1;
            }
        }
        for i in 0..2 {
            if pressed(game.worms[i].control_states, FIRE) {
                let t = &mut game.worms[i ^ 1];
                t.health = (t.health - 1).max(0);
            }
        }
    }

    fn game(&mut self, g: usize) -> LieroAiGame {
        self.games[g].info
    }

    fn worm(&mut self, g: usize, place: i32) -> LieroAiWorm {
        self.games[g].worms[place as usize]
    }

    fn set_controls(&mut self, g: usize, place: i32, controls: u32) {
        self.games[g].worms[place as usize].control_states = controls;
    }

    fn level(&self, _g: usize) -> LevelView<'_> {
        LevelView {
            data: &self.level,
            width: FULL_WIDTH,
            height: FULL_HEIGHT,
            materials: &self.materials,
        }
    }
}

fn run(ai: &mut FollowAi, fake: &mut Fake, ticks: usize) -> Vec<u32> {
    (0..ticks)
        .map(|_| {
            let cs = ai.process(fake, 0, 0);
            fake.set_controls(0, 0, cs);
            fake.process_frame(0);
            cs
        })
        .collect()
}

#[test]
fn plays_and_frees_every_copy() {
    let mut fake = Fake::new();
    let mut ai = FollowAi::new(Weights::default(), 3, true);
    ai.rand.seed(42);
    let controls = run(&mut ai, &mut fake, 30);
    assert_eq!(ai.frame, 30);
    assert!(fake.clones > 30);
    assert_eq!(fake.clones, fake.frees);
    assert!(controls.iter().all(|&cs| cs < 1 << 7));
    assert!(!ai.evaluate_positions.is_empty());
    let best = ai.best.unwrap();
    assert!(best < 3);
    // Each tick plays a plan's first state and drops it; the rest is kept.
    assert!(ai.cand_plan.iter().all(|p| !p.plan.is_empty()));

    // The same seed plays the same game.
    let mut again = Fake::new();
    let mut ai2 = FollowAi::new(Weights::default(), 3, true);
    ai2.rand.seed(42);
    assert_eq!(run(&mut ai2, &mut again, 30), controls);
    assert_eq!(again.games[0].worms[0].pos, fake.games[0].worms[0].pos);
}

#[test]
fn without_plans_it_presses_nothing() {
    let mut fake = Fake::new();
    let mut ai = FollowAi::new(Weights::default(), 0, false);
    assert_eq!(run(&mut ai, &mut fake, 5), [0; 5]);
    assert_eq!(fake.clones, 0);
}

#[test]
fn context_update_presses_the_state() {
    let mut fake = Fake::new();
    let mut c = InputContext::default();
    // Aiming up, moving right, firing and jumping.
    let cs = c.update(InputState(2 * 16 + 4 + 3), &mut fake, 0, 0);
    assert_eq!(cs, 1 << UP | 1 << RIGHT | 1 << FIRE | 1 << JUMP);
    // Worm 1 is to the right of worm 0, which faces right.
    assert_eq!((c.facing_enemy, c.ninjarope_out, c.hidden_frames), (1, 0, 0));
    // Changing to weapon 3 presses change, then walks there the short way.
    let cs = c.update(InputState::compose_change_weapon(3), &mut fake, 0, 0);
    assert_eq!((cs, c.wanted_weapon), (1 << CHANGE, 3));
    let cs = c.update(InputState(0), &mut fake, 0, 0);
    assert_eq!(cs, 1 << LEFT | 1 << CHANGE);
    // Letting go of the rope jumps.
    c.wanted_weapon = 0;
    let cs = c.update(InputState(53), &mut fake, 0, 0);
    assert_eq!(cs, 1 << CHANGE | 1 << JUMP);
}
