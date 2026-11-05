#include "text.hpp"
#include <liero_game.h>
#include <cctype>
#include <algorithm>

// The time formats are implemented in Rust (rust/liero-game/src/text.rs).

char const* timeToString(int sec)
{
	static char ret[6];
	liero_time_to_string(sec, reinterpret_cast<uint8_t*>(ret));
	return ret;
}

char const* timeToStringEx(int ms, bool forceHours, bool forceMinutes)
{
	static char ret[10];
	liero_time_to_string_ex(ms, forceHours, forceMinutes, reinterpret_cast<uint8_t*>(ret));
	return ret;
}

char const* timeToStringFrames(int frames)
{
	return timeToStringEx(frames * 14, false, false);
}

int safeToUpper(char ch)
{
	return std::toupper(static_cast<unsigned char>(ch));
}

bool ciCompare(std::string const& a, std::string const& b)
{
	if(a.size() != b.size())
		return false;

	for(std::size_t i = 0; i < a.size(); ++i)
	{
		if(safeToUpper(a[i]) != safeToUpper(b[i]))
			return false;
	}

	return true;
}

bool ciStartsWith(std::string const& text, std::string const& startsWith)
{
	if (startsWith.size() > text.size())
		return false;

	for(std::size_t i = 0; i < startsWith.size(); ++i)
	{
		if(safeToUpper(text[i]) != safeToUpper(startsWith[i]))
			return false;
	}

	return true;
}

bool ciLess(std::string const& a, std::string const& b)
{
	for(std::size_t i = 0; i < a.size(); ++i)
	{
		if(i >= b.size()) // a is longer, thus a > b
			return false;
		int ach = safeToUpper(a[i]);
		int bch = safeToUpper(b[i]);
		if(ach < bch)
			return true;
		else if(ach > bch)
			return false;
	}

	return b.size() > a.size(); // if b is longer, then a < b, otherwise a == b
}

char utf8ToDOS(char* str)
{
	if (str[1] == 0) {
		return str[0];
	}
	char table[][3] =
	{
		{(char)0xc3, (char)0xa5, (char)0x86}, // å
		{(char)0xc3, (char)0xa4, (char)0x84}, // ä
		{(char)0xc3, (char)0xb6, (char)0x94}, // ö
		{(char)0xc3, (char)0x85, (char)0x8f}, // Å
		{(char)0xc3, (char)0x84, (char)0x8e}, // Ä
		{(char)0xc3, (char)0x96, (char)0x99}, // Ö
	};

	for(std::size_t i = 0; i < sizeof(table) / sizeof(*table); ++i)
	{
		if(table[i][0] == str[0] && table[i][1] == str[1])
			return table[i][2];
	}
	return '?';
}
