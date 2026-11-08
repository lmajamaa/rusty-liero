#include "palette.hpp"

#include "../settings.hpp"
#include "../bytes.hpp"
#include "../gfx.hpp"

// The palette effects are implemented in Rust (rust/liero-game/src/gfx.rs).

static_assert(sizeof(Color) == sizeof(LieroColor), "Color layout");

namespace
{

LieroColor* toRust(Color* c)
{
	return reinterpret_cast<LieroColor*>(c);
}

}

void Palette::activate(Color realPal[256])
{
	liero_palette_activate(toRust(entries), toRust(realPal));
}

void Palette::fade(int amount)
{
	liero_palette_fade(toRust(entries), amount);
}

void Palette::lightUp(int amount)
{
	liero_palette_light_up(toRust(entries), amount);
}

void Palette::rotateFrom(Palette& source, int from, int to, unsigned dist)
{
	liero_palette_rotate_from(toRust(entries), toRust(source.entries), from, to, dist);
}

void Palette::clear()
{
	std::memset(entries, 0, sizeof(entries));
}

void Palette::read(ByteReader& r)
{
	for(int i = 0; i < 256; ++i)
	{
		uint8_t rgb[3];
		r.get(rgb, 3);

		entries[i].r = rgb[0] & 63;
		entries[i].g = rgb[1] & 63;
		entries[i].b = rgb[2] & 63;
	}
}

int const Palette::wormColourIndexes[2] = {0x58, 0x78}; // TODO: Read from EXE?

void Palette::setWormColour(int i, WormSettings const& settings)
{
	liero_palette_set_worm_colour(toRust(entries), i, settings.color, &settings.rgb);
}


void Palette::setWormColours(Settings const& settings)
{
	for(int i = 0; i < 2; ++i)
	{
		setWormColour(i, *settings.wormSettings[i]);
	}
}
