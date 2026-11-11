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
#include "tclist.hpp"
#include <string>
#include <memory>

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
	int32_t from;
	int32_t to;
};

struct AIParams
{
	int32_t k[2][7]; // 0x1AEEE, contiguous words
};

// A sound of the TC, with the samples the mixer plays (in Rust,
// rust/liero-game/src/tc.rs). One the TC didn't have isn't valid and plays
// silently.
typedef LieroTcSound SfxSample;

struct Bitmap;
struct FsNode;

using std::vector;

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

	PalIdx const* wormSprite(int f, int dir, int w) const
	{
		return wormSprites.spritePtr(f + dir*7*3 + w*2*7*3);
	}

	Sprite wormSpriteObj(int f, int dir, int w) const
	{
		return wormSprites[f + dir*7*3 + w*2*7*3];
	}

	PalIdx const* fireConeSprite(int f, int dir) const
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

	Material const* materials = nullptr;
	Texture const* textures = nullptr;
	TcList<Weapon> weapons;
	TcList<SObjectType> sobjectTypes;
	TcList<NObjectType> nobjectTypes;
	int32_t const (*bonusRandTimer)[2] = nullptr;
	int32_t const* bonusSObjects = nullptr;
	AIParams const* aiParams = nullptr;
	ColourAnim const* colorAnim = nullptr;
	int32_t const* bonusFrames = nullptr;
	SpriteSet smallSprites;
	SpriteSet largeSprites;

	// The tables the Rust level code reads.
	LieroLevelCommon levelCommon()
	{
		LieroLevelCommon c = { materials, textures, largeSprites.data, largeSprites.count };
		return c;
	}

	SpriteSet textSprites;
	Palette const* exepal = nullptr;
	Font font;
	TcList<SfxSample> sounds;

	// The constants, texts and hacks, in the order of the C, S and H enums.
	int32_t const* C = nullptr;
	LieroStr const* S = nullptr;
	bool const* H = nullptr;
};

#endif // UUID_9E238CFB9F074A3A432E22AE5B8EE5FB
