#ifndef LIERO_GFX_BITMAP_HPP
#define LIERO_GFX_BITMAP_HPP

#include "color.hpp"
#include "../geom.hpp"
#include <liero_game.h>

// A bitmap. Rust owns it (LieroBitmapBox in rust/liero-game/src/renderer.rs);
// the fields here refer to it.
struct Bitmap
{
	// A bitmap with no pixels, of its own.
	Bitmap();
	// A view of `box`, which someone else owns (a renderer's).
	explicit Bitmap(LieroBitmapBox* box);
	~Bitmap();
	Bitmap(Bitmap const&) = delete;
	Bitmap& operator=(Bitmap const&) = delete;

	void alloc(int w, int h)
	{
		alloc(w, h, w);
	}

	// Pixels for w x h, kept if the size is the same; the clip rectangle
	// is the whole bitmap.
	void alloc(int newW, int newH, unsigned int newPitch)
	{
		liero_bitmap_box_alloc(box, newW, newH, int(newPitch));
	}

	unsigned char& getPixel(int x, int y)
	{
		return (pixels + y*pitch)[x];
	}

	void setPixel(int x, int y, PalIdx v)
	{
		if (clip_rect.inside(x, y))
			(pixels + y*pitch)[x] = v;
	}

	// The view Rust draws on (rust/liero-game/src/gfx.rs).
	LieroBitmap view()
	{
		return v;
	}

	void copy(Bitmap const& other)
	{
		liero_bitmap_box_copy(box, other.box);
	}

	// The bitmap in Rust.
	LieroBitmapBox* rust() const { return box; }

private:
	LieroBitmapBox* box;
	bool owned;
	LieroBitmap& v;

public:
	int& w;
	int& h;
	int& pitch;
	unsigned char*& pixels;
	Rect& clip_rect;
};

#endif // LIERO_GFX_BITMAP_HPP
