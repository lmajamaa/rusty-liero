//! The predictive AI's path finding (`ai/dijkstra.hpp`): Dijkstra's
//! algorithm from the AI's target over the level in 4 x 4 pixel cells, with
//! gvl's pairing heap as the open list.
//!
//! The heap is ported link for link. Cells with the same distance leave the
//! heap in an order that depends on its shape, and that order picks the
//! parent each cell keeps, which the AI follows to aim.

use crate::material::Material;

/// No cell: a null pointer in C++.
pub const NO_CELL: u32 = u32::MAX;

pub const FULL_WIDTH: i32 = 504;
pub const FULL_HEIGHT: i32 = 350;
pub const FACTOR: i32 = 4;
pub const WIDTH: i32 = FULL_WIDTH / FACTOR;
pub const HEIGHT: i32 = FULL_HEIGHT / FACTOR;
/// A border of rock cells goes around the level.
pub const PITCH: i32 = WIDTH + 2;
pub const CELL_COUNT: usize = (PITCH * (HEIGHT + 2)) as usize;

/// `path_node`'s states.
pub const NONE: i32 = 0;
pub const OPEN: i32 = 1;
pub const CLOSED: i32 = 2;

/// Up, down, left, right, then the diagonals.
const OFFSETS: [i32; 8] = [-PITCH, PITCH, -1, 1, -PITCH - 1, -PITCH + 1, PITCH - 1, PITCH + 1];
/// 256 straight and 362 diagonally, about 256 * sqrt(2).
const COSTS: [i32; 8] = [256, 256, 256, 256, 362, 362, 362, 362];

/// A `level_cell`. The heap links are indexes; `prev_next` names the slot
/// that points at the cell: `2 * i` for cell i's left child, `2 * i + 1` for
/// its right sibling.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    left_child: u32,
    prev_next: u32,
    right_sibling: u32,
    pub parent: u32,
    pub state: i32,
    /// The distance, set once a search reaches the cell.
    pub g: i32,
    /// 1 for air, 2 for dirt, -1 for rock and outside the level.
    pub cost: i32,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            left_child: NO_CELL,
            prev_next: NO_CELL,
            right_sibling: NO_CELL,
            parent: NO_CELL,
            state: NONE,
            g: 0,
            cost: 0,
        }
    }
}

/// The level the cells are built from.
pub struct LevelView<'a> {
    pub data: &'a [u8],
    pub width: i32,
    pub height: i32,
    /// The TC's 256 materials.
    pub materials: &'a [Material],
}

impl LevelView<'_> {
    fn inside(&self, x: i32, y: i32) -> bool {
        (x as u32) < (self.width as u32) && (y as u32) < (self.height as u32)
    }

    fn pixel(&self, x: i32, y: i32) -> u8 {
        self.data[(x + y * self.width) as usize]
    }

    /// `Level::checkedPixelWrap`: the pixel at x + y * width if that's in
    /// the level, else 0.
    pub fn checked_pixel_wrap(&self, x: i32, y: i32) -> u8 {
        let idx = x.wrapping_add(y.wrapping_mul(self.width)) as u32;
        self.data.get(idx as usize).copied().unwrap_or(0)
    }

    pub fn material(&self, pixel: u8) -> Material {
        self.materials[pixel as usize]
    }
}

/// `dijkstra_level`.
#[derive(Clone)]
pub struct PathLevel {
    pub cells: Vec<Cell>,
    root: u32,
}

impl Default for PathLevel {
    fn default() -> Self {
        PathLevel {
            cells: vec![Cell::default(); CELL_COUNT],
            root: NO_CELL,
        }
    }
}

impl PathLevel {
    pub fn cell(x: i32, y: i32) -> u32 {
        ((y + 1) * PITCH + x + 1) as u32
    }

    pub fn coords(c: u32) -> (i32, i32) {
        let c = c as i32;
        (c % PITCH - 1, c / PITCH - 1)
    }

    /// The middle of the cell, in pixels.
    pub fn coords_level(c: u32) -> (i32, i32) {
        let (x, y) = Self::coords(c);
        (x * FACTOR + FACTOR / 2, y * FACTOR + FACTOR / 2)
    }

    pub fn cell_from_px(x: i32, y: i32) -> u32 {
        let x = x.clamp(0, FULL_WIDTH - 1);
        let y = y.clamp(0, FULL_HEIGHT - 1);
        Self::cell(x / FACTOR, y / FACTOR)
    }

    /// Sets each cell's cost from the level and clears the search.
    pub fn build(&mut self, level: &LevelView) {
        self.root = NO_CELL;

        for x in 0..PITCH {
            self.cells[x as usize].cost = -1;
            self.cells[((HEIGHT + 1) * PITCH + x) as usize].cost = -1;
        }
        for y in 0..HEIGHT + 2 {
            self.cells[(y * PITCH) as usize].cost = -1;
            self.cells[(y * PITCH + WIDTH + 1) as usize].cost = -1;
        }

        for ly in 0..HEIGHT {
            for lx in 0..WIDTH {
                let mut cost = 1;
                'cell: for cy in ly * FACTOR..(ly + 1) * FACTOR {
                    for cx in lx * FACTOR..(lx + 1) * FACTOR {
                        if !level.inside(cx, cy) {
                            cost = -1;
                            break 'cell;
                        }
                        let m = level.material(level.pixel(cx, cy));
                        if !m.background() {
                            if m.any_dirt() {
                                cost = 2;
                            } else {
                                cost = -1;
                                break 'cell;
                            }
                        }
                    }
                }
                self.cells[Self::cell(lx, ly) as usize].cost = cost;
            }
        }

        for c in &mut self.cells {
            c.state = NONE;
        }
    }

    /// Starts a search from `origin`.
    pub fn set_origin(&mut self, origin: u32) {
        let c = &mut self.cells[origin as usize];
        c.g = 0;
        c.parent = NO_CELL;
        self.add_open(origin);
    }

    /// Runs the search until `stop` is closed or nothing is left open, and
    /// says whether `stop` was reached. Later calls go on from where the
    /// search stopped.
    pub fn run(&mut self, stop: u32) -> bool {
        while self.root != NO_CELL && self.cells[stop as usize].state != CLOSED {
            let min = self.unlink_min();
            self.cells[min as usize].state = CLOSED;
            let min_g = self.cells[min as usize].g;

            for i in 0..8 {
                let n = (min as i32 + OFFSETS[i]) as u32;
                let cell = self.cells[n as usize];
                if cell.cost < 0 || cell.state == CLOSED {
                    continue;
                }
                let g = min_g + COSTS[i] * cell.cost;
                if cell.state != OPEN {
                    let c = &mut self.cells[n as usize];
                    c.g = g;
                    c.parent = min;
                    self.add_open(n);
                } else if g < cell.g {
                    let c = &mut self.cells[n as usize];
                    c.g = g;
                    c.parent = min;
                    self.decreased_key(n);
                }
            }
        }
        self.cells[stop as usize].state == CLOSED
    }

    fn add_open(&mut self, c: u32) {
        self.insert(c);
        self.cells[c as usize].state = OPEN;
    }

    // ---- gvl::pairing_heap ----

    fn set_slot(&mut self, slot: u32, v: u32) {
        let c = &mut self.cells[(slot / 2) as usize];
        if slot.is_multiple_of(2) {
            c.left_child = v;
        } else {
            c.right_sibling = v;
        }
    }

    fn insert(&mut self, el: u32) {
        self.cells[el as usize].left_child = NO_CELL;
        self.root = if self.root == NO_CELL {
            el
        } else {
            self.link(self.root, el)
        };
    }

    fn decreased_key(&mut self, el: u32) {
        if el != self.root {
            self.unlink_subtree(el);
            self.root = self.link(self.root, el);
        }
    }

    fn unlink_min(&mut self) -> u32 {
        let ret = self.root;
        let left_child = self.cells[ret as usize].left_child;
        self.root = if left_child != NO_CELL {
            self.combine_siblings(left_child)
        } else {
            NO_CELL
        };
        ret
    }

    fn unlink_subtree(&mut self, el: u32) {
        let Cell {
            prev_next,
            right_sibling,
            ..
        } = self.cells[el as usize];
        if right_sibling != NO_CELL {
            self.cells[right_sibling as usize].prev_next = prev_next;
        }
        self.set_slot(prev_next, right_sibling);
    }

    /// `comparison_link_`: the cell with the smaller g, `a` unless `b`'s is
    /// smaller, takes the other as its first child.
    fn link(&mut self, a: u32, b: u32) -> u32 {
        let (parent, child) = if self.cells[a as usize].g < self.cells[b as usize].g {
            (a, b)
        } else {
            (b, a)
        };
        let ch = self.cells[parent as usize].left_child;
        let c = &mut self.cells[child as usize];
        c.prev_next = 2 * parent;
        c.right_sibling = ch;
        if ch != NO_CELL {
            self.cells[ch as usize].prev_next = 2 * child + 1;
        }
        self.cells[parent as usize].left_child = child;
        parent
    }

    fn right(&self, c: u32) -> u32 {
        self.cells[c as usize].right_sibling
    }

    /// `combine_siblings_`: pairs the siblings from the left, then links the
    /// pairs from the right.
    fn combine_siblings(&mut self, el: u32) -> u32 {
        let first = el;
        let second = self.right(first);
        if second == NO_CELL {
            return first;
        }
        let mut next = self.right(second);
        let mut stack = self.link(first, second);
        if next == NO_CELL {
            return stack;
        }
        self.cells[stack as usize].right_sibling = NO_CELL;

        loop {
            let first = next;
            let second = self.right(first);
            if second == NO_CELL {
                self.cells[first as usize].right_sibling = stack;
                stack = first;
                break;
            }
            next = self.right(second);
            let tree = self.link(first, second);
            self.cells[tree as usize].right_sibling = stack;
            stack = tree;
            if next == NO_CELL {
                break;
            }
        }

        let mut first = stack;
        let mut second = self.right(stack);
        loop {
            let next = self.right(second);
            first = self.link(first, second);
            second = next;
            if second == NO_CELL {
                break;
            }
        }
        first
    }
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
