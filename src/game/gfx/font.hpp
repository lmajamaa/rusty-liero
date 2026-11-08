#ifndef UUID_B06B65B783A849C7B4E509A9676180F8
#define UUID_B06B65B783A849C7B4E509A9676180F8

#include <vector>
#include <string>

#include <liero_game.h>
#include "../tclist.hpp"

struct Bitmap;

struct Font
{
	// The TC's characters, in Rust (rust/liero-game/src/tc.rs).
	typedef LieroTcFontChar Char;

	void drawText(Bitmap& scr, char const* str, std::size_t len, int x, int y, int color, int size);
	int getDims(char const* str, std::size_t len, int* height = 0);
	void drawChar(Bitmap& scr, unsigned char ch, int x, int y, int color, int size);

	void drawChar(Bitmap& scr, unsigned char ch, int x, int y, int color)
	{
		drawChar(scr, ch, x, y, color, 1);
	}

	void drawCenteredText(Bitmap& scr, std::string const& str, int x, int y, int color, int size)
	{
		int len = getDims(str) * size;
		drawText(scr, str.data(), str.size(), x - (len / 2), y, color, size);
	}

	void drawCenteredText(Bitmap& scr, std::string const& str, int x, int y, int color)
	{
		drawCenteredText(scr, str, x, y, color, 1);
	}

	// draws text with a simple shadow underneath it, so even text that would blend into the background can
	// be displayed
	void drawShadowedText(Bitmap& scr, std::string const& str, int x, int y, int color)
	{
		drawText(scr, str, x + 1, y + 1, color / 2);
		drawText(scr, str, x, y, color);
	}

	void drawText(Bitmap& scr, char const* str, std::size_t len, int x, int y, int color)
	{
		drawText(scr, str, len, x, y, color, 1);
	}

	void drawText(Bitmap& scr, std::string const& str, int x, int y, int color)
	{
		drawText(scr, str.data(), str.size(), x, y, color, 1);
	}

	int getDims(std::string const& str, int* height = 0)
	{
		return getDims(str.data(), str.size(), height);
	}

	void drawFramedText(Bitmap& scr, std::string const& text, int x, int y, int color);

	// The font as Rust draws with it.
	LieroFont view() const
	{
		LieroFont f = { reinterpret_cast<LieroFontChar const*>(chars.items), chars.size() };
		return f;
	}

	TcList<Char> chars;
};

#endif // UUID_B06B65B783A849C7B4E509A9676180F8
