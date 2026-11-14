use super::*;

const NEVER: [[i32; 7]; 2] = [[1 << 30; 7]; 2];

fn worm(x: i32, y: i32) -> DumbAiWorm {
    DumbAiWorm {
        pos: [x << 16, y << 16],
        visible: true,
        aiming_angle: 32 << 16,
        direction: 1,
        speed: 100,
        ..Default::default()
    }
}

#[test]
fn alone_it_changes_nothing() {
    let mut rand = Rand::new(1);
    let w = DumbAiWorm {
        control_states: 0b1010101,
        ..worm(10, 10)
    };
    assert_eq!(process(&mut rand, &w, &[], &NEVER), 0b1010101);
    assert_eq!(rand, Rand::new(1));
}

#[test]
fn it_goes_for_the_nearest_worm() {
    let w = worm(100, 100);
    let near = [300 << 16, 100 << 16];
    let far = [100 << 16, 400 << 16];
    let mut a = Rand::new(5);
    let mut b = Rand::new(5);
    let both = process(&mut a, &w, &[far, near], &NEVER);
    assert_eq!(both, process(&mut b, &w, &[near], &NEVER));
    assert_eq!(a, b);
    // Far away, it walks to the right and doesn't fire.
    assert!(Controls(both).pressed(RIGHT) && !Controls(both).pressed(LEFT));
    assert!(!Controls(both).pressed(FIRE));
}

#[test]
fn it_fires_when_close_and_stops_when_far() {
    // A chance of one in one: every roll toggles.
    let mut always = NEVER;
    always[0][FIRE as usize] = 1;
    let mut rand = Rand::new(9);
    let cs = process(&mut rand, &worm(100, 100), &[[150 << 16, 100 << 16]], &always);
    assert!(Controls(cs).pressed(FIRE));

    let firing = DumbAiWorm {
        control_states: 1 << FIRE,
        ..worm(100, 100)
    };
    let cs = process(&mut rand, &firing, &[[400 << 16, 100 << 16]], &NEVER);
    assert!(!Controls(cs).pressed(FIRE));
}

#[test]
fn it_jumps_over_walls() {
    // Walking right into a wall on the right with nothing below.
    let mut w = worm(100, 100);
    w.reacts[RF_LEFT] = 1;
    let mut rand = Rand::new(2);
    let cs = process(&mut rand, &w, &[[400 << 16, 100 << 16]], &NEVER);
    assert!(Controls(cs).pressed(RIGHT) && Controls(cs).pressed(JUMP));
}
