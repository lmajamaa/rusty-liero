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

Level::Level(LieroLevelBox* box, bool owned)
: box(box)
, owned(owned)
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
: Level(liero_level_box_new(), true)
{
}

Level::Level(Level const& other)
: Level(liero_level_box_clone(other.box), true)
{
}

Level::Level(Level&& other)
: Level(other.box, other.owned)
{
	other.owned = false;
}

Level& Level::operator=(Level const& other)
{
	liero_level_box_assign(box, other.box);
	return *this;
}

Level::~Level()
{
	if (owned)
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
	LieroLevelCommon c = common.levelCommon();
	std::vector<uint8_t> const& bytes = *r.data;
	std::size_t pos = std::min(r.pos, bytes.size());
	return liero_level_box_load(box, &c, reinterpret_cast<LieroColor const*>(common.exepal->entries),
		settings.extensions && settings.loadPowerlevelPalette, bytes.data() + pos, bytes.size() - pos);
}

void Level::generateFromSettings(Common& common, Settings const& settings, Rand& rand)
{
	LieroLevelCommon c = common.levelCommon();
	liero_level_box_generate_from_settings(box, &c, reinterpret_cast<LieroColor const*>(common.exepal->entries), settings.rust(), &rand);
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
