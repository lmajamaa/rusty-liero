#include "level.hpp"

#include "game.hpp"
#include "gfx.hpp"
#include "gfx/color.hpp"
#include "filesystem.hpp"

#include <cstring>

// Level generation, shadows, spawn selection and the miniature are
// implemented in Rust (rust/liero-game/src/level.rs), which owns the level.

static_assert(sizeof(Palette) == sizeof(LieroColor) * 256, "Palette layout");
static_assert(sizeof(Material) == sizeof(LieroMaterial), "Material layout");

Level::Level(LieroLevelBox* box)
: box(box)
, level(*liero_level_box_data(box))
, data(level.level.data, level.level.width, level.level.height)
, materials(reinterpret_cast<Material*&>(level.level.materials), level.level.width, level.level.height)
, oldRandomLevel(level.old_random_level)
, oldLevelFile(box, &getOldLevelFile, &setOldLevelFile)
, width(level.level.width)
, height(level.level.height)
, origpal(reinterpret_cast<Palette&>(level.origpal))
{
}

Level::Level()
: Level(liero_level_box_new())
{
}

Level::Level(Level const& other)
: Level(liero_level_box_clone(other.box))
{
}

Level& Level::operator=(Level const& other)
{
	liero_level_box_assign(box, other.box);
	return *this;
}

Level::~Level()
{
	liero_level_box_free(box);
}

void Level::swap(Level& other)
{
	if (this != &other)
		liero_level_box_swap(box, other.box);
}

void Level::generateDirtPattern(Common& common, Rand& rand)
{
	resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);
	LieroLevel v = view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_generate_dirt_pattern(&v, &c, &rand);
}

void Level::generateRandom(Common& common, Settings const& settings, Rand& rand)
{
	origpal.resetPalette(*common.exepal, settings);

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
	liero_level_box_resize(box, width_new, height_new);
}

bool Level::load(Common& common, Settings const& settings, ByteReader r)
{
	resize(LIERO_LEVEL_WIDTH, LIERO_LEVEL_HEIGHT);

	//std::size_t len = f.len;
	bool resetPalette = true;

	r.get(reinterpret_cast<uint8_t*>(&data[0]), width * height);

	if(/*len >= 504*350 + 10 + 256*3
	&&*/ (settings.extensions && settings.loadPowerlevelPalette))
	{
		uint8_t buf[10] = {};
		if (r.tryGet(buf, 10))
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
		origpal.resetPalette(*common.exepal, settings);

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
			loaded = load(common, settings, ByteReader(FsNode(path).read()));
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

bool Level::selectSpawn(Rand& rand, int w, int h, IVec2& selected)
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
