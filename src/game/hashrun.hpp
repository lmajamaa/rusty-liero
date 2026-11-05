#ifndef LIERO_HASHRUN_HPP
#define LIERO_HASHRUN_HPP

// Plays fixed-seed bot games headlessly and prints game-state hashes.
// See hashrun.cpp.
int hashRun(int argc, char* argv[]);

#include <gvl/resman/shared_ptr.hpp>
#include <cstddef>
#include <cstdint>
#include "math.hpp"

struct Common;

// Drives the menus and weapon selection with scripted keys and prints hashes
// of what they show. See hashmenus.cpp.
void hashMenus(gvl::shared_ptr<Common> common);
void hashTc(Common& common);

// 64-bit FNV-1a over 32-bit words, the same as the Rust tests use.
struct Fnv
{
	uint64_t h = 0xcbf29ce484222325ull;

	void add(uint32_t v)
	{
		for (int i = 0; i < 4; ++i)
		{
			h ^= (v >> (i * 8)) & 0xff;
			h *= 0x100000001b3ull;
		}
	}

	void add(int32_t v) { add(uint32_t(v)); }
	void add(bool v) { add(uint32_t(v ? 1 : 0)); }
	void add(fixedvec v) { add(v.x); add(v.y); }

	void bytes(uint8_t const* p, std::size_t n)
	{
		for (std::size_t i = 0; i < n; ++i)
		{
			h ^= p[i];
			h *= 0x100000001b3ull;
		}
	}
};


#endif // LIERO_HASHRUN_HPP
