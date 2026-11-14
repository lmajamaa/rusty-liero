//! The state the game's controllers share (`CommonController` and the
//! local and replay controllers in `src/game/controller`): which part of a
//! match is running, the fade to and from the menu, and the frame skip.
//! Rust owns it, and the fields of C++'s `CommonController` refer to it.

use crate::keys::Keys;

/// `GameState`: before the match, choosing weapons, playing, and the match
/// over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Initial,
    WeaponSelection,
    Game,
    GameEnded,
}

/// The fade value when the game is fully shown.
pub const FADE_SHOWN: i32 = 33;

/// A controller's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerData {
    pub state: State,
    /// 0 is black, `FADE_SHOWN` fully shown.
    pub fade_value: i32,
    /// Fading out to the menu.
    pub going_to_menu: bool,
    /// Game ticks per frame, or with `inverse_frame_skip`, frames per tick.
    pub frame_skip: i32,
    pub inverse_frame_skip: bool,
    /// Frames the controller has run.
    pub cycles: i32,
}

impl Default for ControllerData {
    /// What C++'s controller constructors set.
    fn default() -> Self {
        ControllerData {
            state: State::Initial,
            fade_value: 0,
            going_to_menu: false,
            frame_skip: 1,
            inverse_frame_skip: false,
            cycles: 0,
        }
    }
}

impl ControllerData {
    /// How many game ticks this frame runs: `frame_skip`, or with
    /// `inverse_frame_skip`, one every `frame_skip` frames.
    pub fn ticks(&self) -> i32 {
        if self.inverse_frame_skip {
            (self.cycles % self.frame_skip == 0) as i32
        } else {
            self.frame_skip
        }
    }

    /// `CommonController::process`: 1 to 0 choose the frame skip, with ctrl
    /// inverted, and the frame is counted.
    pub fn process(&mut self, keys: &mut Keys) {
        keys.frame_skip(&mut self.frame_skip, &mut self.inverse_frame_skip);
        self.cycles += 1;
    }

    /// Whether a match is being played or is over but still shown.
    pub fn in_game(&self) -> bool {
        self.state == State::Game || self.state == State::GameEnded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_controller_matches_cpp() {
        {
            let c = ControllerData::default();
            assert_eq!(
                c,
                ControllerData {
                    state: State::Initial,
                    fade_value: 0,
                    going_to_menu: false,
                    frame_skip: 1,
                    inverse_frame_skip: false,
                    cycles: 0,
                }
            );
        }
    }

    #[test]
    fn ticks_follow_the_frame_skip() {
        let mut c = ControllerData {
            frame_skip: 3,
            ..Default::default()
        };
        assert_eq!(
            (0..6)
                .map(|i| ControllerData { cycles: i, ..c }.ticks())
                .collect::<Vec<_>>(),
            [3; 6]
        );
        c.inverse_frame_skip = true;
        // C++: !(cycles % frameSkip).
        assert_eq!(
            (0..6)
                .map(|i| ControllerData { cycles: i, ..c }.ticks())
                .collect::<Vec<_>>(),
            [1, 0, 0, 1, 0, 0]
        );
    }

    #[test]
    fn in_game_states() {
        let at = |state| {
            ControllerData {
                state,
                ..Default::default()
            }
            .in_game()
        };
        assert!(!at(State::Initial) && !at(State::WeaponSelection));
        assert!(at(State::Game) && at(State::GameEnded));
    }
}
