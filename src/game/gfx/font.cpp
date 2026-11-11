#include "font.hpp"
#include "../gfx.hpp"
#include "macros.hpp"
#include "color.hpp"
#include "bitmap.hpp"
#include <cstddef>

// Text drawing is implemented in Rust (rust/liero-game/src/gfx.rs).

static_assert(sizeof(Font::Char) == sizeof(LieroFontChar), "Font::Char layout");
static_assert(offsetof(Font::Char, data) == offsetof(LieroFontChar, data), "Font::Char layout");
static_assert(offsetof(Font::Char, width) == offsetof(LieroFontChar, width), "Font::Char layout");

void Font::drawChar(Bitmap& scr, unsigned char c, int x, int y, int color, int size)
{
	LieroBitmap v = scr.view();
	liero_font_draw_char(&v, reinterpret_cast<LieroFontChar const*>(chars.items), chars.size(), c, x, y, uint8_t(color), size);
}

void Font::drawText(Bitmap& scr, char const* str, std::size_t len, int x, int y, int color, int size)
{
	LieroBitmap v = scr.view();
	liero_font_draw_text(&v, reinterpret_cast<LieroFontChar const*>(chars.items), chars.size(),
		reinterpret_cast<uint8_t const*>(str), len, x, y, uint8_t(color), size);
}

void Font::drawFramedText(Bitmap& scr, std::string const& text, int x, int y, int color)
{
	drawRoundedBox(scr, x, y, 0, 7, getDims(text));
	drawText(scr, text, x + 2, y + 1, color);
}

int Font::getDims(char const* str, std::size_t len, int* height)
{
	return liero_font_get_dims(reinterpret_cast<LieroFontChar const*>(chars.items), chars.size(),
		reinterpret_cast<uint8_t const*>(str), len, height);
}
