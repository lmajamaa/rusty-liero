//! Worms (`worm.cpp`): a worm's state, the TC's constants for worms, and
//! the sprite frame for its aim.
//!
//! The tick (`Worm::process`) is in `world/worm.rs`, where it can reach the
//! rest of the game.

use crate::math::{ftoi, IVec2};
use crate::object::Ninjarope;
use crate::weapon::{WormWeapon, SELECTABLE_WEAPONS};

/// Indices into `reacts`: how much terrain pushes the worm in each
/// direction (`Worm::RFDown` and so on). RF_UP is ground below the worm.
pub(crate) const RF_DOWN: usize = 0;
pub(crate) const RF_LEFT: usize = 1;
pub(crate) const RF_UP: usize = 2;
pub(crate) const RF_RIGHT: usize = 3;

/// A worm's controls (`Worm::Control`), numbered by their bits in
/// `control_states`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Fire = 4,
    Change = 5,
    Jump = 6,
}

impl Control {
    /// The control's bit in `control_states`.
    pub const fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// `Settings::gameMode`.
pub const LIERO_GAME_KILL_EM_ALL: i32 = 0;
pub const LIERO_GAME_GAME_OF_TAG: i32 = 1;
pub const LIERO_GAME_HOLDAZONE: i32 = 2;
pub const LIERO_GAME_SCALES_OF_JUSTICE: i32 = 3;

/// A worm of a game: its place among every worm the game has made
/// (`Game`'s pool), listed or not. Copies of the game keep the places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WormId(pub u32);

/// A worm's state. Rust owns it, and the fields of C++'s `Worm` refer to
/// it (`WormData::default`), except its settings and AI.
#[derive(Clone, Copy, Debug)]
pub struct WormData {
    pub pos: IVec2,
    pub vel: IVec2,
    /// Where the respawn camera is, in pixels.
    pub logic_respawn: IVec2,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    /// Fixed-point, 0 to 128.
    pub aiming_angle: i32,
    pub aiming_speed: i32,
    pub able_to_jump: bool,
    pub able_to_dig: bool,
    pub key_change_pressed: bool,
    pub movable: bool,
    pub animate: bool,
    pub visible: bool,
    pub ready: bool,
    /// Whether the worm has a flag.
    pub flag: bool,
    pub make_sight_green: bool,
    pub health: i32,
    pub lives: i32,
    pub kills: i32,
    /// Time as 'it' in Game of Tag.
    pub timer: i32,
    pub killed_timer: i32,
    pub current_frame: i32,
    pub flags: i32,
    pub ninjarope: Ninjarope,
    pub current_weapon: i32,
    pub last_killed_by_idx: i32,
    pub fire_cone: i32,
    pub leave_shell_timer: i32,
    /// 0 or 1.
    pub index: i32,
    /// Indexed by `RF_*`.
    pub reacts: [i32; 4],
    pub weapons: [WormWeapon; SELECTABLE_WEAPONS],
    /// 0 is left, 1 is right.
    pub direction: i32,
    /// One bit per `Control`: the controls the game sees, those of
    /// the last tick, and the ones the player holds (`LocalController`).
    pub control_states: u32,
    pub prev_control_states: u32,
    pub clean_control_states: u32,
    pub steerable_sum_x: i32,
    pub steerable_sum_y: i32,
    pub steerable_count: i32,
    /// Where the worm's stats are drawn.
    pub stats_x: i32,
}

impl WormData {
    /// `Worm::pressed`.
    pub fn pressed(&self, c: Control) -> bool {
        self.control_states & c.bit() != 0
    }

    /// `Worm::release`.
    pub fn release(&mut self, c: Control) {
        self.control_states &= !c.bit();
    }

    /// `Worm::pressedOnce`: whether it was pressed, then clears it.
    pub fn pressed_once(&mut self, c: Control) -> bool {
        let p = self.pressed(c);
        self.release(c);
        p
    }
}

impl Default for WormData {
    /// What C++'s `Worm()` set.
    fn default() -> Self {
        let zero = IVec2 { x: 0, y: 0 };
        WormData {
            pos: zero,
            vel: zero,
            logic_respawn: zero,
            hotspot_x: 0,
            hotspot_y: 0,
            aiming_angle: 0,
            aiming_speed: 0,
            able_to_jump: false,
            able_to_dig: false,
            key_change_pressed: false,
            movable: true,
            animate: false,
            visible: false,
            ready: true,
            flag: false,
            make_sight_green: false,
            health: 0,
            lives: 0,
            kills: 0,
            timer: 0,
            killed_timer: 150,
            current_frame: 0,
            flags: 0,
            ninjarope: Ninjarope {
                out: false,
                attached: false,
                anchor: None,
                pos: zero,
                vel: zero,
                length: 0,
                cur_len: 0,
            },
            current_weapon: 0,
            last_killed_by_idx: -1,
            fire_cone: 0,
            leave_shell_timer: 0,
            index: 0,
            reacts: [0; 4],
            weapons: [WormWeapon {
                kind: -1,
                ammo: 0,
                delay_left: 0,
                loading_left: 0,
            }; SELECTABLE_WEAPONS],
            direction: 0,
            control_states: 0,
            prev_control_states: 0,
            clean_control_states: 0,
            steerable_sum_x: 0,
            steerable_sum_y: 0,
            steerable_count: 0,
            stats_x: 0,
        }
    }
}

/// Constants for worms from the TC (`LC(...)` and `H[...]` in C++).
#[derive(Clone, Copy, Debug, Default)]
pub struct WormConstants {
    pub min_bounce_up: i32,
    pub min_bounce_down: i32,
    pub min_bounce_left: i32,
    pub min_bounce_right: i32,
    pub worm_gravity: i32,
    pub walk_vel_left: i32,
    pub max_vel_left: i32,
    pub walk_vel_right: i32,
    pub max_vel_right: i32,
    pub jump_force: i32,
    pub max_aim_vel_left: i32,
    pub aim_acc_left: i32,
    pub max_aim_vel_right: i32,
    pub aim_acc_right: i32,
    pub nr_initial_length: i32,
    pub nr_min_length: i32,
    pub nr_max_length: i32,
    pub nr_throw_vel_x: i32,
    pub nr_throw_vel_y: i32,
    pub nr_pull_vel: i32,
    pub nr_release_vel: i32,
    pub worm_fric_mult: i32,
    pub worm_fric_div: i32,
    pub worm_min_spawn_dist_last: i32,
    pub worm_min_spawn_dist_enemy: i32,
    pub worm_spawn_rect_x: i32,
    pub worm_spawn_rect_y: i32,
    pub worm_spawn_rect_w: i32,
    pub worm_spawn_rect_h: i32,
    pub aim_fric_mult: i32,
    pub aim_fric_div: i32,
    pub aim_max_right: i32,
    pub aim_min_right: i32,
    pub aim_max_left: i32,
    pub aim_min_left: i32,
    pub bonus_explode_risk: i32,
    pub bonus_health_var: i32,
    pub bonus_min_health: i32,
    /// 1-based id of the weapon that always has a laser sight.
    pub laser_weapon: i32,
    pub fall_damage_right: i32,
    pub fall_damage_down: i32,
    pub worm_float_level: i32,
    pub worm_float_power: i32,
    /// The TC's hacks (`H[...]`).
    pub fall_damage: bool,
    pub worm_float: bool,
    pub bonus_reload_only: bool,
    pub air_jump: bool,
    pub multi_jump: bool,
    pub signed_recoil: bool,
}

/// Everything else `Worm::process` reads from the game.
pub struct WormEnv {
    pub cycles: i32,
    /// `LIERO_GAME_*`.
    pub game_mode: i32,
    /// Blood amount, in percent.
    pub blood: i32,
    /// Weapon loading time, in percent.
    pub loading_time: i32,
    /// Whether weapons can be changed while loading.
    pub load_change: bool,
    pub shadow: bool,
    /// Worms don't respawn in quick simulations.
    pub quick_sim: bool,
    /// `Settings::wormAnimTab`: frame offsets for the walking animation.
    pub anim_tab: [i32; 4],
}

/// The sprite frame for a worm aiming at `aiming_angle` (fixed-point) in
/// `direction`, without the walking animation: `Worm::angleFrame`.
pub fn angle_frame(aiming_angle: i32, direction: i32) -> i32 {
    let mut x = ftoi(aiming_angle) - 12;

    if direction != 0 {
        x -= 49;
    }

    x >>= 3;
    x = x.clamp(0, 6);

    if direction != 0 {
        x = 6 - x;
    }

    x
}

/// Clamps (x, y) to (0, 0) - (max_x, max_y): `limitXY`.
pub(crate) fn limit_xy(p: &mut IVec2, max_x: i32, max_y: i32) {
    if p.x < 0 {
        p.x = 0;
    } else if p.x > max_x {
        p.x = max_x;
    }

    if p.y < 0 {
        p.y = 0;
    }
    if p.y > max_y {
        p.y = max_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::itof;

    #[test]
    fn angle_frame_matches_cpp() {
        // Worm::angleFrame, written out.
        fn cpp(aiming_angle: i32, direction: i32) -> i32 {
            let mut x = ftoi(aiming_angle) - 12;
            if direction != 0 {
                x -= 49;
            }
            x >>= 3;
            x = x.clamp(0, 6);
            if direction != 0 {
                x = 6 - x;
            }
            x
        }
        for angle in -10..140 {
            for dir in 0..2 {
                assert_eq!(angle_frame(itof(angle), dir), cpp(itof(angle), dir));
            }
        }
        assert_eq!(angle_frame(itof(32), 0), 2);
        assert_eq!(angle_frame(itof(96), 1), 2);
    }

    #[test]
    fn new_worm_matches_cpp_and_copies_are_separate() {
        {
            let mut w = Box::<WormData>::default();
            // What C++'s Worm() set.
            assert!(w.ready && w.movable && !w.visible);
            assert_eq!(w.killed_timer, 150);
            assert_eq!(w.last_killed_by_idx, -1);
            assert_eq!(w.reacts, [0; 4]);
            assert!(w.ninjarope.anchor.is_none() && !w.ninjarope.out);

            w.health = 75;
            w.kills = 3;
            w.flags = 2;
            w.weapons[2].ammo = 9;
            let mut c = w.clone();
            // Every field is copied.
            assert_eq!(format!("{:?}", c), format!("{:?}", w));
            c.health = 1;
            c.weapons[2].ammo = 0;
            assert_eq!(w.health, 75);
            assert_eq!(w.weapons[2].ammo, 9);
            assert_eq!(c.health, 1);
        }
    }

    #[test]
    fn limit_xy_clamps() {
        let mut p = IVec2 { x: -3, y: 500 };
        limit_xy(&mut p, 100, 200);
        assert_eq!((p.x, p.y), (0, 200));
        let mut p = IVec2 { x: 150, y: -1 };
        limit_xy(&mut p, 100, 200);
        assert_eq!((p.x, p.y), (100, 0));
    }
}
