#ifndef UUID_9059AB0F9EA54E1EDF52E7BF41433D0B
#define UUID_9059AB0F9EA54E1EDF52E7BF41433D0B

#include "color.hpp"
#include "sprite.hpp"
#include "../geom.hpp"
#include <vector>
#include <algorithm>

struct Level;
struct Common;
struct Rand;
struct Bitmap;

void vline(Bitmap& scr, int x, int y1, int y2, int color);

void fillRect(Bitmap& scr, int x, int y, int w, int h, int color);
void fill(Bitmap& scr, int color);
void drawBar(Bitmap& scr, int x, int y, int width, int color);
void drawBar(Bitmap& scr, int x, int y, int width, int height, int color);
void drawRoundedBox(Bitmap& scr, int x, int y, int color, int height, int width);
void drawRoundedLineBox(Bitmap& scr, int x, int y, int color, int width, int height);
void blitImageNoKeyColour(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height, int pitch);
//void blitImage(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height);
void blitImage(Bitmap& scr, Sprite spr, int x, int y);
void blitImageR(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height);
void blitImageTrans(Bitmap& scr, Sprite spr, int x, int y, int phase);
void blitShadowImage(Common& common, Bitmap& scr, PalIdx* mem, int x, int y, int width, int height);
void blitStone(Common& common, Level& level, bool p1, PalIdx* mem, int x, int y);
void blitFireCone(Bitmap& scr, int fc, PalIdx* mem, int x, int y);
void drawDashedLineBox(Bitmap& scr, int x, int y, int color, int color2, int num, int den, int width, int height, int phase);

void drawNinjarope(Common& common, Bitmap& scr, int fromX, int fromY, int toX, int toY);
void drawLaserSight(Bitmap& scr, Rand& rand, int fromX, int fromY, int toX, int toY);
void drawShadowLine(Common& common, Bitmap& scr, int fromX, int fromY, int toX, int toY);
void drawLine(Bitmap& scr, int fromX, int fromY, int toX, int toY, int color);

void scaleDraw(
	PalIdx* src, int w, int h, std::size_t srcPitch,
	uint8_t* dest, std::size_t destPitch, int mag, uint32_t* pal32);

void preparePaletteBgra(Color realPal[256], uint32_t (&pal32)[256]);
int fitScreen(int backW, int backH, int scrW, int scrH, int& offsetX, int& offsetY);

inline void blitImageNoKeyColour(Bitmap& scr, PalIdx* mem, int x, int y, int width, int height)
{
	blitImageNoKeyColour(scr, mem, x, y, width, height, width);
}

#endif // UUID_9059AB0F9EA54E1EDF52E7BF41433D0B
