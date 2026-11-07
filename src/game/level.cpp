#include "level.hpp"

#include "game.hpp"
#include "gfx.hpp"
#include "gfx/color.hpp"
#include "filesystem.hpp"

#include <cstring>

// Level generation, shadows, spawn selection and the miniature are
// implemented in Rust (rust/liero-game/src/level.rs). The level keeps its
// pixels here and hands Rust a view of them.

void Level::generateDirtPattern(Common& common, Rand& rand)
{
	resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
	LieroLevel v = view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_generate_dirt_pattern(&v, &c, &rand);
}

void Level::generateRandom(Common& common, Settings const& settings, Rand& rand)
{
	origpal.resetPalette(common.exepal, settings);

	resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
	LieroLevel v = view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_generate_random(&v, &c, &rand);
}

void Level::makeShadow(Common& common)
{
	LieroLevel v = view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_make_shadow(&v, &c);
}

void Level::resize(int width_new, int height_new)
{
	width = width_new;
	height = height_new;
	data.resize(width * height);
	materials.resize(width * height);
}

bool Level::load(Common& common, Settings const& settings, gvl::octet_reader r)
{
	resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);

	//std::size_t len = f.len;
	bool resetPalette = true;

	r.get(reinterpret_cast<uint8_t*>(&data[0]), width * height);

	if(/*len >= 504*350 + 10 + 256*3
	&&*/ (settings.extensions && settings.loadPowerlevelPalette))
	{
		uint8_t buf[10] = {};
		if (r.try_get(buf, 10))
		{
			if(!std::memcmp("POWERLEVEL", buf, 10))
			{
				Palette pal;
				pal.read(r);
				origpal.resetPalette(pal, settings);

				resetPalette = false;
			}
		}
	}

	LieroLevel v = view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_update_materials(&v, &c);

	if (resetPalette)
		origpal.resetPalette(common.exepal, settings);

	return true;
}

void Level::generateFromSettings(Common& common, Settings const& settings, Rand& rand)
{
	if(settings.randomLevel)
	{
		generateRandom(common, settings, rand);
	}
	else
	{
		std::string path = settings.levelFile.str();
		if (path.find('.', 0) == std::string::npos)
			path += ".LEV";

		bool loaded = false;
		try
		{
			loaded = load(common, settings, FsNode(path).toOctetReader());
		}
		catch (std::runtime_error&)
		{
			// Ignore
		}

		if (!loaded)
			generateRandom(common, settings, rand);
	}

	oldRandomLevel = settings.randomLevel;
	oldLevelFile = settings.levelFile.str();

	if(settings.shadow)
	{
		makeShadow(common);
	}
}

bool Level::selectSpawn(Rand& rand, int w, int h, gvl::ivec2& selected)
{
	LieroLevel v = view();
	LieroIVec2 pos = { selected.x, selected.y };
	bool found = liero_level_select_spawn(&v, &rand, w, h, &pos);
	selected.x = pos.x;
	selected.y = pos.y;
	return found;
}

void Level::drawMiniature(Bitmap& dest, int mapX, int mapY, int step)
{
	LieroLevel v = view();
	liero_level_draw_miniature(&v, static_cast<uint8_t*>(dest.pixels), dest.pitch, mapX, mapY, step);
}
