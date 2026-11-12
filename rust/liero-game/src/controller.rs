//! The state the game's controllers share (`CommonController` and the
//! local and replay controllers in `src/game/controller`): which part of a
//! match is running, the fade to and from the menu, and the frame skip.
//! Rust owns it, and the fields of C++'s `CommonController` refer to it.

use crate::keys::LieroKeys;

/// `GameState`: before the match, choosing weapons, playing, and the match
/// over.
pub const LIERO_STATE_INITIAL: i32 = 0;
pub const LIERO_STATE_WEAPON_SELECTION: i32 = 1;
pub const LIERO_STATE_GAME: i32 = 2;
pub const LIERO_STATE_GAME_ENDED: i32 = 3;

/// The fade value when the game is fully shown.
pub const LIERO_FADE_SHOWN: i32 = 33;

/// A controller's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LieroControllerData {
    /// `LIERO_STATE_*`.
    pub state: i32,
    /// 0 is black, `LIERO_FADE_SHOWN` fully shown.
    pub fade_value: i32,
    /// Fading out to the menu.
    pub going_to_menu: bool,
    /// Game ticks per frame, or with `inverse_frame_skip`, frames per tick.
    pub frame_skip: i32,
    pub inverse_frame_skip: bool,
    /// Frames the controller has run.
    pub cycles: i32,
}

impl Default for LieroControllerData {
    /// What C++'s controller constructors set.
    fn default() -> Self {
        LieroControllerData {
            state: LIERO_STATE_INITIAL,
            fade_value: 0,
            going_to_menu: false,
            frame_skip: 1,
            inverse_frame_skip: false,
            cycles: 0,
        }
    }
}

impl LieroControllerData {
    /// How many game ticks this frame runs: `frame_skip`, or with
    /// `inverse_frame_skip`, one every `frame_skip` frames.
    pub fn ticks(&self) -> i32 {
        if self.inverse_frame_skip {
            (self.cycles % self.frame_skip == 0) as i32
        } else {
            self.frame_skip
        }
    }

    /// Whether a match is being played or is over but still shown.
    pub fn in_game(&self) -> bool {
        self.state == LIERO_STATE_GAME || self.state == LIERO_STATE_GAME_ENDED
    }
}

pub fn liero_controller_data_new() -> *mut LieroControllerData {
    Box::into_raw(Box::default())
}

/// # Safety
/// `c` comes from `liero_controller_data_new`, and isn't used after.
pub unsafe fn liero_controller_data_free(c: *mut LieroControllerData) {
    unsafe {
        if !c.is_null() {
            drop(Box::from_raw(c));
        }
    }
}

/// `CommonController::process`: 1 to 0 choose the frame skip, with ctrl
/// inverted, and the frame is counted.
pub fn liero_controller_process(c: &mut LieroControllerData, keys: &mut LieroKeys) {
    keys.frame_skip(&mut c.frame_skip, &mut c.inverse_frame_skip);
    c.cycles += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_controller_matches_cpp() {
        unsafe {
            let c = liero_controller_data_new();
            assert_eq!(
                *c,
                LieroControllerData {
                    state: LIERO_STATE_INITIAL,
                    fade_value: 0,
                    going_to_menu: false,
                    frame_skip: 1,
                    inverse_frame_skip: false,
                    cycles: 0,
                }
            );
            liero_controller_data_free(c);
        }
    }

    #[test]
    fn ticks_follow_the_frame_skip() {
        let mut c = LieroControllerData {
            frame_skip: 3,
            ..Default::default()
        };
        assert_eq!(
            (0..6)
                .map(|i| LieroControllerData { cycles: i, ..c }.ticks())
                .collect::<Vec<_>>(),
            [3; 6]
        );
        c.inverse_frame_skip = true;
        // C++: !(cycles % frameSkip).
        assert_eq!(
            (0..6)
                .map(|i| LieroControllerData { cycles: i, ..c }.ticks())
                .collect::<Vec<_>>(),
            [1, 0, 0, 1, 0, 0]
        );
    }

    #[test]
    fn in_game_states() {
        let at = |state| {
            LieroControllerData {
                state,
                ..Default::default()
            }
            .in_game()
        };
        assert!(!at(LIERO_STATE_INITIAL) && !at(LIERO_STATE_WEAPON_SELECTION));
        assert!(at(LIERO_STATE_GAME) && at(LIERO_STATE_GAME_ENDED));
    }
}
