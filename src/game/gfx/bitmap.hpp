#ifndef LIERO_GFX_BITMAP_HPP
#define LIERO_GFX_BITMAP_HPP

#include "color.hpp"
#include <cstring>
#include "../geom.hpp"
#include <liero_game.h>

struct Bitmap
{
	Bitmap(Bitmap const&) = delete;
	Bitmap& operator=(Bitmap const&) = delete;

	int w, h;
	unsigned int pitch;
	unsigned char* pixels;
	Rect clip_rect;

	Bitmap()
	: pixels(0)
	{
	}

	void alloc(int w, int h)
	{
		alloc(w, h, w);
	}

	void alloc(int newW, int newH, unsigned int newPitch)
	{
		if (!pixels || w != newW || h != newH || pitch != newPitch)
		{
			delete[] pixels;
			pixels = new unsigned char[newPitch * newH];
			w = newW;
			h = newH;
			pitch = newPitch;
		}

		clip_rect.x1 = 0;
		clip_rect.y1 = 0;
		clip_rect.x2 = w;
		clip_rect.y2 = h;
	}

	unsigned char& getPixel(int x, int y)
	{
		return (static_cast<unsigned char*>(pixels) + y*pitch)[x];
	}

	void setPixel(int x, int y, PalIdx v)
	{
		if (clip_rect.inside(x, y))
			(static_cast<unsigned char*>(pixels) + y*pitch)[x] = v;
	}

	// The view Rust draws on (rust/liero-game/src/gfx.rs).
	LieroBitmap view()
	{
		LieroBitmap v = { pixels, w, h, int(pitch), { clip_rect.x1, clip_rect.y1, clip_rect.x2, clip_rect.y2 } };
		return v;
	}

	void copy(Bitmap const& other)
	{
		alloc(other.w, other.h, other.pitch);
		std::memcpy(pixels, other.pixels, other.pitch * other.h);
	}

	~Bitmap()
	{
		delete[] pixels;
		pixels = 0;
	}
};

#endif // LIERO_GFX_BITMAP_HPP
