//! The original game's bot (`DumbLieroAI` in `worm.cpp`): it walks toward
//! the nearest worm, aims roughly at it, fires when it's close enough, and
//! jumps, changes weapons and swings on the rope at random, with the TC's
//! chances for each.

use super::predictive::{CHANGE, DOWN, FIRE, JUMP, LEFT, RIGHT, UP};
use crate::math::{cos_sin, ftoi, vector_length};
use crate::rand::Rand;

/// `reacts`' order.
const RF_DOWN: usize = 0;
const RF_LEFT: usize = 1;
const RF_RIGHT: usize = 3;

/// What the bot reads of its worm.
#[derive(Clone, Copy, Debug, Default)]
pub struct DumbAiWorm {
    pub pos: [i32; 2],
    pub visible: bool,
    pub aiming_angle: i32,
    pub direction: i32,
    pub reacts: [i32; 4],
    pub rope_out: bool,
    pub rope_attached: bool,
    pub control_states: u32,
    /// The current weapon's fuse, its random part, speed and gravity.
    pub time_to_explo: i32,
    pub time_to_explo_v: i32,
    pub speed: i32,
    pub gravity: i32,
}

struct Controls(u32);

impl Controls {
    fn pressed(&self, c: u32) -> bool {
        (self.0 >> c) & 1 != 0
    }

    fn set(&mut self, c: u32, on: bool) {
        if on {
            self.0 |= 1 << c;
        } else {
            self.0 &= !(1 << c);
        }
    }

    fn toggle(&mut self, c: u32) {
        let on = self.pressed(c);
        self.set(c, !on);
    }
}

/// One tick: the worm's controls after it. `others` are the other worms'
/// positions, and `k` the TC's `aiParams`: for each control, one in how
/// many ticks the bot changes it, when it's up and when it's down.
pub fn process(rand: &mut Rand, worm: &DumbAiWorm, others: &[[i32; 2]], k: &[[i32; 7]; 2]) -> u32 {
    let mut cs = Controls(worm.control_states);
    // A chance of one in k[pressed][control] ticks.
    let chance = |rand: &mut Rand, pressed: bool, control: u32| {
        rand.below(k[usize::from(pressed)][control as usize] as u32) == 0
    };

    // The nearest worm, the first of any as near. C++ read a null worm
    // when there were no others.
    let mut target: Option<[i32; 2]> = None;
    let mut min_len = 0;
    for &w in others {
        let dx = ftoi(worm.pos[0]) - ftoi(w[0]);
        let dy = ftoi(worm.pos[1]) - ftoi(w[1]);
        let len = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
        if target.is_none() || len < min_len {
            target = Some(w);
            min_len = len;
        }
    }
    let Some(target) = target else {
        return cs.0;
    };

    let mut max_dist = if worm.time_to_explo > 0 && worm.time_to_explo < 500 {
        (worm.time_to_explo - worm.time_to_explo_v / 2) * worm.speed / 130
    } else {
        worm.speed - worm.gravity / 10
    };
    if max_dist < 90 {
        max_dist = 90;
    }

    let mut delta = [target[0] - worm.pos[0], target[1] - worm.pos[1]];
    let real_dist = vector_length(ftoi(delta[0]), ftoi(delta[1]));

    if real_dist < max_dist || !worm.visible {
        let fire = cs.pressed(FIRE);
        if chance(rand, fire, FIRE) {
            cs.set(FIRE, !fire);
        }
    } else if worm.visible {
        cs.set(FIRE, false);
    }

    let jump = cs.pressed(JUMP);
    if chance(rand, jump, JUMP) {
        cs.toggle(JUMP);
    }
    let change = cs.pressed(CHANGE);
    if chance(rand, change, CHANGE) {
        cs.toggle(CHANGE);
    }

    if real_dist > 0 {
        delta = [delta[0] / real_dist, delta[1] / real_dist];
    } else {
        delta = [0, 0];
    }

    // The first direction close to the target's, with the original's
    // 0xC000 corrected to 0xC00.
    let mut dir = 1;
    while dir < 128 {
        let c = cos_sin(dir);
        if (c.x - delta[0]).abs() < 0xC00 && (c.y - delta[1]).abs() < 0xC00 {
            break;
        }
        dir += 1;
    }

    let adx = delta[0].abs();
    let ady = delta[1].abs();
    if dir >= 128 {
        dir = if delta[0] > 0 {
            if delta[1] < 0 {
                if ady > adx {
                    64 + rand.below(16) as i32
                } else if adx > ady {
                    80 + rand.below(16) as i32
                } else {
                    80
                }
            } else if adx > ady {
                96 + rand.below(16) as i32
            } else {
                116
            }
        } else if delta[1] < 0 {
            if ady > adx {
                48 + rand.below(16) as i32
            } else if adx > ady {
                32 + rand.below(16) as i32
            } else {
                48
            }
        } else if adx > ady {
            12 + rand.below(16) as i32
        } else {
            12
        };
    }

    if cs.pressed(CHANGE) {
        if chance(rand, cs.pressed(LEFT), LEFT) {
            cs.toggle(LEFT);
        }
        if chance(rand, cs.pressed(RIGHT), RIGHT) {
            cs.toggle(RIGHT);
        }
        if worm.rope_out && worm.rope_attached {
            let up = cs.pressed(UP);
            if chance(rand, up, UP) {
                cs.toggle(UP);
            }
            let down = cs.pressed(DOWN);
            if chance(rand, down, DOWN) {
                cs.toggle(DOWN);
            }
        } else {
            cs.set(UP, false);
            cs.set(DOWN, false);
        }
    } else {
        if real_dist > max_dist {
            cs.set(RIGHT, delta[0] > 0);
            cs.set(LEFT, delta[0] <= 0);
        } else {
            cs.set(RIGHT, false);
            cs.set(LEFT, false);
        }

        let aim = ftoi(worm.aiming_angle);
        if worm.direction != 0 {
            if dir < 64 {
                cs.set(LEFT, true);
            }
            cs.set(UP, dir + 1 < aim);
            cs.set(DOWN, dir - 1 > aim);
        } else {
            if dir > 64 {
                cs.set(RIGHT, true);
            }
            cs.set(UP, dir - 1 > aim);
            cs.set(DOWN, dir + 1 < aim);
        }

        if cs.pressed(LEFT) && worm.reacts[RF_RIGHT] != 0 {
            if worm.reacts[RF_DOWN] > 0 {
                cs.set(RIGHT, true);
            } else {
                cs.set(JUMP, true);
            }
        }
        if cs.pressed(RIGHT) && worm.reacts[RF_LEFT] != 0 {
            if worm.reacts[RF_DOWN] > 0 {
                cs.set(LEFT, true);
            } else {
                cs.set(JUMP, true);
            }
        }
    }
    cs.0
}

#[cfg(test)]
#[path = "dumb_tests.rs"]
mod tests;
