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
#include <memory>
extern "C" {
}

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

// A list of the TC's, in Rust.
template<typename T>
struct TcList
{
	TcList()
	{
	}

	TcList(T const* items, std::size_t count)
	: items(items), count(count)
	{
	}

	T const* items = nullptr;
	std::size_t count = 0;

	std::size_t size() const { return count; }
	bool empty() const { return count == 0; }
	T const& operator[](std::size_t i) const { return items[i]; }
	T const* begin() const { return items; }
	T const* end() const { return items + count; }
};

// A TC's name or text as a std::string.
inline std::string tcString(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

struct Common
{
	Common();
	~Common();
	Common(Common const&) = delete;
	Common& operator=(Common const&) = delete;

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

	// The TC, loaded in Rust (rust/liero-game/src/tc.rs), which owns what
	// the lists and tables below point to.
	LieroTc* tc = nullptr;

	// Computed
	Texts texts;
	TcList<int32_t> weapOrder;
	SpriteSet wormSprites;
	SpriteSet fireConeSprites;

	Material materials[256];
	Texture textures[LIERO_TEXTURE_COUNT];
	TcList<Weapon> weapons;
	TcList<SObjectType> sobjectTypes;
	TcList<NObjectType> nobjectTypes;
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

	// The constants, texts and hacks, in the order of the C, S and H enums.
	int32_t const* C = nullptr;
	LieroStr const* S = nullptr;
	bool const* H = nullptr;
};

#endif // UUID_9E238CFB9F074A3A432E22AE5B8EE5FB
