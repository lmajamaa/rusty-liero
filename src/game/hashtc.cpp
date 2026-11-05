// Hashes everything Common::load reads from a TC folder, for the hash run.
// The TOML parts are written back out with the game's own TOML writer and
// hashed as text, so every field the archive functions name is covered; the
// sprites, palette, font, sounds and precomputed tables are hashed directly.

#include "hashrun.hpp"

#include "common_model.hpp"

#include <cstdio>

namespace
{

// The output stream gvl::toml::writer writes to, hashing instead of storing.
struct HashWriter
{
	Fnv h;

	void put(uint8_t c) { h.bytes(&c, 1); }
	void put(uint8_t const* p, std::size_t n) { h.bytes(p, n); }

	HashWriter& operator<<(char c) { put((uint8_t)c); return *this; }

	HashWriter& operator<<(char const* s)
	{
		for (; *s; ++s)
			put((uint8_t)*s);
		return *this;
	}

	HashWriter& operator<<(int32_t v)
	{
		char buf[16];
		std::snprintf(buf, sizeof buf, "%d", (int)v);
		return *this << (char const*)buf;
	}
};

void print(char const* name, int index, Fnv const& h)
{
	std::printf("%s %d %016llx\n", name, index, (unsigned long long)h.h);
}

void addSprites(Fnv& h, SpriteSet const& ss)
{
	h.add(int32_t(ss.width));
	h.add(int32_t(ss.height));
	h.add(int32_t(ss.spriteSize));
	h.add(int32_t(ss.count));
	h.add(uint32_t(ss.data.size()));
	h.bytes(ss.data.data(), ss.data.size());
}

} // namespace

void hashTc(Common& common)
{
	{
		HashWriter w;
		gvl::toml::writer<HashWriter> ar(w);
		archive_text(common, ar);
		print("tc-cfg", 0, w.h);
	}

	for (std::size_t i = 0; i < common.weapons.size(); ++i)
	{
		auto& weapon = common.weapons[i];
		HashWriter w;
		gvl::toml::writer<HashWriter> ar(w);
		archive_text(common, weapon, ar);
		w.h.add(int32_t(weapon.id));
		w.h.bytes((uint8_t const*)weapon.idStr.data(), weapon.idStr.size());
		print("tc-weapon", (int)i, w.h);
	}

	for (std::size_t i = 0; i < common.nobjectTypes.size(); ++i)
	{
		auto& nobject = common.nobjectTypes[i];
		HashWriter w;
		gvl::toml::writer<HashWriter> ar(w);
		archive_text(common, nobject, ar);
		w.h.add(int32_t(nobject.id));
		w.h.bytes((uint8_t const*)nobject.idStr.data(), nobject.idStr.size());
		print("tc-nobject", (int)i, w.h);
	}

	for (std::size_t i = 0; i < common.sobjectTypes.size(); ++i)
	{
		auto& sobject = common.sobjectTypes[i];
		HashWriter w;
		gvl::toml::writer<HashWriter> ar(w);
		archive_text(common, sobject, ar);
		w.h.add(int32_t(sobject.id));
		w.h.bytes((uint8_t const*)sobject.idStr.data(), sobject.idStr.size());
		print("tc-sobject", (int)i, w.h);
	}

	for (std::size_t i = 0; i < common.sounds.size(); ++i)
	{
		auto& s = common.sounds[i];
		Fnv h;
		h.bytes((uint8_t const*)s.name.data(), s.name.size());
		h.add(uint32_t(s.originalData.size()));
		h.bytes(s.originalData.data(), s.originalData.size());
		h.add(s.sound != 0);
		print("tc-sound", (int)i, h);
	}

	{
		Fnv h;
		addSprites(h, common.smallSprites);
		addSprites(h, common.largeSprites);
		addSprites(h, common.textSprites);
		for (auto const& c : common.exepal.entries)
		{
			h.add(uint32_t(c.r));
			h.add(uint32_t(c.g));
			h.add(uint32_t(c.b));
			h.add(uint32_t(c.unused));
		}
		print("tc-sprites", 0, h);
	}

	{
		Fnv h;
		h.add(uint32_t(common.font.chars.size()));
		for (auto const& c : common.font.chars)
		{
			h.bytes(c.data, sizeof c.data);
			h.add(int32_t(c.width));
		}
		print("tc-font", 0, h);
	}

	{
		Fnv h;
		h.add(uint32_t(common.weapOrder.size()));
		for (int i : common.weapOrder)
			h.add(int32_t(i));
		addSprites(h, common.wormSprites);
		addSprites(h, common.fireConeSprites);
		print("tc-computed", 0, h);
	}
}
