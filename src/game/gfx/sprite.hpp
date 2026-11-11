#ifndef UUID_FB823C685B8D47570B89508CD25CC4A6
#define UUID_FB823C685B8D47570B89508CD25CC4A6

#include <cstddef>
#include <liero_game.h>
#include <cstdio>
#include <cassert>
#include "color.hpp"

struct Sprite
{
	PalIdx const* mem;
	int width, height, pitch;
};

// Sprites of the TC, which Rust owns (rust/liero-game/src/tc.rs): `count`
// sprites of `width` x `height` palette indices.
struct SpriteSet
{
	SpriteSet()
	{
	}

	SpriteSet(LieroTcSprites const& s)
	: data(s.data), width(s.width), height(s.height), spriteSize(s.width * s.height), count(s.count)
	{
	}

	PalIdx const* data = nullptr;
	int width = 0;
	int height = 0;
	int spriteSize = 0;
	int count = 0;

	std::size_t size() const { return std::size_t(spriteSize) * count; }

	PalIdx const* spritePtr(int frame) const
	{
		assert(frame >= 0 && frame < count);
		return &data[frame*spriteSize];
	}

	Sprite operator[](int frame) const
	{
		assert(frame >= 0 && frame < count);
		Sprite s = {&data[frame*spriteSize], width, height, width};
		return s;
	}
};

#endif // UUID_FB823C685B8D47570B89508CD25CC4A6
