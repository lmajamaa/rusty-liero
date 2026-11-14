//! Materials: what each of the 256 palette indices means to the game (dirt,
//! rock, background...), from `src/game/material.hpp`. Each TC defines one
//! material per palette index; the level stores the material of every pixel
//! next to its colour.

pub const LIERO_MATERIAL_DIRT: u8 = 1 << 0;
pub const LIERO_MATERIAL_DIRT2: u8 = 1 << 1;
pub const LIERO_MATERIAL_ROCK: u8 = 1 << 2;
pub const LIERO_MATERIAL_BACKGROUND: u8 = 1 << 3;
pub const LIERO_MATERIAL_SEE_SHADOW: u8 = 1 << 4;
pub const LIERO_MATERIAL_WORM: u8 = 1 << 5;

/// Material flags of one palette index or level pixel. C++'s `Material`
/// inherits from this struct and keeps its tests inline, because the
/// renderer and AI call them for every pixel.
// `repr(C)`: the TC's material bytes are read as these.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Material {
    pub flags: u8,
}

impl Material {
    fn any(self, mask: u8) -> bool {
        self.flags & mask != 0
    }

    pub fn dirt(self) -> bool {
        self.any(LIERO_MATERIAL_DIRT)
    }

    pub fn dirt2(self) -> bool {
        self.any(LIERO_MATERIAL_DIRT2)
    }

    pub fn rock(self) -> bool {
        self.any(LIERO_MATERIAL_ROCK)
    }

    pub fn background(self) -> bool {
        self.any(LIERO_MATERIAL_BACKGROUND)
    }

    pub fn see_shadow(self) -> bool {
        self.any(LIERO_MATERIAL_SEE_SHADOW)
    }

    pub fn dirt_rock(self) -> bool {
        self.any(LIERO_MATERIAL_DIRT | LIERO_MATERIAL_DIRT2 | LIERO_MATERIAL_ROCK)
    }

    pub fn any_dirt(self) -> bool {
        self.any(LIERO_MATERIAL_DIRT | LIERO_MATERIAL_DIRT2)
    }

    pub fn dirt_back(self) -> bool {
        self.any(LIERO_MATERIAL_DIRT | LIERO_MATERIAL_DIRT2 | LIERO_MATERIAL_BACKGROUND)
    }

    pub fn worm(self) -> bool {
        self.any(LIERO_MATERIAL_WORM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_match_flags() {
        let m = |flags| Material { flags };
        assert!(m(LIERO_MATERIAL_DIRT).dirt() && m(LIERO_MATERIAL_DIRT).any_dirt());
        assert!(m(LIERO_MATERIAL_DIRT2).dirt2() && !m(LIERO_MATERIAL_DIRT2).dirt());
        assert!(m(LIERO_MATERIAL_ROCK).dirt_rock() && !m(LIERO_MATERIAL_ROCK).dirt_back());
        assert!(m(LIERO_MATERIAL_BACKGROUND).dirt_back() && !m(LIERO_MATERIAL_BACKGROUND).dirt_rock());
        assert!(m(LIERO_MATERIAL_SEE_SHADOW).see_shadow());
        assert!(m(LIERO_MATERIAL_WORM).worm() && !m(LIERO_MATERIAL_WORM).dirt_rock());
        assert!(!m(0).dirt_rock() && !m(0).dirt_back() && !m(0).worm());
    }
}
