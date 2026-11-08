//! The screens the game draws on: bitmaps (C++'s `Bitmap`) and renderers,
//! a bitmap with its palettes and fade (C++'s `Renderer`). Rust owns them,
//! and the C++ types are views of them.

use crate::game::LieroRect;
use crate::gfx::{liero_gfx_fill, LieroBitmap, LieroColor};
use crate::tc::LieroTcData;

/// A bitmap that owns its pixels, which `view` points to.
pub struct LieroBitmapBox {
    pixels: Vec<u8>,
    view: LieroBitmap,
}

impl LieroBitmapBox {
    /// No pixels, as C++'s `Bitmap()` left it.
    pub fn new() -> Box<Self> {
        let rect = LieroRect { x1: 0, y1: 0, x2: 0, y2: 0 };
        Box::new(LieroBitmapBox {
            pixels: Vec::new(),
            view: LieroBitmap { pixels: std::ptr::null_mut(), w: 0, h: 0, pitch: 0, clip: rect },
        })
    }

    pub fn view(&self) -> LieroBitmap {
        self.view
    }

    /// `Bitmap::alloc`: pixels for `w` x `h` with rows `pitch` apart, kept
    /// if the size is the same, and the clip rectangle reset to the whole.
    /// New pixels are zero, where C++ left them uninitialized.
    pub fn alloc(&mut self, w: i32, h: i32, pitch: i32) {
        let v = &mut self.view;
        if v.pixels.is_null() || v.w != w || v.h != h || v.pitch != pitch {
            self.pixels = vec![0; pitch.max(0) as usize * h.max(0) as usize];
            v.pixels = self.pixels.as_mut_ptr();
            (v.w, v.h, v.pitch) = (w, h, pitch);
        }
        v.clip = LieroRect { x1: 0, y1: 0, x2: w, y2: h };
    }

    /// `Bitmap::copy`: the same size and pixels as `other`.
    pub fn copy_from(&mut self, other: &LieroBitmapBox) {
        let o = other.view;
        self.alloc(o.w, o.h, o.pitch);
        self.pixels.copy_from_slice(&other.pixels);
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[no_mangle]
pub extern "C" fn liero_bitmap_box_new() -> *mut LieroBitmapBox {
    Box::into_raw(LieroBitmapBox::new())
}

/// # Safety
/// `b` comes from `liero_bitmap_box_new`, or is null, and isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_bitmap_box_free(b: *mut LieroBitmapBox) {
    if !b.is_null() {
        drop(Box::from_raw(b));
    }
}

/// The bitmap's fields, at a fixed address. Its pixels move when it's
/// reallocated.
#[no_mangle]
pub extern "C" fn liero_bitmap_box_view(b: &mut LieroBitmapBox) -> *mut LieroBitmap {
    &mut b.view
}

#[no_mangle]
pub extern "C" fn liero_bitmap_box_alloc(b: &mut LieroBitmapBox, w: i32, h: i32, pitch: i32) {
    b.alloc(w, h, pitch);
}

/// # Safety
/// Both are valid; they may be the same.
#[no_mangle]
pub unsafe extern "C" fn liero_bitmap_box_copy(to: *mut LieroBitmapBox, from: *const LieroBitmapBox) {
    if !std::ptr::eq(to, from) {
        (*to).copy_from(&*from);
    }
}

/// A renderer's palettes and size, which C++'s `Renderer` refers to.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LieroRendererData {
    /// The palette shown.
    pub pal: [LieroColor; 256],
    /// The palette `pal` is made from.
    pub origpal: [LieroColor; 256],
    pub fade_value: i32,
    /// The size drawn at, which `set_render_resolution` sets.
    pub render_res_x: i32,
    pub render_res_y: i32,
}

/// A bitmap to draw on, with its palettes.
pub struct LieroRenderer {
    pub data: LieroRendererData,
    pub bmp: Box<LieroBitmapBox>,
}

impl LieroRenderer {
    pub fn new() -> Box<Self> {
        Box::new(LieroRenderer {
            data: LieroRendererData {
                pal: [LieroColor::default(); 256],
                origpal: [LieroColor::default(); 256],
                fade_value: 0,
                render_res_x: 320,
                render_res_y: 200,
            },
            bmp: LieroBitmapBox::new(),
        })
    }

    /// `setRenderResolution`.
    pub fn set_render_resolution(&mut self, x: i32, y: i32) {
        (self.data.render_res_x, self.data.render_res_y) = (x, y);
        self.bmp.alloc(x, y, x);
    }

    /// `clear`: the bitmap filled with colour 0.
    pub fn clear(&mut self) {
        // SAFETY: the view is the box's own.
        unsafe { liero_gfx_fill(&self.bmp.view(), 0) };
    }

    /// `loadPalette`: both palettes from the TC's.
    pub fn load_palette(&mut self, tc: &LieroTcData) {
        self.data.origpal = tc.palette_colors;
        self.data.pal = self.data.origpal;
    }
}

#[no_mangle]
pub extern "C" fn liero_renderer_new() -> *mut LieroRenderer {
    Box::into_raw(LieroRenderer::new())
}

/// # Safety
/// `r` comes from `liero_renderer_new`, or is null, and isn't used again.
#[no_mangle]
pub unsafe extern "C" fn liero_renderer_free(r: *mut LieroRenderer) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

#[no_mangle]
pub extern "C" fn liero_renderer_data(r: &mut LieroRenderer) -> *mut LieroRendererData {
    &mut r.data
}

#[no_mangle]
pub extern "C" fn liero_renderer_bitmap(r: &mut LieroRenderer) -> *mut LieroBitmapBox {
    &mut *r.bmp
}

#[no_mangle]
pub extern "C" fn liero_renderer_set_render_resolution(r: &mut LieroRenderer, x: i32, y: i32) {
    r.set_render_resolution(x, y);
}

#[no_mangle]
pub extern "C" fn liero_renderer_clear(r: &mut LieroRenderer) {
    r.clear();
}

#[no_mangle]
pub extern "C" fn liero_renderer_load_palette(r: &mut LieroRenderer, tc: &LieroTcData) {
    r.load_palette(tc);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bitmap_keeps_its_pixels_at_the_same_size() {
        let mut b = LieroBitmapBox::new();
        assert!(b.view().pixels.is_null());
        b.alloc(3, 2, 4);
        let v = b.view();
        assert_eq!((v.w, v.h, v.pitch, b.pixels().len()), (3, 2, 4, 8));
        assert_eq!((v.clip.x2, v.clip.y2), (3, 2));
        unsafe { *v.pixels.add(5) = 9 };
        b.view.clip.x2 = 1;
        b.alloc(3, 2, 4);
        assert_eq!((b.view().pixels, b.pixels()[5], b.view().clip.x2), (v.pixels, 9, 3));
        b.alloc(3, 2, 3);
        assert_eq!((b.view().pitch, b.pixels().len(), b.pixels()[5]), (3, 6, 0));
    }

    #[test]
    fn a_copy_has_the_same_size_and_pixels() {
        let mut a = LieroBitmapBox::new();
        a.alloc(2, 2, 3);
        unsafe { *a.view().pixels.add(4) = 7 };
        let mut b = LieroBitmapBox::new();
        b.alloc(5, 5, 5);
        unsafe { liero_bitmap_box_copy(&mut *b, &*a) };
        assert_eq!((b.view().w, b.view().pitch, b.pixels()), (2, 3, &[0, 0, 0, 0, 7, 0][..]));
        // Copying a bitmap to itself changes nothing.
        let p: *mut LieroBitmapBox = &mut *b;
        unsafe { liero_bitmap_box_copy(p, p) };
        assert_eq!(b.pixels()[4], 7);
    }

    #[test]
    fn a_renderer_sizes_and_clears_its_bitmap() {
        let mut r = LieroRenderer::new();
        r.set_render_resolution(4, 3);
        let v = r.bmp.view();
        assert_eq!((r.data.render_res_x, r.data.render_res_y, v.w, v.h, v.pitch), (4, 3, 4, 3, 4));
        unsafe { *v.pixels.add(11) = 5 };
        r.clear();
        assert_eq!(r.bmp.pixels(), &[0; 12]);
    }
}
