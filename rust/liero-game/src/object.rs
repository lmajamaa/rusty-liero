//! Object physics: blood particles (`bobject.cpp`), bonuses (`bonus.cpp`),
//! special objects such as explosions (`sobject.cpp`), the explosions'
//! pushes and the ninja rope's state. Results have to stay bit-identical to
//! the C++ versions, which the replay system and `tools/hashrun` check.
//!
//! Non-weapon objects (`nobject.cpp`) and the rope's physics
//! (`ninjarope.cpp`) are in `world/`, where they can create other objects
//! and hit worms directly.

use crate::level::{Common, Level};
use crate::math::{ftoi, FVec2, Fixed, IVec2};
use crate::objects::{Bonus, SObject};
use crate::rand::Rand;

/// Like C++'s `Level::inside`: negative coordinates wrap to large unsigned
/// values and fail.
pub(crate) fn inside(level: &Level, x: i32, y: i32) -> bool {
    (x as u32) < (level.width as u32) && (y as u32) < (level.height as u32)
}

/// A blood particle, laid out like C++'s `BObject`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BObject {
    pub pos: FVec2,
    pub vel: FVec2,
    pub color: i32,
}

impl BObject {
    /// Moves the particle one tick. Returns false when it left the level or
    /// landed and coloured a pixel, so C++ frees it.
    pub fn process(&mut self, level: &mut Level, common: &Common, rand: &mut Rand, gravity: i32) -> bool {
        self.pos += self.vel;
        let ipos = self.pos.to_int();

        if !inside(level, ipos.x, ipos.y) {
            return false;
        }

        let c = level.pixel(ipos.x, ipos.y);
        let m = level.mat(ipos.x, ipos.y);

        if m.background() {
            self.vel.y += Fixed(gravity);
        }

        // The colours of blood and of worms. TODO in C++: read from the EXE.
        let base = if (1..=2).contains(&c) || (77..=79).contains(&c) {
            77
        } else if m.any_dirt() {
            82
        } else if m.rock() {
            85
        } else {
            return true;
        };

        let w = base + rand.below(3) as u8;
        level.set_pixel(ipos.x, ipos.y, w, common);
        false
    }
}

/// Constants for bonuses, from the TC (`LC(Bonus...)` in C++).
#[derive(Clone, Copy, Debug, Default)]
pub struct BonusConstants {
    pub gravity: i32,
    pub bounce_mul: i32,
    pub bounce_div: i32,
}

impl Bonus {
    /// Moves the bonus box one tick: falling, bouncing on the ground and
    /// counting down `timer`. Returns true when the timer ran out, so C++
    /// explodes it. `x` must be inside the level.
    pub fn process(&mut self, level: &Level, constants: &BonusConstants) -> bool {
        let lv = level;

        self.y = self.y.wrapping_add(self.vel_y);

        let ix = ftoi(self.x);
        let iy = ftoi(self.y);

        if inside(level, ix, iy + 1) && lv.mat(ix, iy + 1).background() {
            self.vel_y = self.vel_y.wrapping_add(constants.gravity);
        }

        let inew_y = ftoi(self.y.wrapping_add(self.vel_y));
        if inew_y < 0 || inew_y >= level.height - 1 || lv.mat(ix, inew_y).dirt_rock() {
            self.vel_y = -(self.vel_y.wrapping_mul(constants.bounce_mul)) / constants.bounce_div;

            // TODO in C++: read from the EXE.
            if self.vel_y.abs() < 100 {
                self.vel_y = 0;
            }
        }

        self.timer -= 1;
        self.timer <= 0
    }
}

impl SObject {
    /// Advances the explosion's animation one tick. Returns true when the
    /// animation is over, so C++ frees the object.
    pub fn animate(&mut self, type_anim_delay: i32, num_frames: i32) -> bool {
        self.anim_delay -= 1;
        if self.anim_delay <= 0 {
            self.anim_delay = type_anim_delay;
            self.cur_frame += 1;
            if self.cur_frame > num_frames {
                return true;
            }
        }
        false
    }
}

/// Whether integer position (px, py) is strictly within `range` of (x, y)
/// on both axes, as explosions test it.
fn in_blast(px: i32, py: i32, x: i32, y: i32, range: i32) -> bool {
    px < x + range && px > x - range && py < y + range && py > y - range
}

/// Pushes a worm at fixed-point `pos` away from an explosion at (x, y), as
/// `SObjectType::create` does. A worm already moving at 2 pixels per tick or
/// more on an axis isn't pushed on that axis. Returns false when the worm is
/// out of range; otherwise sets `power_sum`, which sets the damage.
pub fn explosion_push_worm(
    pos: FVec2,
    vel: &mut FVec2,
    x: i32,
    y: i32,
    range: i32,
    blow_away: i32,
    power_sum: &mut i32,
) -> bool {
    let IVec2 { x: wix, y: wiy } = pos.to_int();
    if !in_blast(wix, wiy, x, y, range) {
        return false;
    }

    let delta = wix - x;
    let power = range - delta.abs();
    let mut sum = power;

    // TODO in C++: read from the EXE.
    if vel.x.abs() < Fixed::from_int(2) {
        let push = Fixed(blow_away.wrapping_mul(power));
        vel.x = if delta > 0 { vel.x + push } else { vel.x - push };
    }

    let delta = wiy - y;
    let power = range - delta.abs();
    sum = (sum + power) / 2;

    if vel.y.abs() < Fixed::from_int(2) {
        let push = Fixed(blow_away.wrapping_mul(power));
        vel.y = if delta > 0 { vel.y + push } else { vel.y - push };
    }

    *power_sum = sum;
    true
}

/// The damage an explosion does to a worm, from `power_sum`.
pub fn explosion_damage(damage: i32, power_sum: i32, range: i32) -> i32 {
    let z = damage.wrapping_mul(power_sum);
    if range != 0 {
        z / range
    } else {
        z
    }
}

/// Pushes an object that explosions affect away from an explosion at
/// (x, y). Returns whether it was in range (for chain explosions).
pub fn explosion_push_object(pos: FVec2, vel: &mut FVec2, x: i32, y: i32, range: i32, blow_away: i32) -> bool {
    let ipos = pos.to_int();
    if !in_blast(ipos.x, ipos.y, x, y, range) {
        return false;
    }

    for (p, c, v) in [(ipos.x, x, &mut vel.x), (ipos.y, y, &mut vel.y)] {
        let delta = p - c;
        let power = range - delta.abs();
        if power > 0 {
            let push = Fixed(blow_away.wrapping_mul(power));
            if delta > 0 {
                *v += push;
            } else if delta < 0 {
                *v -= push;
            }
        }
    }
    true
}

/// The ninja rope, laid out like C++'s `Ninjarope`.
#[derive(Clone, Copy, Debug)]
pub struct Ninjarope {
    pub out: bool,
    pub attached: bool,
    /// The worm the rope is attached to, if any.
    pub anchor: Option<crate::worm::WormId>,
    pub pos: FVec2,
    pub vel: FVec2,
    pub length: i32,
    pub cur_len: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::itof;

    #[test]
    fn sobject_animation_ends_after_last_frame() {
        let mut o = SObject {
            used: true,
            x: 0,
            y: 0,
            id: 0,
            cur_frame: 0,
            anim_delay: 1,
        };
        let mut ticks = 0;
        while !o.animate(2, 3) {
            ticks += 1;
        }
        // Frames 1, 2 and 3 each last 2 ticks after the first.
        assert_eq!((ticks, o.cur_frame), (6, 4));
    }

    #[test]
    fn explosion_pushes_worm_away_and_sets_power() {
        let mut vel = FVec2::default();
        let mut sum = 0;
        let pos = FVec2::new(Fixed(itof(13)), Fixed(itof(8)));
        assert!(explosion_push_worm(pos, &mut vel, 10, 10, 6, 100, &mut sum));
        // power 3 on x, 4 on y; y pushed up since the worm is above.
        assert_eq!(vel, FVec2::new(Fixed(300), Fixed(-400)));
        assert_eq!(sum, 3);
        assert_eq!(explosion_damage(10, sum, 6), 5);

        let far = FVec2::new(Fixed(itof(16)), Fixed(itof(10)));
        assert!(!explosion_push_worm(far, &mut vel, 10, 10, 6, 100, &mut sum));
    }

    #[test]
    fn fast_worm_is_not_pushed() {
        let mut vel = FVec2::new(Fixed(itof(2)), Fixed(-itof(3)));
        let mut sum = 0;
        let pos = FVec2::new(Fixed(itof(9)), Fixed(itof(12)));
        assert!(explosion_push_worm(pos, &mut vel, 10, 10, 6, 100, &mut sum));
        assert_eq!(vel, FVec2::new(Fixed(itof(2)), Fixed(-itof(3))));
    }

    #[test]
    fn object_at_centre_is_not_pushed() {
        let mut vel = FVec2::default();
        let pos = FVec2::new(Fixed(itof(10)), Fixed(itof(12)));
        assert!(explosion_push_object(pos, &mut vel, 10, 10, 6, 30));
        assert_eq!(vel, FVec2::new(Fixed(0), Fixed(30 * 4)));
    }
}
