//! The screens the game draws on: bitmaps (C++'s `Bitmap`) and renderers,
//! a bitmap with its palettes and fade (C++'s `Renderer`). Rust owns them,
//! and the C++ types are views of them.

use crate::game::Rect;
use std::cell::Cell;

use crate::gfx::{liero_gfx_fill, Bitmap, Color};
use crate::tc::TcData;

/// A bitmap that owns its pixels. They are cells, so that views of it can
/// be drawn on while others are held.
pub struct BitmapBox {
    pixels: Vec<Cell<u8>>,
    w: i32,
    h: i32,
    pitch: i32,
    clip: Rect,
}

impl BitmapBox {
    /// No pixels, as C++'s `Bitmap()` left it.
    pub fn new() -> Box<Self> {
        Box::new(BitmapBox {
            pixels: Vec::new(),
            w: 0,
            h: 0,
            pitch: 0,
            clip: Rect::default(),
        })
    }

    pub fn view(&self) -> Bitmap<'_> {
        Bitmap {
            pixels: &self.pixels,
            w: self.w,
            h: self.h,
            pitch: self.pitch,
            clip: self.clip,
        }
    }

    /// `Bitmap::alloc`: pixels for `w` x `h` with rows `pitch` apart, kept
    /// if the size is the same, and the clip rectangle reset to the whole.
    /// New pixels are zero, where C++ left them uninitialized.
    pub fn alloc(&mut self, w: i32, h: i32, pitch: i32) {
        if self.pixels.is_empty() || self.w != w || self.h != h || self.pitch != pitch {
            self.pixels = vec![Cell::new(0); pitch.max(0) as usize * h.max(0) as usize];
            (self.w, self.h, self.pitch) = (w, h, pitch);
        }
        self.clip = Rect {
            x1: 0,
            y1: 0,
            x2: w,
            y2: h,
        };
    }

    /// `Bitmap::copy`: the same size and pixels as `other`.
    pub fn copy_from(&mut self, other: &BitmapBox) {
        self.alloc(other.w, other.h, other.pitch);
        self.pixels.clone_from(&other.pixels);
    }

    /// A copy of the pixels.
    pub fn pixels(&self) -> Vec<u8> {
        self.pixels.iter().map(Cell::get).collect()
    }
}

/// A renderer's palettes and size, which C++'s `Renderer` refers to.
#[derive(Clone, Copy)]
pub struct RendererData {
    /// The palette shown.
    pub pal: [Color; 256],
    /// The palette `pal` is made from.
    pub origpal: [Color; 256],
    pub fade_value: i32,
    /// The size drawn at, which `set_render_resolution` sets.
    pub render_res_x: i32,
    pub render_res_y: i32,
}

/// A bitmap to draw on, with its palettes.
pub struct Renderer {
    pub data: RendererData,
    pub bmp: Box<BitmapBox>,
}

impl Renderer {
    pub fn new() -> Box<Self> {
        Box::new(Renderer {
            data: RendererData {
                pal: [Color::default(); 256],
                origpal: [Color::default(); 256],
                fade_value: 0,
                render_res_x: 320,
                render_res_y: 200,
            },
            bmp: BitmapBox::new(),
        })
    }

    /// `setRenderResolution`.
    pub fn set_render_resolution(&mut self, x: i32, y: i32) {
        (self.data.render_res_x, self.data.render_res_y) = (x, y);
        self.bmp.alloc(x, y, x);
    }

    /// `clear`: the bitmap filled with colour 0.
    pub fn clear(&mut self) {
        liero_gfx_fill(&self.bmp.view(), 0);
    }

    /// `loadPalette`: both palettes from the TC's.
    pub fn load_palette(&mut self, tc: &TcData) {
        self.data.origpal = tc.palette_colors;
        self.data.pal = self.data.origpal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bitmap_keeps_its_pixels_at_the_same_size() {
        let mut b = BitmapBox::new();
        assert!(b.view().pixels.is_empty());
        b.alloc(3, 2, 4);
        let v = b.view();
        assert_eq!((v.w, v.h, v.pitch, v.pixels.len()), (3, 2, 4, 8));
        assert_eq!((v.clip.x2, v.clip.y2), (3, 2));
        v.pixels[5].set(9);
        b.clip.x2 = 1;
        let before = b.pixels.as_ptr();
        b.alloc(3, 2, 4);
        assert_eq!((b.pixels.as_ptr(), b.pixels()[5], b.view().clip.x2), (before, 9, 3));
        b.alloc(3, 2, 3);
        assert_eq!((b.view().pitch, b.pixels().len(), b.pixels()[5]), (3, 6, 0));
    }

    #[test]
    fn a_copy_has_the_same_size_and_pixels() {
        let mut a = BitmapBox::new();
        a.alloc(2, 2, 3);
        a.view().pixels[4].set(7);
        let mut b = BitmapBox::new();
        b.alloc(5, 5, 5);
        b.copy_from(&a);
        assert_eq!((b.view().w, b.view().pitch, b.pixels()), (2, 3, vec![0, 0, 0, 0, 7, 0]));
    }

    #[test]
    fn a_renderer_sizes_and_clears_its_bitmap() {
        let mut r = Renderer::new();
        r.set_render_resolution(4, 3);
        let v = r.bmp.view();
        assert_eq!(
            (r.data.render_res_x, r.data.render_res_y, v.w, v.h, v.pitch),
            (4, 3, 4, 3, 4)
        );
        v.pixels[11].set(5);
        r.clear();
        assert_eq!(r.bmp.pixels(), [0; 12]);
    }
}
