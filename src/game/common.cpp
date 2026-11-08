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

struct TcHandle
{
	LieroTc* tc;
	~TcHandle() { liero_tc_free(tc); }
};

} // namespace

// The TC is read in Rust (liero_data::load); this copies the result in.
void Common::load(FsNode node)
{
	if (!node.rust())
		throw std::runtime_error("Could not load the TC: Could not read tc.cfg");
	TcHandle handle = { liero_tc_load(node.rust()) };
	LieroTcData const* d = liero_tc_data(handle.tc);
	if (!d)
		throw std::runtime_error("Could not load the TC: " + str(liero_tc_error(handle.tc)));
	if (d->constant_count != MaxC || d->text_count != MaxS || d->hack_count != MaxH)
		throw std::runtime_error("The TC's constants, texts and hacks don't match the game's");

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

	weapons.resize(d->weapon_count);
	for (std::size_t i = 0; i < d->weapon_count; ++i)
	{
		LieroTcWeapon const& src = d->weapons[i];
		Weapon& w = weapons[i];
		w.name = str(src.name);
		w.idStr = str(src.id_str);
		w.id = (int)i;
		w.detectDistance = src.detect_distance;
		w.affectByWorm = src.affect_by_worm;
		w.blowAway = src.blow_away;
		w.gravity = src.gravity;
		w.shadow = src.shadow;
		w.laserSight = src.laser_sight;
		w.launchSound = src.launch_sound;
		w.loopSound = src.loop_sound;
		w.exploSound = src.explo_sound;
		w.speed = src.speed;
		w.addSpeed = src.add_speed;
		w.distribution = src.distribution;
		w.parts = src.parts;
		w.recoil = src.recoil;
		w.multSpeed = src.mult_speed;
		w.delay = src.delay;
		w.loadingTime = src.loading_time;
		w.ammo = src.ammo;
		w.createOnExp = src.create_on_exp;
		w.dirtEffect = src.dirt_effect;
		w.leaveShells = src.leave_shells;
		w.leaveShellDelay = src.leave_shell_delay;
		w.playReloadSound = src.play_reload_sound;
		w.wormExplode = src.worm_explode;
		w.explGround = src.expl_ground;
		w.wormCollide = src.worm_collide;
		w.fireCone = src.fire_cone;
		w.collideWithObjects = src.collide_with_objects;
		w.affectByExplosions = src.affect_by_explosions;
		w.bounce = src.bounce;
		w.timeToExplo = src.time_to_explo;
		w.timeToExploV = src.time_to_explo_v;
		w.hitDamage = src.hit_damage;
		w.bloodOnHit = src.blood_on_hit;
		w.startFrame = src.start_frame;
		w.numFrames = src.num_frames;
		w.loopAnim = src.loop_anim;
		w.shotType = src.shot_type;
		w.colorBullets = src.color_bullets;
		w.splinterAmount = src.splinter_amount;
		w.splinterColour = src.splinter_colour;
		w.splinterType = src.splinter_type;
		w.splinterScatter = src.splinter_scatter;
		w.objTrailType = src.obj_trail_type;
		w.objTrailDelay = src.obj_trail_delay;
		w.partTrailType = src.part_trail_type;
		w.partTrailObj = src.part_trail_obj;
		w.partTrailDelay = src.part_trail_delay;
		w.chainExplosion = src.chain_explosion;
	}

	nobjectTypes.resize(d->nobject_count);
	for (std::size_t i = 0; i < d->nobject_count; ++i)
	{
		LieroTcNObject const& src = d->nobjects[i];
		NObjectType& n = nobjectTypes[i];
		n.idStr = str(src.id_str);
		n.id = (int)i;
		n.detectDistance = src.detect_distance;
		n.gravity = src.gravity;
		n.speed = src.speed;
		n.speedV = src.speed_v;
		n.distribution = src.distribution;
		n.blowAway = src.blow_away;
		n.bounce = src.bounce;
		n.hitDamage = src.hit_damage;
		n.wormExplode = src.worm_explode;
		n.explGround = src.expl_ground;
		n.wormDestroy = src.worm_destroy;
		n.bloodOnHit = src.blood_on_hit;
		n.startFrame = src.start_frame;
		n.numFrames = src.num_frames;
		n.drawOnMap = src.draw_on_map;
		n.colorBullets = src.color_bullets;
		n.createOnExp = src.create_on_exp;
		n.affectByExplosions = src.affect_by_explosions;
		n.dirtEffect = src.dirt_effect;
		n.splinterAmount = src.splinter_amount;
		n.splinterColour = src.splinter_colour;
		n.splinterType = src.splinter_type;
		n.bloodTrail = src.blood_trail;
		n.bloodTrailDelay = src.blood_trail_delay;
		n.leaveObj = src.leave_obj;
		n.leaveObjDelay = src.leave_obj_delay;
		n.timeToExplo = src.time_to_explo;
		n.timeToExploV = src.time_to_explo_v;
	}

	sobjectTypes.resize(d->sobject_count);
	for (std::size_t i = 0; i < d->sobject_count; ++i)
	{
		LieroTcSObject const& src = d->sobjects[i];
		SObjectType& s = sobjectTypes[i];
		s.idStr = str(src.id_str);
		s.id = (int)i;
		s.startSound = src.start_sound;
		s.numSounds = src.num_sounds;
		s.animDelay = src.anim_delay;
		s.startFrame = src.start_frame;
		s.numFrames = src.num_frames;
		s.detectRange = src.detect_range;
		s.damage = src.damage;
		s.blowAway = src.blow_away;
		s.shadow = src.shadow;
		s.shake = src.shake;
		s.flash = src.flash;
		s.dirtEffect = src.dirt_effect;
	}

	for (int i = 0; i < MaxC; ++i)
		C[i] = d->constants[i];
	for (int i = 0; i < MaxS; ++i)
		S[i] = str(d->texts[i]);
	for (int i = 0; i < MaxH; ++i)
		H[i] = d->hacks[i];

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

	weapOrder.assign(d->weap_order, d->weap_order + d->weapon_count);
	copySprites(wormSprites, d->worm_sprites);
	copySprites(fireConeSprites, d->fire_cone_sprites);
}

Common::Common()
{
}

std::string Common::guessName() const
{
	std::string const& cp = S[SCopyright2];
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


