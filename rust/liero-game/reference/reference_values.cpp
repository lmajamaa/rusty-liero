// Prints the expected values for the tests in rust/liero-game/src/rand.rs,
// math.rs, level.rs, cpp_sort.rs and stats.rs by running the same loops
// through the original C++ code: gvl::mwc and prng_common from src/gvl, libc++'s
// std::sort, the stats screen's percent, and math.cpp's, level.cpp's and
// blit.cpp's functions as they were before the port (copied below, unchanged
// apart from the minimal Level and Common they work on).
//
// Build it with em++ and run it with Bun, so it goes through the same compiler
// as the game. From the repo root:
//
//   docker compose run --rm web sh -c "em++ -std=c++11 -O2 -I src rust/liero-game/reference/reference_values.cpp src/gvl/support/debug.cpp -o /tmp/reference_values.js && bun /tmp/reference_values.js"
//
// A native g++ gives the same values only with -fwrapv (the C++ code relies on
// signed overflow wrapping around, as clang's wasm output does) and
// -ffp-contract=off (wasm has no fused multiply-add, which g++ uses on arm64).

#include <gvl/math/cmwc.hpp>
#include <gvl/math/rect.hpp>
#include <algorithm>
#include <cassert>
#include <vector>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

// ---- From src/game/math.cpp before the Rust port ----

struct ivec2 { int x, y; };
ivec2 cossinTable[128];

uint32_t sqr(uint32_t op)
{
    uint32_t res = 0;
    uint32_t one = 1uL << 30; // The second-to-top bit is set: use 1u << 14 for uint16_t type; use 1uL<<30 for uint32_t type

    // "one" starts at the highest power of four <= than the argument.
    while (one > op)
        one >>= 2;

    while (one != 0)
    {
        if (op >= res + one)
        {
            op -= res + one;
            res += 2 * one;
        }
        res >>= 1;
        one >>= 2;
    }
    return res;
}

int vectorLength(int x, int y)
{
	return int(sqr(x*x + y*y));
}

struct FP
{
	FP(int64_t s, int bits)
	: s(s), bits(bits)
	{
	}

	void reduce(int tobits)
	{
		int64_t lim = (1ll << tobits);

		while (s < (-lim - 1) || s > lim)
		{
			s >>= 1;
			--bits;
		}
	}

	int64_t reducedfrac(int tobits)
	{
		int64_t rs = s;
		int rbits = bits;
		while (rbits > 60)
		{
			rs >>= 1;
			--rbits;
		}

		return rs << (tobits - rbits);
	}

	int64_t s;
	int bits;
};

void precomputeTables()
{
	int scalebits = 28;
	int32_t scale = 13176795; // (2pi / 128) << scalebits

	for(int i = 0; i < 128; ++i)
	{
		int64_t rf = 0;
		int32_t c = -1;
		int32_t xf = i * scale;

		// Simple Taylor series. Performance is not important.
		FP num(xf, scalebits);
		for(int t = 1; t < 26; )
		{
			rf += c * num.reducedfrac(60);

			num.s /= ++t;
			num.reduce(31);
			num.s = num.s * xf;
			num.bits += scalebits;

			num.s /= ++t;
			num.reduce(31);
			num.s = num.s * xf;
			num.bits += scalebits;

			c = -c;
		}

		int shift = 60 - 16;

		rf += (1LL << (shift - 1)); // Correct rounding

		int32_t r = (int32_t)(rf >> shift);

		cossinTable[i].x = r;
		cossinTable[(i + 32) & 0x7f].y = r;
	}
}

// ---- From src/game/material.hpp before the Rust port ----

struct Material
{
	enum
	{
		Dirt = 1<<0,
		Dirt2 = 1<<1,
		Rock = 1<<2,
		Background = 1<<3,
		SeeShadow = 1<<4,
		WormM = 1<<5
	};

	bool dirt() { return (flags & Dirt) != 0; }
	bool dirt2() { return (flags & Dirt2) != 0; }
	bool rock() { return (flags & Rock) != 0; }
	bool background() { return (flags & Background) != 0; }
	bool seeShadow() { return (flags & SeeShadow) != 0; }

	// Constructed
	bool dirtRock() { return (flags & (Dirt | Dirt2 | Rock)) != 0; }
	bool anyDirt() { return (flags & (Dirt | Dirt2)) != 0; }
	bool dirtBack() { return (flags & (Dirt | Dirt2 | Background)) != 0; }
	bool worm() { return (flags & WormM) != 0; }

	uint8_t flags;
};

// ---- Just enough of Common, SpriteSet and Level for the code below ----

typedef uint8_t PalIdx;
typedef gvl::mwc Rand;

struct Texture
{
	bool nDrawBack; // 1C208
	int  mFrame; // 1C1EA
	int  sFrame; // 1C1F4
	int  rFrame; // 1C1FE
};

struct SpriteSet
{
	std::vector<PalIdx> data;
	int spriteSize = 16 * 16;
	int count = 0;

	PalIdx* spritePtr(int frame)
	{
		assert(frame >= 0 && frame < count);
		return &data[frame*spriteSize];
	}
};

struct Common
{
	Material materials[256];
	Texture textures[9];
	SpriteSet largeSprites;
};

int stoneTab[3][4] =
{
	{98, 60, 61, 62},
	{63, 75, 85, 86},
	{89, 90, 97, 96}
};

struct Level
{
	void generateDirtPattern(Common& common, Rand& rand);
	void generateRandom(Common& common, Rand& rand);
	void makeShadow(Common& common);
	bool selectSpawn(Rand& rand, int w, int h, ivec2& selected);
	void drawMiniature(uint8_t* pixels, int pitch, int mapX, int mapY, int step);

	unsigned char pixel(int x, int y) { return data[x + y*width]; }
	unsigned char* pixelp(int x, int y) { return &data[x + y*width]; }

	void setPixel(int x, int y, PalIdx w, Common& common)
	{
		data[x + y*width] = w;
		materials[x + y*width] = common.materials[w];
	}

	Material& mat(int x, int y) { return materials[x + y*width]; }
	Material* matp(int x, int y) { return &materials[x + y*width]; }

	unsigned char checkedPixelWrap(int x, int y)
	{
		unsigned int idx = static_cast<unsigned int>(x + y*width);
		if(idx < data.size())
			return data[idx];
		return 0;
	}

	void resize(int width_new, int height_new)
	{
		width = width_new;
		height = height_new;
		data.resize(width * height);
		materials.resize(width * height);
	}

	std::vector<unsigned char> data;
	std::vector<Material> materials;
	int width = 0, height = 0;
};

// ---- From src/game/gfx/macros.hpp and blit.cpp before the Rust port ----

#define CLIP_IMAGE(clip) \
{ \
	int top = y - (clip).y1; \
	if(top < 0)	{ \
		mem += -top * pitch; \
		height += top; \
		y = (clip).y1; } \
	int bottom = y + height - ((clip).y2); \
	if(bottom > 0) height -= bottom; \
	int left = x - (clip).x1; \
	if(left < 0) { \
		mem -= left; \
		width += left; \
		x = (clip).x1; } \
	int right = x + width - ((clip).x2); \
	if(right > 0) width -= right; \
	if(width <= 0 || height <= 0) return; \
}

#define BLITL(pixels, destpitch, matpixels, body) do { \
	PalIdx* scrptr = (pixels) + y*(destpitch) + x; \
	Material* matptr = (matpixels) + y*(destpitch) + x; \
	for(int y_ = 0; y_ < height; ++y_)	{ \
		PalIdx* rowdest = scrptr; \
		Material* rowmatdest = matptr; \
		PalIdx* rowsrc = mem; \
		for(int x_ = 0; x_ < width; ++x_) { \
			PalIdx c = *rowsrc; \
			body \
			++rowsrc; \
			++rowdest; \
			++rowmatdest; } \
		scrptr += (destpitch); \
		matptr += (destpitch); \
		mem += pitch; } } while(false)

void blitImageOnMap(Common& common, Level& level, PalIdx* mem, int x, int y, int width, int height)
{
	int pitch = width;
	gvl::rect clipRect(0, 0, level.width, level.height);

	CLIP_IMAGE(clipRect);

	BLITL(&level.data[0], level.width, &level.materials[0],
	{
		if(c)
		{
			PalIdx n;
			if(rowmatdest->dirtBack())
				n = c;
			else
				n = c + 3;
			*rowdest = n;
			*rowmatdest = common.materials[n];
		}
	});
}

void blitStone(Common& common, Level& level, bool p1, PalIdx* mem, int x, int y)
{
	int width = 16;
	int height = 16;
	int pitch = width;

	gvl::rect clip(0, 0, level.width, level.height);

	CLIP_IMAGE(clip);

	PalIdx* dest = level.pixelp(x, y);
	Material* matdest = level.matp(x, y);

	if(p1)
	{
		for(int y_ = 0; y_ < height; ++y_)
		{
			PalIdx* rowdest = dest;
			Material* rowmatdest = matdest;
			PalIdx* rowsrc = mem;

			for(int x_ = 0; x_ < width; ++x_)
			{
				PalIdx c = *rowsrc;
				PalIdx n;
				if(c && rowmatdest->dirtBack()) // TODO: Speed up this test?
					n = c;
				else
					n = c + 3;
				*rowdest = n;
				*rowmatdest = common.materials[n];
				++rowsrc;
				++rowdest;
				++rowmatdest;
			}

			dest += level.width;
			matdest += level.width;
			mem += pitch;
		}
	}
	else
	{
		for(int y_ = 0; y_ < height; ++y_)
		{
			PalIdx* rowdest = dest;
			Material* rowmatdest = matdest;
			PalIdx* rowsrc = mem;

			for(int x_ = 0; x_ < width; ++x_)
			{
				PalIdx c = *rowsrc;
				if(c)
				{
					*rowdest = c;
					*rowmatdest = common.materials[c];
				}

				++rowsrc;
				++rowdest;
				++rowmatdest;
			}

			dest += level.width;
			matdest += level.width;
			mem += pitch;
		}
	}
}

void drawDirtEffect(Common& common, Rand& rand, Level& level, int dirtEffect, int x, int y)
{
	assert(dirtEffect >= 0 && dirtEffect < 9);
	Texture& tex = common.textures[dirtEffect];
	PalIdx* tFrame = common.largeSprites.spritePtr(tex.sFrame + rand(tex.rFrame));
	PalIdx* mFrame = common.largeSprites.spritePtr(tex.mFrame);

	int width = 16;
	int height = 16;
	int pitch = width;
	PalIdx* mem = mFrame;

	gvl::rect clip(0, 0, level.width, level.height - 1);

	CLIP_IMAGE(clip);

	if(tex.nDrawBack)
	{
		BLITL(&level.data[0], level.width, &level.materials[0],
		{
			switch(c)
			{
			case 6:
				if(rowmatdest->anyDirt())
				{
					int mx = x + x_;
					int my = y + y_;

					*rowdest = tFrame[((my & 15) << 4) + (mx & 15)];
					*rowmatdest = common.materials[*rowdest];
				}
			break;

			case 1:
				Material m = *rowmatdest;
				if(m.dirt2())
				{
					*rowdest = 2;
					*rowmatdest = common.materials[2];
				}
				else if(m.dirt())
				{
					*rowdest = 1;
					*rowmatdest = common.materials[1];
				}
			}
		});
	}
	else
	{
		BLITL(&level.data[0], level.width, &level.materials[0],
		{
			switch(c)
			{
			case 10:
			case 6:
				if(rowmatdest->background())
				{
					int mx = x + x_;
					int my = y + y_;

					*rowdest = tFrame[((my & 15) << 4) + (mx & 15)];
					*rowmatdest = common.materials[*rowdest];
				}
			break;

			case 2:
				if(rowmatdest->background())
				{
					*rowdest = 2;
					*rowmatdest = common.materials[2];
				}
			break;

			case 1:
				if(rowmatdest->background())
				{
					*rowdest = 1;
					*rowmatdest = common.materials[1];
				}
			}
		});
	}
}

void correctShadow(Common& common, Level& level, gvl::rect rect)
{
	rect.intersect(gvl::rect(0, 3, level.width - 3, level.height));

	for(int x = rect.x1; x < rect.x2; ++x)
	for(int y = rect.y1; y < rect.y2; ++y)
	{
		PalIdx pix = level.pixel(x, y);

		if(level.mat(x, y).seeShadow()
		&& level.mat(x + 3, y - 3).dirtRock())
		{
			level.setPixel(x, y, pix + 4, common);
		}
		else if(pix >= 164 // Remove shadow
		&& pix <= 167
		&& !level.mat(x + 3, y - 3).dirtRock())
		{
			level.setPixel(x, y, pix - 4, common);
		}
	}
}

// ---- From src/game/level.cpp before the Rust port ----
// generateRandom no longer resets the palette (that stays in C++), and
// drawMiniature takes the bitmap's pixels and pitch.

void Level::generateDirtPattern(Common& common, Rand& rand)
{
	resize(504, 350);

	setPixel(0, 0, rand(7) + 12, common);

	for(int y = 1; y < height; ++y)
		setPixel(0, y, ((rand(7) + 12) + pixel(0, y - 1)) >> 1, common);

	for(int x = 1; x < width; ++x)
		setPixel(x, 0, ((rand(7) + 12) + pixel(x - 1, 0)) >> 1, common);

	for(int y = 1; y < height; ++y)
	for(int x = 1; x < width; ++x)
	{
		setPixel(x, y, (pixel(x - 1, y) + pixel(x, y - 1) + rand(8) + 12) / 3, common);
	}

	// TODO: Optimize the following

	int count = rand(100);

	for(int i = 0; i < count; ++i)
	{
		int x = rand(width) - 8;
		int y = rand(height) - 8;

		int temp = rand(4) + 69;

		PalIdx* image = common.largeSprites.spritePtr(temp);

		for(int cy = 0; cy < 16; ++cy)
		{
			int my = cy + y;
			if(my >= height)
				break;

			if(my < 0)
				continue;

			for(int cx = 0; cx < 16; ++cx)
			{
				int mx = cx + x;
				if(mx >= width)
					break;

				if(mx < 0)
					continue;

				PalIdx srcPix = image[(cy << 4) + cx];
				if(srcPix > 0)
				{
					PalIdx pix = pixel(mx, my);
					if(pix > 176 && pix < 180)
						setPixel(mx, my, (srcPix + pix) / 2, common);
					else
						setPixel(mx, my, srcPix, common);
				}
			}
		}
	}

	count = rand(15);

	for(int i = 0; i < count; ++i)
	{
		int x = rand(width) - 8;
		int y = rand(height) - 8;

		int which = rand(4) + 56;

		blitStone(common, *this, false, common.largeSprites.spritePtr(which), x, y);
	}
}

bool isNoRock(Common& common, Level& level, int size, int x, int y)
{
	gvl::rect rect(x, y, x + size + 1, y + size + 1);

	rect.intersect(gvl::rect(0, 0, level.width, level.height));

	for(int y = rect.y1; y < rect.y2; ++y)
	for(int x = rect.x1; x < rect.x2; ++x)
	{
		if(level.mat(x, y).rock())
			return false;
	}

	return true;
}

void Level::generateRandom(Common& common, Rand& rand)
{
	generateDirtPattern(common, rand);

	int count = rand(50) + 5;

	for(int i = 0; i < count; ++i)
	{
		int cx = rand(width) - 8;
		int cy = rand(height) - 8;

		int dx = rand(11) - 5;
		int dy = rand(5) - 2;

		int count2 = rand(12);

		for(int j = 0; j < count2; ++j)
		{
			int count3 = rand(5);

			for(int k = 0; k < count3; ++k)
			{
				cx += dx;
				cy += dy;
				drawDirtEffect(common, rand, *this, 1, cx, cy); // TODO: Check if it really should be dirt effect 1
			}

			cx -= (count3 + 1) * dx; // TODO: Check if it really should be (count3 + 1)
			cy -= (count3 + 1) * dy; // TODO: Check if it really should be (count3 + 1)

			cx += rand(7) - 3;
			cy += rand(15) - 7;
		}
	}

	count = rand(15) + 5;
	for(int i = 0; i < count; ++i)
	{
		int cx, cy;
		do
		{
			cx = rand(width) - 16;

			if(rand(4) == 0)
				cy = height - 1 - rand(20);
			else
				cy = rand(height) - 16;
		}
		while(!isNoRock(common, *this, 32, cx, cy));

		int rock = rand(3);

		blitStone(common, *this, false, common.largeSprites.spritePtr(stoneTab[rock][0]), cx, cy);
		blitStone(common, *this, false, common.largeSprites.spritePtr(stoneTab[rock][1]), cx + 16, cy);
		blitStone(common, *this, false, common.largeSprites.spritePtr(stoneTab[rock][2]), cx, cy + 16);
		blitStone(common, *this, false, common.largeSprites.spritePtr(stoneTab[rock][3]), cx + 16, cy + 16);
	}

	count = rand(25) + 5;

	for(int i = 0; i < count; ++i)
	{
		int cx, cy;
		do
		{
			cx = rand(width) - 8;

			if(rand(5) == 0)
				cy = height - 1 - rand(13);
			else
				cy = rand(height) - 8;
		}
		while(!isNoRock(common, *this, 15, cx, cy));

		blitStone(common, *this, false, common.largeSprites.spritePtr(rand(6) + 3), cx, cy);
	}
}

void Level::makeShadow(Common& common)
{
	for(int x = 0; x < width - 3; ++x)
	for(int y = 3; y < height; ++y)
	{
		if(mat(x, y).seeShadow()
		&& mat(x + 3, y - 3).dirtRock())
		{
			setPixel(x, y, pixel(x, y) + 4, common);
		}

		if(pixel(x, y) >= 12
		&& pixel(x, y) <= 18
		&& mat(x + 3, y - 3).rock())
		{
			setPixel(x, y, pixel(x, y) - 2, common);
			if(pixel(x, y) < 12)
				setPixel(x, y, 12, common);
		}
	}

	for(int x = 0; x < width; ++x)
	{
		if(mat(x, height - 1).background())
		{
			setPixel(x, height - 1, 13, common);
		}
	}
}

using std::vector;

inline bool free(Material m)
{
	return m.background() || m.anyDirt();
}

bool Level::selectSpawn(Rand& rand, int w, int h, ivec2& selected)
{
	vector<int> vruns(width - w + 1);
	vector<int> vdists(width - w + 1);

	Material* m = &materials[0];

	uint32_t i = 0;

	for (int y = 0; y < height; ++y)
	{
		int hrun = 0;
		int filled = 0;

		for (int x = 0; x < width; ++x)
		{
			if (free(*m))
			{
				++hrun;
			}
			else
			{
				hrun = 0;
				++filled;
			}
			++m;

			int cx = x - (w - 1);
			if (cx < 0)
				continue;

			int& vrun = vruns[cx];
			int& vdist = vdists[cx];

			if (hrun >= w)
			{
				if (vdist > 0)
				{
					vrun = 0;
					vdist = 0;
				}
				++vrun;
			}
			else
			{
				if (vrun >= h
				&& vdist <= 8
				&& filled > w / 4)
				{
					// We have a supported square at (x + 1 - w, y - h)
					++i;
					if (rand(i) < 1)
					{
						selected.x = cx;
						selected.y = y - h;
					}
				}
				++vdist;
			}

			filled -= !free(m[-w]);
		}
	}

	return i > 0;
}

void Level::drawMiniature(uint8_t* pixels, int pitch, int mapX, int mapY, int step)
{
	int my = step / 2;

	int mapEndY = mapY + ((height + step / 2) / step);
	int mapEndX = mapX + ((width + step / 2) / step);

	for (int y = mapY; y < mapEndY; ++y)
	{
		int mx = step / 2;
		for (int x = mapX; x < mapEndX; ++x)
		{
			(pixels + y*pitch)[x] = checkedPixelWrap(mx, my);
			mx += step;
		}
		my += step;
	}
}

// ---- Hashes in the same order as the Rust tests ----

struct Fnv
{
	uint64_t h = 0xcbf29ce484222325ull;
	void add(uint32_t v) { add64(v); }
	void add64(uint64_t v) { h = (h ^ v) * 0x100000001b3ull; }
};

template<typename F>
uint64_t streamHash(uint32_t seed, F f)
{
	gvl::mwc r(seed);
	Fnv h;
	for (uint32_t i = 0; i < 1000000; ++i)
		h.add(f(r, i));
	h.add(r.x);
	h.add(r.c);
	return h.h;
}

// ---- Level hashes, in the same order as rust/liero-game/src/level.rs ----

// Made-up TC tables: random materials with few rocks (none in the dirt
// colours 12 to 19, so random levels always find room for their stones),
// random dirt effects, and sprites with many transparent pixels and many of
// the mask values 1, 2, 6 and 10 the dirt effects look for.
Common testCommon()
{
	Common common;
	gvl::mwc r(0xa55e7);
	for (int i = 0; i < 256; ++i)
	{
		uint32_t f = r(64);
		if (r(4) != 0 || (i >= 12 && i <= 19))
			f &= ~Material::Rock;
		common.materials[i].flags = uint8_t(f);
	}
	for (int t = 0; t < 9; ++t)
	{
		common.textures[t].nDrawBack = r(2) == 1;
		common.textures[t].mFrame = int(r(110));
		common.textures[t].sFrame = int(r(100));
		common.textures[t].rFrame = int(r(10) + 1);
	}
	common.largeSprites.count = 110;
	common.largeSprites.data.resize(110 * 256);
	for (auto& p : common.largeSprites.data)
	{
		if (r(3) == 0)
			p = 0;
		else if (r(2) == 0)
			p = PalIdx(r(12));
		else
			p = PalIdx(r(256));
	}
	return common;
}

void hashLevel(Fnv& h, Level& level)
{
	h.add(uint32_t(level.width));
	h.add(uint32_t(level.height));
	for (std::size_t i = 0; i < level.data.size(); ++i)
	{
		h.add(level.data[i]);
		h.add(level.materials[i].flags);
	}
}

void hashRand(Fnv& h, gvl::mwc& r)
{
	h.add(r.x);
	h.add(r.c);
}

Level randomLevel(Common& common, uint32_t seed)
{
	Level level;
	gvl::mwc r(seed);
	level.generateRandom(common, r);
	level.makeShadow(common);
	return level;
}

void levelHashes()
{
	Common common = testCommon();

	{
		Level level;
		gvl::mwc r(1);
		level.generateDirtPattern(common, r);
		Fnv h;
		hashLevel(h, level);
		hashRand(h, r);
		std::printf("EXPECTED_DIRT_PATTERN: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Fnv h;
		uint32_t seeds[] = {2, 3, 0xdeadbeef, 0x1337};
		for (uint32_t seed : seeds)
		{
			Level level;
			gvl::mwc r(seed);
			level.generateRandom(common, r);
			hashLevel(h, level);
			hashRand(h, r);
			level.makeShadow(common);
			hashLevel(h, level);
		}
		std::printf("EXPECTED_RANDOM_LEVELS: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Level level = randomLevel(common, 2);
		gvl::mwc ops(99), gameRand(100);
		for (int i = 0; i < 3000; ++i)
		{
			uint32_t op = ops(4);
			int x = int(ops(level.width + 32)) - 16;
			int y = int(ops(level.height + 32)) - 16;
			switch (op)
			{
			case 0:
			{
				int effect = int(ops(9));
				drawDirtEffect(common, gameRand, level, effect, x, y);
				break;
			}
			case 1:
			{
				int w = int(ops(40));
				int h = int(ops(40));
				correctShadow(common, level, gvl::rect(x, y, x + w, y + h));
				break;
			}
			case 2:
			{
				bool p1 = ops(2) == 1;
				int frame = int(ops(110));
				blitStone(common, level, p1, common.largeSprites.spritePtr(frame), x, y);
				break;
			}
			case 3:
			{
				int frame = int(ops(110));
				int w = int(ops(16)) + 1;
				int h = int(ops(16)) + 1;
				blitImageOnMap(common, level, common.largeSprites.spritePtr(frame), x, y, w, h);
				break;
			}
			}
		}
		Fnv h;
		hashLevel(h, level);
		hashRand(h, ops);
		hashRand(h, gameRand);
		std::printf("EXPECTED_TERRAIN_OPS: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Fnv h;
		uint32_t seeds[] = {2, 3};
		int sizes[][2] = {{50, 26}, {30, 20}, {5, 5}, {100, 60}, {504, 10}, {1, 1}, {200, 200}};
		for (uint32_t seed : seeds)
		{
			Level level = randomLevel(common, seed);
			for (auto& size : sizes)
			{
				gvl::mwc r(7);
				ivec2 pos = {-1, -1};
				bool found = level.selectSpawn(r, size[0], size[1], pos);
				h.add(found);
				h.add(uint32_t(pos.x));
				h.add(uint32_t(pos.y));
				hashRand(h, r);
			}
		}
		std::printf("EXPECTED_SELECT_SPAWN: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Level level = randomLevel(common, 3);
		std::vector<uint8_t> bitmap(400 * 300, 0xee);
		level.drawMiniature(&bitmap[0], 400, 134, 162, 10);
		level.drawMiniature(&bitmap[0], 400, 10, 20, 2);
		level.drawMiniature(&bitmap[0], 400, 0, 0, 3);
		Fnv h;
		for (uint8_t p : bitmap)
			h.add(p);
		std::printf("EXPECTED_MINIATURE: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		// What Level::load does after reading the pixels.
		Level level;
		level.resize(504, 350);
		gvl::mwc r(5);
		for (auto& p : level.data)
			p = uint8_t(r(256));
		for (std::size_t i = 0; i < level.data.size(); ++i)
			level.materials[i] = common.materials[level.data[i]];
		Fnv h;
		hashLevel(h, level);
		std::printf("EXPECTED_LOADED_MATERIALS: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}
}

// ---- std::sort and the stats screen's percent, for cpp_sort.rs and stats.rs ----

struct SortItem { int key, id; };

// Mostly up to 80 elements, sometimes up to 2,000, with few distinct keys, in
// several layouts. The same arrays as cpp_sort.rs's tests.
template<typename F>
void sortArrays(uint32_t seed, F f)
{
	gvl::mwc r(seed);
	for (uint32_t t = 0; t < 20000; ++t)
	{
		uint32_t n = t % 50 == 0 ? r(2000) : r(81);
		uint32_t keys = 1 + r(t % 3 == 0 ? 3 : 40);
		uint32_t pattern = r(6);
		uint32_t d = std::max(n, 1u);
		std::vector<SortItem> v;
		for (uint32_t i = 0; i < n; ++i)
		{
			uint32_t key;
			switch (pattern)
			{
			case 1: key = i * keys / d; break;
			case 2: case 4: key = (n - i) * keys / d; break;
			case 3: key = std::min(i, n - i) * keys * 2 / d; break;
			case 5: key = 0; break;
			default: key = r(keys);
			}
			v.push_back(SortItem{int(key), int(i)});
		}
		if (pattern == 4 && n > 1)
		{
			for (int k = 0; k < 3; ++k)
			{
				uint32_t a = r(n);
				uint32_t b = r(n);
				std::swap(v[a], v[b]);
			}
		}
		f(v);
	}
}

template<typename Sort>
uint64_t sortHash(uint32_t seed, Sort sort)
{
	Fnv h;
	sortArrays(seed, [&](std::vector<SortItem>& v) {
		sort(v, [](SortItem const& a, SortItem const& b) { return a.key > b.key; });
		h.add(uint32_t(v.size()));
		for (SortItem const& e : v)
			h.add(uint32_t(e.id));
	});
	return h.h;
}

// From src/game/controller/stats_presenter.cpp.
std::string percent(int nom, int den)
{
	if (den == 0)
		return "";

	char buf[256];
	sprintf(buf, "%.2f%%", double(nom)*100.0/den);
	return buf;
}

void sortHashes()
{
	std::printf("EXPECTED_SORT: u64 = 0x%016llx\n", (unsigned long long)sortHash(4242, [](std::vector<SortItem>& v, bool (*c)(SortItem const&, SortItem const&)) {
		std::sort(v.begin(), v.end(), c);
	}));
	std::printf("EXPECTED_HEAP_SORT: u64 = 0x%016llx\n", (unsigned long long)sortHash(4243, [](std::vector<SortItem>& v, bool (*c)(SortItem const&, SortItem const&)) {
		std::partial_sort(v.begin(), v.end(), v.end(), c);
	}));

	Fnv h;
	for (int den = 0; den <= 1000; ++den)
	{
		for (int nom = 0; nom <= den + 3; ++nom)
		{
			for (char ch : percent(nom, den))
				h.add(uint32_t(uint8_t(ch)));
			h.add(0xffffffffu);
		}
	}
	for (int nom : { -5, 1000000, 2147483647, -2147483647 - 1 })
	{
		for (char ch : percent(nom, 7))
			h.add(uint32_t(uint8_t(ch)));
		h.add(0xffffffffu);
	}
	std::printf("EXPECTED_PERCENT: u64 = 0x%016llx\n", (unsigned long long)h.h);
}

int main()
{
	std::printf("EXPECTED_NEXT: u64 = 0x%016llx\n", (unsigned long long)streamHash(0x1337, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_NEXT_SEED0: u64 = 0x%016llx\n", (unsigned long long)streamHash(0, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_NEXT_SEEDMAX: u64 = 0x%016llx\n", (unsigned long long)streamHash(0xffffffff, [](gvl::mwc& r, uint32_t) { return r(); }));
	std::printf("EXPECTED_BELOW: u64 = 0x%016llx\n", (unsigned long long)streamHash(12345, [](gvl::mwc& r, uint32_t i) { return r(i % 1000); }));
	std::printf("EXPECTED_RANGE: u64 = 0x%016llx\n", (unsigned long long)streamHash(777, [](gvl::mwc& r, uint32_t i) { return r(i % 50, i % 50 + i % 7); }));

	{
		gvl::mwc r(4242);
		Fnv h;
		for (uint32_t i = 0; i < 100000; ++i)
		{
			double d = r.get_double(1.0 + i * 0.37);
			uint64_t bits;
			std::memcpy(&bits, &d, sizeof bits);
			h.add64(bits);
		}
		std::printf("EXPECTED_DOUBLE: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		Fnv h;
		for (int x = -2000; x <= 2000; x += 7)
			for (int y = -2000; y <= 2000; y += 11)
				h.add(uint32_t(vectorLength(x, y)));
		for (int x = 0; x <= 200000; x += 997)
			h.add(uint32_t(vectorLength(x, -x / 3)));
		std::printf("EXPECTED_VECTOR_LENGTH: u64 = 0x%016llx\n", (unsigned long long)h.h);
	}

	{
		precomputeTables();
		Fnv h;
		for (int i = 0; i < 128; ++i)
		{
			h.add(uint32_t(cossinTable[i].x));
			h.add(uint32_t(cossinTable[i].y));
		}
		std::printf("EXPECTED_COS_SIN: u64 = 0x%016llx\n", (unsigned long long)h.h);
		std::printf("cossinTable[0] = (%d, %d), [32] = (%d, %d)\n", cossinTable[0].x, cossinTable[0].y, cossinTable[32].x, cossinTable[32].y);
	}

	levelHashes();
	sortHashes();
}
