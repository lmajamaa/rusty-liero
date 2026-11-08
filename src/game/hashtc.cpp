// Hashes everything Common::load reads from a TC folder, for the hash run.
// The TOML parts are written back out as gvl's TOML writer wrote them and
// hashed as text, so every field the archive functions name is covered; the
// sprites, palette, font, sounds and precomputed tables are hashed directly.

#include "hashrun.hpp"

#include "common_model.hpp"
#include "filesystem.hpp"
#include "gfx.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace
{

// Writes TOML as gvl's toml::writer did, hashing it instead of storing it.
struct TomlWriter
{
	Fnv h;
	int indent = 0;
	std::vector<char const*> chain;
	bool inObject = false;

	TomlWriter& operator<<(char c) { h.bytes((uint8_t const*)&c, 1); return *this; }

	TomlWriter& operator<<(char const* s)
	{
		for (; *s; ++s)
			*this << *s;
		return *this;
	}

	TomlWriter& operator<<(int32_t v)
	{
		char buf[16];
		std::snprintf(buf, sizeof buf, "%d", (int)v);
		return *this << (char const*)buf;
	}

	void windent()
	{
		*this << "\r\n";
		for (int i = 0; i < indent; ++i)
			*this << "  ";
	}

	void beginf(char const* name)
	{
		chain.push_back(name);
		inObject = false;
	}

	void fname()
	{
		for (std::size_t i = 0; i < chain.size(); ++i)
		{
			if (i)
				*this << '.';
			*this << chain[i];
		}
	}

	void f(char const* name)
	{
		if (!name)
			return;
		if (!inObject)
		{
			if (!chain.empty())
			{
				*this << "\r\n";
				windent();
				*this << "[";
				fname();
				*this << "]\r\n";
			}
			inObject = true;
		}
		windent();
		*this << name << " = ";
	}

	void endf()
	{
		chain.pop_back();
		inObject = false;
	}

	template<typename F>
	void obj(char const* name, F func)
	{
		if (name)
			beginf(name);
		func();
		if (name)
			endf();
	}

	template<typename A, typename F>
	void arr(char const* name, A& arr, F func)
	{
		f(name);
		*this << '[';
		bool first = true;
		for (auto& e : arr)
		{
			if (!first)
				*this << ", ";
			first = false;
			func(e);
		}
		*this << "]";
	}

	template<typename A, typename F>
	void array_obj(char const* name, A& arr, F func)
	{
		beginf(name);
		for (auto& e : arr)
		{
			*this << "\r\n";
			windent();
			*this << "[[";
			fname();
			*this << "]]\r\n";
			++indent;
			inObject = true;
			func(e);
			--indent;
		}
		endf();
	}

	void i32(char const* name, int32_t v) { f(name); *this << v; }
	void b(char const* name, bool v) { f(name); *this << (v ? "true" : "false"); }
	void null(char const* name) { f(name); *this << "null"; }

	template<typename T, typename Resolver>
	void ref(char const* name, T const& v, Resolver resolver)
	{
		f(name);
		resolver.v2r(*this, v);
	}

	void str(char const* name, LieroStr s)
	{
		str(name, tcString(s));
	}

	void str(char const* name, std::string const& s)
	{
		f(name);
		*this << '"';
		for (char c : s)
		{
			if (c >= 0x20 && c <= 0x7e)
			{
				if (c == '"' || c == '\\')
					*this << '\\';
				*this << c;
			}
			else
			{
				char buf[8];
				std::snprintf(buf, sizeof buf, "\\u%04x", (unsigned)(uint8_t)c);
				*this << (char const*)buf;
			}
		}
		*this << '"';
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
		TomlWriter ar;
		archive_text(common, ar);
		print("tc-cfg", 0, ar.h);
	}

	for (std::size_t i = 0; i < common.weapons.size(); ++i)
	{
		auto& weapon = common.weapons[i];
		TomlWriter ar;
		archive_text(common, weapon, ar);
		ar.h.add(int32_t(weapon.id));
		ar.h.bytes(weapon.id_str.ptr, weapon.id_str.len);
		print("tc-weapon", (int)i, ar.h);
	}

	for (std::size_t i = 0; i < common.nobjectTypes.size(); ++i)
	{
		auto& nobject = common.nobjectTypes[i];
		TomlWriter ar;
		archive_text(common, nobject, ar);
		ar.h.add(int32_t(nobject.id));
		ar.h.bytes(nobject.id_str.ptr, nobject.id_str.len);
		print("tc-nobject", (int)i, ar.h);
	}

	for (std::size_t i = 0; i < common.sobjectTypes.size(); ++i)
	{
		auto& sobject = common.sobjectTypes[i];
		TomlWriter ar;
		archive_text(common, sobject, ar);
		ar.h.add(int32_t(sobject.id));
		ar.h.bytes(sobject.id_str.ptr, sobject.id_str.len);
		print("tc-sobject", (int)i, ar.h);
	}

	for (std::size_t i = 0; i < common.sounds.size(); ++i)
	{
		auto& s = common.sounds[i];
		Fnv h;
		h.bytes((uint8_t const*)s.name.data(), s.name.size());
		h.add(uint32_t(s.originalData.size()));
		h.bytes(s.originalData.data(), s.originalData.size());
		h.add(s.loaded);
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

// F8's random TC, on a TC of its own. Only the weapons and special objects:
// the order the other objects get their values in was shuffled with
// std::random_device in the C++.
void hashRandomTc()
{
	std::shared_ptr<Common> common(new Common());
	common->load(FsNode(std::string("TC/openliero")));
	std::shared_ptr<Common> old = gfx.common;
	gfx.common = common;
	gfx.randomizeTc();
	gfx.common = old;

	for (std::size_t i = 0; i < common->weapons.size(); ++i)
	{
		TomlWriter ar;
		archive_text(*common, common->weapons[i], ar);
		print("tc-random-weapon", (int)i, ar.h);
	}
	for (std::size_t i = 0; i < common->sobjectTypes.size(); ++i)
	{
		TomlWriter ar;
		archive_text(*common, common->sobjectTypes[i], ar);
		print("tc-random-sobject", (int)i, ar.h);
	}
}
