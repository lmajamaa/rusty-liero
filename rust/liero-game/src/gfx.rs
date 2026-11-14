//! Drawing: the 8-bit screen bitmap's primitives (`gfx/blit.cpp`), palette
//! effects (`gfx/palette.cpp`) and the bitmap font (`gfx/font.cpp`). Frames
//! have to stay pixel-identical to the C++ versions, which `tools/hashrun`
//! checks by drawing the viewports every 100 ticks.
//!
//! Drawing goes through a `Bitmap`, a copyable view of a bitmap's
//! pixels as cells, so that the screen can be drawn on from wherever a view
//! of it was passed. Images are slices; reads outside them panic, where C++
//! read other memory. Writes to the screen are kept inside its buffer.

use liero_data::model::FontChar;
use std::cell::Cell;
use std::collections::BTreeMap;

use crate::game::Rect;
use crate::material::Material;
use crate::rand::Rand;

/// A view of a bitmap: `h` rows of `pitch` pixels, of which the first `w`
/// are the image. Drawing stays inside `clip` (x2 and y2 excluded), except
/// where the C++ didn't clip either.
#[derive(Clone, Copy, Debug)]
pub struct Bitmap<'a> {
    pub pixels: &'a [Cell<u8>],
    pub w: i32,
    pub h: i32,
    pub pitch: i32,
    pub clip: Rect,
}

/// A palette entry, laid out like C++'s `Color`. Components are 0 to 63 in
/// `Palette`, 0 to 255 once activated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub unused: u8,
}

impl<'a> Bitmap<'a> {
    /// A view of all of `pixels`, `pitch` wide, clipped to `w` x `h`.
    pub fn new(pixels: &'a mut [u8], w: i32, h: i32, pitch: i32) -> Self {
        Bitmap {
            pixels: Cell::from_mut(pixels).as_slice_of_cells(),
            w,
            h,
            pitch,
            clip: Rect {
                x1: 0,
                y1: 0,
                x2: w,
                y2: h,
            },
        }
    }

    /// Index of (x, y) as C++'s `getPixel` computed it, or None outside the
    /// buffer, where C++ would have written to other memory.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let i = (y as i64) * (self.pitch as i64) + x as i64;
        (0 <= i && (i as usize) < self.pixels.len()).then_some(i as usize)
    }

    pub(crate) fn get(&self, x: i32, y: i32) -> u8 {
        self.index(x, y).map_or(0, |i| self.pixels[i].get())
    }

    pub(crate) fn set(&self, x: i32, y: i32, c: u8) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i].set(c);
        }
    }

    /// `memset` of `n` pixels from (x, y).
    fn span(&self, x: i32, y: i32, n: i32, c: u8) {
        for i in 0..n.max(0) {
            self.set(x + i, y, c);
        }
    }
}

impl Rect {
    /// `gvl::rect::inside`.
    pub(crate) fn contains(&self, x: i32, y: i32) -> bool {
        let dx = x.wrapping_sub(self.x1);
        let dy = y.wrapping_sub(self.y1);
        dx < self.x2 - self.x1 && dx >= 0 && dy < self.y2 - self.y1 && dy >= 0
    }
}

/// A `width` x `height` image drawn at (x, y) and clipped to `clip`, as the
/// `CLIP_IMAGE` macro did. `src` is where its visible part starts in the
/// source image, whose rows are `pitch` apart.
struct Clipped {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    src: isize,
}

fn clip_image(mut x: i32, mut y: i32, mut width: i32, mut height: i32, pitch: i32, clip: Rect) -> Option<Clipped> {
    let mut src = 0isize;

    let top = y - clip.y1;
    if top < 0 {
        src += (-top as isize) * pitch as isize;
        height += top;
        y = clip.y1;
    }
    let bottom = y + height - clip.y2;
    if bottom > 0 {
        height -= bottom;
    }
    let left = x - clip.x1;
    if left < 0 {
        src -= left as isize;
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
    Some(Clipped {
        x,
        y,
        width,
        height,
        src,
    })
}

/// Draws a `width` x `height` image from `mem` (rows `pitch` apart) at
/// (x, y), clipped. `f` gets the source and screen pixel, the position in
/// the visible part, and returns the new screen pixel, if any.
// The arguments of the C++ it was ported from; phase 4 of PLAN.md groups them.
#[allow(clippy::too_many_arguments)]
fn blit(
    scr: &Bitmap,
    mem: &[u8],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
    mut f: impl FnMut(u8, u8, i32, i32) -> Option<u8>,
) {
    let Some(c) = clip_image(x, y, width, height, pitch, scr.clip) else {
        return;
    };

    for row in 0..c.height {
        let src = c.src + row as isize * pitch as isize;
        for col in 0..c.width {
            let (sx, sy) = (c.x + col, c.y + row);
            if let Some(n) = f(mem[(src + col as isize) as usize], scr.get(sx, sy), col, row) {
                scr.set(sx, sy, n);
            }
        }
    }
}

/// Calls `f` for each point of the line from (from_x, from_y) to (to_x,
/// to_y), without the start: the `DO_LINE` macro.
fn line(from_x: i32, from_y: i32, to_x: i32, to_y: i32, mut f: impl FnMut(i32, i32)) {
    let (mut cx, mut cy) = (from_x, from_y);
    let (dx, dy) = (to_x - from_x, to_y - from_y);
    let (sx, sy) = (dx.signum(), dy.signum());
    let (dx, dy) = (dx.abs(), dy.abs());

    if dx > dy {
        let mut c = -(dx >> 1);
        while cx != to_x {
            c += dy;
            cx += sx;
            if c > 0 {
                cy += sy;
                c -= dx;
            }
            f(cx, cy);
        }
    } else {
        let mut c = -(dy >> 1);
        while cy != to_y {
            c += dx;
            cy += sy;
            if c > 0 {
                cx += sx;
                c -= dy;
            }
            f(cx, cy);
        }
    }
}

/// Fills a rectangle, clipped: `fillRect`.
pub fn liero_gfx_fill_rect(scr: &Bitmap, x: i32, y: i32, w: i32, h: i32, color: u8) {
    let s = scr;
    let x2 = (x + w).min(s.clip.x2);
    let y2 = (y + h).min(s.clip.y2);
    let x = x.max(s.clip.x1);
    let y = y.max(s.clip.y1);

    if x2 > x {
        for y in y..y2 {
            s.span(x, y, x2 - x, color);
        }
    }
}

/// Fills the whole buffer: `fill`.
pub fn liero_gfx_fill(scr: &Bitmap, color: u8) {
    scr.pixels.iter().for_each(|p| p.set(color));
}

/// A bar `width` long and `height` high, not clipped: `drawBar`.
pub fn liero_gfx_draw_bar(scr: &Bitmap, x: i32, y: i32, width: i32, height: i32, color: u8) {
    let s = scr;
    for h in 0..height {
        if width > 0 {
            s.span(x, y + h, width, color);
        }
    }
}

/// A vertical line from y1 to y2 (excluded), clipped: `vline`.
pub fn liero_gfx_vline(scr: &Bitmap, x: i32, y1: i32, y2: i32, color: u8) {
    let s = scr;
    if x < s.clip.x1 || x >= s.clip.x2 {
        return;
    }
    for y in y1.max(s.clip.y1)..y2.min(s.clip.y2) {
        s.set(x, y, color);
    }
}

/// A filled box with cut corners: `drawRoundedBox`.
pub fn liero_gfx_draw_rounded_box(scr: &Bitmap, x: i32, y: i32, color: u8, height: i32, width: i32) {
    liero_gfx_fill_rect(scr, x, y + 1, width + 3, height - 2, color);
    liero_gfx_fill_rect(scr, x + 1, y, width + 1, 1, color);
    liero_gfx_fill_rect(scr, x + 1, y + height - 1, width + 1, 1, color);
}

/// The outline of a box with cut corners: `drawRoundedLineBox`.
pub fn liero_gfx_draw_rounded_line_box(scr: &Bitmap, x: i32, y: i32, color: u8, width: i32, height: i32) {
    liero_gfx_fill_rect(scr, x + 1, y, width - 2, 1, color);
    liero_gfx_fill_rect(scr, x + 1, y + height - 1, width - 2, 1, color);
    liero_gfx_fill_rect(scr, x, y + 1, 1, height - 2, color);
    liero_gfx_fill_rect(scr, x + width - 1, y + 1, 1, height - 2, color);
}

/// A dashed outline, going round anticlockwise from the top-left corner,
/// in `color2` for the first `num / den` of the way and `color` for the
/// rest: `drawDashedLineBox`, which shows Holdazone's zone and how far it
/// is taken.
#[allow(clippy::too_many_arguments)]
pub fn liero_gfx_draw_dashed_line_box(
    scr: &Bitmap,
    x: i32,
    y: i32,
    color: u8,
    color2: u8,
    num: i32,
    den: i32,
    width: i32,
    height: i32,
    phase: i32,
) {
    let s = scr;
    let mut p = 0;
    let perim = 2 * (width + height) - 2;
    let color2_lim = num * perim / den;

    let mut dash = |x1: i32, y1: i32| {
        if s.clip.contains(x1, y1) && (p + phase) % 4 < 2 {
            s.set(x1, y1, if p >= color2_lim { color } else { color2 });
        }
        p += 1;
    };

    for y1 in y..y + height {
        dash(x, y1);
    }
    for x1 in x + 1..x + width - 1 {
        dash(x1, y + height - 1);
    }
    for y1 in (y..y + height).rev() {
        dash(x + width - 1, y1);
    }
    for x1 in (x + 1..x + width - 1).rev() {
        dash(x1, y);
    }
}

/// Copies an image, colour 0 included: `blitImageNoKeyColour`.
pub fn liero_gfx_blit_image_no_key_colour(
    scr: &Bitmap,
    mem: &[u8],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
) {
    blit(scr, mem, x, y, width, height, pitch, |c, _, _, _| Some(c));
}

/// Draws a sprite, colour 0 transparent: `blitImage`.
pub fn liero_gfx_blit_image(scr: &Bitmap, mem: &[u8], x: i32, y: i32, width: i32, height: i32, pitch: i32) {
    blit(scr, mem, x, y, width, height, pitch, |c, _, _, _| (c != 0).then_some(c));
}

/// Draws every other pixel of a sprite, in a checkerboard that `phase`
/// shifts: `blitImageTrans`, for worms flickering at respawn. The pattern
/// follows the visible part, as in C++.
#[allow(clippy::too_many_arguments)]
pub fn liero_gfx_blit_image_trans(
    scr: &Bitmap,
    mem: &[u8],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
    phase: i32,
) {
    blit(scr, mem, x, y, width, height, pitch, |c, _, col, row| {
        (c != 0 && (col ^ row ^ phase) & 1 != 0).then_some(c)
    });
}

/// Draws a sprite only over colours 160 to 167: `blitImageR`, for
/// bonuses showing through the fog.
pub fn liero_gfx_blit_image_r(scr: &Bitmap, mem: &[u8], x: i32, y: i32, width: i32, height: i32) {
    blit(scr, mem, x, y, width, height, width, |c, d, _, _| {
        (c != 0 && d.wrapping_sub(160) < 8).then_some(c)
    });
}

/// Draws the 16 x 16 fire cone sprite, darker as `fc` counts down:
/// `blitFireCone`.
pub fn liero_gfx_blit_fire_cone(scr: &Bitmap, fc: i32, mem: &[u8], x: i32, y: i32) {
    blit(scr, mem, x, y, 16, 16, 16, |c, _, _, _| match fc {
        0 => (c > 116).then(|| c - 5),
        1 => (c > 114).then(|| c - 3),
        2 => (c > 112).then(|| c - 1),
        _ => (c != 0).then_some(c),
    });
}

/// Darkens the screen under a sprite's non-zero pixels where the material
/// shows shadows: `blitShadowImage`.
pub fn liero_gfx_blit_shadow_image(
    scr: &Bitmap,
    materials: &[Material; 256],
    mem: &[u8],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    blit(scr, mem, x, y, width, height, width, |c, d, _, _| {
        (c != 0 && materials[d as usize].see_shadow()).then(|| d.wrapping_add(4))
    });
}

/// The ninja rope, cycling through colours `begin` to `end` (excluded):
/// `drawNinjarope`.
pub fn liero_gfx_draw_ninjarope(scr: &Bitmap, begin: i32, end: i32, from_x: i32, from_y: i32, to_x: i32, to_y: i32) {
    let s = scr;
    let mut color = begin;

    line(from_x, from_y, to_x, to_y, |cx, cy| {
        color += 1;
        if color == end {
            color = begin;
        }
        if s.clip.contains(cx, cy) {
            s.set(cx, cy, color as u8);
        }
    });
}

/// A sparkling laser sight: a fifth of the points, in colour 83 or 84.
/// `drawLaserSight`.
pub fn liero_gfx_draw_laser_sight(scr: &Bitmap, rand: &mut Rand, from_x: i32, from_y: i32, to_x: i32, to_y: i32) {
    let s = scr;

    line(from_x, from_y, to_x, to_y, |cx, cy| {
        if rand.below(5) == 0 && s.clip.contains(cx, cy) {
            s.set(cx, cy, rand.below(2) as u8 + 83);
        }
    });
}

/// The ninja rope's shadow: `drawShadowLine`.
pub fn liero_gfx_draw_shadow_line(
    scr: &Bitmap,
    materials: &[Material; 256],
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
) {
    let s = scr;

    line(from_x, from_y, to_x, to_y, |cx, cy| {
        if s.clip.contains(cx, cy) {
            let pix = s.get(cx, cy);
            if materials[pix as usize].see_shadow() {
                s.set(cx, cy, pix.wrapping_add(4));
            }
        }
    });
}

/// A line, clipped: `drawLine`.
pub fn liero_gfx_draw_line(scr: &Bitmap, from_x: i32, from_y: i32, to_x: i32, to_y: i32, color: u8) {
    let s = scr;
    line(from_x, from_y, to_x, to_y, |cx, cy| {
        if s.clip.contains(cx, cy) {
            s.set(cx, cy, color);
        }
    });
}

/// A bar graph of `len` values in a framed box `height` high, from the
/// bottom or, when `balanced`, from the middle: `drawGraph`.
#[allow(clippy::too_many_arguments)]
pub fn liero_gfx_draw_graph(
    scr: &Bitmap,
    data: &[f64],
    height: i32,
    start_x: i32,
    start_y: i32,
    color: u8,
    neg_color: u8,
    balanced: bool,
) {
    let base_y = start_y + if balanced { height / 2 } else { height };

    for (x, &v) in (start_x..).zip(data) {
        let mut y1 = base_y - (v + 0.5).floor() as i32;
        let mut y2 = base_y;
        if y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        liero_gfx_vline(scr, x, y1, y2, if v >= 0.0 { color } else { neg_color });
    }

    liero_gfx_draw_rounded_line_box(scr, start_x, start_y, 7, data.len() as i32, height);
}

/// A heatmap of `width` x `height` counts, in colours 104 to 119 by rank,
/// so each colour covers about as many pixels: `drawHeatmap`.
pub fn liero_gfx_draw_heatmap(scr: &Bitmap, x: i32, y: i32, cells: &[i32], width: i32, height: i32) {
    let mut counts = BTreeMap::<i32, i64>::new();
    for &v in cells.iter().filter(|&&v| v != 0) {
        *counts.entry(v).or_default() += 1;
    }
    let total: i64 = counts.values().sum();

    let max_idx = 119 - 104 + 1;
    let mut mapping = BTreeMap::from([(0, 0)]);
    let mut cum = 0i64;
    for (&v, &count) in &counts {
        mapping.insert(v, (104 + cum * max_idx / total) as i32);
        cum += count;
    }

    let Some(c) = clip_image(x, y, width, height, width, scr.clip) else {
        return;
    };
    let s = scr;
    for row in 0..c.height {
        for col in 0..c.width {
            let v = cells[(c.src + (row * width + col) as isize) as usize];
            s.set(c.x + col, c.y + row, mapping[&v] as u8);
        }
    }
}

/// Converts `w` x `h` palette indices (rows `src_pitch` apart) to 32-bit
/// pixels with `pal32`, each `mag` x `mag` pixels big, into `dest`, whose
/// rows are `dest_pitch` bytes apart: `scaleDraw`. Above 1x, only whole
/// groups of 4 pixels in a row are drawn, as in C++.
#[allow(clippy::too_many_arguments)]
pub fn liero_gfx_scale_draw(
    src: &[Cell<u8>],
    w: i32,
    h: i32,
    src_pitch: usize,
    dest: &mut [u8],
    dest_pitch: usize,
    mag: i32,
    pal32: &[u32; 256],
) {
    let mut put = |at: usize, pix: u32| dest[at..at + 4].copy_from_slice(&pix.to_ne_bytes());
    if mag == 1 {
        for y in 0..h as usize {
            for x in 0..w as usize {
                put(y * dest_pitch + x * 4, pal32[src[y * src_pitch + x].get() as usize]);
            }
        }
    } else if mag > 1 {
        let mag = mag as usize;
        for y in 0..h as usize {
            let mut at = y * mag * dest_pitch;
            for x in 0..(w / 4) as usize {
                for i in 0..4 {
                    let pix = pal32[src[y * src_pitch + x * 4 + i].get() as usize];
                    for _ in 0..mag {
                        for dy in 0..mag {
                            put(at + dy * dest_pitch, pix);
                        }
                        at += 4;
                    }
                }
            }
        }
    }
}

/// The largest whole magnification that fits a `scr_w` x `scr_h` screen in
/// `back_w` x `back_h`, and where to put it to centre it: `fitScreen`.
pub fn liero_gfx_fit_screen(
    back_w: i32,
    back_h: i32,
    scr_w: i32,
    scr_h: i32,
    offset_x: &mut i32,
    offset_y: &mut i32,
) -> i32 {
    let mut mag = 1;
    while scr_w * mag <= back_w && scr_h * mag <= back_h {
        mag += 1;
    }
    mag -= 1; // mag was the first that didn't fit

    *offset_x = back_w / 2 - scr_w * mag / 2;
    *offset_y = back_h / 2 - scr_h * mag / 2;
    mag
}

/// A palette: 256 colours.
pub type Palette = [Color; 256];

/// The VGA palette (0 to 63) as 8-bit colours: `Palette::activate`.
pub fn liero_palette_activate(pal: &Palette, real: &mut Palette) {
    for (r, c) in real.iter_mut().zip(pal) {
        r.r = c.r << 2;
        r.g = c.g << 2;
        r.b = c.b << 2;
    }
}

/// Applies `f` to every component of `pal`.
fn map_components(pal: &mut Palette, f: impl Fn(i32) -> i32) {
    for c in pal.iter_mut() {
        c.r = f(c.r as i32) as u8;
        c.g = f(c.g as i32) as u8;
        c.b = f(c.b as i32) as u8;
    }
}

/// Darkens the palette to `amount` / 32; 32 and more leave it:
/// `Palette::fade`.
pub fn liero_palette_fade(pal: &mut Palette, amount: i32) {
    if amount >= 32 {
        return;
    }
    map_components(pal, |v| ((v * amount) >> 5).max(0));
}

/// Brightens the palette towards white by `amount` / 32, for the screen
/// flash: `Palette::lightUp`.
pub fn liero_palette_light_up(pal: &mut Palette, amount: i32) {
    map_components(pal, |v| ((v * (32 - amount) + amount * 63) >> 5).min(63));
}

/// Sets entries `from` to `to` of `pal` to those of `source`, rotated by
/// `dist`: `Palette::rotateFrom`, for animated colours. Panics unless
/// 0 <= from <= to < 256.
pub fn liero_palette_rotate_from(pal: &mut Palette, source: &Palette, from: i32, to: i32, dist: u32) {
    // C++ mixed signed and unsigned here, so it worked in unsigned.
    let count = (to - from + 1) as u32;
    let dist = dist % count;

    for i in 0..count {
        pal[(from as u32 + i) as usize] = source[(from as u32 + (i + count - dist) % count) as usize];
    }
}

/// The palette a game shows (`Game::draw`): `origpal` with the TC's four
/// colour animations at `cycles`, faded by `fade` and lit up by the
/// screen flash.
pub fn liero_game_palette(
    pal: &mut Palette,
    origpal: &Palette,
    color_anim: &[(i32, i32); 4],
    cycles: i32,
    fade: i32,
    screen_flash: i32,
) {
    *pal = *origpal;
    for &(from, to) in color_anim {
        liero_palette_rotate_from(pal, origpal, from, to, (cycles >> 3) as u32);
    }
    liero_palette_fade(pal, fade);
    if screen_flash > 0 {
        liero_palette_light_up(pal, screen_flash);
    }
}

/// Worm `i`'s colours from its settings' colour index and `rgb`:
/// `Palette::setWormColour`. The settings keep `color` from 2 to 253;
/// entries outside the palette, which C++ wrote past it, are skipped, and
/// read as black.
pub fn liero_palette_set_worm_colour(pal: &mut Palette, i: i32, color: i32, rgb: &[i32; 3]) {
    const WORM_COLOUR_INDEXES: [i32; 2] = [0x58, 0x78];

    let mut scale_add = |dest: i32, scale: i32, add: i32| {
        if let Some(e) = usize::try_from(dest).ok().and_then(|d| pal.get_mut(d)) {
            e.r = ((add + rgb[0] * scale) / 64) as u8;
            e.g = ((add + rgb[1] * scale) / 64) as u8;
            e.b = ((add + rgb[2] * scale) / 64) as u8;
        }
    };

    scale_add(color - 2, 38, 0);
    scale_add(color - 1, 50, 0);
    scale_add(color, 64, 0);
    scale_add(color + 1, 47, 1008);
    scale_add(color + 2, 28, 2205);

    let at = |pal: &Palette, idx: i32| {
        usize::try_from(idx)
            .ok()
            .and_then(|i| pal.get(i))
            .copied()
            .unwrap_or_default()
    };
    let base = WORM_COLOUR_INDEXES[i as usize];
    for j in 0..6 {
        pal[(base + j) as usize] = at(pal, color + (j % 3) - 1);
    }
    for j in 0..3 {
        pal[(129 + i * 4 + j) as usize] = at(pal, color + j);
    }
}

/// Font functions. Text bytes 2 to 251 are characters 0 to 249, and 0
/// starts a new line.
///
/// Draws character `c` in `color`, `size` times as big: `Font::drawChar`.
/// Clipping is for the unscaled size, as in C++.
#[allow(clippy::too_many_arguments)]
pub fn liero_font_draw_char(scr: &Bitmap, chars: &[FontChar], c: u8, x: i32, y: i32, color: u8, size: i32) {
    // C++ checks the character after drawText subtracted 2. Keep that.
    if !(2..252).contains(&c) || c as usize >= chars.len() {
        return;
    }
    let mem = &chars[c as usize].data;
    let Some(cl) = clip_image(x, y, 7, 8, 7, scr.clip) else {
        return;
    };

    let mut dy = 0;
    for cy in 0..cl.height {
        for _ in 0..size {
            let mut dx = 0;
            for cx in 0..cl.width {
                let px = mem[(cl.src + (cy * 7 + cx) as isize) as usize];
                for _ in 0..size {
                    if px != 0 {
                        scr.set(cl.x + dx, cl.y + dy, color);
                    }
                    dx += 1;
                }
            }
            dy += 1;
        }
    }
}

/// Draws `text`: `Font::drawText`.
#[allow(clippy::too_many_arguments)]
pub fn liero_font_draw_text(
    scr: &Bitmap,
    chars: &[FontChar],
    text: &[u8],
    mut x: i32,
    mut y: i32,
    color: u8,
    size: i32,
) {
    let org_x = x;
    for &c in text {
        if c == 0 {
            x = org_x;
            y += 8 * size;
        } else if (2..252).contains(&c) {
            let c = c - 2;
            liero_font_draw_char(scr, chars, c, x, y, color, size);
            x += chars[c as usize].width as i32 * size;
        }
    }
}

/// The width of `text`, its widest line if there are several, and its
/// height: `Font::getDims`.
pub fn liero_font_get_dims(chars: &[FontChar], text: &[u8]) -> (i32, i32) {
    let (mut width, mut max_width, mut max_height) = (0, 0, 8);
    for &c in text {
        if (2..252).contains(&c) {
            width += chars[c as usize - 2].width as i32;
        } else if c == 0 {
            max_width = max_width.max(width);
            width = 0;
            max_height += 8;
        }
    }
    (max_width.max(width), max_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_games_palette_animates_fades_and_flashes() {
        let mut orig = [Color::default(); 256];
        for (i, c) in orig.iter_mut().enumerate() {
            *c = Color {
                r: (i % 64) as u8,
                g: 32,
                b: 0,
                unused: 0,
            };
        }
        let anims = [(10, 12), (20, 21), (0, 0), (0, 0)];
        let mut pal = [Color::default(); 256];
        // Tick 8 rotates each range by one.
        liero_game_palette(&mut pal, &orig, &anims, 8, 32, 0);
        assert_eq!(
            [pal[10].r, pal[11].r, pal[12].r, pal[20].r, pal[21].r, pal[30].r],
            [12, 10, 11, 21, 20, 30]
        );
        liero_game_palette(&mut pal, &orig, &anims, 0, 16, 0);
        assert_eq!((pal[30].r, pal[30].g), (15, 16));
        liero_game_palette(&mut pal, &orig, &anims, 0, 32, 16);
        assert_eq!((pal[30].r, pal[30].g), (46, 47));
    }

    struct Bmp {
        px: Vec<u8>,
        clip: Rect,
        pitch: i32,
    }

    impl Bmp {
        fn view(&mut self) -> Bitmap<'_> {
            let (w, h) = (self.pitch, self.px.len() as i32 / self.pitch);
            Bitmap {
                clip: self.clip,
                ..Bitmap::new(&mut self.px, w, h, self.pitch)
            }
        }
    }

    fn bmp(w: i32, h: i32) -> Bmp {
        Bmp {
            px: vec![0u8; (w * h) as usize],
            clip: Rect {
                x1: 0,
                y1: 0,
                x2: w,
                y2: h,
            },
            pitch: w,
        }
    }

    fn rows(b: &Bmp) -> Vec<String> {
        b.px.chunks(b.pitch as usize)
            .map(|r| {
                r.iter()
                    .map(|&p| if p == 0 { '.' } else { (b'0' + p % 10) as char })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fill_rect_clips() {
        let mut b = bmp(5, 4);
        b.clip = Rect {
            x1: 1,
            y1: 1,
            x2: 4,
            y2: 3,
        };
        liero_gfx_fill_rect(&b.view(), -2, 0, 10, 10, 7);
        assert_eq!(rows(&b), [".....", ".777.", ".777.", "....."]);
    }

    #[test]
    fn blit_image_skips_colour_0_and_clips() {
        let mut b = bmp(4, 3);
        let sprite = [1, 0, 2, 3, 4, 0];
        liero_gfx_blit_image(&b.view(), &sprite, -1, 1, 3, 2, 3);
        assert_eq!(rows(&b), ["....", ".2..", "4..."]);
        b.px.fill(0);
        liero_gfx_blit_image(&b.view(), &sprite, 2, 0, 3, 2, 3);
        assert_eq!(rows(&b), ["..1.", "..34", "...."]);
    }

    #[test]
    fn lines_skip_the_start_point() {
        let mut b = bmp(5, 3);
        liero_gfx_draw_line(&b.view(), 0, 0, 4, 2, 5);
        assert_eq!(rows(&b), [".5...", "..55.", "....5"]);
    }

    #[test]
    fn dashed_box_goes_round() {
        let mut b = bmp(4, 3);
        liero_gfx_draw_dashed_line_box(&b.view(), 0, 0, 1, 2, 1, 2, 4, 3, 0);
        // Perimeter 2 * (4 + 3) - 2 = 12; the first 6 points use colour 2.
        // Dashes: points 0, 1, 4, 5, 8, 9 are drawn.
        assert_eq!(rows(&b), ["211.", "2...", "..22"]);
    }

    #[test]
    fn fit_screen_centres() {
        let (mut x, mut y) = (0, 0);
        assert_eq!(liero_gfx_fit_screen(1000, 700, 320, 200, &mut x, &mut y), 3);
        assert_eq!((x, y), (500 - 480, 350 - 300));
    }

    #[test]
    fn palette_effects() {
        let mut pal = [Color {
            r: 63,
            g: 32,
            b: 0,
            unused: 0,
        }; 256];
        liero_palette_fade(&mut pal, 16);
        assert_eq!((pal[0].r, pal[0].g, pal[0].b), (31, 16, 0));
        liero_palette_light_up(&mut pal, 16);
        assert_eq!((pal[0].r, pal[0].g, pal[0].b), (47, 39, 31));

        let mut src = [Color::default(); 256];
        for (i, c) in src.iter_mut().enumerate() {
            c.r = i as u8;
        }
        let mut dst = src;
        liero_palette_rotate_from(&mut dst, &src, 10, 13, 5);
        assert_eq!(
            dst[9..15].iter().map(|c| c.r).collect::<Vec<_>>(),
            [9, 13, 10, 11, 12, 14]
        );
    }

    #[test]
    fn font_draws_and_measures() {
        let mut chars = vec![
            FontChar {
                data: [0; 56],
                width: 4
            };
            250
        ];
        chars[3].data[0] = 1; // top-left pixel of character 3, text byte 5
        chars[3].width = 2;
        let text = [5u8, 5, 0, 5];
        let mut b = bmp(8, 10);
        liero_font_draw_text(&b.view(), &chars, &text, 1, 0, 9, 1);
        assert_eq!(&rows(&b)[0], ".9.9....");
        assert_eq!(&rows(&b)[8], ".9......");
        assert_eq!(liero_font_get_dims(&chars, &text), (4, 16));
    }

    #[test]
    fn vline_and_boxes() {
        let mut b = bmp(3, 3);
        liero_gfx_vline(&b.view(), 1, -1, 2, 6);
        assert_eq!(rows(&b), [".6.", ".6.", "..."]);

        let mut b = bmp(5, 3);
        liero_gfx_draw_rounded_box(&b.view(), 0, 0, 3, 3, 1);
        assert_eq!(rows(&b), [".33..", "3333.", ".33.."]);

        let mut b = bmp(4, 3);
        liero_gfx_draw_rounded_line_box(&b.view(), 0, 0, 4, 4, 3);
        assert_eq!(rows(&b), [".44.", "4..4", ".44."]);
    }

    #[test]
    fn trans_blit_is_a_checkerboard() {
        let mut b = bmp(2, 2);
        let sprite = [5u8; 4];
        liero_gfx_blit_image_trans(&b.view(), &sprite, 0, 0, 2, 2, 2, 0);
        assert_eq!(rows(&b), [".5", "5."]);
    }

    #[test]
    fn graph_draws_bars_and_frame() {
        let mut b = bmp(6, 9);
        let data = [2.4, -1.6, 0.0];
        liero_gfx_draw_graph(&b.view(), &data, 6, 1, 1, 1, 2, false);
        let px = |x: usize, y: usize| b.px[y * 6 + x];
        // Bars from the base line at y 7: up 2 for 2.4, down 2 for -1.6.
        assert_eq!((px(1, 6), px(1, 5)), (1, 7)); // the frame covers y 5
        assert_eq!((px(2, 7), px(2, 8)), (2, 2));
        assert_eq!(px(3, 7), 0);
        // The frame: 3 wide, 6 high, corners cut.
        assert_eq!((px(2, 1), px(2, 6), px(1, 2), px(3, 5), px(1, 1)), (7, 7, 7, 7, 0));
    }

    #[test]
    fn heatmap_ranks_counts() {
        let mut b = bmp(4, 2);
        let map = [0, 5, 5, 9];
        liero_gfx_draw_heatmap(&b.view(), 1, 0, &map, 2, 2);
        // 5 covers 2 of 3 cells, so 9 starts 2/3 of the way up 16 colours.
        assert_eq!(b.px, [0, 0, 104, 0, 0, 104, 114, 0]);
    }

    #[test]
    fn scale_draw_repeats_pixels() {
        let mut src = [1u8, 2, 3, 4];
        let src = Cell::from_mut(&mut src[..]).as_slice_of_cells();
        let pal: [u32; 256] = std::array::from_fn(|i| i as u32 * 0x010101);
        let words = |b: &[u8]| -> Vec<u32> { b.chunks(4).map(|c| u32::from_ne_bytes(c.try_into().unwrap())).collect() };

        let mut dest = vec![0u8; 16];
        liero_gfx_scale_draw(src, 4, 1, 4, &mut dest, 16, 1, &pal);
        let dest = words(&dest);
        assert_eq!(dest, [0x010101, 0x020202, 0x030303, 0x040404]);

        let mut dest = vec![0u8; 64];
        liero_gfx_scale_draw(src, 4, 1, 4, &mut dest, 32, 2, &pal);
        let dest = words(&dest);
        let row: Vec<u32> = [1, 1, 2, 2, 3, 3, 4, 4].iter().map(|&i| i * 0x010101).collect();
        assert_eq!(&dest[..8], &row[..]);
        assert_eq!(&dest[8..], &row[..]);
    }

    #[test]
    fn palette_to_screen_colours() {
        let mut pal = [Color::default(); 256];
        pal[7] = Color {
            r: 63,
            g: 1,
            b: 0,
            unused: 0,
        };
        let mut real = [Color::default(); 256];
        liero_palette_activate(&pal, &mut real);
        assert_eq!((real[7].r, real[7].g, real[7].b), (252, 4, 0));
    }
}
