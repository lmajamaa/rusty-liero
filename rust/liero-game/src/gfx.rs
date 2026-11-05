//! Drawing: the 8-bit screen bitmap's primitives (`gfx/blit.cpp`), palette
//! effects (`gfx/palette.cpp`) and the bitmap font (`gfx/font.cpp`). Frames
//! have to stay pixel-identical to the C++ versions, which `tools/hashrun`
//! checks by drawing the viewports every 100 ticks.
//!
//! C++ still owns the bitmaps, sprites, palettes and font. Rust gets views
//! of them for each call. Sprite and source images come as raw pointers
//! without lengths, as they did in C++; writes to the screen are kept
//! inside its buffer.

use std::collections::BTreeMap;

use crate::game::LieroRect;
use crate::material::LieroMaterial;
use crate::rand::LieroRand;

/// View of a C++ `Bitmap`: `h` rows of `pitch` bytes, of which the first
/// `w` are the image. Drawing stays inside `clip` (x2 and y2 excluded),
/// except where the C++ didn't clip either.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroBitmap {
    pub pixels: *mut u8,
    pub w: i32,
    pub h: i32,
    pub pitch: i32,
    pub clip: LieroRect,
}

/// A palette entry, laid out like C++'s `Color`. Components are 0 to 63 in
/// `Palette`, 0 to 255 once activated.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LieroColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub unused: u8,
}

/// One character of the bitmap font, laid out like `Font::Char`: 7 x 8
/// pixels, non-zero where the character is, and how far to move after it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroFontChar {
    pub data: [u8; 56],
    pub width: i32,
}

/// The screen as Rust draws on it.
struct Screen<'a> {
    px: &'a mut [u8],
    pitch: i32,
    clip: LieroRect,
}

impl LieroBitmap {
    /// # Safety
    /// `pixels` must point to `pitch * h` bytes that nothing else uses
    /// during the call.
    unsafe fn get(&self) -> Screen<'_> {
        let len = self.pitch.max(0) as usize * self.h.max(0) as usize;
        let px = if len == 0 { &mut [][..] } else { std::slice::from_raw_parts_mut(self.pixels, len) };
        Screen { px, pitch: self.pitch, clip: self.clip }
    }
}

impl Screen<'_> {
    /// Index of (x, y) as C++'s `getPixel` computed it, or None outside the
    /// buffer, where C++ would have written to other memory.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let i = (y as i64) * (self.pitch as i64) + x as i64;
        (0 <= i && (i as usize) < self.px.len()).then_some(i as usize)
    }

    fn get(&self, x: i32, y: i32) -> u8 {
        self.index(x, y).map_or(0, |i| self.px[i])
    }

    fn set(&mut self, x: i32, y: i32, c: u8) {
        if let Some(i) = self.index(x, y) {
            self.px[i] = c;
        }
    }

    /// `memset` of `n` pixels from (x, y).
    fn span(&mut self, x: i32, y: i32, n: i32, c: u8) {
        for i in 0..n.max(0) {
            self.set(x + i, y, c);
        }
    }
}

impl LieroRect {
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

fn clip_image(mut x: i32, mut y: i32, mut width: i32, mut height: i32, pitch: i32, clip: LieroRect) -> Option<Clipped> {
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
    Some(Clipped { x, y, width, height, src })
}

/// Draws a `width` x `height` image from `mem` (rows `pitch` apart) at
/// (x, y), clipped. `f` gets the source and screen pixel, the position in
/// the visible part, and returns the new screen pixel, if any.
///
/// # Safety
/// `mem` must hold the image.
unsafe fn blit(
    scr: &mut Screen,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
    mut f: impl FnMut(u8, u8, i32, i32) -> Option<u8>,
) {
    let Some(c) = clip_image(x, y, width, height, pitch, scr.clip) else { return };
    let mem = mem.offset(c.src);

    for row in 0..c.height {
        let src = mem.offset(row as isize * pitch as isize);
        for col in 0..c.width {
            let (sx, sy) = (c.x + col, c.y + row);
            if let Some(n) = f(*src.offset(col as isize), scr.get(sx, sy), col, row) {
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

// The functions C++ calls. In each, `scr` is a view as documented on
// `LieroBitmap`, valid for the call, and images are as documented on the
// function.

/// Fills a rectangle, clipped: `fillRect`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_fill_rect(scr: &LieroBitmap, x: i32, y: i32, w: i32, h: i32, color: u8) {
    let mut s = scr.get();
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
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_fill(scr: &LieroBitmap, color: u8) {
    scr.get().px.fill(color);
}

/// A bar `width` long and `height` high, not clipped: `drawBar`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_bar(scr: &LieroBitmap, x: i32, y: i32, width: i32, height: i32, color: u8) {
    let mut s = scr.get();
    for h in 0..height {
        if width > 0 {
            s.span(x, y + h, width, color);
        }
    }
}

/// A vertical line from y1 to y2 (excluded), clipped: `vline`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_vline(scr: &LieroBitmap, x: i32, y1: i32, y2: i32, color: u8) {
    let mut s = scr.get();
    if x < s.clip.x1 || x >= s.clip.x2 {
        return;
    }
    for y in y1.max(s.clip.y1)..y2.min(s.clip.y2) {
        s.set(x, y, color);
    }
}

/// A filled box with cut corners: `drawRoundedBox`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_rounded_box(scr: &LieroBitmap, x: i32, y: i32, color: u8, height: i32, width: i32) {
    liero_gfx_fill_rect(scr, x, y + 1, width + 3, height - 2, color);
    liero_gfx_fill_rect(scr, x + 1, y, width + 1, 1, color);
    liero_gfx_fill_rect(scr, x + 1, y + height - 1, width + 1, 1, color);
}

/// The outline of a box with cut corners: `drawRoundedLineBox`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_rounded_line_box(
    scr: &LieroBitmap,
    x: i32,
    y: i32,
    color: u8,
    width: i32,
    height: i32,
) {
    liero_gfx_fill_rect(scr, x + 1, y, width - 2, 1, color);
    liero_gfx_fill_rect(scr, x + 1, y + height - 1, width - 2, 1, color);
    liero_gfx_fill_rect(scr, x, y + 1, 1, height - 2, color);
    liero_gfx_fill_rect(scr, x + width - 1, y + 1, 1, height - 2, color);
}

/// A dashed outline, going round anticlockwise from the top-left corner,
/// in `color2` for the first `num / den` of the way and `color` for the
/// rest: `drawDashedLineBox`, which shows Holdazone's zone and how far it
/// is taken.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_dashed_line_box(
    scr: &LieroBitmap,
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
    let mut s = scr.get();
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
///
/// # Safety
/// See `LieroBitmap`; `mem` holds `height` rows `pitch` apart.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_image_no_key_colour(
    scr: &LieroBitmap,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
) {
    blit(&mut scr.get(), mem, x, y, width, height, pitch, |c, _, _, _| Some(c));
}

/// Draws a sprite, colour 0 transparent: `blitImage`.
///
/// # Safety
/// See `LieroBitmap`; `mem` holds `height` rows `pitch` apart.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_image(
    scr: &LieroBitmap,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
) {
    blit(&mut scr.get(), mem, x, y, width, height, pitch, |c, _, _, _| (c != 0).then_some(c));
}

/// Draws every other pixel of a sprite, in a checkerboard that `phase`
/// shifts: `blitImageTrans`, for worms flickering at respawn. The pattern
/// follows the visible part, as in C++.
///
/// # Safety
/// See `LieroBitmap`; `mem` holds `height` rows `pitch` apart.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_image_trans(
    scr: &LieroBitmap,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: i32,
    phase: i32,
) {
    blit(&mut scr.get(), mem, x, y, width, height, pitch, |c, _, col, row| {
        (c != 0 && (col ^ row ^ phase) & 1 != 0).then_some(c)
    });
}

/// Draws a sprite only over colours 160 to 167: `blitImageR`, for
/// bonuses showing through the fog.
///
/// # Safety
/// See `LieroBitmap`; `mem` holds `height` rows `width` apart.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_image_r(
    scr: &LieroBitmap,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    blit(&mut scr.get(), mem, x, y, width, height, width, |c, d, _, _| {
        (c != 0 && d.wrapping_sub(160) < 8).then_some(c)
    });
}

/// Draws the 16 x 16 fire cone sprite, darker as `fc` counts down:
/// `blitFireCone`.
///
/// # Safety
/// See `LieroBitmap`; `mem` holds 16 x 16 pixels.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_fire_cone(scr: &LieroBitmap, fc: i32, mem: *const u8, x: i32, y: i32) {
    blit(&mut scr.get(), mem, x, y, 16, 16, 16, |c, _, _, _| match fc {
        0 => (c > 116).then(|| c - 5),
        1 => (c > 114).then(|| c - 3),
        2 => (c > 112).then(|| c - 1),
        _ => (c != 0).then_some(c),
    });
}

/// Darkens the screen under a sprite's non-zero pixels where the material
/// shows shadows: `blitShadowImage`.
///
/// # Safety
/// See `LieroBitmap`; `mem` holds `height` rows `width` apart, and
/// `materials` has 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_blit_shadow_image(
    scr: &LieroBitmap,
    materials: *const LieroMaterial,
    mem: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    let materials = &*(materials as *const [LieroMaterial; 256]);
    blit(&mut scr.get(), mem, x, y, width, height, width, |c, d, _, _| {
        (c != 0 && materials[d as usize].see_shadow()).then(|| d.wrapping_add(4))
    });
}

/// The ninja rope, cycling through colours `begin` to `end` (excluded):
/// `drawNinjarope`.
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_ninjarope(
    scr: &LieroBitmap,
    begin: i32,
    end: i32,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
) {
    let mut s = scr.get();
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
///
/// # Safety
/// See `LieroBitmap`; `rand` must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_laser_sight(
    scr: &LieroBitmap,
    rand: &mut LieroRand,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
) {
    let mut s = scr.get();

    line(from_x, from_y, to_x, to_y, |cx, cy| {
        if rand.below(5) == 0 && s.clip.contains(cx, cy) {
            s.set(cx, cy, rand.below(2) as u8 + 83);
        }
    });
}

/// The ninja rope's shadow: `drawShadowLine`.
///
/// # Safety
/// See `LieroBitmap`; `materials` has 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_shadow_line(
    scr: &LieroBitmap,
    materials: *const LieroMaterial,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
) {
    let materials = &*(materials as *const [LieroMaterial; 256]);
    let mut s = scr.get();

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
///
/// # Safety
/// See `LieroBitmap`.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_line(
    scr: &LieroBitmap,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    color: u8,
) {
    let mut s = scr.get();
    line(from_x, from_y, to_x, to_y, |cx, cy| {
        if s.clip.contains(cx, cy) {
            s.set(cx, cy, color);
        }
    });
}

/// A bar graph of `len` values in a framed box `height` high, from the
/// bottom or, when `balanced`, from the middle: `drawGraph`.
///
/// # Safety
/// See `LieroBitmap`; `data` holds `len` values.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_graph(
    scr: &LieroBitmap,
    data: *const f64,
    len: usize,
    height: i32,
    start_x: i32,
    start_y: i32,
    color: u8,
    neg_color: u8,
    balanced: bool,
) {
    let data = if len == 0 { &[][..] } else { std::slice::from_raw_parts(data, len) };
    let base_y = start_y + if balanced { height / 2 } else { height };

    for (x, &v) in (start_x..).zip(data) {
        let mut y1 = base_y - (v + 0.5).floor() as i32;
        let mut y2 = base_y;
        if y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        liero_gfx_vline(scr, x, y1, y2, if v >= 0.0 { color } else { neg_color });
    }

    liero_gfx_draw_rounded_line_box(scr, start_x, start_y, 7, len as i32, height);
}

/// A heatmap of `width` x `height` counts, in colours 104 to 119 by rank,
/// so each colour covers about as many pixels: `drawHeatmap`.
///
/// # Safety
/// See `LieroBitmap`; `map` holds `width * height` counts.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_draw_heatmap(
    scr: &LieroBitmap,
    x: i32,
    y: i32,
    map: *const i32,
    width: i32,
    height: i32,
) {
    let n = width.max(0) as usize * height.max(0) as usize;
    let cells = if n == 0 { &[][..] } else { std::slice::from_raw_parts(map, n) };

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

    let Some(c) = clip_image(x, y, width, height, width, scr.clip) else { return };
    let mut s = scr.get();
    for row in 0..c.height {
        for col in 0..c.width {
            let v = cells[(c.src + (row * width + col) as isize) as usize];
            s.set(c.x + col, c.y + row, mapping[&v] as u8);
        }
    }
}

/// Converts `w` x `h` palette indices (rows `src_pitch` apart) to 32-bit
/// pixels with `pal32`, each `mag` x `mag` pixels big: `scaleDraw`. Above
/// 1x, only whole groups of 4 pixels in a row are drawn, as in C++.
///
/// # Safety
/// `src` holds the image, `dest` `h * mag` rows `dest_pitch` bytes apart
/// with room for `w * mag` pixels, and `pal32` 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_scale_draw(
    src: *const u8,
    w: i32,
    h: i32,
    src_pitch: usize,
    dest: *mut u8,
    dest_pitch: usize,
    mag: i32,
    pal32: *const u32,
) {
    let pal = &*(pal32 as *const [u32; 256]);

    if mag == 1 {
        for y in 0..h as usize {
            let line = src.add(y * src_pitch);
            let dest_line = dest.add(y * dest_pitch) as *mut u32;
            for x in 0..w as usize {
                dest_line.add(x).write_unaligned(pal[*line.add(x) as usize]);
            }
        }
    } else if mag > 1 {
        let mag = mag as usize;
        for y in 0..h as usize {
            let line = src.add(y * src_pitch);
            let mut dest_line = dest.add(y * mag * dest_pitch);
            for x in 0..(w / 4) as usize {
                for i in 0..4 {
                    let pix = pal[*line.add(x * 4 + i) as usize];
                    for _ in 0..mag {
                        for dy in 0..mag {
                            (dest_line.add(dy * dest_pitch) as *mut u32).write_unaligned(pix);
                        }
                        dest_line = dest_line.add(4);
                    }
                }
            }
        }
    }
}

/// 32-bit pixels for an activated palette: `preparePaletteBgra`.
///
/// # Safety
/// Both hold 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_gfx_prepare_palette_bgra(pal: *const LieroColor, pal32: *mut u32) {
    for i in 0..256 {
        let c = *pal.add(i);
        *pal32.add(i) = (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32;
    }
}

/// The largest whole magnification that fits a `scr_w` x `scr_h` screen in
/// `back_w` x `back_h`, and where to put it to centre it: `fitScreen`.
#[no_mangle]
pub extern "C" fn liero_gfx_fit_screen(
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

/// Palette functions. Each palette is 256 entries.
///
/// The VGA palette (0 to 63) as 8-bit colours: `Palette::activate`.
///
/// # Safety
/// Both hold 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_palette_activate(pal: *const LieroColor, real: *mut LieroColor) {
    for i in 0..256 {
        let c = *pal.add(i);
        let r = &mut *real.add(i);
        r.r = c.r << 2;
        r.g = c.g << 2;
        r.b = c.b << 2;
    }
}

/// Applies `f` to every component of `pal`.
unsafe fn map_components(pal: *mut LieroColor, f: impl Fn(i32) -> i32) {
    for i in 0..256 {
        let c = &mut *pal.add(i);
        c.r = f(c.r as i32) as u8;
        c.g = f(c.g as i32) as u8;
        c.b = f(c.b as i32) as u8;
    }
}

/// Darkens the palette to `amount` / 32; 32 and more leave it:
/// `Palette::fade`.
///
/// # Safety
/// `pal` holds 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_palette_fade(pal: *mut LieroColor, amount: i32) {
    if amount >= 32 {
        return;
    }
    map_components(pal, |v| ((v * amount) >> 5).max(0));
}

/// Brightens the palette towards white by `amount` / 32, for the screen
/// flash: `Palette::lightUp`.
///
/// # Safety
/// `pal` holds 256 entries.
#[no_mangle]
pub unsafe extern "C" fn liero_palette_light_up(pal: *mut LieroColor, amount: i32) {
    map_components(pal, |v| ((v * (32 - amount) + amount * 63) >> 5).min(63));
}

/// Sets entries `from` to `to` of `pal` to those of `source`, rotated by
/// `dist`: `Palette::rotateFrom`, for animated colours.
///
/// # Safety
/// Both hold 256 entries, and 0 <= from <= to < 256.
#[no_mangle]
pub unsafe extern "C" fn liero_palette_rotate_from(
    pal: *mut LieroColor,
    source: *const LieroColor,
    from: i32,
    to: i32,
    dist: u32,
) {
    // C++ mixed signed and unsigned here, so it worked in unsigned.
    let count = (to - from + 1) as u32;
    let dist = dist % count;

    for i in 0..count {
        *pal.add((from as u32 + i) as usize) = *source.add((from as u32 + (i + count - dist) % count) as usize);
    }
}

/// Worm `i`'s colours from its settings' colour index and `rgb`:
/// `Palette::setWormColour`.
///
/// # Safety
/// `pal` holds 256 entries, and `color` is at least 2 and at most 253.
#[no_mangle]
pub unsafe extern "C" fn liero_palette_set_worm_colour(pal: *mut LieroColor, i: i32, color: i32, rgb: &[i32; 3]) {
    const WORM_COLOUR_INDEXES: [i32; 2] = [0x58, 0x78];

    let entry = |idx: i32| &mut *pal.add(idx as usize);
    let scale_add = |dest: i32, scale: i32, add: i32| {
        let e = entry(dest);
        e.r = ((add + rgb[0] * scale) / 64) as u8;
        e.g = ((add + rgb[1] * scale) / 64) as u8;
        e.b = ((add + rgb[2] * scale) / 64) as u8;
    };

    scale_add(color - 2, 38, 0);
    scale_add(color - 1, 50, 0);
    scale_add(color, 64, 0);
    scale_add(color + 1, 47, 1008);
    scale_add(color + 2, 28, 2205);

    let base = WORM_COLOUR_INDEXES[i as usize];
    for j in 0..6 {
        *entry(base + j) = *entry(color + (j % 3) - 1);
    }
    for j in 0..3 {
        *entry(129 + i * 4 + j) = *entry(color + j);
    }
}

/// Font functions. `chars` holds `count` characters; text bytes 2 to 251
/// are characters 0 to 249, and 0 starts a new line.
///
/// Draws character `c` in `color`, `size` times as big: `Font::drawChar`.
/// Clipping is for the unscaled size, as in C++.
///
/// # Safety
/// See `LieroBitmap`; `chars` holds `count` characters.
#[no_mangle]
pub unsafe extern "C" fn liero_font_draw_char(
    scr: &LieroBitmap,
    chars: *const LieroFontChar,
    count: usize,
    c: u8,
    x: i32,
    y: i32,
    color: u8,
    size: i32,
) {
    // C++ checks the character after drawText subtracted 2. Keep that.
    if !(2..252).contains(&c) || c as usize >= count {
        return;
    }
    let mem = (*chars.add(c as usize)).data.as_ptr();
    let mut s = scr.get();
    let Some(cl) = clip_image(x, y, 7, 8, 7, s.clip) else { return };
    let mem = mem.offset(cl.src);

    let mut dy = 0;
    for cy in 0..cl.height {
        for _ in 0..size {
            let mut dx = 0;
            for cx in 0..cl.width {
                let px = *mem.offset((cy * 7 + cx) as isize);
                for _ in 0..size {
                    if px != 0 {
                        s.set(cl.x + dx, cl.y + dy, color);
                    }
                    dx += 1;
                }
            }
            dy += 1;
        }
    }
}

/// Draws `len` bytes of text: `Font::drawText`.
///
/// # Safety
/// See `liero_font_draw_char`; `text` holds `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_font_draw_text(
    scr: &LieroBitmap,
    chars: *const LieroFontChar,
    count: usize,
    text: *const u8,
    len: usize,
    mut x: i32,
    mut y: i32,
    color: u8,
    size: i32,
) {
    let text = if len == 0 { &[][..] } else { std::slice::from_raw_parts(text, len) };
    let org_x = x;

    for &c in text {
        if c == 0 {
            x = org_x;
            y += 8 * size;
        } else if (2..252).contains(&c) {
            let c = c - 2;
            liero_font_draw_char(scr, chars, count, c, x, y, color, size);
            x += (*chars.add(c as usize)).width * size;
        }
    }
}

/// The width of `len` bytes of text, its widest line if there are several,
/// and its height in `height` unless null: `Font::getDims`.
///
/// # Safety
/// `chars` holds `count` characters and `text` `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn liero_font_get_dims(
    chars: *const LieroFontChar,
    count: usize,
    text: *const u8,
    len: usize,
    height: *mut i32,
) -> i32 {
    let text = if len == 0 { &[][..] } else { std::slice::from_raw_parts(text, len) };
    let chars = std::slice::from_raw_parts(chars, count);
    let (mut width, mut max_width, mut max_height) = (0, 0, 8);

    for &c in text {
        if (2..252).contains(&c) {
            width += chars[c as usize - 2].width;
        } else if c == 0 {
            max_width = max_width.max(width);
            width = 0;
            max_height += 8;
        }
    }

    if !height.is_null() {
        *height = max_height;
    }
    max_width.max(width)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bmp {
        px: Vec<u8>,
        view: LieroBitmap,
    }

    fn bmp(w: i32, h: i32) -> Bmp {
        let mut px = vec![0u8; (w * h) as usize];
        let view = LieroBitmap {
            pixels: px.as_mut_ptr(),
            w,
            h,
            pitch: w,
            clip: LieroRect { x1: 0, y1: 0, x2: w, y2: h },
        };
        Bmp { px, view }
    }

    fn rows(b: &Bmp) -> Vec<String> {
        b.px.chunks(b.view.pitch as usize)
            .map(|r| r.iter().map(|&p| if p == 0 { '.' } else { (b'0' + p % 10) as char }).collect())
            .collect()
    }

    #[test]
    fn fill_rect_clips() {
        let mut b = bmp(5, 4);
        b.view.clip = LieroRect { x1: 1, y1: 1, x2: 4, y2: 3 };
        unsafe { liero_gfx_fill_rect(&b.view, -2, 0, 10, 10, 7) };
        assert_eq!(rows(&b), [".....", ".777.", ".777.", "....."]);
    }

    #[test]
    fn blit_image_skips_colour_0_and_clips() {
        let mut b = bmp(4, 3);
        let sprite = [1, 0, 2, 3, 4, 0];
        unsafe { liero_gfx_blit_image(&b.view, sprite.as_ptr(), -1, 1, 3, 2, 3) };
        assert_eq!(rows(&b), ["....", ".2..", "4..."]);
        b.px.fill(0);
        unsafe { liero_gfx_blit_image(&b.view, sprite.as_ptr(), 2, 0, 3, 2, 3) };
        assert_eq!(rows(&b), ["..1.", "..34", "...."]);
    }

    #[test]
    fn lines_skip_the_start_point() {
        let b = bmp(5, 3);
        unsafe { liero_gfx_draw_line(&b.view, 0, 0, 4, 2, 5) };
        assert_eq!(rows(&b), [".5...", "..55.", "....5"]);
    }

    #[test]
    fn dashed_box_goes_round() {
        let b = bmp(4, 3);
        unsafe { liero_gfx_draw_dashed_line_box(&b.view, 0, 0, 1, 2, 1, 2, 4, 3, 0) };
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
        let mut pal = [LieroColor { r: 63, g: 32, b: 0, unused: 0 }; 256];
        unsafe { liero_palette_fade(pal.as_mut_ptr(), 16) };
        assert_eq!((pal[0].r, pal[0].g, pal[0].b), (31, 16, 0));
        unsafe { liero_palette_light_up(pal.as_mut_ptr(), 16) };
        assert_eq!((pal[0].r, pal[0].g, pal[0].b), (47, 39, 31));

        let mut src = [LieroColor::default(); 256];
        for (i, c) in src.iter_mut().enumerate() {
            c.r = i as u8;
        }
        let mut dst = src;
        unsafe { liero_palette_rotate_from(dst.as_mut_ptr(), src.as_ptr(), 10, 13, 5) };
        assert_eq!(dst[9..15].iter().map(|c| c.r).collect::<Vec<_>>(), [9, 13, 10, 11, 12, 14]);
    }

    #[test]
    fn font_draws_and_measures() {
        let mut chars = vec![LieroFontChar { data: [0; 56], width: 4 }; 250];
        chars[3].data[0] = 1; // top-left pixel of character 3, text byte 5
        chars[3].width = 2;
        let text = [5u8, 5, 0, 5];
        let b = bmp(8, 10);
        unsafe { liero_font_draw_text(&b.view, chars.as_ptr(), chars.len(), text.as_ptr(), 4, 1, 0, 9, 1) };
        assert_eq!(&rows(&b)[0], ".9.9....");
        assert_eq!(&rows(&b)[8], ".9......");
        let mut h = 0;
        assert_eq!(unsafe { liero_font_get_dims(chars.as_ptr(), chars.len(), text.as_ptr(), 4, &mut h) }, 4);
        assert_eq!(h, 16);
    }

    #[test]
    fn vline_and_boxes() {
        let b = bmp(3, 3);
        unsafe { liero_gfx_vline(&b.view, 1, -1, 2, 6) };
        assert_eq!(rows(&b), [".6.", ".6.", "..."]);

        let b = bmp(5, 3);
        unsafe { liero_gfx_draw_rounded_box(&b.view, 0, 0, 3, 3, 1) };
        assert_eq!(rows(&b), [".33..", "3333.", ".33.."]);

        let b = bmp(4, 3);
        unsafe { liero_gfx_draw_rounded_line_box(&b.view, 0, 0, 4, 4, 3) };
        assert_eq!(rows(&b), [".44.", "4..4", ".44."]);
    }

    #[test]
    fn trans_blit_is_a_checkerboard() {
        let b = bmp(2, 2);
        let sprite = [5u8; 4];
        unsafe { liero_gfx_blit_image_trans(&b.view, sprite.as_ptr(), 0, 0, 2, 2, 2, 0) };
        assert_eq!(rows(&b), [".5", "5."]);
    }

    #[test]
    fn graph_draws_bars_and_frame() {
        let b = bmp(6, 9);
        let data = [2.4, -1.6, 0.0];
        unsafe { liero_gfx_draw_graph(&b.view, data.as_ptr(), 3, 6, 1, 1, 1, 2, false) };
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
        let b = bmp(4, 2);
        let map = [0, 5, 5, 9];
        unsafe { liero_gfx_draw_heatmap(&b.view, 1, 0, map.as_ptr(), 2, 2) };
        // 5 covers 2 of 3 cells, so 9 starts 2/3 of the way up 16 colours.
        assert_eq!(b.px, [0, 0, 104, 0, 0, 104, 114, 0]);
    }

    #[test]
    fn scale_draw_repeats_pixels() {
        let src = [1u8, 2, 3, 4];
        let pal: Vec<u32> = (0..256).map(|i| i * 0x010101).collect();

        let mut dest = vec![0u32; 4];
        unsafe { liero_gfx_scale_draw(src.as_ptr(), 4, 1, 4, dest.as_mut_ptr().cast(), 16, 1, pal.as_ptr()) };
        assert_eq!(dest, [0x010101, 0x020202, 0x030303, 0x040404]);

        let mut dest = vec![0u32; 16];
        unsafe { liero_gfx_scale_draw(src.as_ptr(), 4, 1, 4, dest.as_mut_ptr().cast(), 32, 2, pal.as_ptr()) };
        let row: Vec<u32> = [1, 1, 2, 2, 3, 3, 4, 4].iter().map(|&i| i * 0x010101).collect();
        assert_eq!(&dest[..8], &row[..]);
        assert_eq!(&dest[8..], &row[..]);
    }

    #[test]
    fn palette_to_screen_colours() {
        let mut pal = [LieroColor::default(); 256];
        pal[7] = LieroColor { r: 63, g: 1, b: 0, unused: 0 };
        let mut real = [LieroColor::default(); 256];
        let mut pal32 = [0u32; 256];
        unsafe {
            liero_palette_activate(pal.as_ptr(), real.as_mut_ptr());
            liero_gfx_prepare_palette_bgra(real.as_ptr(), pal32.as_mut_ptr());
        }
        assert_eq!((real[7].r, real[7].g, real[7].b), (252, 4, 0));
        assert_eq!(pal32[7], 0xfc0400);
    }
}
