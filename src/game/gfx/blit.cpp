#include "blit.hpp"

#include "../constants.hpp"
#include "../level.hpp"
#include "../common.hpp"
#include "../rand.hpp"
#include "../settings.hpp"
#include "bitmap.hpp"
#include "macros.hpp"
#include "../geom.hpp"
#include <cstring>
#include <cassert>
#include <cstdlib>
#include <algorithm>
#include <cstddef>

// The drawing functions are implemented in Rust (rust/liero-game/src/gfx.rs).

void fillRect(Bitmap& scr, int x, int y, int w, int h, int color)
{
	LieroBitmap v = scr.view();
	liero_gfx_fill_rect(&v, x, y, w, h, uint8_t(color));
}

void fill(Bitmap& scr, int color)
{
	LieroBitmap v = scr.view();
	liero_gfx_fill(&v, uint8_t(color));
}

void drawBar(Bitmap& scr, int x, int y, int width, int color)
{
	drawBar(scr, x, y, width, 2, color);
}

void drawBar(Bitmap& scr, int x, int y, int width, int height, int color)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_bar(&v, x, y, width, height, uint8_t(color));
}

void vline(Bitmap& scr, int x, int y1, int y2, int color)
{
	LieroBitmap v = scr.view();
	liero_gfx_vline(&v, x, y1, y2, uint8_t(color));
}

void drawRoundedBox(Bitmap& scr, int x, int y, int color, int height, int width)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_rounded_box(&v, x, y, uint8_t(color), height, width);
}

void drawRoundedLineBox(Bitmap& scr, int x, int y, int color, int width, int height)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_rounded_line_box(&v, x, y, uint8_t(color), width, height);
}

void drawDashedLineBox(Bitmap& scr, int x, int y, int color, int color2, int num, int den, int width, int height, int phase)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_dashed_line_box(&v, x, y, uint8_t(color), uint8_t(color2), num, den, width, height, phase);
}

void blitImageNoKeyColour(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height, int pitch)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_image_no_key_colour(&v, mem, x, y, width, height, pitch);
}

void blitImage(Bitmap& scr, Sprite spr, int x, int y)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_image(&v, spr.mem, x, y, spr.width, spr.height, spr.pitch);
}

void blitImageTrans(Bitmap& scr, Sprite spr, int x, int y, int phase)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_image_trans(&v, spr.mem, x, y, spr.width, spr.height, spr.pitch, phase);
}

void blitImageR(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_image_r(&v, mem, x, y, width, height);
}

void blitFireCone(Bitmap& scr, int fc, PalIdx* mem, int x, int y)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_fire_cone(&v, fc, mem, x, y);
}

void blitShadowImage(Common& common, Bitmap& scr, PalIdx* mem, int x, int y, int width, int height)
{
	LieroBitmap v = scr.view();
	liero_gfx_blit_shadow_image(&v, common.materials, mem, x, y, width, height);
}

// The terrain operations below are implemented in Rust
// (rust/liero-game/src/level.rs).

void blitImageOnMap(Common& common, Level& level, PalIdx* mem, int x, int y, int width, int height)
{
	LieroLevel v = level.view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_blit_image(&v, &c, mem, x, y, width, height);
}

void blitStone(Common& common, Level& level, bool p1, PalIdx* mem, int x, int y)
{
	LieroLevel v = level.view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_blit_stone(&v, &c, p1, mem, x, y);
}

void drawDirtEffect(Common& common, Rand& rand, Level& level, int dirtEffect, int x, int y)
{
	assert(dirtEffect >= 0 && dirtEffect < LIERO_TEXTURE_COUNT);
	LieroLevel v = level.view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_draw_dirt_effect(&v, &c, &rand, dirtEffect, x, y);
}

void correctShadow(Common& common, Level& level, Rect rect)
{
	LieroLevel v = level.view();
	LieroLevelCommon c = common.levelCommon();
	liero_level_correct_shadow(&v, &c, rect.x1, rect.y1, rect.x2, rect.y2);
}

void drawNinjarope(Common& common, Bitmap& scr, int fromX, int fromY, int toX, int toY)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_ninjarope(&v, LC(NRColourBegin), LC(NRColourEnd), fromX, fromY, toX, toY);
}

void drawLaserSight(Bitmap& scr, Rand& rand, int fromX, int fromY, int toX, int toY)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_laser_sight(&v, &rand, fromX, fromY, toX, toY);
}

void drawShadowLine(Common& common, Bitmap& scr, int fromX, int fromY, int toX, int toY)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_shadow_line(&v, common.materials, fromX, fromY, toX, toY);
}

void drawLine(Bitmap& scr, int fromX, int fromY, int toX, int toY, int color)
{
	LieroBitmap v = scr.view();
	liero_gfx_draw_line(&v, fromX, fromY, toX, toY, uint8_t(color));
}

void scaleDraw(
	PalIdx* src, int w, int h, std::size_t srcPitch,
	uint8_t* dest, std::size_t destPitch, int mag, uint32_t* pal32)
{
	liero_gfx_scale_draw(src, w, h, srcPitch, dest, destPitch, mag, pal32);
}

static_assert(sizeof(Color) == sizeof(LieroColor), "Color layout");
static_assert(offsetof(Color, r) == offsetof(LieroColor, r) && offsetof(Color, g) == offsetof(LieroColor, g)
	&& offsetof(Color, b) == offsetof(LieroColor, b), "Color layout");

void preparePaletteBgra(Color realPal[256], uint32_t (&pal32)[256])
{
	liero_gfx_prepare_palette_bgra(reinterpret_cast<LieroColor*>(realPal), pal32);
}

int fitScreen(int backW, int backH, int scrW, int scrH, int& offsetX, int& offsetY)
{
	return liero_gfx_fit_screen(backW, backH, scrW, scrH, &offsetX, &offsetY);
}
