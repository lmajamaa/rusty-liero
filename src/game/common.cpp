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


// The TC is read in Rust (liero_data::load). Common keeps it and points into
// it.
void Common::load(FsNode node)
{
	if (!node.rust())
		throw std::runtime_error("Could not load the TC: Could not read tc.cfg");
	LieroTc* loaded = liero_tc_load(node.rust());
	LieroTcData const* d = liero_tc_data(loaded);
	if (!d)
	{
		std::string error = tcString(liero_tc_error(loaded));
		liero_tc_free(loaded);
		throw std::runtime_error("Could not load the TC: " + error);
	}
	if (d->constant_count != MaxC || d->text_count != MaxS || d->hack_count != MaxH)
	{
		liero_tc_free(loaded);
		throw std::runtime_error("The TC's constants, texts and hacks don't match the game's");
	}
	attach(loaded);
}

std::shared_ptr<Common> Common::of(LieroTc* tc)
{
	std::shared_ptr<Common> c(new Common());
	c->attach(liero_tc_retain(tc));
	return c;
}

void Common::attach(LieroTc* loaded)
{
	LieroTcData const* d = liero_tc_data(loaded);
	liero_tc_free(tc);
	tc = loaded;

	sounds = { d->sounds, d->sound_count };
	weapons = { d->weapons, d->weapon_count };
	nobjectTypes = { d->nobjects, d->nobject_count };
	sobjectTypes = { d->sobjects, d->sobject_count };
	C = d->constants;
	S = d->texts;
	H = d->hacks;

	static_assert(sizeof(ColourAnim) == 2 * sizeof(int32_t), "ColourAnim layout");
	static_assert(sizeof(AIParams) == 14 * sizeof(int32_t), "AIParams layout");
	static_assert(sizeof(Material) == 1, "Material layout");
	static_assert(sizeof(Palette) == sizeof(d->palette_colors), "Palette layout");
	static_assert(sizeof(Font::Char) == sizeof(LieroFontChar), "Font::Char layout");
	bonusRandTimer = reinterpret_cast<int32_t const (*)[2]>(d->bonus_rand_timer);
	bonusFrames = d->bonus_frames;
	bonusSObjects = d->bonus_sobjects;
	textures = d->textures;
	colorAnim = reinterpret_cast<ColourAnim const*>(d->color_anim);
	materials = reinterpret_cast<Material const*>(d->materials);
	aiParams = reinterpret_cast<AIParams const*>(d->ai_params);
	exepal = reinterpret_cast<Palette const*>(d->palette_colors);

	smallSprites = SpriteSet(d->small_sprites);
	largeSprites = SpriteSet(d->large_sprites);
	textSprites = SpriteSet(d->text_sprites);
	font.chars = { d->font, LIERO_TC_FONT_CHARS };

	weapOrder = { d->weap_order, d->weapon_count };
	wormSprites = SpriteSet(d->worm_sprites);
	fireConeSprites = SpriteSet(d->fire_cone_sprites);
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
