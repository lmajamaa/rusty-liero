use super::*;
use crate::material::{LIERO_MATERIAL_BACKGROUND, LIERO_MATERIAL_DIRT, LIERO_MATERIAL_ROCK};
use crate::rand::LieroRand;

const AIR: u8 = 0;
const DIRT: u8 = 1;
const ROCK: u8 = 2;

fn materials() -> Vec<LieroMaterial> {
    let mut m = vec![LieroMaterial { flags: LIERO_MATERIAL_BACKGROUND }; 256];
    m[DIRT as usize].flags = LIERO_MATERIAL_DIRT;
    m[ROCK as usize].flags = LIERO_MATERIAL_ROCK;
    m
}

fn level(paint: impl Fn(i32, i32) -> u8) -> Vec<u8> {
    let mut data = vec![AIR; (FULL_WIDTH * FULL_HEIGHT) as usize];
    for y in 0..FULL_HEIGHT {
        for x in 0..FULL_WIDTH {
            data[(x + y * FULL_WIDTH) as usize] = paint(x, y);
        }
    }
    data
}

fn built(data: &[u8]) -> PathLevel {
    let m = materials();
    let mut l = PathLevel::default();
    l.build(&LevelView { data, width: FULL_WIDTH, height: FULL_HEIGHT, materials: &m });
    l
}

#[test]
fn cells_and_coords_round_trip() {
    for (x, y) in [(0, 0), (WIDTH - 1, 0), (0, HEIGHT - 1), (WIDTH - 1, HEIGHT - 1), (17, 33)] {
        assert_eq!(PathLevel::coords(PathLevel::cell(x, y)), (x, y));
    }
    assert_eq!(PathLevel::coords_level(PathLevel::cell(2, 3)), (10, 14));
    // Pixels outside the level clamp to its edge. The bottom two pixel rows
    // are below the last whole cell, so they land in the border, as in C++.
    assert_eq!(PathLevel::cell_from_px(-50, 347), PathLevel::cell(0, HEIGHT - 1));
    assert_eq!(PathLevel::cell_from_px(-50, 9999), PathLevel::cell(0, HEIGHT));
}

#[test]
fn build_costs_air_dirt_and_rock() {
    // One dirt pixel makes a cell dirt; one rock pixel makes it rock.
    let data = level(|x, y| match (x, y) {
        (5, 5) => DIRT,
        (9, 1) => ROCK,
        (13, 2) => DIRT,
        (14, 3) => ROCK,
        _ => AIR,
    });
    let l = built(&data);
    assert_eq!(l.cells[PathLevel::cell(0, 0) as usize].cost, 1);
    assert_eq!(l.cells[PathLevel::cell(1, 1) as usize].cost, 2);
    assert_eq!(l.cells[PathLevel::cell(2, 0) as usize].cost, -1);
    assert_eq!(l.cells[PathLevel::cell(3, 0) as usize].cost, -1);
    // The border is rock.
    assert_eq!(l.cells[0].cost, -1);
    assert_eq!(l.cells[CELL_COUNT - 1].cost, -1);
    // Cells past a smaller level's edge are rock.
    let data = vec![AIR; (FULL_WIDTH * 341) as usize];
    let m = materials();
    let mut l = PathLevel::default();
    l.build(&LevelView { data: &data, width: FULL_WIDTH, height: 341, materials: &m });
    assert_eq!(l.cells[PathLevel::cell(0, 84) as usize].cost, 1);
    assert_eq!(l.cells[PathLevel::cell(0, 85) as usize].cost, -1);
}

#[test]
fn open_level_distances_are_octile() {
    let data = level(|_, _| AIR);
    let mut l = built(&data);
    let origin = PathLevel::cell(60, 40);
    l.set_origin(origin);
    for (x, y) in [(60, 40), (61, 40), (70, 40), (65, 45), (0, 0), (WIDTH - 1, HEIGHT - 1), (63, 50)] {
        let c = PathLevel::cell(x, y);
        assert!(l.run(c));
        let (dx, dy) = ((x - 60).abs(), (y - 40).abs());
        let expected = 362 * dx.min(dy) + 256 * (dx.max(dy) - dx.min(dy));
        assert_eq!(l.cells[c as usize].g, expected, "({x}, {y})");
    }
    // Every closed cell's parent is a neighbour one step nearer.
    for (i, c) in l.cells.iter().enumerate() {
        if c.state == CLOSED && c.parent != NO_CELL {
            let p = l.cells[c.parent as usize];
            let step = c.g - p.g;
            assert!(step == 256 || step == 362, "cell {i}: step {step}");
        }
    }
}

#[test]
fn walls_block() {
    // A rock wall at x = 200..208 with a gap at the bottom.
    let data = level(|x, y| if (200..208).contains(&x) && y < 300 { ROCK } else { AIR });
    let mut l = built(&data);
    let origin = PathLevel::cell(10, 10);
    l.set_origin(origin);
    let behind = PathLevel::cell(60, 10);
    assert!(l.run(behind));
    // Around the wall: down to the gap and back up, far more than straight.
    assert!(l.cells[behind as usize].g > 256 * 2 * 65);
    let mut c = behind;
    while l.cells[c as usize].parent != NO_CELL {
        assert_ne!(l.cells[c as usize].cost, -1);
        c = l.cells[c as usize].parent;
    }
    assert_eq!(c, origin);
}

#[test]
fn dirt_costs_double() {
    // A dirt column one cell wide at cell x = 12.
    let data = level(|x, _| if (48..52).contains(&x) { DIRT } else { AIR });
    let mut l = built(&data);
    l.set_origin(PathLevel::cell(10, 10));
    let target = PathLevel::cell(14, 10);
    assert!(l.run(target));
    // Four steps, one of them into dirt.
    assert_eq!(l.cells[target as usize].g, 256 * 3 + 256 * 2);
}

#[test]
fn unreachable_cells_fail() {
    // A rock box around (100, 100) in cells.
    let data = level(|x, y| {
        let (cx, cy) = (x / FACTOR, y / FACTOR);
        if (95..=105).contains(&cx) && (35..=45).contains(&cy) && !((96..=104).contains(&cx) && (36..=44).contains(&cy)) {
            ROCK
        } else {
            AIR
        }
    });
    let mut l = built(&data);
    l.set_origin(PathLevel::cell(10, 10));
    assert!(!l.run(PathLevel::cell(100, 40)));
    // Once the search has run out, cells it reached still count.
    assert!(l.run(PathLevel::cell(120, 80)));
}

/// The pairing heap, pushed and decreased at random, gives cells back in
/// order of g.
#[test]
fn heap_pops_in_order() {
    let mut rand = LieroRand::new(7);
    for round in 0..50 {
        let mut l = PathLevel::default();
        let n = 1 + rand.below(300) as usize;
        let mut open = Vec::new();
        for i in 0..n {
            let c = (PITCH as usize + 1 + i) as u32;
            l.cells[c as usize].g = rand.below(1000) as i32;
            l.add_open(c);
            open.push(c);
            // Now and then decrease a random open cell's key, and pop one.
            if rand.below(3) == 0 {
                let d = open[rand.below(open.len() as u32) as usize];
                let g = &mut l.cells[d as usize].g;
                *g -= rand.below(*g as u32 + 1) as i32;
                l.decreased_key(d);
            }
            if rand.below(4) == 0 {
                let min = l.unlink_min();
                let g = l.cells[min as usize].g;
                assert!(open.iter().all(|&o| l.cells[o as usize].g >= g), "round {round}");
                open.retain(|&o| o != min);
            }
        }
        let mut last = i32::MIN;
        while l.root != NO_CELL {
            let min = l.unlink_min();
            let g = l.cells[min as usize].g;
            assert!(g >= last, "round {round}");
            last = g;
            assert!(open.contains(&min));
            open.retain(|&o| o != min);
        }
        assert!(open.is_empty(), "round {round}");
    }
}

/// Ties go to the newer cell: gvl's link keeps the old root only if it's
/// strictly smaller. 21 ties with 13 under 20, so it sinks below each of
/// 13, 12, 11 and 10 in turn. Which tied cell closes first picks the parents the AI
/// follows.
#[test]
fn heap_ties_go_to_the_newer_cell() {
    let mut l = PathLevel::default();
    for c in [10, 11, 12, 13] {
        l.cells[c].g = 5;
        l.add_open(c as u32);
    }
    l.cells[20].g = 4;
    l.add_open(20);
    l.cells[21].g = 5;
    l.add_open(21);
    let order: Vec<u32> = std::iter::from_fn(|| (l.root != NO_CELL).then(|| l.unlink_min())).collect();
    assert_eq!(order, [20, 13, 12, 11, 10, 21]);
}
