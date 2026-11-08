#include "common.hpp"

#include "rand.hpp"
#include "gfx/blit.hpp"
#include "filesystem.hpp"
#include "worm.hpp"
#include <map>
#include <string>
#include <algorithm>
#include <stdexcept>

int Common::fireConeOffset[2][7][2] =
{
	{{-3, 1}, {-4, 0}, {-4, -2}, {-4, -4}, {-3, -5}, {-2, -6}, {0, -6}},
	{{3, 1}, {4, 0}, {4, -2}, {4, -4}, {3, -5}, {2, -6}, {0, -6}},
};

char const* Texts::keyNames[177] = {
	"",
	"Esc",
	"1",
	"2",
	"3",
	"4",
	"5",
	"6",
	"7",
	"8",
	"9",
	"0",
	"+",
	"`",
	"Backspace",
	"Tab",
	"Q",
	"W",
	"E",
	"R",
	"T",
	"Y",
	"U",
	"I",
	"O",
	"P",
	"\xC5",
	"^",
	"Enter",
	"Left Crtl",
	"A",
	"S",
	"D",
	"F",
	"G",
	"H",
	"J",
	"K",
	"L",
	"\xD6",
	"\xC4",
	"\xBD",
	"Left Shift",
	"'",
	"Z",
	"X",
	"C",
	"V",
	"B",
	"N",
	"M",
	",",
	".",
	"-",
	"Right Shift",
	"* (Pad)",
	"Left Alt",
	"",
	"Caps Lock",
	"F1",
	"F2",
	"F3",
	"F4",
	"F5",
	"F6",
	"F7",
	"F8",
	"F9",
	"F10",
	"Num Lock",
	"Scroll Lock",
	"7 (Pad)",
	"8 (Pad)",
	"9 (Pad)",
	"- (Pad)",
	"4 (Pad)",
	"5 (Pad)",
	"6 (Pad)",
	"+ (Pad)",
	"1 (Pad)",
	"2 (Pad)",
	"3 (Pad)",
	"0 (Pad)",
	", (Pad)",
	"",
	"",
	"<",
	"F11",
	"F12",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"Enter (Pad)",
	"Right Ctrl",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"Print Screen",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"/ (Pad)",
	"",
	"Print Screen",
	"Right Alt",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"",
	"Home",
	"Up",
	"Page Up",
	"",
	"Left",
	"",
	"Right",
	"",
	"End",
	"Down",
	"Page Down",
	"Insert",
	"Delete",
	"",
	"",
	"",
	"",
	"",
};

Texts::Texts()
{
	gameModes[0] = "Kill'em All";
	gameModes[1] = "Game of Tag";
	gameModes[2] = "Holdazone";
	gameModes[3] = "Scales of Justice";

	onoff[0] = "OFF";
	onoff[1] = "ON";

	controllers[0] = "Human";
	controllers[1] = "CPU";
	controllers[2] = "AI";

	weapStates[0] = "Menu";
	weapStates[1] = "Bonus";
	weapStates[2] = "Banned";

	copyrightBarFormat = 64;
}


void Common::drawTextSmall(Bitmap& scr, char const* str, int x, int y)
{
	for(; *str; ++str)
	{
		unsigned char c = *str - 'A';

		if(c < 26)
		{
			blitImage(scr, textSprites[c], x, y);
		}

		x += 4;
	}
}


namespace
{

std::string str(LieroStr s)
{
	return std::string(reinterpret_cast<char const*>(s.ptr), s.len);
}

void copySprites(SpriteSet& dest, LieroTcSprites const& src)
{
	dest.allocate(src.width, src.height, src.count);
	std::copy(src.data, src.data + dest.data.size(), dest.data.begin());
}


} // namespace

// The TC is read in Rust (liero_data::load). Common keeps it, and copies in
// the sprites, palette, font and sounds.
void Common::load(FsNode node)
{
	if (!node.rust())
		throw std::runtime_error("Could not load the TC: Could not read tc.cfg");
	LieroTc* loaded = liero_tc_load(node.rust());
	LieroTcData const* d = liero_tc_data(loaded);
	if (!d)
	{
		std::string error = str(liero_tc_error(loaded));
		liero_tc_free(loaded);
		throw std::runtime_error("Could not load the TC: " + error);
	}
	if (d->constant_count != MaxC || d->text_count != MaxS || d->hack_count != MaxH)
	{
		liero_tc_free(loaded);
		throw std::runtime_error("The TC's constants, texts and hacks don't match the game's");
	}
	liero_tc_free(tc);
	tc = loaded;

	sounds.clear();
	sounds.resize(d->sound_count);
	for (std::size_t i = 0; i < d->sound_count; ++i)
	{
		LieroTcSound const& src = d->sounds[i];
		SfxSample& s = sounds[i];
		s.name = str(src.name);
		if (src.valid)
		{
			s.originalData.assign(src.data, src.data + src.len);
			s.loaded = true;
			s.createSound();
		}
	}

	weapons = { d->weapons, d->weapon_count };
	nobjectTypes = { d->nobjects, d->nobject_count };
	sobjectTypes = { d->sobjects, d->sobject_count };
	C = d->constants;
	S = d->texts;
	H = d->hacks;

	for (int i = 0; i < 2; ++i)
	{
		bonusRandTimer[i][0] = d->bonus_rand_timer[i * 2];
		bonusRandTimer[i][1] = d->bonus_rand_timer[i * 2 + 1];
		bonusFrames[i] = d->bonus_frames[i];
		bonusSObjects[i] = d->bonus_sobjects[i];
	}
	for (int i = 0; i < LIERO_TEXTURE_COUNT; ++i)
		textures[i] = d->textures[i];
	for (int i = 0; i < 4; ++i)
	{
		colorAnim[i].from = d->color_anim[i * 2];
		colorAnim[i].to = d->color_anim[i * 2 + 1];
	}
	for (int i = 0; i < 256; ++i)
		materials[i].flags = d->materials[i];
	for (int on = 0; on < 2; ++on)
	for (int k = 0; k < 7; ++k)
		aiParams.k[on][k] = d->ai_params[on * 7 + k];

	copySprites(smallSprites, d->small_sprites);
	copySprites(largeSprites, d->large_sprites);
	copySprites(textSprites, d->text_sprites);
	for (int i = 0; i < 256; ++i)
	{
		exepal.entries[i].r = d->palette[i * 3];
		exepal.entries[i].g = d->palette[i * 3 + 1];
		exepal.entries[i].b = d->palette[i * 3 + 2];
	}

	static_assert(LIERO_TC_FONT_CHARS == 250, "Font has 250 characters");
	font.chars.resize(LIERO_TC_FONT_CHARS);
	for (std::size_t i = 0; i < LIERO_TC_FONT_CHARS; ++i)
	{
		std::copy(d->font[i].data, d->font[i].data + 56, font.chars[i].data);
		font.chars[i].width = d->font[i].width;
	}

	weapOrder = { d->weap_order, d->weapon_count };
	copySprites(wormSprites, d->worm_sprites);
	copySprites(fireConeSprites, d->fire_cone_sprites);
}

Common::Common()
{
}

Common::~Common()
{
	liero_tc_free(tc);
}

std::string Common::guessName() const
{
	std::string cp = tcString(S[SCopyright2]);
	auto p = cp.find('(');
	if (p == std::string::npos)
		p = cp.size();

	while (p > 0 && cp[p - 1] == ' ')
		--p;

	return cp.substr(0, p);
}

void SfxSample::createSound()
{
	samples.resize(originalData.size() * 2);
	liero_sound_from_tc(originalData.data(), originalData.size(), samples.data());
}


