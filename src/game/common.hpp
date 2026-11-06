#ifndef UUID_9E238CFB9F074A3A432E22AE5B8EE5FB
#define UUID_9E238CFB9F074A3A432E22AE5B8EE5FB

#include "gfx/font.hpp"
#include "weapon.hpp"
#include "sobject.hpp"
#include "nobject.hpp"
#include "constants.hpp"
#include "material.hpp"
#include "gfx/palette.hpp"
#include "gfx/sprite.hpp"
#include <string>
#include <gvl/resman/shared_ptr.hpp>
#include <gvl/support/platform.hpp>
extern "C" {
}

#if ENABLE_TRACING
#include <gvl/io2/fstream.hpp>
#include <gvl/serialization/archive.hpp>
#endif

// A dirt effect, defined in Rust (rust/liero-game/src/level.rs). Its fields
// were at 1C208 (n_draw_back), 1C1EA (m_frame), 1C1F4 (s_frame) and 1C1FE
// (r_frame) in the original game.
typedef LieroTexture Texture;

struct Texts
{
	Texts();

	std::string gameModes[4];
	std::string onoff[2];
	std::string controllers[3];

	static char const* keyNames[177];

	std::string weapStates[3];

	int copyrightBarFormat;

};

struct ColourAnim
{
	int from;
	int to;
};

struct AIParams
{
	int k[2][7]; // 0x1AEEE, contiguous words
};

struct SfxSample
{
	SfxSample()
	: loaded(false)
	{
	}

	SfxSample(std::string name, int length)
	: name(std::move(name)), loaded(true), originalData(length)
	{
	}

	// Makes the samples the mixer plays from originalData, in Rust
	// (liero_sound_from_tc).
	void createSound();

	std::string name;
	// Whether the TC had this sound. One it didn't plays silently.
	bool loaded;
	std::vector<uint8_t> originalData;
	// 16-bit samples, twice as many as originalData. The mixer plays them
	// in place, so they must not change while a sound is playing.
	std::vector<int16_t> samples;
};

struct Bitmap;
struct FsNode;

using std::vector;

#if ENABLE_TRACING
#define LTRACE(category, object, attribute, value) common.ltrace(#category, (uint32)(object), #attribute, value)
#define IF_ENABLE_TRACING(...) __VA_ARGS__
#else
#define LTRACE(category, object, attribute, value) ((void)0)
#define IF_ENABLE_TRACING(x) ((void)0)
#endif

struct Common : gvl::shared
{
	Common();

	~Common()
	{
	}

	static int fireConeOffset[2][7][2];

	void load(FsNode node);
	void drawTextSmall(Bitmap& scr, char const* str, int x, int y);

	std::string guessName() const;

	PalIdx* wormSprite(int f, int dir, int w)
	{
		return wormSprites.spritePtr(f + dir*7*3 + w*2*7*3);
	}

	Sprite wormSpriteObj(int f, int dir, int w)
	{
		return wormSprites[f + dir*7*3 + w*2*7*3];
	}

	PalIdx* fireConeSprite(int f, int dir)
	{
		return fireConeSprites.spritePtr(f + dir*7);
	}

	// Computed
	Texts texts;
	vector<int> weapOrder;
	SpriteSet wormSprites;
	SpriteSet fireConeSprites;

	Material materials[256];
	Texture textures[LIERO_TEXTURE_COUNT];
	vector<Weapon> weapons;
	vector<SObjectType> sobjectTypes;
	vector<NObjectType> nobjectTypes;
	int bonusRandTimer[2][2];
	int bonusSObjects[2];
	AIParams aiParams;
	ColourAnim colorAnim[4];
	int bonusFrames[2];
	SpriteSet smallSprites;
	SpriteSet largeSprites;

	// The tables the Rust level code reads.
	LieroLevelCommon levelCommon()
	{
		LieroLevelCommon c = { materials, textures, largeSprites.data.data(), largeSprites.count };
		return c;
	}

	SpriteSet textSprites;
	Palette exepal;
	Font font;
	vector<SfxSample> sounds;

	int32_t C[MaxC];
	std::string S[MaxS];
	bool H[MaxH];

#if ENABLE_TRACING
	void ltrace(char const* category, uint32 object, char const* attribute, uint32 value);

	gvl::octet_writer trace_writer;
	gvl::octet_reader trace_reader;

	bool writeTrace;
#endif
};

#endif // UUID_9E238CFB9F074A3A432E22AE5B8EE5FB
