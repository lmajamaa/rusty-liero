#ifndef LIERO_LEVEL_HPP
#define LIERO_LEVEL_HPP

#include <vector>
#include <string>
#include <cstdio>
#include <utility>
#include "gfx/palette.hpp"
#include "material.hpp"
#include "common.hpp"
#include "settingsffi.hpp"
#include "bytes.hpp"
#include <liero_game.h>
#include <gvl/math/vec.hpp>
#include <gvl/math/rect.hpp>

struct Game;
struct Settings;
struct Rand;
struct Common;

// A level's pixels or materials, which Rust owns: `size()` entries from
// `data()`, row by row. Valid until the level is resized.
template<typename T>
struct LevelArray
{
	LevelArray(T*& ptr, int& width, int& height)
	: ptr(ptr), width(width), height(height)
	{
	}

	LevelArray(LevelArray const&) = delete;
	LevelArray& operator=(LevelArray const&) = delete;

	T& operator[](std::size_t i) const { return ptr[i]; }
	std::size_t size() const { return std::size_t(width) * std::size_t(height); }
	T* data() const { return ptr; }
	T* begin() const { return ptr; }
	T* end() const { return ptr + size(); }

private:
	T*& ptr;
	int& width;
	int& height;
};

// A level. Rust owns it (LieroLevelBox in rust/liero-game/src/level.rs);
// the fields here refer to it.
struct Level
{
	Level();
	// A copy with pixels of its own.
	Level(Level const& other);
	Level& operator=(Level const& other);
	~Level();

	bool load(Common& common, Settings const& settings, ByteReader r);
	void generateDirtPattern(Common& common, Rand& rand);
	void generateRandom(Common& common, Settings const& settings, Rand& rand);
	void makeShadow(Common& common);
	void generateFromSettings(Common& common, Settings const& settings, Rand& rand);
	bool selectSpawn(Rand& rand, int w, int h, gvl::ivec2& selected);
	void drawMiniature(Bitmap& dest, int mapX, int mapY, int step);

	unsigned char pixel(int x, int y)
	{
		return data[x + y*width];
	}

	unsigned char pixel(fixedvec pos)
	{
		return data[pos.x + pos.y*width];
	}

	unsigned char* pixelp(int x, int y)
	{
		return &data[x + y*width];
	}

	void setPixel(int x, int y, PalIdx w, Common& common)
	{
		data[x + y*width] = w;
		materials[x + y*width] = common.materials[w];
	}

	void setPixel(fixedvec pos, PalIdx w, Common& common)
	{
		data[pos.x + pos.y*width] = w;
		materials[pos.x + pos.y*width] = common.materials[w];
	}

	Material& mat(int x, int y)
	{
		return materials[x + y*width];
	}

	Material& mat(fixedvec pos)
	{
		return materials[pos.x + pos.y*width];
	}

	Material* matp(int x, int y)
	{
		return &materials[x + y*width];
	}

	bool inside(int x, int y)
	{
		return static_cast<unsigned int>(x) < static_cast<unsigned int>(width)
		    && static_cast<unsigned int>(y) < static_cast<unsigned int>(height);
	}

	bool inside(fixedvec pos)
	{
		return static_cast<unsigned int>(pos.x) < static_cast<unsigned int>(width)
		    && static_cast<unsigned int>(pos.y) < static_cast<unsigned int>(height);
	}

	// The levels trade everything.
	void swap(Level& other);

	gvl::rect rect()
	{
		return gvl::rect(0, 0, width, height);
	}

	// Keeps the first pixels; new ones are zero.
	void resize(int width_new, int height_new);

	// View of the pixels and materials for the Rust level code
	// (rust/liero-game/src/level.rs), valid until the level is resized.
	LieroLevel view()
	{
		return level.level;
	}

	// The level in Rust.
	LieroLevelBox* rust() const { return box; }

private:
	explicit Level(LieroLevelBox* box);

	static LieroStr getOldLevelFile(void const* b) { return liero_level_box_old_level_file(static_cast<LieroLevelBox const*>(b)); }
	static void setOldLevelFile(void* b, LieroStr s) { liero_level_box_set_old_level_file(static_cast<LieroLevelBox*>(b), s); }

	LieroLevelBox* box;
	LieroLevelData& level;

public:
	LevelArray<unsigned char> data;
	LevelArray<Material> materials;
	// What the level was made from.
	bool& oldRandomLevel;
	SettingsText oldLevelFile;
	int& width;
	int& height;
	Palette& origpal;
};

#endif // LIERO_LEVEL_HPP
