//! The level: random level generation, shadows, spawn selection and the
//! terrain operations that change the level during a game (stones, dirt
//! effects, images drawn on the map). Ported from `src/game/level.cpp` and
//! the level parts of `src/game/gfx/blit.cpp`. Results have to stay
//! bit-identical to the C++ versions: the same seed must give the same
//! level, and replays depend on every pixel.
//!
//! Rust owns the level (`LieroLevelBox`): its pixels and materials, palette
//! and where it came from. C++'s `Level` refers to its fields, and hands
//! Rust the [`LieroLevel`] view of the pixels for each call, together with a
//! [`LieroLevelCommon`] view of the TC's tables in `Common`.

use crate::gfx::LieroColor;
use crate::material::LieroMaterial;
use crate::text::LieroStr;
use crate::math::LieroIVec2;
use crate::rand::LieroRand;

/// Width and height of the large sprites (stones, dirt textures...).
pub const LIERO_LARGE_SPRITE_SIZE: i32 = 16;
/// Number of dirt effects (`Common::textures`).
pub const LIERO_TEXTURE_COUNT: usize = 9;
/// Width and height of generated and loaded `.LEV` levels.
pub const LIERO_LEVEL_WIDTH: i32 = 504;
pub const LIERO_LEVEL_HEIGHT: i32 = 350;

const SPRITE_PIXELS: usize = (LIERO_LARGE_SPRITE_SIZE * LIERO_LARGE_SPRITE_SIZE) as usize;

/// Large sprites making up the three big stones, as top-left, top-right,
/// bottom-left and bottom-right quarters. Was `stoneTab` in `common.cpp`.
const STONE_TAB: [[i32; 4]; 3] = [[98, 60, 61, 62], [63, 75, 85, 86], [89, 90, 97, 96]];

/// A dirt effect: how explosions and digging change the terrain. C++ uses it
/// as `Texture` in `Common::textures`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroTexture {
    /// Draws dirt onto background instead of removing dirt.
    pub n_draw_back: bool,
    /// Large sprite used as the mask.
    pub m_frame: i32,
    /// First large sprite used as the dirt texture...
    pub s_frame: i32,
    /// ...of this many, picked at random.
    pub r_frame: i32,
}

/// View of a C++ `Level`'s pixels: `data` holds the palette index of each
/// pixel and `materials` its material, both `width * height` long, row by
/// row.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroLevel {
    pub data: *mut u8,
    pub materials: *mut LieroMaterial,
    pub width: i32,
    pub height: i32,
}

/// View of the TC's tables in C++'s `Common` that level code reads.
#[repr(C)]
#[derive(Debug)]
pub struct LieroLevelCommon {
    /// 256 materials, one per palette index.
    pub materials: *const LieroMaterial,
    /// `LIERO_TEXTURE_COUNT` dirt effects.
    pub textures: *const LieroTexture,
    /// `large_sprite_count` sprites of 16 x 16 palette indices.
    pub large_sprites: *const u8,
    pub large_sprite_count: i32,
}

/// The level as Rust works on it.
pub struct Level<'a> {
    pub data: &'a mut [u8],
    pub materials: &'a mut [LieroMaterial],
    pub width: i32,
    pub height: i32,
}

/// The TC's tables as Rust works on them.
pub struct Common<'a> {
    pub materials: &'a [LieroMaterial; 256],
    pub textures: &'a [LieroTexture; LIERO_TEXTURE_COUNT],
    pub large_sprites: &'a [u8],
}

impl Common<'_> {
    /// One 16 x 16 large sprite. Panics (and so aborts) on a frame the TC
    /// doesn't have; C++ read past the sprites there.
    pub fn large_sprite(&self, frame: i32) -> &[u8] {
        let start = frame as usize * SPRITE_PIXELS;
        &self.large_sprites[start..start + SPRITE_PIXELS]
    }
}

/// Rectangle from (x1, y1) to (x2, y2), x2 and y2 excluded, like `gvl::rect`.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    fn intersect(self, b: Rect) -> Rect {
        Rect {
            x1: self.x1.max(b.x1),
            y1: self.y1.max(b.y1),
            x2: self.x2.min(b.x2),
            y2: self.y2.min(b.y2),
        }
    }
}

/// An image of `width` x `height` drawn at (x, y), clipped to `clip` as the
/// `CLIP_IMAGE` macro did. `offset` is where the visible part starts in the
/// image.
struct Clipped {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    offset: usize,
}

fn clip_image(mut x: i32, mut y: i32, mut width: i32, mut height: i32, clip: Rect) -> Option<Clipped> {
    let pitch = width;
    let mut offset = 0i32;

    let top = y - clip.y1;
    if top < 0 {
        offset += -top * pitch;
        height += top;
        y = clip.y1;
    }
    let bottom = y + height - clip.y2;
    if bottom > 0 {
        height -= bottom;
    }
    let left = x - clip.x1;
    if left < 0 {
        offset -= left;
        width += left;
        x = clip.x1;
    }
    let right = x + width - clip.x2;
    if right > 0 {
        width -= right;
    }
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(Clipped { x, y, width, height, offset: offset as usize })
}

impl Level<'_> {
    fn idx(&self, x: i32, y: i32) -> usize {
        (x + y * self.width) as usize
    }

    pub fn pixel(&self, x: i32, y: i32) -> u8 {
        self.data[self.idx(x, y)]
    }

    pub fn mat(&self, x: i32, y: i32) -> LieroMaterial {
        self.materials[self.idx(x, y)]
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, w: u8, common: &Common) {
        let i = self.idx(x, y);
        self.set_index(i, w, common);
    }

    fn set_index(&mut self, i: usize, w: u8, common: &Common) {
        self.data[i] = w;
        self.materials[i] = common.materials[w as usize];
    }

    /// The pixel at (x, y), or 0 when x + y * width falls outside the level.
    pub fn checked_pixel_wrap(&self, x: i32, y: i32) -> u8 {
        let idx = x.wrapping_add(y.wrapping_mul(self.width)) as u32 as usize;
        self.data.get(idx).copied().unwrap_or(0)
    }

    fn rect(&self) -> Rect {
        Rect { x1: 0, y1: 0, x2: self.width, y2: self.height }
    }

    /// Sets every pixel's material from its colour, as after loading a level.
    pub fn update_materials(&mut self, common: &Common) {
        for (m, &d) in self.materials.iter_mut().zip(self.data.iter()) {
            *m = common.materials[d as usize];
        }
    }

    /// The dirt pattern every random level starts from: dirt colours that
    /// blend into their neighbours, with some dirt textures and stones on top.
    pub fn generate_dirt_pattern(&mut self, common: &Common, rand: &mut LieroRand) {
        let (width, height) = (self.width, self.height);

        self.set_pixel(0, 0, (rand.below(7) + 12) as u8, common);

        for y in 1..height {
            let v = ((rand.below(7) + 12) + self.pixel(0, y - 1) as u32) >> 1;
            self.set_pixel(0, y, v as u8, common);
        }

        for x in 1..width {
            let v = ((rand.below(7) + 12) + self.pixel(x - 1, 0) as u32) >> 1;
            self.set_pixel(x, 0, v as u8, common);
        }

        for y in 1..height {
            for x in 1..width {
                let v = (self.pixel(x - 1, y) as u32 + self.pixel(x, y - 1) as u32 + rand.below(8) + 12) / 3;
                self.set_pixel(x, y, v as u8, common);
            }
        }

        let count = rand.below(100);
        for _ in 0..count {
            let x = rand.below(width as u32) as i32 - 8;
            let y = rand.below(height as u32) as i32 - 8;
            let image = common.large_sprite(rand.below(4) as i32 + 69);

            for cy in 0..16 {
                let my = cy + y;
                if my >= height {
                    break;
                }
                if my < 0 {
                    continue;
                }

                for cx in 0..16 {
                    let mx = cx + x;
                    if mx >= width {
                        break;
                    }
                    if mx < 0 {
                        continue;
                    }

                    let src = image[((cy << 4) + cx) as usize];
                    if src > 0 {
                        let pix = self.pixel(mx, my);
                        if pix > 176 && pix < 180 {
                            self.set_pixel(mx, my, ((src as u32 + pix as u32) / 2) as u8, common);
                        } else {
                            self.set_pixel(mx, my, src, common);
                        }
                    }
                }
            }
        }

        let count = rand.below(15);
        for _ in 0..count {
            let x = rand.below(width as u32) as i32 - 8;
            let y = rand.below(height as u32) as i32 - 8;
            let which = rand.below(4) as i32 + 56;
            self.blit_stone(common, false, common.large_sprite(which), x, y);
        }
    }

    /// Whether the size + 1 square at (x, y) has no rock in it.
    fn is_no_rock(&self, size: i32, x: i32, y: i32) -> bool {
        let r = Rect { x1: x, y1: y, x2: x + size + 1, y2: y + size + 1 }.intersect(self.rect());
        for y in r.y1..r.y2 {
            for x in r.x1..r.x2 {
                if self.mat(x, y).rock() {
                    return false;
                }
            }
        }
        true
    }

    /// A random level: the dirt pattern with tunnels dug through it, then big
    /// and small stones where there is no rock yet. The palette is reset by
    /// C++ before.
    pub fn generate_random(&mut self, common: &Common, rand: &mut LieroRand) {
        self.generate_dirt_pattern(common, rand);
        let (width, height) = (self.width, self.height);

        let count = rand.below(50) + 5;
        for _ in 0..count {
            let mut cx = rand.below(width as u32) as i32 - 8;
            let mut cy = rand.below(height as u32) as i32 - 8;

            let dx = rand.below(11) as i32 - 5;
            let dy = rand.below(5) as i32 - 2;

            let count2 = rand.below(12);
            for _ in 0..count2 {
                let count3 = rand.below(5) as i32;
                for _ in 0..count3 {
                    cx += dx;
                    cy += dy;
                    // The C++ version wondered whether this should really be dirt effect 1.
                    self.draw_dirt_effect(common, rand, 1, cx, cy);
                }

                // Also from C++: should it really be (count3 + 1)?
                cx -= (count3 + 1) * dx;
                cy -= (count3 + 1) * dy;

                cx += rand.below(7) as i32 - 3;
                cy += rand.below(15) as i32 - 7;
            }
        }

        let count = rand.below(15) + 5;
        for _ in 0..count {
            let (mut cx, mut cy);
            loop {
                cx = rand.below(width as u32) as i32 - 16;
                if rand.below(4) == 0 {
                    cy = height - 1 - rand.below(20) as i32;
                } else {
                    cy = rand.below(height as u32) as i32 - 16;
                }
                if self.is_no_rock(32, cx, cy) {
                    break;
                }
            }

            let rock = STONE_TAB[rand.below(3) as usize];
            self.blit_stone(common, false, common.large_sprite(rock[0]), cx, cy);
            self.blit_stone(common, false, common.large_sprite(rock[1]), cx + 16, cy);
            self.blit_stone(common, false, common.large_sprite(rock[2]), cx, cy + 16);
            self.blit_stone(common, false, common.large_sprite(rock[3]), cx + 16, cy + 16);
        }

        let count = rand.below(25) + 5;
        for _ in 0..count {
            let (mut cx, mut cy);
            loop {
                cx = rand.below(width as u32) as i32 - 8;
                if rand.below(5) == 0 {
                    cy = height - 1 - rand.below(13) as i32;
                } else {
                    cy = rand.below(height as u32) as i32 - 8;
                }
                if self.is_no_rock(15, cx, cy) {
                    break;
                }
            }

            let frame = rand.below(6) as i32 + 3;
            self.blit_stone(common, false, common.large_sprite(frame), cx, cy);
        }
    }

    /// Darkens what lies 3 pixels down and left of dirt and rock, and gives
    /// the bottom row a floor.
    pub fn make_shadow(&mut self, common: &Common) {
        let (width, height) = (self.width, self.height);

        for x in 0..width - 3 {
            for y in 3..height {
                if self.mat(x, y).see_shadow() && self.mat(x + 3, y - 3).dirt_rock() {
                    self.set_pixel(x, y, self.pixel(x, y).wrapping_add(4), common);
                }

                let pix = self.pixel(x, y);
                if (12..=18).contains(&pix) && self.mat(x + 3, y - 3).rock() {
                    self.set_pixel(x, y, pix - 2, common);
                    if self.pixel(x, y) < 12 {
                        self.set_pixel(x, y, 12, common);
                    }
                }
            }
        }

        for x in 0..width {
            if self.mat(x, height - 1).background() {
                self.set_pixel(x, height - 1, 13, common);
            }
        }
    }

    /// Picks a random w x h area of background or dirt that has ground under
    /// it, for Holdazone's zone. Returns its top-left corner, or None if there
    /// is no such area. C++ only calls it with 0 < w <= width.
    pub fn select_spawn(&self, rand: &mut LieroRand, w: i32, h: i32) -> Option<LieroIVec2> {
        if w < 1 || w > self.width {
            return None;
        }

        let free = |m: LieroMaterial| m.background() || m.any_dirt();
        let columns = (self.width - w + 1) as usize;
        let mut vruns = vec![0i32; columns];
        let mut vdists = vec![0i32; columns];
        let mut selected = None;
        let mut i: u32 = 0;
        // Index of the next material, like C++'s pointer m.
        let mut m = 0usize;

        for y in 0..self.height {
            let mut hrun = 0i32;
            let mut filled = 0i32;

            for x in 0..self.width {
                if free(self.materials[m]) {
                    hrun += 1;
                } else {
                    hrun = 0;
                    filled += 1;
                }
                m += 1;

                let cx = x - (w - 1);
                if cx < 0 {
                    continue;
                }

                let vrun = &mut vruns[cx as usize];
                let vdist = &mut vdists[cx as usize];

                if hrun >= w {
                    if *vdist > 0 {
                        *vrun = 0;
                        *vdist = 0;
                    }
                    *vrun += 1;
                } else {
                    if *vrun >= h && *vdist <= 8 && filled > w / 4 {
                        // A supported area at (x + 1 - w, y - h).
                        i += 1;
                        if rand.below(i) < 1 {
                            selected = Some(LieroIVec2 { x: cx, y: y - h });
                        }
                    }
                    *vdist += 1;
                }

                filled -= !free(self.materials[m - w as usize]) as i32;
            }
        }

        selected
    }

    /// Draws the level shrunk by `step` into an 8-bit bitmap at (map_x, map_y),
    /// one pixel per step x step square (no clipping, as in C++).
    ///
    /// # Safety
    /// `dest` with `pitch` must cover the whole miniature.
    pub unsafe fn draw_miniature(&self, dest: *mut u8, pitch: i32, map_x: i32, map_y: i32, step: i32) {
        let mut my = step / 2;
        let map_end_y = map_y + (self.height + step / 2) / step;
        let map_end_x = map_x + (self.width + step / 2) / step;

        for y in map_y..map_end_y {
            let mut mx = step / 2;
            for x in map_x..map_end_x {
                *dest.offset((y * pitch + x) as isize) = self.checked_pixel_wrap(mx, my);
                mx += step;
            }
            my += step;
        }
    }

    /// Draws a 16 x 16 stone sprite onto the level, clipped to it. With `p1`,
    /// every pixel is written, as palette index + 3 where the sprite is
    /// transparent or the level is rock.
    pub fn blit_stone(&mut self, common: &Common, p1: bool, sprite: &[u8], x: i32, y: i32) {
        let size = LIERO_LARGE_SPRITE_SIZE;
        let Some(c) = clip_image(x, y, size, size, self.rect()) else { return };

        for y_ in 0..c.height {
            let row = self.idx(c.x, c.y + y_);
            let src = c.offset + (y_ * size) as usize;
            for x_ in 0..c.width as usize {
                let s = sprite[src + x_];
                let i = row + x_;
                if p1 {
                    let n = if s != 0 && self.materials[i].dirt_back() { s } else { s.wrapping_add(3) };
                    self.set_index(i, n, common);
                } else if s != 0 {
                    self.set_index(i, s, common);
                }
            }
        }
    }

    /// Draws a `width` x `height` image onto the level, clipped to it: where
    /// the level is dirt or background as it is, elsewhere as palette index
    /// + 3. Was `blitImageOnMap`.
    pub fn blit_image(&mut self, common: &Common, image: &[u8], x: i32, y: i32, width: i32, height: i32) {
        let pitch = width;
        let Some(c) = clip_image(x, y, width, height, self.rect()) else { return };

        for y_ in 0..c.height {
            let row = self.idx(c.x, c.y + y_);
            let src = c.offset + (y_ * pitch) as usize;
            for x_ in 0..c.width as usize {
                let s = image[src + x_];
                if s != 0 {
                    let i = row + x_;
                    let n = if self.materials[i].dirt_back() { s } else { s.wrapping_add(3) };
                    self.set_index(i, n, common);
                }
            }
        }
    }

    /// Applies dirt effect `dirt_effect` with its 16 x 16 mask at (x, y):
    /// removes dirt (explosions, digging) or adds it, depending on the effect.
    /// Picks the effect's dirt texture with `rand`. The bottom row of the
    /// level is never changed.
    pub fn draw_dirt_effect(&mut self, common: &Common, rand: &mut LieroRand, dirt_effect: i32, x: i32, y: i32) {
        let tex = common.textures[dirt_effect as usize];
        let t_frame = common.large_sprite((tex.s_frame as u32).wrapping_add(rand.below(tex.r_frame as u32)) as i32);
        let m_frame = common.large_sprite(tex.m_frame);

        let size = LIERO_LARGE_SPRITE_SIZE;
        let clip = Rect { x1: 0, y1: 0, x2: self.width, y2: self.height - 1 };
        let Some(c) = clip_image(x, y, size, size, clip) else { return };

        for y_ in 0..c.height {
            let row = self.idx(c.x, c.y + y_);
            let src = c.offset + (y_ * size) as usize;
            let my = c.y + y_;
            for x_ in 0..c.width {
                let i = row + x_ as usize;
                let m = self.materials[i];
                let texture = || t_frame[(((my & 15) << 4) + ((c.x + x_) & 15)) as usize];
                let new = if tex.n_draw_back {
                    match m_frame[src + x_ as usize] {
                        6 if m.any_dirt() => Some(texture()),
                        1 if m.dirt2() => Some(2),
                        1 if m.dirt() => Some(1),
                        _ => None,
                    }
                } else {
                    match m_frame[src + x_ as usize] {
                        10 | 6 if m.background() => Some(texture()),
                        2 if m.background() => Some(2),
                        1 if m.background() => Some(1),
                        _ => None,
                    }
                };
                if let Some(n) = new {
                    self.set_index(i, n, common);
                }
            }
        }
    }

    /// Adds and removes shadows in the rectangle from (x1, y1) to (x2, y2)
    /// after the terrain in it changed.
    pub fn correct_shadow(&mut self, common: &Common, x1: i32, y1: i32, x2: i32, y2: i32) {
        let r = Rect { x1, y1, x2, y2 }.intersect(Rect { x1: 0, y1: 3, x2: self.width - 3, y2: self.height });

        for x in r.x1..r.x2 {
            for y in r.y1..r.y2 {
                let pix = self.pixel(x, y);
                let caster = self.mat(x + 3, y - 3);

                if self.mat(x, y).see_shadow() && caster.dirt_rock() {
                    self.set_pixel(x, y, pix.wrapping_add(4), common);
                } else if (164..=167).contains(&pix) && !caster.dirt_rock() {
                    // Remove shadow
                    self.set_pixel(x, y, pix - 4, common);
                }
            }
        }
    }
}

/// Makes a slice from a C++ pointer, which may be null when `len` is 0.
unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

unsafe fn slice_mut<'a, T>(ptr: *mut T, len: usize) -> &'a mut [T] {
    if len == 0 {
        &mut []
    } else {
        std::slice::from_raw_parts_mut(ptr, len)
    }
}

impl LieroLevel {
    /// # Safety
    /// `data` and `materials` must each point to `width * height` elements
    /// that nothing else uses during the call.
    pub unsafe fn get(&self) -> Level<'_> {
        let len = (self.width.max(0) as usize) * (self.height.max(0) as usize);
        Level {
            data: slice_mut(self.data, len),
            materials: slice_mut(self.materials, len),
            width: self.width,
            height: self.height,
        }
    }
}

impl LieroLevelCommon {
    /// # Safety
    /// The pointers must point to tables of the documented sizes.
    pub unsafe fn get(&self) -> Common<'_> {
        Common {
            materials: &*(self.materials as *const [LieroMaterial; 256]),
            textures: &*(self.textures as *const [LieroTexture; LIERO_TEXTURE_COUNT]),
            large_sprites: slice(self.large_sprites, self.large_sprite_count.max(0) as usize * SPRITE_PIXELS),
        }
    }
}

// The functions C++ calls. In each, `level` and `common` are views as
// documented on `LieroLevel` and `LieroLevelCommon`, valid for the call.

/// Sets every pixel's material from its colour.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
#[no_mangle]
pub unsafe extern "C" fn liero_level_update_materials(level: &LieroLevel, common: &LieroLevelCommon) {
    level.get().update_materials(&common.get());
}

/// Fills the level with the dirt pattern of random levels. C++ sizes the
/// level first.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
#[no_mangle]
pub unsafe extern "C" fn liero_level_generate_dirt_pattern(
    level: &LieroLevel,
    common: &LieroLevelCommon,
    rand: &mut LieroRand,
) {
    level.get().generate_dirt_pattern(&common.get(), rand);
}

/// Generates a random level. C++ sizes the level and resets its palette first.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
#[no_mangle]
pub unsafe extern "C" fn liero_level_generate_random(level: &LieroLevel, common: &LieroLevelCommon, rand: &mut LieroRand) {
    level.get().generate_random(&common.get(), rand);
}

/// Adds the shadows of the "Shadows" option.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
#[no_mangle]
pub unsafe extern "C" fn liero_level_make_shadow(level: &LieroLevel, common: &LieroLevelCommon) {
    level.get().make_shadow(&common.get());
}

/// Picks a random w x h area for Holdazone's zone and writes its top-left
/// corner to `selected`. Returns false, leaving `selected` as it was, if there
/// is none.
///
/// # Safety
/// See `LieroLevel`; only `materials` is read.
#[no_mangle]
pub unsafe extern "C" fn liero_level_select_spawn(
    level: &LieroLevel,
    rand: &mut LieroRand,
    w: i32,
    h: i32,
    selected: &mut LieroIVec2,
) -> bool {
    match level.get().select_spawn(rand, w, h) {
        Some(pos) => {
            *selected = pos;
            true
        }
        None => false,
    }
}

/// Draws the level shrunk by `step` into an 8-bit bitmap with `pitch` bytes
/// per row, at (map_x, map_y).
///
/// # Safety
/// See `LieroLevel`; `dest` must cover the whole miniature.
#[no_mangle]
pub unsafe extern "C" fn liero_level_draw_miniature(
    level: &LieroLevel,
    dest: *mut u8,
    pitch: i32,
    map_x: i32,
    map_y: i32,
    step: i32,
) {
    level.get().draw_miniature(dest, pitch, map_x, map_y, step);
}

/// Draws a 16 x 16 stone sprite onto the level.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`; `sprite` must point to 256 bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_level_blit_stone(
    level: &LieroLevel,
    common: &LieroLevelCommon,
    p1: bool,
    sprite: *const u8,
    x: i32,
    y: i32,
) {
    level.get().blit_stone(&common.get(), p1, slice(sprite, SPRITE_PIXELS), x, y);
}

/// Draws a `width` x `height` image onto the level.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`; `image` must point to
/// `width * height` bytes.
pub unsafe fn liero_level_blit_image(
    level: &LieroLevel,
    common: &LieroLevelCommon,
    image: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    let len = (width.max(0) * height.max(0)) as usize;
    level.get().blit_image(&common.get(), slice(image, len), x, y, width, height);
}

/// Applies dirt effect `dirt_effect` (0 to 8) at (x, y).
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
pub unsafe fn liero_level_draw_dirt_effect(
    level: &LieroLevel,
    common: &LieroLevelCommon,
    rand: &mut LieroRand,
    dirt_effect: i32,
    x: i32,
    y: i32,
) {
    level.get().draw_dirt_effect(&common.get(), rand, dirt_effect, x, y);
}

/// Corrects shadows in the rectangle from (x1, y1) to (x2, y2), excluded.
///
/// # Safety
/// See `LieroLevel` and `LieroLevelCommon`.
pub unsafe fn liero_level_correct_shadow(
    level: &LieroLevel,
    common: &LieroLevelCommon,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    level.get().correct_shadow(&common.get(), x1, y1, x2, y2);
}

/// A level's fields that C++'s `Level` refers to.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroLevelData {
    /// The pixels and materials, which the box owns.
    pub level: LieroLevel,
    /// What the level was made from: `Settings::randomLevel`, and the level
    /// file in the box.
    pub old_random_level: bool,
    /// The level's palette, 0 to 63 per component.
    pub origpal: [LieroColor; 256],
}

/// A level. C++ holds a pointer to it, and its fields refer to `data`.
pub struct LieroLevelBox {
    data: LieroLevelData,
    pixels: Vec<u8>,
    materials: Vec<LieroMaterial>,
    old_level_file: Vec<u8>,
}

impl LieroLevelBox {
    /// An empty level: C++'s `Level` constructor.
    pub fn new() -> Box<Self> {
        let mut b = Box::new(LieroLevelBox {
            data: LieroLevelData {
                level: LieroLevel { data: std::ptr::null_mut(), materials: std::ptr::null_mut(), width: 0, height: 0 },
                old_random_level: false,
                origpal: [LieroColor::default(); 256],
            },
            pixels: Vec::new(),
            materials: Vec::new(),
            old_level_file: Vec::new(),
        });
        b.update_view();
        b
    }

    /// Points the view at the pixels and materials, after they move.
    fn update_view(&mut self) {
        self.data.level.data = self.pixels.as_mut_ptr();
        self.data.level.materials = self.materials.as_mut_ptr();
    }

    /// `Level::resize`: keeps the first pixels, and new ones are zero.
    pub fn resize(&mut self, width: i32, height: i32) {
        let n = (width.max(0) as usize) * (height.max(0) as usize);
        self.pixels.resize(n, 0);
        self.materials.resize(n, LieroMaterial::default());
        self.data.level.width = width;
        self.data.level.height = height;
        self.update_view();
    }

    pub(crate) fn data(&self) -> &LieroLevelData {
        &self.data
    }

    /// The level file the level was made from (`oldLevelFile`).
    pub(crate) fn old_level_file(&self) -> &[u8] {
        &self.old_level_file
    }

    pub(crate) fn data_mut(&mut self) -> &mut LieroLevelData {
        &mut self.data
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Sets every pixel, with its material (`Level::setPixel`).
    pub(crate) fn set_pixels(&mut self, pixels: &[u8], material: impl Fn(u8) -> LieroMaterial) {
        for ((p, m), &new) in self.pixels.iter_mut().zip(&mut self.materials).zip(pixels) {
            *p = new;
            *m = material(new);
        }
    }

    /// Makes this a copy of `other`, in place, so that C++'s references
    /// into `data` stay valid.
    pub fn assign(&mut self, other: &Self) {
        self.data = other.data;
        self.pixels.clone_from(&other.pixels);
        self.materials.clone_from(&other.materials);
        self.old_level_file.clone_from(&other.old_level_file);
        self.update_view();
    }

    /// `Level::generateRandom`: the TC's palette, then a random level.
    ///
    /// # Safety
    /// `common` is valid.
    pub(crate) unsafe fn generate_random(&mut self, common: &LieroLevelCommon, exepal: &[LieroColor; 256], rand: &mut LieroRand) {
        self.data.origpal = *exepal;
        self.resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
        self.data.level.get().generate_random(&common.get(), rand);
    }

    /// `Level::load`: a .LEV file's pixels, and with `powerlevel_palette`
    /// the palette after "POWERLEVEL" if there is one, else the TC's. False
    /// where C++ threw: the file ends inside the pixels or that palette.
    ///
    /// # Safety
    /// `common` is valid.
    pub(crate) unsafe fn load(
        &mut self,
        common: &LieroLevelCommon,
        exepal: &[LieroColor; 256],
        powerlevel_palette: bool,
        bytes: &[u8],
    ) -> bool {
        self.resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
        let n = self.pixels.len();
        // What there is is read before the error, as C++'s reader did.
        let k = n.min(bytes.len());
        self.pixels[..k].copy_from_slice(&bytes[..k]);
        if k < n {
            return false;
        }
        let mut rest = &bytes[n..];
        let mut palette = None;
        // Any bytes at all are read as the mark, the missing ones as zero.
        if powerlevel_palette && !rest.is_empty() {
            let mut mark = [0; 10];
            let m = rest.len().min(10);
            mark[..m].copy_from_slice(&rest[..m]);
            rest = &rest[m..];
            if &mark == b"POWERLEVEL" {
                if rest.len() < 768 {
                    return false;
                }
                palette = Some(std::array::from_fn(|i| LieroColor {
                    r: rest[i * 3] & 63,
                    g: rest[i * 3 + 1] & 63,
                    b: rest[i * 3 + 2] & 63,
                    unused: 0,
                }));
            }
        }
        self.data.level.get().update_materials(&common.get());
        self.data.origpal = palette.unwrap_or(*exepal);
        true
    }

    /// `Level::generateFromSettings`: the settings' level file (".LEV"
    /// added if it has no dot), or a random level if it's random or can't
    /// be read, with shadows if they're on.
    ///
    /// # Safety
    /// `common` is valid.
    pub(crate) unsafe fn generate_from_settings(
        &mut self,
        common: &LieroLevelCommon,
        exepal: &[LieroColor; 256],
        settings: &crate::settings::LieroSettingsBox,
        rand: &mut LieroRand,
    ) {
        let s = &settings.data;
        let mut loaded = false;
        if !s.random_level {
            let mut path = settings.level_file.clone();
            if !path.contains(&b'.') {
                path.extend_from_slice(b".LEV");
            }
            if let Some(bytes) = crate::fs::Node::new(&path).read() {
                loaded = self.load(common, exepal, s.load_powerlevel_palette, &bytes);
            }
        }
        if !loaded {
            self.generate_random(common, exepal, rand);
        }
        self.data.old_random_level = s.random_level;
        self.old_level_file.clone_from(&settings.level_file);
        if s.shadow {
            self.data.level.get().make_shadow(&common.get());
        }
    }
}

/// See `LieroLevelBox::generate_from_settings`.
///
/// # Safety
/// All are valid; `exepal` holds 256 colours.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_generate_from_settings(
    l: &mut LieroLevelBox,
    common: &LieroLevelCommon,
    exepal: *const LieroColor,
    settings: &crate::settings::LieroSettingsBox,
    rand: &mut LieroRand,
) {
    l.generate_from_settings(common, &*(exepal as *const [LieroColor; 256]), settings, rand);
}

/// See `LieroLevelBox::load`.
///
/// # Safety
/// All are valid; `exepal` holds 256 colours and `bytes` `len`.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_load(
    l: &mut LieroLevelBox,
    common: &LieroLevelCommon,
    exepal: *const LieroColor,
    powerlevel_palette: bool,
    bytes: *const u8,
    len: usize,
) -> bool {
    let bytes = if len == 0 { &[][..] } else { std::slice::from_raw_parts(bytes, len) };
    l.load(common, &*(exepal as *const [LieroColor; 256]), powerlevel_palette, bytes)
}

#[no_mangle]
pub extern "C" fn liero_level_box_new() -> *mut LieroLevelBox {
    Box::into_raw(LieroLevelBox::new())
}

/// # Safety
/// `l` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_clone(l: &LieroLevelBox) -> *mut LieroLevelBox {
    let mut b = LieroLevelBox::new();
    b.assign(l);
    Box::into_raw(b)
}

/// `Level::operator=`.
///
/// # Safety
/// Both are valid, and no view of `to`'s pixels is used after.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_assign(to: &mut LieroLevelBox, from: &LieroLevelBox) {
    if !std::ptr::eq(to, from) {
        to.assign(from);
    }
}

/// `Level::swap`: the two boxes trade contents, so C++'s references into
/// each stay valid.
///
/// # Safety
/// Both are valid and different.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_swap(a: &mut LieroLevelBox, b: &mut LieroLevelBox) {
    std::mem::swap(a, b);
}

/// # Safety
/// `l` comes from `liero_level_box_new` or `_clone`, and isn't used after.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_free(l: *mut LieroLevelBox) {
    if !l.is_null() {
        drop(Box::from_raw(l));
    }
}

/// The fields C++ refers to, valid as long as the box.
///
/// # Safety
/// `l` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_data(l: &mut LieroLevelBox) -> *mut LieroLevelData {
    &mut l.data
}

/// See `LieroLevelBox::resize`. Views of the pixels are invalid after.
///
/// # Safety
/// `l` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_resize(l: &mut LieroLevelBox, width: i32, height: i32) {
    l.resize(width, height);
}

/// The level file the level was made from.
///
/// # Safety
/// `l` is valid; the text is valid until the level changes.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_old_level_file(l: &LieroLevelBox) -> LieroStr {
    LieroStr { ptr: l.old_level_file.as_ptr(), len: l.old_level_file.len() }
}

/// # Safety
/// `l` and `text` are valid.
#[no_mangle]
pub unsafe extern "C" fn liero_level_box_set_old_level_file(l: &mut LieroLevelBox, text: LieroStr) {
    l.old_level_file = text.bytes().to_vec();
}

#[cfg(test)]
#[path = "level_box_tests.rs"]
mod box_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_box_resizes_copies_and_swaps() {
        unsafe {
            let a = liero_level_box_new();
            let d = liero_level_box_data(&mut *a);
            assert_eq!(((*d).level.width, (*d).level.height, (*d).old_random_level), (0, 0, false));

            liero_level_box_resize(&mut *a, 3, 2);
            *(*d).level.data.add(5) = 7;
            (*(*d).level.materials.add(5)).flags = 2;
            (*d).origpal[9].g = 33;
            (*d).old_random_level = true;
            let name = b"levels/A.LEV";
            liero_level_box_set_old_level_file(&mut *a, LieroStr { ptr: name.as_ptr(), len: name.len() });

            // A copy has pixels of its own.
            let c = liero_level_box_clone(&*a);
            let cd = liero_level_box_data(&mut *c);
            assert_ne!((*cd).level.data, (*d).level.data);
            assert_eq!(((*cd).level.width, *(*cd).level.data.add(5), (*(*cd).level.materials.add(5)).flags), (3, 7, 2));
            assert_eq!(((*cd).origpal[9].g, (*cd).old_random_level), (33, true));
            assert_eq!(liero_level_box_old_level_file(&*c).bytes(), name);
            *(*cd).level.data.add(5) = 1;
            assert_eq!(*(*d).level.data.add(5), 7);

            // Growing keeps the first pixels.
            liero_level_box_resize(&mut *a, 4, 2);
            assert_eq!((*(*d).level.data.add(5), *(*d).level.data.add(7)), (7, 0));

            // Swapping trades contents; the data pointers stay put.
            liero_level_box_swap(&mut *a, &mut *c);
            assert_eq!(((*d).level.width, *(*d).level.data.add(5)), (3, 1));
            assert_eq!(((*cd).level.width, *(*cd).level.data.add(5)), (4, 7));

            liero_level_box_assign(&mut *a, &*c);
            assert_eq!(((*d).level.width, *(*d).level.data.add(5)), (4, 7));
            assert_ne!((*d).level.data, (*cd).level.data);
            liero_level_box_free(c);
            liero_level_box_free(a);
        }
    }
    use crate::material::LIERO_MATERIAL_ROCK;
    use crate::test_util::Fnv;

    // The expected hashes are printed by reference/reference_values.cpp,
    // which runs the same steps through the original C++ code.

    /// Made-up TC tables, generated as in the reference: random materials with
    /// few rocks (none in the dirt colours 12 to 19, so random levels always
    /// find room for their stones), random dirt effects, and sprites with many
    /// transparent pixels and many of the mask values 1, 2, 6 and 10.
    struct TestCommon {
        materials: [LieroMaterial; 256],
        textures: [LieroTexture; LIERO_TEXTURE_COUNT],
        large_sprites: Vec<u8>,
    }

    impl TestCommon {
        fn new() -> Self {
            let mut r = LieroRand::new(0xa55e7);
            let mut materials = [LieroMaterial::default(); 256];
            for (i, m) in materials.iter_mut().enumerate() {
                let mut f = r.below(64) as u8;
                if r.below(4) != 0 || (12..=19).contains(&i) {
                    f &= !LIERO_MATERIAL_ROCK;
                }
                m.flags = f;
            }
            let mut textures = [LieroTexture::default(); LIERO_TEXTURE_COUNT];
            for t in textures.iter_mut() {
                t.n_draw_back = r.below(2) == 1;
                t.m_frame = r.below(110) as i32;
                t.s_frame = r.below(100) as i32;
                t.r_frame = r.below(10) as i32 + 1;
            }
            let mut large_sprites = vec![0u8; 110 * SPRITE_PIXELS];
            for p in large_sprites.iter_mut() {
                *p = if r.below(3) == 0 {
                    0
                } else if r.below(2) == 0 {
                    r.below(12) as u8
                } else {
                    r.below(256) as u8
                };
            }
            TestCommon { materials, textures, large_sprites }
        }

        fn get(&self) -> Common<'_> {
            Common { materials: &self.materials, textures: &self.textures, large_sprites: &self.large_sprites }
        }
    }

    /// A level that owns its pixels, like C++'s `Level`.
    struct TestLevel {
        data: Vec<u8>,
        materials: Vec<LieroMaterial>,
        width: i32,
        height: i32,
    }

    impl TestLevel {
        fn new(width: i32, height: i32) -> Self {
            let len = (width * height) as usize;
            TestLevel { data: vec![0; len], materials: vec![LieroMaterial::default(); len], width, height }
        }

        fn get(&mut self) -> Level<'_> {
            Level { data: &mut self.data, materials: &mut self.materials, width: self.width, height: self.height }
        }

        fn random(common: &Common, seed: u32) -> Self {
            let mut level = TestLevel::new(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
            let mut r = LieroRand::new(seed);
            level.get().generate_random(common, &mut r);
            level.get().make_shadow(common);
            level
        }

        fn hash(&self, h: &mut Fnv) {
            h.add(self.width as u32);
            h.add(self.height as u32);
            for (d, m) in self.data.iter().zip(&self.materials) {
                h.add(*d as u32);
                h.add(m.flags as u32);
            }
        }
    }

    fn hash_rand(h: &mut Fnv, r: &LieroRand) {
        h.add(r.x);
        h.add(r.c);
    }

    #[test]
    fn dirt_pattern_matches_cpp() {
        let tc = TestCommon::new();
        let mut level = TestLevel::new(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
        let mut r = LieroRand::new(1);
        level.get().generate_dirt_pattern(&tc.get(), &mut r);
        let mut h = Fnv::new();
        level.hash(&mut h);
        hash_rand(&mut h, &r);
        assert_eq!(h.finish(), EXPECTED_DIRT_PATTERN);
    }

    #[test]
    fn random_levels_match_cpp() {
        let tc = TestCommon::new();
        let common = tc.get();
        let mut h = Fnv::new();
        for seed in [2, 3, 0xdead_beef, 0x1337] {
            let mut level = TestLevel::new(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
            let mut r = LieroRand::new(seed);
            level.get().generate_random(&common, &mut r);
            level.hash(&mut h);
            hash_rand(&mut h, &r);
            level.get().make_shadow(&common);
            level.hash(&mut h);
        }
        assert_eq!(h.finish(), EXPECTED_RANDOM_LEVELS);
    }

    #[test]
    fn terrain_ops_match_cpp() {
        let tc = TestCommon::new();
        let common = tc.get();
        let mut level = TestLevel::random(&common, 2);
        let mut ops = LieroRand::new(99);
        let mut game_rand = LieroRand::new(100);
        for _ in 0..3000 {
            let op = ops.below(4);
            let x = ops.below(level.width as u32 + 32) as i32 - 16;
            let y = ops.below(level.height as u32 + 32) as i32 - 16;
            let mut l = level.get();
            match op {
                0 => {
                    let effect = ops.below(9) as i32;
                    l.draw_dirt_effect(&common, &mut game_rand, effect, x, y);
                }
                1 => {
                    let w = ops.below(40) as i32;
                    let h = ops.below(40) as i32;
                    l.correct_shadow(&common, x, y, x + w, y + h);
                }
                2 => {
                    let p1 = ops.below(2) == 1;
                    let frame = ops.below(110) as i32;
                    l.blit_stone(&common, p1, common.large_sprite(frame), x, y);
                }
                _ => {
                    let frame = ops.below(110) as i32;
                    let w = ops.below(16) as i32 + 1;
                    let h = ops.below(16) as i32 + 1;
                    l.blit_image(&common, common.large_sprite(frame), x, y, w, h);
                }
            }
        }
        let mut h = Fnv::new();
        level.hash(&mut h);
        hash_rand(&mut h, &ops);
        hash_rand(&mut h, &game_rand);
        assert_eq!(h.finish(), EXPECTED_TERRAIN_OPS);
    }

    #[test]
    fn select_spawn_matches_cpp() {
        let tc = TestCommon::new();
        let common = tc.get();
        let mut h = Fnv::new();
        let mut found_any = false;
        for seed in [2, 3] {
            let mut level = TestLevel::random(&common, seed);
            for (w, ht) in [(50, 26), (30, 20), (5, 5), (100, 60), (504, 10), (1, 1), (200, 200)] {
                let mut r = LieroRand::new(7);
                let selected = level.get().select_spawn(&mut r, w, ht);
                let pos = selected.unwrap_or(LieroIVec2 { x: -1, y: -1 });
                found_any |= selected.is_some();
                h.add(selected.is_some() as u32);
                h.add(pos.x as u32);
                h.add(pos.y as u32);
                hash_rand(&mut h, &r);
            }
        }
        assert!(found_any);
        assert_eq!(h.finish(), EXPECTED_SELECT_SPAWN);
    }

    #[test]
    fn miniature_matches_cpp() {
        let tc = TestCommon::new();
        let mut level = TestLevel::random(&tc.get(), 3);
        let mut bitmap = vec![0xeeu8; 400 * 300];
        let l = level.get();
        unsafe {
            l.draw_miniature(bitmap.as_mut_ptr(), 400, 134, 162, 10);
            l.draw_miniature(bitmap.as_mut_ptr(), 400, 10, 20, 2);
            l.draw_miniature(bitmap.as_mut_ptr(), 400, 0, 0, 3);
        }
        let mut h = Fnv::new();
        for p in bitmap {
            h.add(p as u32);
        }
        assert_eq!(h.finish(), EXPECTED_MINIATURE);
    }

    #[test]
    fn loaded_materials_match_cpp() {
        let tc = TestCommon::new();
        let mut level = TestLevel::new(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
        let mut r = LieroRand::new(5);
        for p in level.data.iter_mut() {
            *p = r.below(256) as u8;
        }
        level.get().update_materials(&tc.get());
        let mut h = Fnv::new();
        level.hash(&mut h);
        assert_eq!(h.finish(), EXPECTED_LOADED_MATERIALS);
    }

    #[test]
    fn c_functions_work_on_views() {
        // The same as random_levels_match_cpp's first level, through the
        // functions C++ calls.
        let tc = TestCommon::new();
        let mut level = TestLevel::new(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
        let view = LieroLevel {
            data: level.data.as_mut_ptr(),
            materials: level.materials.as_mut_ptr(),
            width: level.width,
            height: level.height,
        };
        let common = LieroLevelCommon {
            materials: tc.materials.as_ptr(),
            textures: tc.textures.as_ptr(),
            large_sprites: tc.large_sprites.as_ptr(),
            large_sprite_count: 110,
        };
        let mut r = LieroRand::new(2);
        unsafe {
            liero_level_generate_random(&view, &common, &mut r);
            liero_level_make_shadow(&view, &common);
        }
        let mut expected = TestLevel::random(&tc.get(), 2);
        assert!(level.data == expected.data && level.materials == expected.materials);

        let mut pos = LieroIVec2 { x: -1, y: -1 };
        let mut r1 = LieroRand::new(7);
        let found = unsafe { liero_level_select_spawn(&view, &mut r1, 50, 26, &mut pos) };
        let mut r2 = LieroRand::new(7);
        assert_eq!(found.then_some(pos), expected.get().select_spawn(&mut r2, 50, 26));
    }

    const EXPECTED_DIRT_PATTERN: u64 = 0x9367_c037_eaab_3ca0;
    const EXPECTED_RANDOM_LEVELS: u64 = 0xb660_2342_bf02_9a43;
    const EXPECTED_TERRAIN_OPS: u64 = 0x3f82_b758_4150_9509;
    const EXPECTED_SELECT_SPAWN: u64 = 0x5ff4_1507_aa05_1c80;
    const EXPECTED_MINIATURE: u64 = 0xcbf3_6c7c_e386_6158;
    const EXPECTED_LOADED_MATERIALS: u64 = 0x2e41_4997_78f4_a4de;
}
